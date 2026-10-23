//! Native feed data and codecs; lifecycle authority belongs to Engine.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};
#[derive(serde::Serialize)]
struct PublicArg<'a> {
    symbol: &'a str,
    ch: &'a str,
}
impl BitunixFutures {
    pub(super) fn feed_index(&self, feed: FeedId) -> Option<usize> {
        self.sockets.iter().position(|s| s.handle == feed)
    }
    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        feed: Feed,
    ) -> Result<(), SocketError> {
        if matches!(feed, Feed::Public { .. })
            && cx.public_feed_policy() != safe_cex_engine::PublicFeedPolicy::Direct
        {
            return Ok(());
        }
        let private = matches!(feed, Feed::Private);
        let role = match &feed {
            Feed::Private => SocketChannel::Private,
            Feed::Futures => SocketChannel::Business,
            Feed::Public { .. } => SocketChannel::Public,
        };
        let policy = FeedPolicy {
            role,
            supervision: Config {
                connect_timeout_ms: 10_000.,
                idle_timeout_ms: 45_000.,
                ping_interval_ms: Some(15_000.),
                ..Config::default()
            },
            auth: if private {
                AuthMode::Observe
            } else {
                AuthMode::None
            },
            recovery: if private {
                AuthRecovery::RetrySameSocket { delay_ms: 2_000. }
            } else {
                AuthRecovery::Reconnect
            },
            permanent_refusal_group: None,
            traffic: if private {
                TrafficPolicy::TextAfterFirstPong
            } else {
                TrafficPolicy::AnyText
            },
            invalidation: if matches!(feed, Feed::Futures) {
                Invalidation::Required
            } else {
                Invalidation::None
            },
            subscription_interval_ms: if matches!(feed, Feed::Public { .. }) {
                250.
            } else {
                0.
            },
        };
        let handle = cx.register_socket_feed(policy)?;
        let spec = Self::socket_spec(&feed, false);
        self.sockets.push(Socket {
            handle,
            feed,
            recover: false,
        });
        cx.connect_socket(handle, spec)?;
        Ok(())
    }
    pub(super) fn managed_interests(&self, cx: &Cx<'_, Pending>, index: usize) -> Vec<Interest> {
        match &self.sockets[index].feed {
            Feed::Private => Vec::new(),
            Feed::Futures => ["system_notice", "reconnected", "onconnected"]
                .into_iter()
                .enumerate()
                .map(|(i, t)| Interest {
                    key: format!("0:{i}").into(),
                    wire_args: t.into(),
                })
                .chain(self.subscriptions(cx).into_iter().map(|topic| Interest {
                    key: format!("1:{topic}").into(),
                    wire_args: topic.into(),
                }))
                .collect(),
            Feed::Public { channel, range } => cx.store.markets()[range.clone()]
                .iter()
                .enumerate()
                .map(|(i, m)| Interest {
                    key: format!("{i:08}").into(),
                    wire_args: serde_json::to_string(&PublicArg {
                        symbol: &m.symbol,
                        ch: channel,
                    })
                    .expect("native public argument")
                    .into(),
                })
                .collect(),
        }
    }
    pub(super) fn refresh_futures_interests(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(i) = self
            .sockets
            .iter()
            .position(|s| matches!(s.feed, Feed::Futures))
            && let Err(error) =
                cx.set_socket_interests(self.sockets[i].handle, self.managed_interests(cx, i))
        {
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("Managed socket interest admission refused: {error:?}"),
            );
            cx.stop_socket_feed(self.sockets[i].handle);
        }
    }
    pub(super) fn managed_control(&self, feed: FeedId, text: &str) -> Control {
        let Some(i) = self.feed_index(feed) else {
            return Control::Other;
        };
        if !matches!(self.sockets[i].feed, Feed::Private) {
            return Control::Other;
        }
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return Control::Other;
        };
        if m.op == "pong" {
            return Control::Pong;
        }
        let operation = if m.op.is_empty() {
            m.ch.as_ref()
        } else {
            m.op.as_ref()
        };
        if operation != "login" {
            return Control::Other;
        }
        #[derive(serde::Deserialize)]
        struct Login {
            result: bool,
        }
        match m
            .data
            .and_then(|v| serde_json::from_str::<Login>(v.get()).ok())
            .map(|v| v.result)
        {
            Some(true) => Control::Auth(AuthEvidence::Accepted),
            Some(false) => Control::Auth(AuthEvidence::Rejected(account::socket_login_error())),
            None => Control::Auth(AuthEvidence::Malformed),
        }
    }
    pub(super) fn managed_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                let i = self.feed_index(epoch.feed()).ok_or(SocketError::Stale)?;
                if matches!(self.sockets[i].feed, Feed::Private) {
                    cx.send(epoch.socket(), r#"{"op":"ping"}"#);
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
                }
            }
            LifecycleEvent::Ready { epoch, .. } => {
                let i = self.feed_index(epoch.feed()).ok_or(SocketError::Stale)?;
                if !matches!(self.sockets[i].feed, Feed::Private) {
                    let desired = self.managed_interests(cx, i);
                    cx.set_socket_interests(epoch.feed(), desired)?;
                    cx.send_after_socket_interests(
                        epoch.feed(),
                        format!(r#"{{"ping":{}}}"#, num::format(cx.now)),
                    )?;
                }
            }
            LifecycleEvent::Ping(epoch) => {
                let i = self.feed_index(epoch.feed()).ok_or(SocketError::Stale)?;
                if matches!(self.sockets[i].feed, Feed::Private) {
                    cx.send(epoch.socket(), r#"{"op":"ping"}"#);
                } else {
                    cx.send(
                        epoch.socket(),
                        format!(r#"{{"ping":{}}}"#, num::format(cx.now)),
                    );
                }
            }
            LifecycleEvent::RetryLogin(epoch) => self.login(cx, epoch.socket()),
            LifecycleEvent::AuthFailure { evidence, .. } => match evidence {
                AuthEvidence::Rejected(e) => {
                    cx.notice(Level::Error, code::AUTH_REJECTED, e.message)
                }
                AuthEvidence::Malformed => cx.notice(
                    Level::Warn,
                    code::STREAM_MALFORMED,
                    "Malformed Bitunix private login response",
                ),
                AuthEvidence::Accepted => {}
            },
            LifecycleEvent::Retired { epoch, reason } => {
                let i = self.feed_index(epoch.feed()).ok_or(SocketError::Stale)?;
                self.sockets[i].recover = matches!(reason, RetireReason::Timeout(_));
                if matches!(self.sockets[i].feed, Feed::Futures) {
                    for (symbol, book) in &mut self.books {
                        book.levels.clear();
                        book.initialized = false;
                        cx.book_changed(symbol);
                    }
                }
            }
            LifecycleEvent::Reconnect(feed) => {
                let i = self.feed_index(feed).ok_or(SocketError::Stale)?;
                cx.connect_socket(
                    feed,
                    Self::socket_spec(&self.sockets[i].feed, self.sockets[i].recover),
                )?;
            }
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }
    pub(super) fn managed_encode(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let i = self.feed_index(feed).ok_or(SocketError::Stale)?;
        match &self.sockets[i].feed {
            Feed::Private => {
                if !items.is_empty() {
                    return Err(SocketError::InvalidInterest);
                }
            }
            Feed::Futures => {
                for item in items {
                    emit(EncodedBatch {
                        text: topic_frame(&item.wire_args, subscribe),
                        covered_keys: vec![item.key.clone()],
                    })?;
                }
            }
            Feed::Public { .. } => {
                for batch in items.chunks(20) {
                    let args = batch
                        .iter()
                        .map(|i| i.wire_args.as_ref())
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
                }
            }
        }
        Ok(())
    }
}
