#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde;

pub mod db;
pub mod models;
pub mod net;
pub mod settings;

use futures::TryFutureExt;
use lazy_static::lazy_static;
use warp::Filter;

use crate::{db::Database, net::errors::*, settings::Settings};

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new().expect("couldn't load config");
}

#[tokio::main]
async fn main() {
    // Open DB
    let db = Database::try_new(&SETTINGS.db_path).expect("failed to open database");

    // Init message bus

    // Message handlers
    let db_inner = db.clone();
    let messages = warp::path::param().and(warp::path("messages"));
    let messages_get = messages
        .and(warp::get())
        .and(warp::query())
        .and_then(move |addr, query| {
            net::get_messages(addr, query, db_inner.clone()).map_err(warp::reject::custom)
        });
    let db_inner = db.clone();
    let messages_put = messages
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.message_size,
        ))
        .and(warp::body::bytes())
        .and_then(move |addr, body| {
            net::put_message(addr, body, db_inner.clone())
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });

    // Filter handlers
    let db_inner = db.clone();
    let filters = warp::path::param().and(warp::path("filters"));
    let filters_get = filters.and(warp::get()).and_then(move |addr| {
        net::get_filters(addr, db_inner.clone()).map_err(warp::reject::custom)
    });
    let filters_put = filters
        .and(warp::put())
        .and(warp::body::content_length_limit(
            SETTINGS.limits.filter_size,
        ))
        .and(warp::body::bytes())
        .and_then(move |addr, body| {
            net::put_filters(addr, body, db.clone())
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });

    // Root handler
    let root = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./static/index.html"));

    // Init REST API
    let server = root
        .or(messages_get)
        .or(messages_put)
        .or(filters_get)
        .or(filters_put)
        .recover(net::errors::handle_rejection);
    warp::serve(server).run(SETTINGS.bind).await;
}
