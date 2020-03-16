#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde;

pub mod bitcoin;
pub mod db;
pub mod models;
pub mod net;
pub mod settings;
pub mod stamps;

use std::{env, sync::Arc, time::Duration};

use cashweb::{
    payments::{preprocess_payment, wallet::Wallet},
    token::schemes::hmac_bearer::HmacTokenScheme,
};
use dashmap::DashMap;
use futures::prelude::*;
use lazy_static::lazy_static;
use warp::{
    http::{header, Method},
    Filter,
};

use crate::bitcoin::BitcoinClient;
use db::Database;
use net::{payments, protection};
use settings::Settings;

const DASHMAP_CAPACITY: usize = 2048;
const PROFILE_PATH: &str = "profile";
const WS_PATH: &str = "ws";
const MESSAGES_PATH: &str = "messages";
pub const PAYMENTS_PATH: &str = "payments";

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new().expect("couldn't load config");
}

#[tokio::main]
async fn main() {
    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "cash-relay=info");
    }
    pretty_env_logger::init();

    // Database state
    let db = Database::try_new(&SETTINGS.db_path).expect("failed to open database");
    let db_state = warp::any().map(move || db.clone());

    // Message broadcast state
    let message_bus = Arc::new(DashMap::with_capacity(DASHMAP_CAPACITY));
    let msg_bus_state = warp::any().map(move || message_bus.clone());

    // Wallet state
    let wallet = Wallet::new(Duration::from_millis(SETTINGS.wallet.timeout));
    let wallet_state = warp::any().map(move || wallet.clone());

    // Bitcoin client state
    let bitcoin_client = BitcoinClient::new(
        SETTINGS.rpc_addr.clone(),
        SETTINGS.rpc_username.clone(),
        SETTINGS.rpc_password.clone(),
    );
    let bitcoin_client_state = warp::any().map(move || bitcoin_client.clone());

    // Address string converter
    let addr_base = warp::path::param().and_then(|addr_str: String| async move {
        net::address_decode(&addr_str).map_err(warp::reject::custom)
    });

    // Token generator
    let key = hex::decode(&SETTINGS.hmac_secret).expect("unable to interpret hmac key as hex");
    let token_scheme = Arc::new(HmacTokenScheme::new(&key));
    let token_scheme_state = warp::any().map(move || token_scheme.clone());

    // Protection
    let addr_protected = addr_base
        .clone()
        .and(warp::header::headers_cloned())
        .and(token_scheme_state.clone())
        .and(wallet_state.clone())
        .and(bitcoin_client_state.clone())
        .and_then(move |addr, headers, token_scheme, wallet, bitcoin| {
            protection::pop_protection(addr, headers, token_scheme, wallet, bitcoin)
                .map_err(warp::reject::custom)
        });

    // Message handlers
    let messages_get = warp::path(MESSAGES_PATH)
        .and(addr_protected.clone())
        .and(warp::get())
        .and(warp::query())
        .and(db_state.clone())
        .and_then(move |addr, query, db| {
            net::get_messages(addr, query, db).map_err(warp::reject::custom)
        });
    let messages_put = warp::path(MESSAGES_PATH)
        .and(addr_base)
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.message_size,
        ))
        .and(warp::body::bytes())
        .and(db_state.clone())
        .and(bitcoin_client_state.clone())
        .and(msg_bus_state.clone())
        .and_then(move |addr, body, db, bitcoin_client, msg_bus| {
            net::put_message(addr, body, db, bitcoin_client, msg_bus).map_err(warp::reject::custom)
        });

    // Websocket handler
    let websocket = warp::path(WS_PATH)
        .and(addr_protected.clone())
        .and(warp::ws())
        .and(msg_bus_state)
        .map(|addr, ws: warp::ws::Ws, msg_bus| net::upgrade_ws(addr, ws, msg_bus));

    // Profile handlers
    let profile_get = warp::path(PROFILE_PATH)
        .and(addr_base)
        .and(warp::get())
        .and(db_state.clone())
        .and_then(move |addr, db| net::get_profile(addr, db).map_err(warp::reject::custom));
    let profile_put = warp::path(PROFILE_PATH)
        .and(addr_protected)
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.filter_size,
        ))
        .and(warp::body::bytes())
        .and(db_state)
        .and_then(move |addr, body, db| {
            net::put_profile(addr, body, db).map_err(warp::reject::custom)
        });

    // Payment handler
    let payments = warp::path(PAYMENTS_PATH)
        .and(warp::post())
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
        .and(bitcoin_client_state.clone())
        .and(token_scheme_state)
        .and_then(
            move |payment, wallet, bitcoin_client, token_state| async move {
                net::process_payment(payment, wallet, bitcoin_client, token_state)
                    .await
                    .map_err(warp::reject::custom)
            },
        );

    // Root handler
    let root = warp::path::end()
        .and(warp::get())
        .and(warp::fs::file("./static/index.html"));

    // CORs
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec![Method::GET, Method::PUT, Method::POST, Method::DELETE])
        .allow_headers(vec![header::AUTHORIZATION, header::CONTENT_TYPE])
        .expose_headers(vec![
            header::AUTHORIZATION,
            header::ACCEPT,
            header::LOCATION,
        ])
        .build();

    // Init REST API
    let server = root
        .or(payments)
        .or(websocket)
        .or(messages_get)
        .or(messages_put)
        .or(profile_get)
        .or(profile_put)
        .with(cors)
        .with(warp::log("cash-relay"))
        .recover(net::handle_rejection);
    warp::serve(server).run(SETTINGS.bind).await;
}
