#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

pub mod db;
pub mod models;
pub mod net;
pub mod settings;

use env_logger::Env;
use futures::TryFutureExt;
use lazy_static::lazy_static;
use warp::Filter;

use crate::{db::Database, net::errors::*, settings::Settings};

lazy_static! {
    pub static ref SETTINGS: Settings = Settings::new().expect("couldn't load config");
}

#[tokio::main]
async fn main() {
    // Init logging
    env_logger::from_env(Env::default().default_filter_or("actix_web=info,keyserver=info")).init();
    info!("starting server @ {}", SETTINGS.bind);

    // Open DB
    let db = Database::try_new(&SETTINGS.db_path).expect("failed to open database");

    // Init message bus

    // Init REST server
    let db_inner_get = db.clone();
    let messages = warp::path::param().and(warp::path("messages"));
    let messages_get = messages
        .and(warp::query::<net::GetQuery>())
        .and(warp::get())
        .and_then(move |addr, query| {
            net::get_messages(addr, query, db_inner_get.clone())
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });
    let messages_put = messages
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::bytes())
        .and(warp::put())
        .and_then(move |addr, body| {
            net::put_message(addr, body, db.clone())
                .map_ok(|_| vec![])
                .map_err(warp::reject::custom)
        });
    let root = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./static/index.html"));
    let server = root
        .or(messages_get)
        .or(messages_put)
        .recover(net::errors::handle_rejection);

    warp::serve(server).run(SETTINGS.bind).await;
}
