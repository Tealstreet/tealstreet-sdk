//! Native socket wire and financial invalidation; the engine owns lifecycle.
use super::*;
use safe_cex_engine::SocketChannel;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, Epoch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct Connection {
    pub(super) feed: Option<FeedId>,
    pub(super) socket: Option<SocketId>,
    pub(super) open: bool,
    pub(super) ping_at: f64,
    pub(super) started: bool,
    base_topics: Vec<String>,
    change: Option<(u64, InterestChange)>,
    refresh: Option<u64>,
    refresh_needed: bool,
    snapshots: HashSet<String>,
    armed: HashMap<String, u64>,
    retry: Option<(TimerId, f64)>,
    retries: u8,
    retry_exhausted: bool,
}

enum InterestChange {
    AddBook(String),
    RemoveBook(String),
    AddCandle(String, Interval),
    RemoveCandle(String, Interval),
}

fn policy(role: SocketChannel) -> FeedPolicy {
    FeedPolicy {
        invalidation: if role == SocketChannel::Public {
            Invalidation::Required
        } else {
            Invalidation::None
        },
        role,
        supervision: Config {
            idle_timeout_ms: 20_000.0,
            ping_interval_ms: Some(5_000.0),
            backoff_base_ms: 1_000.0,
            backoff_cap_ms: 16_000.0,
            ..Config::default()
        },
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::Native,
        subscription_interval_ms: 0.0,
    }
}

impl Backpack {
    pub(super) fn register_sockets(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        if !cx.now.is_finite() {
            return Err(SocketError::InvalidTime);
        }
        if self.public.feed.is_some() || self.account.socket.feed.is_some() {
            return Err(SocketError::NotPermitted);
        }
        let public = cx.register_socket_feed(policy(SocketChannel::Public))?;
        self.public.feed = Some(public);
        match cx.register_socket_feed(policy(SocketChannel::Private)) {
            Ok(private) => self.account.socket.feed = Some(private),
            Err(error) => {
                cx.stop_socket_feed(public);
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn connect_feed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        private: bool,
    ) -> Result<(), SocketError> {
        let connection = if private {
            &mut self.account.socket
        } else {
            &mut self.public
        };
        let feed = connection.feed.ok_or(SocketError::NotPermitted)?;
        let epoch = cx.connect_socket(feed, safe_cex_engine::SocketSpec::new(WS))?;
        connection.socket = Some(epoch.socket());
        connection.started = true;
        Ok(())
    }

    pub(super) fn socket_notice(cx: &mut Cx<'_, Pending>, error: SocketError) {
        if error != SocketError::NotPermitted && error != SocketError::InvalidTime {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::STREAM_UNAVAILABLE,
                format!("Backpack socket lifecycle: {error:?}"),
            );
        }
    }

    fn interest_notice(cx: &mut Cx<'_, Pending>, error: SocketError) {
        cx.notice(
            safe_cex_engine::Level::Error,
            code::SUBSCRIPTION_REJECTED,
            format!("Backpack socket interest refused: {error:?}"),
        );
    }

    fn public_topics(&self, change: Option<&InterestChange>) -> Vec<Interest> {
        let mut topics: Vec<_> = self
            .public
            .base_topics
            .iter()
            .cloned()
            .map(Interest::from_wire_args)
            .collect();
        for symbol in self.books.keys() {
            if !matches!(change, Some(InterestChange::RemoveBook(s)) if s == symbol) {
                topics.push(Interest::from_wire_args(format!("depth.{symbol}")));
            }
        }
        for (symbol, interval) in self.candles.keys() {
            if !matches!(change, Some(InterestChange::RemoveCandle(s, i)) if s == symbol && i == interval)
            {
                topics.push(Interest::from_wire_args(format!(
                    "kline.{interval}.{symbol}"
                )));
            }
        }
        match change {
            Some(InterestChange::AddBook(s)) => {
                topics.push(Interest::from_wire_args(format!("depth.{s}")))
            }
            Some(InterestChange::AddCandle(s, i)) => {
                topics.push(Interest::from_wire_args(format!("kline.{i}.{s}")))
            }
            _ => {}
        }
        topics
    }

    fn commit_interest_change(&mut self, change: InterestChange) {
        match change {
            InterestChange::AddBook(symbol) => {
                self.books.insert(
                    symbol.clone(),
                    BookState {
                        book: Book::default(),
                        sync: Sync::Waiting(vec![]),
                        subscribers: 1,
                        generation: 0,
                        recoveries: 0,
                    },
                );
                self.public.snapshots.insert(symbol);
            }
            InterestChange::RemoveBook(symbol) => {
                self.books.remove(&symbol);
                self.public.snapshots.remove(&symbol);
                self.public.armed.remove(&symbol);
            }
            InterestChange::AddCandle(symbol, interval) => {
                self.candles.insert((symbol, interval), 1);
            }
            InterestChange::RemoveCandle(symbol, interval) => {
                self.candles.remove(&(symbol, interval));
            }
        }
    }

    fn change_interests(&mut self, cx: &mut Cx<'_, Pending>, change: InterestChange) {
        if !self.public.open {
            self.commit_interest_change(change);
            return;
        }
        if self.public.change.is_some()
            || matches!(&change, InterestChange::AddBook(_)) && self.public.retry_exhausted
        {
            Self::interest_notice(cx, SocketError::Capacity);
            return;
        }
        let desired = self.public_topics(Some(&change));
        let result = self
            .public
            .feed
            .ok_or(SocketError::Stale)
            .and_then(|feed| cx.set_socket_interests(feed, desired));
        match result {
            Ok(proposal) => {
                self.public.change = Some((proposal, change));
            }
            Err(error) => Self::interest_notice(cx, error),
        }
    }

    pub(super) fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if let Some(state) = self.books.get_mut(&symbol) {
            state.subscribers += 1;
            return;
        }
        self.change_interests(cx, InterestChange::AddBook(symbol));
    }
    pub(super) fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if state.subscribers > 1 {
            state.subscribers -= 1;
            return;
        }
        self.change_interests(cx, InterestChange::RemoveBook(symbol));
    }
    pub(super) fn subscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: Interval,
    ) {
        if let Some(count) = self.candles.get_mut(&(symbol.clone(), interval)) {
            *count += 1;
            return;
        }
        self.change_interests(cx, InterestChange::AddCandle(symbol, interval));
    }
    pub(super) fn unsubscribe_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: Interval,
    ) {
        let Some(count) = self.candles.get_mut(&(symbol.clone(), interval)) else {
            return;
        };
        if *count > 1 {
            *count -= 1;
            return;
        }
        self.change_interests(cx, InterestChange::RemoveCandle(symbol, interval));
    }

    fn current_public_epoch(&self, cx: &Cx<'_, Pending>) -> bool {
        cx.now.is_finite()
            && self.public.open
            && self
                .public
                .feed
                .and_then(|feed| cx.socket_epoch(feed))
                .map(|epoch| epoch.socket())
                == self.public.socket
    }

    fn interest_work_allowed(&self, cx: &Cx<'_, Pending>) -> bool {
        self.current_public_epoch(cx) && self.public.retry.is_none() && !self.public.retry_exhausted
    }

    fn cancel_resolved_retry(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.public.refresh_needed
            && self
                .public
                .snapshots
                .iter()
                .all(|s| self.public.armed.contains_key(s))
            && let Some((timer, _)) = self.public.retry.take()
        {
            cx.clear_timer(timer);
        }
    }

    fn retry_interests(&mut self, cx: &mut Cx<'_, Pending>, error: SocketError) {
        if error != SocketError::Capacity
            || !self.current_public_epoch(cx)
            || self.public.retry.is_some()
            || self.public.retry_exhausted
        {
            return;
        }
        if self.public.retries < 3 && (cx.now + 1000.).is_finite() && cx.now + 1000. > cx.now {
            self.public.retry = Some((cx.set_timer(1000.), cx.now + 1000.));
        } else {
            self.public.retry_exhausted = true;
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                "Backpack socket interest recovery exhausted after 3 observations",
            );
        }
    }

    fn refresh_interests(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.interest_work_allowed(cx) || self.public.change.is_some() {
            return;
        }
        let Some(feed) = self.public.feed else {
            return;
        };
        if self.public.refresh_needed {
            if self.public.refresh.is_none() {
                match cx.set_socket_interests(feed, self.public_topics(None)) {
                    Ok(proposal) => self.public.refresh = Some(proposal),
                    Err(error) => {
                        Self::interest_notice(cx, error);
                        self.retry_interests(cx, error);
                    }
                }
            }
            return;
        }
        self.arm_snapshots(cx);
    }

    fn arm_snapshots(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.interest_work_allowed(cx)
            || self.public.change.is_some()
            || self.public.refresh_needed
        {
            return;
        }
        let Some(feed) = self.public.feed else {
            return;
        };
        let symbols: Vec<_> = self
            .public
            .snapshots
            .iter()
            .filter(|s| !self.public.armed.contains_key(*s))
            .cloned()
            .collect();
        for symbol in symbols {
            let key = format!("depth.{symbol}");
            match cx.set_socket_interests_then(
                feed,
                self.public_topics(None),
                &[&key],
                Pending::SocketSnapshot(symbol.clone()),
            ) {
                Ok(proposal) => {
                    self.public.armed.insert(symbol, proposal);
                }
                Err(error) => {
                    Self::interest_notice(cx, error);
                    self.retry_interests(cx, error);
                    break;
                }
            }
        }
        self.cancel_resolved_retry(cx);
    }

    pub(super) fn request_book_snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if self.books.contains_key(&symbol) {
            self.public.snapshots.insert(symbol);
            self.arm_snapshots(cx);
        }
    }

    pub(super) fn socket_interest_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        timer: TimerId,
    ) -> bool {
        let Some((issued, not_before)) = self.public.retry else {
            return false;
        };
        if issued != timer {
            return false;
        }
        self.public.retry = None;
        if !self.current_public_epoch(cx) {
            self.public.retry_exhausted = true;
            return true;
        }
        if cx.now < not_before {
            let remaining = not_before - cx.now;
            if remaining.is_finite() && cx.now + remaining > cx.now {
                self.public.retry = Some((cx.set_timer(remaining), not_before));
            } else {
                self.public.retry_exhausted = true;
            }
            return true;
        }
        self.public.retries += 1;
        self.refresh_interests(cx);
        true
    }

    pub(super) fn managed_socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                if self.account.socket.feed == Some(epoch.feed()) {
                    if self.account.socket.socket != Some(epoch.socket()) {
                        return Err(SocketError::Stale);
                    }
                    self.private_open(cx, epoch.socket());
                } else if self.public.feed == Some(epoch.feed()) {
                    if self.public.socket != Some(epoch.socket()) {
                        return Err(SocketError::Stale);
                    }
                    self.public.open = true;
                    self.public.ping_at = cx.now;
                    self.public.retries = 0;
                    self.public.retry_exhausted = false;
                    self.public.refresh_needed = true;
                    self.public.base_topics = cx
                        .store
                        .markets()
                        .iter()
                        .flat_map(|m| {
                            [
                                format!("ticker.{}", m.id),
                                format!("trade.{}", m.id),
                                format!("bookTicker.{}", m.id),
                            ]
                        })
                        .collect();
                    self.public.snapshots = self.books.keys().cloned().collect();
                    for state in self.books.values_mut() {
                        state.recoveries = 0;
                    }
                    self.refresh_interests(cx);
                    if let Err(error) =
                        cx.send_after_socket_interests(epoch.feed(), "{\"method\":\"PING\"}".into())
                    {
                        Self::interest_notice(cx, error);
                    }
                } else {
                    return Err(SocketError::Stale);
                }
            }
            LifecycleEvent::InterestsCommitted { feed, proposal }
                if self.public.feed == Some(feed) =>
            {
                if self
                    .public
                    .change
                    .as_ref()
                    .is_some_and(|(id, _)| *id == proposal)
                {
                    let (_, change) = self.public.change.take().unwrap();
                    self.commit_interest_change(change);
                }
                if self.public.refresh == Some(proposal) {
                    self.public.refresh = None;
                    self.public.refresh_needed = false;
                }
                self.public.refresh_needed = false;
                self.cancel_resolved_retry(cx);
                self.arm_snapshots(cx);
            }
            LifecycleEvent::InterestsRefused {
                feed,
                proposal,
                error,
            } if self.public.feed == Some(feed) => {
                if self
                    .public
                    .change
                    .as_ref()
                    .is_some_and(|(id, _)| *id == proposal)
                {
                    // The committed native maps/refcounts were never changed.
                    self.public.change = None;
                } else {
                    if self.public.refresh == Some(proposal) {
                        self.public.refresh = None;
                    }
                    self.public.armed.retain(|_, id| *id != proposal);
                    self.retry_interests(cx, error);
                }
                Self::interest_notice(cx, error);
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.public.feed == Some(epoch.feed())
                    && self.public.socket == Some(epoch.socket())
                {
                    self.public.open = false;
                    self.public.socket = None;
                    self.public.change = None;
                    self.public.refresh = None;
                    self.public.refresh_needed = false;
                    self.public.armed.clear();
                    if let Some((timer, _)) = self.public.retry.take() {
                        cx.clear_timer(timer);
                    }
                    for state in self.books.values_mut() {
                        state.generation += 1;
                        state.sync = Sync::Waiting(vec![]);
                    }
                } else if self.account.socket.feed == Some(epoch.feed())
                    && self.account.socket.socket == Some(epoch.socket())
                {
                    self.account.socket.open = false;
                    self.account.socket.socket = None;
                }
            }
            LifecycleEvent::Ping(epoch) => {
                let connection = if self.public.feed == Some(epoch.feed()) {
                    &mut self.public
                } else if self.account.socket.feed == Some(epoch.feed()) {
                    &mut self.account.socket
                } else {
                    return Err(SocketError::Stale);
                };
                if connection.socket != Some(epoch.socket()) {
                    return Err(SocketError::Stale);
                }
                connection.ping_at = cx.now;
                cx.send(epoch.socket(), "{\"method\":\"PING\"}");
            }
            LifecycleEvent::Reconnect(feed) => {
                if self.public.feed == Some(feed) {
                    self.connect_feed(cx, false)?;
                } else if self.account.socket.feed == Some(feed) {
                    self.connect_feed(cx, true)?;
                } else {
                    return Err(SocketError::Stale);
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn released_socket_snapshots(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        epoch: Epoch,
        pending: Pending,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(epoch.feed()) || self.public.socket != Some(epoch.socket()) {
            return Err(SocketError::Stale);
        }
        let Pending::SocketSnapshot(symbol) = pending else {
            return Err(SocketError::NotPermitted);
        };
        self.public.armed.remove(&symbol);
        if self.public.snapshots.remove(&symbol) && self.books.contains_key(&symbol) {
            self.snapshot(cx, &symbol);
        }
        self.cancel_resolved_retry(cx);
        self.arm_snapshots(cx);
        Ok(())
    }

    pub(super) fn encode_public_interests(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(feed) {
            return Err(SocketError::NotPermitted);
        }
        let topics = self.public_topics(self.public.change.as_ref().map(|(_, change)| change));
        let ranks: HashMap<_, _> = topics
            .iter()
            .enumerate()
            .map(|(n, s)| (s.key.as_ref(), n))
            .collect();
        let mut values: Vec<_> = items.iter().collect();
        values.sort_by_key(|item| ranks.get(item.key.as_ref()).copied().unwrap_or(usize::MAX));
        if values.is_empty() {
            return Ok(());
        }
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            method: &'a str,
            params: Vec<&'a str>,
        }
        let text = serde_json::to_string(&Subscription {
            method: if subscribe {
                "SUBSCRIBE"
            } else {
                "UNSUBSCRIBE"
            },
            params: values.iter().map(|item| item.wire_args.as_ref()).collect(),
        })
        .map_err(|_| SocketError::InvalidInterest)?;
        if text.len() > safe_cex_engine::socket_lifecycle::MAX_FRAME_BYTES {
            return Err(SocketError::Capacity);
        }
        emit(EncodedBatch {
            text,
            covered_keys: values.iter().map(|item| item.key.clone()).collect(),
        })
    }
}
