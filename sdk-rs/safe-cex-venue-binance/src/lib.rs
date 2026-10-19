//! Binance USD-M futures on the sans-I/O engine.
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{Command, CommandId, Cx, LogLevel, Method, SocketId, TimerId, Venue, num};
use std::collections::HashMap;
use wire::*;

const MAX_RECONNECT_DELAY_MS: f64 = 30_000.0;
const DEPTH_SNAPSHOT_LIMIT: u32 = 1000;

#[derive(Clone, Debug)]
pub struct Options {
    pub testnet: bool,
    /// Skip `!bookTicker` (high-frequency, only needed to trade).
    pub public_data_only: bool,
}

struct Urls {
    rest: &'static str,
    public_ws: &'static str,
    market_ws: &'static str,
}

impl Urls {
    fn for_options(options: &Options) -> Self {
        if options.testnet {
            Self {
                rest: "https://demo-fapi.binance.com",
                public_ws: "wss://demo-fstream.binance.com/public/ws",
                market_ws: "wss://demo-fstream.binance.com/market/ws",
            }
        } else {
            Self {
                rest: "https://fapi.binance.com",
                public_ws: "wss://fstream.binance.com/public/ws",
                market_ws: "wss://fstream.binance.com/market/ws",
            }
        }
    }
}

pub enum Pending {
    ExchangeInfo,
    Daily,
    BookTickers,
    PremiumIndex,
    DepthSnapshot { symbol: String, generation: u32 },
    Klines { command: CommandId },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Public,
    Market,
}

struct Connection {
    socket: Option<SocketId>,
    open: bool,
    attempts: u32,
    reconnect: Option<TimerId>,
    next_request_id: u64,
}

impl Connection {
    fn new() -> Self {
        Self {
            socket: None,
            open: false,
            attempts: 0,
            reconnect: None,
            next_request_id: 10,
        }
    }
}

/// A depth update held until the REST snapshot arrives.
struct BufferedDepth {
    first: u64,
    last: u64,
    previous: Option<u64>,
    bids: Vec<(f64, f64)>,
    asks: Vec<(f64, f64)>,
}

enum Sync {
    /// Waiting for the first diff before requesting a snapshot.
    Idle,
    Buffering(Vec<BufferedDepth>),
    /// `awaiting_first` until a diff has been applied on top of the snapshot:
    /// that first diff is checked by its id range, later ones by `pu`.
    Synced {
        last_applied: u64,
        awaiting_first: bool,
    },
    /// Backing off after a failed or discontinuous snapshot.
    Cooldown(TimerId),
}

struct BookState {
    subscribers: u32,
    book: Book,
    sync: Sync,
    generation: u32,
    failures: u32,
}

/// Diffs buffered while a snapshot is in flight; beyond this the snapshot is stale.
const MAX_BUFFERED_DIFFS: usize = 2_000;
const RATE_LIMIT_PAUSE_MS: f64 = 10_000.0;
const IP_BAN_PAUSE_MS: f64 = 120_000.0;
const MAX_SNAPSHOT_BACKOFF_MS: f64 = 60_000.0;

/// The three REST ticker sources, joined once all have arrived.
#[derive(Default)]
struct TickerSeed {
    daily: Option<HashMap<String, (f64, f64, f64, f64)>>,
    books: Option<Vec<(String, f64, f64)>>,
    premium: Option<HashMap<String, (f64, f64, f64, f64)>>,
}

pub struct BinanceFutures {
    options: Options,
    urls: Urls,
    public: Connection,
    market: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    ticker_seed: TickerSeed,
    /// Depth snapshots are paused until this time after a 418 or 429.
    rest_paused_until: f64,
}

impl BinanceFutures {
    pub fn new(options: Options) -> Self {
        let urls = Urls::for_options(&options);
        Self {
            options,
            urls,
            public: Connection::new(),
            market: Connection::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            ticker_seed: TickerSeed::default(),
            rest_paused_until: 0.0,
        }
    }

    fn connection(&mut self, channel: Channel) -> &mut Connection {
        match channel {
            Channel::Public => &mut self.public,
            Channel::Market => &mut self.market,
        }
    }

    fn channel_of(&self, socket: SocketId) -> Option<Channel> {
        if self.public.socket == Some(socket) {
            Some(Channel::Public)
        } else if self.market.socket == Some(socket) {
            Some(Channel::Market)
        } else {
            None
        }
    }

    fn get(&self, cx: &mut Cx<'_, Pending>, path: &str, pending: Pending) {
        cx.request(
            Method::Get,
            format!("{}{}", self.urls.rest, path),
            Vec::new(),
            None,
            pending,
        );
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: Channel) {
        let url = match channel {
            Channel::Public => self.urls.public_ws,
            Channel::Market => self.urls.market_ws,
        };
        let socket = cx.connect(url);
        let connection = self.connection(channel);
        connection.socket = Some(socket);
        connection.open = false;
    }

    fn send_subscribe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: Channel,
        method: &str,
        params: &[String],
    ) {
        let connection = self.connection(channel);
        let (Some(socket), true) = (connection.socket, connection.open) else {
            return;
        };
        if params.is_empty() {
            return;
        }
        connection.next_request_id += 1;
        let id = connection.next_request_id;
        let params = params
            .iter()
            .map(|p| format!("\"{p}\""))
            .collect::<Vec<_>>()
            .join(",");
        cx.send(
            socket,
            format!("{{\"method\":\"{method}\",\"params\":[{params}],\"id\":{id}}}"),
        );
    }

    fn depth_topics(&self) -> Vec<String> {
        self.books
            .keys()
            .map(|symbol| format!("{}@depth", symbol.to_lowercase()))
            .collect()
    }

    fn kline_topics(&self) -> Vec<String> {
        self.candles
            .keys()
            .map(|(symbol, interval)| format!("{}@kline_{interval}", symbol.to_lowercase()))
            .collect()
    }

    fn on_markets(&mut self, cx: &mut Cx<'_, Pending>, body: &str) {
        let info: ExchangeInfo = match serde_json::from_str(body) {
            Ok(info) => info,
            Err(error) => {
                cx.log(LogLevel::Error, format!("Binance exchangeInfo: {error}"));
                return;
            }
        };
        let markets = info
            .symbols
            .iter()
            .filter(|m| {
                matches!(&*m.contract_type, "PERPETUAL" | "TRADIFI_PERPETUAL")
                    && matches!(&*m.margin_asset, "USDT" | "USDC")
            })
            .map(market_from_wire)
            .collect();
        cx.store.set_markets(markets);
        self.get(cx, "/fapi/v1/ticker/24hr", Pending::Daily);
        self.get(cx, "/fapi/v1/ticker/bookTicker", Pending::BookTickers);
        self.get(cx, "/fapi/v1/premiumIndex", Pending::PremiumIndex);
    }

    fn try_seed_tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        let seed = &mut self.ticker_seed;
        let (Some(daily), Some(books), Some(premium)) = (&seed.daily, &seed.books, &seed.premium)
        else {
            return;
        };
        let tickers: Vec<Ticker> = books
            .iter()
            .filter_map(|(symbol, bid, ask)| {
                let market = cx.store.market(symbol)?;
                let &(last, percentage, volume, quote_volume) = daily.get(symbol)?;
                let &(mark, index, funding_rate, funding_time) = premium.get(symbol)?;
                Some(Ticker {
                    id: market.id.clone(),
                    symbol: market.symbol.clone(),
                    bid: *bid,
                    ask: *ask,
                    last,
                    mark,
                    index,
                    mid_price: None,
                    percentage,
                    open_interest: 0.0,
                    funding_rate,
                    funding_time,
                    volume,
                    quote_volume,
                })
            })
            .collect();
        cx.log(
            LogLevel::Info,
            format!(
                "Loaded {} Binance markets",
                tickers.len().min(cx.store.markets().len())
            ),
        );
        self.ticker_seed = TickerSeed::default();
        cx.store.set_tickers(tickers);
        if self.options.public_data_only {
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(Vec::new());
            cx.store.set_balance(Default::default());
        }
        self.connect(cx, Channel::Public);
        self.connect(cx, Channel::Market);
    }

    fn on_public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("bookTicker") => {
                if let Ok(event) = serde_json::from_str::<BookTickerEvent>(text) {
                    let (bid, ask) = (num::parse(&event.b), num::parse(&event.a));
                    cx.store.update_ticker(&event.s, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                }
            }
            Some("depthUpdate") => {
                if let Ok(event) = serde_json::from_str::<DepthEvent>(text) {
                    self.on_depth(cx, event);
                }
            }
            _ => {}
        }
    }

    fn on_market_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        match event_type(text) {
            Some("24hrTicker") => {
                if let Ok(events) = serde_json::from_str::<Vec<TickerEvent>>(text) {
                    for event in events {
                        let last = num::parse(&event.c);
                        let percentage = num::parse(&event.percentage);
                        let volume = num::parse(&event.v);
                        let quote_volume = num::parse(&event.q);
                        cx.store.update_ticker(&event.s, |t| {
                            t.last = last;
                            t.percentage = percentage;
                            t.volume = volume;
                            t.quote_volume = quote_volume;
                        });
                    }
                }
            }
            Some("markPriceUpdate") => {
                if let Ok(events) = serde_json::from_str::<Vec<MarkPriceEvent>>(text) {
                    for event in events {
                        let mark = num::parse(&event.p);
                        let index = num::parse(&event.i);
                        let funding_rate = num::parse(&event.r);
                        cx.store.update_ticker(&event.s, |t| {
                            t.mark = mark;
                            t.index = index;
                            t.funding_rate = funding_rate;
                            t.funding_time = event.funding_time;
                        });
                    }
                }
            }
            Some("kline") => {
                if let Ok(event) = serde_json::from_str::<KlineEvent>(text) {
                    let k = &event.k;
                    if self
                        .candles
                        .contains_key(&(k.s.to_string(), k.i.to_string()))
                    {
                        cx.candle(&k.s, &k.i, kline_candle(k.t, &k.o, &k.h, &k.l, &k.c, &k.v));
                    }
                }
            }
            _ => {}
        }
    }

    fn on_depth(&mut self, cx: &mut Cx<'_, Pending>, event: DepthEvent<'_>) {
        let paused = cx.now < self.rest_paused_until;
        let Some(state) = self.books.get_mut(event.s.as_ref()) else {
            return;
        };
        match &mut state.sync {
            Sync::Cooldown(_) => {}
            Sync::Idle if paused => {}
            Sync::Idle => {
                state.sync = Sync::Buffering(vec![buffered(&event)]);
                state.generation += 1;
                let generation = state.generation;
                let symbol = event.s.to_string();
                let path = format!("/fapi/v1/depth?symbol={symbol}&limit={DEPTH_SNAPSHOT_LIMIT}");
                self.get(cx, &path, Pending::DepthSnapshot { symbol, generation });
            }
            Sync::Buffering(updates) if updates.len() >= MAX_BUFFERED_DIFFS => {
                state.sync = Sync::Idle;
            }
            Sync::Buffering(updates) => updates.push(buffered(&event)),
            Sync::Synced {
                last_applied,
                awaiting_first,
            } => {
                let last = *last_applied;
                if event.final_update_id <= last {
                    return;
                }
                let continuous = if *awaiting_first {
                    event.first_update_id <= last + 1
                } else {
                    event.pu == Some(last)
                };
                if !continuous {
                    // Gap: drop the book; the next diff starts a fresh snapshot.
                    state.book.clear();
                    state.sync = Sync::Idle;
                    return;
                }
                for level in &event.b {
                    state
                        .book
                        .apply(BookSide::Bids, num::parse(&level[0]), num::parse(&level[1]));
                }
                for level in &event.a {
                    state
                        .book
                        .apply(BookSide::Asks, num::parse(&level[0]), num::parse(&level[1]));
                }
                state.sync = Sync::Synced {
                    last_applied: event.final_update_id,
                    awaiting_first: false,
                };
                cx.book_changed(&event.s);
            }
        }
    }

    /// Back off this book's next snapshot attempt exponentially.
    fn snapshot_failed(cx: &mut Cx<'_, Pending>, state: &mut BookState, min_delay_ms: f64) {
        state.book.clear();
        state.failures += 1;
        let backoff = (1_000.0 * 2f64.powi(state.failures as i32 - 1)).min(MAX_SNAPSHOT_BACKOFF_MS);
        state.sync = Sync::Cooldown(cx.set_timer(backoff.max(min_delay_ms)));
    }

    fn on_depth_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        generation: u32,
        status: u16,
        body: &str,
    ) {
        if matches!(status, 418 | 429) {
            let pause = if status == 418 {
                IP_BAN_PAUSE_MS
            } else {
                RATE_LIMIT_PAUSE_MS
            };
            self.rest_paused_until = self.rest_paused_until.max(cx.now + pause);
            cx.log(
                LogLevel::Warning,
                format!(
                    "Binance REST limited (HTTP {status}); pausing depth snapshots for {pause} ms"
                ),
            );
        }
        let pause_left = (self.rest_paused_until - cx.now).max(0.0);
        let Some(state) = self.books.get_mut(&symbol) else {
            return;
        };
        if state.generation != generation {
            return;
        }
        let snapshot = (status == 200)
            .then(|| serde_json::from_str::<DepthSnapshot>(body).ok())
            .flatten();
        let Some(snapshot) = snapshot else {
            Self::snapshot_failed(cx, state, pause_left);
            return;
        };
        let Sync::Buffering(updates) = std::mem::replace(&mut state.sync, Sync::Idle) else {
            return;
        };
        state.book.load_sorted(
            BookSide::Bids,
            snapshot
                .bids
                .iter()
                .map(|l| (num::parse(&l[0]), num::parse(&l[1]))),
        );
        state.book.load_sorted(
            BookSide::Asks,
            snapshot
                .asks
                .iter()
                .map(|l| (num::parse(&l[0]), num::parse(&l[1]))),
        );
        let mut last_applied = snapshot.last_update_id;
        let mut awaiting_first = true;
        for update in updates
            .into_iter()
            .filter(|u| u.last > snapshot.last_update_id)
        {
            let continuous = if awaiting_first {
                update.first <= last_applied + 1
            } else {
                update.previous == Some(last_applied)
            };
            if !continuous {
                Self::snapshot_failed(cx, state, pause_left);
                return;
            }
            awaiting_first = false;
            for (price, amount) in update.bids {
                state.book.apply(BookSide::Bids, price, amount);
            }
            for (price, amount) in update.asks {
                state.book.apply(BookSide::Asks, price, amount);
            }
            last_applied = update.last;
        }
        state.failures = 0;
        state.sync = Sync::Synced {
            last_applied,
            awaiting_first,
        };
        cx.book_changed(&symbol);
    }

    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let state = self
            .books
            .entry(symbol.clone())
            .or_insert_with(|| BookState {
                subscribers: 0,
                book: Book::default(),
                sync: Sync::Idle,
                generation: 0,
                failures: 0,
            });
        state.subscribers += 1;
        if state.subscribers == 1 {
            let topic = format!("{}@depth", symbol.to_lowercase());
            self.send_subscribe(cx, Channel::Public, "SUBSCRIBE", &[topic]);
        }
    }

    fn unsubscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        state.subscribers -= 1;
        if state.subscribers == 0 {
            self.books.remove(symbol);
            let topic = format!("{}@depth", symbol.to_lowercase());
            self.send_subscribe(cx, Channel::Public, "UNSUBSCRIBE", &[topic]);
        }
    }

    fn on_klines(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, status: u16, body: &str) {
        if status != 200 {
            cx.done(
                command,
                Err(format!("Binance klines HTTP {status}: {body}")),
            );
            return;
        }
        match serde_json::from_str::<Vec<KlineRow>>(body) {
            Ok(rows) => {
                let candles: Vec<Candle> = rows
                    .iter()
                    .map(|row| kline_candle(row.0, &row.1, &row.2, &row.3, &row.4, &row.5))
                    .collect();
                cx.done(
                    command,
                    Ok(serde_json::to_string(&candles).unwrap_or_default()),
                );
            }
            Err(error) => cx.done(command, Err(format!("Binance klines: {error}"))),
        }
    }
}

fn filter_value<'a>(filter: Option<&Filter<'a>>, pick: fn(&Filter<'a>) -> Option<f64>) -> f64 {
    filter.and_then(pick).unwrap_or(f64::NAN)
}

fn market_from_wire(m: &SymbolInfo<'_>) -> Market {
    let filter = |kind: &str| m.filters.iter().find(|f| f.filter_type == kind);
    let price = filter("PRICE_FILTER");
    let lot = filter("LOT_SIZE");
    let market_lot = filter("MARKET_LOT_SIZE");
    let min_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.min_qty.as_deref().map(num::parse);
    let max_qty: fn(&Filter<'_>) -> Option<f64> = |f| f.max_qty.as_deref().map(num::parse);
    Market {
        id: format!("{}/{}:{}", m.base_asset, m.quote_asset, m.margin_asset),
        symbol: m.symbol.to_string(),
        pretty_symbol: None,
        base: m.base_asset.to_string(),
        quote: m.quote_asset.to_string(),
        active: m.status == "TRADING",
        precision: Precision {
            amount: filter_value(lot, |f| f.step_size.as_deref().map(num::parse)),
            price: filter_value(price, |f| f.tick_size.as_deref().map(num::parse)),
        },
        limits: Limits {
            amount: MinMax {
                min: filter_value(lot, min_qty).max(filter_value(market_lot, min_qty)),
                max: filter_value(lot, max_qty).min(filter_value(market_lot, max_qty)),
            },
            leverage: MinMax { min: 1.0, max: 1.0 },
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
    }
}

fn kline_candle(open_time_ms: f64, o: &str, h: &str, l: &str, c: &str, v: &str) -> Candle {
    Candle {
        timestamp: open_time_ms / 1000.0,
        open: num::parse(o),
        high: num::parse(h),
        low: num::parse(l),
        close: num::parse(c),
        volume: num::parse(v),
    }
}

fn buffered(event: &DepthEvent<'_>) -> BufferedDepth {
    let levels = |side: &[[std::borrow::Cow<'_, str>; 2]]| {
        side.iter()
            .map(|l| (num::parse(&l[0]), num::parse(&l[1])))
            .collect()
    };
    BufferedDepth {
        first: event.first_update_id,
        last: event.final_update_id,
        previous: event.pu,
        bids: levels(&event.b),
        asks: levels(&event.a),
    }
}

impl Venue for BinanceFutures {
    type Pending = Pending;

    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.get(cx, "/fapi/v1/exchangeInfo", Pending::ExchangeInfo);
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        let connection = self.connection(channel);
        connection.open = true;
        connection.attempts = 0;
        match channel {
            Channel::Public => {
                let mut topics = if self.options.public_data_only {
                    Vec::new()
                } else {
                    vec!["!bookTicker".to_owned()]
                };
                for state in self.books.values_mut() {
                    state.book.clear();
                    state.sync = Sync::Idle;
                }
                topics.extend(self.depth_topics());
                self.send_subscribe(cx, Channel::Public, "SUBSCRIBE", &topics);
            }
            Channel::Market => {
                let mut topics = vec!["!ticker@arr".to_owned(), "!markPrice@arr@1s".to_owned()];
                topics.extend(self.kline_topics());
                self.send_subscribe(cx, Channel::Market, "SUBSCRIBE", &topics);
            }
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        match self.channel_of(socket) {
            Some(Channel::Public) => self.on_public_frame(cx, text),
            Some(Channel::Market) => self.on_market_frame(cx, text),
            None => {}
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        let Some(channel) = self.channel_of(socket) else {
            return;
        };
        let connection = self.connection(channel);
        connection.socket = None;
        connection.open = false;
        connection.attempts += 1;
        let delay = (250.0 * 2f64.powi(connection.attempts as i32)).min(MAX_RECONNECT_DELAY_MS);
        let timer = cx.set_timer(delay);
        self.connection(channel).reconnect = Some(timer);
        cx.log(
            LogLevel::Warning,
            format!("Binance socket closed ({code}); reconnecting in {delay} ms"),
        );
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, status: u16, body: &str) {
        match pending {
            Pending::ExchangeInfo => self.on_markets(cx, body),
            Pending::Daily => {
                let rows: Vec<Daily> = serde_json::from_str(body).unwrap_or_default();
                self.ticker_seed.daily = Some(
                    rows.iter()
                        .map(|d| {
                            let values = (
                                num::parse(&d.last_price),
                                num::parse(&d.price_change_percent),
                                num::parse(&d.volume),
                                num::parse(&d.quote_volume),
                            );
                            (d.symbol.to_string(), values)
                        })
                        .collect(),
                );
                self.try_seed_tickers(cx);
            }
            Pending::BookTickers => {
                let rows: Vec<BookTickerRest> = serde_json::from_str(body).unwrap_or_default();
                self.ticker_seed.books = Some(
                    rows.iter()
                        .map(|b| {
                            (
                                b.symbol.to_string(),
                                num::parse(&b.bid_price),
                                num::parse(&b.ask_price),
                            )
                        })
                        .collect(),
                );
                self.try_seed_tickers(cx);
            }
            Pending::PremiumIndex => {
                let rows: Vec<PremiumIndex> = serde_json::from_str(body).unwrap_or_default();
                self.ticker_seed.premium = Some(
                    rows.iter()
                        .map(|p| {
                            let values = (
                                num::parse(&p.mark_price),
                                num::parse(&p.index_price),
                                num::parse(&p.last_funding_rate),
                                p.next_funding_time,
                            );
                            (p.symbol.to_string(), values)
                        })
                        .collect(),
                );
                self.try_seed_tickers(cx);
            }
            Pending::DepthSnapshot { symbol, generation } => {
                self.on_depth_snapshot(cx, symbol, generation, status, body)
            }
            Pending::Klines { command } => self.on_klines(cx, command, status, body),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Klines { command } => cx.done(command, Err(error.to_owned())),
            Pending::DepthSnapshot { symbol, generation } => {
                if let Some(state) = self.books.get_mut(&symbol)
                    && state.generation == generation
                {
                    Self::snapshot_failed(cx, state, 0.0);
                }
            }
            _ => cx.log(
                LogLevel::Error,
                format!("Binance startup request failed: {error}"),
            ),
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        for state in self.books.values_mut() {
            if matches!(state.sync, Sync::Cooldown(t) if t == timer) {
                state.sync = Sync::Idle;
            }
        }
        for channel in [Channel::Public, Channel::Market] {
            if self.connection(channel).reconnect == Some(timer) {
                self.connection(channel).reconnect = None;
                self.connect(cx, channel);
            }
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.subscribe_book(cx, symbol),
            Command::UnsubscribeOrderBook { symbol } => self.unsubscribe_book(cx, &symbol),
            Command::SubscribeCandles { symbol, interval } => {
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_insert(0);
                *count += 1;
                if *count == 1 {
                    let topic = format!("{}@kline_{interval}", symbol.to_lowercase());
                    self.send_subscribe(cx, Channel::Market, "SUBSCRIBE", &[topic]);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(count) = self.candles.get_mut(&key) {
                    *count -= 1;
                    if *count == 0 {
                        self.candles.remove(&key);
                        let topic = format!("{}@kline_{}", key.0.to_lowercase(), key.1);
                        self.send_subscribe(cx, Channel::Market, "UNSUBSCRIBE", &[topic]);
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
                let mut path = format!(
                    "/fapi/v1/klines?symbol={symbol}&interval={interval}&limit={}",
                    limit.unwrap_or(500)
                );
                if let Some(from) = from_ms {
                    path.push_str(&format!("&startTime={}", num::format(from)));
                }
                if let Some(to) = to_ms {
                    path.push_str(&format!("&endTime={}", num::format(to)));
                }
                self.get(cx, &path, Pending::Klines { command });
            }
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. } => {
                cx.done(command, Err("Binance trading is not available yet".into()))
            }
        }
    }

    fn book_mut(&mut self, symbol: &str) -> Option<&mut Book> {
        self.books.get_mut(symbol).map(|state| &mut state.book)
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|state| matches!(state.sync, Sync::Synced { .. }))
            .map(|state| &state.book)
    }
}
