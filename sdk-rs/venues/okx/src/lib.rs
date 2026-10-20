//! OKX swaps and EEA X-Perps on the sans-I/O engine.
mod account;
mod history;
mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue, num,
};
use sign::Credentials;
use std::collections::{HashMap, HashSet};

const PUBLIC: usize = 0;
const BUSINESS: usize = 2;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub eea: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InstrumentType {
    Swap,
    Futures,
}
impl InstrumentType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Swap => "SWAP",
            Self::Futures => "FUTURES",
        }
    }
}
struct Product {
    kind: InstrumentType,
    underlying: String,
}

#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    authenticated: bool,
    auth_deadline: Option<f64>,
    last_received: f64,
    ping_at: Option<f64>,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}

#[derive(Default)]
struct BookState {
    subscribers: u32,
    sequence: Option<i64>,
    recovering: bool,
    book: Book,
}

pub enum Pending {
    Account(account::Request),
    History(history::Request),
    Trade(trading::Request),
    Markets {
        kind: InstrumentType,
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        kind: InstrumentType,
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
        read: CandleRead,
    },
}

pub struct CandleRead {
    instrument: String,
    interval: String,
    limit: u32,
    before: f64,
    after: f64,
    historical: bool,
    force_history: bool,
}

pub struct Okx {
    options: Options,
    credentials: Option<Credentials>,
    account: account::AccountState,
    trading: trading::TradingState,
    history: history::HistoryState,
    connections: [Connection; 3],
    books: HashMap<String, BookState>,
    marks: HashSet<String>,
    indexes: HashSet<String>,
    candles: HashMap<(String, String), u32>,
    products: HashMap<String, Product>,
    underlying_products: HashMap<String, Vec<String>>,
    price_tickers: HashMap<String, Option<u32>>,
}

impl Okx {
    pub fn new(options: Options) -> Self {
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .key
                .as_ref()
                .filter(|s| !s.is_empty())
                .zip(options.secret.as_ref().filter(|s| !s.is_empty()))
                .zip(options.passphrase.as_ref().filter(|s| !s.is_empty()))
                .map(|((key, secret), passphrase)| Credentials {
                    key: key.clone(),
                    secret: secret.clone(),
                    passphrase: passphrase.clone(),
                })
        };
        Self {
            options,
            credentials,
            account: account::AccountState::default(),
            trading: trading::TradingState::default(),
            history: history::HistoryState::default(),
            connections: std::array::from_fn(|_| Connection::default()),
            books: HashMap::new(),
            marks: HashSet::new(),
            indexes: HashSet::new(),
            candles: HashMap::new(),
            products: HashMap::new(),
            underlying_products: HashMap::new(),
            price_tickers: HashMap::new(),
        }
    }

    pub(crate) fn instrument_types(&self) -> &'static [InstrumentType] {
        if self.options.eea {
            &[InstrumentType::Swap, InstrumentType::Futures]
        } else {
            &[InstrumentType::Swap]
        }
    }

    pub(crate) fn instrument_type(&self, id: &str) -> &'static str {
        self.products.get(id).map_or("SWAP", |p| p.kind.as_str())
    }

    fn next_instrument_type(&self, kind: InstrumentType) -> Option<InstrumentType> {
        (self.options.eea && kind == InstrumentType::Swap).then_some(InstrumentType::Futures)
    }

    fn reference_price(&self, cx: &Cx<'_, Pending>, market: &Market, reference: f64) -> f64 {
        if reference.is_finite() && reference > 0.0 {
            return reference;
        }
        cx.store.ticker(&market.symbol).map_or(0.0, |t| {
            if t.mark.is_finite() && t.mark > 0.0 {
                t.mark
            } else if t.last.is_finite() && t.last > 0.0 {
                t.last
            } else {
                0.0
            }
        })
    }

    pub(crate) fn contracts_to_base(
        &self,
        cx: &Cx<'_, Pending>,
        market: &Market,
        native: f64,
        reference: f64,
    ) -> f64 {
        let value = scaled(native, market.contract_size.unwrap_or(1.0));
        if market.is_inverse == Some(true) {
            let price = self.reference_price(cx, market, reference);
            if price > 0.0 { value / price } else { 0.0 }
        } else {
            value
        }
    }

    fn rest(&self) -> &'static str {
        match (self.options.eea, self.options.testnet) {
            (true, _) => "https://eea.okx.com",
            (false, true) => "https://openapi.okx.com",
            (false, false) => "https://www.okx.com",
        }
    }

    fn socket_url(&self, channel: usize) -> String {
        let host = match (self.options.eea, self.options.testnet) {
            (true, true) => "wseeapap.okx.com",
            (true, false) => "wseea.okx.com",
            (false, true) => "wspap.okx.com",
            (false, false) => "ws.okx.com",
        };
        let path = ["public", "private", "business"][channel];
        format!("wss://{host}:8443/ws/v5/{path}")
    }

    fn headers(
        &self,
        now: f64,
        method: Method,
        path: &str,
        body: &str,
        private: bool,
    ) -> Vec<(&'static str, String)> {
        if private && (method != Method::Get || self.options.read_credentials.is_none()) {
            return self.primary_headers(now, method, path, body);
        }
        let read = (method == Method::Get)
            .then_some(self.options.read_credentials.as_ref())
            .flatten();
        let secondary = read.map(|r| Credentials {
            key: r.key.clone(),
            secret: r.secret.clone(),
            passphrase: r
                .passphrase
                .clone()
                .or_else(|| self.options.passphrase.clone())
                .unwrap_or_default(),
        });
        let mut headers = if private {
            secondary
                .as_ref()
                .or(self.credentials.as_ref())
                .map(|c| c.headers(now, method.as_str(), path, body))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if self.options.testnet {
            headers.push(("x-simulated-trading", "1".into()));
        }
        if method != Method::Get {
            headers.push(("Content-Type", "application/json".into()));
        }
        headers
    }

    pub(crate) fn primary_headers(
        &self,
        now: f64,
        method: Method,
        path: &str,
        body: &str,
    ) -> Vec<(&'static str, String)> {
        let mut headers = self
            .credentials
            .as_ref()
            .map(|c| c.headers(now, method.as_str(), path, body))
            .unwrap_or_default();
        if self.options.testnet {
            headers.push(("x-simulated-trading", "1".into()));
        }
        if method != Method::Get {
            headers.push(("Content-Type", "application/json".into()));
        }
        headers
    }

    fn list(&self, cx: &mut Cx<'_, Pending>, path: &str, private: bool, pending: Pending) {
        let headers = self.headers(cx.now, Method::Get, path, "", private);
        cx.request_streamed(
            Method::Get,
            format!("{}{path}", self.rest()),
            headers,
            ArrayAt::EnvelopeField("data"),
            pending,
        );
    }

    fn connect(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let socket = cx.connect_channel(
            self.socket_url(channel),
            match channel {
                PUBLIC => safe_cex_engine::SocketChannel::Public,
                BUSINESS => safe_cex_engine::SocketChannel::Business,
                _ => safe_cex_engine::SocketChannel::Private,
            },
        );
        let c = &mut self.connections[channel];
        c.socket = Some(socket);
        c.open = false;
        c.authenticated = false;
        c.last_received = cx.now;
        c.ping_at = None;
        c.watchdog = Some(cx.set_timer(10_000.0));
    }

    fn channel(&self, socket: SocketId) -> Option<usize> {
        self.connections
            .iter()
            .position(|c| c.socket == Some(socket))
    }

    fn topics(&self, cx: &mut Cx<'_, Pending>, channel: usize, op: &str, args: &[Topic<'_>]) {
        let c = &self.connections[channel];
        if !c.open {
            return;
        }
        if let Some(socket) = c.socket {
            for args in args.chunks(100) {
                cx.send(
                    socket,
                    serde_json::to_string(&Subscription { op, args }).expect("topic serialization"),
                );
            }
        }
    }

    fn subscribe(&self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if channel == PUBLIC {
            // Serialize bounded topic batches while the store is borrowed;
            // only the owned messages need to survive until Cx can send them.
            let mut args = Vec::with_capacity(100);
            let mut text = Vec::new();
            for name in [
                "tickers",
                "mark-price",
                "index-tickers",
                "open-interest",
                "funding-rate",
            ] {
                for m in cx.store.markets() {
                    args.push(Topic {
                        channel: name,
                        inst_id: Some(if name == "index-tickers" {
                            self.products
                                .get(&m.id)
                                .map_or(m.id.as_str(), |p| p.underlying.as_str())
                        } else {
                            &m.id
                        }),
                        inst_type: None,
                    });
                    if args.len() == 100 {
                        text.push(
                            serde_json::to_string(&Subscription {
                                op: "subscribe",
                                args: &args,
                            })
                            .expect("topics"),
                        );
                        args.clear();
                    }
                }
            }
            if !args.is_empty() {
                text.push(
                    serde_json::to_string(&Subscription {
                        op: "subscribe",
                        args: &args,
                    })
                    .expect("topics"),
                );
            }
            drop(args);
            if let Some(socket) = self.connections[PUBLIC].socket {
                for t in text {
                    cx.send(socket, t);
                }
            }
            for symbol in self.books.keys() {
                if let Some(m) = cx.store.market(symbol) {
                    let id = m.id.clone();
                    self.topics(cx, PUBLIC, "subscribe", &[Topic::instrument("books", &id)]);
                }
            }
        } else if channel == BUSINESS {
            if self.credentials.is_some() && !self.connections[BUSINESS].authenticated {
                return;
            }
            for (symbol, interval) in self.candles.keys() {
                if let Some(m) = cx.store.market(symbol) {
                    let id = m.id.clone();
                    let name = format!("candle{}", bar(interval));
                    self.topics(cx, BUSINESS, "subscribe", &[Topic::instrument(&name, &id)]);
                }
            }
        }
    }

    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, message: wire::Message<'_>) {
        let Some(data) = message.data else {
            return;
        };
        let channel = message.arg.channel;
        if channel == "books" {
            let Some(m) = cx.store.market_by_id(message.arg.inst_id) else {
                return;
            };
            let symbol = m.symbol.clone();
            // The TS book contract scales by lotSz, unlike orders/fills (ctVal).
            let step = m.precision.amount;
            let Some(state) = self.books.get_mut(&symbol) else {
                return;
            };
            if state.recovering && message.action != "snapshot" {
                return;
            }
            let decoded = serde_json::from_str::<Vec<wire::Depth<'_>>>(data.get());
            let valid = decoded.as_ref().is_ok_and(|rows| {
                !rows.is_empty()
                    && rows.iter().all(|d| {
                        d.bids.iter().chain(&d.asks).all(|row| {
                            row.len() >= 2
                                && row[0]
                                    .parse::<f64>()
                                    .is_ok_and(|v| v.is_finite() && v > 0.0)
                                && row[1]
                                    .parse::<f64>()
                                    .is_ok_and(|v| v.is_finite() && v >= 0.0)
                        })
                    })
            });
            let mut gap = !valid || !matches!(message.action, "snapshot" | "update");
            if !gap {
                for d in decoded.expect("validated depth") {
                    if message.action == "snapshot" {
                        state.book.clear();
                        state.sequence = Some(d.seq_id);
                        state.recovering = false;
                    } else if state.recovering {
                        return;
                    } else if state.sequence.is_none() || d.prev_seq_id != state.sequence {
                        gap = true;
                        break;
                    } else {
                        state.sequence = Some(d.seq_id);
                    }
                    for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                        for row in rows {
                            state.book.apply(
                                side,
                                num::parse(row[0]),
                                scaled(num::parse(row[1]), step),
                            );
                        }
                    }
                    cx.book_changed(&symbol);
                }
            }
            if gap {
                state.sequence = None;
                state.recovering = true;
            }
            if gap {
                let id = message.arg.inst_id;
                self.topics(cx, PUBLIC, "unsubscribe", &[Topic::instrument("books", id)]);
                self.topics(cx, PUBLIC, "subscribe", &[Topic::instrument("books", id)]);
            }
            return;
        }
        if let Some(interval) = channel.strip_prefix("candle") {
            let interval = interval_for_bar(interval);
            if let Some(m) = cx.store.market_by_id(message.arg.inst_id) {
                let symbol = m.symbol.clone();
                if !self
                    .candles
                    .contains_key(&(symbol.clone(), interval.clone()))
                {
                    return;
                }
                let _ = json::each::<Vec<&str>>(data.get(), |row| {
                    if let Some(c) = wire::candle(&row) {
                        cx.candle(&symbol, &interval, c);
                    }
                });
            }
            return;
        }
        let _ = json::each::<wire::Prices<'_>>(data.get(), |p| {
            let mut update = |id: &str| {
                let Some(index) = self.price_tickers.get(id).copied() else {
                    return;
                };
                if channel == "mark-price" && !self.marks.contains(id) {
                    self.marks.insert(id.to_owned());
                }
                if channel == "index-tickers" && !self.indexes.contains(id) {
                    self.indexes.insert(id.to_owned());
                }
                let Some(index) = index else {
                    return;
                };
                cx.store.update_ticker_at(index, |t| match channel {
                    "tickers" => {
                        t.bid = num::parse(p.bid_px);
                        t.ask = num::parse(p.ask_px);
                        t.last = num::parse(p.last);
                        t.percentage = percentage(p.last, p.open24h);
                        t.volume = num::parse(p.vol_ccy24h);
                        t.quote_volume = num::parse(p.vol24h);
                    }
                    "mark-price" => t.mark = num::parse(p.mark_px),
                    "index-tickers" => t.index = num::parse(p.idx_px),
                    "open-interest" => t.open_interest = num::parse(p.oi_ccy),
                    "funding-rate" => {
                        t.funding_rate = num::parse(p.funding_rate);
                        t.funding_time = num::parse(p.funding_time);
                    }
                    _ => {}
                });
            };
            if channel == "index-tickers" {
                if let Some(ids) = self.underlying_products.get(p.inst_id) {
                    for id in ids {
                        update(id);
                    }
                }
            } else {
                update(p.inst_id);
            }
        });
    }
}

impl Venue for Okx {
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
        let private = self.options.eea && self.credentials.is_some();
        let path = if private {
            "/api/v5/account/instruments?instType=SWAP"
        } else {
            "/api/v5/public/instruments?instType=SWAP"
        };
        self.list(
            cx,
            path,
            private,
            Pending::Markets {
                kind: InstrumentType::Swap,
                rows: Vec::new(),
                invalid: false,
            },
        );
    }

    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(channel) = self.channel(socket) else {
            return;
        };
        let c = &mut self.connections[channel];
        c.open = true;
        c.attempts = 0;
        c.last_received = cx.now;
        c.auth_deadline =
            (channel != PUBLIC && self.credentials.is_some()).then_some(cx.now + 10_000.0);
        if channel != PUBLIC && self.credentials.is_some() {
            self.login(cx, channel);
        } else {
            self.subscribe(cx, channel);
        }
        cx.send(socket, "ping");
        self.connections[channel].ping_at = Some(cx.now);
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self.channel(socket) else {
            return;
        };
        let c = &mut self.connections[channel];
        c.last_received = cx.now;
        if text == "pong" {
            c.ping_at = None;
            return;
        }
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return;
        };
        if message.event == "error" {
            if channel != PUBLIC && !self.connections[channel].authenticated {
                cx.close(socket);
                self.on_close(cx, socket, 1008);
            }
            cx.log(
                LogLevel::Error,
                format!("OKX {}: {}", message.code, message.msg),
            );
            return;
        }
        if channel != PUBLIC
            && (message.event == "login"
                || matches!(
                    message.arg.channel,
                    "positions" | "orders" | "orders-algo" | "algo-advance" | "account"
                ))
        {
            self.private_message(cx, channel, message);
        } else {
            self.public_message(cx, message);
        }
    }

    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        let Some(channel) = self.channel(socket) else {
            return;
        };
        let c = &mut self.connections[channel];
        c.socket = None;
        c.open = false;
        c.authenticated = false;
        if let Some(timer) = c.watchdog.take() {
            cx.clear_timer(timer);
        }
        c.attempts = c.attempts.saturating_add(1);
        c.reconnect =
            Some(cx.set_timer((500.0 * 2f64.powi(c.attempts.min(6) as i32)).min(30_000.0)));
        if channel == PUBLIC {
            for state in self.books.values_mut() {
                state.sequence = None;
                state.recovering = false;
                state.book.clear();
            }
        }
    }

    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Markets {
                kind,
                rows,
                invalid,
            } => match serde_json::from_str::<wire::Instrument<'_>>(item) {
                Ok(m) => {
                    if !matches!(m.ct_type, "linear" | "inverse")
                        || (!m.inst_type.is_empty() && m.inst_type != kind.as_str())
                        || (*kind == InstrumentType::Futures && m.rule_type != "xperp")
                    {
                        return;
                    }
                    if m.inst_type.is_empty() {
                        *invalid = true;
                        return;
                    }
                    let inverse = m.ct_type == "inverse";
                    let underlying = if !m.uly.is_empty() {
                        m.uly
                    } else {
                        m.inst_id.strip_suffix("-SWAP").unwrap_or(m.inst_id)
                    };
                    let mut family = if !m.uly.is_empty() {
                        m.uly
                    } else {
                        m.inst_family
                    }
                    .split('-');
                    let family_base = family.next().unwrap_or_default();
                    let family_quote = family.next().unwrap_or_default();
                    let base = if !m.base_ccy.is_empty() {
                        m.base_ccy
                    } else if inverse {
                        m.settle_ccy
                    } else {
                        m.ct_val_ccy
                    };
                    let quote = if !m.quote_ccy.is_empty() {
                        m.quote_ccy
                    } else if inverse {
                        m.ct_val_ccy
                    } else {
                        m.settle_ccy
                    };
                    let base = if base.is_empty() { family_base } else { base };
                    let quote = if quote.is_empty() {
                        family_quote
                    } else {
                        quote
                    };
                    if m.inst_id.is_empty()
                        || base.is_empty()
                        || quote.is_empty()
                        || (*kind == InstrumentType::Swap && !m.inst_id.ends_with("-SWAP"))
                        || [m.ct_val, m.lot_sz, m.tick_sz, m.min_sz, m.lever]
                            .iter()
                            .any(|n| n.parse::<f64>().is_err() || num::parse(n) <= 0.0)
                    {
                        *invalid = true;
                        return;
                    }
                    let contract_size = num::parse(m.ct_val);
                    let max = [
                        m.max_iceberg_sz,
                        m.max_lmt_sz,
                        m.max_mkt_sz,
                        m.max_stop_sz,
                        m.max_trigger_sz,
                        m.max_twap_sz,
                    ]
                    .map(num::parse)
                    .into_iter()
                    .fold(f64::INFINITY, f64::min);
                    if max <= 0.0 || !max.is_finite() {
                        *invalid = true;
                        return;
                    }
                    self.products.insert(
                        m.inst_id.into(),
                        Product {
                            kind: *kind,
                            underlying: underlying.into(),
                        },
                    );
                    rows.push(Market {
                        id: m.inst_id.into(),
                        symbol: if *kind == InstrumentType::Futures {
                            m.inst_id.replace('-', "")
                        } else {
                            m.inst_id
                                .strip_suffix("-SWAP")
                                .unwrap_or(m.inst_id)
                                .replace('-', "")
                        },
                        pretty_symbol: (*kind == InstrumentType::Futures)
                            .then(|| format!("{base}/{quote} XPERP")),
                        base: base.to_uppercase(),
                        quote: quote.to_uppercase(),
                        active: m.state == "live",
                        precision: Precision {
                            amount: num::parse(m.lot_sz),
                            price: num::parse(m.tick_sz),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: scaled(num::parse(m.min_sz), contract_size),
                                max,
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: num::parse(m.lever),
                            },
                            price: None,
                        },
                        contract_size: Some(contract_size),
                        is_inverse: Some(inverse),
                        is_expiring: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                }
                Err(_) => *invalid = true,
            },
            Pending::Tickers {
                kind,
                rows,
                invalid,
            } => match serde_json::from_str::<wire::TickerSeed<'_>>(item) {
                Ok(p) => {
                    if let Some(m) = cx.store.market_by_id(p.inst_id) {
                        if self.instrument_type(&m.id) != kind.as_str() {
                            return;
                        }
                        if [
                            p.last,
                            p.open24h,
                            p.bid_px,
                            p.ask_px,
                            p.vol_ccy24h,
                            p.vol24h,
                        ]
                        .iter()
                        .any(|n| {
                            !n.is_empty() && n.parse::<f64>().ok().is_none_or(|v| !v.is_finite())
                        }) {
                            *invalid = true;
                            return;
                        }
                        let last = num::parse(p.last);
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: num::parse(p.bid_px),
                            ask: num::parse(p.ask_px),
                            last,
                            mark: last,
                            index: last,
                            percentage: percentage(p.last, p.open24h),
                            volume: num::parse(p.vol_ccy24h),
                            quote_volume: num::parse(p.vol24h),
                            ..Ticker::default()
                        });
                    }
                }
                Err(_) => *invalid = true,
            },
            Pending::Candles { rows, invalid, .. } => match serde_json::from_str::<Vec<&str>>(item)
                .ok()
                .and_then(|r| wire::candle(&r))
            {
                Some(c) => rows.push(c),
                None => *invalid = true,
            },
        }
    }

    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        let pending = match pending {
            Pending::History(request) if request.is_lookup() => {
                self.history_lookup_response(cx, request, response);
                return;
            }
            other => other,
        };
        let result = if (200..300).contains(&response.status) {
            wire::envelope(response.body).map(|_| ())
        } else {
            Err(format!("OKX HTTP {}: {}", response.status, response.body))
        };
        if let Err(error) = result {
            self.on_request_failed(cx, pending, &error);
            return;
        }
        match pending {
            Pending::Account(request) => self.account_response(cx, request, response),
            Pending::Trade(request) => self.trade_response(cx, request, response),
            Pending::History(request) => self.history_response(cx, request, response),
            Pending::Markets {
                kind,
                rows,
                invalid: false,
            } => {
                if let Some(next) = self.next_instrument_type(kind) {
                    let private = self.options.eea && self.credentials.is_some();
                    let scope = if private { "account" } else { "public" };
                    self.list(
                        cx,
                        &format!("/api/v5/{scope}/instruments?instType={}", next.as_str()),
                        private,
                        Pending::Markets {
                            kind: next,
                            rows,
                            invalid: false,
                        },
                    );
                    return;
                }
                cx.store.set_markets(rows);
                self.price_tickers = price_ticker_indexes(cx.store);
                self.underlying_products.clear();
                for market in cx.store.markets() {
                    if let Some(product) = self.products.get(&market.id) {
                        self.underlying_products
                            .entry(product.underlying.clone())
                            .or_default()
                            .push(market.id.clone());
                    }
                }
                self.list(
                    cx,
                    "/api/v5/market/tickers?instType=SWAP",
                    false,
                    Pending::Tickers {
                        kind: InstrumentType::Swap,
                        rows: Vec::new(),
                        invalid: false,
                    },
                );
            }
            Pending::Tickers {
                kind,
                rows,
                invalid: false,
            } => {
                if let Some(next) = self.next_instrument_type(kind) {
                    self.list(
                        cx,
                        &format!("/api/v5/market/tickers?instType={}", next.as_str()),
                        false,
                        Pending::Tickers {
                            kind: next,
                            rows,
                            invalid: false,
                        },
                    );
                    return;
                }
                cx.store.set_tickers(rows);
                self.price_tickers = price_ticker_indexes(cx.store);
                self.connect(cx, PUBLIC);
                self.connect(cx, BUSINESS);
                self.account_start(cx);
            }
            Pending::Candles {
                command,
                mut rows,
                invalid: false,
                mut read,
            } => {
                if !read.historical && (rows.is_empty() || read.force_history) {
                    read.historical = true;
                    let path = read.path();
                    self.list(
                        cx,
                        &path,
                        false,
                        Pending::Candles {
                            command,
                            rows: Vec::new(),
                            invalid: false,
                            read,
                        },
                    );
                    return;
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    serde_json::to_string(&rows).map_err(|e| e.to_string()),
                );
            }
            other => self.on_request_failed(cx, other, "malformed OKX list item"),
        }
    }

    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Account(request) = pending {
            self.account_failed(cx, request, error);
        } else if let Pending::Trade(request) = pending {
            self.trade_failed(cx, request, error);
        } else if let Pending::History(request) = pending {
            self.history_failed(cx, request, error);
        } else if let Pending::Candles { command, .. } = pending {
            cx.log(LogLevel::Error, error);
            cx.done(command, Ok("[]".into()));
        } else {
            cx.log(LogLevel::Error, error);
        }
    }

    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.history_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        for channel in 0..3 {
            let c = &mut self.connections[channel];
            if c.reconnect == Some(timer) {
                c.reconnect = None;
                self.connect(cx, channel);
                return;
            }
            if c.watchdog == Some(timer) {
                c.watchdog = None;
                if let Some(socket) = c.socket {
                    if !c.open
                        || c.auth_deadline.is_some_and(|at| cx.now >= at)
                        || c.ping_at.is_some_and(|at| cx.now - at >= 10_000.0)
                    {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                        return;
                    }
                    cx.send(socket, "ping");
                    c.ping_at = Some(cx.now);
                }
                c.watchdog = Some(cx.set_timer(10_000.0));
                return;
            }
        }
    }

    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                let state = self.books.entry(symbol.clone()).or_default();
                state.subscribers += 1;
                if state.subscribers == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    self.topics(cx, PUBLIC, "subscribe", &[Topic::instrument("books", &id)]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(state) = self.books.get_mut(&symbol) {
                    state.subscribers = state.subscribers.saturating_sub(1);
                    if state.subscribers > 0 {
                        return;
                    }
                }
                self.books.remove(&symbol);
                if let Some(m) = cx.store.market(&symbol) {
                    let id = m.id.clone();
                    self.topics(
                        cx,
                        PUBLIC,
                        "unsubscribe",
                        &[Topic::instrument("books", &id)],
                    );
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1
                    && let Some(m) = cx.store.market(&symbol)
                {
                    let id = m.id.clone();
                    let name = format!("candle{}", bar(&interval));
                    self.topics(cx, BUSINESS, "subscribe", &[Topic::instrument(&name, &id)]);
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), interval.clone())) {
                    *count = count.saturating_sub(1);
                    if *count > 0 {
                        return;
                    }
                }
                self.candles.remove(&(symbol.clone(), interval.clone()));
                if let Some(m) = cx.store.market(&symbol) {
                    let id = m.id.clone();
                    let name = format!("candle{}", bar(&interval));
                    self.topics(
                        cx,
                        BUSINESS,
                        "unsubscribe",
                        &[Topic::instrument(&name, &id)],
                    );
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
                let Some(m) = cx.store.market(&symbol) else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                if [from_ms, to_ms]
                    .into_iter()
                    .flatten()
                    .any(|v| !v.is_finite())
                {
                    cx.done(command, Err("invalid candle time bounds".into()));
                    return;
                }
                let explicit_limit = limit.filter(|n| *n > 0);
                let limit = explicit_limit.unwrap_or(300).min(300);
                let end = to_ms.unwrap_or(cx.now);
                let Some(default_start) = subtract_bars(end, &interval, limit) else {
                    cx.done(
                        command,
                        Err(format!("unsupported candle interval {interval}")),
                    );
                    return;
                };
                let before = if explicit_limit.is_none() {
                    from_ms.unwrap_or(default_start)
                } else {
                    default_start
                };
                if before > end {
                    cx.done(command, Err("invalid candle time bounds".into()));
                    return;
                }
                let read = CandleRead {
                    instrument: m.id.clone(),
                    interval: interval.clone(),
                    limit,
                    before,
                    after: end,
                    historical: false,
                    force_history: to_ms.is_some_and(|to| {
                        subtract_bars(cx.now, &interval, 1440).is_some_and(|oldest| to < oldest)
                    }),
                };
                let path = read.path();
                self.list(
                    cx,
                    &path,
                    false,
                    Pending::Candles {
                        command,
                        rows: Vec::new(),
                        invalid: false,
                        read,
                    },
                );
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            other => self.trade_command(cx, other),
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.sequence.is_some())
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, b)| b.sequence.is_some())
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}

fn price_ticker_indexes(store: &safe_cex_engine::Store) -> HashMap<String, Option<u32>> {
    // Match Store's last duplicate symbol lookup, independently of either table's order.
    let symbols: HashMap<&str, u32> = store
        .tickers()
        .iter()
        .enumerate()
        .map(|(index, ticker)| (ticker.symbol.as_str(), index as u32))
        .collect();
    store
        .markets()
        .iter()
        .map(|market| {
            (
                market.id.clone(),
                symbols.get(market.symbol.as_str()).copied(),
            )
        })
        .collect()
}

#[derive(serde::Serialize)]
struct Topic<'a> {
    channel: &'a str,
    #[serde(rename = "instId", skip_serializing_if = "Option::is_none")]
    inst_id: Option<&'a str>,
    #[serde(rename = "instType", skip_serializing_if = "Option::is_none")]
    inst_type: Option<&'a str>,
}
impl<'a> Topic<'a> {
    fn instrument(channel: &'a str, id: &'a str) -> Self {
        Self {
            channel,
            inst_id: Some(id),
            inst_type: None,
        }
    }
}
#[derive(serde::Serialize)]
struct Subscription<'a> {
    op: &'a str,
    args: &'a [Topic<'a>],
}

fn bar(interval: &str) -> &str {
    match interval {
        "1h" => "1H",
        "2h" => "2H",
        "4h" => "4H",
        "6h" => "6H",
        "12h" => "12H",
        "1d" => "1D",
        "1w" => "1W",
        "1M" => "1Mutc",
        other => other,
    }
}
fn interval_for_bar(value: &str) -> String {
    if value == "1Mutc" {
        "1M".into()
    } else {
        value.to_lowercase()
    }
}
fn percentage(last: &str, open: &str) -> f64 {
    let open = num::parse(open);
    if open == 0.0 {
        0.0
    } else {
        usd((num::parse(last) - open) / open * 100.0)
    }
}
fn usd(value: f64) -> f64 {
    num::round_half_up(value * 100.0) / 100.0
}
fn scaled(value: f64, factor: f64) -> f64 {
    let digits = num::decimals(value) + num::decimals(factor);
    let scale = 10f64.powi(digits.min(15) as i32);
    (value * factor * scale).round() / scale
}

impl CandleRead {
    fn path(&self) -> String {
        let endpoint = if self.historical {
            "history-candles"
        } else {
            "candles"
        };
        format!(
            "/api/v5/market/{endpoint}?instId={}&bar={}&limit={}&before={}&after={}",
            self.instrument,
            bar(&self.interval),
            if self.historical {
                self.limit.min(100)
            } else {
                self.limit
            },
            num::format(self.before),
            num::format(self.after)
        )
    }
}

fn subtract_bars(end: f64, interval: &str, count: u32) -> Option<f64> {
    let duration = match interval {
        "1m" => 60_000.0,
        "3m" => 180_000.0,
        "5m" => 300_000.0,
        "15m" => 900_000.0,
        "30m" => 1_800_000.0,
        "1h" => 3_600_000.0,
        "2h" => 7_200_000.0,
        "4h" => 14_400_000.0,
        "6h" => 21_600_000.0,
        "12h" => 43_200_000.0,
        "1d" => 86_400_000.0,
        "1w" => 604_800_000.0,
        "1M" => {
            let ms = end.floor() as i64;
            let (year, month, day) = sign::civil_date(ms.div_euclid(86_400_000));
            let months = year * 12 + month - 1 - i64::from(count);
            let year = months.div_euclid(12);
            let month = months.rem_euclid(12) + 1;
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            let days = match month {
                2 => {
                    if leap {
                        29
                    } else {
                        28
                    }
                }
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            let year = year - i64::from(month <= 2);
            let era = year.div_euclid(400);
            let yoe = year - era * 400;
            let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day.min(days) - 1;
            let day = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
            return Some((day * 86_400_000 + ms.rem_euclid(86_400_000)) as f64);
        }
        _ => return None,
    };
    Some(end - duration * f64::from(count))
}

#[cfg(test)]
mod price_index_tests {
    use super::*;
    use safe_cex_engine::{Action, Engine, Event};

    const MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"100"}"#;
    fn request(e: &mut Engine<Okx>) -> safe_cex_engine::HttpRequest {
        e.drain_actions()
            .find_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .expect("seed request")
    }
    fn reply(e: &mut Engine<Okx>, request: safe_cex_engine::HttpRequest, rows: &str) {
        e.handle(
            1.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: &format!(r#"{{"code":"0","data":[{rows}]}}"#),
                },
            },
        );
    }
    fn engine() -> Engine<Okx> {
        Engine::new(Okx::new(Options {
            public_data_only: true,
            ..Options::default()
        }))
    }

    #[test]
    fn successful_catalog_and_ticker_commits_each_rebuild_price_identity() {
        let mut e = engine();
        e.store_mut().set_tickers(vec![
            Ticker {
                symbol: "OTHER".into(),
                ..Ticker::default()
            },
            Ticker {
                symbol: "BTCUSDT".into(),
                ..Ticker::default()
            },
        ]);
        e.start(0.0);
        let r = request(&mut e);
        reply(&mut e, r, MARKET);
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&Some(1)));
        let r = request(&mut e);
        reply(
            &mut e,
            r,
            r#"{"instId":"BTC-USDT-SWAP","bidPx":"1","askPx":"3","last":"2","open24h":"1","volCcy24h":"4","vol24h":"5"}"#,
        );
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&Some(0)));
    }

    #[test]
    fn known_markets_without_tickers_still_record_readiness_but_unknown_ids_do_not() {
        let mut e = engine();
        e.start(0.0);
        let r = request(&mut e);
        reply(&mut e, r, MARKET);
        let r = request(&mut e);
        reply(&mut e, r, "");
        assert_eq!(e.venue().price_tickers.get("BTC-USDT-SWAP"), Some(&None));
        let socket = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with("/public") => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(2.0, Event::WsOpened { socket });
        for _ in 0..2 {
            e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"mark-price"},"data":[{"instId":"BTC-USDT-SWAP","markPx":"2"},{"instId":"UNKNOWN-SWAP","markPx":"3"}]}"# });
            e.handle(3.0, Event::WsMessage { socket, text: r#"{"arg":{"channel":"index-tickers"},"data":[{"instId":"BTC-USDT","idxPx":"4"},{"instId":"UNKNOWN","idxPx":"5"}]}"# });
        }
        assert_eq!(e.venue().marks.len(), 1);
        assert_eq!(e.venue().indexes.len(), 1);
        assert!(e.venue().marks.contains("BTC-USDT-SWAP"));
        assert!(e.venue().indexes.contains("BTC-USDT-SWAP"));
        assert!(e.store().tickers().is_empty());
    }

    #[test]
    fn incomplete_or_failed_catalog_does_not_publish_staged_price_identity() {
        let mut e = engine();
        e.start(0.0);
        let r = request(&mut e);
        e.handle(
            1.0,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        let chunk = format!(r#"{{"code":"50113","data":[{MARKET},"#);
        e.handle(
            2.0,
            Event::HttpChunk {
                request: r.id,
                data: chunk.as_bytes(),
            },
        );
        assert!(e.venue().price_tickers.is_empty());
        assert!(e.store().markets().is_empty());
        e.handle(
            3.0,
            Event::HttpChunk {
                request: r.id,
                data: b"{}]}",
            },
        );
        e.handle(4.0, Event::HttpEnd { request: r.id });
        assert!(e.venue().price_tickers.is_empty());
        assert!(e.store().markets().is_empty());
    }
}
