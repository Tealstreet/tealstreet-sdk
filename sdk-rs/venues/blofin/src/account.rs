//! Atomic private snapshots, protected from newer stream and command updates.
use super::*;
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::credential_selection::{
    CredentialRole, CredentialUnavailable, PrimaryReason, SlotPolicy, Slots,
};
use safe_cex_engine::execution::{
    Arithmetic, CumulativePrice, Cursor, ExecutionRounding, Rejection, Report,
};
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::selected_read::{
    AdmittedRead, BackgroundReadError, LocalRefusal, ReadPolicy, ReadRequest, ReadRole,
    SelectedRead,
};
use safe_cex_engine::startup::rate_limit_pause_ms;
use safe_cex_engine::{Fill, OrderSnapshotToken};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};

mod venue_state;

/// BloFin counts REST limits per one-minute window, so a 418/429 without
/// `Retry-After` waits out a whole window before the settings read retries.
const RATE_LIMIT_PAUSE_MS: f64 = 60_000.0;

/// A skipped row is an error only when the store has no copy for it to keep.
fn skip_level(stored: bool) -> Level {
    if stored { Level::Warn } else { Level::Error }
}
/// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut RowReports<String>,
    key: &str,
    level: Level,
    message: impl Into<String>,
) {
    logged.report(cx, key.to_owned(), level, message);
}
/// The identity fields of a position or order row that failed to decode.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RowIdentity<'a> {
    #[serde(borrow, default)]
    inst_id: Option<Str<'a>>,
    #[serde(borrow, default)]
    inst_type: Option<Str<'a>>,
    #[serde(borrow, default)]
    position_side: Option<Str<'a>>,
    #[serde(borrow, default)]
    order_id: Option<Str<'a>>,
    #[serde(borrow, default)]
    tpsl_id: Option<Str<'a>>,
    #[serde(borrow, default)]
    algo_id: Option<Str<'a>>,
}

#[derive(Clone, Default)]
pub struct Source {
    execution_baseline: (f64, Option<f64>),
    pub inst_id: String,
    pub margin_mode: String,
    pub position_side: String,
    pub order_id: String,
    pub tpsl_id: String,
    pub algo_id: String,
    pub trigger_price_type: String,
    pub order_price: f64,
    pub tp: f64,
    pub sl: f64,
    pub tp_type: String,
    pub sl_type: String,
    pub tp_order_price: f64,
    pub sl_order_price: f64,
}
// BloFin reports cumulative quantities/value, not individual execution IDs.
// Keep early observations until the first complete REST census proves a prefix.
struct Execution {
    fill: Fill,
    qty: f64,
    value: f64,
    contract: f64,
    updated: f64,
}

#[derive(Default)]
pub(crate) struct Account {
    pub(super) authenticated: bool,
    pub connection: Connection,
    pub margin: Option<MarginType>,
    pub mode_loaded: bool,
    pub positions_revision: u64,
    pub balance_revision: u64,
    pub settings_revision: u64,
    orders_busy: bool,
    startup_started: bool,
    session_started: f64,
    orders_seeded: bool,
    /// A reconcile snapshot was requested while another was in flight.
    reconcile_owed: bool,
    uncertainty_generation: u64,
    reporting_started: f64,
    startup_executions: VecDeque<Execution>,
    account_busy: bool,
    pub sources: HashMap<String, Source>,
    fills: HashMap<String, (f64, Option<f64>)>,
    fill_order: VecDeque<String>,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
    tick: Option<TimerId>,
    debounce: Option<TimerId>,
    retry: Option<(TimerId, u32)>,
    auth_failures: u32,
    /// Backs off the settings reads that gate startup; transient failures never end it.
    startup: safe_cex_engine::startup::Backoff<StartupRead>,
    /// Consecutive 152409 answers per settings read, indexed by `StartupRead`.
    signature_rejections: [u8; 2],
    pub startup_rejected: bool,
    /// Instrument ids (empty when unidentified) whose skipped position row was already logged.
    position_skips: RowReports<String>,
    /// Order ids (empty when unidentified) whose skipped row was already logged.
    order_skips: RowReports<String>,
}
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum StartupRead {
    Margin,
    Mode,
}
impl Account {
    fn execution_progress(&mut self, id: &str, qty: f64, value: Option<f64>) {
        if !matches!(
            execution_prefix(self.fills.get(id).copied(), qty, value),
            Ok(true)
        ) {
            return;
        }
        self.store_execution_prefix(id, qty, value);
    }
    fn historical_execution_floor(&mut self, id: &str, qty: f64) {
        if qty.is_finite() && qty > self.fills.get(id).map_or(0.0, |prefix| prefix.0) {
            // Unpriceable pre-session quantity is history, never a live suffix.
            self.store_execution_prefix(id, qty, None);
        }
    }
    fn store_execution_prefix(&mut self, id: &str, qty: f64, value: Option<f64>) {
        self.fills.insert(id.to_owned(), (qty, value));
        self.fill_order.retain(|key| key != id);
        self.fill_order.push_back(id.to_owned());
        while self.fill_order.len() > 500 {
            if let Some(id) = self.fill_order.pop_front() {
                self.fills.remove(&id);
            }
        }
    }
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn remember_terminal(&mut self, id: &str) {
        if self.terminal.insert(id.to_owned()) {
            self.terminal_order.push_back(id.to_owned());
            if self.terminal_order.len() > 500
                && let Some(expired) = self.terminal_order.pop_front()
            {
                self.terminal.remove(&expired);
            }
        }
    }
}
/// One command-owned leverage Read under its selected opaque identity.
pub struct Leverage {
    read: AdmittedRead,
    symbol: String,
    inst_id: String,
    margin: MarginType,
    settings_revision: u64,
    positions_revision: u64,
}
pub enum Request {
    Margin {
        revision: u64,
    },
    Mode {
        revision: u64,
    },
    Positions {
        positions: SnapshotBuilder<String, Position>,
        used: f64,
        upnl: f64,
        positions_revision: u64,
        balance_revision: u64,
        failed: bool,
    },
    Balance {
        positions: SnapshotBuilder<String, Position>,
        used: f64,
        upnl: f64,
        available: f64,
        frozen: f64,
        positions_revision: u64,
        balance_revision: u64,
        failed: bool,
    },
    Orders {
        family: u8,
        page: u32,
        attempt: u32,
        cursor: String,
        last_cursor: String,
        page_rows: usize,
        rows: SnapshotBuilder<String, Order>,
        sources: HashMap<String, Source>,
        snapshot: OrderSnapshotToken,
        /// A Primary-signed reconcile snapshot: only it may clear an order fence.
        reconcile: bool,
        uncertainty_generation: u64,
        read_epoch: u64,
        failed: bool,
    },
}
impl Request {
    fn reconciles(&self) -> bool {
        matches!(
            self,
            Self::Orders {
                reconcile: true,
                ..
            }
        )
    }
}
impl Blofin {
    fn seed_execution_prefix(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        qty: f64,
        value: Option<f64>,
    ) {
        match execution_prefix(self.account.fills.get(id).copied(), qty, value) {
            Ok(true) => self.account.execution_progress(id, qty, value),
            Ok(false) => {}
            Err(error) => cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {id}: {error}; retained proven execution baseline")),
        }
    }
    /// The signed-GET selection every private read shares, with its native
    /// refusal text. A configured Secondary serves GETs beside a Primary; the
    /// refusal inspects the same immutable Slots that SelectedRead selects from.
    pub(crate) fn read_selection(
        &self,
    ) -> (Result<SelectedRead, CredentialUnavailable>, &'static str) {
        self.read_selection_for(ReadRole::Read { scope: None })
    }
    fn read_selection_for(
        &self,
        role: ReadRole,
    ) -> (Result<SelectedRead, CredentialUnavailable>, &'static str) {
        let slots = self.credential_slots();
        let credential = match role {
            ReadRole::Read { scope } => CredentialRole::Read { scope },
            ReadRole::Primary { reason, scope } => CredentialRole::PrimaryRead { reason, scope },
        };
        let refusal = match slots.select(credential) {
            Ok((_, identity)) if identity.passphrase.is_some() => None,
            Ok(_) => Some("BloFin requires a passphrase"),
            Err(_) => Some("BloFin requires API credentials"),
        };
        let selected = match refusal {
            Some(_) => Err(CredentialUnavailable::Missing),
            None => SelectedRead::select(&slots, role),
        };
        (
            selected,
            refusal.unwrap_or("BloFin requires API credentials"),
        )
    }
    fn credential_slots(&self) -> Slots<safe_cex_engine::signing::Identity> {
        let identities = &self.options.identities;
        Slots::new(
            identities
                .primary
                .clone()
                .filter(|_| !self.options.public_data_only),
            identities.secondary.clone(),
            SlotPolicy {
                secondary_requires_primary: true,
            },
        )
    }
    /// The trading job's credential for `role`: Primary for writes, and for
    /// the Verify GET the same Secondary-else-Primary selection reads use.
    pub(crate) fn trade_identity(
        &self,
        role: CredentialRole,
    ) -> Option<(
        safe_cex_engine::request_context::Credential,
        safe_cex_engine::signing::Identity,
    )> {
        let slots = self.credential_slots();
        let (selection, identity) = slots.select(role).ok()?;
        Some((
            selection.credential(identity.credential.clone()),
            identity.clone(),
        ))
    }
    pub(crate) fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Some(market) = cx.store.market(&symbol) else {
            let Ok(context) = cx.admit_http_command(Self::read_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, format!("Unknown market {symbol}"));
            return;
        };
        let Some(margin) = self.account.margin.filter(|_| self.account.mode_loaded) else {
            let Ok(context) = cx.admit_http_command(Self::read_policy()) else {
                return;
            };
            let _ = cx.fail_context(&context, "BloFin margin mode is not loaded");
            return;
        };
        let inst_id = market.id.clone();
        let path = format!(
            "/api/v1/account/batch-leverage-info?instId={inst_id}&marginMode={}",
            if margin == MarginType::Isolated {
                "isolated"
            } else {
                "cross"
            }
        );
        let (selected, message) = self.read_selection();
        let Ok(read) = SelectedRead::command(
            cx,
            selected,
            ReadPolicy {
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            LocalRefusal {
                diagnostic: Some(message.into()),
                error: message.into(),
            },
        ) else {
            return;
        };
        let testnet = self.options.testnet;
        let leverage = |read| {
            Pending::Leverage(Leverage {
                read,
                symbol,
                inst_id,
                margin,
                settings_revision: self.account.settings_revision,
                positions_revision: self.account.positions_revision,
            })
        };
        if let Err((pending, error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: format!("{}{path}", self.host()),
                body: None,
                stream: None,
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path.clone(),
                    None,
                ))
            },
            leverage,
        ) {
            <Self as Venue>::on_sign_error(self, cx, pending, error.venue_error());
        }
    }
    /// Native terminal text, now bound to the command's own Read context.
    pub(crate) fn leverage_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        leverage: Leverage,
        error: VenueError,
    ) {
        let context = leverage.read.context();
        if cx.command_error_context(context, &error.message).is_ok() {
            let _ = cx.fail_context(context, error);
        }
    }
    pub(crate) fn leverage_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        leverage: Leverage,
        response: Response<'_>,
    ) {
        let context = leverage.read.context();
        if cx.validate_read_http_delivery(context, context).is_err() {
            return;
        }
        if let Err(e) = check(&response) {
            if response.body.contains("152401") || response.body.contains("152406") {
                self.account.auth_failures += 1;
            }
            self.leverage_failed(cx, leverage, VenueError::new(e));
            return;
        }
        self.account.auth_failures = 0;
        let Leverage {
            read,
            symbol,
            inst_id,
            margin,
            settings_revision,
            positions_revision,
        } = leverage;
        let result = (|| {
            let Data { data: rows } =
                serde_json::from_str::<Data<Vec<LeverageRow<'_>>>>(response.body)
                    .map_err(|_| "Malformed BloFin leverage response")?;
            if rows.is_empty() || rows.len() > 2 {
                return Err("Malformed BloFin leverage rows");
            }
            let expected_margin = if margin == MarginType::Isolated {
                "isolated"
            } else {
                "cross"
            };
            let mut sides = std::collections::HashSet::new();
            for row in &rows {
                if row.inst_id != inst_id
                    || row.margin_mode != expected_margin
                    || !matches!(row.position_side.as_ref(), "net" | "long" | "short")
                    || !row.leverage.0.is_finite()
                    || row.leverage.0 <= 0.0
                    || !sides.insert(row.position_side.as_ref())
                {
                    return Err("Malformed BloFin leverage row");
                }
            }
            let net = rows.iter().find(|row| row.position_side == "net");
            if net.is_some() && rows.len() != 1 {
                return Err("Conflicting BloFin leverage sides");
            }
            if settings_revision != self.account.settings_revision
                || positions_revision != self.account.positions_revision
            {
                return Err("BloFin leverage response was superseded");
            }
            let buy = rows
                .iter()
                .find(|row| row.position_side == "long")
                .or(net)
                .unwrap_or(&rows[0]);
            let sell = rows
                .iter()
                .find(|row| row.position_side == "short")
                .or(net)
                .unwrap_or(&rows[0]);
            #[derive(Serialize)]
            #[serde(rename_all = "camelCase")]
            struct FetchedLeverage {
                buy_leverage: f64,
                sell_leverage: f64,
                margin_type: MarginType,
                position_mode: PositionMode,
            }
            let result = FetchedLeverage {
                buy_leverage: buy.leverage.0,
                sell_leverage: sell.leverage.0,
                margin_type: margin,
                position_mode: if net.is_some() {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                },
            };
            let json =
                serde_json::to_string(&result).map_err(|_| "Invalid BloFin leverage result")?;
            self.account.settings_revision += 1;
            self.account.positions_revision += 1;
            cx.store.update_setting(&symbol, |setting| {
                setting.leverage = result.buy_leverage;
                setting.buy_leverage = Some(result.buy_leverage);
                setting.sell_leverage = Some(result.sell_leverage);
                setting.margin_type = Some(margin);
            });
            Ok(json)
        })();
        match result {
            Ok(data) => {
                let _ = cx.done_context(read.context(), data);
            }
            Err(error) => {
                let _ = cx.fail_context(read.context(), error);
            }
        }
    }
    pub(crate) fn remember_terminal(&mut self, id: &str) {
        self.account.remember_terminal(id);
    }
    pub(crate) fn filled_native(&self, id: &str) -> f64 {
        self.account.fills.get(id).map_or(0.0, |value| value.0)
    }
    pub(crate) fn signed(&self) -> bool {
        !self.options.public_data_only && self.options.identities.primary.is_some()
    }
    /// One Background read per account leg, signed by the #2084 recipe; the
    /// native revision checks stay the join. A refused selection, admission or
    /// queue settles the leg through `account_failed` before any send.
    fn private_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        path: &str,
        request: Request,
        stream: bool,
    ) {
        // The snapshot that may clear an order fence reads as the trading account.
        let role = if request.reconciles() {
            ReadRole::Primary {
                reason: PrimaryReason::PostUnknownOrderRead,
                scope: None,
            }
        } else {
            ReadRole::Read { scope: None }
        };
        let (selected, message) = self.read_selection_for(role);
        let read = match SelectedRead::background(
            cx,
            selected,
            ReadPolicy {
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(read) => read,
            Err(BackgroundReadError::Selection(_)) => {
                return self.account_failed(cx, request, message);
            }
            Err(BackgroundReadError::Admission(error)) => {
                let error = error.read_venue_error().message;
                return self.account_failed(cx, request, &error);
            }
        };
        let testnet = self.options.testnet;
        let path = path.to_owned();
        if let Err((Pending::Account(_, request), error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: format!("{}{path}", self.host()),
                body: None,
                stream: stream.then_some(ArrayAt::EnvelopeField("data")),
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    testnet,
                    Method::Get,
                    path,
                    None,
                ))
            },
            |read| Pending::Account(read, request),
        ) {
            let error = error.read_venue_error().message;
            self.account_failed(cx, request, &error);
        }
    }
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.signed() {
            return;
        }
        self.account.session_started = cx.now;
        if let Err(error) = self.connect_feed(cx, true) {
            Self::socket_notice(cx, error);
        }
        self.account.tick = Some(cx.set_poll_timer(10_000.0));
        self.read_setting(cx, StartupRead::Margin);
        self.read_setting(cx, StartupRead::Mode);
    }
    fn read_setting(&mut self, cx: &mut Cx<'_, Pending>, read: StartupRead) {
        let revision = self.account.settings_revision;
        let (path, request) = match read {
            StartupRead::Margin => ("/api/v1/account/margin-mode", Request::Margin { revision }),
            StartupRead::Mode => ("/api/v1/account/position-mode", Request::Mode { revision }),
        };
        self.private_read(cx, path, request, false);
    }
    fn setting_loaded(&self, read: StartupRead) -> bool {
        match read {
            StartupRead::Margin => self.account.margin.is_some(),
            StartupRead::Mode => self.account.mode_loaded,
        }
    }
    /// Any outcome but 152409 ends that read's run of signature rejections.
    fn setting_unsettled(&mut self, cx: &mut Cx<'_, Pending>, read: StartupRead) {
        self.account.signature_rejections[read as usize] = 0;
        self.setting_retry(cx, read, 0.0);
    }
    /// An unloaded setting retries with backoff, never before `pause_ms`; the
    /// startup gate never fails closed.
    fn setting_retry(&mut self, cx: &mut Cx<'_, Pending>, read: StartupRead, pause_ms: f64) {
        if self.setting_loaded(read) {
            self.account.startup.succeeded(cx, &read);
        } else if !self.account.startup_rejected {
            self.account.startup.failed_after(cx, read, pause_ms);
        }
    }
    /// A refused key can never pass the settings reads, so startup fails instead
    /// of polling. Only documented key codes count; 152409 only when the same read
    /// gets it twice in a row, i.e. after one re-signed resend.
    fn settings_rejected(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: StartupRead,
        body: &str,
    ) -> bool {
        let envelope = serde_json::from_str::<Envelope<'_>>(body).ok();
        let rejections = &mut self.account.signature_rejections[read as usize];
        let Some(envelope) = envelope else {
            *rejections = 0;
            return false;
        };
        if envelope.code == "152409" {
            *rejections += 1;
            if *rejections < 2 {
                return false;
            }
        } else {
            *rejections = 0;
        }
        let Some(kind) = key_rejection(&envelope.code) else {
            return false;
        };
        let read_key = self.options.identities.secondary.is_some();
        let which = if read_key {
            "read-only API key"
        } else {
            "API key"
        };
        let error = rejection(which, &envelope.code, &envelope.msg, kind);
        self.reject_startup(cx, error, kind == ErrorKind::AuthInvalid && !read_key);
        true
    }
    /// Fails startup once and retires every private retry, read or socket.
    fn reject_startup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        error: VenueError,
        trading_key_refused: bool,
    ) {
        if std::mem::replace(&mut self.account.startup_rejected, true) {
            return;
        }
        for read in [StartupRead::Margin, StartupRead::Mode] {
            self.account.startup.succeeded(cx, &read);
        }
        if let Some(timer) = self.account.tick.take() {
            cx.clear_timer(timer);
        }
        if let Some((timer, _)) = self.account.retry.take() {
            cx.clear_timer(timer);
        }
        if let Some(feed) = self.account.connection.feed {
            cx.stop_socket_feed(feed);
        }
        self.account.authenticated = false;
        if let Some(socket) = self.account.connection.socket.take() {
            cx.close(socket);
        }
        cx.notice(
            Level::Error,
            code::for_startup_error(&error),
            &error.message,
        );
        cx.startup_failed(error);
        // As Bybit: a refused trading key also stops the host's login loop.
        if trading_key_refused {
            cx.deactivate();
        }
    }
    fn private_url(&self) -> &'static str {
        if self.options.testnet {
            "wss://demo-trading-openapi.blofin.com/ws/private"
        } else {
            "wss://openapi.blofin.com/ws/private"
        }
    }
    /// The login signs through the provider with the Primary, as every
    /// private request does; a refusal latches the role and the engine closes.
    pub(crate) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(primary) = self.options.identities.primary.clone() else {
            cx.close(socket);
            if let Some(feed) = self.account.connection.feed {
                cx.stop_socket_feed(feed);
            }
            return;
        };
        cx.send_signed(
            socket,
            String::new(),
            Box::new(crate::dispatch_signing::WsLogin::new(
                primary,
                self.private_url(),
            )),
        );

        cx.send(socket, "ping");
    }
    pub(super) fn private_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.authenticated = true;
        if self.account.margin.is_some() && self.account.mode_loaded {
            self.refresh_account(cx);
            self.refresh_orders(cx, 1);
        }
    }
    pub(super) fn private_auth_failure(&mut self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        if !self.account.authenticated
            && !(cx.store.loaded().balance && cx.store.loaded().orders)
            && error.venue_code.as_deref() != Some("152409")
            && let Some(kind) = error.venue_code.as_deref().and_then(key_rejection)
        {
            let error = rejection(
                "API key login",
                error.venue_code.as_deref().unwrap_or_default(),
                error.venue_message.as_deref().unwrap_or_default(),
                kind,
            );
            self.reject_startup(cx, error, kind == ErrorKind::AuthInvalid);
            return;
        }
        cx.notice(Level::Error, code::STREAM_UNAVAILABLE, error.message);
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        // Frame-fed: positions and balance arrive as frames.
        if cx.frame_fed() || self.account.account_busy || !self.signed() {
            return;
        }
        self.account.account_busy = true;
        self.private_read(
            cx,
            "/api/v1/account/positions",
            Request::Positions {
                positions: SnapshotBuilder::default(),
                used: 0.0,
                upnl: 0.0,
                positions_revision: self.account.positions_revision,
                balance_revision: self.account.balance_revision,
                failed: false,
            },
            true,
        );
    }
    pub(crate) fn reconcile_uncertain_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.reconcile_reask() {
            self.account.uncertainty_generation =
                self.account.uncertainty_generation.wrapping_add(1);
        }
        self.settle_retired_trade(cx);
        self.reconcile_orders(cx);
    }
    /// A Primary order snapshot that may clear an order fence; owed when
    /// another snapshot is in flight.
    pub(crate) fn reconcile_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.reconcile_reask() && (self.account.orders_busy || self.account.retry.is_some()) {
            return;
        }
        self.account.reconcile_owed = true;
        self.refresh_orders(cx, 1);
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>, attempt: u32) {
        // Frame-fed: the data source's ORDERS frames are the reconcile.
        if cx.frame_fed() || self.account.orders_busy || !self.signed() {
            return;
        }
        let Some(snapshot) = cx.store.begin_orders_snapshot() else {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "BloFin order snapshot fence exhausted",
            );
            return;
        };
        self.account.orders_busy = true;
        let reconcile = std::mem::take(&mut self.account.reconcile_owed);
        self.orders_page(
            cx,
            Request::Orders {
                family: 0,
                page: 0,
                attempt,
                cursor: String::new(),
                last_cursor: String::new(),
                page_rows: 0,
                rows: SnapshotBuilder::default(),
                sources: HashMap::new(),
                snapshot,
                reconcile,
                uncertainty_generation: self.account.uncertainty_generation,
                read_epoch: cx.order_read_epoch(),
                failed: false,
            },
        );
    }
    fn orders_page(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Request::Orders { family, cursor, .. } = &request else {
            return;
        };
        let endpoint = match family {
            0 => "orders-pending",
            1 => "orders-tpsl-pending",
            _ => "orders-algo-pending",
        };
        let mut path = format!("/api/v1/trade/{endpoint}?limit=100");
        if *family == 2 {
            path.push_str("&orderType=trigger");
        }
        if !cursor.is_empty() {
            path.push_str("&after=");
            path.push_str(cursor);
        }
        self.private_read(cx, &path, request, true);
    }
    pub(crate) fn map_orders(&self, cx: &Cx<'_, Pending>, o: &OrderRow<'_>) -> Vec<Order> {
        if !o.inst_type.is_empty() && !o.inst_type.eq_ignore_ascii_case("SWAP") {
            return vec![];
        }
        let Some(m) = cx.store.market_by_id(&o.inst_id) else {
            return vec![];
        };
        let side = if o.side == "sell" {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };
        let amount = clean(o.size.0 * m.contract_size.unwrap_or(1.0));
        let filled = clean(o.filled_size.0 * m.contract_size.unwrap_or(1.0));
        let status = match o.state.as_ref() {
            "filled" | "effective" => OrderStatus::Closed,
            "canceled" | "order_failed" => OrderStatus::Canceled,
            _ => OrderStatus::Open,
        };
        let base = Order {
            id: o.order_id.to_string(),
            parent_id: None,
            status,
            symbol: m.symbol.clone(),
            kind: OrderType::Limit,
            side,
            price: o.price.0,
            amount,
            filled,
            remaining: clean(amount - filled),
            reduce_only: o.reduce_only.0,
            close: None,
            client_order_id: (!o.client_order_id.is_empty()).then(|| o.client_order_id.to_string()),
            time_in_force: Some(if o.order_type == "post_only" {
                TimeInForce::PostOnly
            } else {
                TimeInForce::GoodTillCancel
            }),
            last_update: cx.now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(clean(o.size.0 - o.filled_size.0)),
                filled_in_placement_units: Some(o.filled_size.0),
                ..OrderExtra::default()
            }),
        };
        if o.order_type == "trigger"
            && !o.algo_id.is_empty()
            && !o.reduce_only.0
            && o.trigger_price.0 > 0.0
        {
            return vec![Order {
                id: o.algo_id.to_string(),
                kind: OrderType::Conditional,
                price: o.trigger_price.0,
                filled: 0.0,
                remaining: amount,
                time_in_force: None,
                ..base
            }];
        }
        if !o.tpsl_id.is_empty() {
            return [
                (OrderType::TakeProfit, o.tp_trigger_price.0, "take_profit"),
                (OrderType::StopLoss, o.sl_trigger_price.0, "stop_loss"),
            ]
            .into_iter()
            .filter(|(_, p, _)| *p > 0.0)
            .map(|(kind, price, suffix)| Order {
                id: format!("{}__{suffix}", o.tpsl_id),
                kind,
                price,
                side: if o.order_id.is_empty() {
                    side
                } else {
                    side.opposite()
                },
                reduce_only: true,
                filled: 0.0,
                remaining: amount,
                time_in_force: None,
                ..base.clone()
            })
            .collect();
        }
        if o.order_id.is_empty() || o.order_category != "normal" {
            return vec![];
        }
        vec![Order {
            kind: if matches!(o.order_type.as_ref(), "market" | "optimal_limit_ioc") {
                OrderType::Market
            } else {
                OrderType::Limit
            },
            ..base
        }]
    }
    fn source(o: &OrderRow<'_>) -> Source {
        let attached = o.attach_algo_orders.first();
        let (tp, sl, tp_order_price, sl_order_price, tp_type, sl_type) =
            attached.map_or((0.0, 0.0, 0.0, 0.0, "", ""), |a| {
                (
                    a.tp_trigger_price.0,
                    a.sl_trigger_price.0,
                    a.tp_order_price.0,
                    a.sl_order_price.0,
                    a.tp_trigger_price_type.as_ref(),
                    a.sl_trigger_price_type.as_ref(),
                )
            });
        Source {
            execution_baseline: (
                o.filled_size.0,
                (o.average_price.0 > 0.0 || o.filled_size.0 == 0.0)
                    .then_some(o.filled_size.0 * o.average_price.0),
            ),
            inst_id: o.inst_id.to_string(),
            margin_mode: o.margin_mode.to_string(),
            position_side: o.position_side.to_string(),
            order_id: o.order_id.to_string(),
            tpsl_id: o.tpsl_id.to_string(),
            algo_id: o.algo_id.to_string(),
            trigger_price_type: o.trigger_price_type.to_string(),
            order_price: o.order_price.0,
            tp: if o.tp_trigger_price.0 > 0.0 {
                o.tp_trigger_price.0
            } else {
                tp
            },
            sl: if o.sl_trigger_price.0 > 0.0 {
                o.sl_trigger_price.0
            } else {
                sl
            },
            tp_type: if o.tp_trigger_price_type.is_empty() {
                tp_type.to_owned()
            } else {
                o.tp_trigger_price_type.to_string()
            },
            sl_type: if o.sl_trigger_price_type.is_empty() {
                sl_type.to_owned()
            } else {
                o.sl_trigger_price_type.to_string()
            },
            tp_order_price: if o.tp_order_price.0 != 0.0 {
                o.tp_order_price.0
            } else {
                tp_order_price
            },
            sl_order_price: if o.sl_order_price.0 != 0.0 {
                o.sl_order_price.0
            } else {
                sl_order_price
            },
        }
    }
    fn map_position(&self, cx: &Cx<'_, Pending>, p: &PositionRow<'_>) -> Option<Position> {
        if !p.inst_type.is_empty() && !p.inst_type.eq_ignore_ascii_case("SWAP") {
            return None;
        }
        let m = cx.store.market_by_id(&p.inst_id)?;
        let contracts = clean(p.positions.0 * m.contract_size.unwrap_or(1.0));
        let side = if p.position_side == "short" || (p.position_side == "net" && contracts < 0.0) {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let mode = if p.position_side == "net" {
            PositionMode::Oneway
        } else {
            PositionMode::Hedged
        };
        let id = if mode == PositionMode::Oneway {
            m.symbol.clone()
        } else {
            format!(
                "{}:{}",
                m.symbol,
                if side == PositionSide::Long {
                    "long"
                } else {
                    "short"
                }
            )
        };
        let existing = cx.store.position(&id);
        let leverage = if p.leverage.0 > 0.0 {
            p.leverage.0
        } else {
            existing
                .map(|p| p.leverage)
                .filter(|v| v.is_finite() && *v > 0.0)
                .or_else(|| {
                    cx.store
                        .setting(&m.symbol)
                        .map(|s| {
                            if mode == PositionMode::Hedged {
                                if side == PositionSide::Long {
                                    s.buy_leverage
                                } else {
                                    s.sell_leverage
                                }
                                .unwrap_or(s.leverage)
                            } else {
                                s.leverage
                            }
                        })
                        .filter(|v| v.is_finite() && *v > 0.0)
                })
                .unwrap_or(0.0)
        };
        Some(Position {
            id,
            symbol: m.symbol.clone(),
            side,
            entry_price: if contracts != 0.0 {
                p.average_price.0
            } else {
                0.0
            },
            notional: num::adjust((contracts * p.mark_price.0).abs(), m.precision.price),
            leverage,
            margin_type: Some(if p.margin_mode == "isolated" {
                MarginType::Isolated
            } else {
                MarginType::Cross
            }),
            position_mode: Some(mode),
            unrealized_pnl: num::adjust(p.unrealized_pnl.0, m.precision.price),
            realized_pnl: None,
            contracts: contracts.abs(),
            liquidation_price: p
                .liquidation_price
                .parse::<f64>()
                .ok()
                .filter(|p| p.is_finite())
                .unwrap_or_else(|| existing.map_or(0.0, |p| p.liquidation_price)),
            margin_size: None,
            last_update: None,
        })
    }
    fn commit_positions(&mut self, cx: &mut Cx<'_, Pending>, positions: Vec<Position>) {
        for p in &positions {
            cx.store.update_setting(&p.symbol, |s| {
                if p.position_mode == Some(PositionMode::Hedged) {
                    if p.side == PositionSide::Long {
                        s.buy_leverage = Some(p.leverage);
                    } else {
                        s.sell_leverage = Some(p.leverage);
                    }
                } else {
                    s.buy_leverage = Some(p.leverage);
                    s.sell_leverage = Some(p.leverage);
                }
                s.leverage = s.buy_leverage.or(s.sell_leverage).unwrap_or(p.leverage);
                s.margin_type = p.margin_type;
            });
        }
        // Position margin overrides apply only while that symbol is open. A
        // close (including omission from a full snapshot) restores the account
        // default without losing the symbol's last confirmed leverage.
        if let Some(margin) = self.account.margin {
            let open: HashSet<_> = positions
                .iter()
                .filter(|position| position.contracts > 0.0)
                .map(|position| position.symbol.as_str())
                .collect();
            let closed: Vec<_> = cx
                .store
                .markets()
                .iter()
                .filter(|market| !open.contains(market.symbol.as_str()))
                .filter(|market| {
                    cx.store
                        .setting(&market.symbol)
                        .is_some_and(|setting| setting.margin_type != Some(margin))
                })
                .map(|market| market.symbol.clone())
                .collect();
            for symbol in closed {
                cx.store
                    .update_setting(&symbol, |setting| setting.margin_type = Some(margin));
            }
        }
        cx.store.set_positions(
            positions
                .into_iter()
                .filter(|p| p.contracts > 0.0)
                .collect(),
        );
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        match request {
            Request::Positions {
                positions,
                used,
                upnl,
                failed,
                ..
            } => match serde_json::from_str::<PositionRow<'_>>(item) {
                Ok(p) => {
                    self.account.position_skips.forget(p.inst_id.as_ref());
                    if (p.inst_type.is_empty() || p.inst_type.eq_ignore_ascii_case("SWAP"))
                        && p.positions.0 != 0.0
                        && cx.store.market_by_id(&p.inst_id).is_none()
                    {
                        if self.malformed_markets.contains(p.inst_id.as_ref()) {
                            *failed = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "BloFin owned position has unavailable market metadata; preserving account snapshot");
                            return;
                        }
                        // As the stream does, account totals still cover the native row.
                        *used += p.maintenance_margin.0;
                        *upnl += p.unrealized_pnl.0;
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!(
                                "Skipping BloFin position on uncatalogued market {}",
                                p.inst_id
                            ),
                        );
                        return;
                    }
                    if let Some(position) = self.map_position(cx, &p) {
                        *used += p.maintenance_margin.0;
                        *upnl += position.unrealized_pnl;
                        positions.push(position);
                    }
                }
                Err(_) => self.reject_position(cx, positions, failed, item),
            },
            Request::Balance {
                available,
                frozen,
                failed,
                ..
            } => match serde_json::from_str::<AssetRow>(item) {
                Ok(b) => {
                    *available += b.available.0;
                    *frozen += b.frozen.0;
                }
                Err(_) => *failed = true,
            },
            Request::Orders {
                family,
                last_cursor,
                page_rows,
                rows,
                sources,
                failed,
                ..
            } => match serde_json::from_str::<OrderRow<'_>>(item) {
                Ok(o) => {
                    for id in [&o.order_id, &o.tpsl_id, &o.algo_id] {
                        self.account.order_skips.forget(id.as_ref());
                    }
                    if (o.inst_type.is_empty() || o.inst_type.eq_ignore_ascii_case("SWAP"))
                        && !matches!(
                            o.state.as_ref(),
                            "filled" | "effective" | "canceled" | "order_failed"
                        )
                        && cx.store.market_by_id(&o.inst_id).is_none()
                    {
                        if self.malformed_markets.contains(o.inst_id.as_ref()) {
                            *failed = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "BloFin working order has unavailable market metadata; preserving order snapshot");
                            return;
                        }
                        // Still counted for paging; it maps to no stored order.
                        cx.notice(
                            Level::Warn,
                            code::ROW_SKIPPED,
                            format!("Skipping BloFin order on uncatalogued market {}", o.inst_id),
                        );
                    }
                    *page_rows += 1;
                    *last_cursor = match family {
                        0 => o.order_id.to_string(),
                        1 => o.tpsl_id.to_string(),
                        _ => o.algo_id.to_string(),
                    };
                    if last_cursor.is_empty() {
                        *failed = true;
                    }
                    let source = Self::source(&o);
                    for order in self.map_orders(cx, &o) {
                        sources.insert(order.id.clone(), source.clone());
                        rows.push(order);
                    }
                }
                Err(_) => {
                    let Some(cursor) = self.reject_order(cx, *family, rows, item) else {
                        *failed = true;
                        return;
                    };
                    // Still counted for paging; its stored copies stay.
                    *page_rows += 1;
                    *last_cursor = cursor;
                }
            },
            _ => {}
        }
    }
    /// Rejects an undecodable position row by its instrument's legs. A row on a
    /// catalogue-malformed market holds the snapshot; one on a filtered market is skipped.
    fn reject_position(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        positions: &mut SnapshotBuilder<String, Position>,
        failed: &mut bool,
        item: &str,
    ) {
        let row = serde_json::from_str::<RowIdentity<'_>>(item).unwrap_or_default();
        if row
            .inst_type
            .as_deref()
            .is_some_and(|t| !t.is_empty() && !t.eq_ignore_ascii_case("SWAP"))
        {
            return;
        }
        let inst_id = row.inst_id.as_deref().unwrap_or_default();
        let skips = &mut self.account.position_skips;
        if inst_id.is_empty() {
            let level = skip_level(!cx.store.positions().is_empty());
            let message = "BloFin skipped malformed position row without an instrument";
            log_skip(cx, skips, "", level, message);
            positions.reject(None);
            return;
        }
        let Some(symbol) = cx.store.market_by_id(inst_id).map(|m| m.symbol.clone()) else {
            if self.malformed_markets.contains(inst_id) {
                *failed = true;
            }
            return;
        };
        let ids = match row.position_side.as_deref() {
            Some("net") => vec![symbol.clone()],
            Some(side @ ("long" | "short")) => vec![format!("{symbol}:{side}")],
            _ => vec![
                symbol.clone(),
                format!("{symbol}:long"),
                format!("{symbol}:short"),
            ],
        };
        let level = skip_level(ids.iter().any(|id| cx.store.position(id).is_some()));
        let message = format!("BloFin skipped malformed position row {inst_id}");
        log_skip(cx, skips, inst_id, level, message);
        for id in ids {
            positions.reject(Some(id));
        }
    }
    /// Rejects an undecodable order row by every id it may map to and returns its paging
    /// cursor; None (the read fails) when it names no cursor or a catalogue-malformed market.
    fn reject_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        family: u8,
        rows: &mut SnapshotBuilder<String, Order>,
        item: &str,
    ) -> Option<String> {
        let row = serde_json::from_str::<RowIdentity<'_>>(item).ok()?;
        if let Some(inst_id) = row.inst_id.as_deref().filter(|id| !id.is_empty())
            && cx.store.market_by_id(inst_id).is_none()
            && self.malformed_markets.contains(inst_id)
        {
            return None;
        }
        let text = |id: &Option<Str<'_>>| id.as_deref().unwrap_or_default().to_owned();
        let (order_id, tpsl_id, algo_id) =
            (text(&row.order_id), text(&row.tpsl_id), text(&row.algo_id));
        let cursor = match family {
            0 => order_id.clone(),
            1 => tpsl_id.clone(),
            _ => algo_id.clone(),
        };
        if cursor.is_empty() {
            return None;
        }
        let mut ids = vec![];
        for base in [&order_id, &tpsl_id] {
            if !base.is_empty() {
                ids.extend(["take_profit", "stop_loss"].map(|suffix| format!("{base}__{suffix}")));
            }
        }
        ids.extend([order_id, algo_id].into_iter().filter(|id| !id.is_empty()));
        let level = skip_level(ids.iter().any(|id| cx.store.order(id).is_some()));
        let message = format!("BloFin skipped malformed order row {cursor}");
        log_skip(cx, &mut self.account.order_skips, &cursor, level, message);
        for id in ids {
            rows.reject(Some(id));
        }
        Some(cursor)
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if let Err(e) = check(&response) {
            let gate = match request {
                Request::Margin { .. } => Some(StartupRead::Margin),
                Request::Mode { .. } => Some(StartupRead::Mode),
                _ => None,
            };
            if let Some(read) = gate.filter(|&read| !self.setting_loaded(read)) {
                // settings_rejected already counted this answer; retry without resetting it.
                if !self.settings_rejected(cx, read, response.body) {
                    if response.body.contains("152401") || response.body.contains("152406") {
                        self.account.auth_failures += 1;
                    }
                    let pause = rate_limit_pause_ms(&response, RATE_LIMIT_PAUSE_MS);
                    self.setting_retry(cx, read, pause);
                    cx.retried_failure("blofin.account:977", e);
                }
                return;
            }
            if response.body.contains("152401") || response.body.contains("152406") {
                self.account.auth_failures += 1;
            }
            self.account_failed(cx, request, &e);
            return;
        }
        self.account.auth_failures = 0;
        match request {
            Request::Margin { revision } => {
                if let Ok(Data { data: m }) =
                    serde_json::from_str::<Data<ModeRow<'_>>>(response.body)
                {
                    if !matches!(m.margin_mode.as_ref(), "cross" | "isolated") {
                        cx.notice(
                            Level::Error,
                            code::STREAM_MALFORMED,
                            "Malformed BloFin margin mode",
                        );
                        self.setting_unsettled(cx, StartupRead::Margin);
                        return;
                    }
                    if revision == self.account.settings_revision {
                        self.account.margin = Some(if m.margin_mode == "isolated" {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        });
                        let symbols: Vec<_> = cx
                            .store
                            .markets()
                            .iter()
                            .map(|m| m.symbol.clone())
                            .collect();
                        for symbol in symbols {
                            cx.store
                                .update_setting(&symbol, |s| s.margin_type = self.account.margin);
                        }
                    }
                    self.setting_unsettled(cx, StartupRead::Margin);
                    self.bootstrap_account(cx);
                } else {
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Malformed BloFin margin mode",
                    );
                    self.setting_unsettled(cx, StartupRead::Margin);
                }
            }
            Request::Mode { revision } => {
                if let Ok(Data { data: m }) =
                    serde_json::from_str::<Data<ModeRow<'_>>>(response.body)
                {
                    if !matches!(m.position_mode.as_ref(), "net_mode" | "long_short_mode") {
                        cx.notice(
                            Level::Error,
                            code::STREAM_MALFORMED,
                            "Malformed BloFin position mode",
                        );
                        self.setting_unsettled(cx, StartupRead::Mode);
                        return;
                    }
                    if revision == self.account.settings_revision {
                        cx.store.set_hedged(m.position_mode == "long_short_mode");
                        self.account.mode_loaded = true;
                    }
                    self.setting_unsettled(cx, StartupRead::Mode);
                    self.bootstrap_account(cx);
                } else {
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Malformed BloFin position mode",
                    );
                    self.setting_unsettled(cx, StartupRead::Mode);
                }
            }
            Request::Positions {
                positions,
                positions_revision,
                ..
            } if positions.rejected() > 0 => {
                // Used margin and UPnL sum the rows, so the balance waits for a clean read.
                self.account.account_busy = false;
                if positions_revision == self.account.positions_revision {
                    // A rejected row cannot prove its position flat; keep the stored copy.
                    let positions =
                        positions.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                    self.commit_positions(cx, positions);
                }
            }
            Request::Positions {
                positions,
                used,
                upnl,
                positions_revision,
                balance_revision,
                failed: false,
            } => self.private_read(
                cx,
                "/api/v1/asset/balances?accountType=futures",
                Request::Balance {
                    positions,
                    used,
                    upnl,
                    available: 0.0,
                    frozen: 0.0,
                    positions_revision,
                    balance_revision,
                    failed: false,
                },
                true,
            ),
            Request::Balance {
                positions,
                used,
                upnl,
                available,
                frozen,
                positions_revision,
                balance_revision,
                failed: false,
            } => {
                self.account.account_busy = false;
                if positions_revision == self.account.positions_revision {
                    self.account.position_skips.clear();
                    self.commit_positions(cx, positions.commit([], |p| p.id.clone()));
                }
                if balance_revision == self.account.balance_revision {
                    let used = round_usd(used);
                    let upnl = round_usd(upnl);
                    cx.store.set_balance(Balance {
                        used,
                        upnl,
                        free: clean(clean(round_usd(available) + round_usd(frozen)) - used),
                        total: clean(clean(round_usd(available) + round_usd(frozen)) - upnl),
                    });
                }
            }
            Request::Orders {
                family,
                page,
                attempt,
                cursor,
                last_cursor,
                page_rows,
                rows,
                mut sources,
                snapshot,
                reconcile,
                uncertainty_generation,
                read_epoch,
                failed: false,
            } => {
                if page_rows > 0 {
                    if page >= 49 || cursor == last_cursor {
                        self.account_failed(
                            cx,
                            Request::Orders {
                                family,
                                page,
                                attempt,
                                cursor,
                                last_cursor,
                                page_rows,
                                rows,
                                sources,
                                snapshot,
                                reconcile,
                                uncertainty_generation,
                                read_epoch,
                                failed: true,
                            },
                            "BloFin pending order pagination did not advance",
                        );
                        return;
                    }
                    self.orders_page(
                        cx,
                        Request::Orders {
                            family,
                            page: page + 1,
                            attempt,
                            cursor: last_cursor,
                            last_cursor: String::new(),
                            page_rows: 0,
                            rows,
                            sources,
                            snapshot,
                            reconcile,
                            uncertainty_generation,
                            read_epoch,
                            failed: false,
                        },
                    );
                } else if family < 2 {
                    self.orders_page(
                        cx,
                        Request::Orders {
                            family: family + 1,
                            page: 0,
                            attempt,
                            cursor: String::new(),
                            last_cursor: String::new(),
                            page_rows: 0,
                            rows,
                            sources,
                            snapshot,
                            reconcile,
                            uncertainty_generation,
                            read_epoch,
                            failed: false,
                        },
                    );
                } else {
                    self.account.orders_busy = false;
                    if !self.account.orders_seeded {
                        // Even when newer WS state rejects this store snapshot, its
                        // complete cumulative rows still prove execution prefixes.
                        self.seed_startup_executions(cx, rows.rows(), &sources);
                    }
                    let clean = rows.rejected() == 0;
                    let committed = cx.store.commit_orders_snapshot([snapshot], |current| {
                        if clean {
                            self.account.order_skips.clear();
                        }
                        let accepted = rows.rows().len();
                        let mut rows = rows.commit(current.iter().cloned(), |o| o.id.clone());
                        // Stored copies kept for rejected rows bypass the snapshot's own rules.
                        let mut kept = rows.split_off(accepted);
                        for order in current {
                            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                self.account.remember_terminal(&order.id);
                            }
                        }
                        rows.retain(|order| {
                            !self.account.terminal.contains(&order.id)
                                && (order
                                    .extra
                                    .as_ref()
                                    .and_then(|extra| extra.filled_in_placement_units)
                                    .unwrap_or(0.0)
                                    >= self.filled_native(&order.id)
                                    || current.iter().any(|existing| existing.id == order.id))
                        });
                        let retained: HashSet<_> =
                            rows.iter().map(|order| order.id.as_str()).collect();
                        sources.retain(|id, _| retained.contains(id.as_str()));
                        for order in &mut rows {
                            if let Some(existing) = current
                                .iter()
                                .find(|existing| existing.id == order.id)
                                .filter(|existing| existing.filled > order.filled)
                            {
                                *order = existing.clone();
                                if let Some(source) = self.account.sources.get(&order.id) {
                                    sources.insert(order.id.clone(), source.clone());
                                }
                            }
                        }
                        kept.retain(|order| !self.account.terminal.contains(&order.id));
                        // A kept attached TP/SL leg already stands for its parent's leg.
                        let kept_leg = |parent: &str, kind: OrderType| {
                            kept.iter().any(|leg| {
                                leg.kind == kind
                                    && self.account.sources.get(&leg.id).is_some_and(|s| {
                                        s.order_id == parent && !s.tpsl_id.is_empty()
                                    })
                            })
                        };
                        let mut extra = Vec::new();
                        for order in &rows {
                            let Some(source) = sources.get(&order.id) else {
                                continue;
                            };
                            if order.kind != OrderType::Limit
                                || !source.tpsl_id.is_empty()
                                || !source.algo_id.is_empty()
                            {
                                continue;
                            }
                            for (kind, price, suffix) in [
                                (OrderType::TakeProfit, source.tp, "take_profit"),
                                (OrderType::StopLoss, source.sl, "stop_loss"),
                            ] {
                                if price <= 0.0
                                    || rows.iter().any(|leg| {
                                        leg.kind == kind
                                            && sources.get(&leg.id).is_some_and(|s| {
                                                s.order_id == order.id && !s.tpsl_id.is_empty()
                                            })
                                    })
                                    || kept_leg(&order.id, kind)
                                {
                                    continue;
                                }
                                let id = format!("{}__{suffix}", order.id);
                                extra.push(Order {
                                    id: id.clone(),
                                    kind,
                                    price,
                                    side: order.side.opposite(),
                                    parent_id: Some(order.id.clone()),
                                    reduce_only: true,
                                    filled: 0.0,
                                    amount: if order.remaining > 0.0 {
                                        order.remaining
                                    } else {
                                        order.amount
                                    },
                                    remaining: if order.remaining > 0.0 {
                                        order.remaining
                                    } else {
                                        order.amount
                                    },
                                    extra: Some(OrderExtra {
                                        remaining_native: order
                                            .extra
                                            .as_ref()
                                            .and_then(|e| e.remaining_native),
                                        ..OrderExtra::default()
                                    }),
                                    ..order.clone()
                                });
                            }
                        }
                        extra.retain(|order| !self.account.terminal.contains(&order.id));
                        for order in &extra {
                            if let Some(source) = order
                                .parent_id
                                .as_ref()
                                .and_then(|id| sources.get(id))
                                .cloned()
                            {
                                sources.insert(order.id.clone(), source);
                            }
                        }
                        rows.extend(extra);
                        kept.retain(|order| !rows.iter().any(|row| row.id == order.id));
                        for order in &kept {
                            if let Some(source) = self.account.sources.get(&order.id) {
                                sources.insert(order.id.clone(), source.clone());
                            }
                        }
                        rows.extend(kept);
                        self.account.sources = sources;
                        rows
                    });
                    if committed {
                        // The last page's token cannot freshen a pre-mark first page.
                        if clean
                            && reconcile
                            && uncertainty_generation == self.account.uncertainty_generation
                        {
                            self.account.reconcile_owed = false;
                            cx.orders_reconciled_since(None, read_epoch);
                        } else if reconcile {
                            self.account.reconcile_owed = true;
                        }
                    } else {
                        self.account.reconcile_owed |= reconcile;
                        if self.account.retry.is_none() {
                            self.account.retry = Some((cx.set_timer(1_000.0), 1));
                        }
                    }
                    // A reconcile requested behind this snapshot runs now.
                    if !reconcile && self.account.reconcile_owed && self.account.retry.is_none() {
                        self.refresh_orders(cx, 1);
                    }
                }
            }
            other => self.account_failed(cx, other, "Malformed BloFin private response"),
        }
    }
    fn bootstrap_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.margin.is_some()
            && self.account.mode_loaded
            && !self.account.startup_started
        {
            self.account.startup_started = true;
            self.refresh_account(cx);
            self.refresh_orders(cx, 1);
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        match request {
            Request::Orders {
                attempt, reconcile, ..
            } => {
                self.account.orders_busy = false;
                self.account.reconcile_owed |= reconcile;
                if attempt < 3 && self.account.auth_failures == 0 {
                    self.account.retry =
                        Some((cx.set_timer(1000.0 * f64::from(attempt)), attempt + 1));
                }
            }
            Request::Positions { .. } | Request::Balance { .. } => {
                self.account.account_busy = false
            }
            Request::Margin { .. } => self.setting_unsettled(cx, StartupRead::Margin),
            Request::Mode { .. } => self.setting_unsettled(cx, StartupRead::Mode),
        }
        cx.retried_failure("blofin.account:1402", error);
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) {
        if text == "pong" {
            return;
        }
        if text == "ping" {
            cx.send(socket, "pong");
            return;
        }
        let Ok(message) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if !message.event.is_empty() || !self.account.authenticated {
            return;
        }
        match message.arg.channel.as_ref() {
            "orders" | "orders-algo" => {
                // Missing quantity is not an explicit zero proof; a present
                // quantity must retain OrderRow's rejection of malformed/null values.
                fn present_filled_size<'de, D: serde::Deserializer<'de>>(
                    d: D,
                ) -> Result<Option<Number>, D::Error> {
                    Number::deserialize(d).map(Some)
                }
                #[derive(Deserialize)]
                struct StreamOrder<'a> {
                    #[serde(flatten, borrow)]
                    row: OrderRow<'a>,
                    #[serde(rename = "updateTime", default)]
                    updated: Number,
                    #[serde(rename = "createTime", default)]
                    created: Number,
                    #[serde(
                        rename = "filledSize",
                        default,
                        deserialize_with = "present_filled_size"
                    )]
                    filled: Option<Number>,
                }
                if let Ok(Data { data: rows }) =
                    serde_json::from_str::<Data<Vec<&serde_json::value::RawValue>>>(text)
                {
                    cx.store.invalidate_order_snapshots();
                    for raw in rows {
                        let Ok(StreamOrder {
                            mut row,
                            updated,
                            created,
                            filled,
                        }) = serde_json::from_str::<StreamOrder<'_>>(raw.get())
                        else {
                            cx.notice(
                                Level::Error,
                                code::ROW_SKIPPED,
                                "Malformed BloFin order row; retaining known order",
                            );
                            continue;
                        };
                        if let Some(qty) = filled {
                            row.filled_size = qty;
                        }
                        let orders = self.map_orders(cx, &row);
                        if filled.is_some_and(|qty| qty.0 == 0.0)
                            && let Some(order) = orders.first()
                            && matches!(order.kind, OrderType::Limit | OrderType::Market)
                            && !self.account.fills.contains_key(&order.id)
                        {
                            self.account.execution_progress(&order.id, 0.0, Some(0.0));
                        }
                        if matches!(row.state.as_ref(), "partially_filled" | "filled")
                            && !orders.is_empty()
                        {
                            self.execution(cx, &row, &orders[0], updated.0, created.0);
                        }
                        let source = Self::source(&row);
                        for order in orders {
                            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                self.account.remember_terminal(&order.id);
                                self.account.sources.remove(&order.id);
                                if !cx
                                    .store
                                    .order(&order.id)
                                    .is_some_and(|existing| existing.status == OrderStatus::Closed)
                                {
                                    cx.store.set_order_status(&order.id, order.status);
                                }
                                continue;
                            }
                            if self.account.terminal.contains(&order.id) {
                                continue;
                            }
                            if row.filled_size.0 < self.filled_native(&order.id)
                                || cx.store.order(&order.id).is_some_and(|existing| {
                                    matches!(
                                        existing.status,
                                        OrderStatus::Closed | OrderStatus::Canceled
                                    ) || order.filled < existing.filled
                                })
                            {
                                continue;
                            }
                            self.account
                                .sources
                                .insert(order.id.clone(), source.clone());
                            cx.store.upsert_order(order);
                        }
                    }
                    if let Some(timer) = self.account.debounce.take() {
                        cx.clear_timer(timer);
                    }
                    self.account.debounce = Some(cx.set_timer(2_000.0));
                }
            }
            "positions" => {
                if let Ok(Data { data: rows }) =
                    serde_json::from_str::<Data<Vec<&serde_json::value::RawValue>>>(text)
                {
                    let mut positions = Vec::new();
                    let mut malformed = HashSet::new();
                    let mut used = 0.0;
                    let mut upnl = 0.0;
                    for raw in rows {
                        match serde_json::from_str::<PositionRow<'_>>(raw.get()) {
                            Ok(row) => {
                                // Account totals cover every native position, including
                                // instruments outside the tradable market catalog. Sum
                                // raw PnL before any per-market display rounding.
                                used += row.maintenance_margin.0;
                                upnl += row.unrealized_pnl.0;
                                if let Some(position) = self.map_position(cx, &row) {
                                    positions.push(position);
                                }
                            }
                            Err(_) => {
                                cx.notice(Level::Error, code::ROW_SKIPPED, "Malformed BloFin position row; retaining known position and balance totals");
                                #[derive(serde::Deserialize)]
                                struct Identity<'a> {
                                    #[serde(borrow, rename = "instId")]
                                    inst_id: Str<'a>,
                                }
                                let Ok(identity) = serde_json::from_str::<Identity<'_>>(raw.get())
                                else {
                                    // With no identity, any missing known position might be
                                    // the malformed row. Preserve this authoritative snapshot.
                                    return;
                                };
                                let Some(market) = cx.store.market_by_id(&identity.inst_id) else {
                                    return;
                                };
                                malformed.insert(market.symbol.clone());
                            }
                        }
                    }
                    // Preserve the complete affected symbol, including both hedge legs.
                    positions.retain(|position| !malformed.contains(&position.symbol));
                    positions.extend(
                        cx.store
                            .positions()
                            .iter()
                            .filter(|position| malformed.contains(&position.symbol))
                            .cloned(),
                    );
                    self.account.positions_revision += 1;
                    self.account.balance_revision += 1;
                    self.commit_positions(cx, positions);
                    if malformed.is_empty() {
                        let mut balance = cx.store.balance().clone();
                        balance.used = round_usd(used);
                        balance.upnl = round_usd(upnl);
                        cx.store.set_balance(balance);
                    }
                }
            }
            "account" => {
                if let Ok(Data { data: row }) = serde_json::from_str::<Data<AccountRow>>(text) {
                    self.account.balance_revision += 1;
                    let mut balance = cx.store.balance().clone();
                    let total = round_usd(row.total_equity.0);
                    balance.free = clean(total - balance.used);
                    balance.total = clean(total - balance.upnl);
                    cx.store.set_balance(balance);
                }
            }
            _ => {}
        }
    }
    fn execution(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        row: &OrderRow<'_>,
        order: &Order,
        updated: f64,
        created: f64,
    ) {
        let id = if row.order_id.is_empty() {
            &row.ord_id
        } else {
            &row.order_id
        };
        let qty = row.filled_size.0;
        let value = qty * row.average_price.0;
        if qty > 0.0 && (row.average_price.0 <= 0.0 || !value.is_finite() || value <= 0.0) {
            if updated > 0.0 && updated < self.account.session_started {
                self.account.historical_execution_floor(id, qty);
            } else {
                cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {id}: invalid cumulative quote value; retained proven execution baseline"));
            }
            return;
        }
        let contract = cx
            .store
            .market(&order.symbol)
            .and_then(|m| m.contract_size)
            .unwrap_or(1.0);
        let execution = Execution {
            fill: Fill {
                id: None,
                order_id: id.to_string(),
                symbol: order.symbol.clone(),
                side: order.side,
                price: row.average_price.0,
                amount: clean(qty * contract),
            },
            qty,
            value,
            contract,
            updated,
        };
        let session_order = created > 0.0
            && created >= self.account.session_started
            && created <= cx.now
            && (updated == 0.0 || created <= updated);
        let known_prefix = self.account.fills.contains_key(id.as_ref());
        if !session_order
            && (!self.account.orders_seeded || !known_prefix)
            && !(updated > 0.0 && updated < self.account.session_started)
        {
            if self.account.orders_seeded {
                // A delayed terminal row absent from the startup open-order census
                // cannot distinguish old fills from its unobserved startup suffix.
                self.seed_execution_prefix(cx, id, qty, Some(execution.value));
                cx.notice(if updated == 0.0 || updated >= self.account.reporting_started {
                        Level::Error
                    } else {
                        Level::Warn
                    }, code::VALUATION_UNAVAILABLE,
                    format!("BloFin execution prefix unavailable for existing order {id}; cannot distinguish historical fills from live executions; refreshed cumulative baseline"),
                );
            } else {
                let prefix = self
                    .account
                    .startup_executions
                    .iter()
                    .filter(|queued| queued.fill.order_id == id.as_ref())
                    .fold(
                        self.account.fills.get(id.as_ref()).copied(),
                        |prefix, queued| {
                            if matches!(
                                execution_prefix(prefix, queued.qty, Some(queued.value)),
                                Ok(true)
                            ) {
                                Some((queued.qty, Some(queued.value)))
                            } else {
                                prefix
                            }
                        },
                    );
                if let Err(error) = execution_prefix(prefix, qty, Some(execution.value)) {
                    cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {id}: {error}; retained proven execution baseline"));
                    return;
                }
                if self.account.startup_executions.len() == 500
                    && let Some(old) = self.account.startup_executions.pop_front()
                {
                    // Bound startup memory; an evicted observation becomes a quiet
                    // baseline, so later cumulative rows cannot replay its prefix.
                    let previous = self
                        .account
                        .fills
                        .get(&old.fill.order_id)
                        .map_or(0.0, |p| p.0);
                    if old.qty > previous {
                        self.seed_execution_prefix(
                            cx,
                            &old.fill.order_id,
                            old.qty,
                            Some(old.value),
                        );
                    }
                }
                self.account.startup_executions.push_back(execution);
            }
            return;
        }
        self.emit_execution(cx, execution);
    }
    fn seed_startup_executions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &[Order],
        sources: &HashMap<String, Source>,
    ) {
        for order in rows {
            if matches!(order.kind, OrderType::Limit | OrderType::Market)
                && let Some(source) = sources.get(&order.id)
                && !self.account.fills.contains_key(&order.id)
            {
                let (qty, value) = source.execution_baseline;
                self.seed_execution_prefix(cx, &order.id, qty, value);
            }
        }
        let pending = std::mem::take(&mut self.account.startup_executions);
        let mut unknown = HashMap::<String, (f64, f64)>::new();
        for execution in &pending {
            if !self.account.fills.contains_key(&execution.fill.order_id) {
                let previous = unknown
                    .get(&execution.fill.order_id)
                    .map(|&(qty, value)| (qty, Some(value)));
                match execution_prefix(previous, execution.qty, Some(execution.value)) {
                    Ok(true) => { unknown.insert(execution.fill.order_id.clone(), (execution.qty, execution.value)); }
                    Ok(false) => {}
                    Err(error) => cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {}: {error}; retained proven execution baseline", execution.fill.order_id)),
                }
            }
        }
        // An old order already terminal before the first census has no proven
        // historical prefix. Start reporting after its last startup observation.
        for (id, (qty, value)) in unknown {
            self.seed_execution_prefix(cx, &id, qty, Some(value));
        }
        self.account.orders_seeded = true;
        self.account.reporting_started = cx.now;
        for execution in pending {
            if !self.account.fills.contains_key(&execution.fill.order_id) {
                // The bounded tracker may have evicted a queued order while
                // seeding a large census. Never turn that eviction into a zero prefix.
                if let Some(source) = sources.get(&execution.fill.order_id) {
                    let (qty, value) = source.execution_baseline;
                    self.seed_execution_prefix(cx, &execution.fill.order_id, qty, value);
                } else {
                    self.seed_execution_prefix(
                        cx,
                        &execution.fill.order_id,
                        execution.qty,
                        Some(execution.value),
                    );
                    continue;
                }
            }
            self.emit_execution(cx, execution);
        }
    }
    fn emit_execution(&mut self, cx: &mut Cx<'_, Pending>, execution: Execution) {
        let Execution {
            mut fill,
            qty,
            value,
            contract,
            updated,
        } = execution;
        let previous = self
            .account
            .fills
            .get(&fill.order_id)
            .copied()
            .unwrap_or((0.0, Some(0.0)));
        if qty <= previous.0 {
            if qty == previous.0 && previous.1.is_none() {
                self.account
                    .execution_progress(&fill.order_id, qty, Some(value));
            }
            return;
        }
        let historical = updated > 0.0 && updated < self.account.session_started;
        // Missing historical quote value cannot prove the suffix price. This
        // observation supplies the quote baseline for subsequent live executions.
        let Some(previous_value) = previous.1 else {
            self.account
                .execution_progress(&fill.order_id, qty, Some(value));
            if !historical {
                cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {}: missing cumulative quote baseline; cannot recover execution price; refreshed execution baseline", fill.order_id));
            }
            return;
        };
        if let Err(error) =
            execution_prefix(Some((previous.0, Some(previous_value))), qty, Some(value))
        {
            if historical {
                self.account.historical_execution_floor(&fill.order_id, qty);
            } else {
                cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {}: {error}; retained proven execution baseline", fill.order_id));
            }
            return;
        }
        if historical {
            self.account
                .execution_progress(&fill.order_id, qty, Some(value));
            return;
        }
        let prepared = Cursor::seed(previous.0, Some(previous_value))
            .and_then(|cursor| {
                cursor.prepare(
                    Report::Cumulative {
                        quantity: qty,
                        price: CumulativePrice::Value {
                            cumulative: Some(value),
                            fallback: None,
                        },
                    },
                    Arithmetic::DecimalCapped20,
                )
            })
            .and_then(|prepared| {
                prepared
                    .map(|prepared| {
                        prepared.scaled(contract, ExecutionRounding::ScaledSignificant15)
                    })
                    .transpose()
            });
        let Ok(Some(prepared)) = prepared else {
            cx.notice(Level::Error, code::VALUATION_UNAVAILABLE, format!("BloFin execution price unavailable for order {}: invalid incremental execution output; retained proven execution baseline", fill.order_id));
            return;
        };
        let next = prepared.next();
        self.account
            .execution_progress(&fill.order_id, next.quantity(), next.value());
        fill.price = prepared.price();
        fill.amount = prepared.amount();
        cx.fill(fill);
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(read) = self.account.startup.on_timer(timer) {
            self.read_setting(cx, read);
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            if self.account.margin.is_some() && self.account.mode_loaded {
                self.refresh_account(cx);
                self.refresh_orders(cx, 1);
            }
            let not_before_ms = if self.account.auth_failures > 0 {
                // Only auth-failure retries spread; healthy ticks keep their cadence.
                cx.now
                    + cx.jittered(
                        Backoff::new(10_000.0, 3, 60_000.0).delay(self.account.auth_failures),
                    )
            } else {
                f64::NEG_INFINITY
            };
            self.account.tick = Some(cx.set_poll_timer_with_bounds(
                10_000.0,
                safe_cex_engine::PollTimerBounds {
                    not_before_ms,
                    ..Default::default()
                },
            ));
            return true;
        }
        if self.account.debounce == Some(timer) {
            self.account.debounce = None;
            self.refresh_orders(cx, 1);
            return true;
        }
        if self.account.retry.is_some_and(|(t, _)| t == timer) {
            let (_, attempt) = self.account.retry.take().unwrap();
            self.refresh_orders(cx, attempt);
            return true;
        }
        false
    }
}
pub(crate) fn clean(n: f64) -> f64 {
    num::significant15_scaled(n)
}
fn execution_prefix(
    previous: Option<(f64, Option<f64>)>,
    qty: f64,
    value: Option<f64>,
) -> Result<bool, &'static str> {
    let previous = previous
        .map(|(quantity, value)| Cursor::seed(quantity, value))
        .transpose()
        .map_err(|_| "invalid cumulative quote value")?;
    Cursor::prepare_prefix(previous, qty, value, Arithmetic::DecimalCapped20)
        .map(|next| next.is_some())
        .map_err(|error| match error {
            Rejection::InvalidQuantity | Rejection::InvalidValue => {
                "invalid cumulative quote value"
            }
            Rejection::Unpriced => "missing cumulative quote value",
            Rejection::ValueRegressed
            | Rejection::QuantityRegressed
            | Rejection::NonFiniteArithmetic => "nonpositive cumulative quote interval",
        })
}

/// BloFin key refusals no retry can fix (docs "Error Codes", 1524xx); 152405,
/// 152407, 152410 (timestamp/nonce) and every other code stay transient.
fn key_rejection(code: &str) -> Option<ErrorKind> {
    match code {
        "152401" | "152408" | "152409" => Some(ErrorKind::AuthInvalid),
        "152402" => Some(ErrorKind::AuthExpired),
        "152406" => Some(ErrorKind::AuthPermission),
        _ => None,
    }
}

fn rejection(which: &str, code: &str, msg: &str, kind: ErrorKind) -> VenueError {
    let mut error = VenueError::new(format!("BloFin rejected the {which} ({code}): {msg}"))
        .kind(kind)
        .venue_code(code);
    if !msg.is_empty() {
        error = error.venue_message(msg);
    }
    error
}
