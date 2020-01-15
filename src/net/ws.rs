use std::collections::HashMap;
use std::time::{Duration, Instant};

use actix::{
    fut::wrap_future,
    prelude::{ActorContext, AsyncContext},
    Actor, Addr, Handler, Message, Running, StreamHandler,
};
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use bitcoincash_addr::Address;
use futures::future::{FutureExt, TryFutureExt};

use super::errors::*;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub type AddrSocketsMap = HashMap<Vec<u8>, Vec<Addr<MessagingSocket>>>;

#[derive(Clone)]
pub struct MessageBus {
    ws_map: AddrSocketsMap,
}

impl Default for MessageBus {
    fn default() -> Self {
        MessageBus {
            ws_map: HashMap::new(),
        }
    }
}

impl Actor for MessageBus {
    type Context = actix::Context<Self>;
}

pub struct NewSocket {
    addr: Vec<u8>,
    actor_addr: Addr<MessagingSocket>,
}

impl Message for NewSocket {
    type Result = ();
}

impl Handler<NewSocket> for MessageBus {
    type Result = ();

    fn handle(&mut self, msg: NewSocket, _: &mut actix::Context<Self>) {
        if let Some(sockets) = self.ws_map.get_mut(&msg.addr) {
            sockets.push(msg.actor_addr);
        } else {
            self.ws_map.insert(msg.addr, vec![msg.actor_addr]);
        }
    }
}

pub struct RemoveSocket {
    raw_addr: Vec<u8>,
    actor_addr: Addr<MessagingSocket>,
}

impl Message for RemoveSocket {
    type Result = ();
}

impl Handler<RemoveSocket> for MessageBus {
    type Result = ();

    fn handle(&mut self, msg: RemoveSocket, _: &mut actix::Context<Self>) {
        if let Some(sockets) = self.ws_map.get_mut(&msg.raw_addr) {
            if let Some((id, _)) = sockets
                .iter()
                .enumerate()
                .find(move |(_, addr)| **addr == msg.actor_addr)
            {
                sockets.remove(id);
            }
        }
    }
}

pub struct SendMessage {
    pub addr: Vec<u8>,
    pub message_set_raw: Vec<u8>, // TODO: Make Bytes
}

impl Message for SendMessage {
    type Result = ();
}

impl Handler<SendMessage> for MessageBus {
    type Result = ();

    fn handle(&mut self, msg: SendMessage, ctx: &mut actix::Context<Self>) {
        if let Some(sockets) = self.ws_map.get(&msg.addr) {
            for addr in sockets {
                let send_message_set = SendMessageSet(msg.message_set_raw.clone());
                let send_message_fut = addr
                    .send(send_message_set)
                    .map_err(|err| error!("{:#?}", err))
                    .map(|_| ());
                ctx.spawn(wrap_future(send_message_fut));
            }
        }
    }
}

pub struct MessagingSocket {
    hb: Instant,
    message_bus: Addr<MessageBus>,
    addr_raw: Vec<u8>,
}

impl MessagingSocket {
    fn new(addr_raw: Vec<u8>, message_bus: Addr<MessageBus>) -> Self {
        MessagingSocket {
            hb: Instant::now(),
            addr_raw,
            message_bus,
        }
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

    fn stopping(&mut self, ctx: &mut Self::Context) -> Running {
        let remove_socket = RemoveSocket {
            raw_addr: self.addr_raw.clone(),
            actor_addr: ctx.address(),
        };
        let terminate_fut = self
            .message_bus
            .send(remove_socket)
            .map_err(|err| error!("{:#?}", err))
            .map(|_| ());
        ctx.spawn(wrap_future(terminate_fut));
        Running::Stop
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

pub struct SendMessageSet(pub Vec<u8>);

impl Message for SendMessageSet {
    type Result = ();
}

impl Handler<SendMessageSet> for MessagingSocket {
    type Result = ();

    fn handle(
        &mut self,
        msg: SendMessageSet,
        ctx: &mut ws::WebsocketContext<Self>,
    ) -> Self::Result {
        ctx.binary(msg.0)
    }
}

pub async fn ws_connect(
    request: HttpRequest,
    addr_str: web::Path<String>,
    stream: web::Payload,
    msg_bus: web::Data<Addr<MessageBus>>,
) -> Result<HttpResponse, Error> {
    // Decode address
    let addr = match Address::decode(&addr_str) {
        Ok(ok) => ok,
        Err((cash_err, base58_err)) => {
            return Err(ServerError::Address(cash_err, base58_err).into())
        }
    };
    let raw_addr = addr.into_body();

    // Start websocket
    let (actor_addr, response) = ws::start_with_addr(
        MessagingSocket::new(raw_addr.clone(), msg_bus.as_ref().clone()),
        &request,
        stream,
    )?;
    let new_socket = NewSocket {
        addr: raw_addr.clone(),
        actor_addr,
    };
    msg_bus.send(new_socket).await.unwrap(); // TODO: Make safe

    Ok(response)
}
