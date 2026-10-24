//! Kraken Spot has separate REST authentication and v2 sockets from futures.
mod account;
mod book;
mod trading;
mod wire;
use crate::{Options, sign};
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{
    Context, Operation, Policy, RetryEvidence, RetryPolicy, Scope,
};
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId, Venue,
    VenueError,
    book::{Book, BookSide},
    model::*,
    socket::{Config, Supervisor, Tick},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use wire::*;

#[derive(Clone)]
pub enum Pending {
    Private(account::Request),
    Markets {
        context: Context,
    },
    Tickers {
        context: Context,
    },
    Candles {
        context: Context,
        params: String,
        retries: u8,
    },
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
                connect_timeout_ms: 45_000.,
                idle_timeout_ms: 45_000.,
                ping_interval_ms: Some(15_000.),
                backoff_cap_ms: 16_000.,
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct DepthState {
    book: Book,
    checksum: book::Checksum,
    synced: bool,
    subscribers: u32,
}
pub struct KrakenSpot {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    live_tickers: safe_cex_engine::startup::LiveTickers,
    signer: Option<sign::Signer>,
    public: Connection,
    account: account::Account,
    trading: trading::Trading,
    pairs: HashMap<String, String>,
    identities: HashMap<String, String>,
    ws_symbols: HashMap<String, String>,
    ws_canonical: HashMap<String, String>,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, Interval), u32>,
    ticker_symbols: Vec<String>,
    poll: Option<TimerId>,
    markets_in_flight: bool,
    tickers_in_flight: bool,
    /// Bases whose USD route the last catalogue read rejected as malformed.
    rejected_routes: HashSet<String>,
    ticker_reseed: bool,
}
impl KrakenSpot {
    pub fn new(options: Options) -> Self {
        let mut options = safe_cex_crypto::secrets::Sensitive::new(options);
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
            discovery: Default::default(),
            live_tickers: Default::default(),
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
            rejected_routes: HashSet::new(),
            ticker_reseed: false,
        }
    }
    fn public_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Markets { context } => {
                let _ = cx.finish_http_context(&context);
                self.markets_in_flight = false;
                self.discovery.failed(cx, false);
            }
            Pending::Tickers { context } => {
                let _ = cx.finish_http_context(&context);
                self.tickers_in_flight = false;
                self.ticker_reseed = false;
                self.discovery.failed(cx, true);
            }
            Pending::Candles { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::Private(_) => unreachable!(),
        }
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
    }
    fn read(&mut self, cx: &mut Cx<'_, Pending>, path: &str, context: Context, pending: Pending) {
        if let Err((pending, reason)) = cx.request_managed(
            context,
            Method::Get,
            format!("https://api.kraken.com/0/public/{path}"),
            None,
            pending,
        ) {
            self.public_failed(cx, pending, &reason.to_string());
        }
    }
    fn candle_request(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let Pending::Candles {
            context, params, ..
        } = &pending
        else {
            unreachable!()
        };
        if let Err((pending, error)) = cx.request_managed(
            context.clone(),
            Method::Get,
            format!("https://api.kraken.com/0/public/OHLC{params}"),
            None,
            pending,
        ) {
            self.candle_preparation_failed(cx, pending, error.venue_error());
        }
    }
    fn candle_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: VenueError,
    ) {
        let Pending::Candles { context, .. } = pending else {
            unreachable!()
        };
        let _ = cx.fail_context(&context, error);
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.markets_in_flight {
            let context = match cx.http_context(
                Scope::Background,
                crate::public_context::policy(Operation::Read),
            ) {
                Ok(context) => context,
                Err(reason) => {
                    self.discovery.failed(cx, false);
                    cx.notice(Level::Warn, code::REQUEST_FAILED, reason.to_string());
                    return;
                }
            };
            self.markets_in_flight = true;
            self.read(
                cx,
                "AssetPairs",
                context.clone(),
                Pending::Markets { context },
            );
        }
    }
    fn tickers(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.tickers_in_flight {
            let context = match cx.http_context(
                Scope::Background,
                crate::public_context::policy(Operation::Read),
            ) {
                Ok(context) => context,
                Err(reason) => {
                    self.discovery.failed(cx, true);
                    cx.notice(Level::Warn, code::REQUEST_FAILED, reason.to_string());
                    return;
                }
            };
            self.tickers_in_flight = true;
            self.live_tickers.begin();
            self.read(cx, "Ticker", context.clone(), Pending::Tickers { context });
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new("wss://ws.kraken.com/v2")
                    .channel(SocketChannel::Public),
            ),
        );
        self.public.open = false;
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
        let mut rejected = false;
        let mut rejected_routes = HashSet::new();
        map_each::<&serde_json::value::RawValue>(text, |key, raw| {
            if key.ends_with(".d") {
                return;
            }
            let mut reject = |wsname: &str| {
                rejected = true;
                rejected_routes.extend(rejected_usd_route(&key, wsname));
                safe_cex_engine::startup::skipped_row(cx);
            };
            let Ok(p) = serde_json::from_str::<Pair<'_>>(raw.get()) else {
                let name = serde_json::from_str::<PairName<'_>>(raw.get());
                reject(name.as_ref().map_or("", |n| &n.wsname));
                return;
            };
            let Some((base, quote)) = p.wsname.split_once('/') else {
                reject(&p.wsname);
                return;
            };
            if base.is_empty()
                || quote.is_empty()
                || base.trim() != base
                || quote.trim() != quote
                || quote.contains('/')
            {
                reject(&p.wsname);
                return;
            }
            let base = asset(base);
            let quote = asset(quote);
            if !matches!(quote, "USD" | "USDT" | "USDC") {
                return;
            }
            let pd = p.pair_decimals.unwrap_or(2);
            let ad = p.lot_decimals.unwrap_or(8);
            if pd > 18 || ad > 18 || p.ordermin < 0. {
                reject(&p.wsname);
                return;
            }
            let symbol = p.wsname.to_string();
            let pretty = format!("{base}{quote}");
            if pairs.contains_key(&symbol) {
                reject(&p.wsname);
                return;
            }
            for id in [&*key, &*p.altname, &symbol, &pretty] {
                if !id.is_empty() {
                    identities.insert(id.to_owned(), symbol.clone());
                }
            }
            pairs.insert(symbol.clone(), key.to_string());
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
        if markets.is_empty() {
            return Err("No supported Kraken Spot market rows".into());
        }
        let added = markets
            .iter()
            .any(|market| !self.pairs.contains_key(&market.symbol));
        let recovered: Vec<_> = markets
            .iter()
            .filter(|market| {
                !self.pairs.contains_key(&market.symbol)
                    && (self.ticker_symbols.is_empty()
                        || self.ticker_symbols.contains(&market.symbol))
            })
            .map(|market| market.symbol.clone())
            .collect();
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
        self.rejected_routes = rejected_routes;
        if rejected {
            self.discovery.failed(cx, false);
        } else {
            self.discovery.succeeded(cx, &false);
        }
        cx.store.set_markets(markets);
        if self.public.open {
            for batch in recovered.chunks(50) {
                self.subscribe(cx, "subscribe", "ticker", batch, None);
            }
        }
        self.ticker_reseed |= added && cx.store.loaded().tickers;
        if added {
            self.revalue_after_catalogue(cx);
        } else {
            self.revalue_pending_balances(cx);
        }
        if self.public.socket.is_none() {
            self.connect(cx);
        }
        self.private_start(cx);
        // A persistently rejected pair re-reads only the catalogue; tickers are
        // re-seeded when the read lists a new pair.
        if !cx.store.loaded().tickers || added {
            self.tickers(cx);
        }
        Ok(())
    }
    fn tickers_response(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> Result<(), String> {
        let mut rows = vec![];
        map_each::<&serde_json::value::RawValue>(text, |key, raw| {
            let Ok(t) = serde_json::from_str::<RestTicker>(raw.get()) else {
                safe_cex_engine::startup::skipped_row(cx);
                return;
            };
            let Some(symbol) = self.identities.get(&*key) else {
                return;
            };
            let Some(m) = cx.store.market(symbol) else {
                return;
            };
            let n = |s: Option<&String>| -> Option<f64> {
                safe_cex_engine::startup::number_or_zero(s?)
            };
            let Some((ask, bid, last, volume, open)) = n(t.a.first())
                .zip(n(t.b.first()))
                .zip(n(t.c.first()))
                .zip(n(t.v.get(1)))
                .zip(n(Some(&t.o)))
                .map(|((((a, b), c), v), o)| (a, b, c, v, o))
            else {
                safe_cex_engine::startup::skipped_row(cx);
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
        if rows.is_empty() {
            return Err("No valid Kraken Spot ticker rows".into());
        }
        self.discovery.succeeded(cx, &true);
        self.live_tickers.finish(cx.store, &mut rows);
        cx.store.set_tickers(rows);
        self.revalue_pending_balances(cx);
        if self.public.socket.is_none() {
            self.connect(cx);
        }
        Ok(())
    }
}
impl Venue for KrakenSpot {
    #[inline]
    fn dispatch_nonce() -> bool {
        true
    }

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
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.revalue_pending_balances(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.private_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.ticker_subscribe(cx, "subscribe");
        for symbol in self.books.keys() {
            self.subscribe(cx, "subscribe", "book", std::slice::from_ref(symbol), None)
        }
        let mut sent = HashSet::new();
        for (symbol, tf) in self.candles.keys() {
            if let Some(i) = interval(*tf)
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
        self.public.supervisor.received(cx.now);
        let Ok(msg) = serde_json::from_str::<Ws<'_>>(text) else {
            return;
        };
        if msg.success == Some(false) {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                format!("Kraken Spot subscription: {}", msg.error),
            );
            return;
        }
        let Some(data) = msg.data else { return };
        match &*msg.channel {
            "ticker" => {
                let mut has_wallet_quote = false;
                let _ = safe_cex_engine::json::each::<WsTicker<'_>>(data.get(), |t| {
                    has_wallet_quote |= t.last.is_some_and(|last| last.is_finite() && last > 0.0);
                    let symbol = self
                        .ws_canonical
                        .get(&*t.symbol)
                        .map_or(&*t.symbol, String::as_str);
                    let supplied = u16::from(t.bid.is_some())
                        | (u16::from(t.ask.is_some()) << 1)
                        | (u16::from(t.last.is_some()) << 2)
                        | (u16::from(t.last.is_some()) << 3)
                        | (u16::from(t.last.is_some()) << 4)
                        | (u16::from(t.change_pct.is_some()) << 5)
                        | (u16::from(t.volume.is_some()) << 9);
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
                    self.live_tickers.record(cx.store, symbol, supplied);
                });
                if has_wallet_quote {
                    self.revalue_pending_balances(cx);
                }
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
                        .any(|l| l.price.value <= 0. || l.qty.value < 0.)
                    {
                        return;
                    }
                    if snapshot {
                        state.book.clear();
                        state.checksum.clear();
                        state.synced = true
                    }
                    for (side, levels) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
                        for l in levels {
                            state.checksum.apply(side, &l);
                            state.book.apply(side, l.price.value, l.qty.value)
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
                    if !state.checksum.matches(&state.book, d.checksum) {
                        state.book.clear();
                        state.checksum.clear();
                        state.synced = false;
                        cx.book_changed(symbol);
                        let symbols = [symbol.to_owned()];
                        self.subscribe(cx, "unsubscribe", "book", &symbols, None);
                        self.subscribe(cx, "subscribe", "book", &symbols, None);
                        return;
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
                            && interval(*tf) == Some(o.interval)
                        {
                            cx.candle(symbol, *tf, candle);
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
        for (symbol, s) in &mut self.books {
            s.book.clear();
            s.checksum.clear();
            s.synced = false;
            cx.book_changed(symbol);
        }
        self.public.supervisor.retry(cx);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Pending::Private(request) = pending {
            self.private_response(cx, request, response);
            return;
        }
        if let Pending::Candles { context, .. } = &pending
            && let Err(error) = cx.validate_read_http_delivery(context, context)
        {
            let _ = cx.fail_context(context, error.venue_error());
            return;
        }
        if let Pending::Markets { context } | Pending::Tickers { context } = &pending
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        let r = result(response).and_then(|text| match pending.clone() {
            Pending::Private(_) => unreachable!(),
            Pending::Markets { context } => {
                let _ = cx.finish_http_context(&context);
                self.markets_in_flight = false;
                self.markets_response(cx, text)
            }
            Pending::Tickers { context } => {
                let _ = cx.finish_http_context(&context);
                self.tickers_in_flight = false;
                self.ticker_reseed = false;
                self.tickers_response(cx, text)
            }
            Pending::Candles { context, .. } => {
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
                {
                    let result: Result<String, String> =
                        serde_json::to_string(&rows).map_err(|e| e.to_string());
                    match result {
                        Ok(data) => {
                            let _ = cx.done_context(&context, data);
                        }
                        Err(error) => {
                            let _ = cx.fail_context(&context, error);
                        }
                    }
                };
                Ok(())
            }
        });
        if let Err(e) = r {
            self.public_failed(cx, pending, &e);
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        if matches!(&pending, Pending::Candles { .. }) {
            self.candle_preparation_failed(cx, pending, error);
        } else {
            self.on_sign_error(cx, pending, error);
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        error: VenueError,
    ) {
        let Pending::Candles {
            context, retries, ..
        } = &mut pending
        else {
            self.on_request_failed(cx, pending, &error.message);
            return;
        };
        if let Err(reason) = cx.validate_http_context(context) {
            self.candle_preparation_failed(cx, pending, reason.venue_error());
            return;
        }
        if *retries < 3 {
            match cx.http_retry_context(context, RetryEvidence::ReadFailure) {
                Ok(next) => {
                    *context = next;
                    *retries += 1;
                    self.candle_request(cx, pending);
                }
                Err(reason) => self.candle_preparation_failed(cx, pending, reason.venue_error()),
            }
        } else {
            let message = error.message.clone();
            self.candle_preparation_failed(cx, pending, error);
            cx.notice(Level::Warn, code::REQUEST_FAILED, message);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::Private(request) = pending {
            self.private_network_failed(cx, request, error);
        } else {
            self.public_failed(cx, pending, error);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(tickers) = self.discovery.on_timer(timer) {
            if tickers {
                self.tickers(cx);
            } else {
                self.markets(cx);
            }
            return;
        }
        if self.private_timer(cx, timer) {
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => (),
            Tick::Reconnect => return self.connect(cx),
            Tick::Ping => {
                if let Some(s) = self.public.socket.filter(|_| self.public.open) {
                    cx.send(s, "{\"method\":\"ping\"}");
                }
                return;
            }
            Tick::TimedOut(_) => {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006)
                }
                return;
            }
            Tick::Handled => return,
        }
        if self.poll == Some(timer) {
            self.poll = None;
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
                let Some(i) = interval(tf) else {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        tf.unsupported("Kraken Spot"),
                    );
                    return;
                };
                let count = self.candles.entry((symbol.clone(), tf)).or_default();
                *count += 1;
                if *count == 1 {
                    self.subscribe(cx, "subscribe", "ohlc", &[symbol], Some(i));
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: tf,
            } => {
                let k = (symbol.clone(), tf);
                if let Some(count) = self.candles.get_mut(&k) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&k);
                        if let Some(i) = interval(tf)
                            && !self
                                .candles
                                .keys()
                                .any(|(s, t)| s == &symbol && interval(*t) == Some(i))
                        {
                            self.subscribe(cx, "unsubscribe", "ohlc", &[symbol], Some(i));
                        }
                    }
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval: tf,
                from_ms,
                ..
            } => {
                if let Some(i) = interval(tf) {
                    let pair = self
                        .pairs
                        .get(&symbol)
                        .map_or(symbol.as_str(), String::as_str);
                    let mut p = format!(
                        "?pair={}&interval={i}",
                        safe_cex_engine::url::encode_component(pair)
                    );
                    if let Some(s) = from_ms {
                        p.push_str(&format!("&since={}", (s / 1000.).floor()));
                    }
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::ReadFailure,
                    }) else {
                        return;
                    };
                    self.candle_request(
                        cx,
                        Pending::Candles {
                            context,
                            params: p,
                            retries: 0,
                        },
                    );
                } else {
                    crate::public_context::local(
                        cx,
                        Operation::Read,
                        Err(tf.unsupported("Kraken Spot").into()),
                    )
                }
            }
            _ => {}
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books.get(symbol).filter(|s| s.synced).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
