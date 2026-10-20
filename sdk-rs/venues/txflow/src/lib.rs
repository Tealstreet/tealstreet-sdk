//! TxFlow testnet perpetuals, using separate node and API-service feeds.
mod account;
mod history;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use wire::*;

pub const REST_URL: &str = "https://rpc.txflow-testnet.xyz";
pub const API_VERSION: &str = "2026-08-16";
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub vault: Option<String>,
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        if !self.testnet {
            return Err("TxFlow mainnet is offline; testnet is required".into());
        }
        if let Some(vault) = self.vault.as_deref() {
            safe_cex_crypto::ethereum::get_address(vault)?;
        }
        if !self.public_data_only {
            safe_cex_crypto::ethereum::get_address(self.application_id.as_deref().unwrap_or(""))?;
            safe_cex_crypto::ethereum::private_key_address(self.secret.as_deref().unwrap_or(""))?;
        }
        Ok(())
    }
}
pub enum Pending {
    Meta {
        rows: Vec<Instrument>,
        valid: bool,
    },
    Ticker {
        asset: u32,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        valid: bool,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    ping: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
#[derive(Default)]
struct Ladder {
    subscribers: usize,
    book: Book,
    time: Option<f64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Sub<'a> {
    r#type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interval: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instrument_id: Option<u32>,
}
pub struct TxFlow {
    pub(crate) options: Options,
    pub(crate) account: account::AccountState,
    pub(crate) trading: trading::Trading,
    pub(crate) assets: HashMap<String, u32>,
    pub(crate) coins: HashMap<String, String>,
    pub(crate) instruments: HashMap<u32, Instrument>,
    node: Connection,
    api: Connection,
    books: HashMap<String, Ladder>,
    candles: HashMap<(String, String), usize>,
    ticker_queue: VecDeque<u32>,
    seed_tickers: Vec<Ticker>,
    ticker_timer: Option<TimerId>,
    meta_timer: Option<TimerId>,
}
impl TxFlow {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: Default::default(),
            trading: Default::default(),
            assets: Default::default(),
            coins: Default::default(),
            instruments: Default::default(),
            node: Default::default(),
            api: Default::default(),
            books: Default::default(),
            candles: Default::default(),
            ticker_queue: Default::default(),
            seed_tickers: Vec::new(),
            ticker_timer: None,
            meta_timer: None,
        }
    }
    pub(crate) fn rest(&self) -> &'static str {
        REST_URL
    }
    pub(crate) fn asset(&self, _cx: &Cx<'_, Pending>, symbol: &str) -> Option<u32> {
        self.assets.get(symbol).copied()
    }
    pub(crate) fn symbol(&self, asset: u32) -> Option<&str> {
        self.coins.get(&asset.to_string()).map(String::as_str)
    }
    pub(crate) fn coin_symbol(&self, coin: &str) -> Option<&str> {
        self.coins
            .get(&coin.to_ascii_lowercase())
            .map(String::as_str)
    }
    pub(crate) fn info(
        &self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        at: ArrayAt,
        pending: Pending,
    ) {
        #[derive(serde::Deserialize)]
        struct Kind<'a> {
            r#type: &'a str,
        }
        let public = serde_json::from_str::<Kind>(&body).is_ok_and(|k| {
            matches!(
                k.r#type,
                "perpMeta"
                    | "activeAssetCtx"
                    | "candleSnapshot"
                    | "l2Book"
                    | "recentTrades"
                    | "trades"
            )
        });
        let id = cx.request_streamed_body(
            Method::Post,
            format!("{}/info", self.rest()),
            vec![
                ("Content-Type", "application/json".into()),
                ("x-api-version", API_VERSION.into()),
            ],
            Some(body),
            at,
            pending,
        );
        if public {
            cx.route_request(id, safe_cex_engine::RequestRoute::PublicCors);
        }
    }
    fn metadata(&self, cx: &mut Cx<'_, Pending>) {
        self.info(
            cx,
            "{\"type\":\"perpMeta\",\"includeDelisted\":false}".into(),
            ArrayAt::Path(&["data", "universe"]),
            Pending::Meta {
                rows: Vec::new(),
                valid: true,
            },
        );
    }
    fn next_ticker(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(asset) = self.ticker_queue.pop_front() {
            let id = cx.request(
                Method::Post,
                format!("{}/info", self.rest()),
                vec![
                    ("Content-Type", "application/json".into()),
                    ("x-api-version", API_VERSION.into()),
                ],
                Some(format!(
                    "{{\"type\":\"activeAssetCtx\",\"coin\":\"{asset}\"}}"
                )),
                Pending::Ticker { asset },
            );
            cx.route_request(id, safe_cex_engine::RequestRoute::PublicCors);
        } else {
            cx.store.set_tickers(std::mem::take(&mut self.seed_tickers));
            self.open_sockets(cx);
        }
    }
    fn open_sockets(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.node.socket.is_none() {
            self.node.socket = Some(cx.connect_channel(
                format!("{}/ws", REST_URL.replacen("https:", "wss:", 1)),
                if self.options.public_data_only {
                    SocketChannel::Public
                } else {
                    SocketChannel::Private
                },
            ));
        }
        if self.api.socket.is_none() {
            self.api.socket = Some(cx.connect_channel(
                format!("{}/api-ws", REST_URL.replacen("https:", "wss:", 1)),
                SocketChannel::Public,
            ));
        }
        self.account_start(cx);
    }
    fn subscribe(&self, cx: &mut Cx<'_, Pending>, api: bool, method: &str, subscription: Sub<'_>) {
        #[derive(Serialize)]
        struct Frame<'a> {
            method: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            r#type: Option<&'a str>,
            subscription: Sub<'a>,
        }
        let conn = if api { &self.api } else { &self.node };
        if conn.open
            && let Some(socket) = conn.socket
        {
            cx.send(
                socket,
                serde_json::to_string(&Frame {
                    method,
                    r#type: if api { None } else { Some(subscription.r#type) },
                    subscription,
                })
                .unwrap(),
            );
        }
    }
    pub(crate) fn node_subscribe(&self, cx: &mut Cx<'_, Pending>, kind: &str, user: &str) {
        self.subscribe(
            cx,
            false,
            "subscribe",
            Sub {
                r#type: kind,
                coin: None,
                interval: None,
                user: Some(user),
                instrument_id: None,
            },
        );
    }
    fn context(cx: &mut Cx<'_, Pending>, symbol: &str, c: Context) {
        cx.store
            .update_ticker(symbol, |t| Self::context_update(t, c));
    }
    fn context_update(t: &mut Ticker, c: Context) {
        t.mark = c.mark_px;
        t.index = c.oracle_px;
        t.open_interest = c.open_interest * c.mark_px;
        t.funding_rate = c.funding / 100.0;
        t.funding_time = c.next_funding_time.unwrap_or(0.0);
        if c.mid_px > 0.0 {
            t.mid_price = Some(c.mid_px);
        }
        if c.last_price > 0.0 {
            t.last = c.last_price;
        }
        if c.prev_day_px > 0.0 {
            t.percentage = (t.last - c.prev_day_px) / c.prev_day_px * 100.0;
        }
        t.volume = c.day_base_vlm;
        t.quote_volume = c.day_ntl_vlm;
    }
    fn ticker_update(t: &mut Ticker, row: TickerRow, seed: bool) {
        t.last = row.last_price;
        t.volume = row.coin_vlm;
        t.quote_volume = row.day_ntl_vlm;
        t.percentage = if row.prev_day_px > 0.0 {
            (row.last_price - row.prev_day_px) / row.prev_day_px * 100.0
        } else {
            0.0
        };
        if seed {
            t.mark = row.mark_px;
            t.index = row.oracle_px;
            t.open_interest = row.open_interest * row.mark_px;
        }
        if let Some(ctx) = row.node_ctx {
            Self::context_update(t, ctx);
        }
    }
    fn apply_ticker(cx: &mut Cx<'_, Pending>, symbol: &str, row: TickerRow, seed: bool) {
        cx.store
            .update_ticker(symbol, |t| Self::ticker_update(t, row, seed));
    }
    fn public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        match frame.channel.as_ref() {
            "allAssetCtxs" => {
                if let Ok(rows) = serde_json::from_str::<Contexts>(frame.data.get()) {
                    for row in rows.ctxs {
                        if let Some(symbol) = self.coin_symbol(&row.coin) {
                            Self::context(cx, symbol, row.ctx);
                        }
                    }
                }
            }
            "activeAssetCtx" => {
                if let Ok(row) = serde_json::from_str::<ContextFrame>(frame.data.get())
                    && let Some(symbol) = self.coin_symbol(&row.coin)
                {
                    Self::context(cx, symbol, row.ctx);
                }
            }
            "ticker" => {
                if let Ok(row) = serde_json::from_str::<TickerRow>(frame.data.get())
                    && let Some(symbol) = self.symbol(row.instrument_id)
                {
                    Self::apply_ticker(cx, symbol, row, false);
                }
            }
            "l2Book" => {
                if let Ok(row) = serde_json::from_str::<Depth>(frame.data.get())
                    && let Some(symbol) = self.coin_symbol(&row.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                    && ladder.time.is_none_or(|t| row.time >= t)
                {
                    if row
                        .levels
                        .iter()
                        .flatten()
                        .any(|l| l.px <= 0.0 || l.sz < 0.0)
                    {
                        return;
                    }
                    for (index, side) in [BookSide::Bids, BookSide::Asks].into_iter().enumerate() {
                        ladder.book.load_sorted(
                            side,
                            row.levels[index]
                                .iter()
                                .filter(|l| l.sz > 0.0)
                                .map(|l| (l.px, l.sz)),
                        );
                    }
                    ladder.time = Some(row.time);
                    cx.book_changed(&symbol);
                }
            }
            "candle" => {
                let mut apply = |row: CandleRow<'_>| {
                    if let Some(symbol) = self.coin_symbol(&row.s)
                        && self
                            .candles
                            .contains_key(&(symbol.to_owned(), row.i.to_string()))
                    {
                        cx.candle(symbol, &row.i, row.candle());
                    }
                };
                let raw = frame.data.get();
                if raw.starts_with('[') {
                    let _ = json::each(raw, &mut apply);
                } else if let Ok(row) = serde_json::from_str(raw) {
                    apply(row);
                }
            }
            _ => {}
        }
    }
    fn book_sub(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, add: bool) {
        if cx.store.loaded().markets && !self.assets.contains_key(&symbol) {
            return;
        }
        let ladder = self.books.entry(symbol.clone()).or_default();
        if add {
            ladder.subscribers += 1;
            if ladder.subscribers > 1 {
                return;
            }
        } else {
            ladder.subscribers = ladder.subscribers.saturating_sub(1);
            if ladder.subscribers > 0 {
                return;
            }
            self.books.remove(&symbol);
        }
        if let Some(asset) = self.assets.get(&symbol) {
            self.subscribe(
                cx,
                false,
                if add { "subscribe" } else { "unsubscribe" },
                Sub {
                    r#type: "l2Book",
                    coin: Some(&asset.to_string()),
                    interval: None,
                    user: None,
                    instrument_id: None,
                },
            );
        }
    }
    fn candle_sub(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: String,
        interval: String,
        add: bool,
    ) {
        if !valid_interval(&interval)
            || (cx.store.loaded().markets && !self.assets.contains_key(&symbol))
        {
            return;
        }
        let count = self
            .candles
            .entry((symbol.clone(), interval.clone()))
            .or_default();
        if add {
            *count += 1;
            if *count > 1 {
                return;
            }
        } else {
            *count = count.saturating_sub(1);
            if *count > 0 {
                return;
            }
            self.candles.remove(&(symbol.clone(), interval.clone()));
        }
        if let Some(asset) = self.assets.get(&symbol) {
            self.subscribe(
                cx,
                true,
                if add { "subscribe" } else { "unsubscribe" },
                Sub {
                    r#type: "candle",
                    coin: Some(&asset.to_string()),
                    interval: Some(&interval),
                    user: None,
                    instrument_id: None,
                },
            );
        }
    }
    fn fetch_candles(&self, cx: &mut Cx<'_, Pending>, request: Command) {
        let Command::FetchCandles {
            command,
            symbol,
            interval,
            limit,
            from_ms,
            to_ms,
        } = request
        else {
            return;
        };
        let Some(asset) = self.asset(cx, &symbol) else {
            cx.done(command, Err("Market not found".into()));
            return;
        };
        if !valid_interval(&interval) {
            cx.done(command, Err("Unsupported candle interval".into()));
            return;
        }
        let end = to_ms.unwrap_or(cx.now);
        let start = from_ms
            .unwrap_or(end - interval_ms(&interval) * f64::from(limit.unwrap_or(500).min(1500)))
            .max(0.0);
        if !start.is_finite()
            || !end.is_finite()
            || end < 0.0
            || start > end
            || end > 9_007_199_254_740_991.0
            || from_ms.is_some_and(|t| t < 0.0)
        {
            cx.done(command, Err("Invalid candle time range".into()));
            return;
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req<'a> {
            coin: String,
            interval: &'a str,
            start_time: u64,
            end_time: u64,
        }
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'a str,
            req: Req<'a>,
        }
        let body = serde_json::to_string(&Read {
            r#type: "candleSnapshot",
            req: Req {
                coin: asset.to_string(),
                interval: &interval,
                start_time: start.floor() as u64,
                end_time: end.floor() as u64,
            },
        })
        .unwrap();
        self.info(
            cx,
            body,
            ArrayAt::Field("data"),
            Pending::Candles {
                command,
                rows: Vec::new(),
                valid: true,
            },
        );
    }
}
impl Venue for TxFlow {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Err(error) = self.options.validate() {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.metadata(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let api = self.api.socket == Some(socket);
        if !api && self.node.socket != Some(socket) {
            return;
        }
        let conn = if api { &mut self.api } else { &mut self.node };
        conn.open = true;
        conn.attempts = 0;
        conn.last_received = cx.now;
        conn.ping = Some(cx.set_timer(10_000.0));
        if api {
            for (symbol, asset) in &self.assets {
                let base = cx
                    .store
                    .market(symbol)
                    .map(|m| m.base.clone())
                    .unwrap_or_else(|| symbol.clone());
                self.subscribe(
                    cx,
                    true,
                    "subscribe",
                    Sub {
                        r#type: "ticker",
                        coin: Some(&base),
                        interval: None,
                        user: None,
                        instrument_id: Some(*asset),
                    },
                );
            }
            for (symbol, interval) in self.candles.keys() {
                if let Some(asset) = self.assets.get(symbol) {
                    self.subscribe(
                        cx,
                        true,
                        "subscribe",
                        Sub {
                            r#type: "candle",
                            coin: Some(&asset.to_string()),
                            interval: Some(interval),
                            user: None,
                            instrument_id: None,
                        },
                    );
                }
            }
        } else {
            self.subscribe(
                cx,
                false,
                "subscribe",
                Sub {
                    r#type: "allAssetCtxs",
                    coin: None,
                    interval: None,
                    user: None,
                    instrument_id: None,
                },
            );
            for (symbol, ladder) in &mut self.books {
                ladder.book.clear();
                ladder.time = None;
                cx.book_changed(symbol);
            }
            for symbol in self.books.keys() {
                if let Some(asset) = self.assets.get(symbol) {
                    self.subscribe(
                        cx,
                        false,
                        "subscribe",
                        Sub {
                            r#type: "l2Book",
                            coin: Some(&asset.to_string()),
                            interval: None,
                            user: None,
                            instrument_id: None,
                        },
                    );
                }
            }
            if !self.options.public_data_only {
                if let Some(user) = self.user() {
                    for kind in ["orderUpdates", "clearinghouseState", "userAssetData"] {
                        self.node_subscribe(cx, kind, user);
                    }
                }
                self.account_refresh(cx);
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.node.socket == Some(socket) {
            self.node.last_received = cx.now;
            self.public_frame(cx, text);
            self.account_node_frame(cx, text);
        } else if self.api.socket == Some(socket) {
            self.api.last_received = cx.now;
            self.public_frame(cx, text);
            self.account_api_frame(cx, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        let api = self.api.socket == Some(socket);
        if !api && self.node.socket != Some(socket) {
            return;
        }
        let conn = if api { &mut self.api } else { &mut self.node };
        conn.socket = None;
        conn.open = false;
        if let Some(timer) = conn.ping.take() {
            cx.clear_timer(timer);
        }
        conn.attempts = conn.attempts.saturating_add(1);
        conn.reconnect =
            Some(cx.set_timer((1000.0 * 2f64.powi(conn.attempts.min(5) as i32 - 1)).min(30_000.0)));
        if !api {
            for (symbol, ladder) in &mut self.books {
                ladder.book.clear();
                ladder.time = None;
                cx.book_changed(symbol);
            }
            self.account_disconnected(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if !(200..300).contains(&response.status) {
            self.on_request_failed(cx, pending, &format!("TxFlow HTTP {}", response.status));
            return;
        }
        match pending {
            Pending::Meta { rows, valid } => {
                if !valid || !Header::valid(response.body) {
                    self.on_request_failed(
                        cx,
                        Pending::Meta { rows, valid: false },
                        "Invalid TxFlow metadata",
                    );
                    return;
                }
                let mut identities = std::collections::HashSet::new();
                let mut names = std::collections::HashSet::new();
                if rows
                    .iter()
                    .filter_map(|r| r.market())
                    .any(|m| !identities.insert(m.id) || !names.insert(m.symbol))
                {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows: Vec::new(),
                            valid: false,
                        },
                        "Duplicate TxFlow market identities",
                    );
                    return;
                }
                let mut markets = Vec::new();
                for row in rows {
                    if let Some(market) = row.market() {
                        self.assets.insert(market.symbol.clone(), row.index);
                        for key in [
                            row.index.to_string(),
                            row.name.to_ascii_lowercase(),
                            row.base_currency.to_ascii_lowercase(),
                        ] {
                            self.coins.insert(key, market.symbol.clone());
                        }
                        self.ticker_queue.push_back(row.index);
                        self.instruments.insert(row.index, row);
                        markets.push(market);
                    }
                }
                if markets.is_empty() {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            rows: Vec::new(),
                            valid: false,
                        },
                        "No supported TxFlow markets",
                    );
                    return;
                }
                let tickers = markets
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Default::default()
                    })
                    .collect();
                self.books
                    .retain(|symbol, _| self.assets.contains_key(symbol));
                self.candles
                    .retain(|(symbol, _), _| self.assets.contains_key(symbol));
                cx.store.set_markets(markets);
                self.seed_tickers = tickers;
                self.next_ticker(cx);
            }
            Pending::Ticker { asset } => {
                match serde_json::from_str::<Reply<TickerRow>>(response.body) {
                    Ok(reply)
                        if reply.success
                            && reply.code == 0
                            && reply.data.instrument_id == asset =>
                    {
                        if let Some(ticker) = self
                            .seed_tickers
                            .iter_mut()
                            .find(|t| t.id == asset.to_string())
                        {
                            Self::ticker_update(ticker, reply.data, true);
                        }
                    }
                    _ => {
                        self.on_request_failed(
                            cx,
                            Pending::Ticker { asset },
                            "Invalid TxFlow ticker seed identity or envelope",
                        );
                        return;
                    }
                };
                self.ticker_timer = Some(cx.set_timer(25.0));
            }
            Pending::Candles {
                command,
                rows,
                valid,
            } => cx.done(
                command,
                if valid && Header::valid(response.body) {
                    Ok(serde_json::to_string(&rows).unwrap())
                } else {
                    Err("Invalid TxFlow candle response".into())
                },
            ),
            Pending::Account(request) => self.account_response(cx, request, response),
            Pending::Trade(request) => self.trade_response(cx, request, Ok(response)),
            Pending::History(request) => self.history_response(cx, *request, response),
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Meta { rows, valid } => match serde_json::from_str(item) {
                Ok(row) => rows.push(row),
                Err(_) => *valid = false,
            },
            Pending::Candles { rows, valid, .. } => match serde_json::from_str::<CandleRow>(item) {
                Ok(row) => rows.push(row.candle()),
                Err(_) => *valid = false,
            },
            Pending::Account(request) => self.account_item(cx, request, item),
            Pending::History(request) => self.history_item(cx, request, item),
            _ => {}
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Meta { .. } => {
                cx.log(LogLevel::Error, error);
                self.meta_timer = Some(cx.set_timer(5000.0));
            }
            Pending::Ticker { asset } => {
                cx.log(LogLevel::Warning, error);
                self.ticker_queue.push_front(asset);
                self.ticker_timer = Some(cx.set_timer(5000.0));
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Account(request) => self.account_failed(cx, request, error),
            Pending::Trade(request) => self.trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(cx, *request, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if self.meta_timer == Some(timer) {
            self.meta_timer = None;
            self.metadata(cx);
        }
        if self.ticker_timer == Some(timer) {
            self.ticker_timer = None;
            self.next_ticker(cx);
        }
        for api in [false, true] {
            let conn = if api { &mut self.api } else { &mut self.node };
            if conn.reconnect == Some(timer) {
                conn.reconnect = None;
                let role = if api || self.options.public_data_only {
                    SocketChannel::Public
                } else {
                    SocketChannel::Private
                };
                conn.socket = Some(cx.connect_channel(
                    format!(
                        "{}/{}",
                        REST_URL.replacen("https:", "wss:", 1),
                        if api { "api-ws" } else { "ws" }
                    ),
                    role,
                ));
            }
            if conn.ping == Some(timer) {
                conn.ping = None;
                if let Some(socket) = conn.socket {
                    if cx.now - conn.last_received >= 30_000.0 {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                    } else {
                        cx.send(socket, "{\"method\":\"ping\"}");
                        let next = cx.set_timer(10_000.0);
                        if api {
                            self.api.ping = Some(next);
                        } else {
                            self.node.ping = Some(next);
                        }
                    }
                }
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.book_sub(cx, symbol, true),
            Command::UnsubscribeOrderBook { symbol } => self.book_sub(cx, symbol, false),
            Command::SubscribeCandles { symbol, interval } => {
                self.candle_sub(cx, symbol, interval, true)
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                self.candle_sub(cx, symbol, interval, false)
            }
            request @ Command::FetchCandles { .. } => self.fetch_candles(cx, request),
            Command::Query { command, query } => self.query(cx, command, query),
            other => self.trade_command(cx, other),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|l| l.time.is_some())
            .map(|l| &l.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, l)| l.time.is_some())
                .map(|(symbol, ladder)| (symbol.as_str(), &ladder.book)),
        )
    }
}
fn valid_interval(interval: &str) -> bool {
    matches!(
        interval,
        "1m" | "3m" | "5m" | "15m" | "30m" | "1h" | "2h" | "4h" | "8h" | "12h" | "1d"
    )
}
fn interval_ms(interval: &str) -> f64 {
    let (n, unit) = interval.split_at(interval.len() - 1);
    n.parse::<f64>().unwrap_or(1.0)
        * match unit {
            "m" => 60_000.0,
            "h" => 3_600_000.0,
            "d" => 86_400_000.0,
            "w" => 604_800_000.0,
            _ => 2_592_000_000.0,
        }
}
