//! Market discovery and one quote socket with reference-counted consumers.
use crate::{ApexOmni, Pending, public_wire as wire};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, PositionSide, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Response, SocketChannel, SocketId, TimerId,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ContractConfig {
    pub pair_id: u16,
    pub initial_margin_rate: f64,
    pub default_initial_margin_rate: f64,
    pub max_market_price_range: f64,
}
pub enum Request {
    Markets {
        rows: Vec<Market>,
        contracts: HashMap<String, ContractConfig>,
        bad: bool,
    },
    Candles {
        command: CommandId,
        symbol: String,
    },
}
impl Request {
    pub(crate) fn reset(&mut self) {
        if let Self::Markets {
            rows,
            contracts,
            bad,
        } = self
        {
            rows.clear();
            contracts.clear();
            *bad = false;
        }
    }
}
pub struct Depth {
    pub book: Book,
    sequence: Option<u64>,
    count: u32,
}
#[derive(Default)]
struct CandleState {
    count: u32,
    last: Option<Candle>,
}
#[derive(Default)]
pub struct Public {
    pub books: HashMap<String, Depth>,
    candles: HashMap<(String, String), CandleState>,
    socket: Option<SocketId>,
    open: bool,
    received: f64,
    ping_at: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    seed_retry: safe_cex_engine::startup::Backoff<()>,
    attempts: u32,
}
impl Public {
    pub fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|d| &d.book)
    }
    pub fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, d)| (s.as_str(), &d.book)))
    }
}
pub(crate) fn interval(interval: &str) -> Option<(&'static str, f64)> {
    Some(match interval {
        "1m" => ("1", 60.0),
        "3m" => ("3", 180.0),
        "5m" => ("5", 300.0),
        "15m" => ("15", 900.0),
        "30m" => ("30", 1800.0),
        "1h" => ("60", 3600.0),
        "2h" => ("120", 7200.0),
        "4h" => ("240", 14400.0),
        "6h" => ("360", 21600.0),
        "12h" => ("720", 43200.0),
        "1d" => ("D", 86400.0),
        "1w" => ("W", 604800.0),
        _ => return None,
    })
}
fn valid_candle(c: &Candle) -> bool {
    c.timestamp >= 0.0
        && c.open > 0.0
        && c.close > 0.0
        && c.low > 0.0
        && c.high >= c.low
        && c.volume >= 0.0
}
impl ApexOmni {
    fn revalue_positions(cx: &mut Cx<'_, Pending>, symbol: &str, reference: f64) {
        if !cx
            .store
            .positions()
            .iter()
            .any(|p| p.symbol == symbol && p.contracts != 0.0)
        {
            return;
        }
        cx.store.update_positions(|p| {
            if p.symbol == symbol && p.contracts != 0.0 {
                p.unrealized_pnl = (reference - p.entry_price)
                    * p.contracts
                    * if p.side == PositionSide::Long {
                        1.0
                    } else {
                        -1.0
                    };
            }
        });
    }

    pub(crate) fn revalue_public_positions(&self, cx: &mut Cx<'_, Pending>) {
        // Shared quotes cannot establish a private snapshot or mark it loaded.
        if !cx.store.loaded().positions {
            return;
        }
        let references: Vec<_> = cx
            .store
            .tickers()
            .iter()
            .filter_map(|ticker| {
                // The direct mapper already folds an absent mark into index.
                // An explicit zero mark falls back to last, even with a live index.
                [ticker.mark, ticker.last]
                    .into_iter()
                    .find(|price| *price != 0.0)
                    .map(|price| (ticker.symbol.clone(), price))
            })
            .collect();
        for (symbol, reference) in references {
            Self::revalue_positions(cx, &symbol, reference);
        }
    }

    pub(crate) fn start_public(&mut self, cx: &mut Cx<'_, Pending>) {
        let request = Pending::Public(Request::Markets {
            rows: vec![],
            contracts: HashMap::new(),
            bad: false,
        });
        if let Err(error) = self.signed(
            cx,
            crate::sign::Request::get("/v3/symbols", vec![]),
            request,
            Some(ArrayAt::Path(&[
                "data",
                "contractConfig",
                "perpetualContract",
            ])),
        ) {
            cx.log(LogLevel::Error, error);
            self.public.seed_retry.failed(cx, ());
        }
    }
    pub(crate) fn public_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if let Request::Markets {
            rows,
            contracts,
            bad: _,
        } = request
        {
            let Ok(raw) = serde_json::from_str::<wire::Contract<'_>>(item) else {
                safe_cex_engine::startup::skipped_row(cx);
                return;
            };
            if !raw.enable_trade {
                return;
            }
            if raw.symbol.is_empty()
                || raw.cross_symbol_name.is_empty()
                || raw.base_token_id.is_empty()
                || raw.settle_asset_id.is_empty()
                || raw.step_size <= 0.0
                || raw.tick_size <= 0.0
                || raw.min_order_size < 0.0
                || raw.max_order_size < raw.min_order_size
                || raw.display_min_leverage <= 0.0
                || raw.display_max_leverage < raw.display_min_leverage
                || raw.l2_pair_id < 0.0
                || raw.l2_pair_id > u16::MAX as f64
                || raw.l2_pair_id.fract() != 0.0
            {
                safe_cex_engine::startup::skipped_row(cx);
                return;
            }
            contracts.insert(
                raw.cross_symbol_name.to_string(),
                ContractConfig {
                    pair_id: raw.l2_pair_id as u16,
                    initial_margin_rate: raw.initial_margin_rate,
                    default_initial_margin_rate: raw.default_initial_margin_rate,
                    max_market_price_range: raw.max_market_price_range,
                },
            );
            rows.push(Market {
                id: raw.symbol.into_owned(),
                symbol: raw.cross_symbol_name.into_owned(),
                base: raw.base_token_id.into_owned(),
                quote: raw.settle_asset_id.into_owned(),
                active: true,
                pretty_symbol: None,
                precision: Precision {
                    amount: raw.step_size,
                    price: raw.tick_size,
                },
                limits: Limits {
                    amount: MinMax {
                        min: raw.min_order_size,
                        max: raw.max_order_size,
                    },
                    leverage: MinMax {
                        min: raw.display_min_leverage,
                        max: raw.display_max_leverage,
                    },
                    price: None,
                },
                contract_size: None,
                is_expiring: Some(false),
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            });
        }
    }
    pub(crate) fn public_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let envelope = serde_json::from_str::<wire::Envelope<'_>>(response.body).ok();
        let success = (200..300).contains(&response.status)
            && envelope
                .as_ref()
                .is_some_and(|e| e.code.unwrap_or(0.0) == 0.0 && e.data.is_some());
        match request {
            Request::Markets {
                rows,
                contracts,
                bad,
            } => {
                if !success || bad || rows.is_empty() {
                    self.public_failed(
                        cx,
                        Request::Markets {
                            rows,
                            contracts,
                            bad,
                        },
                        "invalid market response",
                    );
                    return;
                }
                let tickers = rows
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Default::default()
                    })
                    .collect();
                self.public.seed_retry.succeeded(cx, &());
                self.contracts = contracts;
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect_public(cx);
                self.start_private(cx);
            }
            Request::Candles { command, symbol } => {
                if !success {
                    cx.done(command, Err("ApeX candle request failed".into()));
                    return;
                }
                let rows = envelope
                    .and_then(|e| e.data)
                    .and_then(|data| {
                        serde_json::from_str::<HashMap<String, Vec<wire::Kline>>>(data.get()).ok()
                    })
                    .and_then(|mut rows| rows.remove(&symbol));
                let Some(rows) = rows else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                let mut rows: Vec<_> = rows
                    .into_iter()
                    .map(|r| Candle {
                        timestamp: r.t / 1000.0,
                        open: r.o,
                        high: r.h,
                        low: r.l,
                        close: r.c,
                        volume: r.v,
                    })
                    .collect();
                if !rows.iter().all(valid_candle) {
                    cx.done(command, Err("invalid ApeX candles".into()));
                    return;
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    serde_json::to_string(&rows).map_err(|e| e.to_string()),
                );
            }
        }
    }
    pub(crate) fn public_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Markets { .. } => {
                cx.log(LogLevel::Error, format!("ApeX markets: {error}"));
                self.public.seed_retry.failed(cx, ());
            }
            Request::Candles { command, .. } => cx.done(command, Err(error.to_owned())),
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        let host = if self.options.testnet {
            "qa-quote.omni.apex.exchange"
        } else {
            "quote.omni.apex.exchange"
        };
        self.public.socket = Some(cx.connect_channel(
            format!(
                "wss://{host}/realtime_public?v=2&timestamp={}&requestId=rust-{}",
                cx.now as u64, cx.now as u64
            ),
            SocketChannel::Public,
        ));
        self.public.received = cx.now;
        self.public.watchdog = Some(cx.set_timer(10_000.0));
    }
    fn public_topics(&self) -> Vec<String> {
        let mut topics = vec!["instrumentInfo.all".to_owned()];
        topics.extend(
            self.public
                .books
                .keys()
                .map(|s| format!("orderBook200.H.{s}")),
        );
        let mut trades = HashSet::new();
        for (s, i) in self.public.candles.keys() {
            if let Some((i, _)) = interval(i) {
                topics.push(format!("candle.{i}.{s}"));
            }
            if !self.options.public_data_only {
                trades.insert(format!("recentlyTrade.H.{s}"));
            }
        }
        topics.extend(trades);
        topics.sort();
        topics
    }
    fn public_subscribe(&self, cx: &mut Cx<'_, Pending>, op: &str, args: &[String]) {
        if !self.public.open {
            return;
        }
        let Some(socket) = self.public.socket else {
            return;
        };
        for args in args.chunks(50) {
            cx.send(
                socket,
                serde_json::to_string(&Subscription { op, args }).expect("string subscription"),
            );
        }
    }
    pub(crate) fn public_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.open = true;
        self.public.received = cx.now;
        self.public.attempts = 0;
        self.public_subscribe(cx, "subscribe", &self.public_topics());
        cx.send(socket, r#"{"op":"ping"}"#);
        self.public.ping_at = cx.now;
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.public.watchdog = Some(cx.set_timer(10_000.0));
        true
    }
    pub(crate) fn public_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.socket = None;
        self.public.open = false;
        if let Some(t) = self.public.watchdog.take() {
            cx.clear_timer(t);
        }
        for (symbol, depth) in &mut self.public.books {
            if !depth.book.is_empty() {
                depth.book.clear();
                cx.book_changed(symbol);
            }
            depth.sequence = None;
        }
        if self.public.reconnect.is_none() {
            self.public.attempts = self.public.attempts.saturating_add(1);
            self.public.reconnect = Some(cx.set_timer(
                (1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.0),
            ));
        }
        true
    }
    pub(crate) fn public_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.public.seed_retry.on_timer(timer).is_some() {
            self.start_public(cx);
            return true;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect_public(cx);
            return true;
        }
        if self.public.watchdog != Some(timer) {
            return false;
        }
        self.public.watchdog = None;
        if let Some(socket) = self.public.socket {
            if cx.now - self.public.received >= 30_000.0 {
                cx.close(socket);
                self.public_close(cx, socket);
            } else {
                if self.public.open {
                    cx.send(socket, r#"{"op":"ping"}"#);
                    self.public.ping_at = cx.now;
                }
                self.public.watchdog = Some(cx.set_timer(10_000.0));
            }
        }
        true
    }
    pub(crate) fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.public.socket != Some(socket) {
            return false;
        }
        self.public.received = cx.now;
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return true;
        };
        if message.op == "ping" {
            cx.send(
                socket,
                format!(r#"{{"op":"pong","args":["{}"]}}"#, cx.now as u64),
            );
            return true;
        }
        if message.op == "pong" || (message.topic.is_empty() && text.contains("pong")) {
            cx.store
                .set_latency(((cx.now - self.public.ping_at).max(0.0) / 2.0).round());
            return true;
        }
        let Some(data) = message.data else {
            return true;
        };
        if message.topic == "instrumentInfo.all" {
            if let Ok(rows) = serde_json::from_str::<Vec<wire::Tick<'_>>>(data.get()) {
                for row in rows {
                    let symbol = cx
                        .store
                        .market(&row.s)
                        .map(|m| m.symbol.clone())
                        .or_else(|| cx.store.market_by_id(&row.s).map(|m| m.symbol.clone()));
                    let Some(symbol) = symbol else {
                        continue;
                    };
                    cx.store.update_ticker(&symbol, |t| {
                        if let Some(n) = row.p {
                            t.last = n;
                        }
                        if let Some(n) = row.mp.or(row.xp) {
                            t.mark = n;
                        }
                        if let Some(n) = row.xp {
                            t.index = n;
                        }
                        if let Some(n) = row.pr {
                            t.percentage = n * 100.0;
                        }
                        if let Some(n) = row.o {
                            t.open_interest = n;
                        }
                        if let Some(n) = row.fr {
                            t.funding_rate = n;
                        }
                        if let Some(n) = row.v {
                            t.volume = n;
                        }
                        if let Some(n) = row.turnover {
                            t.quote_volume = n;
                        }
                    });
                    if let Some(reference) = row
                        .mp
                        .or(row.xp)
                        .filter(|n| *n != 0.0)
                        .or(row.p)
                        .filter(|n| *n != 0.0)
                    {
                        Self::revalue_positions(cx, &symbol, reference);
                    }
                }
            }
        } else if let Some(symbol) = message.topic.strip_prefix("orderBook200.H.") {
            let Some(depth) = self.public.books.get_mut(symbol) else {
                return true;
            };
            let Ok(rows) = serde_json::from_str::<wire::Depth>(data.get()) else {
                return true;
            };
            if !rows
                .a
                .iter()
                .chain(&rows.b)
                .all(|p| p.0 > 0.0 && p.1 >= 0.0)
            {
                return true;
            }
            if message.kind == "snapshot" {
                depth.book.clear();
            } else if message.kind == "delta" {
                let Some(previous) = depth.sequence else {
                    return true;
                };
                if rows.u <= previous {
                    return true;
                }
                if previous.checked_add(1) != Some(rows.u) {
                    depth.book.clear();
                    depth.sequence = None;
                    cx.book_changed(symbol);
                    self.public_subscribe(cx, "unsubscribe", &[message.topic.to_string()]);
                    self.public_subscribe(cx, "subscribe", &[message.topic.to_string()]);
                    return true;
                }
            } else {
                return true;
            }
            depth.sequence = Some(rows.u);
            for (side, rows) in [(BookSide::Asks, rows.a), (BookSide::Bids, rows.b)] {
                for wire::Level(price, amount) in rows {
                    depth.book.apply(side, price, amount);
                }
            }
            cx.book_changed(symbol);
        } else if let Some(topic) = message.topic.strip_prefix("candle.") {
            let Some((native, symbol)) = topic.split_once('.') else {
                return true;
            };
            let Ok(rows) = serde_json::from_str::<Vec<wire::Candle>>(data.get()) else {
                return true;
            };
            let Some(row) = rows.into_iter().next() else {
                return true;
            };
            let candle = Candle {
                timestamp: row.start / 1000.0,
                open: row.open,
                high: row.high,
                low: row.low,
                close: row.close,
                volume: row.volume,
            };
            if !valid_candle(&candle) {
                return true;
            }
            for ((s, i), state) in &mut self.public.candles {
                if s == symbol
                    && interval(i).is_some_and(|(n, _)| n == native)
                    && state
                        .last
                        .as_ref()
                        .is_none_or(|c| c.timestamp <= candle.timestamp)
                {
                    state.last = Some(candle);
                    cx.candle(s, i, candle);
                }
            }
        } else if let Some(symbol) = message.topic.strip_prefix("recentlyTrade.H.") {
            if self.options.public_data_only {
                return true;
            }
            let Ok(rows) = serde_json::from_str::<Vec<wire::Trade>>(data.get()) else {
                return true;
            };
            let Some(row) = rows.into_iter().next() else {
                return true;
            };
            if row.p.value <= 0.0 || row.v.value < 0.0 {
                return true;
            }
            let decimals = row.v.scale.max(row.p.scale).max(8);
            let factor = 10f64.powi(decimals as i32);
            let volume =
                safe_cex_engine::num::round_half_up(row.v.value / row.p.value * factor) / factor;
            if !volume.is_finite() {
                return true;
            }
            for ((s, i), state) in &mut self.public.candles {
                if s != symbol {
                    continue;
                }
                let Some((_, seconds)) = interval(i) else {
                    continue;
                };
                let Some(last) = state.last.as_mut() else {
                    continue;
                };
                let time = (row.time / 1000.0 / seconds).floor() * seconds;
                if time < last.timestamp {
                    continue;
                }
                if time > last.timestamp {
                    *last = Candle {
                        timestamp: time,
                        open: last.close,
                        high: last.close,
                        low: last.close,
                        close: last.close,
                        volume: 0.0,
                    };
                }
                last.high = last.high.max(row.p.value);
                last.low = last.low.min(row.p.value);
                last.close = row.p.value;
                let factor = 10f64.powi(
                    safe_cex_engine::num::decimals(last.volume)
                        .max(safe_cex_engine::num::decimals(volume))
                        .min(308) as i32,
                );
                last.volume =
                    safe_cex_engine::num::round_half_up((last.volume + volume) * factor) / factor;
                cx.candle(s, i, *last);
            }
        }
        true
    }
    pub(crate) fn public_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let depth = self
                    .public
                    .books
                    .entry(symbol.clone())
                    .or_insert_with(|| Depth {
                        count: 0,
                        book: Book::default(),
                        sequence: None,
                    });
                depth.count += 1;
                if depth.count == 1 {
                    self.public_subscribe(cx, "subscribe", &[format!("orderBook200.H.{symbol}")]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(depth) = self.public.books.get_mut(&symbol) {
                    depth.count = depth.count.saturating_sub(1);
                    if depth.count == 0 {
                        self.public.books.remove(&symbol);
                        self.public_subscribe(
                            cx,
                            "unsubscribe",
                            &[format!("orderBook200.H.{symbol}")],
                        );
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                let Some((native, _)) = interval(&i) else {
                    return Ok(());
                };
                let was_trades = self.public.candles.keys().any(|(s, _)| s == &symbol);
                let state = self.public.candles.entry((symbol.clone(), i)).or_default();
                state.count += 1;
                if state.count == 1 {
                    self.public_subscribe(cx, "subscribe", &[format!("candle.{native}.{symbol}")]);
                    if !self.options.public_data_only && !was_trades {
                        self.public_subscribe(
                            cx,
                            "subscribe",
                            &[format!("recentlyTrade.H.{symbol}")],
                        );
                    }
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(state) = self.public.candles.get_mut(&(symbol.clone(), i.clone())) {
                    state.count = state.count.saturating_sub(1);
                    if state.count == 0 {
                        self.public.candles.remove(&(symbol.clone(), i.clone()));
                        if let Some((native, _)) = interval(&i) {
                            self.public_subscribe(
                                cx,
                                "unsubscribe",
                                &[format!("candle.{native}.{symbol}")],
                            );
                        }
                        if !self.options.public_data_only
                            && !self.public.candles.keys().any(|(s, _)| s == &symbol)
                        {
                            self.public_subscribe(
                                cx,
                                "unsubscribe",
                                &[format!("recentlyTrade.H.{symbol}")],
                            );
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some((native, seconds)) = interval(&i) else {
                    cx.done(command, Err("unsupported ApeX interval".into()));
                    return Ok(());
                };
                let limit = limit.filter(|n| *n > 0);
                let count = limit.unwrap_or(500).min(1500) as f64;
                let end = to_ms.filter(|n| n.is_finite()).unwrap_or(cx.now) / 1000.0;
                let start = if limit.is_none() {
                    from_ms
                        .filter(|n| n.is_finite())
                        .map(|n| n / 1000.0)
                        .unwrap_or(end - seconds * count)
                } else {
                    end - seconds * count
                };
                let mut params = vec![
                    ("symbol".into(), symbol.clone()),
                    ("start".into(), (start.floor() as i64).to_string()),
                    ("interval".into(), native.into()),
                ];
                if native != "D" {
                    params.push(("end".into(), (end.floor() as i64).to_string()));
                }
                if let Err(error) = self.signed(
                    cx,
                    crate::sign::Request::get("/v3/klines", params),
                    Pending::Public(Request::Candles { command, symbol }),
                    None,
                ) {
                    cx.done(command, Err(error));
                }
            }
            other => return Err(other),
        }
        Ok(())
    }
}
#[derive(serde::Serialize)]
struct Subscription<'a> {
    op: &'a str,
    args: &'a [String],
}
