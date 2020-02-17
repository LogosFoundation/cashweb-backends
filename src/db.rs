use std::{convert::TryInto, sync::Arc};

use prost::Message as PMessage;
use rocksdb::{Direction, Error as RocksError, IteratorMode, Options, DB};
use sha2::{Digest, Sha256};

use crate::models::{
    filters::Filters,
    messaging::{Message, MessagePage, TimedMessage},
};

const DIGEST_LEN: usize = 4;

const MESSAGE_NAMESPACE: u8 = b'm';
const INBOX_NAMESPACE: u8 = b'i';
const OUTBOX_NAMESPACE: u8 = b'o';
const DIGEST_NAMESPACE: u8 = b'd';
const FILTER_NAMESPACE: u8 = b'f';

#[derive(Clone)]
pub struct Database(Arc<DB>);

pub enum BoxType {
    Inbox,
    Outbox,
}

impl Into<u8> for BoxType {
    fn into(self) -> u8 {
        match self {
            Self::Inbox => INBOX_NAMESPACE,
            Self::Outbox => OUTBOX_NAMESPACE,
        }
    }
}

impl Database {
    pub fn try_new(path: &str) -> Result<Self, RocksError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        DB::open(&opts, &path).map(Arc::new).map(Database)
    }

    pub fn get_msg_key_by_digest(
        &self,
        addr: &[u8],
        digest: &[u8],
    ) -> Result<Option<Vec<u8>>, RocksError> {
        let digest_key = [addr, &[DIGEST_NAMESPACE], &digest].concat();

        let opt_key = self.0.get(digest_key)?;
        Ok(opt_key.map(|key| [addr, &key, &digest[..DIGEST_LEN]].concat()))
    }

    pub fn push_message(
        &self,
        addr: &[u8],
        msg_type: BoxType,
        timestamp: u64,
        raw_message: &[u8],
    ) -> Result<(), RocksError> {
        // Message digest
        let digest = Sha256::new().chain(raw_message).result();

        let msg_type = msg_type.into();

        // Create key
        let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
        let key = [
            addr,
            &[MESSAGE_NAMESPACE],
            &[msg_type],
            &raw_timestamp,
            &digest[..DIGEST_LEN],
        ]
        .concat();

        self.0.put(key, raw_message)?;

        // Create digest key
        let digest_key = [addr, &[DIGEST_NAMESPACE], &digest].concat();

        // Create msg key pointer
        let raw_digest_pointer = [&[msg_type][..], &raw_timestamp].concat();

        self.0.put(digest_key, raw_digest_pointer)?;

        Ok(())
    }

    pub fn get_message_by_digest(
        &self,
        addr: &[u8],
        digest: &[u8],
    ) -> Result<Option<Message>, RocksError> {
        let msg_key = match self.get_msg_key_by_digest(addr, digest)? {
            Some(some) => some,
            None => return Ok(None),
        };

        self.get_message_by_key(&msg_key)
    }

    pub fn get_message_by_key(&self, key: &[u8]) -> Result<Option<Message>, RocksError> {
        self.0.get(key).map(|res| {
            res.map(|item| {
                Message::decode(&item[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub fn get_messages_range(
        &self,
        start_key: &[u8],
        opt_end_key: Option<&[u8]>,
    ) -> Result<MessagePage, RocksError> {
        const NAMESPACE_LEN: usize = 20 + 1 + 1;
        let namespace = &start_key[..NAMESPACE_LEN]; // addr || msg namespace byte || inbox namespace byte

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..NAMESPACE_LEN] == namespace[..];

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_key, Direction::Forward));

        // Convert timestamp array to u64
        let time_slice = |key: &[u8]| {
            let arr: [u8; 8] = key[NAMESPACE_LEN..NAMESPACE_LEN + 8].try_into().unwrap(); // This is safe
            u64::from_be_bytes(arr)
        };

        let messages: Vec<TimedMessage> = if let Some(end_key) = opt_end_key {
            // Check whether key is before end time
            let before_end_key = |key: &[u8]| key[NAMESPACE_LEN..] < end_key[NAMESPACE_LEN..];

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

    pub fn get_messages(
        &self,
        addr: &[u8],
        start_time: u64,
        end_time: Option<u64>,
    ) -> Result<MessagePage, RocksError> {
        // Prefix key
        let raw_start_time: [u8; 8] = start_time.to_be_bytes();
        let start_key = [addr, &[MESSAGE_NAMESPACE], &raw_start_time].concat();
        let namespace_key = [addr, &[MESSAGE_NAMESPACE]].concat();

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..namespace_key.len()] == namespace_key[..];

        // Convert timestamp array to u64
        let time_slice = |key: &[u8]| {
            let arr: [u8; 8] = key[namespace_key.len()..namespace_key.len() + 8]
                .try_into()
                .unwrap(); // This is safe
            u64::from_be_bytes(arr)
        };

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_key, Direction::Forward));

        let raw_end_time = end_time.map(|end_time| end_time.to_be_bytes());

        let messages: Vec<TimedMessage> = if let Some(raw_end_time) = raw_end_time {
            // Check whether key is before end time
            let before_end_time =
                |key: &[u8]| key[namespace_key.len()..namespace_key.len() + 8] < raw_end_time[..];

            // Take items inside namespace and before end time
            iter.take_while(|(key, _)| in_namespace(key) && before_end_time(key))
                .map(|(key, item)| {
                    let message = Some(Message::decode(&item[..]).unwrap()); // This panics if stored bytes are malformed
                    TimedMessage {
                        timestamp: time_slice(&key) as i64,
                        message,
                    }
                })
                .collect()
        } else {
            // Take items inside namespace
            iter.take_while(|(key, _)| in_namespace(key))
                .map(|(key, item)| {
                    let message = Some(Message::decode(&item[..]).unwrap()); // This panics if stored bytes are malformed
                    TimedMessage {
                        timestamp: time_slice(&key) as i64,
                        message,
                    }
                })
                .collect()
        };
        Ok(MessagePage { messages })
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
