use std::fmt;

use bitcoincash_addr::Address;
use bytes::Bytes;
use prost::Message as _;
use rocksdb::Error as RocksError;
use warp::{http::Response, hyper::Body, reject::Reject};

use super::IntoResponse;
use crate::{db::Database, models::wrapper::AuthWrapper};

#[derive(Debug)]
pub enum ProfileError {
    NotFound,
    Database(RocksError),
    ProfileDecode(prost::DecodeError),
}

impl From<RocksError> for ProfileError {
    fn from(err: RocksError) -> Self {
        ProfileError::Database(err)
    }
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::NotFound => "not found",
            Self::Database(err) => return err.fmt(f),
            Self::ProfileDecode(err) => return err.fmt(f),
        };
        f.write_str(printable)
    }
}

impl Reject for ProfileError {}

impl IntoResponse for ProfileError {
    fn to_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Database(_) => 500,
            Self::ProfileDecode(_) => 400,
        }
    }
}

pub async fn get_profile(addr: Address, database: Database) -> Result<Response<Body>, ProfileError> {
    // Get profile
    let mut profile = database
        .get_profile(addr.as_body())?
        .ok_or(ProfileError::NotFound)?;

    // Serialize messages
    let mut raw_profile = Vec::with_capacity(profile.encoded_len());
    profile.encode(&mut raw_profile).unwrap();

    // Respond
    Ok(Response::builder().body(Body::from(raw_profile)).unwrap()) // TODO: Headers
}

pub async fn put_profile(
    addr: Address,
    profile_raw: Bytes,
    db_data: Database,
) -> Result<Response<Body>, ProfileError> {
    // TODO: Do validation
    let profile =
        AuthWrapper::decode(profile_raw).map_err(ProfileError::ProfileDecode)?;

    db_data.put_profile(addr.as_body(), &profile.serialized_payload)?;

    // Respond
    Ok(Response::builder().body(Body::empty()).unwrap())
}
