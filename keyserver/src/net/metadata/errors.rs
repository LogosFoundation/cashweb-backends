use cashweb::auth_wrapper::{ParseError, VerifyError};
use thiserror::Error;
use warp::reject::Reject;

use crate::net::IntoResponse;

#[derive(Debug, Error)]
pub enum PutMetadataError {
    #[error("failed to write to database: {0}")]
    Database(rocksdb::Error),
    #[error("failed to verify authorization wrapper: {0}")]
    InvalidAuthWrapper(ParseError),
    #[error("failed to parse authorization wrapper: {0}")]
    VerifyAuthWrapper(VerifyError),
}

impl From<rocksdb::Error> for PutMetadataError {
    fn from(err: rocksdb::Error) -> Self {
        Self::Database(err)
    }
}

impl Reject for PutMetadataError {}

impl IntoResponse for PutMetadataError {
    fn to_status(&self) -> u16 {
        match self {
            Self::Database(_) => 500,
            _ => 400,
        }
    }
}

#[derive(Debug, Error)]
pub enum GetMetadataError {
    #[error("not found")]
    NotFound,
    #[error("failed to read from database: {0}")]
    Database(rocksdb::Error),
}

impl Reject for GetMetadataError {}

impl From<rocksdb::Error> for GetMetadataError {
    fn from(err: rocksdb::Error) -> Self {
        Self::Database(err)
    }
}

impl IntoResponse for GetMetadataError {
    fn to_status(&self) -> u16 {
        match self {
            Self::NotFound => 404,
            Self::Database(_) => 500,
        }
    }
}
