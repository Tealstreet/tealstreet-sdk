//! Backpack perpetuals on the sans-I/O engine.
mod account;
mod history;
mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, Min, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use std::collections::HashMap;
use wire::*;

const REST: &str = "https://api.backpack.exchange";
const WS: &str = "wss://ws.backpack.exchange";
const MAX_BUFFER: usize = 2000;

#[derive(Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub recv_window: Option<f64>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        rows: Vec<Market>,
        invalid: bool,
    },
    Daily {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Marks {
        rows: Vec<(String, [f64; 4])>,
        invalid: bool,
    },
    Depth {
        symbol: String,
        generation: u64,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    received: f64,
    ping_at: f64,
    attempts: u32,
}

struct Delta {
    first: u64,
    last: u64,
    bids: Vec<Level>,
    asks: Vec<Level>,
}

enum Sync {
    Waiting(Vec<Delta>),
    Live { last: u64, bridge: bool },
    Retry(TimerId, Vec<Delta>),
    Failed,
}

struct BookState {
    book: Book,
    sync: Sync,
    subscribers: u32,
    generation: u64,
    recoveries: u32,
}

#[derive(Default)]
struct Seed {
    daily: Option<Vec<Ticker>>,
    marks: Option<Vec<(String, [f64; 4])>>,
    retries: Vec<(TimerId, SeedRead)>,
    attempts: [u32; 3],
}

#[derive(Clone, Copy, PartialEq)]
enum SeedRead {
    Markets,
    Daily,
    Marks,
}

pub struct Backpack {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    seed: Seed,
    account: account::Account,
    trading: trading::Trading,
    generation: u64,
}

impl Backpack {
    fn seed_read(&self, cx: &mut Cx<'_, Pending>, kind: SeedRead) {
        let (path, pending) = match kind {
            SeedRead::Markets => (
                "/api/v1/markets",
                Pending::Markets {
                    rows: vec![],
                    invalid: false,
                },
            ),
            SeedRead::Daily => (
                "/api/v1/tickers",
                Pending::Daily {
                    rows: vec![],
                    invalid: false,
                },
            ),
            SeedRead::Marks => (
                "/api/v1/markPrices",
                Pending::Marks {
                    rows: vec![],
                    invalid: false,
                },
            ),
        };
        self.get(cx, path, Default::default(), pending, true);
    }

    fn retry_seed(&mut self, cx: &mut Cx<'_, Pending>, kind: SeedRead, error: &str) {
        cx.log(LogLevel::Warning, error);
        if self.seed.retries.iter().any(|(_, k)| *k == kind) {
            return;
        }
        let attempt = &mut self.seed.attempts[kind as usize];
        let delay = (1000u32 << (*attempt).min(5)).min(30000);
        *attempt = attempt.saturating_add(1);
        self.seed.retries.push((cx.set_timer(delay as f64), kind));
    }
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            seed: Seed::default(),
            account: account::Account::default(),
            trading: trading::Trading::default(),
            generation: 0,
        }
    }

    fn get(
        &self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        params: sign::Params,
        pending: Pending,
        stream: bool,
    ) {
        let query = sign::query(&params);
        let url = format!(
            "{REST}{path}{}",
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            }
        );
        let headers = vec![("X-Broker-Id", "1200".into())];
        if stream {
            cx.request_streamed(Method::Get, url, headers, ArrayAt::Root, pending);
        } else {
            cx.request(Method::Get, url, headers, None, pending);
        }
    }

    fn subscribe(&self, cx: &mut Cx<'_, Pending>, topics: Vec<String>, add: bool) {
        if !self.public.open || topics.is_empty() {
            return;
        }
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            method: &'a str,
            params: Vec<String>,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Subscription {
                method: if add { "SUBSCRIBE" } else { "UNSUBSCRIBE" },
                params: topics,
            })
            .unwrap(),
        );
    }

    fn finish_seed(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.seed.daily.is_none() || self.seed.marks.is_none() {
            return;
        }
        let mut tickers = self.seed.daily.take().unwrap();
        let marks: HashMap<_, _> = self.seed.marks.take().unwrap().into_iter().collect();
        for t in &mut tickers {
            if let Some(m) = marks.get(&t.symbol) {
                t.mark = m[0];
                t.index = m[1];
                t.funding_rate = m[2];
                t.funding_time = m[3];
            }
        }
        cx.store.set_tickers(tickers);
        self.public.socket = Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Public));
        self.account_start(cx);
        if self.options.public_data_only || self.options.key.as_ref().is_none_or(String::is_empty) {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Default::default());
        }
    }

    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        self.generation += 1;
        state.generation = self.generation;
        let prior = std::mem::replace(&mut state.sync, Sync::Failed);
        let buffer = match prior {
            Sync::Retry(_, buffer) | Sync::Waiting(buffer) => buffer,
            _ => vec![],
        };
        state.sync = Sync::Waiting(buffer);
        self.get(
            cx,
            "/api/v1/depth",
            [("symbol".into(), symbol.into())].into(),
            Pending::Depth {
                symbol: symbol.into(),
                generation: self.generation,
            },
            false,
        );
    }

    fn recover_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if matches!(state.sync, Sync::Failed) {
            return;
        }
        state.recoveries += 1;
        if state.recoveries > 3 {
            state.sync = Sync::Failed;
            state.book.clear();
            cx.book_changed(symbol);
            cx.log(
                LogLevel::Error,
                format!("Backpack depth recovery exhausted for {symbol}"),
            );
            return;
        }
        let prior = std::mem::replace(&mut state.sync, Sync::Failed);
        let buffer = match prior {
            Sync::Waiting(buffer) | Sync::Retry(_, buffer) => buffer,
            _ => vec![],
        };
        state.sync = Sync::Retry(cx.set_timer(1000.0), buffer);
    }

    fn apply(state: &mut BookState, delta: Delta) -> bool {
        let Sync::Live { last, bridge } = &mut state.sync else {
            return false;
        };
        if delta.last <= *last {
            return true;
        }
        let Some(expected) = last.checked_add(1) else {
            return false;
        };
        if delta.first > expected
            || (!*bridge && delta.first != expected)
            || delta.first > delta.last
        {
            return false;
        }
        for Level(p, q) in delta.bids {
            state.book.apply(BookSide::Bids, p, q);
        }
        for Level(p, q) in delta.asks {
            state.book.apply(BookSide::Asks, p, q);
        }
        *last = delta.last;
        *bridge = false;
        true
    }

    fn depth(&mut self, cx: &mut Cx<'_, Pending>, event: DepthEvent<'_>) {
        let Some(state) = self.books.get_mut(event.s.as_ref()) else {
            return;
        };
        let delta = Delta {
            first: event.first,
            last: event.last,
            bids: event.b,
            asks: event.a,
        };
        match &mut state.sync {
            Sync::Waiting(buffer) | Sync::Retry(_, buffer) if buffer.len() < MAX_BUFFER => {
                buffer.push(delta);
            }
            Sync::Waiting(_) | Sync::Retry(_, _) => {
                state.sync = Sync::Failed;
                state.book.clear();
                cx.book_changed(&event.s);
                cx.log(
                    LogLevel::Error,
                    format!("Backpack depth buffer overflow for {}", event.s),
                );
            }
            Sync::Live { .. } => {
                if delta.last
                    <= match state.sync {
                        Sync::Live { last, .. } => last,
                        _ => unreachable!(),
                    }
                {
                    return;
                }
                let valid = match state.sync {
                    Sync::Live { last, bridge } => last.checked_add(1).is_some_and(|expected| {
                        delta.first <= expected
                            && (bridge || delta.first == expected)
                            && delta.first <= delta.last
                    }),
                    _ => false,
                };
                if valid {
                    Self::apply(state, delta);
                    cx.book_changed(&event.s);
                } else {
                    state.sync = Sync::Waiting(vec![delta]);
                    self.recover_book(cx, &event.s);
                }
            }
            Sync::Failed => {}
        }
    }

    fn public_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        stream: &str,
        text: &str,
    ) -> Result<(), String> {
        let topic = stream.split('.').next().unwrap_or("");
        match topic {
            "depth" => self.depth(cx, serde_json::from_str(text).map_err(|e| e.to_string())?),
            "ticker" => {
                let d: TickerEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&d.s, |t| {
                    t.last = d.c;
                    if let Some(change) = d
                        .percentage
                        .map(|n| n.0)
                        .or_else(|| d.o.filter(|n| n.0 > 0.0).map(|n| (d.c - n.0) / n.0))
                    {
                        t.percentage = change * 100.0;
                    }
                    t.volume = d.v;
                    t.quote_volume = d.q;
                });
            }
            "trade" => {
                let d: TradeEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&d.s, |t| t.last = d.p);
            }
            "bookTicker" => {
                let d: BookEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&d.s, |t| {
                    t.bid = d.b;
                    t.ask = d.a;
                });
            }
            "kline" => {
                let d: KlineEvent<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
                if let (Some(o), Some(h), Some(l), Some(c), Some(v)) = (d.o, d.h, d.l, d.c, d.v) {
                    let interval = stream.split('.').nth(1).unwrap_or("");
                    if self
                        .candles
                        .contains_key(&(d.s.to_string(), interval.to_owned()))
                    {
                        cx.candle(
                            &d.s,
                            interval,
                            Candle {
                                timestamp: timestamp(&d.t)?,
                                open: finite(&o)?,
                                high: finite(&h)?,
                                low: finite(&l)?,
                                close: finite(&c)?,
                                volume: finite(&v)?,
                            },
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Venue for Backpack {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = sign::Credentials::new(read.key.clone(), &read.secret)
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.seed_read(cx, SeedRead::Markets);
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
        self.public.attempts = 0;
        self.public.received = cx.now;
        self.public.ping_at = cx.now;
        self.public.watchdog = Some(cx.set_timer(5000.0));
        let topics = cx
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
            .chain(self.books.keys().map(|s| format!("depth.{s}")))
            .chain(self.candles.keys().map(|(s, i)| format!("kline.{i}.{s}")))
            .collect();
        self.subscribe(cx, topics, true);
        cx.send(socket, "{\"method\":\"PING\"}");
        let symbols: Vec<_> = self.books.keys().cloned().collect();
        for s in symbols {
            self.books.get_mut(&s).unwrap().recoveries = 0;
            self.snapshot(cx, &s);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.socket.socket == Some(socket) {
            if let Err(e) = self.private_message(cx, text) {
                cx.log(LogLevel::Error, e);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.received = cx.now;
        let result = (|| {
            let envelope: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            if let Some(error) = envelope.error {
                if error.code == 4002 {
                    cx.store.set_latency((cx.now - self.public.ping_at) / 2.0);
                    return Ok(());
                }
                return Err(format!("Backpack socket {}: {}", error.code, error.message));
            }
            if let Some(data) = envelope.data {
                self.public_message(cx, &envelope.stream, data.get())?;
            }
            Ok(())
        })();
        if let Err(e) = result {
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
            state.generation += 1;
            state.sync = Sync::Waiting(vec![]);
        }
        self.public.attempts += 1;
        self.public.reconnect = Some(
            cx.set_timer((1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30000.0)),
        );
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        let result = match pending {
            Pending::History(request) => {
                self.history_item(cx, request, item);
                return;
            }
            Pending::Account(request) => {
                self.account_item(cx, request, item);
                return;
            }
            Pending::Markets { rows, invalid } => {
                let r = serde_json::from_str::<MarketRow<'_>>(item).map(|r| {
                    if r.visible == Some(false)
                        || r.market_type != "PERP"
                        || !r.symbol.contains('_')
                    {
                        return;
                    }
                    let mut split = r.symbol.split('_');
                    let base = split.next().unwrap_or("").to_owned();
                    let quote = split.next().unwrap_or("").to_owned();
                    let max = r.imf_function.map_or(1.0, |f| {
                        let n = safe_cex_engine::num::parse(&f.base);
                        if n > 0.0 { (1.0 / n).round() } else { 1.0 }
                    });
                    rows.push(Market {
                        id: r.symbol.to_string(),
                        symbol: r.symbol.into_owned(),
                        pretty_symbol: None,
                        base,
                        quote,
                        active: true,
                        precision: Precision {
                            amount: r
                                .quantity_increment
                                .or(r.filters.quantity.step_size)
                                .map_or(0.001, |n| n.0),
                            price: r
                                .price_increment
                                .or(r.filters.price.tick_size)
                                .map_or(0.01, |n| n.0),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: r
                                    .min_quantity
                                    .or(r.filters.quantity.min_quantity)
                                    .map_or(0.0, |n| n.0),
                                max: r
                                    .max_quantity
                                    .or(r.filters.quantity.max_quantity)
                                    .map_or(0.0, |n| n.0),
                            },
                            leverage: MinMax { min: 1.0, max },
                            price: Some(Min {
                                min: r.filters.price.min_price.map_or(0.0, |n| n.0),
                            }),
                        },
                        contract_size: Some(1.0),
                        is_expiring: Some(false),
                        is_inverse: Some(false),
                        is_spot: Some(false),
                        is_perpetual: Some(true),
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                });
                if r.is_err() {
                    *invalid = true;
                }
                r
            }
            Pending::Daily { rows, invalid } => {
                let r = serde_json::from_str::<Daily<'_>>(item).map(|d| {
                    if cx.store.market(&d.symbol).is_some() {
                        rows.push(Ticker {
                            id: d.symbol.to_string(),
                            symbol: d.symbol.into_owned(),
                            last: d.last_price,
                            mark: d.last_price,
                            index: d.last_price,
                            percentage: d.price_change_percent * 100.0,
                            volume: d.volume,
                            quote_volume: d.quote_volume,
                            ..Default::default()
                        });
                    }
                });
                if r.is_err() {
                    *invalid = true;
                }
                r
            }
            Pending::Marks { rows, invalid } => {
                let r = serde_json::from_str::<Mark<'_>>(item).map(|d| {
                    rows.push((
                        d.symbol.into_owned(),
                        [
                            d.mark_price,
                            d.index_price,
                            d.funding_rate,
                            d.next_funding_timestamp,
                        ],
                    ))
                });
                if r.is_err() {
                    *invalid = true;
                }
                r
            }
            Pending::Candles { rows, invalid, .. } => {
                let r = serde_json::from_str::<Kline<'_>>(item)
                    .and_then(|d| candle(d).map_err(serde::de::Error::custom))
                    .map(|c| rows.push(c));
                if r.is_err() {
                    *invalid = true;
                }
                r
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            cx.log(LogLevel::Error, format!("Backpack decode: {e}"));
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::History(request) = pending {
            self.history_response(cx, request, response);
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if let Pending::Account(request) = pending {
            self.account_response(cx, request, response);
            return;
        }
        if !(200..300).contains(&response.status) {
            self.on_request_failed(
                cx,
                pending,
                &format!("Backpack HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        match pending {
            Pending::Markets {
                rows,
                invalid: false,
            } => {
                self.seed.attempts[SeedRead::Markets as usize] = 0;
                cx.store.set_markets(rows);
                self.get(
                    cx,
                    "/api/v1/tickers",
                    Default::default(),
                    Pending::Daily {
                        rows: vec![],
                        invalid: false,
                    },
                    true,
                );
                self.get(
                    cx,
                    "/api/v1/markPrices",
                    Default::default(),
                    Pending::Marks {
                        rows: vec![],
                        invalid: false,
                    },
                    true,
                );
            }
            Pending::Daily {
                rows,
                invalid: false,
            } => {
                self.seed.attempts[SeedRead::Daily as usize] = 0;
                self.seed.daily = Some(rows);
                self.finish_seed(cx);
            }
            Pending::Marks {
                rows,
                invalid: false,
            } => {
                self.seed.attempts[SeedRead::Marks as usize] = 0;
                self.seed.marks = Some(rows);
                self.finish_seed(cx);
            }
            Pending::Depth { symbol, generation } => {
                let Some(state) = self.books.get_mut(&symbol) else {
                    return;
                };
                if state.generation != generation || !matches!(state.sync, Sync::Waiting(_)) {
                    return;
                }
                let Ok(depth) = serde_json::from_str::<Depth>(response.body) else {
                    self.recover_book(cx, &symbol);
                    return;
                };
                let Sync::Waiting(buffer) = std::mem::replace(
                    &mut state.sync,
                    Sync::Live {
                        last: depth.last_update_id,
                        bridge: true,
                    },
                ) else {
                    unreachable!()
                };
                state.book.load_sorted_owned(BookSide::Bids, depth.bids);
                state.book.load_sorted_owned(BookSide::Asks, depth.asks);
                let mut iter = buffer.into_iter();
                let mut gap = None;
                while let Some(d) = iter.next() {
                    let valid = match state.sync {
                        Sync::Live { last, bridge } => {
                            d.last <= last
                                || last.checked_add(1).is_some_and(|expected| {
                                    d.first <= expected
                                        && (bridge || d.first == expected)
                                        && d.first <= d.last
                                })
                        }
                        _ => false,
                    };
                    if valid {
                        Self::apply(state, d);
                    } else {
                        gap = Some(std::iter::once(d).chain(iter).collect());
                        break;
                    }
                }
                if let Some(buffer) = gap {
                    state.sync = Sync::Waiting(buffer);
                    self.recover_book(cx, &symbol);
                } else {
                    state.recoveries = 0;
                    cx.book_changed(&symbol);
                }
            }
            Pending::Candles {
                command,
                rows,
                invalid: false,
            } => cx.done(command, Ok(serde_json::to_string(&rows).unwrap())),
            Pending::Candles { command, .. } => {
                cx.done(command, Err("Backpack invalid candles".into()))
            }
            Pending::Markets { .. } => {
                self.retry_seed(cx, SeedRead::Markets, "Backpack invalid markets")
            }
            Pending::Daily { .. } => {
                self.retry_seed(cx, SeedRead::Daily, "Backpack invalid tickers")
            }
            Pending::Marks { .. } => {
                self.retry_seed(cx, SeedRead::Marks, "Backpack invalid mark prices")
            }
            _ => cx.log(LogLevel::Error, "Backpack invalid REST list"),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(request) => cx.done(request.command, Err(error.into())),
            Pending::Depth { symbol, generation }
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|s| s.generation == generation) =>
            {
                self.recover_book(cx, &symbol)
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Markets { .. } => self.retry_seed(cx, SeedRead::Markets, error),
            Pending::Daily { .. } => self.retry_seed(cx, SeedRead::Daily, error),
            Pending::Marks { .. } => self.retry_seed(cx, SeedRead::Marks, error),
            _ => cx.log(LogLevel::Error, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(index) = self.seed.retries.iter().position(|(t, _)| *t == timer) {
            let (_, kind) = self.seed.retries.swap_remove(index);
            self.seed_read(cx, kind);
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.public.socket =
                Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Public));
        } else if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.received >= 20000.0 {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
            } else {
                if let Some(socket) = self.public.socket {
                    self.public.ping_at = cx.now;
                    cx.send(socket, "{\"method\":\"PING\"}");
                }
                self.public.watchdog = Some(cx.set_timer(5000.0));
            }
        } else if let Some(symbol) = self
            .books
            .iter()
            .find_map(|(s, b)| matches!(b.sync,Sync::Retry(t, _) if t==timer).then(|| s.clone()))
        {
            self.snapshot(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers += 1;
                    return;
                }
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
                if self.public.open {
                    self.subscribe(cx, vec![format!("depth.{symbol}")], true);
                    self.snapshot(cx, &symbol);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers -= 1;
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(cx, vec![format!("depth.{symbol}")], false);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1 {
                    self.subscribe(cx, vec![format!("kline.{interval}.{symbol}")], true);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        self.subscribe(cx, vec![format!("kline.{interval}.{symbol}")], false);
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
                let mut params: sign::Params =
                    [("symbol".into(), symbol), ("interval".into(), interval)].into();
                if let Some(from) = from_ms {
                    params.insert(
                        "startTime".into(),
                        format!("{:.0}", (from / 1000.0).floor()),
                    );
                }
                if let Some(to) = to_ms {
                    params.insert("endTime".into(), format!("{:.0}", (to / 1000.0).floor()));
                }
                self.get(
                    cx,
                    "/api/v1/klines",
                    params,
                    Pending::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                    },
                    true,
                );
            }
            Command::Query { command, query } => self.history_command(cx, command, query),
            command => self.trade_command(cx, command),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| matches!(s.sync, Sync::Live { .. } | Sync::Failed))
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

#[cfg(test)]
mod secondary_tests;
