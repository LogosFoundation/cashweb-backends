pub mod errors;
pub mod services;

use std::sync::Arc;

use prost::Message as PMessage;
use rocksdb::{Direction, Error as RocksError, IteratorMode, Options, DB};
use sha2::{Digest, Sha256};

use crate::models::{
    filters::Filters,
    messaging::{Message, MessageSet},
};

const DIGEST_LEN: usize = 4;

const MESSAGE_NAMESPACE: u8 = b'm';
const FILTER_NAMESPACE: u8 = b'f';

#[derive(Clone)]
pub struct Database(Arc<DB>);

impl Database {
    pub fn try_new(path: &str) -> Result<Self, RocksError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        DB::open(&opts, &path).map(Arc::new).map(Database)
    }

    pub fn push_message(
        &self,
        addr: &[u8],
        raw_message: &[u8],
        timestamp: u64,
    ) -> Result<(), RocksError> {
        // Message digest
        let digest = Sha256::new().chain(raw_message).result();

        // Create key
        let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
        let key = [
            addr,
            &[MESSAGE_NAMESPACE],
            &raw_timestamp,
            &digest[..DIGEST_LEN],
        ]
        .concat();

        self.0.put(key, raw_message)?;
        Ok(())
    }

    pub fn get_message(&self, addr: &[u8], position: u64) -> Result<Option<Message>, RocksError> {
        // Create key
        let position_raw = position.to_be_bytes();
        let key = [addr, &[MESSAGE_NAMESPACE], &position_raw].concat();

        self.0.get(key).map(|res| {
            res.map(|item| {
                Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub fn get_messages(
        &self,
        addr: &[u8],
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<MessageSet, RocksError> {
        // Prefix key
        let raw_start_time: [u8; 8] = start_time.to_be_bytes();
        let start_key = [addr, &[MESSAGE_NAMESPACE], &raw_start_time].concat();
        let namespace_key = [addr, &[MESSAGE_NAMESPACE]].concat();

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..namespace_key.len()] == namespace_key[..];

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_key, Direction::Forward));

        let raw_end_time = end_time.map(|end_time| end_time.to_be_bytes());

        let messages: Vec<Message> = if let Some(raw_end_time) = raw_end_time {
            // Check whether key is before end time
            let before_end_time =
                |key: &[u8]| key[namespace_key.len()..namespace_key.len() + 8] < raw_end_time[..];

            // Take items inside namespace and before end time
            iter.take_while(|(key, _)| in_namespace(key) && before_end_time(key))
                .map(|(_, item)| {
                    Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
                })
                .collect()
        } else {
            // Take items inside namespace
            iter.take_while(|(key, _)| in_namespace(key))
                .map(|(_, item)| {
                    Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
                })
                .collect()
        };
        Ok(MessageSet { messages })
    }

    pub fn get_filters(&self, addr: &[u8]) -> Result<Option<Filters>, RocksError> {
        // Prefix key
        let key = [addr, &[FILTER_NAMESPACE]].concat();

        self.0.get(key).map(|raw_filter_opt| {
            raw_filter_opt.map(|raw_filter| {
                Filters::decode(&raw_filter[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub fn put_filters(&self, addr: &[u8], raw_filters: &[u8]) -> Result<(), RocksError> {
        // Prefix key
        let key = [addr, &[FILTER_NAMESPACE]].concat();

        self.0.put(key, raw_filters)
    }
}
