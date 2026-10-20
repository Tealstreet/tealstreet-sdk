//! WOO perpetuals on the active legacy-public/V1/V3 protocol.
mod account;
mod history;
mod identity;
mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
    num,
};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
}

pub enum Pending {
    Identity(CommandId),
    Trade(trading::Request),
    History(history::Request),
    Account(account::Request),
    Markets {
        rows: Vec<Market>,
        ranges: HashMap<String, f64>,
        bad: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        indexes: HashMap<String, u32>,
        bad: bool,
    },
    Funding {
        rows: Vec<(String, f64, f64)>,
    },
    Depth {
        symbol: String,
        generation: u64,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        bad: bool,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    received: f64,
    ping_at: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
struct Buffered {
    time: f64,
    delta: Delta,
}
enum Sync {
    Buffer(Vec<Buffered>),
    Live(f64),
    Retry(TimerId),
}
struct Depth {
    count: u32,
    generation: u64,
    book: Book,
    sync: Sync,
}
#[derive(Default)]
struct CandleState {
    count: u32,
    last: Option<Candle>,
}
pub struct Woo {
    trading: trading::Trading,
    history: history::History,
    account: account::Account,
    options: Options,
    rest: &'static str,
    public: Connection,
    depth: HashMap<String, Depth>,
    candles: HashMap<(String, String), CandleState>,
    generation: u64,
    funding: Option<TimerId>,
    seed_retry: Option<(TimerId, bool)>,
    seed_attempts: u32,
    price_ranges: HashMap<String, f64>,
    ticker_indexes: HashMap<String, u32>,
}

pub(crate) fn compact(native: &str) -> String {
    native
        .strip_prefix("PERP_")
        .unwrap_or(native)
        .replace('_', "")
}
pub(crate) fn native(m: &Market) -> String {
    format!("PERP_{}_{}", m.base, m.quote)
}
pub(crate) fn interval_ms(interval: &str) -> Option<f64> {
    if interval == "1M" {
        return Some(2_592_000_000.0);
    }
    if !interval.is_ascii() {
        return None;
    }
    let (value, unit) = interval.split_at(interval.len().checked_sub(1)?);
    let multiplier = match unit {
        "s" => 1000.0,
        "m" => 60_000.0,
        "h" => 3_600_000.0,
        "d" => 86_400_000.0,
        "w" => 604_800_000.0,
        _ => return None,
    };
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .map(|v| v * multiplier)
}
fn candle(raw: Kline) -> Option<Candle> {
    if raw.start_timestamp < 0.0
        || raw.low <= 0.0
        || raw.high < raw.low
        || raw.open <= 0.0
        || raw.close <= 0.0
        || raw.volume < 0.0
    {
        return None;
    }
    Some(Candle {
        timestamp: raw.start_timestamp / 1000.0,
        open: raw.open,
        high: raw.high,
        low: raw.low,
        close: raw.close,
        volume: raw.volume,
    })
}
impl Woo {
    fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<String>,
        pending: Pending,
    ) -> Result<safe_cex_engine::RequestId, String> {
        if method == Method::Get && path.contains("/public/") {
            let query = sign::query(&params, true);
            let path = if query.is_empty() {
                path.to_owned()
            } else {
                format!("{path}?{query}")
            };
            return Ok(cx.request(
                method,
                format!("{}{}", self.rest, path),
                vec![],
                None,
                pending,
            ));
        }
        let (Some(key), Some(secret)) = (&self.options.key, &self.options.secret) else {
            return Err("WOO private requests require credentials".into());
        };
        if self.options.public_data_only || key.is_empty() || secret.is_empty() {
            return Err("WOO private requests require credentials".into());
        }
        let read = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get);
        let key = read.map_or(key.as_str(), |r| r.key.as_str());
        let secret = read.map_or(secret.as_str(), |r| r.secret.as_str());
        let request = sign::request(key, secret, cx.now, method, path, &params, body);
        let url = if request.path.starts_with("https://") {
            request.path
        } else {
            format!("{}{}", self.rest, request.path)
        };
        Ok(cx.request(method, url, request.headers, request.body, pending))
    }
    pub fn new(options: Options) -> Self {
        Self {
            account: account::Account::default(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            rest: if options.testnet {
                "https://api.staging.woox.io"
            } else {
                "https://api.woox.io"
            },
            options,
            public: Connection::default(),
            depth: HashMap::new(),
            candles: HashMap::new(),
            generation: 0,
            funding: None,
            seed_retry: None,
            seed_attempts: 0,
            price_ranges: HashMap::new(),
            ticker_indexes: HashMap::new(),
        }
    }
    pub fn price_range(&self, symbol: &str) -> Option<f64> {
        self.price_ranges.get(symbol).copied()
    }
    fn seed(&self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        let (path, pending) = if tickers {
            (
                "/v1/public/futures",
                Pending::Tickers {
                    indexes: HashMap::new(),
                    rows: Vec::new(),
                    bad: false,
                },
            )
        } else {
            (
                "/v1/public/info",
                Pending::Markets {
                    rows: Vec::new(),
                    ranges: HashMap::new(),
                    bad: false,
                },
            )
        };
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.rest),
            vec![],
            ArrayAt::Field("rows"),
            pending,
        );
    }
    fn apply_ticks(&self, cx: &mut Cx<'_, Pending>, topic: &str, rows: Vec<Tick<'_>>) {
        #[derive(Clone, Copy)]
        enum Kind {
            Tickers,
            Bbos,
            Marks,
        }
        let kind = match topic {
            "tickers" => Kind::Tickers,
            "bbos" => Kind::Bbos,
            _ => Kind::Marks,
        };
        for row in rows {
            if !row.symbol.starts_with("PERP_") {
                continue;
            }
            let Some(&index) = self.ticker_indexes.get(row.symbol.as_ref()) else {
                continue;
            };
            let Some(t) = cx.store.tickers().get(index as usize) else {
                continue;
            };
            // Most WOO batches repeat existing values. Avoid the store's full-field
            // dirty-mask scan unless this topic changes a supplied field. Compare
            // bits to retain its signed-zero change semantics.
            let changed =
                |value: Option<f64>, old: f64| value.is_some_and(|v| v.to_bits() != old.to_bits());
            let dirty = match kind {
                Kind::Tickers => {
                    changed(row.close, t.last)
                        || changed(row.volume, t.volume)
                        || changed(row.amount, t.quote_volume)
                }
                Kind::Bbos => changed(row.bid, t.bid) || changed(row.ask, t.ask),
                Kind::Marks => changed(row.price, t.mark),
            };
            if !dirty {
                continue;
            }
            cx.store.update_ticker_at(index, |t| match kind {
                Kind::Tickers => {
                    if let Some(v) = row.close {
                        t.last = v;
                    }
                    if let Some(v) = row.volume {
                        t.volume = v;
                    }
                    if let Some(v) = row.amount {
                        t.quote_volume = v;
                    }
                }
                Kind::Bbos => {
                    if let Some(v) = row.bid {
                        t.bid = v;
                    }
                    if let Some(v) = row.ask {
                        t.ask = v;
                    }
                }
                Kind::Marks => {
                    if let Some(v) = row.price {
                        t.mark = v;
                    }
                }
            });
        }
    }
    fn funding_read(&self, cx: &mut Cx<'_, Pending>) {
        cx.request_streamed(
            Method::Get,
            format!("{}/v1/public/funding_rates", self.rest),
            vec![],
            ArrayAt::Field("rows"),
            Pending::Funding { rows: Vec::new() },
        );
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        let host = if self.options.testnet {
            "wss://wss.staging.woox.io/ws/stream/"
        } else {
            "wss://wss.woox.io/ws/stream/"
        };
        self.public.socket = Some(cx.connect_channel(
            format!("{host}{}", self.options.application_id.as_deref().unwrap_or("undefined")),
            SocketChannel::Public,
        ));
    }
    fn topic(&self, cx: &mut Cx<'_, Pending>, topic: &str, subscribe: bool) {
        if let Some(socket) = self.public.socket.filter(|_| self.public.open) {
            cx.send(
                socket,
                format!(
                    r#"{{"event":"{}","topic":{}}}"#,
                    if subscribe {
                        "subscribe"
                    } else {
                        "unsubscribe"
                    },
                    serde_json::to_string(topic).unwrap()
                ),
            );
        }
    }
    fn book_request(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(m) = cx.store.market(symbol) else {
            return;
        };
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        self.generation += 1;
        state.generation = self.generation;
        state.sync = Sync::Buffer(Vec::new());
        cx.request(
            Method::Get,
            format!("{}/v1/public/orderbook/{}", self.rest, native(m)),
            vec![],
            None,
            Pending::Depth {
                symbol: symbol.into(),
                generation: state.generation,
            },
        );
    }
    fn book_retry(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        if let Sync::Retry(timer) = state.sync {
            cx.clear_timer(timer);
        }
        self.generation += 1;
        state.generation = self.generation;
        state.book.clear();
        state.sync = Sync::Retry(cx.set_timer(1000.0));
    }
    fn apply(book: &mut Book, delta: Delta) {
        for (side, rows) in [(BookSide::Bids, delta.bids), (BookSide::Asks, delta.asks)] {
            for (p, q) in rows {
                book.apply(side, p.0, q.0);
            }
        }
    }
    fn depth_message(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, time: f64, text: &str) {
        let Some(state) = self.depth.get_mut(symbol) else {
            return;
        };
        let Ok(delta) = serde_json::from_str::<Delta>(text) else {
            self.book_retry(cx, symbol);
            return;
        };
        if !delta.valid() || !time.is_finite() || time <= 0.0 {
            self.book_retry(cx, symbol);
            return;
        }
        match &mut state.sync {
            Sync::Buffer(rows) if rows.len() < 2000 => rows.push(Buffered { time, delta }),
            Sync::Buffer(_) => self.book_retry(cx, symbol),
            Sync::Live(previous) => {
                if time < *previous {
                    return;
                }
                *previous = time;
                Self::apply(&mut state.book, delta);
                cx.book_changed(symbol);
            }
            Sync::Retry(_) => (),
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        if let Some(state) = self.depth.get_mut(&symbol) {
            state.count += 1;
            return;
        }
        self.depth.insert(
            symbol.clone(),
            Depth {
                count: 1,
                generation: 0,
                book: Book::default(),
                sync: Sync::Buffer(Vec::new()),
            },
        );
        if self.public.open
            && let Some(m) = cx.store.market(&symbol)
        {
            self.topic(cx, &format!("{}@orderbookupdate", native(m)), true);
            self.book_request(cx, &symbol);
        }
    }
    fn stream_candle(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        interval: &str,
        raw: Kline,
    ) {
        let Some(mut next) = candle(raw) else {
            return;
        };
        let Some(state) = self.candles.get_mut(&(symbol.into(), interval.into())) else {
            return;
        };
        if let Some(old) = &state.last {
            if next.timestamp < old.timestamp {
                return;
            }
            if next.timestamp == old.timestamp {
                next.high = next.high.max(old.high);
                next.low = next.low.min(old.low);
                next.open = old.open;
            }
        }
        state.last = Some(next);
        cx.candle(symbol, interval, next);
    }
    fn trade_candles(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, time: f64, text: &str) {
        #[derive(serde::Deserialize)]
        struct Trade {
            #[serde(deserialize_with = "number")]
            p: f64,
            #[serde(deserialize_with = "number")]
            a: f64,
        }
        let Ok(rows) = serde_json::from_str::<Vec<Trade>>(text) else {
            return;
        };
        // The TS stream synthesizes from the first trade in each message.
        let Some(t) = rows.first().filter(|t| t.p > 0.0 && t.a >= 0.0) else {
            return;
        };
        for ((s, interval), state) in &mut self.candles {
            if s != symbol {
                continue;
            }
            let Some(ms) = interval_ms(interval) else {
                continue;
            };
            let start = (time / ms).floor() * ms / 1000.0;
            if interval.ends_with('s') && state.last.as_ref().is_none_or(|c| c.timestamp != start) {
                if state.last.as_ref().is_some_and(|c| c.timestamp > start) {
                    continue;
                }
                state.last = Some(Candle {
                    timestamp: start,
                    open: state.last.as_ref().map_or(t.p, |c| c.close),
                    high: t.p,
                    low: t.p,
                    close: t.p,
                    volume: 0.0,
                });
            }
            let Some(c) = &mut state.last else {
                continue;
            };
            if start < c.timestamp {
                continue;
            }
            if start > c.timestamp {
                *c = Candle {
                    timestamp: start,
                    open: c.close,
                    high: c.close,
                    low: c.close,
                    close: c.close,
                    volume: 0.0,
                };
            }
            c.high = c.high.max(t.p);
            c.low = c.low.min(t.p);
            c.close = t.p;
            c.volume += t.a;
            cx.candle(s, interval, *c);
        }
    }
    fn fetch_candles(&self, cx: &mut Cx<'_, Pending>, cmd: Command) {
        let Command::FetchCandles {
            command,
            symbol,
            interval,
            limit,
            from_ms: from,
            to_ms: to,
        } = cmd
        else {
            unreachable!()
        };
        let (symbol, interval) = (symbol.as_str(), interval.as_str());
        let Some(m) = cx.store.market(symbol) else {
            cx.done(command, Err("Market not found".into()));
            return;
        };
        if interval_ms(interval).is_none()
            || from.is_some_and(|v| !v.is_finite())
            || to.is_some_and(|v| !v.is_finite())
            || from.zip(to).is_some_and(|(a, b)| a > b)
        {
            cx.done(command, Err("Invalid WOO candle range or interval".into()));
            return;
        }
        let size = limit.unwrap_or(500).clamp(1, 1000);
        let ms = interval_ms(interval).unwrap();
        let recent = (((cx.now / ms).floor() * ms - to.unwrap_or(0.0)).abs() / ms) < 100.0;
        let (mut url, at) = if recent {
            (
                format!(
                    "{}/v1/kline?symbol={}&type={interval}&limit={size}",
                    self.rest,
                    native(m)
                ),
                ArrayAt::Field("rows"),
            )
        } else {
            (
                format!(
                    "https://api-pub.woox.io/v1/hist/kline?symbol={}&type={interval}&size={size}",
                    native(m)
                ),
                ArrayAt::Path(&["data", "rows"]),
            )
        };
        if !recent {
            if let Some(v) = from {
                url.push_str(&format!("&start_time={}", num::format(v)));
            }
            if let Some(v) = to {
                url.push_str(&format!("&end_time={}", num::format(v)));
            }
        }
        let id = cx.request_streamed(
            Method::Get,
            url,
            vec![],
            at,
            Pending::Candles {
                command,
                rows: Vec::new(),
                bad: false,
            },
        );
        if !recent {
            cx.route_request(id, safe_cex_engine::RequestRoute::Primary);
        }
    }
}

impl Venue for Woo {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.seed(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.received = cx.now;
        self.public.attempts = 0;
        self.public.ping_at = cx.now;
        cx.send(socket, r#"{"event":"ping"}"#);
        for topic in ["tickers", "bbos", "markprices"] {
            self.topic(cx, topic, true);
        }
        self.public.watchdog = Some(cx.set_timer(6000.0));
        let symbols: Vec<_> = self.depth.keys().cloned().collect();
        for symbol in symbols {
            if let Some(m) = cx.store.market(&symbol) {
                self.topic(cx, &format!("{}@orderbookupdate", native(m)), true);
            }
            self.book_request(cx, &symbol);
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(m) = cx.store.market(symbol) {
                self.topic(cx, &format!("{}@kline_{interval}", native(m)), true);
            }
        }
        if !self.options.public_data_only {
            let mut seen = std::collections::HashSet::new();
            for (symbol, _) in self.candles.keys() {
                if seen.insert(symbol)
                    && let Some(m) = cx.store.market(symbol)
                {
                    self.topic(cx, &format!("{}@trades", native(m)), true);
                }
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.received = cx.now;
        if let Ok(msg) = serde_json::from_str::<TickerMessage<'_>>(text)
            && msg.event.is_empty()
            && matches!(msg.topic.as_ref(), "tickers" | "bbos" | "markprices")
        {
            self.apply_ticks(cx, msg.topic.as_ref(), msg.data);
            return;
        }
        let Ok(msg) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if !msg.event.is_empty() {
            if msg.success == Some(false) {
                cx.log(LogLevel::Error, "WOO public subscription rejected");
            }
            match msg.event.as_ref() {
                "ping" => cx.send(socket, r#"{"event":"pong"}"#),
                "pong" => cx
                    .store
                    .set_latency((cx.now - self.public.ping_at).max(0.0)),
                _ => (),
            }
            return;
        }
        let Some(data) = msg.data else {
            return;
        };
        match msg.topic.as_ref() {
            "tickers" | "bbos" | "markprices" => (),
            topic => {
                let Some((symbol, kind)) = topic.split_once('@') else {
                    return;
                };
                let symbol = compact(symbol);
                if kind == "orderbookupdate" {
                    self.depth_message(cx, &symbol, msg.ts, data.get());
                } else if let Some(interval) = kind.strip_prefix("kline_") {
                    if let Ok(raw) = serde_json::from_str(data.get()) {
                        self.stream_candle(cx, &symbol, interval, raw);
                    }
                } else if kind == "trades" {
                    self.trade_candles(cx, &symbol, msg.ts, data.get());
                }
            }
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        if let Some(t) = self.public.watchdog.take() {
            cx.clear_timer(t);
        }
        self.public.attempts += 1;
        self.public.reconnect =
            Some(cx.set_timer(
                (1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.0),
            ));
        for state in self.depth.values_mut() {
            self.generation += 1;
            state.generation = self.generation;
            state.book.clear();
            state.sync = Sync::Buffer(Vec::new());
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Identity(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Trade(_) => (),
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Markets { rows, ranges, bad } => {
                let raw = match serde_json::from_str::<Instrument<'_>>(item) {
                    Ok(raw) => raw,
                    Err(_) => {
                        if serde_json::from_str::<Symbol<'_>>(item)
                            .is_ok_and(|s| !s.symbol.starts_with("PERP_"))
                        {
                            return;
                        }
                        *bad = true;
                        return;
                    }
                };
                let Some((base, quote)) = raw
                    .symbol
                    .strip_prefix("PERP_")
                    .and_then(|s| s.split_once('_'))
                else {
                    return;
                };
                if raw.base_tick <= 0.0
                    || raw.quote_tick <= 0.0
                    || raw.base_min <= 0.0
                    || raw.base_max < raw.base_min
                {
                    *bad = true;
                    return;
                }
                let symbol = compact(&raw.symbol);
                if let Some(range) = raw.price_range.filter(|v| *v > 0.0) {
                    ranges.insert(symbol.clone(), range);
                }
                rows.push(Market {
                    id: format!("{base}/{quote}:USDT"),
                    symbol,
                    pretty_symbol: None,
                    base: base.into(),
                    quote: quote.into(),
                    active: true,
                    precision: Precision {
                        amount: raw.base_tick,
                        price: raw.quote_tick,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: raw.base_min,
                            max: raw.base_max,
                        },
                        leverage: MinMax {
                            min: 1.0,
                            max: (1.0
                                / if raw.base_imr > 0.0 {
                                    raw.base_imr
                                } else {
                                    0.01
                                })
                            .floor(),
                        },
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
                });
            }
            Pending::Tickers { rows, indexes, bad } => {
                let raw = match serde_json::from_str::<Statistics<'_>>(item) {
                    Ok(raw) => raw,
                    Err(_) => {
                        if serde_json::from_str::<Symbol<'_>>(item)
                            .is_ok_and(|s| cx.store.market(&compact(&s.symbol)).is_none())
                        {
                            return;
                        }
                        *bad = true;
                        return;
                    }
                };
                let symbol = compact(&raw.symbol);
                let Some(m) = cx.store.market(&symbol) else {
                    return;
                };
                if raw.open <= 0.0 || raw.close <= 0.0 {
                    *bad = true;
                    return;
                }
                indexes.insert(raw.symbol.to_string(), rows.len() as u32);
                rows.push(Ticker {
                    id: m.id.clone(),
                    symbol,
                    bid: raw.close,
                    ask: raw.close,
                    last: raw.close,
                    mark: raw.mark_price,
                    index: raw.index_price.unwrap_or(f64::NAN),
                    percentage: num::round_half_up((raw.close - raw.open) / raw.open * 10000.0)
                        / 100.0,
                    open_interest: raw.open_interest,
                    funding_rate: raw.last_funding_rate,
                    funding_time: raw.next_funding_time,
                    volume: raw.volume,
                    quote_volume: raw.amount,
                    mid_price: None,
                });
            }
            Pending::Funding { rows } => {
                if let Ok(raw) = serde_json::from_str::<Funding<'_>>(item)
                    && raw.next_funding_time > 0.0
                {
                    rows.push((
                        compact(&raw.symbol),
                        raw.est_funding_rate,
                        raw.next_funding_time,
                    ));
                }
            }
            Pending::Candles { rows, bad, .. } => {
                if let Some(c) = serde_json::from_str(item).ok().and_then(candle) {
                    rows.push(c);
                } else {
                    *bad = true;
                }
            }
            Pending::Depth { .. } => (),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Identity(command) = pending {
            self.identity_response(cx, command, response);
            return;
        }
        let pending = match pending {
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            Pending::History(request) => {
                self.history_response(cx, request, response);
                return;
            }
            other => other,
        };
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            return;
        }
        if !(200..300).contains(&response.status) || Envelope::checked(response.body).is_err() {
            self.on_request_failed(cx, pending, "WOO read rejected");
            return;
        }
        match pending {
            Pending::Identity(_) | Pending::History(_) | Pending::Trade(_) => unreachable!(),
            Pending::Account(_) => unreachable!(),
            Pending::Markets { rows, ranges, bad } => {
                if bad || rows.is_empty() {
                    self.on_request_failed(
                        cx,
                        Pending::Markets {
                            rows: vec![],
                            ranges: HashMap::new(),
                            bad: false,
                        },
                        "Invalid WOO markets",
                    );
                    return;
                }
                self.price_ranges = ranges;
                cx.store.set_markets(rows);
                self.seed_attempts = 0;
                self.seed(cx, true);
            }
            Pending::Tickers { rows, indexes, bad } => {
                if bad || rows.is_empty() {
                    self.on_request_failed(
                        cx,
                        Pending::Tickers {
                            indexes: HashMap::new(),
                            rows: vec![],
                            bad: false,
                        },
                        "Invalid WOO tickers",
                    );
                    return;
                }
                self.ticker_indexes = indexes;
                cx.store.set_tickers(rows);
                self.seed_attempts = 0;
                self.connect_public(cx);
                self.funding = Some(cx.set_timer(60_000.0));
                if self.options.public_data_only
                    || self.options.key.as_deref().is_none_or(str::is_empty)
                {
                    cx.store.set_balance(Default::default());
                    cx.store.set_positions(Vec::new());
                    cx.store.set_orders(Vec::new());
                } else {
                    self.account_start(cx);
                }
            }
            Pending::Funding { rows } => {
                for (symbol, rate, time) in rows {
                    cx.store.update_ticker(&symbol, |t| {
                        t.funding_rate = rate;
                        t.funding_time = time;
                    });
                }
            }
            Pending::Candles {
                command,
                mut rows,
                bad,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    if bad {
                        Err("Invalid WOO candle page".into())
                    } else {
                        Ok(serde_json::to_string(&rows).unwrap())
                    },
                );
            }
            Pending::Depth { symbol, generation } => {
                let Some(state) = self
                    .depth
                    .get_mut(&symbol)
                    .filter(|s| s.generation == generation)
                else {
                    return;
                };
                let Ok(raw) = serde_json::from_str::<Snapshot>(response.body) else {
                    self.book_retry(cx, &symbol);
                    return;
                };
                if !raw.success
                    || raw.timestamp <= 0.0
                    || !raw
                        .bids
                        .iter()
                        .chain(&raw.asks)
                        .all(|l| l.price > 0.0 && l.quantity >= 0.0)
                {
                    self.book_retry(cx, &symbol);
                    return;
                }
                if !matches!(state.sync, Sync::Buffer(_)) {
                    return;
                }
                let Sync::Buffer(buffer) =
                    std::mem::replace(&mut state.sync, Sync::Live(raw.timestamp))
                else {
                    unreachable!()
                };
                state.book.clear();
                for (side, levels) in [(BookSide::Bids, raw.bids), (BookSide::Asks, raw.asks)] {
                    for l in levels {
                        state.book.apply(side, l.price, l.quantity);
                    }
                }
                let mut last = raw.timestamp;
                for b in buffer {
                    if b.time > raw.timestamp && b.time >= last {
                        last = b.time;
                        Self::apply(&mut state.book, b.delta);
                    }
                }
                state.sync = Sync::Live(last);
                cx.book_changed(&symbol);
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Identity(command) => self.identity_failed(cx, command, error),
            Pending::History(request) => self.history_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Markets { .. } | Pending::Tickers { .. } => {
                self.seed_attempts += 1;
                cx.log(LogLevel::Error, error);
                if self.seed_attempts <= 3 {
                    self.seed_retry = Some((
                        cx.set_timer(1000.0 * self.seed_attempts as f64),
                        matches!(pending, Pending::Tickers { .. }),
                    ));
                }
            }
            Pending::Depth { symbol, generation } => {
                if self
                    .depth
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation)
                {
                    self.book_retry(cx, &symbol);
                }
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Funding { .. } => cx.log(LogLevel::Warning, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trade_timer(cx, timer) || self.history_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if self.seed_retry.is_some_and(|(t, _)| t == timer) {
            let (_, tickers) = self.seed_retry.take().unwrap();
            self.seed(cx, tickers);
            return;
        }
        if self.funding == Some(timer) {
            self.funding_read(cx);
            self.funding = Some(cx.set_timer(60_000.0));
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect_public(cx);
            return;
        }
        if self.public.watchdog == Some(timer) {
            if cx.now - self.public.received >= 20_000.0 {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006);
                }
            } else {
                if let Some(s) = self.public.socket {
                    self.public.ping_at = cx.now;
                    cx.send(s, r#"{"event":"ping"}"#);
                }
                self.public.watchdog = Some(cx.set_timer(6000.0));
            }
            return;
        }
        let retry = self.depth.iter().find_map(|(s, d)| {
            if matches!(d.sync,Sync::Retry(t) if t==timer) {
                Some(s.clone())
            } else {
                None
            }
        });
        if let Some(symbol) = retry {
            self.book_request(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        if self.account_command(cx, &command) {
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => (),
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => {
                let Some(state) = self.depth.get_mut(&symbol) else {
                    return;
                };
                state.count = state.count.saturating_sub(1);
                if state.count > 0 {
                    return;
                }
                if let Some(d) = self.depth.remove(&symbol)
                    && let Sync::Retry(t) = d.sync
                {
                    cx.clear_timer(t);
                }
                if let Some(m) = cx.store.market(&symbol) {
                    self.topic(cx, &format!("{}@orderbookupdate", native(m)), false);
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if interval_ms(&interval).is_none() {
                    return;
                }
                let Some(m) = cx.store.market(&symbol).map(native) else {
                    return;
                };
                let trade = self.candles.keys().any(|(s, _)| s == &symbol);
                let state = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                state.count += 1;
                if state.count > 1 {
                    return;
                }
                self.topic(cx, &format!("{m}@kline_{interval}"), true);
                if !trade && !self.options.public_data_only {
                    self.topic(cx, &format!("{m}@trades"), true);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                let Some(s) = self.candles.get_mut(&key) else {
                    return;
                };
                s.count = s.count.saturating_sub(1);
                if s.count > 0 {
                    return;
                }
                self.candles.remove(&key);
                if let Some(m) = cx.store.market(&symbol).map(native) {
                    self.topic(cx, &format!("{m}@kline_{interval}"), false);
                    if !self.options.public_data_only
                        && !self.candles.keys().any(|(s, _)| s == &symbol)
                    {
                        self.topic(cx, &format!("{m}@trades"), false);
                    }
                }
            }
            cmd @ Command::FetchCandles { .. } => self.fetch_candles(cx, cmd),
            Command::Query { command, query } => self.history_command(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.depth
            .get(symbol)
            .filter(|s| matches!(s.sync, Sync::Live(_)))
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.depth
                .iter()
                .filter(|(_, s)| matches!(s.sync, Sync::Live(_)))
                .map(|(s, d)| (s.as_str(), &d.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;
