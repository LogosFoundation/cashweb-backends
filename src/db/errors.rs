use bitcoincash_addr::{Base58Error, CashAddrDecodingError};
use hyper::Error as HyperError;
use rocksdb::Error as RocksError;

#[derive(Debug)]
pub enum GetError {
    Address(CashAddrDecodingError, Base58Error),
    Db(RocksError),
}

impl From<(CashAddrDecodingError, Base58Error)> for GetError {
    fn from((cash_err, base58_err): (CashAddrDecodingError, Base58Error)) -> Self {
        GetError::Address(cash_err, base58_err)
    }
}

impl From<RocksError> for GetError {
    fn from(err: RocksError) -> Self {
        GetError::Db(err)
    }
}

#[derive(Debug)]
pub enum DbPushError {
    Db(RocksError),
    MissingWriteHead,
}

impl From<RocksError> for DbPushError {
    fn from(err: RocksError) -> Self {
        DbPushError::Db(err)
    }
}

#[derive(Debug)]
pub enum PushError {
    Address(CashAddrDecodingError, Base58Error),
    Buffer(HyperError),
    MessageDecode(prost::DecodeError),
    Db(RocksError),
}

impl From<(CashAddrDecodingError, Base58Error)> for PushError {
    fn from((cash_err, base58_err): (CashAddrDecodingError, Base58Error)) -> Self {
        PushError::Address(cash_err, base58_err)
    }
}

impl From<RocksError> for PushError {
    fn from(err: RocksError) -> Self {
        PushError::Db(err)
    }
}

#[derive(Debug)]
pub enum GetFiltersError {
    Address(CashAddrDecodingError, Base58Error),
    Db(RocksError),
    NotFound,
}

impl From<RocksError> for GetFiltersError {
    fn from(err: RocksError) -> Self {
        GetFiltersError::Db(err)
    }
}

impl From<(CashAddrDecodingError, Base58Error)> for GetFiltersError {
    fn from((cash_err, base58_err): (CashAddrDecodingError, Base58Error)) -> Self {
        GetFiltersError::Address(cash_err, base58_err)
    }
}

#[derive(Debug)]
pub enum PutFiltersError {
    Address(CashAddrDecodingError, Base58Error),
    Buffer(HyperError),
    Db(RocksError),
    FilterDecode(prost::DecodeError),
    NotFound,
}

impl From<RocksError> for PutFiltersError {
    fn from(err: RocksError) -> Self {
        PutFiltersError::Db(err)
    }
}

impl From<(CashAddrDecodingError, Base58Error)> for PutFiltersError {
    fn from((cash_err, base58_err): (CashAddrDecodingError, Base58Error)) -> Self {
        PutFiltersError::Address(cash_err, base58_err)
    }
}
