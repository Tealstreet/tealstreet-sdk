//! BingX stablecoin-margined perpetual swaps on the sans-I/O engine.
pub mod account;
pub mod account_wire;
mod history;
pub mod paths;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use serde::Deserialize;
use sign::{Credentials, Params};
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Markets {
        rows: Vec<Market>,
        valid: bool,
    },
    Daily {
        rows: Vec<Ticker>,
        valid: bool,
    },
    Premium {
        valid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        valid: bool,
    },
}

#[derive(Clone, Copy)]
enum List {
    Markets,
    Daily,
    Premium,
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
    last_received: f64,
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    book: Book,
    ready: bool,
}

pub struct Bingx {
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    credentials: Option<Credentials>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    seed: Vec<Ticker>,
    seed_index: Vec<Option<usize>>,
    seed_reported: Vec<bool>,
    sequence: u64,
    retries: HashMap<TimerId, List>,
    failures: [u32; 3],
}

impl Bingx {
    pub fn new(options: Options) -> Self {
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .key
                .zip(options.secret)
                .filter(|(k, s)| !k.is_empty() && !s.is_empty())
                .map(|(key, secret)| Credentials { key, secret })
        };
        Self {
            account: account::Account::default(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            credentials,
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            seed: Vec::new(),
            seed_index: Vec::new(),
            seed_reported: Vec::new(),
            sequence: 0,
            retries: HashMap::new(),
            failures: [0; 3],
        }
    }

    fn observe_orders(&mut self, cx: &Cx<'_, Pending>) {
        for (order, kind, side, attached, native_status) in self.account.drain_order_meta() {
            self.trading
                .observe_order(&order, &kind, &side, attached, &native_status);
        }
        self.trading.retain_open_orders(cx.store);
    }

    fn retry_list(&mut self, cx: &mut Cx<'_, Pending>, kind: List) {
        let slot = match kind {
            List::Markets => 0,
            List::Daily => 1,
            List::Premium => 2,
        };
        let delay = (1000. * 2f64.powi(self.failures[slot].min(5) as i32)).min(30_000.);
        self.failures[slot] += 1;
        self.retries.insert(cx.set_timer(delay), kind);
    }

    fn read(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        let (url, headers, _) =
            Params::new().into_request(Method::Get, path, self.credentials.as_ref(), cx.now);
        cx.request_streamed(
            Method::Get,
            url,
            headers,
            ArrayAt::EnvelopeField("data"),
            pending,
        );
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = Some(cx.connect(paths::PUBLIC_WS.to_owned()));
        self.public.open = false;
        self.public.last_received = cx.now;
        self.public.watchdog = Some(cx.set_timer(5000.));
    }

    fn send_topic(&mut self, cx: &mut Cx<'_, Pending>, topic: &str, subscribe: bool) {
        if !self.public.open {
            return;
        }
        if let Some(socket) = self.public.socket {
            self.sequence += 1;
            cx.send(
                socket,
                format!(
                    r#"{{"id":"{}","reqType":"{}","dataType":"{}"}}"#,
                    self.sequence,
                    if subscribe { "sub" } else { "unsub" },
                    topic
                ),
            );
        }
    }

    fn symbol_id(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        cx.store.market(symbol).map(|m| m.id.clone())
    }

    fn topic_book(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        Some(format!(
            "market.depth.{}.step0.level100",
            self.symbol_id(cx, symbol)?
        ))
    }
    fn topic_candle(&self, cx: &Cx<'_, Pending>, symbol: &str, interval: &str) -> Option<String> {
        Some(format!(
            "market.kline.{}.{}.trade.utc+8",
            self.symbol_id(cx, symbol)?,
            stream_interval(interval)?
        ))
    }

    fn reconnect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.public.socket.take() {
            cx.close(socket)
        }
        self.public.open = false;
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer)
        }
        for state in self.books.values_mut() {
            state.ready = false;
            state.book.clear()
        }
        if self.public.reconnect.is_none() {
            let delay = (1000. * 2f64.powi(self.public.attempts.min(5) as i32)).min(30_000.);
            self.public.attempts += 1;
            self.public.reconnect = Some(cx.set_timer(delay));
        }
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Some(socket) = self.public.socket else {
            return;
        };
        self.public.last_received = cx.now;
        if text == "Ping" {
            cx.send(socket, "Pong");
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.ping.is_some() {
            cx.send(socket, "Pong");
            return;
        }
        if message.pong.is_some() {
            return;
        }
        let Some(data) = message.data else { return };
        if message.data_type == "market.contracts" {
            let result = safe_cex_engine::json::each_in_field::<LiveTicker<'_>>(
                data.get(),
                "contracts",
                |row| {
                    let Some(market) = cx.store.market_by_id(&row.symbol) else {
                        return;
                    };
                    let symbol = market.symbol.clone();
                    let step = market.precision.price;
                    cx.store.update_ticker(&symbol, |t| {
                        if row.trade_price > 0. {
                            t.last = row.trade_price;
                            (t.bid, t.ask) = touch(row.trade_price, step)
                        }
                        t.mark = row.fair_price;
                        t.index = row.index_price;
                        t.funding_rate = row.funding_rate / 100.;
                        t.percentage = row.change_percentage.unwrap_or_default();
                        t.volume = row.volume.unwrap_or_default();
                        t.quote_volume = row.value.unwrap_or_default();
                        t.funding_time = 0.;
                    });
                },
            );
            if result.is_err() {
                cx.log(LogLevel::Warning, "invalid BingX ticker frame")
            }
            return;
        }
        if let Some(rest) = message.data_type.strip_prefix("market.depth.") {
            let Some(id) = rest.strip_suffix(".step0.level100") else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let Ok(Depth { bids, asks }) = serde_json::from_str::<Depth>(data.get()) else {
                return;
            };
            if let Some(state) = self.books.get_mut(&symbol) {
                let mut bids: Vec<_> = bids.into_iter().map(|l| (l.price, l.volume)).collect();
                let mut asks: Vec<_> = asks.into_iter().map(|l| (l.price, l.volume)).collect();
                bids.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
                asks.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                state.book.load_sorted(BookSide::Bids, bids);
                state.book.load_sorted(BookSide::Asks, asks);
                state.ready = true;
                cx.book_changed(&symbol);
            }
        } else if let Some(rest) = message.data_type.strip_prefix("market.kline.") {
            let mut split = rest.split('.');
            let (Some(id), Some(native_interval)) = (split.next(), split.next()) else {
                return;
            };
            let Some(m) = cx.store.market_by_id(id) else {
                return;
            };
            let symbol = m.symbol.clone();
            let matches: Vec<_> = self
                .candles
                .keys()
                .filter(|(s, i)| s == &symbol && stream_interval(i) == Some(native_interval))
                .cloned()
                .collect();
            let Ok(data) = serde_json::from_str::<CandleData>(data.get()) else {
                return;
            };
            if let Some(row) = data.kline_infos_vo.into_iter().next() {
                let candle = Candle::from(row);
                for (symbol, interval) in matches {
                    cx.candle(&symbol, &interval, candle)
                }
            }
        }
    }
}

fn touch(last: f64, step: f64) -> (f64, f64) {
    if last <= 0. {
        return (0., 0.);
    }
    // Decimal arithmetic at the venue's tick precision avoids binary subtraction noise.
    let factor = 10f64.powi(num::decimals(step) as i32);
    (
        ((last * factor - step * factor).round() / factor).max(0.),
        (last * factor + step * factor).round() / factor,
    )
}

fn stream_interval(interval: &str) -> Option<&'static str> {
    Some(match interval {
        "1m" => "1min",
        "3m" => "3min",
        "5m" => "5min",
        "15m" => "15min",
        "30m" => "30min",
        "1h" => "1hour",
        "2h" => "2hour",
        "3h" => "3hour",
        "4h" => "4hour",
        "6h" => "6hour",
        "12h" => "12hour",
        "1d" => "1day",
        "1w" => "1week",
        "1M" => "1month",
        "3M" => "3month",
        "1Y" => "1year",
        _ => return None,
    })
}

fn interval_ms(interval: &str) -> Option<f64> {
    stream_interval(interval)?;
    let unit = interval.chars().last()?;
    let count: f64 = interval[..interval.len() - 1].parse().ok()?;
    Some(
        count
            * match unit {
                'm' => 60_000.,
                'h' => 3_600_000.,
                'd' => 86_400_000.,
                'w' => 604_800_000.,
                'M' => 2_592_000_000.,
                'Y' => 31_536_000_000.,
                _ => return None,
            },
    )
}

pub(crate) fn success(response: Response<'_>) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Envelope {
        code: i64,
        #[serde(default)]
        msg: String,
    }
    if !(200..300).contains(&response.status) {
        return Err(format!("BingX HTTP {}: {}", response.status, response.body));
    }
    let envelope: Envelope =
        serde_json::from_str(response.body).map_err(|e| format!("invalid BingX response: {e}"))?;
    if envelope.code == 0 {
        Ok(())
    } else {
        Err(format!("BingX {}: {}", envelope.code, envelope.msg))
    }
}

impl Venue for Bingx {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.read(
            cx,
            paths::MARKETS,
            Pending::Markets {
                rows: Vec::new(),
                valid: true,
            },
        );
        if self.credentials.is_none() {
            cx.store.set_balance(Default::default());
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_open(cx, c);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer)
        }
        self.public.watchdog = Some(cx.set_timer(5000.));
        self.send_topic(cx, "market.contracts", true);
        let topics: Vec<_> = self
            .books
            .keys()
            .filter_map(|s| self.topic_book(cx, s))
            .chain(
                self.candles
                    .keys()
                    .filter_map(|(s, i)| self.topic_candle(cx, s, i)),
            )
            .collect();
        for topic in topics {
            self.send_topic(cx, &topic, true)
        }
        cx.send(socket, "Ping");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_message(cx, text, c);
            }
            self.observe_orders(cx);
        } else if self.public.socket == Some(socket) {
            self.public_message(cx, text)
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.handles_socket(socket) {
            if let Some(c) = &self.credentials {
                self.account.on_close(cx, c);
            }
        } else if self.public.socket == Some(socket) {
            self.reconnect(cx)
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(request) => self.account.on_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history.on_item(cx, request, item),
            Pending::Markets { rows, valid } => {
                if rows.len() == rows.capacity() {
                    rows.reserve_exact(128)
                }
                match serde_json::from_str::<Contract<'_>>(item) {
                    Ok(m) => rows.push(Market {
                        id: m.symbol.to_string(),
                        symbol: m.symbol.replace('-', ""),
                        pretty_symbol: None,
                        base: m.asset.into_owned(),
                        quote: m.currency.into_owned(),
                        active: m.status == 1.,
                        precision: Precision {
                            amount: 10f64.powi(m.quantity_precision as i32).recip(),
                            price: 10f64.powi(m.price_precision as i32).recip(),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: m.trade_min_limit,
                                max: f64::INFINITY,
                            },
                            leverage: MinMax {
                                min: 1.,
                                max: if m.max_long_leverage > 0. {
                                    m.max_long_leverage
                                } else {
                                    100.
                                },
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
                    }),
                    Err(_) => *valid = false,
                }
            }
            Pending::Daily { rows, valid } => match serde_json::from_str::<Daily<'_>>(item) {
                Ok(d) => {
                    if let Some(m) = cx.store.market_by_id(&d.symbol) {
                        let (bid, ask) = touch(d.last_price, m.precision.price);
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid,
                            ask,
                            last: d.last_price,
                            percentage: d.price_change_percent,
                            volume: d.volume,
                            quote_volume: d.quote_volume,
                            ..Ticker::default()
                        })
                    }
                }
                Err(_) => *valid = false,
            },
            Pending::Premium { valid } => match serde_json::from_str::<Premium<'_>>(item) {
                Ok(p) => {
                    let index = cx
                        .store
                        .market_by_id(&p.symbol)
                        .and_then(|m| cx.store.market_index(&m.symbol));
                    if let Some(i) = index.and_then(|index| self.seed_index[index as usize]) {
                        let t = &mut self.seed[i];
                        t.mark = p.mark_price;
                        t.index = p.index_price;
                        t.funding_rate = p.last_funding_rate / 100.;
                        t.funding_time = p.next_funding_time;
                        self.seed_reported[i] = true
                    }
                }
                Err(_) => *valid = false,
            },
            Pending::Candles { rows, valid, .. } => match serde_json::from_str::<Kline>(item) {
                Ok(k) => rows.push(k.into()),
                Err(_) => *valid = false,
            },
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let pending = match pending {
            Pending::History(request) => {
                self.history
                    .on_response(cx, self.credentials.as_ref(), request, response);
                return;
            }
            Pending::Trade(request) => {
                self.account.bump_orders();
                self.account.bump_positions();
                let settings = request.changes_settings() && success(response).is_ok();
                if request.places_order() && (200..300).contains(&response.status) {
                    self.account.accept_placed_response(cx, response.body);
                }
                self.trading
                    .on_response(cx, self.credentials.as_ref(), request, response);
                if settings && let Some(c) = &self.credentials {
                    self.account.reconcile(cx, c);
                }

                self.observe_orders(cx);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_response(cx, request, response, c);
                }
                self.observe_orders(cx);
                return;
            }
            pending => pending,
        };
        if let Err(error) = success(response) {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Markets {
                mut rows,
                valid: true,
            } => {
                rows.shrink_to_fit();
                cx.store.set_markets(rows);
                self.connect(cx);
                self.read(
                    cx,
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        valid: true,
                    },
                )
            }
            Pending::Daily { rows, valid: true } => {
                self.seed_index = vec![None; cx.store.markets().len()];
                for (i, ticker) in rows.iter().enumerate() {
                    if let Some(index) = cx.store.market_index(&ticker.symbol) {
                        self.seed_index[index as usize] = Some(i)
                    }
                }
                self.seed_reported = vec![false; rows.len()];
                self.seed = rows;
                self.read(cx, paths::PREMIUM, Pending::Premium { valid: true })
            }
            Pending::Premium { valid: true } => {
                self.seed_index = Vec::new();
                let reported = std::mem::take(&mut self.seed_reported);
                let mut index = 0;
                self.seed.retain(|_| {
                    let keep = reported[index];
                    index += 1;
                    keep
                });
                let rows = std::mem::take(&mut self.seed);
                cx.store.set_tickers(rows);
                if let Some(c) = &self.credentials {
                    self.account.start(cx, c);
                }
            }
            Pending::Candles {
                command,
                mut rows,
                valid: true,
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    serde_json::to_string(&rows).map_err(|e| e.to_string()),
                )
            }
            pending => self.on_request_failed(cx, pending, "malformed BingX list row"),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::History(request) => {
                self.history.on_failed(cx, request, error);
                return;
            }
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, self.credentials.as_ref(), request, error);
                return;
            }
            Pending::Account(request) => {
                if let Some(c) = &self.credentials {
                    self.account.on_request_failed(cx, request, error, c);
                }
                return;
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Markets { .. } => self.retry_list(cx, List::Markets),
            Pending::Daily { .. } => self.retry_list(cx, List::Daily),
            Pending::Premium { .. } => self.retry_list(cx, List::Premium),
        }
        cx.log(LogLevel::Error, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.history.on_timer(cx, self.credentials.as_ref(), timer);
        if self.account.handles_timer(timer) {
            if let Some(c) = &self.credentials {
                self.account.on_timer(cx, timer, c);
            }
            return;
        }
        if let Some(kind) = self.retries.remove(&timer) {
            let (path, pending) = match kind {
                List::Markets => (
                    paths::MARKETS,
                    Pending::Markets {
                        rows: Vec::new(),
                        valid: true,
                    },
                ),
                List::Daily => (
                    paths::DAILY,
                    Pending::Daily {
                        rows: Vec::with_capacity(cx.store.markets().len()),
                        valid: true,
                    },
                ),
                List::Premium => (paths::PREMIUM, Pending::Premium { valid: true }),
            };
            self.read(cx, path, pending);
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect(cx)
        } else if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.last_received >= 20_000. {
                self.reconnect(cx)
            } else {
                if self.public.open
                    && let Some(socket) = self.public.socket
                {
                    cx.send(socket, "Ping")
                }
                self.public.watchdog = Some(cx.set_timer(5000.))
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::Query { command, query } => {
                self.history
                    .query(cx, self.credentials.as_ref(), command, query)
            }
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1
                    && let Some(topic) = self.topic_book(cx, &symbol)
                {
                    self.send_topic(cx, &topic, true)
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers == 0 {
                        self.books.remove(&symbol);
                        if let Some(topic) = self.topic_book(cx, &symbol) {
                            self.send_topic(cx, &topic, false)
                        }
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if stream_interval(&interval).is_none() {
                    cx.log(LogLevel::Error, "unsupported BingX candle interval");
                    return;
                }
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1
                    && let Some(topic) = self.topic_candle(cx, &symbol, &interval)
                {
                    self.send_topic(cx, &topic, true)
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(count) = self.candles.get_mut(&key) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&key);
                        if let Some(topic) = self.topic_candle(cx, &symbol, &interval) {
                            self.send_topic(cx, &topic, false)
                        }
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
                let Some(market) = cx.store.market(&symbol) else {
                    cx.done(command, Err(format!("unknown market {symbol}")));
                    return;
                };
                let Some(step) = interval_ms(&interval) else {
                    cx.done(command, Err("invalid interval".into()));
                    return;
                };
                let end = to_ms.unwrap_or(cx.now);
                let start = if limit.is_none() {
                    from_ms.unwrap_or(end - step * 500.)
                } else {
                    end - step * f64::from(limit.unwrap_or(500).min(1500))
                };
                if !start.is_finite() || !end.is_finite() || start > end {
                    cx.done(command, Err("invalid candle time range".into()));
                    return;
                }
                let params = Params::new()
                    .str("symbol", &market.id)
                    .str("interval", &interval)
                    .num("startTime", start)
                    .num("endTime", end);
                let (url, headers, _) = params.into_request(
                    Method::Get,
                    paths::KLINES,
                    self.credentials.as_ref(),
                    cx.now,
                );
                cx.request_streamed(
                    Method::Get,
                    url,
                    headers,
                    ArrayAt::EnvelopeField("data"),
                    Pending::Candles {
                        command,
                        rows: Vec::new(),
                        valid: true,
                    },
                );
            }
            other => {
                self.account.bump_orders();
                self.account.bump_positions();
                if !self.trading.command(cx, self.credentials.as_ref(), other) {
                    cx.log(LogLevel::Warning, "unsupported BingX command");
                }
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.ready).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.ready)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}
