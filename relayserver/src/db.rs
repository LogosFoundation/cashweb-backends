use std::sync::Arc;

use cashweb::relay::*;
use prost::Message as PMessage;
use tokio_postgres::{types::ToSql, Client, Error as PostgresError, NoTls};

use crate::models::wrapper::AuthWrapper;

const DIGEST_LEN: usize = 4;
const NAMESPACE_LEN: usize = 20 + 1;

const DIGEST_NAMESPACE: u8 = b'd';
pub const FEED_NAMESPACE: u8 = b'f';
pub const MESSAGE_NAMESPACE: u8 = b'm';
const PROFILE_NAMESPACE: u8 = b'p';

#[derive(Clone)]
pub struct Database(Arc<Client>);

pub fn msg_key(pubkey_hash: &[u8], timestamp: u64, digest: &[u8], namespace: u8) -> Vec<u8> {
    let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
    [
        pubkey_hash,
        &[namespace],
        &raw_timestamp,
        &digest[..DIGEST_LEN],
    ]
    .concat()
}

pub fn msg_prefix(pubkey_hash: &[u8], timestamp: u64, namespace: u8) -> Vec<u8> {
    let raw_timestamp: [u8; 8] = timestamp.to_be_bytes();
    [&pubkey_hash[..], &[namespace], &raw_timestamp].concat()
}

impl Database {
    pub async fn try_new(config: &str) -> Result<Self, PostgresError> {
        let (client, connection) = tokio_postgres::connect(config, NoTls).await?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::error!(message = "connection error", error = %e);
            }
        });

        Ok(Self(Arc::new(client)))
    }

    pub async fn remove_message_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
        namespace: u8,
    ) -> Result<Option<Vec<u8>>, PostgresError> {
        let namespace = namespace as i8;
        let params: Vec<&(dyn ToSql + Sync)> = vec![&pubkey_hash, &digest, &namespace];
        let rows = self
            .0
            .query_opt(
                "DELETE FROM messages WHERE pk_hash=$1 AND message_digest=$2 AND namespace = $3",
                &params,
            )
            .await?;
        Ok(rows.map(|row| row.get(0)))
    }

    pub async fn push_message(
        &self,
        pubkey_hash: &[u8],
        timestamp: u64,
        raw_message: &[u8],
        digest: &[u8],
        namespace: u8,
    ) -> Result<(), PostgresError> {
        let timestamp = timestamp as i32;
        let namespace = namespace as i8;
        let params: Vec<&(dyn ToSql + Sync)> =
            vec![&pubkey_hash, &timestamp, &digest, &namespace, &raw_message];
        self.0
            .query("INSERT INTO messages VALUES ($1, $2, $3, $4, $5)", &params)
            .await?;

        Ok(())
    }

    pub async fn get_message_by_digest(
        &self,
        pubkey_hash: &[u8],
        digest: &[u8],
        namespace: u8,
    ) -> Result<Option<Vec<u8>>, PostgresError> {
        let namespace = namespace.to_string(); // TODO: This is weird
        let params: Vec<&(dyn ToSql + Sync)> = vec![&pubkey_hash, &digest, &namespace];
        let rows = self.0.query_opt("SELECT message FROM messages WHERE pk_hash = $1 AND message_digest = $2 AND namespace = $3", &params).await?;
        Ok(rows.map(|row| row.get(0)))
    }

    pub fn get_messages_range(
        &self,
        start_prefix: &[u8],
        opt_end_prefix: Option<&[u8]>,
    ) -> Result<MessagePage, PostgresError> {
        todo!()
    }

    pub fn remove_messages_range(
        &self,
        start_prefix: &[u8],
        opt_end_prefix: Option<&[u8]>,
    ) -> Result<(), PostgresError> {
        todo!()
    }

    pub async fn get_raw_profile(
        &self,
        pubkey_hash: &[u8],
    ) -> Result<Option<Vec<u8>>, PostgresError> {
        let params: Vec<&(dyn ToSql + Sync)> = vec![&pubkey_hash];
        let rows = self
            .0
            .query_opt("SELECT profile FROM profiles WHERE pk_hash = $1", &params)
            .await?;
        Ok(rows.map(|row| row.get(0)))
    }

    pub async fn get_profile(&self, addr: &[u8]) -> Result<Option<AuthWrapper>, PostgresError> {
        self.get_raw_profile(addr).await.map(|raw_profile_opt| {
            raw_profile_opt.map(|raw_profile| {
                AuthWrapper::decode(&raw_profile[..]).unwrap() // This panics if stored bytes are malformed
            })
        })
    }

    pub async fn put_profile(
        &self,
        pk_hash: &[u8],
        raw_profile: &[u8],
    ) -> Result<(), PostgresError> {
        let params: Vec<&(dyn ToSql + Sync)> = vec![&pk_hash, &raw_profile];
        self.0
            .query("INSERT INTO profile VALUES ($1, $2)", &params)
            .await?;
        Ok(())
    }

    pub async fn clear_messages(&self) -> Result<(), PostgresError> {
        self.0.query("DELETE FROM messages", &[]).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoincash_addr::Address;
    use ring::digest::{digest, SHA256};

    #[tokio::test]
    async fn get_digest() {
        let database = Database::try_new("postgresql://postgres:root@localhost/relay")
            .await
            .unwrap();
        database.clear_messages().await.unwrap();

        let addr = Address::decode("bchtest:qz35wy0grm4tze4p5tvu0fc6kujsa5vnrcr7y5xl65").unwrap();
        let address_payload = addr.as_body();

        let message = Message::default();
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();
        let digest = digest(&SHA256, &raw_message);

        let timestamp = 100;
        database
            .push_message(
                &address_payload,
                timestamp,
                &raw_message[..],
                digest.as_ref(),
                MESSAGE_NAMESPACE,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_digest() {
        let database = Database::try_new("postgresql://postgres:root@localhost/relay")
            .await
            .unwrap();
        database.clear_messages().await.unwrap();

        let addr = Address::decode("bchtest:qz35wy0grm4tze4p5tvu0fc6kujsa5vnrcr7y5xl65").unwrap();
        let address_payload = addr.as_body();

        let message = Message::default();
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();
        let digest = digest(&SHA256, &raw_message);

        let timestamp = 100;
        database
            .push_message(
                &address_payload,
                timestamp,
                &raw_message[..],
                digest.as_ref(),
                MESSAGE_NAMESPACE,
            )
            .await
            .unwrap();

        assert!(database
            .remove_message_by_digest(&address_payload, digest.as_ref(), MESSAGE_NAMESPACE)
            .await
            .unwrap()
            .is_some());

        assert!(database
            .get_message_by_digest(&address_payload, digest.as_ref(), MESSAGE_NAMESPACE)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_time_range() {
        let database = Database::try_new("postgresql://postgres:root@localhost/relay")
            .await
            .unwrap();
        database.clear_messages().await.unwrap();

        let addr = Address::decode("bchtest:qz35wy0grm4tze4p5tvu0fc6kujsa5vnrcr7y5xl65").unwrap();
        let address_payload = addr.as_body();

        let mut message = Message::default();
        message.payload_digest = vec![0; 32];
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap();
        let digest = digest(&SHA256, &raw_message);

        // Put at 100 and 105
        database
            .push_message(
                &address_payload,
                100,
                &raw_message[..],
                digest.as_ref(),
                MESSAGE_NAMESPACE,
            )
            .await
            .unwrap();
        database
            .push_message(
                &address_payload,
                105,
                &raw_message[..],
                digest.as_ref(),
                MESSAGE_NAMESPACE,
            )
            .await
            .unwrap();

        // Check out of range [106, inf)
        let prefix = msg_prefix(&address_payload, 106, MESSAGE_NAMESPACE);
        assert_eq!(
            database.get_messages_range(&prefix, None).unwrap().messages,
            vec![]
        );

        // Check within range [100, inf)
        let prefix = msg_prefix(&address_payload, 100, MESSAGE_NAMESPACE);
        assert_eq!(
            database
                .get_messages_range(&prefix, None)
                .unwrap()
                .messages
                .len(),
            2
        );

        // Check within range [100, 101)
        let prefix = msg_prefix(&address_payload, 100, MESSAGE_NAMESPACE);
        let prefix_end = msg_prefix(&address_payload, 101, MESSAGE_NAMESPACE);
        assert_eq!(
            database
                .get_messages_range(&prefix, Some(&prefix_end))
                .unwrap()
                .messages
                .len(),
            1
        );

        // Check within range [101, 105)
        let prefix = msg_prefix(&address_payload, 101, MESSAGE_NAMESPACE);
        let prefix_end = msg_prefix(&address_payload, 105, MESSAGE_NAMESPACE);
        assert_eq!(
            database
                .get_messages_range(&prefix, Some(&prefix_end))
                .unwrap()
                .messages
                .len(),
            0
        );
    }
}
