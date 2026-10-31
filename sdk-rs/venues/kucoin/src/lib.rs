//! KuCoin Classic Futures on the sans-I/O engine.
mod account;
mod funding;
mod history;
mod identity;
mod leverage;
pub mod sign;
#[cfg(feature = "spot")]
pub mod spot;
mod trading;
mod wire;

use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Balance, Candle, Interval, Market, Ticker};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
pub(crate) use safe_cex_engine::url::encode;
use safe_cex_engine::{
    Command, CommandId, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, Store,
    TimerId, Venue,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap, HashSet};
use wire::*;

const REST: &str = "https://api-futures.kucoin.com";
const SNAPSHOT_SHARD: usize = 380;

fn local_mutation_command_policy() -> safe_cex_engine::request_context::Policy {
    safe_cex_engine::request_context::Policy {
        operation: safe_cex_engine::request_context::Operation::Mutation,
        ..local_read_command_policy()
    }
}

fn local_read_command_policy() -> safe_cex_engine::request_context::Policy {
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub broker: Option<sign::BrokerOptions>,
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
        if let Some(broker) = self.broker.as_mut() {
            safe_cex_crypto::secrets::EraseSecrets::erase_secrets(broker);
        }
    }
}

pub enum Pending {
    Identity(CommandId),
    Leverage(leverage::Read),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Funding(funding::Request),
    Markets {
        rows: Vec<Market>,
        malformed: HashSet<String>,
    },
    Tickers {
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Bullet {
        channel: usize,
        generation: u64,
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

#[derive(Clone, Copy)]
enum Role {
    Detail,
    Snapshot { index: usize, count: usize },
    Private,
}
struct Connection {
    role: Role,
    socket: Option<SocketId>,
    open: bool,
    ready: bool,
    subscribed: bool,
    generation: u64,
    token_pending: bool,
    supervisor: Supervisor,
    ping_interval: f64,
    ping_timeout: f64,
    pong: Option<TimerId>,
}
impl Connection {
    fn new(role: Role) -> Self {
        Self {
            role,
            socket: None,
            open: false,
            ready: false,
            subscribed: false,
            generation: 0,
            token_pending: false,
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 90_000.0,
                ..Config::default()
            }),
            ping_interval: 18000.0,
            ping_timeout: 10000.0,
            pong: None,
        }
    }
}
#[derive(Clone)]
struct Delta {
    sequence: u64,
    side: BookSide,
    price: f64,
    amount: f64,
}
#[derive(Default)]
struct BookState {
    book: Book,
    subscribers: u32,
    synced: bool,
    loading: bool,
    sequence: u64,
    generation: u64,
    buffer: Vec<Delta>,
    retry: Option<TimerId>,
}

pub struct Kucoin {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    credentials: Option<sign::Credentials>,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
    funding: funding::Funding,
    connections: Vec<Connection>,
    books: HashMap<String, BookState>,
    candles: BTreeMap<(String, Interval), u32>,
    message_id: u64,
    book_generation: u64,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    /// Discovery retries wait out a rate-limit window the venue announced.
    discovery_paused_until: f64,
    // Unified symbols of contracts rejected as malformed; owned exposure there holds.
    malformed_markets: HashSet<String>,
    reread_stalls: u8,
}

pub(crate) fn internal_symbol(id: &str) -> String {
    let id = id.strip_suffix('M').unwrap_or(id);
    id.strip_prefix("XBT")
        .map_or_else(|| id.into(), |suffix| format!("BTC{suffix}"))
}
pub(crate) fn api_symbol(store: &Store, symbol: &str) -> String {
    store
        .market(symbol)
        .map_or_else(|| symbol.into(), |m| m.id.clone())
}
// Callers supply raw query values for signing; only their wire representation is escaped.
fn encoded_path(path: &str) -> String {
    let Some((base, query)) = path.split_once('?') else {
        return path.into();
    };
    let query = query
        .split('&')
        .map(|part| {
            let (k, v) = part.split_once('=').unwrap_or((part, ""));
            format!("{}={}", encode(k), encode(v))
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}
/// KuCoin candle minutes and native type name.
pub(crate) fn interval(value: Interval) -> Result<(u32, &'static str), String> {
    use Interval::*;
    match value {
        M1 => Ok((1, "1min")),
        M3 => Ok((3, "3min")),
        M5 => Ok((5, "5min")),
        M15 => Ok((15, "15min")),
        M30 => Ok((30, "30min")),
        H1 => Ok((60, "1hour")),
        H2 => Ok((120, "2hour")),
        H4 => Ok((240, "4hour")),
        H6 => Ok((360, "6hour")),
        H12 => Ok((720, "12hour")),
        D1 => Ok((1440, "1day")),
        W1 => Ok((10080, "1week")),
        _ => Err(value.unsupported("KuCoin")),
    }
}
fn success(status: u16, body: &str) -> Result<Envelope<'_>, String> {
    let envelope = serde_json::from_str::<Envelope>(body)
        .map_err(|e| format!("invalid KuCoin response: {e}"))?;
    if !(200..300).contains(&status) || envelope.code != "200000" {
        return Err(format!(
            "KuCoin API Error {}: {} (HTTP {status})",
            envelope.code, envelope.msg
        ));
    }
    Ok(envelope)
}

impl Kucoin {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        let credentials = if options.public_data_only {
            None
        } else {
            options
                .key
                .as_ref()
                .filter(|v| !v.is_empty())
                .zip(options.secret.as_ref().filter(|v| !v.is_empty()))
                .zip(options.passphrase.as_ref().filter(|v| !v.is_empty()))
                .map(|((key, secret), passphrase)| sign::Credentials {
                    key: key.clone(),
                    secret: secret.clone(),
                    passphrase: passphrase.clone(),
                    broker: options.broker.clone(),
                })
        };
        Self {
            options,
            credentials,
            account: Default::default(),
            trading: Default::default(),
            history: Default::default(),
            funding: Default::default(),
            connections: vec![],
            books: HashMap::new(),
            candles: BTreeMap::new(),
            message_id: 0,
            book_generation: 0,
            discovery: Default::default(),
            discovery_paused_until: 0.0,
            malformed_markets: HashSet::new(),
            reread_stalls: 0,
        }
    }
    pub(crate) fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        pending: Pending,
    ) {
        let read_credentials = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get && self.credentials.is_some())
            .map(|read| sign::Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
                passphrase: read
                    .passphrase
                    .clone()
                    .or_else(|| self.options.passphrase.clone())
                    .unwrap_or_default(),
                broker: self.options.broker.clone(),
            });
        let headers = read_credentials
            .as_ref()
            .or(self.credentials.as_ref())
            .map_or_else(sign::public_headers, |c| {
                sign::headers(c, method, path, body.as_deref().unwrap_or(""), cx.now)
            });
        cx.request(
            method,
            format!("{REST}{}", encoded_path(path)),
            headers,
            body,
            pending,
        );
    }
    pub(crate) fn signed_streamed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        at: ArrayAt,
        pending: Pending,
    ) {
        let read_credentials = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get && self.credentials.is_some())
            .map(|read| sign::Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
                passphrase: read
                    .passphrase
                    .clone()
                    .or_else(|| self.options.passphrase.clone())
                    .unwrap_or_default(),
                broker: self.options.broker.clone(),
            });
        let headers = read_credentials
            .as_ref()
            .or(self.credentials.as_ref())
            .map_or_else(sign::public_headers, |c| {
                sign::headers(c, method, path, body.as_deref().unwrap_or(""), cx.now)
            });
        let request = cx.request(
            method,
            format!("{REST}{}", encoded_path(path)),
            headers,
            body,
            pending,
        );
        cx.stream_response(request, at);
    }
    fn public(&self, cx: &mut Cx<'_, Pending>, path: &str, at: Option<ArrayAt>, pending: Pending) {
        let request = cx.request(
            Method::Get,
            format!("{REST}{}", encoded_path(path)),
            sign::public_request_headers(self.credentials.as_ref(), Method::Get, path, cx.now),
            None,
            pending,
        );
        if let Some(at) = at {
            cx.stream_response(request, at);
        }
    }
    fn token(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.connections[channel];
        if c.token_pending || c.socket.is_some() {
            return;
        }
        // Suppressed public feeds never open; bullets and supervision would only churn retries.
        if !matches!(c.role, Role::Private) && cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        c.token_pending = true;
        c.supervisor.connecting(cx);
        let pending = Pending::Bullet {
            channel,
            generation: c.generation,
        };
        if matches!(self.connections[channel].role, Role::Private) {
            self.signed(cx, Method::Post, "/api/v1/bullet-private", None, pending);
        } else {
            cx.request(
                Method::Post,
                format!("{REST}/api/v1/bullet-public"),
                sign::public_request_headers(
                    self.credentials.as_ref(),
                    Method::Post,
                    "/api/v1/bullet-public",
                    cx.now,
                ),
                None,
                pending,
            );
        }
    }
    fn discovery_read(&self, cx: &mut Cx<'_, Pending>, tickers: bool) {
        self.public(
            cx,
            if tickers {
                "/api/v1/allTickers"
            } else {
                "/api/v1/contracts/active"
            },
            Some(ArrayAt::EnvelopeField("data")),
            if tickers {
                Pending::Tickers {
                    rows: vec![],
                    invalid: false,
                }
            } else {
                Pending::Markets {
                    rows: vec![],
                    malformed: HashSet::new(),
                }
            },
        );
    }
    fn launch(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.connections.is_empty() {
            return;
        }
        self.connections.push(Connection::new(Role::Detail));
        let count = cx.store.markets().len().div_ceil(SNAPSHOT_SHARD).max(1);
        for index in 0..count {
            self.connections
                .push(Connection::new(Role::Snapshot { index, count }));
        }
        if self.credentials.is_some() {
            self.connections.push(Connection::new(Role::Private));
            // REST reconciliation must not depend on the private socket connecting.
            self.account_start(cx);
        } else {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
        }
        for channel in 0..self.connections.len() {
            self.token(cx, channel);
        }
    }
    fn subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        channel: usize,
        topic: &str,
        subscribe: bool,
    ) {
        let c = &self.connections[channel];
        if !c.ready {
            return;
        }
        let Some(socket) = c.socket else {
            return;
        };
        self.message_id += 1;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Subscription<'a> {
            id: u64,
            #[serde(rename = "type")]
            kind: &'a str,
            topic: &'a str,
            private_channel: bool,
            response: bool,
        }
        cx.send(
            socket,
            serde_json::to_string(&Subscription {
                id: self.message_id,
                kind: if subscribe {
                    "subscribe"
                } else {
                    "unsubscribe"
                },
                topic,
                private_channel: matches!(c.role, Role::Private),
                response: true,
            })
            .unwrap(),
        );
    }
    fn detail(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, subscribe: bool) {
        if self.connections.is_empty() {
            return;
        }
        let api = api_symbol(cx.store, symbol);
        for prefix in [
            "/contractMarket/level2",
            "/contractMarket/tickerV2",
            "/contract/instrument",
        ] {
            self.subscription(cx, 0, &format!("{prefix}:{api}"), subscribe);
        }
    }
    fn subscribe_all(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        if !self.connections[channel].ready || self.connections[channel].subscribed {
            return;
        }
        self.connections[channel].subscribed = true;
        match self.connections[channel].role {
            Role::Private => {
                for topic in [
                    "/contractMarket/tradeOrders",
                    "/contractMarket/advancedOrders",
                    "/contract/positionAll",
                    "/contractAccount/wallet",
                    "/contract/marginMode",
                    "/contract/crossLeverage",
                ] {
                    self.subscription(cx, channel, topic, true);
                }
            }
            Role::Snapshot { index, count } => {
                let symbols = cx
                    .store
                    .markets()
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i % count == index)
                    .map(|(_, m)| m.id.clone())
                    .collect::<Vec<_>>();
                for chunk in symbols.chunks(100) {
                    self.subscription(
                        cx,
                        channel,
                        &format!("/contractMarket/snapshot:{}", chunk.join(",")),
                        true,
                    );
                }
            }
            Role::Detail => {
                let books = self.books.keys().cloned().collect::<Vec<_>>();
                for symbol in books {
                    self.detail(cx, &symbol, true);
                    self.fetch_depth(cx, &symbol);
                }
                for (symbol, timeframe) in self.candles.keys().cloned().collect::<Vec<_>>() {
                    if let Ok((_, ws)) = interval(timeframe) {
                        let api = api_symbol(cx.store, &symbol);
                        self.subscription(
                            cx,
                            channel,
                            &format!("/contractMarket/limitCandle:{api}_{ws}"),
                            true,
                        );
                    }
                }
            }
        }
    }
    fn ping(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.connections[channel];
        if !c.ready || c.pong.is_some() {
            return;
        }
        if let Some(socket) = c.socket {
            self.message_id += 1;
            cx.send(
                socket,
                format!(r#"{{"id":{},"type":"ping"}}"#, self.message_id),
            );
            c.pong = Some(cx.set_timer(c.ping_timeout));
        }
    }
    fn reconnect_later(&mut self, cx: &mut Cx<'_, Pending>, channel: usize) {
        let c = &mut self.connections[channel];
        if let Some(socket) = c.socket.take() {
            cx.close(socket);
        }
        c.open = false;
        c.ready = false;
        c.subscribed = false;
        c.token_pending = false;
        c.generation = c.generation.wrapping_add(1);
        if let Some(timer) = c.pong.take() {
            cx.clear_timer(timer);
        }
        c.supervisor.retry(cx);
        if matches!(c.role, Role::Detail) {
            for book in self.books.values_mut() {
                if let Some(timer) = book.retry.take() {
                    cx.clear_timer(timer);
                }
                book.synced = false;
                book.loading = false;
                self.book_generation += 1;
                book.generation = self.book_generation;
                book.buffer.clear();
            }
        }
    }
    fn fetch_depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.connections.first().is_some_and(|c| c.ready) {
            return;
        }
        let Some(book) = self.books.get_mut(symbol) else {
            return;
        };
        if book.loading {
            return;
        }
        if let Some(timer) = book.retry.take() {
            cx.clear_timer(timer);
        }
        book.loading = true;
        self.book_generation += 1;
        book.generation = self.book_generation;
        let generation = book.generation;
        let api = api_symbol(cx.store, symbol);
        self.public(
            cx,
            &format!("/api/v1/level2/snapshot?symbol={api}"),
            None,
            Pending::Depth {
                symbol: symbol.into(),
                generation,
            },
        );
    }
    fn depth_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        generation: u64,
        error: &str,
    ) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if let Some(book) = self
            .books
            .get_mut(symbol)
            .filter(|s| s.generation == generation)
        {
            book.loading = false;
            book.synced = false;
            if book.retry.is_none() {
                book.retry = Some(cx.set_timer(1000.0));
            }
        }
    }
    fn depth_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        generation: u64,
        status: u16,
        body: &str,
    ) {
        let parsed = success(status, body)
            .and_then(|e| e.data.ok_or_else(|| "missing KuCoin depth".into()))
            .and_then(|raw| serde_json::from_str::<Snapshot>(raw.get()).map_err(|e| e.to_string()));
        let snapshot = match parsed {
            Ok(s) => s,
            Err(e) => {
                self.depth_failed(cx, &symbol, generation, &e);
                return;
            }
        };
        let Some(book) = self
            .books
            .get_mut(&symbol)
            .filter(|s| s.generation == generation)
        else {
            return;
        };
        book.loading = false;
        book.book.clear();
        book.sequence = snapshot.sequence;
        book.synced = true;
        for (side, levels) in [
            (BookSide::Bids, snapshot.bids),
            (BookSide::Asks, snapshot.asks),
        ] {
            for (price, amount) in levels {
                book.book.apply(side, price.0, amount.0);
            }
        }
        let replay = std::mem::take(&mut book.buffer);
        for delta in replay {
            if delta.sequence <= book.sequence {
                continue;
            }
            if delta.sequence != book.sequence + 1 {
                book.synced = false;
                book.buffer.push(delta);
                continue;
            }
            if !book.synced {
                book.buffer.push(delta);
                continue;
            }
            book.book.apply(delta.side, delta.price, delta.amount);
            book.sequence = delta.sequence;
        }
        if book.synced {
            cx.book_changed(&symbol);
        } else {
            self.fetch_depth(cx, &symbol);
        }
    }
    fn depth_message(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, data: &RawValue) {
        let Ok(row) = serde_json::from_str::<Increment>(data.get()) else {
            return;
        };
        let mut parts = row.change.split(',');
        let Some(price) = parts
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
        else {
            return;
        };
        let side = match parts.next() {
            Some("buy") => BookSide::Bids,
            Some("sell") => BookSide::Asks,
            _ => return,
        };
        let Some(amount) = parts
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
        else {
            return;
        };
        if parts.next().is_some() {
            return;
        }
        let delta = Delta {
            sequence: row.sequence,
            price,
            side,
            amount,
        };
        let Some(book) = self.books.get_mut(symbol) else {
            return;
        };
        if row.sequence <= book.sequence && book.synced {
            return;
        }
        if book.synced && row.sequence == book.sequence + 1 {
            book.book.apply(side, price, amount);
            book.sequence = row.sequence;
            cx.book_changed(symbol);
            return;
        }
        book.synced = false;
        if book.buffer.len() >= 10000 {
            book.buffer.clear();
            self.book_generation += 1;
            book.generation = self.book_generation;
            book.loading = false;
        }
        book.buffer.push(delta);
        self.fetch_depth(cx, symbol);
    }
    fn ticker_message(&mut self, cx: &mut Cx<'_, Pending>, topic: &str, data: &RawValue) {
        let api = topic.split_once(':').map_or("", |(_, s)| s);
        #[derive(Deserialize)]
        struct InstrumentPrice {
            #[serde(default, rename = "markPrice")]
            mark: Option<Number>,
            #[serde(default, rename = "indexPrice")]
            index: Option<Number>,
        }
        if topic.starts_with("/contract/instrument:") {
            let symbol = internal_symbol(api);
            if let Ok(row) = serde_json::from_str::<InstrumentPrice>(data.get()) {
                cx.store.update_ticker(&symbol, |t| {
                    if let Some(n) = row.mark {
                        t.mark = n.0;
                    }
                    if let Some(n) = row.index {
                        t.index = n.0;
                    }
                });
            }
            return;
        }
        let Ok(row) = serde_json::from_str::<TickerRow>(data.get()) else {
            return;
        };
        let symbol = internal_symbol(row.symbol);
        cx.store.update_ticker(&symbol, |t| {
            if topic.starts_with("/contractMarket/tickerV2:") {
                if let Some(n) = row.best_bid_price {
                    t.bid = n.0;
                }
                if let Some(n) = row.best_ask_price {
                    t.ask = n.0;
                }
            } else if topic.starts_with("/contractMarket/snapshot:") {
                if let Some(n) = row.last_price {
                    t.last = n.0;
                }
                if let Some(n) = row.price_chg_pct {
                    t.percentage = n.0 * 100.0;
                }
                if let Some(n) = row.funding_rate {
                    t.funding_rate = n.0;
                }
                if let Some(n) = row.volume {
                    t.volume = n.0;
                }
                if let Some(n) = row.turnover {
                    t.quote_volume = n.0;
                }
            }
        });
    }
}

impl Venue for Kucoin {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            quantity_units: safe_cex_engine::QuantityUnits::NativePlacement,
            cancel_all: safe_cex_engine::CancelAll::Native,
            account: safe_cex_engine::AccountLookup::Query,
            leverage_query: safe_cex_engine::LeverageQuery::OnReadAndMetadata,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read
                .validate()
                .and_then(|_| read.validate_passphrase(self.options.passphrase.as_deref()))
        {
            cx.notice(Level::Error, code::CONFIG_INVALID, error);
            return;
        }
        if self.options.testnet {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "KuCoin Futures testnet is unsupported",
            );
            return;
        }
        if !self.options.public_data_only && self.credentials.is_none() {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "KuCoin credentials require key, secret and passphrase",
            );
            return;
        }
        self.discovery_read(cx, false);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(channel) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket))
        else {
            return;
        };
        self.connections[channel].open = true;
        self.connections[channel].supervisor.opened(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        let Some(channel) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket) && c.open)
        else {
            return;
        };
        self.connections[channel].supervisor.received(cx.now);
        let Ok(message) = serde_json::from_str::<Message>(text) else {
            return;
        };
        if message.kind == "welcome" {
            if self.connections[channel].ready {
                return;
            }
            self.connections[channel].ready = true;
            let c = &mut self.connections[channel];
            c.supervisor.authenticated(cx);
            c.supervisor.set_ping_interval(cx, Some(c.ping_interval));
            if let Some(timer) = self.connections[channel].pong.take() {
                cx.clear_timer(timer);
            }
            if matches!(self.connections[channel].role, Role::Private) {
                self.account_start(cx);
            }
            self.subscribe_all(cx, channel);
            self.ping(cx, channel);
            return;
        }
        if !self.connections[channel].ready {
            return;
        }
        if message.kind == "pong" {
            if let Some(timer) = self.connections[channel].pong.take() {
                cx.clear_timer(timer);
            }
            return;
        }
        let Some(data) = message.data else {
            return;
        };
        if matches!(self.connections[channel].role, Role::Private) {
            self.account_message(cx, message.topic, data);
            return;
        }
        if message.topic.starts_with("/contractMarket/level2:") {
            let symbol = internal_symbol(message.topic.split_once(':').unwrap().1);
            self.depth_message(cx, &symbol, data);
        } else if message.topic.starts_with("/contractMarket/limitCandle:") {
            let Some((api, ws)) = message
                .topic
                .split_once(':')
                .and_then(|(_, s)| s.rsplit_once('_'))
            else {
                return;
            };
            let symbol = internal_symbol(api);
            let timeframe = self
                .candles
                .keys()
                .find(|(s, i)| s == &symbol && interval(*i).is_ok_and(|(_, v)| v == ws))
                .map(|(_, i)| *i);
            #[derive(Deserialize)]
            struct Row {
                candles: Vec<Number>,
            }
            if let (Some(timeframe), Ok(row)) = (timeframe, serde_json::from_str::<Row>(data.get()))
                && let Some(candle) = wire::candle(&row.candles, false)
            {
                cx.candle(&symbol, timeframe, candle);
            }
        } else {
            self.ticker_message(cx, message.topic, data);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        let Some(channel) = self
            .connections
            .iter()
            .position(|c| c.socket == Some(socket))
        else {
            return;
        };
        self.reconnect_later(cx, channel);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::History(request) => self.history_item(cx, request, item),
            Pending::Funding(request) => self.funding_item(cx, request, item),
            Pending::Trade(request) => self.trading_item(cx, request, item),
            Pending::Markets { rows, malformed } => {
                match serde_json::from_str::<Instrument>(item) {
                    Ok(row) => {
                        if let Some(m) = row.market() {
                            rows.push(m);
                        }
                    }
                    Err(_) => {
                        safe_cex_engine::startup::skipped_row(cx);
                        if let Ok(key) = serde_json::from_str::<InstrumentKey>(item)
                            && !key.symbol.is_empty()
                            && key.in_scope()
                        {
                            malformed.insert(internal_symbol(&key.symbol));
                        }
                    }
                }
            }
            Pending::Tickers { rows, invalid: _ } => {
                match serde_json::from_str::<TickerRow>(item) {
                    Ok(row) => {
                        if let Some(m) = cx.store.market_by_id(row.symbol) {
                            rows.push(row.ticker(m.symbol.clone()));
                        }
                    }
                    Err(_) => safe_cex_engine::startup::skipped_row(cx),
                }
            }
            Pending::Candles { rows, invalid, .. } => {
                match serde_json::from_str::<Vec<Number>>(item)
                    .ok()
                    .and_then(|row| wire::candle(&row, true))
                {
                    Some(c) => rows.push(c),
                    None => *invalid = true,
                }
            }
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Identity(command) = pending {
            self.identity_response(cx, command, response);
            return;
        }
        if let Pending::Leverage(read) = pending {
            self.leverage_response(cx, read, response);
            return;
        }
        let status = response.status;
        let body = response.body;
        if matches!(pending, Pending::Markets { .. } | Pending::Tickers { .. }) {
            let pause = safe_cex_engine::startup::rate_limit_pause_ms(&response, 10_000.0);
            self.discovery_paused_until = self.discovery_paused_until.max(cx.now + pause);
        }
        match pending {
            Pending::Identity(_) | Pending::Leverage(_) => unreachable!(),
            Pending::Account(request) => self.account_response(cx, request, status, body),
            Pending::Trade(request) => self.trading_response(cx, request, status, body),
            Pending::History(request) => self.history_response(cx, request, status, body),
            Pending::Funding(request) => self.funding_response(cx, request, status, body),
            Pending::Depth { symbol, generation } => {
                self.depth_response(cx, symbol, generation, status, body)
            }
            Pending::Bullet {
                channel,
                generation,
            } => {
                if generation != self.connections[channel].generation
                    || !self.connections[channel].token_pending
                {
                    return;
                }
                self.connections[channel].token_pending = false;
                let parsed = success(status, body)
                    .and_then(|e| {
                        e.data
                            .ok_or_else(|| "missing KuCoin websocket token".into())
                    })
                    .and_then(|raw| {
                        serde_json::from_str::<Bullet>(raw.get()).map_err(|e| e.to_string())
                    });
                match parsed {
                    Ok(bullet) => {
                        let Some(server) = bullet.instance_servers.first().filter(|s| {
                            s.endpoint.starts_with("wss://")
                                && s.ping_interval.is_finite()
                                && s.ping_interval >= 1000.0
                                && s.ping_timeout.is_finite()
                                && s.ping_timeout >= 1000.0
                        }) else {
                            cx.notice(
                                Level::Error,
                                code::STREAM_UNAVAILABLE,
                                "invalid KuCoin websocket server",
                            );
                            self.reconnect_later(cx, channel);
                            return;
                        };
                        let c = &mut self.connections[channel];
                        c.ping_interval = server.ping_interval;
                        c.ping_timeout = server.ping_timeout;
                        c.supervisor.set_auth_timeout(cx, Some(server.ping_timeout));
                        let separator = if server.endpoint.contains('?') {
                            '&'
                        } else {
                            '?'
                        };
                        self.message_id += 1;
                        let (role, feed) = match c.role {
                            Role::Detail => (SocketChannel::Public, "detail".into()),
                            Role::Snapshot { index, .. } => {
                                (SocketChannel::Public, format!("snapshot-{index}"))
                            }
                            Role::Private => (SocketChannel::Private, "private".into()),
                        };
                        c.socket = Some(
                            cx.connect(
                                safe_cex_engine::SocketSpec::new(format!(
                                    "{}{separator}token={}&connectId={}-{}",
                                    server.endpoint,
                                    encode(bullet.token),
                                    cx.now as u64,
                                    self.message_id
                                ))
                                .channel(role)
                                .feed(feed, false),
                            ),
                        );
                        c.pong = Some(cx.set_timer(c.ping_timeout));
                    }
                    Err(error) => {
                        cx.notice(Level::Warn, code::STREAM_UNAVAILABLE, error);
                        self.reconnect_later(cx, channel);
                    }
                }
            }
            Pending::Markets { rows, malformed } => {
                if success(status, body).is_err() || rows.is_empty() {
                    let pause = self.discovery_paused_until - cx.now;
                    self.discovery.failed_after(cx, false, pause);
                } else {
                    let refresh = cx.store.loaded().markets;
                    let rows = if refresh {
                        keep_listed(cx.store.markets(), rows)
                    } else {
                        rows
                    };
                    cx.store.set_markets(rows);
                    self.books
                        .retain(|symbol, _| cx.store.market(symbol).is_some());
                    let before = self.malformed_markets.len();
                    self.malformed_markets.extend(malformed);
                    self.malformed_markets
                        .retain(|symbol| cx.store.market(symbol).is_none());
                    let after = self.malformed_markets.len();
                    // A partial catalogue commits so private reads start, then re-reads.
                    if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                        self.discovery.failed(cx, false);
                    } else {
                        self.discovery.succeeded(cx, &false);
                    }
                    if refresh {
                        return;
                    }
                    self.discovery_read(cx, true);
                    if self.credentials.is_some() {
                        self.account_start(cx);
                    }
                }
            }
            Pending::Tickers { rows, invalid } => {
                if success(status, body).is_err() || invalid || rows.is_empty() {
                    let pause = self.discovery_paused_until - cx.now;
                    self.discovery.failed_after(cx, true, pause);
                } else {
                    self.discovery.succeeded(cx, &true);
                    cx.store.set_tickers(rows);
                    self.launch(cx);
                }
            }
            Pending::Candles {
                command,
                rows,
                invalid,
            } => {
                {
                    let result: Result<String, String> = success(status, body).and_then(|_| {
                        if invalid {
                            Err("invalid KuCoin candle row".into())
                        } else {
                            serde_json::to_string(&rows).map_err(|e| e.to_string())
                        }
                    });
                    match result {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    }
                };
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Identity(command) = pending {
            self.identity_failed(cx, command, error);
            return;
        }
        if let Pending::Leverage(read) = pending {
            cx.fail(read.command(), error);
            return;
        }
        match pending {
            Pending::Account(request) => self.account_response(cx, request, 0, error),
            Pending::Trade(request) => self.trading_response(cx, request, 0, error),
            Pending::History(request) => self.history_response(cx, request, 0, error),
            Pending::Funding(request) => self.funding_response(cx, request, 0, error),
            Pending::Depth { symbol, generation } => {
                self.depth_failed(cx, &symbol, generation, error)
            }
            Pending::Bullet {
                channel,
                generation,
            } => {
                if generation != self.connections[channel].generation
                    || !self.connections[channel].token_pending
                {
                    return;
                }
                self.connections[channel].token_pending = false;
                cx.notice(Level::Warn, code::STREAM_UNAVAILABLE, error);
                self.reconnect_later(cx, channel);
            }
            Pending::Candles { command, .. } => cx.fail(command, error),
            Pending::Markets { .. } => {
                let pause = self.discovery_paused_until - cx.now;
                self.discovery.failed_after(cx, false, pause);
            }
            Pending::Tickers { .. } => {
                let pause = self.discovery_paused_until - cx.now;
                self.discovery.failed_after(cx, true, pause);
            }
            Pending::Identity(_) | Pending::Leverage(_) => unreachable!(),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(tickers) = self.discovery.on_timer(timer) {
            self.discovery_read(cx, tickers);
            return;
        }
        if self.account_timer(cx, timer)
            || self.trading_timer(cx, timer)
            || self.history_timer(cx, timer)
            || self.funding_timer(cx, timer)
        {
            return;
        }
        if let Some(symbol) = self
            .books
            .iter()
            .find(|(_, b)| b.retry == Some(timer))
            .map(|(s, _)| s.clone())
        {
            self.books.get_mut(&symbol).unwrap().retry = None;
            self.fetch_depth(cx, &symbol);
            return;
        }
        for channel in 0..self.connections.len() {
            match self.connections[channel].supervisor.on_timer(cx, timer) {
                Tick::Reconnect => {
                    self.token(cx, channel);
                    return;
                }
                Tick::TimedOut(_) => {
                    self.reconnect_later(cx, channel);
                    return;
                }
                Tick::Ping => {
                    self.ping(cx, channel);
                    return;
                }
                Tick::Handled => return,
                Tick::Unhandled => {}
            }
            if self.connections[channel].pong == Some(timer) {
                self.connections[channel].pong = None;
                if let Some(socket) = self.connections[channel].socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
                return;
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::FetchLeverage { command, symbol } => self.fetch_leverage(cx, command, symbol),
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && cx.store.market(&symbol).is_none() {
                    cx.notice(
                        Level::Error,
                        code::SUBSCRIPTION_REJECTED,
                        "unknown KuCoin market",
                    );
                    return;
                }
                if !self.books.contains_key(&symbol) {
                    self.book_generation += 1;
                    self.books.insert(
                        symbol.clone(),
                        BookState {
                            generation: self.book_generation,
                            ..Default::default()
                        },
                    );
                }
                let state = self.books.get_mut(&symbol).unwrap();
                state.subscribers += 1;
                if state.subscribers == 1 {
                    self.detail(cx, &symbol, true);
                    if self.connections.first().is_some_and(|c| c.ready) {
                        self.fetch_depth(cx, &symbol);
                    }
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(book) = self.books.get_mut(&symbol) {
                    book.subscribers = book.subscribers.saturating_sub(1);
                    if book.subscribers == 0 {
                        if let Some(timer) = book.retry.take() {
                            cx.clear_timer(timer);
                        }
                        self.books.remove(&symbol);
                        self.detail(cx, &symbol, false);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: timeframe,
            } => match interval(timeframe) {
                Ok((_, ws)) => {
                    let n = self.candles.entry((symbol.clone(), timeframe)).or_default();
                    *n += 1;
                    if *n == 1 && !self.connections.is_empty() {
                        let api = api_symbol(cx.store, &symbol);
                        self.subscription(
                            cx,
                            0,
                            &format!("/contractMarket/limitCandle:{api}_{ws}"),
                            true,
                        );
                    }
                }
                Err(e) => cx.notice(Level::Error, code::COMMAND_UNSUPPORTED, e),
            },
            Command::UnsubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                let key = (symbol.clone(), timeframe);
                if let Some(n) = self.candles.get_mut(&key) {
                    *n = n.saturating_sub(1);
                    if *n == 0 {
                        self.candles.remove(&key);
                        if let Ok((_, ws)) = interval(timeframe)
                            && !self.connections.is_empty()
                        {
                            let api = api_symbol(cx.store, &symbol);
                            self.subscription(
                                cx,
                                0,
                                &format!("/contractMarket/limitCandle:{api}_{ws}"),
                                false,
                            );
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
                match interval(timeframe) {
                    Ok((minutes, _)) => {
                        let end = to_ms.unwrap_or(cx.now);
                        let start = if let Some(limit) = limit {
                            end - minutes as f64 * 60000.0 * limit.clamp(1, 1000) as f64
                        } else {
                            from_ms.unwrap_or(end - minutes as f64 * 60000.0 * 500.0)
                        };
                        if !start.is_finite() || !end.is_finite() || start < 0.0 || start > end {
                            let Ok(context) = cx.admit_http_command(local_read_command_policy())
                            else {
                                return;
                            };
                            let _ = cx.fail_context(&context, "invalid KuCoin candle range");
                            return;
                        }
                        let api = api_symbol(cx.store, &symbol);
                        self.public(cx,&format!("/api/v1/kline/query?symbol={api}&granularity={minutes}&from={}&to={}",start as u64,end as u64),Some(ArrayAt::EnvelopeField("data")),Pending::Candles{command,rows:vec![],invalid:false});
                    }
                    Err(e) => {
                        let Ok(context) = cx.admit_http_command(local_read_command_policy()) else {
                            return;
                        };
                        let _ = cx.fail_context(&context, e);
                    }
                }
            }
            Command::Query { command, query } => self.history_command(cx, command, query),
            command => {
                let id = match &command {
                    Command::PlaceOrders { command, .. }
                    | Command::CancelOrders { command, .. }
                    | Command::AmendOrder { command, .. }
                    | Command::CancelSymbolOrders { command, .. }
                    | Command::SetLeverage { command, .. }
                    | Command::SetMarginType { command, .. }
                    | Command::SetPositionMode { command, .. } => *command,
                    _ => unreachable!(),
                };
                self.trading_command(cx, id, command);
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|b| b.synced).map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// A refresh keeps every listed market the new catalogue omits or filters out, unchanged.
fn keep_listed(
    listed: &[safe_cex_engine::model::Market],
    rows: Vec<safe_cex_engine::model::Market>,
) -> Vec<safe_cex_engine::model::Market> {
    let mut fresh: Vec<_> = rows.into_iter().map(Some).collect();
    let mut index = std::collections::HashMap::new();
    for (i, market) in fresh.iter().enumerate() {
        if let Some(market) = market {
            index.entry(market.symbol.clone()).or_insert(i);
        }
    }
    let mut merged: Vec<_> = listed
        .iter()
        .map(|old| {
            index
                .get(&old.symbol)
                .and_then(|&i| fresh[i].take())
                .unwrap_or_else(|| old.clone())
        })
        .collect();
    merged.extend(fresh.into_iter().flatten());
    merged
}

/// Whether a partial catalogue re-reads; five refreshes in a row that leave its
/// malformed set no smaller end the retry.
fn reread_partial<P>(
    cx: &mut safe_cex_engine::Cx<'_, P>,
    stalls: &mut u8,
    refresh: bool,
    before: usize,
    after: usize,
) -> bool {
    if after == 0 {
        *stalls = 0;
        return false;
    }
    *stalls = if refresh && after >= before {
        stalls.saturating_add(1)
    } else {
        0
    };
    if *stalls == 5 {
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("KuCoin stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

#[cfg(test)]
mod secondary_tests;
