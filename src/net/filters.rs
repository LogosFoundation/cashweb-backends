use crate::db::Database;

use bitcoincash_addr::Address;
use bytes::Bytes;
use prost::Message as _;
use warp::http::Response;

use super::errors::*;
use crate::models::filters::FilterApplication;

pub async fn get_filters(
    addr_str: String,
    database: Database,
) -> Result<Response<Vec<u8>>, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Get filters
    let mut filters = database
        .get_filters(addr.as_body())?
        .ok_or(ServerError::NotFound)?;

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
    Ok(Response::builder().body(raw_payload).unwrap()) // TODO: Headers
}

pub async fn put_filters(
    addr_str: String,
    filters_raw: Bytes,
    db_data: Database,
) -> Result<Response<()>, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // TODO: Do validation
    let filter_application =
        FilterApplication::decode(filters_raw).map_err(ServerError::FilterDecode)?;

    db_data.put_filters(addr.as_body(), &filter_application.serialized_filters)?;

    // Respond
    Ok(Response::builder().body(()).unwrap())
}
