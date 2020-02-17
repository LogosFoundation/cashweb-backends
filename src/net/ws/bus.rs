use std::collections::HashMap;

use actix::{fut::wrap_future, Actor, Addr, AsyncContext, Handler, Message};
use futures::{FutureExt, TryFutureExt};

use super::{MessagingSocket, SendMessageSet};

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
    pub addr: Vec<u8>,
    pub actor_addr: Addr<MessagingSocket>,
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
    pub raw_addr: Vec<u8>,
    pub actor_addr: Addr<MessagingSocket>,
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
    pub timed_msg_set_raw: Vec<u8>, // TODO: Make Bytes
}

impl Message for SendMessage {
    type Result = ();
}

impl Handler<SendMessage> for MessageBus {
    type Result = ();

    fn handle(&mut self, msg: SendMessage, ctx: &mut actix::Context<Self>) {
        if let Some(sockets) = self.ws_map.get(&msg.addr) {
            for addr in sockets {
                let send_message_set = SendMessageSet(msg.timed_msg_set_raw.clone());
                let send_message_fut = addr
                    .send(send_message_set)
                    .map_err(|err| error!("{:#?}", err))
                    .map(|_| ());
                ctx.spawn(wrap_future(send_message_fut));
            }
        }
    }
}
