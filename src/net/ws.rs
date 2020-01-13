use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;

const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

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
        // Start heartbeat
        self.hb(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for MessagingSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // process websocket messages
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
) -> Result<HttpResponse, Error> {
    // TODO: Get addr stream
    let res = ws::start(MessagingSocket::new(), &request, stream);
    res
}
