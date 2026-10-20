//! Hyperliquid perpetuals: typed, deterministic state machines without I/O.
mod account;
mod history;
pub mod sign;
mod trading;
mod wire;

use base64::Engine as _;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub vault: Option<String>,
    pub builder: Option<Builder>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Builder {
    pub b: String,
    pub f: u32,
}

pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    Meta {
        dex: String,
        offset: u32,
        instruments: Vec<Instrument>,
        markets: Vec<Market>,
        tickers: Vec<Ticker>,
        valid: bool,
        stage: u8,
        attempt: u8,
    },
    Dexs {
        dexs: Vec<(String, u32)>,
        index: u32,
    },
    Candles {
        command: CommandId,
        candles: Vec<Candle>,
        valid: bool,
    },
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
    depth: [Vec<Level>; 2],
    depth_time: Option<f64>,
    bbo: Option<(f64, [Option<Level>; 2])>,
    book: Book,
}
pub struct Hyperliquid {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    connection: Connection,
    books: HashMap<String, Ladder>,
    candles: HashMap<(String, String), usize>,
    assets: HashMap<String, u32>,
    coins: HashMap<String, String>,
    dex_queue: std::collections::VecDeque<(String, u32)>,
    dex_names: Vec<String>,
    meta_retries: HashMap<TimerId, (String, u32, u8)>,
}
impl Hyperliquid {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            connection: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            assets: HashMap::new(),
            coins: HashMap::new(),
            dex_queue: Default::default(),
            dex_names: Vec::new(),
            meta_retries: HashMap::new(),
        }
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://api.hyperliquid-testnet.xyz"
        } else {
            "https://api.hyperliquid.xyz"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://api.hyperliquid-testnet.xyz/ws"
        } else {
            "wss://api-ui.hyperliquid.xyz/ws"
        }
    }
    fn info(&self, cx: &mut Cx<'_, Pending>, body: String, at: ArrayAt, pending: Pending) {
        cx.request_streamed_body(
            Method::Post,
            format!("{}/info", self.rest()),
            vec![("Content-Type", "application/json".into())],
            Some(body),
            at,
            pending,
        );
    }
    fn meta(&self, cx: &mut Cx<'_, Pending>, dex: String, offset: u32) {
        self.meta_attempt(cx, dex, offset, 0);
    }
    fn meta_attempt(&self, cx: &mut Cx<'_, Pending>, dex: String, offset: u32, attempt: u8) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            #[serde(skip_serializing_if = "str::is_empty")]
            dex: &'a str,
        }
        self.info(
            cx,
            serde_json::to_string(&Read {
                r#type: "metaAndAssetCtxs",
                dex: &dex,
            })
            .unwrap(),
            ArrayAt::Root,
            Pending::Meta {
                dex,
                offset,
                instruments: Vec::new(),
                markets: Vec::new(),
                tickers: Vec::new(),
                valid: true,
                stage: 0,
                attempt,
            },
        );
    }
    fn next_dex(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some((dex, offset)) = self.dex_queue.pop_front() {
            self.meta(cx, dex, offset);
        } else if self.connection.socket.is_none() {
            self.account_start(cx);
            self.connection.socket = Some(cx.connect_channel(
                self.ws(),
                if self.options.public_data_only {
                    safe_cex_engine::SocketChannel::Public
                } else {
                    safe_cex_engine::SocketChannel::Private
                },
            ));
        }
    }
    fn subscription(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        kind: &str,
        coin: Option<&str>,
        interval: Option<&str>,
        dex: Option<&str>,
    ) {
        #[derive(Serialize)]
        struct Sub<'a> {
            r#type: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            interval: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            dex: Option<&'a str>,
        }
        #[derive(Serialize)]
        struct Send<'a> {
            method: &'a str,
            subscription: Sub<'a>,
        }
        if self.connection.open
            && let Some(socket) = self.connection.socket
        {
            cx.send(
                socket,
                serde_json::to_string(&Send {
                    method,
                    subscription: Sub {
                        r#type: kind,
                        coin,
                        interval,
                        dex,
                    },
                })
                .unwrap(),
            );
        }
    }
    fn coin<'a>(&'a self, cx: &'a Cx<'_, Pending>, symbol: &str) -> Option<&'a str> {
        cx.store.market(symbol).map(|m| m.id.as_str())
    }
    fn symbol(&self, coin: &str) -> Option<&str> {
        self.coins
            .get(&coin.to_ascii_lowercase())
            .map(String::as_str)
    }
    fn public_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        let raw = frame.data.get();
        match frame.channel.as_ref() {
            "allMids" => {
                if let Ok(mids) = serde_json::from_str::<Mids>(raw) {
                    for (coin, price) in mids.mids {
                        if let (Some(symbol), Ok(mid)) = (self.symbol(&coin), price.parse::<f64>())
                            && mid.is_finite()
                            && mid > 0.0
                        {
                            cx.store.update_ticker(symbol, |t| {
                                t.last = mid;
                                t.mid_price = Some(mid);
                            });
                        }
                    }
                }
            }
            "fastAssetCtxs" => {
                if let Ok(encoded) = serde_json::from_str::<&str>(raw)
                    && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
                    && let Ok(decoded) =
                        miniz_oxide::inflate::decompress_to_vec_with_limit(&bytes, 2_000_000)
                    && let Ok(contexts) = serde_json::from_slice::<HashMap<&str, FastCtx>>(&decoded)
                {
                    for (coin, context) in contexts {
                        if let Some(symbol) = self.symbol(coin) {
                            cx.store.update_ticker(symbol, |t| {
                                if let Some(mark) =
                                    context.mark_px.filter(|v| v.is_finite() && *v > 0.0)
                                {
                                    t.mark = mark;
                                }
                                if let Some(mid) =
                                    context.mid_px.filter(|v| v.is_finite() && *v > 0.0)
                                {
                                    t.last = mid;
                                    t.mid_price = Some(mid);
                                }
                            });
                        }
                    }
                }
            }
            "l2Book" => {
                if let Ok(depth) = serde_json::from_str::<Depth>(raw)
                    && let Some(symbol) = self.symbol(&depth.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                {
                    ladder.depth = depth.levels;
                    ladder.depth_time = Some(depth.time);
                    Self::combine(ladder);
                    cx.book_changed(&symbol);
                }
            }
            "bbo" => {
                if let Ok(bbo) = serde_json::from_str::<Bbo>(raw)
                    && let Some(symbol) = self.symbol(&bbo.coin).map(str::to_owned)
                    && let Some(ladder) = self.books.get_mut(&symbol)
                    && ladder
                        .bbo
                        .as_ref()
                        .is_none_or(|(time, _)| bbo.time >= *time)
                {
                    ladder.bbo = Some((bbo.time, bbo.bbo));
                    if ladder.depth_time.is_some() {
                        Self::combine(ladder);
                        cx.book_changed(&symbol);
                    }
                }
            }
            "candle" => {
                let mut apply = |row: CandleRow<'_>| {
                    if let Some(symbol) = self.symbol(&row.s)
                        && self
                            .candles
                            .contains_key(&(symbol.to_owned(), row.i.to_string()))
                    {
                        cx.candle(symbol, &row.i, row.candle());
                    }
                };
                if raw.starts_with('[') {
                    let _ = json::each(raw, &mut apply);
                } else if let Ok(row) = serde_json::from_str::<CandleRow>(raw) {
                    apply(row);
                }
            }
            _ => {}
        }
    }
    fn combine(ladder: &mut Ladder) {
        for (index, side) in [BookSide::Bids, BookSide::Asks].into_iter().enumerate() {
            let depth = &ladder.depth[index];
            if let Some((time, bbo)) = &ladder.bbo
                && *time >= ladder.depth_time.unwrap_or(0.0)
            {
                let mut levels = Vec::with_capacity(20);
                if let Some(best) = &bbo[index] {
                    levels.push((best.px, best.sz));
                    for level in depth {
                        if levels.len() == 20 {
                            break;
                        }
                        if if index == 0 {
                            level.px < best.px
                        } else {
                            level.px > best.px
                        } {
                            levels.push((level.px, level.sz));
                        }
                    }
                }
                ladder.book.load_sorted(side, levels);
            } else {
                ladder
                    .book
                    .load_sorted(side, depth.iter().map(|l| (l.px, l.sz)));
            }
        }
    }
    fn book_sub(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, add: bool) {
        let count = self.books.entry(symbol.clone()).or_default();
        if add {
            count.subscribers += 1;
            if count.subscribers > 1 {
                return;
            }
        } else {
            count.subscribers = count.subscribers.saturating_sub(1);
            if count.subscribers > 0 {
                return;
            }
            self.books.remove(&symbol);
        }
        if let Some(coin) = self.coin(cx, &symbol).map(str::to_owned) {
            for kind in ["l2Book", "bbo"] {
                self.subscription(
                    cx,
                    if add { "subscribe" } else { "unsubscribe" },
                    kind,
                    Some(&coin),
                    None,
                    None,
                );
            }
        }
    }
}
fn price_decimals(price: f64, size: i32) -> i32 {
    let significant = if price == 0.0 {
        5
    } else {
        4 - price.abs().log10().floor() as i32
    };
    (6 - size).min(significant.max(0))
}
fn market_rows(
    dex: &str,
    instruments: &[Instrument],
    raw: &str,
    markets: &mut Vec<Market>,
    tickers: &mut Vec<Ticker>,
) -> bool {
    let mut i = 0;
    json::each::<AssetCtx>(raw, |ctx| {
        let index = i;
        i += 1;
        let Some(meta) = instruments.get(index) else {
            return;
        };
        let Some(price) = ctx.mid_px.filter(|p| p.is_finite() && *p > 0.0) else {
            return;
        };
        if !dex.is_empty() && meta.is_delisted {
            return;
        }
        let base = meta
            .name
            .split_once(':')
            .map_or(meta.name.as_str(), |(_, b)| b);
        let id = if dex.is_empty() || meta.name.contains(':') {
            meta.name.clone()
        } else {
            format!("{dex}:{}", meta.name)
        };
        let symbol = if dex.is_empty() {
            format!("{}-PERP", base.to_uppercase())
        } else {
            format!("{}|{}-PERP", dex.to_uppercase(), base.to_uppercase())
        };
        let step = 10f64.powi(-meta.sz_decimals);
        markets.push(Market {
            id: id.clone(),
            symbol: symbol.clone(),
            pretty_symbol: None,
            base: base.into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: step,
                price: 10f64.powi(-price_decimals(price, meta.sz_decimals)),
            },
            limits: Limits {
                amount: MinMax {
                    min: step,
                    max: 9_007_199_254_740_991.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: meta.max_leverage,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: Some(0.00045),
            maker_fee: Some(0.00015),
        });
        let mark = if ctx.mark_px > 0.0 {
            ctx.mark_px
        } else {
            price
        };
        tickers.push(Ticker {
            id,
            symbol,
            bid: 0.0,
            ask: 0.0,
            last: price,
            mark: ctx.mark_px,
            index: ctx.oracle_px,
            mid_price: Some(price),
            percentage: if ctx.prev_day_px > 0.0 {
                (mark - ctx.prev_day_px) / ctx.prev_day_px * 100.0
            } else {
                0.0
            },
            open_interest: ctx.open_interest * mark,
            funding_rate: ctx.funding,
            funding_time: 0.0,
            volume: if ctx.day_base_vlm > 0.0 {
                ctx.day_base_vlm
            } else {
                ctx.day_ntl_vlm / price
            },
            quote_volume: ctx.day_ntl_vlm,
        });
    })
    .is_ok()
        && i == instruments.len()
}
impl Venue for Hyperliquid {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.meta(cx, String::new(), 0);
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        // Public prices cannot establish a private snapshot or mark it loaded.
        if cx.store.loaded().balance {
            self.live_balance(cx);
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.connection.socket != Some(socket) {
            return;
        }
        self.connection.open = true;
        self.connection.attempts = 0;
        self.connection.last_received = cx.now;
        self.connection.ping = Some(cx.set_timer(15_000.0));
        self.private_subscribe(cx);
        if self.account.started && self.user().is_some() && !self.options.public_data_only {
            self.refresh_account(cx);
        }
        // The account socket multiplexes public feeds; transport roles cannot split them.
        if cx.public_feed_policy() != PublicFeedPolicy::Direct {
            return;
        }
        self.subscription(cx, "subscribe", "allMids", None, None, None);
        self.subscription(cx, "subscribe", "fastAssetCtxs", None, None, None);
        // TS owns dex mids on its account socket, which public-only sessions skip.
        if !self.options.public_data_only && self.user().is_some() {
            for dex in &self.dex_names {
                self.subscription(cx, "subscribe", "allMids", None, None, Some(dex));
            }
        }
        for ladder in self.books.values_mut() {
            ladder.depth_time = None;
            ladder.bbo = None;
            ladder.book.clear();
        }
        for symbol in self.books.keys() {
            if let Some(coin) = self.coin(cx, symbol).map(str::to_owned) {
                for kind in ["l2Book", "bbo"] {
                    self.subscription(cx, "subscribe", kind, Some(&coin), None, None);
                }
            }
        }
        for (symbol, interval) in self.candles.keys() {
            if let Some(coin) = self.coin(cx, symbol).map(str::to_owned) {
                self.subscription(cx, "subscribe", "candle", Some(&coin), Some(interval), None);
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.connection.socket != Some(socket) {
            return;
        }
        self.connection.last_received = cx.now;
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public_frame(cx, text);
        }
        self.account_frame(cx, text);
        self.live_balance(cx);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.connection.socket != Some(socket) {
            return;
        }
        self.connection.open = false;
        self.connection.socket = None;
        if let Some(timer) = self.connection.ping.take() {
            cx.clear_timer(timer);
        }
        for ladder in self.books.values_mut() {
            ladder.depth_time = None;
            ladder.book.clear();
        }
        self.connection.attempts = self.connection.attempts.saturating_add(1);
        self.connection.reconnect = Some(cx.set_timer(
            (1000.0 * 2f64.powi(self.connection.attempts.min(5) as i32 - 1)).min(30_000.0),
        ));
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if !(200..300).contains(&response.status) {
            self.on_request_failed(
                cx,
                pending,
                &format!("Hyperliquid HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        match pending {
            Pending::Account(request) => {
                let builder = matches!(request, account::Request::Builder);
                self.account_response(cx, request, response);
                if builder {
                    self.builder_result(cx);
                }
            }
            Pending::Trade(request) => self.trade_response(cx, request, Ok(response)),
            Pending::History(request) => self.history_response(cx, *request, response),
            Pending::Meta {
                dex,
                offset,
                instruments,
                markets,
                tickers,
                valid,
                stage,
                attempt,
            } => {
                if !valid || stage != 2 {
                    self.on_request_failed(
                        cx,
                        Pending::Meta {
                            dex,
                            offset,
                            instruments,
                            markets,
                            tickers,
                            valid: false,
                            stage,
                            attempt,
                        },
                        "Malformed Hyperliquid metadata tuple",
                    );
                    return;
                }
                if valid {
                    for (index, meta) in instruments.iter().enumerate() {
                        let id = if dex.is_empty() || meta.name.contains(':') {
                            meta.name.clone()
                        } else {
                            format!("{dex}:{}", meta.name)
                        };
                        if let Some(m) = markets.iter().find(|m| m.id == id) {
                            self.assets.insert(m.symbol.clone(), offset + index as u32);
                            self.coins
                                .insert(m.id.to_ascii_lowercase(), m.symbol.clone());
                        }
                    }
                    let mut all = cx.store.markets().to_vec();
                    all.extend(markets);
                    cx.store.set_markets(all);
                    let mut all = cx.store.tickers().to_vec();
                    all.extend(tickers);
                    cx.store.set_tickers(all);
                }
                if dex.is_empty() {
                    self.info(
                        cx,
                        "{\"type\":\"perpDexs\"}".into(),
                        ArrayAt::Root,
                        Pending::Dexs {
                            dexs: Vec::new(),
                            index: 0,
                        },
                    );
                } else {
                    self.next_dex(cx);
                }
            }
            Pending::Dexs { dexs, .. } => {
                self.dex_names = dexs.iter().map(|(d, _)| d.clone()).collect();
                self.dex_queue = dexs.into();
                self.next_dex(cx);
            }
            Pending::Candles {
                command,
                candles,
                valid,
            } => cx.done(
                command,
                if valid {
                    Ok(serde_json::to_string(&candles).unwrap())
                } else {
                    Err("Malformed Hyperliquid candles".into())
                },
            ),
        }
    }
    fn on_item(&mut self, _cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(request) => self.account_item(_cx, request, item),
            Pending::Trade(_) => {}
            Pending::History(request) => self.history_item(request, item),
            Pending::Meta {
                dex,
                instruments,
                markets,
                tickers,
                valid,
                stage,
                ..
            } => {
                match *stage {
                    0 => match serde_json::from_str::<Meta>(item) {
                        Ok(meta) => *instruments = meta.universe,
                        Err(_) => *valid = false,
                    },
                    1 => *valid &= market_rows(dex, instruments, item, markets, tickers),
                    _ => *valid = false,
                }
                *stage = stage.saturating_add(1);
            }
            Pending::Dexs { dexs, index } => {
                let position = *index;
                *index += 1;
                if let Ok(Some(dex)) = serde_json::from_str::<Option<Dex>>(item)
                    && position > 0
                    && (!self.options.testnet
                        || ["vntls", "xyz", "VOLMEX", "flx"].contains(&dex.name.as_str()))
                {
                    dexs.push((dex.name, 110_000 + (position - 1) * 10_000));
                }
            }
            Pending::Candles { candles, valid, .. } => {
                match serde_json::from_str::<CandleRow>(item) {
                    Ok(row) => candles.push(row.candle()),
                    Err(_) => *valid = false,
                }
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(request) => {
                let builder = matches!(request, account::Request::Builder);
                self.account_failed(cx, request, error);
                if builder {
                    self.builder_result(cx);
                }
            }
            Pending::Trade(request) => self.trade_response(cx, request, Err(error)),
            Pending::History(request) => self.history_failed(cx, *request, error),
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Dexs { .. } => self.next_dex(cx),
            Pending::Meta {
                dex,
                offset,
                attempt,
                ..
            } if attempt < 2 => {
                let timer = cx.set_timer(500.0 * f64::from(attempt + 1));
                self.meta_retries.insert(timer, (dex, offset, attempt + 1));
                cx.log(LogLevel::Warning, error);
            }
            Pending::Meta { dex, .. } if !dex.is_empty() => {
                cx.log(LogLevel::Warning, error);
                self.next_dex(cx);
            }
            _ => cx.log(LogLevel::Error, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.account_timer(cx, timer);
        self.trade_timer(cx, timer);
        if let Some((dex, offset, attempt)) = self.meta_retries.remove(&timer) {
            self.meta_attempt(cx, dex, offset, attempt);
        }
        if self.connection.reconnect == Some(timer) {
            self.connection.reconnect = None;
            self.connection.socket = Some(cx.connect_channel(
                self.ws(),
                if self.options.public_data_only {
                    safe_cex_engine::SocketChannel::Public
                } else {
                    safe_cex_engine::SocketChannel::Private
                },
            ));
        }
        if self.connection.ping == Some(timer) {
            self.connection.ping = None;
            if let Some(socket) = self.connection.socket {
                if cx.now - self.connection.last_received >= 45_000.0 {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                } else {
                    cx.send(socket, "{\"method\":\"ping\"}");
                    self.connection.ping = Some(cx.set_timer(15_000.0));
                }
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => self.book_sub(cx, symbol, true),
            Command::UnsubscribeOrderBook { symbol } => self.book_sub(cx, symbol, false),
            Command::SubscribeCandles { symbol, interval } => {
                let count = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_default();
                *count += 1;
                if *count == 1
                    && let Some(coin) = self.coin(cx, &symbol).map(str::to_owned)
                {
                    self.subscription(
                        cx,
                        "subscribe",
                        "candle",
                        Some(&coin),
                        Some(&interval),
                        None,
                    );
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), interval.clone())) {
                    *count -= 1;
                    if *count > 0 {
                        return;
                    }
                }
                self.candles.remove(&(symbol.clone(), interval.clone()));
                if let Some(coin) = self.coin(cx, &symbol).map(str::to_owned) {
                    self.subscription(
                        cx,
                        "unsubscribe",
                        "candle",
                        Some(&coin),
                        Some(&interval),
                        None,
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
                if !valid_interval(&interval) {
                    cx.done(command, Err("Unsupported candle interval".into()));
                    return;
                }
                let Some(coin) = self.coin(cx, &symbol) else {
                    cx.done(command, Err("Market not found".into()));
                    return;
                };
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Req<'a> {
                    coin: &'a str,
                    interval: &'a str,
                    start_time: u64,
                    end_time: u64,
                }
                #[derive(Serialize)]
                struct Read<'a> {
                    r#type: &'static str,
                    req: Req<'a>,
                }
                let end = to_ms.unwrap_or(cx.now).floor();
                let start = match (limit, from_ms) {
                    (None, Some(from)) => from,
                    _ => end - interval_ms(&interval) * f64::from(limit.unwrap_or(500).min(1500)),
                };
                if !start.is_finite()
                    || !end.is_finite()
                    || start > end
                    || end > 9_007_199_254_740_991.0
                    || end < 0.0
                    || from_ms.is_some_and(|t| t < 0.0)
                {
                    cx.done(command, Err("Invalid candle time range".into()));
                    return;
                }
                let body = serde_json::to_string(&Read {
                    r#type: "candleSnapshot",
                    req: Req {
                        coin,
                        interval: &interval,
                        start_time: start.max(0.0).floor() as u64,
                        end_time: end as u64,
                    },
                })
                .unwrap();
                self.info(
                    cx,
                    body,
                    ArrayAt::Root,
                    Pending::Candles {
                        command,
                        candles: Vec::new(),
                        valid: true,
                    },
                );
            }
            Command::Query { command, query } => self.start_query(cx, command, query),
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|l| l.depth_time.is_some())
            .map(|l| &l.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, l)| l.depth_time.is_some())
                .map(|(s, l)| (s.as_str(), &l.book)),
        )
    }
}
fn interval_ms(interval: &str) -> f64 {
    let (n, unit) = interval.split_at(interval.char_indices().last().map_or(0, |(i, _)| i));
    n.parse::<f64>().unwrap_or(1.0)
        * match unit {
            "s" => 1000.0,
            "h" => 3_600_000.0,
            "d" => 86_400_000.0,
            "w" => 604_800_000.0,
            "M" => 2_592_000_000.0,
            _ => 60_000.0,
        }
}

fn valid_interval(interval: &str) -> bool {
    [
        "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "8h", "12h", "1d", "3d", "1w", "1M",
    ]
    .contains(&interval)
}
