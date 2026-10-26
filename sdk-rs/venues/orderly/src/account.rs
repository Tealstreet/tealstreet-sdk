use crate::{Connection, Orderly, Pending, Seed, sign, symbol, wire::*};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowAdmission, SnapshotBuilder};
use safe_cex_engine::execution::{Arithmetic, Cursor};
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::RetryPolicy;
use safe_cex_engine::selected_read::{
    BackgroundReadError, ReadPolicy, ReadRequest, ReadRole, SelectedRead,
};
use safe_cex_engine::socket::Tick as SocketTick;
use safe_cex_engine::{Cx, Fill, Method, Response, SocketId, TimerId};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Orderly's account reads never poll faster than once a second, however the
/// host scales the cadence.
const POLL_FLOOR: safe_cex_engine::PollTimerBounds = safe_cex_engine::PollTimerBounds {
    minimum_interval_ms: 1000.0,
    not_before_ms: f64::NEG_INFINITY,
};
const STALE_SNAPSHOT_LIMIT: u8 = 2;

pub enum Request {
    Info,
    Modes {
        rows: SnapshotBuilder<String, (String, MarginType)>,
        invalid: bool,
        revision: u64,
    },
    Positions {
        rows: Vec<Position>,
        invalid: bool,
        revision: u64,
        issued: u64,
        stamps: HashMap<String, (Option<u64>, u64)>,
        coverage: RowAdmission<String>,
        /// Each skipped row's symbol and PnL, as far as they parsed.
        skipped: Vec<(Option<String>, Option<f64>)>,
    },
    Orders {
        algo: bool,
        rows: Vec<Order>,
        proofs: HashMap<String, Proof>,
        invalid: bool,
        revision: u64,
        page: u32,
        seen: HashSet<String>,
        page_count: u32,
        stamps: HashMap<String, (Option<u64>, u64)>,
        coverage: RowAdmission<String>,
    },
}
impl Request {
    pub(crate) fn is_info(&self) -> bool {
        matches!(self, Self::Info)
    }
}
#[derive(Clone)]
pub struct Proof {
    pub mode: MarginType,
    pub amend: bool,
    /// Only an explicit standalone positional CLOSE_POSITION leg may be rebuilt.
    pub positional_repair: bool,
    pub close_compatible: bool,
    pub mode_known: bool,
}
struct MappedOrder<'r, 'a> {
    order: Order,
    row: &'r OrderRow<'a>,
    proof: Proof,
    seq: Option<u64>,
    timestamp: Option<u64>,
}

/// Validate native wire economics before any terminal or fill bookkeeping.
fn validate_execution_numbers(r: &OrderRow<'_>) -> Result<(), String> {
    fn walk(r: &OrderRow<'_>, depth: usize, nodes: &mut usize) -> Result<(), String> {
        *nodes += 1;
        if depth > 8 || *nodes > 1000 {
            return Err("Orderly algo tree exceeds depth or node limit".into());
        }
        if [r.price, r.quantity]
            .iter()
            .any(|n| !n.is_finite() || *n < 0.0)
            || [
                r.total_executed_quantity,
                r.executed,
                r.executed_quantity,
                r.executed_price,
                r.average_executed_price,
                r.trigger_price,
            ]
            .into_iter()
            .flatten()
            .any(|n| !n.0.is_finite() || n.0 < 0.0)
        {
            return Err("Orderly invalid execution quantity or price".into());
        }
        for child in &r.child_orders {
            walk(child, depth + 1, nodes)?;
        }
        Ok(())
    }
    walk(r, 0, &mut 0)
}

struct PreparedFill {
    total: Option<(String, Cursor)>,
    fill: Option<(String, Fill, Cursor)>,
}
struct PreparedExecution<'r, 'a> {
    root: Option<(String, OrderStatus, Option<u64>, u64)>,
    rows: PreparedRows<'r, 'a>,
}
enum PreparedRows<'r, 'a> {
    Sparse {
        order: Box<Order>,
        fill: Option<PreparedFill>,
        seq: Option<u64>,
        timestamp: u64,
    },
    Mapped(Vec<(MappedOrder<'r, 'a>, Option<PreparedFill>)>),
    StaleSparse,
}

/// Stored positions carry the unified symbol, which keeps any broker suffix.
fn position_key(native: &str) -> String {
    symbol(native).unwrap_or_else(|| native.into())
}
fn position_identity(item: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Identity<'a> {
        #[serde(borrow)]
        symbol: &'a str,
        #[serde(alias = "marginMode", default, borrow)]
        margin_mode: &'a str,
    }
    let row: Identity<'_> = serde_json::from_str(item).ok()?;
    let symbol = symbol(row.symbol)?;
    Some(if row.margin_mode == "ISOLATED" {
        format!("{symbol}:isolated")
    } else {
        symbol
    })
}
/// The root id of an order row too malformed to parse.
fn order_key(item: &str, algo: bool) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Identity {
        #[serde(alias = "orderId", default)]
        order_id: Id,
        #[serde(alias = "algoOrderId", default)]
        algo_order_id: Id,
    }
    let row: Identity = serde_json::from_str(item).ok()?;
    Some(if algo {
        row.algo_order_id.0
    } else {
        row.order_id.0
    })
}
fn order_lineages<'r, 'a>(
    root: &'r OrderRow<'a>,
    algo: bool,
) -> Result<Vec<Vec<&'r OrderRow<'a>>>, String> {
    fn walk<'r, 'a>(
        row: &'r OrderRow<'a>,
        lineage: &mut Vec<&'r OrderRow<'a>>,
        out: &mut Vec<Vec<&'r OrderRow<'a>>>,
        nodes: &mut usize,
        algo: bool,
    ) -> Result<(), String> {
        *nodes += 1;
        if lineage.len() > 8 || *nodes > 1000 {
            return Err("Orderly algo tree exceeds depth or node limit".into());
        }
        // The documented STOP response may carry an empty child_orders placeholder.
        if algo
            && row.algo_order_id.0.is_empty()
            && row.algo_type.is_empty()
            && row.kind.is_empty()
            && row.symbol.is_empty()
            && row.order_id.0.is_empty()
            && row.root_algo_order_id.0.is_empty()
            && row.side.is_empty()
            && row.status.is_empty()
            && row.algo_status.is_empty()
            && row.root_algo_status.is_empty()
            && row.quantity == 0.0
            && row.price == 0.0
            && row.trigger_price.is_none()
            && row.reduce_only.is_none()
            && row.client_order_id.is_none()
            && row.margin_mode.is_empty()
            && row.child_orders.is_empty()
        {
            return Ok(());
        }
        lineage.push(row);
        if !algo || !matches!(row.algo_type.as_ref(), "POSITIONAL_TP_SL" | "TP_SL") {
            out.push(lineage.clone());
        }
        if algo {
            for child in &row.child_orders {
                walk(child, lineage, out, nodes, true)?;
            }
        }
        lineage.pop();
        Ok(())
    }
    let mut out = vec![];
    walk(root, &mut vec![], &mut out, &mut 0, algo)?;
    Ok(out)
}

pub struct Account {
    pub credentials: Option<sign::Credentials>,
    pub socket: Connection,
    pub defaults: HashMap<String, MarginType>,
    pub leverage: HashMap<(String, bool), f64>,
    pub fallback_leverage: f64,
    pub algo_snapshots: u64,
    pub positions_revision: u64,
    pub order_revisions: [u64; 2],
    pub settings_revision: u64,
    pub proofs: HashMap<String, Proof>,
    pub uncertain_placements: HashSet<String>,
    // A confirmed POST of our typed full-position payload is also provenance
    // when a later child-only stream omits immutable parent/type fields.
    pub created_pairs: HashMap<String, (String, OrderSide, MarginType)>,
    authenticated: bool,
    inflight: [bool; 4],
    loaded: [bool; 2],
    tick: Option<TimerId>,
    startup_retry: Option<TimerId>,
    ticks: u32,
    order_stamps: HashMap<String, (Option<u64>, u64)>,
    position_stamps: HashMap<String, (Option<u64>, u64)>,
    executions: HashSet<String>,
    execution_order: VecDeque<String>,
    totals: HashMap<String, Cursor>,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
    positions_guard: StaleGuard,
    positions_issued: u64,
}
impl Default for Account {
    fn default() -> Self {
        Self {
            credentials: None,
            socket: Connection::new(Some(10_000.0)),
            defaults: HashMap::new(),
            leverage: HashMap::new(),
            fallback_leverage: 1.0,
            algo_snapshots: 0,
            positions_revision: 0,
            order_revisions: [0; 2],
            settings_revision: 0,
            proofs: HashMap::new(),
            uncertain_placements: HashSet::new(),
            created_pairs: HashMap::new(),
            authenticated: false,
            inflight: [false; 4],
            loaded: [false; 2],
            tick: None,
            startup_retry: None,
            ticks: 0,
            order_stamps: HashMap::new(),
            position_stamps: HashMap::new(),
            executions: HashSet::new(),
            execution_order: VecDeque::new(),
            totals: HashMap::new(),
            terminal: HashSet::new(),
            terminal_order: VecDeque::new(),
            positions_guard: StaleGuard::default(),
            positions_issued: 0,
        }
    }
}
pub fn margin(raw: &str) -> Result<MarginType, String> {
    match raw {
        "ISOLATED" => Ok(MarginType::Isolated),
        "CROSS" | "" => Ok(MarginType::Cross),
        _ => Err("Orderly invalid margin mode".into()),
    }
}

pub fn side(raw: &str) -> Result<OrderSide, String> {
    match raw {
        "BUY" => Ok(OrderSide::Buy),
        "SELL" => Ok(OrderSide::Sell),
        _ => Err("Orderly invalid order side".into()),
    }
}
pub fn status(raw: &str) -> Result<OrderStatus, String> {
    match raw {
        "NEW" | "REPLACED" | "PARTIAL_FILLED" | "PARTIALLY_FILLED" | "PENDING_NEW" => {
            Ok(OrderStatus::Open)
        }
        "FILLED" => Ok(OrderStatus::Closed),
        "CANCELLED" | "CANCELED" | "REJECTED" | "EXPIRED" => Ok(OrderStatus::Canceled),
        _ => Err(format!("Orderly unknown order status {raw}")),
    }
}
fn raw_order_status<'r>(row: &'r OrderRow<'_>, algo: bool) -> &'r str {
    if algo && !row.algo_status.is_empty() {
        &row.algo_status
    } else {
        &row.status
    }
}
fn stamp_newer(prior: Option<&(Option<u64>, u64)>, seq: Option<u64>, ts: u64) -> bool {
    prior.is_none_or(|(old_seq, old_ts)| match (*old_seq, seq) {
        (Some(old), Some(new)) => new > old,
        _ => ts >= *old_ts,
    })
}
fn snapshot_older(prior: Option<&(Option<u64>, u64)>, new: (Option<u64>, u64)) -> bool {
    prior.is_some_and(|old| match (old.0, new.0) {
        (Some(a), Some(b)) => b < a,
        _ => new.1 > 0 && new.1 < old.1,
    })
}
fn merge_stamp(
    stamps: &mut HashMap<String, (Option<u64>, u64)>,
    id: String,
    new: (Option<u64>, u64),
) {
    let previous = stamps.get(&id).copied();
    stamps.insert(
        id,
        (
            new.0.or(previous.and_then(|p| p.0)),
            new.1.max(previous.map_or(0, |p| p.1)),
        ),
    );
}

impl Orderly {
    /// Every private account read is signed by the installed provider over the
    /// Primary identity's refs; the venue never holds signing material for it.
    pub(crate) fn signed(&self) -> bool {
        !self.options.public_data_only && self.options.identities.primary.is_some()
    }
    /// Orderly has one account key: every signed read selects the Primary.
    pub(crate) fn read_selection(
        &self,
    ) -> Result<SelectedRead, safe_cex_engine::credential_selection::CredentialUnavailable> {
        let slots = safe_cex_engine::credential_selection::Slots::new(
            self.options
                .identities
                .primary
                .clone()
                .filter(|_| !self.options.public_data_only),
            None,
            safe_cex_engine::credential_selection::SlotPolicy {
                secondary_requires_primary: true,
            },
        );
        SelectedRead::select(&slots, ReadRole::Read { scope: None })
    }
    /// One Background read per account leg; the native revision checks stay
    /// the join. A refused selection, admission or queue settles the leg
    /// through `account_failed` before any send.
    fn private_read(&mut self, cx: &mut Cx<'_, Pending>, path: String, request: Request) {
        let read = match SelectedRead::background(
            cx,
            self.read_selection(),
            ReadPolicy {
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        ) {
            Ok(read) => read,
            Err(BackgroundReadError::Selection(_)) => {
                return self.account_failed(cx, request, "Orderly credentials unavailable");
            }
            Err(BackgroundReadError::Admission(error)) => {
                let error = error.read_venue_error().message;
                return self.account_failed(cx, request, &error);
            }
        };
        let stream = !request.is_info();
        let origin = self.rest();
        let account = self.options.application_id.clone().unwrap_or_default();
        if let Err((Pending::Account(_, request), error)) = read.queue(
            cx,
            ReadRequest {
                method: Method::Get,
                url: format!("{origin}{path}"),
                body: None,
                stream: stream.then_some(ArrayAt::Path(&["data", "rows"])),
            },
            |identity| {
                Box::new(sign::Http::new(
                    identity.clone(),
                    &account,
                    Method::Get,
                    origin,
                    &path,
                ))
            },
            |read| Pending::Account(read, request),
        ) {
            let error = error.read_venue_error().message;
            self.account_failed(cx, request, &error);
        }
    }
    pub(crate) fn initialize_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.signed() {
            self.seed(cx, Seed::Markets);
            return;
        }
        let credentials = sign::Credentials::new(
            self.options.key.as_deref().unwrap_or(""),
            self.options.secret.as_deref().unwrap_or(""),
            self.options.application_id.as_deref().unwrap_or(""),
        );
        match credentials {
            Ok(c) => {
                self.account.credentials = Some(c);
                self.account_read(cx, Request::Info);
            }
            Err(e) => cx.notice(Level::Error, code::CONFIG_INVALID, e),
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.credentials.is_none() {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Default::default());
            return;
        }
        cx.store.retain_position_ids();
        cx.store.set_hedged(false);
        self.account_read(
            cx,
            Request::Modes {
                rows: SnapshotBuilder::default(),
                invalid: false,
                revision: self.account.settings_revision,
            },
        );
    }
    fn private_url(&self) -> String {
        let host = if self.options.testnet {
            "testnet-ws-private-evm.orderly.org"
        } else {
            "ws-private-evm.orderly.org"
        };
        format!(
            "wss://{host}/v2/ws/private/stream/{}",
            self.options.application_id.as_deref().unwrap_or_default()
        )
    }
    fn account_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.socket.socket.is_none() {
            self.private_connect(cx);
        }
        let symbols: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for s in symbols {
            self.sync_setting(cx, &s);
        }
        self.account_poll(cx, true);
        if self.account.tick.is_none() {
            self.account.tick =
                Some(cx.set_poll_timer_with_bounds(self.poll_interval(), POLL_FLOOR));
        }
    }
    fn poll_interval(&self) -> f64 {
        safe_cex_engine::engine::bounded_tick_interval(self.options.tick_interval.unwrap_or(2000.0))
    }
    pub(crate) fn sync_setting(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let mode = self.selected_mode(cx, symbol);
        let leverage = self
            .account
            .leverage
            .get(&(symbol.into(), mode == MarginType::Isolated))
            .copied()
            .unwrap_or(self.account.fallback_leverage);
        cx.store.update_setting(symbol, |s| {
            s.margin_type = Some(mode);
            s.leverage = leverage;
        });
    }
    pub(crate) fn selected_mode(&self, cx: &Cx<'_, Pending>, symbol: &str) -> MarginType {
        if let Some(m) = self.account.defaults.get(symbol) {
            return *m;
        }
        let mut rows = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == symbol && p.contracts > 0.0);
        let first = rows
            .next()
            .and_then(|p| p.margin_type)
            .unwrap_or(MarginType::Cross);
        if rows.all(|p| p.margin_type == Some(first)) {
            first
        } else {
            MarginType::Cross
        }
    }
    pub(crate) fn account_poll(&mut self, cx: &mut Cx<'_, Pending>, orders: bool) {
        if !self.account.inflight[1] {
            self.account.inflight[1] = true;
            self.account.positions_issued += 1;
            self.account_read(
                cx,
                Request::Positions {
                    rows: vec![],
                    invalid: false,
                    revision: self.account.positions_revision,
                    issued: self.account.positions_issued,
                    stamps: HashMap::new(),
                    coverage: RowAdmission::default(),
                    skipped: Vec::new(),
                },
            );
        }
        if orders {
            for algo in [false, true] {
                let i = algo as usize;
                if !self.account.inflight[2 + i] {
                    self.account.inflight[2 + i] = true;
                    self.account_read(
                        cx,
                        Request::Orders {
                            algo,
                            rows: vec![],
                            proofs: HashMap::new(),
                            invalid: false,
                            revision: self.account.order_revisions[i],
                            page: 1,
                            seen: HashSet::new(),
                            page_count: 0,
                            stamps: HashMap::new(),
                            coverage: RowAdmission::default(),
                        },
                    );
                }
            }
        }
    }
    pub(crate) fn account_after_uncertain_write(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.positions_revision += 1;
        for revision in &mut self.account.order_revisions {
            *revision += 1;
        }
        self.account.inflight[1..].fill(false);
        self.account_poll(cx, true);
    }
    fn account_read(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let path = match &request {
            Request::Info => "/v1/client/info".to_owned(),
            Request::Modes { .. } => "/v1/client/margin_modes".to_owned(),
            Request::Positions { .. } => "/v1/positions".to_owned(),
            Request::Orders { algo, page, .. } => format!(
                "/v1/{}orders?status=INCOMPLETE&page={page}&size=500",
                if *algo { "algo/" } else { "" }
            ),
        };
        self.private_read(cx, path, request);
    }
    pub(crate) fn position(
        &self,
        cx: &Cx<'_, Pending>,
        r: &PositionRow<'_>,
    ) -> Result<Option<Position>, String> {
        let Some(symbol) = symbol(&r.symbol).filter(|s| cx.store.market(s).is_some()) else {
            return if r.position_qty == 0.0 {
                Ok(None)
            } else {
                Err(
                    "Orderly owned position market unavailable; preserving previous snapshot"
                        .into(),
                )
            };
        };
        let mode = margin(&r.margin_mode)?;
        let mark = if r.position_qty == 0.0 {
            0.0
        } else {
            Some(r.mark_price)
                .filter(|price| price.is_finite() && *price > 0.0)
                .or_else(|| {
                    cx.store.ticker(&symbol).and_then(|ticker| {
                        [ticker.mark, ticker.last]
                            .into_iter()
                            .find(|price| price.is_finite() && *price > 0.0)
                    })
                })
                .ok_or_else(|| {
                    "Orderly position mark unavailable; preserving previous positions and valuation"
                        .to_string()
                })?
        };
        let leverage = if r.leverage > 0.0 {
            r.leverage
        } else if r.imr > 0.0 {
            1.0 / r.imr
        } else {
            self.account
                .leverage
                .get(&(symbol.clone(), mode == MarginType::Isolated))
                .copied()
                .unwrap_or(self.account.fallback_leverage)
        };
        let size = r.position_qty.abs();
        let notional = size * mark;
        Ok(Some(Position {
            id: format!(
                "{symbol}{}",
                if mode == MarginType::Isolated {
                    ":isolated"
                } else {
                    ""
                }
            ),
            symbol,
            side: if r.position_qty >= 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            },
            entry_price: r.average_open_price,
            notional,
            leverage,
            margin_type: Some(mode),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: safe_cex_engine::num::round_half_up(
                (mark - r.average_open_price) * r.position_qty * 100.0,
            ) / 100.0,
            realized_pnl: None,
            contracts: size,
            liquidation_price: r.est_liq_price,
            margin_size: Some(if r.margin > 0.0 {
                r.margin
            } else if leverage > 0.0 {
                notional / leverage
            } else {
                0.0
            }),
            last_update: Some(cx.now),
        }))
    }
    /// Maps a root and its algo children; working ids on unknown markets go to `unknown`.
    fn orders<'r, 'a>(
        &self,
        cx: &Cx<'_, Pending>,
        r: &'r OrderRow<'a>,
        algo: bool,
        unknown: &mut Vec<String>,
    ) -> Result<Vec<MappedOrder<'r, 'a>>, String> {
        let mut out = vec![];
        let mut ids = HashSet::new();
        for lineage in order_lineages(r, algo)? {
            let row = *lineage.last().unwrap();
            let native = lineage
                .iter()
                .rev()
                .find(|r| !r.symbol.is_empty())
                .map(|r| r.symbol.as_ref())
                .unwrap_or("");
            let Some(symbol) = symbol(native) else {
                continue;
            };
            if cx.store.market(&symbol).is_none() {
                let raw = lineage
                    .iter()
                    .rev()
                    .map(|r| raw_order_status(r, algo))
                    .find(|s| !s.is_empty())
                    .unwrap_or("");
                let final_status = status(raw)
                    .is_ok_and(|s| matches!(s, OrderStatus::Closed | OrderStatus::Canceled))
                    || status(raw).is_ok_and(|s| s == OrderStatus::Open)
                        && lineage.iter().any(|r| {
                            status(&r.root_algo_status).is_ok_and(|s| {
                                matches!(s, OrderStatus::Closed | OrderStatus::Canceled)
                            })
                        });
                if !final_status {
                    unknown.push(if algo {
                        row.algo_order_id.0.clone()
                    } else {
                        row.order_id.0.clone()
                    });
                }
                continue;
            }
            if lineage
                .iter()
                .any(|r| !r.symbol.is_empty() && r.symbol != native)
            {
                return Err("Orderly algo symbol identity mismatch".into());
            }
            let id = if algo {
                &row.algo_order_id.0
            } else {
                &row.order_id.0
            };
            if id.is_empty() || id == "0" || !ids.insert(id.clone()) {
                return Err("Orderly missing or duplicate order ID".into());
            }
            let protective = matches!(
                row.algo_type.as_ref(),
                "STOP_LOSS" | "TAKE_PROFIT" | "TRAILING_STOP"
            );
            let reduce_only = row
                .reduce_only
                .or_else(|| {
                    lineage
                        .iter()
                        .rev()
                        .skip(1)
                        .find(|r| r.algo_type != "BRACKET" && r.reduce_only.is_some())
                        .and_then(|r| r.reduce_only)
                })
                .unwrap_or(protective);
            let raw_kind = if !algo || row.algo_type == "BRACKET" {
                row.kind.as_ref()
            } else {
                row.algo_type.as_ref()
            };
            let (kind, tif) = match raw_kind {
                "LIMIT" => (OrderType::Limit, Some(TimeInForce::GoodTillCancel)),
                "MARKET" => (OrderType::Market, None),
                "POST_ONLY" => (OrderType::Limit, Some(TimeInForce::PostOnly)),
                "IOC" => (OrderType::Limit, Some(TimeInForce::ImmediateOrCancel)),
                "FOK" => (OrderType::Limit, Some(TimeInForce::FillOrKill)),
                "STOP" if reduce_only => (OrderType::StopLoss, None),
                "STOP" => (OrderType::Conditional, None),
                "STOP_LOSS" => (OrderType::StopLoss, None),
                "TAKE_PROFIT" => (OrderType::TakeProfit, None),
                "TRAILING_STOP" => (OrderType::TrailingStopLoss, None),
                _ => return Err("Orderly unsupported order type".into()),
            };
            let triggered = algo && row.algo_type != "BRACKET";
            if triggered && row.trigger_price.is_none() && row.algo_type != "TRAILING_STOP" {
                return Err("Orderly missing algo trigger price".into());
            }
            if triggered
                && !matches!(
                    row.trigger_price_type.as_ref(),
                    "" | "MARK_PRICE" | "USELESS"
                )
            {
                return Err("Orderly unsupported algo trigger source".into());
            }
            let filled = row
                .total_executed_quantity
                .or(row.executed)
                .map_or(0.0, |n| n.0);
            // Finite TP_SL quantities may be declared on the container. They must
            // never be inferred to mean a full-position CLOSE_POSITION leg.
            let amount = if algo && row.kind != "CLOSE_POSITION" && row.quantity == 0.0 {
                lineage
                    .iter()
                    .rev()
                    .skip(1)
                    .find(|r| r.algo_type == "TP_SL")
                    .map_or(row.quantity, |r| r.quantity)
            } else {
                row.quantity
            };
            if amount < 0.0 || filled < 0.0 {
                return Err("Orderly invalid quantity".into());
            }
            let parent = lineage
                .iter()
                .rev()
                .find(|r| !r.root_algo_order_id.0.is_empty() && r.root_algo_order_id.0 != "0")
                .map(|r| r.root_algo_order_id.0.clone())
                .unwrap_or_else(|| r.algo_order_id.0.clone());
            if algo && (parent.is_empty() || parent == "0") {
                return Err("Orderly missing algo root identity".into());
            }
            if algo
                && lineage.iter().any(|r| {
                    !r.root_algo_order_id.0.is_empty()
                        && r.root_algo_order_id.0 != "0"
                        && r.root_algo_order_id.0 != parent
                })
            {
                return Err("Orderly conflicting algo root identity".into());
            }
            let mut raw_status = lineage
                .iter()
                .rev()
                .map(|r| raw_order_status(r, algo))
                .find(|s| !s.is_empty())
                .unwrap_or("");
            if status(raw_status).is_ok_and(|s| s == OrderStatus::Open) {
                // A final root closes stale NEW children, while explicit child
                // terminal states (e.g. CANCELLED below FILLED) take precedence.
                if let Some(final_root) = lineage
                    .iter()
                    .rev()
                    .map(|r| r.root_algo_status.as_ref())
                    .find(|s| {
                        status(s)
                            .is_ok_and(|s| matches!(s, OrderStatus::Closed | OrderStatus::Canceled))
                    })
                {
                    raw_status = final_root;
                }
            }
            let tif = match row.time_in_force.as_ref() {
                "IOC" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" => Some(TimeInForce::PostOnly),
                _ => tif,
            };
            let raw_mode = lineage
                .iter()
                .rev()
                .find(|r| !r.margin_mode.is_empty())
                .map(|r| r.margin_mode.as_ref())
                .unwrap_or("");
            let side_ = side(
                lineage
                    .iter()
                    .rev()
                    .find(|r| !r.side.is_empty())
                    .map(|r| r.side.as_ref())
                    .unwrap_or(""),
            )?;
            let created = self
                .account
                .created_pairs
                .get(&parent)
                .filter(|(s, side, _)| *s == symbol && *side == side_);
            let previous = self.account.proofs.get(id).filter(|_| {
                cx.store.orders().iter().any(|old| {
                    old.id == *id
                        && old.parent_id.as_ref() == Some(&parent)
                        && old.symbol == symbol
                        && old.side == side_
                        && old.kind == kind
                })
            });
            let prior = previous.filter(|p| p.positional_repair);
            let mode = if raw_mode.is_empty() {
                previous
                    .filter(|p| p.mode_known)
                    .map(|p| p.mode)
                    .or_else(|| created.map(|(_, _, mode)| *mode))
                    .unwrap_or(MarginType::Cross)
            } else {
                margin(raw_mode)?
            };
            let mode_known =
                !raw_mode.is_empty() || created.is_some() || previous.is_some_and(|p| p.mode_known);
            let rest_positional = row.kind == "CLOSE_POSITION"
                && row.reduce_only == Some(true)
                && matches!(raw_mode, "CROSS" | "ISOLATED")
                && lineage
                    .iter()
                    .any(|r| r.algo_type == "POSITIONAL_TP_SL" && r.algo_order_id.0 == parent);
            let close_compatible = algo
                && protective
                && row.algo_type != "TRAILING_STOP"
                && matches!(row.kind.as_ref(), "" | "CLOSE_POSITION")
                && row.reduce_only != Some(false)
                && (rest_positional || previous.is_none_or(|p| p.close_compatible))
                && lineage.iter().all(|r| {
                    r.algo_type != "BRACKET"
                        && r.algo_type != "TP_SL"
                        && (r.margin_mode.is_empty()
                            || margin(&r.margin_mode).is_ok_and(|m| m == mode))
                });
            let positional_repair = close_compatible
                && (rest_positional
                    || created.is_some_and(|(_, _, m)| *m == mode)
                    || prior.is_some_and(|p| p.mode == mode));
            let seq = lineage.iter().rev().find_map(|r| r.seq);
            let timestamp = lineage.iter().rev().find_map(|r| r.timestamp);
            out.push(MappedOrder {
                order: Order {
                    id: id.clone(),
                    parent_id: algo.then_some(parent),
                    symbol,
                    status: status(raw_status)?,
                    kind,
                    side: side_,
                    price: row.display_price(side_),
                    amount,
                    filled,
                    remaining: (amount - filled).max(0.0),
                    reduce_only,
                    close: None,
                    client_order_id: row.client_order_id.as_ref().map(|s| s.to_string()),
                    time_in_force: tif,
                    last_update: cx.now,
                    trigger_type: triggered.then_some(StopTriggerType::MarkPrice),
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                },
                row,
                proof: Proof {
                    mode,
                    amend: false,
                    positional_repair,
                    close_compatible,
                    mode_known,
                },
                seq,
                timestamp,
            });
        }
        Ok(out)
    }
    fn proof(&self, cx: &Cx<'_, Pending>, item: &str) -> Option<(String, Proof)> {
        let r: RestProof<'_> = serde_json::from_str(item).ok()?;
        let symbol = symbol(&r.symbol)?;
        let market = cx.store.market(&symbol)?;
        let valid = r.kind == "LIMIT"
            && r.status == "NEW"
            && matches!(r.side.as_ref(), "BUY" | "SELL")
            && r.order_id.0 != "0"
            && r.price > 0.0
            && r.quantity > 0.0
            && r.visible_quantity == r.quantity
            && r.total_executed_quantity == 0.0
            && r.executed.is_none_or(|n| n == 0.0)
            && r.executed_quantity.is_none_or(|n| n == 0.0)
            && matches!(r.margin_mode.as_ref(), "CROSS" | "ISOLATED")
            && matches!(r.time_in_force.as_ref(), "" | "GTC")
            && r.trigger_price.is_none()
            && r.algo_order_id.is_none()
            && r.root_algo_order_id.is_none()
            && r.algo_type.is_empty()
            && safe_cex_engine::num::adjust(r.price, market.precision.price) == r.price
            && safe_cex_engine::num::adjust(r.quantity, market.precision.amount) == r.quantity;
        let _reduce_only = r.reduce_only;
        Some((
            r.order_id.0,
            Proof {
                mode: margin(&r.margin_mode).ok()?,
                amend: valid,
                positional_repair: false,
                close_compatible: false,
                mode_known: true,
            },
        ))
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        let result: Result<(), String> = (|| {
            match request {
                Request::Info => {}
                Request::Modes { rows, .. } => {
                    let mapped = serde_json::from_str::<ModeRow<'_>>(item)
                        .ok()
                        .and_then(|r| {
                            Some((symbol(&r.symbol)?, margin(&r.default_margin_mode).ok()?))
                        });
                    if let Some(mapped) = mapped {
                        rows.push(mapped);
                    } else {
                        #[derive(serde::Deserialize)]
                        struct Identity<'a> {
                            #[serde(default, borrow)]
                            symbol: Option<std::borrow::Cow<'a, str>>,
                        }
                        let key = serde_json::from_str::<Identity<'_>>(item)
                            .ok()
                            .and_then(|r| r.symbol.as_deref().and_then(symbol));
                        rows.skip(
                            cx,
                            key,
                            Level::Warn,
                            "Orderly skipped malformed margin-mode row",
                        );
                    }
                }
                Request::Positions {
                    rows,
                    stamps,
                    coverage,
                    skipped,
                    ..
                } => {
                    let (symbol, pnl) = match serde_json::from_str::<PositionRow<'_>>(item) {
                        Ok(r) => match self.position(cx, &r) {
                            Ok(Some(p)) => {
                                stamps.insert(p.id.clone(), (r.seq, r.timestamp.unwrap_or(0)));
                                rows.push(p);
                                return Ok(());
                            }
                            // A flat row closes its symbol even when its mode is unknown.
                            Ok(None) => return Ok(()),
                            Err(_) if r.position_qty == 0.0 => return Ok(()),
                            Err(_) => (
                                Some(position_key(&r.symbol)),
                                Some(r.mark_price)
                                    .filter(|price| price.is_finite() && *price > 0.0)
                                    .map(|mark| {
                                        safe_cex_engine::num::round_half_up(
                                            (mark - r.average_open_price) * r.position_qty * 100.0,
                                        ) / 100.0
                                    }),
                            ),
                        },
                        Err(_) => {
                            #[derive(serde::Deserialize)]
                            struct Identity<'a> {
                                #[serde(borrow)]
                                symbol: std::borrow::Cow<'a, str>,
                                #[serde(alias = "positionQty", default, borrow)]
                                position_qty: Option<&'a serde_json::value::RawValue>,
                            }
                            #[derive(serde::Deserialize)]
                            struct Qty(#[serde(deserialize_with = "number")] f64);
                            if let Ok(r) = serde_json::from_str::<Identity<'_>>(item) {
                                let flat = r.position_qty.is_some_and(|raw| {
                                    serde_json::from_str::<Qty>(raw.get())
                                        .is_ok_and(|qty| qty.0 == 0.0)
                                });
                                if flat {
                                    return Ok(());
                                }
                                (Some(position_key(&r.symbol)), None)
                            } else {
                                (None, None)
                            }
                        }
                    };
                    skipped.push((symbol.clone(), pnl));
                    let message = match &symbol {
                        Some(symbol) => format!("Orderly skipped unmappable position {symbol}"),
                        None => "Orderly skipped unmappable position without a symbol".into(),
                    };
                    coverage.skip(cx, symbol, Level::Warn, message);
                }
                Request::Orders {
                    algo,
                    rows,
                    proofs,
                    page,
                    seen,
                    page_count,
                    stamps,
                    coverage,
                    ..
                } => {
                    *page_count += 1;
                    let parsed = serde_json::from_str::<OrderRow<'_>>(item);
                    let key = match &parsed {
                        Ok(r) if *algo => r.algo_order_id.0.clone(),
                        Ok(r) => r.order_id.0.clone(),
                        Err(_) => order_key(item, *algo).unwrap_or_default(),
                    };
                    let key = (!key.is_empty() && key != "0").then_some(key);
                    if let Some(key) = &key
                        && !seen.insert(key.clone())
                    {
                        return Err("Orderly repeated order page".into());
                    }
                    let mut unknown = vec![];
                    let mapped = parsed
                        .as_ref()
                        .ok()
                        .and_then(|r| self.orders(cx, r, *algo, &mut unknown).ok());
                    let Some(mapped) = mapped else {
                        let message = match &key {
                            Some(key) => format!("Orderly skipped unmappable order {key}"),
                            None => "Orderly skipped unmappable order without an id".into(),
                        };
                        let unidentified = key.is_none();
                        coverage.skip(cx, key, Level::Warn, message);
                        if unidentified {
                            // Numeric ids cannot collide with this page-position census key.
                            seen.insert(format!("row:{page}:{page_count}"));
                        }
                        return Ok(());
                    };
                    for id in unknown {
                        let message = format!("Orderly skipped order {id} on an unknown market");
                        coverage.skip(cx, Some(id), Level::Warn, message);
                    }
                    if let Some((id, proof)) = self.proof(cx, item).filter(|_| !*algo) {
                        proofs.insert(id, proof);
                    }
                    for mapped in mapped {
                        let o = mapped.order;
                        if rows.iter().any(|prior| prior.id == o.id) {
                            return Err("Orderly duplicate order across roots".into());
                        }
                        if *algo {
                            proofs.insert(o.id.clone(), mapped.proof);
                        }
                        stamps.insert(o.id.clone(), (mapped.seq, mapped.timestamp.unwrap_or(0)));
                        rows.push(o);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            match request {
                Request::Modes { invalid, .. }
                | Request::Positions { invalid, .. }
                | Request::Orders { invalid, .. } => *invalid = true,
                Request::Info => {}
            };
            cx.notice(Level::Error, code::STREAM_MALFORMED, e);
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let parsed = checked(response);
        if let Err(e) = parsed {
            if request.is_info()
                && (400..500).contains(&response.status)
                && !matches!(response.status, 408 | 429)
            {
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    "Orderly API key expired or rejected",
                );
                cx.deactivate();
                return;
            }
            self.account_failed(cx, request, &e);
            return;
        }
        let rest = parsed.unwrap();
        let Some(data) = rest.data.map(|d| d.get()) else {
            self.account_failed(cx, request, "Orderly missing account response data");
            return;
        };
        match request {
            Request::Info => {
                let Ok(info) = serde_json::from_str::<Info>(data) else {
                    self.account_failed(cx, Request::Info, "Orderly invalid account info");
                    return;
                };
                self.account.fallback_leverage = if info.max_leverage > 0.0 {
                    info.max_leverage
                } else {
                    20.0
                };
                self.seed(cx, Seed::Markets);
            }
            Request::Modes {
                rows,
                invalid,
                revision,
            } => {
                if !invalid && revision == self.account.settings_revision {
                    let current = std::mem::take(&mut self.account.defaults);
                    self.account.defaults = rows
                        .commit(current, |row| row.0.clone())
                        .into_iter()
                        .collect();
                }
                self.account_ready(cx);
            }
            Request::Positions {
                mut rows,
                invalid,
                revision,
                issued,
                stamps,
                coverage,
                skipped,
            } => {
                self.account.inflight[1] = false;
                if invalid {
                    return;
                }
                let current = revision == self.account.positions_revision;
                if !self
                    .account
                    .positions_guard
                    .admit_issued(current, issued, STALE_SNAPSHOT_LIMIT)
                {
                    return;
                }
                let Ok(meta) = serde_json::from_str::<PositionMeta>(data) else {
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Orderly invalid collateral response",
                    );
                    return;
                };
                let kept = cx
                    .store
                    .positions()
                    .iter()
                    .filter(|p| {
                        !coverage.allows_absence(&p.symbol) && !rows.iter().any(|q| q.id == p.id)
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                rows.extend(kept);
                // Collateral is a native aggregate; its PnL split sums the rows plus
                // retained copies, plus each skipped row with no copy. A skipped row
                // with neither a copy nor a known PnL leaves the split unknown.
                let skipped_pnl = skipped.iter().try_fold(0.0, |sum, (symbol, pnl)| {
                    if symbol
                        .as_ref()
                        .is_some_and(|symbol| rows.iter().any(|p| &p.symbol == symbol))
                    {
                        Some(sum)
                    } else {
                        pnl.map(|pnl| sum + pnl)
                    }
                });
                let balance = skipped_pnl.map(|skipped_pnl| {
                    let upnl = safe_cex_engine::num::round_half_up(
                        (skipped_pnl + rows.iter().map(|p| p.unrealized_pnl).sum::<f64>()) * 100.0,
                    ) / 100.0;
                    Balance {
                        total: meta.total_collateral_value - upnl,
                        free: meta.free_collateral,
                        used: meta.total_collateral_value - meta.free_collateral - upnl,
                        upnl,
                    }
                });
                // Positions are pushed, so a stale read only commits the REST-only
                // balance; stale rows could drop a position opened mid-read.
                if !current {
                    if let Some(balance) = balance {
                        cx.store.set_balance(balance);
                    }
                    self.account_poll(cx, false);
                    return;
                }
                if stamps
                    .iter()
                    .any(|(id, new)| snapshot_older(self.account.position_stamps.get(id), *new))
                {
                    return;
                }
                for p in &rows {
                    if let Some(m) = p.margin_type {
                        self.account
                            .leverage
                            .insert((p.symbol.clone(), m == MarginType::Isolated), p.leverage);
                    }
                }
                for (id, new) in stamps {
                    merge_stamp(&mut self.account.position_stamps, id, new);
                }
                cx.store.set_positions(rows);
                if let Some(balance) = balance {
                    cx.store.set_balance(balance);
                }
                for s in cx
                    .store
                    .markets()
                    .iter()
                    .map(|m| m.symbol.clone())
                    .collect::<Vec<_>>()
                {
                    self.sync_setting(cx, &s);
                }
            }
            Request::Orders {
                algo,
                rows,
                proofs,
                invalid,
                revision,
                page,
                seen,
                page_count,
                stamps,
                coverage,
            } => {
                let i = algo as usize;
                if invalid || revision != self.account.order_revisions[i] {
                    self.account.inflight[2 + i] = false;
                    return;
                }
                let Ok(meta) = serde_json::from_str::<Page>(data) else {
                    self.account.inflight[2 + i] = false;
                    cx.notice(
                        Level::Error,
                        code::STREAM_MALFORMED,
                        "Orderly invalid order page",
                    );
                    return;
                };
                if meta.meta.as_ref().is_some_and(|m| {
                    m.current_page.is_some_and(|p| p != page)
                        || m.records_per_page.is_some_and(|n| n > 500)
                }) {
                    self.account.inflight[2 + i] = false;
                    cx.retried_failure("orderly.account:1195", "Orderly inconsistent pagination");
                    return;
                }
                let more = meta
                    .meta
                    .and_then(|m| m.total)
                    .map_or(page_count > 0, |total| (seen.len() as u64) < total);
                if more {
                    if page >= 50 || page_count == 0 {
                        self.account.inflight[2 + i] = false;
                        cx.retried_failure(
                            "orderly.account:1209",
                            "Orderly order pagination incomplete",
                        );
                        return;
                    }
                    self.account_read(
                        cx,
                        Request::Orders {
                            algo,
                            rows,
                            proofs,
                            invalid: false,
                            revision,
                            page: page + 1,
                            seen,
                            page_count: 0,
                            stamps,
                            coverage,
                        },
                    );
                    return;
                }
                self.account.inflight[2 + i] = false;
                if stamps
                    .iter()
                    .any(|(id, new)| snapshot_older(self.account.order_stamps.get(id), *new))
                {
                    return;
                }
                self.account.loaded[i] = true;
                if algo {
                    self.account.algo_snapshots += 1;
                } else {
                    for id in self.account.uncertain_placements.drain() {
                        cx.store.remove_orders(&[&id]);
                    }
                }
                let mut combined: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| {
                        o.parent_id.is_some() != algo
                            || o.status == OrderStatus::Creating
                            || ((!coverage.allows_absence(&o.id)
                                || o.parent_id
                                    .as_ref()
                                    .is_some_and(|p| !coverage.allows_absence(p)))
                                && !self.account.terminal.contains(&o.id)
                                && !o.parent_id.as_ref().is_some_and(|p| {
                                    self.account.terminal.contains(&format!("root:{p}"))
                                })
                                && !rows.iter().any(|row| row.id == o.id))
                    })
                    .cloned()
                    .collect();
                self.account
                    .proofs
                    .retain(|id, _| combined.iter().any(|o| &o.id == id));
                self.account.proofs.extend(proofs);
                for (id, new) in stamps {
                    merge_stamp(&mut self.account.order_stamps, id, new);
                }
                combined.extend(rows.into_iter().filter(|o| {
                    !self.account.terminal.contains(&o.id)
                        && !o
                            .parent_id
                            .as_ref()
                            .is_some_and(|p| self.account.terminal.contains(&format!("root:{p}")))
                }));
                if self.account.loaded.iter().all(|b| *b) {
                    cx.store.set_orders(combined);
                } else {
                    for o in combined {
                        cx.store.upsert_order(o);
                    }
                }
            }
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        cx.retried_failure("orderly.account:1297", error);
        match request {
            Request::Info => {
                if self.account.startup_retry.is_none() {
                    self.account.startup_retry = Some(cx.set_timer(5000.0));
                }
            }
            Request::Modes { .. } => self.account_ready(cx),
            Request::Positions { .. } => self.account.inflight[1] = false,
            Request::Orders { algo, .. } => self.account.inflight[2 + algo as usize] = false,
        }
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.authenticated = false;
        self.account.socket.open = false;
        self.account.socket.supervisor.connecting(cx);
        self.account.socket.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new(self.private_url())
                    .channel(safe_cex_engine::SocketChannel::Private),
            ),
        );
    }
    pub(crate) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) || self.account.socket.open {
            return;
        }
        self.account.authenticated = false;
        self.account.socket.open = true;
        self.account.socket.supervisor.opened(cx);
        let Some(primary) = self.options.identities.primary.clone() else {
            cx.close(socket);
            self.private_close(cx, socket);
            return;
        };
        let url = self.private_url();
        cx.send_signed(
            socket,
            String::new(),
            Box::new(sign::WsAuth::new(primary, &url)),
        );
    }
    pub(crate) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) {
            return;
        }
        self.account.authenticated = false;
        self.account.socket.socket = None;
        self.account.socket.open = false;
        self.account.socket.supervisor.retry(cx);
    }
    pub(crate) fn order_terminal(&self, id: &str) -> bool {
        self.account.terminal.contains(id)
    }
    pub(crate) fn root_terminal(&self, parent: &str) -> bool {
        self.account.terminal.contains(&format!("root:{parent}"))
    }
    pub(crate) fn terminal_order(&mut self, id: &str) {
        if self.account.terminal.insert(id.into()) {
            self.account.terminal_order.push_back(id.into());
        }
        if self.account.terminal_order.len() > 4096 {
            let id = self.account.terminal_order.pop_front().unwrap();
            self.account.terminal.remove(&id);
            self.account.order_stamps.remove(&id);
            self.account.totals.remove(&id);
        }
    }
    fn prepare_fill(&self, r: &OrderRow<'_>, order: &Order) -> Result<PreparedFill, String> {
        let filled = r
            .total_executed_quantity
            .or(r.executed)
            .map_or(0.0, |n| n.0);
        let previous = self
            .account
            .totals
            .get(&order.id)
            .copied()
            .unwrap_or_default();
        let prepared = previous
            .prepare_observed_average(
                filled,
                r.executed_quantity.map(|n| n.0),
                r.average_executed_price.map(|n| n.0),
                r.executed_price.map(|n| n.0),
                Arithmetic::Binary,
            )
            .map_err(|_| "Orderly invalid execution quantity or price".to_string())?;
        let total = prepared
            .observation()
            .map(|total| (order.id.clone(), total));
        let Some(delivery) = prepared.delivery() else {
            return Ok(PreparedFill { total, fill: None });
        };
        let event = if !r.trade_id.0.is_empty() {
            format!("trade:{}:{}", order.id, r.trade_id.0)
        } else if !r.match_id.0.is_empty() {
            format!("match:{}:{}", order.id, r.match_id.0)
        } else {
            format!("cumulative:{}:{}", order.id, filled)
        };
        Ok(PreparedFill {
            total,
            fill: Some((
                event,
                Fill {
                    id: None,
                    order_id: order.id.clone(),
                    symbol: order.symbol.clone(),
                    side: order.side,
                    price: delivery.price(),
                    amount: delivery.amount(),
                },
                delivery.next(),
            )),
        })
    }
    fn commit_fill(&mut self, cx: &mut Cx<'_, Pending>, prepared: PreparedFill) {
        if let Some((id, total)) = prepared.total {
            self.account.totals.insert(id, total);
        }
        let Some((event, fill, next)) = prepared.fill else {
            return;
        };
        if !self.account.executions.insert(event.clone()) {
            return;
        }
        self.account.execution_order.push_back(event);
        if self.account.execution_order.len() > 4096 {
            self.account
                .executions
                .remove(&self.account.execution_order.pop_front().unwrap());
        }
        self.account.positions_revision += 1;
        let order_id = fill.order_id.clone();
        cx.fill(fill);
        if let Some(total) = self.account.totals.get_mut(&order_id)
            && let Some(enriched) = total.enrich_quote(next)
        {
            *total = enriched;
        }
    }
    fn prepare_execution<'r, 'a>(
        &self,
        cx: &Cx<'_, Pending>,
        r: &'r OrderRow<'a>,
        algo: bool,
        ts: u64,
    ) -> Result<PreparedExecution<'r, 'a>, String> {
        validate_execution_numbers(r)?;
        if algo {
            if (r.algo_order_id.0.is_empty() || r.algo_order_id.0 == "0")
                && (r.root_algo_order_id.0.is_empty() || r.root_algo_order_id.0 == "0")
            {
                return Err("Orderly missing algo root identity".into());
            }
            self.validate_execution_identity(cx, r)?;
        }
        let mut root = None;
        if algo {
            let raw_status = if matches!(
                r.root_algo_status.as_ref(),
                "FILLED" | "CANCELLED" | "REJECTED"
            ) {
                r.root_algo_status.as_ref()
            } else if matches!(r.algo_type.as_ref(), "POSITIONAL_TP_SL" | "TP_SL")
                && !r.algo_order_id.0.is_empty()
                && (r.root_algo_order_id.0.is_empty()
                    || r.root_algo_order_id.0 == "0"
                    || r.algo_order_id.0 == r.root_algo_order_id.0)
            {
                if r.algo_status.is_empty() {
                    r.status.as_ref()
                } else {
                    r.algo_status.as_ref()
                }
            } else {
                ""
            };
            if let Ok(final_status @ (OrderStatus::Closed | OrderStatus::Canceled)) =
                status(raw_status)
            {
                let parent = if r.root_algo_order_id.0.is_empty() || r.root_algo_order_id.0 == "0" {
                    &r.algo_order_id.0
                } else {
                    &r.root_algo_order_id.0
                };
                if !parent.is_empty() && parent != "0" {
                    let key = format!("root:{parent}");
                    let timestamp = r.timestamp.unwrap_or(ts);
                    if stamp_newer(self.account.order_stamps.get(&key), r.seq, timestamp) {
                        root = Some((parent.clone(), final_status, r.seq, timestamp));
                    }
                }
            }
        }
        // A sparse terminal leaf/BRACKET-entry report may omit the trigger and
        // immutable economic fields. It finalizes only that known native ID;
        // absence of children/trigger never proves completion of its whole root.
        if algo
            && r.trigger_price.is_none()
            && r.child_orders.is_empty()
            && !matches!(r.algo_type.as_ref(), "POSITIONAL_TP_SL" | "TP_SL")
            && let Ok(final_status @ (OrderStatus::Closed | OrderStatus::Canceled)) =
                status(raw_order_status(r, true))
            && let Some(mut order) = cx
                .store
                .orders()
                .iter()
                .find(|o| o.id == r.algo_order_id.0)
                .cloned()
        {
            if (!r.root_algo_order_id.0.is_empty()
                && r.root_algo_order_id.0 != "0"
                && order.parent_id.as_ref() != Some(&r.root_algo_order_id.0))
                || (!r.symbol.is_empty()
                    && symbol(&r.symbol).as_deref() != Some(order.symbol.as_str()))
                || (!r.side.is_empty() && side(&r.side)? != order.side)
            {
                return Err("Orderly terminal child identity mismatch".into());
            }
            let timestamp = r.timestamp.unwrap_or(ts);
            if stamp_newer(self.account.order_stamps.get(&order.id), r.seq, timestamp) {
                let fill = if let Some(total) = r.total_executed_quantity.or(r.executed) {
                    order.filled = order.filled.max(total.0);
                    order.remaining = (order.amount - order.filled).max(0.0);
                    Some(self.prepare_fill(r, &order)?)
                } else {
                    None
                };
                order.status = final_status;
                order.last_update = cx.now;
                return Ok(PreparedExecution {
                    root,
                    rows: PreparedRows::Sparse {
                        order: Box::new(order),
                        fill,
                        seq: r.seq,
                        timestamp,
                    },
                });
            }
            return Ok(PreparedExecution {
                root,
                rows: PreparedRows::StaleSparse,
            });
        }
        let mut unknown = vec![];
        let orders = self.orders(cx, r, algo, &mut unknown)?;
        if !unknown.is_empty() {
            return Err(
                "Orderly working order market unavailable; preserving previous orders".into(),
            );
        }
        let mut prepared = Vec::with_capacity(orders.len());
        for mapped in orders {
            let row = mapped.row;
            let fill = if matches!(
                if algo && !row.algo_status.is_empty() {
                    row.algo_status.as_ref()
                } else {
                    row.status.as_ref()
                },
                "PARTIAL_FILLED" | "PARTIALLY_FILLED" | "FILLED"
            ) {
                Some(self.prepare_fill(row, &mapped.order)?)
            } else {
                None
            };
            prepared.push((mapped, fill));
        }
        Ok(PreparedExecution {
            root,
            rows: PreparedRows::Mapped(prepared),
        })
    }

    // Native sparse/root identity validation is pure. A root cascade may not run
    // before a supplied child identity, side or nested tree has been validated.
    fn validate_execution_identity(
        &self,
        cx: &Cx<'_, Pending>,
        r: &OrderRow<'_>,
    ) -> Result<(), String> {
        fn walk(
            cx: &Cx<'_, Pending>,
            r: &OrderRow<'_>,
            parent: Option<&str>,
            native: Option<&str>,
            depth: usize,
            nodes: &mut usize,
        ) -> Result<(), String> {
            *nodes += 1;
            if depth > 8 || *nodes > 1000 {
                return Err("Orderly algo tree exceeds depth or node limit".into());
            }
            let bound_parent = parent.or_else(|| {
                (!r.root_algo_order_id.0.is_empty() && r.root_algo_order_id.0 != "0")
                    .then_some(r.root_algo_order_id.0.as_str())
            });
            let parent = bound_parent.or_else(|| {
                (!r.algo_order_id.0.is_empty() && r.algo_order_id.0 != "0")
                    .then_some(r.algo_order_id.0.as_str())
            });
            if !r.root_algo_order_id.0.is_empty()
                && r.root_algo_order_id.0 != "0"
                && Some(r.root_algo_order_id.0.as_str()) != parent
            {
                return Err("Orderly conflicting algo root identity".into());
            }
            if !r.side.is_empty() {
                side(&r.side)?;
            }
            if !r.symbol.is_empty() {
                let unified = symbol(&r.symbol).ok_or("Orderly invalid algo symbol identity")?;
                if cx
                    .store
                    .orders()
                    .iter()
                    .any(|old| old.parent_id.as_deref() == parent && old.symbol != unified)
                {
                    return Err("Orderly algo symbol identity mismatch".into());
                }
            }
            let native = native.or((!r.symbol.is_empty()).then_some(r.symbol.as_ref()));
            if !r.symbol.is_empty() && Some(r.symbol.as_ref()) != native {
                return Err("Orderly algo symbol identity mismatch".into());
            }
            if let Some(old) = cx.store.orders().iter().find(|o| o.id == r.algo_order_id.0)
                && ((bound_parent.is_some() && old.parent_id.as_deref() != bound_parent)
                    || (!r.symbol.is_empty()
                        && symbol(&r.symbol).as_deref() != Some(old.symbol.as_str()))
                    || (!r.side.is_empty() && side(&r.side)? != old.side))
            {
                return Err("Orderly terminal child identity mismatch".into());
            }
            for child in &r.child_orders {
                walk(cx, child, parent, native, depth + 1, nodes)?;
            }
            Ok(())
        }
        walk(cx, r, None, None, 0, &mut 0)
    }

    fn commit_execution(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        prepared: PreparedExecution<'_, '_>,
        algo: bool,
        ts: u64,
    ) {
        if let Some((parent, final_status, seq, timestamp)) = prepared.root {
            let key = format!("root:{parent}");
            self.account
                .order_stamps
                .insert(key.clone(), (seq, timestamp));
            self.terminal_order(&key);
            self.account.order_revisions[1] += 1;
            let ids: Vec<_> = cx
                .store
                .orders()
                .iter()
                .filter(|o| o.parent_id.as_deref() == Some(parent.as_str()))
                .map(|o| o.id.clone())
                .collect();
            for id in ids {
                self.terminal_order(&id);
                cx.store.update_order(&id, |o| o.status = final_status);
            }
        }
        let rows = match prepared.rows {
            PreparedRows::StaleSparse => return,
            PreparedRows::Sparse {
                order,
                fill,
                seq,
                timestamp,
            } => {
                if let Some(fill) = fill {
                    self.commit_fill(cx, fill);
                }
                self.account
                    .order_stamps
                    .insert(order.id.clone(), (seq, timestamp));
                self.account.order_revisions[1] += 1;
                self.terminal_order(&order.id);
                self.account.proofs.remove(&order.id);
                cx.store.upsert_order(*order);
                return;
            }
            PreparedRows::Mapped(rows) => rows,
        };
        for (mapped, fill) in rows {
            let order = mapped.order;
            if let Some(fill) = fill {
                self.commit_fill(cx, fill);
            }
            let timestamp = mapped.timestamp.unwrap_or(ts);
            if !stamp_newer(
                self.account.order_stamps.get(&order.id),
                mapped.seq,
                timestamp,
            ) || ((self.account.terminal.contains(&order.id)
                || order
                    .parent_id
                    .as_ref()
                    .is_some_and(|p| self.account.terminal.contains(&format!("root:{p}"))))
                && order.status == OrderStatus::Open)
            {
                continue;
            }
            self.account
                .order_stamps
                .insert(order.id.clone(), (mapped.seq, timestamp));
            self.account.order_revisions[algo as usize] += 1;
            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                self.terminal_order(&order.id);
            }
            self.account.proofs.insert(order.id.clone(), mapped.proof);
            cx.store.upsert_order(order);
        }
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
    ) -> Result<(), String> {
        let env: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let socket = self.account.socket.socket.unwrap();
        self.account.socket.supervisor.received(cx.now);
        if env.event == "auth" {
            if env.success != Some(true) {
                cx.close(socket);
                self.private_close(cx, socket);
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    format!("Orderly socket authentication rejected: {}", env.message),
                );
                return Ok(());
            }
            if !self.account.authenticated {
                self.account.authenticated = true;
                self.account.socket.supervisor.authenticated(cx);
                for topic in ["executionreport", "algoexecutionreportv2", "position"] {
                    #[derive(Serialize)]
                    struct Sub<'a> {
                        topic: &'a str,
                        event: &'a str,
                    }
                    cx.send(
                        socket,
                        serde_json::to_string(&Sub {
                            topic,
                            event: "subscribe",
                        })
                        .unwrap(),
                    );
                }
                self.account.socket.ping_at = cx.now;
                cx.send(socket, "{\"event\":\"ping\"}");
                self.account_poll(cx, true);
            }
            return Ok(());
        }
        if env.event == "ping" {
            cx.send(socket, "{\"event\":\"pong\"}");
            return Ok(());
        }
        if env.event == "pong" {
            return Ok(());
        }
        if env.success == Some(false) {
            cx.close(socket);
            self.private_close(cx, socket);
            cx.notice(
                Level::Error,
                code::SUBSCRIPTION_REJECTED,
                format!("Orderly private subscription rejected: {}", env.message),
            );
            return Ok(());
        }
        if !self.account.authenticated {
            return Ok(());
        }
        let Some(data) = env.data else {
            return Ok(());
        };
        match env.topic.as_ref() {
            "executionreport" | "algoexecutionreportv2" => {
                let algo = env.topic == "algoexecutionreportv2";
                let rows: Vec<&serde_json::value::RawValue> = if data.get().starts_with('[') {
                    serde_json::from_str(data.get()).map_err(|e| e.to_string())?
                } else if data.get().starts_with('{') {
                    vec![data]
                } else {
                    return Err("Orderly execution container must be an object or array".into());
                };
                let mut admission = RowAdmission::<String>::default();
                for raw in rows {
                    let row: OrderRow<'_> = match serde_json::from_str(raw.get()) {
                        Ok(row) => row,
                        Err(error) => {
                            admission.skip(
                                cx,
                                order_key(raw.get(), algo),
                                Level::Warn,
                                format!("Orderly skipped execution row: {error}"),
                            );
                            continue;
                        }
                    };
                    let prepared = self.prepare_execution(cx, &row, algo, env.ts);
                    let message = prepared.as_ref().err().cloned().unwrap_or_default();
                    if let Some(prepared) = admission.admit(
                        cx,
                        prepared.map_err(|_| order_key(raw.get(), algo)),
                        Level::Warn,
                        message,
                    ) {
                        self.commit_execution(cx, prepared, algo, env.ts);
                    }
                }
            }
            "position" => {
                let push: PositionPush<'_> =
                    serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                let mut admission = RowAdmission::<String>::default();
                for raw in push.positions {
                    let r: PositionRow<'_> = match serde_json::from_str(raw.get()) {
                        Ok(row) => row,
                        Err(error) => {
                            admission.skip(
                                cx,
                                position_identity(raw.get()),
                                Level::Warn,
                                format!("Orderly skipped position row: {error}"),
                            );
                            continue;
                        }
                    };
                    let mapped = self.position(cx, &r);
                    let message = mapped.as_ref().err().cloned().unwrap_or_default();
                    if let Some(Some(p)) = admission.admit(
                        cx,
                        mapped.map_err(|_| position_identity(raw.get())),
                        Level::Warn,
                        message,
                    ) {
                        let ts = r.timestamp.unwrap_or(env.ts);
                        if stamp_newer(self.account.position_stamps.get(&p.id), r.seq, ts) {
                            self.account
                                .position_stamps
                                .insert(p.id.clone(), (r.seq, ts));
                            self.account.positions_revision += 1;
                            cx.store.upsert_position(p);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.startup_retry == Some(timer) {
            self.account.startup_retry = None;
            self.account_read(cx, Request::Info);
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            self.account.ticks = self.account.ticks.wrapping_add(1);
            self.account_poll(cx, self.account.ticks.is_multiple_of(4));
            self.account.tick =
                Some(cx.set_poll_timer_with_bounds(self.poll_interval(), POLL_FLOOR));
            return true;
        }
        match self.account.socket.supervisor.on_timer(cx, timer) {
            SocketTick::Unhandled => return false,
            SocketTick::Reconnect => self.private_connect(cx),
            SocketTick::Ping => {
                if let Some(socket) = self.account.socket.socket {
                    self.account.socket.ping_at = cx.now;
                    cx.send(socket, "{\"event\":\"ping\"}");
                }
            }
            SocketTick::TimedOut(_) => {
                if let Some(socket) = self.account.socket.socket {
                    cx.close(socket);
                    self.private_close(cx, socket);
                }
            }
            SocketTick::Handled => {}
        }
        true
    }
}

#[cfg(test)]
mod security_poll_interval {
    use super::*;
    #[test]
    fn native_options_cannot_bypass_bounded_polling_or_pause_semantics() {
        for (value, expected) in [
            (0.001, 1000.0),
            (1e20, 120000.0),
            (f64::INFINITY, f64::INFINITY),
        ] {
            let venue = Orderly::new(crate::Options {
                tick_interval: Some(value),
                ..crate::Options::default()
            });
            assert_eq!(venue.poll_interval(), expected);
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/margin_mode_rows.rs"]
mod margin_mode_rows;
