//! Bitrue stablecoin perpetuals, driven entirely by host events.
mod account;
mod history;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::HashMap;
use wire::{CandleWire, Contract, Message, Payload};
const REST: &str = "https://fapi.bitrue.com";
const PUBLIC: &str = "wss://fmarket-ws.bitrue.com/kline-api/ws";
#[derive(Clone, Debug)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub recv_window: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            read_credentials: None,
            public_data_only: false,
            key: None,
            secret: None,
            recv_window: 20_000,
        }
    }
}
pub struct Pending {
    pub request: sign::Request,
    pub job: Job,
    pub signature_resent: bool,
    pub attempts: u8,
}
pub enum Job {
    Markets {
        rows: Vec<Market>,
        contracts: HashMap<String, ContractInfo>,
        failed: bool,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}
#[derive(Clone, Copy)]
pub struct ContractInfo {
    pub min_order_money: f64,
    pub max_limit_volume: f64,
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: None,
                backoff_base_ms: 1_000.0,
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct DepthState {
    book: Book,
    subscribers: u32,
    received: bool,
    fallback: Option<TimerId>,
    fallback_active: bool,
}
#[derive(Clone)]
enum Channel {
    Ticker(u32),
    Book(String),
    Candle(String, String),
}
enum SocketReadKind {
    Candles {
        from: Option<f64>,
        to: Option<f64>,
        limit: u32,
    },
    Trades {
        limit: u32,
    },
}
struct SocketRead {
    command: CommandId,
    symbol: String,
    kind: SocketReadKind,
    timer: TimerId,
}
pub struct Bitrue {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    contracts: HashMap<String, ContractInfo>,
    public: Connection,
    channels: HashMap<String, Channel>,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, String), u32>,
    reads: HashMap<String, Vec<SocketRead>>,
    startup_retry: Option<TimerId>,
    deferred: Vec<Command>,
}
#[derive(Serialize)]
struct Params<'a> {
    channel: &'a str,
    cb_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "endIdx")]
    end_idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "pageSize")]
    page_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top: Option<u32>,
}
#[derive(Serialize)]
struct Subscription<'a> {
    event: &'a str,
    params: Params<'a>,
}
fn ws_symbol(symbol: &str) -> String {
    format!(
        "e_{}",
        symbol
            .strip_prefix("E-")
            .unwrap_or(symbol)
            .replace('-', "")
            .to_ascii_lowercase()
    )
}
fn interval(s: &str) -> Option<&'static str> {
    Some(match s {
        "1m" => "1min",
        "5m" => "5min",
        "15m" => "15min",
        "30m" => "30min",
        "1h" => "60min",
        "1d" => "1day",
        "1w" => "1week",
        _ => return None,
    })
}
fn topic(symbol: &str, suffix: &str) -> String {
    format!("market_{}_{suffix}", ws_symbol(symbol))
}
// `ds` is a civil Singapore time, not the machine's local timezone.
fn singapore_seconds(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let part = |a, b| s.get(a..b)?.parse::<i64>().ok();
    let (mut y, m, d, h, min, sec) = (
        part(0, 4)?,
        part(5, 7)?,
        part(8, 10)?,
        part(11, 13)?,
        part(14, 16)?,
        part(17, 19)?,
    );
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=12).contains(&m)
        || !(1..=days[(m - 1) as usize]).contains(&d)
        || h > 23
        || min > 59
        || sec > 59
        || h < 0
        || min < 0
        || sec < 0
    {
        return None;
    }
    y -= i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yo = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yo * 365 + yo / 4 - yo / 100 + doy;
    Some(((era * 146097 + doe - 719468) * 86400 + h * 3600 + min * 60 + sec - 28800) as f64)
}
fn candle(row: &CandleWire<'_>, multiplier: f64) -> Option<Candle> {
    let timestamp = if row.ds.is_empty() {
        let t = row.id.0;
        if t > 1e12 { t / 1000.0 } else { t }
    } else {
        singapore_seconds(&row.ds)?
    };
    (timestamp > 0.0).then_some(Candle {
        timestamp,
        open: row.open.0,
        high: row.high.0,
        low: row.low.0,
        close: row.close.0,
        volume: row.vol.0 * multiplier,
    })
}
impl Bitrue {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            contracts: HashMap::new(),
            public: Connection::default(),
            channels: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            reads: HashMap::new(),
            startup_retry: None,
            deferred: Vec::new(),
        }
    }
    fn authenticated(&self) -> bool {
        !self.options.public_data_only
            && self.options.key.as_ref().is_some_and(|s| !s.is_empty())
            && self.options.secret.as_ref().is_some_and(|s| !s.is_empty())
    }
    fn resolve_symbol(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        if cx.store.market(symbol).is_some() {
            return Some(symbol.to_owned());
        }
        let flat = |s: &str| {
            s.strip_prefix("E-")
                .or_else(|| s.strip_prefix("E_"))
                .unwrap_or(s)
                .replace(['-', '_', '/'], "")
                .to_ascii_uppercase()
        };
        let key = flat(symbol);
        cx.store
            .markets()
            .iter()
            .find(|m| flat(&m.symbol) == key)
            .map(|m| m.symbol.clone())
    }
    fn send(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        at: Option<ArrayAt>,
        job: Job,
    ) {
        let pending = Pending {
            request: sign::Request {
                method,
                path: path.to_owned(),
                body,
                at,
                timestamp: cx.now.max(0.0) as u64,
            },
            job,
            signature_resent: false,
            attempts: 0,
        };
        self.issue(cx, pending);
    }
    fn issue(&self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let r = &pending.request;
        let primary_read = r.method == Method::Get && matches!(&pending.job, Job::Markets { .. });
        let path = if r.path.starts_with("https://") {
            r.path
                .split_once(".com")
                .map_or(r.path.as_str(), |(_, p)| p)
        } else {
            r.path.as_str()
        };
        let url = if r.path.starts_with("https://") {
            r.path.clone()
        } else {
            format!("{REST}{}", r.path)
        };
        let mut headers = vec![
            ("Content-Type", "application/json".to_owned()),
            ("X-Referer", "Tealstreet".to_owned()),
        ];
        if let (Some(key), Some(secret)) = (&self.options.key, &self.options.secret) {
            let read = self
                .options
                .read_credentials
                .as_ref()
                .filter(|_| r.method == Method::Get && !primary_read);
            let key = read.map_or(key.as_str(), |r| r.key.as_str());
            let secret = read.map_or(secret.as_str(), |r| r.secret.as_str());
            headers.extend([
                ("X-CH-APIKEY", key.to_owned()),
                ("X-CH-TS", r.timestamp.to_string()),
                ("recvWindow", self.options.recv_window.to_string()),
                (
                    "X-CH-SIGN",
                    sign::signature(
                        secret,
                        r.timestamp,
                        r.method,
                        path,
                        r.body.as_deref().unwrap_or(""),
                    ),
                ),
            ]);
        }
        let (method, body, at) = (r.method, r.body.clone(), r.at);
        let id = cx.request(method, url, headers, body, pending);
        if primary_read {
            cx.route_request(id, safe_cex_engine::RequestRoute::Primary);
        }
        if let Some(at) = at {
            cx.stream_response(id, at);
        }
    }
    fn reset_job(job: &mut Job) {
        match job {
            Job::Markets {
                rows,
                contracts,
                failed,
            } => {
                rows.clear();
                contracts.clear();
                *failed = false;
            }
            Job::Account(r) => r.reset(),
            Job::History(r) => r.reset(),
            Job::Trade(_) => {}
        }
    }
    fn markets(&self, cx: &mut Cx<'_, Pending>) {
        self.send(
            cx,
            Method::Get,
            "/fapi/v1/contracts",
            None,
            Some(ArrayAt::Root),
            Job::Markets {
                rows: Vec::new(),
                contracts: HashMap::new(),
                failed: false,
            },
        );
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.supervisor.connecting(cx);
        self.public.socket = Some(cx.connect_channel(PUBLIC, SocketChannel::Public));
    }
    fn subscription(
        &self,
        cx: &mut Cx<'_, Pending>,
        event: &str,
        channel: &str,
        end: Option<f64>,
        size: Option<u32>,
        top: Option<u32>,
    ) {
        if let Some(socket) = self.public.socket.filter(|_| self.public.open) {
            let text = serde_json::to_string(&Subscription {
                event,
                params: Params {
                    channel,
                    cb_id: "engine",
                    end_idx: end.map(|v| (v / 1000.0).floor().to_string()),
                    page_size: size,
                    top,
                },
            })
            .unwrap();
            cx.send(socket, text);
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let channel = topic(symbol, "simple_depth_step0");
        self.channels
            .insert(channel.clone(), Channel::Book(symbol.to_owned()));
        self.subscription(cx, "sub", &channel, None, None, None);
        if let Some(depth) = self.books.get_mut(symbol) {
            depth.received = false;
            depth.fallback_active = false;
            if let Some(t) = depth.fallback.take() {
                cx.clear_timer(t);
            }
            depth.fallback = Some(cx.set_timer(3000.0));
        }
        cx.store.update_ticker(symbol, |t| {
            t.bid = 0.0;
            t.ask = 0.0;
        });
    }
    fn send_socket_read(&self, cx: &mut Cx<'_, Pending>, channel: &str) {
        let Some(request) = self.reads.get(channel).and_then(|v| v.first()) else {
            return;
        };
        let Some(socket) = self.public.socket.filter(|_| self.public.open) else {
            return;
        };
        let (end_idx, page_size, top) = match &request.kind {
            SocketReadKind::Candles { to, limit, .. } => (
                to.map(|v| (v / 1000.0).floor().to_string()),
                Some((*limit).min(300)),
                None,
            ),
            SocketReadKind::Trades { limit } => (None, None, Some(*limit)),
        };
        let id = format!("engine-{}", request.timer.0);
        cx.send(
            socket,
            serde_json::to_string(&Subscription {
                event: "req",
                params: Params {
                    channel,
                    cb_id: &id,
                    end_idx,
                    page_size,
                    top,
                },
            })
            .unwrap(),
        );
    }
    fn public_read(&mut self, cx: &mut Cx<'_, Pending>, channel: String, request: SocketRead) {
        if self.reads.values().map(Vec::len).sum::<usize>() >= 32 {
            cx.clear_timer(request.timer);
            cx.done(
                request.command,
                Err("Too many Bitrue public requests".into()),
            );
            return;
        }
        let queue = self.reads.entry(channel.clone()).or_default();
        queue.push(request);
        if queue.len() == 1 {
            self.send_socket_read(cx, &channel);
        }
    }
    fn public_trades(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    ) {
        let Some(symbol) = self.resolve_symbol(cx, &symbol) else {
            cx.done(command, Err("Unknown Bitrue market".into()));
            return;
        };
        if !self.public.open {
            cx.done(command, Err("Bitrue public socket not connected".into()));
            return;
        }
        let limit = limit.unwrap_or(100).clamp(1, 300);
        let timer = cx.set_timer(5000.0);
        self.public_read(
            cx,
            topic(&symbol, "trade_ticker"),
            SocketRead {
                command,
                symbol,
                kind: SocketReadKind::Trades { limit },
                timer,
            },
        );
    }
    fn reply(&mut self, cx: &mut Cx<'_, Pending>, message: &Message<'_>, text: &str) {
        let Some(queue) = self.reads.get_mut(message.channel.as_ref()) else {
            return;
        };
        if queue.is_empty() {
            return;
        }
        if !message.cb_id.is_empty() && message.cb_id != format!("engine-{}", queue[0].timer.0) {
            return;
        }
        let request = queue.remove(0);
        {
            cx.clear_timer(request.timer);
            let multiplier = cx
                .store
                .market(&request.symbol)
                .map_or(1.0, |m| m.precision.amount);
            let result = match request.kind {
                SocketReadKind::Trades { limit } => {
                    history::trade_result(&request.symbol, multiplier, text, limit)
                }
                SocketReadKind::Candles { from, to, limit } => {
                    if !message.status.is_empty() && message.status != "ok" {
                        Err("Bitrue candle request rejected".to_owned())
                    } else {
                        message
                            .data
                            .ok_or_else(|| "Missing Bitrue candle data".to_owned())
                            .and_then(|data| {
                                let rows: Vec<CandleWire<'_>> =
                                    serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                                let candles = rows
                                    .iter()
                                    .map(|row| {
                                        candle(row, multiplier).ok_or_else(|| {
                                            "Malformed Bitrue candle timestamp".to_owned()
                                        })
                                    })
                                    .collect::<Result<Vec<_>, _>>()?;
                                let mut result: Vec<_> = candles
                                    .into_iter()
                                    .filter(|c| {
                                        from.is_none_or(|f| c.timestamp * 1000.0 >= f)
                                            && to.is_none_or(|t| c.timestamp * 1000.0 <= t)
                                    })
                                    .collect();
                                result.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                                result.dedup_by(|a, b| a.timestamp == b.timestamp);
                                if result.len() > limit as usize {
                                    result.drain(..result.len() - limit as usize);
                                }
                                serde_json::to_string(&result).map_err(|e| e.to_string())
                            })
                    }
                }
            };
            cx.done(request.command, result);
        }
        self.reads.retain(|_, v| !v.is_empty());
        self.send_socket_read(cx, &message.channel);
    }
}
impl Venue for Bitrue {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.markets(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        let markets: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, m)| (i as u32, m.symbol.clone()))
            .collect();
        for (i, symbol) in markets {
            let channel = topic(&symbol, "ticker");
            self.channels.insert(channel.clone(), Channel::Ticker(i));
            self.subscription(cx, "sub", &channel, None, None, None);
        }
        for symbol in self.books.keys().cloned().collect::<Vec<_>>() {
            self.subscribe_book(cx, &symbol);
        }
        for (symbol, i) in self.candles.keys() {
            let channel = topic(symbol, &format!("kline_{}", interval(i).unwrap()));
            self.channels
                .insert(channel.clone(), Channel::Candle(symbol.clone(), i.clone()));
            self.subscription(cx, "sub", &channel, None, None, None);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        self.public.supervisor.received(cx.now);
        if message.ping.is_some() || message.event == "ping" {
            cx.send(socket, "pong");
            if let Some(ping) = message.ping {
                cx.send(socket, format!(r#"{{"pong":{}}}"#, ping.get()));
            }
            return;
        }
        if message.event_rep == "rep" {
            self.reply(cx, &message, text);
            return;
        }
        let data = match message.tick {
            Some(tick) => tick,
            None => {
                let Some(data) = message.data else {
                    return;
                };
                let Ok(tick) = serde_json::from_str::<Payload<'_>>(data.get()) else {
                    return;
                };
                tick
            }
        };
        match self.channels.get(message.channel.as_ref()) {
            Some(Channel::Ticker(index)) => {
                let multiplier = cx
                    .store
                    .markets()
                    .get(*index as usize)
                    .map_or(1.0, |m| m.precision.amount);
                cx.store.update_ticker_at(*index, |t| {
                    t.last = data.close.unwrap_or_default().0;
                    t.mark = data.close.unwrap_or_default().0;
                    t.percentage = data.rose.0 * 100.0;
                    t.volume = data.vol.0 * multiplier;
                    t.quote_volume = data.amount.0 * multiplier;
                });
            }
            Some(Channel::Book(symbol)) => {
                let (Some(buys), Some(asks)) = (data.buys, data.asks) else {
                    return;
                };
                if buys
                    .iter()
                    .chain(&asks)
                    .any(|r| r[0].0 <= 0.0 || r[1].0 < 0.0)
                {
                    return;
                }
                let multiplier = cx.store.market(symbol).map_or(1.0, |m| m.precision.amount);
                if let Some(state) = self.books.get_mut(symbol) {
                    state.book.load_sorted(
                        BookSide::Bids,
                        buys.iter().map(|r| (r[0].0, r[1].0 * multiplier)),
                    );
                    state.book.load_sorted(
                        BookSide::Asks,
                        asks.iter().map(|r| (r[0].0, r[1].0 * multiplier)),
                    );
                    state.received = true;
                    if let Some(timer) = state.fallback.take() {
                        cx.clear_timer(timer);
                    }
                    let (bid, ask) = (
                        state.book.best_bid().unwrap_or(0.0),
                        state.book.best_ask().unwrap_or(0.0),
                    );
                    cx.store.update_ticker(symbol, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                    cx.book_changed(symbol);
                }
            }
            Some(Channel::Candle(symbol, i)) => {
                if let Some(row) = data.into_candle() {
                    let multiplier = cx.store.market(symbol).map_or(1.0, |m| m.precision.amount);
                    if let Some(c) = candle(&row, multiplier) {
                        cx.candle(symbol, i, c);
                    }
                }
            }
            None => {}
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
        self.channels.clear();
        for reads in self.reads.drain().map(|(_, v)| v) {
            for read in reads {
                cx.clear_timer(read.timer);
                cx.done(
                    read.command,
                    Err("Bitrue public socket disconnected".into()),
                );
            }
        }
        for (symbol, state) in &mut self.books {
            state.book.clear();
            state.received = false;
            if let Some(t) = state.fallback.take() {
                cx.clear_timer(t);
            }
            cx.book_changed(symbol);
        }
        self.public.supervisor.retry(cx);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match &mut pending.job {
            Job::Markets {
                rows,
                contracts,
                failed,
            } => match serde_json::from_str::<Contract<'_>>(item) {
                Ok(row) => {
                    let parts: Vec<_> = row.symbol.split('-').collect();
                    if parts.len() != 3
                        || row.multiplier.0 <= 0.0
                        || row.max_market_volume.0 <= 0.0
                        || row.price_precision > 18
                    {
                        *failed = true;
                        return;
                    }
                    contracts.insert(
                        row.symbol.to_string(),
                        ContractInfo {
                            min_order_money: row.min_order_money.0,
                            max_limit_volume: row.max_limit_volume.0 * row.multiplier.0,
                        },
                    );
                    rows.push(Market {
                        id: row.symbol.to_string(),
                        symbol: row.symbol.to_string(),
                        pretty_symbol: None,
                        base: parts[1].to_owned(),
                        quote: parts[2].to_owned(),
                        active: row.status.0 == 1.0,
                        precision: Precision {
                            amount: row.multiplier.0,
                            price: 10f64.powi(-(row.price_precision as i32)),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: row.min_order_volume.0 * row.multiplier.0,
                                max: row.max_market_volume.0 * row.multiplier.0,
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: 100.0,
                            },
                            price: None,
                        },
                        contract_size: Some(row.multiplier.0),
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: Some(0.001),
                        maker_fee: Some(0.001),
                    });
                }
                Err(_) => *failed = true,
            },
            Job::Account(r) => self.account_item(cx, r, item),
            Job::Trade(r) => self.trade_item(cx, r, item),
            Job::History(r) => self.history_item(cx, r, item),
        }
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if !pending.signature_resent
            && serde_json::from_str::<wire::Envelope<'_>>(response.body)
                .is_ok_and(|e| &*e.code == "-1022")
        {
            pending.signature_resent = true;
            Self::reset_job(&mut pending.job);
            pending.request.timestamp = (cx.now.max(0.0) as u64).max(pending.request.timestamp + 1);
            self.issue(cx, pending);
            return;
        }
        match pending.job {
            Job::Markets {
                rows,
                contracts,
                failed,
            } => {
                if failed || !(200..300).contains(&response.status) || rows.is_empty() {
                    cx.log(LogLevel::Error, "Bitrue contracts read failed");
                    self.startup_retry = Some(cx.set_timer(2000.0));
                    return;
                }
                self.contracts = contracts;
                let tickers = rows
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Ticker::default()
                    })
                    .collect();
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect(cx);
                for command in std::mem::take(&mut self.deferred) {
                    self.on_command(cx, command);
                }
                if self.authenticated() {
                    self.start_private(cx);
                }
            }
            Job::Account(r) => self.account_response(cx, r, response),
            Job::Trade(r) => self.trade_response(cx, r, response),
            Job::History(r) => self.history_response(cx, r, response),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        let rejection = error
            .strip_prefix("response did not contain the expected list: ")
            .is_some_and(|body| {
                serde_json::from_str::<wire::Envelope<'_>>(body).is_ok_and(|e| &*e.code == "-1022")
            });
        if rejection && !pending.signature_resent {
            pending.signature_resent = true;
            Self::reset_job(&mut pending.job);
            pending.request.timestamp = (cx.now.max(0.0) as u64).max(pending.request.timestamp + 1);
            self.issue(cx, pending);
            return;
        }
        let safe = pending.request.method == Method::Get
            || (pending.request.method == Method::Post
                && pending.request.path
                    == "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey");
        if !rejection
            && safe
            && pending.attempts < 3
            && !error.starts_with("response did not contain")
            && !error.contains("UTF-8")
        {
            pending.attempts += 1;
            Self::reset_job(&mut pending.job);
            pending.request.timestamp = (cx.now.max(0.0) as u64).max(pending.request.timestamp + 1);
            self.issue(cx, pending);
            return;
        }
        match pending.job {
            Job::Markets { .. } => {
                cx.log(LogLevel::Error, format!("Bitrue contracts: {error}"));
                self.startup_retry = Some(cx.set_timer(2000.0));
            }
            Job::Account(r) => self.account_failed(cx, r, error),
            Job::Trade(r) => self.trade_failed(cx, r, error),
            Job::History(r) => self.history_failed(cx, r, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            self.markets(cx);
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => {
                self.connect(cx);
                return;
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
                return;
            }
            Tick::Handled | Tick::Ping => return,
            Tick::Unhandled => {}
        }
        let fallback = self
            .books
            .iter()
            .find_map(|(s, b)| (b.fallback == Some(timer)).then(|| s.clone()));
        if let Some(symbol) = fallback {
            let depth = self.books.get_mut(&symbol).unwrap();
            depth.fallback = None;
            if !depth.received {
                depth.fallback_active = true;
                let channel = topic(&symbol, "depth_step0");
                self.channels.insert(channel.clone(), Channel::Book(symbol));
                self.subscription(cx, "sub", &channel, None, None, None);
            }
            return;
        }
        if self
            .reads
            .values()
            .any(|reads| reads.iter().any(|r| r.timer == timer))
        {
            // A reply without cb_id cannot be attributed after a timeout.
            if let Some(socket) = self.public.socket {
                cx.close(socket);
                self.on_close(cx, socket, 1006);
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if cx.store.markets().is_empty()
            && matches!(
                command,
                Command::SubscribeOrderBook { .. }
                    | Command::UnsubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeCandles { .. }
            )
        {
            self.deferred.push(command);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    let depth = self.books.entry(symbol.clone()).or_default();
                    depth.subscribers += 1;
                    if depth.subscribers == 1 && self.public.open {
                        self.subscribe_book(cx, &symbol);
                    }
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    if let Some(depth) = self.books.get_mut(&symbol) {
                        depth.subscribers = depth.subscribers.saturating_sub(1);
                        if depth.subscribers > 0 {
                            return;
                        }
                    }
                    if let Some(depth) = self.books.remove(&symbol) {
                        if let Some(t) = depth.fallback {
                            cx.clear_timer(t);
                        }
                        for suffix in ["simple_depth_step0", "depth_step0"] {
                            let channel = topic(&symbol, suffix);
                            if self.channels.remove(&channel).is_some() {
                                self.subscription(cx, "unsub", &channel, None, None, None);
                            }
                        }
                        cx.book_changed(&symbol);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let (Some(symbol), Some(mapped)) =
                    (self.resolve_symbol(cx, &symbol), interval(&i))
                {
                    let count = self.candles.entry((symbol.clone(), i.clone())).or_default();
                    *count += 1;
                    if *count == 1 {
                        let channel = topic(&symbol, &format!("kline_{mapped}"));
                        self.channels
                            .insert(channel.clone(), Channel::Candle(symbol, i));
                        self.subscription(cx, "sub", &channel, None, None, None);
                    }
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    if let Some(count) = self.candles.get_mut(&(symbol.clone(), i.clone())) {
                        *count = count.saturating_sub(1);
                        if *count > 0 {
                            return;
                        }
                    }
                    self.candles.remove(&(symbol.clone(), i.clone()));
                    if let Some(mapped) = interval(&i) {
                        let channel = topic(&symbol, &format!("kline_{mapped}"));
                        self.channels.remove(&channel);
                        self.subscription(cx, "unsub", &channel, None, None, None);
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let (Some(symbol), Some(mapped)) = (self.resolve_symbol(cx, &symbol), interval(&i))
                else {
                    cx.done(
                        command,
                        Err("Unknown Bitrue market or candle interval".into()),
                    );
                    return;
                };
                if !self.public.open {
                    cx.done(command, Err("Bitrue public socket not connected".into()));
                    return;
                }
                if from_ms.is_some_and(|v| !v.is_finite())
                    || to_ms.is_some_and(|v| !v.is_finite())
                    || matches!((from_ms,to_ms),(Some(f),Some(t))if f>t)
                {
                    cx.done(command, Err("Invalid candle range".into()));
                    return;
                }
                let timer = cx.set_timer(5000.0);
                self.public_read(
                    cx,
                    topic(&symbol, &format!("kline_{mapped}")),
                    SocketRead {
                        command,
                        symbol,
                        kind: SocketReadKind::Candles {
                            from: from_ms,
                            to: to_ms,
                            limit: limit.unwrap_or(300).clamp(1, 300),
                        },
                        timer,
                    },
                );
            }
            Command::Query { command, query } => self.history_query(cx, command, query),
            command => {
                if !self.account_command(cx, &command) {
                    self.trade_command(cx, command);
                }
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.received)
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.received)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;
