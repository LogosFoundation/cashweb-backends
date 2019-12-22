use rocksdb::Error as RocksError;
use bitcoincash_addr::{Base58Error, CashAddrDecodingError};
use hyper::Error as HyperError;

#[derive(Debug)]
pub enum DbPushError {
    Rocks(RocksError),
    MissingWriteHead,
}

impl From<RocksError> for DbPushError {
    fn from(err: RocksError) -> Self {
        DbPushError::Rocks(err)
    }
}

#[derive(Debug)]
pub enum PushError {
    Address(CashAddrDecodingError, Base58Error),
    Buffer(HyperError),
    MessageDecode(prost::DecodeError)
}

impl From<(CashAddrDecodingError, Base58Error)> for PushError {
    fn from((cash_err, base58_err): (CashAddrDecodingError, Base58Error)) -> Self {
        PushError::Address(cash_err, base58_err)
    }
}

#[derive(Debug)]
pub enum GetError {

}
