#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde;

pub mod db;
pub mod models;
pub mod net;
pub mod settings;

use std::{sync::Arc, time::Duration};

use cashweb::{
    payments::{preprocess_payment, wallet::Wallet},
    token::schemes::hmac_bearer::HmacTokenScheme,
};
use dashmap::DashMap;
use futures::prelude::*;
use lazy_static::lazy_static;
use warp::Filter;

use db::Database;
use net::{payments, protection};
use settings::Settings;

const DASHMAP_CAPACITY: usize = 2048;
const FILTERS_PATH: &str = "filters";
const WS_PATH: &str = "ws";
const MESSAGES_PATH: &str = "messages";
const PAYMENTS_PATH: &str = "payments";

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new().expect("couldn't load config");
}

#[tokio::main]
async fn main() {
    // Database state
    let db = Database::try_new(&SETTINGS.db_path).expect("failed to open database");
    let db_state = warp::any().map(move || db.clone());

    // Message broadcast state
    let message_bus = Arc::new(DashMap::with_capacity(DASHMAP_CAPACITY));
    let msg_bus_state = warp::any().map(move || message_bus.clone());

    // Wallet state
    let wallet = Wallet::new(Duration::from_millis(SETTINGS.wallet.timeout));
    let wallet_state = warp::any().map(move || wallet.clone());

    // Address string converter
    let addr_base = warp::path::param().and_then(|addr_str: String| async move {
        net::address_decode(&addr_str).map_err(warp::reject::custom)
    });

    // Token generator
    let key = hex::decode(&SETTINGS.hmac_key).expect("unable to interpret hmac key as hex");
    let token_scheme = Arc::new(HmacTokenScheme::new(&key));
    let token_scheme_state = warp::any().map(move || token_scheme.clone());

    // Payment generator
    // let payment_gen =

    // Message handlers
    let messages = addr_base.and(warp::path(MESSAGES_PATH));
    let messages_get = messages
        .and(warp::get())
        .and(warp::header::headers_cloned())
        .and(token_scheme_state.clone())
        .and_then(|addr, headers, token_scheme: Arc<HmacTokenScheme>| {
            protection::pop_protection(addr, headers, token_scheme).map_err(warp::reject::custom)
        })
        .and(warp::query())
        .and(db_state.clone())
        .and_then(move |addr, query, db| {
            net::get_messages(addr, query, db).map_err(warp::reject::custom)
        });
    let messages_put = messages
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.message_size,
        ))
        .and(warp::body::bytes())
        .and(db_state.clone())
        .and_then(move |addr, body, db| {
            net::put_message(addr, body, db)
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });

    // Websocket handler
    let websocket = addr_base
        .and(warp::path(WS_PATH))
        .and(warp::ws())
        .and(msg_bus_state)
        .and_then(|addr, ws: warp::ws::Ws, msg_bus| {
            net::upgrade_ws(addr, ws, msg_bus).map_err(warp::reject::custom)
        });

    // Filter handlers
    let filters = addr_base.and(warp::path(FILTERS_PATH));
    let filters_get = filters
        .and(warp::get())
        .and(db_state.clone())
        .and_then(move |addr, db| net::get_filters(addr, db).map_err(warp::reject::custom));
    let filters_put = filters
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.filter_size,
        ))
        .and(warp::body::bytes())
        .and(db_state)
        .and_then(move |addr, body, db| {
            net::put_filters(addr, body, db).map_err(warp::reject::custom)
        })
        .map(|_| vec![]);

    // Payment handler
    let payments = warp::path(PAYMENTS_PATH)
        .and(warp::put())
        .and(warp::header::headers_cloned())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.filter_size,
        ))
        .and(warp::body::bytes())
        .and_then(move |headers, body| {
            preprocess_payment(headers, body)
                .map_err(payments::PaymentError::Preprocess)
                .map_err(warp::reject::custom)
        })
        .and(wallet_state.clone())
        .and_then(move |payment, wallet| {
            net::process_payment(payment, wallet)
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });

    // Root handler
    let root = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./static/index.html"));

    // Init REST API
    let server = root
        .or(websocket)
        .or(messages_get)
        .or(messages_put)
        .or(filters_get)
        .or(filters_put)
        .or(payments)
        .recover(net::handle_rejection);
    warp::serve(server).run(SETTINGS.bind).await;
}
