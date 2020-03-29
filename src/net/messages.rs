use std::{
    collections::HashMap,
    convert::TryFrom,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use bitcoin_hashes::{hash160, Hash};
use bitcoincash_addr::Address;
use bytes::Bytes;
use hex::FromHexError;
use json_rpc::clients::http::HttpConnector;
use prost::Message as _;
use rocksdb::Error as RocksError;
use secp256k1::key::PublicKey;
use sha2::{Digest, Sha256};
use warp::{http::Response, hyper::Body, reject::Reject};

use super::{ws::MessageBus, IntoResponse};
use crate::{
    bitcoin::{BitcoinClient, NodeError},
    db::{self, Database},
    models::relay::messaging::*,
    stamps::*,
    SETTINGS,
};

#[derive(Debug, Deserialize)]
pub struct Query {
    start_digest: Option<String>,
    end_digest: Option<String>,
    start_time: Option<u64>,
    end_time: Option<u64>,
    digest: Option<String>,
}

#[derive(Debug)]
pub enum GetMessageError {
    DB(RocksError),
    DigestDecode(FromHexError),
    DestinationMalformed,
    NotFound,
    StartBothGiven,
    StartDigestMalformed(FromHexError),
    StartDigestNotFound,
    MissingStart,
    EndBothGiven,
    EndDigestMalformed(FromHexError),
    EndDigestNotFound,
}

impl From<RocksError> for GetMessageError {
    fn from(err: RocksError) -> Self {
        Self::DB(err)
    }
}

impl Reject for GetMessageError {}

impl fmt::Display for GetMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::DB(err) => return err.fmt(f),
            Self::DigestDecode(err) => return err.fmt(f),
            Self::DestinationMalformed => "destination malformed",
            Self::NotFound => "not found",
            Self::StartBothGiven => "both start time and digest given",
            Self::StartDigestMalformed(err) => return err.fmt(f),
            Self::StartDigestNotFound => "start digest not found",
            Self::MissingStart => "missing start",
            Self::EndBothGiven => "both end time and digest given",
            Self::EndDigestMalformed(err) => return err.fmt(f),
            Self::EndDigestNotFound => "end digest not found",
        };
        f.write_str(printable)
    }
}

impl IntoResponse for GetMessageError {
    fn to_status(&self) -> u16 {
        match self {
            Self::DB(_) => 500,
            Self::NotFound => 404,
            _ => 400,
        }
    }
}

fn get_unix_now() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_millis(),
    )
    .expect("we're in the distant future")
}

fn construct_prefixes(addr: &[u8], query: Query, database: &Database) -> Result<(Vec<u8>, Option<Vec<u8>>), GetMessageError> {
    // Get start prefix
    let start_prefix = match (query.start_time, query.start_digest) {
        (Some(start_time), None) => db::msg_prefix(addr, start_time),
        (None, Some(start_digest_hex)) => {
            let start_digest =
                hex::decode(start_digest_hex).map_err(GetMessageError::StartDigestMalformed)?;
            database
                .get_msg_key_by_digest(addr, &start_digest)?
                .ok_or(GetMessageError::StartDigestNotFound)?
        }
        (Some(_), Some(_)) => return Err(GetMessageError::StartBothGiven),
        _ => return Err(GetMessageError::MissingStart),
    };

    // Get end prefix
    let end_prefix = match (query.end_time, query.end_digest) {
        (Some(end_time), None) => Some(db::msg_prefix(addr, end_time)),
        (None, Some(end_digest_hex)) => {
            let start_digest =
                hex::decode(end_digest_hex).map_err(GetMessageError::EndDigestMalformed)?;
            let msg_key = database
                .get_msg_key_by_digest(addr, &start_digest)?
                .ok_or(GetMessageError::EndDigestNotFound)?;
            Some(msg_key)
        }
        (Some(_), Some(_)) => return Err(GetMessageError::EndBothGiven),
        _ => None,
    };

    Ok((start_prefix, end_prefix))
}

pub async fn get_payloads(
    addr: Address,
    query: Query,
    database: Database,
) -> Result<Response<Body>, GetMessageError> {
    // Convert address
    let addr = addr.as_body();

    // If digest query then get single payload
    if let Some(digest) = query.digest {
        let raw_digest = hex::decode(digest).map_err(GetMessageError::DigestDecode)?;
        let raw_message = database
            .get_message_by_digest(addr, &raw_digest[..])?
            .ok_or(GetMessageError::NotFound)?;
        let message = Message::decode(&raw_message[..]).unwrap(); // This is safe
        return Ok(Response::builder().body(Body::from(message.serialized_payload)).unwrap());
    }

    let (start_prefix, end_prefix) = construct_prefixes(addr, query, &database)?;
    let message_set =
        database.get_messages_range(&start_prefix, end_prefix.as_ref().map(|v| &v[..]))?;
    let payloads: Vec<_> = message_set
        .messages
        .into_iter()
        .map(|timed_message| {
            let payload =
                Payload::decode(&timed_message.message.unwrap().serialized_payload[..]).unwrap(); // This is safe
            TimedPayload {
                server_time: timed_message.server_time,
                payload: Some(payload),
            }
        })
        .collect();
    let payload_page = PayloadPage { payloads };

    // Serialize messages
    let mut raw_payload_page = Vec::with_capacity(payload_page.encoded_len());
    payload_page.encode(&mut raw_payload_page).unwrap();

    // Respond
    Ok(Response::builder()
        .body(Body::from(raw_payload_page))
        .unwrap()) // TODO: Headers
}

pub async fn get_messages(
    addr: Address,
    query: Query,
    database: Database,
) -> Result<Response<Body>, GetMessageError> {
    // Convert address
    let addr = addr.as_body();

    // If digest query then get single message
    if let Some(digest) = query.digest {
        let raw_digest = hex::decode(digest).map_err(GetMessageError::DigestDecode)?;
        let message = database
            .get_message_by_digest(addr, &raw_digest[..])?
            .ok_or(GetMessageError::NotFound)?;
        return Ok(Response::builder().body(Body::from(message)).unwrap());
    }

    let (start_prefix, end_prefix) = construct_prefixes(addr, query, &database)?;
    let message_set =
        database.get_messages_range(&start_prefix, end_prefix.as_ref().map(|v| &v[..]))?;

    // Serialize messages
    let mut raw_message_page = Vec::with_capacity(message_set.encoded_len());
    message_set.encode(&mut raw_message_page).unwrap();

    // Respond
    Ok(Response::builder()
        .body(Body::from(raw_message_page))
        .unwrap()) // TODO: Headers
}

// pub async fn delete_messages_inbox(
//     addr_str: String,
//     database: Database,
//     query: GetQuery,
// ) -> Result<Response<()>, ServerError> {
// }

#[derive(Debug)]
pub enum PutMessageError {
    DB(RocksError),
    DestinationMalformed,
    MessagesDecode(prost::DecodeError),
    PayloadDecode(prost::DecodeError),
    Stamp(StampError),
}

impl From<RocksError> for PutMessageError {
    fn from(err: RocksError) -> Self {
        Self::DB(err)
    }
}

impl Reject for PutMessageError {}

impl fmt::Display for PutMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let printable = match self {
            Self::DB(err) => return err.fmt(f),
            Self::DestinationMalformed => "destination malformed",
            Self::MessagesDecode(err) => return err.fmt(f),
            Self::PayloadDecode(err) => return err.fmt(f),
            Self::Stamp(err) => return err.fmt(f),
        };
        f.write_str(printable)
    }
}

impl IntoResponse for PutMessageError {
    fn to_status(&self) -> u16 {
        match self {
            Self::DB(_) => 500,
            Self::Stamp(err) => match err {
                StampError::TxReject(err) => match err {
                    NodeError::Rpc(_) => 400,
                    _ => 500,
                },
                _ => 400,
            },
            _ => 400,
        }
    }
}

pub async fn put_message(
    addr: Address,
    messages_raw: Bytes,
    database: Database,
    bitcoin_client: BitcoinClient<HttpConnector>,
    msg_bus: MessageBus,
) -> Result<Response<Body>, PutMessageError> {
    // Decode message
    let mut message_set =
        MessageSet::decode(&messages_raw[..]).map_err(PutMessageError::MessagesDecode)?;

    // Verify, collect and truncate
    let n_messages = message_set.messages.len();
    let mut digest_pubkey = Vec::with_capacity(n_messages); // Collect pubkey hashes
    for message in message_set.messages.iter_mut() {
        // Get sender public key
        let sender_pubkey = &message.sender_pub_key;
        let sender_pubkey_hash = hash160::Hash::hash(&sender_pubkey[..]).into_inner();

        // Calculate payload hash
        let serialized_payload = &message.serialized_payload[..];
        let payload_digest = Sha256::new().chain(&serialized_payload).result().to_vec();

        // If sender is not self then check stamp
        if addr.as_body() != sender_pubkey_hash {
            // TODO: Check destination matches?

            // Get destination public key
            let payload =
                Payload::decode(serialized_payload).map_err(PutMessageError::PayloadDecode)?;
            let destination_public_key = PublicKey::from_slice(&payload.destination[..])
                .map_err(|_| PutMessageError::DestinationMalformed)?;

            for (n, outpoint) in message.stamp_outpoints.iter().enumerate() {
                verify_stamp(
                    &outpoint.stamp_tx,
                    n as u32,
                    &outpoint.vouts,
                    serialized_payload,
                    destination_public_key,
                    bitcoin_client.clone(),
                )
                .await
                .map_err(PutMessageError::Stamp)?;
            }
        }

        // Add payload digest to message
        message.payload_digest = payload_digest.clone();

        // Collect digest and pubkey hash
        digest_pubkey.push((payload_digest, sender_pubkey_hash));

        // If serialized payload too long then remove it
        if serialized_payload.len() > SETTINGS.websocket.truncation_length as usize {
            message.serialized_payload = Vec::new();
            log::info!("truncated message");
        }
    }

    // Put to database and construct sender map
    let timestamp = get_unix_now();
    let mut sender_map = HashMap::<_, Vec<Message>>::with_capacity(n_messages);
    for (i, message) in message_set.messages.iter().enumerate() {
        // Push to destination key
        let (payload_digest, pubkey_hash) = &digest_pubkey[i];
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap(); // This is safe
        database.push_message(
            addr.as_body(),
            timestamp,
            &raw_message[..],
            &payload_digest[..],
        )?;

        // Push to source key
        database.push_message(
            pubkey_hash,
            timestamp,
            &raw_message[..],
            &payload_digest[..],
        )?;

        // Add to sender map
        let vec_pubkey_hash = pubkey_hash.to_vec();
        if let Some(messages) = sender_map.get_mut(&vec_pubkey_hash) {
            messages.push(message.clone());
        } else {
            let mut messages = Vec::with_capacity(n_messages);
            messages.push(message.clone());
            sender_map.insert(pubkey_hash.to_vec(), messages);
        }
        // sender_map.insert(pubkey_hash.to_vec(), raw_message);
    }

    // Create WS message
    let timed_message_set = TimedMessageSet {
        server_time: timestamp as i64,
        messages: message_set.messages,
    };
    let mut timed_msg_set_raw = Vec::with_capacity(timed_message_set.encoded_len());
    timed_message_set.encode(&mut timed_msg_set_raw).unwrap(); // This is safe

    // Send over WS to receiver
    if let Some(sender) = msg_bus.get(addr.as_body()) {
        sender.value().send(timed_msg_set_raw);
    }

    // Send over WS to senders
    for (sender_pubkey_hash, messages) in sender_map {
        if let Some(sender) = msg_bus.get(&sender_pubkey_hash) {
            let timed_message_set = TimedMessageSet {
                server_time: timestamp as i64,
                messages,
            };
            let mut timed_msg_set_raw = Vec::with_capacity(timed_message_set.encoded_len());
            timed_message_set.encode(&mut timed_msg_set_raw).unwrap(); // This is safe
            sender.value().send(timed_msg_set_raw);
        }
    }

    // Respond
    Ok(Response::builder().body(Body::empty()).unwrap())
}
