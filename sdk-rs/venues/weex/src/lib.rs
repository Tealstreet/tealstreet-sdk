//! WEEX Contract V3 USDT perpetual futures on the sans-I/O engine.
mod account;
mod account_wire;
mod history;
mod paths;
mod rate;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{Candle, Market, Ticker};
use safe_cex_engine::{
    Command, CommandId, Cx, LogLevel, Method, Response, SocketId, TimerId, Venue,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use wire::*;

#[derive(Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    /// Only a host able to attach authenticated upgrade headers may enable it.
    pub private_ws_headers: bool,
}

pub struct Pending {
    task: Task,
    invalid: bool,
    seed_generation: u32,
}
enum Task {
    Markets(Vec<Market>),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    Daily,
    BookTickers,
    Depth {
        symbol: String,
        generation: u32,
    },
    Candles {
        command: CommandId,
        rows: Vec<Candle>,
    },
}
struct DeferredGet {
    timer: TimerId,
    seed_generation: u32,
    path: String,
    query: String,
    task: Task,
    stream: Option<ArrayAt>,
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    attempts: u32,
    next_id: u32,
    operations: VecDeque<f64>,
    topics: HashSet<String>,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            last_received: 0.0,
            watchdog: None,
            reconnect: None,
            attempts: 0,
            next_id: 1,
            operations: VecDeque::new(),
            topics: HashSet::new(),
        }
    }
}
#[derive(Default)]
struct Seed {
    rows: Vec<Ticker>,
    reported: Vec<u8>,
    finished: u8,
    failed: bool,
}
struct Delta {
    first: u64,
    last: u64,
    bids: Vec<Level>,
    asks: Vec<Level>,
}
struct BookState {
    book: Book,
    users: u32,
    last: Option<u64>,
    generation: u32,
    fetching: bool,
    buffer: Vec<Delta>,
    retry: Option<TimerId>,
    failures: u32,
}
impl BookState {
    fn new() -> Self {
        Self {
            book: Book::default(),
            users: 1,
            last: None,
            generation: 0,
            fetching: false,
            buffer: Vec::new(),
            retry: None,
            failures: 0,
        }
    }
}
pub struct WeexFutures {
    public: Connection,
    active_symbols: HashSet<String>,
    deferred_gets: Vec<DeferredGet>,
    account: account::State,
    trading: trading::State,
    history: history::State,
    credentials: Option<sign::Credentials>,
    read_credentials: Option<safe_cex_engine::ReadCredentials>,
    private_ws_headers: bool,
    books: HashMap<String, BookState>,
    candles: BTreeMap<(String, String), u32>,
    seed: Seed,
    paused_until: f64,
    next_generation: u32,
    startup_retry: Option<TimerId>,
    seed_generation: u32,
}
impl WeexFutures {
    pub fn new(options: Options) -> Self {
        let read_credentials = options.read_credentials.map(|mut read| {
            if read.passphrase.is_none() {
                read.passphrase = options.passphrase.clone();
            }
            read
        });
        let credentials = match (options.key, options.secret, options.passphrase) {
            (Some(key), Some(secret), Some(passphrase))
                if !options.public_data_only
                    && !key.is_empty()
                    && !secret.is_empty()
                    && !passphrase.is_empty() =>
            {
                Some(sign::Credentials {
                    key,
                    secret,
                    passphrase,
                })
            }
            _ => None,
        };
        Self {
            public: Connection::default(),
            active_symbols: HashSet::new(),
            deferred_gets: Vec::new(),
            account: account::State::default(),
            trading: trading::State::default(),
            history: history::State::default(),
            credentials,
            read_credentials,
            private_ws_headers: options.private_ws_headers,
            books: HashMap::new(),
            candles: BTreeMap::new(),
            seed: Seed::default(),
            paused_until: 0.0,
            next_generation: 0,
            startup_retry: None,
            seed_generation: 0,
        }
    }
    fn get(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: &str,
        task: Task,
        stream: Option<ArrayAt>,
    ) {
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.deferred_gets.push(DeferredGet {
                timer,
                seed_generation: self.seed_generation,
                path: path.into(),
                query: query.into(),
                task,
                stream,
            });
            return;
        }
        let url = format!(
            "{}{path}{}",
            paths::REST,
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            }
        );
        let pending = Pending {
            task,
            invalid: false,
            seed_generation: self.seed_generation,
        };
        if let Some(at) = stream {
            cx.request_streamed(Method::Get, url, vec![], at, pending);
        } else {
            cx.request(Method::Get, url, vec![], None, pending);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn signed_request(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        query: &str,
        body: Option<String>,
        task: Task,
        stream: Option<ArrayAt>,
    ) -> bool {
        let read_credentials = self
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get)
            .map(|read| sign::Credentials {
                key: read.key.clone(),
                secret: read.secret.clone(),
                passphrase: read
                    .passphrase
                    .clone()
                    .or_else(|| self.credentials.as_ref().map(|c| c.passphrase.clone()))
                    .unwrap_or_default(),
            });
        let Some(credentials) = read_credentials.as_ref().or(self.credentials.as_ref()) else {
            return false;
        };
        let mut pairs: Vec<_> = query.split('&').filter(|p| !p.is_empty()).collect();
        pairs.sort_unstable_by_key(|p| p.split('=').next().unwrap_or(""));
        let path_query = if pairs.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{}", pairs.join("&"))
        };
        let headers = credentials.headers(
            method.as_str(),
            &path_query,
            body.as_deref().unwrap_or(""),
            cx.now,
        );
        let url = format!("{}{path_query}", paths::REST);
        let pending = Pending {
            task,
            invalid: false,
            seed_generation: self.seed_generation,
        };
        if let Some(at) = stream {
            cx.request_streamed_body(method, url, headers, body, at, pending);
        } else {
            cx.request(method, url, headers, body, pending);
        }
        true
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket =
            Some(cx.connect_channel(paths::PUBLIC_WS, safe_cex_engine::SocketChannel::Public));
        self.public.last_received = cx.now;
        self.public.watchdog = Some(cx.set_timer(5_000.0));
    }
    fn subscription(&mut self, cx: &mut Cx<'_, Pending>, method: &str, topics: &[String]) {
        if !self.public.open {
            return;
        }
        while self
            .public
            .operations
            .front()
            .is_some_and(|t| cx.now - t >= 3_600_000.0)
        {
            self.public.operations.pop_front();
        }
        for lot in topics.chunks(50) {
            if self.public.operations.len() >= 240 {
                cx.log(
                    LogLevel::Warning,
                    "WEEX subscription operation limit reached",
                );
                return;
            }
            self.public.operations.push_back(cx.now);
            self.public.next_id += 1;
            let params = serde_json::to_string(lot).expect("topic strings");
            cx.send(
                self.public.socket.unwrap(),
                format!(
                    r#"{{"method":"{method}","params":{params},"id":{}}}"#,
                    self.public.next_id - 1
                ),
            );
            for topic in lot {
                if method == "SUBSCRIBE" {
                    self.public.topics.insert(topic.clone());
                } else {
                    self.public.topics.remove(topic);
                }
            }
        }
    }
    fn reconcile_topics(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut desired: Vec<String> = self
            .books
            .iter()
            .filter(|(_, b)| b.last.is_some() && b.retry.is_none())
            .map(|(s, _)| format!("{s}@depth200"))
            .collect();
        desired.extend(
            self.candles
                .keys()
                .map(|(s, i)| format!("{s}@kline_{i}_LAST_PRICE")),
        );
        desired.sort();
        let capacity = 90.min(100usize.saturating_sub(desired.len()));
        desired.extend(
            cx.store
                .markets()
                .iter()
                .filter(|m| {
                    self.active_symbols.is_empty() || self.active_symbols.contains(&m.symbol)
                })
                .take(capacity)
                .map(|m| format!("{}@ticker", m.symbol)),
        );
        let remove: Vec<_> = self
            .public
            .topics
            .iter()
            .filter(|s| !desired.contains(s))
            .cloned()
            .collect();
        let add: Vec<_> = desired
            .into_iter()
            .filter(|s| !self.public.topics.contains(s))
            .collect();
        self.subscription(cx, "UNSUBSCRIBE", &remove);
        self.subscription(cx, "SUBSCRIBE", &add);
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.fetching {
            return;
        }
        if cx.now < self.paused_until {
            state.retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        state.fetching = true;
        self.next_generation += 1;
        state.generation = self.next_generation;
        let generation = state.generation;
        self.get(
            cx,
            paths::DEPTH,
            &format!("limit=200&symbol={symbol}"),
            Task::Depth {
                symbol: symbol.into(),
                generation,
            },
            None,
        );
    }
    fn book_failed(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(b) = self.books.get_mut(symbol) {
            self.next_generation += 1;
            b.generation = self.next_generation;
            if let Some(t) = b.retry.take() {
                cx.clear_timer(t);
            }
            b.fetching = false;
            b.last = None;
            b.buffer.clear();
            b.failures += 1;
            let delay = (1000.0 * 2f64.powi(b.failures.min(6) as i32 - 1))
                .min(60_000.0)
                .max(self.paused_until - cx.now);
            b.retry = Some(cx.set_timer(delay));
        }
    }
    fn depth(&mut self, cx: &mut Cx<'_, Pending>, row: DepthEvent<'_>) {
        let symbol = row.s;
        let Some(b) = self.books.get_mut(symbol.as_ref()) else {
            return;
        };
        if b.retry.is_some() {
            return;
        }
        let delta = Delta {
            first: row.first,
            last: row.last,
            bids: row.b,
            asks: row.a,
        };
        if b.fetching || b.last.is_none() {
            if b.buffer.len() >= 2_000 {
                self.book_failed(cx, &symbol);
            } else {
                b.buffer.push(delta);
            }
            return;
        }
        if !apply(b, &delta) {
            b.last = None;
            b.buffer.push(delta);
            self.snapshot(cx, &symbol);
            return;
        }
        cx.book_changed(&symbol);
    }
    fn fail(&mut self, cx: &mut Cx<'_, Pending>, task: Task, error: &str) {
        cx.log(LogLevel::Warning, format!("WEEX: {error}"));
        match task {
            Task::Daily | Task::BookTickers => {
                self.seed.failed = true;
                if self.startup_retry.is_none() {
                    self.startup_retry =
                        Some(cx.set_timer((self.paused_until - cx.now).max(2000.0)));
                }
            }
            Task::Depth { symbol, generation } => {
                if self
                    .books
                    .get(&symbol)
                    .is_some_and(|b| b.generation == generation)
                {
                    self.book_failed(cx, &symbol);
                }
            }
            Task::Candles { command, .. } => cx.done(command, Err(error.into())),
            Task::Account(r) => self.account_failed(cx, r, error),
            Task::Trade(r) => self.trade_failed(cx, r, error),
            Task::History(r) => self.history_failed(cx, r, error),
            Task::Markets(_) => {
                if self.startup_retry.is_none() {
                    self.startup_retry =
                        Some(cx.set_timer((self.paused_until - cx.now).max(2000.0)));
                }
            }
        }
    }
}
fn apply(b: &mut BookState, delta: &Delta) -> bool {
    let Some(last) = b.last else {
        return false;
    };
    if delta.last <= last {
        return true;
    }
    if delta.first > last.saturating_add(1) || delta.first > delta.last {
        return false;
    }
    for (side, levels) in [(BookSide::Bids, &delta.bids), (BookSide::Asks, &delta.asks)] {
        for Level(price, amount) in levels {
            if *price > 0.0 && *amount >= 0.0 {
                b.book.apply(side, *price, *amount);
            }
        }
    }
    b.last = Some(delta.last);
    true
}
fn interval(i: &str, stream: bool) -> bool {
    matches!(
        i,
        "1m" | "5m" | "15m" | "30m" | "1h" | "4h" | "12h" | "1d" | "1w"
    ) || (stream && matches!(i, "2h" | "6h"))
}
impl Venue for WeexFutures {
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.read_credentials
            && let Err(error) = read.validate().and_then(|_| {
                read.validate_passphrase(self.credentials.as_ref().map(|c| c.passphrase.as_str()))
            })
        {
            cx.log(LogLevel::Error, error);
            return;
        }
        self.get(
            cx,
            paths::INFO,
            "",
            Task::Markets(Vec::new()),
            Some(ArrayAt::Field("symbols")),
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
        self.public.attempts = 0;
        self.public.last_received = cx.now;
        self.public.operations.clear();
        self.public.topics.clear();
        self.reconcile_topics(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.last_received = cx.now;
        let Ok(message) = serde_json::from_str::<Envelope<'_>>(text) else {
            return;
        };
        if message.event == "ping" {
            let id = self.public.next_id;
            self.public.next_id += 1;
            cx.send(socket, format!(r#"{{"method":"PONG","id":{id}}}"#));
            return;
        }
        if message.result == Some(false) {
            cx.log(
                LogLevel::Warning,
                format!("WEEX subscription failed: {}", message.msg),
            );
            return;
        }
        match message.e.as_ref() {
            "ticker" => {
                if let Ok(row) = serde_json::from_str::<Rows<'_, OneOrMany<TickerUpdate>>>(text)
                    && let Some(d) = row.d.first()
                {
                    cx.store.update_ticker(&row.s, |t| {
                        if let Some(v) = d.c {
                            t.last = v;
                        }
                        if let Some(v) = d.m {
                            t.mark = v;
                        }
                        if let Some(v) = d.i {
                            t.index = v;
                        }
                        if let Some(v) = d.percentage {
                            t.percentage = percentage(v);
                        }
                        if let Some(v) = d.v {
                            t.volume = v;
                        }
                        if let Some(v) = d.q {
                            t.quote_volume = v;
                        }
                    });
                }
            }
            "depth" => match serde_json::from_str::<DepthEvent<'_>>(text) {
                Ok(row) => self.depth(cx, row),
                Err(_) => {
                    #[derive(serde::Deserialize)]
                    struct Symbol<'a> {
                        s: &'a str,
                    }
                    if let Ok(row) = serde_json::from_str::<Symbol<'_>>(text) {
                        self.book_failed(cx, row.s);
                    }
                }
            },
            "kline" => {
                if let Ok(row) = serde_json::from_str::<Rows<'_, Vec<CandleEvent<'_>>>>(text) {
                    for c in row.d {
                        if self
                            .candles
                            .contains_key(&(row.s.to_string(), c.i.to_string()))
                        {
                            cx.candle(&row.s, &c.i, c.candle());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket, _code) {
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
        self.public.attempts += 1;
        self.public.reconnect =
            Some(cx.set_timer(
                (1000.0 * 2f64.powi(self.public.attempts.min(5) as i32 - 1)).min(30_000.0),
            ));
        for b in self.books.values_mut() {
            b.last = None;
            b.fetching = false;
            self.next_generation += 1;
            b.generation = self.next_generation;
            b.buffer.clear();
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        if matches!(p.task, Task::Daily | Task::BookTickers)
            && p.seed_generation != self.seed_generation
        {
            return;
        }
        let ok = match &mut p.task {
            Task::Account(r) => self.account_item(cx, r, item),
            Task::History(r) => self.history_item(cx, r, item),
            Task::Markets(rows) => serde_json::from_str::<Symbol<'_>>(item)
                .map(|r| {
                    if let Some(m) = r.market() {
                        rows.push(m);
                    }
                })
                .is_ok(),
            Task::Daily => serde_json::from_str::<Daily<'_>>(item)
                .map(|r| {
                    if let Some(i) = cx.store.market_index(&r.symbol) {
                        let i = i as usize;
                        let old = &self.seed.rows[i];
                        let mut t = r.ticker(&r.symbol);
                        if self.seed.reported[i] & 2 != 0 {
                            t.bid = old.bid;
                            t.ask = old.ask;
                        }
                        self.seed.rows[i] = t;
                        self.seed.reported[i] |= 1;
                    }
                })
                .is_ok(),
            Task::BookTickers => serde_json::from_str::<BookTicker<'_>>(item)
                .map(|r| {
                    if let Some(i) = cx.store.market_index(&r.symbol) {
                        let i = i as usize;
                        self.seed.rows[i].bid = r.bid_price;
                        self.seed.rows[i].ask = r.ask_price;
                        self.seed.reported[i] |= 2;
                    }
                })
                .is_ok(),
            Task::Candles { rows, .. } => serde_json::from_str::<CandleRow>(item)
                .map(|r| rows.push(r.candle()))
                .is_ok(),
            _ => true,
        };
        if !ok {
            p.invalid = true;
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, response: Response<'_>) {
        if matches!(p.task, Task::Daily | Task::BookTickers)
            && p.seed_generation != self.seed_generation
        {
            return;
        }
        if response.status == 429 {
            self.paused_until = self
                .paused_until
                .max(cx.now + rate::delay(response, cx.now));
        }
        if response.status >= 400 {
            self.fail(
                cx,
                p.task,
                &format!("HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        if p.invalid {
            self.fail(cx, p.task, "malformed REST row");
            return;
        }
        match p.task {
            Task::Account(r) => self.account_response(cx, r, response),
            Task::Trade(r) => self.trade_response(cx, r, response),
            Task::History(r) => self.history_response(cx, r, response),
            Task::Markets(markets) => {
                cx.store.set_markets(markets);
                self.seed = Seed {
                    rows: cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            ..Ticker::default()
                        })
                        .collect(),
                    reported: vec![0; cx.store.markets().len()],
                    ..Seed::default()
                };
                self.get(cx, paths::DAILY, "", Task::Daily, Some(ArrayAt::Root));
                self.get(
                    cx,
                    paths::BOOK_TICKERS,
                    "",
                    Task::BookTickers,
                    Some(ArrayAt::Root),
                );
            }
            task @ (Task::Daily | Task::BookTickers) => {
                self.seed.finished |= if matches!(task, Task::Daily) { 1 } else { 2 };
                if self.seed.finished == 3 && !self.seed.failed {
                    let rows = std::mem::take(&mut self.seed.rows)
                        .into_iter()
                        .zip(&self.seed.reported)
                        .filter_map(|(t, &mask)| (mask != 0).then_some(t))
                        .collect();
                    cx.store.set_tickers(rows);
                    self.connect_public(cx);
                    self.start_account(cx);
                }
            }
            Task::Depth { symbol, generation } => {
                let Some(b) = self.books.get_mut(&symbol) else {
                    return;
                };
                if b.generation != generation {
                    return;
                }
                let Ok(row) = serde_json::from_str::<Depth>(response.body) else {
                    self.book_failed(cx, &symbol);
                    return;
                };
                b.book.clear();
                for (side, levels) in [(BookSide::Bids, row.bids), (BookSide::Asks, row.asks)] {
                    for Level(p, a) in levels {
                        if p > 0.0 && a >= 0.0 {
                            b.book.apply(side, p, a);
                        }
                    }
                }
                b.last = Some(row.last_update_id);
                b.fetching = false;
                for delta in std::mem::take(&mut b.buffer) {
                    if !apply(b, &delta) {
                        self.book_failed(cx, &symbol);
                        return;
                    }
                }
                b.failures = 0;
                cx.book_changed(&symbol);
                self.reconcile_topics(cx);
            }
            Task::Candles { command, mut rows } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                cx.done(command, Ok(serde_json::to_string(&rows).expect("candles")));
            }
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        if matches!(p.task, Task::Daily | Task::BookTickers)
            && p.seed_generation != self.seed_generation
        {
            return;
        }
        self.fail(cx, p.task, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(i) = self.deferred_gets.iter().position(|r| r.timer == timer) {
            let r = self.deferred_gets.swap_remove(i);
            if matches!(r.task, Task::Daily | Task::BookTickers)
                && r.seed_generation != self.seed_generation
            {
                return;
            }
            self.get(cx, &r.path, &r.query, r.task, r.stream);
            return;
        }
        if self.account_timer(cx, timer) || self.history_timer(cx, timer) {
            return;
        }
        if self.startup_retry == Some(timer) {
            self.startup_retry = None;
            if !cx.store.loaded().markets {
                self.start(cx);
            } else {
                self.seed_generation += 1;
                self.seed = Seed {
                    rows: cx
                        .store
                        .markets()
                        .iter()
                        .map(|m| Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            ..Ticker::default()
                        })
                        .collect(),
                    reported: vec![0; cx.store.markets().len()],
                    ..Seed::default()
                };
                self.get(cx, paths::DAILY, "", Task::Daily, Some(ArrayAt::Root));
                self.get(
                    cx,
                    paths::BOOK_TICKERS,
                    "",
                    Task::BookTickers,
                    Some(ArrayAt::Root),
                );
            }
        } else if self.public.reconnect == Some(timer) {
            self.public.reconnect = None;
            self.connect_public(cx);
            let symbols: Vec<_> = self.books.keys().cloned().collect();
            for s in symbols {
                self.snapshot(cx, &s);
            }
        } else if self.public.watchdog == Some(timer) {
            self.public.watchdog = None;
            if cx.now - self.public.last_received >= 30_000.0 {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006);
                }
            } else {
                if self.public.open {
                    let id = self.public.next_id;
                    self.public.next_id += 1;
                    cx.send(
                        self.public.socket.unwrap(),
                        format!(r#"{{"method":"PING","id":{id}}}"#),
                    );
                }
                self.public.watchdog = Some(cx.set_timer(5_000.0));
            }
        } else {
            let symbol = self.books.iter_mut().find_map(|(s, b)| {
                (b.retry == Some(timer)).then(|| {
                    b.retry = None;
                    s.clone()
                })
            });
            if let Some(s) = symbol {
                self.snapshot(cx, &s);
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Some(command) = self.trade_command(cx, command) else {
            return;
        };
        match command {
            Command::SetTickerSymbols { symbols } => {
                self.active_symbols = symbols.into_iter().collect();
                self.reconcile_topics(cx);
            }
            Command::Query { command, query } => self.history_query(cx, command, query),
            Command::SubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users += 1;
                    return;
                }
                if self.books.len() + self.candles.len() >= 10 {
                    cx.log(LogLevel::Warning, "WEEX book/candle capacity reached");
                    return;
                }
                self.books.insert(symbol.clone(), BookState::new());
                self.snapshot(cx, &symbol);
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(b) = self.books.get_mut(&symbol) {
                    b.users -= 1;
                    if b.users > 0 {
                        return;
                    }
                }
                if let Some(b) = self.books.remove(&symbol)
                    && let Some(t) = b.retry
                {
                    cx.clear_timer(t);
                }
                self.reconcile_topics(cx);
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if !interval(&i, true) {
                    cx.log(
                        LogLevel::Warning,
                        format!("WEEX does not support {i} candles"),
                    );
                    return;
                }
                let key = (symbol, i);
                if !self.candles.contains_key(&key) && self.books.len() + self.candles.len() >= 10 {
                    cx.log(LogLevel::Warning, "WEEX book/candle capacity reached");
                    return;
                }
                *self.candles.entry(key).or_default() += 1;
                self.reconcile_topics(cx);
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol, interval);
                if let Some(n) = self.candles.get_mut(&key) {
                    *n -= 1;
                    if *n == 0 {
                        self.candles.remove(&key);
                    }
                }
                self.reconcile_topics(cx);
            }
            Command::FetchCandles {
                command,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                if !interval(&i, false) {
                    cx.done(command, Err(format!("WEEX does not support {i} candles")));
                    return;
                }
                let historical = from_ms.is_some() || to_ms.is_some();
                let mut query = format!(
                    "interval={i}&limit={}&symbol={symbol}",
                    limit
                        .unwrap_or(if historical { 100 } else { 500 })
                        .clamp(1, if historical { 100 } else { 1000 })
                );
                if let Some(t) = from_ms {
                    query.push_str(&format!("&startTime={t}"));
                }
                if let Some(t) = to_ms {
                    query.push_str(&format!("&endTime={t}"));
                }
                if historical {
                    query.push_str("&priceType=LAST");
                }
                self.get(
                    cx,
                    if historical {
                        paths::HISTORY_KLINES
                    } else {
                        paths::KLINES
                    },
                    &query,
                    Task::Candles {
                        command,
                        rows: vec![],
                    },
                    Some(ArrayAt::Root),
                );
            }
            _ => cx.log(LogLevel::Warning, "WEEX command not implemented yet"),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|b| b.last.is_some() && b.retry.is_none())
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

#[cfg(test)]
mod secondary_tests;
