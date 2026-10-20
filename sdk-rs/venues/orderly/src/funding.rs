//! Orderly limits each public socket to one hundred topics. Keep funding on
//! dedicated ninety-topic shards so books and candles retain the main socket.
use crate::{Connection, Orderly, Pending, wire::Envelope};
use safe_cex_engine::{Cx, LogLevel, SocketId, TimerId};
use serde::Serialize;

const TOPICS_PER_SOCKET: usize = 90;

#[derive(Default)]
pub(crate) struct FundingState {
    shards: Vec<Shard>,
}
struct Shard {
    connection: Connection,
    natives: Vec<String>,
}

impl Orderly {
    pub(crate) fn funding_start(&mut self, cx: &mut Cx<'_, Pending>) {
        for shard in std::mem::take(&mut self.funding.shards) {
            if let Some(socket) = shard.connection.socket {
                cx.close(socket);
            }
            for timer in [shard.connection.watchdog, shard.connection.reconnect]
                .into_iter()
                .flatten()
            {
                cx.clear_timer(timer);
            }
        }
        let mut natives: Vec<_> = self.native.values().cloned().collect();
        natives.sort();
        for (index, chunk) in natives.chunks(TOPICS_PER_SOCKET).enumerate() {
            self.funding.shards.push(Shard {
                connection: Connection {
                    socket: Some(cx.connect_feed(
                        self.public_url(),
                        safe_cex_engine::SocketChannel::Public,
                        format!("funding-{index}"),
                        false,
                    )),
                    received: cx.now,
                    payload_at: cx.now,
                    watchdog: Some(cx.set_timer(10000.0)),
                    ..Default::default()
                },
                natives: chunk.to_vec(),
            });
        }
    }
    pub(crate) fn funding_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        let Some(shard) = self
            .funding
            .shards
            .iter_mut()
            .find(|s| s.connection.socket == Some(socket))
        else {
            return false;
        };
        let connection = &mut shard.connection;
        // A duplicate open callback must not duplicate subscriptions/timers.
        if connection.open {
            return true;
        }
        connection.open = true;
        connection.received = cx.now;
        connection.ping_at = cx.now;
        if let Some(timer) = connection.watchdog.take() {
            cx.clear_timer(timer);
        }
        connection.watchdog = Some(cx.set_timer(10000.0));
        cx.send(socket, "{\"event\":\"ping\"}");
        #[derive(Serialize)]
        struct Subscription<'a> {
            event: &'a str,
            topic: String,
        }
        for native in &shard.natives {
            cx.send(
                socket,
                serde_json::to_string(&Subscription {
                    event: "subscribe",
                    topic: format!("{native}@estfundingrate"),
                })
                .unwrap(),
            );
        }
        true
    }
    pub(crate) fn funding_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        let Some(index) = self
            .funding
            .shards
            .iter()
            .position(|s| s.connection.socket == Some(socket))
        else {
            return false;
        };
        if !self.funding.shards[index].connection.open {
            return true;
        }
        let mut rejected = false;
        let result: Result<(), String> = (|| {
            let env: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let connection = &mut self.funding.shards[index].connection;
            connection.received = cx.now;
            match env.event.as_ref() {
                "ping" => {
                    cx.send(socket, "{\"event\":\"pong\"}");
                    return Ok(());
                }
                "pong" => return Ok(()),
                _ => {}
            }
            if env.success == Some(false) {
                rejected = true;
                return Err(format!(
                    "Orderly funding subscription rejected: {}",
                    env.message
                ));
            }
            if let Some((native, "estfundingrate")) = env.topic.split_once('@') {
                if !self.funding.shards[index]
                    .natives
                    .iter()
                    .any(|s| s == native)
                {
                    return Err("Orderly funding update arrived on the wrong shard".into());
                }
                let data = env.data.ok_or("Orderly funding update missing data")?;
                self.apply_funding(cx, native, data.get())?;
                self.funding.shards[index].connection.attempts = 0;
                self.funding.shards[index].connection.payload_at = cx.now;
                self.funding.shards[index].connection.recover = false;
            }
            Ok(())
        })();
        if let Err(error) = result {
            cx.log(LogLevel::Error, error);
            if rejected {
                self.funding.shards[index].connection.recover = true;
                cx.close(socket);
                self.funding_close(cx, socket);
            }
        }
        true
    }
    pub(crate) fn funding_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        let Some(shard) = self
            .funding
            .shards
            .iter_mut()
            .find(|s| s.connection.socket == Some(socket))
        else {
            return false;
        };
        let connection = &mut shard.connection;
        connection.socket = None;
        connection.open = false;
        for timer in [connection.watchdog.take(), connection.reconnect.take()]
            .into_iter()
            .flatten()
        {
            cx.clear_timer(timer);
        }
        connection.attempts = connection.attempts.saturating_add(1);
        connection.reconnect = Some(
            cx.set_timer((1000.0 * 2f64.powi(connection.attempts.min(6) as i32 - 1)).min(30000.0)),
        );
        true
    }
    pub(crate) fn funding_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(index) = self
            .funding
            .shards
            .iter()
            .position(|s| s.connection.reconnect == Some(timer))
        {
            let socket = cx.connect_feed(
                self.public_url(),
                safe_cex_engine::SocketChannel::Public,
                format!("funding-{index}"),
                self.funding.shards[index].connection.recover,
            );
            let connection = &mut self.funding.shards[index].connection;
            connection.reconnect = None;
            connection.socket = Some(socket);
            connection.received = cx.now;
            connection.payload_at = cx.now;
            connection.watchdog = Some(cx.set_timer(10000.0));
            return true;
        }
        if let Some(index) = self
            .funding
            .shards
            .iter()
            .position(|s| s.connection.watchdog == Some(timer))
        {
            let connection = &mut self.funding.shards[index].connection;
            connection.watchdog = None;
            if let Some(socket) = connection.socket {
                if !connection.open
                    || cx.now - connection.received >= 20000.0
                    || cx.now - connection.payload_at >= 30000.0
                {
                    connection.recover =
                        !connection.open || cx.now - connection.payload_at >= 30000.0;
                    cx.close(socket);
                    self.funding_close(cx, socket);
                } else {
                    connection.ping_at = cx.now;
                    cx.send(socket, "{\"event\":\"ping\"}");
                    connection.watchdog = Some(cx.set_timer(10000.0));
                }
            }
            return true;
        }
        false
    }
}
