//! Managed Spot feeds own epochs, reconnect backoff and subscription release.
use super::*;
use safe_cex_engine::{
    SocketChannel, SocketSpec, Store, SubscriptionTopic,
    socket::Config,
    socket_lifecycle::{
        AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedPolicy, Interest, Invalidation,
        LifecycleEvent, RetireReason, SocketError, TrafficPolicy,
    },
};
fn policy(role: SocketChannel) -> FeedPolicy {
    FeedPolicy {
        role,
        supervision: Config {
            idle_timeout_ms: 45_000.,
            ping_interval_ms: Some(15_000.),
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
    }
}
impl BingxSpot {
    pub(super) fn register_feeds(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        self.public.feed = Some(cx.register_socket_feed(policy(SocketChannel::Public))?);
        if self.credentials().is_some() {
            self.private.feed = Some(cx.register_socket_feed(policy(SocketChannel::Private))?);
        }
        Ok(())
    }
    pub(super) fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = self.public.feed.ok_or(SocketError::NotPermitted)?;
        let epoch = cx.connect_socket(feed, SocketSpec::new(WS))?;
        self.public.socket = Some(epoch.socket());
        Ok(())
    }
    pub(super) fn connect_private(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), SocketError> {
        let feed = self.private.feed.ok_or(SocketError::NotPermitted)?;
        let key = self
            .listen_key
            .as_deref()
            .ok_or(SocketError::NotPermitted)?;
        let epoch = cx.connect_socket(
            feed,
            SocketSpec::new(format!(
                "{WS}?listenKey={}",
                safe_cex_engine::url::encode(key)
            )),
        )?;
        self.private.socket = Some(epoch.socket());
        Ok(())
    }
    fn topics(&self, store: &Store) -> Vec<Interest> {
        let mut out = vec![];
        for m in store.markets() {
            if self
                .ticker_symbols
                .as_ref()
                .is_none_or(|v| v.contains(&m.symbol))
            {
                out.push(Interest::from_wire_args(format!("{}@ticker", m.id)));
            }
        }
        for s in self.books.keys() {
            if let Some(m) = store.market(s) {
                out.push(Interest::from_wire_args(format!("{}@depth50", m.id)));
            }
        }
        for (s, i) in self.candles.keys() {
            if let (Some(m), Some(i)) = (store.market(s), wire::interval(*i, true)) {
                out.push(Interest::from_wire_args(format!("{}@kline_{i}", m.id)));
            }
        }
        out
    }
    pub(super) fn sync_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(f) = self.public.feed
            && let Err(e) = cx.set_socket_interests(f, self.topics(cx.store))
        {
            cx.notice(
                Level::Warn,
                "stream.unavailable",
                format!("Spot subscriptions refused: {e:?}"),
            );
        }
    }
    pub(super) fn message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let public = self.public.socket == Some(socket);
        let private = self.private.socket == Some(socket);
        if !public && !private {
            return;
        }
        if text == "Ping" {
            cx.send(socket, "Pong");
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(m) = Message::decode(text, public) else {
            return;
        };
        if m.code != 0 {
            return;
        }
        if private && (m.e == "listenKeyExpired" || m.data_type == "listenKeyExpired") {
            if let Some(f) = self.private.feed {
                cx.retire_socket(f, RetireReason::Closed);
            }
            return;
        }
        if m.data.is_none() && m.ticker.is_none() && m.depth.is_none() {
            return;
        }
        if public {
            self.public_message(cx, m);
        } else {
            self.private_message(cx, m);
        }
    }
    // Keep private report handling out of the public stream's WASM body.
    #[inline(never)]
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        if m.data_type.ends_with("@ticker") {
            if let Some(t) = m.ticker.or_else(|| {
                m.data
                    .and_then(|data| serde_json::from_str::<LiveTicker<'_>>(data.get()).ok())
            }) && t.c > 0.
                && t.bid >= 0.
                && t.ask >= 0.
                && t.v >= 0.
                && t.q >= 0.
                && m.data_type.strip_suffix("@ticker") == Some(t.s.as_ref())
            {
                let s = wire::stream_symbol(&t.s, &mut self.stream_symbol);
                let index = cx.store.ensure_ticker(s);
                if !self.live_tickers.contains(s) {
                    self.live_tickers.insert(s.to_owned());
                }
                if let Some(index) = index {
                    cx.store.update_ticker_at(index, |r| {
                        r.last = t.c;
                        r.bid = t.bid;
                        r.ask = t.ask;
                        r.volume = t.v;
                        r.quote_volume = t.q;
                        r.percentage = t.percentage;
                    });
                }
            }
        } else if let Some(id) = m.data_type.strip_suffix("@depth50") {
            let s = wire::stream_symbol(id, &mut self.stream_symbol);
            if let Some(b) = self.books.get_mut(s)
                && let Some(d) = m.depth.or_else(|| {
                    m.data
                        .and_then(|data| serde_json::from_str::<Depth>(data.get()).ok())
                })
            {
                if !d.valid() {
                    return;
                }
                b.book.clear();
                for (p, q) in d.bids {
                    b.book.apply(BookSide::Bids, p.0, q.0);
                }
                for (p, q) in d.asks {
                    b.book.apply(BookSide::Asks, p.0, q.0);
                }
                cx.book_changed(s);
            }
        } else if let Some((id, native)) = m.data_type.split_once("@kline_")
            && let Some(data) = m.data
            && let Ok(c) = serde_json::from_str::<LiveCandle>(data.get())
        {
            if c.s != id {
                return;
            }
            let s = wire::symbol(id);
            if let Some(i) = self.candles.keys().find_map(|(symbol, i)| {
                (symbol == &s && wire::interval(*i, true) == Some(native)).then_some(*i)
            }) {
                let b = c.k;
                if let Ok(c) = candle(vec![
                    safe_cex_engine::de::Number(b.t),
                    safe_cex_engine::de::Number(b.o),
                    safe_cex_engine::de::Number(b.h),
                    safe_cex_engine::de::Number(b.l),
                    safe_cex_engine::de::Number(b.c),
                    safe_cex_engine::de::Number(b.v),
                ]) {
                    cx.candle(&s, i, c);
                }
            }
        }
    }
    #[inline(never)]
    fn private_message(&mut self, cx: &mut Cx<'_, Pending>, m: Message<'_>) {
        if m.data_type == "spot.executionReport" {
            if let Some(data) = m.data
                && let Ok(r) = serde_json::from_str::<Report>(data.get())
            {
                let Ok(order) = r.order() else {
                    return;
                };
                if cx.store.market(&order.symbol).is_none() {
                    return;
                }
                if self.terminal.contains(&order.id) && order.status == OrderStatus::Open {
                    return;
                }
                let stale = cx.store.order(&order.id).is_some_and(|old| {
                    old.filled > order.filled
                        || old.last_update > order.last_update
                        || old.status != OrderStatus::Open && order.status == OrderStatus::Open
                });
                if stale {
                    return;
                }
                let id = order.id.clone();
                let symbol = order.symbol.clone();
                let terminal = order.status != OrderStatus::Open;
                let side = order.side;
                cx.store.upsert_order(order);
                if terminal {
                    self.remember_terminal(id.clone());
                }
                if let Some(trade) = r.t.as_ref().filter(|t| t.valid())
                    && r.l > 0.
                    && r.last_price > 0.
                {
                    let trade_id = trade.text();
                    let key = format!("{id}:{trade_id}");
                    if self.seen_fills.insert(key.clone()) {
                        self.fill_fifo.push_back(key);
                        while self.fill_fifo.len() > 20_000 {
                            if let Some(key) = self.fill_fifo.pop_front() {
                                self.seen_fills.remove(&key);
                            }
                        }
                        cx.fill_with_financials(
                            Fill {
                                id: Some(trade_id),
                                order_id: id,
                                symbol,
                                side,
                                price: r.last_price,
                                amount: r.l,
                            },
                            FillFinancials {
                                fee: Some(r.n),
                                realized_pnl: None,
                            },
                        );
                    }
                }
                self.balance_revision = self.balance_revision.saturating_add(1);
                self.balances(cx);
            }
        } else if m.data_type == "ACCOUNT_UPDATE" || m.e == "ACCOUNT_UPDATE" {
            // The balance push is a notification; only Spot REST proves free/locked.
            self.balance_revision = self.balance_revision.saturating_add(1);
            self.balances(cx);
        }
    }
    pub(super) fn lifecycle(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Ready { epoch, .. } => {
                let private = self.private.feed == Some(epoch.feed());
                if private {
                    self.private.socket = Some(epoch.socket());
                    cx.set_socket_interests(
                        epoch.feed(),
                        vec![
                            Interest::from_wire_args("spot.executionReport"),
                            Interest::from_wire_args("ACCOUNT_UPDATE"),
                        ],
                    )?;
                    self.balances(cx);
                    self.orders(cx, false);
                } else if self.public.feed == Some(epoch.feed()) {
                    self.public.socket = Some(epoch.socket());
                    cx.set_socket_interests(epoch.feed(), self.topics(cx.store))?;
                } else {
                    return Err(SocketError::Stale);
                }
            }
            LifecycleEvent::Retired { epoch, .. } => {
                if self.private.feed == Some(epoch.feed()) {
                    self.private.socket = None;
                    self.listen_key = None;
                    self.listen_generation = self.listen_generation.saturating_add(1);
                    if let Some(t) = self.keepalive.take() {
                        cx.clear_timer(t);
                    }
                    if let Some(s) = self.session.take() {
                        let _ = cx.retire_session(&s);
                    }
                } else if self.public.feed == Some(epoch.feed()) {
                    self.public.socket = None;
                    for (s, b) in &mut self.books {
                        b.book.clear();
                        cx.book_changed(s);
                    }
                }
            }
            LifecycleEvent::Reconnect(f) => {
                if self.private.feed == Some(f) {
                    self.listen(cx, true);
                } else if self.public.feed == Some(f) {
                    self.connect_public(cx)?;
                } else {
                    return Err(SocketError::Stale);
                }
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "Ping"),
            LifecycleEvent::InterestsRefused { error, .. } => return Err(error),
            _ => {}
        }
        Ok(())
    }
    pub(super) fn encode(
        &self,
        feed: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        if self.public.feed != Some(feed) && self.private.feed != Some(feed) {
            return Err(SocketError::Stale);
        }
        for interest in items {
            let id = self
                .sequence
                .get()
                .checked_add(1)
                .ok_or(SocketError::Capacity)?;
            self.sequence.set(id);
            let text=serde_json::json!({"id":id.to_string(),"reqType":if subscribe{"sub"}else{"unsub"},"dataType":interest.wire_args}).to_string();
            emit(EncodedBatch {
                text,
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
    pub(super) fn interest(
        &self,
        store: &Store,
        topic: &SubscriptionTopic,
    ) -> Option<(FeedId, String)> {
        let feed = self.public.feed?;
        match topic {
            SubscriptionTopic::OrderBook { symbol } => {
                Some((feed, format!("{}@depth50", store.market(symbol)?.id)))
            }
            SubscriptionTopic::Candles { symbol, interval } => Some((
                feed,
                format!(
                    "{}@kline_{}",
                    store.market(symbol)?.id,
                    wire::interval(*interval, true)?
                ),
            )),
        }
    }
}
