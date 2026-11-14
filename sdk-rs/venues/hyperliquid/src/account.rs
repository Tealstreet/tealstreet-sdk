//! Account scopes reconcile independently; a failed read never becomes empty state.
use crate::{Hyperliquid, Pending, wire::*};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotCoverage;
use safe_cex_engine::fresh_positions;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Cx, ErrorKind, Fill, FillFinancials, Method, Response, TimerId, VenueError, json,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// How soon `readPositions` tries again after a read a newer stream frame held.
const UNREADABLE_ORDER: &str = "Hyperliquid returned an open order this account cannot read, so a fresh read cannot clear this. Cancel that order on Hyperliquid to resume";
const FRESH_POSITIONS_RETRY_MS: f64 = 500.0;
// Four engines expose at most twelve ordinary 20-weight reads: 14.4 s of
// refill, leaving margin inside an issued read's unchanged 20 s deadline.
const MAX_BACKGROUND_READS: u8 = 3;

/// What a state read did to the store, for the `readPositions` waiting on it.
enum StateApplied {
    /// The symbols of owned rows on coins hidden by design.
    Committed(Vec<String>),
    /// A newer stream frame overrode this read.
    Held,
    Unusable(&'static str),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BootstrapRead {
    State,
    Spot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivateReadKind {
    State,
    Spot,
    Orders,
    Role,
    Mode,
    Discovery,
}
impl PrivateReadKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::State => "state",
            Self::Spot => "spot",
            Self::Orders => "orders",
            Self::Role => "role",
            Self::Mode => "mode",
            Self::Discovery => "discovery",
        }
    }
}

pub enum Request {
    Role,
    Mode,
    State {
        dex: String,
        revision: u64,
        settings_revision: u64,
        read: u32,
    },
    Orders {
        epoch: u64,
        dex: String,
        revision: u64,
        orders: Vec<Order>,
        seen: HashSet<String>,
        valid: bool,
        coverage: SnapshotCoverage<String>,
    },
    Spot {
        revision: u64,
    },
    Asset {
        symbol: String,
        revision: u64,
    },
    Builder {
        startup: bool,
    },
    Fees,
    AmendSource {
        id: String,
        amend: crate::trading::AmendRead,
    },
    /// The frame-fed page never ran the startup role read; its amend reads need the account.
    AmendRole {
        id: String,
        amend: crate::trading::AmendRead,
    },
}
impl Request {
    /// Names the read in its `request.failed` notice, so a report shows which read failed.
    pub(crate) fn read_kind(&self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Mode => "mode",
            Self::State { .. } => "state",
            Self::Orders { .. } => "orders",
            Self::Spot { .. } => "spot",
            Self::Asset { .. } => "asset",
            Self::Builder { .. } => "builder",
            Self::Fees => "fees",
            Self::AmendSource { .. } => "amendSource",
            Self::AmendRole { .. } => "amendRole",
        }
    }
    pub(crate) fn amend_read(&self) -> Option<(&str, crate::trading::AmendRead)> {
        match self {
            Self::AmendRole { id, amend } | Self::AmendSource { id, amend } => Some((id, *amend)),
            _ => None,
        }
    }
    pub(crate) fn is_builder(&self) -> bool {
        matches!(self, Self::Builder { .. })
    }
}
enum ReadContinuation {
    Account(Request),
    History(Box<crate::history::Request>),
    Candles(Box<crate::ManagedCandleRead>),
}
impl ReadContinuation {
    fn pending(self) -> Pending {
        match self {
            Self::Account(r) => Pending::Account(r),
            Self::History(r) => Pending::History(r),
            Self::Candles(r) => Pending::ManagedCandles(r),
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ReadPriority {
    Foreground,
    Core,
    Financial,
    Background,
    Bulk,
}
struct QueuedRead {
    body: String,
    continuation: ReadContinuation,
    streamed: bool,
    weight: u32,
    priority: ReadPriority,
}
impl QueuedRead {
    fn occupies_account_slot(&self) -> bool {
        matches!(&self.continuation, ReadContinuation::Account(_))
            && (self.priority != ReadPriority::Foreground
                || matches!(
                    &self.continuation,
                    ReadContinuation::Account(Request::Orders { .. })
                ))
    }
    fn deadline(&self) -> Option<f64> {
        match &self.continuation {
            ReadContinuation::History(read) => Some(read.root().deadline()),
            ReadContinuation::Candles(read) => Some(read.context.deadline()),
            _ => None,
        }
    }
    fn capacity(&self) -> u32 {
        match self.priority {
            ReadPriority::Foreground | ReadPriority::Core | ReadPriority::Financial => 1000,
            _ => 800,
        }
    }
    fn read_class(&self) -> safe_cex_engine::HttpReadClass {
        match self.priority {
            ReadPriority::Foreground => safe_cex_engine::HttpReadClass::Critical,
            ReadPriority::Core | ReadPriority::Financial => {
                safe_cex_engine::HttpReadClass::Reconciliation
            }
            ReadPriority::Bulk => safe_cex_engine::HttpReadClass::Bulk,
            _ => safe_cex_engine::HttpReadClass::Normal,
        }
    }
}
struct SettingBarrier {
    at: f64,
    revision: u64,
    confirmed: bool,
}

struct PendingSpotCollateral {
    balances: Vec<(String, f64, f64)>,
    available: Option<f64>,
    ws: bool,
}

/// Account lookups at start: tries before the failure is shown, and the wait before each retry.
const LOOKUP_TRIES: u32 = 4;
fn backoff_ms(tries: u32) -> f64 {
    2_000.0 * f64::from(1u32 << tries.min(4).saturating_sub(1))
}

#[derive(Default)]
pub struct Account {
    pub started: bool,
    /// The symbol a caller last read leverage for: the page's selection.
    selected: Option<String>,
    /// Symbols whose activeAssetData streams: the selection and open orders.
    asset_scope: std::collections::BTreeSet<String>,
    mode: Mode,
    mode_resolved: bool,
    pub(crate) mode_refresh_due: bool,
    // Managed read completion/deadline owns retries, including host quota waits.
    mode_pending: bool,
    role_pending: bool,
    // A failed lookup is retried with backoff; its timer and the tries so far.
    role_retry: Option<TimerId>,
    role_attempts: u32,
    mode_retry: Option<TimerId>,
    mode_attempts: u32,
    sync_timer: Option<TimerId>,
    last_order_refresh: f64,
    read_timer: Option<(TimerId, f64)>,
    read_window: f64,
    read_weight: u32,
    reads: VecDeque<QueuedRead>,
    background_reads: u8,
    pub refresh_timer: Option<TimerId>,
    spot_at: f64,
    spot_forced_at: Option<f64>,
    spot_inflight: bool,
    spot_revision: u64,
    spot_authoritative_ws: bool,
    core_value: f64,
    core_loaded: bool,
    core_used: f64,
    spot: Option<(f64, f64)>,
    pending_spot: Option<PendingSpotCollateral>,
    authoritative: Option<(f64, f64, f64)>,
    state_revision: HashMap<String, u64>,
    /// Numbers each dex's state reads, so `readPositions` can name one it may trust.
    state_reads: HashMap<String, u32>,
    pub(crate) fresh_positions: safe_cex_engine::fresh_positions::FreshPositions,
    fresh_positions_timer: Option<TimerId>,
    /// The typed cause of the read failure being delivered, for its waiters.
    pub(crate) read_error: Option<VenueError>,
    settings_revision: u64,
    settings_barriers: HashMap<String, SettingBarrier>,
    asset_retry: HashSet<String>,
    asset_retry_timer: Option<TimerId>,
    /// The next queued read is a fence reconcile.
    fence_read: bool,
    orders_retry: safe_cex_engine::startup::Backoff<String>,
    bootstrap_retry: safe_cex_engine::startup::Backoff<BootstrapRead>,
    pub(crate) last_failed_private_read_kind: Option<PrivateReadKind>,
    pub(crate) builder_read_at: f64,
    state_time: HashMap<String, f64>,
    position_time: HashMap<String, f64>,
    snapshot_time: HashMap<String, f64>,
    last_fill: HashMap<String, (f64, u64)>,
    state_dirty: HashSet<String>,
    state_refresh_timer: Option<TimerId>,
    pub orders_revision: HashMap<String, u64>,
    inflight: HashSet<String>,
    orders_refresh_due: HashSet<String>,
    pub suppressed: HashMap<String, f64>,
    /// The read epoch of each dex's last clean order census, "" the main one.
    pub clean_dexes: HashMap<String, u64>,
    seen_fills: HashSet<u64>,
    fill_queue: VecDeque<u64>,
    spot_filled: HashMap<String, (String, f64)>,
    spot_fill_orders: VecDeque<String>,
    spot_fill_tids: HashSet<u64>,
    /// Merged spot holdings (coin, total, hold) from the last spot state.
    spot_holdings: Option<Vec<(String, f64, f64)>>,
    published_coins: Option<Vec<CoinBalance>>,
    spot_fill_queue: VecDeque<u64>,
    notify_fills: HashSet<u64>,
    notify_queue: VecDeque<u64>,
    realized: HashMap<(String, PositionSide), f64>,
    pub records: HashMap<String, Placement>,
    pub preplacements: VecDeque<Placement>,
    pub pending: HashMap<String, Placement>,
    pub builder_approved: Option<bool>,
    valuation_warned: bool,
    /// `position:coin` / `order:coin` keys already warned as on an unknown market.
    unknown_warned: HashSet<String>,
}
#[derive(Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Standard,
    Unified,
    Portfolio,
}
#[derive(Clone)]
pub struct Placement {
    pub order: Order,
    pub at: f64,
}
impl Hyperliquid {
    pub(crate) fn apply_spot_ack_fills(&self, order: &mut Order) {
        if let Some((symbol, filled)) = self.account.spot_filled.get(&order.id)
            && *symbol == order.symbol
        {
            order.filled = order.amount.min(*filled);
            order.remaining = (order.amount - order.filled).max(0.0);
        }
    }
    fn record_spot_fill(&mut self, fill: &FillRow<'_>, symbol: &str) {
        if !self.account.spot_fill_tids.insert(fill.tid) {
            return;
        }
        self.account.spot_fill_queue.push_back(fill.tid);
        if self.account.spot_fill_queue.len() > 20_000
            && let Some(tid) = self.account.spot_fill_queue.pop_front()
        {
            self.account.spot_fill_tids.remove(&tid);
        }
        let id = fill.oid.to_string();
        if !self.account.spot_filled.contains_key(&id) {
            self.account.spot_fill_orders.push_back(id.clone());
        }
        let entry = self
            .account
            .spot_filled
            .entry(id)
            .or_insert_with(|| (symbol.to_owned(), 0.0));
        if entry.0 == symbol {
            entry.1 += fill.sz;
        }
        if self.account.spot_fill_orders.len() > 1024
            && let Some(id) = self.account.spot_fill_orders.pop_front()
        {
            self.account.spot_filled.remove(&id);
        }
    }
    fn apply_role(&mut self, raw: &str) {
        if let Ok(role) = serde_json::from_str::<Role>(raw) {
            match role.role.as_ref() {
                "user" => self.options.application_id = self.options.key.clone(),
                "agent" => self.options.application_id = role.data.user.map(|v| v.into_owned()),
                "subAccount" => {
                    self.options.application_id = role.data.master.map(|v| v.into_owned())
                }
                "vault" => self.options.vault = self.options.key.clone(),
                _ => {}
            }
        }
    }
    pub(crate) fn user(&self) -> Option<&str> {
        self.options
            .vault
            .as_deref()
            .filter(|identity| !identity.is_empty())
            .or(self
                .options
                .application_id
                .as_deref()
                .filter(|identity| !identity.is_empty()))
    }
    pub(super) fn account_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: &str,
        dex: Option<&str>,
        request: Request,
        streamed: bool,
    ) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'a str,
            user: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            dex: Option<&'a str>,
        }
        let body = serde_json::to_string(&Read {
            r#type: kind,
            user: self.user().unwrap_or_default(),
            dex,
        })
        .unwrap();
        self.queue_read(cx, body, request, streamed);
    }
    fn read_body(&mut self, cx: &mut Cx<'_, Pending>, body: String, request: Request) {
        self.queue_read(cx, body, request, false);
    }
    fn queue_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        account_request: Request,
        streamed: bool,
    ) {
        // Frame-fed: account state arrives as frames; only an unknown builder
        // approval is read, once, before the first placement.
        if cx.frame_fed() {
            match &account_request {
                Request::Builder { .. }
                | Request::AmendSource { .. }
                | Request::AmendRole { .. } => {}
                _ => return,
            }
        }
        let weight = match &account_request {
            Request::Role | Request::AmendRole { .. } => 60,
            Request::State { .. } | Request::Spot { .. } | Request::AmendSource { .. } => 2,
            _ => 20,
        };
        let urgent = std::mem::take(&mut self.account.fence_read)
            || matches!(
                &account_request,
                Request::AmendSource { .. } | Request::AmendRole { .. } | Request::Asset { .. }
            );
        let financial = matches!(
            &account_request,
            Request::State { .. } | Request::Orders { .. }
        );
        let core = match &account_request {
            Request::Role | Request::Mode | Request::Spot { .. } => true,
            Request::State { dex, .. } | Request::Orders { dex, .. } => dex.is_empty(),
            _ => false,
        };
        let read = QueuedRead {
            body,
            continuation: ReadContinuation::Account(account_request),
            streamed,
            weight,
            priority: if urgent {
                ReadPriority::Foreground
            } else if core {
                ReadPriority::Core
            } else if financial {
                ReadPriority::Financial
            } else {
                ReadPriority::Background
            },
        };
        // Rank selects the class; insertion order preserves FIFO within it.
        self.account.reads.push_back(read);
        self.pump_reads(cx);
    }
    pub(crate) fn history_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        request: crate::history::Request,
    ) {
        let streamed = request.streamed();
        let weight = if streamed { 120 } else { 2 };
        self.account.reads.push_back(QueuedRead {
            body,
            continuation: ReadContinuation::History(Box::new(request)),
            streamed,
            weight,
            priority: ReadPriority::Bulk,
        });
        self.pump_reads(cx);
    }
    pub(crate) fn queue_candles(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        body: String,
        read: Box<crate::ManagedCandleRead>,
    ) {
        self.account.reads.push_back(QueuedRead {
            body,
            continuation: ReadContinuation::Candles(read),
            streamed: true,
            weight: 104,
            priority: ReadPriority::Bulk,
        });
        self.pump_reads(cx);
    }
    fn refill_read_weight(&mut self, now: f64) {
        if now.is_finite() && now >= self.account.read_window {
            let refill = ((now - self.account.read_window) / 60.)
                .floor()
                .min(f64::from(u32::MAX)) as u32;
            self.account.read_weight = self.account.read_weight.saturating_sub(refill);
            self.account.read_window = if self.account.read_weight == 0 {
                now
            } else {
                self.account.read_window + f64::from(refill) * 60.
            };
        }
    }
    pub(crate) fn history_weight(&mut self, cx: &mut Cx<'_, Pending>, rows: u32, reserved: bool) {
        self.refill_read_weight(cx.now);
        if reserved {
            self.account.read_weight = self.account.read_weight.saturating_sub(100);
        }
        self.account.read_weight = self.account.read_weight.saturating_add(rows.div_ceil(20));
        self.pump_reads(cx);
    }
    fn pump_reads(&mut self, cx: &mut Cx<'_, Pending>) {
        self.refill_read_weight(cx.now);
        while let Some(index) = self
            .account
            .reads
            .iter()
            .position(|read| read.deadline().is_some_and(|deadline| cx.now >= deadline))
        {
            let read = self.account.reads.remove(index).unwrap();
            let error = safe_cex_engine::request_context::Error::TimedOut.read_venue_error();
            match read.continuation {
                ReadContinuation::History(history) => self.reject_history(cx, *history, error),
                ReadContinuation::Candles(read) => {
                    let _ = cx.fail_context(&read.context, error);
                }
                _ => unreachable!("only caller reads have queued deadlines"),
            }
        }
        while let Some(index) = self
            .account
            .reads
            .iter()
            .enumerate()
            .filter(|(_, read)| {
                let ancillary = matches!(
                    &read.continuation,
                    ReadContinuation::Account(Request::Fees | Request::Builder { startup: true })
                );
                // Startup-only work follows the financial census before its 20 s
                // deadline starts. A live placement may promote its queued builder read.
                (!ancillary
                    || (self.account.inflight.is_empty()
                        && !self.account.mode_pending
                        && !self.account.role_pending))
                    && (self.account.background_reads < MAX_BACKGROUND_READS
                        || !read.occupies_account_slot())
            })
            .min_by_key(|(_, read)| read.priority)
            .map(|(index, _)| index)
        {
            let read = &self.account.reads[index];
            if self.account.read_weight.saturating_add(read.weight) > read.capacity() {
                break;
            }
            let read = self.account.reads.remove(index).unwrap();
            let read = match read {
                QueuedRead {
                    body,
                    continuation: ReadContinuation::Candles(read),
                    weight,
                    ..
                } => {
                    let context = read.context.clone();
                    match cx.request_managed_with_headers(
                        context.clone(),
                        None,
                        Method::Post,
                        format!("{}/info", self.rest()),
                        &[("Content-Type", "application/json")],
                        Some(body),
                        Pending::ManagedCandles(read),
                    ) {
                        Ok(id) => {
                            self.account.read_weight += weight;
                            cx.stream_response(id, ArrayAt::Root);
                        }
                        Err((_, error)) => {
                            let _ = cx.fail_context(&context, error.read_venue_error());
                        }
                    }
                    continue;
                }
                QueuedRead {
                    body,
                    continuation: ReadContinuation::History(mut history),
                    streamed,
                    weight,
                    ..
                } => {
                    let leg = match cx.http_leg_context(
                        history.root(),
                        safe_cex_engine::request_context::Policy {
                            operation: safe_cex_engine::request_context::Operation::Read,
                            credential: None,
                            timeout_ms: 20_000,
                            pacing: None,
                            retry: safe_cex_engine::request_context::RetryPolicy::Never,
                        },
                    ) {
                        Ok(leg) => leg,
                        Err(error) => {
                            self.reject_history(cx, *history, error.read_venue_error());
                            continue;
                        }
                    };
                    history.leg = Some(leg.clone());
                    match cx.request_managed_with_headers(
                        leg,
                        None,
                        Method::Post,
                        format!("{}/info", self.rest()),
                        &[("Content-Type", "application/json")],
                        Some(body),
                        Pending::History(history),
                    ) {
                        Ok(id) => {
                            self.account.read_weight += weight;
                            if streamed {
                                cx.stream_response(id, ArrayAt::Root);
                            }
                        }
                        Err((Pending::History(history), error)) => {
                            self.reject_history(cx, *history, error.read_venue_error())
                        }
                        Err(_) => unreachable!("history submission returns its own pending"),
                    }
                    continue;
                }
                read => read,
            };
            if let ReadContinuation::Account(request) = &read.continuation
                && let Some((id, amend)) = request.amend_read()
                && !self.amend_read_pending(cx, id, amend)
            {
                continue;
            }
            let background = read.occupies_account_slot();
            let read_class = read.read_class();
            let QueuedRead {
                body,
                continuation,
                streamed,
                weight,
                ..
            } = read;
            self.account.read_weight += weight;
            if background {
                self.account.background_reads += 1;
            }
            // Only issued reads acquire a managed deadline; queued intents own no grant.
            self.info_read(
                cx,
                body,
                continuation.pending(),
                streamed.then_some(ArrayAt::Root),
                background,
                read_class,
            );
        }
        if !self.account.reads.is_empty() {
            let refill_at = self
                .account
                .reads
                .iter()
                .filter(|read| {
                    self.account.read_weight.saturating_add(read.weight) > read.capacity()
                })
                .map(|read| {
                    self.account.read_window
                        + f64::from(
                            self.account.read_weight.saturating_add(read.weight) - read.capacity(),
                        ) * 60.
                })
                .fold(cx.now + 60_000., f64::min);
            let at = self
                .account
                .reads
                .iter()
                .filter_map(QueuedRead::deadline)
                .fold(refill_at, f64::min)
                .max(cx.now + 1.);
            if self
                .account
                .read_timer
                .is_none_or(|(_, existing)| at < existing)
            {
                if let Some((timer, _)) = self.account.read_timer.take() {
                    cx.clear_timer(timer);
                }
                self.account.read_timer = Some((cx.set_timer(at - cx.now), at));
            }
        }
    }
    pub(crate) fn finish_background_read(&mut self, cx: &mut Cx<'_, Pending>, background: bool) {
        if background {
            debug_assert!(self.account.background_reads > 0);
            self.account.background_reads -= 1;
            self.pump_reads(cx);
        }
    }
    pub(crate) fn discard_history_owner(&mut self, owner: safe_cex_engine::request_context::Owner) {
        self.account.reads.retain(|read| match read {
            QueuedRead {
                continuation: ReadContinuation::History(history),
                ..
            } => history.root().owner() != owner,
            QueuedRead {
                continuation: ReadContinuation::Candles(read),
                ..
            } => read.context.owner() != owner,
            _ => true,
        });
    }
    /// TS synthesises no position (and so no leverage row) for a merged spot pair.
    pub(crate) fn has_setting(&self, m: &Market) -> bool {
        self.spot || m.is_spot != Some(true)
    }
    pub(crate) fn seed_setting(&self, m: &Market) -> MarketSetting {
        // HIP-3 assets get no activeAssetData, so this seed is what writes start from;
        // TS writes every HIP-3 asset isolated.
        let isolated = self.isolated_only.contains(&m.symbol) || m.id.contains(':');
        MarketSetting {
            leverage: if self.spot || m.is_spot == Some(true) {
                1.0
            } else {
                20f64.min(m.limits.leverage.max)
            },
            buy_leverage: None,
            sell_leverage: None,
            margin_type: Some(if isolated {
                MarginType::Isolated
            } else {
                MarginType::Cross
            }),
        }
    }
    pub(crate) fn startup_private_stage(&self) -> &'static str {
        if self.spot {
            "spotClearinghouseState"
        } else if !self.account.mode_resolved {
            "userAbstraction"
        } else if self.account.core_loaded
            && self.account.mode != Mode::Standard
            && self.account.spot.is_none()
        {
            "spotClearinghouseState"
        } else {
            "clearinghouseState"
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if !self.options.public_data_only
            && let Some(actual) = self.sealed_agent()
        {
            // Only the API wallet field names the signer. The trading account is
            // another wallet by design, and a Hosted server holds no such field.
            if let Some(expected) = self.options.key.as_deref()
                && !actual.eq_ignore_ascii_case(expected)
            {
                cx.notice(
                    Level::Warn,
                    code::CONFIG_INVALID,
                    "Hyperliquid invalid credentials: private key does not match API wallet",
                );
            }
        }
        if cx.frame_fed() {
            // The data source owns the account rows, settings and polls.
            if self.spot {
                self.account.mode_resolved = true;
                self.account.builder_approved = Some(false);
            }
            return;
        }
        // Partial discovery may start real account reads, not an empty snapshot.
        // A keyed account's own reads load it: empty here would read as no open orders.
        // A key whose account is still being looked up has rows we have not read: not empty ones.
        let resolving =
            self.user().is_none() && self.options.key.is_some() && !self.options.public_data_only;
        if self.options.public_data_only || (self.user().is_none() && !resolving) {
            cx.store.set_orders(Vec::new());
        }
        if !resolving
            && (self.spot || !self.core_discovery_incomplete || self.options.public_data_only)
        {
            cx.store.set_positions(Vec::new());
        }
        if self.options.public_data_only {
            cx.store.set_balance(Balance::default());
        }
        cx.store.set_hedged(false);
        let settings = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .filter(|(_, m)| self.has_setting(m))
            .map(|(i, m)| (i as u32, self.seed_setting(m)))
            .collect::<Vec<_>>();
        cx.store.set_settings(settings);
        if self.spot {
            self.account.mode_resolved = true;
            self.account.builder_approved = Some(false);
            if !self.options.public_data_only && self.user().is_some() {
                self.refresh_spot(cx, true);
                self.refresh_orders_scope(cx, "");
                self.account.sync_timer = Some(cx.set_poll_timer_with_bounds(
                    10_000.0,
                    safe_cex_engine::PollTimerBounds {
                        minimum_interval_ms: 10_000.0,
                        ..Default::default()
                    },
                ));
            }
            return;
        }
        if self.options.public_data_only || (self.user().is_none() && self.options.key.is_none()) {
            // Public or unresolved discovery keeps the metadata fee defaults.
            // A real trading account must be known before querying its fee tier.
            return;
        }
        if self.user().is_none() {
            self.send_role(cx);
            return;
        }
        self.private_bootstrap(cx);
    }
    fn send_role(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.role_pending {
            return;
        }
        let Some(key) = self.options.key.as_deref() else {
            return;
        };
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
        }
        let body = serde_json::to_string(&Read {
            r#type: "userRole",
            user: key,
        })
        .unwrap();
        self.account.role_attempts += 1;
        self.account.role_pending = true;
        self.read_body(cx, body, Request::Role);
    }
    /// Retries with backoff; once spent, the user is told the rows are not shown.
    fn role_failed(&mut self, cx: &mut Cx<'_, Pending>, why: &str) {
        self.account.role_pending = false;
        if self.user().is_some() || self.account.role_retry.is_some() {
            return;
        }
        let tries = self.account.role_attempts;
        if tries < LOOKUP_TRIES {
            self.account.role_retry = Some(cx.set_timer(backoff_ms(tries)));
            return;
        }
        cx.notice(
            Level::Error,
            code::REQUEST_FAILED,
            format!(
                "Hyperliquid account identity could not be resolved after {tries} tries ({why}): positions and orders are not shown. Reconnect the account to try again."
            ),
        );
    }
    fn mode_failed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.mode_pending = false;
        if !self.account.mode_resolved {
            self.mode_ready(cx);
        }
        if self.account.mode_retry.is_none() && self.account.mode_attempts < LOOKUP_TRIES {
            self.account.mode_retry =
                Some(cx.set_timer(backoff_ms(self.account.mode_attempts.max(1))));
        }
    }
    fn send_mode(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.mode_pending {
            return;
        }
        self.account.mode_pending = true;
        self.account.mode_attempts += 1;
        self.account_read(cx, "userAbstraction", None, Request::Mode, false);
    }
    fn private_bootstrap(&mut self, cx: &mut Cx<'_, Pending>) {
        self.send_mode(cx);
        self.refresh_account(cx);
        self.account_read(cx, "userFees", None, Request::Fees, false);
        self.request_builder(cx, true);
        if !self.dex_names.is_empty() {
            self.account.sync_timer = Some(cx.set_poll_timer_with_bounds(
                120_000.0,
                safe_cex_engine::PollTimerBounds {
                    minimum_interval_ms: 120_000.0,
                    ..Default::default()
                },
            ));
        }
    }
    pub(crate) fn check_builder(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(read) = self.account.reads.iter_mut().find(|read| {
            matches!(
                &read.continuation,
                ReadContinuation::Account(Request::Builder { startup: true })
            )
        }) {
            if let ReadContinuation::Account(Request::Builder { startup }) = &mut read.continuation
            {
                *startup = false;
            }
            self.pump_reads(cx);
            return;
        }
        self.request_builder(cx, false);
    }
    fn request_builder(&mut self, cx: &mut Cx<'_, Pending>, startup: bool) {
        let Some(user) = self.options.application_id.as_deref() else {
            self.account.builder_approved = Some(false);
            return;
        };
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            builder: &'a str,
        }
        self.account.builder_read_at = cx.now;
        let b = self.builder();
        self.read_body(
            cx,
            serde_json::to_string(&Read {
                r#type: "maxBuilderFee",
                user,
                builder: &b.b,
            })
            .unwrap(),
            Request::Builder { startup },
        );
    }
    pub(crate) fn read_amend_source(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        id: &str,
        amend: crate::trading::AmendRead,
    ) {
        if !self.amend_read_pending(cx, id, amend) {
            return;
        }
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            oid: u64,
        }
        let Ok(oid) = id.parse::<u64>() else {
            self.amend_source_read(cx, id, amend, Err("the order ID is not numeric".to_owned()));
            return;
        };
        let Some(user) = self.user() else {
            let Some(key) = self.options.key.as_deref() else {
                self.amend_source_read(
                    cx,
                    id,
                    amend,
                    Err("the account is not resolved".to_owned()),
                );
                return;
            };
            #[derive(Serialize)]
            struct Role<'a> {
                r#type: &'static str,
                user: &'a str,
            }
            let body = serde_json::to_string(&Role {
                r#type: "userRole",
                user: key,
            })
            .unwrap();
            self.account.fence_read = true;
            self.read_body(
                cx,
                body,
                Request::AmendRole {
                    id: id.to_owned(),
                    amend,
                },
            );
            self.account.fence_read = false;
            return;
        };
        let body = serde_json::to_string(&Read {
            r#type: "orderStatus",
            user,
            oid,
        })
        .unwrap();
        self.account.fence_read = true;
        self.read_body(
            cx,
            body,
            Request::AmendSource {
                id: id.to_owned(),
                amend,
            },
        );
        self.account.fence_read = false;
    }
    pub(crate) fn builder(&self) -> crate::Builder {
        let mut builder = self
            .options
            .builder
            .clone()
            .unwrap_or_else(|| crate::Builder {
                b: if self.options.testnet {
                    "0x76676949EA8f83A3a461AdfefD9a8C35d8e41fba"
                } else {
                    "0xea629e1e8534B82f704A818643F2DAF38cB7Fd4d"
                }
                .into(),
                f: 10,
            });
        builder.b.make_ascii_lowercase();
        builder
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            self.refresh_spot(cx, true);
            self.refresh_orders_scope(cx, "");
            return;
        }
        let mut dexes = vec![String::new()];
        dexes.extend(self.dex_names.iter().cloned());
        for dex in dexes {
            self.refresh_account_scope(cx, &dex);
        }
    }
    pub(crate) fn refresh_account_scope(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        self.refresh_state(cx, dex);
        self.refresh_orders_scope(cx, dex);
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn recheck_account_startup(&mut self, cx: &mut Cx<'_, Pending>) -> bool {
        if self.options.public_data_only || !self.account.started || cx.frame_fed() {
            return false;
        }
        if self.user().is_none() {
            self.send_role(cx);
        } else {
            if !self.spot && !self.account.mode_resolved {
                self.send_mode(cx);
            }
            self.refresh_account(cx);
            self.refresh_spot_holdings(cx);
        }
        true
    }
    fn refresh_state(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if !self.account.inflight.insert(format!("state:{dex}")) {
            return;
        }
        let revision = *self.account.state_revision.get(dex).unwrap_or(&0);
        let read = self.account.state_reads.entry(dex.into()).or_default();
        *read += 1;
        let read = *read;
        self.account.fence_read |= self.account.fresh_positions.owed(self.dex_key(dex));
        self.account_read(
            cx,
            "clearinghouseState",
            (!dex.is_empty()).then_some(dex),
            Request::State {
                dex: dex.into(),
                revision,
                settings_revision: self.account.settings_revision,
                read,
            },
            false,
        );
    }
    pub(crate) fn refresh_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            self.refresh_spot(cx, true);
            self.refresh_orders_scope(cx, "");
            return;
        }
        let mut dexes = vec![String::new()];
        dexes.extend(self.dex_names.iter().cloned());
        for dex in dexes {
            self.refresh_orders_scope(cx, &dex);
        }
    }
    pub(crate) fn reconcile_order_scopes(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _symbols: Option<&[String]>,
    ) {
        // Core and every dex, whatever the symbols: the account cover joins
        // them all (H9), so a symbol's dex alone could never answer a refresh ask.
        let dexes: Vec<String> = std::iter::once(String::new())
            .chain(self.dex_names.iter().cloned())
            .collect();
        for dex in dexes {
            if self.account.inflight.contains(&format!("orders:{dex}")) {
                if !cx.reconcile_reask() {
                    self.account.orders_refresh_due.insert(dex.clone());
                    if let Some(read) = self.account.reads.iter_mut().find(|read| matches!(&read.continuation,
                        ReadContinuation::Account(Request::Orders { dex: queued, .. }) if queued == &dex)) {
                        // Promotion preserves the census epoch captured before the uncertain write.
                        read.priority = ReadPriority::Foreground;
                    }
                    self.pump_reads(cx);
                }
            } else {
                self.account.fence_read = true;
                self.refresh_orders_scope(cx, &dex);
                self.account.fence_read = false;
            }
        }
    }
    fn refresh_owed_orders(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if self.account.orders_refresh_due.remove(dex) {
            self.account.fence_read = true;
            self.refresh_orders_scope(cx, dex);
            self.account.fence_read = false;
        }
    }
    fn refresh_orders_scope(&mut self, cx: &mut Cx<'_, Pending>, dex: &str) {
        if self.user().is_none() || !self.account.inflight.insert(format!("orders:{dex}")) {
            return;
        }
        let revision = *self.account.orders_revision.get(dex).unwrap_or(&0);
        self.account_read(
            cx,
            "frontendOpenOrders",
            (!dex.is_empty()).then_some(dex),
            Request::Orders {
                epoch: cx.order_read_epoch(),
                dex: dex.into(),
                revision,
                orders: Vec::new(),
                seen: HashSet::new(),
                valid: true,
                coverage: SnapshotCoverage::default(),
            },
            true,
        );
    }
    pub(crate) fn schedule_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot {
            // Spot ignores orderUpdates, so open orders are only proven by this read.
            self.refresh_spot(cx, true);
            self.refresh_orders_scope(cx, "");
            return;
        }
        if self.account.refresh_timer.is_none() && self.user().is_some() {
            self.account.refresh_timer = Some(
                cx.set_timer(500.0f64.max(1000.0 - (cx.now - self.account.last_order_refresh))),
            );
        }
    }
    /// TS `shouldTrackSpotHoldings`: spot is collateral (unified/portfolio) or the
    /// account opted into spot markets.
    pub(crate) fn tracks_spot_holdings(&self) -> bool {
        self.account.mode != Mode::Standard || self.spot_markets
    }
    pub(crate) fn refresh_spot_holdings(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.tracks_spot_holdings() {
            self.refresh_spot(cx, true);
        }
    }
    fn refresh_spot(&mut self, cx: &mut Cx<'_, Pending>, force: bool) {
        if self.account.spot_inflight
            || (!force && cx.now - self.account.spot_at < 10_000.0)
            || (self.spot
                && force
                && self
                    .account
                    .spot_forced_at
                    .is_some_and(|at| cx.now - at < 1500.0))
        {
            return;
        }
        if force {
            self.account.spot_forced_at = Some(cx.now);
        }
        self.account.spot_inflight = true;
        self.account.spot_at = cx.now;
        self.account_read(
            cx,
            "spotClearinghouseState",
            None,
            Request::Spot {
                revision: self.account.spot_revision,
            },
            false,
        );
    }
    fn mode_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.mode_resolved = true;
        self.account.mode_refresh_due = true;
        self.schedule_socket_refresh(cx, true);
    }
    pub(crate) fn finish_mode_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.mode_refresh_due {
            return;
        }
        self.refresh_spot_holdings(cx);
        if self.tracks_spot_holdings() && !self.account.spot_inflight {
            return;
        }
        self.live_balance(cx);
        self.account.mode_refresh_due = false;
    }
    pub(crate) fn private_socket_interests(
        &self,
        store: &safe_cex_engine::Store,
    ) -> Vec<safe_cex_engine::socket_lifecycle::Interest> {
        let mut interests = Vec::new();
        if self.options.public_data_only || !self.account.mode_resolved {
            return interests;
        }
        let Some(user) = self.user() else {
            return interests;
        };
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Sub<'a> {
            r#type: &'a str,
            user: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            dex: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            coin: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            is_portfolio_margin: Option<bool>,
        }
        #[derive(Serialize)]
        struct Send<'a> {
            method: &'static str,
            subscription: Sub<'a>,
        }
        let markets = store.markets();
        let mut send = |kind, dex, coin, portfolio| {
            interests.push(safe_cex_engine::socket_lifecycle::Interest::from_wire_args(
                serde_json::to_string(&Send {
                    method: "subscribe",
                    subscription: Sub {
                        r#type: kind,
                        user,
                        dex,
                        coin,
                        is_portfolio_margin: portfolio,
                    },
                })
                .unwrap(),
            ));
        };
        if self.spot {
            for kind in [
                "notification",
                "spotState",
                "userFills",
                "userNonFundingLedgerUpdates",
            ] {
                send(kind, None, None, None);
            }
            return interests;
        }
        for kind in [
            "notification",
            "orderUpdates",
            "userEvents",
            "userFills",
            "userFundings",
            "userNonFundingLedgerUpdates",
            "clearinghouseState",
        ] {
            send(kind, None, None, None);
        }
        if self.tracks_spot_holdings() {
            send(
                "spotState",
                None,
                None,
                (self.account.mode == Mode::Portfolio).then_some(true),
            );
        }
        // Only clearinghouseState is per dex: Hyperliquid ignores `dex` on fills, orders and
        // events, refusing each copy as "Already subscribed".
        for dex in &self.dex_names {
            send("clearinghouseState", Some(dex), None, None);
        }
        // One activeAssetData per market filled the 1,000-per-IP cap with ~5 keyed
        // accounts; held symbols take settings from clearinghouseState instead.
        for m in markets {
            if !m.id.contains(':')
                && m.is_spot != Some(true)
                && self.account.asset_scope.contains(&m.symbol)
            {
                send("activeAssetData", None, Some(&m.id), None);
            }
        }
        interests
    }

    /// Re-subscribe settings when the selection or the set of symbols with open orders changes.
    pub(crate) fn refresh_asset_scope(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot || self.options.public_data_only {
            return;
        }
        let scope: std::collections::BTreeSet<String> = cx
            .store
            .orders()
            .iter()
            .map(|order| order.symbol.clone())
            .chain(self.account.selected.clone())
            .collect();
        if scope != self.account.asset_scope {
            self.account.asset_scope = scope;
            self.propose_socket_interests(cx);
        }
    }

    /// A caller read of `symbol`'s leverage, answered from a fresh activeAssetData read;
    /// the symbol becomes the selection, whose settings then stream.
    pub(crate) fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let Ok(context) = cx.admit_http_command(safe_cex_engine::request_context::Policy {
            operation: safe_cex_engine::request_context::Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: safe_cex_engine::request_context::RetryPolicy::Never,
        }) else {
            return;
        };
        let (Some(user), Some(coin)) = (self.user(), self.coin(cx, &symbol)) else {
            let _ = cx.fail_context(
                &context,
                VenueError::new(format!("Hyperliquid has no account leverage for {symbol}"))
                    .kind(ErrorKind::InvalidInput)
                    .outcome(safe_cex_engine::Outcome::NotSent),
            );
            return;
        };
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            coin: &'a str,
        }
        let body = serde_json::to_string(&Read {
            r#type: "activeAssetData",
            user,
            coin,
        })
        .unwrap();
        if self.account.selected.as_deref() != Some(symbol.as_str()) {
            self.account.selected = Some(symbol.clone());
            self.refresh_asset_scope(cx);
        }
        let pending = Pending::Leverage(Box::new(crate::LeverageRead {
            context: context.clone(),
            symbol,
        }));
        if let Err((_pending, error)) = cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Post,
            format!("{}/info", self.rest()),
            &[("Content-Type", "application/json")],
            Some(body),
            pending,
        ) {
            let _ = cx.fail_context(&context, error.venue_error());
        }
    }

    /// The rails a leverage read reports: the venue's row, applied where settings may move.
    pub(crate) fn leverage_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: crate::LeverageRead,
        body: &str,
    ) {
        let row = serde_json::from_str::<ActiveAsset>(body)
            .ok()
            .filter(|row| self.symbol(&row.coin) == Some(read.symbol.as_str()));
        let Some(row) = row else {
            let _ = cx.fail_context(&read.context, "Malformed Hyperliquid active asset settings");
            return;
        };
        let held = cx.store.positions().iter().any(|p| p.symbol == read.symbol);
        if !held && !self.settings_uncertain(&read.symbol) {
            self.apply_asset_setting(cx, &read.symbol, &row);
        }
        let isolated = row.leverage.kind != "cross";
        let _ = cx.done_context(
            &read.context,
            format!(
                "{{\"buyLeverage\":{0},\"sellLeverage\":{0},\"marginType\":\"{1}\",\"positionMode\":\"oneway\"}}",
                row.leverage.value,
                if isolated { "isolated" } else { "cross" }
            ),
        );
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.account_failed(
                cx,
                request,
                &format!("Hyperliquid HTTP {}", response.status),
            );
            return;
        }
        let raw = response.body;
        match request {
            Request::Role => {
                // A late answer still counts while the account is unresolved.
                if self.user().is_some() {
                    return;
                }
                self.account.role_pending = false;
                if let Some(t) = self.account.role_retry.take() {
                    cx.clear_timer(t);
                }
                self.apply_role(raw);
                if self.user().is_some() {
                    self.private_bootstrap(cx);
                } else {
                    cx.notice(
                        Level::Error,
                        code::CONFIG_INVALID,
                        "Hyperliquid account identity could not be resolved; re-add this account",
                    );
                }
            }
            Request::Mode => {
                self.account.mode_pending = false;
                let mode = match serde_json::from_str::<&str>(raw).unwrap_or("") {
                    "unifiedAccount" => Mode::Unified,
                    "portfolioMargin" => Mode::Portfolio,
                    _ => Mode::Standard,
                };
                if let Some(t) = self.account.mode_retry.take() {
                    cx.clear_timer(t);
                }
                // A retried read corrects the fallback, which refreshes spot holdings and sockets.
                if self.account.mode_resolved && mode == self.account.mode {
                    return;
                }
                self.account.mode = mode;
                self.mode_ready(cx);
            }
            Request::Builder { .. } => {
                self.account.builder_approved = serde_json::from_str::<Num>(raw)
                    .ok()
                    .map(|n| n.0 >= f64::from(self.builder().f));
                // Frame-fed: this engine's own read outlasts a source that has not re-read.
                if self.account.builder_approved.is_some() {
                    cx.hold_setting(crate::BUILDER_HOLD.clone());
                }
            }
            Request::AmendRole { id, amend } => {
                if !self.amend_read_pending(cx, &id, amend) {
                    return;
                }
                self.apply_role(raw);
                if self.user().is_some() {
                    self.read_amend_source(cx, &id, amend);
                } else {
                    self.amend_source_read(
                        cx,
                        &id,
                        amend,
                        Err("the account is not resolved".to_owned()),
                    );
                }
            }
            Request::AmendSource { id, amend } => {
                if !self.amend_read_pending(cx, &id, amend) {
                    return;
                }
                #[derive(serde::Deserialize)]
                struct Lookup<'a> {
                    #[serde(borrow)]
                    status: &'a str,
                    #[serde(default, borrow)]
                    order: Option<crate::wire::OrderUpdate<'a>>,
                }
                let read = serde_json::from_str::<Lookup>(raw)
                    .ok()
                    .filter(|lookup| lookup.status == "order")
                    .and_then(|lookup| lookup.order)
                    .ok_or_else(|| "the order read was unreadable".to_owned());
                match read {
                    Ok(row) => {
                        let captured = self.capture_protective_source(cx, &id, &row.order);
                        self.amend_source_read(
                            cx,
                            &id,
                            amend,
                            captured
                                .map(|()| (row.status.as_ref(), row.order.orig_sz, row.order.sz)),
                        );
                    }
                    Err(error) => self.amend_source_read(cx, &id, amend, Err(error)),
                }
            }
            Request::Fees => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Fees {
                    #[serde(deserialize_with = "number")]
                    user_cross_rate: f64,
                    #[serde(deserialize_with = "number")]
                    user_add_rate: f64,
                }
                if let Ok(f) = serde_json::from_str::<Fees>(raw) {
                    cx.store.update_markets(|m| {
                        m.taker_fee = Some(f.user_cross_rate);
                        m.maker_fee = Some(f.user_add_rate);
                    });
                }
            }
            Request::State {
                dex,
                revision,
                settings_revision,
                read,
            } => {
                self.account.inflight.remove(&format!("state:{dex}"));
                let mut applied = StateApplied::Held;
                if let Ok(state) = serde_json::from_str::<Clearinghouse>(raw) {
                    let changed =
                        self.account.state_revision.get(&dex).copied().unwrap_or(0) != revision;
                    if !changed
                        || (state.server_time.or(state.time).is_some()
                            && self.account.state_time.contains_key(&dex))
                    {
                        applied = self.apply_state(
                            cx,
                            &dex,
                            &state,
                            true,
                            false,
                            Some(settings_revision),
                        );
                    }
                } else {
                    cx.notice(
                        Level::Warn,
                        code::SNAPSHOT_DISCARDED,
                        "Malformed Hyperliquid account state; preserving last snapshot",
                    );
                    applied = StateApplied::Unusable("Malformed Hyperliquid account state");
                }
                if let StateApplied::Unusable(message) = &applied {
                    self.private_read_failed(cx, PrivateReadKind::State, message);
                }
                self.settle_fresh_positions(cx, &dex, read, applied);
                if dex.is_empty() {
                    self.retry_bootstrap_read(cx, BootstrapRead::State);
                }
            }
            Request::Orders {
                epoch,
                dex,
                revision,
                orders,
                seen,
                valid,
                coverage,
            } => {
                self.account.inflight.remove(&format!("orders:{dex}"));
                if !valid {
                    self.account
                        .orders_retry
                        .failed_after(cx, dex.clone(), 5000.0);
                    self.refresh_owed_orders(cx, &dex);
                    return;
                }
                self.account.orders_retry.succeeded(cx, &dex);
                if self.account.orders_revision.get(&dex).copied().unwrap_or(0) != revision {
                    self.schedule_orders(cx);
                    self.refresh_owed_orders(cx, &dex);
                    return;
                }
                let mut merged = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        self.dex_of(cx, &o.symbol) != dex
                            || matches!(o.status, OrderStatus::Creating | OrderStatus::Updating)
                            || !coverage.allows_absence(&o.id)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                for order in orders {
                    // A cancel can complete after this streamed row was parsed.
                    if self
                        .account
                        .suppressed
                        .get(&order.id)
                        .is_some_and(|at| cx.now - *at < 30_000.0)
                    {
                        continue;
                    }
                    self.account.suppressed.remove(&order.id);
                    if let Some(old) = merged.iter_mut().find(|o| o.id == order.id) {
                        *old = order;
                    } else {
                        merged.push(order);
                    }
                }
                // This dex's rows and the cover its read proves, together. The
                // account is every dex, and no account cover is proved before
                // discovery has named them all.
                let market_dex: HashMap<String, String> = cx
                    .store
                    .markets()
                    .iter()
                    .map(|market| {
                        (
                            market.symbol.clone(),
                            self.dex_of(cx, &market.symbol).to_owned(),
                        )
                    })
                    .collect();
                let in_dex =
                    |symbol: &str| market_dex.get(symbol).map_or("", String::as_str) == dex;
                let mut account: Vec<&str> = std::iter::once("")
                    .chain(self.dex_names.iter().map(String::as_str))
                    .collect();
                if !self.discovery_settled() {
                    account.push(DEXES_UNSETTLED);
                }
                let unproven = if coverage.rejected() == 0 {
                    Vec::new()
                } else {
                    vec![String::new()]
                };
                cx.commit_orders_read(
                    safe_cex_engine::order_read_cover::OrderReadScope::Account {
                        key: &dex,
                        account: &account,
                    },
                    in_dex,
                    merged
                        .into_iter()
                        .filter(|order| in_dex(&order.symbol))
                        .collect(),
                    &unproven,
                );
                cx.store.set_orders_loaded();
                if coverage.rejected() == 0 {
                    let symbols: Vec<_> = cx
                        .store
                        .markets()
                        .iter()
                        .filter(|market| self.dex_of(cx, &market.symbol) == dex)
                        .map(|market| market.symbol.clone())
                        .collect();
                    for symbol in symbols {
                        cx.orders_reconciled_since(Some(&symbol), epoch);
                    }
                    self.account.clean_dexes.insert(dex.clone(), epoch);
                    if self.discovery_settled()
                        && let Some(epoch) =
                            every_dex_clean(&self.dex_names, &self.account.clean_dexes)
                    {
                        cx.orders_reconciled_since(None, epoch);
                        cx.orders_fence_held(None);
                    }
                } else {
                    cx.orders_fence_held(Some(UNREADABLE_ORDER));
                    self.account.clean_dexes.remove(&dex);
                }
                self.account
                    .suppressed
                    .retain(|_, at| cx.now - *at < 120_000.0);
                let _ = seen;
                self.refresh_owed_orders(cx, &dex);
            }
            Request::Spot { revision } => {
                self.account.spot_inflight = false;
                if revision == self.account.spot_revision {
                    match serde_json::from_str::<SpotState>(raw) {
                        Ok(state) => self.apply_spot(cx, &state, false),
                        Err(_) => self.private_read_failed(
                            cx,
                            PrivateReadKind::Spot,
                            "Malformed Hyperliquid collateral state",
                        ),
                    }
                }
                self.retry_bootstrap_read(cx, BootstrapRead::Spot);
            }
            Request::Asset { symbol, revision } => {
                self.account.inflight.remove(&format!("asset:{symbol}"));
                if !self.settings_uncertain(&symbol)
                    || !self
                        .account
                        .settings_barriers
                        .get(&symbol)
                        .is_some_and(|b| b.revision == revision)
                {
                    if self.settings_uncertain(&symbol) {
                        self.refresh_uncertain_settings(cx, &symbol);
                    }
                    return;
                }
                if let Ok(row) = serde_json::from_str::<ActiveAsset>(raw)
                    && self.symbol(&row.coin) == Some(symbol.as_str())
                {
                    self.apply_asset_setting(cx, &symbol, &row);
                } else {
                    self.account_failed(
                        cx,
                        Request::Asset { symbol, revision },
                        "Malformed Hyperliquid active asset settings",
                    );
                }
            }
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        if let Request::Orders {
            orders,
            seen,
            valid,
            coverage,
            ..
        } = request
        {
            match serde_json::from_str::<OrderRow>(item) {
                Ok(row) => {
                    let id = row.oid.to_string();
                    if !seen.insert(id.clone()) {
                        coverage.reject(Some(id));
                        return;
                    }
                    if self
                        .account
                        .suppressed
                        .get(&id)
                        .is_some_and(|at| cx.now - *at < 30_000.0)
                    {
                        return;
                    }
                    if let Some(order) = self.map_order(cx, &row) {
                        orders.push(order);
                    } else if (spot_coin(&row.coin)
                        && !self.spot
                        && (!self.spot_markets || !self.spot_committed))
                        || (!spot_coin(&row.coin) && self.spot)
                        || self.excluded_coin(&row.coin)
                    {
                        // The main-dex read also lists the other product's orders.
                    } else if self.discovery_settled() {
                        if self
                            .account
                            .unknown_warned
                            .insert(format!("order:{}", row.coin.to_ascii_lowercase()))
                        {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Hyperliquid skipped order {id} on an unknown market"),
                            );
                        }
                        coverage.reject(Some(id));
                    } else {
                        *valid = false;
                        cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED,
                            "Hyperliquid owned order has unavailable market metadata; preserving previous orders");
                    }
                }
                Err(_) => {
                    #[derive(serde::Deserialize)]
                    struct Identity {
                        oid: u64,
                    }
                    let id = serde_json::from_str::<Identity>(item)
                        .ok()
                        .map(|row| row.oid.to_string());
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        match &id {
                            Some(id) => format!("Hyperliquid skipped malformed order {id}"),
                            None => "Hyperliquid skipped malformed order without an id".into(),
                        },
                    );
                    coverage.reject(id);
                }
            }
        }
    }
    pub(crate) fn private_read_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: PrivateReadKind,
        error: &str,
    ) {
        self.account.last_failed_private_read_kind = Some(kind);
        cx.event(
            Level::Warn,
            code::REQUEST_FAILED,
            error,
            vec![
                ("lastFailedPrivateReadKind", kind.as_str().into()),
                ("readKind", kind.as_str().into()),
            ],
        );
    }
    fn bootstrap_read_needed(&self, kind: BootstrapRead) -> bool {
        if self.spot
            || self.options.public_data_only
            || !self.account.started
            || self.user().is_none()
        {
            return false;
        }
        match kind {
            BootstrapRead::State => !self.account.core_loaded,
            BootstrapRead::Spot => {
                self.account.mode_resolved
                    && self.account.mode != Mode::Standard
                    && self.account.spot.is_none()
                    && self.account.pending_spot.is_none()
            }
        }
    }
    fn retry_bootstrap_read(&mut self, cx: &mut Cx<'_, Pending>, kind: BootstrapRead) {
        if self.bootstrap_read_needed(kind) {
            let pause = self
                .account
                .read_error
                .as_ref()
                .and_then(|e| e.retry_after_ms)
                .unwrap_or(0.0);
            self.account.bootstrap_retry.failed_after(cx, kind, pause);
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let kind = match &request {
            Request::State { .. } => Some(PrivateReadKind::State),
            Request::Spot { .. } => Some(PrivateReadKind::Spot),
            Request::Orders { .. } => Some(PrivateReadKind::Orders),
            Request::Role => Some(PrivateReadKind::Role),
            Request::Mode => Some(PrivateReadKind::Mode),
            _ => None,
        };
        let read_kind = request.read_kind();
        debug_assert!(kind.is_none_or(|kind| kind.as_str() == read_kind));
        match request {
            Request::Mode => self.mode_failed(cx),
            Request::Role => self.role_failed(cx, error),
            Request::State { dex, read, .. } => {
                self.account.inflight.remove(&format!("state:{dex}"));
                if dex.is_empty() {
                    self.retry_bootstrap_read(cx, BootstrapRead::State);
                }
                let fresh = self
                    .account
                    .read_error
                    .take()
                    .unwrap_or_else(|| VenueError::new(error));
                let key = self.dex_key(&dex);
                self.account.fresh_positions.failed(cx, key, read, &fresh);
                self.read_fresh_positions_again(cx, &dex, read);
            }
            Request::Orders { dex, .. } => {
                self.account.inflight.remove(&format!("orders:{dex}"));
                self.refresh_owed_orders(cx, &dex);
                let pause = self
                    .account
                    .read_error
                    .as_ref()
                    .and_then(|error| error.retry_after_ms)
                    .unwrap_or(5000.0)
                    .max(5000.0);
                self.account.orders_retry.failed_after(cx, dex, pause);
            }
            Request::Spot { .. } => {
                self.account.spot_inflight = false;
                self.retry_bootstrap_read(cx, BootstrapRead::Spot);
            }
            Request::Asset { symbol, .. } => {
                self.account.inflight.remove(&format!("asset:{symbol}"));
                if self.settings_uncertain(&symbol) {
                    self.account.asset_retry.insert(symbol);
                    if self.account.asset_retry_timer.is_none() {
                        self.account.asset_retry_timer = Some(cx.set_timer(5000.0));
                    }
                }
            }
            Request::Builder { .. } => self.account.builder_approved = None,
            Request::AmendSource { id, amend } | Request::AmendRole { id, amend } => {
                if !self.amend_read_current(&id, amend) {
                    return;
                }
                self.amend_source_read(cx, &id, amend, Err(error.to_owned()));
            }
            Request::Fees => {}
        }
        if let Some(kind) = kind {
            self.private_read_failed(cx, kind, error);
        } else {
            cx.event(
                Level::Warn,
                code::REQUEST_FAILED,
                error,
                vec![("readKind", read_kind.into())],
            );
        }
    }
    /// `readPositions`: wait on each dex's next read. One already out may predate
    /// the close, so it is followed by a new read and never answers.
    pub(crate) fn read_positions(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started
            || self.options.public_data_only
            || self.user().is_none()
            || !self.discovery_settled()
        {
            let error =
                VenueError::new("Hyperliquid account is not loaded").kind(ErrorKind::InvalidInput);
            fresh_positions::refuse(cx, error);
            return;
        }
        let mut dexes = vec![String::new()];
        dexes.extend(self.dex_names.iter().cloned());
        let legs = dexes
            .iter()
            .map(|dex| {
                let last = self.account.state_reads.get(dex).copied().unwrap_or(0);
                (self.dex_key(dex), last + 1)
            })
            .collect();
        if self.account.fresh_positions.admit(cx, legs) {
            for dex in dexes {
                // An undispatched routine intent predates this caller and owns no context.
                // Replace it with a fresh foreground read instead of waiting for a slot.
                if let Some(index) = self.account.reads.iter().position(|read| {
                    read.priority != ReadPriority::Foreground && matches!(&read.continuation,
                        ReadContinuation::Account(Request::State { dex: queued, .. }) if queued == &dex)
                }) {
                    self.account.reads.remove(index);
                    self.account.inflight.remove(&format!("state:{dex}"));
                }
                self.refresh_state(cx, &dex);
            }
        }
    }
    fn dex_key(&self, dex: &str) -> u8 {
        self.dex_names
            .iter()
            .position(|name| name == dex)
            .map_or(0, |index| index as u8 + 1)
    }
    fn settle_fresh_positions(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        dex: &str,
        read: u32,
        applied: StateApplied,
    ) {
        let key = self.dex_key(dex);
        match applied {
            StateApplied::Committed(unlisted) => {
                self.account
                    .fresh_positions
                    .committed(cx, key, read, &unlisted);
            }
            StateApplied::Held => {}
            StateApplied::Unusable(message) => {
                let error = VenueError::new(message);
                self.account.fresh_positions.failed(cx, key, read, &error);
            }
        }
        self.read_fresh_positions_again(cx, dex, read);
    }
    /// At once for a waiter the read predates, else once a newer stream frame
    /// that held the read can have been overtaken.
    fn read_fresh_positions_again(&mut self, cx: &mut Cx<'_, Pending>, dex: &str, read: u32) {
        let key = self.dex_key(dex);
        if self.account.fresh_positions.predates(key, read) {
            self.refresh_state(cx, dex);
        } else if self.account.fresh_positions.owed(key)
            && self.account.fresh_positions_timer.is_none()
        {
            self.account.fresh_positions_timer = Some(cx.set_timer(FRESH_POSITIONS_RETRY_MS));
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(kind) = self.account.bootstrap_retry.on_timer(timer)
            && self.bootstrap_read_needed(kind)
        {
            match kind {
                BootstrapRead::State => self.refresh_state(cx, ""),
                BootstrapRead::Spot => self.refresh_spot(cx, true),
            }
        }
        if self.account.fresh_positions_timer == Some(timer) {
            self.account.fresh_positions_timer = None;
            let mut dexes = vec![String::new()];
            dexes.extend(self.dex_names.iter().cloned());
            for dex in dexes {
                if self.account.fresh_positions.owed(self.dex_key(&dex)) {
                    self.refresh_state(cx, &dex);
                }
            }
        }
        if let Some(dex) = self.account.orders_retry.on_timer(timer) {
            self.refresh_orders_scope(cx, &dex);
        }
        if self.account.asset_retry_timer == Some(timer) {
            self.account.asset_retry_timer = None;
            for symbol in std::mem::take(&mut self.account.asset_retry) {
                if self.settings_uncertain(&symbol) {
                    self.refresh_asset_setting(cx, &symbol);
                }
            }
        }
        if self.account.state_refresh_timer == Some(timer) {
            self.account.state_refresh_timer = None;
            let dirty = std::mem::take(&mut self.account.state_dirty);
            for dex in dirty {
                if self.account.inflight.contains(&format!("state:{dex}")) {
                    self.account.state_dirty.insert(dex);
                } else {
                    self.refresh_state(cx, &dex);
                }
            }
            if !self.account.state_dirty.is_empty() {
                self.account.state_refresh_timer = Some(cx.set_timer(350.0));
            }
        }
        if self.account.read_timer.is_some_and(|(id, _)| id == timer) {
            self.account.read_timer = None;
            self.pump_reads(cx);
        }
        if self.account.mode_retry == Some(timer) {
            self.account.mode_retry = None;
            self.send_mode(cx);
        }
        if self.account.role_retry == Some(timer) {
            self.account.role_retry = None;
            self.send_role(cx);
        }
        if self.account.refresh_timer == Some(timer) {
            self.account.refresh_timer = None;
            self.account.last_order_refresh = cx.now;
            self.refresh_orders(cx);
        }
        if self.account.sync_timer == Some(timer) && self.spot {
            self.refresh_spot(cx, false);
            self.refresh_orders_scope(cx, "");
            self.account.sync_timer = Some(cx.set_poll_timer_with_bounds(
                10_000.0,
                safe_cex_engine::PollTimerBounds {
                    minimum_interval_ms: 10_000.0,
                    ..Default::default()
                },
            ));
            return;
        }
        if self.account.sync_timer == Some(timer) {
            for dex in self.dex_names.clone() {
                self.refresh_state(cx, &dex);
            }
            self.account.sync_timer = Some(cx.set_poll_timer_with_bounds(
                120_000.0,
                safe_cex_engine::PollTimerBounds {
                    minimum_interval_ms: 120_000.0,
                    ..Default::default()
                },
            ));
        }
    }
    fn dex_of<'a>(&self, cx: &'a Cx<'_, Pending>, symbol: &str) -> &'a str {
        cx.store
            .market(symbol)
            .and_then(|m| m.id.split_once(':'))
            .map_or("", |(d, _)| d)
    }
    pub(crate) fn acknowledge_order_write(&mut self, cx: &Cx<'_, Pending>, symbol: &str) {
        let dex = self.dex_of(cx, symbol).to_owned();
        *self.account.orders_revision.entry(dex).or_default() += 1;
    }
    pub(crate) fn acknowledge_settings(&mut self, cx: &Cx<'_, Pending>, symbol: &str) {
        self.account.settings_revision += 1;
        self.account.settings_barriers.insert(
            symbol.into(),
            SettingBarrier {
                at: cx.now,
                revision: self.account.settings_revision,
                confirmed: false,
            },
        );
    }
    pub(crate) fn refresh_uncertain_settings(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let dex = self.dex_of(cx, symbol).to_owned();
        self.account.state_dirty.insert(dex);
        if self.account.state_refresh_timer.is_none() {
            self.account.state_refresh_timer = Some(cx.set_timer(350.0));
        }
        self.refresh_asset_setting(cx, symbol);
    }
    fn refresh_asset_setting(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        #[derive(Serialize)]
        struct Read<'a> {
            r#type: &'static str,
            user: &'a str,
            coin: &'a str,
        }
        if let Some(coin) = self.coin(cx, symbol) {
            let coin = coin.to_owned();
            if !self.account.inflight.insert(format!("asset:{symbol}")) {
                return;
            }
            let body = serde_json::to_string(&Read {
                r#type: "activeAssetData",
                user: self.user().unwrap_or_default(),
                coin: &coin,
            })
            .unwrap();
            self.read_body(
                cx,
                body,
                Request::Asset {
                    symbol: symbol.into(),
                    revision: self
                        .account
                        .settings_barriers
                        .get(symbol)
                        .map_or(0, |b| b.revision),
                },
            );
        }
    }
    fn apply_asset_setting(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        row: &ActiveAsset<'_>,
    ) {
        let margin = if row.leverage.kind == "cross" {
            MarginType::Cross
        } else {
            MarginType::Isolated
        };
        cx.store.update_setting(symbol, |s| {
            s.leverage = row.leverage.value;
            s.margin_type = Some(margin);
        });
        cx.store.update_positions(|p| {
            if p.symbol == symbol {
                p.leverage = row.leverage.value;
                p.margin_type = Some(margin);
            }
        });
        if let Some(barrier) = self.account.settings_barriers.get_mut(symbol) {
            barrier.confirmed = true;
        }
        self.settings_reconciled(symbol);
    }
    fn apply_state(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        dex: &str,
        state: &Clearinghouse<'_>,
        authoritative: bool,
        // The feed's own frame lists every open coin of its dex; one it omits is closed.
        complete: bool,
        settings_revision: Option<u64>,
    ) -> StateApplied {
        if let Some(user) = self.user() {
            cx.record_socket_topic(&["@clearinghouseState|||", user, "|", dex, "|||"]);
        }
        // While discovery retries, unknown nonzero ownership holds the snapshot; once
        // settled the row is skipped and its stored copy kept. Rows TS hides by
        // design (delisted, unquoted) are skipped, as in TS.
        let mut coverage = SnapshotCoverage::default();
        let mut rows = Vec::with_capacity(state.asset_positions.len());
        let mut malformed = false;
        for raw in &state.asset_positions {
            match serde_json::from_str::<AssetPosition>(raw.get()) {
                Ok(row) => rows.push(row),
                Err(_) => {
                    malformed = true;
                    #[derive(serde::Deserialize)]
                    struct Identity<'a> {
                        #[serde(borrow)]
                        position: Coin<'a>,
                    }
                    #[derive(serde::Deserialize)]
                    struct Coin<'a> {
                        #[serde(borrow)]
                        coin: std::borrow::Cow<'a, str>,
                    }
                    let coin = serde_json::from_str::<Identity>(raw.get())
                        .ok()
                        .map(|row| row.position.coin.to_ascii_lowercase());
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        match &coin {
                            Some(coin) => format!("Hyperliquid skipped malformed position {coin}"),
                            None => "Hyperliquid skipped malformed position without a coin".into(),
                        },
                    );
                    coverage.reject(coin);
                }
            }
        }
        let settled = self.discovery_settled();
        for row in &rows {
            let coin = &row.position.coin;
            if row.position.szi == 0.0 || self.symbol(coin).is_some() || self.excluded_coin(coin) {
                continue;
            }
            if !settled {
                cx.notice(Level::Error, code::SNAPSHOT_DISCARDED,
                    "Hyperliquid owned position has unavailable market metadata; preserving previous account snapshot");
                return StateApplied::Unusable("Hyperliquid positions: a row could not be mapped");
            }
            let coin = coin.to_ascii_lowercase();
            if self
                .account
                .unknown_warned
                .insert(format!("position:{coin}"))
            {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    format!("Hyperliquid skipped position {coin} on an unknown market"),
                );
            }
            coverage.reject(Some(coin));
        }
        let source_time = state.server_time.or(state.time);
        if (authoritative || complete || !dex.is_empty())
            && let Some(time) = source_time
            && self
                .account
                .position_time
                .iter()
                .any(|(symbol, last)| self.dex_of(cx, symbol) == dex && *last > time)
        {
            return StateApplied::Held;
        }
        if let Some(time) = source_time {
            if self
                .account
                .state_time
                .get(dex)
                .is_some_and(|last| time < *last)
            {
                return StateApplied::Held;
            }
            self.account.state_time.insert(dex.into(), time);
        }
        *self.account.state_revision.entry(dex.into()).or_default() += 1;
        if authoritative && let Some(time) = source_time {
            for market in cx.store.markets() {
                if market.id.split_once(':').map_or("", |(d, _)| d) == dex {
                    self.account
                        .position_time
                        .insert(market.symbol.clone(), time);
                    self.account
                        .snapshot_time
                        .insert(market.symbol.clone(), time);
                }
            }
        }
        let mut positions = if authoritative || complete || !dex.is_empty() {
            cx.store
                .positions()
                .iter()
                .filter(|p| {
                    self.dex_of(cx, &p.symbol) != dex
                        || !coverage.allows_absence(
                            &self
                                .coin(cx, &p.symbol)
                                .map_or_else(|| perp_coin(&p.symbol), str::to_ascii_lowercase),
                        )
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            cx.store.positions().to_vec()
        };
        for row in &rows {
            let p = &row.position;
            let Some(symbol) = self.symbol(&p.coin).map(str::to_owned) else {
                continue;
            };
            if let Some(time) = source_time {
                if self
                    .account
                    .position_time
                    .get(&symbol)
                    .is_some_and(|last| *last > time)
                {
                    continue;
                }
                self.account.position_time.insert(symbol.clone(), time);
                self.account.snapshot_time.insert(symbol.clone(), time);
            }
            let mut margin = if p.leverage.kind == "cross" {
                MarginType::Cross
            } else {
                MarginType::Isolated
            };
            // Position size can be fresh while its settings predate a local write.
            let settings_fresh =
                self.account
                    .settings_barriers
                    .get(&symbol)
                    .is_none_or(|barrier| {
                        settings_revision.is_none_or(|read| read >= barrier.revision)
                            && source_time.map_or_else(
                                || settings_revision.is_some(),
                                |time| time >= barrier.at,
                            )
                    });
            let mut leverage = p.leverage.value;
            if settings_fresh {
                cx.store.update_setting(&symbol, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(margin);
                });
                if let Some(barrier) = self.account.settings_barriers.get_mut(&symbol) {
                    barrier.confirmed = true;
                }
                self.settings_reconciled(&symbol);
            } else if let Some(setting) = cx.store.setting(&symbol) {
                leverage = setting.leverage;
                margin = setting.margin_type.unwrap_or(margin);
            }
            positions.retain(|old| old.symbol != symbol);
            if p.szi == 0.0 {
                continue;
            }
            let side = if p.szi > 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            positions.push(Position {
                id: symbol.clone(),
                symbol: symbol.clone(),
                side,
                entry_price: p.entry_px.unwrap_or(0.0),
                notional: p.position_value,
                leverage,
                margin_type: Some(margin),
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl: p.unrealized_pnl,
                realized_pnl: self.account.realized.get(&(symbol, side)).copied(),
                contracts: p.szi.abs(),
                liquidation_price: p.liquidation_px.unwrap_or(0.0),
                margin_size: p.margin_used,
                last_update: state.time,
            });
        }
        cx.store.set_positions(positions);
        if dex.is_empty()
            && let Some(summary) = state
                .margin_summary
                .as_ref()
                .or(state.cross_margin_summary.as_ref())
        {
            // Native row PnL, not stored copies: a retained copy is stale and a skipped
            // row has none. A malformed row's PnL is unknown, so the last value holds.
            if !malformed {
                let native = rows
                    .iter()
                    .filter(|row| !self.excluded_coin(&row.position.coin))
                    .map(|row| row.position.unrealized_pnl)
                    .sum::<f64>();
                let other_dexes = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|p| !self.dex_of(cx, &p.symbol).is_empty())
                    .map(|p| p.unrealized_pnl)
                    .sum::<f64>();
                self.account.core_loaded = true;
                self.account
                    .bootstrap_retry
                    .succeeded(cx, &BootstrapRead::State);
                self.account.core_value = summary.account_value - native - other_dexes;
            }
            self.account.core_used = summary.total_margin_used;
        }
        self.live_balance(cx);
        if coverage.rejected() != 0 {
            return StateApplied::Unusable("Hyperliquid positions: a row could not be mapped");
        }
        let unlisted = rows
            .iter()
            .filter(|row| row.position.szi != 0.0 && self.excluded_coin(&row.position.coin))
            .map(|row| row.position.coin.to_string())
            .collect();
        StateApplied::Committed(unlisted)
    }
    pub(crate) fn account_prices(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.store.loaded().balance || self.account.pending_spot.is_some() {
            self.live_balance(cx);
        }
    }
    fn collateral_values<'a>(
        &self,
        cx: &Cx<'_, Pending>,
        balances: impl Iterator<Item = (&'a str, f64, f64)>,
    ) -> Option<(f64, f64, bool)> {
        let mut equity = 0.0;
        let mut hold = 0.0;
        let mut unvalued = false;
        for (coin, total, held) in balances {
            let price = if total == 0.0 && held == 0.0
                || ["USDC", "USDT", "USD", "USDH"]
                    .iter()
                    .any(|stable| coin.eq_ignore_ascii_case(stable))
            {
                1.0
            } else if self.spot_markets
                && let price = self.spot_mark_price(cx, coin)
                && price > 0.0
            {
                price
            } else if let Some(symbol) = self.symbol(coin) {
                let ticker = cx.store.ticker(symbol)?;
                [ticker.mark, ticker.last]
                    .into_iter()
                    .find(|price| price.is_finite() && *price > 0.0)?
            } else if self.core_discovery_incomplete {
                return None;
            } else {
                // Perp mode has no spot meta: tokens like UBTC have no valuation market.
                unvalued = true;
                continue;
            };
            equity += total * price;
            hold += held * price;
            if !equity.is_finite() || !hold.is_finite() {
                return None;
            }
        }
        Some((equity, hold, unvalued))
    }
    fn warn_unvalued(&mut self, cx: &mut Cx<'_, Pending>, unvalued: bool) {
        if unvalued && !self.account.valuation_warned {
            self.account.valuation_warned = true;
            cx.notice(
                Level::Warn,
                code::VALUATION_UNAVAILABLE,
                "Hyperliquid collateral token has no valuation market; excluded from USD balance",
            );
        }
    }
    fn current_upnl(&self, cx: &Cx<'_, Pending>) -> f64 {
        cx.store
            .positions()
            .iter()
            .map(|p| {
                // The mid, as when `last` was always the mid; a trade print moves neither.
                cx.store
                    .ticker(&p.symbol)
                    .and_then(|t| {
                        [t.mid_price.unwrap_or(0.0), t.last]
                            .into_iter()
                            .find(|price| price.is_finite() && *price > 0.0)
                    })
                    .filter(|_| p.entry_price > 0.0)
                    .map_or(p.unrealized_pnl, |price| {
                        (price - p.entry_price)
                            * p.contracts
                            * if p.side == PositionSide::Long {
                                1.0
                            } else {
                                -1.0
                            }
                    })
            })
            .sum()
    }
    fn commit_collateral(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        equity: f64,
        hold: f64,
        available: Option<f64>,
        ws: bool,
    ) {
        self.account.spot = Some((equity, hold));
        self.account
            .bootstrap_retry
            .succeeded(cx, &BootstrapRead::Spot);
        if let Some(free) = available {
            self.account.authoritative =
                Some((equity - self.current_upnl(cx), equity - free, free));
            if ws {
                self.account.spot_authoritative_ws = true;
            }
        }
    }
    pub(crate) fn live_balance(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.spot
            || self.user().is_none()
            || self.options.public_data_only
            || !self.account.mode_resolved
            || !self.account.core_loaded
        {
            return;
        }
        if let Some(pending) = self.account.pending_spot.as_ref() {
            let Some((equity, hold, unvalued)) = self.collateral_values(
                cx,
                pending
                    .balances
                    .iter()
                    .map(|(coin, total, hold)| (coin.as_str(), *total, *hold)),
            ) else {
                return;
            };
            let available = pending.available;
            let ws = pending.ws;
            let pending = self.account.pending_spot.take().expect("pending spot");
            self.warn_unvalued(cx, unvalued);
            self.commit_collateral(cx, equity, hold, available, ws);
            self.publish_spot_coins(
                cx,
                pending
                    .balances
                    .iter()
                    .map(|(coin, total, _)| (coin.as_str(), *total)),
            );
        }
        if self.account.mode != Mode::Standard && self.account.spot.is_none() {
            return;
        }
        let upnl = self.current_upnl(cx);
        let balance = if self.account.mode != Mode::Standard
            && let Some((equity, hold)) = self.account.spot
        {
            let (total, used, free) = self.account.authoritative.unwrap_or((
                equity,
                self.account.core_used + hold,
                equity + upnl - self.account.core_used - hold,
            ));
            Balance {
                total,
                used,
                free,
                upnl,
            }
        } else {
            let total = self.account.core_value;
            Balance {
                total,
                used: self.account.core_used,
                free: total + upnl - self.account.core_used,
                upnl,
            }
        };
        cx.store.set_balance(balance);
        self.publish_spot_holdings(cx);
    }
    /// TS publishes merged spot holdings as `balance.coins`, valued on each balance
    /// update; only a changed valuation reaches the store.
    fn publish_spot_holdings(&mut self, cx: &mut Cx<'_, Pending>) {
        let Some(holdings) = &self.account.spot_holdings else {
            return;
        };
        let coins: Vec<CoinBalance> = holdings
            .iter()
            .map(|(coin, total, _)| CoinBalance {
                base: coin.clone(),
                amount: *total,
                notional: total * self.spot_mark_price(cx, coin),
            })
            .collect();
        if self.account.published_coins.as_ref() != Some(&coins) {
            cx.store.set_coin_balances(coins.clone());
            self.account.published_coins = Some(coins);
        }
    }
    /// TS `getSpotCollateralMarkPrice`: the coin's spot pair (USDC, then other stable
    /// quotes), then its own ticker, then its perp; the first positive mark.
    pub(crate) fn spot_mark_price(&self, cx: &Cx<'_, Pending>, coin: &str) -> f64 {
        let upper = coin.to_ascii_uppercase();
        if ["USDC", "USDT", "USD", "USDH"].contains(&upper.as_str()) {
            return 1.0;
        }
        let pairs = [
            format!("{coin}/USDC"),
            format!("{upper}/USDC"),
            format!("{upper}/USDH"),
            format!("{upper}/USDT0"),
            format!("{upper}/USDE"),
        ];
        let own = self.symbol(coin).map(str::to_owned);
        let perp = format!("{upper}-PERP");
        pairs
            .iter()
            .chain(own.as_ref())
            .chain(std::iter::once(&perp))
            .filter_map(|symbol| cx.store.ticker(symbol))
            .map(|ticker| ticker.mark)
            .find(|mark| mark.is_finite() && *mark > 0.0)
            .unwrap_or(0.0)
    }
    fn spot_price(&self, cx: &Cx<'_, Pending>, coin: &str, path: &mut Vec<String>) -> f64 {
        if ["USDC", "USDT", "USD", "USDH"]
            .iter()
            .any(|c| coin.eq_ignore_ascii_case(c))
        {
            return 1.0;
        }
        if path.len() >= 3
            || path
                .iter()
                .any(|visited| visited.eq_ignore_ascii_case(coin))
        {
            return 0.0;
        }
        path.push(coin.to_owned());
        let price =
            (|| {
                for usdc in [true, false] {
                    for market in cx.store.markets().iter().filter(|m| {
                        m.base.eq_ignore_ascii_case(coin) && (m.quote == "USDC") == usdc
                    }) {
                        let Some(ticker) = cx.store.ticker(&market.symbol) else {
                            continue;
                        };
                        let mark = if ticker.mark > 0.0 {
                            ticker.mark
                        } else {
                            ticker.last
                        };
                        if mark.is_finite() && mark > 0.0 {
                            let quote = self.spot_price(cx, &market.quote, path);
                            let notional = mark * quote;
                            if notional.is_finite() && notional > 0.0 {
                                return notional;
                            }
                        }
                    }
                }
                0.0
            })();
        path.pop();
        price
    }
    fn apply_spot(&mut self, cx: &mut Cx<'_, Pending>, state: &SpotState<'_>, ws: bool) {
        if self.spot {
            let mut balance = Balance::default();
            let mut currencies = Vec::new();
            let mut coins = Vec::new();
            let mut valuation_path = Vec::with_capacity(3);
            for b in &state.balances {
                let coin = b
                    .token
                    .and_then(|token| self.spot_tokens.get(&token))
                    .and_then(|symbol| cx.store.market(symbol))
                    .map_or(b.coin.as_ref(), |m| m.base.as_str());
                let price = self.spot_price(cx, coin, &mut valuation_path);
                if self.core_discovery_incomplete
                    && (b.total != 0.0 || b.hold != 0.0)
                    && (!price.is_finite() || price <= 0.0)
                {
                    cx.notice(Level::Error, code::SNAPSHOT_DISCARDED,
                        "Hyperliquid owned spot balance has unavailable valuation; preserving previous account snapshot");
                    return;
                }
                let free = (b.total - b.hold).max(0.0);
                balance.total += b.total * price;
                balance.used += b.hold * price;
                balance.free += free * price;
                currencies.push(CurrencyBalance {
                    currency: b.coin.to_string(),
                    balance: Balance {
                        total: b.total,
                        used: b.hold,
                        free,
                        upnl: 0.0,
                    },
                });
                coins.push(CoinBalance {
                    base: b.coin.to_string(),
                    amount: b.total,
                    notional: b.total * price,
                });
            }
            cx.store.set_balance(balance);
            cx.store.set_currency_balances(currencies);
            cx.store.set_coin_balances(coins);
            self.account.spot_revision += 1;
            return;
        }
        let available = state
            .token_to_available_after_maintenance
            .iter()
            .find(|(token, _)| *token == 0)
            .map(|(_, n)| n.0);
        if self.account.spot_authoritative_ws && (!ws || available.is_none()) {
            return;
        }
        if self.spot_markets {
            self.account.spot_holdings = Some(
                state
                    .balances
                    .iter()
                    .filter(|b| {
                        !b.coin.trim().is_empty() && b.total.is_finite() && b.hold.is_finite()
                    })
                    .map(|b| (b.coin.trim().to_owned(), b.total, b.hold))
                    .collect(),
            );
            if self.account.mode == Mode::Standard {
                // Standard spot is holdings only, never collateral (TS).
                self.account.spot_revision += 1;
                self.live_balance(cx);
                return;
            }
        }
        self.account.spot_revision += 1;
        let values = self
            .account
            .core_loaded
            .then(|| {
                self.collateral_values(
                    cx,
                    state
                        .balances
                        .iter()
                        .map(|balance| (balance.coin.as_ref(), balance.total, balance.hold)),
                )
            })
            .flatten();
        let Some((equity, hold, unvalued)) = values else {
            if self.account.pending_spot.is_none() {
                cx.notice(
                    Level::Warn,
                    code::VALUATION_UNAVAILABLE,
                    "Hyperliquid collateral valuation unavailable; retaining previous USD balance",
                );
            }
            self.account
                .bootstrap_retry
                .succeeded(cx, &BootstrapRead::Spot);
            self.account.pending_spot = Some(PendingSpotCollateral {
                balances: state
                    .balances
                    .iter()
                    .map(|balance| (balance.coin.to_string(), balance.total, balance.hold))
                    .collect(),
                available,
                ws,
            });
            // An unpriced live snapshot still supersedes an older REST read.
            if ws && available.is_some() {
                self.account.spot_authoritative_ws = true;
            }
            return;
        };
        self.account.pending_spot = None;
        self.warn_unvalued(cx, unvalued);
        self.commit_collateral(cx, equity, hold, available, ws);
        self.live_balance(cx);
        self.publish_spot_coins(
            cx,
            state.balances.iter().map(|b| (b.coin.as_ref(), b.total)),
        );
    }
    /// Spot holdings for the margin panel, as TS shows them; USD totals stay unified.
    fn publish_spot_coins<'a>(
        &self,
        cx: &mut Cx<'_, Pending>,
        balances: impl Iterator<Item = (&'a str, f64)>,
    ) {
        let coins = balances
            .filter(|(_, total)| *total != 0.0)
            .map(|(coin, total)| CoinBalance {
                base: coin.to_string(),
                amount: total,
                notional: total * self.collateral_price(cx, coin).unwrap_or(0.0),
            })
            .collect();
        cx.store.set_coin_balances(coins);
    }
    /// The USD price collateral valuation uses for one coin, when it has one.
    fn collateral_price(&self, cx: &Cx<'_, Pending>, coin: &str) -> Option<f64> {
        if ["USDC", "USDT", "USD", "USDH"]
            .iter()
            .any(|stable| coin.eq_ignore_ascii_case(stable))
        {
            return Some(1.0);
        }
        let ticker = cx.store.ticker(self.symbol(coin)?)?;
        [ticker.mark, ticker.last]
            .into_iter()
            .find(|price| price.is_finite() && *price > 0.0)
    }
    pub(crate) fn map_order(&self, cx: &Cx<'_, Pending>, row: &OrderRow<'_>) -> Option<Order> {
        let symbol = self.symbol(&row.coin)?.to_owned();
        let (kind, price) = infer(row);
        Some(Order {
            id: row.oid.to_string(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol,
            kind,
            side: side(&row.side),
            price,
            amount: row.orig_sz,
            filled: (row.orig_sz - row.sz).max(0.0),
            remaining: row.sz,
            reduce_only: row.reduce_only,
            close: None,
            client_order_id: row
                .cloid
                .as_deref()
                .filter(|c| valid_cloid(c))
                .map(str::to_owned),
            time_in_force: tif(&row.tif),
            last_update: cx.now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
    pub(crate) fn account_frame(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        if self.user().is_none() {
            return;
        }
        let Ok(frame) = serde_json::from_str::<Envelope>(text) else {
            return;
        };
        let raw = frame.data.get();
        // Clearinghouse receipts are attributed only after decoding their dex below.
        if frame.channel != "clearinghouseState"
            && let Some(user) = self.user()
        {
            cx.record_socket_topic(&["@", &frame.channel, "|||", user, "||||"]);
        }
        match frame.channel.as_ref() {
            "clearinghouseState" if !self.spot => {
                if let Ok(env) = serde_json::from_str::<ClearingEnvelope>(raw)
                    && let Some(mut state) = env.clearinghouse_state
                {
                    state.time = env.server_time.or(env.time).or(state.time);
                    self.apply_state(cx, &env.dex, &state, false, true, None);
                } else if let Ok(state) = serde_json::from_str::<Clearinghouse>(raw) {
                    self.apply_state(cx, "", &state, false, false, None);
                }
            }
            "spotState" | "spotClearinghouseState" => {
                if let Ok(env) = serde_json::from_str::<SpotEnvelope>(raw) {
                    self.apply_spot(cx, &env.spot_state, true);
                } else if let Ok(state) = serde_json::from_str::<SpotState>(raw) {
                    self.apply_spot(cx, &state, true);
                }
            }
            "activeAssetData" if !self.spot => {
                if let Ok(row) = serde_json::from_str::<ActiveAsset>(raw)
                    && let Some(symbol) = self.symbol(&row.coin).map(str::to_owned)
                {
                    if let Some(user) = self.user() {
                        cx.record_socket_topic(&[
                            "@activeAssetData|",
                            &row.coin,
                            "||",
                            user,
                            "||||",
                        ]);
                    }
                    let margin = if row.leverage.kind == "cross" {
                        MarginType::Cross
                    } else {
                        MarginType::Isolated
                    };
                    // Until confirmed, untimed asset frames may only echo an acknowledged write.
                    if self.settings_uncertain(&symbol)
                        || cx.store.positions().iter().any(|p| p.symbol == symbol)
                        || (self
                            .account
                            .settings_barriers
                            .get(&symbol)
                            .is_some_and(|b| !b.confirmed)
                            && cx.store.setting(&symbol).is_none_or(|s| {
                                s.leverage != row.leverage.value || s.margin_type != Some(margin)
                            }))
                    {
                        return;
                    }
                    self.apply_asset_setting(cx, &symbol, &row);
                }
            }
            "orderUpdates" if !self.spot => {
                let _ = json::each::<OrderUpdate>(raw, |u| self.order_event(cx, u));
            }
            "userFills" => {
                if let Ok(fills) = serde_json::from_str::<Fills>(raw) {
                    self.fills(cx, fills);
                }
            }
            "user" | "userEvents" if !self.spot => {
                if raw.starts_with('[') {
                    if let Ok(rows) = serde_json::from_str::<Vec<FillRow>>(raw) {
                        self.fills(
                            cx,
                            Fills {
                                is_snapshot: false,
                                fills: rows,
                            },
                        );
                    } else if let Ok(rows) = serde_json::from_str::<Vec<OrderUpdate>>(raw) {
                        for row in rows {
                            self.order_event(cx, row);
                        }
                    }
                } else if let Ok(events) = serde_json::from_str::<UserEvents>(raw) {
                    self.fills(
                        cx,
                        Fills {
                            is_snapshot: events.is_snapshot,
                            fills: events.fills,
                        },
                    );
                    for order in events.orders {
                        self.order_event(cx, order);
                    }
                }
            }
            "userNonFundingLedgerUpdates" if self.spot || self.tracks_spot_holdings() => {
                self.refresh_spot(cx, false)
            }
            _ => {}
        }
    }
    fn order_event(&mut self, cx: &mut Cx<'_, Pending>, update: OrderUpdate<'_>) {
        let row = update.order;
        let id = row.oid.to_string();
        let Some(symbol) = self.symbol(&row.coin).map(str::to_owned) else {
            return;
        };
        let dex = self.dex_of(cx, &symbol).to_owned();
        *self.account.orders_revision.entry(dex).or_default() += 1;
        if terminal(&update.status) {
            if failed_protective_status(&update.status)
                && cx.store.order(&id).is_none()
                && !self.account.suppressed.contains_key(&id)
                && row.oid != 0
                && row.reduce_only
                && row.is_trigger
                && matches!(
                    row.order_type.as_ref(),
                    "Stop Market" | "Stop Limit" | "Take Profit Market" | "Take Profit Limit"
                )
                && let Some(cloid) = row.cloid.as_deref().filter(|cloid| !cloid.is_empty())
            {
                cx.event(
                    Level::Warn,
                    code::CHILD_LEG_FAILED,
                    format!(
                        "Hyperliquid protective order {id} ({cloid}) {}",
                        update.status
                    ),
                    vec![
                        ("symbol", symbol.as_str().into()),
                        ("orderId", id.as_str().into()),
                        ("clientOrderId", cloid.into()),
                        ("status", update.status.as_ref().into()),
                    ],
                );
            }
            if update.status == "filled" {
                self.amend_source_filled(&id);
            }
            self.account.suppressed.insert(id.clone(), cx.now);
            cx.store.remove_orders(&[&id]);
            if let Some(c) = row.cloid
                && self
                    .account
                    .records
                    .get(c.as_ref())
                    .is_none_or(|record| record.order.id.is_empty() || record.order.id == id)
            {
                self.account.records.remove(c.as_ref());
            }
            return;
        }
        if !["open", "triggered"].contains(&update.status.as_ref())
            || self
                .account
                .suppressed
                .get(&id)
                .is_some_and(|at| cx.now - *at < 120_000.0)
        {
            return;
        }
        let Some(mut order) = self.map_order(cx, &row) else {
            return;
        };
        self.account
            .records
            .retain(|_, r| cx.now - r.at < 1_800_000.0);
        let record = row
            .cloid
            .as_deref()
            .and_then(|c| self.account.records.get(c))
            .cloned();
        if let Some(record) = record {
            let native_fields = row.order_type == "Limit"
                || (row.trigger_px.is_some()
                    && matches!(
                        row.order_type.as_ref(),
                        "Stop Market" | "Stop Limit" | "Take Profit Market" | "Take Profit Limit"
                    ));
            if !native_fields {
                order.kind = record.order.kind;
                order.price = record.order.price;
            }
            order.reduce_only = record.order.reduce_only;
            order.time_in_force = order.time_in_force.or(record.order.time_in_force);
            self.account
                .preplacements
                .retain(|p| p.order.client_order_id != order.client_order_id);
        } else if let Some(pending) = self
            .account
            .pending
            .remove(&id)
            .filter(|p| cx.now - p.at < 60_000.0)
        {
            order.kind = pending.order.kind;
            order.price = pending.order.price;
        } else if row.order_type.is_empty() && row.trigger_px.is_none() {
            if let Some(old) = cx.store.order(&id) {
                order.kind = old.kind;
                order.price = old.price;
                order.time_in_force = order.time_in_force.or(old.time_in_force);
            } else if row.reduce_only && !row.is_trigger && !row.coin.contains(':') {
                self.schedule_orders(cx);
                return;
            } else if let Some(record) = self.match_preplacement(
                &symbol,
                order.side,
                row.limit_px,
                row.orig_sz,
                cx.now,
                !row.coin.contains(':'),
            ) {
                order.kind = record.order.kind;
                order.price = record.order.price;
                order.time_in_force = record.order.time_in_force;
            } else if row.coin.contains(':') {
                order.status = OrderStatus::Creating;
                self.schedule_orders(cx);
            }
        }
        order.time_in_force = order
            .time_in_force
            .or_else(|| cx.store.order(&id).and_then(|o| o.time_in_force));
        if order.kind == OrderType::Limit && order.time_in_force.is_none() {
            self.schedule_orders(cx);
        }
        cx.store.upsert_order(order);
    }
    fn match_preplacement(
        &mut self,
        symbol: &str,
        side: OrderSide,
        price: f64,
        amount: f64,
        now: f64,
        strict: bool,
    ) -> Option<Placement> {
        self.account.preplacements.retain(|r| now - r.at < 30_000.0);
        let eligible = |p: &Placement| p.order.symbol == symbol && p.order.side == side;
        if strict {
            let mut matches = self
                .account
                .preplacements
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    eligible(p)
                        && matches!(p.order.kind, OrderType::TakeProfit | OrderType::StopLoss)
                        && p.order.amount == amount
                });
            let index = matches.next().map(|(i, _)| i)?;
            if matches.next().is_some() {
                return None;
            }
            return self.account.preplacements.remove(index);
        }
        let exact = self
            .account
            .preplacements
            .iter()
            .position(|p| eligible(p) && p.order.price == price);
        let mut by_size = self
            .account
            .preplacements
            .iter()
            .enumerate()
            .filter(|(_, p)| eligible(p) && p.order.amount == amount);
        let first = by_size.next().map(|(i, _)| i);
        let unique = if by_size.next().is_none() {
            first
        } else {
            None
        };
        let index = exact
            .or(unique)
            .or_else(|| self.account.preplacements.iter().position(eligible))?;
        self.account.preplacements.remove(index)
    }
    fn fills(&mut self, cx: &mut Cx<'_, Pending>, mut fills: Fills<'_>) {
        fills
            .fills
            .sort_by(|a, b| a.time.total_cmp(&b.time).then(a.tid.cmp(&b.tid)));
        for fill in fills.fills {
            let Some(symbol) = self.symbol(&fill.coin).map(str::to_owned) else {
                continue;
            };
            let ps = if fill.dir.contains("Long") {
                Some(PositionSide::Long)
            } else if fill.dir.contains("Short") {
                Some(PositionSide::Short)
            } else {
                None
            };
            if self.account.seen_fills.insert(fill.tid) {
                self.account.fill_queue.push_back(fill.tid);
                if self.account.fill_queue.len() > 20_000
                    && let Some(id) = self.account.fill_queue.pop_front()
                {
                    self.account.seen_fills.remove(&id);
                }
                if let Some(ps) = ps {
                    let key = (symbol.clone(), ps);
                    if fill.dir.contains("Open") && fill.start_position == Some(0.0) {
                        self.account.realized.insert(key.clone(), 0.0);
                    }
                    if fill.dir.contains("Close") {
                        *self.account.realized.entry(key.clone()).or_default() +=
                            fill.closed_pnl.unwrap_or(0.) - fill.fee.unwrap_or(0.);
                        let realized = self.account.realized[&key];
                        cx.store.update_positions(|p| {
                            if p.symbol == symbol && p.side == ps {
                                p.realized_pnl = Some(realized);
                            }
                        });
                    }
                }
            }
            if fills.is_snapshot || (self.spot && self.account.spot_fill_tids.contains(&fill.tid)) {
                continue;
            }
            if self.account.notify_fills.insert(fill.tid) {
                self.account.notify_queue.push_back(fill.tid);
                if self.account.notify_queue.len() > 500
                    && let Some(id) = self.account.notify_queue.pop_front()
                {
                    self.account.notify_fills.remove(&id);
                }
                let event = Fill {
                    id: Some(fill.tid.to_string()),
                    order_id: fill.oid.to_string(),
                    symbol: symbol.clone(),
                    side: side(&fill.side),
                    price: fill.px,
                    amount: fill.sz,
                };
                if ps.is_some() && fill.dir.contains("Close") {
                    // Live execution events use net close PnL; historical
                    // executions retain their separate gross convention.
                    cx.fill_with_financials(
                        event,
                        FillFinancials {
                            realized_pnl: fill.closed_pnl.zip(fill.fee).map(|(pnl, fee)| pnl - fee),
                            fee: fill.fee,
                        },
                    );
                } else {
                    cx.fill(event);
                }
                if self.spot {
                    self.record_spot_fill(&fill, &symbol);
                    let id = fill.oid.to_string();
                    self.amend_source_filled(&id);
                    // An open-order read sent before this fill must not restore the order.
                    self.acknowledge_order_write(cx, &symbol);
                    if let Some(order) = cx.store.order(&id) {
                        let remaining = (order.remaining - fill.sz).max(0.0);
                        if remaining == 0.0 {
                            self.account.suppressed.insert(id.clone(), cx.now);
                            cx.store.remove_orders(&[&id]);
                        } else {
                            cx.store.update_order(&id, |o| {
                                o.remaining = remaining;
                                o.filled = (o.amount - remaining).max(0.0);
                            });
                        }
                    }
                    self.refresh_spot(cx, true);
                } else if cx
                    .store
                    .market(&symbol)
                    .is_some_and(|m| m.is_spot == Some(true))
                {
                    // A merged spot fill moves holdings, never a position (TS).
                    self.refresh_spot(cx, false);
                } else {
                    self.optimistic_fill(cx, &symbol, &fill);
                    // Partial fills send no orderUpdates: only a read shows what is left.
                    self.schedule_orders(cx);
                }
            }
        }
        self.live_balance(cx);
    }
    fn optimistic_fill(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str, fill: &FillRow<'_>) {
        if fill.coin.contains(':') {
            let dex = fill.coin.split_once(':').unwrap().0;
            self.account
                .position_time
                .entry(symbol.into())
                .and_modify(|time| *time = time.max(fill.time))
                .or_insert(fill.time);
            *self.account.state_revision.entry(dex.into()).or_default() += 1;
            self.account.state_dirty.insert(dex.into());
            if self.account.state_refresh_timer.is_none() {
                self.account.state_refresh_timer = Some(cx.set_timer(350.0));
            }
            return;
        }
        let Some(start) = fill.start_position else {
            return;
        };
        if self
            .account
            .snapshot_time
            .get(symbol)
            .is_some_and(|t| *t >= fill.time)
        {
            return;
        }
        if self
            .account
            .last_fill
            .get(symbol)
            .is_some_and(|(time, tid)| {
                *time > fill.time || (*time == fill.time && *tid >= fill.tid)
            })
        {
            return;
        }
        self.account
            .last_fill
            .insert(symbol.into(), (fill.time, fill.tid));
        let end = start + fill.sz * if fill.side == "B" { 1.0 } else { -1.0 };
        let old = cx
            .store
            .positions()
            .iter()
            .find(|p| p.symbol == symbol)
            .cloned();
        let mut positions = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol != symbol)
            .cloned()
            .collect::<Vec<_>>();
        if end.abs() > 1e-10 {
            let entry = if let Some(old) = &old {
                if start * end <= 0.0 {
                    fill.px
                } else if end.abs() > start.abs() {
                    (old.entry_price * start.abs() + fill.px * fill.sz) / end.abs()
                } else {
                    old.entry_price
                }
            } else {
                fill.px
            };
            let setting = cx.store.setting(symbol).unwrap_or(MarketSetting {
                leverage: 20.0,
                buy_leverage: None,
                sell_leverage: None,
                margin_type: Some(MarginType::Cross),
            });
            let ps = if end > 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            positions.push(Position {
                id: symbol.into(),
                symbol: symbol.into(),
                side: ps,
                entry_price: entry,
                contracts: end.abs(),
                notional: fill.px * end.abs(),
                leverage: setting.leverage,
                margin_type: setting.margin_type,
                position_mode: Some(PositionMode::Oneway),
                unrealized_pnl: (fill.px - entry) * end,
                realized_pnl: self.account.realized.get(&(symbol.into(), ps)).copied(),
                liquidation_price: old.as_ref().map_or(0.0, |p| p.liquidation_price),
                margin_size: old.and_then(|p| p.margin_size),
                last_update: Some(fill.time),
            });
        }
        cx.store.set_positions(positions);
        self.account.position_time.insert(symbol.into(), fill.time);
        *self
            .account
            .state_revision
            .entry(String::new())
            .or_default() += 1;
        if end.abs() <= 1e-10 {
            self.schedule_orders(cx);
        }
    }
}
pub(crate) fn side(side: &str) -> OrderSide {
    if side == "B" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub(crate) fn tif(tif: &str) -> Option<TimeInForce> {
    match tif {
        "Alo" => Some(TimeInForce::PostOnly),
        "Gtc" => Some(TimeInForce::GoodTillCancel),
        _ => None,
    }
}
/// Lowercase coin a perp symbol was built from, for a market no longer listed.
fn perp_coin(symbol: &str) -> String {
    symbol
        .strip_suffix("-PERP")
        .unwrap_or(symbol)
        .replacen('|', ":", 1)
        .to_ascii_lowercase()
}
/// Spot pairs are `@index` or `BASE/QUOTE`; perp coins are bare or `dex:COIN`.
fn spot_coin(coin: &str) -> bool {
    coin.starts_with('@') || coin.contains('/')
}
pub(crate) fn valid_cloid(id: &str) -> bool {
    id.len() == 34
        && id.starts_with("0x")
        && id[2..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn infer(row: &OrderRow<'_>) -> (OrderType, f64) {
    if row.order_type == "Limit" {
        return (OrderType::Limit, row.limit_px);
    }
    // A triggered stop reports triggerPx 0.0; its limit price is what it works at.
    let trigger = row.trigger_px.filter(|px| *px > 0.0);
    let price = trigger.unwrap_or(row.limit_px);
    // Live rows carry no tpsl, so "Take Profit Limit" is known only by its name.
    if matches!(
        row.order_type.as_ref(),
        "Take Profit Market" | "Take Profit Limit"
    ) || (trigger.is_some() && row.tpsl == "tp")
    {
        return (OrderType::TakeProfit, price);
    }
    if row.order_type == "Stop Market" || trigger.is_some() {
        return (OrderType::StopLoss, price);
    }
    if row.limit_px != 0.0 {
        (OrderType::Limit, row.limit_px)
    } else {
        (OrderType::Market, 0.0)
    }
}
pub(crate) fn terminal(status: &str) -> bool {
    status == "filled"
        || status == "canceled"
        || status == "rejected"
        || status == "scheduledCancel"
        || status.ends_with("Canceled")
        || status.ends_with("Rejected")
}

// Closed failure statuses; user, sibling-fill and scheduled cancels are normal.
// https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#query-order-status-by-oid-or-cloid
fn failed_protective_status(status: &str) -> bool {
    matches!(
        status,
        "rejected"
            | "tickRejected"
            | "minTradeNtlRejected"
            | "perpMarginRejected"
            | "reduceOnlyRejected"
            | "badAloPxRejected"
            | "iocCancelRejected"
            | "badTriggerPxRejected"
            | "marketOrderNoLiquidityRejected"
            | "positionIncreaseAtOpenInterestCapRejected"
            | "positionFlipAtOpenInterestCapRejected"
            | "tooAggressiveAtOpenInterestCapRejected"
            | "openInterestIncreaseRejected"
            | "insufficientSpotBalanceRejected"
            | "oracleRejected"
            | "perpMaxPositionRejected"
            | "marginCanceled"
            | "vaultWithdrawalCanceled"
            | "openInterestCapCanceled"
            | "selfTradeCanceled"
            | "reduceOnlyCanceled"
            | "delistedCanceled"
            | "liquidatedCanceled"
    )
}

/// The oldest epoch once every dex has a clean census: an all-symbol fence needs them all.
/// A read-cover scope no read commits: the account waits while dexes are
/// still being discovered.
const DEXES_UNSETTLED: &str = "\u{0}dexes-unsettled";

fn every_dex_clean(names: &[String], clean: &HashMap<String, u64>) -> Option<u64> {
    let main = clean.get("")?;
    names.iter().try_fold(*main, |oldest, name| {
        clean.get(name).map(|epoch| oldest.min(*epoch))
    })
}

impl Request {
    pub(crate) fn startup_sections(&self) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match self {
            Self::Role => S::ACCOUNT,
            Self::Mode => S::ACCOUNT,
            Self::State { .. } => S::BALANCE.union(S::POSITIONS),
            Self::Orders { .. } => S::ORDERS,
            Self::Spot { .. } => S::BALANCE,
            _ => S::NONE,
        }
    }
}

#[cfg(test)]
mod every_dex_clean_tests {
    use super::*;

    #[test]
    fn an_all_symbol_fence_waits_for_every_dex_then_takes_the_oldest_read() {
        let names = vec!["xyz".to_string(), "flx".to_string()];
        let mut clean = HashMap::new();
        clean.insert(String::new(), 5);
        assert_eq!(every_dex_clean(&names, &clean), None);
        clean.insert("xyz".to_string(), 3);
        assert_eq!(every_dex_clean(&names, &clean), None);
        clean.insert("flx".to_string(), 9);
        assert_eq!(every_dex_clean(&names, &clean), Some(3));
        assert_eq!(every_dex_clean(&[], &clean), Some(5));
    }
}
