use crate::crypto::sha256;
use crate::models::broadcast::BroadcastMessage;
use cashweb::auth_wrapper::{AuthWrapper, AuthWrapperSet, BurnOutputs};
use cashweb::bitcoin::{
    transaction::{DecodeError as TransactionDecodeError, Transaction},
    Decodable,
};
use cashweb::bitcoin_client::{BitcoinClient, NodeError};
use prost::Message;
use std::collections::HashMap;
use std::convert::TryInto;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use warp::http::Response;
use warp::{reject::Reject, Rejection, Reply};

use super::{PubSubDatabase, PubSubDatabaseError};

#[derive(Debug, Error)]
pub enum MessagesRpcRejection {
    #[error("protobuf decode error: {0}")]
    ProtoBufDecodeError(#[from] prost::DecodeError),
    #[error("DB Error error: {0}")]
    BitcoinRPCError(#[from] NodeError),
    #[error("DB Error error: {0}")]
    DatabaseError(#[from] PubSubDatabaseError),
    #[error("payload contains an transaction with a burn output in the wrong format")]
    InvalidOutputFormat,
    #[error("burn transaction commitment incorrect")]
    InvalidOutputCommitment,
    #[error("unable to decode a burn transaction")]
    TransactionInvalidError(#[from] TransactionDecodeError),
    #[error("invalid transaction output amount")]
    TransactionOutputInvalid,
    #[error("invalid topic format")]
    InvalidTopicFormat,
}

impl Reject for MessagesRpcRejection {}

static POND_PREFIX: [u8; 4] = [80, 79, 78, 68];

pub async fn get_messages(
    db: PubSubDatabase,
    topic: String,
    from: i64,
    to: i64,
) -> Result<impl Reply, Rejection> {
    let messages = db
        .get_messages_to(&topic, from, to)
        .map_err(MessagesRpcRejection::DatabaseError)?;
    let mut message_page = AuthWrapperSet::default();
    message_page.items = messages;
    // Serialze message which is stored in database
    let mut raw_message_page = Vec::with_capacity(message_page.encoded_len());
    message_page.encode(&mut raw_message_page).unwrap();

    Ok(Response::builder().body(raw_message_page).unwrap())
}

pub async fn get_message(
    db: PubSubDatabase,
    payload_digest: Vec<u8>,
) -> Result<impl Reply, Rejection> {
    let message = db
        .get_message(&payload_digest)
        .map_err(MessagesRpcRejection::DatabaseError)?;
    let mut raw_message = Vec::with_capacity(message.encoded_len());
    message.encode(&mut raw_message).unwrap();

    Ok(Response::builder().body(raw_message).unwrap())
}

const COMMITMENT_LENGTH: usize = 1 /* OP_RETURN */
    + 1 /* PUSH4 */
    + 4 /* PREFIX */
    + 1 /* OP_0/OP_1 (DOWN/UP) */
    + 1 /* PUSH32 */
    + 32 /* PAYLOAD HASH */;

struct BurnOutputsWithAmounts(BurnOutputs, i64);

pub async fn put_message(
    db: PubSubDatabase,
    client: impl BitcoinClient,
    mut message: AuthWrapper,
) -> Result<impl Reply, Rejection> {
    if message.transactions.len() == 0 {
        return Err(warp::reject::custom(
            MessagesRpcRejection::InvalidOutputFormat,
        ));
    }
    if message.payload_digest.len() == 0 {
        // Ensure payload_digest is set
        message.payload_digest = sha256(&message.payload).to_vec();
    }

    let payload = BroadcastMessage::decode(message.payload.as_slice())
        .map_err(MessagesRpcRejection::ProtoBufDecodeError)?;
    let split_topic = payload.topic.split(".").collect::<Vec<_>>();
    if split_topic.len() > 10 {
        return Err(warp::reject::custom(
            MessagesRpcRejection::InvalidTopicFormat,
        ));
    }
    let invalid_segments = split_topic.iter().any(|segment| segment.len() == 0);
    if invalid_segments {
        return Err(warp::reject::custom(
            MessagesRpcRejection::InvalidTopicFormat,
        ));
    }

    let mut transactions = HashMap::<Vec<u8>, BurnOutputsWithAmounts>::new();

    // Check if list of burn outputs contain valid burns.
    for transaction in &message.transactions {
        let idx = transaction.index;
        let tx = Transaction::decode(&mut transaction.tx.as_slice())
            .expect("Failed to decode a transaction");
        let output = &tx.outputs[idx as usize];
        if !output.script.is_op_return() {
            return Err(warp::reject::custom(
                MessagesRpcRejection::InvalidOutputFormat,
            ));
        }
        let raw_script = output.script.as_bytes();
        if raw_script.len() != COMMITMENT_LENGTH {
            return Err(warp::reject::custom(
                MessagesRpcRejection::InvalidOutputFormat,
            ));
        }

        // Lord have mercy on your soul
        if raw_script[1] != 4
            || &raw_script[2..6] != &POND_PREFIX
            || !(raw_script[6] == 81 || raw_script[6] == 0)
            || raw_script[7] != 32
        {
            return Err(warp::reject::custom(
                MessagesRpcRejection::InvalidOutputFormat,
            ));
        }
        let upvote = raw_script[6] == 81;
        let commitment = &raw_script[8..COMMITMENT_LENGTH];
        if &message.payload_digest[..] != commitment {
            return Err(warp::reject::custom(
                MessagesRpcRejection::InvalidOutputCommitment,
            ));
        }
        let value: i64 = output
            .value
            .try_into()
            .map_err(|_| MessagesRpcRejection::TransactionOutputInvalid)?;

        let txid = tx.transaction_id();
        let tx_map_key = [txid.as_ref(), idx.to_be_bytes().as_ref()].concat();
        transactions.insert(
            tx_map_key,
            BurnOutputsWithAmounts(transaction.clone(), if upvote { value } else { -value }),
        );
    }

    // Attempt to broadcast the transactions
    for burn in &message.transactions {
        client
            .send_tx(burn.tx.as_ref())
            .await
            .map_err(|err| warp::reject::custom(MessagesRpcRejection::BitcoinRPCError(err)))?;
    }

    // Check to see if this thing already exists, if so just bump the number of burn transactions.
    let existing_value = db.get_message(&message.payload_digest);
    if existing_value.is_ok() && message.payload.len() == 0 {
        let mut wrapper = existing_value.unwrap();
        // Dedupe transactions
        for transaction in &wrapper.transactions {
            let tx = Transaction::decode(&mut transaction.tx.as_slice())
                .map_err(MessagesRpcRejection::TransactionInvalidError)?;
            let idx = transaction.index;
            let output = &tx.outputs[idx as usize];
            let raw_script = output.script.as_bytes();
            let upvote = raw_script[6] == 81;
            let value: i64 = output
                .value
                .try_into()
                .map_err(|_| MessagesRpcRejection::TransactionOutputInvalid)?;

            let txid = tx.transaction_id();
            let tx_map_key = [txid.as_ref(), idx.to_be_bytes().as_ref()].concat();
            transactions.insert(
                tx_map_key,
                BurnOutputsWithAmounts(transaction.clone(), if upvote { value } else { -value }),
            );
        }
        // Update the transactions in the database
        wrapper.transactions = transactions
            .values()
            .map(|burn_output| burn_output.0.clone())
            .collect();
        wrapper.burn_amount = transactions
            .values()
            .map(|burn_output| burn_output.1)
            .sum::<i64>();
        db.update_message(&wrapper)
            .map_err(MessagesRpcRejection::DatabaseError)?;

        return Ok(Response::builder().status(200).body(b"".as_ref()).unwrap());
    }

    // Time now
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // Ensure the burn_amount is correct.
    message.burn_amount = transactions
        .values()
        .map(|burn_output| burn_output.1)
        .sum::<i64>();

    db.put_message(timestamp, &payload.topic, &message)
        .map_err(MessagesRpcRejection::DatabaseError)?;
    Ok(Response::builder().status(200).body(b"".as_ref()).unwrap())
}

#[cfg(test)]
pub mod tests {
    use async_trait::async_trait;
    use cashweb::{
        auth_wrapper::BurnOutputs,
        bitcoin::{
            transaction::{Output, Script},
            Encodable,
        },
        bitcoin_client::NodeError,
    };
    use rocksdb::{Options, DB};

    use super::*;

    struct MockTransactionSender {}

    #[async_trait]
    impl BitcoinClient for MockTransactionSender {
        async fn send_tx(&self, _raw_tx: &[u8]) -> Result<String, NodeError> {
            return Ok("".to_string());
        }
        /// Get a new receiving address from the bitcoin daemon
        async fn get_new_addr(&self) -> Result<String, NodeError> {
            Ok("".to_string())
        }
        /// Get a raw bitcoin transaction by txid
        async fn get_raw_transaction(&self, _tx_id: &[u8]) -> Result<Vec<u8>, NodeError> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_put_message_no_transactions_fail() {
        const TEST_NAME: &str = "./tests/test_put_message_no_transactions_fail";

        // Create database
        let database = PubSubDatabase::new(TEST_NAME).unwrap();

        // Create database wrapper
        let mut wrapper_in = AuthWrapper::default();

        wrapper_in.scheme = 1;
        let mut message = BroadcastMessage::default();
        message.topic = "cashweb.is.amazing".to_string();
        message.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut message_buf = Vec::with_capacity(message.encoded_len());
        message.encode(&mut message_buf).unwrap();
        wrapper_in.payload = message_buf;

        let result = put_message(database.clone(), MockTransactionSender {}, wrapper_in).await;

        assert!(result.is_err(), "Result is error");

        // Destroy database
        drop(database);
        DB::destroy(&Options::default(), TEST_NAME).unwrap();
    }

    #[tokio::test]
    async fn test_put_valid_message() {
        const TEST_NAME: &str = "./tests/test_put_valid_message";

        // Create database
        let database = PubSubDatabase::new(TEST_NAME).unwrap();

        // Create database wrapper
        let mut wrapper_in = AuthWrapper::default();

        wrapper_in.scheme = 1;
        let mut message = BroadcastMessage::default();
        message.topic = "cashweb.is.amazing".to_string();
        message.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let mut message_buf = Vec::with_capacity(message.encoded_len());
        message.encode(&mut message_buf).unwrap();
        wrapper_in.payload = message_buf;

        // Create the burn transaction
        let mut tx = Transaction::default();
        let mut output = Vec::<u8>::with_capacity(COMMITMENT_LENGTH);
        output.push(106);
        output.push(4);
        output.extend_from_slice(&POND_PREFIX);
        output.push(81);
        output.push(32);

        let payload_hash = sha256(&wrapper_in.payload);
        output.extend(payload_hash);

        tx.outputs.push(Output {
            script: Script::from(output),
            value: 0,
        });

        // Buffer with enough space to encode txn.
        let mut tx_buf = Vec::with_capacity(50);
        tx.encode(&mut tx_buf).unwrap();
        wrapper_in.transactions.push(BurnOutputs {
            tx: tx_buf,
            index: 0,
        });

        let result = put_message(database.clone(), MockTransactionSender {}, wrapper_in).await;
        if let Err(err) = result.as_ref() {
            println!("{:?}", err);
        }
        assert!(
            result.expect("Result is error").into_response().status() == 200,
            "Incorrect status code"
        );

        // Destroy database
        drop(database);
        DB::destroy(&Options::default(), TEST_NAME).unwrap();
    }
}
