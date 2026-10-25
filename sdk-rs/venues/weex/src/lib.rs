//! WEEX Contract V3 USDT perpetual futures on the sans-I/O engine.
mod account;
mod account_wire;
mod depth_query;
/// Opaque canonical recipes signed by the injected dispatch provider.
#[path = "sign/dispatch.rs"]
pub mod dispatch_signing;
mod history;
mod identity;
mod managed;
mod paths;
mod rate;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::ErrorKind;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{SlotPolicy, Slots};
use safe_cex_engine::history::ReadCompletion;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Market, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Owner, Policy, RetryPolicy};
use safe_cex_engine::selected_read::{ReadRole, SelectedRead};
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{
    Command, CommandId, CommandWait, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId,
    Venue,
};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use wire::*;

const READ_CREDENTIALS_INVALID: &str =
    "WEEX read credentials are incomplete in trusted signer setup";

/// Opaque identities from trusted signer setup; the venue holds no material.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub read_credentials: Option<safe_cex_engine::signing::Identity>,
    pub public_data_only: bool,
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    /// Only a host able to attach authenticated upgrade headers may enable it.
    pub private_ws_headers: bool,
}

pub struct Pending {
    task: Task,
    invalid: bool,
    /// Symbols of malformed public rows skipped by this read.
    skipped: Vec<String>,
    seed_generation: u32,
}
enum Task {
    BookQuery(depth_query::Request),
    Markets(Vec<Market>),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    /// The engine Context owns the command until it completes.
    Identity {
        context: safe_cex_engine::request_context::Context,
    },
    Daily,
    BookTickers,
    Depth {
        symbol: String,
        generation: u32,
    },
    Candles(CandleQuery),
    Leverage {
        symbol: String,
        revision: u64,
        context: safe_cex_engine::request_context::Context,
    },
}
struct CandleQuery {
    completion: ReadCompletion,
    leg: Option<Context>,
    rows: Vec<Candle>,
}
impl CandleQuery {
    fn delivered(&self, cx: &Cx<'_, Pending>) -> bool {
        self.leg
            .as_ref()
            .is_some_and(|leg| self.completion.validate_delivery(cx, leg).is_ok())
    }
    fn retire_leg(&self, cx: &mut Cx<'_, Pending>) {
        if let Some(leg) = &self.leg {
            let _ = cx.finish_http_context(leg);
        }
    }
    fn failed(self, cx: &mut Cx<'_, Pending>, error: safe_cex_engine::VenueError) {
        self.retire_leg(cx);
        if cx.validate_http_context(self.completion.root()).is_err() {
            return;
        }
        cx.notice(
            safe_cex_engine::Level::Warn,
            code::REQUEST_FAILED,
            format!("WEEX: {}", error.message),
        );
        let _ = self.completion.finish(cx, Err(error));
    }
}
struct DeferredGet {
    timer: TimerId,
    seed_generation: u32,
    path: String,
    query: String,
    task: Task,
    stream: Option<ArrayAt>,
}
/// Both sockets keep the old watchdog shape: 30s silence, 5s PING, 1s-16s backoff.
fn socket_supervisor() -> Supervisor {
    Supervisor::new(Config {
        connect_timeout_ms: 30_000.0,
        idle_timeout_ms: 30_000.0,
        ping_interval_ms: Some(5_000.0),
        backoff_base_ms: 1_000.0,
        backoff_cap_ms: 16_000.0,
        ..Config::default()
    })
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    supervisor: Supervisor,
    next_id: u32,
    operations: VecDeque<f64>,
    topics: HashSet<String>,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            supervisor: socket_supervisor(),
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
    /// Normal/Algo order reads and Config/Metadata settings reads share a scope.
    orders_auth: safe_cex_engine::MergedScopeAuth,
    settings_auth: safe_cex_engine::MergedScopeAuth,
    trading: trading::State,
    history: history::State,
    credentials: sign::Credentials,
    /// Configured keys, built once. GET reads use the read key, even without a primary.
    slots: Slots<safe_cex_engine::signing::Identity>,
    /// Trusted caller-assigned primary stableId; never derived from key material.
    identity: Option<String>,
    /// A supplied primary identity lacks a valid key/signing/passphrase ref.
    incomplete_credentials: bool,
    /// A supplied secondary identity that trusted setup did not complete.
    invalid_read_credentials: bool,
    private_ws_headers: bool,
    books: HashMap<String, BookState>,
    candles: BTreeMap<(String, Interval), u32>,
    seed: Seed,
    paused_until: f64,
    next_generation: u32,
    discovery: safe_cex_engine::startup::Backoff<bool>,
    seed_generation: u32,
}
fn refuse_leverage_query(cx: &mut Cx<'_, Pending>, error: impl Into<safe_cex_engine::VenueError>) {
    use safe_cex_engine::CommandWait;
    use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};

    let remaining = match cx.command_wait() {
        Some(CommandWait::Remaining(remaining)) => remaining,
        Some(CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            return;
        }
    };
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: remaining.min(20_000),
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    let _ = cx.fail_context(&context, error);
}

impl WeexFutures {
    pub fn new(options: Options) -> Self {
        let complete = |identity: &safe_cex_engine::signing::Identity| {
            identity.validate().is_ok() && identity.passphrase.is_some()
        };
        let primary = options.credentials.filter(|_| !options.public_data_only);
        let incomplete_credentials = primary.as_ref().is_some_and(|p| !complete(p));
        let invalid_read_credentials = options
            .read_credentials
            .as_ref()
            .is_some_and(|read| !complete(read));
        let identity = primary.as_ref().and_then(|p| p.stable_id.clone());
        let credentials = sign::Credentials {
            primary: primary.filter(complete),
            read: options.read_credentials.filter(complete),
        };
        let slots = Slots::new(
            credentials.primary.clone(),
            credentials.read.clone(),
            SlotPolicy::default(),
        );
        Self {
            public: Connection::default(),
            active_symbols: HashSet::new(),
            deferred_gets: Vec::new(),
            account: account::State::new(),
            orders_auth: Default::default(),
            settings_auth: Default::default(),
            trading: trading::State::default(),
            history: history::State::default(),
            credentials,
            slots,
            identity,
            incomplete_credentials,
            invalid_read_credentials,
            private_ws_headers: options.private_ws_headers,
            books: HashMap::new(),
            candles: BTreeMap::new(),
            seed: Seed::default(),
            paused_until: 0.0,
            next_generation: 0,
            discovery: Default::default(),
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
            skipped: Vec::new(),
            seed_generation: self.seed_generation,
        };
        if let Some(at) = stream {
            cx.request_streamed(Method::Get, url, vec![], at, pending);
        } else {
            cx.request(Method::Get, url, vec![], None, pending);
        }
    }
    fn candle_query(
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        requested_interval: Interval,
    ) -> Option<CandleQuery> {
        match cx.command_wait() {
            Some(CommandWait::Expired) => {
                let _ = cx.reject_expired_command(Operation::Read);
                return None;
            }
            None => {
                let _ = cx.reject_unstamped_command(Operation::Read);
                return None;
            }
            Some(CommandWait::Remaining(_)) => {}
        }
        let root = cx.admit_caller_read_root(None, None).ok()?;
        if !interval(requested_interval, false) {
            let _ = cx.fail_context(&root, requested_interval.unsupported("WEEX"));
            return None;
        }
        match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => Some(CandleQuery {
                completion,
                leg: None,
                rows: Vec::new(),
            }),
            Err(error) => {
                let _ = cx.fail_context(&root, error.read_venue_error());
                None
            }
        }
    }
    fn send_candle_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        query: &str,
        mut request: CandleQuery,
    ) {
        if let Err(error) = cx.validate_http_context(request.completion.root()) {
            if error != safe_cex_engine::request_context::Error::Revoked {
                request.failed(cx, error.read_venue_error());
            }
            return;
        }
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.deferred_gets.push(DeferredGet {
                timer,
                seed_generation: self.seed_generation,
                path: path.into(),
                query: query.into(),
                task: Task::Candles(request),
                stream: Some(ArrayAt::Root),
            });
            return;
        }
        let leg = match cx.http_leg_context(
            request.completion.root(),
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(leg) => leg,
            Err(error) => {
                request.failed(cx, error.read_venue_error());
                return;
            }
        };
        request.leg = Some(leg.clone());
        let pending = Pending {
            task: Task::Candles(request),
            invalid: false,
            skipped: Vec::new(),
            seed_generation: self.seed_generation,
        };
        match cx.request_managed_with_headers(
            leg,
            Method::Get,
            format!("{}{path}?{query}", paths::REST),
            &[],
            None,
            pending,
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Root),
            Err((pending, error)) => {
                let Task::Candles(request) = pending.task else {
                    unreachable!("candle continuation")
                };
                request.failed(cx, error.read_venue_error());
            }
        }
    }
    /// A fresh symbol configuration read, as the TS adapter's leverage query does.
    fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Some(market_id) = cx.store.market(&symbol).map(|m| m.id.clone()) else {
            refuse_leverage_query(cx, format!("unknown market {symbol}"));
            return;
        };
        if cx.now < self.paused_until {
            refuse_leverage_query(cx, "WEEX account reads are rate limited");
            return;
        }
        let selected = SelectedRead::select(&self.slots, ReadRole::Read { scope: None });
        if selected.is_err() {
            refuse_leverage_query(cx, "WEEX leverage requires credentials");
            return;
        }
        let revision = self.account.settings_revision();
        managed::read(
            cx,
            selected,
            format!("/capi/v3/account/symbolConfig?symbol={market_id}"),
            self.seed_generation,
            |context| Task::Leverage {
                symbol,
                revision,
                context,
            },
        );
    }
    fn leverage_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: &safe_cex_engine::request_context::Context,
        symbol: &str,
        revision: u64,
        response: Response<'_>,
    ) {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Fetched {
            buy_leverage: f64,
            sell_leverage: f64,
            margin_type: safe_cex_engine::model::MarginType,
        }
        let market_id = cx.store.market(symbol).map(|m| m.id.clone());
        let config = serde_json::from_str::<Vec<account_wire::ConfigRow<'_>>>(response.body)
            .map_err(|e| format!("invalid WEEX symbol configuration: {e}"))
            .and_then(|rows| {
                rows.into_iter()
                    .find(|row| Some(&*row.symbol) == market_id.as_deref())
                    .ok_or_else(|| format!("WEEX symbol configuration omitted {symbol}"))
            })
            .and_then(|row| {
                let setting = row.setting();
                match (
                    setting.buy_leverage,
                    setting.sell_leverage,
                    setting.margin_type,
                ) {
                    (Some(buy), Some(sell), Some(margin_type)) if buy > 0.0 && sell > 0.0 => Ok((
                        account::Config {
                            setting,
                            hedged: row.separated_type == "SEPARATED",
                            isolated_long_leverage: row.isolated_long_leverage,
                            isolated_short_leverage: row.isolated_short_leverage,
                        },
                        Fetched {
                            buy_leverage: buy,
                            sell_leverage: sell,
                            margin_type,
                        },
                    )),
                    _ => Err("invalid WEEX symbol configuration: non-positive leverage".into()),
                }
            });
        let (config, fetched) = match config {
            Ok(config) => config,
            Err(error) => {
                managed::reject(cx, context, error);
                return;
            }
        };
        // A settings write since issue supersedes this snapshot: answer the
        // current settings instead, so the caller's store cannot regress either.
        let fetched = if revision == self.account.settings_revision() {
            cx.store
                .update_setting(symbol, |setting| *setting = config.setting);
            self.account.configs.insert(symbol.to_owned(), config);
            fetched
        } else {
            cx.store
                .setting(symbol)
                .and_then(|s| {
                    Some(Fetched {
                        buy_leverage: s.buy_leverage?,
                        sell_leverage: s.sell_leverage?,
                        margin_type: s.margin_type?,
                    })
                })
                .unwrap_or(fetched)
        };
        match serde_json::to_string(&fetched) {
            Ok(data) => managed::complete(cx, context, data),
            Err(error) => managed::reject(cx, context, error.to_string()),
        }
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(paths::PUBLIC_WS)
                    .channel(safe_cex_engine::SocketChannel::Public),
            ),
        );
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
    }
    fn public_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Unhandled => return false,
            Tick::Reconnect => {
                self.connect_public(cx);
                let symbols: Vec<_> = self.books.keys().cloned().collect();
                for s in symbols {
                    self.snapshot(cx, &s);
                }
            }
            Tick::Ping => {
                if let Some(socket) = self.public.socket {
                    let id = self.public.next_id;
                    self.public.next_id += 1;
                    cx.send(socket, format!(r#"{{"method":"PING","id":{id}}}"#));
                }
            }
            Tick::TimedOut(_) => {
                if let Some(s) = self.public.socket {
                    cx.close(s);
                    self.on_close(cx, s, 1006);
                }
            }
            Tick::Handled => (),
        }
        true
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
                cx.notice(
                    safe_cex_engine::Level::Warn,
                    code::SUBSCRIPTION_REJECTED,
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
            let delay = cx
                .jittered(Backoff::new(1_000.0, 5, 60_000.0).delay(b.failures.saturating_sub(1)))
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
        if let Task::Identity { context, .. } = &task {
            Self::identity_failed(cx, context, error);
            return;
        }
        cx.notice(
            safe_cex_engine::Level::Warn,
            code::REQUEST_FAILED,
            format!("WEEX: {error}"),
        );
        match task {
            Task::Identity { .. } | Task::BookQuery(_) | Task::Candles(_) => unreachable!(),
            Task::Daily | Task::BookTickers => {
                self.seed.failed = true;
                self.discovery.failed(cx, true);
                // Markets are loaded; account state does not depend on tickers.
                self.start_account(cx);
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
            Task::Leverage { context, .. } => managed::reject(cx, &context, error),
            Task::Account(r) => self.account_failed(cx, r, error),
            Task::Trade(r) => {
                self.trade_failed(cx, r, trading::unknown(error).kind(ErrorKind::Network))
            }
            Task::History(r) => self.history_failed(cx, r, error),
            Task::Markets(_) => {
                self.discovery.failed(cx, false);
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
fn row_symbol(item: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Row<'a> {
        #[serde(borrow)]
        symbol: Str<'a>,
    }
    serde_json::from_str::<Row<'_>>(item)
        .map_or_else(|_| "unknown".into(), |r| r.symbol.into_owned())
}
// WEEX kline codes are the TS spellings; 2h and 6h are stream-only.
fn interval(i: Interval, stream: bool) -> bool {
    use Interval::*;
    matches!(i, M1 | M5 | M15 | M30 | H1 | H4 | H12 | D1 | W1) || (stream && matches!(i, H2 | H6))
}
impl Venue for WeexFutures {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            order_book_query: true,
            leverage_query: safe_cex_engine::LeverageQuery::OnReadAndMetadata,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn account_auth_outcome(
        &self,
        pending: &Self::Pending,
        response: &Response<'_>,
        _now: f64,
        _store: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        self.account_auth(pending, response)
    }

    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.invalid_read_credentials {
            cx.notice(
                safe_cex_engine::Level::Error,
                code::CONFIG_INVALID,
                READ_CREDENTIALS_INVALID,
            );
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
        self.public.supervisor.opened(cx);
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
        self.public.supervisor.received(cx.now);
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
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::SUBSCRIPTION_REJECTED,
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
                        if let Some(i) = Interval::parse(&c.i)
                            && self.candles.contains_key(&(row.s.to_string(), i))
                        {
                            cx.candle(&row.s, i, c.candle());
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
        self.public.supervisor.retry(cx);
        for b in self.books.values_mut() {
            b.last = None;
            b.fetching = false;
            self.next_generation += 1;
            b.generation = self.next_generation;
            b.buffer.clear();
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        if let Task::Candles(request) = &p.task
            && !request.delivered(cx)
        {
            return;
        }
        if matches!(p.task, Task::Daily | Task::BookTickers)
            && p.seed_generation != self.seed_generation
        {
            return;
        }
        if matches!(p.task, Task::Account(_)) && p.seed_generation != self.seed_generation {
            return;
        }
        let ok = match &mut p.task {
            Task::Account(r) => self.account_item(cx, r, item),
            Task::History(r) => self.history_item(cx, r, item),
            Task::Markets(rows) => {
                match serde_json::from_str::<Symbol<'_>>(item).map(Symbol::market) {
                    Ok(Some(market)) if market.precision.is_tradable() => rows.push(market),
                    Ok(None) => {}
                    _ => {
                        p.skipped.push(row_symbol(item));
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
                true
            }
            Task::Daily => {
                let row = serde_json::from_str::<Daily<'_>>(item).ok();
                match row.as_ref().and_then(|r| Some((r, r.ticker(&r.symbol)?))) {
                    Some((r, mut t)) => {
                        if let Some(i) = cx.store.market_index(&r.symbol) {
                            let i = i as usize;
                            let old = &self.seed.rows[i];
                            if self.seed.reported[i] & 4 != 0 {
                                t.bid = old.bid;
                            }
                            if self.seed.reported[i] & 8 != 0 {
                                t.ask = old.ask;
                            }
                            self.seed.rows[i] = t;
                            self.seed.reported[i] |= 1;
                        }
                    }
                    None => {
                        p.skipped.push(row_symbol(item));
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
                true
            }
            Task::BookTickers => {
                match serde_json::from_str::<BookTicker<'_>>(item) {
                    Ok(r) => {
                        if let Some(i) = cx.store.market_index(&r.symbol) {
                            let i = i as usize;
                            if let Some(bid) = r.bid_price {
                                self.seed.rows[i].bid = bid;
                                self.seed.reported[i] |= 4;
                            } else if self.seed.reported[i] & 1 != 0 {
                                self.seed.rows[i].bid = self.seed.rows[i].last;
                            }
                            if let Some(ask) = r.ask_price {
                                self.seed.rows[i].ask = ask;
                                self.seed.reported[i] |= 8;
                            } else if self.seed.reported[i] & 1 != 0 {
                                self.seed.rows[i].ask = self.seed.rows[i].last;
                            }
                            self.seed.reported[i] |= 2;
                        }
                    }
                    Err(_) => {
                        p.skipped.push(row_symbol(item));
                        safe_cex_engine::startup::skipped_row(cx);
                    }
                }
                true
            }
            Task::Candles(request) => serde_json::from_str::<CandleRow>(item)
                .map(|r| request.rows.push(r.candle()))
                .is_ok(),
            _ => true,
        };
        if !ok {
            p.invalid = true;
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, response: Response<'_>) {
        if let Task::Candles(mut request) = p.task {
            if !request.delivered(cx) {
                request.retire_leg(cx);
                return;
            }
            if response.status == 429 {
                self.paused_until = self
                    .paused_until
                    .max(cx.now + rate::delay(response, cx.now));
            }
            if response.status >= 400 {
                request.failed(
                    cx,
                    format!("HTTP {}: {}", response.status, response.body).into(),
                );
            } else if p.invalid {
                request.failed(cx, "malformed REST row".into());
            } else {
                request
                    .rows
                    .sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let result = serde_json::to_string(&request.rows).expect("candles");
                let _ = request.completion.finish(cx, Ok(result));
                if let Some(leg) = request.leg {
                    let _ = cx.finish_http_context(&leg);
                }
            }
            return;
        }
        if let Task::Account(request) = p.task {
            let context = request.context.clone();
            // A native rate-limit signal still pauses this immutable identity's
            // other reads even when this snapshot's seed/cycle is stale.
            if response.status == 429 {
                self.paused_until = self
                    .paused_until
                    .max(cx.now + rate::delay(response, cx.now));
            }
            if p.seed_generation == self.seed_generation
                && self.account_delivery_current(cx, &request)
            {
                if response.status >= 400 {
                    if let Some(context) = &context {
                        let _ = cx.finish_http_context(context);
                    }
                    self.account_failed(
                        cx,
                        request,
                        &format!("HTTP {}: {}", response.status, response.body),
                    );
                } else if p.invalid {
                    if let Some(context) = &context {
                        let _ = cx.finish_http_context(context);
                    }
                    self.account_failed(cx, request, "malformed REST row");
                } else {
                    self.account_response(cx, request, response);
                }
            }
            if let Some(context) = context {
                let _ = cx.finish_http_context(&context);
            }
            return;
        }

        if matches!(p.task, Task::Daily | Task::BookTickers)
            && p.seed_generation != self.seed_generation
        {
            return;
        }
        if let Task::Trade(request) = p.task {
            self.trade_response(cx, request, response);
            return;
        }
        if let Task::BookQuery(request) = p.task {
            self.book_query_response(cx, request, response);
            return;
        }
        if response.status == 429 {
            self.paused_until = self
                .paused_until
                .max(cx.now + rate::delay(response, cx.now));
        }
        if let Task::History(request) = &p.task
            && !self.history_delivery_current(cx, request)
        {
            Self::finish_history_page(cx, request);
            return;
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
        if !p.skipped.is_empty() {
            let kind = match p.task {
                Task::Markets(_) => "market",
                Task::Daily => "24h ticker",
                _ => "book ticker",
            };
            cx.notice(
                safe_cex_engine::Level::Warn,
                code::ROW_SKIPPED,
                format!(
                    "WEEX skipped {} malformed {kind} row(s): {}",
                    p.skipped.len(),
                    p.skipped.join(", ")
                ),
            );
        }
        match p.task {
            Task::BookQuery(_) | Task::Candles(_) => unreachable!(),
            Task::Leverage {
                symbol,
                revision,
                context,
                ..
            } => self.leverage_response(cx, &context, &symbol, revision, response),
            Task::Identity { context, .. } => self.identity_response(cx, &context, response),
            Task::Account(_) => unreachable!("account response handled before reducer"),
            Task::Trade(r) => self.trade_response(cx, r, response),
            Task::History(r) => self.history_response(cx, r, response),
            Task::Markets(markets) => {
                if markets.is_empty() {
                    let error = if p.skipped.is_empty() {
                        "empty market catalogue"
                    } else {
                        "no well-formed market rows"
                    };
                    self.fail(cx, Task::Markets(markets), error);
                    return;
                }
                self.discovery.succeeded(cx, &false);
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
                    if !self.seed.reported.iter().any(|mask| mask & 1 != 0) {
                        self.discovery.failed(cx, true);
                        self.start_account(cx);
                        return;
                    }
                    let rows = std::mem::take(&mut self.seed.rows)
                        .into_iter()
                        .zip(&self.seed.reported)
                        // Book-only rows lack last/mark/index and cannot seed a ticker.
                        .filter_map(|(t, &mask)| (mask & 1 != 0).then_some(t))
                        .collect();
                    cx.store.set_tickers(rows);
                    self.discovery.succeeded(cx, &true);
                    self.connect_public(cx);
                    // After the ticker seed so account rows map against seeded prices.
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
        }
    }
    fn on_stream_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, response: Response<'_>) {
        let message = format!(
            "response did not contain the expected list: {}",
            response.body
        );
        if let Task::Candles(request) = p.task {
            if request.delivered(cx) {
                request.failed(
                    cx,
                    safe_cex_engine::VenueError::new(message).kind(ErrorKind::Internal),
                );
            } else {
                request.retire_leg(cx);
            }
        } else {
            self.on_request_failed(cx, p, &message);
        }
    }
    /// Provider refusal is typed, never sent and never venue auth evidence.
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        p: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match p.task {
            Task::Candles(request) => request.failed(cx, error),
            Task::BookQuery(request) => request.failed(cx, error),
            Task::Account(r) => {
                self.account_request_failed(cx, r, p.seed_generation, &error.message)
            }
            Task::Identity { context, .. } => Self::identity_failed(cx, &context, error),
            Task::Leverage { context, .. } => managed::reject(cx, &context, error),
            Task::Trade(r) => self.trade_failed(cx, r, error),
            Task::History(r) => self.history_refused(cx, r, error),
            task => self.on_request_failed(cx, Pending { task, ..p }, &error.message),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        p: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match p.task {
            Task::Candles(request) => request.failed(cx, error),
            Task::BookQuery(request) => request.failed(cx, error),
            Task::Trade(request) => self.trade_failed(cx, request, error),
            task => self.on_request_failed(cx, Pending { task, ..p }, &error.message),
        }
    }
    fn read_root_retired(&mut self, owner: Owner, _pending: Vec<Pending>) {
        self.retire_history(owner);
        self.deferred_gets.retain(|deferred| match &deferred.task {
            Task::BookQuery(query) => query.owner() != owner,
            Task::Candles(candles) => candles.completion.root().owner() != owner,
            _ => true,
        });
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.retire_trade(owner);
    }
    fn unproven_http_evidence(
        &self,
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        if let Task::Trade(request) = &pending.task {
            request.unproven_evidence(response.body, evidence);
        }
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match &pending.task {
            Task::Trade(request) => request.reconciliation(),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        if let Task::Account(request) = p.task {
            self.account_request_failed(cx, request, p.seed_generation, error);
            return;
        }

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
            match r.task {
                Task::Candles(request) => {
                    self.send_candle_query(cx, &r.path, &r.query, request);
                }
                Task::BookQuery(request) => self.send_book_query(cx, request, r.query),
                task => self.get(cx, &r.path, &r.query, task, r.stream),
            }
            return;
        }
        if self.account_timer(cx, timer) || self.history_timer(cx, timer) {
            return;
        }
        if let Some(tickers) = self.discovery.on_timer(timer) {
            if !tickers {
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
        } else if !self.public_timer(cx, timer) {
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
        if let Command::FetchOrderBook {
            command,
            ref symbol,
            limit,
        } = command
        {
            self.fetch_book_query(cx, command, symbol, limit);
            return;
        }

        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
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
                    cx.notice(
                        safe_cex_engine::Level::Warn,
                        code::SUBSCRIPTION_REJECTED,
                        "WEEX book/candle capacity reached",
                    );
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
                if !interval(i, true) {
                    cx.notice(
                        safe_cex_engine::Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        i.unsupported("WEEX"),
                    );
                    return;
                }
                let key = (symbol, i);
                if !self.candles.contains_key(&key) && self.books.len() + self.candles.len() >= 10 {
                    cx.notice(
                        safe_cex_engine::Level::Warn,
                        code::SUBSCRIPTION_REJECTED,
                        "WEEX book/candle capacity reached",
                    );
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
                let Some(request) = Self::candle_query(cx, command, i) else {
                    return;
                };
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
                self.send_candle_query(
                    cx,
                    if historical {
                        paths::HISTORY_KLINES
                    } else {
                        paths::KLINES
                    },
                    &query,
                    request,
                );
            }
            Command::FetchLeverage { symbol, .. } => self.fetch_leverage(cx, symbol),
            _ => cx.notice(
                safe_cex_engine::Level::Warn,
                code::COMMAND_UNSUPPORTED,
                "WEEX command not implemented yet",
            ),
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
#[cfg(test)]
use Options as NativeOptions;
#[cfg(test)]
#[path = "../tests/support/setup.rs"]
mod setup;
