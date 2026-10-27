//! Native wire projection; engine epochs own socket custody and liveness.
use super::*;
use safe_cex_engine::SocketChannel;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
};

#[derive(Default)]
pub(super) struct Connection {
    pub(super) feed: Option<FeedId>,
    pub(super) socket: Option<SocketId>,
    opening: Option<u64>,
}
enum Change {
    AddBook(String),
    RemoveBook(String),
    AddCandle(String, Interval),
    RemoveCandle(String, Interval),
}
fn policy(role: SocketChannel) -> FeedPolicy {
    let private = role == SocketChannel::Private;
    FeedPolicy {
        role,
        supervision: Config {
            idle_timeout_ms: if private { 45_000. } else { 20_000. },
            ping_interval_ms: Some(if private { 15_000. } else { 5_000. }),
            ..Config::default()
        },
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::None,
        invalidation: if private {
            Invalidation::None
        } else {
            Invalidation::Required
        },
        subscription_interval_ms: 0.,
    }
}
impl Bingx {
    pub(super) fn socket_notice(cx: &mut Cx<'_, Pending>, error: SocketError) {
        if error == SocketError::InvalidTime {
            return;
        }
        cx.notice(
            Level::Error,
            code::STREAM_UNAVAILABLE,
            format!("BingX socket lifecycle: {error:?}"),
        );
    }
    pub(super) fn register_sockets(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        if !cx.now.is_finite() {
            return Err(SocketError::InvalidTime);
        }
        let public = cx.register_socket_feed(policy(SocketChannel::Public))?;
        self.public.feed = Some(public);
        if self.credentials.is_some() {
            match cx.register_socket_feed(policy(SocketChannel::Private)) {
                Ok(feed) => self.account.feed = Some(feed),
                Err(error) => {
                    cx.stop_socket_feed(public);
                    return Err(error);
                }
            }
        }
        Ok(())
    }
    pub(super) fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = self.public.feed.ok_or(SocketError::NotPermitted)?;
        let epoch = cx.connect_socket(
            feed,
            safe_cex_engine::SocketSpec::new(paths::PUBLIC_WS.to_owned()),
        )?;
        self.public.socket = Some(epoch.socket());
        Ok(())
    }
    fn topics(&self, cx: &Cx<'_, Pending>, change: Option<&Change>) -> Vec<Interest> {
        let mut topics = vec![Interest::from_wire_args("market.contracts")];
        for symbol in self.books.keys() {
            if !matches!(change,Some(Change::RemoveBook(s)) if s==symbol)
                && let Some(topic) = self.topic_book(cx, symbol)
            {
                topics.push(Interest::from_wire_args(topic));
            }
        }
        for (symbol, interval) in self.candles.keys() {
            if !matches!(change,Some(Change::RemoveCandle(s,i)) if s==symbol && i==interval)
                && let Some(topic) = self.topic_candle(cx, symbol, *interval)
            {
                topics.push(Interest::from_wire_args(topic));
            }
        }
        match change {
            Some(Change::AddBook(s)) => {
                if let Some(topic) = self.topic_book(cx, s) {
                    topics.push(Interest::from_wire_args(topic));
                }
            }
            Some(Change::AddCandle(s, i)) => {
                if let Some(topic) = self.topic_candle(cx, s, *i) {
                    topics.push(Interest::from_wire_args(topic));
                }
            }
            _ => {}
        }
        topics
    }
    fn commit_change(&mut self, change: Change) {
        match change {
            Change::AddBook(s) => {
                self.books.insert(
                    s,
                    BookState {
                        subscribers: 1,
                        ..Default::default()
                    },
                );
            }
            Change::RemoveBook(s) => {
                self.books.remove(&s);
            }
            Change::AddCandle(s, i) => {
                self.candles.insert((s, i), 1);
            }
            Change::RemoveCandle(s, i) => {
                self.candles.remove(&(s, i));
            }
        }
    }
    fn change_interests(&mut self, cx: &mut Cx<'_, Pending>, change: Change) {
        let Some(feed) = self.public.feed else {
            self.commit_change(change);
            return;
        };
        match cx.set_socket_interests(feed, self.topics(cx, Some(&change))) {
            Ok(_) => self.commit_change(change),
            Err(error) => Self::socket_notice(cx, error),
        }
    }
    pub(super) fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if let Some(b) = self.books.get_mut(&symbol) {
            b.subscribers += 1;
            return;
        }
        self.change_interests(cx, Change::AddBook(symbol));
    }
    pub(super) fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Some(b) = self.books.get_mut(&symbol) else {
            return;
        };
        if b.subscribers > 1 {
            b.subscribers -= 1;
            return;
        }
        self.change_interests(cx, Change::RemoveBook(symbol));
    }
    pub(super) fn subscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: Interval,
    ) {
        if let Some(n) = self.candles.get_mut(&(symbol.clone(), interval)) {
            *n += 1;
            return;
        }
        self.change_interests(cx, Change::AddCandle(symbol, interval));
    }
    pub(super) fn unsubscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: Interval,
    ) {
        let Some(n) = self.candles.get_mut(&(symbol.clone(), interval)) else {
            return;
        };
        if *n > 1 {
            *n -= 1;
            return;
        }
        self.change_interests(cx, Change::RemoveCandle(symbol, interval));
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
            LifecycleEvent::Reconnect(f)
            | LifecycleEvent::InterestsCommitted { feed: f, .. }
            | LifecycleEvent::InterestsRefused { feed: f, .. } => *f,
        };
        if self.account.feed == Some(feed) {
            if let Some(c) = &self.credentials {
                self.account.socket_event(cx, event, c)?;
            }
            return Ok(());
        }
        if self.public.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                self.public.socket = Some(epoch.socket());
                self.public.opening = Some(cx.set_socket_interests(feed, self.topics(cx, None))?);
            }
            LifecycleEvent::InterestsCommitted { proposal, .. } => {
                if self.public.opening == Some(proposal) {
                    self.public.opening = None;
                    cx.send_after_socket_interests(feed, "Ping".into())?;
                }
            }
            LifecycleEvent::Retired { .. } => {
                self.public.socket = None;
                self.public.opening = None;
                for (symbol, b) in &mut self.books {
                    b.ready = false;
                    b.book.clear();
                    cx.book_changed(symbol);
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "Ping"),
            LifecycleEvent::Reconnect(_) => self.connect_public(cx)?,
            LifecycleEvent::InterestsRefused {
                error: SocketError::Capacity,
                ..
            } => {
                Self::socket_notice(cx, SocketError::Capacity);
                if cx.socket_epoch(feed).is_some() {
                    cx.retire_socket(feed, RetireReason::Closed);
                }
            }
            LifecycleEvent::InterestsRefused { error, .. } => {
                return Err(error);
            }
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
            return Err(SocketError::Stale);
        }
        for interest in items {
            let id = self
                .sequence
                .get()
                .checked_add(1)
                .ok_or(SocketError::Capacity)?;
            self.sequence.set(id);
            let text = format!(
                r#"{{"id":"{id}","reqType":"{}","dataType":{}}}"#,
                if subscribe { "sub" } else { "unsub" },
                serde_json::to_string(interest.wire_args.as_ref())
                    .map_err(|_| SocketError::InvalidInterest)?
            );
            if text.len() > safe_cex_engine::socket_lifecycle::MAX_FRAME_BYTES {
                return Err(SocketError::Capacity);
            }
            emit(EncodedBatch {
                text,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
}
