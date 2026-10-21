//! Orderly limits each public socket to one hundred topics. Keep funding on
//! dedicated ninety-topic shards so books and candles retain the main socket.
use crate::{Connection, Orderly, Pending, wire::Envelope};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::socket::{Tick as SocketTick, Timeout};
use safe_cex_engine::{Cx, PublicFeedPolicy, SocketId, TimerId};
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
        for mut shard in std::mem::take(&mut self.funding.shards) {
            if let Some(socket) = shard.connection.socket {
                cx.close(socket);
            }
            shard.connection.supervisor.cancel(cx);
        }
        let mut natives: Vec<_> = self.native.values().cloned().collect();
        natives.sort();
        for (index, chunk) in natives.chunks(TOPICS_PER_SOCKET).enumerate() {
            self.funding.shards.push(Shard {
                connection: Connection::new(None),
                natives: chunk.to_vec(),
            });
            self.funding_connect(cx, index);
        }
    }
    fn funding_connect(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        let url = self.public_url();
        let connection = &mut self.funding.shards[index].connection;
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            connection.supervisor.connecting(cx);
        }
        connection.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(url)
                    .channel(safe_cex_engine::SocketChannel::Public)
                    .feed(format!("funding-{index}"), connection.recover),
            ),
        );
        connection.payload_at = cx.now;
    }
    fn funding_drop(&mut self, cx: &mut Cx<'_, Pending>, index: usize) {
        if let Some(socket) = self.funding.shards[index].connection.socket {
            cx.close(socket);
            self.funding_close(cx, socket);
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
        connection.ping_at = cx.now;
        connection.supervisor.opened(cx);
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
            connection.supervisor.received(cx.now);
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
                self.funding.shards[index].connection.payload_at = cx.now;
                self.funding.shards[index].connection.recover = false;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let code = if rejected {
                code::SUBSCRIPTION_REJECTED
            } else {
                code::STREAM_MALFORMED
            };
            cx.notice(Level::Error, code, error);
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
        connection.supervisor.retry(cx);
        true
    }
    pub(crate) fn funding_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        let Some(index) = self
            .funding
            .shards
            .iter()
            .position(|s| s.connection.supervisor.owns_timer(timer))
        else {
            return false;
        };
        let connection = &mut self.funding.shards[index].connection;
        match connection.supervisor.on_timer(cx, timer) {
            SocketTick::Reconnect => self.funding_connect(cx, index),
            SocketTick::Ping if connection.payload_stale(cx.now) => {
                connection.recover = true;
                self.funding_drop(cx, index);
            }
            SocketTick::Ping => {
                if let Some(socket) = connection.socket {
                    connection.ping_at = cx.now;
                    cx.send(socket, "{\"event\":\"ping\"}");
                }
            }
            SocketTick::TimedOut(timeout) => {
                connection.recover =
                    timeout == Timeout::Connect || connection.payload_stale(cx.now);
                self.funding_drop(cx, index);
            }
            SocketTick::Handled | SocketTick::Unhandled => {}
        }
        true
    }
}
