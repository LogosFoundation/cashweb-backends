use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use actix::{
    prelude::{ActorContext, AsyncContext},
    Actor, Addr, StreamHandler,
};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use bitcoincash_addr::Address;
use parking_lot::RwLock;

use super::errors::*;
use crate::models::messaging::Message;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub type AddrMap = HashMap<usize, Addr<MessagingSocket>>;
pub type AddrSocketsMap = HashMap<Vec<u8>, Sockets>;

#[derive(Clone)]
pub struct Sockets {
    pub nonce: Arc<AtomicUsize>,
    pub map: AddrMap,
}

impl Sockets {
    fn new() -> Self {
        Sockets {
            nonce: Arc::new(AtomicUsize::new(0)),
            map: HashMap::new(),
        }
    }

    fn push(&mut self, addr: Addr<MessagingSocket>) {
        let nonce = self.nonce.fetch_add(1, Ordering::SeqCst);
        self.map.insert(nonce, addr);
    }
}

pub struct MessageBus {
    ws_map: Arc<RwLock<AddrSocketsMap>>,
}

impl MessageBus {
    fn get(&self, addr_raw: &Vec<u8>) -> Option<Sockets> {
        self.ws_map.read().get(addr_raw).cloned()
    }

    fn insert(&self, addr_raw: &Vec<u8>, addr: Addr<MessagingSocket>) {
        let mut write_guard = self.ws_map.write();
        if let Some(sockets) = write_guard.get_mut(addr_raw) {
            sockets.push(addr);
        } else {
            let mut sockets = Sockets::new();
            sockets.push(addr);
            write_guard.insert(addr_raw.clone(), sockets);
        }
    }
}

struct MessagingSocket {
    hb: Instant,
}

impl MessagingSocket {
    fn new() -> Self {
        Self { hb: Instant::now() }
    }

    /// Send a ping every heartbeat
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                info!("client timed-out");
                ctx.stop();
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for MessagingSocket {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.hb(ctx);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MessagingSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        info!("received: {:#?}", msg);
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Close(_)) => {
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

pub async fn ws_connect(
    request: HttpRequest,
    addr_str: web::Path<String>,
    stream: web::Payload,
    msg_bus: web::Data<MessageBus>,
) -> Result<HttpResponse, Error> {
    let addr = match Address::decode(&addr_str) {
        Ok(ok) => ok,
        Err((cash_err, base58_err)) => {
            return Err(ServerError::Address(cash_err, base58_err).into())
        }
    };
    let raw_addr = addr.into_body();
    let (actor_addr, response) = ws::start_with_addr(MessagingSocket::new(), &request, stream)?;
    msg_bus.as_ref().insert(&raw_addr, actor_addr);

    Ok(response)
}
