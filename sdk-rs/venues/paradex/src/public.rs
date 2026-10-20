use crate::{Paradex, Pending, wire::*};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, Min, MinMax, Precision, Ticker};
use safe_cex_engine::socket::{Supervisor, Tick as SocketTick};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId,
};
use std::collections::HashMap;

pub enum Request {
    Markets {
        rows: Vec<Market>,
        bands: HashMap<String, f64>,
        invalid: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        target: CandleTarget,
        symbol: String,
        interval: String,
        from: Option<f64>,
        to: Option<f64>,
        legacy: bool,
        rows: Vec<Candle>,
        invalid: bool,
    },
}
pub enum CandleTarget {
    Command(CommandId),
    Poll { generation: u64, revision: u64 },
}
#[derive(Default)]
pub(crate) struct Public {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    next_rpc: u64,
    subscriptions: HashMap<u64, Subscription>,
    retries: Vec<(TimerId, String, u8)>,
    seed_retry: Option<(TimerId, bool)>,
    pub(crate) books: HashMap<String, BookState>,
    pub(crate) price_bands: HashMap<String, f64>,
    candles: HashMap<(String, String), CandleState>,
    generation: u64,
}
struct Subscription {
    channel: String,
    attempt: u8,
    at: f64,
}
pub(crate) struct BookState {
    pub(crate) book: Book,
    subscribers: u32,
    received: f64,
    sequence: Option<u64>,
    initialized: bool,
}
struct CandleState {
    subscribers: u32,
    generation: u64,
    last: Option<Candle>,
    timer: Option<TimerId>,
    last_trade: f64,
    last_recovery: f64,
    revision: u64,
}

fn interval_ms(interval: &str) -> Option<f64> {
    Some(match interval {
        "1m" => 60_000.,
        "3m" => 180_000.,
        "5m" => 300_000.,
        "15m" => 900_000.,
        "30m" => 1_800_000.,
        "1h" => 3_600_000.,
        "2h" => 7_200_000.,
        "3h" => 10_800_000.,
        "4h" => 14_400_000.,
        "5h" => 18_000_000.,
        "6h" => 21_600_000.,
        "7h" => 25_200_000.,
        "8h" => 28_800_000.,
        "9h" => 32_400_000.,
        "10h" => 36_000_000.,
        "11h" => 39_600_000.,
        "12h" => 43_200_000.,
        "1d" | "24h" => 86_400_000.,
        "2d" => 172_800_000.,
        "3d" => 259_200_000.,
        "1w" => 604_800_000.,
        "1M" => 2_592_000_000.,
        _ => return None,
    })
}
fn resolution(interval: &str) -> Option<String> {
    interval_ms(interval)?;
    Some(match interval {
        "1d" => "1D".into(),
        "2d" => "2D".into(),
        "3d" => "3D".into(),
        "1w" => "1W".into(),
        "1M" => "1M".into(),
        _ => safe_cex_engine::num::format(interval_ms(interval)? / 60_000.),
    })
}
fn documented(interval: &str) -> bool {
    matches!(interval, "1m" | "3m" | "5m" | "15m" | "30m" | "1h")
}
fn book_channel(symbol: &str) -> String {
    format!("order_book.{symbol}.deltas")
}
fn candle(row: &[Number]) -> Option<Candle> {
    (row.len() >= 6).then(|| Candle {
        timestamp: row[0].0,
        open: row[1].0,
        high: row[2].0,
        low: row[3].0,
        close: row[4].0,
        volume: row[5].0,
    })
}

impl Paradex {
    pub(crate) fn public_start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_seed(cx, true);
    }
    fn public_seed(&self, cx: &mut Cx<'_, Pending>, markets: bool) {
        let (path, request) = if markets {
            (
                "/markets",
                Request::Markets {
                    rows: vec![],
                    bands: HashMap::new(),
                    invalid: false,
                },
            )
        } else {
            (
                "/markets/summary?market=ALL",
                Request::Tickers {
                    rows: vec![],
                    invalid: false,
                },
            )
        };
        let id = cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.rest()),
            vec![],
            ArrayAt::Field("results"),
            Pending::Public(request),
        );
        // Markets and tickers deserialize every row's identity first.
        cx.venue_parses_items(id);
    }
    fn public_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() || self.public.supervisor.is_waiting() {
            return;
        }
        self.public.socket = Some(cx.connect_binary_channel(self.ws(), SocketChannel::Public));
        self.public.supervisor.connecting(cx);
    }
    fn public_subscribe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        channel: String,
        attempt: u8,
    ) {
        let Some(socket) = self.public.socket.filter(|_| self.public.open) else {
            return;
        };
        self.public.next_rpc = self.public.next_rpc.wrapping_add(1);
        let id = self.public.next_rpc;
        #[derive(serde::Serialize)]
        struct Params<'a> {
            channel: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Rpc<'a> {
            id: u64,
            jsonrpc: &'static str,
            method: &'a str,
            params: Params<'a>,
        }
        cx.send(
            socket,
            serde_json::to_string(&Rpc {
                id,
                jsonrpc: "2.0",
                method,
                params: Params { channel: &channel },
            })
            .unwrap(),
        );
        if method == "subscribe" {
            self.public.subscriptions.insert(
                id,
                Subscription {
                    channel,
                    attempt,
                    at: cx.now,
                },
            );
        }
    }
    fn interested(&self, cx: &Cx<'_, Pending>, channel: &str) -> bool {
        if channel == "markets_summary" {
            return !cx.store.markets().is_empty();
        }
        if let Some(s) = channel
            .strip_prefix("order_book.")
            .and_then(|s| s.split('.').next())
        {
            return self.public.books.contains_key(s);
        }
        channel
            .split_once('.')
            .is_some_and(|(_, s)| cx.store.market(s).is_some())
    }
    fn retry_subscription(&mut self, cx: &mut Cx<'_, Pending>, sub: Subscription) {
        if sub.attempt < 2 && self.interested(cx, &sub.channel) {
            self.public.retries.push((
                cx.set_timer(1_000. * f64::from(sub.attempt + 1)),
                sub.channel,
                sub.attempt + 1,
            ));
        } else if self.interested(cx, &sub.channel) {
            self.public_recover(cx);
        }
    }
    pub(crate) fn public_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        // RFC6455 ping/pong is handled by the host; idle SBE heartbeats feed the watchdog.
        let symbols: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for s in &symbols {
            self.public_subscribe(cx, "subscribe", format!("bbo.{s}"), 0);
        }
        for s in &symbols {
            self.public_subscribe(cx, "subscribe", format!("trades.{s}"), 0);
        }
        self.public_subscribe(cx, "subscribe", "markets_summary".into(), 0);
        for s in self.public.books.keys().cloned().collect::<Vec<_>>() {
            self.public_subscribe(cx, "subscribe", book_channel(&s), 0);
        }
    }
    fn public_recover(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.public.socket {
            cx.close(socket);
            self.public_close(cx, socket);
        }
    }
    pub(crate) fn public_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        self.public.subscriptions.clear();
        for (timer, _, _) in self.public.retries.drain(..) {
            cx.clear_timer(timer);
        }
        for state in self.public.books.values_mut() {
            state.book.clear();
            state.sequence = None;
            state.initialized = false;
            state.received = cx.now;
        }
        self.public.supervisor.retry(cx);
    }
    pub(crate) fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) {
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.kind == "pong" || message.kind == "ping" {
            self.public.supervisor.received(cx.now);
            return;
        }
        if message.jsonrpc != "2.0" {
            return;
        }
        if let Some(id) = message.id {
            let valid = self.public.subscriptions.get(&id).is_some_and(|sub| {
                message.error.is_some()
                    || message
                        .result
                        .as_ref()
                        .is_some_and(|r| r.channel == sub.channel)
            });
            if valid && let Some(sub) = self.public.subscriptions.remove(&id) {
                self.public.supervisor.received(cx.now);
                if let Some(error) = message.error {
                    cx.log(
                        LogLevel::Warning,
                        format!("Paradex subscription failed: {}", error.message),
                    );
                    self.retry_subscription(cx, sub);
                }
            }
            return;
        }
        let Some(params) = message.params.filter(|_| message.method == "subscription") else {
            return;
        };
        let (kind, symbol) = params
            .channel
            .split_once('.')
            .map_or((params.channel.as_ref(), ""), |(k, s)| {
                (k, s.split('.').next().unwrap_or(s))
            });
        match kind {
            "bbo" => {
                if let Ok(row) = serde_json::from_str::<Bbo<'_>>(params.data.get()) {
                    self.public_bbo(cx, symbol, &row);
                }
            }
            "markets_summary" => {
                if let Ok(rows) = serde_json::from_str::<Many<Tick<'_>>>(params.data.get()) {
                    for row in rows.rows() {
                        self.public_summary(cx, &row);
                    }
                }
            }
            "trades" => {
                if let Ok(rows) = serde_json::from_str::<Many<Trade<'_>>>(params.data.get()) {
                    for row in rows.rows() {
                        self.public_trade(cx, symbol, &row);
                    }
                }
            }
            "order_book" => {
                if let Ok(rows) = serde_json::from_str::<Many<Depth<'_>>>(params.data.get()) {
                    for row in rows.rows() {
                        self.public_depth(cx, symbol, row);
                    }
                }
            }
            _ => {}
        }
    }
    pub(crate) fn public_binary(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        bytes: &[u8],
    ) {
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        match crate::sbe::public(bytes) {
            Ok(crate::sbe::Public::Trade(row)) => self.public_trade(cx, &row.market, &row),
            Ok(crate::sbe::Public::Bbo(row)) => self.public_bbo(cx, &row.market, &row),
            Ok(crate::sbe::Public::Summary(row)) => self.public_summary(cx, &row),
            Ok(crate::sbe::Public::Book(row)) => {
                let symbol = row.market.to_string();
                self.public_depth(cx, &symbol, row);
            }
            Ok(crate::sbe::Public::Heartbeat) => self.public.supervisor.received(cx.now),
            Ok(crate::sbe::Public::Other) => {}
            Err(error) => {
                cx.log(LogLevel::Warning, error);
            }
        }
    }
    fn public_bbo(&mut self, cx: &mut Cx<'_, Pending>, fallback: &str, row: &Bbo<'_>) {
        let symbol = if row.market.is_empty() {
            fallback
        } else {
            &row.market
        };
        self.public.supervisor.received(cx.now);
        cx.store.update_ticker(symbol, |t| {
            if row.bid_price.0 > 0. {
                t.bid = row.bid_price.0;
            }
            if row.ask_price.0 > 0. {
                t.ask = row.ask_price.0;
            }
            if row.index_price.0 > 0. {
                t.index = row.index_price.0;
            }
        });
    }
    fn public_summary(&mut self, cx: &mut Cx<'_, Pending>, row: &Tick<'_>) {
        self.public.supervisor.received(cx.now);
        cx.store.update_ticker(&row.symbol, |t| {
            if row.bid.0 > 0. {
                t.bid = row.bid.0;
            }
            if row.ask.0 > 0. {
                t.ask = row.ask.0;
            }
            if row.last_traded_price.0 > 0. {
                t.last = row.last_traded_price.0;
            }
            if row.mark_price.0 > 0. {
                t.mark = row.mark_price.0;
            }
            let index = if row.oracle_price.0 > 0. {
                row.oracle_price.0
            } else if row.index_price.0 > 0. {
                row.index_price.0
            } else {
                row.underlying_price.0
            };
            if index > 0. {
                t.index = index;
            }
            t.percentage = row.price_change_rate_24h.0 * 100.;
            t.volume = row.volume_24h.0;
            t.open_interest = row.open_interest.0;
            t.funding_rate = row.funding_rate.0;
        });
    }
    fn public_trade(&mut self, cx: &mut Cx<'_, Pending>, fallback: &str, row: &Trade<'_>) {
        let symbol = if row.market.is_empty() {
            fallback
        } else {
            &row.market
        };
        if row.price.0 <= 0. || row.size.0 < 0. {
            return;
        }
        self.public.supervisor.received(cx.now);
        cx.store.update_ticker(symbol, |t| {
            t.last = row.price.0;
            t.mark = row.price.0;
        });
        self.trade_candles(
            cx,
            symbol,
            row.price.0,
            row.size.0,
            if row.created_at.0 > 0. {
                row.created_at.0
            } else {
                cx.now
            },
        );
    }
    fn public_depth(&mut self, cx: &mut Cx<'_, Pending>, fallback: &str, row: Depth<'_>) {
        let symbol = if row.market.is_empty() {
            fallback
        } else {
            &row.market
        };
        if symbol != fallback {
            return;
        }
        let sequence = row.seq_no;
        let snapshot = row.update_type == "s";
        if !snapshot && row.update_type != "d" {
            return;
        }
        if row
            .inserts
            .iter()
            .chain(&row.updates)
            .chain(&row.deletes)
            .any(|level| {
                !matches!(level.side.as_ref(), "BUY" | "SELL")
                    || level.price.0 <= 0.
                    || level.size.0 < 0.
            })
        {
            return;
        }
        let Some(state) = self.public.books.get_mut(symbol) else {
            return;
        };
        if state.sequence.is_some_and(|last| {
            sequence < last || (sequence == last && (!snapshot || state.initialized))
        }) {
            return;
        }
        if !snapshot
            && (!state.initialized
                || state.sequence.and_then(|last| last.checked_add(1)) != Some(sequence))
        {
            let recover = state.initialized;
            state.initialized = false;
            state.book.clear();
            if recover {
                self.public_book_resync(cx, symbol);
            }
            return;
        }
        if snapshot {
            state.book.clear();
        }
        for level in row.inserts.into_iter().chain(row.updates) {
            let side = if level.side == "BUY" {
                BookSide::Bids
            } else {
                BookSide::Asks
            };
            state.book.apply(side, level.price.0, level.size.0);
        }
        for level in row.deletes {
            let side = if level.side == "BUY" {
                BookSide::Bids
            } else {
                BookSide::Asks
            };
            state.book.apply(side, level.price.0, 0.);
        }
        state.sequence = Some(sequence);
        state.initialized = true;
        state.received = cx.now;
        self.public.supervisor.received(cx.now);
        if let (Some(bid), Some(ask)) = (state.book.best_bid(), state.book.best_ask()) {
            cx.store.update_ticker(symbol, |t| {
                t.bid = bid;
                t.ask = ask;
            });
        }
        cx.book_changed(symbol);
    }
    fn public_book_resync(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let channel = book_channel(symbol);
        self.public
            .subscriptions
            .retain(|_, sub| sub.channel != channel);
        if let Some(state) = self.public.books.get_mut(symbol) {
            state.initialized = false;
            state.book.clear();
            state.received = cx.now;
        }
        self.public_subscribe(cx, "unsubscribe", channel.clone(), 0);
        self.public_subscribe(cx, "subscribe", channel, 0);
    }
    fn trade_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        price: f64,
        size: f64,
        timestamp: f64,
    ) {
        for ((s, interval), state) in &mut self.public.candles {
            if s != symbol {
                continue;
            }
            let step = interval_ms(interval).unwrap();
            let timestamp = (timestamp / step).floor() * step;
            if state.last.is_some_and(|c| c.timestamp > timestamp) {
                continue;
            }
            let mut c = state
                .last
                .filter(|c| c.timestamp == timestamp)
                .unwrap_or(Candle {
                    timestamp,
                    open: price,
                    high: price,
                    low: price,
                    close: price,
                    volume: 0.,
                });
            c.high = c.high.max(price);
            c.low = c.low.min(price);
            c.close = price;
            c.volume += size.max(0.);
            state.revision = state.revision.wrapping_add(1);
            state.last_trade = cx.now;
            if state.last != Some(c) {
                state.last = Some(c);
                cx.candle(s, interval, c);
            }
        }
    }
    pub(crate) fn public_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Markets {
                rows,
                bands,
                invalid,
            } => {
                let Ok(identity) = serde_json::from_str::<InstrumentIdentity<'_>>(item) else {
                    *invalid = true;
                    return;
                };
                let kind = identity.asset_kind.to_ascii_uppercase();
                let option = kind.contains("OPTION")
                    || identity.market_kind.to_ascii_uppercase().contains("OPTION")
                    || identity.market.ends_with("-C")
                    || identity.market.ends_with("-P");
                if identity.market.is_empty()
                    || option
                    || (!identity.status.is_empty() && identity.status != "ACTIVE")
                    || !(kind.is_empty() || kind == "PERP" || identity.market.ends_with("-PERP"))
                {
                    return;
                }
                let Ok(m) = serde_json::from_str::<Instrument<'_>>(item) else {
                    *invalid = true;
                    return;
                };
                let imf = if m.delta1_cross_margin_params.imf_base.0 > 0. {
                    m.delta1_cross_margin_params.imf_base.0
                } else {
                    m.initial_margin_fraction.0
                };
                let band = [m.price_bands_width.0, m.max_slippage.0]
                    .into_iter()
                    .find(|n| *n > 0.);
                if let Some(band) = band {
                    bands.insert(
                        m.market.to_string(),
                        if band > 1. { band / 100. } else { band },
                    );
                }
                rows.push(Market {
                    id: m.market.to_string(),
                    symbol: m.market.into_owned(),
                    pretty_symbol: None,
                    base: m.base_currency.into_owned(),
                    quote: m.quote_currency.into_owned(),
                    active: true,
                    precision: Precision {
                        amount: m.step_size.0,
                        price: m.tick_size.0,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: m.min_order_size.0,
                            max: m.max_order_size.0,
                        },
                        leverage: MinMax {
                            min: 1.,
                            max: if imf > 0. { (1. / imf).floor() } else { 100. },
                        },
                        price: Some(Min { min: 0. }),
                    },
                    contract_size: Some(1.),
                    is_expiring: None,
                    is_inverse: None,
                    is_spot: None,
                    is_perpetual: None,
                    is_read_only: None,
                    taker_fee: Some(m.taker_fee.0),
                    maker_fee: Some(m.maker_fee.0),
                });
            }
            Request::Tickers { rows, invalid } => {
                let Ok(identity) = serde_json::from_str::<MarketIdentity<'_>>(item) else {
                    *invalid = true;
                    return;
                };
                if !identity.symbol.ends_with("-PERP")
                    || cx.store.market(&identity.symbol).is_none()
                {
                    return;
                }
                match serde_json::from_str::<Tick<'_>>(item) {
                    Ok(t) => {
                        if t.symbol.ends_with("-PERP") && cx.store.market(&t.symbol).is_some() {
                            rows.push(Ticker {
                                id: t.symbol.to_string(),
                                symbol: t.symbol.into_owned(),
                                bid: t.bid.0,
                                ask: t.ask.0,
                                last: t.last_traded_price.0,
                                mark: t.mark_price.0,
                                index: if t.underlying_price.0 != 0. {
                                    t.underlying_price.0
                                } else {
                                    t.mark_price.0
                                },
                                percentage: t.price_change_rate_24h.0 * 100.,
                                volume: t.volume_24h.0,
                                quote_volume: t.volume_24h.0,
                                open_interest: t.open_interest.0,
                                funding_rate: t.funding_rate.0,
                                ..Ticker::default()
                            });
                        }
                    }
                    Err(_) => *invalid = true,
                }
            }
            Request::Candles { rows, invalid, .. } => {
                match serde_json::from_str::<Vec<Number>>(item)
                    .ok()
                    .and_then(|r| candle(&r))
                {
                    Some(c) => rows.push(c),
                    None => *invalid = true,
                }
            }
        }
    }
    pub(crate) fn public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if let Err(e) = check(&response) {
            self.public_failed(cx, request, &e);
            return;
        }
        match request {
            Request::Markets {
                rows,
                bands,
                invalid: false,
            } => {
                self.public.price_bands = bands;
                cx.store.set_markets(rows);
                self.public
                    .books
                    .retain(|symbol, _| cx.store.market(symbol).is_some());
                self.public
                    .candles
                    .retain(|(symbol, _), _| cx.store.market(symbol).is_some());
                let queued: Vec<_> = self
                    .public
                    .candles
                    .iter()
                    .map(|((symbol, interval), state)| {
                        (
                            symbol.clone(),
                            interval.clone(),
                            state.generation,
                            state.revision,
                        )
                    })
                    .collect();
                for (symbol, interval, generation, revision) in queued {
                    let step = interval_ms(&interval).expect("validated interval");
                    self.fetch_candles(
                        cx,
                        CandleTarget::Poll {
                            generation,
                            revision,
                        },
                        symbol,
                        interval,
                        (Some(cx.now - step * 3.), Some(cx.now)),
                        false,
                    );
                }
                self.public_seed(cx, false);
            }
            Request::Tickers {
                rows,
                invalid: false,
            } => {
                cx.store.set_tickers(rows);
                self.public_connect(cx);
                if self.options.public_data_only
                    || self.options.key.as_ref().is_none_or(String::is_empty)
                {
                    cx.store.set_orders(vec![]);
                    cx.store.set_positions(vec![]);
                    cx.store.set_balance(Default::default());
                }
            }
            Request::Candles {
                target,
                symbol,
                interval,
                from,
                to,
                legacy,
                mut rows,
                invalid: false,
            } => {
                if legacy {
                    match serde_json::from_str::<TradingView<'_>>(response.body) {
                        Ok(v) if v.s == "no_data" => {}
                        Ok(v)
                            if v.s == "ok"
                                && [&v.o, &v.h, &v.l, &v.c, &v.v]
                                    .iter()
                                    .all(|a| a.len() == v.t.len()) =>
                        {
                            for i in 0..v.t.len() {
                                rows.push(Candle {
                                    timestamp: v.t[i].0 * 1000.,
                                    open: v.o[i].0,
                                    high: v.h[i].0,
                                    low: v.l[i].0,
                                    close: v.c[i].0,
                                    volume: v.v[i].0,
                                });
                            }
                        }
                        _ => {
                            self.public_failed(
                                cx,
                                Request::Candles {
                                    target,
                                    symbol,
                                    interval,
                                    from,
                                    to,
                                    legacy,
                                    rows,
                                    invalid: true,
                                },
                                "Malformed Paradex candles",
                            );
                            return;
                        }
                    }
                }
                self.finish_candles(cx, target, &symbol, &interval, rows);
            }
            other => self.public_failed(cx, other, "Malformed Paradex response"),
        }
    }
    pub(crate) fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Markets { .. } | Request::Tickers { .. } => {
                let markets = matches!(request, Request::Markets { .. });
                cx.log(LogLevel::Warning, error);
                if self.public.seed_retry.is_none() {
                    self.public.seed_retry = Some((cx.set_timer(1_000.), markets));
                }
            }
            Request::Candles {
                target,
                symbol,
                interval,
                from,
                to,
                legacy: false,
                ..
            } => self.fetch_candles(cx, target, symbol, interval, (from, to), true),
            Request::Candles {
                target,
                symbol,
                interval,
                ..
            } => {
                if let CandleTarget::Command(command) = target {
                    cx.done(command, Err(error.into()));
                } else {
                    cx.log(LogLevel::Warning, error);
                    self.finish_candles(cx, target, &symbol, &interval, vec![]);
                }
            }
        }
    }
    fn fetch_candles(
        &self,
        cx: &mut Cx<'_, Pending>,
        target: CandleTarget,
        symbol: String,
        interval: String,
        bounds: (Option<f64>, Option<f64>),
        legacy: bool,
    ) {
        let (from, to) = bounds;
        let legacy = legacy || !documented(&interval);
        let resolution = resolution(&interval).unwrap();
        let path = if legacy {
            let mut p = format!("/tradingview/history?symbol={symbol}&resolution={resolution}");
            if let Some(from) = from {
                p.push_str(&format!(
                    "&from={}",
                    safe_cex_engine::num::format((from / 1000.).floor())
                ));
            }
            if let Some(to) = to {
                p.push_str(&format!(
                    "&to={}",
                    safe_cex_engine::num::format((to / 1000.).floor())
                ));
            }
            p.push_str("&countback=2000&price_kind=last");
            p
        } else {
            let end = to.unwrap_or(cx.now);
            let start = from.unwrap_or(end - interval_ms(&interval).unwrap() * 2000.);
            format!(
                "/markets/klines?symbol={symbol}&resolution={resolution}&start_at={}&end_at={}&price_kind=last",
                safe_cex_engine::num::format(start),
                safe_cex_engine::num::format(end)
            )
        };
        let request = Pending::Public(Request::Candles {
            target,
            symbol,
            interval,
            from,
            to,
            legacy,
            rows: vec![],
            invalid: false,
        });
        if legacy {
            cx.request(
                Method::Get,
                format!("{}{path}", self.rest()),
                vec![],
                None,
                request,
            );
        } else {
            cx.request_streamed(
                Method::Get,
                format!("{}{path}", self.rest()),
                vec![],
                ArrayAt::Field("results"),
                request,
            );
        }
    }
    fn finish_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        target: CandleTarget,
        symbol: &str,
        interval: &str,
        rows: Vec<Candle>,
    ) {
        match target {
            CandleTarget::Command(command) => {
                cx.done(command, Ok(serde_json::to_string(&rows).unwrap()))
            }
            CandleTarget::Poll {
                generation,
                revision,
            } => {
                let Some(state) = self
                    .public
                    .candles
                    .get_mut(&(symbol.into(), interval.into()))
                    .filter(|s| s.generation == generation)
                else {
                    return;
                };
                if state.revision == revision
                    && let Some(c) = rows.last().copied()
                    && state.last.is_none_or(|old| {
                        c.timestamp > old.timestamp
                            || (c.timestamp == old.timestamp && c.volume >= old.volume)
                    })
                    && state.last.is_none_or(|old| {
                        old.timestamp != c.timestamp
                            || old.close != c.close
                            || old.volume != c.volume
                    })
                {
                    state.last = Some(c);
                    cx.candle(symbol, interval, c);
                }
                state.timer = Some(cx.set_timer(3_000.));
                if cx.now - state.last_trade > 20_000. && cx.now - state.last_recovery >= 30_000. {
                    state.last_recovery = cx.now;
                    self.public_subscribe(cx, "subscribe", format!("trades.{symbol}"), 0);
                }
            }
        }
    }
    pub(crate) fn public_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.public.seed_retry.is_some_and(|(t, _)| t == timer) {
            let (_, markets) = self.public.seed_retry.take().unwrap();
            self.public_seed(cx, markets);
            return true;
        }
        if let Some(i) = self.public.retries.iter().position(|(t, _, _)| *t == timer) {
            let (_, channel, attempt) = self.public.retries.swap_remove(i);
            if self.interested(cx, &channel) {
                self.public_subscribe(cx, "subscribe", channel, attempt);
            }
            return true;
        }
        if self.public.supervisor.owns_timer(timer) {
            match self.public.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => {
                    self.public_connect(cx);
                    return true;
                }
                SocketTick::TimedOut(_) => {
                    self.public_recover(cx);
                    return true;
                }
                _ => {}
            }
            let expired: Vec<_> = self
                .public
                .subscriptions
                .iter()
                .filter(|(_, s)| cx.now - s.at >= 10_000.)
                .map(|(id, _)| *id)
                .collect();
            for id in expired {
                if let Some(sub) = self.public.subscriptions.remove(&id) {
                    self.retry_subscription(cx, sub);
                }
            }
            if self.public.socket.is_none() {
                return true;
            }
            let stale: Vec<_> = self
                .public
                .books
                .iter()
                .filter(|(_, s)| cx.now - s.received >= 20_000.)
                .map(|(s, _)| s.clone())
                .collect();
            for symbol in stale {
                if let Some(state) = self.public.books.get_mut(&symbol) {
                    state.received = cx.now;
                }
                self.public_book_resync(cx, &symbol);
            }
            return true;
        }
        if let Some((symbol, interval, generation, revision)) =
            self.public.candles.iter_mut().find_map(|((s, i), state)| {
                if state.timer == Some(timer) {
                    state.timer = None;
                    Some((s.clone(), i.clone(), state.generation, state.revision))
                } else {
                    None
                }
            })
        {
            let step = interval_ms(&interval).unwrap();
            self.fetch_candles(
                cx,
                CandleTarget::Poll {
                    generation,
                    revision,
                },
                symbol,
                interval,
                (Some(cx.now - step * 3.), Some(cx.now)),
                false,
            );
            return true;
        }
        false
    }
    pub(crate) fn public_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && cx.store.market(&symbol).is_none() {
                    cx.log(LogLevel::Warning, "Unknown Paradex market");
                    return None;
                }
                let state = self
                    .public
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| BookState {
                        book: Book::default(),
                        subscribers: 0,
                        received: cx.now,
                        sequence: None,
                        initialized: false,
                    });
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.public_subscribe(cx, "subscribe", book_channel(&symbol), 0);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.public.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.public.books.remove(&symbol);
                        self.public_subscribe(cx, "unsubscribe", book_channel(&symbol), 0);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if interval_ms(&interval).is_none()
                    || (cx.store.loaded().markets && cx.store.market(&symbol).is_none())
                {
                    cx.log(LogLevel::Warning, "Unsupported Paradex candle subscription");
                    return None;
                }
                if let Some(state) = self
                    .public
                    .candles
                    .get_mut(&(symbol.clone(), interval.clone()))
                {
                    state.subscribers += 1;
                    return None;
                }
                self.public.generation += 1;
                let generation = self.public.generation;
                let revision = 0;
                self.public.candles.insert(
                    (symbol.clone(), interval.clone()),
                    CandleState {
                        subscribers: 1,
                        generation,
                        last: None,
                        timer: None,
                        last_trade: 0.,
                        last_recovery: 0.,
                        revision: 0,
                    },
                );
                if !cx.store.loaded().markets {
                    return None;
                }
                let step = interval_ms(&interval).unwrap();
                self.fetch_candles(
                    cx,
                    CandleTarget::Poll {
                        generation,
                        revision,
                    },
                    symbol,
                    interval,
                    (Some(cx.now - step * 3.), Some(cx.now)),
                    false,
                );
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(state) = self.public.candles.get_mut(&key) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        if let Some(t) = state.timer.take() {
                            cx.clear_timer(t);
                        }
                        self.public.candles.remove(&key);
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                from_ms,
                to_ms,
                ..
            } => {
                if interval_ms(&interval).is_none() || cx.store.market(&symbol).is_none() {
                    cx.done(
                        command,
                        Err("Unsupported Paradex candle market/interval".into()),
                    );
                } else {
                    self.fetch_candles(
                        cx,
                        CandleTarget::Command(command),
                        symbol,
                        interval,
                        (from_ms, to_ms),
                        false,
                    );
                }
            }
            other => return Some(other),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::{Action, Engine, Event};
    #[test]
    fn price_bands_are_normalized_and_only_committed_with_a_valid_market_snapshot() {
        let mut e = Engine::new(Paradex::new(crate::Options {
            public_data_only: true,
            ..Default::default()
        }));
        e.start(0.);
        let id = e
            .drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r.id)
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(0.,Event::HttpResponse{request:id,response:Response{status:200,headers:"",body:r#"{"results":[{"symbol":"BTC-USD-PERP","asset_kind":"PERP","price_bands_width":"5","max_slippage":"0.01"},{"symbol":"ETH-USD-PERP","asset_kind":"PERP","max_slippage":"0.03"},{"symbol":"SOL-USD-PERP","asset_kind":"PERP","price_bands_width":"0.07"}]}"#}});
        assert_eq!(
            e.venue().public.price_bands.get("BTC-USD-PERP"),
            Some(&0.05)
        );
        assert_eq!(
            e.venue().public.price_bands.get("ETH-USD-PERP"),
            Some(&0.03)
        );
        assert_eq!(
            e.venue().public.price_bands.get("SOL-USD-PERP"),
            Some(&0.07)
        );
        let mut failed = Engine::new(Paradex::new(crate::Options {
            public_data_only: true,
            ..Default::default()
        }));
        failed.start(0.);
        let id = failed
            .drain_actions()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r.id)
                } else {
                    None
                }
            })
            .unwrap();
        failed.handle(0.,Event::HttpResponse{request:id,response:Response{status:200,headers:"",body:r#"{"results":[{"symbol":"BTC-USD-PERP","asset_kind":"PERP","price_bands_width":"5"},{"symbol":"ETH-USD-PERP","step_size":"invalid"}]}"#}});
        assert!(!failed.store().loaded().markets);
        assert!(failed.venue().public.price_bands.is_empty());
    }
}
