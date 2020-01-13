pub mod errors;
pub mod payments;
pub mod ws;

use std::{
    convert::TryFrom,
    time::{SystemTime, UNIX_EPOCH},
};

use actix_web::{web, HttpResponse};
use bytes::BytesMut;
use futures::prelude::*;
use prost::Message as _;

use crate::{
    crypto::Address,
    db::Database,
    models::{filters::FilterApplication, messaging::MessageSet},
};

use errors::ServerError;

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
) -> Result<HttpResponse, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;

    // Decode metadata
    let mut messages_raw = BytesMut::new();
    while let Some(item) = payload.next().await {
        messages_raw.extend_from_slice(&item.map_err(ServerError::Buffer)?);
    }

    // TODO: Do validation
    let message_page =
        MessageSet::decode(&messages_raw[..]).map_err(ServerError::MessagesDecode)?;

    let timestamp = get_unix_now();
    for message in message_page.messages {
        let mut raw_message = Vec::with_capacity(message.encoded_len());
        message.encode(&mut raw_message).unwrap(); // This is safe
        db_data.push_message(addr.as_body(), &raw_message[..], timestamp)?;
    }

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
