#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde;

pub mod bitcoin;
pub mod db;
pub mod models;
pub mod net;
pub mod settings;

use std::io;

use env_logger::Env;
use lazy_static::lazy_static;
use warp::Filter;

use crate::{
    bitcoin::{BitcoinClient, WalletState},
    db::Database,
    net::*,
    settings::Settings,
};

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

    // Init wallet
    let wallet_state = WalletState::default();

    // Init Bitcoin client
    let bitcoin_client = BitcoinClient::new(
        format!("http://{}:{}", SETTINGS.node_ip.clone(), SETTINGS.rpc_port),
        SETTINGS.rpc_username.clone(),
        SETTINGS.rpc_password.clone(),
    );

    // Init message bus

    // Init REST server
    let inbox = warp::path::param::<String>()
        .and(warp::path("inbox"))
        .and(warp::path::query());
    let inbox_put = inbox
        .and(warp::body::content_length_limit(1024 * 16))
        .and(warp::body::bytes())
        .and(warp::put());
    let outbox = warp::path::param()
        .and(warp::path("outbox"))
        .and(warp::path::param());
    let server = warp::get()
        .and(warp::path::end())
        .and(warp::fs::file("./static/index.html"))
        .or(inbox)
        .or(outbox);
}
