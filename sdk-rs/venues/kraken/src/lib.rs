//! Kraken futures: typed state machines driven entirely by engine events.
mod account;
mod spot;
pub use spot::KrakenSpot;
mod history;
pub mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketChannel, SocketId, TimerId, Venue,
};
use std::collections::{HashMap, HashSet};
use wire::*;
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
pub enum Pending {
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    Markets {
        rows: Vec<Market>,
        failed: bool,
    },
    Tickers {
        rows: Vec<Ticker>,
        failed: bool,
        revision: u64,
    },
    Candles {
        command: Option<CommandId>,
        symbol: String,
        interval: String,
        generation: Option<u64>,
        rows: Vec<Candle>,
        failed: bool,
    },
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
}
#[derive(Default)]
struct DepthState {
    book: Book,
    sequence: Option<u64>,
    subscribers: u32,
    last_received: f64,
    retry_at: f64,
}
#[derive(Default)]
struct CandleState {
    generation: u64,
    subscribers: u32,
    last: Option<Candle>,
    in_flight: bool,
    next_poll: f64,
}
pub struct Kraken {
    options: Options,
    signer: Option<sign::Signer>,
    read_signer: Option<sign::Signer>,
    account: account::Account,
    trading: trading::Trading,
    public: Connection,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, String), CandleState>,
    generation: u64,
    ticker_symbols: Vec<String>,
    ticker_revision: u64,
    ticker_sources: HashMap<String, u64>,
    ticker_in_flight: bool,
    markets_in_flight: bool,
    poll: Option<TimerId>,
}
impl Kraken {
    pub fn new(options: Options) -> Self {
        let signer =
            if !options.public_data_only && options.key.as_ref().is_some_and(|k| !k.is_empty()) {
                options
                    .secret
                    .as_deref()
                    .and_then(|s| sign::Signer::new(s).ok())
            } else {
                None
            };
        let read_signer = options
            .read_credentials
            .as_ref()
            .and_then(|r| sign::Signer::new(&r.secret).ok());
        Self {
            options,
            read_signer,
            signer,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            public: Connection::default(),
            books: HashMap::new(),
            candles: HashMap::new(),
            generation: 0,
            ticker_symbols: vec![],
            ticker_revision: 0,
            ticker_sources: HashMap::new(),
            ticker_in_flight: false,
            markets_in_flight: false,
            poll: None,
        }
    }
    fn host(&self) -> &'static str {
        if self.options.testnet {
            "https://demo-futures.kraken.com"
        } else {
            "https://futures.kraken.com"
        }
    }
    fn ws_url(&self) -> &'static str {
        if self.options.testnet {
            "wss://demo-futures.kraken.com/ws/v1"
        } else {
            "wss://futures.kraken.com/ws/v1"
        }
    }
    fn markets_request(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.markets_in_flight {
            return;
        }
        self.markets_in_flight = true;
        cx.request_streamed(
            Method::Get,
            format!("{}/derivatives/api/v3/instruments", self.host()),
            vec![],
            ArrayAt::Field("instruments"),
            Pending::Markets {
                rows: vec![],
                failed: false,
            },
        );
    }
    fn tickers_request(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.ticker_in_flight {
            return;
        }
        self.ticker_in_flight = true;
        cx.request_streamed(
            Method::Get,
            format!("{}/derivatives/api/v3/tickers", self.host()),
            vec![],
            ArrayAt::Field("tickers"),
            Pending::Tickers {
                rows: vec![],
                failed: false,
                revision: self.ticker_revision,
            },
        );
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = Some(cx.connect_channel(self.ws_url(), SocketChannel::Public));
        self.public.open = false;
        self.public.last_received = cx.now;
        self.public.watchdog = Some(cx.set_timer(15_000.0));
    }
    fn subscribe(&self, cx: &mut Cx<'_, Pending>, event: &str, feed: &str, products: &[&str]) {
        if let Some(s) = self.public.socket.filter(|_| self.public.open) {
            #[derive(serde::Serialize)]
            struct S<'a> {
                event: &'a str,
                feed: &'a str,
                product_ids: &'a [&'a str],
            }
            cx.send(
                s,
                serde_json::to_string(&S {
                    event,
                    feed,
                    product_ids: products,
                })
                .unwrap(),
            );
        }
    }
    fn selected_tickers<'a>(&'a self, cx: &'a Cx<'_, Pending>) -> Vec<&'a str> {
        let selected: Vec<_> = cx
            .store
            .markets()
            .iter()
            .filter(|m| self.ticker_symbols.contains(&m.symbol))
            .map(|m| m.symbol.as_str())
            .collect();
        if selected.is_empty() {
            cx.store
                .markets()
                .iter()
                .map(|m| m.symbol.as_str())
                .collect()
        } else {
            selected
        }
    }
    fn subscribe_tickers(&self, cx: &mut Cx<'_, Pending>, event: &str) {
        let products: Vec<String> = self
            .selected_tickers(cx)
            .into_iter()
            .map(str::to_owned)
            .collect();
        for batch in products.chunks(50) {
            self.subscribe(
                cx,
                event,
                "ticker",
                &batch.iter().map(String::as_str).collect::<Vec<_>>(),
            );
        }
    }
    fn invalidate_books(&mut self, cx: &mut Cx<'_, Pending>) {
        for (symbol, state) in &mut self.books {
            state.book.clear();
            state.sequence = None;
            cx.book_changed(symbol);
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, text: &str, snapshot: bool) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        let Ok(row) = serde_json::from_str::<Depth<'_>>(text) else {
            return;
        };
        if !snapshot {
            let Some(previous) = state.sequence else {
                return;
            };
            if row.seq <= previous {
                return;
            }
            if previous.checked_add(1) != Some(row.seq) {
                state.sequence = None;
                state.book.clear();
                state.retry_at = cx.now + 10_000.0;
                cx.book_changed(symbol);
                self.subscribe(cx, "unsubscribe", "book", &[symbol]);
                self.subscribe(cx, "subscribe", "book", &[symbol]);
                return;
            }
        }
        let scale = cx
            .store
            .market(symbol)
            .and_then(|m| m.contract_size)
            .unwrap_or(1.0);
        if snapshot {
            if state.sequence.is_some_and(|seq| row.seq <= seq) {
                return;
            }
            let (Some(bids), Some(asks)) = (&row.bids, &row.asks) else {
                return;
            };
            if bids
                .iter()
                .chain(asks)
                .any(|v| v.price <= 0.0 || v.qty < 0.0)
            {
                return;
            }
            state.book.clear();
            for (side, levels) in [(BookSide::Bids, bids), (BookSide::Asks, asks)] {
                for level in levels {
                    state.book.apply(side, level.price, level.qty * scale);
                }
            }
        } else {
            let side = match row.side.as_ref() {
                "buy" => BookSide::Bids,
                "sell" => BookSide::Asks,
                _ => return,
            };
            if row.price <= 0.0 || row.qty < 0.0 {
                return;
            }
            state.book.apply(side, row.price, row.qty * scale);
        }
        state.sequence = Some(row.seq);
        state.last_received = cx.now;
        cx.book_changed(symbol);
    }
    #[allow(clippy::too_many_arguments)]
    fn candles_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Option<CommandId>,
        symbol: String,
        interval: String,
        limit: Option<u32>,
        from_ms: Option<f64>,
        to_ms: Option<f64>,
    ) {
        if cx.store.market(&symbol).is_none() {
            if let Some(c) = command {
                cx.done(c, Err("Unknown Kraken market".into()));
            }
            return;
        }
        let (resolution, step) = interval_spec(&interval);
        let to = to_ms.unwrap_or(cx.now);
        let from = from_ms.unwrap_or(to - step * f64::from(limit.unwrap_or(500).clamp(1, 5000)));
        if !from.is_finite() || !to.is_finite() || from < 0.0 || from > to {
            if let Some(c) = command {
                cx.done(c, Err("Invalid Kraken candle range".into()));
            }
            return;
        }
        let generation = if command.is_none() {
            let Some(s) = self.candles.get_mut(&(symbol.clone(), interval.clone())) else {
                return;
            };
            if s.in_flight {
                return;
            }
            s.in_flight = true;
            Some(s.generation)
        } else {
            None
        };
        cx.request_streamed(
            Method::Get,
            format!(
                "{}/api/charts/v1/trade/{symbol}/{resolution}?from={:.0}&to={:.0}",
                self.host(),
                (from / 1000.0).floor(),
                (to / 1000.0).floor()
            ),
            vec![],
            ArrayAt::Field("candles"),
            Pending::Candles {
                command,
                symbol,
                interval,
                generation,
                rows: vec![],
                failed: false,
            },
        );
    }
    fn public_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Markets { .. } => self.markets_in_flight = false,
            Pending::Tickers { .. } => self.ticker_in_flight = false,
            Pending::Candles {
                command,
                symbol,
                interval,
                generation,
                ..
            } => {
                if let Some(c) = command {
                    cx.done(c, Err(error.into()));
                } else if let Some(s) = self
                    .candles
                    .get_mut(&(symbol, interval))
                    .filter(|s| Some(s.generation) == generation)
                {
                    s.in_flight = false;
                    s.next_poll = cx.now + 5_000.0;
                }
            }
            _ => {}
        }
        cx.log(LogLevel::Warning, error);
    }
}
fn interval_spec(interval: &str) -> (&str, f64) {
    match interval {
        "1m" => ("1m", 60_000.0),
        "5m" => ("5m", 300_000.0),
        "15m" => ("15m", 900_000.0),
        "30m" => ("30m", 1_800_000.0),
        "4h" => ("4h", 14_400_000.0),
        "12h" => ("12h", 43_200_000.0),
        "1d" => ("1d", 86_400_000.0),
        "1w" => ("1w", 604_800_000.0),
        _ => ("1h", 3_600_000.0),
    }
}
impl Venue for Kraken {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && let Err(error) = read.validate()
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        if !self.options.public_data_only
            && self
                .options
                .key
                .as_deref()
                .is_some_and(|key| !key.is_empty())
            && self.options.secret.is_some()
            && self.signer.is_none()
        {
            cx.log(
                LogLevel::Error,
                "Kraken secret must be nonempty valid base64",
            );
        }
        if self.options.read_credentials.is_some() && self.read_signer.is_none() {
            cx.log(
                LogLevel::Error,
                "Kraken secondary secret must be nonempty valid base64",
            );
            return;
        }
        self.markets_request(cx);
        self.poll = Some(cx.set_timer(5_000.0));
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_ws_open(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        self.subscribe_tickers(cx, "subscribe");
        let symbols: Vec<_> = self.books.keys().cloned().collect();
        for s in symbols {
            self.subscribe(cx, "subscribe", "book", &[&s]);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_ws_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        let Ok(msg) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        self.public.last_received = cx.now;
        if !msg.event.is_empty() {
            if msg.event == "error" {
                cx.log(LogLevel::Warning, "Kraken public subscription error");
            }
            return;
        }
        match msg.feed.as_ref() {
            "ticker" => {
                let Ok(row) = serde_json::from_str::<WsTicker>(text) else {
                    return;
                };
                self.ticker_revision += 1;
                self.ticker_sources
                    .insert(msg.product_id.to_string(), self.ticker_revision);
                cx.store.update_ticker(&msg.product_id, |t| {
                    if let Some(v) = row.bid {
                        t.bid = v
                    }
                    if let Some(v) = row.ask {
                        t.ask = v
                    }
                    if let Some(v) = row.last {
                        t.last = v
                    }
                    if let Some(v) = row.mark {
                        t.mark = v
                    }
                    if let Some(v) = row.index {
                        t.index = v
                    }
                    // Full ticker payloads omit relative funding when it is zero.
                    t.funding_rate = row.relative_funding_rate.unwrap_or(0.0);
                    if let Some(v) = row.volume {
                        t.volume = v
                    }
                    if let Some(v) = row.volume_quote {
                        t.quote_volume = v
                    }
                    if let Some(v) = row.open_interest {
                        t.open_interest = v
                    }
                    if let Some(v) = row.next_funding_rate_time {
                        t.funding_time = v
                    }
                    if let Some(change) = row.change {
                        t.percentage = change;
                    } else if let (Some(open), Some(last)) = (row.open_24h, row.last)
                        && open > 0.0
                    {
                        t.percentage = (last - open) / open * 100.0;
                    }
                });
            }
            "book_snapshot" => self.depth(cx, &msg.product_id, text, true),
            "book" => self.depth(cx, &msg.product_id, text, false),
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_ws_close(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        if let Some(t) = self.public.watchdog.take() {
            cx.clear_timer(t);
        }
        self.invalidate_books(cx);
        self.public.attempts = self.public.attempts.saturating_add(1);
        self.public.reconnect =
            Some(cx.set_timer(
                (1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.0),
            ));
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Account(r) => self.account_item(cx, r, item),
            Pending::History(r) => self.history_item(cx, r, item),
            Pending::Trade(_) => {}
            Pending::Markets { rows, failed } => match serde_json::from_str::<Instrument<'_>>(item)
            {
                Ok(m) => {
                    if !m.symbol.starts_with("PF_") || m.tradeable == Some(false) {
                        return;
                    }
                    let decimals = m.contract_value_trade_precision.unwrap_or(0.0);
                    if !(-18.0..=18.0).contains(&decimals) || decimals.fract() != 0.0 {
                        *failed = true;
                        return;
                    }
                    rows.push(Market {
                        id: format!(
                            "{}/{}",
                            m.base,
                            if m.quote.is_empty() { "USD" } else { &m.quote }
                        ),
                        symbol: m.symbol.to_string(),
                        pretty_symbol: Some(m.symbol[3..].into()),
                        base: m.base.to_string(),
                        quote: if m.quote.is_empty() {
                            "USD".into()
                        } else {
                            m.quote.to_string()
                        },
                        active: m.status != "cancel_only",
                        precision: Precision {
                            amount: 10f64.powi(-(decimals as i32)),
                            price: m.tick_size.filter(|n| *n > 0.0).unwrap_or(0.01),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: 0.0,
                                max: 9_007_199_254_740_991.0,
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: m
                                    .margin_levels
                                    .first()
                                    .filter(|m| m.initial_margin > 0.0)
                                    .map_or(100.0, |m| (1.0 / m.initial_margin).floor()),
                            },
                            price: None,
                        },
                        contract_size: Some(m.contract_size.filter(|n| *n > 0.0).unwrap_or(1.0)),
                        is_expiring: Some(m.last_trading_time.is_some()),
                        is_inverse: Some(m.contract_type == "inverse"),
                        is_spot: Some(false),
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                }
                Err(error) => {
                    *failed = true;
                    cx.log(
                        LogLevel::Warning,
                        format!("Kraken invalid public row: {error}"),
                    );
                }
            },
            Pending::Tickers { rows, failed, .. } => {
                match serde_json::from_str::<RestTicker<'_>>(item) {
                    Ok(row) => {
                        if let Some(m) = cx.store.market(&row.symbol) {
                            let last = row.last.filter(|v| *v > 0.0).unwrap_or(row.mark_price);
                            rows.push(Ticker {
                                id: m.id.clone(),
                                symbol: m.symbol.clone(),
                                bid: row.bid.unwrap_or(0.0),
                                ask: row.ask.unwrap_or(0.0),
                                last,
                                mark: if row.mark_price > 0.0 {
                                    row.mark_price
                                } else {
                                    last
                                },
                                index: if row.index_price > 0.0 {
                                    row.index_price
                                } else {
                                    last
                                },
                                percentage: row.change24h,
                                funding_rate: if last > 0.0 {
                                    row.funding_rate / last
                                } else {
                                    0.0
                                },
                                open_interest: row.open_interest,
                                funding_time: 0.0,
                                volume: row.vol24h,
                                quote_volume: row.volume_quote,
                                ..Ticker::default()
                            });
                        }
                    }
                    Err(error) => {
                        *failed = true;
                        cx.log(
                            LogLevel::Warning,
                            format!("Kraken invalid public row: {error}"),
                        );
                    }
                }
            }
            Pending::Candles { rows, failed, .. } => {
                match serde_json::from_str::<CandleRow>(item) {
                    Ok(row) => rows.push(row.candle()),
                    Err(error) => {
                        *failed = true;
                        cx.log(
                            LogLevel::Warning,
                            format!("Kraken invalid public row: {error}"),
                        );
                    }
                }
            }
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let pending = match pending {
            Pending::Account(r) => {
                self.account_http(cx, r, response);
                return;
            }
            Pending::Trade(r) => {
                self.trading_http(cx, r, response);
                return;
            }
            Pending::History(r) => {
                self.history_http(cx, *r, response);
                return;
            }
            p => p,
        };
        if !(200..300).contains(&response.status)
            || (!matches!(pending, Pending::Candles { .. }) && !success(response))
        {
            self.public_failed(cx, pending, "Kraken public read failed");
            return;
        }
        match pending {
            Pending::Markets { rows, failed } => {
                self.markets_in_flight = false;
                if failed || rows.is_empty() {
                    cx.log(LogLevel::Warning, "Invalid Kraken markets");
                    return;
                }
                let mut seen = HashSet::new();
                if rows.iter().any(|m| !seen.insert(&m.symbol)) {
                    cx.log(LogLevel::Warning, "Duplicate Kraken market");
                    return;
                }
                cx.store.set_markets(rows);
                self.tickers_request(cx);
            }
            Pending::Tickers {
                rows,
                failed,
                revision,
            } => {
                self.ticker_in_flight = false;
                if failed || rows.is_empty() {
                    cx.log(LogLevel::Warning, "Invalid Kraken tickers");
                    return;
                }
                if !cx.store.loaded().tickers {
                    cx.store.set_tickers(rows);
                    self.connect(cx);
                    self.account_start(cx);
                } else {
                    for row in rows {
                        if self.ticker_sources.get(&row.symbol).copied().unwrap_or(0) <= revision {
                            let symbol = row.symbol.clone();
                            cx.store.update_ticker(&symbol, |t| *t = row);
                        }
                    }
                }
            }
            Pending::Candles {
                command,
                symbol,
                interval,
                generation,
                mut rows,
                failed,
            } => {
                let envelope = serde_json::from_str::<CandleEnvelope<'_>>(response.body);
                if !envelope.as_ref().is_ok_and(|e| {
                    let _ = &e.candles;
                    !e.more_candles
                        && e.error.is_empty()
                        && !matches!(e.status.as_ref(), "error" | "unauthorized")
                }) {
                    self.public_failed(
                        cx,
                        Pending::Candles {
                            command,
                            symbol,
                            interval,
                            generation,
                            rows,
                            failed,
                        },
                        "Kraken candle read failed or incomplete",
                    );
                    return;
                }
                if command.is_none()
                    && !self
                        .candles
                        .get(&(symbol.clone(), interval.clone()))
                        .is_some_and(|s| Some(s.generation) == generation)
                {
                    return;
                }
                if failed {
                    self.public_failed(
                        cx,
                        Pending::Candles {
                            command,
                            symbol,
                            interval,
                            generation,
                            rows,
                            failed,
                        },
                        "Invalid Kraken candles",
                    );
                    return;
                }
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                rows.dedup_by(|a, b| a.timestamp == b.timestamp);
                if let Some(c) = command {
                    cx.done(c, serde_json::to_string(&rows).map_err(|e| e.to_string()));
                } else if let Some(s) = self.candles.get_mut(&(symbol.clone(), interval.clone())) {
                    s.in_flight = false;
                    s.next_poll = cx.now + 5_000.0;
                    if let Some(latest) = rows.last().copied()
                        && s.last
                            .is_none_or(|old| latest.timestamp >= old.timestamp && latest != old)
                    {
                        cx.candle(&symbol, &interval, latest);
                        s.last = Some(latest);
                    }
                }
            }
            _ => unreachable!(),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(r) => self.account_failed(cx, r, error),
            Pending::Trade(r) => self.trading_failed(cx, r, error),
            Pending::History(r) => self.history_failed(cx, *r, error),
            p => self.public_failed(cx, p, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect(cx);
            return;
        }
        if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.last_received >= 45_000.0 {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006);
                }
            } else {
                if let Some(s) = self.public.socket.filter(|_| self.public.open) {
                    cx.send(s, "{\"event\":\"ping\"}");
                }
                self.public.watchdog = Some(cx.set_timer(15_000.0));
            }
            return;
        }
        if self.poll == Some(timer) {
            self.poll = None;
            if !cx.store.loaded().markets {
                self.markets_request(cx);
            } else if !cx.store.loaded().tickers {
                self.tickers_request(cx);
            }
            let stale: Vec<_> = self
                .books
                .iter()
                .filter(|(_, s)| cx.now - s.last_received > 120_000.0 && cx.now >= s.retry_at)
                .map(|(s, _)| s.clone())
                .collect();
            for symbol in stale {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.sequence = None;
                    s.book.clear();
                    s.retry_at = cx.now + 30_000.0;
                    cx.book_changed(&symbol);
                }
                self.subscribe(cx, "unsubscribe", "book", &[&symbol]);
                self.subscribe(cx, "subscribe", "book", &[&symbol]);
            }
            let candles: Vec<_> = self
                .candles
                .iter()
                .filter(|(_, s)| !s.in_flight && cx.now >= s.next_poll)
                .map(|(k, _)| k.clone())
                .collect();
            for (symbol, interval) in candles {
                self.candles_request(cx, None, symbol, interval, Some(2), None, None);
            }
            self.poll = Some(cx.set_timer(5_000.0));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if self.trading_command(cx, &command) || self.history_command(cx, &command) {
            return;
        }
        match command {
            Command::SetTickerSymbols { symbols } => {
                self.subscribe_tickers(cx, "unsubscribe");
                self.ticker_symbols = symbols;
                self.subscribe_tickers(cx, "subscribe");
            }
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && cx.store.market(&symbol).is_none() {
                    return;
                }
                let first = !self.books.contains_key(&symbol);
                let s = self.books.entry(symbol.clone()).or_default();
                s.subscribers += 1;
                s.last_received = cx.now;
                s.retry_at = cx.now + 30_000.0;
                if first {
                    self.subscribe(cx, "subscribe", "book", &[&symbol]);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers == 0 {
                        self.books.remove(&symbol);
                        self.subscribe(cx, "unsubscribe", "book", &[&symbol]);
                    }
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if cx.store.market(&symbol).is_none() {
                    return;
                }
                self.generation += 1;
                let generation = self.generation;
                let s = self
                    .candles
                    .entry((symbol.clone(), interval.clone()))
                    .or_insert_with(|| CandleState {
                        generation,
                        ..Default::default()
                    });
                s.subscribers += 1;
                self.candles_request(cx, None, symbol, interval, Some(2), None, None);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let k = (symbol, interval);
                if let Some(s) = self.candles.get_mut(&k) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers == 0 {
                        self.candles.remove(&k);
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
            } => self.candles_request(cx, Some(command), symbol, interval, limit, from_ms, to_ms),
            _ => {}
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.sequence.is_some())
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, s)| s.sequence.is_some())
                .map(|(s, b)| (s.as_str(), &b.book)),
        )
    }
}

#[cfg(test)]
mod secondary_tests;
