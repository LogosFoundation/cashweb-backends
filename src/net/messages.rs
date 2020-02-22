use std::{
    convert::TryFrom,
    time::{SystemTime, UNIX_EPOCH},
};

use bitcoin::{util::psbt::serialize::Deserialize, Transaction};
use bitcoin_hashes::{hash160, sha256, Hash};
use bitcoincash_addr::Address;
use bytes::Bytes;
use prost::Message as _;
use secp256k1::{
    key::{PublicKey, SecretKey},
    Secp256k1,
};
use sha2::{Digest, Sha256};
use warp::http::Response;

use super::errors::*;
use crate::{
    db::{self, Database},
    models::messaging::{MessageSet, Payload, TimedMessageSet},
};

#[derive(Deserialize)]
pub struct Query {
    start_digest: Option<String>,
    end_digest: Option<String>,
    start_time: Option<u64>,
    end_time: Option<u64>,
    digest: Option<String>,
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

pub async fn get_messages(
    addr_str: String,
    query: Query,
    database: Database,
) -> Result<Response<Vec<u8>>, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;
    let addr = addr.as_body();

    if let Some(digest) = query.digest {
        let raw_digest = hex::decode(digest).map_err(ServerError::DigestDecode)?;
        let message = database
            .get_message_by_digest(addr, &raw_digest[..])?
            .ok_or(ServerError::NotFound)?;
        return Ok(Response::builder().body(message).unwrap());
    }

    // Get start prefix
    let start_prefix = match (query.start_time, query.start_digest) {
        (Some(start_time), None) => db::msg_prefix(addr, start_time),
        (None, Some(start_digest_hex)) => {
            let start_digest =
                hex::decode(start_digest_hex).map_err(ServerError::MalformedStartDigest)?;
            database
                .get_msg_key_by_digest(addr, &start_digest)?
                .ok_or(ServerError::StartDigestNotFound)?
        }
        (Some(_), Some(_)) => return Err(ServerError::StartBothGiven),
        _ => return Err(ServerError::MissingStart),
    };

    // Get end prefix
    let end_prefix = match (query.end_time, query.end_digest) {
        (Some(end_time), None) => Some(db::msg_prefix(addr, end_time)),
        (None, Some(end_digest_hex)) => {
            let start_digest =
                hex::decode(end_digest_hex).map_err(ServerError::MalformedEndDigest)?;
            let msg_key = database
                .get_msg_key_by_digest(addr, &start_digest)?
                .ok_or(ServerError::EndDigestNotFound)?;
            Some(msg_key)
        }
        (Some(_), Some(_)) => return Err(ServerError::EndBothGiven),
        _ => None,
    };

    let message_set =
        database.get_messages_range(&start_prefix, end_prefix.as_ref().map(|v| &v[..]))?;

    // Serialize messages
    let mut raw_payload = Vec::with_capacity(message_set.encoded_len());
    message_set.encode(&mut raw_payload).unwrap();

    // Respond
    Ok(Response::builder().body(raw_payload).unwrap()) // TODO: Headers
}

// pub async fn delete_messages_inbox(
//     addr_str: String,
//     database: Database,
//     query: GetQuery,
// ) -> Result<Response<()>, ServerError> {
// }

async fn verify_stamp(
    stamp_tx: &[u8],
    serialized_payload: &[u8],
    destination_pubkey: PublicKey,
) -> Result<(), StampError> {
    // Get pubkey hash from stamp tx
    let tx = Transaction::deserialize(stamp_tx).map_err(StampError::Decode)?;
    let output = tx.output.get(0).ok_or(StampError::MissingOutput)?;
    let script = &output.script_pubkey;
    if !script.is_p2pkh() {
        return Err(StampError::NotP2PKH);
    }
    let pubkey_hash = &script.as_bytes()[3..23]; // This is safe as we've checked it's a p2pkh

    // Calculate payload pubkey hash
    let payload_digest = sha256::Hash::hash(serialized_payload);
    let payload_secret_key = SecretKey::from_slice(&payload_digest).unwrap(); // TODO: Check this is safe
    let payload_public_key =
        PublicKey::from_secret_key(&Secp256k1::signing_only(), &payload_secret_key);

    // Combine keys
    let combined_key = destination_pubkey
        .combine(&payload_public_key)
        .map_err(|_| StampError::DegenerateCombination)?;
    let combine_key_raw = combined_key.serialize();
    let combine_pubkey_hash = hash160::Hash::hash(&combine_key_raw[..]).into_inner();

    // Check equivalence
    if combine_pubkey_hash != pubkey_hash {
        return Err(StampError::UnexpectedAddress);
    }

    Ok(())

    // bitcoin_client
    //     .send_tx(stamp_tx)
    //     .await
    //     .map_err(StampError::TxReject)?;
}

pub async fn put_message(
    addr_str: String,
    messages_raw: Bytes,
    database: Database,
) -> Result<Response<()>, ServerError> {
    // Convert path address
    let addr = Address::decode(&addr_str)?;
    let message_set = MessageSet::decode(&messages_raw[..]).map_err(ServerError::MessagesDecode)?;

    // Verify
    for message in &message_set.messages {
        // Get sender public key
        let sender_pubkey = &message.sender_pub_key;
        let sender_pubkey_hash = hash160::Hash::hash(&sender_pubkey[..]).into_inner();

        // If sender is not self then check stamp
        if addr.as_body() != sender_pubkey_hash {
            let stamp_tx = &message.stamp_tx;
            let serialized_payload = &message.serialized_payload[..];

            // Get destination public key
            let payload =
                Payload::decode(serialized_payload).map_err(ServerError::PayloadDecode)?;
            let destination_public_key = PublicKey::from_slice(&payload.destination[..])
                .map_err(|_| ServerError::DestinationMalformed)?;

            verify_stamp(stamp_tx, serialized_payload, destination_public_key)
                .await
                .map_err(ServerError::Stamp)?;
        }
    }

    let timestamp = get_unix_now();
    for message in &message_set.messages {
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap(); // This is safe
        let digest = Sha256::new().chain(&raw_message).result();
        database.push_message(addr.as_body(), timestamp, &raw_message[..], &digest[..])?;
    }

    // Create WS message
    let timed_message_set = TimedMessageSet {
        timestamp: timestamp as i64,
        messages: message_set.messages,
    };
    let mut timed_msg_set_raw = Vec::with_capacity(timed_message_set.encoded_len());
    timed_message_set.encode(&mut timed_msg_set_raw).unwrap(); // This is safe

    // Send over WS
    // let send_ws = async move {
    //     let send_message = ws::bus::SendMessage {
    //         addr: addr.into_body(),
    //         timed_msg_set_raw,
    //     };
    //     if let Err(err) = msg_bus.as_ref().send(send_message).await {
    //         error!("{:#?}", err);
    //     }
    // };

    // Respond
    Ok(Response::builder().body(()).unwrap())
}
