//! Kraken Spot has separate REST authentication and v2 sockets from futures.
mod account;
mod trading;
mod wire;
use crate::{Options, sign};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
    book::{Book, BookSide},
    model::*,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use wire::*;

#[derive(Clone)]
pub enum Pending {
    Private(account::Request),
    Markets {
        retries: u8,
    },
    Tickers {
        retries: u8,
    },
    Candles {
        command: CommandId,
        params: String,
        retries: u8,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    received: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
#[derive(Default)]
struct DepthState {
    book: Book,
    synced: bool,
    subscribers: u32,
}
pub struct KrakenSpot {
    options: Options,
    signer: Option<sign::Signer>,
    public: Connection,
    account: account::Account,
    trading: trading::Trading,
    pairs: HashMap<String, String>,
    identities: HashMap<String, String>,
    ws_symbols: HashMap<String, String>,
    ws_canonical: HashMap<String, String>,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, String), u32>,
    ticker_symbols: Vec<String>,
    poll: Option<TimerId>,
    markets_in_flight: bool,
    tickers_in_flight: bool,
    retries: Vec<(TimerId, Pending)>,
}
impl KrakenSpot {
    pub fn new(mut options: Options) -> Self {
        options.public_data_only |= options.key.as_ref().is_none_or(String::is_empty)
            || options.secret.as_ref().is_none_or(String::is_empty);
        let signer =
            if !options.public_data_only && options.key.as_ref().is_some_and(|k| !k.is_empty()) {
                options
                    .secret
                    .as_deref()
                    .and_then(|s| sign::Signer::new(s).ok())
            } else {
                None
            };
        Self {
            options,
            signer,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            pairs: HashMap::new(),
            identities: HashMap::new(),
            ws_symbols: HashMap::new(),
            ws_canonical: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            ticker_symbols: vec![],
            poll: None,
            markets_in_flight: false,
            tickers_in_flight: false,
            retries: vec![],
        }
    }
    fn public_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Markets { .. } => self.markets_in_flight = false,
            Pending::Tickers { .. } => self.tickers_in_flight = false,
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Private(_) => unreachable!(),
        }
        cx.log(LogLevel::Warning, error);
    }
    fn read(&self, cx: &mut Cx<'_, Pending>, path: &str, params: &str, pending: Pending) {
        let id = cx.request(
            Method::Get,
            format!("https://api.kraken.com/0/public/{path}{params}"),
            vec![],
            None,
            pending,
        );
        cx.request_timeout(id, 20_000);
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.markets_in_flight {
            self.markets_in_flight = true;
            self.read(cx, "AssetPairs", "", Pending::Markets { retries: 0 });
        }
    }
    fn tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.tickers_in_flight {
            self.tickers_in_flight = true;
            self.read(cx, "Ticker", "", Pending::Tickers { retries: 0 });
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket =
            Some(cx.connect_channel("wss://ws.kraken.com/v2", SocketChannel::Public));
        self.public.open = false;
        self.public.received = cx.now;
        self.public.watchdog = Some(cx.set_timer(15_000.));
    }
    fn subscribe(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: &str,
        channel: &str,
        symbols: &[String],
        interval: Option<u32>,
    ) {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
            symbol: &'a [String],
            #[serde(skip_serializing_if = "Option::is_none")]
            depth: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            interval: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            event_trigger: Option<&'static str>,
        }
        #[derive(Serialize)]
        struct Message<'a> {
            method: &'a str,
            params: Params<'a>,
        }
        let translated: Vec<String> = symbols
            .iter()
            .map(|s| self.ws_symbols.get(s).cloned().unwrap_or_else(|| s.clone()))
            .collect();
        if let Some(s) = self.public.socket.filter(|_| self.public.open) {
            cx.send(
                s,
                serde_json::to_string(&Message {
                    method,
                    params: Params {
                        channel,
                        symbol: &translated,
                        depth: (channel == "book").then_some(100),
                        interval,
                        event_trigger: (channel == "ticker").then_some("trades"),
                    },
                })
                .unwrap(),
            );
        }
    }
    fn ticker_subscribe(&self, cx: &mut Cx<'_, Pending>, method: &str) {
        let selected: Vec<_> = cx
            .store
            .markets()
            .iter()
            .filter(|m| self.ticker_symbols.is_empty() || self.ticker_symbols.contains(&m.symbol))
            .map(|m| m.symbol.clone())
            .collect();
        for batch in selected.chunks(50) {
            self.subscribe(cx, method, "ticker", batch, None);
        }
    }
    fn markets_response(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> Result<(), String> {
        let mut markets = vec![];
        let mut pairs = HashMap::new();
        let mut identities = HashMap::new();
        let mut invalid = false;
        map_each::<Pair<'_>>(text, |key, p| {
            if key.ends_with(".d") {
                return;
            }
            let Some((base, quote)) = p.wsname.split_once('/') else {
                return;
            };
            let base = asset(base);
            let quote = asset(quote);
            if !matches!(quote, "USD" | "USDT" | "USDC") {
                return;
            }
            let pd = p.pair_decimals.unwrap_or(2);
            let ad = p.lot_decimals.unwrap_or(8);
            if pd > 18 || ad > 18 || p.ordermin < 0. {
                invalid = true;
                return;
            }
            let symbol = p.wsname.to_string();
            let pretty = format!("{base}{quote}");
            for id in [&*key, &*p.altname, &symbol, &pretty] {
                if !id.is_empty() {
                    identities.insert(id.to_owned(), symbol.clone());
                }
            }
            if pairs.insert(symbol.clone(), key.to_string()).is_some() {
                invalid = true;
                return;
            }
            markets.push(Market {
                id: format!("{base}/{quote}"),
                symbol,
                pretty_symbol: Some(pretty),
                base: base.into(),
                quote: quote.into(),
                active: p.status == "online",
                precision: Precision {
                    amount: 10f64.powi(-(ad as i32)),
                    price: 10f64.powi(-(pd as i32)),
                },
                limits: Limits {
                    amount: MinMax {
                        min: p.ordermin,
                        max: 9_007_199_254_740_991.,
                    },
                    leverage: MinMax { min: 1., max: 1. },
                    price: None,
                },
                contract_size: Some(1.),
                is_expiring: None,
                is_inverse: Some(false),
                is_spot: Some(true),
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            });
        })
        .map_err(|_| "Kraken Spot invalid markets")?;
        if invalid || markets.is_empty() {
            return Err("Kraken Spot invalid markets".into());
        }
        self.ws_symbols = markets
            .iter()
            .map(|m| (m.symbol.clone(), format!("{}/{}", m.base, m.quote)))
            .collect();
        self.ws_canonical = self
            .ws_symbols
            .iter()
            .map(|(a, b)| (b.clone(), a.clone()))
            .collect();
        self.pairs = pairs;
        self.identities = identities;
        cx.store.set_markets(markets);
        self.tickers(cx);
        Ok(())
    }
    fn tickers_response(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> Result<(), String> {
        let mut rows = vec![];
        let mut invalid = false;
        map_each::<RestTicker>(text, |key, t| {
            let Some(symbol) = self.identities.get(&*key) else {
                return;
            };
            let Some(m) = cx.store.market(symbol) else {
                return;
            };
            let n = |s: Option<&String>| -> Option<f64> {
                s?.parse::<f64>().ok().filter(|n| n.is_finite())
            };
            let Some((ask, bid, last, volume, open)) = n(t.a.first())
                .zip(n(t.b.first()))
                .zip(n(t.c.first()))
                .zip(n(t.v.get(1)))
                .zip(n(Some(&t.o)))
                .map(|((((a, b), c), v), o)| (a, b, c, v, o))
            else {
                invalid = true;
                return;
            };
            rows.push(Ticker {
                id: m.id.clone(),
                symbol: symbol.clone(),
                bid,
                ask,
                last,
                mark: last,
                index: last,
                percentage: if open > 0. {
                    (last - open) / open * 100.
                } else {
                    0.
                },
                volume,
                quote_volume: volume * last,
                ..Ticker::default()
            });
        })
        .map_err(|_| "Kraken Spot invalid tickers")?;
        if invalid || rows.is_empty() {
            return Err("Kraken Spot invalid tickers".into());
        }
        cx.store.set_tickers(rows);
        if self.public.socket.is_none() {
            self.connect(cx);
        }
        self.private_start(cx);
        Ok(())
    }
}
impl Venue for KrakenSpot {
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::NO_HISTORY
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.options.public_data_only && self.signer.is_none() {
            self.reject_auth(cx, "Kraken Spot invalid secret: expected nonempty base64");
            return;
        }
        self.markets(cx);
        self.poll = Some(cx.set_timer(5_000.));
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.private_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.received = cx.now;
        self.ticker_subscribe(cx, "subscribe");
        for symbol in self.books.keys() {
            self.subscribe(cx, "subscribe", "book", std::slice::from_ref(symbol), None)
        }
        let mut sent = HashSet::new();
        for (symbol, tf) in self.candles.keys() {
            if let Some(i) = interval(tf)
                && sent.insert((symbol, i))
            {
                self.subscribe(
                    cx,
                    "subscribe",
                    "ohlc",
                    std::slice::from_ref(symbol),
                    Some(i),
                );
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.private_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.received = cx.now;
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return;
        };
        if msg.success == Some(false) {
            cx.log(
                LogLevel::Warning,
                format!("Kraken Spot subscription: {}", msg.error),
            );
            return;
        }
        let Some(data) = msg.data else { return };
        match &*msg.channel {
            "ticker" => {
                let _ = safe_cex_engine::json::each::<WsTicker<'_>>(data.get(), |t| {
                    let symbol = self
                        .ws_canonical
                        .get(&*t.symbol)
                        .map_or(&*t.symbol, String::as_str);
                    cx.store.update_ticker(symbol, |old| {
                        if let Some(n) = t.bid {
                            old.bid = n
                        }
                        if let Some(n) = t.ask {
                            old.ask = n
                        }
                        if let Some(n) = t.last {
                            old.last = n;
                            old.mark = n;
                            old.index = n
                        }
                        if let Some(n) = t.volume {
                            old.volume = n
                        }
                        if let Some(n) = t.change_pct {
                            old.percentage = n
                        }
                    });
                });
            }
            "book" => {
                let _ = safe_cex_engine::json::each::<Depth<'_>>(data.get(), |d| {
                    let symbol = self
                        .ws_canonical
                        .get(&*d.symbol)
                        .map_or(&*d.symbol, String::as_str);
                    let Some(state) = self.books.get_mut(symbol) else {
                        return;
                    };
                    let snapshot = msg.kind == "snapshot";
                    if !snapshot && !state.synced {
                        return;
                    }
                    if d.bids
                        .iter()
                        .chain(&d.asks)
                        .any(|l| l.price <= 0. || l.qty < 0.)
                    {
                        return;
                    }
                    if snapshot {
                        state.book.clear();
                        state.synced = true
                    }
                    for (side, levels) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                        for l in levels {
                            state.book.apply(side, l.price, l.qty)
                        }
                    }
                    // Kraken drops levels that leave subscribed depth without explicit zero updates.
                    for side in [BookSide::Bids, BookSide::Asks] {
                        let levels = if side == BookSide::Bids {
                            state.book.bids()
                        } else {
                            state.book.asks()
                        };
                        if levels.len() > 100 {
                            let levels = levels[..100].to_vec();
                            state.book.load_sorted_owned(side, levels);
                        }
                    }
                    cx.book_changed(symbol);
                });
            }
            "ohlc" => {
                let _ = safe_cex_engine::json::each::<Ohlc<'_>>(data.get(), |o| {
                    let Some(timestamp) = crate::history::timestamp(&o.interval_begin) else {
                        return;
                    };
                    let candle = Candle {
                        timestamp: timestamp / 1000.,
                        open: o.open,
                        high: o.high,
                        low: o.low,
                        close: o.close,
                        volume: o.volume,
                    };
                    for (symbol, tf) in self.candles.keys() {
                        if symbol
                            == self
                                .ws_canonical
                                .get(&*o.symbol)
                                .map_or(&*o.symbol, String::as_str)
                            && interval(tf) == Some(o.interval)
                        {
                            cx.candle(symbol, tf, candle);
                        }
                    }
                });
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.private_close(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        if let Some(t) = self.public.watchdog.take() {
            cx.clear_timer(t)
        }
        for (symbol, s) in &mut self.books {
            s.book.clear();
            s.synced = false;
            cx.book_changed(symbol);
        }
        self.public.attempts = self.public.attempts.saturating_add(1);
        self.public.reconnect = Some(
            cx.set_timer((1000. * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.)),
        );
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Private(request) = pending {
            self.private_response(cx, request, response);
            return;
        }
        let r = result(response).and_then(|text| match pending.clone() {
            Pending::Private(_) => unreachable!(),
            Pending::Markets { .. } => {
                self.markets_in_flight = false;
                self.markets_response(cx, text)
            }
            Pending::Tickers { .. } => {
                self.tickers_in_flight = false;
                self.tickers_response(cx, text)
            }
            Pending::Candles { command, .. } => {
                let mut rows = vec![];
                let mut failed = false;
                map_each::<&serde_json::value::RawValue>(text, |key, raw| {
                    if key == "last" {
                        return;
                    }
                    let r = safe_cex_engine::json::each::<(
                        f64,
                        String,
                        String,
                        String,
                        String,
                        String,
                        String,
                        u64,
                    )>(raw.get(), |r| {
                        let n = |s: &str| s.parse::<f64>().ok().filter(|n| n.is_finite());
                        if let Some((o, h, l, c, v)) = n(&r.1)
                            .zip(n(&r.2))
                            .zip(n(&r.3))
                            .zip(n(&r.4))
                            .zip(n(&r.6))
                            .map(|((((o, h), l), c), v)| (o, h, l, c, v))
                        {
                            rows.push(Candle {
                                timestamp: r.0,
                                open: o,
                                high: h,
                                low: l,
                                close: c,
                                volume: v,
                            })
                        } else {
                            failed = true
                        }
                    });
                    if r.is_err() {
                        failed = true
                    }
                })
                .map_err(|_| "Kraken Spot invalid candles")?;
                if failed {
                    return Err("Kraken Spot invalid candles".into());
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(
                    command,
                    serde_json::to_string(&rows).map_err(|e| e.to_string()),
                );
                Ok(())
            }
        });
        if let Err(e) = r {
            self.public_failed(cx, pending, &e);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        match &mut pending {
            Pending::Private(_) => {
                let Pending::Private(r) = pending else {
                    unreachable!()
                };
                self.private_network_failed(cx, r, error);
                return;
            }
            Pending::Markets { retries }
            | Pending::Tickers { retries }
            | Pending::Candles { retries, .. }
                if *retries < 3 =>
            {
                *retries += 1;
                let timer = cx.set_timer(0.);
                self.retries.push((timer, pending));
                return;
            }
            _ => {}
        }
        self.public_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(i) = self.retries.iter().position(|(t, _)| *t == timer) {
            let (_, p) = self.retries.remove(i);
            match &p {
                Pending::Markets { .. } => self.read(cx, "AssetPairs", "", p),
                Pending::Tickers { .. } => self.read(cx, "Ticker", "", p),
                Pending::Candles { params, .. } => {
                    let params = params.clone();
                    self.read(cx, "OHLC", &params, p)
                }
                _ => unreachable!(),
            }
            return;
        }

        if self.private_timer(cx, timer) {
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect(cx)
        } else if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.received >= 45_000. {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006)
                }
            } else {
                if let Some(s) = self.public.socket.filter(|_| self.public.open) {
                    cx.send(s, "{\"method\":\"ping\"}");
                }
                self.public.watchdog = Some(cx.set_timer(15_000.));
            }
        } else if self.poll == Some(timer) {
            self.poll = None;
            if !cx.store.loaded().markets {
                self.markets(cx)
            } else if !cx.store.loaded().tickers {
                self.tickers(cx)
            }
            self.poll = Some(cx.set_timer(5_000.));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if self.trade_command(cx, &command) {
            return;
        }
        match command {
            Command::SetTickerSymbols { symbols } => {
                self.ticker_subscribe(cx, "unsubscribe");
                self.ticker_symbols = symbols;
                self.ticker_subscribe(cx, "subscribe");
            }
            Command::SubscribeOrderBook { symbol } => {
                let s = self.books.entry(symbol.clone()).or_default();
                s.subscribers += 1;
                if s.subscribers == 1 {
                    self.subscribe(cx, "subscribe", "book", &[symbol], None)
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(cx, "unsubscribe", "book", &[symbol], None);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: tf,
            } => {
                if let Some(i) = interval(&tf) {
                    let count = self.candles.entry((symbol.clone(), tf)).or_default();
                    *count += 1;
                    if *count == 1 {
                        self.subscribe(cx, "subscribe", "ohlc", &[symbol], Some(i));
                    }
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: tf,
            } => {
                let k = (symbol.clone(), tf.clone());
                if let Some(count) = self.candles.get_mut(&k) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&k);
                        if let Some(i) = interval(&tf)
                            && !self
                                .candles
                                .keys()
                                .any(|(s, t)| s == &symbol && interval(t) == Some(i))
                        {
                            self.subscribe(cx, "unsubscribe", "ohlc", &[symbol], Some(i));
                        }
                    }
                }
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: tf,
                from_ms,
                ..
            } => {
                if let Some(i) = interval(&tf) {
                    let pair = self
                        .pairs
                        .get(&symbol)
                        .map_or(symbol.as_str(), String::as_str);
                    let mut p = format!("?pair={}&interval={i}", sign::encode_uri(pair));
                    if let Some(s) = from_ms {
                        p.push_str(&format!("&since={}", (s / 1000.).floor()));
                    }
                    self.read(
                        cx,
                        "OHLC",
                        &p,
                        Pending::Candles {
                            command,
                            params: p.clone(),
                            retries: 0,
                        },
                    );
                } else {
                    cx.done(command, Err("Kraken Spot unsupported interval".into()))
                }
            }
            _ => {}
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|s| s.synced).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.synced)
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}
