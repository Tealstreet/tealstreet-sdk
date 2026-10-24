//! GRVT perpetuals: deterministic market, account and order state machines.
mod account;
mod candles;
mod depth_query;
mod history;
mod identity;
#[cfg(test)]
mod preparation_delegation_tests;
mod sign;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub trading_account_id: Option<String>,
    pub signer_address: Option<String>,
    pub session_proxy: bool,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

#[derive(Clone)]
pub struct InstrumentInfo {
    pub hash: String,
    pub base_decimals: Option<i32>,
    pub quote_decimals: Option<i32>,
}
pub struct Pending {
    pub(crate) path: &'static str,
    pub(crate) body: String,
    pub(crate) job: Job,
    pub(crate) attempts: u8,
    pub(crate) auth_resent: bool,
    pub(crate) auth_generation: u64,
    /// Engine authority of managed work (the session login); None for legacy.
    pub(crate) context: Option<safe_cex_engine::request_context::Context>,
}
pub(crate) enum Job {
    BookQuery(Box<depth_query::Request>),
    Account(account::Request),
    Trade(trading::Request),
    History(Box<history::Request>),
    Identity {
        command: safe_cex_engine::CommandId,
    },
    Markets {
        context: Context,
        rows: Vec<Market>,
        metadata: HashMap<String, InstrumentInfo>,
        /// Instrument ids whose rows were skipped as malformed.
        malformed: Vec<String>,
        failed: bool,
    },
    Candles(Box<candles::Request>),
}
impl Job {
    fn is_private(&self) -> bool {
        match self {
            Self::BookQuery(_)
            | Self::Markets { .. }
            | Self::Candles(_)
            | Self::Account(account::Request::Login) => false,
            Self::History(request) => !request.is_public(),
            _ => true,
        }
    }
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    last_received: f64,
    supervisor: Supervisor,
}
impl Default for Connection {
    fn default() -> Self {
        Self {
            socket: None,
            open: false,
            last_received: 0.,
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 20_000.,
                ping_interval_ms: Some(5_000.),
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct Depth {
    book: Book,
    subscribers: u32,
    sequence: Option<u64>,
    ready: bool,
}
#[derive(Clone, Copy, Default)]
struct TickerState {
    buy: f64,
    sell: f64,
    open: f64,
    sequence: Option<u64>,
    first_sequence: Option<u64>,
    subscriptions: u8,
}
pub struct Grvt {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    account: account::Account,
    trading: trading::Trading,
    metadata: HashMap<String, InstrumentInfo>,
    public: Connection,
    books: HashMap<String, Depth>,
    candle_sockets: HashMap<Interval, Connection>,
    candles: HashMap<(String, Interval), u32>,
    // Catalog-indexed state shares the Store identities instead of allocating
    // the same stream/selector strings in four separate maps.
    ticker_state: Vec<TickerState>,
    request_id: u32,
    ticker_batch: Option<TimerId>,
    ticker_offset: usize,
    startup_retry: safe_cex_engine::startup::Backoff<()>,
    refresh: Option<TimerId>,
}
#[derive(Serialize)]
struct Subscription<'a> {
    jsonrpc: &'static str,
    id: u32,
    method: &'a str,
    params: Selector<'a>,
}
#[derive(Serialize)]
struct Selector<'a> {
    stream: &'a str,
    selectors: [&'a str; 1],
}
fn interval(i: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match i {
        M1 => "CI_1_M",
        M3 => "CI_3_M",
        M5 => "CI_5_M",
        M15 => "CI_15_M",
        M30 => "CI_30_M",
        H1 => "CI_1_H",
        H2 => "CI_2_H",
        H4 => "CI_4_H",
        H6 => "CI_6_H",
        H8 => "CI_8_H",
        H12 => "CI_12_H",
        D1 => "CI_1_D",
        D3 => "CI_3_D",
        W1 => "CI_1_W",
        _ => return None,
    })
}
impl Grvt {
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            account: account::Account::new(),
            trading: trading::Trading::default(),
            metadata: HashMap::new(),
            public: Connection::default(),
            books: HashMap::new(),
            candle_sockets: HashMap::new(),
            candles: HashMap::new(),
            ticker_state: Vec::new(),
            request_id: 0,
            ticker_batch: None,
            ticker_offset: 0,
            startup_retry: Default::default(),
            refresh: None,
        }
    }
    fn edge(&self) -> &'static str {
        if self.options.testnet {
            "https://edge.testnet.grvt.io"
        } else {
            "https://edge.grvt.io"
        }
    }
    fn mdg(&self) -> &'static str {
        if self.options.testnet {
            "https://market-data.testnet.grvt.io"
        } else {
            "https://market-data.grvt.io"
        }
    }
    fn public_url(&self) -> &'static str {
        if self.options.testnet {
            "wss://market-data.testnet.grvt.io/ws/full"
        } else {
            "wss://market-data.grvt.io/ws/full"
        }
    }
    fn request(&mut self, cx: &mut Cx<'_, Pending>, path: &'static str, body: String, job: Job) {
        self.send_request(
            cx,
            Pending {
                path,
                body,
                job,
                attempts: 0,
                auth_resent: false,
                auth_generation: self.account.generation,
                context: None,
            },
        );
    }
    fn send_request(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending) {
        if let Job::BookQuery(request) = pending.job {
            self.book_query_send(cx, *request);
            return;
        }
        if let Job::Candles(request) = pending.job {
            self.candle_send(cx, *request);
            return;
        }
        if let Job::Trade(request) = &pending.job
            && let Err(error) = self.trade_pending_guard(cx, request)
        {
            self.fail_pending(cx, pending, &error);
            return;
        }
        if pending.job.is_private() && self.account.session.is_none() {
            self.account_defer(cx, pending);
            return;
        }
        pending.auth_generation = self.account.generation;
        let (base, headers, at) = match &pending.job {
            Job::BookQuery(_) => unreachable!(),
            Job::Candles(_) => unreachable!(),
            Job::Markets { .. } => unreachable!("catalog uses managed HTTP"),
            Job::History(request) => (
                if request.is_public() {
                    self.mdg()
                } else if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                if request.is_public() {
                    vec![("Content-Type", "application/json".into())]
                } else {
                    self.account_headers()
                },
                request.array_at(),
            ),
            Job::Trade(request) => (
                if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                self.account_headers(),
                request.array_at(),
            ),
            Job::Identity { .. } => (
                if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                self.account_headers(),
                None,
            ),
            Job::Account(request) => (
                if self.options.testnet {
                    "https://trades.testnet.grvt.io"
                } else {
                    "https://trades.grvt.io"
                },
                self.account_headers(),
                request.array_at(),
            ),
        };
        let url = format!("{base}{}", pending.path);
        let body = Some(pending.body.clone());
        let builders = matches!(pending.job, Job::Account(account::Request::Builders { .. }));
        let id = if let Some(at) = at {
            cx.request_streamed_body(Method::Post, url, headers, body, at, pending)
        } else {
            cx.request(Method::Post, url, headers, body, pending)
        };
        if builders {
            // Matches TS's 10s abort; its own retry budget lives in `builder_failed`.
            cx.request_timeout(id, 10_000);
        }
    }
    fn fail_pending(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending.job {
            Job::BookQuery(request) => {
                let _ = cx.fail_plan(
                    &request.plan,
                    depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
                );
            }
            Job::Identity { command } => self.identity_failed(cx, command, error),
            Job::Account(request) => self.account_failed(cx, request, error),
            Job::Trade(request) => {
                self.trade_failed(cx, request, error, safe_cex_engine::Outcome::NotSent)
            }
            Job::History(request) => self.history_rejected(cx, *request, error),
            Job::Markets { context, .. } => {
                let _ = cx.finish_http_context(&context);
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    "GRVT instrument read failed",
                );
                self.startup_retry.failed(cx, ());
            }
            Job::Candles(request) => {
                let _ = cx.fail_plan(&request.plan, "GRVT candle read failed");
            }
        }
    }
    fn fetch_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        let context = match cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(context) => context,
            Err(_) => {
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    "GRVT instrument read failed",
                );
                self.startup_retry.failed(cx, ());
                return;
            }
        };
        let pending = Pending {
            path: "/lite/v1/all_instruments",
            body: "{}".into(),
            job: Job::Markets {
                context: context.clone(),
                rows: vec![],
                metadata: HashMap::new(),
                malformed: vec![],
                failed: false,
            },
            attempts: 0,
            auth_resent: false,
            auth_generation: self.account.generation,
            context: None,
        };
        match cx.request_managed_with_headers(
            context,
            None,
            Method::Post,
            format!("{}/lite/v1/all_instruments", self.mdg()),
            &[("Content-Type", "application/json")],
            Some(pending.body.clone()),
            pending,
        ) {
            Ok(id) => cx.stream_response(id, ArrayAt::Envelope(&[&["r"], &["result"]])),
            Err((pending, _)) => self.fail_pending(cx, pending, "GRVT catalog preparation failed"),
        }
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.public.socket.is_some() || self.public.supervisor.is_waiting() {
            return;
        }
        self.public.socket = Some(cx.connect(
            safe_cex_engine::SocketSpec::new(self.public_url()).channel(SocketChannel::Public),
        ));
        self.public.last_received = cx.now;
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
    }
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, stream: &str, selector: &str, remove: bool) {
        if !self.public.open {
            return;
        }
        let bit = match stream {
            "v1.ticker.d" => 1,
            "v1.book.s" => 2,
            _ => return,
        };
        let primary = selector.split('@').next().unwrap_or(selector);
        let Some(index) = cx.store.market_index_by_id(primary) else {
            return;
        };
        let Some(state) = self.ticker_state.get_mut(index as usize) else {
            return;
        };
        let subscribed = state.subscriptions & bit != 0;
        if subscribed == !remove {
            return;
        }
        if remove {
            state.subscriptions &= !bit;
        } else {
            state.subscriptions |= bit;
        }
        self.request_id = self.request_id.wrapping_add(1).max(1);
        let message = Subscription {
            jsonrpc: "2.0",
            id: self.request_id,
            method: if remove { "unsubscribe" } else { "subscribe" },
            params: Selector {
                stream,
                selectors: [selector],
            },
        };
        if let Some(socket) = self.public.socket {
            cx.send(
                socket,
                serde_json::to_string(&message).expect("subscription"),
            );
        }
    }
    fn ticker_batch(&mut self, cx: &mut Cx<'_, Pending>) {
        let ids: Vec<_> = cx
            .store
            .markets()
            .iter()
            .skip(self.ticker_offset)
            .take(20)
            .map(|m| m.id.clone())
            .collect();
        for id in &ids {
            self.subscribe(cx, "v1.ticker.d", &format!("{id}@500"), false);
        }
        self.ticker_offset += ids.len();
        if self.ticker_offset < cx.store.markets().len() {
            self.ticker_batch = Some(cx.set_timer(100.));
        }
    }
    fn disconnect(&mut self, cx: &mut Cx<'_, Pending>, close: bool) {
        if let Some(socket) = self.public.socket.take()
            && close
        {
            cx.close(socket);
        }
        self.public.open = false;
        if let Some(timer) = self.ticker_batch.take() {
            cx.clear_timer(timer);
        }
        for state in &mut self.ticker_state {
            state.subscriptions = 0;
            state.sequence = None;
            state.first_sequence = None;
        }
        for depth in self.books.values_mut() {
            depth.ready = false;
            depth.sequence = None;
        }
        self.public.supervisor.retry(cx);
    }
    fn candle_connect(&mut self, cx: &mut Cx<'_, Pending>, timeframe: Interval) {
        let url = self.public_url();
        let c = self.candle_sockets.entry(timeframe).or_default();
        if c.socket.is_some() || c.supervisor.is_waiting() {
            return;
        }
        // Each supported interval has its own feed identity on the shared URL.
        let feed: &'static str = match timeframe {
            Interval::M1 => "candles-1m",
            Interval::M3 => "candles-3m",
            Interval::M5 => "candles-5m",
            Interval::M15 => "candles-15m",
            Interval::M30 => "candles-30m",
            Interval::H1 => "candles-1h",
            Interval::H2 => "candles-2h",
            Interval::H4 => "candles-4h",
            Interval::H6 => "candles-6h",
            Interval::H8 => "candles-8h",
            Interval::H12 => "candles-12h",
            Interval::D1 => "candles-1d",
            Interval::D3 => "candles-3d",
            Interval::W1 => "candles-1w",
            _ => return,
        };
        c.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(url)
                    .channel(SocketChannel::Public)
                    .feed(feed, false),
            ),
        );
        c.last_received = cx.now;
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            c.supervisor.connecting(cx);
        }
    }
    fn candle_subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        timeframe: Interval,
        remove: bool,
    ) {
        let Some(c) = self.candle_sockets.get(&timeframe).filter(|c| c.open) else {
            return;
        };
        let (Some(socket), Some(market), Some(interval)) =
            (c.socket, cx.store.market(symbol), interval(timeframe))
        else {
            return;
        };
        self.request_id = self.request_id.wrapping_add(1).max(1);
        let selector = format!("{}@{interval}-TRADE", market.id);
        let msg = Subscription {
            jsonrpc: "2.0",
            id: self.request_id,
            method: if remove { "unsubscribe" } else { "subscribe" },
            params: Selector {
                stream: "v1.candle",
                selectors: [&selector],
            },
        };
        cx.send(
            socket,
            serde_json::to_string(&msg).expect("candle subscription"),
        );
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if text == "ping" {
            if let Some(s) = self.public.socket {
                cx.send(s, "pong");
            }
            return;
        }
        if text == "pong" {
            return;
        }
        let Ok(message) = serde_json::from_str::<wire::Message<'_>>(text) else {
            return;
        };
        if let Some(ack) = message.result {
            if ack.stream == "v1.ticker.d" {
                for (selector, first) in ack.subs.iter().zip(ack.first) {
                    let primary = selector.split('@').next().unwrap_or(selector);
                    if let (Some(index), Ok(first)) =
                        (cx.store.market_index_by_id(primary), first.parse::<u64>())
                        && let Some(state) = self.ticker_state.get_mut(index as usize)
                    {
                        state.first_sequence = Some(first);
                    }
                }
            }
            return;
        }
        let Some(feed) = message.feed else {
            return;
        };
        let Some(index) = cx
            .store
            .market_index_by_id(&feed.instrument)
            .or_else(|| cx.store.market_index(&feed.instrument))
        else {
            return;
        };
        let stream = &*message.stream;
        if !matches!(
            stream,
            "v1.ticker.d" | "v1.ticker.s" | "v1.book.s" | "v1.candle"
        ) {
            return;
        }
        let primary = message
            .selector
            .split('@')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(&feed.instrument);
        if let Ok(sequence) = message.sequence.parse::<u64>()
            && stream == "v1.ticker.d"
        {
            let Some(sequence_index) = cx
                .store
                .market_index_by_id(primary)
                .or_else(|| cx.store.market_index(primary))
            else {
                return;
            };
            let state = &mut self.ticker_state[sequence_index as usize];
            if sequence == 0 {
                // Initial snapshot has no live sequence; the ACK anchors the first delta.
                state.sequence = None;
            } else {
                if let Some(last) = state.sequence {
                    if sequence == last {
                        return;
                    }
                    if sequence != last.saturating_add(1) {
                        self.disconnect(cx, true);
                        return;
                    }
                } else if state.first_sequence.is_some_and(|first| sequence != first) {
                    self.disconnect(cx, true);
                    return;
                }
                state.sequence = Some(sequence);
            }
        }
        match stream {
            "v1.ticker.d" | "v1.ticker.s" => {
                let state = &mut self.ticker_state[index as usize];
                if let Some(v) = feed.buy_volume {
                    state.buy = v.0;
                }
                if let Some(v) = feed.sell_volume {
                    state.sell = v.0;
                }
                if let Some(v) = feed.open_price {
                    state.open = v.0;
                }
                // This venue constructs tickers in catalog order at every replacement.
                cx.store.update_ticker_at(index, |t| {
                    if let Some(v) = feed.last {
                        t.last = v.0;
                    }
                    if let Some(v) = feed.mark {
                        t.mark = v.0;
                        if t.last == 0. && v.0 > 0. {
                            t.last = v.0;
                        }
                    }
                    if let Some(v) = feed.index {
                        t.index = v.0;
                    }
                    if let Some(v) = feed.bid {
                        t.bid = v.0;
                    }
                    if let Some(v) = feed.ask {
                        t.ask = v.0;
                    }
                    if let Some(v) = feed.funding {
                        t.funding_rate = v.0 / 100.;
                    }
                    if let Some(v) = feed.interest {
                        t.open_interest = v.0;
                    }
                    if let Some(v) = feed.funding_time {
                        t.funding_time = v.0 / 1e9;
                    }
                    t.volume = state.buy + state.sell;
                    t.quote_volume = t.volume * t.last;
                    if state.open > 0. {
                        t.percentage = (t.last - state.open) / state.open * 100.;
                    }
                });
                self.sync_balance_upnl(cx);
            }
            "v1.book.s" => {
                let symbol = cx.store.markets()[index as usize].symbol.clone();
                let Some(depth) = self.books.get_mut(&symbol).filter(|d| d.subscribers > 0) else {
                    return;
                };
                let (Some(bids), Some(asks)) = (feed.bids, feed.asks) else {
                    return;
                };
                if bids
                    .iter()
                    .chain(&asks)
                    .any(|l| l.price.0 <= 0. || l.size.0 < 0.)
                {
                    return;
                }
                if bids.windows(2).any(|x| x[0].price.0 <= x[1].price.0)
                    || asks.windows(2).any(|x| x[0].price.0 >= x[1].price.0)
                {
                    return;
                }
                if let Ok(sequence) = message.sequence.parse::<u64>() {
                    // Snapshot books are self-contained: gaps need no REST stitching.
                    if sequence != 0 && depth.sequence.is_some_and(|last| sequence <= last) {
                        return;
                    }
                    depth.sequence = Some(sequence);
                }
                depth
                    .book
                    .load_sorted(BookSide::Bids, bids.iter().map(|l| (l.price.0, l.size.0)));
                depth
                    .book
                    .load_sorted(BookSide::Asks, asks.iter().map(|l| (l.price.0, l.size.0)));
                depth.ready = true;
                cx.book_changed(&symbol);
            }
            "v1.candle" => {
                let market_symbol = cx.store.markets()[index as usize].symbol.clone();
                let Some((_, suffix)) = message.selector.split_once('@') else {
                    return;
                };
                let Some(grvt_interval) = suffix.strip_suffix("-TRADE") else {
                    return;
                };
                for ((symbol, interval_name), subscribers) in &self.candles {
                    if *subscribers > 0
                        && *symbol == market_symbol
                        && interval(*interval_name) == Some(grvt_interval)
                        && let Some(candle) = feed.candle()
                    {
                        cx.candle(symbol, *interval_name, candle);
                    }
                }
            }
            _ => {}
        }
    }
}
impl Venue for Grvt {
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            order_book_query: true,
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            client_order_lookup: true,
            fills_by_order_id: true,
            cancel_all: safe_cex_engine::CancelAll::Native,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        // Public quotes cannot make an unseeded account appear loaded.
        if cx.store.loaded().balance && cx.store.loaded().positions {
            self.sync_balance_upnl(cx);
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        // GRVT itself refuses an untagged signer; a stale configured address must not block trading.
        if let (Some(secret), Some(expected)) = (
            self.options.secret.as_deref(),
            self.options.signer_address.as_deref(),
        ) && let Ok(signer) = sign::signer_address(secret)
            && !signer.eq_ignore_ascii_case(expected)
        {
            cx.notice(Level::Warn, code::CONFIG_INVALID,
                format!(
                    "Provided signer address ({expected}) does not match wallet address ({signer}). Make sure {signer} is tagged to your API key in the GRVT UI."
                ),
            );
        }
        self.fetch_markets(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account_open(cx, socket) {
            return;
        }
        if let Some(interval) = self
            .candle_sockets
            .iter()
            .find(|(_, c)| c.socket == Some(socket))
            .map(|(i, _)| *i)
        {
            let c = self
                .candle_sockets
                .get_mut(&interval)
                .expect("candle socket");
            c.open = true;
            c.supervisor.opened(cx);
            c.last_received = cx.now;
            let symbols: Vec<_> = self
                .candles
                .iter()
                .filter(|((_, i), n)| i == &interval && **n > 0)
                .map(|((s, _), _)| s.clone())
                .collect();
            for symbol in symbols {
                self.candle_subscription(cx, &symbol, interval, false);
            }
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = true;
        self.public.supervisor.opened(cx);
        self.public.last_received = cx.now;
        self.ticker_offset = 0;
        self.ticker_batch(cx);
        let books: Vec<_> = self
            .books
            .iter()
            .filter(|(_, d)| d.subscribers > 0)
            .map(|(s, _)| s.clone())
            .collect();
        for s in books {
            if let Some(m) = cx.store.market(&s) {
                self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), false);
            }
        }
    }

    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if let Some(connection) = self
            .candle_sockets
            .values_mut()
            .find(|c| c.socket == Some(socket))
        {
            connection.last_received = cx.now;
            connection.supervisor.received(cx.now);
            if text == "ping" {
                cx.send(socket, "pong");
            } else {
                self.public_message(cx, text);
            }
            return;
        }
        if self.public.socket == Some(socket) {
            self.public.last_received = cx.now;
            self.public.supervisor.received(cx.now);
            self.public_message(cx, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket) {
            return;
        }
        if let Some(connection) = self
            .candle_sockets
            .values_mut()
            .find(|c| c.socket == Some(socket))
        {
            connection.socket = None;
            connection.open = false;
            connection.supervisor.retry(cx);
            return;
        }
        if self.public.socket == Some(socket) {
            self.disconnect(cx, false);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match &mut pending.job {
            Job::Identity { .. } | Job::BookQuery(_) => {}
            Job::Account(request) => self.account_item(cx, request, item),
            Job::Trade(request) => self.trade_item(cx, request, item),
            Job::History(request) => self.history_item(cx, request, item),
            Job::Markets {
                context,
                rows,
                metadata,
                malformed,
                failed: _,
            } => {
                if cx.validate_background_read_http_delivery(context).is_err() {
                    return;
                }
                let Ok(i) = serde_json::from_str::<wire::Instrument<'_>>(item) else {
                    safe_cex_engine::startup::skipped_row(cx);
                    if let Some(id) = instrument_id(item) {
                        malformed.push(id);
                    }
                    return;
                };
                if i.kind != "PERPETUAL" {
                    return;
                }
                if i.id.is_empty() || i.min.0 <= 0. || i.tick.0 <= 0. {
                    safe_cex_engine::startup::skipped_row(cx);
                    if !i.id.is_empty() {
                        malformed.push(i.id.to_string());
                    }
                    return;
                }
                metadata.insert(
                    i.id.to_string(),
                    InstrumentInfo {
                        hash: i.hash.into_owned(),
                        base_decimals: i.base_decimals,
                        quote_decimals: i.quote_decimals,
                    },
                );
                rows.push(Market {
                    id: i.id.to_string(),
                    symbol: i.id.to_ascii_uppercase(),
                    pretty_symbol: None,
                    base: i.base.into_owned(),
                    quote: i.quote.into_owned(),
                    active: i.active.unwrap_or(true),
                    precision: Precision {
                        amount: i.min.0,
                        price: i.tick.0,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: i.min.0,
                            max: i.max.or(i.max_size).map_or(999999999., |v| v.0),
                        },
                        leverage: MinMax { min: 1., max: 100. },
                        price: None,
                    },
                    contract_size: None,
                    is_expiring: None,
                    is_inverse: None,
                    is_spot: Some(false),
                    is_perpetual: Some(true),
                    is_read_only: None,
                    taker_fee: None,
                    maker_fee: None,
                });
            }
            Job::Candles(request) => request.item(cx, item),
        }
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if let Job::Markets { context, .. } = &pending.job
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            let _ = cx.finish_http_context(context);
            return;
        }
        if let Job::Candles(request) = pending.job {
            self.candle_response(cx, *request, response);
            return;
        }
        if let Job::BookQuery(request) = pending.job {
            self.book_query_response(cx, *request, response);
            return;
        }
        if let Some(context) = pending.context.take() {
            self.login_answered(cx, context, &response);
        }
        if let Err(error) = wire::check(&response) {
            let code = wire::code(&response);
            if code == "1000"
                && !pending.auth_resent
                && !matches!(pending.job, Job::Account(account::Request::Login))
                && !matches!(pending.job, Job::Markets { .. } | Job::Candles(_))
                && !matches!(&pending.job,Job::History(request) if request.is_public())
            {
                if let Job::History(request) = &mut pending.job
                    && !request.reset()
                {
                    self.fail_pending(cx, pending, "GRVT history page reset failed");
                    return;
                }
                if let Job::Trade(request) = &mut pending.job {
                    request.reset();
                }
                self.account_refresh(cx, pending);
            } else if let Job::Trade(request) = pending.job {
                self.trade_failed(cx, request, &error, wire::write_outcome(&response));
            } else if let Job::History(request) = pending.job {
                self.history_failed(cx, *request, &error);
            } else {
                self.fail_pending(cx, pending, &error);
            }
            return;
        }
        if matches!(pending.job, Job::Identity { .. })
            && pending.auth_generation != self.account.generation
        {
            self.fail_pending(cx, pending, "Stale GRVT account identity session");
            return;
        }
        if pending.job.is_private() {
            self.account_response_headers(cx, pending.auth_generation, &response);
        }
        if matches!(&pending.job, Job::Trade(request) if request.is_symbol_read())
            && pending.auth_generation != self.account.generation
        {
            self.fail_pending(cx, pending, "Stale GRVT cancellation snapshot session");
            return;
        }
        match pending.job {
            Job::BookQuery(_) => unreachable!(),
            Job::Identity { command } => self.identity_response(cx, command, response),
            Job::Account(request) => self.account_response(cx, request, response),
            Job::Trade(request) => self.trade_response(cx, request, response),
            Job::History(request) => self.history_response(cx, *request, response),
            Job::Markets {
                context,
                mut rows,
                mut metadata,
                malformed,
                failed,
            } => {
                if failed || rows.is_empty() {
                    let _ = cx.finish_http_context(&context);
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "Invalid or empty GRVT instrument catalog",
                    );
                    self.startup_retry.failed(cx, ());
                    return;
                }
                // A held market whose row is briefly malformed keeps its previous row,
                // so its positions and orders stay mapped across the refresh.
                for id in malformed {
                    if rows.iter().any(|market| market.id == id) {
                        continue;
                    }
                    if let Some(previous) = cx.store.market_by_id(&id)
                        && let Some(info) = self.metadata.get(&id)
                    {
                        rows.push(previous.clone());
                        metadata.insert(id, info.clone());
                    }
                }
                // The public instrument catalog has no authenticated leverage
                // rails. Retain those already fetched from the trading API;
                // replacing signing metadata must not restore the 1..100 defaults.
                let has_new_markets = rows
                    .iter()
                    .any(|market| cx.store.market_by_id(&market.id).is_none());
                for market in &mut rows {
                    if let Some(previous) = cx.store.market_by_id(&market.id) {
                        market.limits.leverage = previous.limits.leverage;
                    }
                }
                self.startup_retry.succeeded(cx, &());
                let tickers = rows
                    .iter()
                    .map(|m| {
                        cx.store
                            .ticker(&m.symbol)
                            .cloned()
                            .unwrap_or_else(|| Ticker {
                                id: m.id.clone(),
                                symbol: m.symbol.clone(),
                                ..Ticker::default()
                            })
                    })
                    .collect();
                let first = !cx.store.loaded().markets;
                // Catalog refreshes can reorder or insert rows. Remap by native
                // instrument identity before replacing the Store's indexes.
                let previous = std::mem::take(&mut self.ticker_state);
                self.ticker_state = rows
                    .iter()
                    .map(|market| {
                        cx.store
                            .market_index_by_id(&market.id)
                            .and_then(|index| previous.get(index as usize))
                            .copied()
                            .unwrap_or_default()
                    })
                    .collect();
                self.metadata = metadata;
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect(cx);
                if first {
                    self.private_start(cx);
                } else if has_new_markets {
                    // Hydrate account-specific rails for newly listed markets.
                    // The helper waits for a private session and avoids duplicate reads.
                    self.refresh_leverage(cx);
                }
                if self.public.open {
                    self.ticker_offset = 0;
                    self.ticker_batch(cx);
                }
                if self.refresh.is_none() {
                    self.refresh = Some(cx.set_timer(300_000.));
                }
                let _ = cx.finish_http_context(&context);
            }
            Job::Candles(_) => unreachable!(),
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if let Job::Markets { context, .. } = &pending.job
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            let _ = cx.finish_http_context(context);
            return;
        }
        if let Job::Candles(request) = pending.job {
            if cx
                .validate_plan_delivery(&request.plan, &request.leg)
                .is_ok()
            {
                let _ = cx.fail_plan(&request.plan, "GRVT candle read failed");
            }
            return;
        }
        if wire::check(&response).is_err() {
            self.on_response(cx, pending, response);
        } else if let Job::History(request) = pending.job {
            self.history_failed(cx, *request, "Missing GRVT response array");
        } else {
            self.fail_pending(cx, pending, "Missing GRVT response array");
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        if matches!(pending.job, Job::Markets { .. }) {
            self.fail_pending(cx, pending, &error.message);
            return;
        }
        if let Job::BookQuery(request) = pending.job {
            let _ = cx.fail_plan(&request.plan, error);
        } else if let Job::Candles(request) = pending.job {
            let _ = cx.fail_plan(&request.plan, error);
        } else {
            self.on_sign_error(cx, pending, error);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        if matches!(pending.job, Job::Markets { .. }) {
            self.fail_pending(cx, pending, error);
            return;
        }
        if let Job::BookQuery(request) = pending.job {
            let _ = cx.fail_plan(
                &request.plan,
                depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
            );
            return;
        }
        if let Job::Candles(request) = pending.job {
            let _ = cx.fail_plan(&request.plan, "GRVT candle read failed");
            return;
        }
        if pending.context.take().is_some() {
            self.login_failed(cx, pending);
            return;
        }
        let read = match &pending.job {
            Job::Trade(request) => request.is_read(),
            Job::Account(account::Request::Builders { .. }) => false,
            _ => true,
        };
        if read && !matches!(pending.job, Job::Markets { .. }) && pending.attempts < 5 {
            pending.attempts += 1;
            match &mut pending.job {
                Job::Markets { .. } => unreachable!("catalog retries use startup backoff"),
                Job::Candles(_) => unreachable!(),
                Job::Identity { .. } | Job::BookQuery(_) => {}
                Job::Account(request) => request.reset(),
                Job::Trade(request) => request.reset(),
                Job::History(request) => {
                    if !request.reset() {
                        self.fail_pending(cx, pending, "GRVT history page reset failed");
                        return;
                    }
                }
            }
            self.send_request(cx, pending);
            return;
        }
        match pending.job {
            Job::Trade(request) => {
                self.trade_failed(cx, request, error, safe_cex_engine::Outcome::Unknown)
            }
            Job::History(request) => self.history_failed(cx, *request, error),
            _ => self.fail_pending(cx, pending, error),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        if let Job::BookQuery(request) = pending.job {
            if matches!(
                error.kind,
                Some(safe_cex_engine::ErrorKind::Network | safe_cex_engine::ErrorKind::Timeout)
            ) {
                self.book_query_failed(cx, *request, error);
            } else {
                let _ = cx.fail_plan(&request.plan, error);
            }
        } else if let Job::Candles(request) = pending.job {
            if matches!(
                error.kind,
                Some(safe_cex_engine::ErrorKind::Network | safe_cex_engine::ErrorKind::Timeout)
            ) {
                self.candle_failed(cx, *request);
            } else {
                let _ = cx.fail_plan(&request.plan, error);
            }
        } else {
            self.on_request_failed(cx, pending, &error.message);
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(timeframe) = self
            .candle_sockets
            .iter()
            .find(|(_, c)| c.supervisor.owns_timer(timer))
            .map(|(i, _)| *i)
        {
            let c = self
                .candle_sockets
                .get_mut(&timeframe)
                .expect("candle timer");
            match c.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.candle_connect(cx, timeframe),
                SocketTick::TimedOut(_) => {
                    if let Some(socket) = c.socket.take() {
                        cx.close(socket);
                    }
                    c.open = false;
                    c.supervisor.retry(cx);
                }
                SocketTick::Ping if c.open && cx.now - c.last_received >= 6_000. => {
                    if let Some(socket) = c.socket {
                        cx.send(socket, "ping");
                    }
                }
                _ => {}
            }
            return;
        }
        if self.public.supervisor.owns_timer(timer) {
            match self.public.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect(cx),
                SocketTick::TimedOut(_) => self.disconnect(cx, true),
                SocketTick::Ping
                    if self.public.open && cx.now - self.public.last_received >= 6_000. =>
                {
                    if let Some(socket) = self.public.socket {
                        cx.send(socket, "ping");
                    }
                }
                _ => {}
            }
        } else if self.ticker_batch == Some(timer) {
            self.ticker_batch = None;
            self.ticker_batch(cx);
        } else if self.startup_retry.on_timer(timer).is_some() {
            self.fetch_markets(cx);
        } else if self.refresh == Some(timer) {
            self.refresh = None;
            self.fetch_markets(cx);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchOrderBook { ref symbol, .. } = command {
            self.fetch_book_query(cx, symbol);
            return;
        }

        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            return;
        }
        if let Command::Query { command, query } = command {
            self.start_query(cx, command, query);
            return;
        }
        if let Command::FetchLeverage { command, symbol } = command {
            self.fetch_leverage_command(cx, command, symbol);
            return;
        }
        if matches!(
            &command,
            Command::PlaceOrders { .. }
                | Command::CancelOrders { .. }
                | Command::AmendOrder { .. }
                | Command::CancelSymbolOrders { .. }
                | Command::SetLeverage { .. }
                | Command::SetMarginType { .. }
                | Command::SetPositionMode { .. }
                | Command::FetchLeverage { .. }
        ) {
            self.trade_command(cx, command);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::Venue { command, .. } => {
                safe_cex_engine::unsupported_venue_action(cx, command)
            }
            Command::SubscribeOrderBook { symbol } => {
                let key = cx
                    .store
                    .market(&symbol)
                    .map_or(symbol.clone(), |m| m.symbol.clone());
                let depth = self.books.entry(key.clone()).or_default();
                depth.subscribers += 1;
                if depth.subscribers == 1 {
                    depth.ready = false;
                    depth.sequence = None;
                }
                if depth.subscribers == 1
                    && let Some(m) = cx.store.market(&key)
                {
                    self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), false);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(depth) = self.books.get_mut(&symbol) {
                    depth.subscribers = depth.subscribers.saturating_sub(1);
                    if depth.subscribers == 0
                        && let Some(m) = cx.store.market(&symbol)
                    {
                        self.subscribe(cx, "v1.book.s", &format!("{}@500-50", m.id), true);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                if interval(timeframe).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        timeframe.unsupported("GRVT"),
                    );
                    return;
                }
                if cx.store.market(&symbol).is_none() {
                    return;
                }
                let count = self.candles.entry((symbol.clone(), timeframe)).or_default();
                *count += 1;
                if *count == 1 {
                    self.candle_connect(cx, timeframe);
                    self.candle_subscription(cx, &symbol, timeframe, false);
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: timeframe,
            } => {
                if let Some(count) = self.candles.get_mut(&(symbol.clone(), timeframe)) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.candles.remove(&(symbol.clone(), timeframe));
                        self.candle_subscription(cx, &symbol, timeframe, true);
                        if !self.candles.keys().any(|(_, i)| i == &timeframe)
                            && let Some(mut c) = self.candle_sockets.remove(&timeframe)
                        {
                            if let Some(socket) = c.socket {
                                cx.close(socket);
                            }
                            c.supervisor.cancel(cx);
                        }
                    }
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval: timeframe,
                limit,
                from_ms,
                to_ms,
            } => {
                self.fetch_candles(cx, symbol, timeframe, limit, from_ms, to_ms);
            }
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::AmendOrder { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. }
            | Command::FetchLeverage { command, .. }
            | Command::Query { command, .. } => cx.fail(command, "GRVT private phase pending"),
            Command::CancelOrdersWithSymbols { command, .. }
            | Command::FetchMaxLeverage { command, .. }
            | Command::AmendOrderWithSymbol { command, .. }
            | Command::FetchStpMode { command }
            | Command::SetStpMode { command, .. }
            | Command::FetchAccount { command }
            | Command::FetchOrderBook { command, .. }
            | Command::FetchOptionChain { command, .. }
            | Command::FetchOptionExpiries { command, .. }
            | Command::FetchAccountGreeks { command } => {
                cx.fail(command, "Unsupported GRVT command")
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|d| d.ready && d.subscribers > 0)
            .map(|d| &d.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .filter(|(_, d)| d.subscribers > 0)
                .map(|(s, d)| (s.as_str(), &d.book)),
        )
    }
}

/// The native id of an instrument row that failed to decode, when it has one.
fn instrument_id(item: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Identity<'a> {
        #[serde(borrow, rename = "i", alias = "instrument")]
        id: std::borrow::Cow<'a, str>,
    }
    let row: Identity<'_> = serde_json::from_str(item).ok()?;
    (!row.id.is_empty()).then(|| row.id.into_owned())
}
