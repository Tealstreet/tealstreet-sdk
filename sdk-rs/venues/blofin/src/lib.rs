//! BloFin linear perpetuals on the sans-I/O engine.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use serde::Serialize;
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    /// TS enables payload attribution in production builds or with tealstreetProxy.
    pub broker_attribution: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        safe_cex_crypto::secrets::erase(&mut self.passphrase);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}

pub enum Pending {
    History(history::Request),
    Trade(trading::Request),
    Account(account::Request),
    Markets {
        rows: Vec<Market>,
        failed: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        failed: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        failed: bool,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
#[derive(Default)]
struct DepthState {
    book: Book,
    sequence: Option<u64>,
    subscribers: u32,
    resync: bool,
}
#[derive(Default)]
struct CandleState {
    subscribers: u32,
    last: Option<Candle>,
}
pub struct Blofin {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    account: account::Account,
    trading: trading::Trading,
    public: Connection,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, String), CandleState>,
}
#[derive(Serialize)]
struct Topic<'a> {
    channel: &'a str,
    #[serde(rename = "instId")]
    inst_id: &'a str,
}
#[derive(Serialize)]
struct Subscription<'a> {
    op: &'a str,
    args: &'a [Topic<'a>],
}
fn interval_ms(value: &str) -> Option<f64> {
    let seconds = match value {
        "1m" => 60,
        "3m" => 180,
        "5m" => 300,
        "15m" => 900,
        "30m" => 1800,
        "1h" => 3600,
        "2h" => 7200,
        "4h" => 14400,
        "6h" => 21600,
        "12h" => 43200,
        "1d" => 86400,
        "1w" => 604800,
        _ => return None,
    };
    Some(f64::from(seconds) * 1000.0)
}
fn interval(value: &str) -> String {
    value.replace('h', "H").replace('d', "D").replace('w', "W")
}
fn candle(row: &[Number]) -> Option<Candle> {
    if row.len() < 6 {
        return None;
    }
    Some(Candle {
        timestamp: row[0].0 / 1000.0,
        open: row[1].0,
        high: row[2].0,
        low: row[3].0,
        close: row[4].0,
        volume: row.get(7).unwrap_or(&row[5]).0,
    })
}
fn round_usd(v: f64) -> f64 {
    num::adjust(v, 0.01)
}
impl Blofin {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
        }
    }
    fn host(&self) -> &'static str {
        if self.options.testnet {
            "https://demo-trading-openapi.blofin.com"
        } else {
            "https://openapi.blofin.com"
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        let url = if self.options.testnet {
            "wss://demo-trading-openapi.blofin.com/ws/public"
        } else {
            "wss://openapi.blofin.com/ws/public"
        };
        self.public.socket = Some(cx.connect_channel(url, safe_cex_engine::SocketChannel::Public));
        self.public.last_received = cx.now;
        self.public.watchdog = Some(cx.set_timer(5_000.0));
    }
    fn read(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.host()),
            vec![],
            ArrayAt::EnvelopeField("data"),
            pending,
        );
    }
    fn topics(&self, cx: &mut Cx<'_, Pending>, op: &str, topics: &[Topic<'_>]) {
        let Some(socket) = self.public.socket.filter(|_| self.public.open) else {
            return;
        };
        let overhead = serde_json::to_string(&Subscription { op, args: &[] })
            .unwrap()
            .len();
        let mut start = 0;
        let mut length = overhead;
        // Measure each topic once; serializing every growing prefix is quadratic.
        for (end, topic) in topics.iter().enumerate() {
            let size = serde_json::to_string(topic).unwrap().len();
            if length + size + usize::from(end > start) > 4096 && end > start {
                cx.send(
                    socket,
                    serde_json::to_string(&Subscription {
                        op,
                        args: &topics[start..end],
                    })
                    .unwrap(),
                );
                start = end;
                length = overhead;
            }
            length += size + usize::from(end > start);
        }
        if start < topics.len() {
            cx.send(
                socket,
                serde_json::to_string(&Subscription {
                    op,
                    args: &topics[start..],
                })
                .unwrap(),
            );
        }
    }
    fn subscription(&self, cx: &mut Cx<'_, Pending>, op: &str, symbol: &str, channels: &[&str]) {
        if let Some(market) = cx.store.market(symbol) {
            let id = market.id.clone();
            self.topics(
                cx,
                op,
                &channels
                    .iter()
                    .map(|channel| Topic {
                        channel,
                        inst_id: &id,
                    })
                    .collect::<Vec<_>>(),
            );
        }
    }
    fn subscribe_all(&self, cx: &mut Cx<'_, Pending>) {
        let ids: Vec<_> = cx.store.markets().iter().map(|m| m.id.clone()).collect();
        let topics: Vec<_> = ids
            .iter()
            .flat_map(|id| {
                [
                    Topic {
                        channel: "tickers",
                        inst_id: id,
                    },
                    Topic {
                        channel: "funding-rate",
                        inst_id: id,
                    },
                ]
            })
            .collect();
        self.topics(cx, "subscribe", &topics);
        for symbol in self.books.keys() {
            self.subscription(cx, "subscribe", symbol, &["books", "books5"]);
        }
        for (symbol, i) in self.candles.keys() {
            self.subscription(
                cx,
                "subscribe",
                symbol,
                &[&format!("candle{}", interval(i)), "books5"],
            );
        }
    }
    fn ticker(&self, cx: &mut Cx<'_, Pending>, row: &Tick<'_>, funding: bool) {
        let Some(m) = cx.store.market_by_id(&row.inst_id) else {
            return;
        };
        let symbol = m.symbol.clone();
        cx.store.update_ticker(&symbol, |t| {
            if funding {
                t.funding_rate = row.funding_rate.0;
                t.funding_time = row.funding_time.0;
            } else {
                t.bid = row.bid_price.0;
                t.ask = row.ask_price.0;
                t.last = row.last.0;
                t.mark = t.last;
                t.index = t.last;
                t.open_interest = 0.0;
                t.percentage = round_usd((t.last - row.open24h.0) / row.open24h.0 * 100.0);
                t.volume = row.vol_currency24h.0;
                t.quote_volume = row.vol24h.0;
            }
        });
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, message: &Message<'_>, d: &Depth) {
        if message.arg.channel == "books5" {
            for ((s, i), state) in &mut self.candles {
                if s != symbol {
                    continue;
                }
                if let (Some(last), Some(bid), Some(ask)) =
                    (state.last.as_mut(), d.bids.first(), d.asks.first())
                {
                    if bid.is_empty() || ask.is_empty() {
                        continue;
                    }
                    let step = match i.as_str() {
                        "1w" => 604800.0,
                        "1d" => 86400.0,
                        _ => {
                            let n = i[..i.len() - 1].parse::<f64>().unwrap_or(1.0);
                            n * if i.ends_with('h') { 3600.0 } else { 60.0 }
                        }
                    };
                    let time = (d.ts.0 / 1000.0 / step).floor() * step;
                    if last.timestamp == time {
                        let mut prices = [bid[0].0, last.close, ask[0].0];
                        prices.sort_by(f64::total_cmp);
                        if last.close == prices[1] {
                            continue;
                        }
                        last.close = prices[1];
                        last.high = last.high.max(last.close);
                        last.low = last.low.min(last.close);
                        cx.candle(s, i, *last);
                    }
                }
            }
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        let secondary = message.arg.channel == "books5";
        if !secondary
            && message.action == "update"
            && (state.sequence.is_none()
                || d.prev_seq_id.map(|n| n.0) != state.sequence
                || d.seq_id.is_none())
        {
            state.book.clear();
            state.sequence = None;
            let request = !state.resync;
            state.resync = true;
            if request {
                self.subscription(cx, "unsubscribe", symbol, &["books"]);
                self.subscription(cx, "subscribe", symbol, &["books"]);
            }
            return;
        }
        if !secondary && message.action == "snapshot" {
            state.book.clear();
            state.resync = false;
        }
        if !secondary {
            state.sequence = d.seq_id.map(|n| n.0);
        }
        for (side, rows) in [(BookSide::Bids, &d.bids), (BookSide::Asks, &d.asks)] {
            if secondary {
                let old = if side == BookSide::Bids {
                    state.book.bids()
                } else {
                    state.book.asks()
                };
                let mut levels: Vec<_> = rows
                    .iter()
                    .filter(|r| r.len() >= 2 && r[1].0 != 0.0)
                    .map(|r| (r[0].0, r[1].0))
                    .collect();
                if levels.len() >= 5
                    && let Some(boundary) = levels.last().map(|(price, _)| *price)
                {
                    levels.extend(old.iter().copied().filter(|(price, _)| {
                        if side == BookSide::Bids {
                            *price < boundary
                        } else {
                            *price > boundary
                        }
                    }));
                }
                state.book.load_sorted(side, levels);
            } else {
                for row in rows {
                    if row.len() >= 2 {
                        state.book.apply(side, row[0].0, row[1].0);
                    }
                }
            }
        }
        if !secondary
            && let Some(last) = cx.store.ticker(symbol).map(|t| t.last).filter(|v| *v > 0.0)
        {
            let bids = state
                .book
                .bids()
                .iter()
                .copied()
                .filter(|(p, _)| *p <= last)
                .collect::<Vec<_>>();
            let asks = state
                .book
                .asks()
                .iter()
                .copied()
                .filter(|(p, _)| *p >= last)
                .collect::<Vec<_>>();
            state.book.load_sorted(BookSide::Bids, bids);
            state.book.load_sorted(BookSide::Asks, asks);
        }
        cx.book_changed(symbol);
    }
}
impl Venue for Blofin {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|_| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.read(
            cx,
            "/api/v1/market/instruments",
            Pending::Markets {
                rows: vec![],
                failed: false,
            },
        );
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.connection.socket == Some(socket) {
            self.private_open(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        self.subscribe_all(cx);
        cx.send(socket, "ping");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.connection.socket == Some(socket) {
            self.private_message(cx, socket, text);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.last_received = cx.now;
        if text == "pong" {
            return;
        }
        if text == "ping" {
            cx.send(socket, "pong");
            return;
        }
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if !m.event.is_empty() {
            if m.event == "error" {
                cx.log(LogLevel::Error, format!("BloFin socket error {}", m.code));
            }
            return;
        }
        match m.arg.channel.as_ref() {
            "tickers" | "funding-rate" => {
                if let Ok(rows) = serde_json::from_str::<Data<Vec<Tick<'_>>>>(text) {
                    for row in rows.data {
                        self.ticker(cx, &row, m.arg.channel == "funding-rate");
                    }
                }
            }
            "books" | "books5" => {
                if let Ok(rows) = serde_json::from_str::<Data<OneOrMany<Depth>>>(text)
                    && let Some(d) = rows.data.first()
                    && let Some(market) = cx.store.market_by_id(&m.arg.inst_id)
                {
                    let symbol = market.symbol.clone();
                    self.depth(cx, &symbol, &m, d);
                }
            }
            channel if channel.starts_with("candle") => {
                if let Ok(rows) = serde_json::from_str::<Data<OneOrMany<CandleWire>>>(text)
                    && let Some(c) = rows.data.first().and_then(CandleWire::candle)
                    && let Some(market) = cx.store.market_by_id(&m.arg.inst_id)
                {
                    let symbol = market.symbol.clone();
                    for ((s, i), state) in &mut self.candles {
                        if *s == symbol
                            && format!("candle{}", interval(i)) == channel
                            && state.last != Some(c)
                        {
                            state.last = Some(c);
                            cx.candle(s, i, c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.connection.socket == Some(socket) {
            self.private_close(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer);
        }
        for state in self.books.values_mut() {
            state.sequence = None;
            state.book.clear();
            state.resync = false;
        }
        self.public.attempts += 1;
        self.public.reconnect =
            Some(cx.set_timer(
                (1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.0),
            ));
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Trade(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Markets { rows, failed } => match serde_json::from_str::<Instrument<'_>>(item)
            {
                Ok(m)
                    if m.inst_type.eq_ignore_ascii_case("SWAP")
                        && m.contract_type.eq_ignore_ascii_case("linear") =>
                {
                    rows.push(Market {
                        id: m.inst_id.to_string(),
                        symbol: m.inst_id.trim_end_matches("-SWAP").replace('-', ""),
                        pretty_symbol: None,
                        base: m.base_currency.into_owned(),
                        quote: m.quote_currency.into_owned(),
                        active: m.state == "live",
                        precision: Precision {
                            amount: if m.lot_size.0 > 0.0 {
                                m.lot_size.0
                            } else {
                                m.min_size.0
                            },
                            price: m.tick_size.0,
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: m.min_size.0 * m.contract_value.0,
                                max: m.max_limit_size.0.min(m.max_market_size.0),
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: m.max_leverage.0,
                            },
                            price: None,
                        },
                        contract_size: Some(m.contract_value.0),
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    })
                }
                Ok(_) => {}
                Err(_) => *failed = true,
            },
            Pending::Tickers { rows, failed } => match serde_json::from_str::<Tick<'_>>(item) {
                Ok(t) => {
                    if let Some(m) = cx.store.market_by_id(&t.inst_id) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: t.bid_price.0,
                            ask: t.ask_price.0,
                            last: t.last.0,
                            mark: t.last.0,
                            index: t.last.0,
                            percentage: round_usd((t.last.0 - t.open24h.0) / t.open24h.0 * 100.0),
                            volume: t.vol_currency24h.0,
                            quote_volume: t.vol24h.0,
                            ..Ticker::default()
                        });
                    }
                }
                Err(_) => *failed = true,
            },
            Pending::Candles { rows, failed, .. } => {
                match serde_json::from_str::<Vec<Number>>(item)
                    .ok()
                    .and_then(|r| candle(&r))
                {
                    Some(c) => rows.push(c),
                    None => *failed = true,
                }
            }
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let pending = match pending {
            Pending::History(request) => {
                self.history_response(cx, request, response);
                return;
            }
            Pending::Account(request) => {
                self.account_response(cx, request, response);
                return;
            }
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            other => other,
        };
        if let Err(error) = check(&response) {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Markets {
                rows,
                failed: false,
            } => {
                cx.store.set_markets(rows);
                self.connect(cx);
                self.read(
                    cx,
                    "/api/v1/market/tickers",
                    Pending::Tickers {
                        rows: vec![],
                        failed: false,
                    },
                );
            }
            Pending::Tickers {
                rows,
                failed: false,
            } => {
                cx.store.set_tickers(rows);
                self.private_start(cx);
            }
            Pending::Candles {
                command,
                mut rows,
                failed: false,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(command, Ok(serde_json::to_string(&rows).unwrap()));
            }
            other => self.on_request_failed(cx, other, "Malformed BloFin response"),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let pending = match pending {
            Pending::History(request) => {
                self.history_failed(cx, request, error);
                return;
            }
            Pending::Trade(request) => {
                self.trade_failed(cx, request, error);
                return;
            }
            other => other,
        };
        if let Pending::Account(request) = pending {
            self.account_failed(cx, request, error);
            return;
        }
        if let Pending::Candles { command, .. } = pending {
            cx.done(command, Err(error.to_string()));
        } else {
            cx.log(LogLevel::Error, format!("BloFin read failed: {error}"));
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trading_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect(cx);
        } else if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.last_received >= 20_000.0 {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
            } else {
                if let Some(socket) = self.public.socket.filter(|_| self.public.open) {
                    cx.send(socket, "ping");
                }
                self.public.watchdog = Some(cx.set_timer(5_000.0));
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscription(cx, "subscribe", &symbol, &["books", "books5"]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscription(cx, "unsubscribe", &symbol, &["books"]);
                        if !self.candles.keys().any(|(s, _)| *s == symbol) {
                            self.subscription(cx, "unsubscribe", &symbol, &["books5"]);
                        }
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval_ms(&i).is_none() {
                    cx.log(LogLevel::Error, "Unsupported candle interval");
                    return;
                }
                let state = self.candles.entry((symbol.clone(), i.clone())).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscription(
                        cx,
                        "subscribe",
                        &symbol,
                        &[&format!("candle{}", interval(&i)), "books5"],
                    );
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(state) = self.candles.get_mut(&(symbol.clone(), i.clone())) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.candles.remove(&(symbol.clone(), i.clone()));
                        self.subscription(
                            cx,
                            "unsubscribe",
                            &symbol,
                            &[&format!("candle{}", interval(&i))],
                        );
                        if !self.books.contains_key(&symbol)
                            && !self.candles.keys().any(|(s, _)| *s == symbol)
                        {
                            self.subscription(cx, "unsubscribe", &symbol, &["books5"]);
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
                if let Some(m) = cx.store.market(&symbol) {
                    let Some(step) = interval_ms(&i) else {
                        cx.done(command, Err("Unsupported candle interval".into()));
                        return;
                    };
                    let end = to_ms.unwrap_or(cx.now);
                    let count = limit.filter(|n| *n > 0).unwrap_or(1440).min(1440);
                    let start = if limit.is_none() {
                        from_ms.unwrap_or(end - step * f64::from(count))
                    } else {
                        end - step * f64::from(count)
                    };
                    let path = format!(
                        "/api/v1/market/candles?instId={}&bar={}&before={}&after={}",
                        m.id,
                        interval(&i),
                        num::format(start),
                        num::format(end)
                    );
                    self.read(
                        cx,
                        &path,
                        Pending::Candles {
                            command,
                            rows: vec![],
                            failed: false,
                        },
                    );
                } else {
                    cx.done(command, Err(format!("Unknown market {symbol}")));
                }
            }
            Command::FetchLeverage { command, symbol } => self.fetch_leverage(cx, command, symbol),
            Command::Query { command, query } => self.start_query(cx, command, query),
            other => self.trading_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

#[cfg(test)]
mod secondary_tests;
