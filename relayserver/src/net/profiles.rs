use bitcoincash_addr::Address;
use bytes::Bytes;
use cashweb::auth_wrapper::{ParseError, VerifyError};
use prost::Message as _;
use thiserror::Error;
use tokio_postgres::Error as PostgresError;
use warp::{http::Response, hyper::Body, reject::Reject};

use super::IntoResponse;
use crate::{db::Database, models::wrapper::AuthWrapper};

#[derive(Debug, Error)]
pub enum GetProfileError {
    #[error("not found")]
    NotFound,
    #[error("failed to read from database: {0}")]
    Database(#[from] PostgresError),
}

impl Reject for GetProfileError {}

impl IntoResponse for GetProfileError {
    fn to_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Database(_) => 500,
        }
    }
}

#[derive(Debug, Error)]
pub enum PutProfileError {
    #[error("failed to write to database: {0}")]
    Database(#[from] PostgresError),
    #[error("failed to decode authorization wrapper: {0}")]
    ProfileDecode(prost::DecodeError),
    #[error("failed to verify authorization wrapper: {0}")]
    Verify(VerifyError),
    #[error("failed to parse authorization wrapper: {0}")]
    Parse(ParseError),
}

impl Reject for PutProfileError {}

impl IntoResponse for PutProfileError {
    fn to_status(&self) -> u16 {
        match self {
            Self::Database(_) => 500,
            _ => 400,
        }
    }
}

pub async fn get_profile(
    addr: Address,
    database: Database,
) -> Result<Response<Body>, GetProfileError> {
    // Get profile
    let raw_profile = database
        .get_raw_profile(addr.as_body())
        .await?
        .ok_or(GetProfileError::NotFound)?;

    // Respond
    Ok(Response::builder().body(Body::from(raw_profile)).unwrap())
}

pub async fn put_profile(
    addr: Address,
    profile_raw: Bytes,
    database: Database,
) -> Result<Response<Body>, PutProfileError> {
    // Decode profile
    let profile =
        AuthWrapper::decode(profile_raw.clone()).map_err(PutProfileError::ProfileDecode)?;

    // Verify signatures
    profile
        .parse()
        .map_err(PutProfileError::Parse)?
        .verify()
        .map_err(PutProfileError::Verify)?;

    // Put to database
    database.put_profile(addr.as_body(), &profile_raw).await?;

    // Respond
    Ok(Response::builder().body(Body::empty()).unwrap())
}
