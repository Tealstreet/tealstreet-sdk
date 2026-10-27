//! WOO wire codecs and invalidation; Engine owns socket lifecycle and replay.
use super::*;
use safe_cex_engine::VenueError;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};

impl Woo {
    fn private_feed(&self, feed: FeedId) -> Result<bool, SocketError> {
        if self.public.feed == Some(feed) {
            Ok(false)
        } else if self.account.connection.feed == Some(feed) {
            Ok(true)
        } else {
            Err(SocketError::Stale)
        }
    }

    pub(super) fn desired_interests(
        &self,
        store: &safe_cex_engine::Store,
        private: bool,
    ) -> Vec<Interest> {
        if private {
            return ["executionreport", "algoexecutionreportv2"]
                .into_iter()
                .map(Interest::from_wire_args)
                .collect();
        }
        let mut topics = vec![
            "tickers".to_owned(),
            "bbos".to_owned(),
            "markprices".to_owned(),
        ];
        for symbol in self.depth.keys() {
            if let Some(market) = store.market(symbol) {
                topics.push(format!("{}@orderbookupdate", native(market)));
            }
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(market) = store.market(symbol) {
                topics.push(format!("{}@kline_{interval}", native(market)));
            }
        }
        if !self.options.public_data_only {
            let mut seen = std::collections::HashSet::new();
            for (symbol, _) in self.candles.keys() {
                if seen.insert(symbol)
                    && let Some(market) = store.market(symbol)
                {
                    topics.push(format!("{}@trades", native(market)));
                }
            }
        }
        topics.into_iter().map(Interest::from_wire_args).collect()
    }

    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        if !private && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        let connection = if private {
            &self.account.connection
        } else {
            &self.public
        };
        let feed = if let Some(feed) = connection.feed {
            feed
        } else {
            let feed = cx.register_socket_feed(FeedPolicy {
                role: if private {
                    SocketChannel::Private
                } else {
                    SocketChannel::Public
                },
                supervision: Config {
                    auth_timeout_ms: private.then_some(10_000.),
                    idle_timeout_ms: 20_000.,
                    ping_interval_ms: Some(6_000.),
                    backoff_cap_ms: if private { 30_000. } else { 16_000. },
                    ..Config::default()
                },
                auth: if private {
                    AuthMode::Required
                } else {
                    AuthMode::None
                },
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: if private {
                    ControlPolicy::Native
                } else {
                    ControlPolicy::None
                },
                invalidation: if private {
                    Invalidation::None
                } else {
                    Invalidation::Required
                },
                subscription_interval_ms: 0.,
                close_floors: &[],
            })?;
            if private {
                self.account.connection.feed = Some(feed);
            } else {
                self.public.feed = Some(feed);
            }
            feed
        };
        cx.set_socket_interests(feed, self.desired_interests(cx.store, private))?;
        let host = match (self.options.testnet, private) {
            (false, false) => "wss://wss.woox.io/ws/stream/",
            (true, false) => "wss://wss.staging.woox.io/ws/stream/",
            (false, true) => "wss://wss.woox.io/v2/ws/private/stream/",
            (true, true) => "wss://wss.staging.woox.io/v2/ws/private/stream/",
        };
        let epoch = cx.connect_socket(
            feed,
            format!(
                "{host}{}",
                self.options
                    .application_id
                    .as_deref()
                    .unwrap_or("undefined")
            ),
        )?;
        let connection = if private {
            &mut self.account.connection
        } else {
            &mut self.public
        };
        connection.socket = Some(epoch.socket());
        connection.open = false;
        Ok(())
    }

    pub(super) fn socket_error(&self, cx: &mut Cx<'_, Pending>, private: bool, error: SocketError) {
        let feed = if private {
            self.account.connection.feed
        } else {
            self.public.feed
        };
        if let Some(feed) = feed {
            cx.stop_socket_feed(feed);
        }
        cx.notice(
            Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("WOO socket admission failed: {error:?}"),
        );
    }

    pub(super) fn sync_public_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(feed) = self.public.feed else {
            return;
        };
        let desired = self.desired_interests(cx.store, false);
        let seeds: Vec<_> = self
            .depth
            .iter()
            .filter(|(symbol, state)| state.seed_due && cx.store.market(symbol).is_some())
            .map(|(symbol, state)| (symbol.clone(), state.generation))
            .collect();
        let epoch = cx.socket_epoch(feed);
        let result = if self.public.open && !seeds.is_empty() {
            let required: Vec<_> = seeds
                .iter()
                .map(|(symbol, _)| {
                    format!(
                        "{}@orderbookupdate",
                        native(cx.store.market(symbol).unwrap())
                    )
                })
                .collect();
            let keys: Vec<_> = required.iter().map(String::as_str).collect();
            cx.set_socket_interests_then(feed, desired, &keys, Pending::SocketBookSeeds(seeds))
        } else {
            cx.set_socket_interests(feed, desired)
        };
        if let Err(error) = result {
            if error == SocketError::Capacity && epoch.is_some() && cx.socket_epoch(feed) == epoch {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    format!("WOO socket admission failed: {error:?}"),
                );
                cx.retire_socket(feed, RetireReason::Closed);
            } else {
                self.socket_error(cx, false, error);
            }
        }
    }

    pub(super) fn managed_control(&self, feed: FeedId, text: &str) -> Control {
        if self.account.connection.feed != Some(feed) {
            return Control::Other;
        }
        #[derive(serde::Deserialize)]
        struct Event<'a> {
            event: Option<&'a str>,
        }
        let Ok(event) = serde_json::from_str::<Event<'_>>(text) else {
            return Control::Other;
        };
        match event.event {
            Some("pong") => Control::Pong,
            Some("auth") => {
                #[derive(serde::Deserialize)]
                struct Auth {
                    success: Option<bool>,
                    message: Option<String>,
                }
                let evidence = match serde_json::from_str::<Auth>(text) {
                    Ok(Auth {
                        success: Some(true),
                        ..
                    }) => AuthEvidence::Accepted,
                    Ok(Auth {
                        success: Some(false),
                        message,
                    }) => AuthEvidence::Rejected(VenueError::new(
                        message
                            .filter(|value| !value.is_empty())
                            .unwrap_or_else(|| {
                                "WOO X private websocket authentication failed".into()
                            }),
                    )),
                    _ => AuthEvidence::Malformed,
                };
                Control::Auth(evidence)
            }
            _ => Control::Other,
        }
    }

    pub(super) fn managed_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                if self.private_feed(epoch.feed())? {
                    self.account_open(cx, epoch.socket());
                } else {
                    self.public.open = true;
                    self.public.ping_at = cx.now;
                    cx.send(epoch.socket(), r#"{"event":"ping"}"#);
                }
            }
            LifecycleEvent::Ready { epoch, .. } if !self.private_feed(epoch.feed())? => {
                self.sync_public_interests(cx);
            }
            LifecycleEvent::Authenticated { epoch, first: true } => {
                cx.send_after_socket_interests(epoch.feed(), r#"{"event":"ping"}"#.into())?;
            }
            LifecycleEvent::AuthFailure { evidence, .. } => {
                let message = match evidence {
                    AuthEvidence::Rejected(error) => error.message,
                    _ => "WOO X private websocket authentication failed".into(),
                };
                cx.notice(Level::Error, code::AUTH_REJECTED, message);
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.private_feed(epoch.feed())? {
                    self.account_close(cx, epoch.socket());
                } else {
                    self.public.socket = None;
                    self.public.open = false;
                    for (symbol, state) in &mut self.depth {
                        cx.book_changed(symbol);
                        self.generation += 1;
                        state.generation = self.generation;
                        state.seed_due = true;
                        state.book.clear();
                        state.sync = Sync::Buffer(Vec::new());
                    }
                }
            }
            LifecycleEvent::Ping(epoch) => {
                if self.private_feed(epoch.feed())? {
                    self.account.connection.ping_at = cx.now;
                } else {
                    self.public.ping_at = cx.now;
                }
                cx.send(epoch.socket(), r#"{"event":"ping"}"#);
            }
            LifecycleEvent::Reconnect(feed) => {
                self.managed_connect(cx, self.private_feed(feed)?)?
            }
            LifecycleEvent::InterestsRefused {
                feed,
                error: SocketError::Capacity,
                ..
            } if self.public.feed == Some(feed) => {
                cx.notice(
                    Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    "WOO socket admission failed: Capacity",
                );
                if cx.socket_epoch(feed).is_some() {
                    cx.retire_socket(feed, RetireReason::Closed);
                }
            }
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => (),
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
        let private = self.private_feed(feed)?;
        let desired = self.desired_interests(store, private);
        let ranks: HashMap<_, _> = desired
            .iter()
            .enumerate()
            .map(|(index, item)| (item.key.as_ref(), index))
            .collect();
        let mut ordered: Vec<_> = items.iter().collect();
        ordered.sort_by_key(|item| ranks.get(item.key.as_ref()).copied().unwrap_or(usize::MAX));
        for item in ordered {
            let topic = serde_json::to_string(item.wire_args.as_ref())
                .map_err(|_| SocketError::InvalidInterest)?;
            let event = if subscribe {
                "subscribe"
            } else {
                "unsubscribe"
            };
            emit(EncodedBatch {
                text: if private {
                    format!(r#"{{"topic":{topic},"event":"{event}"}}"#)
                } else {
                    format!(r#"{{"event":"{event}","topic":{topic}}}"#)
                },
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
}
