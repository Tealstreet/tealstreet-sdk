//! Orderly perpetuals, shared by its six broker frontends.
mod account;
mod funding;
mod history;
pub mod sign;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick, Timeout};
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketId, TimerId, Venue, VenueError,
};
use serde::Serialize;
use std::collections::HashMap;
use wire::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Broker {
    #[default]
    Tealstreet,
    KodiakFi,
    Mode,
    SalsaDex,
    What,
    WooFi,
}
impl Broker {
    pub fn order_tag(self) -> &'static str {
        match self {
            Self::KodiakFi => "TEALKODIAK",
            Self::Mode => "TEALMODE",
            Self::SalsaDex => "TEALSALSA",
            Self::What => "TEALWHAT",
            Self::Tealstreet | Self::WooFi => "TEALSTREET",
        }
    }
}
#[derive(Default)]
pub struct Options {
    pub broker: Broker,
    pub public_data_only: bool,
    pub testnet: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub tick_interval: Option<f64>,
    /// Opaque refs installed by trusted setup; the provider signs every
    /// private read and the socket auth with them.
    pub identities: safe_cex_engine::signing::Identities,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

/// Whether `key` and `secret` are base58 Ed25519 keys this venue can sign with.
pub fn signing_keys_valid(key: &str, secret: &str) -> bool {
    sign::Credentials::new(key, secret, "account").is_ok()
}

pub enum Pending {
    History(history::Request),
    Trade(trading::Request),
    Account(
        safe_cex_engine::selected_read::AdmittedRead,
        account::Request,
    ),
    Markets {
        context: Context,
        rows: Vec<Market>,
        invalid: bool,
    },
    Tickers {
        context: Context,
        rows: Vec<Ticker>,
        invalid: bool,
    },
    Candles {
        context: Context,
        rows: Vec<Candle>,
        invalid: bool,
        from: Option<f64>,
        to: Option<f64>,
    },
    CandleHistory {
        context: Context,
        from: Option<f64>,
        to: Option<f64>,
        limit: usize,
    },
}
struct Connection {
    socket: Option<SocketId>,
    open: bool,
    payload_at: f64,
    recover: bool,
    ping_at: f64,
    supervisor: Supervisor,
}
impl Connection {
    fn new(auth_timeout_ms: Option<f64>) -> Self {
        Self {
            socket: None,
            open: false,
            payload_at: 0.0,
            recover: false,
            ping_at: 0.0,
            supervisor: Supervisor::new(Config {
                auth_timeout_ms,
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(10_000.0),
                ..Config::default()
            }),
        }
    }
    fn payload_stale(&self, now: f64) -> bool {
        now - self.payload_at >= 30_000.0
    }
}
struct Delta {
    ts: u64,
    depth: Depth,
}
struct BookState {
    book: Book,
    subscribers: u32,
    generation: u64,
    last: Option<u64>,
    bridge: bool,
    buffer: Vec<Delta>,
    snapshot: Option<(String, TimerId)>,
    retry: Option<TimerId>,
}
#[derive(Clone, Copy, PartialEq)]
enum Seed {
    Markets,
    Tickers,
}

pub struct Orderly {
    options: safe_cex_crypto::secrets::Sensitive<Options>,
    public: Connection,
    books: HashMap<String, BookState>,
    candles: HashMap<(String, Interval), u32>,
    native: HashMap<String, String>,
    seed_backoff: safe_cex_engine::startup::Backoff<Seed>,
    generation: u64,
    account: account::Account,
    trading: trading::Trading,
    history: history::HistoryState,
    funding: funding::FundingState,
}
fn complete_local_read(cx: &mut Cx<'_, Pending>, result: Result<String, VenueError>) {
    match cx.command_wait() {
        Some(safe_cex_engine::CommandWait::Remaining(_)) => {}
        Some(safe_cex_engine::CommandWait::Expired) => {
            let _ = cx.reject_expired_command(Operation::Read);
            return;
        }
        None => {
            let _ = cx.reject_unstamped_command(Operation::Read);
            return;
        }
    }
    let Ok(context) = cx.admit_http_command(Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }) else {
        return;
    };
    match result {
        Ok(result) => {
            let _ = cx.done_context(&context, result);
        }
        Err(error) => {
            let _ = cx.fail_context(&context, error);
        }
    }
}

impl Orderly {
    fn seed_context(pending: &Pending) -> Option<(Seed, &Context)> {
        match pending {
            Pending::Markets { context, .. } => Some((Seed::Markets, context)),
            Pending::Tickers { context, .. } => Some((Seed::Tickers, context)),
            _ => None,
        }
    }
    fn candle_context(pending: &Pending) -> Option<&Context> {
        match pending {
            Pending::Candles { context, .. } | Pending::CandleHistory { context, .. } => {
                Some(context)
            }
            _ => None,
        }
    }
    fn candle_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        url: String,
        streamed: bool,
        pending: Pending,
    ) {
        let context = Self::candle_context(&pending)
            .expect("candle continuation")
            .clone();
        match cx.request_managed(context, None, Method::Get, url, None, pending) {
            Ok(request) => {
                if streamed {
                    cx.stream_response(request, ArrayAt::Path(&["data", "rows"]));
                }
            }
            Err((pending, reason)) => {
                self.on_http_preparation_failed(cx, pending, reason, reason.venue_error())
            }
        }
    }
    pub fn new(options: Options) -> Self {
        let options = safe_cex_crypto::secrets::Sensitive::new(options);
        Self {
            options,
            public: Connection::new(None),
            books: HashMap::new(),
            candles: HashMap::new(),
            native: HashMap::new(),
            seed_backoff: Default::default(),
            generation: 0,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            history: history::HistoryState::default(),
            funding: funding::FundingState::default(),
        }
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://testnet-api-evm.orderly.org"
        } else {
            "https://api-evm.orderly.org"
        }
    }
    /// The broker application is the account's identity and affiliate.
    fn account_identity(&self) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            user_id: &'a str,
            affiliate_id: &'a str,
        }
        match self.options.application_id.as_deref() {
            Some(id) if !id.is_empty() && !self.options.public_data_only => {
                serde_json::to_string(&Identity {
                    user_id: id,
                    affiliate_id: id,
                })
                .map_err(|error| error.to_string())
            }
            _ => Err("Orderly private account ID is required".into()),
        }
    }

    fn public_url(&self) -> String {
        let host = if self.options.testnet {
            "testnet-ws-evm.orderly.org"
        } else {
            "ws-evm.orderly.org"
        };
        format!(
            "wss://{host}/ws/stream/{}",
            self.options
                .application_id
                .as_deref()
                .unwrap_or("OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY")
        )
    }
    fn connect_public(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public.open = false;
        self.public.payload_at = cx.now;
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
        self.public.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(self.public_url())
                    .channel(safe_cex_engine::SocketChannel::Public)
                    .feed("market", self.public.recover),
            ),
        );
    }
    fn drop_public(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.public.socket {
            cx.close(socket);
            self.on_close(cx, socket, 1006);
        }
    }
    fn public_topic_available(&self, cx: &mut Cx<'_, Pending>) -> bool {
        if self.books.len() + self.candles.len() >= 97 {
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                "Orderly public socket has reached its 100-topic limit",
            );
            false
        } else {
            true
        }
    }
    fn seed(&mut self, cx: &mut Cx<'_, Pending>, kind: Seed) {
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
            Err(error) => {
                self.retry_seed(cx, kind, &error.to_string());
                return;
            }
        };
        let (path, pending) = match kind {
            Seed::Markets => (
                "/v1/public/info",
                Pending::Markets {
                    context: context.clone(),
                    rows: vec![],
                    invalid: false,
                },
            ),
            Seed::Tickers => (
                "/v1/public/futures",
                Pending::Tickers {
                    context: context.clone(),
                    rows: vec![],
                    invalid: false,
                },
            ),
        };
        match cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}{path}", self.rest()),
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(&["data", "rows"])),
            Err((pending, error)) => self.on_request_failed(cx, pending, &error.to_string()),
        }
    }
    fn retry_seed(&mut self, cx: &mut Cx<'_, Pending>, kind: Seed, error: &str) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if cx.now.is_finite() {
            self.seed_backoff.failed(cx, kind);
        }
    }
    fn subscribe(&self, cx: &mut Cx<'_, Pending>, topic: &str, add: bool) {
        if !self.public.open {
            return;
        }
        #[derive(Serialize)]
        struct Sub<'a> {
            event: &'a str,
            topic: &'a str,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Sub {
                event: if add { "subscribe" } else { "unsubscribe" },
                topic,
            })
            .unwrap(),
        );
    }
    fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !self.public.open {
            return;
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.snapshot.is_some() {
            return;
        }
        self.generation += 1;
        state.generation = self.generation;
        let id = format!("book-{}", state.generation);
        #[derive(Serialize)]
        struct Params<'a> {
            r#type: &'a str,
            symbol: &'a str,
        }
        #[derive(Serialize)]
        struct Request<'a> {
            id: &'a str,
            event: &'a str,
            params: Params<'a>,
        }
        cx.send(
            self.public.socket.unwrap(),
            serde_json::to_string(&Request {
                id: &id,
                event: "request",
                params: Params {
                    r#type: "orderbook",
                    symbol: &self.native[symbol],
                },
            })
            .unwrap(),
        );
        state.snapshot = Some((id, cx.set_timer(10000.0)));
    }
    fn reset_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(state) = self.books.get_mut(symbol) {
            if let Some((_, t)) = state.snapshot.take() {
                cx.clear_timer(t);
            }
            state.last = None;
            state.bridge = true;
            state.buffer.clear();
            if state.retry.is_none() {
                state.retry = Some(cx.set_timer(1000.0));
            }
        }
    }
    fn depth(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        ts: u64,
        depth: Depth,
    ) -> Result<(), String> {
        if !depth.valid(false)
            || ts == 0
            || depth.previous.is_none_or(|p| p >= ts)
            || depth
                .symbol
                .as_deref()
                .is_some_and(|n| self.native.get(symbol).map(String::as_str) != Some(n))
        {
            self.reset_book(cx, symbol);
            return Err("Orderly invalid depth update".into());
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return Ok(());
        };
        if let Some(last) = state.last {
            if ts > last {
                if Self::apply(state, Delta { ts, depth }) {
                    cx.book_changed(symbol);
                } else {
                    self.reset_book(cx, symbol);
                    return Err("Orderly book timestamp gap".into());
                }
            }
        } else {
            if state.buffer.len() >= 2000 {
                self.reset_book(cx, symbol);
                return Err("Orderly depth buffer overflow".into());
            }
            state.buffer.push(Delta { ts, depth });
            if state.retry.is_none() {
                self.snapshot(cx, symbol);
            }
        }
        Ok(())
    }
    fn apply(state: &mut BookState, d: Delta) -> bool {
        if state.last.is_some_and(|last| d.ts <= last) {
            return true;
        }
        if state.last.is_some_and(|last| {
            d.depth.previous.is_none_or(|prev| {
                if state.bridge {
                    prev > last
                } else {
                    prev != last
                }
            })
        }) {
            return false;
        }
        for (side, levels) in [
            (BookSide::Bids, d.depth.bids),
            (BookSide::Asks, d.depth.asks),
        ] {
            for (p, q) in levels {
                state.book.apply(side, p, q);
            }
        }
        state.last = Some(d.ts);
        state.bridge = false;
        true
    }
    fn snapshot_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        env: &Envelope<'_>,
    ) -> Result<(), String> {
        let symbol = self
            .books
            .iter()
            .find(|(_, s)| s.snapshot.as_ref().is_some_and(|(id, _)| id == &env.id))
            .map(|(s, _)| s.clone());
        let Some(symbol) = symbol else {
            return Ok(());
        };
        if env.success != Some(true) {
            self.reset_book(cx, &symbol);
            return Err(format!("Orderly book snapshot: {}", env.message));
        }
        let depth: Depth = match env
            .data
            .and_then(|data| serde_json::from_str(data.get()).ok())
        {
            Some(depth) => depth,
            None => {
                self.reset_book(cx, &symbol);
                return Err("Orderly invalid book snapshot".into());
            }
        };
        let ts = depth.ts.max(depth.timestamp);
        if !depth.valid(true)
            || ts == 0
            || depth.symbol.as_deref() != self.native.get(&symbol).map(String::as_str)
        {
            self.reset_book(cx, &symbol);
            return Err("Orderly invalid book snapshot".into());
        }
        let state = self.books.get_mut(&symbol).unwrap();
        if let Some((_, t)) = state.snapshot.take() {
            cx.clear_timer(t);
        }
        state.book.load_sorted_owned(BookSide::Bids, depth.bids);
        state.book.load_sorted_owned(BookSide::Asks, depth.asks);
        state.last = Some(ts);
        state.bridge = true;
        for d in std::mem::take(&mut state.buffer) {
            if !Self::apply(state, d) {
                self.reset_book(cx, &symbol);
                return Err("Orderly buffered timestamp gap".into());
            }
        }
        cx.book_changed(&symbol);
        Ok(())
    }
    fn public_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> Result<(), String> {
        let env: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        match env.event.as_ref() {
            "ping" => {
                cx.send(self.public.socket.unwrap(), "{\"event\":\"pong\"}");
                return Ok(());
            }
            "pong" => {
                cx.store.set_latency((cx.now - self.public.ping_at) / 2.0);
                return Ok(());
            }
            "request" => return self.snapshot_message(cx, &env),
            _ => {}
        }
        if env.success == Some(false) {
            if matches!(env.event.as_ref(), "subscribe" | "unsubscribe") {
                self.public.recover = true;
                let socket = self.public.socket.unwrap();
                cx.close(socket);
                self.on_close(cx, socket, 1006);
            }
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("Orderly socket: {}", env.message),
            );
            return Ok(());
        }
        let Some(data) = env.data else {
            if env.event.is_empty()
                && let Some((native, "orderbookupdate")) = env.topic.split_once('@')
                && let Some(symbol) = symbol(native)
                && self.books.contains_key(&symbol)
            {
                self.reset_book(cx, &symbol);
                return Err("Orderly missing depth data".into());
            }
            return Ok(());
        };
        match env.topic.as_ref() {
            topic @ ("tickers" | "bbos" | "markprices") => {
                let mut invalid = false;
                json::each::<LiveTicker<'_>>(data.get(), |r| {
                    if !r.valid(topic) {
                        invalid = true;
                        return;
                    }
                    if let Some(symbol) = symbol(&r.symbol)
                        && self
                            .native
                            .get(&symbol)
                            .is_some_and(|native| native == r.symbol.as_ref())
                    {
                        self.public.payload_at = cx.now;
                        self.public.recover = false;
                        cx.store.update_ticker(&symbol, |t| match topic {
                            "tickers" => {
                                t.last = r.close.unwrap().0;
                                t.volume = r.volume.unwrap().0;
                                t.quote_volume = r.amount.unwrap().0;
                            }
                            "bbos" => {
                                t.bid = r.bid.unwrap().0;
                                t.ask = r.ask.unwrap().0;
                            }
                            "markprices" => t.mark = r.price.unwrap().0,
                            _ => {}
                        });
                    }
                })
                .map_err(|e| e.to_string())?;
                if invalid {
                    return Err("Orderly invalid live ticker fields".into());
                }
            }
            topic => {
                if let Some((native, suffix)) = topic.split_once('@')
                    && let Some(s) = symbol(native)
                {
                    if suffix == "estfundingrate" {
                        self.apply_funding(cx, native, data.get())?;
                    } else if suffix == "orderbookupdate" && self.books.contains_key(&s) {
                        let d: Depth = match serde_json::from_str(data.get()) {
                            Ok(d) => d,
                            Err(e) => {
                                self.reset_book(cx, &s);
                                return Err(e.to_string());
                            }
                        };
                        self.depth(cx, &s, env.ts, d)?;
                    } else if let Some(interval) =
                        suffix.strip_prefix("kline_").and_then(Interval::parse)
                        && self.candles.contains_key(&(s.clone(), interval))
                    {
                        let k: Kline =
                            serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                        cx.candle(&s, interval, k.candle()?);
                    }
                }
            }
        }
        Ok(())
    }
    fn apply_funding(
        &self,
        cx: &mut Cx<'_, Pending>,
        native: &str,
        data: &str,
    ) -> Result<(), String> {
        let r: FundingTicker<'_> = serde_json::from_str(data).map_err(|e| e.to_string())?;
        let symbol = symbol(native).ok_or("Orderly invalid funding topic")?;
        if r.symbol != native
            || self.native.get(&symbol).map(String::as_str) != Some(native)
            || r.funding_time < 0.0
        {
            return Err("Orderly funding topic/symbol mismatch or invalid time".into());
        }
        cx.store.update_ticker(&symbol, |t| {
            t.funding_rate = r.funding_rate;
            t.funding_time = r.funding_time;
        });
        Ok(())
    }
}
fn symbol(native: &str) -> Option<String> {
    let (base, settlement) = native_parts(native)?;
    Some(format!("{base}{settlement}"))
}
/// Broker-listed markets share USDC as quote but retain their broker suffix in
/// settlement identity: PERP_XLM_USDC_mythos -> XLMUSDC_mythos.
fn native_parts(native: &str) -> Option<(&str, &str)> {
    let (base, settlement) = native.strip_prefix("PERP_")?.split_once('_')?;
    let safe = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
    };
    if !safe(base) {
        return None;
    }
    if settlement != "USDC" && !settlement.strip_prefix("USDC_").is_some_and(safe) {
        return None;
    }
    Some((base, settlement))
}
// Orderly kline types are the TS spellings.
fn interval_valid(i: Interval) -> bool {
    use Interval::*;
    matches!(i, M1 | M5 | M15 | M30 | H1 | H4 | H12 | D1 | W1)
}

impl Venue for Orderly {
    fn cancel_needs_stored_order(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            reject_order_history_filters: true,
            cancel_all: safe_cex_engine::CancelAll::Native,
            account: safe_cex_engine::AccountLookup::AfterLoad,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            account_leverage: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.initialize_account(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.funding_open(cx, socket) {
            return;
        }
        if self.account.socket.socket == Some(socket) {
            self.private_open(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) || self.public.open {
            return;
        }
        self.public.open = true;
        self.public.ping_at = cx.now;
        self.public.supervisor.opened(cx);
        cx.send(socket, "{\"event\":\"ping\"}");
        for topic in ["tickers", "bbos", "markprices"] {
            self.subscribe(cx, topic, true);
        }
        self.books
            .retain(|symbol, _| self.native.contains_key(symbol));
        self.candles
            .retain(|(symbol, _), _| self.native.contains_key(symbol));
        for symbol in self.books.keys() {
            self.subscribe(
                cx,
                &format!("{}@orderbookupdate", self.native[symbol]),
                true,
            );
        }
        for (symbol, interval) in self.candles.keys() {
            self.subscribe(
                cx,
                &format!("{}@kline_{interval}", self.native[symbol]),
                true,
            );
        }
        let books: Vec<_> = self.books.keys().cloned().collect();
        for symbol in books {
            self.snapshot(cx, &symbol);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.funding_message(cx, socket, text) {
            return;
        }
        if self.account.socket.socket == Some(socket) {
            if let Err(e) = self.private_message(cx, text) {
                cx.notice(Level::Error, code::STREAM_MALFORMED, e);
            }
            self.trade_reconcile(cx);
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        self.public.supervisor.received(cx.now);
        if let Err(e) = self.public_message(cx, text) {
            cx.notice(Level::Error, code::STREAM_MALFORMED, e);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.funding_close(cx, socket) {
            return;
        }
        if self.account.socket.socket == Some(socket) {
            self.private_close(cx, socket);
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.open = false;
        self.public.socket = None;
        self.public.supervisor.retry(cx);
        for state in self.books.values_mut() {
            state.last = None;
            state.bridge = true;
            state.buffer.clear();
            if let Some((_, t)) = state.snapshot.take() {
                cx.clear_timer(t);
            }
            if let Some(t) = state.retry.take() {
                cx.clear_timer(t);
            }
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Some((_, context)) = Self::seed_context(pending)
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            return;
        }
        if let Some(context) = Self::candle_context(pending)
            && let Err(reason) = cx.validate_read_http_delivery(context, context)
        {
            let _ = cx.fail_context(context, reason.venue_error());
            return;
        }
        if let Pending::Account(read, request) = pending {
            if cx
                .validate_background_read_http_delivery(read.context())
                .is_ok()
            {
                self.account_item(cx, request, item);
            }
            return;
        }
        if let Pending::History(request) = pending {
            self.history_item(cx, request, item);
            return;
        }
        let result: Result<(), String> = (|| {
            match pending {
                Pending::Account(..)
                | Pending::Trade(_)
                | Pending::History(_)
                | Pending::CandleHistory { .. } => unreachable!(),
                Pending::Markets { rows, .. } => {
                    let r: MarketRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let Some((base, settlement)) = native_parts(&r.symbol) else {
                        return if r.symbol.starts_with("PERP_") {
                            Err("Orderly malformed USDC perpetual market symbol".into())
                        } else {
                            Ok(())
                        };
                    };
                    let s = format!("{base}{settlement}");
                    if rows.iter().any(|market| market.symbol == s) {
                        return Err("Orderly duplicate public market identity".into());
                    }
                    if r.base_tick <= 0.0
                        || r.quote_tick <= 0.0
                        || r.base_min < 0.0
                        || r.base_max < r.base_min
                    {
                        return Err("Orderly invalid market limits".into());
                    }

                    let max = if r.base_imr > 0.0 {
                        (1.0 / r.base_imr).round()
                    } else if r.max_leverage > 0.0 {
                        r.max_leverage
                    } else {
                        100.0
                    };
                    rows.push(Market {
                        id: format!("{base}/USDC:{settlement}"),
                        symbol: s,
                        pretty_symbol: None,
                        base: base.into(),
                        quote: "USDC".into(),
                        active: true,
                        precision: Precision {
                            amount: r.base_tick,
                            price: r.quote_tick,
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: r.base_min,
                                max: r.base_max,
                            },
                            leverage: MinMax {
                                min: if r.min_leverage > 0.0 {
                                    r.min_leverage
                                } else {
                                    1.0
                                },
                                max,
                            },
                            price: None,
                        },
                        contract_size: None,
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: None,
                        maker_fee: None,
                    });
                }
                Pending::Tickers { rows, .. } => {
                    let r: TickerRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    if let Some(m) = symbol(&r.symbol).and_then(|s| cx.store.market(&s)) {
                        rows.push(Ticker {
                            id: m.id.clone(),
                            symbol: m.symbol.clone(),
                            bid: r.close,
                            ask: r.close,
                            last: r.close,
                            mark: r.mark_price,
                            index: r.index_price,
                            percentage: if r.open > 0.0 {
                                safe_cex_engine::num::round_half_up(
                                    (r.close - r.open) / r.open * 10000.0,
                                ) / 100.0
                            } else {
                                0.0
                            },
                            open_interest: r.open_interest,
                            funding_rate: r.last_funding_rate,
                            funding_time: r.next_funding_time,
                            volume: r.volume,
                            quote_volume: r.amount,
                            mid_price: None,
                        });
                    }
                }
                Pending::Candles { rows, from, to, .. } => {
                    let r: Kline = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let c = r.candle()?;
                    if from.is_none_or(|t| c.timestamp * 1000.0 >= t)
                        && to.is_none_or(|t| c.timestamp * 1000.0 <= t)
                    {
                        rows.push(c);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            match pending {
                Pending::Markets { .. } | Pending::Tickers { .. } => {
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                }
                Pending::Candles { invalid, .. } => *invalid = true,
                Pending::Account(..)
                | Pending::Trade(_)
                | Pending::History(_)
                | Pending::CandleHistory { .. } => unreachable!(),
            }
            cx.notice(Level::Error, code::STREAM_MALFORMED, e);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        if let Some((_, context)) = Self::seed_context(&pending)
            && let Err(error) = cx.validate_background_read_http_delivery(context)
        {
            self.on_request_failed(cx, pending, &error.to_string());
            return;
        }
        if let Some(context) = Self::candle_context(&pending)
            && let Err(reason) = cx.validate_read_http_delivery(context, context)
        {
            let _ = cx.fail_context(context, reason.venue_error());
            return;
        }
        if let Pending::Account(read, request) = pending {
            let current = cx
                .validate_background_read_http_delivery(read.context())
                .is_ok();
            // Every terminal finishes the leg; a stale delivery changes nothing.
            let _ = cx.finish_http_context(read.context());
            if current {
                self.account_response(cx, request, response);
                self.trade_reconcile(cx);
            }
            return;
        }
        if let Pending::Trade(request) = pending {
            self.trade_response(cx, request, response);
            return;
        }
        if let Pending::History(request) = pending {
            self.history_response(cx, request, response);
            return;
        }
        if let Pending::CandleHistory {
            context,
            from,
            to,
            limit,
        } = pending
        {
            match candle_history(response, from, to, limit) {
                Ok(data) => {
                    let _ = cx.done_context(&context, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(&context, error);
                }
            };
            return;
        }
        if let Err(e) = checked(response) {
            self.on_request_failed(cx, pending, &e);
            return;
        }
        match pending {
            Pending::Markets {
                context,
                rows,
                invalid: false,
            } => {
                if rows.is_empty() {
                    if cx.finish_http_context(&context).is_ok() {
                        self.seed_backoff.failed(cx, Seed::Markets);
                    }
                    return;
                }
                let mut native = HashMap::new();
                for m in &rows {
                    let Some(settlement) = m.symbol.strip_prefix(&m.base) else {
                        if cx.finish_http_context(&context).is_ok() {
                            self.retry_seed(cx, Seed::Markets, "Orderly invalid market identity");
                        }
                        return;
                    };
                    let actual = format!("PERP_{}_{settlement}", m.base);
                    if native_parts(&actual).is_none()
                        || native.insert(m.symbol.clone(), actual).is_some()
                    {
                        if cx.finish_http_context(&context).is_ok() {
                            self.retry_seed(
                                cx,
                                Seed::Markets,
                                "Orderly duplicate or invalid market identity",
                            );
                        }
                        return;
                    }
                }
                self.native = native;
                cx.store.set_markets(rows);
                if cx.finish_http_context(&context).is_err() {
                    return;
                }
                self.seed_backoff.succeeded(cx, &Seed::Markets);
                self.account_start(cx);
                self.seed(cx, Seed::Tickers);
            }
            Pending::Tickers {
                context,
                rows,
                invalid: false,
            } => {
                if rows.is_empty() {
                    if cx.finish_http_context(&context).is_ok() {
                        self.retry_seed(
                            cx,
                            Seed::Tickers,
                            "Orderly ticker list has no usable rows",
                        );
                    }
                    return;
                }
                cx.store.set_tickers(rows);
                if cx.finish_http_context(&context).is_err() {
                    return;
                }
                self.seed_backoff.succeeded(cx, &Seed::Tickers);
                self.connect_public(cx);
                self.funding_start(cx);
            }
            Pending::Candles {
                context,
                mut rows,
                invalid: false,
                ..
            } => {
                rows.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                let _ = cx.done_context(&context, serde_json::to_string(&rows).unwrap());
            }
            invalid => self.on_request_failed(cx, invalid, "Orderly invalid response rows"),
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: VenueError,
    ) {
        if let Some(context) = Self::candle_context(&pending) {
            let _ = cx.fail_context(context, error);
        } else {
            self.on_sign_error(cx, pending, error);
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::History(request) => self.history_failed(cx, request, error),
            pending => match Self::candle_context(&pending) {
                Some(context) => {
                    let _ = cx.fail_context(context, error);
                }
                None => self.on_request_failed(cx, pending, &error.message),
            },
        }
    }
    /// A provider refusal of a private read: history is strict, and the
    /// startup Info read is not re-armed (the engine has already reported it).
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        match pending {
            Pending::History(request) => self.history_refused(cx, request, error),
            Pending::Account(read, request) if request.is_info() => {
                let _ = cx.finish_http_context(read.context());
            }
            pending => self.on_sign_failed(cx, pending, &error.message),
        }
    }
    /// A malformed stream was delivered, so its custody is checked first: a
    /// stale or superseded leg only closes its Context and changes nothing.
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _status: u16,
        _headers: &str,
        error: &str,
    ) {
        match pending {
            Pending::Account(read, request) => {
                let current = cx
                    .validate_background_read_http_delivery(read.context())
                    .is_ok();
                let _ = cx.finish_http_context(read.context());
                if current {
                    self.account_failed(cx, request, error);
                }
            }
            Pending::History(mut request) => {
                if request.valid_delivery(cx) {
                    self.history_failed(cx, request, VenueError::new(error));
                } else {
                    request.release(cx);
                }
            }
            pending => self.on_http_failed(
                cx,
                pending,
                VenueError::new(error).kind(safe_cex_engine::ErrorKind::Internal),
            ),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Some((kind, context)) = Self::seed_context(&pending) {
            if cx.finish_http_context(context).is_ok() {
                self.retry_seed(cx, kind, error);
            }
            return;
        }
        match pending {
            Pending::Account(read, request) => {
                let _ = cx.finish_http_context(read.context());
                self.account_failed(cx, request, error);
            }
            Pending::Trade(request) => {
                let error = safe_cex_engine::VenueError::new(error)
                    .kind(safe_cex_engine::ErrorKind::Network);
                if request.is_read() {
                    self.trade_failed(cx, request, error)
                } else {
                    self.trade_lost(
                        cx,
                        request,
                        error.outcome(safe_cex_engine::Outcome::Unknown),
                    )
                }
            }
            Pending::History(request) => self.history_failed(cx, request, VenueError::new(error)),
            Pending::Markets { .. } | Pending::Tickers { .. } => unreachable!(),
            Pending::Candles { context, .. } | Pending::CandleHistory { context, .. } => {
                let _ = cx.fail_context(&context, error);
            }
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.funding_timer(cx, timer) {
            return;
        }
        if self.history_timer(cx, timer) {
            return;
        }
        if self.trade_timer(cx, timer) {
            return;
        }
        if self.account_timer(cx, timer) {
            return;
        }
        if let Some(kind) = self.seed_backoff.on_timer(timer) {
            self.seed(cx, kind);
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            SocketTick::Unhandled => {}
            SocketTick::Reconnect => return self.connect_public(cx),
            SocketTick::Ping if self.public.payload_stale(cx.now) => {
                self.public.recover = true;
                return self.drop_public(cx);
            }
            SocketTick::Ping => {
                if let Some(socket) = self.public.socket {
                    self.public.ping_at = cx.now;
                    cx.send(socket, "{\"event\":\"ping\"}");
                }
                return;
            }
            SocketTick::TimedOut(timeout) => {
                self.public.recover =
                    timeout == Timeout::Connect || self.public.payload_stale(cx.now);
                return self.drop_public(cx);
            }
            SocketTick::Handled => return,
        }
        let timed = self
            .books
            .iter()
            .find(|(_, s)| s.snapshot.as_ref().is_some_and(|(_, t)| *t == timer))
            .map(|(s, _)| s.clone());
        if let Some(symbol) = timed {
            self.reset_book(cx, &symbol);
            return;
        }
        let retry = self
            .books
            .iter()
            .find(|(_, s)| s.retry == Some(timer))
            .map(|(s, _)| s.clone());
        if let Some(symbol) = retry {
            self.books.get_mut(&symbol).unwrap().retry = None;
            self.snapshot(cx, &symbol);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if cx.store.loaded().markets && !self.native.contains_key(&symbol) {
                    return;
                }
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_add(1);
                    return;
                }
                if !self.public_topic_available(cx) {
                    return;
                }
                self.books.insert(
                    symbol.clone(),
                    BookState {
                        book: Book::default(),
                        subscribers: 1,
                        generation: 0,
                        last: None,
                        bridge: true,
                        buffer: vec![],
                        snapshot: None,
                        retry: None,
                    },
                );
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@orderbookupdate"), true);
                    self.snapshot(cx, &symbol);
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(s) = self.books.get_mut(&symbol) {
                    s.subscribers = s.subscribers.saturating_sub(1);
                    if s.subscribers > 0 {
                        return;
                    }
                } else {
                    return;
                }
                let s = self.books.remove(&symbol).unwrap();
                if let Some((_, t)) = s.snapshot {
                    cx.clear_timer(t);
                }
                if let Some(t) = s.retry {
                    cx.clear_timer(t);
                }
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@orderbookupdate"), false);
                }
            }
            Command::SubscribeCandles { symbol, interval } => {
                if !interval_valid(interval) {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        interval.unsupported("Orderly"),
                    );
                    return;
                }
                if cx.store.loaded().markets && !self.native.contains_key(&symbol) {
                    return;
                }
                if !self.candles.contains_key(&(symbol.clone(), interval))
                    && !self.public_topic_available(cx)
                {
                    return;
                }
                let n = self.candles.entry((symbol.clone(), interval)).or_default();
                *n = n.saturating_add(1);
                if *n == 1 && self.native.contains_key(&symbol) {
                    self.subscribe(
                        cx,
                        &format!("{}@kline_{interval}", self.native[&symbol]),
                        true,
                    );
                }
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval);
                if let Some(n) = self.candles.get_mut(&key) {
                    *n = n.saturating_sub(1);
                    if *n > 0 {
                        return;
                    }
                } else {
                    return;
                }
                self.candles.remove(&key);
                if let Some(native) = self.native.get(&symbol) {
                    self.subscribe(cx, &format!("{native}@kline_{interval}"), false);
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval,
                limit,
                from_ms,
                to_ms,
            } => {
                let Some(native) = self.native.get(&symbol) else {
                    complete_local_read(cx, Err("Orderly unknown symbol".into()));
                    return;
                };
                if !interval_valid(interval) {
                    complete_local_read(cx, Err(interval.unsupported("Orderly").into()));
                    return;
                }
                if limit == Some(0)
                    || [from_ms, to_ms]
                        .into_iter()
                        .flatten()
                        .any(|n| !n.is_finite() || !(0.0..=253402300799000.0).contains(&n))
                    || from_ms.zip(to_ms).is_some_and(|(a, b)| a > b)
                {
                    complete_local_read(cx, Err("Orderly invalid candle parameters".into()));
                    return;
                }
                if from_ms.is_some() || to_ms.is_some() {
                    let limit = limit.unwrap_or(500).min(1000) as usize;
                    let from = from_ms.unwrap_or(0.0);
                    let to = to_ms.unwrap_or(cx.now);
                    if from > to {
                        complete_local_read(cx, Ok("[]".into()));
                        return;
                    }
                    // Both bounds are mandatory on the native history endpoint.
                    let mut path = format!(
                        "/v1/tv/kline_history?symbol={native}&resolution={interval}&limit={limit}"
                    );
                    path.push_str(&format!(
                        "&from={:.0}&to={:.0}",
                        (from / 1000.0).floor(),
                        (to / 1000.0).floor()
                    ));
                    let Ok(context) = cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    }) else {
                        return;
                    };
                    self.candle_request(
                        cx,
                        format!("{}{path}", self.rest()),
                        false,
                        Pending::CandleHistory {
                            context,
                            from: from_ms,
                            to: to_ms,
                            limit,
                        },
                    );
                    return;
                }
                let path = format!(
                    "/v1/kline?symbol={native}&type={interval}&limit={}",
                    limit.unwrap_or(500).min(1000)
                );
                let Ok(context) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) else {
                    return;
                };
                self.candle_request(
                    cx,
                    format!("{}{path}", self.rest()),
                    true,
                    Pending::Candles {
                        context,
                        rows: vec![],
                        invalid: false,
                        from: from_ms,
                        to: to_ms,
                    },
                );
            }
            Command::SetTickerSymbols { .. } => {}
            Command::Query { command, query } => self.history_command(cx, command, query),
            Command::FetchAccount { command: _ } => {
                let result: Result<String, String> = self.account_identity();
                complete_local_read(cx, result.map_err(Into::into));
            }
            trade => self.trade_command(cx, trade),
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.last.is_some())
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(
            self.books
                .iter()
                .map(|(s, state)| (s.as_str(), &state.book)),
        )
    }
}

#[cfg(test)]
#[path = "../tests/unit/public_seed_custody.rs"]
mod public_seed_custody;
