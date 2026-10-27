//! Bitunix native wire over engine-managed sockets: the engine owns epochs,
//! supervision, interest replay and the private login retry.
use super::*;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};

/// Native public subscribe batch; the feed paces batches 250 ms apart.
const PUBLIC_BATCH: usize = 20;

impl BitunixFutures {
    fn feed_policy(feed: &Feed) -> FeedPolicy {
        let private = matches!(feed, Feed::Private);
        FeedPolicy {
            role: match feed {
                Feed::Futures => SocketChannel::Business,
                Feed::Private => SocketChannel::Private,
                Feed::Public { .. } => SocketChannel::Public,
            },
            supervision: Config {
                idle_timeout_ms: 45_000.0,
                ping_interval_ms: Some(15_000.0),
                ..Config::default()
            },
            // Observe: login evidence drives the same-socket retry without a
            // login deadline or gating private frames, as the legacy session did.
            auth: if private {
                AuthMode::Observe
            } else {
                AuthMode::None
            },
            recovery: if private {
                AuthRecovery::RetrySameSocket { delay_ms: 2000.0 }
            } else {
                AuthRecovery::Reconnect
            },
            permanent_refusal_group: None,
            // A private session may stay silent until its first real pong.
            traffic: if private {
                TrafficPolicy::TextAfterFirstPong
            } else {
                TrafficPolicy::AnyText
            },
            controls: if private {
                ControlPolicy::Native
            } else {
                ControlPolicy::None
            },
            invalidation: if matches!(feed, Feed::Futures) {
                Invalidation::Required
            } else {
                Invalidation::None
            },
            subscription_interval_ms: if matches!(feed, Feed::Public { .. }) {
                250.0
            } else {
                0.0
            },
        }
    }

    pub(crate) fn connect_feed(&mut self, cx: &mut Cx<'_, Pending>, feed: Feed) {
        let id = match cx.register_socket_feed(Self::feed_policy(&feed)) {
            Ok(id) => id,
            Err(error) => return self.socket_failed(cx, None, error),
        };
        self.sockets.push(Socket {
            feed: feed.clone(),
            id,
            recover: false,
        });
        if let Err(error) = cx.connect_socket(id, Self::socket_spec(&feed, false)) {
            self.socket_failed(cx, Some(id), error);
        }
    }

    fn socket_failed(&self, cx: &mut Cx<'_, Pending>, feed: Option<FeedId>, error: SocketError) {
        if let Some(feed) = feed {
            cx.stop_socket_feed(feed);
        }
        cx.notice(
            Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Bitunix socket admission failed: {error:?}"),
        );
    }

    fn socket(&self, feed: FeedId) -> Option<&Socket> {
        self.sockets.iter().find(|s| s.id == feed)
    }

    /// The registered feed whose current epoch owns `socket`.
    pub(crate) fn socket_feed(&self, cx: &Cx<'_, Pending>, socket: SocketId) -> Option<&Feed> {
        self.sockets
            .iter()
            .find(|s| cx.socket_epoch(s.id).is_some_and(|e| e.socket() == socket))
            .map(|s| &s.feed)
    }

    fn futures_interests(&self, cx: &Cx<'_, Pending>) -> Vec<Interest> {
        ["system_notice", "reconnected", "onconnected"]
            .into_iter()
            .map(|topic| (format!("0:{topic}"), topic.to_owned()))
            .chain(
                self.subscriptions(cx)
                    .into_iter()
                    .map(|topic| (format!("1:{topic}"), topic)),
            )
            .map(|(key, topic)| Interest {
                key: key.into(),
                wire_args: topic.into(),
            })
            .collect()
    }

    fn public_interests(cx: &Cx<'_, Pending>, range: &std::ops::Range<usize>) -> Vec<Interest> {
        cx.store.markets()[range.clone()]
            .iter()
            .enumerate()
            .map(|(i, m)| Interest {
                key: format!("{:06}", range.start + i).into(),
                wire_args: m.symbol.as_str().into(),
            })
            .collect()
    }

    /// Book and candle interest changes reach the futures feed as native sub/unsub frames.
    pub(crate) fn sync_futures(&self, cx: &mut Cx<'_, Pending>) {
        let Some(feed) = self
            .sockets
            .iter()
            .find(|s| matches!(s.feed, Feed::Futures))
            .map(|s| s.id)
        else {
            return;
        };
        if cx.socket_epoch(feed).is_none() {
            return;
        }
        if let Err(error) = cx.set_socket_interests(feed, self.futures_interests(cx)) {
            self.socket_failed(cx, Some(feed), error);
        }
    }

    fn ping(cx: &mut Cx<'_, Pending>, feed: &Feed, socket: SocketId) {
        if matches!(feed, Feed::Private) {
            cx.send(socket, r#"{"op":"ping"}"#);
        } else {
            cx.send(socket, format!(r#"{{"ping":{}}}"#, num::format(cx.now)));
        }
    }

    pub(crate) fn socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                let feed = self
                    .socket(epoch.feed())
                    .ok_or(SocketError::Stale)?
                    .feed
                    .clone();
                match &feed {
                    Feed::Private => {
                        Self::ping(cx, &feed, epoch.socket());
                        self.login(cx, epoch.socket());
                        if cx.store.loaded().positions {
                            for axis in [
                                account::Axis::Balance,
                                account::Axis::Positions,
                                account::Axis::Orders,
                            ] {
                                self.read_axis(cx, axis);
                            }
                        }
                        return Ok(());
                    }
                    Feed::Futures => {
                        for book in self.books.values_mut() {
                            book.levels.clear();
                            book.initialized = false;
                        }
                        cx.set_socket_interests(epoch.feed(), self.futures_interests(cx))?;
                    }
                    Feed::Public { range, .. } => {
                        cx.set_socket_interests(epoch.feed(), Self::public_interests(cx, range))?;
                    }
                }
                Self::ping(cx, &feed, epoch.socket());
            }
            LifecycleEvent::Ping(epoch) => {
                let feed = self
                    .socket(epoch.feed())
                    .ok_or(SocketError::Stale)?
                    .feed
                    .clone();
                Self::ping(cx, &feed, epoch.socket());
            }
            LifecycleEvent::RetryLogin(epoch) => {
                if matches!(
                    self.socket(epoch.feed()).map(|s| &s.feed),
                    Some(Feed::Private)
                ) {
                    self.login(cx, epoch.socket());
                }
            }
            LifecycleEvent::AuthFailure { evidence, .. } => match evidence {
                AuthEvidence::Rejected(_) => self.login_failed(cx, true),
                AuthEvidence::Malformed => self.login_failed(cx, false),
                AuthEvidence::Accepted | AuthEvidence::Progress => {}
            },
            LifecycleEvent::Retired { epoch, reason } => {
                let s = self
                    .sockets
                    .iter_mut()
                    .find(|s| s.id == epoch.feed())
                    .ok_or(SocketError::Stale)?;
                // The reconnect's routing context: a timeout recovers, a close does not.
                s.recover = matches!(reason, RetireReason::Timeout(_));
                if matches!(s.feed, Feed::Futures) {
                    for (symbol, book) in &mut self.books {
                        book.levels.clear();
                        book.initialized = false;
                        cx.book_changed(symbol);
                    }
                }
            }
            LifecycleEvent::Reconnect(feed) => {
                let s = self.socket(feed).ok_or(SocketError::Stale)?;
                cx.connect_socket(feed, Self::socket_spec(&s.feed, s.recover))?;
            }
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn encode_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let mut ordered: Vec<_> = interests.iter().collect();
        ordered.sort_by(|a, b| a.key.cmp(&b.key));
        match &self.socket(feed).ok_or(SocketError::Stale)?.feed {
            Feed::Futures => {
                for interest in ordered {
                    emit(EncodedBatch {
                        text: topic_frame(&interest.wire_args, subscribe),
                        covered_keys: vec![interest.key.clone()],
                    })?;
                }
            }
            Feed::Public { channel, .. } => {
                #[derive(serde::Serialize)]
                struct Arg<'a> {
                    symbol: &'a str,
                    ch: &'a str,
                }
                for batch in ordered.chunks(PUBLIC_BATCH) {
                    let args: Vec<_> = batch
                        .iter()
                        .map(|i| Arg {
                            symbol: &i.wire_args,
                            ch: channel,
                        })
                        .collect();
                    emit(EncodedBatch {
                        text: format!(
                            r#"{{"op":"{}","args":{}}}"#,
                            if subscribe {
                                "subscribe"
                            } else {
                                "unsubscribe"
                            },
                            serde_json::to_string(&args)
                                .map_err(|_| SocketError::InvalidInterest)?
                        ),
                        covered_keys: batch.iter().map(|i| i.key.clone()).collect(),
                    })?;
                }
            }
            Feed::Private => return Err(SocketError::InvalidInterest),
        }
        Ok(())
    }

    /// Private login results and pongs are control evidence; everything else is traffic.
    pub(crate) fn control(&self, feed: FeedId, text: &str) -> Control {
        if !matches!(self.socket(feed).map(|s| &s.feed), Some(Feed::Private)) {
            return Control::Other;
        }
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return Control::Other;
        };
        let operation = if m.op.is_empty() {
            m.ch.as_ref()
        } else {
            m.op.as_ref()
        };
        match operation {
            "pong" => Control::Pong,
            "login" => {
                #[derive(serde::Deserialize)]
                struct Login {
                    result: bool,
                }
                match m
                    .data
                    .and_then(|raw| serde_json::from_str::<Login>(raw.get()).ok())
                    .map(|login| login.result)
                {
                    Some(true) => Control::Auth(AuthEvidence::Accepted),
                    Some(false) => Control::Auth(AuthEvidence::Rejected(VenueError::new(
                        "Bitunix private ws login refused",
                    ))),
                    None => Control::Auth(AuthEvidence::Malformed),
                }
            }
            _ => Control::Other,
        }
    }
}
