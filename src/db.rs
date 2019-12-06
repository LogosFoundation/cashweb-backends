use std::{convert::TryInto, sync::Arc};

use prost::Message as PMessage;
use rocksdb::{Error as RocksError, Options, DB};

use crate::models::messaging::Message;

const write_namespace: u8 = b'W';
const message_namespace: u8 = b'M';

#[derive(Debug)]
pub enum PushError {
    Rocks(RocksError),
    MissingWriteHead,
}

impl From<RocksError> for PushError {
    fn from(err: RocksError) -> Self {
        PushError::Rocks(err)
    }
}

#[derive(Clone)]
pub struct Database(Arc<DB>);

impl Database {
    pub fn try_new(path: &str) -> Result<Self, RocksError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        DB::open(&opts, &path).map(Arc::new).map(Database)
    }

    /// Add new address to a database
    pub fn new_address(&self, addr: &[u8]) -> Result<(), RocksError> {
        let key = [addr, &[write_namespace]].concat();
        self.0.put(key, [0; 8])
    }

    pub fn get_write_head(&self, addr: &[u8]) -> Result<Option<u64>, RocksError> {
        let key = [addr, &[write_namespace]].concat();
        self.0.get(key).map(move |res| {
            res.map(move |item| {
                let arr: [u8; 8] = item[..8].try_into().unwrap(); // This panics if stored bytes are malformed
                u64::from_be_bytes(arr)
            })
        })
    }

    pub fn get_write_head_raw(&self, addr: &[u8]) -> Result<Option<Vec<u8>>, RocksError> {
        let key = [addr, &[write_namespace]].concat();
        self.0.get(key)
    }

    pub fn push_message(&self, addr: &[u8], message: &Message) -> Result<(), PushError> {
        // TODO: There is a race condition here
        // a) Wait until rocksdb supports transactions
        // b) Implement some sort of concurrent hashmap to represent locks

        // Create key
        let write_head = self
            .get_write_head_raw(addr)?
            .ok_or(PushError::MissingWriteHead)?;
        let key = [addr, &[message_namespace], &write_head].concat();

        // Encode message
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();

        self.0.put(key, raw_message)?;
        Ok(())
    }
}
