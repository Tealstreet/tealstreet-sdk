//! Phemex public wire projection; the engine owns epochs, interests and timers.
use super::*;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, FeedPolicy, Invalidation, TrafficPolicy,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Topic {
    Tickers,
    Book { symbol: String },
    Candle { symbol: String, resolution: u32 },
}

impl PhemexPerpetuals {
    fn public_interests(&self) -> Vec<Interest> {
        let mut topics = vec![("0:tickers".to_owned(), Topic::Tickers)];
        for symbol in self.books.keys() {
            topics.push((
                format!("1:book:{symbol}"),
                Topic::Book {
                    symbol: self.native_symbol(symbol).to_owned(),
                },
            ));
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(resolution) = resolution(*interval) {
                topics.push((
                    format!("2:candle:{symbol}:{resolution}"),
                    Topic::Candle {
                        symbol: self.native_symbol(symbol).to_owned(),
                        resolution,
                    },
                ));
            }
        }
        topics
            .into_iter()
            .map(|(key, topic)| Interest {
                key: key.into(),
                wire_args: serde_json::to_string(&topic)
                    .expect("native topic serializes")
                    .into(),
            })
            .collect()
    }

    pub(super) fn connect_managed_public(
        &mut self,
        cx: &mut Cx<'_, Pending>,
    ) -> Result<(), SocketError> {
        if cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        let feed = match self.public.feed {
            Some(feed) => {
                if cx.socket_epoch(feed).is_some() {
                    return Ok(());
                }
                feed
            }
            None => {
                let feed = cx.register_socket_feed(FeedPolicy {
                    role: safe_cex_engine::SocketChannel::Public,
                    supervision: Config {
                        idle_timeout_ms: 20_000.,
                        ping_interval_ms: Some(5_000.),
                        ..Config::default()
                    },
                    auth: AuthMode::None,
                    recovery: AuthRecovery::Reconnect,
                    permanent_refusal_group: None,
                    traffic: TrafficPolicy::AnyText,
                    controls: ControlPolicy::None,
                    invalidation: Invalidation::Required,
                    subscription_interval_ms: 0.,
                })?;
                self.public.feed = Some(feed);
                feed
            }
        };
        cx.connect_socket(feed, self.ws())?;
        Ok(())
    }

    pub(super) fn public_socket_error(&self, cx: &mut Cx<'_, Pending>, error: SocketError) {
        if let Some(feed) = self.public.feed {
            cx.stop_socket_feed(feed);
        }
        cx.notice(
            Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Phemex public socket admission failed: {error:?}"),
        );
    }

    pub(super) fn sync_public_interests(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(feed) = self.public.feed
            && let Err(error) = cx.set_socket_interests(feed, self.public_interests())
        {
            self.public_socket_error(cx, error);
        }
    }

    fn public_frame(&self, method: &str, params: String) -> Result<String, SocketError> {
        let id = self
            .public
            .request
            .get()
            .checked_add(1)
            .ok_or(SocketError::Capacity)?;
        self.public.request.set(id);
        Ok(format!(
            r#"{{"id":{id},"method":"{method}","params":{params}}}"#
        ))
    }

    pub(super) fn public_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                if self.public.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.public.opening_ping =
                    Some(cx.set_socket_interests(epoch.feed(), self.public_interests())?);
            }
            LifecycleEvent::InterestsCommitted { feed, proposal }
                if self.public.feed == Some(feed) && self.public.opening_ping == Some(proposal) =>
            {
                self.public.opening_ping = None;
                cx.send_after_socket_interests(
                    feed,
                    self.public_frame("server.ping", "[]".into())?,
                )?;
            }
            LifecycleEvent::Ping(epoch) => {
                if self.public.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                cx.send(
                    epoch.socket(),
                    self.public_frame("server.ping", "[]".into())?,
                );
            }
            LifecycleEvent::Reconnect(feed) => {
                if self.public.feed != Some(feed) {
                    return Err(SocketError::Stale);
                }
                self.connect_managed_public(cx)?;
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.public.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.public.opening_ping = None;
                for b in self.books.values_mut() {
                    self.generation += 1;
                    b.generation = self.generation;
                    b.last = None;
                    b.fetching = false;
                    b.buffer.clear();
                    if let Some(t) = b.deadline.take() {
                        cx.clear_timer(t);
                    }
                    if let Some(t) = b.retry.take() {
                        cx.clear_timer(t);
                    }
                }
            }
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn encode_public_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        let mut ordered: Vec<_> = interests.iter().collect();
        ordered.sort_by(|a, b| a.key.cmp(&b.key));
        for interest in ordered {
            let topic: Topic = serde_json::from_str(&interest.wire_args)
                .map_err(|_| SocketError::InvalidInterest)?;
            let (method, params) = match topic {
                Topic::Tickers => (
                    if subscribe {
                        "perp_market24h_pack_p.subscribe"
                    } else {
                        "perp_market24h_pack_p.unsubscribe"
                    },
                    "[]".into(),
                ),
                Topic::Book { symbol } => (
                    if subscribe {
                        "orderbook_p.subscribe"
                    } else {
                        "orderbook_p.unsubscribe"
                    },
                    if subscribe {
                        format!(
                            "[{},false,0]",
                            serde_json::to_string(&symbol).expect("native symbol serializes")
                        )
                    } else {
                        serde_json::to_string(&[symbol]).expect("native symbol serializes")
                    },
                ),
                Topic::Candle { symbol, resolution } => (
                    if subscribe {
                        "kline_p.subscribe"
                    } else {
                        "kline_p.unsubscribe"
                    },
                    format!(
                        "[{},{}]",
                        serde_json::to_string(&symbol).expect("native symbol serializes"),
                        resolution
                    ),
                ),
            };
            emit(EncodedBatch {
                text: self.public_frame(method, params)?,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
}
