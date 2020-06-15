use std::fmt;

use bitcoincash_addr::Address;
use bytes::Bytes;
use cashweb::auth_wrapper::ValidationError;
use prost::Message as _;
use rocksdb::Error as RocksError;
use tokio::task;
use warp::{http::Response, hyper::Body, reject::Reject};

use super::IntoResponse;
use crate::{db::Database, models::wrapper::AuthWrapper};

#[derive(Debug)]
pub enum ProfileError {
    NotFound,
    Database(RocksError),
    ProfileDecode(prost::DecodeError),
    Validation(ValidationError),
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
            Self::Validation(err) => return err.fmt(f),
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
            _ => 400,
        }
    }
}

pub async fn get_profile(
    addr: Address,
    database: Database,
) -> Result<Response<Body>, ProfileError> {
    // Get profile
    let raw_profile = task::spawn_blocking(move || database.get_raw_profile(addr.as_body()))
        .await
        .unwrap()?
        .ok_or(ProfileError::NotFound)?;

    // Respond
    Ok(Response::builder().body(Body::from(raw_profile)).unwrap())
}

pub async fn put_profile(
    addr: Address,
    profile_raw: Bytes,
    database: Database,
) -> Result<Response<Body>, ProfileError> {
    // Decode profile
    let profile = AuthWrapper::decode(profile_raw.clone()).map_err(ProfileError::ProfileDecode)?;

    // Verify signatures
    profile.validate().map_err(ProfileError::Validation)?;

    // Put to database
    task::spawn_blocking(move || database.put_profile(addr.as_body(), &profile_raw))
        .await
        .unwrap()?;

    // Respond
    Ok(Response::builder().body(Body::empty()).unwrap())
}
