//! Native topic projection and wire encoding; the shared core owns transport.
use super::*;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, Control, ControlPolicy, DataSilence, EncodedBatch, Epoch, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, TrafficPolicy,
};
use std::borrow::Cow;
use std::collections::BTreeMap;

// Diagnostic identity within this runtime; never used for socket or write authority.
pub(super) fn next_repair_boot_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

const PRIVATE_SNAPSHOT_TIMEOUT_MS: f64 = 30_000.;
const MAX_PRIVATE_SNAPSHOT_TIMEOUT_MS: f64 = 300_000.;
/// Mids and asset contexts tick every second; half a minute without them is a dead
/// feed. The engine's repair threshold, so a repair and the watch agree on silence.
const PUBLIC_SILENCE_MS: f64 = safe_cex_engine::engine::REPAIR_SILENCE_MS;
const PUBLIC_STREAMS: &[&str] = &[
    r#"{"method":"subscribe","subscription":{"type":"allMids""#,
    r#"{"method":"subscribe","subscription":{"type":"fastAssetCtxs""#,
    r#"{"method":"subscribe","subscription":{"type":"l2Book""#,
    r#"{"method":"subscribe","subscription":{"type":"bbo""#,
];
/// Refusals named per socket; a capped IP refuses every later subscription.
const REFUSAL_NOTICES: u8 = 3;

/// Only public market data proves the public streams alive; account frames
/// sharing the socket and subscription replies are delivered without counting.
pub(super) fn managed_control(text: &str) -> Control {
    let channel = match text
        .trim_start()
        .strip_prefix(r#"{"channel":""#)
        .and_then(|rest| rest.split('"').next())
    {
        Some(channel) => Cow::Borrowed(channel),
        None => match serde_json::from_str::<Envelope>(text) {
            Ok(frame) => frame.channel,
            Err(_) => return Control::Unwatched,
        },
    };
    match channel.as_ref() {
        "pong" => Control::Pong,
        "allMids" | "fastAssetCtxs" | "l2Book" | "bbo" | "candle" => Control::Other,
        _ => Control::Unwatched,
    }
}

/// A subscription request or reply: the subscription it names.
#[derive(serde::Deserialize)]
struct Subscribing<'a> {
    #[serde(borrow)]
    subscription: &'a serde_json::value::RawValue,
}

/// The subscription a frame names, alone or as the `subscription` member of a request.
fn named_subscription(raw: &str) -> &str {
    serde_json::from_str::<Subscribing<'_>>(raw).map_or(raw, |request| request.subscription.get())
}

/// A subscription as the venue echoes it, order-free and without the defaults
/// it adds (`dex: ""`, `aggregateByTime: false`).
fn canonical_subscription(raw: &str) -> Option<String> {
    let mut fields: BTreeMap<Cow<'_, str>, &serde_json::value::RawValue> =
        serde_json::from_str(named_subscription(raw)).ok()?;
    fields.retain(|_, value| !matches!(value.get(), r#""""# | "false" | "null"));
    fields
        .contains_key("type")
        .then(|| serde_json::to_string(&fields).ok())?
}

/// The refused subscription's label for a notice: its type and market, never the user.
fn subscription_label(key: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Label<'a> {
        #[serde(borrow, rename = "type")]
        kind: Cow<'a, str>,
        #[serde(borrow, default)]
        coin: Option<Cow<'a, str>>,
        #[serde(borrow, default)]
        interval: Option<Cow<'a, str>>,
        #[serde(borrow, default)]
        dex: Option<Cow<'a, str>>,
    }
    let Ok(label) = serde_json::from_str::<Label<'_>>(named_subscription(key)) else {
        return String::from("a subscription");
    };
    [Some(label.kind), label.coin, label.interval, label.dex]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

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

/// One encoded subscribe or unsubscribe frame awaiting the venue's answer.
pub(super) struct Unanswered {
    subscribe: bool,
    key: std::sync::Arc<str>,
    /// The engine's released ledger showed this frame went out.
    sent: bool,
}

/// Public market data and pongs: frames that cannot change which symbols hold orders.
pub(super) fn public_market_frame(text: &str) -> bool {
    text.trim_start()
        .strip_prefix(r#"{"channel":""#)
        .and_then(|rest| rest.split('"').next())
        .is_some_and(|channel| {
            matches!(
                channel,
                "allMids" | "fastAssetCtxs" | "l2Book" | "bbo" | "candle" | "trades" | "pong"
            )
        })
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

    pub(super) fn unnamed_candle_repair(
        &self,
        cx: &mut Cx<'_, Pending>,
        topic: &SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        let SubscriptionTopic::Candles { symbol, interval } = topic else {
            return None;
        };
        let known = cx.store.market(symbol).is_some();
        let discovery = if self.discovery_settled() && (self.spot || self.perps_published) {
            "complete"
        } else {
            "pending"
        };
        let reason = if !known {
            "unknown-market"
        } else if self.connection.feed.is_some() {
            // Registered topics keep the existing refusal retry and unsubscribed
            // behavior; readiness attribution only explains an unnamed stream.
            return None;
        } else if !self.candles.contains_key(&(symbol.clone(), *interval)) {
            "not-subscribed"
        } else if self.connection.feed_refused {
            "feed-refused"
        } else {
            "feed-not-started"
        };
        let attribution = format!(
            "{reason} (engineBoot={},symbolKnown={known},catalogueSize={},discovery={discovery},feedRegistered={})",
            self.repair_boot_sequence,
            cx.store.markets().len(),
            self.connection.feed.is_some()
        );
        if reason == "feed-not-started" {
            cx.notice(
                safe_cex_engine::Level::Debug,
                code::SUBSCRIPTION_DEFERRED,
                format!("{topic} repair deferred: {attribution}"),
            );
            Some(Ok(()))
        } else {
            Some(Err(attribution))
        }
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
        // Carries every HIP-3 dex's mids and marks, so no per-dex allMids.
        if !self.spot {
            add(&mut interests, "fastAssetCtxs", None, None, None);
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
        // One trades stream per watched coin, shared by its books and charts.
        for symbol in self.watched_symbols() {
            if let Some(market) = store.market(symbol) {
                interests.push(Interest::from_wire_args(public_topic(
                    "trades",
                    Some(&market.id),
                    None,
                    None,
                )));
            }
        }
        if !self.connection.refused.is_empty() {
            interests.retain(|interest| !self.connection.refused.contains(interest.key.as_ref()));
        }
        interests
    }

    /// A repair of a topic whose subscribe the venue refused (a chart's forced reset)
    /// sends it once more; another refusal withdraws it again, with a notice.
    pub(super) fn retry_refused(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        let mut retried = false;
        for (_, key) in self.topic_interests(cx.store, topic) {
            retried |= self.connection.refused.remove(&key);
        }
        if !retried {
            return None;
        }
        self.propose_socket_interests(cx);
        Some(Ok(()))
    }

    /// A topic subscribed afresh may try its refused keys again.
    pub(super) fn forget_refusals(
        &mut self,
        store: &safe_cex_engine::Store,
        topic: &SubscriptionTopic,
    ) {
        if self.connection.refused.is_empty() {
            return;
        }
        for (_, key) in self.topic_interests(store, topic) {
            self.connection.refused.remove(&key);
        }
        // The coin's trades stream is retried with the book or chart that watches it.
        let (SubscriptionTopic::OrderBook { symbol } | SubscriptionTopic::Candles { symbol, .. }) =
            topic;
        if let Some(market) = store.market(symbol) {
            let trades = public_topic("trades", Some(&market.id), None, None);
            self.connection.refused.remove(&trades);
        }
    }

    /// A new proposal supersedes the engine's queued but unsent frames: keep only frames
    /// its released ledger shows went out (a subscribe released, an unsubscribe gone).
    /// A subscribe followed by an unsubscribe of its key was sent: the engine encodes
    /// an unsubscribe only for a released key, though the ledger no longer holds it.
    pub(super) fn forget_unsent(&self, cx: &Cx<'_, Pending>, feed: FeedId) {
        let mut frames = self.connection.unanswered.borrow_mut();
        for index in 0..frames.len() {
            let undone = frames[index].subscribe
                && frames
                    .iter()
                    .skip(index + 1)
                    .any(|later| !later.subscribe && later.key == frames[index].key);
            let frame = &mut frames[index];
            frame.sent |= undone || cx.socket_released(feed, &frame.key) == frame.subscribe;
        }
        frames.retain(|frame| frame.sent);
    }

    /// Acknowledgements and refusals answer this engine's subscribe frames, in order.
    pub(super) fn subscription_reply(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return false;
        };
        match frame.channel.as_ref() {
            "subscriptionResponse" => {
                self.connection.unanswered.borrow_mut().pop_front();
                true
            }
            "error" => {
                let answered = self
                    .connection
                    .unanswered
                    .borrow_mut()
                    .pop_front()
                    .map(|frame| (frame.subscribe, frame.key));
                if let Ok(message) = serde_json::from_str::<Cow<'_, str>>(frame.data.get()) {
                    self.subscription_refused(cx, &message, answered);
                }
                true
            }
            _ => false,
        }
    }

    /// A refusal names its subscription, except the per-IP cap's; that one answers
    /// the oldest frame this socket sent and the venue has not yet answered.
    fn subscription_refused(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        message: &str,
        answered: Option<(bool, std::sync::Arc<str>)>,
    ) {
        if message.starts_with("Already subscribed") || message.starts_with("Already unsubscribed")
        {
            return;
        }
        let named = message.find('{').and_then(|at| {
            let raw = serde_json::Deserializer::from_str(&message[at..])
                .into_iter::<&serde_json::value::RawValue>()
                .next()?
                .ok()?;
            canonical_subscription(raw.get())
        });
        let refused = match named {
            Some(named) => self
                .desired_socket_interests(cx.store, cx.public_feed_policy())
                .into_iter()
                .find(|interest| canonical_subscription(&interest.wire_args) == Some(named.clone()))
                .map(|interest| interest.key.to_string()),
            None => answered
                .filter(|(subscribe, _)| *subscribe)
                .map(|(_, key)| key.to_string()),
        };
        if self.connection.refusal_notices < REFUSAL_NOTICES {
            self.connection.refusal_notices += 1;
            let label = refused
                .as_deref()
                .map_or_else(|| "a subscription".to_owned(), subscription_label);
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                format!("Hyperliquid refused {label}: {message}"),
            );
        }
        if let Some(key) = refused {
            self.connection.refused.insert(key);
            self.propose_socket_interests(cx);
        }
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

    /// One account's subscription count, when it changes and at most once a minute:
    /// Hyperliquid caps an IP at 1,000, shared by every account behind it.
    pub(super) fn report_subscriptions(&mut self, cx: &mut Cx<'_, Pending>, desired: &[Interest]) {
        let total = desired.len();
        if self
            .connection
            .reported_subscriptions
            .is_some_and(|(count, at)| count == total || cx.now - at < 60_000.)
        {
            return;
        }
        self.connection.reported_subscriptions = Some((total, cx.now));
        let private = desired
            .iter()
            .filter(|interest| interest.wire_args.contains("\"user\":"))
            .count();
        let settings = desired
            .iter()
            .filter(|interest| interest.wire_args.contains("\"type\":\"activeAssetData\""))
            .count();
        cx.event(
            safe_cex_engine::Level::Info,
            code::SUBSCRIPTION_COUNT,
            format!(
                "Hyperliquid account holds {total} subscriptions ({private} private, {settings} activeAssetData)"
            ),
            vec![
                ("subscriptions", (total as f64).into()),
                ("publicSubscriptions", ((total - private) as f64).into()),
                ("privateSubscriptions", (private as f64).into()),
                ("activeAssetDataSubscriptions", (settings as f64).into()),
            ],
        );
    }

    pub(super) fn managed_connect(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = if let Some(feed) = self.connection.feed {
            feed
        } else {
            let registered = cx.register_socket_feed(FeedPolicy {
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
                controls: ControlPolicy::Native,
                invalidation: Invalidation::Required,
                subscription_interval_ms: 0.,
                close_floors: &[],
                // Pongs and account frames never count, so a keyed socket is watched too.
                data_silence: Some(DataSilence {
                    after_ms: PUBLIC_SILENCE_MS,
                    streams: PUBLIC_STREAMS,
                }),
            });
            let feed = match registered {
                Ok(feed) => feed,
                Err(error) => {
                    self.connection.feed_refused = true;
                    return Err(error);
                }
            };
            self.connection.feed_refused = false;
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
        self.report_subscriptions(cx, &desired);
        let admitted: HashSet<_> = desired.iter().map(|i| i.key.clone()).collect();
        let required: Vec<_> = private
            .iter()
            .filter(|interest| admitted.contains(&interest.key))
            .map(|interest| interest.key.as_ref())
            .collect();
        let epoch = cx.socket_epoch(feed);
        self.forget_unsent(cx, feed);
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
            // Frames encoded before this socket opened were never sent on it; Ready
            // replays every desired interest through the encoder afterwards.
            LifecycleEvent::Opened(epoch) if self.connection.feed == Some(epoch.feed()) => {
                self.connection.unanswered.borrow_mut().clear();
            }
            LifecycleEvent::Ready { epoch, .. } => {
                if self.connection.feed != Some(epoch.feed()) {
                    return Err(SocketError::Stale);
                }
                self.connection.open = true;
                self.connection.refusal_notices = 0;
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
                // A reconnect resumes the mid as last until its first trade.
                self.trade_last.clear();
                // A new socket tries every topic again; a repeat refusal is reported again.
                self.connection.refused.clear();
                self.connection.unanswered.borrow_mut().clear();
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
                // Within a coin: depth (slow, then fast), then bbo, then trades.
                let book_order = if value.subscription.kind == "trades" {
                    3
                } else {
                    2 * usize::from(value.subscription.kind == "bbo")
                        + usize::from(value.subscription.fast)
                };
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
            self.connection
                .unanswered
                .borrow_mut()
                .push_back(Unanswered {
                    subscribe,
                    key: interest.key.clone(),
                    sent: false,
                });
        }
        Ok(())
    }
}

#[cfg(test)]
mod repair_readiness_tests {
    use super::*;
    use safe_cex_engine::test_support::{ProbeVenue, seeded_engine};
    use safe_cex_engine::{Action, Notice, Venue};

    #[test]
    fn a_refused_feed_registration_keeps_later_candle_repairs_visible() {
        let venue = Hyperliquid::new(Options {
            public_data_only: true,
            ..Options::default()
        });
        let mut engine = seeded_engine(ProbeVenue::new(venue, |venue, cx| {
            cx.store.set_markets(vec![Market {
                id: "HYPE".into(),
                symbol: "HYPE-PERP".into(),
                pretty_symbol: None,
                base: "HYPE".into(),
                quote: "USDC".into(),
                active: true,
                precision: Precision {
                    amount: 1.,
                    price: 1.,
                },
                limits: Limits {
                    amount: MinMax {
                        min: 1.,
                        max: 1000.,
                    },
                    leverage: MinMax { min: 1., max: 50. },
                    price: None,
                },
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            }]);
            venue.candles.insert(("HYPE-PERP".into(), Interval::H4), 1);
            let policy = FeedPolicy {
                role: safe_cex_engine::SocketChannel::Public,
                supervision: Config::default(),
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::Required,
                subscription_interval_ms: 0.,
                close_floors: &[],
                data_silence: None,
            };
            for _ in 0..safe_cex_engine::socket_lifecycle::MAX_FEEDS {
                cx.register_socket_feed(policy.clone()).unwrap();
            }
            venue.start_socket(cx);
            assert!(venue.connection.feed.is_none());
            for _ in 0..2 {
                let reason = venue
                    .repair_subscription(
                        cx,
                        &SubscriptionTopic::Candles {
                            symbol: "HYPE-PERP".into(),
                            interval: Interval::H4,
                        },
                    )
                    .unwrap()
                    .unwrap_err();
                assert!(reason.starts_with("feed-refused"), "{reason}");
                assert!(reason.contains("symbolKnown=true"), "{reason}");
            }
        }));
        engine.set_log_level(safe_cex_engine::Level::Debug);
        engine.start(1000.);
        let actions: Vec<_> = engine.drain_actions().collect();
        let events: Vec<_> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event)) => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 1, "{actions:?}");
        assert_eq!(events[0].level, safe_cex_engine::Level::Error);
        assert!(events[0].message.contains("Capacity"));
        assert_eq!(actions.len(), 1, "no new IO or repair timer: {actions:?}");
    }
}
