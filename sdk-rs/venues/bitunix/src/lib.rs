//! Bitunix USDT futures: sans-I/O public and account state machines.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::{
    Command, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, json, num,
};
use safe_cex_engine::{
    book::{Book, BookSide},
    model::*,
};
use std::collections::{HashMap, HashSet, VecDeque};
use wire::*;

const REST: &str = "https://fapi.bitunix.com";
const PUBLIC: &str = "wss://fapi.bitunix.com/public/";
const FUTURES: &str = "wss://api.bitunix.com/ws-futures/?from=contract&compress=2";
const PRIVATE: &str = "wss://fapi.bitunix.com/private/";
const MARKETS: &str = "/api/v1/futures/market/trading_pairs";
const TICKERS: &str = "/api/v1/futures/market/tickers";
const KLINE: &str = "/api/v1/futures/market/kline";

#[derive(Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}

pub enum Pending {
    Private(account::Read),
    Trade(trading::Request),
    History(history::Request),
    Markets { rows: Vec<Market>, invalid: bool },
    Tickers { rows: Vec<Ticker>, invalid: bool },
    Candles { command: u32 },
}
#[derive(Clone)]
enum Feed {
    Private,
    Futures,
    Public {
        channel: &'static str,
        symbols: Vec<String>,
    },
}
struct Socket {
    id: SocketId,
    feed: Feed,
    opened: bool,
    received: f64,
    pong_observed: bool,
    watchdog: Option<TimerId>,
    queue_timer: Option<TimerId>,
    queue: VecDeque<String>,
}

pub struct BitunixFutures {
    options: Options,
    credentials: Option<sign::Credentials>,
    account: account::AccountState,
    trading: trading::TradingState,
    sockets: Vec<Socket>,
    books: HashMap<String, Book>,
    candles: HashSet<(String, String)>,
    retry: Option<TimerId>,
}
impl BitunixFutures {
    pub fn new(options: Options) -> Self {
        Self {
            credentials: if options.public_data_only {
                None
            } else {
                options
                    .key
                    .as_ref()
                    .zip(options.secret.as_ref())
                    .filter(|(k, s)| !k.is_empty() && !s.is_empty())
                    .map(|(k, s)| sign::Credentials {
                        key: k.clone(),
                        secret: s.clone(),
                    })
            },
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            options,
            sockets: Vec::new(),
            books: HashMap::new(),
            candles: HashSet::new(),
            retry: None,
        }
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.request_streamed(
            Method::Get,
            format!("{REST}{MARKETS}"),
            self.headers(),
            json::ArrayAt::FieldEnvelope("data"),
            Pending::Markets {
                rows: Vec::new(),
                invalid: false,
            },
        );
    }
    fn headers(&self) -> Vec<(&'static str, String)> {
        if self.options.testnet {
            vec![("x-simulated-trading", "1".into())]
        } else {
            vec![]
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>, feed: Feed) {
        let url = match feed {
            Feed::Futures => FUTURES,
            Feed::Private => PRIVATE,
            Feed::Public { .. } => PUBLIC,
        };
        let id = cx.connect(url);
        self.sockets.push(Socket {
            id,
            feed,
            opened: false,
            received: cx.now,
            pong_observed: false,
            watchdog: Some(cx.set_timer(15_000.0)),
            queue_timer: None,
            queue: VecDeque::new(),
        });
    }
    fn futures_topic(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        let m = cx.store.market(symbol)?;
        Some(format!(
            "market_{}_depthop_{}",
            symbol.to_ascii_lowercase(),
            num::format(m.precision.price)
        ))
    }
    fn send_topic(&self, cx: &mut Cx<'_, Pending>, topic: &str, subscribe: bool) {
        if let Some(s) = self
            .sockets
            .iter()
            .find(|s| s.opened && matches!(s.feed, Feed::Futures))
        {
            cx.send(s.id, topic_frame(topic, subscribe));
        }
    }
    fn subscriptions(&self, cx: &Cx<'_, Pending>) -> Vec<String> {
        self.books
            .keys()
            .filter_map(|s| self.futures_topic(cx, s))
            .chain(
                self.candles.iter().map(|(s, i)| {
                    format!("market_{}_klineop_{}", s.to_ascii_lowercase(), interval(i))
                }),
            )
            .collect()
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, m: &Message<'_>) -> Result<(), String> {
        let _ = &m.op;
        let Some(raw) = m.data else {
            return Ok(());
        };
        let text = raw.get();
        match m.ch.as_ref() {
            "ticker" => {
                let d: Tick = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&m.symbol, |t| {
                    t.last = d.la.0;
                    t.percentage = (d.la.0 - d.o.0) / d.o.0 * 100.0;
                    t.volume = d.b.0;
                    t.quote_volume = d.q.0;
                });
            }
            "price" => {
                let d: Price = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&m.symbol, |t| {
                    t.mark = d.mp.0;
                    t.index = d.ip.0;
                    t.funding_rate = d.fr.0;
                    t.funding_time = d.ft.0;
                });
            }
            "depth_book1" => {
                let d: Top = serde_json::from_str(text).map_err(|e| e.to_string())?;
                cx.store.update_ticker(&m.symbol, |t| {
                    t.bid = d.b.first().map_or(0.0, |v| v[0].0);
                    t.ask = d.a.first().map_or(0.0, |v| v[0].0);
                });
            }
            _ => {}
        }
        if let Some(rest) = m.channel.strip_prefix("market_") {
            if let Some((symbol, _)) = rest.split_once("_depthop_") {
                let symbol = symbol.to_ascii_uppercase();
                if let Some(book) = self.books.get_mut(&symbol) {
                    let d: Depth = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    // Legacy depthop frames are complete sorted snapshots, without sequence ids.
                    book.load_sorted(BookSide::Bids, d.bids.into_iter().map(|v| (v[0].0, v[1].0)));
                    book.load_sorted(BookSide::Asks, d.asks.into_iter().map(|v| (v[0].0, v[1].0)));
                    cx.book_changed(&symbol);
                }
            } else if let Some((symbol, native)) = rest.split_once("_klineop_") {
                let symbol = symbol.to_ascii_uppercase();
                let intervals: Vec<_> = self
                    .candles
                    .iter()
                    .filter(|(s, i)| s == &symbol && interval(i) == native)
                    .map(|(_, i)| i.clone())
                    .collect();
                if !intervals.is_empty() {
                    let d: Kline = serde_json::from_str(text).map_err(|e| e.to_string())?;
                    cx.store.update_ticker(&symbol, |t| t.last = d.close.0);
                    for i in intervals {
                        cx.candle(&symbol, &i, candle(&d, false));
                    }
                }
            }
        }
        Ok(())
    }
}
fn topic_frame(topic: &str, subscribe: bool) -> String {
    format!(
        r#"{{"event":"{}","params":{{"channel":"{topic}"}}}}"#,
        if subscribe { "sub" } else { "unsub" }
    )
}
fn interval(i: &str) -> &str {
    match i {
        "1m" => "1min",
        "3m" => "3min",
        "5m" => "5min",
        "15m" => "15min",
        "30m" => "30min",
        "1h" => "60min",
        "1d" => "1day",
        "1w" => "1week",
        _ => i,
    }
}
fn candle(k: &Kline, rest: bool) -> Candle {
    Candle {
        timestamp: if rest { k.time.0 / 1000.0 } else { k.time.0 },
        open: k.open.0,
        high: k.high.0,
        low: k.low.0,
        close: k.close.0,
        volume: if rest { k.quote_vol.0 } else { k.volume.0 },
    }
}
impl Venue for BitunixFutures {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.markets(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let topics = self.subscriptions(cx);
        let Some(s) = self.sockets.iter_mut().find(|s| s.id == socket) else {
            return;
        };
        s.opened = true;
        s.received = cx.now;
        match &s.feed {
            Feed::Private => {}
            Feed::Futures => {
                for book in self.books.values_mut() {
                    book.clear();
                }
                for topic in ["system_notice", "reconnected", "onconnected"]
                    .into_iter()
                    .chain(topics.iter().map(String::as_str))
                {
                    cx.send(socket, topic_frame(topic, true));
                }
            }
            Feed::Public { channel, symbols } => {
                for batch in symbols.chunks(20) {
                    let args: Vec<_> = batch
                        .iter()
                        .map(|s| {
                            #[derive(serde::Serialize)]
                            struct Arg<'a> {
                                symbol: &'a str,
                                ch: &'a str,
                            }
                            Arg {
                                symbol: s,
                                ch: channel,
                            }
                        })
                        .collect();
                    s.queue.push_back(format!(
                        r#"{{"op":"subscribe","args":{}}}"#,
                        serde_json::to_string(&args).unwrap()
                    ));
                }
                if let Some(frame) = s.queue.pop_front() {
                    cx.send(socket, frame);
                    s.queue_timer = Some(cx.set_timer(250.0));
                }
            }
        }
        if matches!(s.feed, Feed::Private) {
            cx.send(socket, r#"{"op":"ping"}"#);
            self.login(cx, socket);
            if cx.store.loaded().positions {
                for axis in [
                    account::Axis::Balance,
                    account::Axis::Positions,
                    account::Axis::Orders,
                ] {
                    self.read_axis(cx, axis);
                }
            }
        } else {
            cx.send(socket, format!(r#"{{"ping":{}}}"#, num::format(cx.now)));
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(s) = self.sockets.iter_mut().find(|s| s.id == socket) else {
            return;
        };
        s.received = cx.now;
        match serde_json::from_str::<Message<'_>>(text) {
            Ok(m) => {
                if matches!(s.feed, Feed::Private) {
                    if m.op == "pong" {
                        s.pong_observed = true;
                    }
                    self.private_message(cx, &m);
                    return;
                }
                if let Err(e) = self.public_message(cx, &m) {
                    cx.log(LogLevel::Warning, e);
                }
            }
            Err(e) => cx.log(LogLevel::Warning, format!("invalid Bitunix frame: {e}")),
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if let Some(i) = self.sockets.iter().position(|s| s.id == socket) {
            let s = self.sockets.remove(i);
            if let Some(t) = s.watchdog {
                cx.clear_timer(t);
            }
            if let Some(t) = s.queue_timer {
                cx.clear_timer(t);
            }
            self.connect(cx, s.feed);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Private(read) => self.account_item(cx, read, item),
            Pending::History(read) => self.history_item(cx, read, item),
            Pending::Markets { rows, invalid } => match serde_json::from_str::<Pair<'_>>(item) {
                Ok(m) => {
                    if m.symbol_status == "OPEN"
                        && m.quote == "USDT"
                        && m.is_api_supported.as_ref().is_none_or(Supported::allowed)
                    {
                        rows.push(Market {
                            id: format!("{}/{}", m.base, m.quote),
                            symbol: m.symbol.into_owned(),
                            base: m.base.into_owned(),
                            quote: m.quote.into_owned(),
                            active: true,
                            precision: Precision {
                                amount: 10f64.powf(-m.base_precision.0),
                                price: 10f64.powf(-m.quote_precision.0),
                            },
                            limits: Limits {
                                amount: MinMax {
                                    min: 0.0,
                                    max: m.max_limit_order_volume.0,
                                },
                                leverage: MinMax {
                                    min: 1.0,
                                    max: 100.0,
                                },
                                price: None,
                            },
                            pretty_symbol: None,
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
                }
                Err(_) => *invalid = true,
            },
            Pending::Tickers { rows, invalid } => match serde_json::from_str::<Daily<'_>>(item) {
                Ok(t) => {
                    if let Some(m) = cx.store.market(&t.symbol) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: t.last.0,
                            ask: t.last.0,
                            last: t.last.0,
                            mark: t.mark_price.0,
                            index: t.mark_price.0,
                            percentage: if t.open.0 == 0.0 {
                                0.0
                            } else {
                                (t.last.0 - t.open.0) / t.open.0 * 100.0
                            },
                            volume: t.base_vol.0,
                            quote_volume: t.quote_vol.0,
                            ..Ticker::default()
                        });
                    }
                }
                Err(_) => *invalid = true,
            },
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let pending = match pending {
            Pending::History(read) => {
                self.history_response(cx, read, response);
                return;
            }
            Pending::Private(read) => {
                let after = read.after;
                let success = (200..300).contains(&response.status)
                    && data(response.body).is_ok()
                    && !read.invalid
                    && self.account.generations[read.axis.index()] == read.generation
                    && self.account.serials[match read.axis {
                        account::Axis::Balance => 0,
                        account::Axis::Positions => 1,
                        account::Axis::Orders => 2,
                        account::Axis::Tpsl => 3,
                    }] == read.serial;
                self.account_response(cx, read, response);
                if let Some(command) = after {
                    self.prepared(cx, command, success);
                }
                return;
            }
            Pending::Trade(request) => {
                self.trade_response(cx, request, response);
                return;
            }
            other => other,
        };
        if !(200..300).contains(&response.status) {
            self.on_request_failed(cx, pending, &format!("Bitunix HTTP {}", response.status));
            return;
        }
        if !matches!(pending, Pending::Candles { .. })
            && let Err(error) = data(response.body)
        {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Markets {
                rows,
                invalid: false,
            } => {
                cx.store.set_markets(rows);
                self.connect(cx, Feed::Futures);
                cx.request_streamed(
                    Method::Get,
                    format!("{REST}{TICKERS}"),
                    self.headers(),
                    json::ArrayAt::FieldEnvelope("data"),
                    Pending::Tickers {
                        rows: Vec::new(),
                        invalid: false,
                    },
                );
            }
            Pending::Tickers {
                rows,
                invalid: false,
            } => {
                cx.store.set_tickers(rows);
                self.private_start(cx);
                let symbols: Vec<_> = cx
                    .store
                    .markets()
                    .iter()
                    .map(|m| m.symbol.clone())
                    .collect();
                for batch in symbols.chunks(300) {
                    for channel in ["ticker", "price"] {
                        self.connect(
                            cx,
                            Feed::Public {
                                channel,
                                symbols: batch.to_vec(),
                            },
                        );
                    }
                }
            }
            Pending::Candles { command } => {
                let result = data(response.body).and_then(|text| {
                    let mut rows = Vec::new();
                    json::each::<Kline>(text, |k| rows.push(candle(&k, true)))
                        .map_err(|e| e.to_string())?;
                    serde_json::to_string(&rows).map_err(|e| e.to_string())
                });
                cx.done(command, result);
            }
            other => self.on_request_failed(cx, other, "malformed Bitunix list"),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, read, error),
            Pending::Private(read) => {
                if let Some(command) = read.after {
                    self.prepared(cx, command, false);
                }
                cx.log(LogLevel::Warning, error);
                self.debounce(cx, read.axis);
            }
            Pending::Candles { command } => cx.done(command, Err(error.into())),
            _ => {
                cx.log(LogLevel::Error, error);
                self.retry = Some(cx.set_timer(5_000.0));
            }
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.trade_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if self.retry == Some(timer) {
            self.retry = None;
            if cx.store.loaded().markets {
                cx.request_streamed(
                    Method::Get,
                    format!("{REST}{TICKERS}"),
                    self.headers(),
                    json::ArrayAt::FieldEnvelope("data"),
                    Pending::Tickers {
                        rows: Vec::new(),
                        invalid: false,
                    },
                );
            } else {
                self.markets(cx);
            }
            return;
        }
        if let Some(s) = self
            .sockets
            .iter_mut()
            .find(|s| s.queue_timer == Some(timer))
        {
            s.queue_timer = None;
            if let Some(frame) = s.queue.pop_front() {
                cx.send(s.id, frame);
                s.queue_timer = Some(cx.set_timer(250.0));
            }
            return;
        }
        if let Some(i) = self.sockets.iter().position(|s| s.watchdog == Some(timer)) {
            let s = &mut self.sockets[i];
            if cx.now - s.received >= 45_000.0
                && (!matches!(s.feed, Feed::Private) || !s.opened || s.pong_observed)
            {
                let id = s.id;
                cx.close(id);
                self.on_close(cx, id, 1006);
            } else {
                if s.opened {
                    if matches!(s.feed, Feed::Private) {
                        cx.send(s.id, r#"{"op":"ping"}"#);
                    } else {
                        cx.send(s.id, format!(r#"{{"ping":{}}}"#, num::format(cx.now)));
                    }
                }
                s.watchdog = Some(cx.set_timer(15_000.0));
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if self.books.contains_key(&symbol) {
                    return;
                }
                self.books.insert(symbol.clone(), Book::default());
                if let Some(t) = self.futures_topic(cx, &symbol) {
                    self.send_topic(cx, &t, true);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                self.books.remove(&symbol);
                if let Some(t) = self.futures_topic(cx, &symbol) {
                    self.send_topic(cx, &t, false);
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if self.candles.insert((symbol.clone(), i.clone())) {
                    self.send_topic(
                        cx,
                        &format!(
                            "market_{}_klineop_{}",
                            symbol.to_ascii_lowercase(),
                            interval(&i)
                        ),
                        true,
                    );
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                self.candles.remove(&(symbol.clone(), i.clone()));
                self.send_topic(
                    cx,
                    &format!(
                        "market_{}_klineop_{}",
                        symbol.to_ascii_lowercase(),
                        interval(&i)
                    ),
                    false,
                );
            }
            Command::FetchCandles {
                command,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let explicit_limit = limit;
                let limit = limit.unwrap_or(200).clamp(1, 200);
                let end = to_ms.unwrap_or(cx.now);
                let minutes = interval[..interval.len().saturating_sub(1)]
                    .parse::<f64>()
                    .unwrap_or(1.0);
                let factor = match interval.chars().last() {
                    Some('h') => 60.0,
                    Some('d') => 1440.0,
                    Some('w') => 10080.0,
                    _ => 1.0,
                };
                let derived = end - minutes * factor * 60_000.0 * f64::from(limit);
                let start = if explicit_limit.is_some() {
                    derived
                } else {
                    from_ms.unwrap_or(derived)
                };
                let limit_query = if explicit_limit.is_some() {
                    format!("&limit={limit}")
                } else {
                    String::new()
                };
                cx.request(Method::Get,format!("{REST}{KLINE}?symbol={symbol}&interval={interval}&startTime={start}&endTime={end}{limit_query}"),self.headers(),None,Pending::Candles {command});
            }
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trading_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| !b.is_empty())
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| !b.is_empty())
                .map(|(s, b)| (s.as_str(), b)),
        )
    }
}
