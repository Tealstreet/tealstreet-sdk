//! Account reads and authenticated streams for USDC perpetuals. The gateway
//! session is enabled only after a correlated successful CDP exchange.
#[path = "private_drb_history.rs"]
mod history;
#[path = "private_drb_wire.rs"]
pub(crate) mod wire;
use crate::{Coinbase, Pending, drb, drb_auth};
use safe_cex_engine::classify::{self, Intent, Native, NativeCode, Released};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{
    Context, Egress, Operation, Owner, Policy, RetryPolicy, Scope, Session, SessionBearer,
    SessionHeaders, SessionPolicy,
};
use safe_cex_engine::startup::Sections;
use safe_cex_engine::{
    Cx, ErrorKind, Fill, FillFinancials, Level, Response, SocketId, TimerId, VenueError,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use wire::{order, position, summary};

const CHANNELS: [&str; 3] = [
    "user.changes.future.USDC.100ms",
    "user.portfolio.USDC",
    "user.isolated.changes.future.USDC.100ms",
];
const MAX_ROWS: usize = 10_000;
#[derive(Clone)]
pub struct Request {
    ticket: u64,
    sections: Sections,
}
impl Request {
    pub(crate) fn startup_sections(&self) -> Sections {
        self.sections
    }
}
enum Kind {
    Auth,
    Summary,
    Positions,
    Orders,
    History(Box<history::Request>),
}
impl Kind {
    fn slot(&self) -> usize {
        match self {
            Self::Auth => 0,
            Self::Summary => 1,
            Self::Positions => 2,
            Self::Orders => 3,
            Self::History(_) => 4,
        }
    }
    fn sections(&self) -> Sections {
        match self {
            Self::Auth => Sections::ACCOUNT,
            Self::Summary => Sections::BALANCE,
            Self::Positions => Sections::POSITIONS,
            Self::Orders => Sections::ORDERS,
            Self::History(_) => Sections::NONE,
        }
    }
}
struct InFlight {
    orders_token: Option<safe_cex_engine::store::OrderSnapshotToken>,
    rpc: drb::RpcId,
    context: Context,
    kind: Kind,
    started: f64,
    revision: u64,
    bearer_revision: u64,
}
enum WsKind {
    Auth,
    Subscribe,
    Heartbeat,
    Test,
}
struct WsRequest {
    kind: WsKind,
    started: f64,
}
pub(crate) struct Private {
    pub(crate) enabled: bool,
    active: bool,
    auth_refused: bool,
    ready: bool,
    http: drb_auth::Session,
    bearer: Option<Session>,
    bearer_revision: u64,
    now: f64,
    next: u64,
    pending: BTreeMap<u64, InFlight>,
    revisions: [u64; 4],
    timer: Option<TimerId>,
    next_poll: f64,
    socket: Option<SocketId>,
    ws: Option<drb_auth::Session>,
    ws_pending: BTreeMap<drb::RpcId, WsRequest>,
    ws_ready: bool,
    payload: f64,
    reconnect_at: f64,
    auth_retry_at: f64,
    failures: u32,
    versions: HashMap<String, f64>,
    versions_fifo: VecDeque<String>,
    trades: HashSet<String>,
    trade_fifo: VecDeque<String>,
}
impl Default for Private {
    fn default() -> Self {
        Self {
            enabled: false,
            active: false,
            auth_refused: false,
            ready: false,
            http: drb_auth::Session::new(drb::Channel::Http),
            bearer: None,
            bearer_revision: 0,
            now: 0.,
            next: 0,
            pending: BTreeMap::new(),
            revisions: [0; 4],
            timer: None,
            next_poll: 0.,
            socket: None,
            ws: None,
            ws_pending: BTreeMap::new(),
            ws_ready: false,
            payload: 0.,
            reconnect_at: 0.,
            auth_retry_at: 0.,
            failures: 0,
            versions: HashMap::new(),
            versions_fifo: VecDeque::new(),
            trades: HashSet::new(),
            trade_fifo: VecDeque::new(),
        }
    }
}
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: drb::REQUEST_TIMEOUT_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
fn warn(cx: &mut Cx<'_, Pending>, message: impl Into<String>) {
    cx.notice(Level::Warn, code::REQUEST_FAILED, message);
}
/// Project S0's verdict without reclassifying native strings or acquiring any
/// mutation/resend authority. Native fields remain structured error evidence.
fn rpc_read_error(
    failure: &drb::Failure,
    released: Released<'_>,
    message: impl Into<String>,
) -> VenueError {
    let native = failure.code.map(|code| Native {
        code: NativeCode::Int(code),
        message: failure
            .rpc_error
            .as_ref()
            .map_or("", |error| error.message.as_str()),
    });
    classify::error(
        failure.verdict,
        Intent::Read,
        released,
        native.as_ref(),
        message,
    )
}
fn rows(v: Value) -> Result<Vec<Value>, String> {
    let rows: Vec<Value> =
        serde_json::from_value(v).map_err(|_| "drb expected result array".to_owned())?;
    if rows.len() > MAX_ROWS {
        return Err("drb account result exceeds row limit".into());
    }
    Ok(rows)
}
impl Private {
    fn serves(&self, generation: u64) -> bool {
        self.bearer
            .as_ref()
            .is_some_and(|s| s.owner() == Owner::Session { generation })
    }
    pub(crate) fn bearer(&self, generation: u64) -> SessionBearer {
        if !self.serves(generation) {
            return SessionBearer::NotReady;
        }
        let headers = self
            .http
            .authorization(self.now)
            .and_then(|value| SessionHeaders::bearer(value.strip_prefix("Bearer ")?).ok());
        match headers {
            Some(headers) => SessionBearer::Ready {
                headers,
                revision: self.bearer_revision,
            },
            None => SessionBearer::NotReady,
        }
    }
    pub(crate) fn bearer_revision(&self, generation: u64) -> Option<u64> {
        (self.serves(generation) && !self.http.needs_refresh(self.now))
            .then_some(self.bearer_revision)
    }
}
impl Coinbase {
    /// Capability hook for callers displaying the retired-INTX migration notice.
    /// A configured CDP key alone is not evidence of a successful gateway login.
    pub fn drb_authenticated(&self) -> bool {
        self.private_drb.active
    }
    pub(crate) fn drb_private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.private_drb.enabled = true;
        self.private_drb.now = cx.now;
        if self.private_drb.bearer.is_none() {
            let Ok((selection, _)) = self.signers.select(CredentialRole::SessionAuth) else {
                warn(cx, "drb credentials unavailable");
                return;
            };
            match cx.open_session(SessionPolicy {
                credential: None,
                repeat_safe: false,
                private_egress: selection.session_egress(),
            }) {
                Ok(s) => self.private_drb.bearer = Some(s),
                Err(e) => {
                    warn(cx, e.to_string());
                    return;
                }
            }
        }
        self.drb_ensure_http(cx);
        self.drb_schedule(cx);
    }
    fn drb_schedule(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.private_drb.auth_refused && self.private_drb.timer.is_none() {
            self.private_drb.timer = Some(cx.set_timer(1000.));
        }
    }
    /// Opaque engine bearer binding shared with the trading consumer. No token
    /// bytes leave the private module; preparation still asks Venue for headers.
    pub(crate) fn drb_http_session(&self) -> Option<&Session> {
        if !self.private_drb.active || self.private_drb.http.needs_refresh(self.private_drb.now) {
            return None;
        }
        self.private_drb.bearer.as_ref()
    }
    /// Start or refresh the shared HTTP login without consuming a caller's
    /// command root. Consumers retain that root while authentication is pending.
    pub(crate) fn drb_ensure_http(&mut self, cx: &mut Cx<'_, Pending>) {
        self.private_drb.now = cx.now;
        if self.private_drb.auth_refused {
            return;
        }
        if !self.private_drb.enabled {
            if self.account.is_intx() && !self.options.spot_only {
                self.drb_private_start(cx);
            }
            return;
        }
        if !self.private_drb.http.needs_refresh(cx.now) || cx.now < self.private_drb.auth_retry_at {
            return;
        }
        if self
            .private_drb
            .pending
            .values()
            .any(|p| matches!(p.kind, Kind::Auth))
        {
            return;
        }
        let params = self
            .signers
            .select(CredentialRole::SessionAuth)
            .ok()
            .map(|(_, credentials)| credentials)
            .ok_or("drb credentials unavailable".to_owned())
            .and_then(|c| {
                if c.identity().is_some() {
                    self.private_drb.http.pending_params(cx.now)
                } else {
                    self.private_drb
                        .http
                        .params_at(c, cx.now, cx.protocol_now(), |b| cx.fill_signing_entropy(b))
                }
            });
        match params {
            Ok(params) => self.drb_read(cx, Kind::Auth, "public/auth", params),
            Err(e) => warn(cx, e),
        }
    }
    fn drb_read(&mut self, cx: &mut Cx<'_, Pending>, kind: Kind, method: &str, params: Value) {
        self.private_drb.now = cx.now;
        let authentication = if matches!(kind, Kind::Auth) {
            match self.signers.select(CredentialRole::SessionAuth) {
                Ok((selection, credentials)) => Some((
                    selection,
                    credentials.identity().cloned(),
                    credentials.binding(),
                )),
                Err(_) => {
                    self.drb_read_failed(cx, kind, VenueError::new("drb credentials unavailable"));
                    return;
                }
            }
        } else {
            None
        };
        let issued = match &kind {
            Kind::History(h) => cx.http_leg_context(h.owner.root(), policy()),
            Kind::Auth => {
                let mut policy = policy();
                if let Some((selection, Some(identity), _)) = &authentication {
                    policy.credential = Some(selection.authority(identity));
                }
                cx.http_live_context_with_egress(Scope::Background, policy, Egress::Primary)
            }
            _ => cx.http_live_context(Scope::Background, policy()),
        };
        let context = match issued {
            Ok(c) => c,
            Err(e) => {
                self.drb_read_failed(cx, kind, e.venue_error());
                return;
            }
        };
        self.private_drb.next += 1;
        let ticket = self.private_drb.next;
        let slot = kind.slot();
        let orders_token = if matches!(kind, Kind::Orders) {
            let Some(token) = cx.store.begin_orders_snapshot() else {
                let _ = cx.finish_http_context(&context);
                self.drb_read_failed(
                    cx,
                    kind,
                    VenueError::new("drb order snapshot authority exhausted"),
                );
                return;
            };
            Some(token)
        } else {
            None
        };
        let pending = Pending::PrivateDrb(Request {
            ticket,
            sections: kind.sections(),
        });
        let session = if matches!(kind, Kind::Auth) {
            None
        } else {
            self.drb_http_session()
        }
        .cloned();
        let issued = if let Some((_, Some(identity), binding)) = authentication {
            self.drb.http_auth(cx, &context, params, pending, |rpc| {
                Box::new(crate::sign::DrbAuth::new(
                    identity,
                    binding,
                    drb::Channel::Http,
                    rpc,
                ))
            })
        } else {
            self.drb
                .http(cx, &context, method, params, session.as_ref(), pending)
        };
        match issued {
            Ok((rpc, _)) => {
                self.private_drb.pending.insert(
                    ticket,
                    InFlight {
                        orders_token,
                        rpc,
                        context,
                        kind,
                        started: cx.now,
                        revision: if slot < 4 {
                            self.private_drb.revisions[slot]
                        } else {
                            0
                        },
                        bearer_revision: self.private_drb.bearer_revision,
                    },
                );
            }
            Err(e) => {
                let _ = cx.finish_http_context(&context);
                self.drb_read_failed(cx, kind, VenueError::new(e));
            }
        }
    }
    pub(crate) fn drb_after_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.private_drb.active || self.private_drb.ready || !cx.store.loaded().markets {
            return;
        }
        self.private_drb.ready = true;
        cx.store.set_hedged(false);
        cx.store.retain_position_ids();
        self.drb_poll(cx);
        self.drb_private_connect(cx);
    }
    pub(crate) fn drb_poll(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.private_drb.ready || self.private_drb.http.needs_refresh(cx.now) {
            return;
        }
        self.private_drb.next_poll = cx.now + 5000.;
        for (slot, method, params) in [
            (
                1,
                "private/get_account_summary",
                json!({"currency":"USDC","extended":true}),
            ),
            (
                2,
                "private/get_positions",
                json!({"currency":"USDC","kind":"future","include_isolated":true}),
            ),
            (
                3,
                "private/get_open_orders",
                json!({"kind":"future","include_isolated":true}),
            ),
        ] {
            if self
                .private_drb
                .pending
                .values()
                .any(|p| p.kind.slot() == slot)
            {
                continue;
            }
            self.drb_read(
                cx,
                match slot {
                    1 => Kind::Summary,
                    2 => Kind::Positions,
                    _ => Kind::Orders,
                },
                method,
                params,
            );
        }
    }
    pub(crate) fn drb_private_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        response: Response<'_>,
    ) {
        let Some(read) = self.private_drb.pending.remove(&pending.ticket) else {
            return;
        };
        self.private_drb.now = cx.now;
        let result = self
            .drb
            .response_at(read.rpc, response.status, response.body, cx.now);
        let valid = match &read.kind {
            Kind::History(h) => cx.validate_read_http_delivery(h.owner.root(), &read.context),
            _ => cx.validate_background_read_http_delivery(&read.context),
        };
        if valid.is_err() {
            let _ = cx.finish_http_context(&read.context);
            return;
        }
        match result {
            Ok(value) => {
                if matches!(read.kind, Kind::Auth) {
                    let result = self.private_drb.http.accept(value, read.started);
                    if let Err(e) = result {
                        self.drb_read_failed(cx, read.kind, VenueError::new(e));
                    } else {
                        self.private_drb.bearer_revision += 1;
                        self.private_drb.failures = 0;
                        if !self.private_drb.active {
                            self.private_drb.active = true;
                            self.drb_start(cx);
                        }
                        self.drb_after_markets(cx);
                        self.drb_trading_ready(cx);
                    }
                } else if read.bearer_revision == self.private_drb.bearer_revision {
                    match read.kind {
                        Kind::History(h) => self.drb_history_response(cx, *h, value),
                        kind => {
                            let slot = kind.slot();
                            if read.revision == self.private_drb.revisions[slot]
                                && let Err(e) =
                                    self.drb_publish(cx, &kind, value, read.orders_token)
                            {
                                warn(cx, e);
                            }
                        }
                    }
                } else {
                    self.drb_read_failed(
                        cx,
                        read.kind,
                        VenueError::new("drb stale bearer response"),
                    );
                }
            }
            Err(failure) => {
                let error = rpc_read_error(
                    &failure,
                    Released::Response(&response),
                    format!(
                        "drb private read failed (HTTP {}, code {:?})",
                        response.status, failure.code
                    ),
                );
                if matches!(read.kind, Kind::Auth)
                    && matches!(
                        failure.verdict,
                        safe_cex_engine::classify::Verdict::Refused(
                            ErrorKind::AuthInvalid
                                | ErrorKind::AuthExpired
                                | ErrorKind::AuthPermission
                        )
                    )
                {
                    self.drb_trading_auth_failed(
                        cx,
                        VenueError::new("drb HTTP authentication was refused")
                            .kind(ErrorKind::AuthInvalid),
                    );
                }
                if read.bearer_revision == self.private_drb.bearer_revision
                    && matches!(
                        error.kind,
                        Some(
                            ErrorKind::AuthInvalid
                                | ErrorKind::AuthExpired
                                | ErrorKind::AuthPermission
                        )
                    )
                {
                    self.private_drb.http.invalidate();
                    self.private_drb.bearer_revision += 1;
                    self.private_drb.active = false;
                    let loaded = cx.store.loaded();
                    if error.kind != Some(ErrorKind::AuthExpired)
                        || !(loaded.balance && loaded.orders && loaded.positions)
                    {
                        self.private_drb.auth_refused = true;
                        if let Some(timer) = self.private_drb.timer.take() {
                            cx.clear_timer(timer);
                        }
                        if let Some(socket) = self.private_drb.socket {
                            self.drb_private_close(cx, socket);
                        }
                        if !cx.startup_read_refused(&error) {
                            cx.notice_error(Level::Error, code::AUTH_REJECTED, &error);
                        }
                    }
                }
                self.drb_read_failed(cx, read.kind, error);
            }
        }
        let _ = cx.finish_http_context(&read.context);
    }
    fn drb_read_failed(&mut self, cx: &mut Cx<'_, Pending>, kind: Kind, error: VenueError) {
        cx.startup_read_failure(kind.sections(), &error);
        match kind {
            Kind::History(h) => {
                let _ = h.owner.finish(cx, Err(error));
            }
            Kind::Auth => {
                self.private_drb.http.invalidate();
                self.private_drb.active = false;
                if self.private_drb.auth_refused {
                    return;
                }
                self.private_drb.failures = self.private_drb.failures.saturating_add(1);
                self.private_drb.auth_retry_at = cx.now
                    + safe_cex_engine::backoff::Backoff::new(2000., 5, 60000.)
                        .delay(self.private_drb.failures);
                cx.notice_error(Level::Warn, code::REQUEST_FAILED, &error);
            }
            _ if !self.private_drb.auth_refused => {
                cx.notice_error(Level::Warn, code::REQUEST_FAILED, &error)
            }
            _ => {}
        }
    }
    pub(crate) fn drb_private_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        error: &str,
    ) {
        self.drb_private_lost(cx, pending, error, ErrorKind::Network);
    }
    pub(crate) fn drb_private_lost(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        message: &str,
        kind: ErrorKind,
    ) {
        if let Some(read) = self.private_drb.pending.remove(&pending.ticket) {
            let error = match self.drb.failed(read.rpc, kind) {
                Some(failure) => rpc_read_error(&failure, Released::Transport, message),
                None => VenueError::new(message).kind(kind),
            };
            self.drb_read_failed(cx, read.kind, error);
            let _ = cx.finish_http_context(&read.context);
        }
    }
    fn drb_publish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: &Kind,
        value: Value,
        orders_token: Option<safe_cex_engine::store::OrderSnapshotToken>,
    ) -> Result<(), String> {
        match kind {
            Kind::Summary => {
                let balance = summary(value)?;
                cx.store.set_balance(balance);
            }
            Kind::Positions => {
                let mut ids = HashSet::new();
                let positions = rows(value)?
                    .into_iter()
                    .map(position)
                    .collect::<Result<Vec<_>, _>>()?;
                for p in &positions {
                    if !ids.insert(p.id.clone()) {
                        return Err("drb duplicate position identity".into());
                    }
                    self.drb_market(cx, &p.symbol)?;
                }
                for p in &positions {
                    Self::drb_setting(cx, p);
                }
                cx.store.set_positions(positions);
            }
            Kind::Orders => {
                let Some(token) = orders_token else {
                    return Err("drb missing order snapshot authority".into());
                };
                if !cx.store.orders_snapshot_current(&token) {
                    return Ok(());
                }
                let mut ids = HashSet::new();
                let mut linkage = crate::drb_linkage::Linkage::default();
                let orders = rows(value)?
                    .into_iter()
                    .map(|row| {
                        linkage.observe(&row)?;
                        order(row)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for o in &orders {
                    if o.status != OrderStatus::Open || !ids.insert(o.id.clone()) {
                        return Err("drb invalid open order snapshot".into());
                    }
                    self.drb_market(cx, &o.symbol)?;
                }
                for o in &orders {
                    self.drb_order_version(o);
                }
                self.trading_drb.linkage.merge(linkage);
                cx.store.commit_orders_snapshot([token], |_| orders);
            }
            _ => {}
        }
        Ok(())
    }
    fn drb_market(&self, cx: &Cx<'_, Pending>, symbol: &str) -> Result<(), String> {
        if cx.store.market(symbol).is_none() {
            return Err(format!("drb account market unavailable: {symbol}"));
        }
        Ok(())
    }
    fn drb_setting(cx: &mut Cx<'_, Pending>, p: &Position) {
        cx.store.update_setting(&p.symbol, |s| {
            if p.leverage > 0. {
                s.leverage = p.leverage;
            }
            s.margin_type = p.margin_type;
        });
    }
    fn drb_order_version(&mut self, o: &Order) {
        if !self.private_drb.versions.contains_key(&o.id) {
            self.private_drb.versions_fifo.push_back(o.id.clone());
        }
        self.private_drb
            .versions
            .insert(o.id.clone(), o.last_update);
        while self.private_drb.versions_fifo.len() > MAX_ROWS {
            if let Some(id) = self.private_drb.versions_fifo.pop_front() {
                self.private_drb.versions.remove(&id);
            }
        }
    }
    fn drb_private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.private_drb.socket.is_some() || cx.now < self.private_drb.reconnect_at {
            return;
        }
        let socket = crate::account::private_socket(cx, drb::WS_URL);
        self.private_drb.socket = Some(socket);
        self.private_drb.ws = Some(drb_auth::Session::new(drb::Channel::WebSocket(socket)));
        self.private_drb.payload = cx.now;
        self.private_drb.ws_ready = false;
    }
    fn drb_ws_send(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: WsKind,
        method: &str,
        params: Value,
    ) -> Result<(), String> {
        let socket = self
            .private_drb
            .socket
            .ok_or("drb private socket unavailable")?;
        let rpc = self.drb.websocket(cx, socket, method, params)?;
        self.private_drb.ws_pending.insert(
            rpc,
            WsRequest {
                kind,
                started: cx.now,
            },
        );
        Ok(())
    }
    fn drb_ws_auth(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), String> {
        let session = self
            .private_drb
            .ws
            .as_ref()
            .ok_or("drb private socket unavailable")?;
        let credentials = self
            .signers
            .select(CredentialRole::SessionAuth)
            .ok()
            .map(|(_, credentials)| credentials)
            .ok_or("drb credentials unavailable")?;
        if let Some(identity) = credentials.identity().cloned() {
            let binding = credentials.binding();
            let params = session.pending_params(cx.now)?;
            let socket = self
                .private_drb
                .socket
                .ok_or("drb private socket unavailable")?;
            let rpc = self.drb.websocket_auth(cx, socket, params, |rpc| {
                Box::new(crate::sign::DrbAuth::new(
                    identity,
                    binding,
                    drb::Channel::WebSocket(socket),
                    rpc,
                ))
            })?;
            self.private_drb.ws_pending.insert(
                rpc,
                WsRequest {
                    kind: WsKind::Auth,
                    started: cx.now,
                },
            );
            Ok(())
        } else {
            let params = session.params_at(credentials, cx.now, cx.protocol_now(), |b| {
                cx.fill_signing_entropy(b)
            })?;
            self.drb_ws_send(cx, WsKind::Auth, "public/auth", params)
        }
    }
    pub(crate) fn drb_private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.private_drb.socket != Some(socket) {
            return false;
        }
        if let Err(e) = self.drb_ws_auth(cx) {
            warn(cx, e);
            self.drb_private_close(cx, socket);
        }
        true
    }
    pub(crate) fn drb_private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.private_drb.socket != Some(socket) {
            return false;
        }
        cx.close(socket);
        self.drb.disconnected(socket);
        self.private_drb.ws_pending.clear();
        self.private_drb.ws = None;
        self.private_drb.socket = None;
        self.private_drb.ws_ready = false;
        // No HTTP snapshot started before this gap can clear newer stream state.
        for r in &mut self.private_drb.revisions {
            *r += 1;
        }
        self.private_drb.reconnect_at = cx.now + 2000.;
        true
    }
    pub(crate) fn drb_private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.private_drb.socket != Some(socket) {
            return false;
        }
        let result = (|| -> Result<(), String> {
            if text.len() > drb::MAX_FRAME_BYTES {
                return Err("drb private frame exceeds limit".into());
            }
            let envelope: Value =
                serde_json::from_str(text).map_err(|_| "drb invalid private frame")?;
            if envelope["jsonrpc"] != "2.0" {
                return Err("drb invalid private envelope".into());
            }
            if let Some(id) = envelope.get("id").and_then(Value::as_u64) {
                let Some(request) = self.private_drb.ws_pending.remove(&id) else {
                    return Ok(());
                };
                let value = self
                    .drb
                    .response_at(id, 200, text, cx.now)
                    .map_err(|_| "drb private websocket request failed")?;
                match request.kind {
                    WsKind::Auth => {
                        self.private_drb
                            .ws
                            .as_mut()
                            .ok_or("drb missing socket session")?
                            .accept(value, request.started)?;
                        self.drb_ws_send(
                            cx,
                            WsKind::Subscribe,
                            "private/subscribe",
                            json!({"channels":CHANNELS}),
                        )?;
                        self.drb_ws_send(
                            cx,
                            WsKind::Heartbeat,
                            "public/set_heartbeat",
                            json!({"interval":10}),
                        )?;
                    }
                    WsKind::Subscribe => {
                        let confirmed: Vec<String> = serde_json::from_value(value)
                            .map_err(|_| "drb invalid subscribe acknowledgement")?;
                        if CHANNELS.iter().any(|c| !confirmed.iter().any(|s| s == c)) {
                            return Err("drb incomplete private subscription".into());
                        }
                        self.private_drb.ws_ready = true;
                        for revision in &mut self.private_drb.revisions {
                            *revision += 1;
                        }
                        self.drb_poll(cx);
                    }
                    WsKind::Heartbeat | WsKind::Test => {}
                }
            } else if envelope["method"] == "heartbeat" {
                if envelope["params"]["type"] == "test_request" {
                    self.drb_ws_send(cx, WsKind::Test, "public/test", json!({}))?;
                }
            } else if envelope["method"] == "subscription" && self.private_drb.ws_ready {
                let params = &envelope["params"];
                let channel = params["channel"]
                    .as_str()
                    .ok_or("drb missing private channel")?;
                if channel == CHANNELS[1] {
                    let balance = summary(params["data"].clone())?;
                    self.private_drb.revisions[1] += 1;
                    cx.store.set_balance(balance);
                } else if channel == CHANNELS[0] || channel == CHANNELS[2] {
                    self.drb_changes(cx, params["data"].clone())?;
                }
            }
            self.private_drb.payload = cx.now;
            Ok(())
        })();
        if let Err(e) = result {
            warn(cx, e);
            self.drb_private_close(cx, socket);
        }
        true
    }
    fn drb_changes(&mut self, cx: &mut Cx<'_, Pending>, value: Value) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Changes {
            #[serde(default)]
            orders: Vec<Value>,
            #[serde(default)]
            positions: Vec<Value>,
            #[serde(default)]
            trades: Vec<Value>,
        }
        let changes: Changes =
            serde_json::from_value(value).map_err(|_| "drb invalid changes notification")?;
        if changes.orders.len() + changes.positions.len() + changes.trades.len() > MAX_ROWS {
            return Err("drb change row limit".into());
        }
        let mut linkage = crate::drb_linkage::Linkage::default();
        let orders = changes
            .orders
            .into_iter()
            .map(|row| {
                linkage.observe(&row)?;
                order(row)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let positions = changes
            .positions
            .into_iter()
            .map(|row| {
                let mut p = position(row.clone())?;
                // The change stream omits fields available from get_positions.
                // Preserve their last authoritative values until the next REST read.
                if let Some(previous) = cx.store.position(&p.id) {
                    if row.get("estimated_liquidation_price").is_none() {
                        p.liquidation_price = previous.liquidation_price;
                    }
                    if row.get("leverage").is_none() {
                        p.leverage = previous.leverage;
                    }
                }
                Ok(p)
            })
            .collect::<Result<Vec<_>, String>>()?;
        let trades = changes
            .trades
            .iter()
            .map(wire::trade)
            .collect::<Result<Vec<_>, _>>()?;
        // Validate the whole event before publishing any component.
        for s in orders
            .iter()
            .map(|o| o.symbol.as_str())
            .chain(positions.iter().map(|p| p.symbol.as_str()))
        {
            self.drb_market(cx, s)?;
        }
        for trade in &trades {
            self.drb_market(
                cx,
                trade["symbol"].as_str().ok_or("drb invalid trade symbol")?,
            )?;
        }
        if !orders.is_empty() {
            self.private_drb.revisions[3] += 1;
        }
        self.trading_drb.linkage.merge(linkage);
        if !positions.is_empty() {
            self.private_drb.revisions[2] += 1;
        }
        for o in orders {
            if self
                .private_drb
                .versions
                .get(&o.id)
                .is_some_and(|t| *t >= o.last_update)
            {
                continue;
            }
            self.drb_order_version(&o);
            cx.store.upsert_order(o);
        }
        for p in positions {
            Self::drb_setting(cx, &p);
            cx.store.upsert_position(p);
        }
        for trade in trades {
            let id = trade["id"].as_str().unwrap().to_owned();
            if !self.private_drb.trades.insert(id.clone()) {
                continue;
            }
            self.private_drb.trade_fifo.push_back(id.clone());
            while self.private_drb.trade_fifo.len() > MAX_ROWS {
                if let Some(id) = self.private_drb.trade_fifo.pop_front() {
                    self.private_drb.trades.remove(&id);
                }
            }
            cx.fill_with_financials(
                Fill {
                    id: Some(id),
                    order_id: trade["orderId"].as_str().unwrap().into(),
                    symbol: trade["symbol"].as_str().unwrap().into(),
                    side: if trade["side"] == "buy" {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    },
                    price: trade["price"].as_f64().unwrap(),
                    amount: trade["qty"].as_f64().unwrap(),
                },
                FillFinancials {
                    fee: trade["commission"].as_f64(),
                    realized_pnl: None,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn drb_private_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.private_drb.timer != Some(timer) {
            return false;
        }
        self.private_drb.timer = None;
        self.private_drb.now = cx.now;
        // Retire only this consumer's expired correlation IDs; other drb
        // streams share Transport and own their own timeout continuations.
        let expired = self
            .private_drb
            .pending
            .iter()
            .filter(|(_, r)| cx.now - r.started >= f64::from(drb::REQUEST_TIMEOUT_MS))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for ticket in expired {
            self.drb_private_lost(
                cx,
                Request {
                    ticket,
                    sections: Sections::NONE,
                },
                "drb private read timed out",
                ErrorKind::Timeout,
            );
        }
        if self.private_drb.http.needs_refresh(cx.now) {
            self.drb_ensure_http(cx);
        } else {
            self.drb_after_markets(cx);
            self.drb_private_connect(cx);
            if cx.now >= self.private_drb.next_poll {
                self.drb_poll(cx);
            }
        }
        if let Some(socket) = self.private_drb.socket {
            let timeout = cx.now - self.private_drb.payload > 30_000.
                || self
                    .private_drb
                    .ws_pending
                    .values()
                    .any(|p| cx.now - p.started >= f64::from(drb::REQUEST_TIMEOUT_MS));
            if timeout {
                self.drb_private_close(cx, socket);
            } else if self.private_drb.ws_ready
                && self
                    .private_drb
                    .ws
                    .as_ref()
                    .is_some_and(|s| s.needs_refresh(cx.now))
                && !self
                    .private_drb
                    .ws_pending
                    .values()
                    .any(|p| matches!(p.kind, WsKind::Auth))
                && let Err(e) = self.drb_ws_auth(cx)
            {
                warn(cx, e);
                self.drb_private_close(cx, socket);
            }
        }
        self.drb_schedule(cx);
        true
    }
    pub(crate) fn drb_private_retired(&mut self, pending: Vec<Pending>) {
        for p in pending {
            if let Pending::PrivateDrb(p) = p
                && let Some(read) = self.private_drb.pending.remove(&p.ticket)
            {
                self.drb.failed(read.rpc, ErrorKind::Timeout);
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/support/private_drb.rs"]
mod tests;
