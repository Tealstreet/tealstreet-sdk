//! Aster perpetuals: typed public streams and versioned REST on the sans-I/O engine.
mod account;
mod history;
mod request;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
    json, num,
};
use serde_json::value::RawValue;
use sign::{Credentials, Version};
use std::collections::{HashMap, VecDeque};
use wire::*;

const WATCHDOG_MS: f64 = 5_000.0;
const SILENCE_MS: f64 = 20_000.0;
const INTEREST_MS: f64 = 15_000.0;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub auth_user_address: Option<String>,
    pub auth_signer_address: Option<String>,
    pub auth_private_key: Option<String>,
    pub api_version: Option<Version>,
    pub read_key: Option<String>,
    pub read_secret: Option<String>,
    pub builder_address: Option<String>,
    pub builder_fee_rate_maker: Option<String>,
    pub builder_fee_rate_taker: Option<String>,
}

struct Urls {
    rest: &'static str,
    public_ws: &'static str,
}

impl Urls {
    fn new(testnet: bool) -> Self {
        if testnet {
            Self {
                rest: "https://fapi.asterdex-testnet.com",
                public_ws: "wss://fstream5.asterdex-testnet.com/stream",
            }
        } else {
            Self {
                rest: "https://fapi.asterdex.com",
                public_ws: "wss://fstream.asterdex.com/stream",
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct MarketRule {
    pub tick_size: f64,
    pub max_algo_orders: Option<u32>,
}

pub enum Pending {
    DepthRead {
        command: CommandId,
    },
    MarketBrackets {
        rows: HashMap<String, f64>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Signed(Box<request::SignedRequest>),
    Clock(request::ClockRequest),
    ExchangeInfo {
        rows: Vec<(Market, MarketRule)>,
        malformed: bool,
    },
    Daily {
        rows: Vec<Ticker>,
        malformed: bool,
    },
    OpenInterest {
        rows: HashMap<String, f64>,
        initial: bool,
        single: bool,
    },
    Klines {
        command: CommandId,
        rows: Vec<Candle>,
        malformed: bool,
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
    next_request_id: u64,
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    book: Book,
    last_id: Option<u64>,
}

pub struct AsterDex {
    options: Options,
    version: Version,
    credentials: Option<Credentials>,
    read_credentials: Option<Credentials>,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
    requests: request::RequestState,
    urls: Urls,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    market_rules: HashMap<String, MarketRule>,
    interest_timer: Option<TimerId>,
    interest_cursor: usize,
    interest_fallback: VecDeque<String>,
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

impl AsterDex {
    pub fn new(options: Options) -> Self {
        let user = nonempty(options.auth_user_address.as_deref())
            .or_else(|| nonempty(options.application_id.as_deref()));
        let signer = nonempty(options.auth_signer_address.as_deref())
            .or_else(|| nonempty(options.key.as_deref()));
        let secret = nonempty(options.auth_private_key.as_deref())
            .or_else(|| nonempty(options.secret.as_deref()));
        let version = options.api_version.unwrap_or_else(|| {
            if user.is_some_and(sign::is_address)
                && signer.is_some_and(sign::is_address)
                && secret.is_some_and(sign::is_private_key)
            {
                Version::V3
            } else {
                Version::V1
            }
        });
        let credentials = if !options.public_data_only {
            let secret = if version == Version::V3 {
                secret
            } else {
                options.secret.as_deref().filter(|value| !value.is_empty())
            };
            options
                .key
                .as_ref()
                .filter(|value| !value.is_empty())
                .zip(secret)
                .map(|(key, secret)| Credentials {
                    version,
                    key: key.clone(),
                    secret: secret.to_owned(),
                    user: user.map(str::to_owned),
                    signer: signer.map(str::to_owned),
                })
        } else {
            None
        };
        let read_credentials = options
            .read_key
            .as_ref()
            .zip(options.read_secret.as_ref())
            .map(|(key, secret)| Credentials {
                version,
                key: key.clone(),
                secret: nonempty(options.auth_private_key.as_deref())
                    .unwrap_or(secret)
                    .to_owned(),
                user: credentials.as_ref().and_then(|c| c.user.clone()),
                signer: Some(
                    nonempty(options.auth_signer_address.as_deref())
                        .unwrap_or(key)
                        .to_owned(),
                ),
            });
        Self {
            urls: Urls::new(options.testnet),
            options,
            version,
            credentials,
            read_credentials,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
            requests: request::RequestState::default(),
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            market_rules: HashMap::new(),
            interest_timer: None,
            interest_cursor: 0,
            interest_fallback: VecDeque::new(),
        }
    }

    pub fn is_private(&self) -> bool {
        !self.options.public_data_only && self.credentials.is_some()
    }

    fn path(&self, endpoint: &str) -> String {
        format!(
            "{}/{}",
            self.version.prefix(),
            endpoint.trim_start_matches('/')
        )
    }

    fn get(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request(
            Method::Get,
            format!("{}{path}", self.urls.rest),
            Vec::new(),
            None,
            pending,
        );
    }

    fn get_streamed(&self, cx: &mut Cx<'_, Pending>, path: &str, at: ArrayAt, pending: Pending) {
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.urls.rest),
            Vec::new(),
            at,
            pending,
        );
    }

    fn bootstrap(&self, cx: &mut Cx<'_, Pending>) {
        self.get_streamed(
            cx,
            &self.path("exchangeInfo"),
            ArrayAt::Field("symbols"),
            Pending::ExchangeInfo {
                rows: Vec::new(),
                malformed: false,
            },
        );
    }

    fn seed_daily(&self, cx: &mut Cx<'_, Pending>) {
        self.get_streamed(
            cx,
            &self.path("ticker/24hr"),
            ArrayAt::Root,
            Pending::Daily {
                rows: Vec::new(),
                malformed: false,
            },
        );
    }
    fn public_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.is_private() {
            self.start_account(cx);
        }
        self.connect_public(cx);
    }

    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() {
            return;
        }
        self.public.socket = Some(cx.connect_channel(self.urls.public_ws, SocketChannel::Public));
        self.public.open = false;
    }

    fn watch_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.watchdog = Some(cx.set_timer(WATCHDOG_MS));
    }

    fn close_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = None;
        self.public.open = false;
        self.public.attempts = self.public.attempts.saturating_add(1);
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer);
        }
        if self.public.reconnect.is_none() {
            let delay = (500.0 * 2f64.powi(self.public.attempts.min(6) as i32 - 1)).min(30_000.0);
            self.public.reconnect = Some(cx.set_timer(delay));
        }
    }

    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, method: &str, topics: &[String]) {
        let (Some(socket), true) = (self.public.socket, self.public.open) else {
            return;
        };
        if topics.is_empty() {
            return;
        }
        self.public.next_request_id += 1;
        let params = serde_json::to_string(topics).unwrap_or_default();
        cx.send(
            socket,
            format!(
                "{{\"method\":\"{method}\",\"params\":{params},\"id\":{}}}",
                self.public.next_request_id
            ),
        );
    }

    fn request_interest(&self, cx: &mut Cx<'_, Pending>, initial: bool, symbol: Option<&str>) {
        let mut path = self.path("openInterest");
        if let Some(symbol) = symbol {
            path.push_str(&format!("?symbol={symbol}"));
        }
        let pending = Pending::OpenInterest {
            rows: HashMap::new(),
            initial,
            single: symbol.is_some(),
        };
        if symbol.is_some() {
            self.get(cx, &path, pending);
        } else {
            self.get_streamed(cx, &path, ArrayAt::Envelope(&[&[], &["data"]]), pending);
        }
    }

    fn finish_interest(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut rows: HashMap<String, f64>,
        initial: bool,
        single: bool,
        response: Option<Response<'_>>,
    ) {
        if !response.is_some_and(|response| response.status == 200) {
            rows.clear();
        }
        if let Some(response) = response
            && response.status == 200
        {
            collect_interest(response.body, &mut rows);
        }
        for (symbol, value) in &rows {
            cx.store
                .update_ticker(symbol, |ticker| ticker.open_interest = *value);
        }
        if initial {
            self.public_ready(cx);
            self.interest_timer = Some(cx.set_timer(250.0));
            return;
        } else if rows.is_empty() && !single {
            let tickers = cx.store.tickers();
            let length = tickers.len();
            let count = length.min(25);
            for offset in 0..count {
                self.interest_fallback.push_back(
                    tickers[(self.interest_cursor + offset) % length]
                        .symbol
                        .clone(),
                );
            }
            if length > 0 {
                self.interest_cursor = (self.interest_cursor + count) % length;
            }
        }
        if let Some(symbol) = self.interest_fallback.pop_front() {
            self.request_interest(cx, false, Some(&symbol));
        } else {
            self.interest_timer = Some(cx.set_timer(INTEREST_MS));
        }
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if text.trim_start().starts_with('[') {
            let _ = json::each::<&RawValue>(text, |item| self.on_public_frame(cx, item.get()));
            return;
        }
        let Ok(envelope) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        if envelope.id.is_some() {
            return;
        }
        let stream = envelope.stream.as_deref();
        let body = envelope.data.map_or(text, RawValue::get);
        match stream {
            Some("!ticker@arr") => {
                let _ = json::each::<TickerEvent>(body, |event| update_ticker(cx, event));
            }
            Some("!markPrice@arr@1s" | "!markPrice@arr") => {
                let _ = json::each::<MarkEvent>(body, |event| update_mark(cx, event));
            }
            Some(stream) if is_partial_stream(stream) => self.on_depth(cx, stream, body),
            Some(stream) if stream.contains("@depth") => {}
            Some(stream) if stream.contains("@kline_") => self.on_kline(cx, body),
            Some(stream) if stream.ends_with("@aggTrade") => self.on_trade(cx, body),
            _ => match envelope.e.as_deref() {
                Some("24hrTicker") => {
                    if let Ok(event) = serde_json::from_str::<TickerEvent>(body) {
                        update_ticker(cx, event);
                    }
                }
                Some("markPriceUpdate") => {
                    if let Ok(event) = serde_json::from_str::<MarkEvent>(body) {
                        update_mark(cx, event);
                    }
                }
                Some("kline") => self.on_kline(cx, body),
                Some("aggTrade") => self.on_trade(cx, body),
                _ => {}
            },
        }
    }

    fn on_kline(&self, cx: &mut Cx<'_, Pending>, body: &str) {
        if let Ok(event) = serde_json::from_str::<KlineEvent>(body) {
            let k = event.k;
            if self
                .candles
                .contains_key(&(k.s.to_string(), k.i.to_string()))
            {
                cx.candle(&k.s, &k.i, candle(k.t, &k.o, &k.h, &k.l, &k.c, &k.v));
            }
        }
    }

    fn on_trade(&self, cx: &mut Cx<'_, Pending>, body: &str) {
        if let Ok(event) = serde_json::from_str::<TradeEvent>(body) {
            let price = num::parse(&event.p);
            cx.store
                .update_ticker(&event.s, |ticker| ticker.last = price);
        }
    }

    fn on_depth(&mut self, cx: &mut Cx<'_, Pending>, stream: &str, body: &str) {
        let Ok(event) = serde_json::from_str::<PartialDepth>(body) else {
            return;
        };
        let symbol = stream
            .split('@')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if event.s.as_deref().is_some_and(|s| s != symbol) {
            return;
        }
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if event
            .u
            .zip(state.last_id)
            .is_some_and(|(next, last)| next <= last)
        {
            return;
        }
        let levels = |rows: Vec<[std::borrow::Cow<'_, str>; 2]>| -> Option<Vec<(f64, f64)>> {
            rows.into_iter()
                .map(|[price, size]| {
                    let price: f64 = price.trim().parse().ok()?;
                    let size: f64 = size.trim().parse().ok()?;
                    (price.is_finite() && size.is_finite() && price > 0.0 && size > 0.0)
                        .then_some((price, size))
                })
                .collect()
        };
        let (Some(mut bids), Some(mut asks)) = (levels(event.b), levels(event.a)) else {
            return;
        };
        if bids.is_empty() || asks.is_empty() {
            return;
        }
        bids.sort_by(|a, b| b.0.total_cmp(&a.0));
        asks.sort_by(|a, b| a.0.total_cmp(&b.0));
        if bids[0].0 >= asks[0].0 {
            return;
        }
        cx.store.update_ticker(&symbol, |ticker| {
            ticker.bid = bids[0].0;
            ticker.ask = asks[0].0;
        });
        state.book.load_sorted_owned(BookSide::Bids, bids);
        state.book.load_sorted_owned(BookSide::Asks, asks);
        state.last_id = event.u.or(state.last_id);
        cx.book_changed(&symbol);
    }
}

fn is_partial_stream(stream: &str) -> bool {
    let Some((_, depth)) = stream.split_once('@') else {
        return false;
    };
    matches!(
        depth,
        "depth5"
            | "depth10"
            | "depth20"
            | "depth5@100ms"
            | "depth5@250ms"
            | "depth5@500ms"
            | "depth10@100ms"
            | "depth10@250ms"
            | "depth10@500ms"
            | "depth20@100ms"
            | "depth20@250ms"
            | "depth20@500ms"
    )
}

fn update_ticker(cx: &mut Cx<'_, Pending>, event: TickerEvent<'_>) {
    cx.store.update_ticker(&event.s, |ticker| {
        if let Some(value) = event.c.as_deref().filter(|s| !s.is_empty()) {
            ticker.last = num::parse(value);
        }
        if let Some(value) = event.percentage.as_deref().filter(|s| !s.is_empty()) {
            ticker.percentage = num::parse(value);
        }
        if let Some(value) = event.v.as_deref().filter(|s| !s.is_empty()) {
            ticker.volume = num::parse(value);
        }
        if let Some(value) = event.q.as_deref().filter(|s| !s.is_empty()) {
            ticker.quote_volume = num::parse(value);
        }
    });
}

fn update_mark(cx: &mut Cx<'_, Pending>, event: MarkEvent<'_>) {
    cx.store.update_ticker(&event.s, |ticker| {
        if let Some(value) = event.p.as_deref().filter(|s| !s.is_empty()) {
            ticker.mark = num::parse(value);
        }
        if let Some(value) = event.i.as_deref().filter(|s| !s.is_empty()) {
            ticker.index = num::parse(value);
        }
        if let Some(value) = event.r.as_deref().filter(|s| !s.is_empty()) {
            ticker.funding_rate = num::parse(value);
        }
        if let Some(value) = event.funding_time.filter(|value| *value != 0.0) {
            ticker.funding_time = value;
        }
    });
}

fn collect_interest(body: &str, output: &mut HashMap<String, f64>) {
    if let Ok(row) = serde_json::from_str::<Interest>(body) {
        let value = serde_json::from_str::<f64>(row.open_interest.get())
            .ok()
            .or_else(|| {
                serde_json::from_str::<String>(row.open_interest.get())
                    .ok()?
                    .parse::<f64>()
                    .ok()
            });
        if let Some(value) = value.filter(|value| value.is_finite()) {
            output.insert(row.symbol.into_owned(), value);
        }
    } else if body.trim_start().starts_with('[') {
        let _ = json::each::<&RawValue>(body, |row| collect_interest(row.get(), output));
    } else if let Ok(envelope) = serde_json::from_str::<InterestEnvelope>(body) {
        collect_interest(envelope.data.get(), output);
    }
}

fn candle_interval(value: &str) -> &str {
    match value {
        "1m" | "3m" | "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "6h" | "12h" | "1d" | "1w" => {
            value
        }
        _ => "1h",
    }
}

fn candle(time: f64, o: &str, h: &str, l: &str, c: &str, v: &str) -> Candle {
    Candle {
        timestamp: time / 1000.0,
        open: num::parse(o),
        high: num::parse(h),
        low: num::parse(l),
        close: num::parse(c),
        volume: num::parse(v),
    }
}

fn market_from_wire(row: SymbolInfo<'_>) -> (Market, MarketRule) {
    let find = |kind: &str| row.filters.iter().find(|filter| filter.filter_type == kind);
    let lot = find("LOT_SIZE");
    let price = 10f64.powi(-row.price_precision);
    let rule = MarketRule {
        tick_size: find("PRICE_FILTER")
            .and_then(|filter| filter.tick_size.as_deref())
            .map(num::parse)
            .unwrap_or(price),
        max_algo_orders: find("MAX_NUM_ALGO_ORDERS").and_then(|filter| filter.limit),
    };
    let market = Market {
        id: row.symbol.to_string(),
        symbol: row.symbol.into_owned(),
        pretty_symbol: None,
        base: row.base_asset.into_owned(),
        quote: row.quote_asset.into_owned(),
        active: true,
        precision: Precision {
            price,
            amount: 10f64.powi(-row.quantity_precision),
        },
        limits: Limits {
            amount: MinMax {
                min: lot
                    .and_then(|filter| filter.min_qty.as_deref())
                    .map(num::parse)
                    .unwrap_or(0.0),
                max: lot
                    .and_then(|filter| filter.max_qty.as_deref())
                    .map(num::parse)
                    .unwrap_or(0.0),
            },
            leverage: MinMax {
                min: 1.0,
                max: 10.0,
            },
            price: None,
        },
        contract_size: Some(1.0),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    };
    (market, rule)
}

impl Venue for AsterDex {
    type Pending = Pending;

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(credentials) = &self.credentials
            && let Err(error) = credentials.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.start_requests(cx);
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.on_account_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        self.watch_public(cx);
        cx.send(socket, "{\"id\":-1,\"method\":\"LIST_SUBSCRIPTIONS\"}");
        let mut topics = vec!["!ticker@arr".to_owned(), "!markPrice@arr@1s".to_owned()];
        for (symbol, state) in &mut self.books {
            state.last_id = None;
            topics.push(format!("{}@depth20@100ms", symbol.to_ascii_lowercase()));
        }
        for (symbol, interval) in self.candles.keys() {
            topics.push(format!("{}@kline_{interval}", symbol.to_ascii_lowercase()));
        }
        self.subscribe(cx, "SUBSCRIBE", &topics);
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.on_account_message(cx, socket, text) {
            return;
        }
        if self.public.socket == Some(socket) {
            self.public.last_received = cx.now;
            self.on_public_frame(cx, text);
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.on_account_close(cx, socket) {
            return;
        }
        if self.public.socket == Some(socket) {
            self.close_public(cx);
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::DepthRead { command } => {
                #[derive(serde::Serialize)]
                struct Level {
                    price: f64,
                    amount: f64,
                    total: f64,
                }
                #[derive(serde::Serialize)]
                struct Output {
                    bids: Vec<Level>,
                    asks: Vec<Level>,
                }
                let result = if let Some((_, error)) = account::venue_error(&response) {
                    Err(error)
                } else {
                    serde_json::from_str::<safe_cex_venue_binance::wire::DepthSnapshot>(
                        response.body,
                    )
                    .map_err(|e| e.to_string())
                    .and_then(|depth| {
                        let levels = |raw: Vec<[std::borrow::Cow<'_, str>; 2]>,
                                      bid: bool|
                         -> Result<Vec<Level>, String> {
                            let mut levels = raw
                                .into_iter()
                                .map(|l| {
                                    Ok(Level {
                                        price: l[0].parse::<f64>().map_err(|e| e.to_string())?,
                                        amount: l[1].parse::<f64>().map_err(|e| e.to_string())?,
                                        total: 0.0,
                                    })
                                })
                                .collect::<Result<Vec<_>, String>>()?;
                            if levels.iter().any(|l| {
                                !l.price.is_finite()
                                    || !l.amount.is_finite()
                                    || l.price <= 0.0
                                    || l.amount < 0.0
                            }) {
                                return Err("Malformed Aster depth level".into());
                            }
                            levels.sort_by(|a, b| {
                                if bid {
                                    b.price.total_cmp(&a.price)
                                } else {
                                    a.price.total_cmp(&b.price)
                                }
                            });
                            let mut total = 0.0;
                            for l in &mut levels {
                                total += l.price * l.amount;
                                l.total = total;
                            }
                            Ok(levels)
                        };
                        serde_json::to_string(&Output {
                            bids: levels(depth.bids, true)?,
                            asks: levels(depth.asks, false)?,
                        })
                        .map_err(|e| e.to_string())
                    })
                };
                cx.done(command, result);
            }
            Pending::MarketBrackets { rows } => {
                if account::venue_error(&response).is_none() {
                    cx.store.update_markets(|m| {
                        if let Some(max) = rows.get(&m.symbol) {
                            m.limits.leverage.max = *max;
                        }
                    });
                }
                self.seed_daily(cx);
            }
            Pending::Signed(req) => self.on_signed_response(cx, req, Ok(response)),
            Pending::Clock(req) => self.on_clock_response(cx, req, Ok(response)),
            Pending::Account(req) => self.on_account_response(cx, req, Ok(response)),
            Pending::Trade(req) => self.on_trade_response(cx, req, Ok(response)),
            Pending::History(req) => self.on_history_response(cx, req, Ok(response)),
            Pending::ExchangeInfo { rows, malformed } => {
                if response.status != 200 || malformed {
                    cx.log(LogLevel::Error, "Aster market bootstrap failed");
                    return;
                }
                let mut markets = Vec::with_capacity(rows.len());
                for (market, rule) in rows {
                    self.market_rules.insert(market.symbol.clone(), rule);
                    markets.push(market);
                }
                cx.store.set_markets(markets);
                let path = self.path("leverageBracket");
                let pending = Pending::MarketBrackets {
                    rows: HashMap::new(),
                };
                if self.is_private() {
                    self.signed_streamed(cx, &path, sign::Query::new(), pending);
                } else {
                    self.get_streamed(cx, &path, ArrayAt::Root, pending);
                }
            }
            Pending::Daily { rows, malformed } => {
                if response.status != 200 || malformed {
                    cx.log(LogLevel::Error, "Aster ticker bootstrap failed");
                    return;
                }
                cx.store.set_tickers(rows);
                self.request_interest(cx, true, None);
            }
            Pending::OpenInterest {
                rows,
                initial,
                single,
            } => self.finish_interest(cx, rows, initial, single, Some(response)),
            Pending::Klines {
                command,
                rows,
                malformed,
            } => {
                let result = if response.status == 200 && !malformed {
                    serde_json::to_string(&rows).map_err(|error| error.to_string())
                } else {
                    Err(format!(
                        "Aster klines HTTP {} or malformed payload",
                        response.status
                    ))
                };
                cx.done(command, result);
            }
        }
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::DepthRead { .. } => {}
            Pending::MarketBrackets { rows } => {
                if let Ok(row) =
                    serde_json::from_str::<safe_cex_venue_binance::wire::LeverageBracket>(item)
                    && let Some(bracket) = row.brackets.first()
                {
                    rows.insert(row.symbol.to_string(), bracket.initial_leverage);
                }
            }
            Pending::Signed(req) => self.on_signed_item(cx, req, item),
            Pending::Account(req) => self.on_account_item(cx, req, item),
            Pending::History(req) => self.on_history_item(cx, req, item),
            Pending::Clock(_) | Pending::Trade(_) => {}
            Pending::ExchangeInfo { rows, malformed } => {
                match serde_json::from_str::<SymbolInfo>(item) {
                    Ok(row) if row.status == "TRADING" => rows.push(market_from_wire(row)),
                    Ok(_) => {}
                    Err(_) => *malformed = true,
                }
            }
            Pending::Daily { rows, malformed } => match serde_json::from_str::<Daily>(item) {
                Ok(row) => rows.push(Ticker {
                    id: row.symbol.to_string(),
                    symbol: row.symbol.into_owned(),
                    last: num::parse(&row.last_price),
                    percentage: num::parse(&row.price_change_percent),
                    volume: num::parse(&row.volume),
                    quote_volume: num::parse(&row.quote_volume),
                    ..Ticker::default()
                }),
                Err(_) => *malformed = true,
            },
            Pending::OpenInterest { rows, .. } => collect_interest(item, rows),
            Pending::Klines {
                rows, malformed, ..
            } => match serde_json::from_str::<KlineRow>(item) {
                Ok(row) => rows.push(candle(row.0, &row.1, &row.2, &row.3, &row.4, &row.5)),
                Err(_) => *malformed = true,
            },
        }
    }

    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if let Pending::Signed(req) = pending {
            self.on_signed_stream_error(cx, req, response);
            return;
        }
        if let Pending::OpenInterest {
            rows,
            initial,
            single,
        } = pending
        {
            self.finish_interest(cx, rows, initial, single, Some(response));
        } else {
            self.on_request_failed(cx, pending, "missing expected response list");
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::DepthRead { command } => cx.done(command, Err(error.into())),
            Pending::MarketBrackets { .. } => self.seed_daily(cx),
            Pending::Signed(req) => self.on_signed_response(cx, req, Err(error)),
            Pending::Clock(req) => self.on_clock_response(cx, req, Err(error)),
            Pending::Account(req) => self.on_account_response(cx, req, Err(error)),
            Pending::Trade(req) => self.on_trade_response(cx, req, Err(error)),
            Pending::History(req) => self.on_history_response(cx, req, Err(error)),
            Pending::OpenInterest {
                initial, single, ..
            } => self.finish_interest(cx, HashMap::new(), initial, single, None),
            Pending::Klines { command, .. } => cx.done(command, Err(error.to_owned())),
            _ => cx.log(
                LogLevel::Error,
                format!("Aster public request failed: {error}"),
            ),
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.on_request_timer(cx, timer)
            || self.on_account_timer(cx, timer)
            || self.on_history_timer(cx, timer)
        {
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect_public(cx);
        }
        if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if let Some(socket) = self.public.socket {
                if cx.now - self.public.last_received > SILENCE_MS {
                    cx.close(socket);
                    self.close_public(cx);
                } else {
                    cx.send(socket, "{\"id\":-1,\"method\":\"LIST_SUBSCRIPTIONS\"}");
                    self.watch_public(cx);
                }
            }
        }
        if self.interest_timer == Some(timer) {
            self.interest_timer = None;
            self.request_interest(cx, false, None);
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.subscribe(
                        cx,
                        "SUBSCRIBE",
                        &[format!("{}@depth20@100ms", symbol.to_ascii_lowercase())],
                    );
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(
                            cx,
                            "UNSUBSCRIBE",
                            &[format!("{}@depth20@100ms", symbol.to_ascii_lowercase())],
                        );
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let interval = candle_interval(&interval).to_owned();
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1 {
                    self.subscribe(
                        cx,
                        "SUBSCRIBE",
                        &[format!("{}@kline_{interval}", symbol.to_ascii_lowercase())],
                    );
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let interval = candle_interval(&interval).to_owned();
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        self.subscribe(
                            cx,
                            "UNSUBSCRIBE",
                            &[format!("{}@kline_{}", key.0.to_ascii_lowercase(), key.1)],
                        );
                    }
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
                let interval = candle_interval(&interval);
                let mut path = format!(
                    "{}?symbol={symbol}&interval={interval}&limit={}",
                    self.path("klines"),
                    limit.unwrap_or(500)
                );
                if let Some(from) = from_ms {
                    path.push_str(&format!("&startTime={}", num::format(from)));
                }
                if let Some(to) = to_ms {
                    path.push_str(&format!("&endTime={}", num::format(to)));
                }
                self.get_streamed(
                    cx,
                    &path,
                    ArrayAt::Root,
                    Pending::Klines {
                        command,
                        rows: Vec::new(),
                        malformed: false,
                    },
                );
            }
            Command::FetchOrderBook {
                command,
                symbol,
                limit,
            } => {
                let path = format!(
                    "{}?{}",
                    self.path("depth"),
                    sign::Query::new()
                        .str("symbol", &symbol)
                        .num("limit", f64::from(limit.unwrap_or(100)))
                        .as_str()
                );
                self.get(cx, &path, Pending::DepthRead { command });
            }
            Command::FetchAccount { command } => self.fetch_account_info(cx, command),
            Command::FetchLeverage { command, symbol } => {
                let result = cx
                    .store
                    .setting(&symbol)
                    .filter(|s| s.leverage.is_finite() && s.leverage > 0.0)
                    .map(|s| {
                        format!(
                            "{{\"long\":{},\"short\":{}}}",
                            num::format(s.leverage),
                            num::format(s.leverage)
                        )
                    })
                    .ok_or_else(|| format!("Aster leverage is not loaded for {symbol}"));
                cx.done(command, result);
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            command => self.on_trade_command(cx, command),
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|state| !state.book.is_empty())
            .map(|state| &state.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, state)| !state.book.is_empty())
                .map(|(symbol, state)| (symbol.as_str(), &state.book)),
        )
    }
}

fn reset_pending(pending: &mut Pending) {
    match pending {
        Pending::Account(account::Request::Positions {
            rows,
            settings,
            invalid,
            ..
        }) => {
            rows.clear();
            settings.clear();
            *invalid = false;
        }
        Pending::Account(account::Request::Orders { rows, invalid, .. }) => {
            rows.clear();
            *invalid = false;
        }
        Pending::History(req) => req.reset(),
        Pending::MarketBrackets { rows } => rows.clear(),
        _ => {}
    }
}
