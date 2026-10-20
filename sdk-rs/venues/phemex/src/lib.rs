//! Phemex stablecoin perpetuals, as a typed sans-I/O state machine.
mod account;
mod account_wire;
mod history;
mod http;
mod income;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Market, Ticker};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use std::collections::HashMap;
use wire::*;
#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
}
pub enum Pending {
    Markets(Vec<Market>, bool),
    Tickers(Vec<Ticker>, bool),
    Depth {
        symbol: String,
        generation: u64,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
        invalid: bool,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Income(income::Request),
}
#[derive(Default)]
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    request: u32,
}
struct BookState {
    users: u32,
    book: Book,
    last: Option<u64>,
    generation: u64,
    fetching: bool,
    buffer: Vec<(u64, bool, Depth)>,
    retry: Option<TimerId>,
    deadline: Option<TimerId>,
}
impl BookState {
    fn new(generation: u64) -> Self {
        Self {
            users: 1,
            book: Book::default(),
            last: None,
            generation,
            fetching: false,
            buffer: vec![],
            retry: None,
            deadline: None,
        }
    }
}
pub struct PhemexPerpetuals {
    options: Options,
    credentials: Option<sign::Credentials>,
    read_credentials: Option<sign::Credentials>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, String), u32>,
    generation: u64,
    native_symbols: HashMap<String, String>,
    startup_retry: Option<TimerId>,
    paused_until: f64,
    account: account::State,
    trading: trading::State,
    history: history::State,
    http_budget: http::Budget,
    income: income::State,
}
impl PhemexPerpetuals {
    pub fn new(options: Options) -> Self {
        let credentials = options
            .key
            .clone()
            .zip(options.secret.clone())
            .filter(|(k, s)| !k.is_empty() && !s.is_empty())
            .map(|(key, secret)| sign::Credentials { key, secret });
        let read_credentials = options
            .read_credentials
            .as_ref()
            .map(|read| sign::Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
            });
        Self {
            options,
            credentials,
            read_credentials,
            public: Connection {
                supervisor: Supervisor::new(Config {
                    idle_timeout_ms: 20_000.,
                    ping_interval_ms: Some(5_000.),
                    ..Config::default()
                }),
                ..Connection::default()
            },
            books: HashMap::new(),
            candles: HashMap::new(),
            generation: 0,
            native_symbols: HashMap::new(),
            startup_retry: None,
            paused_until: 0.,
            account: account::State::default(),
            trading: trading::State::default(),
            history: history::State::default(),
            http_budget: http::Budget::default(),
            income: income::State::default(),
        }
    }
    fn native_symbol<'a>(&'a self, symbol: &'a str) -> &'a str {
        self.native_symbols
            .get(symbol)
            .map(String::as_str)
            .unwrap_or(symbol)
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://testnet-api.phemex.com"
        } else {
            "https://api.phemex.com"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://testnet-api.phemex.com/ws"
        } else {
            "wss://ws.phemex.com"
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() || self.public.supervisor.is_waiting() {
            return;
        }
        self.public.socket =
            Some(cx.connect_channel(self.ws(), safe_cex_engine::SocketChannel::Public));
        self.public.supervisor.connecting(cx);
    }
    fn send(&mut self, cx: &mut Cx<'_, Pending>, method: &str, params: String) {
        if !self.public.open {
            return;
        }
        if let Some(s) = self.public.socket {
            self.public.request += 1;
            cx.send(
                s,
                format!(
                    r#"{{"id":{},"method":"{method}","params":{params}}}"#,
                    self.public.request
                ),
            );
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, subscribe: bool) {
        self.send(
            cx,
            if subscribe {
                "orderbook_p.subscribe"
            } else {
                "orderbook_p.unsubscribe"
            },
            if subscribe {
                format!(
                    "[{},false,0]",
                    serde_json::to_string(self.native_symbol(symbol)).unwrap()
                )
            } else {
                serde_json::to_string(&[self.native_symbol(symbol)]).unwrap()
            },
        );
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(b) = self.books.get_mut(symbol) else {
            return;
        };
        if b.fetching {
            return;
        }
        if cx.now < self.paused_until {
            b.retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        b.fetching = true;
        b.deadline = Some(cx.set_timer(20_000.));
        let generation = b.generation;
        self.request(
            cx,
            Method::Get,
            "/md/v2/fullbook",
            &format!(
                "symbol={}&limit=1000",
                url_encode(self.native_symbol(symbol))
            ),
            None,
            Pending::Depth {
                symbol: symbol.into(),
                generation,
            },
            None,
        );
    }
    fn book_failed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(b) = self.books.get_mut(symbol) {
            self.generation += 1;
            b.generation = self.generation;
            b.fetching = false;
            if let Some(t) = b.deadline.take() {
                cx.clear_timer(t)
            }
            b.last = None;
            b.buffer.clear();
            if b.retry.is_none() {
                b.retry = Some(cx.set_timer(1_000.));
            }
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, r: DepthEvent<'_>) {
        let normalized = r.symbol.to_ascii_uppercase();
        let symbol = normalized.as_str();
        let Some(b) = self.books.get_mut(symbol) else {
            return;
        };
        if b.last.is_none() {
            if b.buffer.len() >= 2_000 {
                self.book_failed(cx, symbol);
                return;
            }
            b.buffer
                .push((r.sequence, r.kind == "snapshot", r.orderbook_p));
            self.snapshot(cx, symbol);
            return;
        }
        // Phemex coalesces updates every 20ms; sequence numbers need not be adjacent.
        if r.sequence <= b.last.unwrap() {
            return;
        }
        if r.kind == "snapshot" {
            b.book.clear();
        }
        apply(&mut b.book, r.orderbook_p);
        b.last = Some(r.sequence);
        cx.book_changed(symbol);
    }
    fn fail(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Account(r) => self.account_failed(cx, r, error),
            Pending::Trade(r) => self.trade_failed(cx, r, error),
            Pending::History(r) => self.history_failed(cx, r, error),
            Pending::Income(r) => self.income_failed(cx, r, error),
            Pending::Depth { symbol, generation } => {
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|b| b.generation == generation)
                {
                    self.book_failed(cx, &symbol)
                }
            }
            Pending::Candles { command, .. } => cx.done(command, Err(error.into())),
            Pending::Markets(..) | Pending::Tickers(..) => {
                cx.log(LogLevel::Warning, format!("Phemex public startup: {error}"));
                if self.startup_retry.is_none() {
                    self.startup_retry = Some(cx.set_timer(2_000.));
                }
            }
        }
    }
}
fn apply(book: &mut Book, d: Depth) {
    for (side, rows) in [(BookSide::Bids, d.bids), (BookSide::Asks, d.asks)] {
        for Level(N(p), N(a)) in rows {
            if p > 0. && a >= 0. {
                book.apply(side, p, a);
            }
        }
    }
}
impl Venue for PhemexPerpetuals {
    fn guard_pending_placements(&self) -> bool {
        true
    }

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
        if cx.now < self.paused_until {
            self.startup_retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        self.request(
            cx,
            Method::Get,
            "/public/products",
            "",
            None,
            Pending::Markets(vec![], false),
            Some(ArrayAt::PathOmitLists(&["data", "perpProductsV2"])),
        );
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
        self.send(cx, "perp_market24h_pack_p.subscribe", "[]".into());
        for s in self.books.keys().cloned().collect::<Vec<_>>() {
            self.subscribe_book(cx, &s, true)
        }
        for (s, i) in self.candles.keys().cloned().collect::<Vec<_>>() {
            self.send(
                cx,
                "kline_p.subscribe",
                format!(
                    "[{},{}]",
                    serde_json::to_string(self.native_symbol(&s)).unwrap(),
                    resolution(&i).unwrap()
                ),
            );
        }
        self.send(cx, "server.ping", "[]".into());
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.supervisor.received(cx.now);
        let Ok(e) = serde_json::from_str::<Envelope<'_>>(text) else {
            return;
        };
        if e.method == "perp_market24h_pack_p.update" {
            #[derive(serde::Deserialize)]
            struct Pack<'a> {
                #[serde(borrow)]
                data: Vec<Packed<'a>>,
            }
            if let Ok(p) = serde_json::from_str::<Pack<'_>>(text) {
                let mut added = Vec::new();
                for r in p.data {
                    let symbol = r.0.to_ascii_uppercase();
                    if cx.store.ticker(&symbol).is_none()
                        && !added.iter().any(|t: &Ticker| t.symbol == symbol)
                        && let Some(m) = cx.store.market(&symbol)
                    {
                        added.push(Ticker {
                            id: m.id.clone(),
                            symbol: symbol.clone(),
                            last: r.4.0,
                            mark: r.9.0,
                            index: r.8.0,
                            bid: r.12.as_ref().map_or(r.4.0, |n| n.0),
                            ask: r.13.as_ref().map_or(r.4.0, |n| n.0),
                            volume: r.5.0,
                            quote_volume: r.5.0 * r.4.0,
                            percentage: percentage(r.4.0, r.1.0),
                            open_interest: r.7.0,
                            funding_rate: r.10.0,
                            ..Ticker::default()
                        });
                    }
                    cx.store.update_ticker(&r.0.to_ascii_uppercase(), |t| {
                        t.last = r.4.0;
                        t.volume = r.5.0;
                        t.quote_volume = r.5.0 * r.4.0;
                        t.percentage = percentage(r.4.0, r.1.0);
                        t.open_interest = r.7.0;
                        t.index = r.8.0;
                        t.mark = r.9.0;
                        t.funding_rate = r.10.0;
                        if let Some(b) = r.12 {
                            t.bid = b.0
                        }
                        if let Some(a) = r.13 {
                            t.ask = a.0
                        }
                    });
                }
                if !added.is_empty() {
                    let mut rows = cx.store.tickers().to_vec();
                    rows.extend(added);
                    cx.store.set_tickers(rows);
                }
            }
        } else if e.orderbook_p.is_some() {
            if let Ok(r) = serde_json::from_str::<DepthEvent<'_>>(text) {
                if r.kind == "snapshot"
                    && !e.orderbook_p.is_some_and(|raw| snapshot_sides(raw.get()))
                {
                    self.book_failed(cx, &e.symbol.to_ascii_uppercase());
                    return;
                }
                self.depth(cx, r)
            } else {
                self.book_failed(cx, &e.symbol.to_ascii_uppercase());
            }
        } else if e.kline_p.is_some() {
            #[derive(serde::Deserialize)]
            struct K {
                kline_p: Vec<CandleRow>,
            }
            if let Ok(k) = serde_json::from_str::<K>(text) {
                for r in k.kline_p.into_iter().rev() {
                    for ((s, i), _) in self.candles.iter().filter(|((s, i), _)| {
                        s == &e.symbol.to_ascii_uppercase()
                            && resolution(i).is_some_and(|n| n as f64 == r.1.0)
                    }) {
                        cx.candle(s, i, r.candle());
                    }
                }
            }
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        if self.account_close(cx, socket, code) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        self.public.supervisor.retry(cx);
        for b in self.books.values_mut() {
            self.generation += 1;
            b.generation = self.generation;
            b.last = None;
            b.fetching = false;
            b.buffer.clear();
            if let Some(t) = b.deadline.take() {
                cx.clear_timer(t);
            }
            if let Some(t) = b.retry.take() {
                cx.clear_timer(t);
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        match p {
            Pending::Markets(rows, invalid) => {
                if let Ok(r) = serde_json::from_str::<Product<'_>>(item) {
                    if r.supported() && !r.valid() {
                        *invalid = true;
                        return;
                    }
                    let native = r.symbol.to_string();
                    if let Some(m) = r.market() {
                        self.native_symbols.insert(m.symbol.clone(), native);
                        rows.push(m)
                    }
                } else {
                    *invalid = true;
                }
            }
            Pending::Tickers(rows, invalid) => {
                if let Ok(r) = serde_json::from_str::<Daily<'_>>(item) {
                    if let Some(m) = cx.store.market(&r.symbol.to_ascii_uppercase()) {
                        rows.push(r.ticker(m.id.clone()))
                    }
                } else {
                    *invalid = true;
                }
            }
            Pending::Candles { rows, invalid, .. } => {
                if let Ok(r) = serde_json::from_str::<CandleRow>(item) {
                    rows.push(r.candle())
                } else {
                    *invalid = true;
                }
            }
            Pending::Account(r) => self.account_item(cx, r, item),
            Pending::History(r) => self.history_item(cx, r, item),
            Pending::Income(r) => self.income_item(cx, r, item),
            _ => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        if r.status == 429 {
            self.paused_until = self.paused_until.max(
                cx.now
                    + r.header("x-ratelimit-retry-after-contract")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(60.)
                        .max(1.)
                        * 1000.,
            );
        }
        if r.status >= 400 {
            self.fail(cx, p, &format!("HTTP {}: {}", r.status, r.body));
            return;
        }
        if let Ok(e) = serde_json::from_str::<Envelope<'_>>(r.body)
            && (e.error.is_some() || e.code.is_some_and(|n| n != 0))
        {
            match p {
                Pending::Trade(p) if e.error.is_none() => self.trade_response(cx, p, r),
                p => self.fail(cx, p, &format!("Phemex {} {}", e.code.unwrap_or(-1), e.msg)),
            }
            return;
        }
        match p {
            bad @ (Pending::Markets(_, true)
            | Pending::Tickers(_, true)
            | Pending::Candles { invalid: true, .. }) => {
                self.fail(cx, bad, "malformed public response row")
            }
            Pending::Markets(rows, false) => {
                cx.store.set_markets(rows);
                self.request(
                    cx,
                    Method::Get,
                    "/md/v3/ticker/24hr/all",
                    "",
                    None,
                    Pending::Tickers(vec![], false),
                    Some(ArrayAt::Field("result")),
                );
            }
            Pending::Tickers(rows, false) => {
                cx.store.set_tickers(rows);
                self.connect_public(cx);
                self.start_account(cx);
            }
            Pending::Depth { symbol, generation } => {
                let Some(b) = self.books.get_mut(&symbol) else {
                    return;
                };
                if b.generation != generation {
                    return;
                }
                let Ok(row) = serde_json::from_str::<Snapshot>(r.body) else {
                    self.book_failed(cx, &symbol);
                    return;
                };
                b.book.clear();
                apply(&mut b.book, row.result.orderbook_p.into());
                let mut last = row.result.sequence;
                for (seq, snapshot, d) in std::mem::take(&mut b.buffer) {
                    if seq > last {
                        if snapshot {
                            b.book.clear();
                        }
                        apply(&mut b.book, d);
                        last = seq
                    }
                }
                b.last = Some(last);
                b.fetching = false;
                if let Some(t) = b.deadline.take() {
                    cx.clear_timer(t)
                }
                cx.book_changed(&symbol);
            }
            Pending::Candles {
                command, mut rows, ..
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(command, Ok(serde_json::to_string(&rows).unwrap()));
            }
            Pending::Account(p) => self.account_response(cx, p, r),
            Pending::Trade(p) => self.trade_response(cx, p, r),
            Pending::History(p) => self.history_response(cx, p, r),
            Pending::Income(p) => self.income_response(cx, p, r),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.fail(cx, p, error)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.http_timer(cx, timer)
            || self.account_timer(cx, timer)
            || self.history_timer(cx, timer)
            || self.income_timer(cx, timer)
        {
            return;
        }
        if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            self.start(cx)
        } else if self.public.supervisor.owns_timer(timer) {
            match self.public.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect_public(cx),
                SocketTick::TimedOut(_) => {
                    if let Some(socket) = self.public.socket {
                        cx.close(socket);
                        self.on_close(cx, socket, 1006);
                    }
                }
                SocketTick::Ping => self.send(cx, "server.ping", "[]".into()),
                _ => {}
            }
        } else {
            let timed = self
                .books
                .iter()
                .find_map(|(s, b)| (b.deadline == Some(timer)).then(|| s.clone()));
            if let Some(s) = timed {
                self.book_failed(cx, &s);
                return;
            }
            let symbol = self.books.iter_mut().find_map(|(s, b)| {
                (b.retry == Some(timer)).then(|| {
                    b.retry = None;
                    s.clone()
                })
            });
            if let Some(s) = symbol {
                self.snapshot(cx, &s)
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Some(command) = self.trade_command(cx, command) else {
            return;
        };
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::Query { command, query } => self.history_query(cx, command, query),
            Command::SubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users += 1;
                    return;
                }
                if self.books.len() + self.candles.len() >= 19 {
                    cx.log(LogLevel::Warning, "Phemex subscription capacity reached");
                    return;
                }
                self.generation += 1;
                self.books
                    .insert(symbol.clone(), BookState::new(self.generation));
                self.subscribe_book(cx, &symbol, true);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users -= 1;
                    if b.users > 0 {
                        return;
                    }
                }
                if let Some(b) = self.books.remove(&symbol) {
                    if let Some(t) = b.retry {
                        cx.clear_timer(t);
                    }
                    if let Some(t) = b.deadline {
                        cx.clear_timer(t);
                    }
                }
                self.subscribe_book(cx, &symbol, false);
            }
            Command::SubscribeCandles { symbol, interval } => {
                let Some(n) = resolution(&interval) else {
                    return;
                };
                let key = (symbol.clone(), interval);
                if let Some(users) = self.candles.get_mut(&key) {
                    *users += 1;
                    return;
                }
                if self.books.len() + self.candles.len() >= 19 {
                    return;
                }
                self.candles.insert(key, 1);
                self.send(
                    cx,
                    "kline_p.subscribe",
                    format!(
                        "[{},{}]",
                        serde_json::to_string(self.native_symbol(&symbol)).unwrap(),
                        n
                    ),
                );
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if let Some(n) = self.candles.get_mut(&key) {
                    *n -= 1;
                    if *n > 0 {
                        return;
                    }
                }
                self.candles.remove(&key);
                if let Some(n) = resolution(&interval) {
                    self.send(
                        cx,
                        "kline_p.unsubscribe",
                        format!(
                            "[{},{}]",
                            serde_json::to_string(self.native_symbol(&symbol)).unwrap(),
                            n
                        ),
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
                if cx.now < self.paused_until {
                    cx.done(
                        command,
                        Err("Phemex requests are rate limited; candle read was not sent".into()),
                    );
                    return;
                }
                let Some(n) = resolution(&interval) else {
                    cx.done(command, Err("unsupported Phemex candle interval".into()));
                    return;
                };
                let to = to_ms.unwrap_or(cx.now);
                let requested_limit = limit.filter(|n| *n > 0);
                let default_from =
                    to - n as f64 * 1000. * requested_limit.unwrap_or(500).min(1000) as f64;
                // Match the TS boundary: a positive limit selects the recent
                // window; zero behaves like an absent limit.
                let from = if requested_limit.is_some() {
                    default_from
                } else {
                    from_ms.unwrap_or(default_from)
                };
                self.request(
                    cx,
                    Method::Get,
                    "/exchange/public/md/v2/kline/list",
                    &format!(
                        "symbol={}&resolution={n}&from={:.0}&to={:.0}",
                        url_encode(self.native_symbol(&symbol)),
                        (from / 1000.).floor(),
                        (to / 1000.).ceil()
                    ),
                    None,
                    Pending::Candles {
                        command,
                        rows: vec![],
                        invalid: false,
                    },
                    Some(ArrayAt::Path(&["data", "rows"])),
                );
            }
            _ => {}
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.last.is_some())
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
