//! JWT singleflight, private socket recovery and fenced account snapshots.
use crate::{
    Options, Pending,
    sign::{Credentials, SystemConfig},
    wire::StrictNumber as Number,
};
use alloy_primitives::U256;
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotCoverage;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, TimeInForce,
};
use safe_cex_engine::request_context::{
    Context, Operation, Owner, Policy, RetryPolicy, Scope, Session, SessionBearer, SessionHeaders,
    SessionPolicy,
};
use safe_cex_engine::signing::{Algorithm, SignError};
use safe_cex_engine::socket::{Config, Supervisor, Tick as SocketTick};
use safe_cex_engine::{
    CredentialSlot, Cx, Fill, Method, Response, SignStep, SocketChannel, SocketId, TimerId,
    VenueError, json::ArrayAt,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_QUEUED: usize = 256;
const RETAINED: usize = 20_000;
const POLL_MS: f64 = 10_000.;
const AUTH_REFRESH_MS: f64 = 180_000.;
/// Frame-fed: how long before `refresh_at` a held JWT is renewed.
const JWT_RENEW_EARLY_MS: f64 = 15_000.;
/// The Session bundle (`Authorization: Bearer`) goes first, as native requests sent it.
const READ_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("Accept", "application/json"),
];

pub enum Request {
    Config(Context),
    Auth {
        started: f64,
    },
    Margins {
        context: Context,
        revision: u64,
        rows: Vec<(String, f64)>,
        boot: bool,
    },
    Positions {
        context: Context,
        revision: u64,
        started: f64,
        rows: Vec<Position>,
        coverage: SnapshotCoverage<String>,
        cycle: bool,
    },
    Balance {
        context: Context,
        revision: u64,
        cycle: bool,
    },
    Orders {
        context: Context,
        token: safe_cex_engine::OrderSnapshotToken,
        rows: Vec<NativeOrder>,
        seen: HashSet<String>,
        coverage: SnapshotCoverage<String>,
        invalid: bool,
        cycle: bool,
    },
}

impl Request {
    /// The managed Background read this snapshot belongs to.
    fn read_context(&self) -> Option<&Context> {
        match self {
            Self::Margins { context, .. }
            | Self::Positions { context, .. }
            | Self::Balance { context, .. }
            | Self::Orders { context, .. } => Some(context),
            Self::Config(_) | Self::Auth { .. } => None,
        }
    }
}

#[derive(Clone, Copy)]
struct OrderVersion {
    updated: f64,
    filled: U256,
}

impl OrderVersion {
    fn new(row: &NativeOrder) -> Result<Self, String> {
        let quantum = |value: &str| {
            U256::from_str_radix(&crate::sign::quantum(value)?, 10)
                .map_err(|_| "Invalid Paradex cumulative fill".to_owned())
        };
        // Compare exact venue quanta: binary subtraction can make an unchanged
        // fill look smaller after a valid size amendment (0.3-0.2 vs 0.2-0.1).
        let filled = quantum(&row.size)?
            .checked_sub(quantum(&row.remaining_size)?)
            .ok_or("Invalid Paradex cumulative fill")?;
        Ok(Self {
            updated: row.updated(),
            filled,
        })
    }
}

#[derive(Clone)]
pub(crate) struct AmendTarget {
    market: String,
    side: String,
    kind: String,
    size: String,
    price: String,
    trigger: String,
}
impl AmendTarget {
    pub(crate) fn new(
        row: &NativeOrder,
        size: &str,
        price: &str,
        trigger: Option<&str>,
    ) -> Result<Self, String> {
        Ok(Self {
            market: row.market.clone(),
            side: row.side.clone(),
            kind: row.kind.clone(),
            size: crate::sign::quantum(size)?,
            price: Self::quantity(price)?,
            trigger: Self::quantity(trigger.unwrap_or(""))?,
        })
    }
    fn quantity(text: &str) -> Result<String, String> {
        crate::sign::quantum(if text.is_empty() { "0" } else { text })
    }
    fn same_identity(&self, row: &NativeOrder) -> bool {
        self.market == row.market && self.side == row.side && self.kind == row.kind
    }
    fn matches(&self, row: &NativeOrder) -> bool {
        self.same_identity(row)
            && crate::sign::quantum(&row.size).is_ok_and(|n| n == self.size)
            && Self::quantity(&row.price).is_ok_and(|n| n == self.price)
            && Self::quantity(&row.trigger_price).is_ok_and(|n| n == self.trigger)
    }
}
struct AmendFence {
    target: AmendTarget,
    floor: f64,
    pending: bool,
    sent: bool,
    previous: Option<Box<AmendFence>>,
}
#[derive(PartialEq)]
enum OrderEvidence {
    Ignore,
    Unresolved,
    Coherent,
}

struct Queued {
    method: Method,
    path: String,
    body: Option<String>,
    at: Option<ArrayAt>,
    pending: Pending,
}

pub struct Account {
    options: Options,
    started: bool,
    credentials: Option<Credentials>,
    token: Option<String>,
    /// The JWT Session every private read binds; one generation per account.
    session: Option<Session>,
    /// Moves on every JWT set within the Session generation (the Bearer revision).
    bearer_revision: u64,
    refresh_at: f64,
    auth_in_flight: bool,
    /// A definitive credential refusal; terminal until the account is rebuilt.
    auth_rejected: bool,
    queue: VecDeque<Queued>,
    failed: Vec<(Pending, VenueError)>,
    socket: Option<SocketId>,
    open: bool,
    authenticated: bool,
    next_rpc: u64,
    auth_rpc: Option<u64>,
    subscriptions: HashMap<u64, String>,
    subscriptions_started: f64,
    supervisor: Supervisor,
    retry: Option<TimerId>,
    /// Frame-fed: renews the held JWT before it expires.
    renew: Option<TimerId>,
    poll: Option<TimerId>,
    repair_orders: Option<TimerId>,
    repair_positions: Option<TimerId>,
    positions_revision: u64,
    position_versions: HashMap<String, (f64, bool)>,
    position_sequences: HashMap<String, i64>,
    balance_revision: u64,
    margins_revision: u64,
    orders_busy: bool,
    /// An uncertain placement asked for a read dispatched after its write.
    orders_owed: bool,
    positions_busy: bool,
    balance_busy: bool,
    margins_busy: bool,
    native: HashMap<String, NativeOrder>,
    amend_fences: HashMap<String, AmendFence>,
    order_versions: HashMap<String, OrderVersion>,
    order_version_ids: VecDeque<String>,
    terminals: HashSet<String>,
    terminal_ids: VecDeque<String>,
    executions: HashSet<String>,
    execution_ids: VecDeque<String>,
    /// Perpetuals the catalogue drops on purpose (non-active); not malformed.
    pub(crate) excluded_perps: HashSet<String>,
}

/// The one dispatch-time signing attachment for `/auth` and every signed
/// write; an interim legacy site until these requests move to the funnel.
fn sign(cx: &mut Cx<'_, Pending>, request: safe_cex_engine::RequestId, step: Box<dyn SignStep>) {
    cx.sign_http(request, step);
}

impl Account {
    pub fn new(options: Options) -> Self {
        Self {
            options,
            started: false,
            credentials: None,
            token: None,
            session: None,
            bearer_revision: 0,
            refresh_at: 0.,
            auth_in_flight: false,
            auth_rejected: false,
            queue: VecDeque::new(),
            failed: vec![],
            socket: None,
            open: false,
            authenticated: false,
            next_rpc: 0,
            auth_rpc: None,
            subscriptions: HashMap::new(),
            subscriptions_started: 0.,
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(15_000.),
                idle_timeout_ms: 90_000.,
                ..Config::default()
            }),
            retry: None,
            renew: None,
            poll: None,
            repair_orders: None,
            repair_positions: None,
            positions_revision: 0,
            position_versions: HashMap::new(),
            position_sequences: HashMap::new(),
            balance_revision: 0,
            margins_revision: 0,
            orders_busy: false,
            orders_owed: false,
            positions_busy: false,
            balance_busy: false,
            margins_busy: false,
            native: HashMap::new(),
            amend_fences: HashMap::new(),
            order_versions: HashMap::new(),
            order_version_ids: VecDeque::new(),
            terminals: HashSet::new(),
            terminal_ids: VecDeque::new(),
            executions: HashSet::new(),
            execution_ids: VecDeque::new(),
            excluded_perps: HashSet::new(),
        }
    }
    pub fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://api.testnet.paradex.trade/v1"
        } else {
            "https://api.prod.paradex.trade/v1"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        } else {
            "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        }
    }
    pub fn credentials(&self) -> Option<&Credentials> {
        self.credentials.as_ref()
    }
    pub(crate) fn testnet(&self) -> bool {
        self.options.testnet
    }
    pub fn invalidate_order(&mut self, id: &str) {
        if let Some(row) = self.native.remove(id)
            && let Ok(version) = OrderVersion::new(&row)
        {
            self.remember_order(id, version);
        }
    }
    pub(crate) fn begin_amend(&mut self, id: &str, target: AmendTarget) -> Result<(), String> {
        if !self.amend_fences.contains_key(id) && self.amend_fences.len() >= RETAINED {
            return Err("Paradex amendment reconciliation capacity reached".into());
        }
        self.invalidate_order(id);
        let floor = self.order_versions.get(id).map_or(0., |v| v.updated);
        let previous = self.amend_fences.remove(id).map(Box::new);
        self.amend_fences.insert(
            id.into(),
            AmendFence {
                target,
                floor,
                pending: true,
                sent: false,
                previous,
            },
        );
        Ok(())
    }
    pub(crate) fn amend_ack(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if let Ok(row) = serde_json::from_str::<NativeOrder>(text)
            && self
                .amend_fences
                .get(&row.id)
                .is_some_and(|f| f.target.matches(&row))
        {
            // A complete Modify response is venue evidence; an id-only ACK is not.
            self.apply_order(cx, row);
        }
    }
    pub(crate) fn finish_amend(&mut self, id: &str, now: f64, rejected: bool) {
        let Some(mut fence) = self.amend_fences.remove(id) else {
            return;
        };
        fence.floor = fence
            .floor
            .max(now)
            .max(self.order_versions.get(id).map_or(0., |v| v.updated));
        if rejected || !fence.sent {
            if let Some(mut previous) = fence.previous.take() {
                previous.floor = previous.floor.max(fence.floor);
                self.amend_fences.insert(id.into(), *previous);
            }
            self.invalidate_order(id);
            return;
        }
        fence.pending = false;
        fence.previous = None;
        if !self
            .native
            .get(id)
            .is_some_and(|row| fence.target.matches(row))
        {
            self.invalidate_order(id);
        }
        self.amend_fences.insert(id.into(), fence);
    }
    pub fn native_order(&self, id: &str) -> Option<&NativeOrder> {
        self.native.get(id)
    }
    pub fn bump_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.invalidate_order_snapshots();
    }
    pub fn bump_positions(&mut self) {
        self.positions_revision = self.positions_revision.wrapping_add(1);
    }
    pub fn drain_failed(&mut self) -> Vec<(Pending, VenueError)> {
        std::mem::take(&mut self.failed)
    }
    pub fn handles_socket(&self, socket: SocketId) -> bool {
        self.socket == Some(socket)
    }
    pub fn handles_timer(&self, timer: TimerId) -> bool {
        [
            self.retry,
            self.poll,
            self.repair_orders,
            self.repair_positions,
            self.renew,
        ]
        .contains(&Some(timer))
            || self.supervisor.owns_timer(timer)
    }
    pub fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.started {
            return;
        }
        self.started = true;
        cx.store.set_hedged(false);
        if self.options.public_data_only
            || self.options.credentials.is_none()
            || self
                .options
                .paradex_address
                .as_deref()
                .is_none_or(str::is_empty)
            || self
                .options
                .subkey_address
                .as_deref()
                .is_none_or(str::is_empty)
        {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Balance::default());
            return;
        }
        self.fetch_config(cx);
    }
    fn fetch_config(&mut self, cx: &mut Cx<'_, Pending>) {
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
                self.config_failed(cx, "Paradex config lifecycle is unavailable");
                return;
            }
        };
        if let Err((pending, error)) = cx.request_managed(
            context.clone(),
            None,
            Method::Get,
            format!("{}/system/config", self.rest()),
            None,
            Pending::Account(Request::Config(context)),
        ) {
            let Pending::Account(request) = pending else {
                unreachable!("config preserves its pending value")
            };
            self.on_failed(
                cx,
                request,
                &format!("Paradex config preparation refused: {error:?}"),
            );
        }
    }
    fn config_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if self.retry.is_none() {
            self.retry = Some(cx.set_timer(2_000.));
        }
    }
    pub fn request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: String,
        body: Option<String>,
        at: Option<ArrayAt>,
        pending: Pending,
    ) {
        if self.credentials.is_none() {
            self.failed.push((
                pending,
                VenueError::new("Paradex private authentication is not ready"),
            ));
            return;
        }
        if self.auth_rejected {
            self.failed.push((pending, auth_rejected_error()));
            return;
        }
        let request = Queued {
            method,
            path,
            body,
            at,
            pending,
        };
        if self.token.is_some() && cx.now < self.refresh_at {
            self.dispatch(cx, request);
            return;
        }
        if self.queue.len() >= MAX_QUEUED {
            self.failed.push((
                request.pending,
                VenueError::new("Paradex authentication queue is full"),
            ));
            return;
        }
        self.queue.push_back(request);
        self.refresh(cx);
    }
    /// A private GET bound to the JWT Session: the engine attaches the current
    /// Bearer at prepare and parks the read while the JWT is refreshing.
    pub(crate) fn read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: Context,
        path: &str,
        at: Option<ArrayAt>,
        pending: Pending,
    ) {
        if self.auth_rejected {
            self.failed.push((pending, auth_rejected_error()));
            return;
        }
        let Some(session) = self.session.clone() else {
            self.failed.push((
                pending,
                VenueError::new("Paradex private authentication is not ready"),
            ));
            return;
        };
        match cx.request_managed_with_headers(
            context,
            Some(&session),
            Method::Get,
            format!("{}{path}", self.rest()),
            READ_HEADERS,
            None,
            pending,
        ) {
            Ok(id) => {
                if let Some(at) = at {
                    cx.stream_response(id, at);
                }
            }
            Err((pending, error)) => {
                self.failed.push((
                    pending,
                    VenueError::new(format!("Paradex read preparation refused: {error:?}")),
                ));
                return;
            }
        }
        if self.token.is_none() || cx.now >= self.refresh_at {
            self.refresh(cx);
        }
    }
    /// A Background read Context for the account cycle.
    fn background_context(&mut self, cx: &mut Cx<'_, Pending>) -> Option<Context> {
        let context = cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        );
        if context.is_err() {
            cx.notice(
                Level::Warn,
                code::REQUEST_FAILED,
                "Paradex account read lifecycle is unavailable",
            );
        }
        context.ok()
    }
    fn serves(&self, generation: u64) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.owner() == Owner::Session { generation })
    }
    pub(crate) fn session_bearer(&self, generation: u64) -> SessionBearer {
        let Some(token) = self.token.as_deref().filter(|_| self.serves(generation)) else {
            return SessionBearer::NotReady;
        };
        match SessionHeaders::bearer(token) {
            Ok(headers) => SessionBearer::Ready {
                headers,
                revision: self.bearer_revision,
            },
            Err(_) => SessionBearer::NotReady,
        }
    }
    pub(crate) fn session_bearer_revision(&self, generation: u64) -> Option<u64> {
        (self.token.is_some() && self.serves(generation)).then_some(self.bearer_revision)
    }
    /// An unsent read after a refused `/auth` failed because its Session was
    /// retired: that is the account's terminal credential refusal.
    pub(crate) fn read_error(&self, error: VenueError) -> VenueError {
        if self.auth_rejected && error.outcome == Some(safe_cex_engine::Outcome::NotSent) {
            auth_rejected_error()
        } else {
            error
        }
    }
    fn dispatch(&mut self, cx: &mut Cx<'_, Pending>, mut request: Queued) {
        if let Pending::Trade(trade) = &request.pending {
            trade.report_progress(cx);
        }
        let signing = if let Pending::Trade(trade) = &mut request.pending {
            trade.take_signed()
        } else {
            None
        };
        if let Pending::Trade(trade) = &request.pending
            && let Some(id) = trade.amend_id()
            && let Some(fence) = self.amend_fences.get_mut(id)
        {
            fence.sent = true;
        }
        let mut headers = vec![
            (
                "Authorization",
                format!("Bearer {}", self.token.as_deref().expect("ready JWT")),
            ),
            ("Content-Type", "application/json".into()),
            ("Accept", "application/json".into()),
        ];
        if request.method == Method::Post && request.path.starts_with("/orders") {
            headers.push(("PARADEX-PARTNER", "TEALSTREET".into()));
        }
        let url = format!("{}{}", self.rest(), request.path);
        let id = if let Some(at) = request.at {
            cx.request_streamed_body(
                request.method,
                url,
                headers,
                request.body,
                at,
                request.pending,
            )
        } else {
            cx.request(request.method, url, headers, request.body, request.pending)
        };
        if let Some(step) = signing {
            sign(cx, id, step);
        }
    }
    fn refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.auth_in_flight || self.auth_rejected {
            return;
        }
        let Some(credentials) = &self.credentials else {
            return;
        };
        // An expiring JWT never binds again: Session reads park until the new one.
        self.token = None;
        let signature = crate::sign::dispatch::Auth::new(
            credentials,
            self.options.testnet,
            format!("{}/auth/{}", self.rest(), credentials.subkey()),
        );
        self.auth_in_flight = true;
        let id = cx.request(
            Method::Post,
            format!("{}/auth/{}", self.rest(), credentials.subkey()),
            signature.headers(credentials.address()),
            None,
            Pending::Account(Request::Auth { started: cx.now }),
        );
        cx.route_request(id, safe_cex_engine::RequestRoute::Direct);
        sign(cx, id, Box::new(signature));
    }
    fn auth_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        self.auth_in_flight = false;
        self.token = None;
        self.refresh_at = 0.;
        for queued in self.queue.drain(..) {
            self.failed.push((queued.pending, VenueError::new(error)));
        }
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if self.retry.is_none() {
            self.retry = Some(cx.set_timer(2_000.));
        }
    }
    /// A definitive 401/403 from `/auth`: the credentials are wrong, so retrying
    /// cannot help. Every queued and later private request refuses typed.
    fn auth_rejected(&mut self, cx: &mut Cx<'_, Pending>) {
        self.auth_in_flight = false;
        self.auth_rejected = true;
        self.token = None;
        self.refresh_at = 0.;
        if let Some(session) = self.session.take() {
            // Fails every unreleased read bound to it NotSent. If the engine
            // refuses, those reads still end at their own 20 s windows.
            if cx.retire_session(&session).is_err() {
                cx.notice(
                    Level::Warn,
                    code::REQUEST_FAILED,
                    "Paradex could not retire its private session",
                );
            }
        }
        for queued in self.queue.drain(..) {
            self.failed.push((queued.pending, auth_rejected_error()));
        }
        cx.notice(
            Level::Error,
            code::AUTH_REJECTED,
            "Paradex rejected the account credentials",
        );
    }
    fn connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.socket.is_none() && !self.supervisor.is_waiting() {
            self.socket = Some(
                cx.connect(
                    safe_cex_engine::SocketSpec::new(self.ws())
                        .channel(SocketChannel::Private)
                        .binary(),
                ),
            );
            self.supervisor.connecting(cx);
        }
    }
    fn send_auth(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.open || self.auth_rpc.is_some() || self.authenticated {
            return;
        }
        if self.token.is_none() || cx.now >= self.refresh_at {
            self.refresh(cx);
            return;
        }
        #[derive(Serialize)]
        struct Params<'a> {
            bearer: &'a str,
        }
        #[derive(Serialize)]
        struct Rpc<'a> {
            jsonrpc: &'static str,
            method: &'static str,
            params: Params<'a>,
            id: u64,
        }
        let id = self.rpc();
        self.auth_rpc = Some(id);
        cx.send(
            self.socket.expect("open socket"),
            serde_json::to_string(&Rpc {
                jsonrpc: "2.0",
                method: "auth",
                params: Params {
                    bearer: self.token.as_deref().expect("JWT"),
                },
                id,
            })
            .expect("serializable auth"),
        );
    }
    fn rpc(&mut self) -> u64 {
        let id = self.next_rpc;
        self.next_rpc = self.next_rpc.wrapping_add(1);
        id
    }
    fn subscribe(&mut self, cx: &mut Cx<'_, Pending>, channel: &str) {
        #[derive(Serialize)]
        struct Params<'a> {
            channel: &'a str,
        }
        #[derive(Serialize)]
        struct Rpc<'a> {
            jsonrpc: &'static str,
            method: &'static str,
            params: Params<'a>,
            id: u64,
        }
        let id = self.rpc();
        self.subscriptions.insert(id, channel.to_owned());
        cx.send(
            self.socket.expect("authenticated socket"),
            serde_json::to_string(&Rpc {
                jsonrpc: "2.0",
                method: "subscribe",
                params: Params { channel },
                id,
            })
            .expect("serializable channel"),
        );
    }
    pub fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if !self.handles_socket(socket) {
            return;
        }
        self.open = true;
        self.authenticated = false;
        self.auth_rpc = None;
        self.subscriptions.clear();
        self.supervisor.opened(cx);
        // Hosts answer RFC6455 pings; the server's SBE heartbeat proves idle liveness.
        self.send_auth(cx);
    }
    fn disconnect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(socket) = self.socket.take() {
            cx.close(socket);
        }
        self.open = false;
        self.authenticated = false;
        self.auth_rpc = None;
        self.subscriptions.clear();
        self.supervisor.retry(cx);
    }
    pub fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.handles_socket(socket) {
            self.disconnect(cx);
        }
    }
    pub fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if !self.handles_socket(socket) || !self.open {
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if message.kind == "ping" || message.kind == "pong" {
            self.supervisor.received(cx.now);
            return;
        }
        if message.jsonrpc != "2.0" {
            return;
        }
        if message.id == self.auth_rpc && self.auth_rpc.is_some() {
            fn node_id<'de, D: serde::Deserializer<'de>>(
                d: D,
            ) -> Result<Option<&'de str>, D::Error> {
                let id = <&str>::deserialize(d)?;
                if id.is_empty() {
                    return Err(serde::de::Error::custom("empty Paradex node id"));
                }
                Ok(Some(id))
            }
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct AuthResult<'a> {
                #[serde(default, borrow, deserialize_with = "node_id")]
                node_id: Option<&'a str>,
            }
            let valid_result = message.result.is_some_and(|result| {
                result.get().trim_start().starts_with('{')
                    && serde_json::from_str::<AuthResult<'_>>(result.get())
                        .is_ok_and(|r| r.node_id.is_none_or(|id| !id.is_empty()))
            });
            if message.error.is_some() || !valid_result {
                self.token = None;
                self.refresh_at = 0.;
                self.disconnect(cx);
                return;
            }
            self.auth_rpc = None;
            self.authenticated = true;
            self.supervisor.authenticated(cx);
            self.supervisor.received(cx.now);
            self.subscriptions_started = cx.now;
            for channel in [
                "orders.ALL",
                "positions",
                "balance_events",
                "account",
                "fills.ALL",
            ] {
                self.subscribe(cx, channel);
            }
            return;
        }
        if let Some(id) = message.id
            && let Some(expected) = self.subscriptions.get(&id)
        {
            if message.error.is_some() {
                cx.notice(
                    Level::Warn,
                    code::SUBSCRIPTION_REJECTED,
                    "Paradex private subscription was rejected",
                );
                self.disconnect(cx);
            } else {
                #[derive(Deserialize)]
                struct Ack<'a> {
                    #[serde(borrow)]
                    channel: &'a str,
                }
                let channel = message
                    .result
                    .and_then(|result| serde_json::from_str::<Ack<'_>>(result.get()).ok())
                    .map(|ack| ack.channel);
                if channel == Some(expected.as_str()) {
                    self.subscriptions.remove(&id);
                    self.supervisor.received(cx.now);
                }
            }
            return;
        }
        if !self.authenticated || message.method != "subscription" {
            return;
        }
        let Some(params) = message.params else {
            return;
        };
        let valid = if params.channel.starts_with("orders.") {
            self.order_message(cx, params.data.get())
        } else if params.channel == "positions" {
            self.position_message(cx, params.data.get())
        } else if matches!(params.channel, "balance_events" | "account") {
            self.balance_message(cx, params.data.get())
        } else if params.channel.starts_with("fills.") {
            self.fill_message(cx, params.data.get())
        } else {
            false
        };
        if valid {
            self.supervisor.received(cx.now);
        }
    }
    pub fn on_binary(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, bytes: &[u8]) {
        if !self.handles_socket(socket) || !self.open || !self.authenticated {
            return;
        }
        let result =
            crate::sbe::Frame::parse(bytes).and_then(|mut frame| self.binary(cx, &mut frame));
        match result {
            Ok(true) => self.supervisor.received(cx.now),
            Ok(false) => {}
            Err(error) => cx.notice(Level::Warn, code::STREAM_MALFORMED, error),
        }
    }
    fn binary(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        frame: &mut crate::sbe::Frame<'_>,
    ) -> Result<bool, &'static str> {
        let root = frame.root;
        let minimum = match frame.template {
            20 => 126,
            21 => {
                if frame.version >= 2 {
                    116
                } else {
                    107
                }
            }
            22 => 154,
            23 => 113,
            40 => 16,
            _ => 0,
        };
        if root.len() < minimum {
            return Err("Truncated Paradex private SBE block");
        }
        match frame.template {
            20 => {
                let status = match root.u8(16)? {
                    1 => "NEW",
                    2 => "UNTRIGGERED",
                    3 => "OPEN",
                    4 => "CLOSED",
                    _ => return Err("Unknown Paradex SBE order status"),
                };
                let side = binary_side(root.u8(17)?)?;
                let kind = match root.u8(18)? {
                    1 => "LIMIT",
                    2 => "MARKET",
                    3 => "STOP_LIMIT",
                    4 => "STOP_MARKET",
                    5 => "TAKE_PROFIT_LIMIT",
                    6 => "TAKE_PROFIT_MARKET",
                    7 => "STOP_LOSS_LIMIT",
                    8 => "STOP_LOSS_MARKET",
                    _ => return Err("Unknown Paradex SBE order type"),
                };
                let instruction = match root.u8(19)? {
                    1 => "GTC",
                    2 => "IOC",
                    3 => "POST_ONLY",
                    4 => "RPI",
                    _ => return Err("Unknown Paradex SBE order instruction"),
                };
                let id = frame.tail.string()?.to_owned();
                let client_id = frame.tail.string()?.to_owned();
                let market = frame.tail.string()?.to_owned();
                let cancel_reason = frame.tail.string()?.to_owned();
                if frame.version >= 2 && frame.tail.remaining() > 0 {
                    frame.tail.string()?;
                }
                if frame.version >= 2 && frame.tail.remaining() > 0 {
                    frame.tail.string()?;
                }
                if frame.tail.remaining() != 0 {
                    return Err("Trailing Paradex private SBE data");
                }
                let flags = root.u8(125)?;
                let mut flag_names = vec![];
                for (bit, name) in [
                    (0, "REDUCE_ONLY"),
                    (1, "STOP_CONDITION_BELOW_TRIGGER"),
                    (2, "STOP_CONDITION_ABOVE_TRIGGER"),
                    (3, "INTERACTIVE"),
                    (4, "TARGET_STRATEGY_VWAP"),
                    (5, "MMP"),
                ] {
                    if flags & (1 << bit) != 0 {
                        flag_names.push(name.to_owned());
                    }
                }
                let row = NativeOrder {
                    id,
                    client_id,
                    market,
                    cancel_reason,
                    status: status.into(),
                    side: side.into(),
                    kind: kind.into(),
                    instruction: instruction.into(),
                    flags: Some(flag_names),
                    price: decimal8(root.i64(20)?),
                    trigger_price: decimal8(root.i64(28)?),
                    size: decimal8(root.i64(36)?),
                    remaining_size: decimal8(root.i64(44)?),
                    created_at: (root.i64(60)? / 1000).to_string(),
                    last_updated_at: (root.i64(68)? / 1000).to_string(),
                    ..NativeOrder::default()
                };
                Ok(self.apply_order(cx, row))
            }
            21 => {
                let side = binary_side(root.u8(17)?)?;
                let id = frame.tail.string()?;
                let order_id = frame.tail.string()?;
                frame.tail.string()?;
                frame.tail.string()?;
                let market = frame.tail.string()?;
                if frame.version >= 2 {
                    frame.tail.string()?;
                }
                if frame.tail.remaining() != 0 {
                    return Err("Trailing Paradex private SBE data");
                }
                let price = root.f8(19)?.ok_or("Null Paradex fill price")?;
                let size = root.f8(27)?.ok_or("Null Paradex fill size")?;
                Ok(self.apply_fill(
                    cx,
                    FillRow {
                        id,
                        order_id,
                        market,
                        side,
                        price: Number(price),
                        size: Number(size),
                    },
                ))
            }
            22 => {
                let side = match binary_side(root.u8(16)?)? {
                    "BUY" => "LONG",
                    _ => "SHORT",
                };
                let status = match root.u8(153)? {
                    1 => "OPEN",
                    2 => "CLOSED",
                    _ => return Err("Unknown Paradex SBE position status"),
                };
                let market = frame.tail.string()?;
                frame.tail.string()?;
                if frame.tail.remaining() != 0 {
                    return Err("Trailing Paradex private SBE data");
                }
                let value = |offset| {
                    root.f8(offset)
                        .and_then(|n| n.ok_or("Null Paradex SBE position field"))
                };
                let row = PositionRow {
                    market,
                    side,
                    status,
                    size: Number(value(17)?),
                    average_entry_price: Number(value(25)?),
                    unrealized_pnl: Number(value(33)?),
                    realized_positional_pnl: Number(value(41)?),
                    leverage: Number(value(65)?),
                    liquidation_price: Number(root.f8(57)?.unwrap_or(0.)),
                    last_updated_at: Number(root.i64(73)? as f64 / 1000.),
                };
                let sequence = root.i64(8)?;
                if sequence < 0
                    || self
                        .position_sequences
                        .get(market)
                        .is_some_and(|old| *old > sequence)
                {
                    return Ok(false);
                }
                let applied = self.apply_position(cx, row);
                if applied {
                    self.position_sequences.insert(market.into(), sequence);
                }
                Ok(applied)
            }
            23 => {
                frame.tail.string()?;
                if frame.tail.remaining() != 0 {
                    return Err("Trailing Paradex private SBE data");
                }
                let total = root.f8(16)?.ok_or("Null Paradex SBE collateral")?;
                let free = root.f8(24)?.ok_or("Null Paradex SBE free collateral")?;
                let used = root.f8(32)?.ok_or("Null Paradex SBE margin")?;
                self.balance_revision = self.balance_revision.wrapping_add(1);
                cx.store.set_balance(Balance {
                    total,
                    free,
                    used,
                    upnl: cx.store.positions().iter().map(|p| p.unrealized_pnl).sum(),
                });
                Ok(true)
            }
            40 => {
                root.i64(0)?;
                root.i64(8)?;
                if frame.tail.remaining() != 0 {
                    return Err("Trailing Paradex private SBE data");
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.supervisor.owns_timer(timer) {
            match self.supervisor.on_timer(cx, timer) {
                SocketTick::Reconnect => self.connect(cx),
                SocketTick::TimedOut(_) => self.disconnect(cx),
                SocketTick::Handled
                    if self.authenticated
                        && !self.subscriptions.is_empty()
                        && cx.now - self.subscriptions_started >= 15_000. =>
                {
                    self.disconnect(cx)
                }
                _ => {}
            }
        } else if self.renew == Some(timer) {
            self.renew = None;
            self.refresh(cx);
        } else if self.retry == Some(timer) {
            self.retry = None;
            if self.credentials.is_none() {
                self.fetch_config(cx);
            } else {
                self.refresh(cx);
                self.positions(cx, true);
            }
        } else if self.poll == Some(timer) {
            self.poll = None;
            self.positions(cx, true);
        } else if self.repair_orders == Some(timer) {
            self.repair_orders = None;
            self.orders(cx, false);
        } else if self.repair_positions == Some(timer) {
            self.repair_positions = None;
            self.positions(cx, false);
        }
    }
    fn margins(&mut self, cx: &mut Cx<'_, Pending>, boot: bool) {
        // Frame-fed: the data source's frames carry the account; nothing is read here.
        if cx.frame_fed() {
            return;
        }
        if self.margins_busy {
            return;
        }
        self.margins_busy = true;
        let Some(context) = self.background_context(cx) else {
            self.margins_ended(cx, boot);
            return;
        };
        self.read(
            cx,
            context.clone(),
            "/account/margin",
            Some(ArrayAt::Field("configs")),
            Pending::Account(Request::Margins {
                context,
                revision: self.margins_revision,
                rows: vec![],
                boot,
            }),
        );
    }
    fn positions(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        // Frame-fed: the data source's frames carry the account; nothing is read here.
        if cx.frame_fed() {
            return;
        }
        if self.positions_busy {
            if cycle && self.poll.is_none() {
                self.poll = Some(cx.set_poll_timer(POLL_MS));
            }
            return;
        }
        self.positions_busy = true;
        let Some(context) = self.background_context(cx) else {
            self.positions_ended(cx, cycle);
            return;
        };
        self.read(
            cx,
            context.clone(),
            "/positions",
            Some(ArrayAt::Field("results")),
            Pending::Account(Request::Positions {
                context,
                revision: self.positions_revision,
                started: cx.now,
                rows: vec![],
                coverage: SnapshotCoverage::default(),
                cycle,
            }),
        );
    }
    fn balance(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        // Frame-fed: the data source's frames carry the account; nothing is read here.
        if cx.frame_fed() {
            return;
        }
        if self.balance_busy {
            return;
        }
        self.balance_busy = true;
        let Some(context) = self.background_context(cx) else {
            self.balance_ended(cx, cycle);
            return;
        };
        self.read(
            cx,
            context.clone(),
            "/account",
            None,
            Pending::Account(Request::Balance {
                context,
                revision: self.balance_revision,
                cycle,
            }),
        );
    }
    /// Start or join an order read dispatched after an uncertain placement;
    /// a clean snapshot releases the engine's fence through `orders_reconciled`.
    pub fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.orders_busy {
            self.orders_owed = true;
        } else {
            self.orders(cx, false);
        }
    }
    fn owed_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if std::mem::take(&mut self.orders_owed) {
            self.orders(cx, false);
        }
    }
    fn orders(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        // Frame-fed: the data source's frames carry the account; nothing is read here.
        if cx.frame_fed() {
            return;
        }
        if self.orders_busy {
            return;
        }
        let Some(token) = cx.store.begin_orders_snapshot() else {
            cx.notice(
                Level::Error,
                code::SNAPSHOT_DISCARDED,
                "Paradex order snapshot admission exhausted",
            );
            if cycle {
                self.margins(cx, false);
            }
            return;
        };
        self.orders_busy = true;
        let Some(context) = self.background_context(cx) else {
            self.orders_ended(cx, cycle);
            return;
        };
        self.read(
            cx,
            context.clone(),
            "/orders",
            Some(ArrayAt::Field("results")),
            Pending::Account(Request::Orders {
                context,
                token,
                rows: vec![],
                seen: HashSet::new(),
                coverage: SnapshotCoverage::default(),
                invalid: false,
                cycle,
            }),
        );
    }
    pub fn set_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, leverage: f64) {
        self.margins_revision = self.margins_revision.wrapping_add(1);
        cx.store.update_setting(symbol, |s| {
            s.leverage = leverage;
            s.margin_type = Some(MarginType::Cross);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = leverage;
            }
        });
    }
    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        let registered = registered_market(
            cx,
            &self.excluded_perps,
            item,
            matches!(request, Request::Positions { .. }),
        );
        match request {
            Request::Margins { rows, .. } => {
                match serde_json::from_str::<Margin<'_>>(item) {
                    Ok(row) if !row.market.is_empty() && row.leverage.0 > 0. => {
                        rows.push((row.market.into(), row.leverage.0))
                    }
                    // Settings are upserts: a skipped row leaves its stored leverage as is.
                    _ => cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        match row_market(item) {
                            Some(market) => {
                                format!("Paradex skipped malformed margin row {market}")
                            }
                            None => "Paradex skipped malformed margin row without a market".into(),
                        },
                    ),
                }
            }
            Request::Positions { rows, coverage, .. } => {
                match registered {
                    // Other products, excluded perpetuals and flat unknown rows.
                    Ok(false) => return,
                    Ok(true) => {
                        if let Ok(Ok(position)) = serde_json::from_str::<PositionRow<'_>>(item)
                            .map(|row| map_position(&row, cx))
                        {
                            return rows.extend(position);
                        }
                    }
                    Err(()) => {}
                }
                let market = row_market(item);
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    match (market, registered) {
                        (Some(market), Ok(_)) => {
                            format!("Paradex skipped unmappable position {market}")
                        }
                        (Some(market), Err(())) => {
                            format!("Paradex skipped position {market} on an unknown market")
                        }
                        (None, _) => "Paradex skipped unmappable position without a market".into(),
                    },
                );
                coverage.reject(market.map(Into::into));
            }
            Request::Orders {
                rows,
                seen,
                coverage,
                invalid,
                ..
            } => {
                // Out-of-scope rows drop first; a repeat among them proves nothing.
                if registered == Ok(false) {
                    return;
                }
                let row = serde_json::from_str::<NativeOrder>(item);
                if let Ok(row) = &row
                    && !row.id.is_empty()
                    && !seen.insert(row.id.clone())
                {
                    *invalid = true;
                    return;
                }
                if let Ok(row) = row
                    && registered == Ok(true)
                    && map_order(&row).is_ok()
                    && OrderVersion::new(&row).is_ok()
                {
                    return rows.push(row);
                }
                #[derive(Deserialize)]
                struct Identity {
                    id: String,
                }
                let id = serde_json::from_str::<Identity>(item)
                    .ok()
                    .map(|row| row.id)
                    .filter(|id| !id.is_empty());
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    match (&id, registered.is_err() && row_market(item).is_some()) {
                        (Some(id), true) => {
                            format!("Paradex skipped order {id} on an unknown market")
                        }
                        (Some(id), false) => format!("Paradex skipped unmappable order {id}"),
                        (None, _) => "Paradex skipped unmappable order without an id".into(),
                    },
                );
                coverage.reject(id);
            }
            _ => {}
        }
    }
    /// A snapshot read is applied first and its Context retired after, so the
    /// published rows are those of a live read.
    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let context = request.read_context().cloned();
        if let Some(context) = &context
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            // A stale delivery belongs to a read that already ended.
            let _ = cx.finish_http_context(context);
            return;
        }
        self.apply_response(cx, request, response);
        if let Some(context) = context {
            // Already retired when the response failed through `on_failed`.
            let _ = cx.finish_http_context(&context);
        }
    }
    fn apply_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if let Request::Config(context) = &request
            && cx.validate_background_read_http_delivery(context).is_err()
        {
            let _ = cx.finish_http_context(context);
            return;
        }
        if matches!(request, Request::Auth { .. }) && matches!(response.status, 401 | 403) {
            self.auth_rejected(cx);
            return;
        }
        if let Err(error) = crate::success(response) {
            self.on_failed(cx, request, &error);
            return;
        }
        match request {
            Request::Config(context) => {
                let credentials = serde_json::from_str::<SystemConfig>(response.body)
                    .map_err(|_| "Invalid Paradex system config".to_owned())
                    .and_then(|config| {
                        // Only trusted local setup's sealed key, read under the
                        // installed provider; any other provider has none.
                        let signing = self
                            .options
                            .credentials
                            .as_ref()
                            .and_then(|identity| identity.signing_ref().ok())
                            .ok_or("Paradex signing key is not installed")?
                            .clone();
                        // A local provider's seal, else the key a host-answered
                        // provider bound at setup; every signature verifies under it.
                        let public = cx
                            .verified_public_key(CredentialSlot::Primary, Algorithm::StarkDigest)
                            .or_else(|_| {
                                cx.setup_public_key(CredentialSlot::Primary, Algorithm::StarkDigest)
                                    .and_then(|key| {
                                        <[u8; 32]>::try_from(key)
                                            .map_err(|_| SignError::InvalidResult)
                                    })
                            })
                            .map_err(|_| "Paradex signing key is not installed".to_owned())?;
                        Credentials::new(
                            signing,
                            public,
                            self.options.paradex_address.as_deref().unwrap_or_default(),
                            self.options.subkey_address.as_deref().unwrap_or_default(),
                            self.options.ethereum_key.then_some(self.options.testnet),
                            &config,
                        )
                    });
                let credentials = credentials.and_then(|credentials| {
                    let session = cx
                        .open_session(SessionPolicy {
                            credential: None,
                            // `/auth` stays outside the Session: it only attaches the JWT.
                            repeat_safe: false,
                            private_egress: None,
                        })
                        .map_err(|_| "Paradex private session is unavailable".to_owned())?;
                    self.session = Some(session);
                    Ok(credentials)
                });
                match credentials {
                    Ok(credentials) => {
                        self.credentials = Some(credentials);
                        if cx.frame_fed() {
                            // Writes ride the JWT Session: hold one ready so a first write is one leg.
                            self.refresh(cx);
                            let _ = cx.finish_http_context(&context);
                            return;
                        }
                        self.connect(cx);
                        let symbols: Vec<String> = cx
                            .store
                            .markets()
                            .iter()
                            .map(|m| m.symbol.clone())
                            .collect();
                        for symbol in symbols {
                            cx.store.update_setting(&symbol, |s| {
                                s.leverage = 10.;
                                s.margin_type = Some(MarginType::Cross);
                            });
                        }
                        self.margins(cx, true);
                        let _ = cx.finish_http_context(&context);
                    }
                    Err(error) => self.on_failed(cx, Request::Config(context), &error),
                }
            }
            Request::Auth { started } => {
                let Some(token) = auth_token(response.body) else {
                    self.auth_failed(cx, "Paradex authentication returned no JWT");
                    return;
                };
                if SessionHeaders::bearer(&token).is_err() {
                    self.auth_failed(cx, "Paradex authentication returned an invalid JWT");
                    return;
                }
                self.auth_in_flight = false;
                if cx.now >= started + 300_000. {
                    self.token = None;
                    self.refresh_at = 0.;
                    self.refresh(cx);
                    return;
                }
                self.token = Some(token);
                self.bearer_revision += 1;
                self.refresh_at = started + AUTH_REFRESH_MS;
                if let Some(session) = &self.session {
                    // Wakes reads parked for this JWT. Paradex never parks a
                    // continuation through `session_wait`, so no waiter comes back.
                    let waiters = cx.session_ready(session);
                    debug_assert!(waiters.is_empty(), "Paradex has no session waiters");
                }
                while let Some(request) = self.queue.pop_front() {
                    self.dispatch(cx, request);
                }
                if cx.frame_fed() {
                    // Renewed before it expires, so no write waits on `/auth`.
                    let due = (self.refresh_at - cx.now - JWT_RENEW_EARLY_MS).max(0.);
                    self.renew = Some(cx.set_timer(due));
                    return;
                }
                self.send_auth(cx);
            }
            Request::Margins {
                revision,
                rows,
                boot,
                ..
            } => {
                self.margins_busy = false;
                if revision == self.margins_revision {
                    for (symbol, leverage) in rows {
                        cx.store.update_setting(&symbol, |s| {
                            s.leverage = leverage;
                            s.margin_type = Some(MarginType::Cross);
                        });
                    }
                }
                if boot {
                    self.positions(cx, true);
                } else if self.poll.is_none() {
                    self.poll = Some(cx.set_poll_timer(POLL_MS));
                }
            }
            Request::Positions {
                revision,
                started,
                rows,
                coverage,
                cycle,
                ..
            } => {
                self.positions_busy = false;
                if revision == self.positions_revision {
                    let mut snapshot = Vec::new();
                    for row in rows {
                        let updated = row.last_update.unwrap_or(started);
                        let stale =
                            self.position_versions
                                .get(&row.symbol)
                                .is_some_and(|(old, closed)| {
                                    *old > updated || (*old == updated && *closed)
                                });
                        if stale {
                            if let Some(current) = cx.store.position(&row.id) {
                                snapshot.push(current.clone());
                            }
                        } else {
                            self.position_versions
                                .insert(row.symbol.clone(), (updated, false));
                            snapshot.push(row);
                        }
                    }
                    // Kept before the closure pass so skipped rows never record a close.
                    for old in cx.store.positions() {
                        if !coverage.allows_absence(&old.symbol)
                            && !snapshot.iter().any(|row| row.symbol == old.symbol)
                        {
                            snapshot.push(old.clone());
                        }
                    }
                    for old in cx.store.positions() {
                        if !snapshot.iter().any(|row| row.symbol == old.symbol) {
                            self.position_versions
                                .insert(old.symbol.clone(), (started, true));
                        }
                    }
                    cx.store.set_positions(snapshot);
                    self.update_upnl(cx);
                }
                if cycle {
                    self.balance(cx, true);
                }
            }
            Request::Balance {
                revision, cycle, ..
            } => {
                if revision == self.balance_revision
                    && !self.balance_message_inner(cx, response.body, false)
                {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid Paradex balance snapshot",
                    );
                }
                self.balance_busy = false;
                if cycle {
                    self.orders(cx, true);
                }
            }
            Request::Orders {
                token,
                rows,
                coverage,
                invalid,
                cycle,
                ..
            } => {
                self.orders_busy = false;
                if !invalid {
                    let clean = coverage.rejected() == 0;
                    // Decode all fallible mapping before entering native publication.
                    let prepared = rows
                        .into_iter()
                        .filter_map(|row| {
                            let order = map_order(&row).ok()?;
                            let version = OrderVersion::new(&row).ok()?;
                            Some((row, order, version))
                        })
                        .collect();
                    let committed = cx.store.commit_orders_snapshot([token], |current| {
                        self.publish_orders_snapshot(prepared, coverage, current)
                    });
                    if committed && clean {
                        cx.orders_reconciled(None);
                    }
                } else {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Invalid Paradex orders snapshot",
                    );
                }
                self.owed_orders(cx);
                if cycle {
                    self.margins(cx, false);
                }
            }
        }
    }
    /// Runs only after Store has validated the read's complete authority.
    fn publish_orders_snapshot(
        &mut self,
        rows: Vec<(NativeOrder, Order, OrderVersion)>,
        coverage: SnapshotCoverage<String>,
        current_rows: &[Order],
    ) -> Vec<Order> {
        let mut mapped = vec![];
        let mut native = HashMap::new();
        for (row, order, version) in rows {
            if self.terminals.contains(&row.id) {
                continue;
            }
            let evidence = if self.stale_order(&row.id, version) {
                OrderEvidence::Ignore
            } else {
                self.order_evidence(&row, version, order.status != OrderStatus::Open)
            };
            if evidence == OrderEvidence::Ignore {
                // A read started after a live event can still hit an
                // older replica. Preserve both store and signing
                // evidence, without reviving invalidated amend data.
                if let Some(current) = current_rows.iter().find(|current| current.id == row.id)
                    && current.status == OrderStatus::Open
                {
                    mapped.push(current.clone());
                    if let Some(current) = self.native.get(&row.id) {
                        native.insert(row.id.clone(), current.clone());
                    }
                }
                continue;
            }
            self.remember_order(&row.id, version);
            if order.status == OrderStatus::Open {
                mapped.push(order);
                if evidence == OrderEvidence::Coherent {
                    native.insert(row.id.clone(), row);
                }
            } else {
                self.terminal_order(&row.id);
            }
        }
        for current in current_rows {
            if !coverage.allows_absence(&current.id)
                && current.status == OrderStatus::Open
                && !self.terminals.contains(&current.id)
                && !mapped.iter().any(|o: &Order| o.id == current.id)
            {
                mapped.push(current.clone());
                if let Some(row) = self.native.get(&current.id) {
                    native.insert(current.id.clone(), row.clone());
                }
            }
        }
        self.native = native;
        mapped
    }
    pub fn on_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let Some(context) = request.read_context() {
            let _ = cx.finish_http_context(context);
        }
        match request {
            Request::Config(context) => {
                let _ = cx.finish_http_context(&context);
                self.config_failed(cx, error);
            }
            Request::Auth { .. } => self.auth_failed(cx, error),
            Request::Margins { boot, .. } => self.margins_ended(cx, boot),
            Request::Positions { cycle, .. } => self.positions_ended(cx, cycle),
            Request::Balance { cycle, .. } => self.balance_ended(cx, cycle),
            Request::Orders { cycle, .. } => self.orders_ended(cx, cycle),
        }
    }
    fn margins_ended(&mut self, cx: &mut Cx<'_, Pending>, boot: bool) {
        self.margins_busy = false;
        if boot {
            self.positions(cx, true);
        } else if self.poll.is_none() {
            self.poll = Some(cx.set_poll_timer(POLL_MS));
        }
    }
    fn positions_ended(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        self.positions_busy = false;
        if cycle {
            self.balance(cx, true);
        }
    }
    fn balance_ended(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        self.balance_busy = false;
        if cycle {
            self.orders(cx, true);
        }
    }
    fn orders_ended(&mut self, cx: &mut Cx<'_, Pending>, cycle: bool) {
        self.orders_busy = false;
        self.owed_orders(cx);
        if cycle {
            self.margins(cx, false);
        }
    }
    fn order_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        let Ok(rows) = raw_rows(text) else {
            return false;
        };
        let mut valid = false;
        for raw in rows {
            match registered_market(cx, &self.excluded_perps, raw.get(), false) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(()) => {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        "Paradex private update contains an unmappable market",
                    );
                    if self.repair_orders.is_none() {
                        self.repair_orders = Some(cx.set_timer(500.));
                    }
                    continue;
                }
            }
            if let Ok(row) = serde_json::from_str::<NativeOrder>(raw.get()) {
                valid |= self.apply_order(cx, row);
            }
        }
        valid
    }
    fn order_evidence(
        &mut self,
        row: &NativeOrder,
        version: OrderVersion,
        terminal: bool,
    ) -> OrderEvidence {
        if self.terminals.contains(&row.id) {
            return OrderEvidence::Ignore;
        }
        let Some(fence) = self.amend_fences.get_mut(&row.id) else {
            return OrderEvidence::Coherent;
        };
        if !fence.target.same_identity(row) {
            return OrderEvidence::Ignore;
        }
        if terminal {
            return OrderEvidence::Coherent;
        }
        if fence.target.matches(row) {
            fence.floor = fence.floor.max(version.updated);
            return OrderEvidence::Coherent;
        }
        if fence.pending {
            // Executions can advance while the write is in flight. They remain
            // visible, but old mutable attributes are never signing evidence.
            fence.floor = fence.floor.max(version.updated);
            return OrderEvidence::Unresolved;
        }
        if version.updated > fence.floor {
            // A strictly newer coherent venue version may be an external edit.
            if let Ok(target) =
                AmendTarget::new(row, &row.size, &row.price, Some(&row.trigger_price))
            {
                fence.target = target;
                fence.floor = version.updated;
                return OrderEvidence::Coherent;
            }
        }
        OrderEvidence::Ignore
    }
    fn stale_order(&self, id: &str, version: OrderVersion) -> bool {
        let old = self.order_versions.get(id).copied().or_else(|| {
            self.native
                .get(id)
                .and_then(|row| OrderVersion::new(row).ok())
        });
        old.is_some_and(|old| version.updated < old.updated || version.filled < old.filled)
    }
    fn remember_order(&mut self, id: &str, version: OrderVersion) {
        if self.order_versions.insert(id.into(), version).is_none() {
            self.order_version_ids.push_back(id.into());
        }
        while self.order_version_ids.len() > RETAINED {
            if let Some(id) = self.order_version_ids.pop_front() {
                self.order_versions.remove(&id);
            }
        }
    }
    fn terminal_order(&mut self, id: &str) {
        self.amend_fences.remove(id);
        if self.terminals.insert(id.into()) {
            self.terminal_ids.push_back(id.into());
        }
        while self.terminal_ids.len() > RETAINED {
            if let Some(id) = self.terminal_ids.pop_front() {
                self.terminals.remove(&id);
            }
        }
    }
    fn apply_order(&mut self, cx: &mut Cx<'_, Pending>, row: NativeOrder) -> bool {
        if cx.store.market(&row.market).is_none() {
            return false;
        }
        let (Ok(mut order), Ok(version)) = (map_order(&row), OrderVersion::new(&row)) else {
            return false;
        };
        if self.terminals.contains(&row.id) && order.status == OrderStatus::Open {
            return false;
        }
        if self.stale_order(&row.id, version) {
            return false;
        }
        let evidence = self.order_evidence(&row, version, order.status != OrderStatus::Open);
        if evidence == OrderEvidence::Ignore {
            return false;
        }
        self.remember_order(&row.id, version);
        self.bump_orders(cx);
        order.last_update = cx.now;
        if order.status != OrderStatus::Open {
            self.native.remove(&row.id);
            self.terminal_order(&row.id);
        } else if evidence == OrderEvidence::Coherent {
            self.native.insert(row.id.clone(), row.clone());
        } else {
            self.native.remove(&row.id);
        }
        if !row.cancel_reason.is_empty() {
            cx.notice(
                Level::Error,
                code::ORDER_REJECTED,
                format!("Paradex order rejected: {}", row.cancel_reason),
            );
        }
        cx.store.upsert_order(order);
        if self.repair_orders.is_none() {
            self.repair_orders = Some(cx.set_timer(1_000.));
        }
        true
    }
    fn position_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        let Ok(rows) = raw_rows(text) else {
            return false;
        };
        let mut valid = false;
        for raw in rows {
            match registered_market(cx, &self.excluded_perps, raw.get(), true) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(()) => {
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        "Paradex private update contains an unmappable market",
                    );
                    if self.repair_positions.is_none() {
                        self.repair_positions = Some(cx.set_timer(500.));
                    }
                    continue;
                }
            }
            if let Ok(row) = serde_json::from_str::<PositionRow<'_>>(raw.get()) {
                valid |= self.apply_position(cx, row);
            }
        }
        valid
    }
    fn apply_position(&mut self, cx: &mut Cx<'_, Pending>, row: PositionRow<'_>) -> bool {
        if cx.store.market(row.market).is_none() {
            return false;
        }
        let updated = if row.last_updated_at.0 > 0. {
            row.last_updated_at.0
        } else {
            cx.now
        };
        let closed = row.status == "CLOSED" || row.size.0 == 0.;
        if self
            .position_versions
            .get(row.market)
            .is_some_and(|(old, was_closed)| {
                *old > updated || (*old == updated && *was_closed && !closed)
            })
        {
            return false;
        }
        match map_position(&row, cx) {
            Ok(Some(position)) => {
                self.bump_positions();
                cx.store.upsert_position(position);
            }
            Ok(None) => {
                self.bump_positions();
                let symbol = row.market;
                cx.store.update_positions(|p| {
                    if p.symbol == symbol {
                        p.contracts = 0.;
                    }
                });
            }
            Err(_) => return false,
        }
        self.position_versions
            .insert(row.market.into(), (updated, closed));
        self.update_upnl(cx);
        if self.repair_positions.is_none() {
            self.repair_positions = Some(cx.set_timer(500.));
        }
        true
    }
    fn update_upnl(&self, cx: &mut Cx<'_, Pending>) {
        if !cx.store.loaded().balance {
            return;
        }
        let mut balance = cx.store.balance().clone();
        balance.upnl = cx.store.positions().iter().map(|p| p.unrealized_pnl).sum();
        cx.store.set_balance(balance);
    }
    fn balance_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        self.balance_message_inner(cx, text, true)
    }
    fn balance_message_inner(&mut self, cx: &mut Cx<'_, Pending>, text: &str, bump: bool) -> bool {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Payload {
            Rows(Vec<BalanceRow>),
            Nested { data: BalanceRow },
            One(BalanceRow),
        }
        let Ok(payload) = serde_json::from_str::<Payload>(text) else {
            return false;
        };
        let row = match payload {
            Payload::Rows(rows) => {
                let Some(row) = rows.into_iter().next() else {
                    return false;
                };
                row
            }
            Payload::Nested { data } => data,
            Payload::One(row) => row,
        };
        let Some(total) = row
            .total_collateral
            .or(row.account_value)
            .or(row.equity)
            .or(row.margin_balance)
            .or(row.wallet_balance)
        else {
            return false;
        };
        let Some(free) = row
            .free_collateral
            .or(row.free)
            .or(row.available_margin)
            .or(row.available_collateral)
        else {
            return false;
        };
        let used = row
            .initial_margin_requirement
            .or(row.position_margin)
            .or(row.used_margin)
            .map(|n| n.0)
            .unwrap_or((total.0 - free.0).max(0.));
        if bump {
            self.balance_revision = self.balance_revision.wrapping_add(1);
        }
        cx.store.set_balance(Balance {
            total: total.0,
            free: free.0,
            used,
            upnl: cx.store.positions().iter().map(|p| p.unrealized_pnl).sum(),
        });
        true
    }
    fn fill_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) -> bool {
        let Ok(rows) = raw_rows(text) else {
            return false;
        };
        let mut valid = false;
        for raw in rows {
            if registered_market(cx, &self.excluded_perps, raw.get(), false) != Ok(true) {
                continue;
            }
            if let Ok(row) = serde_json::from_str::<FillRow<'_>>(raw.get()) {
                valid |= self.apply_fill(cx, row);
            }
        }
        valid
    }
    fn apply_fill(&mut self, cx: &mut Cx<'_, Pending>, row: FillRow<'_>) -> bool {
        if cx.store.market(row.market).is_none() {
            return false;
        }
        let side = match row.side {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return false,
        };
        if row.id.is_empty()
            || row.order_id.is_empty()
            || row.market.is_empty()
            || row.price.0 <= 0.
            || row.size.0 <= 0.
        {
            return false;
        }
        if !self.executions.insert(row.id.into()) {
            return true;
        }
        self.execution_ids.push_back(row.id.into());
        while self.execution_ids.len() > RETAINED {
            if let Some(id) = self.execution_ids.pop_front() {
                self.executions.remove(&id);
            }
        }
        cx.fill(Fill {
            id: None,
            order_id: row.order_id.into(),
            symbol: row.market.into(),
            side,
            price: row.price.0,
            amount: row.size.0,
        });
        true
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NativeOrder {
    pub id: String,
    pub market: String,
    pub side: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub size: String,
    pub remaining_size: String,
    pub price: String,
    pub trigger_price: String,
    pub flags: Option<Vec<String>>,
    pub reduce_only: Option<bool>,
    pub instruction: String,
    pub post_only: Option<bool>,
    pub time_in_force: String,
    pub on_behalf_of_account: String,
    #[serde(alias = "client_order_id")]
    pub client_id: String,
    pub cancel_reason: String,
    #[serde(deserialize_with = "scalar")]
    pub created_at: String,
    #[serde(deserialize_with = "scalar")]
    pub last_updated_at: String,
}
impl NativeOrder {
    fn updated(&self) -> f64 {
        self.last_updated_at
            .parse::<f64>()
            .ok()
            .filter(|stamp| stamp.is_finite() && *stamp > 0.)
            .unwrap_or(0.)
    }
}
fn scalar<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum S {
        Text(String),
        Number(u64),
    }
    S::deserialize(d).map(|s| match s {
        S::Text(s) => s,
        S::Number(n) => n.to_string(),
    })
}
fn finite(input: &str) -> Result<f64, String> {
    safe_cex_engine::num::parse_decimal(input).ok_or_else(|| "Invalid Paradex numeric field".into())
}
pub fn map_order(row: &NativeOrder) -> Result<Order, String> {
    if row.id.is_empty() || row.market.is_empty() {
        return Err("Invalid Paradex order identity".into());
    }
    let side = match row.side.as_str() {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        _ => return Err("Invalid Paradex order side".into()),
    };
    let reduce_only = row.reduce_only.unwrap_or_else(|| {
        row.flags
            .as_ref()
            .is_some_and(|flags| flags.iter().any(|f| f == "REDUCE_ONLY"))
    });
    let kind = match row.kind.as_str() {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP_LIMIT" | "STOP_MARKET" if !reduce_only => OrderType::Conditional,
        "STOP_LIMIT" | "STOP_MARKET" | "STOP_LOSS_LIMIT" | "STOP_LOSS_MARKET" => {
            OrderType::StopLoss
        }
        "TAKE_PROFIT_LIMIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => return Err("Unsupported Paradex order type".into()),
    };
    let amount = finite(&row.size)?;
    let remaining = finite(&row.remaining_size)?;
    let status = match row.status.as_str() {
        "NEW" | "OPEN" | "PENDING" | "UNTRIGGERED" => OrderStatus::Open,
        "CLOSED" if remaining > 0. => OrderStatus::Canceled,
        "FILLED" | "CLOSED" => OrderStatus::Closed,
        "CANCELED" | "REJECTED" | "EXPIRED" => OrderStatus::Canceled,
        _ => return Err("Invalid Paradex order status".into()),
    };
    if amount <= 0. || remaining < 0. || remaining > amount {
        return Err("Invalid Paradex order quantity".into());
    }
    let trigger = matches!(
        kind,
        OrderType::Conditional | OrderType::StopLoss | OrderType::TakeProfit
    );
    let price = if trigger && !row.trigger_price.is_empty() {
        finite(&row.trigger_price)?
    } else if row.price.is_empty() {
        0.
    } else {
        finite(&row.price)?
    };
    Ok(Order {
        id: row.id.clone(),
        parent_id: None,
        status,
        symbol: row.market.clone(),
        kind,
        side,
        price,
        amount,
        filled: amount - remaining,
        remaining,
        reduce_only,
        close: None,
        client_order_id: (!row.client_id.is_empty()).then(|| row.client_id.clone()),
        time_in_force: if row.post_only == Some(true) {
            Some(TimeInForce::PostOnly)
        } else {
            match if row.instruction.is_empty() {
                row.time_in_force.as_str()
            } else {
                row.instruction.as_str()
            } {
                "GTC" | "GTT" | "GoodTillCancel" => Some(TimeInForce::GoodTillCancel),
                "IOC" | "ImmediateOrCancel" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" | "FillOrKill" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" | "PostOnly" => Some(TimeInForce::PostOnly),
                _ => None,
            }
        },
        last_update: row.created_at.parse().unwrap_or(0.),
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    })
}
#[derive(Deserialize)]
struct Margin<'a> {
    #[serde(borrow)]
    market: &'a str,
    leverage: Number,
}

fn optional_position_number<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Number, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OptionalNumber<'a> {
        Value(Number),
        Text(#[serde(borrow)] &'a str),
        Null(()),
    }
    // TS applies `parseFloat(value || '0')` to these two optional fields.
    // Keep every nonempty value on the strict private numeric contract.
    match OptionalNumber::deserialize(deserializer)? {
        OptionalNumber::Value(number) => Ok(number),
        OptionalNumber::Text("") | OptionalNumber::Null(()) => Ok(Number(0.)),
        OptionalNumber::Text(_) => {
            Err(serde::de::Error::custom("invalid optional position number"))
        }
    }
}

#[derive(Deserialize)]
struct PositionRow<'a> {
    #[serde(borrow)]
    market: &'a str,
    #[serde(borrow)]
    status: &'a str,
    #[serde(borrow, default)]
    side: &'a str,
    size: Number,
    #[serde(default)]
    average_entry_price: Number,
    #[serde(default)]
    unrealized_pnl: Number,
    #[serde(default)]
    realized_positional_pnl: Number,
    #[serde(default, deserialize_with = "optional_position_number")]
    leverage: Number,
    #[serde(default, deserialize_with = "optional_position_number")]
    liquidation_price: Number,
    #[serde(default)]
    last_updated_at: Number,
}
fn map_position(row: &PositionRow<'_>, cx: &Cx<'_, Pending>) -> Result<Option<Position>, String> {
    if row.market.is_empty() {
        return Err("Invalid Paradex position market".into());
    }
    if row.status == "CLOSED" || row.size.0 == 0. {
        return Ok(None);
    }
    if row.status != "OPEN" {
        return Err("Invalid Paradex position status".into());
    }
    let side = match row.side {
        "LONG" => PositionSide::Long,
        "SHORT" => PositionSide::Short,
        _ => return Err("Invalid Paradex position side".into()),
    };
    let contracts = row.size.0.abs();
    let leverage = if row.leverage.0 > 0. {
        row.leverage.0
    } else {
        cx.store
            .setting(row.market)
            .map(|s| s.leverage)
            .filter(|l| *l > 0.)
            .unwrap_or(10.)
    };
    Ok(Some(Position {
        id: row.market.into(),
        symbol: row.market.into(),
        side,
        entry_price: row.average_entry_price.0,
        notional: contracts * row.average_entry_price.0,
        leverage,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: row.unrealized_pnl.0,
        realized_pnl: Some(row.realized_positional_pnl.0),
        contracts,
        liquidation_price: row.liquidation_price.0,
        margin_size: None,
        last_update: Some(row.last_updated_at.0),
    }))
}
#[derive(Deserialize, Default)]
#[serde(default)]
struct BalanceRow {
    total_collateral: Option<Number>,
    account_value: Option<Number>,
    equity: Option<Number>,
    margin_balance: Option<Number>,
    wallet_balance: Option<Number>,
    free_collateral: Option<Number>,
    free: Option<Number>,
    available_margin: Option<Number>,
    available_collateral: Option<Number>,
    initial_margin_requirement: Option<Number>,
    position_margin: Option<Number>,
    used_margin: Option<Number>,
}
#[derive(Deserialize)]
struct FillRow<'a> {
    #[serde(borrow)]
    id: &'a str,
    #[serde(borrow)]
    order_id: &'a str,
    #[serde(borrow)]
    market: &'a str,
    #[serde(borrow)]
    side: &'a str,
    price: Number,
    size: Number,
}
#[derive(Deserialize)]
struct Message<'a> {
    #[serde(default, borrow)]
    jsonrpc: &'a str,
    #[serde(default, borrow)]
    method: &'a str,
    #[serde(default, borrow, rename = "type")]
    kind: &'a str,
    #[serde(default)]
    id: Option<u64>,
    #[serde(default, borrow)]
    result: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    error: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    params: Option<Params<'a>>,
}
#[derive(Deserialize)]
struct Params<'a> {
    #[serde(borrow)]
    channel: &'a str,
    #[serde(borrow)]
    data: &'a serde_json::value::RawValue,
}

fn binary_side(value: u8) -> Result<&'static str, &'static str> {
    match value {
        1 => Ok("BUY"),
        2 => Ok("SELL"),
        _ => Err("Unknown Paradex SBE side"),
    }
}
fn decimal8(mantissa: i64) -> String {
    if mantissa == i64::MIN {
        return String::new();
    }
    let value = mantissa.unsigned_abs();
    let sign = if mantissa < 0 { "-" } else { "" };
    let integer = value / 100_000_000;
    let fraction = value % 100_000_000;
    if fraction == 0 {
        return format!("{sign}{integer}");
    }
    let fraction = format!("{fraction:08}");
    format!("{sign}{integer}.{}", fraction.trim_end_matches('0'))
}

/// The market a row names, when it names one.
fn row_market(raw: &str) -> Option<&str> {
    #[derive(Deserialize)]
    struct Identity<'a> {
        #[serde(borrow)]
        market: &'a str,
    }
    serde_json::from_str::<Identity<'_>>(raw)
        .ok()
        .map(|row| row.market)
        .filter(|market| !market.is_empty())
}
fn registered_market(
    cx: &Cx<'_, Pending>,
    excluded_perps: &HashSet<String>,
    raw: &str,
    position: bool,
) -> Result<bool, ()> {
    #[derive(Deserialize)]
    struct Identity<'a> {
        #[serde(borrow)]
        market: &'a str,
        #[serde(default, borrow)]
        size: Option<&'a serde_json::value::RawValue>,
        #[serde(default, borrow)]
        status: Option<&'a serde_json::value::RawValue>,
    }
    let identity: Identity<'_> = serde_json::from_str(raw).map_err(|_| ())?;
    if identity.market.is_empty() {
        return Err(());
    }
    if cx.store.market(identity.market).is_some() {
        return Ok(true);
    }
    if excluded_perps.contains(identity.market) {
        return Ok(false);
    }
    // Options/spot are intentionally outside this perpetual adapter. An unknown
    // perpetual may be a malformed catalogue row, so it cannot prove absence of
    // working orders or positions in an authoritative account snapshot.
    if identity.market.ends_with("-PERP") {
        if position
            && (identity
                .status
                .and_then(|status| serde_json::from_str::<&str>(status.get()).ok())
                == Some("CLOSED")
                || identity
                    .size
                    .and_then(|size| serde_json::from_str::<Number>(size.get()).ok())
                    .is_some_and(|size| size.0 == 0.0))
        {
            return Ok(false);
        }
        return Err(());
    }
    Ok(false)
}

fn raw_rows(text: &str) -> Result<Vec<&serde_json::value::RawValue>, serde_json::Error> {
    if text.trim_start().starts_with('[') {
        serde_json::from_str(text)
    } else {
        serde_json::from_str(text).map(|row| vec![row])
    }
}

pub(crate) fn auth_rejected_error() -> VenueError {
    VenueError::new("Paradex rejected the account credentials")
        .kind(safe_cex_engine::ErrorKind::AuthInvalid)
        .outcome(safe_cex_engine::Outcome::NotSent)
}

/// The JWT a successful `/auth` returns, if it carries one.
pub(crate) fn auth_token(body: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Token {
        jwt_token: Option<String>,
        token: Option<String>,
    }
    serde_json::from_str::<Token>(body).ok().and_then(|t| {
        t.jwt_token
            .filter(|s| !s.trim().is_empty())
            .or(t.token.filter(|s| !s.trim().is_empty()))
    })
}
