use std::sync::Arc;

use bitcoincash_addr::Address;
use dashmap::DashMap;
use futures::prelude::*;
use tokio::sync::broadcast;
use warp::{
    ws::{Message, WebSocket, Ws},
    Reply,
};

use super::errors::*;

const BROADCAST_CHANNEL_CAPACITY: usize = 256;

// pubkey hash:serialized timed message
pub type MessageBus = Arc<DashMap<Vec<u8>, broadcast::Sender<Vec<u8>>>>;

pub async fn upgrade_ws(
    addr_str: String,
    ws: Ws,
    msg_bus: MessageBus,
) -> Result<impl Reply, ServerError> {
    // Convert address
    let addr = Address::decode(&addr_str)?;
    let pubkey_hash = addr.into_body();

    Ok(ws.on_upgrade(move |socket| connect_ws(pubkey_hash, socket, msg_bus)))
}

#[derive(Debug)]
enum WsError {
    SinkError(warp::Error),
    BusError(broadcast::RecvError),
}

pub async fn connect_ws(pubkey_hash: Vec<u8>, ws: WebSocket, msg_bus: MessageBus) {
    let rx = msg_bus
        .entry(pubkey_hash.clone())
        .or_insert(broadcast::channel(BROADCAST_CHANNEL_CAPACITY).0)
        .subscribe()
        .map_ok(|res| Message::binary(res))
        .map_err(WsError::BusError);

    let (user_ws_tx, _) = ws.split();

    if let Err(err) = rx
        .forward(user_ws_tx.sink_map_err(WsError::SinkError))
        .await
    {
        // TODO: Log error
    }

    // TODO: Disconnect using https://github.com/xacrimon/dashmap/issues/56
}
