//! Bitrue stablecoin perpetuals, driven entirely by host events.
mod account;
mod codes;
mod history;
pub mod sign;
mod trading;
mod wire;
use safe_cex_engine::Level;
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason, SlotPolicy, Slots};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{
    Context, Egress, Operation, Policy, RetryPolicy, Scope, SessionPolicy,
};
use safe_cex_engine::selected_read::{
    AdmittedRead, ReadLeg, ReadLegPolicy, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::time;
use safe_cex_engine::{
    Command, Cx, Method, PublicFeedPolicy, Response, SocketChannel, SocketId, TimerId, Venue,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use wire::{CandleWire, Contract, ContractKey, Message, Payload};
pub(crate) const REST: &str = "https://fapi.bitrue.com";
const PUBLIC: &str = "wss://fmarket-ws.bitrue.com/kline-api/ws";
/// ```compile_fail
/// let _ = safe_cex_venue_bitrue::Options { key: None, ..Default::default() };
/// ```
#[derive(Clone, Debug)]
pub struct Options {
    pub read_credentials: Option<Identity>,
    pub public_data_only: bool,
    pub credentials: Option<Identity>,
    pub recv_window: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            read_credentials: None,
            public_data_only: false,
            credentials: None,
            recv_window: 20_000,
        }
    }
}
pub struct Pending {
    pub wire: sign::Request,
    pub job: Job,
    pub signature_resent: bool,
    pub attempts: u8,
    custody: Option<Custody>,
}
/// The engine authority a released request holds until it settles.
enum Custody {
    /// A Background account read; native retries reuse its authority.
    Read(AdmittedRead),
    /// One physical page under a command's original Read root.
    Leg(ReadLeg),
    /// A listen-key Session operation, or a keyless catalog read.
    Context(Context),
}
impl Pending {
    /// A trading call: its Plan Context travels in the request itself.
    pub(crate) fn trade(
        method: Method,
        path: String,
        body: Option<String>,
        request: trading::Request,
    ) -> Self {
        Self {
            wire: sign::Request {
                method,
                path,
                body,
                at: None,
                floor: None,
            },
            job: Job::Trade(request),
            signature_resent: false,
            attempts: 0,
            custody: None,
        }
    }
    pub(crate) fn into_trade(self) -> Option<trading::Request> {
        match self.job {
            Job::Trade(request) => Some(request),
            _ => None,
        }
    }
}
impl Custody {
    fn context(&self) -> &Context {
        match self {
            Self::Read(read) => read.context(),
            Self::Leg(leg) => leg.context(),
            Self::Context(context) => context,
        }
    }
}
pub enum Job {
    Markets {
        rows: Vec<Market>,
        contracts: HashMap<String, ContractInfo>,
        malformed: HashSet<String>,
    },
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
}
#[derive(Clone, Copy)]
pub struct ContractInfo {
    pub min_order_money: f64,
    pub max_limit_volume: f64,
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
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: None,
                backoff_base_ms: 1_000.0,
                ..Config::default()
            }),
        }
    }
}
#[derive(Default)]
struct DepthState {
    book: Book,
    subscribers: u32,
    received: bool,
    fallback: Option<TimerId>,
    fallback_active: bool,
}
#[derive(Clone)]
enum Channel {
    Ticker(u32),
    Book(String),
    Candle(String, Interval),
}
enum SocketReadKind {
    Candles {
        from: Option<f64>,
        to: Option<f64>,
        limit: u32,
    },
    Trades {
        limit: u32,
    },
}
struct SocketRead {
    context: Context,
    symbol: String,
    kind: SocketReadKind,
    timer: TimerId,
}
pub struct Bitrue {
    options: Options,
    account: account::Account,
    trading: trading::Trading,
    contracts: HashMap<String, ContractInfo>,
    public: Connection,
    channels: HashMap<String, Channel>,
    books: HashMap<String, DepthState>,
    candles: HashMap<(String, Interval), u32>,
    reads: HashMap<String, Vec<SocketRead>>,
    startup_retry: safe_cex_engine::startup::Backoff<()>,
    // Contracts rejected as malformed; owned exposure there holds the snapshot.
    malformed_markets: HashSet<String>,
    reread_stalls: u8,
    deferred: Vec<Command>,
}
#[derive(Serialize)]
struct Params<'a> {
    channel: &'a str,
    cb_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none", rename = "endIdx")]
    end_idx: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "pageSize")]
    page_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top: Option<u32>,
}
#[derive(Serialize)]
struct Subscription<'a> {
    event: &'a str,
    params: Params<'a>,
}
fn ws_symbol(symbol: &str) -> String {
    format!(
        "e_{}",
        symbol
            .strip_prefix("E-")
            .unwrap_or(symbol)
            .replace('-', "")
            .to_ascii_lowercase()
    )
}
fn interval(i: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match i {
        M1 => "1min",
        M5 => "5min",
        M15 => "15min",
        M30 => "30min",
        H1 => "60min",
        D1 => "1day",
        W1 => "1week",
        _ => return None,
    })
}
fn topic(symbol: &str, suffix: &str) -> String {
    format!("market_{}_{suffix}", ws_symbol(symbol))
}
// `ds` is a civil Singapore time, not the machine's local timezone.
fn singapore_seconds(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let part = |a, b| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, h, min, sec) = (
        part(0, 4)?,
        part(5, 7)?,
        part(8, 10)?,
        part(11, 13)?,
        part(14, 16)?,
        part(17, 19)?,
    );
    if !time::days_in_month(y, m).is_some_and(|last| (1..=last).contains(&d))
        || h > 23
        || min > 59
        || sec > 59
        || h < 0
        || min < 0
        || sec < 0
    {
        return None;
    }
    Some((time::days_from_civil(y, m, d) * 86400 + h * 3600 + min * 60 + sec - 28800) as f64)
}
fn candle(row: &CandleWire<'_>, multiplier: f64) -> Option<Candle> {
    let timestamp = if row.ds.is_empty() {
        let t = row.id.0;
        if t > 1e12 { t / 1000.0 } else { t }
    } else {
        singapore_seconds(&row.ds)?
    };
    (timestamp > 0.0).then_some(Candle {
        timestamp,
        open: row.open.0,
        high: row.high.0,
        low: row.low.0,
        close: row.close.0,
        volume: row.vol.0 * multiplier,
    })
}
impl Bitrue {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            account: account::Account::default(),
            trading: trading::Trading::default(),
            contracts: HashMap::new(),
            public: Connection::default(),
            channels: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            reads: HashMap::new(),
            startup_retry: Default::default(),
            malformed_markets: HashSet::new(),
            reread_stalls: 0,
            deferred: Vec::new(),
        }
    }
    fn authenticated(&self) -> bool {
        !self.options.public_data_only && self.options.credentials.is_some()
    }
    fn resolve_symbol(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Option<String> {
        if cx.store.market(symbol).is_some() {
            return Some(symbol.to_owned());
        }
        let flat = |s: &str| {
            s.strip_prefix("E-")
                .or_else(|| s.strip_prefix("E_"))
                .unwrap_or(s)
                .replace(['-', '_', '/'], "")
                .to_ascii_uppercase()
        };
        let key = flat(symbol);
        cx.store
            .markets()
            .iter()
            .find(|m| flat(&m.symbol) == key)
            .map(|m| m.symbol.clone())
    }
    fn send(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        at: Option<ArrayAt>,
        job: Job,
    ) {
        let pending = Pending {
            wire: sign::Request {
                method,
                path: path.to_owned(),
                body,
                at,
                floor: None,
            },
            job,
            signature_resent: false,
            attempts: 0,
            custody: None,
        };
        self.issue(cx, pending);
    }
    fn slots(&self) -> Slots<Identity> {
        Slots::new(
            self.options.credentials.clone(),
            self.options.read_credentials.clone(),
            SlotPolicy {
                secondary_requires_primary: true,
            },
        )
    }
    /// Exact delivery precedes any decoding or Store effect.
    fn delivered(cx: &Cx<'_, Pending>, pending: &Pending) -> bool {
        match (&pending.custody, account::Request::root(&pending.job)) {
            (Some(Custody::Leg(leg)), Some(root)) => root.validate_delivery(cx, leg).is_ok(),
            (Some(custody), _) => cx.validate_http_context(custody.context()).is_ok(),
            (None, _) => true,
        }
    }
    fn url(path: &str) -> String {
        if path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{REST}{path}")
        }
    }
    fn recipe(
        &self,
        identity: &Identity,
        wire: &sign::Request,
    ) -> Box<dyn safe_cex_engine::SignStep> {
        Box::new(sign::dispatch::Http::new(
            identity.clone(),
            wire.method,
            Self::url(&wire.path),
            wire.body.clone(),
            self.options.recv_window,
            wire.floor,
        ))
    }
    fn read_policy() -> ReadPolicy {
        ReadPolicy {
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        }
    }
    /// Admit and queue one physical request under the authority its job needs.
    fn issue(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        if let Some(root) = account::Request::root(&pending.job) {
            let leg = match root.leg(
                cx,
                ReadLegPolicy {
                    timeout_ms: 20_000,
                    pacing: None,
                },
            ) {
                Ok(leg) => leg,
                Err(_) => {
                    return self.on_request_failed(
                        cx,
                        pending,
                        "Bitrue managed read lifecycle is unavailable",
                    );
                }
            };
            return self.queue_leg(cx, leg, pending);
        }
        match &pending.job {
            Job::Trade(_) => unreachable!("trading calls queue under their Plan"),
            Job::Account(account::Request::Listen { .. } | account::Request::Keepalive { .. }) => {
                self.issue_session(cx, pending)
            }
            Job::Markets { .. } => {
                let selected = SelectedRead::select(
                    &self.slots(),
                    ReadRole::Primary {
                        reason: PrimaryReason::Catalog,
                        scope: None,
                    },
                );
                // Keys sign the catalog even in public-data-only mode, as native.
                if selected.is_ok() {
                    self.issue_read(cx, selected, pending)
                } else {
                    self.issue_public(cx, pending)
                }
            }
            Job::Account(account::Request::Orders { primary: true, .. }) => {
                let selected = SelectedRead::select(
                    &self.slots(),
                    ReadRole::Primary {
                        reason: PrimaryReason::PostUnknownOrderRead,
                        scope: Some(safe_cex_engine::AccountReadScope::Orders),
                    },
                );
                self.issue_read(cx, selected, pending)
            }
            Job::Account(_) | Job::History(_) => {
                let selected = SelectedRead::select(&self.slots(), ReadRole::Read { scope: None });
                self.issue_read(cx, selected, pending)
            }
        }
    }
    fn issue_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        selected: Result<
            SelectedRead,
            safe_cex_engine::credential_selection::CredentialUnavailable,
        >,
        pending: Pending,
    ) {
        match SelectedRead::scoped(cx, selected, Scope::Background, Self::read_policy()) {
            Ok(read) => self.queue_read(cx, read, pending),
            Err(_) => {
                self.on_request_failed(cx, pending, "Bitrue managed read lifecycle is unavailable")
            }
        }
    }
    fn queue_read(&mut self, cx: &mut Cx<'_, Pending>, read: AdmittedRead, pending: Pending) {
        let request = ReadRequest {
            method: pending.wire.method,
            url: Self::url(&pending.wire.path),
            body: pending.wire.body.clone(),
            stream: pending.wire.at,
        };
        let wire = pending.wire.clone();
        let queued = read.queue(
            cx,
            request,
            |identity| self.recipe(identity, &wire),
            |read| Pending {
                custody: Some(Custody::Read(read)),
                ..pending
            },
        );
        if let Err((pending, error)) = queued {
            self.on_request_failed(cx, pending, &error.venue_error().message);
        }
    }
    fn queue_leg(&mut self, cx: &mut Cx<'_, Pending>, leg: ReadLeg, pending: Pending) {
        let request = ReadRequest {
            method: pending.wire.method,
            url: Self::url(&pending.wire.path),
            body: pending.wire.body.clone(),
            stream: pending.wire.at,
        };
        let wire = pending.wire.clone();
        let queued = leg.queue(
            cx,
            request,
            |identity| self.recipe(identity, &wire),
            |leg| Pending {
                custody: Some(Custody::Leg(leg)),
                ..pending
            },
        );
        if let Err((pending, error)) = queued {
            self.on_request_failed(cx, pending, &error.venue_error().message);
        }
    }
    /// The keyless catalog keeps native's explicit Primary route and headers.
    fn issue_public(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let policy = Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        };
        let Ok(context) = cx.http_context_with_egress(Scope::Background, policy, Egress::Primary)
        else {
            return self.on_request_failed(
                cx,
                pending,
                "Bitrue managed read lifecycle is unavailable",
            );
        };
        let (method, url, body, at) = (
            pending.wire.method,
            Self::url(&pending.wire.path),
            pending.wire.body.clone(),
            pending.wire.at,
        );
        let pending = Pending {
            custody: Some(Custody::Context(context.clone())),
            ..pending
        };
        match cx.request_managed_with_headers(
            context,
            None,
            method,
            url,
            sign::dispatch::HEADERS,
            body,
            pending,
        ) {
            Ok(id) => {
                if let Some(at) = at {
                    cx.stream_response(id, at);
                }
            }
            Err((pending, error)) => {
                self.on_request_failed(cx, pending, &error.venue_error().message)
            }
        }
    }
    /// A listen-key create opens its Session; its native retries and the
    /// keepalive are further operations of that Session.
    fn issue_session(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending) {
        let Ok((selection, identity)) = self
            .slots()
            .select(CredentialRole::SessionAuth)
            .map(|(selection, identity)| (selection, identity.clone()))
        else {
            return self.on_request_failed(cx, pending, "Bitrue private credentials required");
        };
        let credential = selection.credential(identity.credential.clone());
        let create = matches!(pending.job, Job::Account(account::Request::Listen { .. }));
        // A fresh create opens the Session; its native retries and -1022
        // re-sign stay operations of that same Session generation (D1).
        let fresh = create && pending.attempts == 0 && !pending.signature_resent;
        if fresh || self.account.session.is_none() {
            self.retire_session(cx);
            match cx.open_session(SessionPolicy {
                credential: Some(credential.clone()),
                // Bitrue listen keys expire 60 minutes after their last
                // keepalive, so an extra create is harmless. One-write guard
                // off: a lost create would otherwise block native's retry.
                repeat_safe: true,
                private_egress: None,
            }) {
                Ok(session) => self.account.session = Some(session),
                Err(_) => {
                    return self.on_request_failed(
                        cx,
                        pending,
                        "Bitrue listen key session is unavailable",
                    );
                }
            }
        }
        let session = self.account.session.clone().expect("opened above");
        let Ok(context) = cx.session_context(
            &session,
            Policy {
                operation: Operation::Mutation,
                credential: Some(credential),
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) else {
            return self.on_request_failed(cx, pending, "Bitrue listen key session is unavailable");
        };
        let recipe = self.recipe(&identity, &pending.wire);
        let (method, url, body) = (
            pending.wire.method,
            Self::url(&pending.wire.path),
            pending.wire.body.clone(),
        );
        let pending = Pending {
            custody: Some(Custody::Context(context.clone())),
            ..pending
        };
        if let Err((pending, error)) =
            cx.request_signed_managed(context, None, method, url, body, recipe, pending)
        {
            self.on_request_failed(cx, pending, &error.venue_error().message);
        }
    }
    pub(crate) fn retire_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(session) = self.account.session.take() {
            // Nothing parks on a listen-key session: it never has waiters.
            let _ = cx.retire_session(&session);
        }
    }
    /// Settle whatever authority the request held.
    fn finish_custody(cx: &mut Cx<'_, Pending>, pending: &mut Pending) {
        if let Some(custody) = pending.custody.take() {
            let _ = cx.finish_http_context(custody.context());
        }
    }
    /// A native resend: a Background read retries under its own authority;
    /// a root page or a Session operation is reissued as a new one.
    fn reissue(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending) {
        Self::reset_job(&mut pending.job);
        match pending.custody.take() {
            Some(Custody::Read(read)) => {
                match read.retry(
                    cx,
                    safe_cex_engine::request_context::RetryEvidence::ReadFailure,
                    (),
                ) {
                    Ok((read, ())) => self.queue_read(cx, read, pending),
                    Err(refusal) => {
                        let _ = cx.finish_http_context(refusal.read.context());
                        self.on_request_failed(cx, pending, "Bitrue read retry refused");
                    }
                }
            }
            custody => {
                if let Some(custody) = custody {
                    let _ = cx.finish_http_context(custody.context());
                }
                self.issue(cx, pending);
            }
        }
    }
    fn reset_job(job: &mut Job) {
        match job {
            Job::Markets {
                rows,
                contracts,
                malformed,
            } => {
                rows.clear();
                contracts.clear();
                malformed.clear();
            }
            Job::Account(r) => r.reset(),
            Job::History(r) => r.reset(),
            Job::Trade(_) => {}
        }
    }
    fn markets(&mut self, cx: &mut Cx<'_, Pending>) {
        self.send(
            cx,
            Method::Get,
            "/fapi/v1/contracts",
            None,
            Some(ArrayAt::Root),
            Job::Markets {
                rows: Vec::new(),
                contracts: HashMap::new(),
                malformed: HashSet::new(),
            },
        );
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        // Suppressed public feeds never open; supervising them would only churn retries.
        if cx.public_feed_policy() == PublicFeedPolicy::Direct {
            self.public.supervisor.connecting(cx);
        }
        self.public.socket = Some(
            cx.connect(safe_cex_engine::SocketSpec::new(PUBLIC).channel(SocketChannel::Public)),
        );
    }
    fn subscription(
        &self,
        cx: &mut Cx<'_, Pending>,
        event: &str,
        channel: &str,
        end: Option<f64>,
        size: Option<u32>,
        top: Option<u32>,
    ) {
        if let Some(socket) = self.public.socket.filter(|_| self.public.open) {
            let text = serde_json::to_string(&Subscription {
                event,
                params: Params {
                    channel,
                    cb_id: "engine",
                    end_idx: end.map(|v| (v / 1000.0).floor().to_string()),
                    page_size: size,
                    top,
                },
            })
            .unwrap();
            cx.send(socket, text);
        }
    }
    fn subscribe_book(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let channel = topic(symbol, "simple_depth_step0");
        self.channels
            .insert(channel.clone(), Channel::Book(symbol.to_owned()));
        self.subscription(cx, "sub", &channel, None, None, None);
        if let Some(depth) = self.books.get_mut(symbol) {
            depth.received = false;
            depth.fallback_active = false;
            if let Some(t) = depth.fallback.take() {
                cx.clear_timer(t);
            }
            depth.fallback = Some(cx.set_timer(3000.0));
        }
        cx.store.update_ticker(symbol, |t| {
            t.bid = 0.0;
            t.ask = 0.0;
        });
    }
    fn socket_read_policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 5000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    fn send_socket_read(&self, cx: &mut Cx<'_, Pending>, channel: &str) {
        let Some(read) = self.reads.get(channel).and_then(|v| v.first()) else {
            return;
        };
        if cx.validate_http_context(&read.context).is_err() {
            return;
        }
        let Some(socket) = self.public.socket.filter(|_| self.public.open) else {
            return;
        };
        let (end_idx, page_size, top) = match &read.kind {
            SocketReadKind::Candles { to, limit, .. } => (
                to.map(|v| (v / 1000.0).floor().to_string()),
                Some((*limit).min(300)),
                None,
            ),
            SocketReadKind::Trades { limit } => (None, None, Some(*limit)),
        };
        let id = format!("engine-{}", read.timer.0);
        cx.send(
            socket,
            serde_json::to_string(&Subscription {
                event: "req",
                params: Params {
                    channel,
                    cb_id: &id,
                    end_idx,
                    page_size,
                    top,
                },
            })
            .unwrap(),
        );
    }
    fn public_read(&mut self, cx: &mut Cx<'_, Pending>, channel: String, read: SocketRead) {
        if self.reads.values().map(Vec::len).sum::<usize>() >= 32 {
            cx.clear_timer(read.timer);
            let _ = cx.fail_context(&read.context, "Too many Bitrue public requests");
            return;
        }
        let queue = self.reads.entry(channel.clone()).or_default();
        queue.push(read);
        if queue.len() == 1 {
            self.send_socket_read(cx, &channel);
        }
    }
    fn public_trades(&mut self, cx: &mut Cx<'_, Pending>, symbol: String, limit: Option<u32>) {
        let Ok(context) = cx.admit_http_command(Self::socket_read_policy()) else {
            return;
        };
        let Some(symbol) = self.resolve_symbol(cx, &symbol) else {
            let _ = cx.fail_context(&context, "Unknown Bitrue market");
            return;
        };
        if !self.public.open {
            let _ = cx.fail_context(&context, "Bitrue public socket not connected");
            return;
        }
        let limit = limit.unwrap_or(100).clamp(1, 300);
        let timer = cx.set_timer(5000.0);
        self.public_read(
            cx,
            topic(&symbol, "trade_ticker"),
            SocketRead {
                context,
                symbol,
                kind: SocketReadKind::Trades { limit },
                timer,
            },
        );
    }
    fn reply(&mut self, cx: &mut Cx<'_, Pending>, message: &Message<'_>, text: &str) {
        let Some(queue) = self.reads.get_mut(message.channel.as_ref()) else {
            return;
        };
        if queue.is_empty() {
            return;
        }
        if !message.cb_id.is_empty() && message.cb_id != format!("engine-{}", queue[0].timer.0) {
            return;
        }
        if cx.validate_http_context(&queue[0].context).is_err() {
            return;
        }
        let read = queue.remove(0);
        {
            cx.clear_timer(read.timer);
            let multiplier = cx
                .store
                .market(&read.symbol)
                .map_or(1.0, |m| m.precision.amount);
            let result = match read.kind {
                SocketReadKind::Trades { limit } => {
                    history::trade_result(&read.symbol, multiplier, text, limit)
                }
                SocketReadKind::Candles { from, to, limit } => {
                    if !message.status.is_empty() && message.status != "ok" {
                        Err("Bitrue candle request rejected".to_owned())
                    } else {
                        message
                            .data
                            .ok_or_else(|| "Missing Bitrue candle data".to_owned())
                            .and_then(|data| {
                                let rows: Vec<CandleWire<'_>> =
                                    serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                                let candles = rows
                                    .iter()
                                    .map(|row| {
                                        candle(row, multiplier).ok_or_else(|| {
                                            "Malformed Bitrue candle timestamp".to_owned()
                                        })
                                    })
                                    .collect::<Result<Vec<_>, _>>()?;
                                let mut result: Vec<_> = candles
                                    .into_iter()
                                    .filter(|c| {
                                        from.is_none_or(|f| c.timestamp * 1000.0 >= f)
                                            && to.is_none_or(|t| c.timestamp * 1000.0 <= t)
                                    })
                                    .collect();
                                result.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
                                result.dedup_by(|a, b| a.timestamp == b.timestamp);
                                if result.len() > limit as usize {
                                    result.drain(..result.len() - limit as usize);
                                }
                                serde_json::to_string(&result).map_err(|e| e.to_string())
                            })
                    }
                }
            };
            match result {
                Ok(data) => {
                    let _ = cx.done_context(&read.context, data);
                }
                Err(error) => {
                    let _ = cx.fail_context(&read.context, error);
                }
            };
        }
        self.reads.retain(|_, v| !v.is_empty());
        self.send_socket_read(cx, &message.channel);
    }
}
impl Venue for Bitrue {
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match &pending.job {
            Job::Trade(request) => trading::trade_reconciliation(request),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
    }
    fn on_http_preflight_ready(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        preflight: &safe_cex_engine::request_context::Preflight,
    ) {
        self.trading_preflight_ready(cx, preflight);
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) -> Option<safe_cex_engine::request_context::Preflight> {
        self.trading_retired(owner)
    }
    /// An uncertain or absent-order seal: restore the retired job's rows and
    /// reconcile through a Primary open-order snapshot.
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.reconcile_trading(cx);
    }

    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            cancel_all: safe_cex_engine::CancelAll::EachPlacedSymbol,
            client_order_lookup: true,
            order_history: false,
            income_history: false,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            max_leverage: safe_cex_engine::MaxLeverage::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = &self.options.read_credentials
            && read.validate().is_err()
        {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Secondary read credentials require a nonempty key and secret",
            );
            return;
        }
        self.markets(cx);
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
        let markets: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for symbol in markets {
            // A refreshed catalogue can re-index markets; ticker rows keep their slots.
            let Some(ticker) = cx.store.ensure_ticker(&symbol) else {
                continue;
            };
            let channel = topic(&symbol, "ticker");
            self.channels
                .insert(channel.clone(), Channel::Ticker(ticker));
            self.subscription(cx, "sub", &channel, None, None, None);
        }
        for symbol in self.books.keys().cloned().collect::<Vec<_>>() {
            self.subscribe_book(cx, &symbol);
        }
        for (symbol, i) in self.candles.keys() {
            let Some(mapped) = interval(*i) else {
                continue;
            };
            let channel = topic(symbol, &format!("kline_{mapped}"));
            self.channels
                .insert(channel.clone(), Channel::Candle(symbol.clone(), *i));
            self.subscription(cx, "sub", &channel, None, None, None);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account_message(cx, socket, text) {
            return;
        }
        if self.public.socket != Some(socket) || !self.public.open {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        self.public.supervisor.received(cx.now);
        if message.ping.is_some() || message.event == "ping" {
            cx.send(socket, "pong");
            if let Some(ping) = message.ping {
                cx.send(socket, format!(r#"{{"pong":{}}}"#, ping.get()));
            }
            return;
        }
        if message.event_rep == "rep" {
            self.reply(cx, &message, text);
            return;
        }
        let data = match message.tick {
            Some(tick) => tick,
            None => {
                let Some(data) = message.data else {
                    return;
                };
                let Ok(tick) = serde_json::from_str::<Payload<'_>>(data.get()) else {
                    return;
                };
                tick
            }
        };
        match self.channels.get(message.channel.as_ref()) {
            Some(Channel::Ticker(index)) => {
                let multiplier = cx
                    .store
                    .tickers()
                    .get(*index as usize)
                    .and_then(|t| cx.store.market(&t.symbol))
                    .map_or(1.0, |m| m.precision.amount);
                cx.store.update_ticker_at(*index, |t| {
                    t.last = data.close.unwrap_or_default().0;
                    t.mark = data.close.unwrap_or_default().0;
                    t.percentage = data.rose.0 * 100.0;
                    t.volume = data.vol.0 * multiplier;
                    t.quote_volume = data.amount.0 * multiplier;
                });
            }
            Some(Channel::Book(symbol)) => {
                let (Some(buys), Some(asks)) = (data.buys, data.asks) else {
                    return;
                };
                if buys
                    .iter()
                    .chain(&asks)
                    .any(|r| r[0].0 <= 0.0 || r[1].0 < 0.0)
                {
                    return;
                }
                let multiplier = cx.store.market(symbol).map_or(1.0, |m| m.precision.amount);
                if let Some(state) = self.books.get_mut(symbol) {
                    state.book.load_sorted(
                        BookSide::Bids,
                        buys.iter().map(|r| (r[0].0, r[1].0 * multiplier)),
                    );
                    state.book.load_sorted(
                        BookSide::Asks,
                        asks.iter().map(|r| (r[0].0, r[1].0 * multiplier)),
                    );
                    state.received = true;
                    if let Some(timer) = state.fallback.take() {
                        cx.clear_timer(timer);
                    }
                    let (bid, ask) = (
                        state.book.best_bid().unwrap_or(0.0),
                        state.book.best_ask().unwrap_or(0.0),
                    );
                    cx.store.update_ticker(symbol, |t| {
                        t.bid = bid;
                        t.ask = ask;
                    });
                    cx.book_changed(symbol);
                }
            }
            Some(Channel::Candle(symbol, i)) => {
                if let Some(row) = data.into_candle() {
                    let multiplier = cx.store.market(symbol).map_or(1.0, |m| m.precision.amount);
                    if let Some(c) = candle(&row, multiplier) {
                        cx.candle(symbol, *i, c);
                    }
                }
            }
            None => {}
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account_close(cx, socket) {
            return;
        }
        if self.public.socket != Some(socket) {
            return;
        }
        self.public.socket = None;
        self.public.open = false;
        self.channels.clear();
        for reads in self.reads.drain().map(|(_, v)| v) {
            for read in reads {
                cx.clear_timer(read.timer);
                let _ = cx.fail_context(&read.context, "Bitrue public socket disconnected");
            }
        }
        for (symbol, state) in &mut self.books {
            state.book.clear();
            state.received = false;
            if let Some(t) = state.fallback.take() {
                cx.clear_timer(t);
            }
            cx.book_changed(symbol);
        }
        self.public.supervisor.retry(cx);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match &mut pending.job {
            Job::Markets {
                rows,
                contracts,
                malformed,
            } => match serde_json::from_str::<Contract<'_>>(item) {
                Ok(row) => {
                    let parts: Vec<_> = row.symbol.split('-').collect();
                    if parts.len() != 3 {
                        safe_cex_engine::startup::skipped_row(cx);
                        return;
                    }
                    if row.multiplier.0 <= 0.0
                        || row.max_market_volume.0 <= 0.0
                        || row.price_precision > 18
                    {
                        safe_cex_engine::startup::skipped_row(cx);
                        malformed.insert(row.symbol.to_string());
                        return;
                    }
                    contracts.insert(
                        row.symbol.to_string(),
                        ContractInfo {
                            min_order_money: row.min_order_money.0,
                            max_limit_volume: row.max_limit_volume.0 * row.multiplier.0,
                        },
                    );
                    rows.push(Market {
                        id: row.symbol.to_string(),
                        symbol: row.symbol.to_string(),
                        pretty_symbol: None,
                        base: parts[1].to_owned(),
                        quote: parts[2].to_owned(),
                        active: row.status.0 == 1.0,
                        precision: Precision {
                            amount: row.multiplier.0,
                            price: 10f64.powi(-(row.price_precision as i32)),
                        },
                        limits: Limits {
                            amount: MinMax {
                                min: row.min_order_volume.0 * row.multiplier.0,
                                max: row.max_market_volume.0 * row.multiplier.0,
                            },
                            leverage: MinMax {
                                min: 1.0,
                                max: 100.0,
                            },
                            price: None,
                        },
                        contract_size: Some(row.multiplier.0),
                        is_expiring: None,
                        is_inverse: None,
                        is_spot: None,
                        is_perpetual: None,
                        is_read_only: None,
                        taker_fee: Some(0.001),
                        maker_fee: Some(0.001),
                    });
                }
                Err(_) => {
                    safe_cex_engine::startup::skipped_row(cx);
                    if let Ok(key) = serde_json::from_str::<ContractKey<'_>>(item)
                        && key.symbol.split('-').count() == 3
                    {
                        malformed.insert(key.symbol.into_owned());
                    }
                }
            },
            Job::Account(r) => self.account_item(cx, r, item),
            Job::Trade(r) => self.trade_item(cx, r, item),
            Job::History(r) => self.history_item(cx, r, item),
        }
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if let Job::Trade(request) = pending.job {
            return self.trade_response(cx, request, response);
        }
        if !Self::delivered(cx, &pending) {
            return;
        }
        if !pending.signature_resent && codes::resend(&response) {
            pending.signature_resent = true;
            return self.reissue(cx, pending);
        }
        // Settled after its job: a history page admits rows under its live leg.
        let custody = pending.custody.take();
        let leg = custody.as_ref().map(Custody::context);
        match pending.job {
            Job::Markets {
                rows,
                contracts,
                malformed,
            } => {
                if !(200..300).contains(&response.status) || rows.is_empty() {
                    cx.notice(
                        Level::Warn,
                        code::REQUEST_FAILED,
                        "Bitrue contracts read failed",
                    );
                    self.startup_retry.failed(cx, ());
                    return;
                }
                let refresh = cx.store.loaded().markets;
                let mut contracts = contracts;
                let rows = if refresh {
                    for (symbol, contract) in std::mem::take(&mut self.contracts) {
                        contracts.entry(symbol).or_insert(contract);
                    }
                    let merged = keep_listed(cx.store.markets(), rows);
                    cx.store.set_markets(merged);
                    Vec::new()
                } else {
                    rows
                };
                self.contracts = contracts;
                let before = self.malformed_markets.len();
                self.malformed_markets.extend(malformed);
                self.malformed_markets
                    .retain(|symbol| cx.store.market(symbol).is_none());
                let after = self.malformed_markets.len();
                // A partial catalogue commits so private reads start, then re-reads.
                if reread_partial(cx, &mut self.reread_stalls, refresh, before, after) {
                    self.startup_retry.failed(cx, ());
                } else {
                    self.startup_retry.succeeded(cx, &());
                }
                if refresh {
                    return;
                }
                let tickers = rows
                    .iter()
                    .map(|m| Ticker {
                        id: m.id.clone(),
                        symbol: m.symbol.clone(),
                        ..Ticker::default()
                    })
                    .collect();
                cx.store.set_markets(rows);
                cx.store.set_tickers(tickers);
                self.connect(cx);
                for command in std::mem::take(&mut self.deferred) {
                    self.on_command(cx, command);
                }
                if self.authenticated() {
                    self.start_private(cx);
                }
            }
            Job::Account(r) => self.account_response(cx, r, response),
            Job::Trade(_) => unreachable!(),
            Job::History(r) => self.history_response(cx, r, leg, response),
        }
        if let Some(custody) = custody {
            let _ = cx.finish_http_context(custody.context());
        }
    }
    fn on_signing_release(
        &mut self,
        target: safe_cex_engine::SigningTarget<'_, Pending>,
        release: safe_cex_engine::SigningRelease,
    ) {
        if let safe_cex_engine::SigningTarget::Http { pending, .. } = target {
            pending.wire.floor = release.stamp_floor();
        }
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        if let Job::Trade(request) = pending.job {
            return self.trade_response(cx, request, response);
        }
        if !pending.signature_resent && codes::resend(&response) {
            self.on_response(cx, pending, response);
        } else {
            self.on_request_failed(
                cx,
                pending,
                &format!(
                    "response did not contain the expected list: {}",
                    response.body
                ),
            );
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        if let Job::Trade(request) = pending.job {
            return self.trade_call_failed(cx, request, error, true);
        }
        let safe = pending.wire.method == Method::Get
            || (pending.wire.method == Method::Post
                && pending.wire.path
                    == "https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey");
        if matches!(
            error.kind,
            Some(safe_cex_engine::ErrorKind::Network | safe_cex_engine::ErrorKind::Timeout)
        ) && !matches!(pending.job, Job::Markets { .. })
            && safe
            && pending.attempts < 3
        {
            pending.attempts += 1;
            return self.reissue(cx, pending);
        }
        self.on_request_failed(cx, pending, &error.message);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        mut error: safe_cex_engine::VenueError,
    ) {
        Self::finish_custody(cx, &mut pending);
        if let Job::Account(account::Request::Brackets { root, .. }) = &pending.job {
            error.outcome = None;
            let _ = cx.fail_context(root.context(), error);
            return;
        }
        match pending.job {
            Job::Trade(request) => self.trade_call_failed(cx, request, error, false),
            Job::History(request) => request.sign_failed(cx, error),
            _ => self.on_request_failed(cx, pending, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, mut pending: Pending, error: &str) {
        Self::finish_custody(cx, &mut pending);
        if let Job::Account(account::Request::Brackets { root, .. }) = &pending.job
            && cx.validate_read_root_context(root.context()).is_err()
        {
            return;
        }
        match pending.job {
            Job::Markets { .. } => {
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    format!("Bitrue contracts: {error}"),
                );
                self.startup_retry.failed(cx, ());
            }
            Job::Account(r) => self.account_failed(cx, r, error),
            Job::Trade(r) => self.trade_call_failed(
                cx,
                r,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
                true,
            ),
            Job::History(r) => self.history_failed(cx, r, error),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account_timer(cx, timer) {
            return;
        }
        if self.startup_retry.on_timer(timer).is_some() {
            self.markets(cx);
            return;
        }
        match self.public.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => {
                self.connect(cx);
                return;
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.public.socket {
                    cx.close(socket);
                    self.on_close(cx, socket, 1006);
                }
                return;
            }
            Tick::Handled | Tick::Ping => return,
            Tick::Unhandled => {}
        }
        let fallback = self
            .books
            .iter()
            .find_map(|(s, b)| (b.fallback == Some(timer)).then(|| s.clone()));
        if let Some(symbol) = fallback {
            let depth = self.books.get_mut(&symbol).unwrap();
            depth.fallback = None;
            if !depth.received {
                depth.fallback_active = true;
                let channel = topic(&symbol, "depth_step0");
                self.channels.insert(channel.clone(), Channel::Book(symbol));
                self.subscription(cx, "sub", &channel, None, None, None);
            }
            return;
        }
        if self
            .reads
            .values()
            .any(|reads| reads.iter().any(|r| r.timer == timer))
        {
            // A reply without cb_id cannot be attributed after a timeout.
            if let Some(socket) = self.public.socket {
                cx.close(socket);
                self.on_close(cx, socket, 1006);
            }
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if cx.store.markets().is_empty()
            && matches!(
                command,
                Command::SubscribeOrderBook { .. }
                    | Command::UnsubscribeOrderBook { .. }
                    | Command::SubscribeCandles { .. }
                    | Command::UnsubscribeCandles { .. }
            )
        {
            self.deferred.push(command);
            return;
        }
        match command {
            Command::SetTickerSymbols { .. } => {}
            Command::SubscribeOrderBook { symbol } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    let depth = self.books.entry(symbol.clone()).or_default();
                    depth.subscribers += 1;
                    if depth.subscribers == 1 && self.public.open {
                        self.subscribe_book(cx, &symbol);
                    }
                }
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    if let Some(depth) = self.books.get_mut(&symbol) {
                        depth.subscribers = depth.subscribers.saturating_sub(1);
                        if depth.subscribers > 0 {
                            return;
                        }
                    }
                    if let Some(depth) = self.books.remove(&symbol) {
                        if let Some(t) = depth.fallback {
                            cx.clear_timer(t);
                        }
                        for suffix in ["simple_depth_step0", "depth_step0"] {
                            let channel = topic(&symbol, suffix);
                            if self.channels.remove(&channel).is_some() {
                                self.subscription(cx, "unsub", &channel, None, None, None);
                            }
                        }
                        cx.book_changed(&symbol);
                    }
                }
            }
            Command::SubscribeCandles {
                symbol,
                interval: i,
            } => {
                if interval(i).is_none() {
                    cx.notice(
                        Level::Error,
                        code::COMMAND_UNSUPPORTED,
                        i.unsupported("Bitrue"),
                    );
                    return;
                }
                if let (Some(symbol), Some(mapped)) =
                    (self.resolve_symbol(cx, &symbol), interval(i))
                {
                    let count = self.candles.entry((symbol.clone(), i)).or_default();
                    *count += 1;
                    if *count == 1 {
                        let channel = topic(&symbol, &format!("kline_{mapped}"));
                        self.channels
                            .insert(channel.clone(), Channel::Candle(symbol, i));
                        self.subscription(cx, "sub", &channel, None, None, None);
                    }
                }
            }
            Command::UnsubscribeCandles {
                symbol,
                interval: i,
            } => {
                if let Some(symbol) = self.resolve_symbol(cx, &symbol) {
                    if let Some(count) = self.candles.get_mut(&(symbol.clone(), i)) {
                        *count = count.saturating_sub(1);
                        if *count > 0 {
                            return;
                        }
                    }
                    self.candles.remove(&(symbol.clone(), i));
                    if let Some(mapped) = interval(i) {
                        let channel = topic(&symbol, &format!("kline_{mapped}"));
                        self.channels.remove(&channel);
                        self.subscription(cx, "unsub", &channel, None, None, None);
                    }
                }
            }
            Command::FetchCandles {
                command: _,
                symbol,
                interval: i,
                limit,
                from_ms,
                to_ms,
            } => {
                let Ok(context) = cx.admit_http_command(Self::socket_read_policy()) else {
                    return;
                };
                let Some(mapped) = interval(i) else {
                    let _ = cx.fail_context(&context, i.unsupported("Bitrue"));
                    return;
                };
                let Some(symbol) = self.resolve_symbol(cx, &symbol) else {
                    let _ = cx.fail_context(&context, "Unknown Bitrue market");
                    return;
                };
                if !self.public.open {
                    let _ = cx.fail_context(&context, "Bitrue public socket not connected");
                    return;
                }
                if from_ms.is_some_and(|v| !v.is_finite())
                    || to_ms.is_some_and(|v| !v.is_finite())
                    || matches!((from_ms,to_ms),(Some(f),Some(t))if f>t)
                {
                    let _ = cx.fail_context(&context, "Invalid candle range");
                    return;
                }
                let timer = cx.set_timer(5000.0);
                self.public_read(
                    cx,
                    topic(&symbol, &format!("kline_{mapped}")),
                    SocketRead {
                        context,
                        symbol,
                        kind: SocketReadKind::Candles {
                            from: from_ms,
                            to: to_ms,
                            limit: limit.unwrap_or(300).clamp(1, 300),
                        },
                        timer,
                    },
                );
            }
            Command::Query { command, query } => self.history_query(cx, command, query),
            command => {
                if !self.account_command(cx, &command) {
                    self.trade_command(cx, command);
                }
            }
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.books
            .get(symbol)
            .filter(|s| s.received)
            .map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

/// A refresh keeps every listed market the new catalogue omits or filters out, unchanged.
fn keep_listed(
    listed: &[safe_cex_engine::model::Market],
    rows: Vec<safe_cex_engine::model::Market>,
) -> Vec<safe_cex_engine::model::Market> {
    let mut fresh: Vec<_> = rows.into_iter().map(Some).collect();
    let mut index = std::collections::HashMap::new();
    for (i, market) in fresh.iter().enumerate() {
        if let Some(market) = market {
            index.entry(market.symbol.clone()).or_insert(i);
        }
    }
    let mut merged: Vec<_> = listed
        .iter()
        .map(|old| {
            index
                .get(&old.symbol)
                .and_then(|&i| fresh[i].take())
                .unwrap_or_else(|| old.clone())
        })
        .collect();
    merged.extend(fresh.into_iter().flatten());
    merged
}

/// Whether a partial catalogue re-reads; five refreshes in a row that leave its
/// malformed set no smaller end the retry.
fn reread_partial<P>(
    cx: &mut safe_cex_engine::Cx<'_, P>,
    stalls: &mut u8,
    refresh: bool,
    before: usize,
    after: usize,
) -> bool {
    if after == 0 {
        *stalls = 0;
        return false;
    }
    *stalls = if refresh && after >= before {
        stalls.saturating_add(1)
    } else {
        0
    };
    if *stalls == 5 {
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("Bitrue stops re-reading a catalogue with {after} malformed markets"),
        );
    }
    *stalls < 5
}

#[cfg(test)]
mod secondary_tests;
