//! Orderly perpetuals, shared by its six broker frontends.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Broker {
    #[default]
    Tealstreet,
    KodiakFi,
    Mode,
    SalsaDex,
    What,
    WooFi,
}
impl Broker {
    pub fn order_tag(self) -> &'static str {
        match self {
            Self::KodiakFi => "TEALKODIAK",
            Self::Mode => "TEALMODE",
            Self::SalsaDex => "TEALSALSA",
            Self::What => "TEALWHAT",
            Self::Tealstreet | Self::WooFi => "TEALSTREET",
        }
    }
}
#[derive(Default)]
pub struct Options {
    pub broker: Broker,
    pub public_data_only: bool,
    pub testnet: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub tick_interval: Option<f64>,
}

pub enum Pending {
    History(history::Request),
    Trade(trading::Request),
    Account(account::Request),
    Markets {
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
        from: Option<f64>,
        to: Option<f64>,
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
struct Delta {
    ts: u64,
    depth: Depth,
}
struct BookState {
    book: Book,
    subscribers: u32,
    generation: u64,
    last: Option<u64>,
    bridge: bool,
    buffer: Vec<Delta>,
    snapshot: Option<(String, TimerId)>,
    retry: Option<TimerId>,
}
#[derive(Clone, Copy)]
enum Seed {
    Markets,
    Tickers,
}

pub struct Orderly {
    options: Options,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    native: HashMap<String, String>,
    seed_retry: Option<(TimerId, Seed)>,
    seed_attempts: u32,
    generation: u64,
    account: account::Account,
    trading: trading::Trading,
    history: history::HistoryState,
}
impl Orderly {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            native: HashMap::new(),
            seed_retry: None,
            seed_attempts: 0,
            generation: 0,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            history: history::HistoryState::default(),
        }
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://testnet-api-evm.orderly.org"
        } else {
            "https://api-evm.orderly.org"
        }
    }
    fn public_url(&self) -> String {
        let host = if self.options.testnet {
            "testnet-ws-evm.orderly.org"
        } else {
            "ws-evm.orderly.org"
        };
        format!(
            "wss://{host}/ws/stream/{}",
            self.options
                .application_id
                .as_deref()
                .unwrap_or("OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY")
        )
    }
    fn seed(&self, cx: &mut Cx<'_, Pending>, kind: Seed) {
        let (path, pending) = match kind {
            Seed::Markets => (
                "/v1/public/info",
                Pending::Markets {
                    rows: vec![],
                    invalid: false,
                },
            ),
            Seed::Tickers => (
                "/v1/public/futures",
                Pending::Tickers {
                    rows: vec![],
                    invalid: false,
                },
            ),
        };
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.rest()),
            vec![],
            ArrayAt::Path(&["data", "rows"]),
            pending,
        );
    }
    fn retry_seed(&mut self, cx: &mut Cx<'_, Pending>, kind: Seed, error: &str) {
        cx.log(LogLevel::Warning, error);
        if self.seed_retry.is_none() {
            let t =
                cx.set_timer((1000.0 * 2f64.powi(self.seed_attempts.min(5) as i32)).min(30000.0));
            self.seed_attempts = self.seed_attempts.saturating_add(1);
            self.seed_retry = Some((t, kind));
        }
    }
    fn subscribe(&self, cx: &mut Cx<'_, Pending>, topic: &str, add: bool) {
        if !self.public.open {
            return;
        }
        #[derive(Serialize)]
        struct Sub<'a> {
            event: &'a str,
            topic: &'a str,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Sub {
                event: if add { "subscribe" } else { "unsubscribe" },
                topic,
            })
            .unwrap(),
        );
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.public.open {
            return;
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.snapshot.is_some() {
            return;
        }
        self.generation += 1;
        state.generation = self.generation;
        let id = format!("book-{}", state.generation);
        #[derive(Serialize)]
        struct Params<'a> {
            r#type: &'a str,
            symbol: &'a str,
        }
        #[derive(Serialize)]
        struct Request<'a> {
            id: &'a str,
            event: &'a str,
            params: Params<'a>,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Request {
                id: &id,
                event: "request",
                params: Params {
                    r#type: "orderbook",
                    symbol: &self.native[symbol],
                },
            })
            .unwrap(),
        );
        state.snapshot = Some((id, cx.set_timer(10000.0)));
    }
    fn reset_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(state) = self.books.get_mut(symbol) {
            if let Some((_, t)) = state.snapshot.take() {
                cx.clear_timer(t);
            }
            state.last = None;
            state.bridge = true;
            state.buffer.clear();
            if state.retry.is_none() {
                state.retry = Some(cx.set_timer(1000.0));
            }
        }
    }
    fn depth(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        ts: u64,
        depth: Depth,
    ) -> Result<(), String> {
        if !depth.valid(false)
            || ts == 0
            || depth.previous.is_none_or(|p| p >= ts)
            || depth
                .symbol
                .as_deref()
                .is_some_and(|n| self.native.get(symbol).map(String::as_str) != Some(n))
        {
            self.reset_book(cx, symbol);
            return Err("Orderly invalid depth update".into());
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return Ok(());
        };
        if let Some(last) = state.last {
            if ts > last {
                if Self::apply(state, Delta { ts, depth }) {
                    cx.book_changed(symbol);
                } else {
                    self.reset_book(cx, symbol);
                    return Err("Orderly book timestamp gap".into());
                }
            }
        } else {
            if state.buffer.len() >= 2000 {
                self.reset_book(cx, symbol);
                return Err("Orderly depth buffer overflow".into());
            }
            state.buffer.push(Delta { ts, depth });
            if state.retry.is_none() {
                self.snapshot(cx, symbol);
            }
        }
        Ok(())
    }
    fn apply(state: &mut BookState, d: Delta) -> bool {
        if state.last.is_some_and(|last| d.ts <= last) {
            return true;
        }
        if state.last.is_some_and(|last| {
            d.depth.previous.is_none_or(|prev| {
                if state.bridge {
                    prev > last
                } else {
                    prev != last
                }
            })
        }) {
            return false;
        }
        for (side, levels) in [
            (BookSide::Bids, d.depth.bids),
            (BookSide::Asks, d.depth.asks),
        ] {
            for (p, q) in levels {
                state.book.apply(side, p, q);
            }
        }
        state.last = Some(d.ts);
        state.bridge = false;
        true
    }
    fn snapshot_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        env: &Envelope<'_>,
    ) -> Result<(), String> {
        let symbol = self
            .books
            .iter()
            .find(|(_, s)| s.snapshot.as_ref().is_some_and(|(id, _)| id == &env.id))
            .map(|(s, _)| s.clone());
        let Some(symbol) = symbol else {
            return Ok(());
        };
        if env.success != Some(true) {
            self.reset_book(cx, &symbol);
            return Err(format!("Orderly book snapshot: {}", env.message));
        }
        let depth: Depth = match env
            .data
            .and_then(|data| serde_json::from_str(data.get()).ok())
        {
            Some(depth) => depth,
            None => {
                self.reset_book(cx, &symbol);
                return Err("Orderly invalid book snapshot".into());
            }
        };
        let ts = depth.ts.max(depth.timestamp);
        if !depth.valid(true)
            || ts == 0
            || depth.symbol.as_deref() != self.native.get(&symbol).map(String::as_str)
        {
            self.reset_book(cx, &symbol);
            return Err("Orderly invalid book snapshot".into());
        }
        let state = self.books.get_mut(&symbol).unwrap();
        if let Some((_, t)) = state.snapshot.take() {
            cx.clear_timer(t);
        }
        state.book.load_sorted_owned(BookSide::Bids, depth.bids);
        state.book.load_sorted_owned(BookSide::Asks, depth.asks);
        state.last = Some(ts);
        state.bridge = true;
        for d in std::mem::take(&mut state.buffer) {
            if !Self::apply(state, d) {
                self.reset_book(cx, &symbol);
                return Err("Orderly buffered timestamp gap".into());
            }
        }
        cx.book_changed(&symbol);
        Ok(())
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> Result<(), String> {
        let env: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        match env.event.as_ref() {
            "ping" => {
                cx.send(self.public.socket.unwrap(), "{\"event\":\"pong\"}");
                return Ok(());
            }
            "pong" => {
                cx.store.set_latency((cx.now - self.public.ping_at) / 2.0);
                return Ok(());
            }
            "request" => return self.snapshot_message(cx, &env),
            _ => {}
        }
        if env.success == Some(false) {
            return Err(format!("Orderly socket: {}", env.message));
        }
        let Some(data) = env.data else {
            if env.event.is_empty()
                && let Some((native, "orderbookupdate")) = env.topic.split_once('@')
                && let Some(symbol) = symbol(native)
                && self.books.contains_key(&symbol)
            {
                self.reset_book(cx, &symbol);
                return Err("Orderly missing depth data".into());
            }
            return Ok(());
        };
        match env.topic.as_ref() {
            topic @ ("tickers" | "bbos" | "markprices" | "estfundingrate") => {
                json::each::<LiveTicker<'_>>(data.get(), |r| {
                    if let Some(symbol) = symbol(&r.symbol) {
                        cx.store.update_ticker(&symbol, |t| match topic {
                            "tickers" => {
                                t.last = r.close;
                                t.volume = r.volume;
                                t.quote_volume = r.amount;
                            }
                            "bbos" => {
                                t.bid = r.bid;
                                t.ask = r.ask;
                            }
                            "markprices" => t.mark = r.price,
                            "estfundingrate" => {
                                t.funding_rate = r.funding_rate;
                                t.funding_time = r.funding_time;
                            }
                            _ => {}
                        });
                    }
                })
                .map_err(|e| e.to_string())?;
            }
            topic => {
                if let Some((native, suffix)) = topic.split_once('@')
                    && let Some(s) = symbol(native)
                {
                    if suffix == "orderbookupdate" && self.books.contains_key(&s) {
                        let d: Depth = match serde_json::from_str(data.get()) {
                            Ok(d) => d,
                            Err(e) => {
                                self.reset_book(cx, &s);
                                return Err(e.to_string());
                            }
                        };
                        self.depth(cx, &s, env.ts, d)?;
                    } else if let Some(interval) = suffix.strip_prefix("kline_")
                        && self.candles.contains_key(&(s.clone(), interval.into()))
                    {
                        let k: Kline =
                            serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                        cx.candle(&s, interval, k.candle()?);
                    }
                }
            }
        }
        Ok(())
    }
}
fn symbol(native: &str) -> Option<String> {
    let (base, quote) = native.strip_prefix("PERP_")?.rsplit_once('_')?;
    (!base.is_empty() && !quote.is_empty()).then(|| format!("{base}{quote}"))
}
fn interval_valid(i: &str) -> bool {
    matches!(
        i,
        "1m" | "5m" | "15m" | "30m" | "1h" | "4h" | "12h" | "1d" | "1w"
    )
}

impl Venue for Orderly {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.initialize_account(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket == Some(socket) {
            self.private_open(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.received = cx.now;
        self.public.ping_at = cx.now;
        self.public.watchdog = Some(cx.set_timer(10000.0));
        cx.send(socket, "{\"event\":\"ping\"}");
        for topic in ["tickers", "bbos", "markprices", "estfundingrate"] {
            self.subscribe(cx, topic, true);
        }
        self.books
            .retain(|symbol, _| self.native.contains_key(symbol));
        self.candles
            .retain(|(symbol, _), _| self.native.contains_key(symbol));
        for symbol in self.books.keys() {
            self.subscribe(
                cx,
                &format!("{}@orderbookupdate", self.native[symbol]),
                true,
            );
        }
        for (symbol, interval) in self.candles.keys() {
            self.subscribe(
                cx,
                &format!("{}@kline_{interval}", self.native[symbol]),
                true,
            );
        }
        let books: Vec<_> = self.books.keys().cloned().collect();
        for symbol in books {
            self.snapshot(cx, &symbol);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.socket.socket == Some(socket) {
            if let Err(e) = self.private_message(cx, text) {
                cx.log(LogLevel::Error, e);
            }
            self.trade_reconcile(cx);
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        self.public.received = cx.now;
        self.public.attempts = 0;
        if let Err(e) = self.public_message(cx, text) {
            cx.log(LogLevel::Error, e);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.socket.socket == Some(socket) {
            self.private_close(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = false;
        self.public.socket = None;
        if let Some(t) = self.public.watchdog.take() {
            cx.clear_timer(t);
        }
        for state in self.books.values_mut() {
            state.last = None;
            state.bridge = true;
            state.buffer.clear();
            if let Some((_, t)) = state.snapshot.take() {
                cx.clear_timer(t);
            }
            if let Some(t) = state.retry.take() {
                cx.clear_timer(t);
            }
        }
        self.public.attempts = self.public.attempts.saturating_add(1);
        self.public.reconnect = Some(
            cx.set_timer((1000.0 * 2f64.powi(self.public.attempts.min(6) as i32 - 1)).min(30000.0)),
        );
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Pending::Account(request) = pending {
            self.account_item(cx, request, item);
            return;
        }
        if let Pending::History(request) = pending {
            self.history_item(cx, request, item);
            return;
        }
        let result: Result<(), String> = (|| {
            match pending {
                Pending::Account(_) | Pending::Trade(_) | Pending::History(_) => unreachable!(),
                Pending::Markets { rows, .. } => {
                    let r: MarketRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let Some(s) = symbol(&r.symbol) else {
                        return Ok(());
                    };
                    let (base, quote) = r
                        .symbol
                        .strip_prefix("PERP_")
                        .unwrap()
                        .rsplit_once('_')
                        .unwrap();
                    if r.base_tick <= 0.0
                        || r.quote_tick <= 0.0
                        || r.base_min < 0.0
                        || r.base_max < r.base_min
                    {
                        return Err("Orderly invalid market limits".into());
                    }

                    let max = if r.base_imr > 0.0 {
                        (1.0 / r.base_imr).round()
                    } else if r.max_leverage > 0.0 {
                        r.max_leverage
                    } else {
                        100.0
                    };
                    rows.push(Market {
                        id: format!("{base}/{quote}:USDC"),
                        symbol: s,
                        pretty_symbol: None,
                        base: base.into(),
                        quote: quote.into(),
                        active: true,
                        precision: Precision {
                            amount: r.base_tick,
                            price: r.quote_tick,
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: r.base_min,
                                max: r.base_max,
                            },
                            leverage: MinMax {
                                min: if r.min_leverage > 0.0 {
                                    r.min_leverage
                                } else {
                                    1.0
                                },
                                max,
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
                Pending::Tickers { rows, .. } => {
                    let r: TickerRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    if let Some(m) = symbol(&r.symbol).and_then(|s| cx.store.market(&s)) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: r.close,
                            ask: r.close,
                            last: r.close,
                            mark: r.mark_price,
                            index: r.index_price,
                            percentage: if r.open > 0.0 {
                                safe_cex_engine::num::round_half_up(
                                    (r.close - r.open) / r.open * 10000.0,
                                ) / 100.0
                            } else {
                                0.0
                            },
                            open_interest: r.open_interest,
                            funding_rate: r.last_funding_rate,
                            funding_time: r.next_funding_time,
                            volume: r.volume,
                            quote_volume: r.amount,
                            mid_price: None,
                        });
                    }
                }
                Pending::Candles { rows, from, to, .. } => {
                    let r: Kline = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let c = r.candle()?;
                    if from.is_none_or(|t| c.timestamp * 1000.0 >= t)
                        && to.is_none_or(|t| c.timestamp * 1000.0 <= t)
                    {
                        rows.push(c);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            match pending {
                Pending::Markets { invalid, .. }
                | Pending::Tickers { invalid, .. }
                | Pending::Candles { invalid, .. } => *invalid = true,
                Pending::Account(_) | Pending::Trade(_) | Pending::History(_) => unreachable!(),
            }
            cx.log(LogLevel::Error, e);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            self.trade_reconcile(cx);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if let Pending::History(request) = pending {
            self.history_response(cx, request, response);
            return;
        }
        if let Err(e) = checked(response) {
            self.on_request_failed(cx, pending, &e);
            return;
        }
        match pending {
            Pending::Markets {
                rows,
                invalid: false,
            } => {
                self.native = rows
                    .iter()
                    .map(|m| (m.symbol.clone(), format!("PERP_{}_{}", m.base, m.quote)))
                    .collect();
                cx.store.set_markets(rows);
                self.seed_attempts = 0;
                self.seed(cx, Seed::Tickers);
            }
            Pending::Tickers {
                rows,
                invalid: false,
            } => {
                cx.store.set_tickers(rows);
                self.seed_attempts = 0;
                self.public.socket = Some(cx.connect(self.public_url()));
                self.account_start(cx);
            }
            Pending::Candles {
                command,
                mut rows,
                invalid: false,
                ..
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(command, Ok(serde_json::to_string(&rows).unwrap()));
            }
            invalid => self.on_request_failed(cx, invalid, "Orderly invalid response rows"),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(request) => self.history_failed(cx, request, error),
            Pending::Markets { .. } => self.retry_seed(cx, Seed::Markets, error),
            Pending::Tickers { .. } => self.retry_seed(cx, Seed::Tickers, error),
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.history_timer(cx, timer) {
            return;
        }
        if self.trade_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some((t, kind)) = self.seed_retry
            && t == timer
        {
            self.seed_retry = None;
            self.seed(cx, kind);
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.public.socket = Some(cx.connect(self.public_url()));
            return;
        }
        if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.received >= 20000.0 {
                let socket = self.public.socket.unwrap();
                cx.close(socket);
                self.on_close(cx, socket, 1006);
            } else {
                self.public.ping_at = cx.now;
                cx.send(self.public.socket.unwrap(), "{\"event\":\"ping\"}");
                self.public.watchdog = Some(cx.set_timer(10000.0));
            }
            return;
        }
        let timed = self
            .books
            .iter()
            .find(|(_, s)| s.snapshot.as_ref().is_some_and(|(_, t)| *t == timer))
            .map(|(s, _)| s.clone());
        if let Some(symbol) = timed {
            self.reset_book(cx, &symbol);
            return;
        }
        let retry = self
            .books
            .iter()
            .find(|(_, s)| s.retry == Some(timer))
            .map(|(s, _)| s.clone());
        if let Some(symbol) = retry {
            self.books.get_mut(&symbol).unwrap().retry = None;
            self.snapshot(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && !self.native.contains_key(&symbol) {
                    return;
                }
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_add(1);
                    return;
                }
                self.books.insert(
                    symbol.clone(),
                    BookState {
                        book: Book::default(),
                        subscribers: 1,
                        generation: 0,
                        last: None,
                        bridge: true,
                        buffer: vec![],
                        snapshot: None,
                        retry: None,
                    },
                );
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@orderbookupdate"), true);
                    self.snapshot(cx, &symbol);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers > 0 {
                        return;
                    }
                } else {
                    return;
                }
                let s = self.books.remove(&symbol).unwrap();
                if let Some((_, t)) = s.snapshot {
                    cx.clear_timer(t);
                }
                if let Some(t) = s.retry {
                    cx.clear_timer(t);
                }
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@orderbookupdate"), false);
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if (cx.store.loaded().markets && !self.native.contains_key(&symbol))
                    || !interval_valid(&interval)
                {
                    return;
                }
                let n = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *n = n.saturating_add(1);
                if *n == 1 && self.native.contains_key(&symbol) {
                    self.subscribe(
                        cx,
                        &format!("{}@kline_{interval}", self.native[&symbol]),
                        true,
                    );
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(n) = self.candles.get_mut(&key) {
                    *n = n.saturating_sub(1);
                    if *n > 0 {
                        return;
                    }
                } else {
                    return;
                }
                self.candles.remove(&key);
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@kline_{interval}"), false);
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(native) = self.native.get(&symbol) else {
                    cx.done(command, Err("Orderly unknown symbol".into()));
                    return;
                };
                if !interval_valid(&interval)
                    || limit == Some(0)
                    || [from_ms, to_ms]
                        .into_iter()
                        .flatten()
                        .any(|n| !n.is_finite() || !(0.0..=253402300799000.0).contains(&n))
                    || from_ms.zip(to_ms).is_some_and(|(a, b)| a > b)
                {
                    cx.done(command, Err("Orderly invalid candle parameters".into()));
                    return;
                }
                let path = format!(
                    "/v1/kline?symbol={native}&type={interval}&limit={}",
                    limit.unwrap_or(500).min(1000)
                );
                cx.request_streamed(
                    Method::Get,
                    format!("{}{path}", self.rest()),
                    vec![],
                    ArrayAt::Path(&["data", "rows"]),
                    Pending::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                        from: from_ms,
                        to: to_ms,
                    },
                );
            }
            Command::SetTickerSymbols { .. } => {}
            Command::Query { command, query } => self.history_command(cx, command, query),
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.last.is_some())
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.last.is_some())
                .map(|(s, state)| (s.as_str(), &state.book)),
        )
    }
}
