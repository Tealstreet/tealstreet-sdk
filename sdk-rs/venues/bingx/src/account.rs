//! Private account state and its independently recovered listen key.
#[path = "metadata.rs"]
mod metadata;
use crate::Pending;
use crate::account_wire::*;
use crate::sign::{Credentials, Params};
pub use metadata::{Leg as MetadataLeg, fail as fail_metadata_plan};
use safe_cex_engine::Level;
use safe_cex_engine::credential_selection::{PrimaryReason, Slots};
use safe_cex_engine::execution::ObservedProgress;
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::log::code;
use safe_cex_engine::metadata::{Observation, ObservedSetting};
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, StopTriggerType, TimeInForce,
};
use safe_cex_engine::request_context::{Session, SessionPolicy};
use safe_cex_engine::selected_read::ReadRole;
use safe_cex_engine::signing::Identity;
use safe_cex_engine::socket_lifecycle::{FeedId, LifecycleEvent, RetireReason, SocketError};
use safe_cex_engine::store::{MarketSetting, OrderSnapshotToken, Store};
use safe_cex_engine::{Cx, Fill, Method, Response, SocketId, TimerId, json::ArrayAt};
use std::collections::{HashMap, HashSet, VecDeque};

const LISTEN_KEY: &str = "/openApi/user/auth/userDataStream";
const POSITION_MODE: &str = "/openApi/swap/v1/positionSide/dual";
const POSITIONS: &str = "/openApi/swap/v2/user/positions";
const BALANCE: &str = "/openApi/swap/v3/user/balance";
const OPEN_ORDERS: &str = "/openApi/swap/v2/trade/openOrders";
const REFRESH_MS: f64 = 30.0 * 60_000.0;
const RETRY_MS: f64 = 30_000.0;
const POLL_MS: f64 = 2_000.0;
// Keep recent terminal quantities across snapshots and reconnects. Evict one
// oldest identity at a time; clearing all state would replay every terminal fill.
const RETAINED_ORDERS: usize = 10_000;
const RETAINED_EXECUTIONS: usize = 20_000;
const REPAIR_MS: f64 = 750.0;
const STALE_SNAPSHOT_LIMIT: u8 = 2;

pub enum Request {
    ListenKey {
        epoch: u64,
    },
    Keepalive {
        epoch: u64,
    },
    PositionMode {
        revision: u64,
    },
    Balance {
        revision: u64,
        first: Option<Balance>,
        usdt: Option<Balance>,
        failed: bool,
    },
    Positions {
        revision: u64,
        settings_stamp: Observation,
        rows: Vec<Position>,
        settings: Vec<(String, MarketSetting)>,
        skipped: Vec<String>,
        // An owned row outside the catalogue makes the snapshot incomplete.
        unavailable: bool,
    },
    OpenOrders {
        owner: u64,
        snapshot: OrderSnapshotToken,
        read_epoch: u64,
        rows: Vec<Order>,
        metas: Vec<OrderMeta>,
        skipped: Vec<String>,
        unavailable: bool,
    },
}

type OrderMeta = (Order, String, String, bool, String);

struct KeyRetry {
    timer: Option<TimerId>,
    not_before: f64,
}

#[derive(Default)]
pub struct Account {
    /// Configured keys, built once; reads choose among them by role.
    pub(crate) slots: Slots<Identity>,
    started: bool,
    mode_known: bool,
    mode_in_flight: bool,
    epoch: u64,
    key_in_flight: bool,
    key: Option<String>,
    /// Listen-key create/keepalive owner; retiring it drops their outcomes.
    session: Option<Session>,
    socket: Option<SocketId>,
    pub(super) feed: Option<FeedId>,
    key_retry: Option<KeyRetry>,
    keepalive: Option<TimerId>,
    poll: Option<TimerId>,
    repair: Option<TimerId>,
    orders_read_sequence: u64,
    orders_witness: Option<OrderSnapshotToken>,
    positions_revision: u64,
    settings_revision: u64,
    balance_revision: u64,
    orders_in_flight: Option<u64>,
    positions_in_flight: Option<u64>,
    balance_in_flight: Option<u64>,
    mode_guard: StaleGuard,
    balance_guard: StaleGuard,
    poll_count: u8,
    last_positions: f64,
    cumulative_fills: HashMap<String, f64>,
    orders_seeded: bool,
    terminal_orders: HashMap<String, f64>,
    seen_executions: HashSet<String>,
    // REST/z can cover executions never observed individually by this account.
    covered_history: HashSet<String>,
    retained_orders: VecDeque<String>,
    retained_terminals: VecDeque<String>,
    retained_executions: VecDeque<String>,
    observations: Vec<OrderMeta>,
    // Markets whose contract rows were malformed; their rows are reported once.
    skipped_markets: HashSet<String>,
    reported_markets: HashSet<String>,
    // Rejected private rows already logged, keyed `kind:id`; bounded by SKIP_LOG_LIMIT.
    skip_logged: HashMap<String, bool>,
}

/// Bounds the once-per-identity skip log; clearing it only re-arms logs.
const SKIP_LOG_LIMIT: usize = 4096;

pub fn order_type(text: &str) -> Option<OrderType> {
    Some(match text {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP" | "STOP_MARKET" | "TRIGGER_MARKET" | "TRIGGER_LIMIT" => OrderType::StopLoss,
        "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" | "TRAILING_TP_SL" => OrderType::TrailingStopLoss,
        _ => return None,
    })
}
pub fn order_side(text: &str) -> OrderSide {
    if text == "SELL" {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}
pub fn position_id(symbol: &str, side: PositionSide, mode: PositionMode) -> String {
    match mode {
        PositionMode::Oneway => symbol.to_owned(),
        PositionMode::Hedged => format!(
            "{symbol}:{}",
            if side == PositionSide::Long {
                "long"
            } else {
                "short"
            }
        ),
    }
}

/// A malformed position row's id, or its symbol when the side is unreadable;
/// empty when even the symbol is.
fn position_identity(item: &str, mode: PositionMode) -> String {
    let Ok(row) = serde_json::from_str::<PositionIdentity<'_>>(item) else {
        return String::new();
    };
    let symbol = row.symbol.replace('-', "");
    match (mode, row.position_side.as_ref()) {
        (_, _) if symbol.is_empty() => String::new(),
        (PositionMode::Hedged, "LONG") => position_id(&symbol, PositionSide::Long, mode),
        (PositionMode::Hedged, "SHORT") => position_id(&symbol, PositionSide::Short, mode),
        _ => symbol,
    }
}
/// Logs a rejected row once per identity until a committed snapshot no longer rejects
/// it; polls would otherwise repeat it. Error only when no stored copy is kept, and a
/// Warning still escalates once to Error when the copy is lost.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut HashMap<String, bool>,
    kind: &str,
    id: &str,
    kept: bool,
) {
    if logged.len() >= SKIP_LOG_LIMIT {
        logged.clear();
    }
    let key = format!("{kind}:{id}");
    if logged.get(&key).is_some_and(|&errored| errored || kept) {
        return;
    }
    logged.insert(key, !kept);
    let level = if kept { Level::Warn } else { Level::Error };
    if id.is_empty() {
        cx.notice(
            level,
            code::ROW_SKIPPED,
            format!("BingX skipped malformed {kind} row without an id"),
        );
    } else {
        cx.notice(
            level,
            code::ROW_SKIPPED,
            format!("BingX skipped malformed {kind} row {id}"),
        );
    }
}
/// Logs a committed snapshot's rejected rows and re-arms every row of its kind the
/// snapshot no longer rejects, whether it mapped, went flat or disappeared.
fn log_snapshot_skips(
    cx: &mut Cx<'_, Pending>,
    logged: &mut HashMap<String, bool>,
    kind: &str,
    skipped: &[String],
    kept: impl Fn(&str) -> bool,
) {
    let prefix = format!("{kind}:");
    logged.retain(|key, _| {
        key.strip_prefix(&prefix)
            .is_none_or(|id| skipped.iter().any(|s| s == id))
    });
    for id in skipped {
        log_skip(cx, logged, kind, id, kept(id));
    }
}
/// A malformed order row's id; empty when even that is unreadable.
fn order_identity(item: &str) -> String {
    serde_json::from_str::<OrderIdentity<'_>>(item)
        .map(|row| row.order_id.into_owned())
        .unwrap_or_default()
}

fn margin_type(text: &str) -> MarginType {
    if text.eq_ignore_ascii_case("isolated") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    }
}
fn mode(cx: &Cx<'_, Pending>) -> PositionMode {
    if cx.store.hedged() {
        PositionMode::Hedged
    } else {
        PositionMode::Oneway
    }
}
fn position_side(text: &str, quantity: f64, mode: PositionMode) -> PositionSide {
    if mode == PositionMode::Oneway {
        if quantity < 0.0 {
            PositionSide::Short
        } else {
            PositionSide::Long
        }
    } else if text == "SHORT" {
        PositionSide::Short
    } else {
        PositionSide::Long
    }
}
fn time_in_force(text: &str) -> Option<TimeInForce> {
    Some(match text {
        "GTC" => TimeInForce::GoodTillCancel,
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "PostOnly" | "POST_ONLY" | "GTX" => TimeInForce::PostOnly,
        _ => return None,
    })
}

/// A venue error is a failed snapshot, including errors transported as HTTP 200.
fn response_error(response: Response<'_>, allow_missing_code: bool) -> Option<String> {
    if allow_missing_code
        && response.body.trim().is_empty()
        && (200..300).contains(&response.status)
    {
        return None;
    }
    let status = serde_json::from_str::<Status<'_>>(response.body);
    if let Ok(s) = &status {
        if s.code != 0 {
            return Some(format!("{} ({})", s.msg, s.code));
        }
    } else if !allow_missing_code || serde_json::from_str::<KeyStatus>(response.body).is_err() {
        return Some("malformed BingX response envelope".into());
    }
    if !(200..300).contains(&response.status) {
        return Some(format!("HTTP {}", response.status));
    }
    None
}

impl Account {
    pub(crate) fn with_slots(slots: Slots<Identity>) -> Self {
        Self {
            slots,
            ..Self::default()
        }
    }
    pub fn handles_socket(&self, socket: SocketId) -> bool {
        self.socket == Some(socket)
    }
    pub fn handles_timer(&self, timer: TimerId) -> bool {
        [self.keepalive, self.poll, self.repair].contains(&Some(timer))
            || self
                .key_retry
                .as_ref()
                .is_some_and(|retry| retry.timer == Some(timer))
    }
    pub fn bump_orders(&mut self, store: &mut Store) {
        store.invalidate_order_snapshots();
    }
    pub fn bump_positions(&mut self) {
        self.positions_revision = self.positions_revision.wrapping_add(1);
    }
    pub fn settings_changed(&mut self) {
        self.settings_revision = self.settings_revision.wrapping_add(1);
    }
    pub fn fetch_metadata(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: Option<&Credentials>,
        _command: u32,
        symbol: String,
        publish: bool,
    ) {
        self.start_metadata(cx, credentials, symbol, publish);
    }
    /// Keeps earlier malformed markets the new catalogue still lacks; returns the
    /// malformed count before and after.
    pub fn set_skipped_markets(
        &mut self,
        store: &safe_cex_engine::store::Store,
        symbols: impl Iterator<Item = String>,
    ) -> (usize, usize) {
        let before = self.skipped_markets.len();
        self.skipped_markets
            .extend(symbols.filter(|s| !s.is_empty()));
        self.skipped_markets
            .retain(|symbol| store.market(symbol).is_none());
        self.reported_markets
            .retain(|symbol| store.market(symbol).is_none());
        (before, self.skipped_markets.len())
    }
    /// Whether a row's market is unknown; a market dropped at startup is an error once.
    fn untracked_market(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) -> bool {
        if cx.store.market(symbol).is_some() {
            return false;
        }
        if self.skipped_markets.contains(symbol) && self.reported_markets.insert(symbol.into()) {
            cx.notice(
                Level::Error,
                code::ROW_SKIPPED,
                format!(
                    "BingX account data for {symbol} is hidden: its contract row was malformed"
                ),
            );
        }
        true
    }
    pub fn drain_order_meta(&mut self) -> impl Iterator<Item = OrderMeta> + '_ {
        self.observations.drain(..)
    }

    pub fn start(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if std::mem::replace(&mut self.started, true) {
            return;
        }
        self.fetch_key(cx, credentials);
        self.fetch_mode(cx, credentials);
        self.fetch_balance(cx, credentials);
        self.fetch_orders(cx, credentials);
        self.poll_count = 1;
        self.poll = Some(cx.set_poll_timer(POLL_MS));
    }

    /// Each create opens a new session; keepalive reuses the live one. BingX
    /// listen keys expire 60 minutes after their last keepalive, so an extra
    /// create or keepalive after an unknown outcome is harmless: repeat-safe.
    fn listen_key_write(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        method: Method,
        params: Params,
        request: Request,
    ) {
        if method == Method::Post || self.session.is_none() {
            self.retire_session(cx);
            match cx.open_session(SessionPolicy {
                credential: Some(crate::managed::session_credential(credentials, method)),
                repeat_safe: true,
                // Signed: routed by its credential, never a session egress.
                private_egress: None,
            }) {
                Ok(session) => self.session = Some(session),
                Err(_) => {
                    let error = "BingX listen key session is unavailable";
                    return self.on_request_failed(cx, request, error, credentials);
                }
            }
        }
        let session = self.session.clone().expect("opened above");
        let queued = crate::managed::session_write(
            cx,
            credentials,
            &session,
            method,
            LISTEN_KEY,
            params,
            Pending::Account(request),
        );
        if let Err((pending, error)) = queued {
            let pending = match *pending {
                Pending::Signed(signed) => {
                    crate::managed::finish(cx, signed.context().cloned());
                    signed.inner
                }
                pending => pending,
            };
            if let Pending::Account(request) = pending {
                self.on_request_failed(cx, request, &error.message, credentials);
            }
        }
    }
    fn retire_session(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(session) = self.session.take() {
            // Nothing parks on a listen-key session: it never has waiters.
            let _ = cx.retire_session(&session);
        }
    }

    /// Account snapshot reads run under an engine-issued background Context.
    #[allow(clippy::too_many_arguments)]
    fn read_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        path: &'static str,
        params: Params,
        request: Request,
        stream: bool,
        primary: Option<PrimaryReason>,
    ) {
        let scope = auth_scope(&request);
        let role = match primary {
            Some(reason) => ReadRole::Primary { reason, scope },
            None => ReadRole::Read { scope },
        };
        let at = stream.then(|| {
            if path == OPEN_ORDERS {
                ArrayAt::EnvelopeFieldOrNested("data", "orders")
            } else {
                ArrayAt::EnvelopeField("data")
            }
        });
        match crate::managed::read(
            cx,
            &self.slots,
            crate::managed::Read {
                method: Method::Get,
                path,
                params,
                at,
            },
            Pending::Account(request),
            safe_cex_engine::request_context::Scope::Background,
            role,
        ) {
            Ok(_) => {}
            Err((pending, error)) => {
                let (pending, context) = match *pending {
                    Pending::Signed(signed) => {
                        let context = signed.context().cloned();
                        (signed.inner, context)
                    }
                    pending => (pending, None),
                };
                if let Pending::Account(request) = pending {
                    self.on_sign_failed(cx, request, error, credentials);
                }
                crate::managed::finish(cx, context);
            }
        }
    }

    fn fetch_key(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.key_in_flight || self.socket.is_some() {
            return;
        }
        self.key_in_flight = true;
        self.listen_key_write(
            cx,
            credentials,
            Method::Post,
            Params::new(),
            Request::ListenKey { epoch: self.epoch },
        );
    }
    fn fetch_mode(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.mode_in_flight {
            return;
        }
        self.mode_in_flight = true;
        self.read_snapshot(
            cx,
            credentials,
            POSITION_MODE,
            Params::new(),
            Request::PositionMode {
                revision: self.positions_revision,
            },
            false,
            None,
        );
    }
    fn fetch_balance(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.balance_in_flight.is_some() {
            return;
        }
        self.balance_in_flight = Some(self.balance_revision);
        self.read_snapshot(
            cx,
            credentials,
            BALANCE,
            Params::new(),
            Request::Balance {
                revision: self.balance_revision,
                first: None,
                usdt: None,
                failed: false,
            },
            true,
            None,
        );
    }
    fn fetch_positions(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if !self.mode_known || self.positions_in_flight.is_some() {
            return;
        }
        self.last_positions = cx.now;
        self.positions_in_flight = Some(self.positions_revision);
        let settings_stamp = cx.store.metadata_observation();
        self.read_snapshot(
            cx,
            credentials,
            POSITIONS,
            Params::new(),
            Request::Positions {
                revision: self.positions_revision,
                settings_stamp,
                rows: Vec::new(),
                settings: Vec::new(),
                skipped: Vec::new(),
                unavailable: false,
            },
            true,
            None,
        );
    }
    fn fetch_orders(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if self.orders_in_flight.is_some() {
            return;
        }
        self.start_order_read(cx, credentials, None);
    }
    /// The snapshot that clears an uncertain-write fence is Primary-signed, so a
    /// read key can never be the read that proves the trading account's orders.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if cx.frame_fed() {
            return;
        }
        if cx.reconcile_reask() && self.orders_in_flight.is_some() {
            return;
        }
        self.bump_orders(cx.store);
        self.start_order_read(cx, credentials, Some(PrimaryReason::PostUnknownOrderRead));
    }
    /// A fence needs a new owner even if an older snapshot is still pending.
    fn start_order_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        credentials: &Credentials,
        primary: Option<PrimaryReason>,
    ) {
        let Some(owner) = self.orders_read_sequence.checked_add(1) else {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "BingX order read ownership exhausted",
            );
            return;
        };
        let Some(snapshot) = cx.store.begin_orders_snapshot() else {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "BingX order snapshot fence exhausted",
            );
            return;
        };
        self.orders_read_sequence = owner;
        self.orders_in_flight = Some(owner);
        self.read_snapshot(
            cx,
            credentials,
            OPEN_ORDERS,
            Params::new(),
            Request::OpenOrders {
                owner,
                snapshot,
                read_epoch: cx.order_read_epoch(),
                rows: Vec::new(),
                metas: Vec::new(),
                skipped: Vec::new(),
                unavailable: false,
            },
            true,
            primary,
        );
    }

    /// Data source: re-read the position mode, whose response re-reads the
    /// positions that carry margin type and leverage. Never a write.
    pub fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if cx.frame_fed() || !self.started {
            return;
        }
        self.fetch_mode(cx, credentials);
    }

    pub(crate) fn mode_known(&self) -> bool {
        self.mode_known
    }

    pub fn reconcile(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        // Frame-fed: the data source's frames carry the outcome.
        if cx.frame_fed() {
            return;
        }
        self.fetch_orders(cx, credentials);
        self.fetch_positions(cx, credentials);
        self.fetch_balance(cx, credentials);
    }

    fn clear_private_timers(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(timer) = self.key_retry.take().and_then(|retry| retry.timer) {
            cx.clear_timer(timer);
        }
        if let Some(timer) = self.keepalive.take() {
            cx.clear_timer(timer);
        }
    }
    fn invalidate_private(&mut self, cx: &mut Cx<'_, Pending>) {
        self.retire_session(cx);
        self.epoch = self.epoch.wrapping_add(1);
        self.key_in_flight = false;
        self.key = None;
        self.clear_private_timers(cx);
        self.socket = None;
        self.bump_orders(cx.store);
        self.bump_positions();
    }
    fn schedule_key_retry(&mut self, cx: &mut Cx<'_, Pending>) {
        let at = cx.now + RETRY_MS;
        if !cx.now.is_finite() || !at.is_finite() || at <= cx.now {
            return;
        }
        if let Some(timer) = self.key_retry.take().and_then(|retry| retry.timer) {
            cx.clear_timer(timer);
        }
        self.key_retry = Some(KeyRetry {
            timer: Some(cx.set_timer(RETRY_MS)),
            not_before: at,
        });
    }
    fn resume_key_retry(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now.is_finite()
            && let Some(retry) = &mut self.key_retry
            && retry.timer.is_none()
            && (retry.not_before - cx.now).is_finite()
        {
            retry.timer = Some(cx.set_timer((retry.not_before - cx.now).max(0.)));
        }
    }
    fn fresh_key(&mut self, cx: &mut Cx<'_, Pending>, _credentials: &Credentials) {
        if let Some(feed) = self.feed
            && cx.socket_epoch(feed).is_some()
        {
            cx.retire_socket(feed, RetireReason::Closed);
        } else {
            self.invalidate_private(cx);
            self.schedule_key_retry(cx);
        }
    }

    pub fn on_open(&mut self, cx: &mut Cx<'_, Pending>, credentials: &Credentials) {
        if let Some(timer) = self.keepalive.take() {
            cx.clear_timer(timer);
        }
        self.keepalive = Some(cx.set_timer(REFRESH_MS));
        if let Some(socket) = self.socket {
            cx.send(socket, "Ping");
        }
        // Reconnects have a blind interval; any old snapshots are now stale.
        self.bump_orders(cx.store);
        self.bump_positions();
        self.reconcile(cx, credentials);
    }
    pub(super) fn socket_event(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        event: LifecycleEvent,
        credentials: &Credentials,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) if self.socket == Some(epoch.socket()) => {
                self.on_open(cx, credentials)
            }
            LifecycleEvent::Retired { .. } => self.invalidate_private(cx),
            LifecycleEvent::Reconnect(_) => self.fetch_key(cx, credentials),
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "Ping"),
            _ => {}
        }
        self.resume_key_retry(cx);
        Ok(())
    }

    pub fn on_timer(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        timer: TimerId,
        credentials: &Credentials,
    ) {
        if let Some(retry) = &mut self.key_retry
            && retry.timer == Some(timer)
        {
            // The engine consumed this physical timer even at an invalid clock.
            retry.timer = None;
            if !cx.now.is_finite() {
                return;
            }
            if cx.now < retry.not_before {
                self.resume_key_retry(cx);
            } else {
                self.key_retry = None;
                self.fetch_key(cx, credentials);
            }
            return;
        }
        if !cx.now.is_finite() {
            return;
        }
        self.resume_key_retry(cx);
        if self.keepalive == Some(timer) {
            self.keepalive = None;
            if let Some(key) = self.key.clone() {
                self.listen_key_write(
                    cx,
                    credentials,
                    Method::Put,
                    Params::new().str("listenKey", &key),
                    Request::Keepalive { epoch: self.epoch },
                );
            } else {
                self.fresh_key(cx, credentials);
            }
        } else if self.repair == Some(timer) {
            self.repair = None;
            self.fetch_positions(cx, credentials);
        } else if self.poll == Some(timer) {
            self.poll = Some(cx.set_poll_timer(POLL_MS));
            if !self.mode_known {
                self.fetch_mode(cx, credentials);
            }
            self.fetch_positions(cx, credentials);
            let count = self.poll_count;
            self.poll_count = (count + 1) % 4;
            if count == 2 {
                self.fetch_orders(cx, credentials);
            }
            if count == 0 {
                self.fetch_balance(cx, credentials);
            }
        }
    }

    fn schedule_repair(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.repair.is_none() {
            self.repair = Some(cx.set_timer((REPAIR_MS - (cx.now - self.last_positions)).max(1.0)));
        }
    }

    pub fn on_item(&mut self, cx: &mut Cx<'_, Pending>, request: &mut Request, item: &str) {
        match request {
            Request::Balance {
                first,
                usdt,
                failed,
                ..
            } => match serde_json::from_str::<BalanceRow<'_>>(item) {
                Ok(row) => {
                    let b = Balance {
                        total: row.balance,
                        free: row.available_margin,
                        used: row.used_margin,
                        upnl: row.unrealized_profit,
                    };
                    if first.is_none() {
                        *first = Some(b.clone());
                    }
                    if row.asset == "USDT" {
                        *usdt = Some(b);
                    }
                }
                Err(_) => *failed = true,
            },
            Request::Positions {
                rows,
                settings,
                skipped,
                unavailable,
                ..
            } => {
                let Ok(row) = serde_json::from_str::<PositionRow<'_>>(item) else {
                    skipped.push(position_identity(item, mode(cx)));
                    return;
                };
                let symbol = row.symbol.replace('-', "");
                if self.untracked_market(cx, &symbol) {
                    if row.position_amt != 0.0 {
                        if self.skipped_markets.contains(&symbol) {
                            *unavailable = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                                "BingX owned position market unavailable; preserving previous snapshot",
                            );
                        } else {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Skipping BingX position on uncatalogued market {symbol}"),
                            );
                        }
                    }
                    return;
                }
                let mode = mode(cx);
                let side = position_side(&row.position_side, row.position_amt, mode);
                if row.leverage <= 0.0 {
                    // A flat row closes its position whatever its leverage.
                    if row.position_amt != 0.0 {
                        skipped.push(position_id(&symbol, side, mode));
                    }
                    return;
                }
                let margin = if row.isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let index = settings
                    .iter()
                    .position(|(s, _)| s == &symbol)
                    .unwrap_or_else(|| {
                        let mut setting = cx.store.setting(&symbol).unwrap_or_default();
                        if !setting.leverage.is_finite() {
                            setting.leverage = row.leverage;
                        }
                        settings.push((symbol.clone(), setting));
                        settings.len() - 1
                    });
                let setting = &mut settings[index].1;
                setting.margin_type = Some(margin);
                if mode == PositionMode::Oneway {
                    setting.leverage = row.leverage;
                    setting.buy_leverage = Some(row.leverage);
                    setting.sell_leverage = Some(row.leverage);
                } else {
                    match side {
                        PositionSide::Long => {
                            setting.leverage = row.leverage;
                            setting.buy_leverage = Some(row.leverage);
                        }
                        PositionSide::Short => setting.sell_leverage = Some(row.leverage),
                    }
                }
                if row.position_amt == 0.0 {
                    return;
                }
                let notional = row.position_amt.abs() * row.avg_price + row.unrealized_profit;
                let margin_size = if row.isolated_margin > 0.0 {
                    row.isolated_margin
                } else if row.initial_margin > 0.0 {
                    row.initial_margin
                } else {
                    notional / row.leverage
                };
                rows.push(Position {
                    id: position_id(&symbol, side, mode),
                    symbol,
                    side,
                    entry_price: row.avg_price,
                    notional,
                    leverage: row.leverage,
                    margin_type: Some(margin),
                    position_mode: Some(mode),
                    unrealized_pnl: row.unrealized_profit,
                    realized_pnl: None,
                    contracts: row.position_amt.abs(),
                    liquidation_price: row.liquidation_price,
                    margin_size: Some(margin_size),
                    last_update: None,
                });
            }
            Request::OpenOrders {
                rows,
                metas,
                skipped,
                unavailable,
                ..
            } => {
                let Some(order) = serde_json::from_str::<NativeOrder<'_>>(item)
                    .ok()
                    .and_then(|native| Some((map_order(&native, cx)?, native)))
                else {
                    skipped.push(order_identity(item));
                    return;
                };
                let (order, native) = order;
                if self.terminal_orders.contains_key(&order.id) {
                    return;
                }
                if self.untracked_market(cx, &order.symbol) {
                    if order.status == OrderStatus::Open {
                        if self.skipped_markets.contains(&order.symbol) {
                            *unavailable = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                                "BingX working order market unavailable; preserving previous orders",
                            );
                        } else {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!(
                                    "Skipping BingX order on uncatalogued market {}",
                                    order.symbol
                                ),
                            );
                        }
                    }
                    return;
                }
                if native.complete_metadata() {
                    metas.push(observation(order.clone(), &native));
                }
                rows.push(order);
            }
            _ => {}
        }
    }

    pub fn on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
        credentials: &Credentials,
    ) {
        if matches!(&request,Request::ListenKey{epoch}|Request::Keepalive{epoch} if *epoch!=self.epoch)
        {
            return;
        }
        self.resume_key_retry(cx);
        let is_key = matches!(
            request,
            Request::ListenKey { .. } | Request::Keepalive { .. }
        );
        if let Some(error) = response_error(response, is_key) {
            if matches!(request, Request::Keepalive { .. })
                && (response.status == 404 || {
                    let m = error.to_ascii_lowercase();
                    m.contains("listen")
                        && (m.contains("expired")
                            || m.contains("invalid")
                            || m.contains("does not exist"))
                })
            {
                cx.notice(
                    Level::Warn,
                    code::STREAM_UNAVAILABLE,
                    "BingX listen key expired; obtaining a fresh key",
                );
                self.fresh_key(cx, credentials);
            } else {
                self.on_request_failed(cx, request, &error, credentials);
            }
            return;
        }
        match request {
            Request::ListenKey { .. } => {
                self.key_in_flight = false;
                let parsed = serde_json::from_str::<ListenKey<'_>>(response.body);
                let key = parsed.as_ref().ok().and_then(|p| p.key()).filter(|k| {
                    !k.is_empty()
                        && k.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                });
                let Some(key) = key else {
                    self.on_request_failed(
                        cx,
                        Request::ListenKey { epoch: self.epoch },
                        "listen key missing or malformed",
                        credentials,
                    );
                    return;
                };
                self.key = Some(key.into());
                let Some(feed) = self.feed else {
                    self.on_request_failed(
                        cx,
                        Request::ListenKey { epoch: self.epoch },
                        "private feed not registered",
                        credentials,
                    );
                    return;
                };
                match cx.connect_socket(
                    feed,
                    safe_cex_engine::SocketSpec::new(format!(
                        "{}?listenKey={key}",
                        crate::paths::PRIVATE_WS
                    )),
                ) {
                    Ok(epoch) => self.socket = Some(epoch.socket()),
                    Err(error) => self.on_request_failed(
                        cx,
                        Request::ListenKey { epoch: self.epoch },
                        &format!("private socket admission: {error:?}"),
                        credentials,
                    ),
                }
            }
            Request::Keepalive { .. } => {
                if let Some(timer) = self.keepalive.take() {
                    cx.clear_timer(timer);
                }
                self.keepalive = Some(cx.set_timer(REFRESH_MS));
            }
            Request::PositionMode { revision } => {
                self.mode_in_flight = false;
                let current = revision == self.positions_revision;
                // Only discovery may commit a stale read; a known mode, possibly
                // just set by a command, waits for a current one.
                let admit = if self.mode_known {
                    current
                } else {
                    self.mode_guard.admit(current, STALE_SNAPSHOT_LIMIT)
                };
                if !admit {
                    self.fetch_mode(cx, credentials);
                    return;
                }
                match serde_json::from_str::<ModeEnvelope>(response.body) {
                    Ok(row) => {
                        if !self.mode_known || cx.store.hedged() != row.data.dual {
                            self.bump_positions();
                        }
                        self.mode_known = true;
                        cx.store.set_hedged(row.data.dual);
                        self.fetch_positions(cx, credentials);
                    }
                    Err(_) => cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "BingX malformed position mode response",
                    ),
                }
                if !current {
                    self.fetch_mode(cx, credentials);
                }
            }
            Request::Balance {
                revision,
                first,
                usdt,
                failed,
            } => {
                self.balance_in_flight = None;
                if failed {
                    cx.notice(
                        Level::Error,
                        code::SNAPSHOT_DISCARDED,
                        "BingX malformed balance snapshot",
                    );
                    return;
                }
                let current = revision == self.balance_revision;
                if self.balance_guard.admit(current, STALE_SNAPSHOT_LIMIT) {
                    if let Some(balance) = usdt.or(first) {
                        cx.store.set_balance(balance);
                    } else {
                        cx.notice(
                            Level::Error,
                            code::SNAPSHOT_DISCARDED,
                            "BingX balance snapshot contained no settlement asset",
                        );
                    }
                }
                if !current {
                    self.fetch_balance(cx, credentials);
                }
            }
            Request::Positions {
                revision,
                settings_stamp,
                mut rows,
                settings,
                skipped,
                unavailable,
            } => {
                self.positions_in_flight = None;
                if unavailable {
                    for id in &skipped {
                        log_skip(cx, &mut self.skip_logged, "position", id, true);
                    }
                    return;
                }
                if revision == self.positions_revision {
                    let unidentified = skipped.iter().any(String::is_empty);
                    // A malformed row never closes the known position it names
                    // (by id, or by symbol when its side is unreadable).
                    let kept: Vec<_> = cx
                        .store
                        .positions()
                        .iter()
                        .filter(|p| {
                            (unidentified || skipped.contains(&p.id) || skipped.contains(&p.symbol))
                                && !rows.iter().any(|row| row.id == p.id)
                        })
                        .cloned()
                        .collect();
                    log_snapshot_skips(cx, &mut self.skip_logged, "position", &skipped, |id| {
                        kept.iter()
                            .any(|p| id.is_empty() || p.id == id || p.symbol == id)
                    });
                    rows.extend(kept);
                    cx.store.set_positions(rows);
                    if !settings.is_empty() {
                        self.settings_changed();
                    }
                    for (symbol, value) in settings {
                        if let Ok(value) = ObservedSetting::new(value) {
                            cx.store.observe_setting(&symbol, &settings_stamp, value);
                        }
                    }
                } else {
                    for id in &skipped {
                        log_skip(cx, &mut self.skip_logged, "position", id, true);
                    }
                    self.schedule_repair(cx);
                }
            }
            Request::OpenOrders {
                owner,
                snapshot,
                read_epoch,
                mut rows,
                metas,
                skipped,
                unavailable,
            } => {
                if self.orders_in_flight == Some(owner) {
                    self.orders_in_flight = None;
                }
                if !cx.store.orders_snapshot_current(&snapshot) {
                    if !self
                        .orders_witness
                        .as_ref()
                        .is_some_and(|token| cx.store.orders_snapshot_current(token))
                    {
                        self.fetch_orders(cx, credentials);
                    }
                    return;
                }
                if unavailable {
                    for id in &skipped {
                        log_skip(cx, &mut self.skip_logged, "open order", id, true);
                    }
                    return;
                }
                let complete = skipped.is_empty();
                let mut kept_ids = Vec::new();
                let committed = cx.store.commit_orders_snapshot([snapshot], |current| {
                    // Startup history is a baseline; later REST reads cannot consume live executions.
                    if !self.orders_seeded {
                        for order in &rows {
                            if order.filled
                                > self.cumulative_fills.get(&order.id).copied().unwrap_or(0.0)
                            {
                                self.covered_history.insert(order.id.clone());
                            }
                            self.record_cumulative(&order.id, order.filled);
                        }
                    }
                    self.orders_seeded = true;
                    self.observations.extend(metas);
                    // A malformed row never removes the known open order it names.
                    let unidentified = skipped.iter().any(String::is_empty);
                    let kept: Vec<_> = current
                        .iter()
                        .filter(|o| {
                            o.status == OrderStatus::Open
                                && (unidentified || skipped.contains(&o.id))
                                && !self.terminal_orders.contains_key(&o.id)
                                && !rows.iter().any(|row| row.id == o.id)
                        })
                        .cloned()
                        .collect();
                    kept_ids.extend(kept.iter().map(|order| order.id.clone()));
                    rows.extend(kept);
                    rows
                });
                if committed {
                    log_snapshot_skips(cx, &mut self.skip_logged, "open order", &skipped, |id| {
                        kept_ids.iter().any(|kept| id.is_empty() || kept == id)
                    });
                    if complete {
                        self.orders_witness = cx.store.begin_orders_snapshot();
                        cx.orders_reconciled_since(None, read_epoch);
                    }
                }
            }
        }
    }

    pub fn on_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: safe_cex_engine::VenueError,
        credentials: &Credentials,
    ) {
        let owns_read = match &request {
            Request::Balance { revision, .. } => self.balance_in_flight == Some(*revision),
            Request::Positions { revision, .. } => self.positions_in_flight == Some(*revision),
            Request::OpenOrders { owner, .. } => self.orders_in_flight == Some(*owner),
            _ => true,
        };
        if owns_read {
            // Native recovery owns retries and listen-key epoch checks. A provider
            // failure supplies no response and therefore no venue-auth evidence.
            self.on_request_failed(cx, request, &error.message, credentials);
        }
    }

    pub fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
        _credentials: &Credentials,
    ) {
        if matches!(&request,Request::ListenKey{epoch}|Request::Keepalive{epoch} if *epoch!=self.epoch)
        {
            return;
        }
        // A failing private read surfaces as an Error at the retried-failure cadence.
        cx.retried_failure(
            "bingx.account:private",
            format!("BingX private request failed: {error}"),
        );
        match request {
            Request::ListenKey { .. } => {
                self.invalidate_private(cx);
                self.schedule_key_retry(cx);
            }
            Request::Keepalive { .. } => {
                if let Some(timer) = self.keepalive.take() {
                    cx.clear_timer(timer);
                }
                self.keepalive = Some(cx.set_timer(RETRY_MS));
            }
            Request::PositionMode { .. } => self.mode_in_flight = false,
            Request::Balance { .. } => self.balance_in_flight = None,
            Request::Positions { .. } => self.positions_in_flight = None,
            Request::OpenOrders { owner, .. } => {
                if self.orders_in_flight == Some(owner) {
                    self.orders_in_flight = None;
                }
            }
        }
    }

    pub fn on_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str, credentials: &Credentials) {
        if text == "Ping" {
            if let Some(socket) = self.socket {
                cx.send(socket, "Pong");
            }
            return;
        }
        if text == "Pong" {
            return;
        }
        let Ok(event) = serde_json::from_str::<UserEvent<'_>>(text) else {
            return;
        };
        if event.ping.is_some() {
            if let Some(socket) = self.socket {
                cx.send(socket, "Pong");
            }
            return;
        }
        if event.pong.is_some() {
            return;
        }
        match event.e.as_ref() {
            "listenKeyExpired" => self.fresh_key(cx, credentials),
            "ACCOUNT_UPDATE" => {
                if !event.a.balances.is_empty() {
                    self.balance_revision = self.balance_revision.wrapping_add(1);
                    self.fetch_balance(cx, credentials);
                }
                let mut selected = None;
                for (index, rows) in [
                    &event.a.positions,
                    &event.a.balances,
                    &event.a.legacy_positions,
                ]
                .into_iter()
                .enumerate()
                {
                    let has_position = rows.iter().any(|raw| {
                        serde_json::from_str::<PositionIntent<'_>>(raw.get())
                            .ok()
                            .is_some_and(|p| !p.s.is_empty())
                    });
                    // P/p are dedicated position arrays; malformed-only updates need repair too.
                    if has_position || (!rows.is_empty() && index != 1) {
                        selected = Some((rows, index == 1));
                        break;
                    }
                }
                if let Some((rows, balances)) = selected {
                    self.bump_positions();
                    for raw in rows {
                        if balances
                            && !serde_json::from_str::<PositionIntent<'_>>(raw.get())
                                .ok()
                                .is_some_and(|p| !p.s.is_empty())
                        {
                            continue;
                        }
                        match serde_json::from_str::<PositionUpdate<'_>>(raw.get()) {
                            Ok(p) if !p.s.is_empty() && p.pa.is_some() => self.on_position(cx, &p),
                            _ => cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                "Skipping malformed BingX position update; REST repair scheduled",
                            ),
                        }
                    }
                    self.schedule_repair(cx);
                }
            }
            "ACCOUNT_CONFIG_UPDATE" => {
                if let Some(config) = event.ac {
                    self.settings_changed();
                    self.bump_positions();
                    let symbol = config.s.replace('-', "");
                    cx.store.invalidate_metadata(Some(&symbol));
                    let hedged = cx.store.hedged();
                    if config.l > 0.0 {
                        cx.store.update_setting(&symbol, |s| {
                            s.leverage = config.l;
                            s.buy_leverage = Some(config.l);
                            if config.short > 0.0 {
                                s.sell_leverage = Some(config.short);
                            } else if !hedged {
                                s.sell_leverage = Some(config.l);
                            }
                            if let Some(m) = &config.mt {
                                s.margin_type = Some(margin_type(m));
                            }
                        });
                        cx.store.update_positions(|p| {
                            if p.symbol == symbol {
                                match p.side {
                                    PositionSide::Short if config.short > 0.0 => {
                                        p.leverage = config.short
                                    }
                                    PositionSide::Short if hedged => {}
                                    _ => p.leverage = config.l,
                                }
                                if let Some(m) = &config.mt {
                                    p.margin_type = Some(margin_type(m));
                                }
                            }
                        });
                    }
                    self.schedule_repair(cx);
                }
            }
            "ORDER_TRADE_UPDATE" | "TRADE_UPDATE" => {
                if let Some(native) = event.o {
                    self.on_order(cx, native, event.time);
                }
            }
            _ => {}
        }
    }

    fn record_cumulative(&mut self, id: &str, quantity: f64) {
        if !self.cumulative_fills.contains_key(id) {
            self.retained_orders.push_back(id.to_owned());
            while self.retained_orders.len() > RETAINED_ORDERS {
                if let Some(old) = self.retained_orders.pop_front() {
                    self.cumulative_fills.remove(&old);
                    if !self.terminal_orders.contains_key(&old) {
                        self.covered_history.remove(&old);
                    }
                }
            }
        }
        let previous = self.cumulative_fills.entry(id.to_owned()).or_insert(0.0);
        *previous = previous.max(quantity);
    }

    fn record_terminal(&mut self, id: &str) {
        // Terminal tombstones have their own budget: a large open-order
        // snapshot must not evict all recent terminal replay protection.
        if !self.terminal_orders.contains_key(id) {
            self.retained_terminals.push_back(id.to_owned());
            while self.retained_terminals.len() > RETAINED_ORDERS {
                if let Some(old) = self.retained_terminals.pop_front() {
                    self.terminal_orders.remove(&old);
                    if !self.cumulative_fills.contains_key(&old) {
                        self.covered_history.remove(&old);
                    }
                }
            }
        }
        let quantity = self.cumulative_fills.get(id).copied().unwrap_or(0.0);
        let previous = self.terminal_orders.entry(id.to_owned()).or_insert(0.0);
        *previous = previous.max(quantity);
    }

    fn first_execution(&mut self, identity: String) -> bool {
        if !self.seen_executions.insert(identity.clone()) {
            return false;
        }
        self.retained_executions.push_back(identity);
        while self.retained_executions.len() > RETAINED_EXECUTIONS {
            if let Some(old) = self.retained_executions.pop_front() {
                self.seen_executions.remove(&old);
            }
        }
        true
    }

    pub fn accept_placed_response(&mut self, cx: &mut Cx<'_, Pending>, body: &str) {
        if !serde_json::from_str::<Status<'_>>(body).is_ok_and(|s| s.code == 0) {
            return;
        }
        if let Ok(placed) = serde_json::from_str::<PlacedEnvelope<'_>>(body) {
            let native = match placed.data {
                Placed::Nested { order } | Placed::Direct(order) => order,
            };
            self.accept_placed(cx, &native);
        }
    }

    pub fn accept_placed(&mut self, cx: &mut Cx<'_, Pending>, native: &NativeOrder<'_>) {
        if let Some(order) = map_order(native, cx)
            && order.status == OrderStatus::Open
            && order.kind != OrderType::Market
            && cx.store.order(&order.id).is_none()
            && !self.terminal_orders.contains_key(&order.id)
        {
            // A stream row is authoritative even when its terminal row has
            // since disappeared from the latest open-order snapshot.
            if native.complete_metadata() {
                self.observations.push(observation(order.clone(), native));
            }
            cx.store.upsert_order(order);
        }
    }

    fn on_order(&mut self, cx: &mut Cx<'_, Pending>, native: NativeOrder<'_>, event_time: f64) {
        // LIQUIDATION and future execution types need not map to an ordinary
        // resting OrderType. Process their executions before order conversion.
        let Some(id) = native.id() else {
            return;
        };
        if !matches!(native.side.as_ref(), "BUY" | "SELL") || native.symbol.is_empty() {
            return;
        }
        let symbol = native.symbol.replace('-', "");
        let filled = native.filled.unwrap_or(0.0);
        let previous = self
            .cumulative_fills
            .get(id)
            .copied()
            .unwrap_or(0.0)
            .max(self.terminal_orders.get(id).copied().unwrap_or(0.0));
        let mut effective_filled = filled.max(previous);
        let trade_status = matches!(
            native.status.as_deref(),
            Some("PARTIALLY_FILLED" | "FILLED")
        );
        let mut emitted = 0.0;
        if trade_status {
            // Both event names can carry one execution with or without z.
            let identity = (native.last_quantity > 0.0).then(|| {
                if !native.trade_id.is_empty() {
                    format!("{id}:trade:{}", native.trade_id)
                } else {
                    format!(
                        "{id}:time:{}:{}:{}",
                        if native.time > 0.0 {
                            native.time
                        } else {
                            event_time
                        },
                        native.last_quantity,
                        native.last_price
                    )
                }
            });
            let fresh = identity
                .as_ref()
                .is_none_or(|key| !self.seen_executions.contains(key));
            // Cumulative growth proves a new fill when heuristic same-ms keys collide.
            let duplicate = !fresh && (!native.trade_id.is_empty() || native.filled.is_none());
            let suppressed = duplicate
                || (native.filled.is_none()
                    && self.terminal_orders.contains_key(id)
                    && (native.trade_id.is_empty() || self.covered_history.contains(id)));
            let last = if suppressed {
                Some(0.0)
            } else {
                (native.last_quantity > 0.0).then_some(native.last_quantity)
            };
            let price = if native.last_price > 0.0 {
                native.last_price
            } else if native.avg_price > 0.0 {
                native.avg_price
            } else {
                native.price
            };
            let Ok(progress) = ObservedProgress::new(previous, native.filled) else {
                return;
            };
            let Ok(prepared) = progress.prepare_capped(last, price) else {
                return;
            };
            // Financial preparation is pure: invalid evidence consumes no ID,
            // revision, quantity, terminal state or repair scheduling.
            self.bump_orders(cx.store);
            if let Some(identity) = identity {
                self.first_execution(identity);
            }
            effective_filled = prepared.map_or(progress.quantity(), |p| p.observed());
            if let Some(prepared) = prepared {
                let delivery = prepared.delivery();
                emitted = delivery.amount();
                cx.fill(Fill {
                    id: None,
                    order_id: id.to_owned(),
                    symbol: symbol.clone(),
                    side: order_side(&native.side),
                    price: delivery.price(),
                    amount: delivery.amount(),
                });
                self.bump_positions();
                self.schedule_repair(cx);
            }
            self.record_cumulative(id, effective_filled);
        } else {
            self.bump_orders(cx.store);
        }
        if matches!(
            native.status.as_deref(),
            Some("FILLED" | "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED")
        ) {
            self.record_cumulative(id, effective_filled);
            self.record_terminal(id);
        }
        if native.filled.is_some()
            && filled > previous + emitted
            && self.cumulative_fills.contains_key(id)
        {
            self.covered_history.insert(id.to_owned());
        }
        let Some(mut order) = map_order(&native, cx) else {
            return;
        };
        order.filled = effective_filled.max(cx.store.order(id).map_or(0.0, |order| order.filled));
        if native.status.as_deref() == Some("FILLED") && native.filled.is_none() {
            order.filled = order.filled.max(order.amount);
        }
        order.remaining = (order.amount - order.filled).max(0.0);
        if native.complete_metadata() {
            self.observations.push(observation(order.clone(), &native));
        }
        if order.status == OrderStatus::Open && order.kind != OrderType::Market {
            let existing = cx.store.order(&order.id);
            if !self.terminal_orders.contains_key(&order.id)
                && !existing
                    .is_some_and(|o| o.status != OrderStatus::Open || o.filled > order.filled)
            {
                cx.store.upsert_order(order);
            }
        } else if order.status != OrderStatus::Open {
            cx.store.update_order(&order.id, |o| {
                o.status = order.status;
                o.filled = o.filled.max(order.filled);
                o.remaining = (o.amount - o.filled).max(0.0);
                o.last_update = cx.now;
            });
        }
    }

    fn on_position(&mut self, cx: &mut Cx<'_, Pending>, p: &PositionUpdate<'_>) {
        let Some(quantity) = p.pa else {
            return;
        };
        if p.s.is_empty() {
            return;
        }
        let symbol = p.s.replace('-', "");
        if self.untracked_market(cx, &symbol) {
            return;
        }
        let current_mode = if self.mode_known {
            mode(cx)
        } else if p.ps == "BOTH" {
            PositionMode::Oneway
        } else {
            PositionMode::Hedged
        };
        let side = position_side(&p.ps, quantity, current_mode);
        let expected = position_id(&symbol, side, current_mode);
        let existing = cx
            .store
            .position(&expected)
            .or_else(|| {
                cx.store.positions().iter().find(|e| {
                    e.symbol == symbol && (current_mode == PositionMode::Oneway || e.side == side)
                })
            })
            .cloned();
        if quantity == 0.0 && existing.is_none() {
            return;
        }
        let setting = cx.store.setting(&symbol);
        let entry =
            p.ep.or_else(|| existing.as_ref().map(|p| p.entry_price))
                .unwrap_or(0.0);
        let upnl =
            p.up.or_else(|| existing.as_ref().map(|p| p.unrealized_pnl))
                .unwrap_or(0.0);
        let leverage = existing
            .as_ref()
            .map(|p| p.leverage)
            .or(setting.map(|s| s.leverage))
            .filter(|l| l.is_finite() && *l > 0.0)
            .unwrap_or(1.0);
        let notional = quantity.abs() * entry + upnl;
        cx.store.upsert_position(Position {
            id: existing.as_ref().map(|p| p.id.clone()).unwrap_or(expected),
            symbol,
            side: if quantity == 0.0 {
                existing.as_ref().map_or(side, |p| p.side)
            } else {
                side
            },
            entry_price: entry,
            notional,
            leverage,
            margin_type: p
                .mt
                .as_deref()
                .map(margin_type)
                .or_else(|| existing.as_ref().and_then(|p| p.margin_type))
                .or(Some(MarginType::Cross)),
            position_mode: Some(current_mode),
            unrealized_pnl: upnl,
            realized_pnl: existing.as_ref().and_then(|p| p.realized_pnl),
            contracts: quantity.abs(),
            liquidation_price: p
                .liqp
                .or_else(|| existing.as_ref().map(|p| p.liquidation_price))
                .unwrap_or(0.0),
            margin_size: existing
                .as_ref()
                .and_then(|p| p.margin_size)
                .or(Some(notional / leverage)),
            last_update: Some(cx.now),
        });
    }
}

fn observation(order: Order, native: &NativeOrder<'_>) -> OrderMeta {
    (
        order,
        native.kind.to_string(),
        native.position_side.to_string(),
        native
            .take_profit
            .as_ref()
            .is_some_and(Protection::meaningful)
            || native
                .stop_loss
                .as_ref()
                .is_some_and(Protection::meaningful),
        native.status.as_deref().unwrap_or_default().to_owned(),
    )
}

pub(crate) fn map_order(native: &NativeOrder<'_>, cx: &Cx<'_, Pending>) -> Option<Order> {
    if !matches!(native.side.as_ref(), "BUY" | "SELL") || native.symbol.is_empty() {
        return None;
    }
    let id = native.id()?;
    let quantity = native.quantity.or(native.close_position.then_some(0.0))?;
    let mut kind = order_type(&native.kind)?;
    let symbol = native.symbol.replace('-', "");
    let side = order_side(&native.side);
    let status = match native.status.as_deref().unwrap_or("NEW") {
        "NEW" | "OPEN" | "PENDING" | "WORKING" | "PARTIALLY_FILLED" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        _ => return None,
    };
    let price = if native.price > 0.0 {
        native.price
    } else {
        native.stop_price
    };
    if kind == OrderType::StopLoss
        && let Some(ticker) = cx.store.ticker(&symbol)
        && ticker.mark > 0.0
        && ((side == OrderSide::Buy && price < ticker.mark)
            || (side == OrderSide::Sell && price > ticker.mark))
    {
        kind = OrderType::TakeProfit;
    }
    let filled = native.filled.unwrap_or(0.0);
    if quantity < 0.0 || filled < 0.0 {
        return None;
    }
    let existing = cx.store.order(id);
    let tif = if native.post_only {
        Some(TimeInForce::PostOnly)
    } else {
        native
            .time_in_force
            .as_deref()
            .and_then(time_in_force)
            .or_else(|| existing.and_then(|o| o.time_in_force))
    };
    let trigger_type = if matches!(kind, OrderType::StopLoss | OrderType::TakeProfit) {
        match native.working_type.as_deref() {
            Some("MARK_PRICE") => Some(StopTriggerType::MarkPrice),
            Some("CONTRACT_PRICE") => Some(StopTriggerType::LastPrice),
            _ => None,
        }
    } else {
        None
    };
    Some(Order {
        id: id.to_owned(),
        parent_id: None,
        status,
        symbol,
        kind,
        side,
        price,
        amount: quantity,
        filled,
        remaining: (quantity - filled).max(0.0),
        reduce_only: native.reduce_only,
        close: Some(native.close_position),
        client_order_id: native.client_order_id.as_deref().map(str::to_owned),
        time_in_force: tif,
        last_update: cx.now,
        trigger_type,
        take_profit: None,
        stop_loss: None,
        extra: None,
    })
}

pub(crate) fn auth_scope(request: &Request) -> Option<safe_cex_engine::AccountReadScope> {
    use safe_cex_engine::AccountReadScope::*;
    Some(match request {
        Request::PositionMode { .. } => Settings,
        Request::Balance { .. } => Balance,
        Request::Positions { .. } => Positions,
        Request::OpenOrders { .. } => Orders,
        _ => return None,
    })
}
impl crate::Bingx {
    pub(crate) fn account_auth(
        &self,
        pending: &Pending,
        response: &Response<'_>,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        use safe_cex_engine::AuthOutcome::{Accepted, Rejected};
        // A first signature rejection is re-signed once; only the final answer counts.
        let pending = match pending {
            Pending::Signed(signed)
                if !signed.resent && crate::signature_rejected(response.body) =>
            {
                return None;
            }
            Pending::Signed(signed) => &signed.inner,
            pending => pending,
        };
        let Pending::Account(request) = pending else {
            return None;
        };
        auth_scope(request)?;
        #[derive(serde::Deserialize)]
        struct Evidence {
            code: i64,
        }
        let evidence: Evidence = serde_json::from_str(response.body).ok()?;
        if crate::trading::KEY_REFUSALS.contains(&evidence.code) {
            return Some(Rejected);
        }
        if !(200..300).contains(&response.status) || evidence.code != 0 {
            return None;
        }
        match request {
            Request::PositionMode { .. } => serde_json::from_str::<ModeEnvelope>(response.body)
                .is_ok()
                .then_some(Accepted),
            Request::Balance { .. } | Request::Positions { .. } => {
                safe_cex_engine::AuthOutcome::accepted_list(
                    response,
                    ArrayAt::EnvelopeField("data"),
                )
            }
            Request::OpenOrders { .. } => safe_cex_engine::AuthOutcome::accepted_list(
                response,
                ArrayAt::EnvelopeFieldOrNested("data", "orders"),
            ),
            _ => None,
        }
    }
}

/// Metadata reads do not submit a trading effect, including failed confirmations.
pub(crate) fn metadata_error(
    error: impl Into<safe_cex_engine::VenueError>,
) -> safe_cex_engine::VenueError {
    let error = error.into();
    // Typed signer failures already declare their outcome and keep it.
    if error.outcome.is_some() {
        error
    } else {
        error.outcome(safe_cex_engine::Outcome::NotSent)
    }
}

#[cfg(test)]
mod sign_failure_owners {
    use super::*;
    use safe_cex_engine::{Action, Command, Event, Venue};

    struct Probe {
        native: crate::Bingx,
        queued: Option<Request>,
        kind: u8,
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            self.queued = Some(match self.kind {
                0 => Request::Balance {
                    revision: 1,
                    first: None,
                    usdt: None,
                    failed: false,
                },
                1 => Request::Positions {
                    revision: 1,
                    settings_stamp: Observation::default(),
                    rows: vec![],
                    settings: vec![],
                    skipped: vec![],
                    unavailable: false,
                },
                2 => Request::OpenOrders {
                    owner: 1,
                    snapshot: cx.store.begin_orders_snapshot().unwrap(),
                    read_epoch: 0,
                    rows: vec![],
                    metas: vec![],
                    skipped: vec![],
                    unavailable: false,
                },
                3 => Request::ListenKey { epoch: 4 },
                _ => Request::Keepalive { epoch: 4 },
            });
            cx.store.invalidate_order_snapshots();
            cx.set_timer(1.);
        }
        fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {
            panic!("fixture issues no request")
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
            panic!("fixture is not a network failure")
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
            self.native.on_message(cx, socket, text);
        }
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, _: TimerId) {
            self.native.on_sign_error(
                cx,
                Pending::Account(self.queued.take().unwrap()),
                safe_cex_engine::signing::SignError::Refused.venue_error(),
            );
        }
        fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn fail(kind: u8, active: u64) -> safe_cex_engine::Engine<Probe> {
        let credentials = Credentials {
            read: None,
            primary: safe_cex_engine::signing::Identity::new(
                safe_cex_engine::signing::KeyRef("ref:owned-credential".into()),
                Some(safe_cex_engine::signing::KeyRef("ref:owned-signing".into())),
            ),
        };
        let mut native = crate::Bingx::new(crate::Options {
            credentials: Some(credentials.primary),
            ..Default::default()
        });
        let a = &mut native.account;
        a.balance_in_flight = Some(active);
        a.positions_in_flight = Some(active);
        a.orders_in_flight = Some(active);
        // Financial revisions may advance while the original reservation owns a read.
        a.balance_revision = 9;
        a.positions_revision = 9;
        a.orders_read_sequence = 9;
        a.epoch = 5;
        a.key_in_flight = true;
        a.socket = Some(SocketId(44));
        a.keepalive = Some(TimerId(77));
        let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
            native,
            queued: None,
            kind,
        });
        engine.start(1_000.);
        let timers: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        engine.handle(1_001., Event::TimerFired { timer: timers[0] });
        engine
    }
    #[test]
    fn signer_failure_cannot_clear_a_newer_read_reservation() {
        for kind in 0..3 {
            let engine = fail(kind, 2);
            let a = &engine.venue().native.account;
            assert_eq!(
                match kind {
                    0 => a.balance_in_flight,
                    1 => a.positions_in_flight,
                    _ => a.orders_in_flight,
                },
                Some(2),
                "owner {kind}"
            );
        }
    }
    #[test]
    fn signer_failure_releases_its_reservation_after_financial_revision_advances() {
        for kind in 0..3 {
            let engine = fail(kind, 1);
            let a = &engine.venue().native.account;
            assert_eq!(
                match kind {
                    0 => a.balance_in_flight,
                    1 => a.positions_in_flight,
                    _ => a.orders_in_flight,
                },
                None,
                "owner {kind}"
            );
        }
    }
    #[test]
    fn stale_listen_and_keepalive_signer_failures_cannot_retire_replacement_session() {
        for kind in 3..5 {
            let mut engine = fail(kind, 2);
            let a = &engine.venue().native.account;
            assert_eq!(a.epoch, 5);
            assert!(a.key_in_flight);
            assert_eq!(a.socket, Some(SocketId(44)));
            assert_eq!(a.keepalive, Some(TimerId(77)));
            assert!(engine.drain_actions().next().is_none());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::{Action, Command, Event, Venue, book::Book, wire};

    #[derive(Default)]
    struct PlacementVenue {
        account: Account,
    }
    impl Venue for PlacementVenue {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            cx.request(
                Method::Post,
                "fixture".into(),
                Vec::new(),
                None,
                Pending::Account(Request::Keepalive { epoch: 0 }),
            );
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_message(&mut self, cx: &mut Cx<'_, Pending>, _: SocketId, text: &str) {
            self.account.on_message(
                cx,
                text,
                &Credentials {
                    read: None,
                    primary: safe_cex_engine::signing::Identity::new(
                        safe_cex_engine::signing::KeyRef("ref:fixture-credential".into()),
                        Some(safe_cex_engine::signing::KeyRef(
                            "ref:fixture-signing".into(),
                        )),
                    ),
                },
            );
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, _: Pending, response: Response<'_>) {
            self.account.accept_placed_response(cx, response.body);
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
        fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn cumulative_engine() -> safe_cex_engine::Engine<PlacementVenue> {
        let mut engine = safe_cex_engine::test_support::seeded_engine(PlacementVenue::default());
        engine.start(0.0);
        engine.drain_actions().for_each(drop);
        engine.take_market_frame(0.0);
        let event = execution_event("123", "seed", Some(0.0), 0.0, 100.0, "NEW");
        deliver_execution(&mut engine, 1.0, &event);
        engine
    }

    fn execution_event(
        id: &str,
        trade: &str,
        cumulative: Option<f64>,
        last: f64,
        price: f64,
        status: &str,
    ) -> String {
        let cumulative = cumulative.map_or(String::new(), |n| format!(r#","z":"{n}""#));
        format!(
            r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":"{id}","t":"{trade}","s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"{status}","q":"2","l":"{last}","L":"0","ap":"0","p":"{price}"{cumulative}}}}}"#
        )
    }

    fn deliver_execution(
        engine: &mut safe_cex_engine::Engine<PlacementVenue>,
        now: f64,
        event: &str,
    ) -> Option<(f64, f64)> {
        engine.handle(
            now,
            Event::WsMessage {
                socket: SocketId(0),
                text: event,
            },
        );
        let frame = engine.take_market_frame(now)?;
        let (_, fills) = wire::sections(frame).find(|(kind, _)| *kind == wire::section::FILLS)?;
        assert_eq!(u32::from_le_bytes(fills[..4].try_into().unwrap()), 1);
        Some((
            f64::from_le_bytes(fills[16..24].try_into().unwrap()),
            f64::from_le_bytes(fills[24..32].try_into().unwrap()),
        ))
    }

    #[test]
    fn cumulative_zero_selected_price_does_not_emit_fill() {
        let mut engine = cumulative_engine();
        let event = execution_event("123", "bad", Some(0.5), 0.5, 0.0, "PARTIALLY_FILLED");
        assert_eq!(deliver_execution(&mut engine, 2.0, &event), None);
    }

    #[test]
    fn cumulative_negative_selected_price_does_not_emit_fill() {
        let mut engine = cumulative_engine();
        let event = execution_event("123", "bad", Some(0.5), 0.5, -1.0, "PARTIALLY_FILLED");
        assert_eq!(deliver_execution(&mut engine, 2.0, &event), None);
    }

    #[test]
    fn cumulative_invalid_price_retains_corrected_trade_and_healthy_peer() {
        let mut engine = cumulative_engine();
        let invalid = execution_event("123", "retry", Some(0.5), 0.5, 0.0, "PARTIALLY_FILLED");
        // Deliberately drain without asserting, so original source reaches the recovery check.
        deliver_execution(&mut engine, 2.0, &invalid);
        let peer = execution_event("124", "peer", Some(0.25), 0.25, 60.0, "PARTIALLY_FILLED");
        assert_eq!(
            deliver_execution(&mut engine, 3.0, &peer),
            Some((60.0, 0.25))
        );
        let corrected = execution_event("123", "retry", Some(0.5), 0.5, 100.0, "PARTIALLY_FILLED");
        assert_eq!(
            deliver_execution(&mut engine, 4.0, &corrected),
            Some((100.0, 0.5))
        );
    }

    #[test]
    fn cumulative_invalid_terminal_price_retains_stored_partial() {
        let mut engine = cumulative_engine();
        let partial = execution_event(
            "123",
            "partial",
            Some(0.25),
            0.25,
            100.0,
            "PARTIALLY_FILLED",
        );
        assert_eq!(
            deliver_execution(&mut engine, 2.0, &partial),
            Some((100.0, 0.25))
        );
        let before = engine.store().order("123").unwrap().clone();
        let invalid = execution_event("123", "terminal", Some(2.0), 1.75, 0.0, "FILLED");
        deliver_execution(&mut engine, 3.0, &invalid);
        let after = engine.store().order("123").unwrap();
        assert_eq!(after.filled, before.filled);
        assert_eq!(after.status, before.status);
        assert_eq!(after.last_update, before.last_update);
    }

    #[test]
    fn cumulative_unused_bad_last_price_keeps_selected_fallback() {
        let mut engine = cumulative_engine();
        let event = execution_event("123", "fallback", Some(0.5), 0.5, 100.0, "PARTIALLY_FILLED")
            .replace(r#""L":"0""#, r#""L":"-1""#);
        assert_eq!(
            deliver_execution(&mut engine, 2.0, &event),
            Some((100.0, 0.5))
        );
    }

    #[test]
    fn cumulative_valid_zero_growth_still_remembers_trade_identity() {
        let mut engine = cumulative_engine();
        let zero = execution_event("123", "quiet", Some(0.0), 0.5, 0.0, "PARTIALLY_FILLED");
        assert_eq!(deliver_execution(&mut engine, 2.0, &zero), None);
        let repeated = execution_event("123", "quiet", None, 0.5, 100.0, "PARTIALLY_FILLED");
        assert_eq!(deliver_execution(&mut engine, 3.0, &repeated), None);
    }

    #[test]
    fn placement_acknowledgements_do_not_suppress_first_stream_execution() {
        for (kind, status, cumulative) in [
            ("MARKET", "FILLED", true),
            ("MARKET", "FILLED", false),
            ("LIMIT", "PARTIALLY_FILLED", true),
            ("LIMIT", "PARTIALLY_FILLED", false),
        ] {
            let mut engine =
                safe_cex_engine::test_support::seeded_engine(PlacementVenue::default());
            engine.start(0.0);
            let request = engine
                .drain_actions()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r.id)
                    } else {
                        None
                    }
                })
                .unwrap();
            let ack = format!(
                r#"{{"code":0,"data":{{"order":{{"orderId":123,"type":"{kind}","symbol":"BTC-USDT","side":"BUY","status":"{status}","quantity":"0.2","executedQty":"0.1","price":"60000"}}}}}}"#
            );
            engine.handle(
                1.0,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: &ack,
                    },
                },
            );
            assert!(!engine.venue().account.cumulative_fills.contains_key("123"));
            let cumulative_field = if cumulative { r#""z":"0.1","# } else { "" };
            let event = format!(
                r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":123,"s":"BTC-USDT","o":"{kind}","S":"BUY","X":"{status}","q":"0.2",{cumulative_field}"l":"0.1","L":"60000"}}}}"#
            );
            engine.handle(
                2.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            let frame = engine.take_market_frame(2.0).unwrap();
            let (_, fills) = wire::sections(frame)
                .find(|(kind, _)| *kind == wire::section::FILLS)
                .expect("WS execution must emit fill after placement ack");
            assert_eq!(f64::from_le_bytes(fills[24..32].try_into().unwrap()), 0.1);
            engine.handle(
                3.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            assert!(engine.take_market_frame(3.0).is_none());
        }
    }

    #[test]
    fn late_placement_ack_preserves_private_partial_and_terminal_tombstone() {
        for terminal in [false, true] {
            let mut engine =
                safe_cex_engine::test_support::seeded_engine(PlacementVenue::default());
            engine.start(0.0);
            let request = engine
                .drain_actions()
                .find_map(|a| {
                    if let Action::Http(r) = a {
                        Some(r.id)
                    } else {
                        None
                    }
                })
                .unwrap();
            engine.handle(1.0,Event::WsMessage{socket:SocketId(0),text:r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":123,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"NEW","q":"0.2","z":"0","p":"60000"}}"#});
            let status = if terminal {
                "FILLED"
            } else {
                "PARTIALLY_FILLED"
            };
            let quantity = if terminal { 0.2 } else { 0.1 };
            let event = format!(
                r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":123,"s":"BTC-USDT","o":"LIMIT","S":"BUY","X":"{status}","q":"0.2","z":"{quantity}","l":"{quantity}","L":"60000"}}}}"#
            );
            engine.handle(
                2.0,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: &event,
                },
            );
            if terminal {
                engine.store_mut().set_orders(Vec::new());
            }
            let body = r#"{"code":0,"data":{"order":{"orderId":123,"type":"LIMIT","symbol":"BTC-USDT","side":"BUY","status":"NEW","quantity":"0.2","executedQty":"0","price":"60000"}}}"#;
            engine.handle(
                3.0,
                Event::HttpResponse {
                    request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            if terminal {
                assert!(engine.store().orders().is_empty());
            } else {
                assert_eq!(engine.store().order("123").unwrap().filled, 0.1);
            }
            assert!(engine.venue().account.observations.is_empty());
        }
    }

    #[test]
    fn execution_retention_evicts_individually_and_keeps_recent_terminals() {
        let mut account = Account::default();
        for i in 0..RETAINED_ORDERS + 100 {
            let id = i.to_string();
            account.record_cumulative(&id, 1.0);
            account.record_terminal(&id);
            account.covered_history.insert(id);
        }
        assert_eq!(account.cumulative_fills.len(), RETAINED_ORDERS);
        assert_eq!(account.terminal_orders.len(), RETAINED_ORDERS);
        assert!(!account.cumulative_fills.contains_key("0"));
        assert_eq!(account.covered_history.len(), RETAINED_ORDERS);
        assert!(!account.covered_history.contains("0"));
        assert!(
            account
                .terminal_orders
                .contains_key(&(RETAINED_ORDERS + 99).to_string())
        );
        for i in 0..RETAINED_ORDERS + 100 {
            account.record_cumulative(&format!("active-{i}"), 0.0);
        }
        assert_eq!(account.covered_history.len(), RETAINED_ORDERS);
        assert!(
            account
                .terminal_orders
                .contains_key(&(RETAINED_ORDERS + 99).to_string())
        );
        for i in 0..RETAINED_EXECUTIONS + 100 {
            assert!(account.first_execution(i.to_string()));
        }
        assert_eq!(account.seen_executions.len(), RETAINED_EXECUTIONS);
        assert!(!account.first_execution((RETAINED_EXECUTIONS + 99).to_string()));
        assert!(account.first_execution("0".into()));
        assert_eq!(account.seen_executions.len(), RETAINED_EXECUTIONS);
    }
}
