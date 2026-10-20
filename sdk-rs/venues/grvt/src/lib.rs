//! GRVT perpetuals: deterministic market, account and order state machines.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub trading_account_id: Option<String>,
    pub signer_address: Option<String>,
    pub session_proxy: bool,
}
#[derive(Clone)]
pub struct InstrumentInfo {
    pub hash: String,
    pub base_decimals: Option<i32>,
    pub quote_decimals: Option<i32>,
}
pub struct Pending {
    pub(crate) path: &'static str,
    pub(crate) body: String,
    pub(crate) job: Job,
    pub(crate) attempts: u8,
    pub(crate) auth_resent: bool,
    pub(crate) auth_generation: u64,
}
pub(crate) enum Job {
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    Markets {
        rows: Vec<Market>,
        metadata: HashMap<String, InstrumentInfo>,
        failed: bool,
    },
    Candles {
        command: u32,
        rows: Vec<Candle>,
        failed: bool,
    },
}
impl Job {
    fn is_private(&self) -> bool {
        match self {
            Self::Markets { .. }
            | Self::Candles { .. }
            | Self::Account(account::Request::Login) => false,
            Self::History(request) => !request.is_public(),
            _ => true,
        }
    }
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
}
#[derive(Default)]
struct Depth {
    book: Book,
    subscribers: u32,
    sequence: Option<u64>,
    ready: bool,
}
#[derive(Clone, Copy, Default)]
struct TickerState {
    buy: f64,
    sell: f64,
    open: f64,
    sequence: Option<u64>,
    first_sequence: Option<u64>,
    subscriptions: u8,
}
pub struct Grvt {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    metadata: HashMap<String, InstrumentInfo>,
    public: Connection,
    books: HashMap<String, Depth>,
    candle_sockets: HashMap<String, Connection>,
    candles: HashMap<(String, String), u32>,
    // Catalog-indexed state shares the Store identities instead of allocating
    // the same stream/selector strings in four separate maps.
    ticker_state: Vec<TickerState>,
    request_id: u32,
    ticker_batch: Option<TimerId>,
    ticker_offset: usize,
    startup_retry: Option<TimerId>,
    refresh: Option<TimerId>,
}
#[derive(Serialize)]
struct Subscription<'a> {
    jsonrpc: &'static str,
    id: u32,
    method: &'a str,
    params: Selector<'a>,
}
#[derive(Serialize)]
struct Selector<'a> {
    stream: &'a str,
    selectors: [&'a str; 1],
}
fn interval(s: &str) -> Option<String> {
    if !s.is_ascii() {
        return None;
    }
    let (digits, unit) = s.split_at(s.len().checked_sub(1)?);
    let allowed = match unit {
        "m" => ["1", "3", "5", "15", "30"].contains(&digits),
        "h" => ["1", "2", "4", "6", "8", "12"].contains(&digits),
        "d" => ["1", "3", "5"].contains(&digits),
        "w" => ["1", "2", "3", "4"].contains(&digits),
        _ => false,
    };
    allowed.then(|| format!("CI_{}_{}", digits, unit.to_ascii_uppercase()))
}
impl Grvt {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            metadata: HashMap::new(),
            public: Connection::default(),
            books: HashMap::new(),
            candle_sockets: HashMap::new(),
            candles: HashMap::new(),
            ticker_state: Vec::new(),
            request_id: 0,
            ticker_batch: None,
            ticker_offset: 0,
            startup_retry: None,
            refresh: None,
        }
    }
    fn mdg(&self) -> &'static str {
        if self.options.testnet {
            "https://market-data.testnet.grvt.io"
        } else {
            "https://market-data.grvt.io"
        }
    }
    fn public_url(&self) -> &'static str {
        if self.options.testnet {
            "wss://market-data.testnet.grvt.io/ws/full"
        } else {
            "wss://market-data.grvt.io/ws/full"
        }
    }
    fn request(&mut self, cx: &mut Cx<'_, Pending>, path: &'static str, body: String, job: Job) {
        self.send_request(
            cx,
            Pending {
                path,
                body,
                job,
                attempts: 0,
                auth_resent: false,
                auth_generation: self.account.generation,
            },
        );
    }
    fn send_request(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending) {
        if let Job::Trade(request) = &pending.job
            && let Err(error) = self.trade_pending_guard(cx, request)
        {
            self.fail_pending(cx, pending, &error);
            return;
        }
        if pending.job.is_private() && self.account.session.is_none() {
            self.account_defer(cx, pending);
            return;
        }
        pending.auth_generation = self.account.generation;
        let (base, headers, at) = match &pending.job {
            Job::Markets { .. } | Job::Candles { .. } => (
                self.mdg(),
                vec![("Content-Type", "application/json".into())],
                Some(ArrayAt::Envelope(&[&["r"], &["result"]])),
            ),
            Job::Account(account::Request::Login) => (
                if self.options.testnet {
                    "https://edge.testnet.grvt.io"
                } else {
                    "https://edge.grvt.io"
                },
                vec![
                    ("Content-Type", "application/json".into()),
                    ("X-Grvt-Remember-Me", "true".into()),
                    ("Cookie", "rm=true".into()),
                ],
                None,
            ),
            Job::History(request) => (
                if request.is_public() {
                    self.mdg()
                } else if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                if request.is_public() {
                    vec![("Content-Type", "application/json".into())]
                } else {
                    self.account_headers()
                },
                request.array_at(),
            ),
            Job::Trade(_) => (
                if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                self.account_headers(),
                None,
            ),
            Job::Account(request) => (
                if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                self.account_headers(),
                request.array_at(),
            ),
        };
        let url = format!("{base}{}", pending.path);
        let body = Some(pending.body.clone());
        if let Some(at) = at {
            cx.request_streamed_body(Method::Post, url, headers, body, at, pending);
        } else {
            cx.request(Method::Post, url, headers, body, pending);
        }
    }
    fn fail_pending(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending.job {
            Job::Account(request) => self.account_failed(cx, request, error),
            Job::Trade(request) => self.trade_failed(cx, request, error),
            Job::History(request) => self.history_failed(cx, *request, error),
            Job::Markets { .. } => {
                cx.log(LogLevel::Warning, "GRVT instrument read failed");
                if self.startup_retry.is_none() {
                    self.startup_retry = Some(cx.set_timer(5000.));
                }
            }
            Job::Candles { command, .. } => cx.done(command, Err("GRVT candle read failed".into())),
        }
    }
    fn fetch_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        self.request(
            cx,
            "/lite/v1/all_instruments",
            "{}".into(),
            Job::Markets {
                rows: vec![],
                metadata: HashMap::new(),
                failed: false,
            },
        );
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() {
            return;
        }
        self.public.socket = Some(cx.connect_channel(self.public_url(), SocketChannel::Public));
        self.public.last_received = cx.now;
        self.public.watchdog = Some(cx.set_timer(5_000.));
    }
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, stream: &str, selector: &str, remove: bool) {
        if !self.public.open {
            return;
        }
        let bit = match stream {
            "v1.ticker.d" => 1,
            "v1.book.s" => 2,
            _ => return,
        };
        let primary = selector.split('@').next().unwrap_or(selector);
        let Some(index) = cx.store.market_index_by_id(primary) else {
            return;
        };
        let Some(state) = self.ticker_state.get_mut(index as usize) else {
            return;
        };
        let subscribed = state.subscriptions & bit != 0;
        if subscribed == !remove {
            return;
        }
        if remove {
            state.subscriptions &= !bit;
        } else {
            state.subscriptions |= bit;
        }
        self.request_id = self.request_id.wrapping_add(1).max(1);
        let message = Subscription {
            jsonrpc: "2.0",
            id: self.request_id,
            method: if remove { "unsubscribe" } else { "subscribe" },
            params: Selector {
                stream,
                selectors: [selector],
            },
        };
        if let Some(socket) = self.public.socket {
            cx.send(
                socket,
                serde_json::to_string(&message).expect("subscription"),
            );
        }
    }
    fn ticker_batch(&mut self, cx: &mut Cx<'_, Pending>) {
        let ids: Vec<_> = cx
            .store
            .markets()
            .iter()
            .skip(self.ticker_offset)
            .take(20)
            .map(|m| m.id.clone())
            .collect();
        for id in &ids {
            self.subscribe(cx, "v1.ticker.d", &format!("{id}@500"), false);
        }
        self.ticker_offset += ids.len();
        if self.ticker_offset < cx.store.markets().len() {
            self.ticker_batch = Some(cx.set_timer(100.));
        }
    }
    fn disconnect(&mut self, cx: &mut Cx<'_, Pending>, close: bool) {
        if let Some(socket) = self.public.socket.take()
            && close
        {
            cx.close(socket);
        }
        self.public.open = false;
        if let Some(timer) = self.public.watchdog.take() {
            cx.clear_timer(timer);
        }
        if let Some(timer) = self.ticker_batch.take() {
            cx.clear_timer(timer);
        }
        for state in &mut self.ticker_state {
            state.subscriptions = 0;
            state.sequence = None;
            state.first_sequence = None;
        }
        for depth in self.books.values_mut() {
            depth.ready = false;
            depth.sequence = None;
        }
        if self.public.reconnect.is_none() {
            self.public.reconnect = Some(cx.set_timer(1_000.));
        }
    }
    fn candle_connect(&mut self, cx: &mut Cx<'_, Pending>, timeframe: &str) {
        let url = self.public_url();
        let c = self.candle_sockets.entry(timeframe.to_owned()).or_default();
        if c.socket.is_some() {
            return;
        }
        let feed = match timeframe {
            "1m" => "candles-1m",
            "3m" => "candles-3m",
            "5m" => "candles-5m",
            "15m" => "candles-15m",
            "30m" => "candles-30m",
            "1h" => "candles-1h",
            "2h" => "candles-2h",
            "4h" => "candles-4h",
            "6h" => "candles-6h",
            "8h" => "candles-8h",
            "12h" => "candles-12h",
            "1d" => "candles-1d",
            "3d" => "candles-3d",
            "5d" => "candles-5d",
            "1w" => "candles-1w",
            "2w" => "candles-2w",
            "3w" => "candles-3w",
            "4w" => "candles-4w",
            _ => return,
        };
        c.socket = Some(cx.connect_feed(url, SocketChannel::Public, feed, false));
        c.last_received = cx.now;
        c.watchdog = Some(cx.set_timer(5000.));
    }
    fn candle_subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        timeframe: &str,
        remove: bool,
    ) {
        let Some(c) = self.candle_sockets.get(timeframe).filter(|c| c.open) else {
            return;
        };
        let (Some(socket), Some(market), Some(interval)) =
            (c.socket, cx.store.market(symbol), interval(timeframe))
        else {
            return;
        };
        self.request_id = self.request_id.wrapping_add(1).max(1);
        let selector = format!("{}@{interval}-TRADE", market.id);
        let msg = Subscription {
            jsonrpc: "2.0",
            id: self.request_id,
            method: if remove { "unsubscribe" } else { "subscribe" },
            params: Selector {
                stream: "v1.candle",
                selectors: [&selector],
            },
        };
        cx.send(
            socket,
            serde_json::to_string(&msg).expect("candle subscription"),
        );
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if text == "ping" {
            if let Some(s) = self.public.socket {
                cx.send(s, "pong");
            }
            return;
        }
        if text == "pong" {
            return;
        }
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return;
        };
        if let Some(ack) = message.result {
            if ack.stream == "v1.ticker.d" {
                for (selector, first) in ack.subs.iter().zip(ack.first) {
                    let primary = selector.split('@').next().unwrap_or(selector);
                    if let (Some(index), Ok(first)) =
                        (cx.store.market_index_by_id(primary), first.parse::<u64>())
                        && let Some(state) = self.ticker_state.get_mut(index as usize)
                    {
                        state.first_sequence = Some(first);
                    }
                }
            }
            return;
        }
        let Some(feed) = message.feed else {
            return;
        };
        let Some(index) = cx
            .store
            .market_index_by_id(&feed.instrument)
            .or_else(|| cx.store.market_index(&feed.instrument))
        else {
            return;
        };
        let stream = &*message.stream;
        if !matches!(
            stream,
            "v1.ticker.d" | "v1.ticker.s" | "v1.book.s" | "v1.candle"
        ) {
            return;
        }
        let primary = message
            .selector
            .split('@')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&feed.instrument);
        if let Ok(sequence) = message.sequence.parse::<u64>()
            && stream == "v1.ticker.d"
        {
            let Some(sequence_index) = cx
                .store
                .market_index_by_id(primary)
                .or_else(|| cx.store.market_index(primary))
            else {
                return;
            };
            let state = &mut self.ticker_state[sequence_index as usize];
            if sequence == 0 {
                // Initial snapshot has no live sequence; the ACK anchors the first delta.
                state.sequence = None;
            } else {
                if let Some(last) = state.sequence {
                    if sequence == last {
                        return;
                    }
                    if sequence != last.saturating_add(1) {
                        self.disconnect(cx, true);
                        return;
                    }
                } else if state.first_sequence.is_some_and(|first| sequence != first) {
                    self.disconnect(cx, true);
                    return;
                }
                state.sequence = Some(sequence);
            }
        }
        match stream {
            "v1.ticker.d" | "v1.ticker.s" => {
                let state = &mut self.ticker_state[index as usize];
                if let Some(v) = feed.buy_volume {
                    state.buy = v.0;
                }
                if let Some(v) = feed.sell_volume {
                    state.sell = v.0;
                }
                if let Some(v) = feed.open_price {
                    state.open = v.0;
                }
                // This venue constructs tickers in catalog order at every replacement.
                cx.store.update_ticker_at(index, |t| {
                    if let Some(v) = feed.last {
                        t.last = v.0;
                    }
                    if let Some(v) = feed.mark {
                        t.mark = v.0;
                        if t.last == 0. && v.0 > 0. {
                            t.last = v.0;
                        }
                    }
                    if let Some(v) = feed.index {
                        t.index = v.0;
                    }
                    if let Some(v) = feed.bid {
                        t.bid = v.0;
                    }
                    if let Some(v) = feed.ask {
                        t.ask = v.0;
                    }
                    if let Some(v) = feed.funding {
                        t.funding_rate = v.0 / 100.;
                    }
                    if let Some(v) = feed.interest {
                        t.open_interest = v.0;
                    }
                    if let Some(v) = feed.funding_time {
                        t.funding_time = v.0 / 1e9;
                    }
                    t.volume = state.buy + state.sell;
                    t.quote_volume = t.volume * t.last;
                    if state.open > 0. {
                        t.percentage = (t.last - state.open) / state.open * 100.;
                    }
                });
                self.sync_balance_upnl(cx);
            }
            "v1.book.s" => {
                let symbol = cx.store.markets()[index as usize].symbol.clone();
                let Some(depth) = self.books.get_mut(&symbol).filter(|d| d.subscribers > 0) else {
                    return;
                };
                let (Some(bids), Some(asks)) = (feed.bids, feed.asks) else {
                    return;
                };
                if bids
                    .iter()
                    .chain(&asks)
                    .any(|l| l.price.0 <= 0. || l.size.0 < 0.)
                {
                    return;
                }
                if bids.windows(2).any(|x| x[0].price.0 <= x[1].price.0)
                    || asks.windows(2).any(|x| x[0].price.0 >= x[1].price.0)
                {
                    return;
                }
                if let Ok(sequence) = message.sequence.parse::<u64>() {
                    // Snapshot books are self-contained: gaps need no REST stitching.
                    if sequence != 0 && depth.sequence.is_some_and(|last| sequence <= last) {
                        return;
                    }
                    depth.sequence = Some(sequence);
                }
                depth
                    .book
                    .load_sorted(BookSide::Bids, bids.iter().map(|l| (l.price.0, l.size.0)));
                depth
                    .book
                    .load_sorted(BookSide::Asks, asks.iter().map(|l| (l.price.0, l.size.0)));
                depth.ready = true;
                cx.book_changed(&symbol);
            }
            "v1.candle" => {
                let market_symbol = cx.store.markets()[index as usize].symbol.clone();
                let Some((_, suffix)) = message.selector.split_once('@') else {
                    return;
                };
                let Some(grvt_interval) = suffix.strip_suffix("-TRADE") else {
                    return;
                };
                for ((symbol, interval_name), subscribers) in &self.candles {
                    if *subscribers > 0
                        && *symbol == market_symbol
                        && interval(interval_name).as_deref() == Some(grvt_interval)
                        && let Some(candle) = feed.candle()
                    {
                        cx.candle(symbol, interval_name, candle);
                    }
                }
            }
            _ => {}
        }
    }
}
impl Venue for Grvt {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.fetch_markets(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_open(cx, socket) {
            return;
        }
        if let Some(interval) = self
            .candle_sockets
            .iter()
            .find(|(_, c)| c.socket == Some(socket))
            .map(|(i, _)| i.clone())
        {
            let c = self
                .candle_sockets
                .get_mut(&interval)
                .expect("candle socket");
            c.open = true;
            c.last_received = cx.now;
            let symbols: Vec<_> = self
                .candles
                .iter()
                .filter(|((_, i), n)| i == &interval && **n > 0)
                .map(|((s, _), _)| s.clone())
                .collect();
            for symbol in symbols {
                self.candle_subscription(cx, &symbol, &interval, false);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.last_received = cx.now;
        self.ticker_offset = 0;
        self.ticker_batch(cx);
        let books: Vec<_> = self
            .books
            .iter()
            .filter(|(_, d)| d.subscribers > 0)
            .map(|(s, _)| s.clone())
            .collect();
        for s in books {
            if let Some(m) = cx.store.market(&s) {
                self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), false);
            }
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if let Some(connection) = self
            .candle_sockets
            .values_mut()
            .find(|c| c.socket == Some(socket))
        {
            connection.last_received = cx.now;
            if text == "ping" {
                cx.send(socket, "pong");
            } else {
                self.public_message(cx, text);
            }
            return;
        }
        if self.public.socket == Some(socket) {
            self.public.last_received = cx.now;
            self.public_message(cx, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket) {
            return;
        }
        if let Some(connection) = self
            .candle_sockets
            .values_mut()
            .find(|c| c.socket == Some(socket))
        {
            connection.socket = None;
            connection.open = false;
            if let Some(timer) = connection.watchdog.take() {
                cx.clear_timer(timer);
            }
            connection.reconnect = Some(cx.set_timer(1000.));
            return;
        }
        if self.public.socket == Some(socket) {
            self.disconnect(cx, false);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match &mut pending.job {
            Job::Account(request) => self.account_item(cx, request, item),
            Job::Trade(_) => {}
            Job::History(request) => self.history_item(cx, request, item),
            Job::Markets {
                rows,
                metadata,
                failed,
            } => {
                let Ok(i) = serde_json::from_str::<wire::Instrument<'_>>(item) else {
                    *failed = true;
                    return;
                };
                if i.kind != "PERPETUAL" {
                    return;
                }
                if i.id.is_empty() || i.min.0 <= 0. || i.tick.0 <= 0. {
                    *failed = true;
                    return;
                }
                metadata.insert(
                    i.id.to_string(),
                    InstrumentInfo {
                        hash: i.hash.into_owned(),
                        base_decimals: i.base_decimals,
                        quote_decimals: i.quote_decimals,
                    },
                );
                rows.push(Market {
                    id: i.id.to_string(),
                    symbol: i.id.to_ascii_uppercase(),
                    pretty_symbol: None,
                    base: i.base.into_owned(),
                    quote: i.quote.into_owned(),
                    active: i.active.unwrap_or(true),
                    precision: Precision {
                        amount: i.min.0,
                        price: i.tick.0,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: i.min.0,
                            max: i.max.or(i.max_size).map_or(999999999., |v| v.0),
                        },
                        leverage: MinMax { min: 1., max: 100. },
                        price: None,
                    },
                    contract_size: None,
                    is_expiring: None,
                    is_inverse: None,
                    is_spot: Some(false),
                    is_perpetual: Some(true),
                    is_read_only: None,
                    taker_fee: None,
                    maker_fee: None,
                });
            }
            Job::Candles { rows, failed, .. } => {
                let c = serde_json::from_str::<wire::Feed<'_>>(item)
                    .ok()
                    .and_then(|f| f.candle());
                if let Some(c) = c {
                    rows.push(c);
                } else {
                    *failed = true;
                }
            }
        }
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if let Err(error) = wire::check(&response) {
            let code = wire::code(&response);
            if code == "1000"
                && !pending.auth_resent
                && !matches!(pending.job, Job::Account(account::Request::Login))
                && !matches!(pending.job, Job::Markets { .. } | Job::Candles { .. })
                && !matches!(&pending.job,Job::History(request) if request.is_public())
            {
                if let Job::History(request) = &mut pending.job {
                    request.reset();
                }
                self.account_refresh(cx, pending);
            } else {
                self.fail_pending(cx, pending, &error);
            }
            return;
        }
        if pending.job.is_private() {
            self.account_response_headers(cx, pending.auth_generation, &response);
        }
        match pending.job {
            Job::Account(request) => self.account_response(cx, request, response),
            Job::Trade(request) => self.trade_response(cx, request, response),
            Job::History(request) => self.history_response(cx, *request, response),
            Job::Markets {
                rows,
                metadata,
                failed,
            } => {
                if failed || rows.is_empty() {
                    cx.log(LogLevel::Warning, "Invalid GRVT instrument catalog");
                    if self.startup_retry.is_none() {
                        self.startup_retry = Some(cx.set_timer(5_000.));
                    }
                    return;
                }
                // The public instrument catalog has no authenticated leverage
                // rails. Retain those already fetched from the trading API;
                // replacing signing metadata must not restore the 1..100 defaults.
                let mut rows = rows;
                let has_new_markets = rows
                    .iter()
                    .any(|market| cx.store.market_by_id(&market.id).is_none());
                for market in &mut rows {
                    if let Some(previous) = cx.store.market_by_id(&market.id) {
                        market.limits.leverage = previous.limits.leverage;
                    }
                }
                let tickers = rows
                    .iter()
                    .map(|m| {
                        cx.store
                            .ticker(&m.symbol)
                            .cloned()
                            .unwrap_or_else(|| Ticker {
                                id: m.id.clone(),
                                symbol: m.symbol.clone(),
                                ..Ticker::default()
                            })
                    })
                    .collect();
                let first = !cx.store.loaded().markets;
                // Catalog refreshes can reorder or insert rows. Remap by native
                // instrument identity before replacing the Store's indexes.
                let previous = std::mem::take(&mut self.ticker_state);
                self.ticker_state = rows
                    .iter()
                    .map(|market| {
                        cx.store
                            .market_index_by_id(&market.id)
                            .and_then(|index| previous.get(index as usize))
                            .copied()
                            .unwrap_or_default()
                    })
                    .collect();
                self.metadata = metadata;
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect(cx);
                if first {
                    self.private_start(cx);
                } else if has_new_markets {
                    // Hydrate account-specific rails for newly listed markets.
                    // The helper waits for a private session and avoids duplicate reads.
                    self.refresh_leverage(cx);
                }
                if self.public.open {
                    self.ticker_offset = 0;
                    self.ticker_batch(cx);
                }
                if self.refresh.is_none() {
                    self.refresh = Some(cx.set_timer(300_000.));
                }
            }
            Job::Candles {
                command,
                mut rows,
                failed,
                ..
            } => {
                if failed {
                    cx.done(command, Err("Invalid GRVT candles".into()));
                } else {
                    rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                    cx.done(
                        command,
                        serde_json::to_string(&rows).map_err(|e| e.to_string()),
                    );
                }
            }
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if wire::check(&response).is_err() {
            self.on_response(cx, pending, response);
        } else {
            self.fail_pending(cx, pending, "Missing GRVT response array");
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        let read = match &pending.job {
            Job::Trade(request) => request.is_read(),
            _ => true,
        };
        if read && pending.attempts < 5 {
            pending.attempts += 1;
            match &mut pending.job {
                Job::Markets {
                    rows,
                    metadata,
                    failed,
                } => {
                    rows.clear();
                    metadata.clear();
                    *failed = false;
                }
                Job::Candles { rows, failed, .. } => {
                    rows.clear();
                    *failed = false;
                }
                Job::Account(request) => request.reset(),
                Job::Trade(_) => {}
                Job::History(request) => request.reset(),
            }
            self.send_request(cx, pending);
            return;
        }
        if matches!(pending.job, Job::Account(account::Request::Login)) {
            self.account_login_unavailable(cx);
        } else {
            self.fail_pending(cx, pending, error);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(timeframe) = self
            .candle_sockets
            .iter()
            .find(|(_, c)| c.watchdog == Some(timer) || c.reconnect == Some(timer))
            .map(|(i, _)| i.clone())
        {
            let c = self
                .candle_sockets
                .get_mut(&timeframe)
                .expect("candle timer");
            if c.reconnect == Some(timer) {
                c.reconnect = None;
                self.candle_connect(cx, &timeframe);
                return;
            }
            c.watchdog = None;
            let age = cx.now - c.last_received;
            if age >= 20_000. {
                if let Some(s) = c.socket.take() {
                    cx.close(s);
                }
                c.open = false;
                c.reconnect = Some(cx.set_timer(1000.));
            } else {
                if age >= 6000.
                    && c.open
                    && let Some(s) = c.socket
                {
                    cx.send(s, "ping");
                }
                c.watchdog = Some(cx.set_timer(5000.));
            }
            return;
        }
        if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            let age = cx.now - self.public.last_received;
            if age >= 20_000. {
                self.disconnect(cx, true);
            } else {
                if age >= 6_000.
                    && self.public.open
                    && let Some(s) = self.public.socket
                {
                    cx.send(s, "ping");
                }
                self.public.watchdog = Some(cx.set_timer(5_000.));
            }
        } else if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect(cx);
        } else if self.ticker_batch == Some(timer) {
            self.ticker_batch = None;
            self.ticker_batch(cx);
        } else if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            self.fetch_markets(cx);
        } else if self.refresh == Some(timer) {
            self.refresh = None;
            self.fetch_markets(cx);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::Query { command, query } = command {
            self.start_query(cx, command, query);
            return;
        }
        if let Command::FetchLeverage { command, symbol } = command {
            self.fetch_leverage_command(cx, command, symbol);
            return;
        }
        if matches!(
            &command,
            Command::PlaceOrders { .. }
                | Command::CancelOrders { .. }
                | Command::AmendOrder { .. }
                | Command::CancelSymbolOrders { .. }
                | Command::SetLeverage { .. }
                | Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
                | Command::FetchLeverage { .. }
        ) {
            self.trade_command(cx, command);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                let key = cx
                    .store
                    .market(&symbol)
                    .map_or(symbol.clone(), |m| m.symbol.clone());
                let depth = self.books.entry(key.clone()).or_default();
                depth.subscribers += 1;
                if depth.subscribers == 1 {
                    depth.ready = false;
                    depth.sequence = None;
                }
                if depth.subscribers == 1
                    && let Some(m) = cx.store.market(&key)
                {
                    self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), false);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(depth) = self.books.get_mut(&symbol) {
                    depth.subscribers = depth.subscribers.saturating_sub(1);
                    if depth.subscribers == 0
                        && let Some(m) = cx.store.market(&symbol)
                    {
                        self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), true);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                let (Some(m), Some(i)) = (cx.store.market(&symbol), interval(&timeframe)) else {
                    return;
                };
                let _ = m;
                let _ = i;
                let symbol_key = symbol.clone();
                let interval_key = timeframe.clone();
                let count = self.candles.entry((symbol, timeframe)).or_default();
                *count += 1;
                if *count == 1 {
                    self.candle_connect(cx, &interval_key);
                    self.candle_subscription(cx, &symbol_key, &interval_key, false);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), timeframe.clone())) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&(symbol.clone(), timeframe.clone()));
                        self.candle_subscription(cx, &symbol, &timeframe, true);
                        if !self.candles.keys().any(|(_, i)| i == &timeframe)
                            && let Some(c) = self.candle_sockets.remove(&timeframe)
                        {
                            if let Some(socket) = c.socket {
                                cx.close(socket);
                            }
                            if let Some(timer) = c.watchdog {
                                cx.clear_timer(timer);
                            }
                            if let Some(timer) = c.reconnect {
                                cx.clear_timer(timer);
                            }
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: timeframe,
                limit,
                from_ms,
                to_ms,
            } => {
                let (Some(m), Some(i)) = (cx.store.market(&symbol), interval(&timeframe)) else {
                    cx.done(
                        command,
                        Err("Unknown GRVT market or candle interval".into()),
                    );
                    return;
                };
                #[derive(Serialize)]
                struct Body<'a> {
                    i: &'a str,
                    t: &'static str,
                    #[serde(rename = "i1")]
                    interval: &'a str,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    st: Option<String>,
                    #[serde(skip_serializing_if = "Option::is_none")]
                    et: Option<String>,
                    l: u32,
                }
                if from_ms
                    .into_iter()
                    .chain(to_ms)
                    .any(|v| !v.is_finite() || v < 0.)
                {
                    cx.done(command, Err("Invalid GRVT candle bounds".into()));
                    return;
                }
                let body = serde_json::to_string(&Body {
                    i: &m.id,
                    t: "TRADE",
                    interval: &i,
                    st: from_ms.map(|v| format!("{:.0}", v * 1e6)),
                    et: to_ms.map(|v| format!("{:.0}", v * 1e6)),
                    l: limit.unwrap_or(500).clamp(1, 1000),
                })
                .expect("candle request");
                self.request(
                    cx,
                    "/lite/v1/kline",
                    body,
                    Job::Candles {
                        command,
                        rows: vec![],
                        failed: false,
                    },
                );
            }
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. }
            | Command::FetchLeverage { command, .. }
            | Command::Query { command, .. } => {
                cx.done(command, Err("GRVT private phase pending".into()))
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|d| d.ready && d.subscribers > 0)
            .map(|d| &d.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, d)| d.ready && d.subscribers > 0)
                .map(|(s, d)| (s.as_str(), &d.book)),
        )
    }
}
