use std::{convert::TryInto, sync::Arc};

use prost::Message as PMessage;
use rocksdb::{Direction, Error as RocksError, IteratorMode, Options, DB};

use crate::models::{
    relay::messaging::{Message, MessagePage, TimedMessage},
    wrapper::AuthWrapper,
};

const DIGEST_LEN: usize = 4;
const NAMESPACE_LEN: usize = 20 + 1;

const DIGEST_NAMESPACE: u8 = b'd';
const MESSAGE_NAMESPACE: u8 = b'm';
const PROFILE_NAMESPACE: u8 = b'p';

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

/// Convert timestamp array to u64
fn time_slice(key: &[u8]) -> u64 {
    let arr: [u8; 8] = key[NAMESPACE_LEN..NAMESPACE_LEN + 8].try_into().unwrap(); // This is safe
    u64::from_be_bytes(arr)
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

        let opt_timestamp = self.0.get(digest_key)?;
        Ok(opt_timestamp.map(|timestamp| {
            [
                pubkey_hash,
                &[MESSAGE_NAMESPACE],
                &timestamp,
                &digest[..DIGEST_LEN],
            ]
            .concat()
        }))
    }

    pub fn remove_message_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
    ) -> Result<Option<()>, RocksError> {
        match self.get_msg_key_by_digest(pubkey_hash, digest)? {
            Some(some) => {
                self.0.delete(&some)?;
                Ok(Some(()))
            }
            None => Ok(None),
        }
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
        let key = [
            pubkey_hash,
            &[MESSAGE_NAMESPACE],
            &raw_timestamp,
            &digest[..DIGEST_LEN],
        ]
        .concat();
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
        let namespace = &start_prefix[..NAMESPACE_LEN]; // addr || msg namespace byte

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..NAMESPACE_LEN] == namespace[..];

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_prefix, Direction::Forward));

        let messages: Vec<TimedMessage> = if let Some(end_prefix) = opt_end_prefix {
            // Check whether key is before end time
            let before_end_key = |key: &[u8]| key[NAMESPACE_LEN..] < end_prefix[NAMESPACE_LEN..];

            // Take items inside namespace and before end time
            iter.take_while(|(key, _)| in_namespace(key) && before_end_key(key))
                .map(|(key, item)| {
                    let message = Some(Message::decode(&item[..]).unwrap()); // This panics if stored bytes are malformed
                    TimedMessage {
                        server_time: time_slice(&key) as i64,
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
                        server_time: time_slice(&key) as i64,
                        message,
                    }
                })
                .collect()
        };

        Ok(MessagePage { messages })
    }

    pub fn remove_messages_range(
        &self,
        start_prefix: &[u8],
        opt_end_prefix: Option<&[u8]>,
    ) -> Result<(), RocksError> {
        let namespace = &start_prefix[..NAMESPACE_LEN]; // addr || msg namespace byte

        // Check whether key is within namespace
        let in_namespace = |key: &[u8]| key[..NAMESPACE_LEN] == namespace[..];

        // Init iterator
        let iter = self
            .0
            .iterator(IteratorMode::From(&start_prefix, Direction::Forward));

        if let Some(end_prefix) = opt_end_prefix {
            // Check whether key is before end time
            let before_end_key = |key: &[u8]| key[NAMESPACE_LEN..] < end_prefix[NAMESPACE_LEN..];

            // Take items inside namespace and before end time
            let iter = iter.take_while(|(key, _)| in_namespace(key) && before_end_key(key));

            for (key, _) in iter {
                self.0.delete(key)?;
            }
        } else {
            // Take items inside namespace
            let iter = iter.take_while(|(key, _)| in_namespace(key));

            for (key, _) in iter {
                self.0.delete(key)?;
            }
        };

        Ok(())
    }

    pub fn get_profile(&self, addr: &[u8]) -> Result<Option<AuthWrapper>, RocksError> {
        // Prefix key
        let key = [addr, &[PROFILE_NAMESPACE]].concat();

        self.0.get(key).map(|raw_profile_opt| {
            raw_profile_opt.map(|raw_profile| {
                AuthWrapper::decode(&raw_profile[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub fn put_profile(&self, addr: &[u8], raw_profile: &[u8]) -> Result<(), RocksError> {
        // Prefix key
        let key = [addr, &[PROFILE_NAMESPACE]].concat();

        self.0.put(key, raw_profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoincash_addr::Address;
    use sha2::{Digest, Sha256};

    #[test]
    fn get_digest() {
        let database = Database::try_new("./test_dbs/get_digest").unwrap();

        let addr = Address::decode("bchtest:qz35wy0grm4tze4p5tvu0fc6kujsa5vnrcr7y5xl65").unwrap();
        let pubkey_hash = addr.as_body();

        let message = Message::default();
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();
        let digest = &Sha256::digest(&raw_message)[..];

        let timestamp = 100;
        database
            .push_message(pubkey_hash, timestamp, &raw_message[..], digest)
            .unwrap();

        assert!(database
            .get_msg_key_by_digest(pubkey_hash, digest)
            .unwrap()
            .is_some());

        assert!(database
            .get_message_by_digest(pubkey_hash, digest)
            .unwrap()
            .is_some())
    }

    #[test]
    fn get_time_range() {
        let database = Database::try_new("./test_dbs/get_time_range").unwrap();

        let addr = Address::decode("bchtest:qz35wy0grm4tze4p5tvu0fc6kujsa5vnrcr7y5xl65").unwrap();
        let pubkey_hash = addr.as_body();

        let message = Message::default();
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();
        let digest = &Sha256::digest(&raw_message)[..];

        // Put at 100 and 105
        database
            .push_message(pubkey_hash, 100, &raw_message[..], digest)
            .unwrap();
        database
            .push_message(pubkey_hash, 105, &raw_message[..], digest)
            .unwrap();

        // Check out of range [106, inf)
        let prefix = msg_prefix(pubkey_hash, 106);
        assert_eq!(
            database.get_messages_range(&prefix, None).unwrap().messages,
            vec![]
        );

        // Check within range [100, inf)
        let prefix = msg_prefix(pubkey_hash, 100);
        assert_eq!(
            database
                .get_messages_range(&prefix, None)
                .unwrap()
                .messages
                .len(),
            2
        );

        // Check within range [100, 101)
        let prefix = msg_prefix(pubkey_hash, 100);
        let prefix_end = msg_prefix(pubkey_hash, 101);
        assert_eq!(
            database
                .get_messages_range(&prefix, Some(&prefix_end))
                .unwrap()
                .messages
                .len(),
            1
        );

        // Check within range [101, 105)
        let prefix = msg_prefix(pubkey_hash, 101);
        let prefix_end = msg_prefix(pubkey_hash, 105);
        assert_eq!(
            database
                .get_messages_range(&prefix, Some(&prefix_end))
                .unwrap()
                .messages
                .len(),
            0
        )
    }
}
