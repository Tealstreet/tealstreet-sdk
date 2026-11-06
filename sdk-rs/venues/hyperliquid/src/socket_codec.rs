//! Native topic projection and wire encoding; the shared core owns transport.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, Epoch, FeedPolicy, Interest, Invalidation,
    LifecycleEvent, RetireReason, TrafficPolicy,
};

const PRIVATE_SNAPSHOT_TIMEOUT_MS: f64 = 30_000.;
const MAX_PRIVATE_SNAPSHOT_TIMEOUT_MS: f64 = 300_000.;

#[derive(Default)]
pub(super) struct PrivateBootstrap {
    epoch: Option<Epoch>,
    timer: Option<TimerId>,
    deadline: Option<f64>,
    pending: bool,
}

impl PrivateBootstrap {
    fn pending(&self) -> bool {
        self.pending
    }
}

#[derive(Serialize)]
struct PublicTopic<'a> {
    r#type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dex: Option<&'a str>,
}

#[derive(Serialize)]
struct Send<T> {
    method: &'static str,
    subscription: T,
}

/// The native subscribe frame, and so the interest key, for one public topic.
fn public_topic(
    kind: &str,
    coin: Option<&str>,
    interval: Option<&str>,
    dex: Option<&str>,
) -> String {
    serde_json::to_string(&Send {
        method: "subscribe",
        subscription: PublicTopic {
            r#type: kind,
            coin,
            interval,
            dex,
        },
    })
    .expect("native public topic")
}

fn candle_topic(coin: &str, interval: Interval) -> String {
    public_topic("candle", Some(coin), Some(interval.as_str()), None)
}

/// Spot depth names its significant figures, `null` for full precision.
fn spot_depth_topic(coin: &str, n_sig_figs: Option<u8>) -> String {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Depth<'a> {
        r#type: &'static str,
        coin: &'a str,
        n_sig_figs: Option<u8>,
    }
    serde_json::to_string(&Send {
        method: "subscribe",
        subscription: Depth {
            r#type: "l2Book",
            coin,
            n_sig_figs,
        },
    })
    .expect("native spot depth topic")
}

#[derive(Serialize)]
struct FastDepth<'a> {
    r#type: &'static str,
    coin: &'a str,
    fast: bool,
}

#[derive(serde::Deserialize)]
struct EncodedTopic<'a> {
    #[serde(borrow)]
    subscription: DepthOrder<'a>,
}

#[derive(serde::Deserialize)]
struct DepthOrder<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(rename = "nSigFigs", default)]
    figures: Option<u8>,
    #[serde(default)]
    fast: bool,
}

impl Hyperliquid {
    /// Track early native snapshots too: delivery can beat the release continuation.
    fn prepare_private_snapshots(&mut self, cx: &mut Cx<'_, Pending>, epoch: Epoch) {
        if self.private_socket_interests(cx.store).is_empty()
            || self.connection.private_bootstrap.epoch == Some(epoch)
            || cx.socket_epoch(epoch.feed()) != Some(epoch)
        {
            return;
        }
        self.clear_private_snapshot_watch(cx);
        self.connection.private_bootstrap = PrivateBootstrap {
            epoch: Some(epoch),
            timer: None,
            deadline: None,
            pending: true,
        };
    }

    /// The initial-state deadline starts when the private subscriptions are released.
    pub(super) fn watch_private_snapshots(&mut self, cx: &mut Cx<'_, Pending>, epoch: Epoch) {
        let timeout = (PRIVATE_SNAPSHOT_TIMEOUT_MS
            * f64::from(1u32 << self.connection.private_bootstrap_misses.min(4)))
        .min(MAX_PRIVATE_SNAPSHOT_TIMEOUT_MS);
        let watch = &mut self.connection.private_bootstrap;
        if watch.epoch != Some(epoch)
            || cx.socket_epoch(epoch.feed()) != Some(epoch)
            || !watch.pending()
            || watch.deadline.is_some()
        {
            return;
        }
        watch.deadline = Some(cx.now + timeout);
        watch.timer = Some(cx.set_timer(timeout));
    }

    fn clear_private_snapshot_watch(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.connection.private_bootstrap.timer.take() {
            cx.clear_timer(timer);
        }
        self.connection.private_bootstrap = PrivateBootstrap::default();
    }

    fn expire_private_snapshots(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        let watch = &self.connection.private_bootstrap;
        if !watch.pending() || !cx.now.is_finite() || watch.deadline.is_none_or(|at| cx.now < at) {
            return false;
        }
        let epoch = watch.epoch;
        self.clear_private_snapshot_watch(cx);
        if let Some(epoch) = epoch
            && cx.socket_epoch(epoch.feed()) == Some(epoch)
        {
            if self.connection.private_bootstrap_misses == 0 {
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    code::STREAM_UNAVAILABLE,
                    "Hyperliquid initial private snapshots did not arrive; reconnecting the account stream",
                );
            }
            self.connection.private_bootstrap_misses = self
                .connection
                .private_bootstrap_misses
                .saturating_add(1)
                .min(4);
            cx.retire_socket(
                epoch.feed(),
                RetireReason::Timeout(safe_cex_engine::socket::Timeout::Silent),
            );
            return true;
        }
        false
    }

    pub(super) fn private_snapshot_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.connection.private_bootstrap.timer == Some(timer) {
            self.connection.private_bootstrap.timer = None;
        }
        if !self.expire_private_snapshots(cx) {
            self.rearm_private_snapshot_watch(cx);
        }
    }

    fn rearm_private_snapshot_watch(&mut self, cx: &mut Cx<'_, Pending>) {
        let watch = &mut self.connection.private_bootstrap;
        if cx.now.is_finite()
            && watch.pending()
            && watch.timer.is_none()
            && let Some(deadline) = watch.deadline
            && deadline > cx.now
        {
            watch.timer = Some(cx.set_timer(deadline - cx.now));
        }
    }

    /// Pongs, public prices and REST reads cannot acknowledge a private subscription.
    pub(super) fn observe_private_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
    ) -> bool {
        if !self.connection.private_bootstrap.pending() {
            return true;
        }
        if !cx.now.is_finite() || self.expire_private_snapshots(cx) {
            return false;
        }
        self.rearm_private_snapshot_watch(cx);
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return true;
        };
        let watch = &mut self.connection.private_bootstrap;
        match frame.channel.as_ref() {
            "clearinghouseState" if !self.spot => {
                if let Ok(env) = serde_json::from_str::<ClearingEnvelope>(frame.data.get())
                    && env.dex.is_empty()
                    && (env.clearinghouse_state.is_some()
                        || serde_json::from_str::<Clearinghouse>(frame.data.get()).is_ok())
                {
                    watch.pending = false;
                }
            }
            "spotState" | "spotClearinghouseState"
                if self.spot
                    && (serde_json::from_str::<SpotEnvelope>(frame.data.get()).is_ok()
                        || serde_json::from_str::<SpotState>(frame.data.get()).is_ok()) =>
            {
                watch.pending = false;
            }
            _ => {}
        }
        if !watch.pending() {
            self.connection.private_bootstrap_misses = 0;
            if let Some(timer) = watch.timer.take() {
                cx.clear_timer(timer);
            }
        }
        true
    }

    /// Every native stream one book on `coin` subscribes: perps' `l2Book` and
    /// fast depth and `bbo`, or spot's full-precision depth and each rounded tier.
    fn book_topics(&self, coin: &str, push: &mut dyn FnMut(String)) {
        if self.spot {
            for n_sig_figs in [None, Some(2), Some(3), Some(4), Some(5)] {
                push(spot_depth_topic(coin, n_sig_figs));
            }
        } else {
            push(public_topic("l2Book", Some(coin), None, None));
            if self.options.fast_book {
                push(
                    serde_json::to_string(&Send {
                        method: "subscribe",
                        subscription: FastDepth {
                            r#type: "l2Book",
                            coin,
                            fast: true,
                        },
                    })
                    .expect("native fast depth topic"),
                );
            }
            push(public_topic("bbo", Some(coin), None, None));
        }
    }

    /// The feed and every key `topic` streams on, as `desired_socket_interests` subscribes them.
    pub(super) fn topic_interests(
        &self,
        store: &safe_cex_engine::Store,
        topic: &SubscriptionTopic,
    ) -> Vec<(FeedId, String)> {
        let mut interests = Vec::new();
        let Some(feed) = self.connection.feed else {
            return interests;
        };
        match topic {
            SubscriptionTopic::Candles { symbol, interval } => {
                if let Some(market) = store.market(symbol) {
                    interests.push((feed, candle_topic(&market.id, *interval)));
                }
            }
            SubscriptionTopic::OrderBook { symbol } => {
                if let Some(market) = store.market(symbol) {
                    self.book_topics(&market.id, &mut |key| interests.push((feed, key)));
                }
            }
        }
        interests
    }

    pub(super) fn socket_refused(cx: &mut Cx<'_, Pending>, error: SocketError) {
        cx.notice(
            safe_cex_engine::Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Managed Hyperliquid socket admission refused: {error:?}"),
        );
    }

    pub(super) fn desired_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
        policy: PublicFeedPolicy,
    ) -> Vec<Interest> {
        let mut interests = self.private_socket_interests(store);
        if policy != PublicFeedPolicy::Direct {
            return interests;
        }
        let add = |values: &mut Vec<Interest>, kind, coin, interval, dex| {
            values.push(Interest::from_wire_args(public_topic(
                kind, coin, interval, dex,
            )));
        };
        add(&mut interests, "allMids", None, None, None);
        if !self.spot {
            add(&mut interests, "fastAssetCtxs", None, None, None);
        }
        if !self.options.public_data_only && self.user().is_some() {
            for dex in &self.dex_names {
                add(&mut interests, "allMids", None, None, Some(dex.as_str()));
            }
        }
        for symbol in self.books.keys() {
            let Some(market) = store.market(symbol) else {
                continue;
            };
            self.book_topics(&market.id, &mut |key| {
                interests.push(Interest::from_wire_args(key));
            });
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(market) = store.market(symbol) {
                interests.push(Interest::from_wire_args(candle_topic(
                    &market.id, *interval,
                )));
            }
        }
        interests
    }

    /// Never evict admitted demand when catalogue/private projection grows.
    pub(super) fn bounded_socket_interests(&mut self, cx: &mut Cx<'_, Pending>) -> Vec<Interest> {
        let mut desired = self.desired_socket_interests(cx.store, cx.public_feed_policy());
        let excess = desired.len().saturating_sub(1000);
        if excess > 0 {
            // On first admission, baseline/user topics precede implicit per-market topics.
            desired.sort_by_key(|i| {
                (
                    !self.connection.admitted_interests.contains(&i.key),
                    i.wire_args.contains("\"type\":\"activeAssetData\""),
                )
            });
            desired.truncate(1000);
            if self.connection.capacity_excess != excess {
                cx.notice(
                    safe_cex_engine::Level::Error,
                    code::SUBSCRIPTION_REJECTED,
                    format!("Managed Hyperliquid socket Capacity: {excess} excess topics refused; accepted subscriptions unchanged"),
                );
            }
        }
        self.connection.capacity_excess = excess;
        desired
    }

    pub(super) fn managed_connect(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = if let Some(feed) = self.connection.feed {
            feed
        } else {
            let feed = cx.register_socket_feed(FeedPolicy {
                role: if self.options.public_data_only {
                    safe_cex_engine::SocketChannel::Public
                } else {
                    safe_cex_engine::SocketChannel::Private
                },
                supervision: Config {
                    idle_timeout_ms: 45_000.,
                    ping_interval_ms: Some(15_000.),
                    backoff_base_ms: 1_000.,
                    backoff_cap_ms: 16_000.,
                    ..Config::default()
                },
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::Required,
                subscription_interval_ms: 0.,
                close_floors: &[],
                data_silence: None,
            })?;
            self.connection.feed = Some(feed);
            feed
        };
        if self.options.public_data_only && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return Ok(());
        }
        cx.connect_socket(
            feed,
            safe_cex_engine::SocketSpec::new(self.ws())
                .public_topics(cx.public_feed_policy() == PublicFeedPolicy::Direct),
        )?;
        Ok(())
    }

    pub(super) fn schedule_socket_refresh(&mut self, cx: &mut Cx<'_, Pending>, mode_ready: bool) {
        let private = if self.connection.open {
            self.private_socket_interests(cx.store)
        } else {
            Vec::new()
        };
        if private.is_empty() {
            self.finish_socket_refresh(cx, mode_ready);
            if self.connection.open {
                self.propose_socket_interests(cx);
            }
            return;
        }
        let Some(feed) = self.connection.feed else {
            return;
        };
        let desired = self.bounded_socket_interests(cx);
        let admitted: HashSet<_> = desired.iter().map(|i| i.key.clone()).collect();
        let required: Vec<_> = private
            .iter()
            .filter(|interest| admitted.contains(&interest.key))
            .map(|interest| interest.key.as_ref())
            .collect();
        let epoch = cx.socket_epoch(feed);
        if let Err(error) = cx.set_socket_interests_then(
            feed,
            desired,
            &required,
            Pending::SocketRefresh { mode_ready },
        ) {
            Self::socket_refused(cx, error);
            if error == SocketError::Capacity && epoch.is_some() && cx.socket_epoch(feed) == epoch {
                cx.retire_socket(feed, RetireReason::Closed);
            }
        } else {
            self.connection.admitted_interests = admitted;
            if let Some(epoch) = epoch {
                self.prepare_private_snapshots(cx, epoch);
            }
        }
    }

    pub(super) fn finish_socket_refresh(&mut self, cx: &mut Cx<'_, Pending>, mode_ready: bool) {
        if !mode_ready
            && self.account.started
            && self.user().is_some()
            && !self.options.public_data_only
        {
            self.refresh_account(cx);
        }
        self.finish_mode_refresh(cx);
    }

    pub(super) fn managed_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                if self.connection.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.connection.open = true;
                if cx.public_feed_policy() == PublicFeedPolicy::Direct {
                    for ladder in self.books.values_mut() {
                        ladder.spot_depth = Default::default();
                        ladder.depth_time = None;
                        ladder.fast_depth = None;
                        ladder.depth = Default::default();
                        ladder.bbo = None;
                        ladder.book.clear();
                    }
                }
                self.schedule_socket_refresh(cx, false);
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.connection.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.clear_private_snapshot_watch(cx);
                self.connection.open = false;
                for ladder in self.books.values_mut() {
                    ladder.spot_depth = Default::default();
                    ladder.depth_time = None;
                    ladder.fast_depth = None;
                    ladder.depth = Default::default();
                    ladder.book.clear();
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "{\"method\":\"ping\"}"),
            LifecycleEvent::Reconnect(_) => self.managed_connect(cx)?,
            LifecycleEvent::InterestsRefused { feed, error, .. } => {
                if error != SocketError::Capacity || self.connection.feed != Some(feed) {
                    return Err(error);
                }
                Self::socket_refused(cx, error);
                cx.retire_socket(feed, RetireReason::Closed);
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn managed_socket_encode(
        &self,
        store: &safe_cex_engine::Store,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.connection.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        let desired = self.desired_socket_interests(store, PublicFeedPolicy::Direct);
        let ranks: HashMap<_, _> = desired
            .iter()
            .enumerate()
            .map(|(index, interest)| (interest.key.as_ref(), index))
            .collect();
        let mut ordered: Vec<_> = interests
            .iter()
            .map(|interest| {
                let value: EncodedTopic<'_> = serde_json::from_str(&interest.wire_args)
                    .map_err(|_| SocketError::InvalidInterest)?;
                let figures = value.subscription.figures.unwrap_or(0);
                let book_order = 2 * usize::from(value.subscription.kind == "bbo")
                    + usize::from(value.subscription.fast);
                Ok((
                    interest,
                    ranks
                        .get(interest.key.as_ref())
                        .copied()
                        .unwrap_or(usize::MAX),
                    book_order,
                    figures,
                ))
            })
            .collect::<Result<_, SocketError>>()?;
        ordered.sort_by_key(|(_, rank, book_order, figures)| (*rank, *book_order, *figures));
        for (interest, _, _, _) in ordered {
            let text = if subscribe {
                interest.wire_args.to_string()
            } else {
                interest.wire_args.replacen(
                    "\"method\":\"subscribe\"",
                    "\"method\":\"unsubscribe\"",
                    1,
                )
            };
            emit(EncodedBatch {
                text,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
}
