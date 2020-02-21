use std::convert::Infallible;

use bitcoin::consensus::encode::Error as TxDeserializeError;
use bitcoincash_addr::{base58, cashaddr};
use hex::FromHexError;
use rocksdb::Error as RocksError;
use warp::{http::StatusCode, reject::Reject, Rejection, Reply};

#[derive(Debug)]
pub enum StampError {
    Decode(TxDeserializeError),
    MissingOutput,
    NotP2PKH,
    // TxReject(BitcoinError),
    UnexpectedAddress,
    DegenerateCombination,
}

#[derive(Debug)]
pub enum ServerError {
    Address(cashaddr::DecodingError, base58::DecodingError),
    DB(RocksError),
    Stamp(StampError),
    MessagesDecode(prost::DecodeError),
    PayloadDecode(prost::DecodeError),
    FilterDecode(prost::DecodeError),
    NotFound,
    DestinationMalformed,
    MalformedStartDigest(FromHexError),
    MalformedEndDigest(FromHexError),
    MissingStart,
    StartDigestNotFound,
    EndDigestNotFound,
    StartBothGiven,
    EndBothGiven,
    InternalDatabaseError,
}

impl From<(cashaddr::DecodingError, base58::DecodingError)> for ServerError {
    fn from(err: (cashaddr::DecodingError, base58::DecodingError)) -> Self {
        ServerError::Address(err.0, err.1)
    }
}

impl From<StampError> for ServerError {
    fn from(err: StampError) -> Self {
        ServerError::Stamp(err)
    }
}

impl From<RocksError> for ServerError {
    fn from(err: RocksError) -> Self {
        ServerError::DB(err)
    }
}

impl Reject for ServerError {}

pub async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
    Ok(warp::reply::with_status("hello", StatusCode::NOT_FOUND))
}
