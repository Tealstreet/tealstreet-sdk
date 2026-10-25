//! Private snapshots are staged until every streamed row and both order lists
//! succeed. Private events and acknowledged writes invalidate older snapshots.
use crate::account_wire::*;
use crate::{Pending, Task, WeexFutures};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderStatus, Position, PositionMode, PositionSide,
};
use safe_cex_engine::request_context::{Context, RetryPolicy};
use safe_cex_engine::selected_read::{
    BackgroundReadError, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::store::{MarketSetting, Store};
use safe_cex_engine::{Cx, Fill, FillFinancials, Method, Response, SocketId, TimerId};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::{HashMap, HashSet, VecDeque};

const TERMINAL_LIMIT: usize = 20_000;
const RECONCILIATION_DEADLINE_MS: f64 = 20_000.0;
const SKIP_LOG_LIMIT: usize = 4_096;
#[derive(Clone, Copy)]
pub enum Kind {
    Balance,
    Positions,
    Normal,
    Algo,
    Config,
    Metadata,
}
pub struct Request {
    pub(crate) context: Option<Context>,
    cycle: u64,
    kind: Kind,
    revision: u64,
    settings_stamp: safe_cex_engine::metadata::Observation,
    settings_revision: safe_cex_engine::metadata::Observation,
    page: u32,
    page_rows: usize,
    orders: Vec<Order>,
    positions: Vec<StagedPosition>,
    balance: Option<Balance>,
    configs: Vec<(String, Config)>,
    skipped_rows: usize,
    skipped: Skipped,
}
/// Order or position IDs of malformed rows a read skipped. A malformed row is
/// not evidence that its row closed; one without an ID protects every row.
#[derive(Default)]
struct Skipped {
    ids: Vec<String>,
    unidentified: bool,
}
impl Skipped {
    fn covers(&self, id: &str) -> bool {
        self.unidentified || self.ids.iter().any(|skipped| skipped == id)
    }
    fn extend(&mut self, other: Skipped) {
        self.ids.extend(other.ids);
        self.unidentified |= other.unidentified;
    }
    fn is_empty(&self) -> bool {
        !self.unidentified && self.ids.is_empty()
    }
}
/// Re-arms the skip logs under `prefix`: every one after a clean snapshot, else the mapped ids.
fn rearm_skips<'a>(
    logged: &mut HashMap<String, bool>,
    prefix: &str,
    skipped: &Skipped,
    mapped: impl Iterator<Item = &'a str>,
) {
    if skipped.is_empty() {
        logged.retain(|key, _| !key.starts_with(prefix));
    } else {
        for id in mapped {
            logged.remove(&format!("{prefix}{id}"));
        }
    }
}
#[derive(Clone)]
pub struct Config {
    pub setting: MarketSetting,
    pub hedged: bool,
    pub isolated_long_leverage: f64,
    pub isolated_short_leverage: f64,
}
/// Keep venue-provided fields distinct from settings fallbacks until every
/// startup/settings response has arrived. Response order cannot affect IDs.
struct StagedPosition {
    position: Position,
    leverage: bool,
    margin: bool,
    mode: bool,
}
impl StagedPosition {
    fn new(row: &PositionRow<'_>, store: &Store) -> Option<Self> {
        Some(Self {
            position: row.position(store)?,
            leverage: row.leverage.is_some(),
            margin: !row.margin_type.is_empty(),
            mode: !row.separated_mode.is_empty(),
        })
    }
    fn resolve(mut self, account: &State, store: &Store) -> Position {
        let p = &mut self.position;
        let config = account.configs.get(&p.symbol);
        if !self.margin {
            p.margin_type = config
                .and_then(|c| c.setting.margin_type)
                .or_else(|| store.setting(&p.symbol).and_then(|s| s.margin_type))
                .or(Some(MarginType::Cross));
        }
        if !self.leverage {
            p.leverage = config
                .map(|c| {
                    if p.margin_type == Some(MarginType::Isolated) {
                        if p.side == PositionSide::Short {
                            c.isolated_short_leverage
                        } else {
                            c.isolated_long_leverage
                        }
                    } else {
                        c.setting.leverage
                    }
                })
                .or_else(|| store.setting(&p.symbol).map(|s| s.leverage))
                .unwrap_or(1.0);
        }
        if !self.mode {
            p.position_mode = Some(if config.map_or(store.hedged(), |c| c.hedged) {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            });
        }
        p.id = if p.position_mode == Some(PositionMode::Hedged) {
            format!(
                "{}:{}",
                p.symbol,
                if p.side == PositionSide::Short {
                    "short"
                } else {
                    "long"
                }
            )
        } else {
            p.symbol.clone()
        };
        self.position
    }
}
#[derive(Default)]
pub struct State {
    started: bool,
    cycle: u64,
    deadline: Option<TimerId>,
    pending: u32,
    failed: bool,
    failures: u32,
    refresh: Option<TimerId>,
    revision: u64,
    orders_revision: u64,
    settings_revision: u64,
    order_stage: Vec<Order>,
    order_skipped: Skipped,
    /// Revision, staged rows, and IDs of positions whose rows were malformed.
    position_stage: Option<(u64, Vec<StagedPosition>, Skipped)>,
    deferred: Vec<(TimerId, Request)>,
    order_parts: u8,
    orders_failed: bool,
    pub configs: HashMap<String, Config>,
    settings_failures: u32,
    settings_retry_at: f64,
    tombstones: HashMap<String, f64>,
    terminal_order: VecDeque<(String, f64)>,
    order_floor: Option<f64>,
    position_updates: HashMap<String, f64>,
    socket: Option<SocketId>,
    next_id: u32,
    unreconciled_fills: bool,
    fills: HashSet<String>,
    fill_order: VecDeque<String>,
    /// `order:` or `position:` plus the ID (empty if none) of skipped rows already logged.
    skip_logged: HashMap<String, bool>,
}
impl State {
    pub fn new() -> Self {
        Self::default()
    }
    pub(crate) fn has_unreconciled_fills(&self) -> bool {
        self.unreconciled_fills
    }
    pub fn order_revision(&self) -> u64 {
        self.orders_revision
    }
    /// Settings writes also supersede position snapshots; keep this financial
    /// fence separate from engine-owned settings publication authority.
    pub fn settings_revision(&self) -> u64 {
        self.settings_revision
    }
    pub fn invalidate_settings(&mut self) {
        self.settings_revision += 1;
        self.revision += 1;
    }
    pub fn invalidate_orders(&mut self) {
        self.orders_revision += 1;
    }
    pub fn terminal(&mut self, id: String, time: f64) {
        self.remember_terminal(id, time);
        self.invalidate_orders();
    }
    fn remember_terminal(&mut self, id: String, time: f64) {
        if self.tombstones.contains_key(&id) {
            return;
        }
        self.tombstones.insert(id.clone(), time);
        self.terminal_order.push_back((id, time));
        while self.tombstones.len() > TERMINAL_LIMIT {
            if let Some((id, time)) = self.terminal_order.pop_front() {
                self.tombstones.remove(&id);
                self.order_floor = Some(self.order_floor.map_or(time, |floor| floor.max(time)));
            }
        }
    }
    fn below_order_floor(&self, time: f64) -> bool {
        self.order_floor.is_some_and(|floor| time <= floor)
    }
    fn snapshot_order(&self, order: Order) -> Option<Order> {
        // Callers accept a full authoritative snapshot only when its revision
        // is current. Its active rows may legitimately predate the WS floor.
        (!self.tombstones.contains_key(&order.id)
            && order
                .parent_id
                .as_ref()
                .is_none_or(|id| !self.tombstones.contains_key(id)))
        .then_some(order)
    }
}
/// The store ID a malformed row would have produced, when it can be known.
fn row_identity(kind: Kind, item: &str, account: &State, store: &Store) -> Option<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OrderIdentity<'a> {
        #[serde(borrow, deserialize_with = "id", alias = "id", alias = "algoId")]
        order_id: crate::wire::Str<'a>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PositionIdentity<'a> {
        #[serde(borrow)]
        symbol: crate::wire::Str<'a>,
        #[serde(borrow, default, alias = "positionSide")]
        side: Option<crate::wire::Str<'a>>,
        #[serde(borrow, default, alias = "separatedType")]
        separated_mode: Option<crate::wire::Str<'a>>,
    }
    if !matches!(kind, Kind::Positions) {
        let id = serde_json::from_str::<OrderIdentity<'_>>(item)
            .ok()?
            .order_id;
        return (!id.is_empty()).then(|| id.into_owned());
    }
    let row = serde_json::from_str::<PositionIdentity<'_>>(item).ok()?;
    let side = match row.side.as_deref()? {
        "LONG" => "long",
        "SHORT" => "short",
        _ => return None,
    };
    if row.symbol.is_empty() {
        return None;
    }
    // Mirrors StagedPosition::resolve: row mode, else symbol config, else account.
    let hedged = match row.separated_mode.as_deref().filter(|m| !m.is_empty()) {
        Some(m) => mode(m) == PositionMode::Hedged,
        None => account
            .configs
            .get(row.symbol.as_ref())
            .map_or(store.hedged(), |c| c.hedged),
    };
    Some(if hedged {
        format!("{}:{side}", row.symbol)
    } else {
        row.symbol.into_owned()
    })
}
impl WeexFutures {
    pub(crate) fn start_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.started {
            return;
        }
        self.account.started = true;
        if self.incomplete_credentials {
            // An empty account would be indistinguishable from a real one.
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "WEEX requires an API key, secret and passphrase; private data is unavailable",
            );
            cx.deactivate();
            return;
        }
        if self.credentials.primary.is_none() {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            return;
        }
        self.account_cycle(cx);
        if self.private_ws_headers {
            self.connect_account(cx);
        } else {
            cx.notice(Level::Warn, code::STREAM_UNAVAILABLE,"WEEX private WebSocket disabled: transport cannot attach upgrade headers; continuing REST safety loop");
        }
    }
    pub(crate) fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.account.started {
            self.start_account(cx);
            return;
        }
        if self.credentials.primary.is_none() || self.account.pending > 0 {
            return;
        }
        if let Some(timer) = self.account.refresh.take() {
            cx.clear_timer(timer);
        }
        self.account_cycle(cx);
    }
    pub(crate) fn connect_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if let (Some(primary), Some(feed)) = (&self.credentials.primary, self.sockets.private) {
            let step = crate::dispatch_signing::Upgrade::new(primary.clone());
            if let Ok(epoch) = cx.connect_socket_signed(feed, step.unsigned(), Box::new(step)) {
                self.account.socket = Some(epoch.socket());
            }
        }
    }
    pub(crate) fn account_ping(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.account.next_id += 1;
        cx.send(
            socket,
            format!(r#"{{"method":"PING","id":{}}}"#, self.account.next_id),
        );
    }
    fn account_cycle(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.pending != 0 {
            return;
        }
        self.account.cycle += 1;
        self.account.deadline =
            Some(cx.set_timer((self.paused_until - cx.now).max(0.0) + RECONCILIATION_DEADLINE_MS));
        // Settings reads back off on their own failures; account reads keep polling.
        let settings_due = cx.now >= self.account.settings_retry_at;
        self.account.pending = if settings_due { 6 } else { 4 };
        self.account.failed = false;
        self.account.order_parts = 0;
        self.account.orders_failed = false;
        self.account.order_stage.clear();
        self.account.order_skipped = Skipped::default();
        self.account.position_stage = None;
        for kind in [
            Kind::Metadata,
            Kind::Config,
            Kind::Balance,
            Kind::Positions,
            Kind::Normal,
            Kind::Algo,
        ] {
            if !settings_due && matches!(kind, Kind::Config | Kind::Metadata) {
                continue;
            }
            let revision = match kind {
                Kind::Normal | Kind::Algo => self.account.orders_revision,
                Kind::Config | Kind::Metadata => 0,
                _ => self.account.revision,
            };
            let request = Request {
                context: None,
                cycle: self.account.cycle,
                kind,
                revision,
                settings_stamp: cx.store.metadata_observation(),
                settings_revision: cx.store.metadata_revision(),
                page: if matches!(kind, Kind::Algo) { 1 } else { 0 },
                page_rows: 0,
                orders: vec![],
                positions: vec![],
                balance: None,
                configs: vec![],
                skipped_rows: 0,
                skipped: Skipped::default(),
            };
            self.send_account(cx, request);
        }
    }
    fn send_account(&mut self, cx: &mut Cx<'_, Pending>, mut request: Request) {
        if request.cycle != self.account.cycle {
            return;
        }
        if cx.now < self.paused_until {
            // A rate limit discovered after this cycle began can defer a
            // later page past its original deadline. Its read budget begins
            // when that page may actually leave the queue.
            if let Some(deadline) = self.account.deadline.take() {
                cx.clear_timer(deadline);
            }
            self.account.deadline =
                Some(cx.set_timer(self.paused_until - cx.now + RECONCILIATION_DEADLINE_MS));
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.account.deferred.push((timer, request));
            return;
        }
        let first_page = if matches!(request.kind, Kind::Algo) {
            1
        } else {
            0
        };
        if matches!(request.kind, Kind::Normal | Kind::Algo) && request.page > first_page {
            // Continuation pages each receive the same bounded HTTP budget.
            // A healthy multi-page snapshot may take more than one budget.
            if let Some(deadline) = self.account.deadline.take() {
                cx.clear_timer(deadline);
            }
            self.account.deadline = Some(cx.set_timer(RECONCILIATION_DEADLINE_MS));
        }
        let path = match request.kind {
            Kind::Balance => "/capi/v3/account/balance",
            Kind::Positions => "/capi/v3/account/position/allPosition",
            Kind::Normal => "/capi/v3/openOrders",
            Kind::Algo => "/capi/v3/openAlgoOrders",
            Kind::Config => "/capi/v3/account/symbolConfig",
            Kind::Metadata => "/capi/v3/account/accountConfig",
        };
        let query = if matches!(request.kind, Kind::Normal | Kind::Algo) {
            format!("limit=100&page={}", request.page)
        } else {
            String::new()
        };
        let stream = (!matches!(request.kind, Kind::Metadata)).then_some(ArrayAt::Root);
        let selected = SelectedRead::select(
            &self.slots,
            ReadRole::Read {
                scope: Some(request.kind.auth_scope()),
            },
        );
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
            Err(error) => {
                let message = match error {
                    BackgroundReadError::Selection(_) => {
                        "WEEX account read requires credentials".into()
                    }
                    BackgroundReadError::Admission(error) => error.read_venue_error().message,
                };
                self.account_failed(cx, request, &message);
                return;
            }
        };
        let context = read.context().clone();
        let path_query = if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{query}")
        };
        request.context = Some(context.clone());
        match read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: format!("{}{path_query}", crate::paths::REST),
                body: None,
                stream,
            },
            |identity| {
                Box::new(crate::dispatch_signing::Http::new(
                    identity.clone(),
                    Method::Get,
                    path_query,
                    None,
                ))
            },
            |_| Pending {
                task: Task::Account(request),
                invalid: false,
                skipped: vec![],
                seed_generation: self.seed_generation,
            },
        ) {
            Ok(_) => {}
            Err((pending, error)) => {
                let _ = cx.finish_http_context(&context);
                let Task::Account(request) = pending.task else {
                    unreachable!()
                };
                self.account_failed(cx, request, &error.read_venue_error().message);
            }
        }
    }
    pub(crate) fn account_delivery_current(&self, cx: &Cx<'_, Pending>, request: &Request) -> bool {
        request.cycle == self.account.cycle
            && request
                .context
                .as_ref()
                .is_some_and(|context| cx.validate_background_read_http_delivery(context).is_ok())
    }
    pub(crate) fn account_request_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        seed: u32,
        error: &str,
    ) {
        if let Some(context) = request.context.take() {
            let _ = cx.finish_http_context(&context);
        }
        if seed == self.seed_generation {
            self.account_failed(cx, request, error);
        }
    }

    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) -> bool {
        if !self.account_delivery_current(cx, request) {
            return true;
        }
        request.page_rows += 1;
        let parsed = match request.kind {
            Kind::Normal | Kind::Algo => serde_json::from_str::<OrderRow<'_>>(item)
                .ok()
                .map(|row| {
                    if matches!(request.kind, Kind::Algo) {
                        row.algo()
                    } else {
                        row.normal().into_iter().collect()
                    }
                })
                .filter(|orders| !orders.is_empty())
                .map(|orders| request.orders.extend(orders)),
            Kind::Balance => serde_json::from_str::<BalanceRow<'_>>(item)
                .map(|row| {
                    if row.asset == "USDT" {
                        request.balance = Some(row.balance());
                    }
                })
                .ok(),
            Kind::Positions => serde_json::from_str::<PositionRow<'_>>(item)
                .ok()
                .and_then(|row| StagedPosition::new(&row, cx.store))
                .map(|p| request.positions.push(p)),
            Kind::Config => serde_json::from_str::<ConfigRow<'_>>(item)
                .map(|row| {
                    request.configs.push((
                        row.symbol.to_string(),
                        Config {
                            setting: row.setting(),
                            hedged: row.separated_type == "SEPARATED",
                            isolated_long_leverage: row.isolated_long_leverage,
                            isolated_short_leverage: row.isolated_short_leverage,
                        },
                    ));
                })
                .ok(),
            Kind::Metadata => return false,
        };
        if parsed.is_none() {
            // Balance and config rows need no identity: a malformed USDT row
            // fails the read, and a skipped config keeps its previous entry.
            match (
                request.kind,
                row_identity(request.kind, item, &self.account, cx.store),
            ) {
                (Kind::Balance | Kind::Config, _) => {}
                (_, Some(key)) => request.skipped.ids.push(key),
                (_, None) => request.skipped.unidentified = true,
            }
            request.skipped_rows += 1;
        }
        true
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        response: Response<'_>,
    ) {
        if request.cycle != self.account.cycle {
            return;
        }
        if response.status >= 400 {
            self.account_failed(cx, request, &format!("HTTP {}", response.status));
            return;
        }
        if matches!(request.kind, Kind::Normal | Kind::Algo) && request.page_rows == 100 {
            if request.page >= 1000 {
                self.account_failed(
                    cx,
                    request,
                    "order paging limit reached; retaining previous snapshot",
                );
                return;
            }
            request.page += 1;
            request.page_rows = 0;
            if let Some(context) = request.context.take() {
                let _ = cx.finish_http_context(&context);
            }
            self.send_account(cx, request);
            return;
        }
        if request.skipped_rows > 0 {
            match request.kind {
                Kind::Normal | Kind::Algo | Kind::Positions => self.log_skipped_rows(cx, &request),
                Kind::Balance | Kind::Config | Kind::Metadata => {
                    let kind = if matches!(request.kind, Kind::Balance) {
                        "balance"
                    } else {
                        "symbol configuration"
                    };
                    let count = request.skipped_rows;
                    cx.notice(
                        Level::Warn,
                        code::ROW_SKIPPED,
                        format!("WEEX skipped {count} malformed {kind} row(s)"),
                    );
                }
            }
        }
        match request.kind {
            // The next cycle corrects a read that raced an account event; fencing it
            // starved the balance whenever events kept arriving.
            Kind::Balance => {
                // Missing USDT on a nonempty response is not a zero balance.
                if let Some(balance) = request.balance {
                    cx.store.set_balance(balance);
                } else if request.page_rows == 0 {
                    cx.store.set_balance(Balance::default());
                } else {
                    self.account_failed(cx, request, "USDT balance missing");
                    return;
                }
            }
            Kind::Positions if request.revision == self.account.revision => {
                self.account.position_stage =
                    Some((request.revision, request.positions, request.skipped));
            }
            Kind::Config => {
                // The native response selects the last valid row for a symbol.
                // Consolidate before the one observed commit: a repeated stamp
                // is deliberately ineligible after that commit.
                let configs: HashMap<_, _> = request.configs.into_iter().collect();
                for (symbol, config) in configs {
                    if let Ok(setting) =
                        safe_cex_engine::metadata::ObservedSetting::new(config.setting)
                        && cx
                            .store
                            .observe_setting(&symbol, &request.settings_stamp, setting)
                    {
                        self.account.configs.insert(symbol, config);
                    }
                }
            }
            Kind::Metadata
                if cx
                    .store
                    .metadata_revision_current(&request.settings_revision) =>
            {
                let Ok(row) = serde_json::from_str::<AccountConfig>(response.body) else {
                    self.account_failed(cx, request, "malformed account configuration");
                    return;
                };
                cx.store.set_hedged(row.dual_side_position);
                cx.store
                    .update_markets(|market| market.is_read_only = Some(!row.can_trade));
            }
            Kind::Normal | Kind::Algo => {
                if request.revision != self.account.orders_revision {
                    self.account.orders_failed = true;
                }
                self.account.order_parts |= if matches!(request.kind, Kind::Normal) {
                    1
                } else {
                    2
                };
                self.account.order_stage.extend(request.orders);
                self.account.order_skipped.extend(request.skipped);
                if self.account.order_parts == 3 && !self.account.orders_failed {
                    let mut seen = HashSet::new();
                    let mut orders: Vec<_> = std::mem::take(&mut self.account.order_stage)
                        .into_iter()
                        .filter(|order| seen.insert(order.id.clone()))
                        .filter_map(|order| self.account.snapshot_order(order))
                        .collect();
                    let skipped = std::mem::take(&mut self.account.order_skipped);
                    rearm_skips(
                        &mut self.account.skip_logged,
                        "order:",
                        &skipped,
                        orders.iter().filter_map(|o| o.id.split("__").next()),
                    );
                    orders.extend(
                        cx.store
                            .orders()
                            .iter()
                            .filter(|o| {
                                !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                                    && o.id.split("__").next().is_some_and(|id| skipped.covers(id))
                                    && seen.insert(o.id.clone())
                            })
                            .filter_map(|o| self.account.snapshot_order(o.clone())),
                    );
                    cx.store.set_orders(orders);
                    self.account.unreconciled_fills = false;
                    // A fresh authoritative read also replaces optimistic
                    // ownership; an older failed write must not roll it back.
                    self.account.invalidate_orders();
                }
            }
            _ => {}
        }
        if matches!(request.kind, Kind::Config | Kind::Metadata) {
            self.account.settings_failures = 0;
        }
        self.account_finished(cx, false);
    }
    /// Logs skipped rows once, allowing one Error upgrade if a kept copy is lost.
    /// A skip without a retained copy is an error: the row reads as absent.
    fn log_skipped_rows(&mut self, cx: &mut Cx<'_, Pending>, request: &Request) {
        let positions = matches!(request.kind, Kind::Positions);
        let (prefix, kind) = match request.kind {
            Kind::Positions => ("position:", "position"),
            Kind::Algo => ("order:", "algo order"),
            _ => ("order:", "order"),
        };
        let stored = |id: Option<&str>| {
            if positions {
                id.map_or(!cx.store.positions().is_empty(), |id| {
                    cx.store.position(id).is_some()
                })
            } else {
                cx.store.orders().iter().any(|o| {
                    !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled)
                        && id.is_none_or(|id| o.id.split("__").next() == Some(id))
                        && self.account.snapshot_order(o.clone()).is_some()
                })
            }
        };
        let mut rows: Vec<_> = request
            .skipped
            .ids
            .iter()
            .map(|id| {
                let error = !stored(Some(id));
                (
                    id.as_str(),
                    error,
                    format!("WEEX skipped malformed {kind} row {id}"),
                )
            })
            .collect();
        if request.skipped.unidentified {
            let message = format!("WEEX skipped malformed {kind} row without an ID");
            rows.push(("", !stored(None), message));
        }
        for (id, error, message) in rows {
            let logged = &mut self.account.skip_logged;
            if logged.len() >= SKIP_LOG_LIMIT {
                logged.clear();
            }
            let key = format!("{prefix}{id}");
            if logged.get(&key).is_some_and(|&errored| errored || !error) {
                continue;
            }
            logged.insert(key, error);
            let level = if error { Level::Error } else { Level::Warn };
            cx.notice(level, code::ROW_SKIPPED, message);
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        if request.cycle != self.account.cycle {
            return;
        }
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            format!("WEEX private read: {error}; retaining last-known-good state"),
        );
        if matches!(request.kind, Kind::Normal | Kind::Algo) {
            self.account.orders_failed = true;
            self.account.order_stage.clear();
            self.account.order_skipped = Skipped::default();
        }
        if matches!(request.kind, Kind::Config | Kind::Metadata) {
            self.account.settings_failures += 1;
            let delay = Backoff::new(2_000.0, 5, 60_000.0).delay(self.account.settings_failures);
            self.account.settings_retry_at = cx.now + cx.jittered(delay);
            self.account_finished(cx, false);
            return;
        }
        self.account_finished(cx, true);
    }
    fn account_finished(&mut self, cx: &mut Cx<'_, Pending>, failed: bool) {
        self.account.failed |= failed;
        self.account.pending = self.account.pending.saturating_sub(1);
        if self.account.pending == 0 {
            if let Some(deadline) = self.account.deadline.take() {
                cx.clear_timer(deadline);
            }
            if let Some((revision, rows, skipped)) = self.account.position_stage.take()
                && revision == self.account.revision
            {
                let mut positions: Vec<_> = rows
                    .into_iter()
                    .map(|p| p.resolve(&self.account, cx.store))
                    .filter_map(|p| {
                        if self
                            .account
                            .position_updates
                            .get(&p.id)
                            .is_some_and(|time| *time > p.last_update.unwrap_or(0.0))
                            || cx
                                .store
                                .position(&p.id)
                                .and_then(|old| old.last_update)
                                .is_some_and(|time| time > p.last_update.unwrap_or(0.0))
                        {
                            cx.store.position(&p.id).cloned()
                        } else {
                            (p.contracts != 0.0).then_some(p)
                        }
                    })
                    .collect();
                rearm_skips(
                    &mut self.account.skip_logged,
                    "position:",
                    &skipped,
                    positions.iter().map(|p| p.id.as_str()),
                );
                positions.extend(
                    cx.store
                        .positions()
                        .iter()
                        .filter(|old| {
                            skipped.covers(&old.id)
                                && !positions.iter().any(|p: &Position| p.id == old.id)
                        })
                        .cloned()
                        .collect::<Vec<_>>(),
                );
                cx.store.set_positions(positions);
            }
            if self.account.failed {
                self.account.failures += 1;
            } else {
                self.account.failures = 0;
            }
            let not_before_ms = self.paused_until.max(if self.account.failed {
                // Only a failed refresh retries with jitter; the pause floor is never shortened.
                cx.now
                    + cx.jittered(Backoff::new(2_000.0, 5, 60_000.0).delay(self.account.failures))
            } else {
                f64::NEG_INFINITY
            });
            self.account.refresh = Some(cx.set_poll_timer_with_bounds(
                2_000.0,
                safe_cex_engine::PollTimerBounds {
                    not_before_ms,
                    ..Default::default()
                },
            ));
        }
    }
    pub(crate) fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.next_id += 1;
        cx.send(socket,format!(r#"{{"method":"SUBSCRIBE","params":["account","positions","orders","fill"],"id":{}}}"#,self.account.next_id));
        self.refresh_account(cx);
        true
    }
    pub(crate) fn account_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        #[derive(Deserialize)]
        struct Event<'a> {
            #[serde(borrow, default)]
            e: &'a str,
            #[serde(borrow, default, rename = "type", alias = "event")]
            kind: &'a str,
            #[serde(default)]
            result: Option<bool>,
            #[serde(borrow, default)]
            d: Option<&'a RawValue>,
        }
        let Ok(event) = serde_json::from_str::<Event<'_>>(text) else {
            return true;
        };
        if event.kind == "ping" {
            self.account.next_id += 1;
            cx.send(
                socket,
                format!(r#"{{"method":"PONG","id":{}}}"#, self.account.next_id),
            );
            return true;
        }
        if event.result == Some(false) {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                "WEEX private subscription failed; continuing REST safety loop",
            );
            return true;
        }
        let Some(data) = event.d else {
            return true;
        };
        match event.e {
            "positions" => {
                self.account.revision += 1;
                if let Ok(rows) = serde_json::from_str::<Vec<PositionRow<'_>>>(data.get()) {
                    for row in rows {
                        if let Some(position) = StagedPosition::new(&row, cx.store)
                            .map(|p| p.resolve(&self.account, cx.store))
                        {
                            let time = position.last_update.unwrap_or(0.0);
                            if self
                                .account
                                .position_updates
                                .get(&position.id)
                                .is_none_or(|old| time >= *old)
                                && cx
                                    .store
                                    .position(&position.id)
                                    .and_then(|old| old.last_update)
                                    .is_none_or(|old| time >= old)
                            {
                                self.account
                                    .position_updates
                                    .insert(position.id.clone(), time);
                                cx.store.upsert_position(position);
                            }
                        }
                    }
                }
            }
            "orders" => {
                self.account.invalidate_orders();
                if let Ok(rows) = serde_json::from_str::<Vec<OrderRow<'_>>>(data.get()) {
                    for row in rows {
                        if let Some(order) = row.normal() {
                            if self.account.below_order_floor(order.last_update) {
                                continue;
                            }
                            if self.account.tombstones.contains_key(&order.id)
                                && !matches!(
                                    order.status,
                                    OrderStatus::Closed | OrderStatus::Canceled
                                )
                            {
                                continue;
                            }
                            if cx
                                .store
                                .order(&order.id)
                                .is_some_and(|old| old.last_update > order.last_update)
                            {
                                continue;
                            }
                            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                                self.account
                                    .remember_terminal(order.id.clone(), order.last_update);
                            }
                            cx.store.upsert_order(order);
                        }
                    }
                }
            }
            "fill" => {
                if let Ok(rows) = serde_json::from_str::<Vec<FillRow<'_>>>(data.get()) {
                    for row in rows {
                        if row.id.is_empty()
                            || row.order_id.is_empty()
                            || row.symbol.is_empty()
                            || row.fill_size <= 0.0
                            || row.fill_value <= 0.0
                            || side(&row.order_side).is_none()
                            || !self.account.fills.insert(row.id.to_string())
                        {
                            continue;
                        }
                        // The execution can precede its cumulative order
                        // update. Replacements wait for an authoritative read.
                        self.account.unreconciled_fills = true;
                        self.account.fill_order.push_back(row.id.to_string());
                        if self.account.fill_order.len() > 20_000
                            && let Some(old) = self.account.fill_order.pop_front()
                        {
                            self.account.fills.remove(&old);
                        }
                        if let Some(side) = side(&row.order_side) {
                            cx.fill_with_financials(
                                Fill {
                                    id: None,
                                    order_id: row.order_id.to_string(),
                                    symbol: row.symbol.to_string(),
                                    side,
                                    price: row.fill_value / row.fill_size,
                                    amount: row.fill_size,
                                },
                                FillFinancials {
                                    realized_pnl: row.realize_pnl,
                                    fee: row.fill_fee.map(f64::abs),
                                },
                            );
                        }
                    }
                }
                self.account.invalidate_orders();
            }
            _ => {}
        }
        true
    }
    pub(crate) fn account_close(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        _code: u16,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.socket = None;
        true
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.deadline == Some(timer) {
            self.account.deadline = None;
            // Expired HTTP streams may still finish later. Their generation
            // cannot publish rows, send another page, or finish a newer cycle.
            self.account.cycle += 1;
            self.account.order_stage.clear();
            self.account.order_skipped = Skipped::default();
            self.account.position_stage = None;
            self.account.orders_failed = true;
            for (timer, _) in self.account.deferred.drain(..) {
                cx.clear_timer(timer);
            }
            cx.notice(Level::Warn, code::REQUEST_FAILED, "WEEX private reconciliation timed out; retaining last-known-good snapshots and retrying reads");
            self.account.pending = 1;
            self.account_finished(cx, true);
            return true;
        }
        if let Some(index) = self
            .account
            .deferred
            .iter()
            .position(|(id, _)| *id == timer)
        {
            let (_, request) = self.account.deferred.swap_remove(index);
            self.send_account(cx, request);
            return true;
        }
        if self.account.refresh == Some(timer) {
            self.account.refresh = None;
            self.account_cycle(cx);
            return true;
        }
        false
    }
}

impl Kind {
    fn auth_scope(self) -> safe_cex_engine::AccountReadScope {
        use safe_cex_engine::AccountReadScope::*;
        match self {
            Self::Balance => Balance,
            Self::Positions => Positions,
            Self::Normal | Self::Algo => Orders,
            Self::Config | Self::Metadata => Settings,
        }
    }
}
impl WeexFutures {
    /// One endpoint's rejection holds back its scope siblings' acceptance.
    pub(crate) fn account_auth(
        &self,
        pending: &Pending,
        response: &Response<'_>,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        if matches!(pending.task, Task::Account(_))
            && pending.seed_generation != self.seed_generation
        {
            return None;
        }
        let outcome = self.account_auth_evidence(pending, response);
        let Task::Account(r) = &pending.task else {
            return outcome;
        };
        match r.kind {
            Kind::Normal => self.orders_auth.report(0, outcome),
            Kind::Algo => self.orders_auth.report(1, outcome),
            Kind::Config => self.settings_auth.report(0, outcome),
            Kind::Metadata => self.settings_auth.report(1, outcome),
            Kind::Balance | Kind::Positions => outcome,
        }
    }
    fn account_auth_evidence(
        &self,
        pending: &Pending,
        response: &Response<'_>,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        use safe_cex_engine::AuthOutcome::{Accepted, Rejected};
        let Task::Account(r) = &pending.task else {
            return None;
        };
        if r.cycle != self.account.cycle {
            return None;
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Code {
            Number(i64),
            Text(String),
        }
        #[derive(Deserialize)]
        struct Evidence {
            code: Code,
        }
        if let Ok(e) = serde_json::from_str::<Evidence>(response.body) {
            let code = match e.code {
                Code::Number(n) => Some(n),
                Code::Text(s) => s.parse().ok(),
            };
            if matches!(code, Some(40006 | 40009 | 40012 | 40014 | 40018)) {
                return Some(Rejected);
            }
            return None;
        }
        if !(200..300).contains(&response.status) {
            return None;
        }
        match r.kind {
            Kind::Metadata => serde_json::from_str::<AccountConfig>(response.body)
                .is_ok()
                .then_some(Accepted),
            _ => safe_cex_engine::AuthOutcome::accepted_list(response, ArrayAt::Root),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_terminal_memory_keeps_ws_floor_but_accepts_old_authoritative_active_rows() {
        let mut state = State::new();
        for i in 0..=TERMINAL_LIMIT {
            state.terminal((i + 1).to_string(), 1_773_295_700_000.0 + i as f64);
        }
        assert_eq!(state.tombstones.len(), TERMINAL_LIMIT);
        assert_eq!(state.terminal_order.len(), TERMINAL_LIMIT);
        assert!(!state.tombstones.contains_key("1"));
        assert!(state.below_order_floor(1_773_295_700_000.0));
        let raw = r#"{"orderId":"old-active","symbol":"BTCUSDT","side":"BUY","type":"LIMIT","status":"NEW","origQty":"1","updateTime":1773295700000}"#;
        let old_active = serde_json::from_str::<OrderRow<'_>>(raw)
            .unwrap()
            .normal()
            .unwrap();
        assert_eq!(state.snapshot_order(old_active.clone()), Some(old_active));
        let raw = raw.replace("old-active", "2");
        let terminal = serde_json::from_str::<OrderRow<'_>>(&raw)
            .unwrap()
            .normal()
            .unwrap();
        assert!(state.snapshot_order(terminal).is_none());
    }
}

#[cfg(test)]
mod auth_envelopes {
    use super::*;
    use safe_cex_engine::AuthOutcome::{Accepted, Rejected};

    fn pending(venue: &WeexFutures, kind: Kind) -> Pending {
        Pending {
            task: Task::Account(Request {
                context: None,
                cycle: venue.account.cycle,
                kind,
                revision: 0,
                settings_stamp: Default::default(),
                settings_revision: Default::default(),
                page: 0,
                page_rows: 0,
                orders: vec![],
                positions: vec![],
                balance: None,
                configs: vec![],
                skipped_rows: 0,
                skipped: Skipped::default(),
            }),
            invalid: false,
            skipped: vec![],
            seed_generation: 0,
        }
    }

    fn response(body: &str) -> Response<'_> {
        Response {
            status: 200,
            headers: "",
            body,
        }
    }

    #[test]
    fn unavailable_account_selection_keeps_the_credential_refusal_text() {
        let venue = WeexFutures::new(crate::Options::default());
        let probe = safe_cex_engine::test_support::ProbeVenue::new(venue, |venue, cx| {
            // Exercise the defensive branch; production account admission
            // requires an immutable configured primary before send_account.
            let Task::Account(request) = pending(venue, Kind::Balance).task else {
                unreachable!()
            };
            venue.account.pending = 1;
            venue.send_account(cx, request);
        });
        let mut engine = safe_cex_engine::test_support::seeded_engine(probe);
        engine.start(1000.);
        let actions: Vec<_> = engine.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(
            a,
            safe_cex_engine::Action::Http(_)
                | safe_cex_engine::Action::ManagedHttp(_)
                | safe_cex_engine::Action::Sign { .. }
                | safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Deactivate)
        )));
        let messages: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Notice(safe_cex_engine::Notice::Event(event)) => {
                    Some(event.message.as_str())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            messages,
            [
                "WEEX private read: WEEX account read requires credentials; retaining last-known-good state"
            ]
        );
    }

    #[test]
    fn stale_seed_account_auth_evidence_is_inert() {
        let mut venue = WeexFutures::new(crate::Options::default());
        let old = pending(&venue, Kind::Balance);
        venue.seed_generation += 1;
        let mut current = pending(&venue, Kind::Balance);
        current.seed_generation = venue.seed_generation;
        assert_eq!(
            venue.account_auth(&current, &response("[]")),
            Some(Accepted)
        );
        let outcomes: Vec<_> = [
            (
                200,
                r#"[{"asset":"USDT","balance":"100","availableBalance":"90"}]"#,
            ),
            (401, r#"{"code":40014}"#),
        ]
        .into_iter()
        .map(|(status, body)| {
            venue.account_auth(
                &old,
                &Response {
                    status,
                    headers: "",
                    body,
                },
            )
        })
        .collect();
        assert_eq!(outcomes, [None, None]);
    }

    #[test]
    fn malformed_success_cannot_clear_grouped_orders_or_settings_evidence() {
        for (kind, sibling, success, sibling_success) in [
            (Kind::Algo, Kind::Normal, "[]", "[]"),
            (
                Kind::Config,
                Kind::Metadata,
                "[]",
                r#"{"dualSidePosition":false,"canTrade":true}"#,
            ),
        ] {
            for bad in ["{}", "null", "false", "[false"] {
                let venue = WeexFutures::new(crate::Options::default());
                let p = pending(&venue, kind);
                let sibling = pending(&venue, sibling);
                assert_eq!(
                    venue.account_auth(&p, &response(r#"{"code":40014}"#)),
                    Some(Rejected)
                );
                assert_eq!(venue.account_auth(&p, &response(bad)), None);
                assert_eq!(
                    venue.account_auth(&sibling, &response(sibling_success)),
                    None
                );
                assert_eq!(venue.account_auth(&p, &response(success)), Some(Accepted));
            }
        }
    }
}
