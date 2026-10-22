//! Native codecs and store invalidation; Engine owns auth, epochs and replay.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
#[derive(serde::Deserialize)]
struct EventName<'a> {
    #[serde(default)]
    event: Option<&'a str>,
}
#[derive(serde::Deserialize)]
struct TopicArgs<'a> {
    channel: &'a str,
    #[serde(rename = "instId", default)]
    inst_id: Option<&'a str>,
    #[serde(rename = "instType", default)]
    inst_type: Option<&'a str>,
}
impl Okx {
    pub(super) fn feed_channel(&self, feed: FeedId) -> Option<usize> {
        self.connections.iter().position(|c| c.feed == Some(feed))
    }
    pub(super) fn desired_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        channel: usize,
    ) -> Vec<Interest> {
        let mut values = Vec::new();
        let mut add = |topic: Topic<'_>| {
            let text = serde_json::to_string(&topic).expect("native topic");
            values.push(Interest {
                key: text.clone(),
                wire_args: text,
            });
        };
        if channel == PUBLIC {
            for name in [
                "tickers",
                "mark-price",
                "index-tickers",
                "open-interest",
                "funding-rate",
            ] {
                let mut seen = HashSet::new();
                for market in store.markets() {
                    let id = if name == "index-tickers" {
                        self.products
                            .get(&market.id)
                            .map_or(market.id.as_str(), |p| p.underlying.as_str())
                    } else {
                        &market.id
                    };
                    if seen.insert(id) {
                        add(Topic::instrument(name, id));
                    }
                }
            }
            for symbol in self.books.keys() {
                if let Some(m) = store.market(symbol) {
                    add(Topic::instrument("books", &m.id));
                }
            }
        } else {
            if self.credentials.is_some() {
                if channel == 1 {
                    add(Topic {
                        channel: "account",
                        inst_id: None,
                        inst_type: None,
                    });
                }
                for kind in self.instrument_types() {
                    for name in if channel == 1 {
                        &["positions", "orders"][..]
                    } else {
                        &["orders-algo", "algo-advance"][..]
                    } {
                        add(Topic {
                            channel: name,
                            inst_id: None,
                            inst_type: Some(kind.as_str()),
                        });
                    }
                }
            }
            if channel == BUSINESS {
                for (symbol, interval) in self.candles.keys() {
                    if let (Some(m), Some(bar)) = (store.market(symbol), bar(*interval)) {
                        add(Topic::instrument(&format!("candle{bar}"), &m.id));
                    }
                }
            }
        }
        values
    }
    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: usize,
    ) -> Result<(), SocketError> {
        if channel != PUBLIC && self.account.startup_rejected {
            return Err(SocketError::NotPermitted);
        }
        if channel == PUBLIC && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        let feed = if let Some(feed) = self.connections[channel].feed {
            feed
        } else {
            let login = channel != PUBLIC && self.credentials.is_some();
            let policy = FeedPolicy {
                role: match channel {
                    PUBLIC => safe_cex_engine::SocketChannel::Public,
                    BUSINESS => safe_cex_engine::SocketChannel::Business,
                    _ => safe_cex_engine::SocketChannel::Private,
                },
                supervision: Config {
                    connect_timeout_ms: 10_000.,
                    auth_timeout_ms: login.then_some(10_000.),
                    idle_timeout_ms: 20_000.,
                    ping_interval_ms: Some(10_000.),
                    ..Config::default()
                },
                auth: if login {
                    AuthMode::Required
                } else {
                    AuthMode::None
                },
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: (channel != PUBLIC).then_some(1),
                traffic: TrafficPolicy::PongOnly,
                invalidation: if channel == PUBLIC {
                    Invalidation::Required
                } else {
                    Invalidation::None
                },
                subscription_interval_ms: 0.,
            };
            let feed = cx.register_socket_feed(policy)?;
            self.connections[channel].feed = Some(feed);
            feed
        };
        cx.connect_socket(
            feed,
            safe_cex_engine::SocketSpec::new(self.socket_url(channel)),
        )?;
        Ok(())
    }
    pub(super) fn managed_control(&self, feed: FeedId, authenticated: bool, text: &str) -> Control {
        if text == "pong" {
            return Control::Pong;
        }
        let Some(channel) = self.feed_channel(feed) else {
            return Control::Other;
        };
        if channel == PUBLIC || !text.trim_start().starts_with('{') {
            return Control::Other;
        }
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            // Recognize a claimed login with a malformed code without treating
            // unrelated malformed traffic as authentication evidence.
            return match serde_json::from_str::<EventName<'_>>(text) {
                Ok(value) if value.event == Some("login") => Control::Auth(AuthEvidence::Malformed),
                _ => Control::Other,
            };
        };
        if message.event == "login" {
            if message.code.is_empty() {
                Control::Auth(AuthEvidence::Malformed)
            } else if message.code == "0" {
                Control::Auth(AuthEvidence::Accepted)
            } else {
                Control::Auth(AuthEvidence::Rejected(account::socket_login_error(
                    message.code,
                    message.msg,
                )))
            }
        } else if message.event == "error" && !authenticated {
            Control::Auth(AuthEvidence::Rejected(account::socket_login_error(
                message.code,
                message.msg,
            )))
        } else {
            Control::Other
        }
    }
    pub(super) fn managed_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                let channel = self.feed_channel(epoch.feed()).ok_or(SocketError::Stale)?;
                if channel != PUBLIC && self.credentials.is_some() {
                    self.login(cx, channel);
                }
                if channel != PUBLIC && self.credentials.is_some() {
                    cx.send(epoch.socket(), "ping");
                }
            }
            LifecycleEvent::Ready { epoch, reopened } => {
                let channel = self.feed_channel(epoch.feed()).ok_or(SocketError::Stale)?;
                self.subscribe(cx, channel);
                if channel == PUBLIC || self.credentials.is_none() {
                    cx.send_after_socket_interests(epoch.feed(), "ping".into())?;
                }
                if channel != PUBLIC && self.credentials.is_some() {
                    self.socket_account_ready(cx, reopened);
                }
            }
            LifecycleEvent::AuthFailure {
                evidence, terminal, ..
            } => {
                let error = match evidence {
                    AuthEvidence::Rejected(e) => e,
                    _ => safe_cex_engine::VenueError::new("Malformed OKX login response"),
                };
                if terminal {
                    self.reject_startup(cx, error, true);
                } else {
                    cx.notice(Level::Error, code::STREAM_UNAVAILABLE, error.message);
                }
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.feed_channel(epoch.feed()) == Some(PUBLIC) {
                    for (symbol, state) in &mut self.books {
                        state.sequence = None;
                        state.recovering = false;
                        state.book.clear();
                        cx.book_changed(symbol);
                    }
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "ping"),
            LifecycleEvent::Reconnect(feed) => {
                let channel = self.feed_channel(feed).ok_or(SocketError::Stale)?;
                self.managed_connect(cx, channel)?;
            }
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }
    pub(super) fn managed_encode(
        &self,
        store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let channel = self.feed_channel(feed).ok_or(SocketError::Stale)?;
        let mut values = items
            .iter()
            .map(|i| {
                serde_json::from_str::<TopicArgs<'_>>(&i.wire_args)
                    .map(|v| (i, v))
                    .map_err(|_| SocketError::InvalidInterest)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let rank = |v: &TopicArgs<'_>| {
            let name = v.channel;
            let kind = usize::from(v.inst_type == Some("FUTURES"));
            let order = if channel == PUBLIC {
                match name {
                    "tickers" => 0,
                    "mark-price" => 1,
                    "index-tickers" => 2,
                    "open-interest" => 3,
                    "funding-rate" => 4,
                    _ => 100,
                }
            } else {
                match name {
                    "account" => 0,
                    "positions" | "orders-algo" => 1 + kind * 2,
                    "orders" | "algo-advance" => 2 + kind * 2,
                    _ => 100,
                }
            };
            let id = v.inst_id.unwrap_or("");
            let index = store
                .markets()
                .iter()
                .position(|m| {
                    m.id == id || self.products.get(&m.id).is_some_and(|p| p.underlying == id)
                })
                .unwrap_or(usize::MAX);
            (order, index)
        };
        values.sort_by_key(|(_, v)| rank(v));
        let mut batch = Vec::new();
        let flush = |batch: &mut Vec<&Interest>,
                     emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>|
         -> Result<(), SocketError> {
            if batch.is_empty() {
                return Ok(());
            }
            let args = batch
                .iter()
                .map(|i| i.wire_args.as_str())
                .collect::<Vec<_>>()
                .join(",");
            emit(EncodedBatch {
                text: format!(
                    r#"{{"op":"{}","args":[{}]}}"#,
                    if subscribe {
                        "subscribe"
                    } else {
                        "unsubscribe"
                    },
                    args
                ),
                covered_keys: batch.iter().map(|i| i.key.clone()).collect(),
            })?;
            batch.clear();
            Ok(())
        };
        for (item, value) in values {
            if rank(&value).0 == 100 {
                flush(&mut batch, emit)?;
                batch.push(item);
                flush(&mut batch, emit)?;
            } else {
                batch.push(item);
                if batch.len() == 100 {
                    flush(&mut batch, emit)?;
                }
            }
        }
        flush(&mut batch, emit)
    }
}
