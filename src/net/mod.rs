pub mod errors;
pub mod payments;
pub mod ws;

use std::{
    convert::TryFrom,
    time::{SystemTime, UNIX_EPOCH},
};

use actix::Addr;
use actix_web::{web, HttpResponse};
use bitcoin::{util::psbt::serialize::Deserialize, Transaction};
use bitcoin_hashes::{hash160, sha256, Hash};
use bytes::BytesMut;
use futures::prelude::*;
use json_rpc::clients::http::HttpConnector;
use prost::Message as _;
use secp256k1::{
    key::{PublicKey, SecretKey},
    Secp256k1,
};

use crate::{
    bitcoin::*,
    crypto::Address,
    db::{BoxType, Database},
    models::{
        filters::FilterApplication,
        messaging::{MessageSet, Payload, TimedMessageSet},
    },
    ws::bus::MessageBus,
};

use errors::{ServerError, StampError};

#[derive(Deserialize)]
pub struct GetQuery {
    start: u64,
    end: Option<u64>,
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
    addr_str: web::Path<String>,
    db_data: web::Data<Database>,
    query: web::Query<GetQuery>,
) -> Result<HttpResponse, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Grab metadata from DB
    let message_set = db_data.get_messages(addr.as_body(), query.start, query.end)?;

    // Serialize messages
    let mut raw_payload = Vec::with_capacity(message_set.encoded_len());
    message_set.encode(&mut raw_payload).unwrap();

    // Respond
    Ok(HttpResponse::Ok().body(raw_payload))
}

pub async fn put_message(
    addr_str: web::Path<String>,
    mut payload: web::Payload,
    db_data: web::Data<Database>,
    bitcoin_client: web::Data<BitcoinClient<HttpConnector>>,
    msg_bus: web::Data<Addr<MessageBus>>,
) -> Result<HttpResponse, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Decode metadata
    let mut messages_raw = BytesMut::new();
    while let Some(item) = payload.next().await {
        messages_raw.extend_from_slice(&item.map_err(ServerError::Buffer)?);
    }

    // Validation
    let message_set = MessageSet::decode(&messages_raw[..]).map_err(ServerError::MessagesDecode)?;
    for message in &message_set.messages {
        let stamp_tx = &message.stamp_tx;

        // Get pubkey hash from stamp tx
        let tx = Transaction::deserialize(stamp_tx).map_err(StampError::Decode)?;
        let output = tx.output.get(0).ok_or(StampError::MissingOutput)?;
        let script = &output.script_pubkey;
        if !script.is_p2pkh() {
            return Err(ServerError::Stamp(StampError::NotP2PKH));
        }
        let pubkey_hash = &script.as_bytes()[3..23]; // This is safe as we've checked it's a p2pkh

        // Calculate payload pubkey hash
        let payload_digest = sha256::Hash::hash(&message.serialized_payload[..]);
        let payload_secret_key = SecretKey::from_slice(&payload_digest).unwrap(); // TODO: Check this is safe
        let payload_public_key =
            PublicKey::from_secret_key(&Secp256k1::signing_only(), &payload_secret_key);

        // Get destination public key
        let payload =
            Payload::decode(&message.serialized_payload[..]).map_err(ServerError::PayloadDecode)?;
        let destination_public_key = PublicKey::from_slice(&payload.destination[..])
            .map_err(|_| ServerError::DestinationMalformed)?;

        // Combine keys
        let combined_key = destination_public_key
            .combine(&payload_public_key)
            .map_err(|_| ServerError::DegenerateCombination)?;
        let combine_key_raw = combined_key.serialize();
        let combine_pubkey_hash = hash160::Hash::hash(&combine_key_raw[..]).into_inner();

        // Check equivalence
        if combine_pubkey_hash != pubkey_hash {
            return Err(ServerError::Stamp(StampError::UnexpectedAddress));
        }

        bitcoin_client
            .send_tx(stamp_tx)
            .await
            .map_err(StampError::TxReject)?;
    }

    let timestamp = get_unix_now();
    for message in &message_set.messages {
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap(); // This is safe
        db_data.push_message(addr.as_body(), BoxType::Inbox, timestamp, &raw_message[..])?;
    }

    // Create WS message
    let timed_message_set = TimedMessageSet {
        timestamp: timestamp as i64,
        messages: message_set.messages,
    };
    let mut timed_msg_set_raw = Vec::with_capacity(timed_message_set.encoded_len());
    timed_message_set.encode(&mut timed_msg_set_raw).unwrap(); // This is safe

    // Send over WS
    let send_ws = async move {
        let send_message = ws::bus::SendMessage {
            addr: addr.into_body(),
            timed_msg_set_raw,
        };
        if let Err(err) = msg_bus.as_ref().send(send_message).await {
            error!("{:#?}", err);
        }
    };
    actix_rt::spawn(send_ws);

    // Respond
    Ok(HttpResponse::Ok().finish())
}

pub async fn get_filters(
    addr_str: web::Path<String>,
    db_data: web::Data<Database>,
) -> Result<HttpResponse, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Get filters
    let mut filters = db_data
        .get_filters(addr.as_body())?
        .ok_or(ServerError::NotFound)?;

    // Don't show private filters
    if let Some(price_filter) = &filters.price_filter {
        if !price_filter.public {
            filters.price_filter = None;
        }
    }

    // Serialize messages
    let mut raw_payload = Vec::with_capacity(filters.encoded_len());
    filters.encode(&mut raw_payload).unwrap();

    // Respond
    Ok(HttpResponse::Ok().body(raw_payload))
}

pub async fn put_filters(
    addr_str: web::Path<String>,
    mut payload: web::Payload,
    db_data: web::Data<Database>,
) -> Result<HttpResponse, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Decode filters
    let mut filters_raw = BytesMut::new();
    while let Some(item) = payload.next().await {
        filters_raw.extend_from_slice(&item.map_err(ServerError::Buffer)?);
    }

    // TODO: Do validation
    let filter_application =
        FilterApplication::decode(filters_raw).map_err(ServerError::FilterDecode)?;

    db_data.put_filters(addr.as_body(), &filter_application.serialized_filters)?;

    // Respond
    Ok(HttpResponse::Ok().finish())
}
