use actix::prelude::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use dashmap::DashMap;
// use rayon::prelude::*;

use std::{
    time::{Duration, Instant}
};

use crate::{
    auth::User,
    orchestrator::{ORC},
    utils::{unix_timestamp}
};


lazy_static!{
    static ref LIVE_USERS: DashMap<u64, i64> = DashMap::new();
}

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// do websocket handshake and start `WSConn` actor
pub async fn ws_conn_setup(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    if let Some(usr) = ORC.user_by_session(&req) {
        if LIVE_USERS.contains_key(&usr.id) {
            return Err(actix_web::error::ErrorConflict("there is already an open connection"));
        }
        return ws::start(
            WSConn::new(usr),
            &req,
            stream
        );
    }
    Err(actix_web::error::ErrorForbidden("only authenticated users may use websocket facilities"))
}
struct WSConn {
    usr: User,
    hb: Instant,
}

impl Actor for WSConn {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        LIVE_USERS.insert(self.usr.id, unix_timestamp());
        self.hb(ctx);
    }
}

/// Handler for `ws::Message`
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WSConn {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                let _msg = text.trim();


                ctx.text(text);
            },
            Ok(ws::Message::Binary(bin)) => {
                ctx.binary(bin)
            },
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
                LIVE_USERS.remove(&self.usr.id);
            }
            _ => {
                ctx.stop();
                LIVE_USERS.remove(&self.usr.id);
            },
        }
    }
}

impl WSConn {
    fn new(usr: User) -> Self {
        Self { usr, hb: Instant::now() }
    }

    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        let usr_id = self.usr.id;
        ctx.run_interval(HEARTBEAT_INTERVAL, move |act, ctx| {
            if Instant::now().duration_since(act.hb) < CLIENT_TIMEOUT {
                ctx.ping(b"pong");
            } else {
                ctx.stop();
                LIVE_USERS.remove(&usr_id);
            }
        });
    }
}