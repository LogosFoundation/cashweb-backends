use std::{convert::TryInto, sync::Arc};

use prost::Message as PMessage;
use rocksdb::{Direction, Error as RocksError, IteratorMode, Options, DB};

use crate::models::{
    filters::Filters,
    messaging::{Message, MessagePage, TimedMessage},
};

const DIGEST_LEN: usize = 4;

const MESSAGE_NAMESPACE: u8 = b'm';
const DIGEST_NAMESPACE: u8 = b'd';
const FILTER_NAMESPACE: u8 = b'f';

const NAMESPACE_LEN: usize = 20 + 1;

#[derive(Clone)]
pub struct Database(Arc<DB>);

pub fn msg_key(pubkey_hash: &[u8], timestamp: u64, digest: &[u8]) -> Vec<u8> {
    let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
    [
        pubkey_hash,
        &[MESSAGE_NAMESPACE],
        &raw_timestamp,
        &digest[..DIGEST_LEN],
    ]
    .concat()
}

pub fn msg_prefix(pubkey_hash: &[u8], timestamp: u64) -> Vec<u8> {
    let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
    [pubkey_hash, &[MESSAGE_NAMESPACE], &raw_timestamp].concat()
}

impl Database {
    pub fn try_new(path: &str) -> Result<Self, RocksError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        DB::open(&opts, &path).map(Arc::new).map(Database)
    }

    pub fn get_msg_key_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
    ) -> Result<Option<Vec<u8>>, RocksError> {
        let digest_key = [pubkey_hash, &[DIGEST_NAMESPACE], &digest].concat();

        let opt_key = self.0.get(digest_key)?;
        Ok(opt_key.map(|key| [pubkey_hash, &key, &digest[..DIGEST_LEN]].concat()))
    }

    pub fn remove_message_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
    ) -> Result<Option<()>, RocksError> {
        let digest_key = [pubkey_hash, &[DIGEST_NAMESPACE], &digest].concat();
        let opt_key = self.0.get(digest_key)?;
        let key = match opt_key {
            Some(some) => some,
            None => return Ok(None),
        };

        self.0.delete(key).map(|_| Some(()))
    }

    pub fn push_message(
        &self,
        pubkey_hash: &[u8],
        timestamp: u64,
        raw_message: &[u8],
        digest: &[u8],
    ) -> Result<(), RocksError> {
        // Create key
        let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
        let key = [pubkey_hash, &[MESSAGE_NAMESPACE], &raw_timestamp, digest].concat();

        self.0.put(key, raw_message)?;

        // Create digest key
        let digest_key = [pubkey_hash, &[DIGEST_NAMESPACE], &digest].concat();

        self.0.put(digest_key, raw_timestamp)?;

        Ok(())
    }

    pub fn get_message_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
    ) -> Result<Option<Vec<u8>>, RocksError> {
        match self.get_msg_key_by_digest(pubkey_hash, digest)? {
            Some(some) => self.get_message_by_key(&some),
            None => Ok(None),
        }
    }

    pub fn get_message_by_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RocksError> {
        self.0.get(key)
    }

    pub fn get_messages_range(
        &self,
        start_prefix: &[u8],
        opt_end_prefix: Option<&[u8]>,
    ) -> Result<MessagePage, RocksError> {
        let namespace = &start_prefix[..NAMESPACE_LEN]; // addr || msg namespace byte || inbox namespace byte

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..NAMESPACE_LEN] == namespace[..];

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_prefix, Direction::Forward));

        // Convert timestamp array to u64
        fn time_slice(key: &[u8]) -> u64 {
            let arr: [u8; 8] = key[NAMESPACE_LEN..NAMESPACE_LEN + 8].try_into().unwrap(); // This is safe
            u64::from_be_bytes(arr)
        }

        let messages: Vec<TimedMessage> = if let Some(end_prefix) = opt_end_prefix {
            // Check whether key is before end time
            let before_end_key = |key: &[u8]| key[NAMESPACE_LEN..] < end_prefix[NAMESPACE_LEN..];

            // Take items inside namespace and before end time
            iter.take_while(|(key, _)| in_namespace(key) && before_end_key(key))
                .map(|(key, item)| {
                    let message = Some(Message::decode(&item[..]).unwrap()); // This panics if stored bytes are malformed
                    TimedMessage {
                        timestamp: time_slice(&key) as i64,
                        message,
                    }
                })
                .collect()
        } else {
            vec![]
        };

        Ok(MessagePage { messages })
    }

    // TODO: Delete range
    // pub fn remove_messages_range(
    //     &self,
    //     start_prefix: &[u8],
    //     opt_end_prefix: Option<&[u8]>,
    // ) -> Result<(), RocksError> {
    //     self.0.delete_range()
    // }

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
