use std::fmt;

use bitcoincash_addr::Address;
use bytes::Bytes;
use prost::Message as _;
use rocksdb::Error as RocksError;
use warp::{http::Response, hyper::Body, reject::Reject};

use super::IntoResponse;
use crate::{db::Database, models::filters::FilterApplication};

#[derive(Debug)]
pub enum FilterError {
    NotFound,
    Database(RocksError),
    FilterDecode(prost::DecodeError),
}

impl From<RocksError> for FilterError {
    fn from(err: RocksError) -> Self {
        FilterError::Database(err)
    }
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::NotFound => "not found",
            Self::Database(err) => return err.fmt(f),
            Self::FilterDecode(err) => return err.fmt(f),
        };
        f.write_str(printable)
    }
}

impl Reject for FilterError {}

impl IntoResponse for FilterError {
    fn to_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Database(_) => 500,
            Self::FilterDecode(_) => 400,
        }
    }
}

pub async fn get_filters(addr: Address, database: Database) -> Result<Response<Body>, FilterError> {
    // Get filters
    let mut filters = database
        .get_filters(addr.as_body())?
        .ok_or(FilterError::NotFound)?;

    // Don't show private filters
    if let Some(price_filter) = &filters.price_filter {
        if !price_filter.public {
            filters.price_filter = None;
        }
    }

    // Serialize messages
    let mut raw_payload = Vec::with_capacity(filters.encoded_len());
    filters.encode(&mut raw_payload).unwrap();

    // Respond
    Ok(Response::builder().body(Body::from(raw_payload)).unwrap()) // TODO: Headers
}

pub async fn put_filters(
    addr: Address,
    filters_raw: Bytes,
    db_data: Database,
) -> Result<Response<Body>, FilterError> {
    // TODO: Do validation
    let filter_application =
        FilterApplication::decode(filters_raw).map_err(FilterError::FilterDecode)?;

    db_data.put_filters(addr.as_body(), &filter_application.serialized_filters)?;

    // Respond
    Ok(Response::builder().body(Body::empty()).unwrap())
}
