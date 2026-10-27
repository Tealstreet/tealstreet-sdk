//! Shared engine lifecycle admission; Kraken retains native protocol evidence.
use super::*;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, NativeTiming, SocketError, StableAnchor, TrafficPolicy,
};

pub(super) fn policy(private: bool, futures: bool) -> FeedPolicy {
    FeedPolicy {
        role: if private {
            SocketChannel::Private
        } else {
            SocketChannel::Public
        },
        supervision: Config {
            connect_timeout_ms: if private && futures { 15_000. } else { 45_000. },
            auth_timeout_ms: (private && futures).then_some(15_000.),
            idle_timeout_ms: if private && futures { 60_000. } else { 45_000. },
            ping_interval_ms: Some(if private && futures { 10_000. } else { 15_000. }),
            backoff_cap_ms: if private && futures { 30_000. } else { 16_000. },
            ..Config::default()
        },
        auth: if private && futures {
            AuthMode::Required
        } else {
            AuthMode::None
        },
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::Declared,
        controls: ControlPolicy::Native,
        invalidation: if private {
            Invalidation::None
        } else {
            Invalidation::Required
        },
        subscription_interval_ms: 0.,
    }
}

impl Kraken {
    fn private_feed(&self, feed: FeedId) -> Result<bool, SocketError> {
        if self.public.feed == Some(feed) {
            Ok(false)
        } else if self.account.feed == Some(feed) {
            Ok(true)
        } else {
            Err(SocketError::Stale)
        }
    }
    pub(super) fn managed_connect(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        if !private && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        if self.public.feed.is_none() && cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.feed = Some(cx.register_socket_feed(policy(false, true))?);
        }
        if self.account.feed.is_none() && self.signer.is_some() && !self.options.public_data_only {
            self.account.feed = Some(cx.register_socket_feed_with_native_timing(
                policy(true, true),
                NativeTiming {
                    initial_payload_timeout_ms: Some(180_000.),
                    stable_anchor: StableAnchor::Open,
                },
            )?);
        }
        let feed = if private {
            self.account.feed
        } else {
            self.public.feed
        }
        .ok_or(SocketError::NotPermitted)?;
        let epoch = cx.connect_socket(feed, self.ws_url())?;
        if private {
            self.install_account_socket(epoch.socket());
        } else {
            self.public.socket = Some(epoch.socket());
            self.public.open = false;
        }
        if !private
            && let Err(error) = cx.set_socket_interests(feed, self.public_interests(cx.store))
        {
            self.socket_error(cx, false, error);
        }
        Ok(())
    }
    pub(super) fn socket_error(&self, cx: &mut Cx<'_, Pending>, private: bool, error: SocketError) {
        if let Some(feed) = if private {
            self.account.feed
        } else {
            self.public.feed
        } {
            if !private && error == SocketError::Capacity {
                if cx.socket_epoch(feed).is_some() {
                    cx.retire_socket(
                        feed,
                        safe_cex_engine::socket_lifecycle::RetireReason::Closed,
                    );
                }
            } else {
                cx.stop_socket_feed(feed);
            }
        }
        cx.notice(
            Level::Error,
            code::STREAM_UNAVAILABLE,
            format!("Kraken socket admission failed: {error:?}"),
        );
    }
    fn public_interests(&self, store: &safe_cex_engine::Store) -> Vec<Interest> {
        let selected: Vec<_> = store
            .markets()
            .iter()
            .filter(|m| self.ticker_symbols.contains(&m.symbol))
            .map(|m| m.symbol.as_str())
            .collect();
        let products: Vec<_> = if selected.is_empty() {
            store.markets().iter().map(|m| m.symbol.as_str()).collect()
        } else {
            selected
        };
        let mut interests = vec![];
        for batch in products.chunks(50) {
            interests.push(futures_interest("ticker", batch));
        }
        let mut books: Vec<_> = self.books.keys().collect();
        books.sort();
        for symbol in books {
            interests.push(futures_interest("book", &[symbol.as_str()]));
        }
        interests
    }
    pub(super) fn sync_public_interests(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(feed) = self.public.feed
            && let Err(error) = cx.set_socket_interests(feed, self.public_interests(cx.store))
        {
            self.socket_error(cx, false, error);
        }
    }
    pub(super) fn repair_book(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(feed) = self.public.feed
            && let Err(error) = cx.repair_socket_interests(
                feed,
                vec![futures_interest("book", &[symbol]).key.to_string()],
            )
        {
            self.socket_error(cx, false, error);
        }
    }
    pub(super) fn managed_control(&self, feed: FeedId, authenticated: bool, text: &str) -> Control {
        if self.account.feed == Some(feed) {
            return self.account_control(authenticated, text);
        }
        if self.public.feed != Some(feed) {
            return Control::Other;
        }
        match serde_json::from_str::<Message<'_>>(text) {
            Ok(msg) if msg.event == "pong" => Control::Pong,
            Ok(_) => Control::Traffic,
            Err(_) => Control::Other,
        }
    }
    pub(super) fn managed_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                let private = self.private_feed(epoch.feed())?;
                if private {
                    cx.set_socket_interests(epoch.feed(), vec![])?;
                }
                self.on_open(cx, epoch.socket());
            }
            LifecycleEvent::Retired { epoch, .. } => {
                self.private_feed(epoch.feed())?;
                self.on_close(cx, epoch.socket(), 1006);
            }
            LifecycleEvent::Reconnect(feed) => {
                self.managed_connect(cx, self.private_feed(feed)?)?
            }
            LifecycleEvent::Ping(epoch) => {
                self.private_feed(epoch.feed())?;
                cx.send(epoch.socket(), r#"{"event":"ping"}"#);
            }
            LifecycleEvent::AuthFailure { evidence, .. } => {
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    format!("Kraken private socket refused: {evidence:?}"),
                );
            }
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                let private = self.private_feed(feed)?;
                self.socket_error(cx, private, error);
            }
            _ => (),
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
        let private = self.private_feed(feed)?;
        if private {
            return if items.is_empty() {
                Ok(())
            } else {
                Err(SocketError::InvalidInterest)
            };
        }
        let mut ordered: Vec<_> = items.iter().collect();
        ordered.sort_by_key(|item| {
            (
                !item.wire_args.contains("\"feed\":\"ticker\""),
                item.key.as_ref(),
            )
        });
        for item in ordered {
            let text = {
                let args: FuturesTopic = serde_json::from_str(&item.wire_args)
                    .map_err(|_| SocketError::InvalidInterest)?;
                serde_json::to_string(&FuturesFrame {
                    event: if subscribe {
                        "subscribe"
                    } else {
                        "unsubscribe"
                    },
                    feed: &args.feed,
                    product_ids: &args.product_ids,
                })
                .map_err(|_| SocketError::InvalidInterest)?
            };
            emit(EncodedBatch {
                text,
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct FuturesTopic {
    feed: String,
    product_ids: Vec<String>,
}
#[derive(serde::Serialize)]
struct FuturesFrame<'a> {
    event: &'a str,
    feed: &'a str,
    product_ids: &'a [String],
}
fn futures_interest(feed: &str, products: &[&str]) -> Interest {
    Interest::from_wire_args(
        serde_json::to_string(&FuturesTopic {
            feed: feed.into(),
            product_ids: products.iter().map(|s| (*s).into()).collect(),
        })
        .unwrap(),
    )
}
