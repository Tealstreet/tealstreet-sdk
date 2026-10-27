//! Native wire codecs; the engine owns epochs, authentication and retries.
use super::*;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};
use std::collections::BTreeSet;

pub(super) enum Change {
    AddBook(String),
    RemoveBook(String),
    AddCandle(String, Interval),
    RemoveCandle(String, Interval),
}
fn policy(private: bool) -> FeedPolicy {
    FeedPolicy {
        role: if private {
            safe_cex_engine::SocketChannel::Private
        } else {
            safe_cex_engine::SocketChannel::Public
        },
        supervision: Config {
            connect_timeout_ms: if private { 10_000. } else { 20_000. },
            auth_timeout_ms: private.then_some(10_000.),
            idle_timeout_ms: 20_000.,
            ping_interval_ms: Some(5_000.),
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
        controls: ControlPolicy::Native,
        invalidation: if private {
            Invalidation::None
        } else {
            Invalidation::Required
        },
        subscription_interval_ms: 0.,
    }
}
fn interest(channel: &str, id: &str) -> Interest {
    Interest::from_wire_args(
        serde_json::to_string(&Topic {
            channel,
            inst_id: id,
        })
        .expect("native topic"),
    )
}
impl Blofin {
    pub(super) fn socket_notice(cx: &mut Cx<'_, Pending>, error: SocketError) {
        if error != SocketError::InvalidTime {
            cx.notice(
                Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("BloFin socket lifecycle: {error:?}"),
            );
        }
    }
    pub(super) fn register_sockets(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        if self.public.feed.is_some() {
            return Ok(());
        }
        let public = cx.register_socket_feed(policy(false))?;
        self.public.feed = Some(public);
        if self.signed() {
            match cx.register_socket_feed(policy(true)) {
                Ok(feed) => self.account.connection.feed = Some(feed),
                Err(error) => {
                    cx.stop_socket_feed(public);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    fn private_feed(&self, feed: FeedId) -> Result<bool, SocketError> {
        if self.public.feed == Some(feed) {
            Ok(false)
        } else if self.account.connection.feed == Some(feed) {
            Ok(true)
        } else {
            Err(SocketError::Stale)
        }
    }
    pub(super) fn connect_feed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        if private && self.account.startup_rejected
            || !private && cx.public_feed_policy() != PublicFeedPolicy::Direct
        {
            return Ok(());
        }
        let feed = if private {
            self.account.connection.feed
        } else {
            self.public.feed
        }
        .ok_or(SocketError::NotPermitted)?;
        let url = match (self.options.testnet, private) {
            (true, true) => "wss://demo-trading-openapi.blofin.com/ws/private",
            (true, false) => "wss://demo-trading-openapi.blofin.com/ws/public",
            (false, true) => "wss://openapi.blofin.com/ws/private",
            (false, false) => "wss://openapi.blofin.com/ws/public",
        };
        let epoch = cx.connect_socket(feed, safe_cex_engine::SocketSpec::new(url.to_owned()))?;
        if private {
            self.account.authenticated = false;
            self.account.connection.socket = Some(epoch.socket());
        } else {
            self.public.socket = Some(epoch.socket());
        }
        match cx.set_socket_interests(feed, self.desired_interests(cx.store, private, None)) {
            Ok(_) => (),
            Err(SocketError::Capacity) if !private => {
                Self::socket_notice(cx, SocketError::Capacity);
                cx.retire_socket(feed, RetireReason::Closed);
            }
            Err(error) => return Err(error),
        }
        Ok(())
    }
    fn desired_interests(
        &self,
        store: &safe_cex_engine::Store,
        private: bool,
        change: Option<&Change>,
    ) -> Vec<Interest> {
        if private {
            return ["account", "positions", "orders", "orders-algo"]
                .into_iter()
                .map(|channel| Interest::from_wire_args(format!("{{\"channel\":\"{channel}\"}}")))
                .collect();
        }
        let mut topics = BTreeSet::new();
        let mut add = |symbol: &str, channels: &[&str]| {
            if let Some(m) = store.market(symbol) {
                for channel in channels {
                    topics.insert(interest(channel, &m.id).wire_args.to_string());
                }
            }
        };
        for m in store.markets() {
            add(&m.symbol, &["tickers", "funding-rate"]);
        }
        for symbol in self.books.keys() {
            if !matches!(change, Some(Change::RemoveBook(s)) if s == symbol) {
                add(symbol, &["books", "books5"]);
            }
        }
        for (symbol, i) in self.candles.keys() {
            if !matches!(change, Some(Change::RemoveCandle(s, n)) if s == symbol && n == i)
                && let Some(bar) = interval(*i)
            {
                add(symbol, &[&format!("candle{bar}"), "books5"]);
            }
        }
        match change {
            Some(Change::AddBook(symbol)) => add(symbol, &["books", "books5"]),
            Some(Change::AddCandle(symbol, i)) => {
                if let Some(bar) = interval(*i) {
                    add(symbol, &[&format!("candle{bar}"), "books5"]);
                }
            }
            _ => (),
        }
        topics.into_iter().map(Interest::from_wire_args).collect()
    }
    fn commit_change(&mut self, change: Change) {
        match change {
            Change::AddBook(symbol) => {
                self.books.insert(
                    symbol,
                    DepthState {
                        subscribers: 1,
                        ..Default::default()
                    },
                );
            }
            Change::RemoveBook(symbol) => {
                self.books.remove(&symbol);
            }
            Change::AddCandle(symbol, i) => {
                self.candles.insert(
                    (symbol, i),
                    CandleState {
                        subscribers: 1,
                        last: None,
                    },
                );
            }
            Change::RemoveCandle(symbol, i) => {
                self.candles.remove(&(symbol, i));
            }
        }
    }
    fn change_interests(&mut self, cx: &mut Cx<'_, Pending>, change: Change) {
        if !self.public.open {
            self.commit_change(change);
            return;
        }
        if self.public.pending.is_some() {
            Self::socket_notice(cx, SocketError::Capacity);
            return;
        }
        let Some(feed) = self.public.feed else {
            return;
        };
        match cx.set_socket_interests(feed, self.desired_interests(cx.store, false, Some(&change)))
        {
            Ok(proposal) => self.public.pending = Some((proposal, change)),
            Err(error) => Self::socket_notice(cx, error),
        }
    }
    pub(super) fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if let Some(state) = self.books.get_mut(&symbol) {
            state.subscribers += 1;
            return;
        }
        self.change_interests(cx, Change::AddBook(symbol));
    }
    pub(super) fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if state.subscribers > 1 {
            state.subscribers -= 1;
            return;
        }
        self.change_interests(cx, Change::RemoveBook(symbol));
    }
    pub(super) fn subscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        i: Interval,
    ) {
        if let Some(state) = self.candles.get_mut(&(symbol.clone(), i)) {
            state.subscribers += 1;
            return;
        }
        self.change_interests(cx, Change::AddCandle(symbol, i));
    }
    pub(super) fn unsubscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        i: Interval,
    ) {
        let Some(state) = self.candles.get_mut(&(symbol.clone(), i)) else {
            return;
        };
        if state.subscribers > 1 {
            state.subscribers -= 1;
            return;
        }
        self.change_interests(cx, Change::RemoveCandle(symbol, i));
    }
    pub(super) fn repair_book_interest(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(feed) = self.public.feed
            && let Some(market) = cx.store.market(symbol)
        {
            let key = interest("books", &market.id).key.to_string();
            if let Err(error) = cx.repair_socket_interests(feed, vec![key]) {
                Self::socket_notice(cx, error);
            }
        }
    }
    pub(super) fn classify_control(&self, feed: FeedId, text: &str) -> Control {
        if text == "pong" {
            return Control::Pong;
        }
        if self.account.connection.feed != Some(feed) {
            return Control::Other;
        }
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return Control::Other;
        };
        if m.event == "login" || m.event == "error" && !self.account.authenticated {
            let evidence = if m.event == "login" && m.code == "0" {
                AuthEvidence::Accepted
            } else if m.code.is_empty() {
                AuthEvidence::Malformed
            } else {
                let mut error = VenueError::new(format!("BloFin login rejected: {}", m.code))
                    .venue_code(m.code.to_string());
                if !m.msg.is_empty() {
                    error = error.venue_message(m.msg.to_string());
                }
                AuthEvidence::Rejected(error)
            };
            return Control::Auth(evidence);
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
            | LifecycleEvent::Authenticated { epoch, .. }
            | LifecycleEvent::AuthFailure { epoch, .. }
            | LifecycleEvent::Retired { epoch, .. } => epoch.feed(),
            LifecycleEvent::Reconnect(feed)
            | LifecycleEvent::InterestsCommitted { feed, .. }
            | LifecycleEvent::InterestsRefused { feed, .. } => *feed,
        };
        let private = self.private_feed(feed)?;
        match event {
            LifecycleEvent::Opened(epoch) if private => self.private_open(cx, epoch.socket()),
            LifecycleEvent::Opened(_) => {
                self.public.open = true;
                cx.send_after_socket_interests(feed, "ping".into())?;
            }
            LifecycleEvent::Authenticated { first: true, .. } if private => self.private_ready(cx),
            LifecycleEvent::AuthFailure { evidence, .. } => {
                let error = match evidence {
                    AuthEvidence::Rejected(error) => error,
                    _ => VenueError::new("BloFin malformed login acknowledgement"),
                };
                self.private_auth_failure(cx, error);
            }
            LifecycleEvent::Retired { .. } if private => {
                self.account.connection.socket = None;
                self.account.connection.open = false;
                self.account.authenticated = false;
            }
            LifecycleEvent::Retired { .. } => {
                self.public.socket = None;
                self.public.open = false;
                if let Some((_, change)) = self.public.pending.take() {
                    self.commit_change(change);
                }
                for (symbol, state) in &mut self.books {
                    state.book.clear();
                    state.sequence = None;
                    state.resync = false;
                    cx.book_changed(symbol);
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "ping"),
            LifecycleEvent::Reconnect(_) => self.connect_feed(cx, private)?,
            LifecycleEvent::InterestsCommitted { proposal, .. } if !private => {
                if self
                    .public
                    .pending
                    .as_ref()
                    .is_some_and(|(own, _)| *own == proposal)
                {
                    let (_, change) = self.public.pending.take().expect("exact proposal");
                    self.commit_change(change);
                }
            }
            LifecycleEvent::InterestsRefused {
                error: SocketError::Capacity,
                ..
            } if !private => {
                if let Some((_, change)) = self.public.pending.take() {
                    self.commit_change(change);
                }
                Self::socket_notice(cx, SocketError::Capacity);
                if cx.socket_epoch(feed).is_some() {
                    cx.retire_socket(feed, RetireReason::Closed);
                }
            }
            LifecycleEvent::InterestsRefused { error, .. } => {
                self.public.pending = None;
                return Err(error);
            }
            _ => (),
        }
        Ok(())
    }
    pub(super) fn encode_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        let private = self.private_feed(feed)?;
        let op = if subscribe {
            "subscribe"
        } else {
            "unsubscribe"
        };
        if private {
            let mut ordered: Vec<_> = interests.iter().collect();
            ordered.sort_by_key(|item| {
                ["account", "positions", "orders", "orders-algo"]
                    .iter()
                    .position(|channel| {
                        item.wire_args.as_ref() == format!("{{\"channel\":\"{channel}\"}}")
                    })
                    .unwrap_or(usize::MAX)
            });
            for item in ordered {
                emit(EncodedBatch {
                    text: format!("{{\"op\":\"{op}\",\"args\":[{}]}}", item.wire_args),
                    covered_keys: vec![item.key.clone()],
                })?;
            }
            return Ok(());
        }
        let overhead = format!("{{\"op\":\"{op}\",\"args\":[]}}").len();
        let mut args = Vec::new();
        let mut keys = Vec::new();
        let mut length = overhead;
        for item in interests {
            let raw = item.wire_args.as_ref();
            if overhead + raw.len() > 4096 {
                return Err(SocketError::Capacity);
            }
            serde_json::from_str::<&serde_json::value::RawValue>(raw)
                .map_err(|_| SocketError::InvalidInterest)?;
            if length + raw.len() + usize::from(!args.is_empty()) > 4096 {
                emit(EncodedBatch {
                    text: format!("{{\"op\":\"{op}\",\"args\":[{}]}}", args.join(",")),
                    covered_keys: std::mem::take(&mut keys),
                })?;
                args.clear();
                length = overhead;
            }
            length += raw.len() + usize::from(!args.is_empty());
            args.push(raw);
            keys.push(item.key.clone());
        }
        if !args.is_empty() {
            emit(EncodedBatch {
                text: format!("{{\"op\":\"{op}\",\"args\":[{}]}}", args.join(",")),
                covered_keys: keys,
            })?;
        }
        Ok(())
    }
}
