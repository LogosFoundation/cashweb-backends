#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde;

pub mod db;
pub mod models;
pub mod net;
pub mod settings;

use std::sync::Arc;

use dashmap::DashMap;
use futures::TryFutureExt;
use lazy_static::lazy_static;
use warp::Filter;

use crate::{db::Database, settings::Settings};

const DASHMAP_CAPACITY: usize = 2048;

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new().expect("couldn't load config");
}

#[tokio::main]
async fn main() {
    // Open DB
    let db = Database::try_new(&SETTINGS.db_path).expect("failed to open database");

    // Init message bus
    let message_bus = Arc::new(DashMap::with_capacity(DASHMAP_CAPACITY));
    let msg_bus_state = warp::any().map(move || message_bus.clone());

    // Database state
    let db_state = warp::any().map(move || db.clone());

    // Message handlers
    let messages = warp::path::param().and(warp::path("messages"));
    let messages_get = messages
        .and(warp::get())
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
    let websocket = warp::path::param()
        .and(warp::path("ws"))
        .and(warp::ws())
        .and(msg_bus_state)
        .and_then(|addr, ws: warp::ws::Ws, msg_bus| {
            net::upgrade_ws(addr, ws, msg_bus).map_err(warp::reject::custom)
        });

    // Filter handlers
    let filters = warp::path::param().and(warp::path("filters"));
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
        .recover(net::errors::handle_rejection);
    warp::serve(server).run(SETTINGS.bind).await;
}
