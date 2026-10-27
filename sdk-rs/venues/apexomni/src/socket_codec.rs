//! Native wire projection under engine-owned feed and epoch custody.
use crate::{ApexOmni, Pending, private_wire};
use safe_cex_engine::log::code;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};
use safe_cex_engine::{Cx, Level, SocketChannel, SocketSpec, Store, VenueError};

#[derive(serde::Deserialize)]
struct ControlHeader<'a> {
    #[serde(default, borrow)]
    op: Option<std::borrow::Cow<'a, str>>,
    #[serde(default, borrow)]
    topic: Option<std::borrow::Cow<'a, str>>,
    #[serde(default, borrow, rename = "type")]
    kind: Option<std::borrow::Cow<'a, str>>,
    #[serde(default)]
    success: Option<bool>,
    #[serde(default, borrow)]
    ret_msg: Option<std::borrow::Cow<'a, str>>,
    #[serde(default, borrow)]
    contents: Option<&'a serde_json::value::RawValue>,
}

fn policy(private: bool) -> FeedPolicy {
    FeedPolicy {
        role: if private {
            SocketChannel::Private
        } else {
            SocketChannel::Public
        },
        supervision: Config {
            connect_timeout_ms: if private { 10_000.0 } else { 30_000.0 },
            auth_timeout_ms: private.then_some(10_000.0),
            idle_timeout_ms: 30_000.0,
            ping_interval_ms: Some(if private { 5_000.0 } else { 10_000.0 }),
            backoff_cap_ms: if private { 30_000.0 } else { 16_000.0 },
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
        controls: ControlPolicy::Native,
        invalidation: if private {
            Invalidation::None
        } else {
            Invalidation::Required
        },
        subscription_interval_ms: 0.0,
    }
}

impl ApexOmni {
    pub(super) fn socket_notice(cx: &mut Cx<'_, Pending>, error: SocketError) {
        if error != SocketError::InvalidTime {
            cx.notice(
                Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("ApeX socket lifecycle: {error:?}"),
            );
        }
    }

    pub(super) fn connect_feed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        if !cx.now.is_finite() {
            return Err(SocketError::InvalidTime);
        }
        let current = if private {
            self.account.feed
        } else {
            self.public.feed
        };
        let feed = match current {
            Some(feed) => feed,
            None => {
                let feed = cx.register_socket_feed(policy(private))?;
                if private {
                    self.account.feed = Some(feed);
                } else {
                    self.public.feed = Some(feed);
                }
                feed
            }
        };
        let host = if self.options.testnet {
            "qa-quote.omni.apex.exchange"
        } else {
            "quote.omni.apex.exchange"
        };
        let path = if private { "private" } else { "public" };
        let request = if private {
            self.account.generation
        } else {
            cx.now as u64
        };
        let epoch = cx.connect_socket(
            feed,
            SocketSpec::new(format!(
                "wss://{host}/realtime_{path}?v=2&timestamp={}&requestId=rust-{request}",
                cx.now as u64
            )),
        )?;
        if private {
            self.account.socket = Some(epoch.socket());
            self.proof.borrow_mut().take();
        } else {
            self.public.socket = Some(epoch.socket());
        }
        Ok(())
    }

    pub(super) fn public_socket_error(&mut self, cx: &mut Cx<'_, Pending>, error: SocketError) {
        Self::socket_notice(cx, error);
        if error == SocketError::Capacity
            && let Some(feed) = self.public.feed
            && cx.socket_epoch(feed).is_some()
        {
            cx.retire_socket(feed, RetireReason::Closed);
        }
    }

    pub(super) fn classify_control(
        &self,
        store: &Store,
        feed: FeedId,
        authenticated: bool,
        text: &str,
    ) -> Control {
        if self.account.feed != Some(feed) {
            return Control::Other;
        }
        self.proof.borrow_mut().take();
        let Ok(message) = serde_json::from_str::<ControlHeader<'_>>(text) else {
            return Control::Other;
        };
        if message.op.as_deref() == Some("login") {
            return match message.success {
                Some(true) => Control::Auth(AuthEvidence::Accepted),
                Some(false) => Control::Auth(AuthEvidence::Rejected(VenueError::new(format!(
                    "ApeX private socket rejected request: {}",
                    message.ret_msg.as_deref().unwrap_or("")
                )))),
                None => Control::Other,
            };
        }
        // The slot lasts only until the synchronous Authenticated callback.
        if !authenticated
            && message.success != Some(false)
            && message.topic.as_deref() == Some("ws_zk_accounts_v3")
            && matches!(message.kind.as_deref(), Some("snapshot" | "delta"))
            && text.len() <= safe_cex_engine::socket_lifecycle::MAX_QUEUED_BYTES
            && message.contents.is_some_and(|raw| {
                serde_json::from_str::<private_wire::Contents<'_>>(raw.get()).is_ok_and(
                    |contents| self.private_auth_payload(store, &contents, self.account.opened_at),
                )
            })
            && let Some(socket) = self.account.socket
        {
            *self.proof.borrow_mut() = Some((socket, text.to_owned()));
            return Control::Auth(AuthEvidence::Accepted);
        }
        Control::Other
    }

    pub(super) fn socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        let feed = match &event {
            LifecycleEvent::Opened(e) | LifecycleEvent::Ping(e) | LifecycleEvent::RetryLogin(e) => {
                e.feed()
            }
            LifecycleEvent::Ready { epoch, .. }
            | LifecycleEvent::Retired { epoch, .. }
            | LifecycleEvent::Authenticated { epoch, .. }
            | LifecycleEvent::AuthFailure { epoch, .. } => epoch.feed(),
            LifecycleEvent::Reconnect(feed)
            | LifecycleEvent::InterestsCommitted { feed, .. }
            | LifecycleEvent::InterestsRefused { feed, .. } => *feed,
        };
        if self.account.feed == Some(feed) {
            match event {
                LifecycleEvent::Opened(epoch) => {
                    self.account_open(cx, epoch.socket());
                }
                LifecycleEvent::Authenticated { epoch, .. } => {
                    let proof = self.proof.borrow_mut().take();
                    if cx.socket_epoch(feed) != Some(epoch) {
                        return Ok(());
                    }
                    self.account.authenticated = true;
                    if let Some((socket, text)) =
                        proof.filter(|(socket, _)| *socket == epoch.socket())
                    {
                        self.account_message(cx, socket, &text);
                    }
                }
                LifecycleEvent::Retired { epoch, .. } => {
                    self.proof.borrow_mut().take();
                    self.account_close(cx, epoch.socket());
                }
                LifecycleEvent::Ping(epoch) => {
                    cx.send(epoch.socket(), r#"{"op":"ping"}"#);
                    self.account.ping_at = cx.now;
                }
                LifecycleEvent::Reconnect(_) => self.connect_private(cx)?,
                LifecycleEvent::AuthFailure { evidence, .. } => {
                    self.proof.borrow_mut().take();
                    if let AuthEvidence::Rejected(error) = evidence {
                        cx.notice(Level::Warn, code::AUTH_REJECTED, error.message);
                    }
                }
                LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
                _ => {}
            }
            return Ok(());
        }
        if self.public.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                if let Err(error) = self.public_open(cx, epoch.socket()) {
                    if error != SocketError::Capacity {
                        return Err(error);
                    }
                    self.public_socket_error(cx, error);
                }
            }
            LifecycleEvent::Retired { epoch, .. } => {
                self.public_close(cx, epoch.socket());
            }
            LifecycleEvent::Reconnect(_) => self.connect_feed(cx, false)?,
            LifecycleEvent::Ping(epoch) => {
                cx.send(epoch.socket(), r#"{"op":"ping"}"#);
                self.public.ping_at = cx.now;
            }
            LifecycleEvent::InterestsRefused {
                error: SocketError::Capacity,
                ..
            } => self.public_socket_error(cx, SocketError::Capacity),
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }

    pub(super) fn encode_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(feed) {
            return if items.is_empty() {
                Ok(())
            } else {
                Err(SocketError::NotPermitted)
            };
        }
        encode_public_batches(subscribe, items, emit)
    }
}

pub(super) fn validate_public_replay(items: &[Interest]) -> Result<(), SocketError> {
    use safe_cex_engine::socket_lifecycle::{MAX_FRAMES, MAX_QUEUED_BYTES};
    // Replay queues include coverage keys and the native post-interest ping.
    let mut bytes = r#"{"op":"ping"}"#.len();
    let mut frames = 0;
    encode_public_batches(true, items, &mut |batch| {
        bytes += batch.text.len()
            + batch
                .covered_keys
                .iter()
                .map(|key| key.len())
                .sum::<usize>();
        frames += 1;
        if bytes > MAX_QUEUED_BYTES || frames > MAX_FRAMES {
            return Err(SocketError::Capacity);
        }
        Ok(())
    })
}

fn encode_public_batches(
    subscribe: bool,
    items: &[Interest],
    emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
) -> Result<(), SocketError> {
    use safe_cex_engine::socket_lifecycle::MAX_FRAME_BYTES;
    let prefix = if subscribe {
        r#"{"op":"subscribe","args":["#
    } else {
        r#"{"op":"unsubscribe","args":["#
    };
    let mut text = prefix.to_owned();
    let mut covered_keys = Vec::new();
    for item in items {
        if item.key != item.wire_args || item.key.is_empty() || item.key.len() > 1024 {
            return Err(SocketError::InvalidInterest);
        }
        let arg = serde_json::to_string(item.wire_args.as_ref())
            .map_err(|_| SocketError::InvalidInterest)?;
        if prefix.len() + arg.len() + 2 > MAX_FRAME_BYTES {
            return Err(SocketError::Capacity);
        }
        if !covered_keys.is_empty()
            && (covered_keys.len() == 50 || text.len() + 1 + arg.len() + 2 > MAX_FRAME_BYTES)
        {
            text.push_str("]}");
            emit(EncodedBatch {
                text: std::mem::replace(&mut text, prefix.to_owned()),
                covered_keys: std::mem::take(&mut covered_keys),
            })?;
        }
        if !covered_keys.is_empty() {
            text.push(',');
        }
        text.push_str(&arg);
        covered_keys.push(item.key.clone());
    }
    if !covered_keys.is_empty() {
        text.push_str("]}");
        emit(EncodedBatch { text, covered_keys })?;
    }
    Ok(())
}
