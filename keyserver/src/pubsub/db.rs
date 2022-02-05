use std::sync::Arc;

use cashweb::auth_wrapper::AuthWrapper;
use prost::Message as _;
use rocksdb::{ColumnFamily, Direction, IteratorMode, Options, DB};
use thiserror::Error;

use crate::crypto::sha256;

const MESSAGE_CF_NAME: &str = "messages";
const PAYLOADS_CF_NAME: &str = "payloads";

#[derive(Clone)]
pub struct PubSubDatabase {
    db: Arc<DB>,
}
#[derive(Debug, Error)]
pub enum PubSubDatabaseError {
    #[error("RocksDB error: {0}")]
    RocksDB(#[from] rocksdb::Error),
    #[error("Prost encode error: {0}")]
    ProstEncode(#[from] prost::EncodeError),
    #[error("Prost decode error: {0}")]
    ProstDecode(#[from] prost::DecodeError),
    #[error("Value not found in messages: {0}")]
    MissingValue(String),
    #[error("Topic has too many separators: {0} > 10")]
    TopicTooLong(usize),
    #[error("Topic contains invalid characters")]
    TopicInvalidCharacters(),
    #[error("Topic contains empty segments")]
    TopicInvalidSegments(),
}

impl PubSubDatabase {
    pub fn new(path: &str) -> Result<Self, PubSubDatabaseError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let db = DB::open_cf(&opts, &path, &[MESSAGE_CF_NAME, PAYLOADS_CF_NAME])?;
        Ok(PubSubDatabase { db: Arc::new(db) })
    }

    /// Put a serialized `Message` to database.
    pub fn put_message(
        &self,
        timestamp: u64,
        topic: &str,
        message: &AuthWrapper,
    ) -> Result<(), PubSubDatabaseError> {
        let mut buf = Vec::new();
        message.encode(&mut buf)?;
        let split_topic = topic.split(".").collect::<Vec<_>>();
        if split_topic.len() > 10 {
            return Err(PubSubDatabaseError::TopicTooLong(split_topic.len()));
        }
        if split_topic.iter().any(|segment| segment.is_empty()) {
            return Err(PubSubDatabaseError::TopicInvalidSegments());
        }

        self.db
            .put_cf(self.cf_payloads(), &message.payload_digest, &buf)?;

        for idx in 0..split_topic.len() + 1 {
            let base_topic_parts = split_topic[..idx].join(".");
            let topic_digest = sha256(base_topic_parts.as_bytes());
            let topical_key = [
                &topic_digest,
                timestamp.to_be_bytes().as_ref(),
                &message.payload_digest,
            ]
            .concat();
            self.db
                .put_cf(self.cf_message(), &topical_key, &message.payload_digest)?;
        }
        Ok(())
    }

    /// Replace a serialized `Message` to database. No need to update
    /// indexes as they are all pointing to this entry.
    pub fn update_message(&self, message: &AuthWrapper) -> Result<(), PubSubDatabaseError> {
        let mut buf = Vec::new();
        message.encode(&mut buf)?;
        self.db
            .put_cf(self.cf_payloads(), &message.payload_digest, &buf)?;
        Ok(())
    }

    /// Get serialized `messages` from database.
    pub fn get_messages_to(
        &self,
        topic: &str,
        from: i64,
        to: i64,
    ) -> Result<Vec<AuthWrapper>, PubSubDatabaseError> {
        let valid_topic = topic
            .chars()
            .all(|c| c.is_lowercase() || c.is_numeric() || c == '.' || c == '-');
        if !valid_topic {
            return Err(PubSubDatabaseError::TopicInvalidCharacters());
        }

        let topic_digest = sha256(topic.as_bytes());
        let start_prefix = [&topic_digest, from.to_be_bytes().as_ref()].concat();
        let end_prefix = [&topic_digest, to.to_be_bytes().as_ref()].concat();

        let iter = self.db.iterator_cf(
            self.cf_message(),
            IteratorMode::From(&start_prefix, Direction::Forward),
        );

        iter.take_while(|(key, _)| key.as_ref() <= end_prefix.as_slice())
            .map(|(_, payload_digest)| self.get_message(&payload_digest))
            .collect()
    }

    /// Get a vector of messages starting at some unix timestamp.
    /// TODO: actually use this
    #[allow(dead_code)]
    pub fn get_messages(
        &self,
        topic: &str,
        from: i64,
    ) -> Result<Vec<AuthWrapper>, PubSubDatabaseError> {
        self.get_messages_to(topic, from, i64::MAX)
    }

    /// Get a specific message by payload hash.
    pub fn get_message(&self, payload_digest: &[u8]) -> Result<AuthWrapper, PubSubDatabaseError> {
        match self.db.get_cf(self.cf_payloads(), payload_digest)? {
            Some(wrapper_bytes) => Ok(AuthWrapper::decode(wrapper_bytes.as_slice())?),
            None => Err(PubSubDatabaseError::MissingValue(hex::encode(
                payload_digest,
            ))),
        }
    }

    fn cf_message(&self) -> &ColumnFamily {
        self.db.cf_handle(MESSAGE_CF_NAME).unwrap()
    }

    fn cf_payloads(&self) -> &ColumnFamily {
        self.db.cf_handle(PAYLOADS_CF_NAME).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use rocksdb::{Options, DB};

    use super::*;

    #[test]
    fn messages() {
        const TEST_NAME: &str = "./tests/messages";

        // Create database
        let database = PubSubDatabase::new(TEST_NAME).unwrap();

        // Create database wrapper
        let mut message_one = AuthWrapper::default();
        message_one.payload_digest = vec![0; 32];

        let data_wrapper_out_0 = database.get_messages("foo.bar.bob", 0).unwrap();
        assert_eq!(data_wrapper_out_0.len(), 0);

        // Put to database
        database
            .put_message(1, "foo.bar.bob", &message_one)
            .unwrap();

        // Get from database
        let data_wrapper_out = database.get_messages("foo.bar.bob", 0).unwrap();
        assert_eq!(data_wrapper_out.len(), 1);
        assert_eq!(message_one, data_wrapper_out[0]);

        // Get from database
        let data_wrapper_out = database.get_messages("foo", 0).unwrap();
        assert_eq!(data_wrapper_out.len(), 1);
        assert_eq!(message_one, data_wrapper_out[0]);

        // Create database wrapper
        let mut message_two = AuthWrapper::default();
        message_two.payload_digest = vec![1; 32];

        // Put to database
        database.put_message(1, "foo.bar", &message_two).unwrap();

        // Get from database
        let data_wrapper_out_two = database.get_messages("foo.bar.bob", 0).unwrap();
        assert_eq!(data_wrapper_out_two.len(), 1);
        assert_eq!(message_one, data_wrapper_out_two[0]);

        // Get from database
        let data_wrapper_three = database.get_messages("foo", 0).unwrap();
        assert_eq!(data_wrapper_three.len(), 2);
        assert_eq!(message_one, data_wrapper_three[0]);
        assert_eq!(message_two, data_wrapper_three[1]);

        let data_wrapper_four = database.get_messages("", 0).unwrap();
        assert_eq!(data_wrapper_four.len(), 2);
        assert_eq!(message_one, data_wrapper_four[0]);
        assert_eq!(message_two, data_wrapper_four[1]);

        // Destroy database
        drop(database);
        DB::destroy(&Options::default(), TEST_NAME).unwrap();
    }
}
