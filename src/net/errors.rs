use bitcoin::consensus::encode::Error as TxDeserializeError;
use hex::FromHexError;
use rocksdb::Error as RocksError;
use warp::{reject::Reject, Rejection, Reply};

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
    DB(RocksError),
    Stamp(StampError),
    MessagesDecode(prost::DecodeError),
    PayloadDecode(prost::DecodeError),
    DigestDecode(hex::FromHexError),
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
