pub mod services;
pub mod errors;

use std::{convert::TryInto, sync::Arc};

use prost::Message as PMessage;
use rocksdb::{Direction, Error as RocksError, IteratorMode, Options, DB};

use crate::models::{messaging::{Message, MessageSet}, filters::Filters};
use errors::*;

const WRITE_NAMESPACE: u8 = b'w';
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

    /// Add new address to a database
    pub fn new_address(&self, addr: &[u8]) -> Result<(), RocksError> {
        let key = [addr, &[WRITE_NAMESPACE]].concat();
        self.0.put(key, [0; 8])
    }

    pub fn get_write_head(&self, addr: &[u8]) -> Result<Option<u64>, RocksError> {
        let key = [addr, &[WRITE_NAMESPACE]].concat();
        self.0.get(key).map(move |res| {
            res.map(move |item| {
                let arr: [u8; 8] = item[..8].try_into().unwrap(); // This panics if stored bytes are malformed
                u64::from_be_bytes(arr)
            })
        })
    }

    pub fn get_write_head_raw(&self, addr: &[u8]) -> Result<Option<Vec<u8>>, RocksError> {
        let key = [addr, &[WRITE_NAMESPACE]].concat();
        self.0.get(key)
    }

    pub fn push_message(&self, addr: &[u8], message: &Message) -> Result<(), DbPushError> {
        // TODO: There is a race condition here
        // a) Wait until rocksdb supports transactions
        // b) Implement some sort of concurrent hashmap to represent locks

        // Create key
        let write_head = self
            .get_write_head_raw(addr)?
            .ok_or(DbPushError::MissingWriteHead)?;
        let key = [addr, &[MESSAGE_NAMESPACE], &write_head].concat();

        // Encode message
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();

        self.0.put(key, raw_message)?;
        Ok(())
    }

    pub fn get_message(&self, addr: &[u8], position: u64) -> Result<Option<Message>, RocksError> {
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
        start: u64,
        count: Option<u64>,
    ) -> Result<MessageSet, RocksError> {
        // Prefix key
        let raw_start_height: [u8; 8] = start.to_be_bytes();
        let start_key = [addr, &[MESSAGE_NAMESPACE], &raw_start_height].concat();
        let namespace_key = [addr, &[MESSAGE_NAMESPACE]].concat();

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_key, Direction::Forward));

        let messages: Vec<Message> = if let Some(count) = count {
            iter.take_while(|(key, _)| key[..namespace_key.len()] == namespace_key[..])
                .take(count as usize)
                .map(|(_, item)| {
                    Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
                })
                .collect()
        } else {
            iter.take_while(|(key, _)| key[..namespace_key.len()] == namespace_key[..])
                .map(|(_, item)| {
                    Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
                })
                .collect()
        };
        Ok(MessageSet { messages })
    }

    pub fn get_filters(&self, addr: &[u8]) -> Result<Option<Filters>, RocksError> {
        let key = [addr, &[FILTER_NAMESPACE]].concat();
        self.0.get(key).map(|raw_filter_opt| {
            raw_filter_opt.map(|raw_filter| {
                Filters::decode(&raw_filter[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub fn put_filters(&self, addr: &[u8]) -> Result<Option<Filters>, RocksError> {
        let key = [addr, &[FILTER_NAMESPACE]].concat();
        self.0.get(key).map(|raw_filter_opt| {
            raw_filter_opt.map(|raw_filter| {
                Filters::decode(&raw_filter[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }
}
