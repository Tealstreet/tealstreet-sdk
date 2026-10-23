use crate::{Coinbase, Pending, ProductKind, account_wire as wire};
use safe_cex_engine::Level;
use safe_cex_engine::backoff::Backoff;
use safe_cex_engine::coverage::{RowRepeat, RowReports, SnapshotBuilder};
use safe_cex_engine::execution::{Arithmetic, Cursor};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::url::encode;
use safe_cex_engine::{Cx, Fill, Method, Response, SocketChannel, SocketId, TimerId};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const ROOT: &str = "/api/v3/brokerage";
const STATUSES: [&str; 5] = ["PENDING", "OPEN", "QUEUED", "CANCEL_QUEUED", "EDIT_QUEUED"];
const MAX_ROWS: usize = 10000;
fn skip_level(kept: bool) -> Level {
    if kept { Level::Warn } else { Level::Error }
}
fn order_kept(store: &safe_cex_engine::Store, terminal: &HashSet<String>, id: &str) -> bool {
    store
        .order(id)
        .is_some_and(|o| !terminal.contains(id) || o.status != OrderStatus::Open)
}
/// INTX UPnL: the sum over committed positions, retained copies of skipped rows included.
fn committed_upnl(store: &safe_cex_engine::Store) -> f64 {
    store.positions().iter().map(|p| p.unrealized_pnl).sum()
}
#[derive(Clone, Copy)]
enum SizeUnit {
    Base,
    Quote,
    Unknown,
}
struct DeferredOrder {
    text: String,
    snapshot: bool,
    live: bool,
}
#[derive(Clone)]
pub(crate) struct Editable {
    pub native_product: String,
    pub price: String,
    pub size: String,
    pub side: OrderSide,
}
#[derive(Clone)]
pub struct Orders {
    rows: HashMap<String, Order>,
    proofs: HashMap<String, Editable>,
    fill_values: HashMap<String, f64>,
    skipped: HashMap<String, Option<String>>,
    /// A row without an id: no current order may be removed by its absence.
    unidentified: bool,
    invalid: bool,
    revision: u64,
    kind: ProductKind,
    status: usize,
    page: usize,
    cursor: String,
    cursors: HashSet<String>,
}
#[derive(Clone)]
pub enum Request {
    /// `None` reads the trading key's portfolio, then `Some` compares the read key's.
    KeyPermissions {
        trading_portfolio: Option<String>,
    },
    Portfolios,
    Probe {
        portfolios: Vec<wire::Portfolio>,
        index: usize,
    },
    DetectCfm {
        portfolio: wire::Portfolio,
    },
    Balance,
    SpotBalance,
    Positions {
        rows: SnapshotBuilder<String, Position>,
        invalid: bool,
        read_revision: u64,
    },
    Orders(Box<Orders>),
    OrderDetails {
        id: String,
        socket: SocketId,
    },
}
#[derive(Default)]
struct Snapshot {
    revision: u64,
    rows: HashMap<String, Order>,
    proofs: HashMap<String, Editable>,
    fill_values: HashMap<String, f64>,
    skipped: HashMap<String, Option<String>>,
    raw_count: usize,
    invalid: bool,
    complete: bool,
    pending: usize,
}
#[derive(Default)]
pub(crate) struct Account {
    pub kind: Option<ProductKind>,
    pub portfolio_id: Option<String>,
    pub socket: Option<SocketId>,
    pub proofs: HashMap<String, Editable>,
    ready: bool,
    opened: bool,
    sequence: Option<u64>,
    supervisor: Supervisor,
    payload: f64,
    tick: Option<TimerId>,
    write_timer: Option<TimerId>,
    startup_retry: Option<(TimerId, Request)>,
    startup_failures: u8,
    inflight: [bool; 4],
    last_orders: f64,
    backoff_until: f64,
    revision: u64,
    snapshot_revision: u64,
    position_read_revision: Option<u64>,
    position_generation: u64,
    terminal_fills: HashMap<String, (u64, f64)>,
    versions: HashMap<String, u64>,
    // REST state must not consume a delayed execution or forget one already delivered.
    stream_totals: HashMap<String, f64>,
    stream_values: HashMap<String, f64>,
    stream_fifo: VecDeque<String>,
    units: HashMap<String, SizeUnit>,
    unit_fifo: VecDeque<String>,
    deferred: HashMap<String, Vec<DeferredOrder>>,
    deferred_rows: usize,
    deferred_bytes: usize,
    detail_retries: HashMap<String, (u8, Option<TimerId>)>,
    terminal: HashSet<String>,
    terminal_fifo: VecDeque<String>,
    snapshot: Option<Snapshot>,
    has_snapshot: bool,
    wire_snapshot_complete: bool,
    pending_clients: HashSet<String>,
    committed_orders_revision: u64,
    // Open orders the engine cannot represent (e.g. brackets placed on Coinbase).
    foreign: HashMap<String, String>,
    /// `position:`, `order:` or `holding:` plus the identity (empty if none) of logged skips.
    skip_logged: RowReports<String>,
}
impl Account {
    pub(crate) fn new() -> Self {
        Self {
            skip_logged: RowReports::new(RowRepeat::ErrorUpgrade),
            supervisor: Supervisor::new(Config {
                idle_timeout_ms: 30_000.0,
                backoff_base_ms: 2_000.0,
                backoff_cap_ms: 30_000.0,
                ..Config::default()
            }),
            ..Self::default()
        }
    }
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }
    /// Account revision at which the last committed order snapshot started.
    pub(crate) fn committed_orders_revision(&self) -> u64 {
        self.committed_orders_revision
    }
    pub(crate) fn forget_foreign(&mut self, id: &str) {
        self.foreign.remove(id);
    }
    pub(crate) fn foreign_orders<'a>(&'a self, symbol: &'a str) -> impl Iterator<Item = &'a str> {
        self.foreign
            .iter()
            .filter(move |(_, s)| s.as_str() == symbol)
            .map(|(id, _)| id.as_str())
    }
    pub(crate) fn editable(&self, id: &str) -> Option<&Editable> {
        self.proofs.get(id)
    }
    pub(crate) fn is_intx(&self) -> bool {
        self.kind == Some(ProductKind::Intx)
    }
    pub(crate) fn position_read_revision(&self) -> Option<u64> {
        self.position_read_revision
    }
    pub(crate) fn position_generation(&self) -> u64 {
        self.position_generation
    }
    pub(crate) fn order_revision(&self, id: &str) -> u64 {
        self.versions.get(id).copied().unwrap_or(0)
    }
    pub(crate) fn terminal_fill(&self, id: &str) -> Option<(u64, f64)> {
        self.terminal_fills.get(id).copied()
    }
}
fn decimal(s: &str) -> Result<f64, String> {
    safe_cex_engine::num::parse_decimal(s).ok_or_else(|| "Coinbase invalid decimal".into())
}
fn unsigned(s: &str) -> Result<f64, String> {
    let n = decimal(s)?;
    if n < 0.0 {
        Err("Coinbase negative decimal".into())
    } else {
        Ok(n)
    }
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
pub(crate) fn order_status(s: &str) -> Result<OrderStatus, String> {
    match s {
        "PENDING" | "OPEN" | "QUEUED" | "CANCEL_QUEUED" | "EDIT_QUEUED" => Ok(OrderStatus::Open),
        "FILLED" => Ok(OrderStatus::Closed),
        "CANCELLED" | "EXPIRED" | "FAILED" => Ok(OrderStatus::Canceled),
        _ => Err("Coinbase unknown order status".into()),
    }
}
pub(crate) fn order_side(s: &str) -> Result<OrderSide, String> {
    match s {
        "BUY" => Ok(OrderSide::Buy),
        "SELL" => Ok(OrderSide::Sell),
        _ => Err("Coinbase unknown side".into()),
    }
}
fn market<'a>(store: &'a safe_cex_engine::Store, product: &str) -> Option<&'a Market> {
    let alias = if let Some(base) = product.strip_suffix("-USD") {
        Some(format!("{base}-USDC"))
    } else {
        product
            .strip_suffix("-USDC")
            .filter(|_| !matches!(product, "USDT-USDC" | "EURC-USDC"))
            .map(|base| format!("{base}-USD"))
    };
    if product.ends_with("-USD")
        && let Some(m) = alias.as_deref().and_then(|s| store.market(s))
    {
        return Some(m);
    }
    store
        .market(product)
        .or_else(|| alias.as_deref().and_then(|s| store.market(s)))
}
#[derive(serde::Deserialize)]
struct RowIdentity<'a> {
    #[serde(borrow)]
    order_id: std::borrow::Cow<'a, str>,
    #[serde(default, borrow)]
    product_id: std::borrow::Cow<'a, str>,
    #[serde(default, borrow)]
    status: std::borrow::Cow<'a, str>,
}
/// Identity of a row that failed to map, so one row cannot discard a snapshot.
/// The symbol is `None` without product or status, empty for an order that is
/// not live or not on a loaded market, else the live order's symbol.
fn skipped_row(store: &safe_cex_engine::Store, text: &str) -> Option<(String, Option<String>)> {
    let row: RowIdentity<'_> = serde_json::from_str(text).ok()?;
    if !safe_id(&row.order_id) {
        return None;
    }
    let symbol = (!row.product_id.is_empty() && !row.status.is_empty()).then(|| {
        STATUSES
            .contains(&row.status.as_ref())
            .then(|| market(store, &row.product_id).map(|m| m.symbol.clone()))
            .flatten()
            .unwrap_or_default()
    });
    Some((row.order_id.into_owned(), symbol))
}
fn multiplier(m: &Market) -> Result<f64, String> {
    if m.is_expiring == Some(true) {
        m.contract_size
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or("Coinbase missing CFM contract size".into())
    } else {
        Ok(1.0)
    }
}
fn configuration_unit(c: &wire::Configuration<'_>) -> Result<SizeUnit, String> {
    let configs = [
        c.market_market_ioc.as_ref(),
        c.market_market_fok.as_ref(),
        c.limit_limit_gtc.as_ref(),
        c.limit_limit_gtd.as_ref(),
        c.limit_limit_fok.as_ref(),
        c.sor_limit_ioc.as_ref(),
        c.stop_limit_stop_limit_gtc.as_ref(),
        c.stop_limit_stop_limit_gtd.as_ref(),
    ];
    if configs.iter().flatten().count() != 1 {
        return Err("Coinbase ambiguous sizing configuration".into());
    }
    let size = configs.into_iter().flatten().next().unwrap();
    match (size.base_size.is_empty(), size.quote_size.is_empty()) {
        (false, true) if unsigned(&size.base_size)? > 0.0 => Ok(SizeUnit::Base),
        (true, false) if unsigned(&size.quote_size)? > 0.0 => Ok(SizeUnit::Quote),
        _ => Err("Coinbase ambiguous sizing configuration".into()),
    }
}
fn configuration_amount(c: &wire::Configuration<'_>, average: f64) -> Result<f64, String> {
    let unit = configuration_unit(c)?;
    let size = [
        c.market_market_ioc.as_ref(),
        c.market_market_fok.as_ref(),
        c.limit_limit_gtc.as_ref(),
        c.limit_limit_gtd.as_ref(),
        c.limit_limit_fok.as_ref(),
        c.sor_limit_ioc.as_ref(),
        c.stop_limit_stop_limit_gtc.as_ref(),
        c.stop_limit_stop_limit_gtd.as_ref(),
    ]
    .into_iter()
    .flatten()
    .next()
    .unwrap();
    match unit {
        SizeUnit::Base => unsigned(&size.base_size),
        SizeUnit::Quote => {
            let price = if c.market_market_ioc.is_some() || c.market_market_fok.is_some() {
                average
            } else {
                unsigned(&size.limit_price)?
            };
            if price <= 0.0 {
                return Err("Coinbase quote-sized order lacks price".into());
            }
            Ok(unsigned(&size.quote_size)? / price)
        }
        SizeUnit::Unknown => unreachable!(),
    }
}
struct Mapped {
    order: Order,
    proof: Option<Editable>,
    average: f64,
    unit: SizeUnit,
}
pub(crate) fn map_order(
    store: &safe_cex_engine::Store,
    text: &str,
    now: f64,
) -> Result<(Order, Option<Editable>), String> {
    let m = map_order_detail(store, text, now)?;
    Ok((m.order, m.proof))
}
fn map_order_detail(
    store: &safe_cex_engine::Store,
    text: &str,
    now: f64,
) -> Result<Mapped, String> {
    map_order_with_unit(store, text, now, None, None)
}
fn map_order_with_unit(
    store: &safe_cex_engine::Store,
    text: &str,
    now: f64,
    known_unit: Option<SizeUnit>,
    known_amount: Option<f64>,
) -> Result<Mapped, String> {
    let r: wire::OrderRow<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !safe_id(&r.order_id) {
        return Err("Coinbase invalid order ID".into());
    }
    let m = market(store, &r.product_id).ok_or("Coinbase order market missing")?;
    let scale = multiplier(m)?;
    if !r.side.is_empty() && !r.order_side.is_empty() && r.side != r.order_side {
        return Err("Coinbase conflicting order side".into());
    }
    if let (Some(a), Some(b)) = (r.cumulative_quantity, r.filled_size)
        && a.0 != b.0
    {
        return Err("Coinbase conflicting filled size".into());
    }
    let side = order_side(if r.order_side.is_empty() {
        &r.side
    } else {
        &r.order_side
    })?;
    let status = order_status(&r.status)?;
    let filled = r
        .cumulative_quantity
        .or(r.filled_size)
        .map(|n| n.0)
        .ok_or("Coinbase missing filled size")?;
    let mut price = 0.0;
    let amount;
    let kind;
    let mut tif = None;
    let mut proof = None;
    let unit;
    if let Some(c) = &r.order_configuration {
        let configs = [
            c.market_market_ioc.as_ref(),
            c.market_market_fok.as_ref(),
            c.limit_limit_gtc.as_ref(),
            c.limit_limit_gtd.as_ref(),
            c.limit_limit_fok.as_ref(),
            c.sor_limit_ioc.as_ref(),
            c.stop_limit_stop_limit_gtc.as_ref(),
            c.stop_limit_stop_limit_gtd.as_ref(),
        ];
        if configs.iter().filter(|c| c.is_some()).count() != 1 {
            return Err("Coinbase ambiguous order configuration".into());
        }
        let config = configs
            .into_iter()
            .flatten()
            .next()
            .ok_or("Coinbase unsupported order configuration")?;
        kind = if c.market_market_ioc.is_some() || c.market_market_fok.is_some() {
            OrderType::Market
        } else if c.stop_limit_stop_limit_gtc.is_some() || c.stop_limit_stop_limit_gtd.is_some() {
            OrderType::StopLoss
        } else {
            OrderType::Limit
        };
        if kind == OrderType::StopLoss {
            price = unsigned(&config.stop_price)?;
        } else if !config.limit_price.is_empty() {
            price = unsigned(&config.limit_price)?;
        }
        if kind == OrderType::Market {
            price = r.average_filled_price.unwrap_or(0.0);
        }
        unit = configuration_unit(c)?;
        amount = if !config.base_size.is_empty() {
            unsigned(&config.base_size)?
        } else {
            let average = if kind == OrderType::Market {
                r.average_filled_price.unwrap_or(0.0)
            } else {
                price
            };
            if average <= 0.0 {
                return Err("Coinbase quote-sized order lacks price".into());
            }
            unsigned(&config.quote_size)? / average
        };
        tif = if config.post_only {
            Some(TimeInForce::PostOnly)
        } else if c.sor_limit_ioc.is_some() || c.market_market_ioc.is_some() {
            Some(TimeInForce::ImmediateOrCancel)
        } else if c.market_market_fok.is_some() || c.limit_limit_fok.is_some() {
            Some(TimeInForce::FillOrKill)
        } else {
            Some(TimeInForce::GoodTillCancel)
        };
        if kind == OrderType::Limit
            && !config.base_size.is_empty()
            && matches!(r.order_type.as_ref(), "LIMIT" | "Limit")
            && m.contract_size == Some(1.0)
            && m.is_spot == Some(true)
            && m.is_expiring != Some(true)
            && scale == 1.0
            && r.status == "OPEN"
            && filled == 0.0
            && r.reduce_only != Some(true)
            && price > 0.0
            && amount > 0.0
            && safe_cex_engine::num::adjust(price, m.precision.price) == price
            && safe_cex_engine::num::adjust(amount, m.precision.amount) == amount
        {
            proof = Some(Editable {
                native_product: r.product_id.to_string(),
                price: config.limit_price.to_string(),
                size: config.base_size.to_string(),
                side,
            });
        }
    } else {
        kind = match r.order_type.as_ref() {
            "Market" | "MARKET" => OrderType::Market,
            "Limit" | "LIMIT" => OrderType::Limit,
            "Stop Limit" | "STOP_LIMIT" => OrderType::StopLoss,
            _ => return Err("Coinbase unsupported user order type".into()),
        };
        let remaining = r
            .leaves_quantity
            .ok_or("Coinbase missing remaining size")?
            .0;
        if kind == OrderType::StopLoss && !r.stop_price.is_empty() {
            price = unsigned(&r.stop_price)?
        } else if !r.limit_price.is_empty() {
            price = unsigned(&r.limit_price)?
        }
        unit = known_unit
            .or_else(|| (m.is_spot != Some(true)).then_some(SizeUnit::Base))
            .ok_or("Coinbase user order sizing provenance missing")?;
        let remaining = match unit {
            SizeUnit::Base => remaining,
            SizeUnit::Quote if remaining == 0.0 => 0.0,
            SizeUnit::Unknown if remaining == 0.0 => 0.0,
            SizeUnit::Unknown => return Err("Coinbase user order sizing provenance missing".into()),
            SizeUnit::Quote => {
                let conversion = if kind == OrderType::Market {
                    r.avg_price.or(r.average_filled_price).unwrap_or(0.0)
                } else {
                    unsigned(&r.limit_price)?
                };
                if conversion <= 0.0 {
                    return Err("Coinbase quote-sized user order lacks price".into());
                }
                remaining / conversion
            }
        };
        // A zero-fill terminal update contains no original size. Recover it only
        // from an existing order, our placement reservation, or typed REST sizing.
        amount = if filled == 0.0 && remaining == 0.0 && status != OrderStatus::Open {
            known_amount.ok_or("Coinbase terminal order original size missing")?
        } else {
            filled + remaining
        };
        if kind == OrderType::Limit
            && matches!(r.order_type.as_ref(), "LIMIT" | "Limit")
            && m.contract_size == Some(1.0)
            && m.is_spot == Some(true)
            && m.is_expiring != Some(true)
            && scale == 1.0
            && matches!(unit, SizeUnit::Base)
            && r.status == "OPEN"
            && filled == 0.0
            && r.reduce_only != Some(true)
            && (r.stop_price.is_empty() || unsigned(&r.stop_price)? == 0.0)
            && price > 0.0
            && amount > 0.0
            && safe_cex_engine::num::adjust(price, m.precision.price) == price
            && safe_cex_engine::num::adjust(amount, m.precision.amount) == amount
        {
            proof = Some(Editable {
                native_product: r.product_id.to_string(),
                price: r.limit_price.to_string(),
                size: amount.to_string(),
                side,
            });
        }
    }
    if !amount.is_finite()
        || amount <= 0.0
        || filled < 0.0
        || filled > amount
        || price < 0.0
        || !price.is_finite()
    {
        return Err("Coinbase invalid order economics".into());
    }
    let average = r.avg_price.or(r.average_filled_price).unwrap_or(0.0);
    if average < 0.0 || (filled > 0.0 && average <= 0.0) || !(average * filled * scale).is_finite()
    {
        return Err("Coinbase filled order lacks valid cumulative execution price".into());
    }
    let amount_base = amount * scale;
    let filled_base = filled * scale;
    let remaining = (amount - filled) * scale;
    if !amount_base.is_finite() || !filled_base.is_finite() || !remaining.is_finite() {
        return Err("Coinbase order amount overflow".into());
    }
    if let Some(candidate) = &mut proof {
        let exact = serde_json::from_str::<wire::EditSource<'_>>(text).ok();
        let valid = exact.as_ref().is_some_and(|e| {
            (r.stop_price.is_empty() || unsigned(&r.stop_price) == Ok(0.0))
                && (r.limit_price.is_empty() || unsigned(&r.limit_price) == Ok(price))
                && r.leaves_quantity.is_none_or(|n| n.0 == amount - filled)
                && e.attached_order_id.is_empty()
                && e.attached_order_configuration.is_none()
                && e.cumulative_quantity
                    .as_deref()
                    .or(e.filled_size.as_deref())
                    .is_some_and(|s| unsigned(s) == Ok(0.0))
        });
        if !valid {
            proof = None;
        } else if r.order_configuration.is_none() {
            let native = exact
                .as_ref()
                .and_then(|e| e.leaves_quantity.as_deref())
                .filter(|s| unsigned(s).is_ok_and(|n| n == amount && n > 0.0));
            if let Some(native) = native {
                candidate.size = native.into();
            } else {
                proof = None;
            }
        }
    }
    let client = (!r.client_order_id.is_empty()).then(|| r.client_order_id.to_string());
    Ok(Mapped {
        order: Order {
            id: r.order_id.into_owned(),
            parent_id: client.clone(),
            client_order_id: client,
            status,
            symbol: m.symbol.clone(),
            kind,
            side,
            price,
            amount: amount_base,
            filled: filled_base,
            remaining,
            reduce_only: r.reduce_only.unwrap_or(false),
            close: None,
            time_in_force: tif,
            last_update: now,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(amount - filled),
                filled_in_placement_units: Some(filled),
                ..Default::default()
            }),
        },
        proof,
        average,
        unit,
    })
}
impl Coinbase {
    fn remember_unit(&mut self, id: &str, unit: SizeUnit) {
        if matches!(unit, SizeUnit::Unknown) {
            return;
        }
        if self.account.units.insert(id.into(), unit).is_none() {
            self.account.unit_fifo.push_back(id.into());
        }
        while self.account.unit_fifo.len() > MAX_ROWS * 2 {
            if let Some(old) = self.account.unit_fifo.pop_front() {
                self.account.units.remove(&old);
            }
        }
    }
    pub(crate) fn private_base_order(&mut self, cx: &mut Cx<'_, Pending>, id: &str, order: &Order) {
        // A duplicate client ID can return an existing quote-sized order. Its
        // authoritative native configuration outranks our outgoing request.
        if matches!(self.account.units.get(id), Some(SizeUnit::Quote)) {
            return;
        }
        self.remember_unit(id, SizeUnit::Base);
        let amount = order.extra.as_ref().and_then(|e| e.remaining_native);
        for pending in self.take_deferred(id) {
            if pending.snapshot
                && let Some(s) = &mut self.account.snapshot
            {
                s.pending -= 1;
            }
            let mapped = map_order_with_unit(
                cx.store,
                &pending.text,
                cx.now,
                Some(SizeUnit::Base),
                amount,
            )
            .and_then(|mapped| {
                if mapped.order.symbol != order.symbol || mapped.order.side != order.side {
                    return Err(
                        "Coinbase placement outcome conflicts with deferred stream identity".into(),
                    );
                }
                Ok(mapped)
            });
            match mapped {
                Ok(mapped) if pending.snapshot => self.stage_user_order(mapped),
                Ok(mapped) => self.apply_user_order(cx, mapped, pending.live),
                Err(error) => {
                    cx.notice(Level::Warn, code::ROW_SKIPPED, error);
                    if let Some(s) = &mut self.account.snapshot {
                        s.invalid = true;
                    }
                }
            }
        }
        self.finish_user_snapshot(cx);
    }
    fn stream_baseline(&mut self, cx: &Cx<'_, Pending>, id: &str, filled: f64, value: f64) {
        if self
            .account
            .stream_totals
            .insert(id.into(), filled)
            .is_none()
        {
            self.account.stream_fifo.push_back(id.into());
        }
        self.account.stream_values.insert(id.into(), value);
        let mut attempts = self.account.stream_fifo.len();
        while self.account.stream_fifo.len() > MAX_ROWS * 2 && attempts > 0 {
            attempts -= 1;
            if let Some(old) = self.account.stream_fifo.pop_front() {
                if cx
                    .store
                    .order(&old)
                    .is_some_and(|o| o.status == OrderStatus::Open)
                {
                    self.account.stream_fifo.push_back(old);
                } else {
                    self.account.stream_totals.remove(&old);
                    self.account.stream_values.remove(&old);
                }
            }
        }
    }
    fn map_user_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
        snapshot: bool,
    ) -> Result<Option<Mapped>, String> {
        let row: wire::OrderRow<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let m = market(cx.store, &row.product_id).ok_or("Coinbase order market missing")?;
        let terminal_zero = row.leaves_quantity.is_some_and(|n| n.0 == 0.0)
            && matches!(
                row.status.as_ref(),
                "FILLED" | "CANCELLED" | "EXPIRED" | "FAILED"
            );
        let needs_amount = terminal_zero
            && row
                .cumulative_quantity
                .or(row.filled_size)
                .is_some_and(|n| n.0 == 0.0);
        let known_amount = if needs_amount {
            let scale = multiplier(m)?;
            let side = order_side(if row.order_side.is_empty() {
                &row.side
            } else {
                &row.order_side
            })?;
            cx.store
                .order(&row.order_id)
                .filter(|o| o.symbol == m.symbol && o.side == side)
                .map(|o| o.amount / scale)
                .or_else(|| {
                    self.trading_order_amount(&row.client_order_id, &row.order_id, &m.symbol, side)
                })
        } else {
            None
        };
        let known = self
            .account
            .units
            .get(row.order_id.as_ref())
            .copied()
            .or_else(|| {
                self.account
                    .pending_clients
                    .contains(row.client_order_id.as_ref())
                    .then_some(SizeUnit::Base)
            });
        let known = known.or_else(|| {
            // A terminal zero remainder has no placement-currency quantity to convert.
            (terminal_zero && (!needs_amount || known_amount.is_some()))
                .then_some(SizeUnit::Unknown)
        });
        if !self.account.deferred.contains_key(row.order_id.as_ref())
            && (row.order_configuration.is_some()
                || (!needs_amount || known_amount.is_some())
                    && (known.is_some() || m.is_spot != Some(true)))
        {
            let mapped = map_order_with_unit(cx.store, text, cx.now, known, known_amount)?;
            self.remember_unit(&mapped.order.id, mapped.unit);
            return Ok(Some(mapped));
        }
        if !safe_id(&row.order_id)
            || text.len() > 65536
            || self.account.deferred_rows >= MAX_ROWS
            || self.account.deferred_bytes + text.len() > 4 * 1024 * 1024
            || self
                .account
                .deferred
                .get(row.order_id.as_ref())
                .is_some_and(|rows| rows.len() >= 64)
        {
            return Err("Coinbase unresolved user order limit".into());
        }
        let id = row.order_id.into_owned();
        let first = !self.account.deferred.contains_key(&id);
        self.account
            .deferred
            .entry(id.clone())
            .or_default()
            .push(DeferredOrder {
                text: text.into(),
                snapshot,
                live: self.account.wire_snapshot_complete,
            });
        self.account.deferred_rows += 1;
        self.account.deferred_bytes += text.len();
        if snapshot && let Some(s) = &mut self.account.snapshot {
            s.pending += 1;
        }
        if first {
            let socket = self
                .account
                .socket
                .ok_or("Coinbase private socket missing")?;
            self.account.detail_retries.insert(id.clone(), (0, None));
            self.account_read(cx, Request::OrderDetails { id, socket });
        }
        Ok(None)
    }
    fn take_deferred(&mut self, id: &str) -> Vec<DeferredOrder> {
        self.account.detail_retries.remove(id);
        let rows = self.account.deferred.remove(id).unwrap_or_default();
        self.account.deferred_rows -= rows.len();
        self.account.deferred_bytes -= rows.iter().map(|row| row.text.len()).sum::<usize>();
        rows
    }
    fn fail_details(&mut self, cx: &mut Cx<'_, Pending>, id: &str, socket: SocketId) {
        if self.account.socket != Some(socket) {
            return;
        }
        if let Some((attempt, timer)) = self.account.detail_retries.get_mut(id)
            && *attempt < 3
        {
            *attempt += 1;
            let delay = cx
                .jittered(Backoff::new(1_000.0, 3, f64::INFINITY).delay(u32::from(*attempt)))
                .max(self.account.backoff_until - cx.now);
            *timer = Some(cx.set_timer(delay));
            return;
        }
        let rows = self.take_deferred(id);
        if let Some(s) = &mut self.account.snapshot {
            s.pending -= rows.iter().filter(|r| r.snapshot).count();
            s.invalid = true;
        }
        self.finish_user_snapshot(cx);
        self.account_poll(cx, true);
    }
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            return;
        }
        if self.read_credentials.is_some() {
            self.account_read(
                cx,
                Request::KeyPermissions {
                    trading_portfolio: None,
                },
            );
        } else {
            self.account_read(cx, Request::Portfolios);
        }
    }
    fn read_key_unverified(&mut self, cx: &mut Cx<'_, Pending>, reason: &str) {
        self.drop_read_key(cx, reason);
        self.account.startup_failures = 0;
        self.account_read(cx, Request::Portfolios);
    }
    fn drop_read_key(&mut self, cx: &mut Cx<'_, Pending>, reason: &str) {
        cx.notice(
            Level::Error,
            code::CONFIG_INVALID,
            format!("Coinbase read-only key {reason}; reads use the trading key"),
        );
        self.read_credentials = None;
    }
    fn account_read(&mut self, cx: &mut Cx<'_, Pending>, req: Request) {
        let id = self.account.portfolio_id.as_deref().unwrap_or("");
        let (path, at) = match &req {
            Request::KeyPermissions { .. } => (format!("{ROOT}/key_permissions"), None),
            Request::Portfolios => (format!("{ROOT}/portfolios"), None),
            Request::Probe { portfolios, index } => (
                format!("{ROOT}/portfolios/{}", portfolios[*index].uuid),
                None,
            ),
            Request::DetectCfm { .. } => (format!("{ROOT}/cfm/balance_summary"), None),
            Request::OrderDetails { id, .. } => (format!("{ROOT}/orders/historical/{id}"), None),
            Request::Balance => {
                if self.account.kind == Some(ProductKind::Intx) {
                    (format!("{ROOT}/intx/portfolio/{id}"), None)
                } else {
                    (format!("{ROOT}/cfm/balance_summary"), None)
                }
            }
            Request::SpotBalance => (format!("{ROOT}/portfolios/{id}"), None),
            Request::Positions { .. } => {
                if self.account.kind == Some(ProductKind::Intx) {
                    (
                        format!("{ROOT}/intx/positions/{id}"),
                        Some(ArrayAt::Path(&["positions"])),
                    )
                } else {
                    (
                        format!("{ROOT}/cfm/positions"),
                        Some(ArrayAt::Path(&["positions"])),
                    )
                }
            }
            Request::Orders(s) => {
                let product = if s.kind == ProductKind::Spot {
                    "SPOT"
                } else {
                    "FUTURE"
                };
                let expiry = match s.kind {
                    ProductKind::Intx => "&contract_expiry_type=PERPETUAL",
                    ProductKind::Cfm => "&contract_expiry_type=EXPIRING",
                    ProductKind::Spot => "",
                };
                let cursor = if s.cursor.is_empty() {
                    String::new()
                } else {
                    format!("&cursor={}", encode(&s.cursor))
                };
                (
                    format!(
                        "{ROOT}/orders/historical/batch?product_type={product}{expiry}&order_status={}&limit=100{cursor}",
                        STATUSES[s.status]
                    ),
                    Some(ArrayAt::Path(&["orders"])),
                )
            }
        };
        let retry = match &req {
            Request::Portfolios | Request::Probe { .. } | Request::DetectCfm { .. } => {
                Some(req.clone())
            }
            _ => None,
        };
        let slot = match &req {
            Request::Balance => Some(0),
            Request::SpotBalance => Some(1),
            Request::Positions { .. } => Some(2),
            Request::Orders(_) => Some(3),
            _ => None,
        };
        let detail = matches!(
            req,
            Request::OrderDetails { .. } | Request::KeyPermissions { .. }
        )
        .then(|| req.clone());
        if let Err(e) = self.request(
            cx,
            Method::Get,
            path,
            None,
            at,
            Pending::Account(req, false),
        ) {
            if let Some(req) = retry {
                self.private_failed(cx, req, &e)
            } else if let Some(req) = detail {
                self.private_failed(cx, req, &e)
            } else {
                cx.notice(Level::Warn, code::REQUEST_FAILED, e);
                if let Some(slot) = slot {
                    self.account.inflight[slot] = false;
                }
            }
        }
    }
    fn select_portfolio(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        p: wire::Portfolio,
        kind: ProductKind,
    ) {
        self.account.portfolio_id = Some(p.uuid);
        self.account.kind = Some(kind);
        self.account.startup_failures = 0;
        self.private_ready(kind, cx);
    }
    pub(crate) fn private_after_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() || self.account.kind.is_none() || self.account.ready {
            return;
        }
        self.account.ready = true;
        cx.store.set_hedged(false);
        cx.store.retain_position_ids();
        for m in cx
            .store
            .markets()
            .iter()
            .map(|m| (m.symbol.clone(), m.is_spot == Some(true)))
            .collect::<Vec<_>>()
        {
            cx.store.update_setting(&m.0, |s| {
                s.margin_type = Some(MarginType::Cross);
                s.leverage = if m.1 { 1.0 } else { 10.0 };
            });
        }
        self.private_connect(cx);
        self.account_poll(cx, true);
        self.account.tick = Some(cx.set_poll_timer(5000.0));
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.opened = false;
        self.account.payload = cx.now;
        self.account.sequence = None;
        self.account.snapshot = None;
        self.account.deferred.clear();
        self.account.deferred_rows = 0;
        self.account.deferred_bytes = 0;
        for (_, (_, timer)) in self.account.detail_retries.drain() {
            if let Some(timer) = timer {
                cx.clear_timer(timer);
            }
        }
        self.account.has_snapshot = false;
        self.account.wire_snapshot_complete = false;
        self.account.supervisor.connecting(cx);
        self.account.socket = Some(
            cx.connect(
                safe_cex_engine::SocketSpec::new("wss://advanced-trade-ws-user.coinbase.com")
                    .channel(SocketChannel::Private),
            ),
        );
    }
    pub(crate) fn private_on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.opened = true;
        self.account.supervisor.opened(cx);
        self.account.snapshot = Some(Snapshot {
            revision: self.account.revision,
            ..Default::default()
        });
        #[derive(Serialize)]
        struct Sub<'a> {
            #[serde(rename = "type")]
            kind: &'static str,
            channel: &'a str,
            jwt: String,
        }
        for channel in ["user", "heartbeats"] {
            let signed = self
                .credentials
                .as_ref()
                .ok_or("Coinbase credentials missing".to_string())
                .and_then(|c| c.jwt(cx.now, None, |bytes| cx.fill_signing_entropy(bytes)));
            match signed {
                Ok(jwt) => cx.send(
                    socket,
                    serde_json::to_string(&Sub {
                        kind: "subscribe",
                        channel,
                        jwt,
                    })
                    .unwrap(),
                ),
                Err(e) => {
                    cx.notice(Level::Error, code::STREAM_UNAVAILABLE, e);
                    self.private_disconnect(cx);
                    break;
                }
            }
        }
        true
    }
    pub(crate) fn private_on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.private_disconnect(cx);
        true
    }
    fn private_disconnect(&mut self, cx: &mut Cx<'_, Pending>) {
        if let Some(s) = self.account.socket.take() {
            cx.close(s)
        }
        self.account.opened = false;
        self.account.snapshot = None;
        self.account.deferred.clear();
        self.account.deferred_rows = 0;
        self.account.deferred_bytes = 0;
        for (_, (_, timer)) in self.account.detail_retries.drain() {
            if let Some(timer) = timer {
                cx.clear_timer(timer);
            }
        }
        self.account.sequence = None;
        self.account.supervisor.retry(cx);
        self.account_poll(cx, true);
    }
    fn account_poll(&mut self, cx: &mut Cx<'_, Pending>, orders: bool) {
        if cx.now < self.account.backoff_until {
            return;
        }
        let Some(kind) = self.account.kind else {
            return;
        };
        if kind != ProductKind::Spot && !self.account.inflight[0] {
            self.account.inflight[0] = true;
            self.account_read(cx, Request::Balance)
        }
        if kind != ProductKind::Intx && !self.account.inflight[1] {
            self.account.inflight[1] = true;
            self.account_read(cx, Request::SpotBalance)
        }
        if !self.account.inflight[2] {
            if kind == ProductKind::Spot {
                cx.store.set_positions(vec![])
            } else {
                self.account.inflight[2] = true;
                self.account_read(
                    cx,
                    Request::Positions {
                        rows: SnapshotBuilder::default(),
                        invalid: false,
                        read_revision: self.account.revision,
                    },
                )
            }
        }
        if orders && !self.account.inflight[3] {
            self.account.inflight[3] = true;
            self.account.last_orders = cx.now;
            self.account_read(
                cx,
                Request::Orders(Box::new(Orders {
                    rows: HashMap::new(),
                    proofs: HashMap::new(),
                    fill_values: HashMap::new(),
                    skipped: HashMap::new(),
                    unidentified: false,
                    invalid: false,
                    revision: self.account.revision,
                    kind,
                    status: 0,
                    page: 1,
                    cursor: String::new(),
                    cursors: HashSet::new(),
                })),
            )
        }
    }
    pub(crate) fn private_placement_started(&mut self, client_id: &str) -> Result<(), String> {
        if self.account.pending_clients.len() >= 1000 {
            return Err("Coinbase unresolved placement limit".into());
        }
        self.account.pending_clients.insert(client_id.into());
        Ok(())
    }
    pub(crate) fn private_placement_finished(&mut self, client_id: &str) {
        self.account.pending_clients.remove(client_id);
    }
    pub(crate) fn private_after_write(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.revision = self.account.revision.wrapping_add(1);
        if let Some(t) = self.account.write_timer.take() {
            cx.clear_timer(t)
        }
        self.account.write_timer = Some(cx.set_timer(2000.0));
    }
    pub(crate) fn private_order_changed(&mut self, id: &str) {
        self.account.revision = self.account.revision.wrapping_add(1);
        self.account
            .versions
            .insert(id.into(), self.account.revision);
        self.account.proofs.remove(id);
    }
    pub(crate) fn private_order_canceled(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.private_order_changed(id);
        let filled = cx
            .store
            .order(id)
            .and_then(|o| o.extra.as_ref().and_then(|e| e.filled_in_placement_units))
            .unwrap_or(0.0);
        self.account
            .terminal_fills
            .insert(id.into(), (self.account.revision, filled));
        if self.account.terminal.insert(id.into()) {
            self.account.terminal_fifo.push_back(id.into());
        }
        while self.account.terminal_fifo.len() > MAX_ROWS {
            if let Some(old) = self.account.terminal_fifo.pop_front() {
                self.account.terminal.remove(&old);
                self.account.terminal_fills.remove(&old);
                self.account.versions.remove(&old);
            }
        }
        cx.store.update_order(id, |o| {
            if o.status != OrderStatus::Closed {
                o.status = OrderStatus::Canceled;
            }
            o.last_update = cx.now;
        });
        if let Some(snapshot) = &mut self.account.snapshot {
            snapshot.proofs.remove(id);
            if let Some(o) = snapshot.rows.get_mut(id)
                && o.status != OrderStatus::Closed
            {
                o.status = OrderStatus::Canceled;
            }
        }
    }
    pub(crate) fn private_on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if let Some(id) = self
            .account
            .detail_retries
            .iter()
            .find_map(|(id, (_, pending))| (*pending == Some(timer)).then(|| id.clone()))
        {
            self.account.detail_retries.get_mut(&id).unwrap().1 = None;
            if let Some(socket) = self.account.socket {
                self.account_read(cx, Request::OrderDetails { id, socket });
            }
            return true;
        }
        if self
            .account
            .startup_retry
            .as_ref()
            .is_some_and(|(t, _)| *t == timer)
        {
            let (_, r) = self.account.startup_retry.take().unwrap();
            self.account_read(cx, r);
            return true;
        }
        if self.account.supervisor.owns_timer(timer) {
            match self.account.supervisor.on_timer(cx, timer) {
                Tick::Reconnect => self.private_connect(cx),
                // Snapshots may wait on paused REST lookups; only user-frame silence expires them.
                Tick::Handled | Tick::Ping
                    if self.account.snapshot.is_none()
                        || cx.now - self.account.payload < 30_000.0 => {}
                Tick::TimedOut(_) | Tick::Handled | Tick::Ping => {
                    cx.notice(
                        Level::Warn,
                        code::STREAM_UNAVAILABLE,
                        "Coinbase private stream watchdog expired",
                    );
                    self.private_disconnect(cx)
                }
                Tick::Unhandled => {}
            }
            return true;
        }
        if self.account.write_timer == Some(timer) {
            self.account.write_timer = None;
            self.account_poll(cx, true);
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            let gap = if self.account.kind == Some(ProductKind::Cfm)
                || (self.account.opened && self.account.snapshot.is_none())
            {
                10000.0
            } else {
                3000.0
            };
            // Finish the initial multi-status scan before recurring reads consume
            // the shared three-request-per-second budget. Write repairs remain
            // independently debounced, and initial reads were already queued.
            if cx.store.loaded().orders || !self.account.inflight[3] {
                self.account_poll(
                    cx,
                    !cx.store.loaded().orders || cx.now - self.account.last_orders >= gap,
                );
            }
            self.account.tick = Some(cx.set_poll_timer(5000.0));
            return true;
        }
        false
    }
    pub(crate) fn private_on_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        req: &mut Request,
        text: &str,
    ) {
        let result: Result<(), String> = (|| {
            match req {
                Request::Positions { rows, .. } => {
                    if rows.rows().len() >= MAX_ROWS {
                        return Err("Coinbase positions exceed limit".into());
                    }
                    let p = match self.map_position(cx, text) {
                        Ok(p) => p,
                        Err(error) => {
                            // The store keeps its copy of a position whose row cannot map;
                            // with none to keep the row commits as flat, so it is an error.
                            let symbol = self.position_symbol(text);
                            let (stored, message) = match &symbol {
                                Some(symbol) => (
                                    cx.store.positions().iter().any(|p| &p.symbol == symbol),
                                    format!(
                                        "Coinbase skipped unmappable position {symbol}: {error}"
                                    ),
                                ),
                                None => (
                                    !cx.store.positions().is_empty(),
                                    "Coinbase skipped unmappable position without a symbol"
                                        .to_owned(),
                                ),
                            };
                            let level = if stored { Level::Warn } else { Level::Error };
                            let key = format!("position:{}", symbol.as_deref().unwrap_or(""));
                            self.account.skip_logged.report(cx, key, level, message);
                            rows.reject(symbol);
                            return Ok(());
                        }
                    };
                    if rows.rows().iter().any(|r| r.id == p.id) {
                        return Err("Coinbase duplicate position".into());
                    }
                    rows.push(p);
                }
                Request::Orders(s) => {
                    if s.rows.len() >= MAX_ROWS {
                        return Err("Coinbase orders exceed limit".into());
                    }
                    let mapped = match map_order_detail(cx.store, text, cx.now) {
                        Ok(mapped) => mapped,
                        Err(error) => {
                            let logged = &mut self.account.skip_logged;
                            let Some((id, symbol)) = skipped_row(cx.store, text) else {
                                let message = "Coinbase skipped unmappable order without an id";
                                let level =
                                    skip_level(cx.store.orders().iter().any(|o| {
                                        order_kept(cx.store, &self.account.terminal, &o.id)
                                    }));
                                logged.report(cx, "order:".into(), level, message);
                                s.unidentified = true;
                                return Ok(());
                            };
                            let message =
                                format!("Coinbase skipped unmappable order {id}: {error}");
                            let level =
                                skip_level(order_kept(cx.store, &self.account.terminal, &id));
                            logged.report(cx, format!("order:{id}"), level, message);
                            s.skipped.insert(id, symbol);
                            return Ok(());
                        }
                    };
                    self.remember_unit(&mapped.order.id, mapped.unit);
                    let o = mapped.order;
                    if mapped.average > 0.0 {
                        s.fill_values
                            .insert(o.id.clone(), o.filled * mapped.average);
                    }
                    if let Some(p) = mapped.proof {
                        s.proofs.insert(o.id.clone(), p);
                    }
                    s.rows.insert(o.id.clone(), o);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = result {
            match req {
                Request::Positions { invalid, .. } => *invalid = true,
                Request::Orders(s) => s.invalid = true,
                _ => {}
            }
            cx.notice(Level::Warn, code::REQUEST_FAILED, e)
        }
    }
    fn position_symbol(&self, text: &str) -> Option<String> {
        #[derive(serde::Deserialize)]
        struct Identity<'a> {
            #[serde(default, borrow)]
            symbol: std::borrow::Cow<'a, str>,
            #[serde(default, borrow)]
            product_id: std::borrow::Cow<'a, str>,
        }
        let row: Identity<'_> = serde_json::from_str(text).ok()?;
        let symbol = if self.account.kind == Some(ProductKind::Intx) {
            row.symbol
        } else {
            row.product_id
        };
        (!symbol.is_empty()).then(|| symbol.into_owned())
    }
    fn map_position(&self, cx: &Cx<'_, Pending>, text: &str) -> Result<Position, String> {
        let (symbol, side, size, entry, mark, pnl, leverage, liq) = if self.account.kind
            == Some(ProductKind::Intx)
        {
            let r: wire::IntxPosition<'_> =
                serde_json::from_str(text).map_err(|e| e.to_string())?;
            let size = r.net_size.0.abs();
            let side = match r.position_side.as_ref() {
                "POSITION_SIDE_LONG" => PositionSide::Long,
                "POSITION_SIDE_SHORT" => PositionSide::Short,
                // A flat row carries no side; it must still clear the position.
                _ if size == 0.0 => PositionSide::Long,
                _ => return Err("Coinbase invalid position side".into()),
            };
            let entry = r.entry_vwap.value.0;
            let mark = r.mark_price.value.0;
            let pnl = size
                * (mark - entry)
                * if side == PositionSide::Long {
                    1.0
                } else {
                    -1.0
                };
            (
                r.symbol.into_owned(),
                side,
                size,
                entry,
                mark,
                pnl,
                r.leverage.map_or(10.0, |n| n.0),
                r.liquidation_price.value.0,
            )
        } else {
            let r: wire::CfmPosition<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let m = cx
                .store
                .market(&r.product_id)
                .ok_or("Coinbase CFM market missing")?;
            if m.is_expiring != Some(true) {
                return Err("Coinbase CFM requires expiring market".into());
            }
            let scale = multiplier(m)?;
            let side = match r.side.as_ref() {
                "LONG" => PositionSide::Long,
                "SHORT" => PositionSide::Short,
                _ if r.number_of_contracts.0 == 0.0 => PositionSide::Long,
                _ => return Err("Coinbase invalid CFM side".into()),
            };
            (
                r.product_id.into_owned(),
                side,
                r.number_of_contracts.0.abs() * scale,
                r.avg_entry_price.0,
                r.current_price.0,
                r.unrealized_pnl.0,
                10.0,
                0.0,
            )
        };
        if cx.store.market(&symbol).is_none()
            || !size.is_finite()
            || !pnl.is_finite()
            || !(size * mark).is_finite()
            || entry < 0.0
            || mark < 0.0
            || leverage <= 0.0
            || liq < 0.0
        {
            return Err("Coinbase invalid position economics".into());
        }
        Ok(Position {
            id: symbol.clone(),
            symbol,
            side,
            entry_price: entry,
            notional: size * mark,
            leverage,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: pnl,
            realized_pnl: None,
            contracts: size,
            liquidation_price: liq,
            margin_size: None,
            last_update: Some(cx.now),
        })
    }
    pub(crate) fn private_on_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        req: Request,
        read_signed: bool,
        response: Response<'_>,
    ) {
        if let Request::OrderDetails { id, socket } = &req
            && (self.account.socket != Some(*socket) || !self.account.deferred.contains_key(id))
        {
            return;
        }
        if let Request::KeyPermissions { trading_portfolio } = req {
            if matches!(response.status, 408 | 429) || response.status >= 500 {
                if response.status == 429 {
                    self.account.backoff_until = cx.now + 60_000.0;
                }
                let error = format!("Coinbase key permissions HTTP {}", response.status);
                self.private_failed(cx, Request::KeyPermissions { trading_portfolio }, &error);
                return;
            }
            let portfolio = (200..300)
                .contains(&response.status)
                .then(|| serde_json::from_str::<wire::KeyPermissions>(response.body).ok())
                .flatten()
                .map(|k| k.portfolio_uuid)
                .filter(|p| !p.is_empty());
            match (trading_portfolio, portfolio) {
                (None, Some(portfolio)) => self.account_read(
                    cx,
                    Request::KeyPermissions {
                        trading_portfolio: Some(portfolio),
                    },
                ),
                (Some(trading), Some(read)) if trading == read => {
                    self.account.startup_failures = 0;
                    self.account_read(cx, Request::Portfolios)
                }
                (Some(_), Some(_)) => {
                    self.read_key_unverified(cx, "belongs to a different portfolio")
                }
                (None, None) if response.status == 401 => {
                    cx.notice(
                        Level::Error,
                        code::AUTH_REJECTED,
                        "Coinbase credentials rejected",
                    );
                    cx.deactivate();
                }
                _ => self.read_key_unverified(
                    cx,
                    &format!("could not be verified (HTTP {})", response.status),
                ),
            }
            return;
        }
        if !(200..300).contains(&response.status) {
            if response.status == 429 {
                let delay = response
                    .header("Retry-After")
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .map_or(60000.0, |n| (n * 1000.0).clamp(1000.0, 3600000.0));
                self.account.backoff_until = cx.now + delay;
            }
            if response.status == 401 {
                // The read-only key signed this read; keep trading on the trading key.
                if read_signed {
                    if self.read_credentials.is_some() {
                        self.drop_read_key(cx, "was rejected (HTTP 401)");
                    }
                    self.private_failed(cx, req, "Coinbase read-only key rejected");
                    return;
                }
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    "Coinbase credentials rejected",
                );
                cx.deactivate();
                return;
            }
            // Only throttling and server failures are worth retrying the same portfolio.
            let retryable = matches!(response.status, 408 | 429) || response.status >= 500;
            if !retryable {
                match req {
                    Request::Probe { portfolios, index } => {
                        cx.notice(
                            Level::Warn,
                            code::REQUEST_FAILED,
                            format!(
                                "Coinbase portfolio {} unavailable (HTTP {}); trying the next",
                                portfolios[index].uuid, response.status
                            ),
                        );
                        self.next_probe(cx, portfolios, index + 1);
                        return;
                    }
                    Request::DetectCfm { portfolio } => {
                        self.select_portfolio(cx, portfolio, ProductKind::Spot);
                        return;
                    }
                    // A read key without portfolio scope says nothing about the trading key.
                    Request::Portfolios if read_signed => {
                        let reason = format!("cannot list portfolios (HTTP {})", response.status);
                        if self.read_credentials.is_some() {
                            self.read_key_unverified(cx, &reason);
                        } else {
                            self.private_failed(cx, req, &reason);
                        }
                        return;
                    }
                    Request::Portfolios if matches!(response.status, 403 | 404) => {
                        cx.notice(
                            Level::Error,
                            code::AUTH_REJECTED,
                            "Coinbase credentials rejected",
                        );
                        cx.deactivate();
                        return;
                    }
                    req => {
                        self.private_failed(
                            cx,
                            req,
                            &format!("Coinbase account HTTP {}", response.status),
                        );
                        return;
                    }
                }
            }
            self.private_failed(
                cx,
                req,
                &format!("Coinbase account HTTP {}", response.status),
            );
            return;
        }
        let retry = match &req {
            Request::Portfolios | Request::Probe { .. } | Request::DetectCfm { .. } => {
                Some(req.clone())
            }
            _ => None,
        };
        let slot = match &req {
            Request::Balance => Some(0),
            Request::SpotBalance => Some(1),
            Request::Positions { .. } => Some(2),
            Request::Orders(_) => Some(3),
            _ => None,
        };
        let detail_failure = match &req {
            Request::OrderDetails { id, socket } => Some((id.clone(), *socket)),
            _ => None,
        };
        let result: Result<(), String> = (|| {
            match req {
                Request::OrderDetails { id, socket } => {
                    if self.account.socket != Some(socket) {
                        return Ok(());
                    }
                    let detail: wire::OrderDetail<'_> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    let row = detail.order;
                    if row.order_id != id {
                        return Err("Coinbase order detail identity mismatch".into());
                    }
                    let unit = match configuration_unit(&row.order_configuration) {
                        Ok(unit) => unit,
                        Err(error) => {
                            let deferred = self.take_deferred(&id);
                            let level = if deferred.iter().any(|p| p.snapshot) {
                                skip_level(order_kept(cx.store, &self.account.terminal, &id))
                            } else {
                                Level::Warn
                            };
                            self.account.skip_logged.report(
                                cx,
                                format!("order:{id}"),
                                level,
                                format!("Coinbase skipped unmappable order {id}: {error}"),
                            );
                            for pending in deferred {
                                if pending.snapshot
                                    && let Some(s) = &mut self.account.snapshot
                                {
                                    s.pending -= 1;
                                    let symbol = skipped_row(cx.store, &pending.text)
                                        .and_then(|(_, symbol)| symbol);
                                    s.skipped.insert(id.clone(), symbol);
                                }
                            }
                            self.finish_user_snapshot(cx);
                            return Ok(());
                        }
                    };
                    let deferred = self.take_deferred(&id);
                    for pending in deferred {
                        if pending.snapshot
                            && let Some(s) = &mut self.account.snapshot
                        {
                            s.pending -= 1;
                        }
                        let mapped = (|| {
                            let original: wire::OrderRow<'_> =
                                serde_json::from_str(&pending.text).map_err(|e| e.to_string())?;
                            if original.product_id != row.product_id {
                                return Err("Coinbase order detail product mismatch".into());
                            }
                            let amount = configuration_amount(
                                &row.order_configuration,
                                original
                                    .avg_price
                                    .or(original.average_filled_price)
                                    .unwrap_or(0.0),
                            )
                            .ok();
                            map_order_with_unit(cx.store, &pending.text, cx.now, Some(unit), amount)
                        })();
                        match mapped {
                            Ok(mapped) => {
                                self.remember_unit(&id, unit);
                                if pending.snapshot {
                                    self.stage_user_order(mapped);
                                } else {
                                    self.apply_user_order(cx, mapped, pending.live);
                                }
                            }
                            Err(error) => {
                                let level = if pending.snapshot {
                                    skip_level(order_kept(cx.store, &self.account.terminal, &id))
                                } else {
                                    Level::Warn
                                };
                                self.account.skip_logged.report(
                                    cx,
                                    format!("order:{id}"),
                                    level,
                                    format!("Coinbase skipped unmappable order {id}: {error}"),
                                );
                                if let Some(s) = &mut self.account.snapshot {
                                    if pending.snapshot {
                                        let symbol = skipped_row(cx.store, &pending.text)
                                            .and_then(|(_, symbol)| symbol);
                                        s.skipped.insert(id.clone(), symbol);
                                    } else {
                                        s.invalid = true;
                                    }
                                }
                            }
                        }
                    }
                    self.finish_user_snapshot(cx);
                }
                Request::KeyPermissions { .. } => unreachable!(),
                Request::Portfolios => {
                    let data: wire::Portfolios =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if data.portfolios.len() > 1000 {
                        return Err("Coinbase portfolio list exceeds limit".into());
                    }
                    let portfolios = data
                        .portfolios
                        .into_iter()
                        .filter(|p| !p.deleted && (!self.options.spot_only || p.kind != "INTX"))
                        .collect::<Vec<_>>();
                    if portfolios.iter().any(|p| !safe_id(&p.uuid)) {
                        return Err("Coinbase invalid portfolio ID".into());
                    }
                    self.next_probe(cx, portfolios, 0);
                }
                Request::Probe { portfolios, index } => {
                    #[derive(serde::Deserialize)]
                    struct Probe<'a> {
                        #[serde(borrow, rename = "breakdown")]
                        _breakdown: &'a serde_json::value::RawValue,
                    }
                    let _: Probe<'_> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    let p = portfolios[index].clone();
                    if self.options.spot_only {
                        self.select_portfolio(cx, p, ProductKind::Spot)
                    } else if p.kind == "INTX" {
                        self.select_portfolio(cx, p, ProductKind::Intx)
                    } else {
                        self.account_read(cx, Request::DetectCfm { portfolio: p })
                    }
                }
                Request::DetectCfm { portfolio } => {
                    let _: wire::CfmBalance =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    self.select_portfolio(cx, portfolio, ProductKind::Cfm)
                }
                Request::Balance => {
                    self.account.inflight[0] = false;
                    let (total, free, upnl) = if self.account.kind == Some(ProductKind::Intx) {
                        let r: wire::IntxBalance =
                            serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                        (
                            r.summary.total_balance.value.0,
                            r.summary.buying_power.value.0,
                            committed_upnl(cx.store),
                        )
                    } else {
                        let r: wire::CfmBalance =
                            serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                        (
                            r.balance_summary.total_usd_balance.value.0,
                            r.balance_summary.available_margin.value.0,
                            r.balance_summary.unrealized_pnl.map_or(0.0, |n| n.value.0),
                        )
                    };
                    if total < 0.0 || free < 0.0 || !upnl.is_finite() || !(total - free).is_finite()
                    {
                        return Err("Coinbase invalid balance".into());
                    }
                    cx.store.set_balance(Balance {
                        total,
                        free,
                        used: total - free,
                        upnl,
                    });
                }
                Request::SpotBalance => {
                    self.account.inflight[1] = false;
                    let r: wire::Breakdown =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if r.breakdown.spot_positions.len() > 1000 {
                        return Err("Coinbase spot balance limit".into());
                    }
                    let mut coins = SnapshotBuilder::default();
                    let mut seen = HashSet::new();
                    let held = |cx: &Cx<'_, Pending>, asset: Option<&str>| {
                        cx.store
                            .currency_balances()
                            .unwrap_or_default()
                            .iter()
                            .any(|c| asset.is_none_or(|a| c.currency == a))
                    };
                    for b in r.breakdown.spot_positions {
                        if b.asset.is_empty() {
                            self.account.skip_logged.report(
                                cx,
                                "holding:".into(),
                                skip_level(held(cx, None)),
                                "Coinbase skipped spot holding without an asset",
                            );
                            coins.reject(None);
                            continue;
                        }
                        if !seen.insert(b.asset.clone()) {
                            return Err("Coinbase duplicate spot holding".into());
                        }
                        if b.total_balance_crypto.0 < 0.0
                            || b.available_to_trade_crypto.0 < 0.0
                            || b.available_to_trade_crypto.0 > b.total_balance_crypto.0
                        {
                            self.account.skip_logged.report(
                                cx,
                                format!("holding:{}", b.asset),
                                skip_level(held(cx, Some(&b.asset))),
                                format!("Coinbase skipped invalid spot holding {}", b.asset),
                            );
                            coins.reject(Some(b.asset));
                            continue;
                        }
                        coins.push(CurrencyBalance {
                            currency: b.asset,
                            balance: Balance {
                                total: b.total_balance_crypto.0,
                                free: b.available_to_trade_crypto.0,
                                used: b.total_balance_crypto.0 - b.available_to_trade_crypto.0,
                                upnl: 0.0,
                            },
                        });
                    }
                    self.account.skip_logged.rearm_namespace(
                        "holding:",
                        coins.rejected() == 0,
                        coins.rows().iter().map(|c| c.currency.as_str()),
                    );
                    let current = cx.store.currency_balances().unwrap_or_default().to_vec();
                    cx.store
                        .set_currency_balances(coins.commit(current, |c| c.currency.clone()));
                    if self.account.kind == Some(ProductKind::Spot) {
                        cx.store.set_balance(Balance::default());
                    }
                }
                Request::Positions {
                    rows: builder,
                    invalid,
                    read_revision,
                } => {
                    self.account.inflight[2] = false;
                    if invalid {
                        return Err("Coinbase rejected partial position snapshot".into());
                    }
                    #[derive(serde::Deserialize)]
                    struct Meta {
                        positions: Vec<serde::de::IgnoredAny>,
                    }
                    let meta: Meta =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if !meta.positions.is_empty() {
                        return Err("Coinbase unstreamed positions".into());
                    }
                    // A flat placeholder may only stand where the snapshot's silence is proof.
                    let mut flat = vec![];
                    for m in cx.store.markets() {
                        if m.is_spot != Some(true)
                            && builder.allows_absence(&m.symbol)
                            && !builder.rows().iter().any(|p| p.symbol == m.symbol)
                        {
                            flat.push(Position {
                                id: m.symbol.clone(),
                                symbol: m.symbol.clone(),
                                side: PositionSide::Long,
                                entry_price: 0.0,
                                notional: 0.0,
                                leverage: 10.0,
                                margin_type: Some(MarginType::Cross),
                                position_mode: Some(PositionMode::Oneway),
                                unrealized_pnl: 0.0,
                                realized_pnl: None,
                                contracts: 0.0,
                                liquidation_price: 0.0,
                                margin_size: None,
                                last_update: Some(cx.now),
                            })
                        }
                    }
                    self.account.skip_logged.rearm_namespace(
                        "position:",
                        builder.rejected() == 0,
                        builder.rows().iter().map(|p| p.symbol.as_str()),
                    );
                    let mut rows =
                        builder.commit(cx.store.positions().iter().cloned(), |p| p.symbol.clone());
                    rows.extend(flat);
                    for p in &rows {
                        cx.store.update_setting(&p.symbol, |s| {
                            s.leverage = p.leverage;
                            s.margin_type = Some(MarginType::Cross)
                        });
                    }
                    cx.store.set_positions(rows);
                    self.account.position_read_revision = Some(read_revision);
                    self.account.position_generation =
                        self.account.position_generation.wrapping_add(1);
                    self.trading_positions(cx);
                    if self.account.kind == Some(ProductKind::Intx) && cx.store.loaded().balance {
                        let mut b = cx.store.balance().clone();
                        b.upnl = committed_upnl(cx.store);
                        cx.store.set_balance(b);
                    }
                }
                Request::Orders(mut s) => {
                    if s.invalid {
                        self.account.inflight[3] = false;
                        return Err("Coinbase rejected partial order snapshot".into());
                    }
                    let page: wire::Page<'_> =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    #[derive(serde::Deserialize)]
                    struct Meta {
                        orders: Vec<serde::de::IgnoredAny>,
                    }
                    let meta: Meta =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if !meta.orders.is_empty() {
                        return Err("Coinbase unstreamed orders".into());
                    }
                    if page.has_next {
                        if s.page >= 50
                            || page.cursor.is_empty()
                            || !s.cursors.insert(page.cursor.to_string())
                        {
                            return Err("Coinbase repeated or excessive order page".into());
                        }
                        s.page += 1;
                        s.cursor = page.cursor.to_string();
                        self.account_read(cx, Request::Orders(s));
                    } else if s.status + 1 < STATUSES.len() {
                        s.status += 1;
                        s.page = 1;
                        s.cursor.clear();
                        s.cursors.clear();
                        self.account_read(cx, Request::Orders(s));
                    } else if s.kind == ProductKind::Cfm {
                        s.kind = ProductKind::Spot;
                        s.status = 0;
                        s.page = 1;
                        s.cursor.clear();
                        s.cursors.clear();
                        self.account_read(cx, Request::Orders(s));
                    } else {
                        self.account.inflight[3] = false;
                        self.commit_orders(cx, *s);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            if let Some(req) = retry {
                self.private_failed(cx, req, &e);
            } else if let Some((id, socket)) = detail_failure {
                cx.notice(Level::Warn, code::REQUEST_FAILED, e);
                self.fail_details(cx, &id, socket);
            } else {
                cx.notice(Level::Warn, code::REQUEST_FAILED, e);
                if let Some(slot) = slot {
                    self.account.inflight[slot] = false;
                }
            }
        }
    }
    fn next_probe(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        portfolios: Vec<wire::Portfolio>,
        index: usize,
    ) {
        if index >= portfolios.len() {
            cx.notice(
                Level::Error,
                code::CONFIG_INVALID,
                "Coinbase no permitted portfolio found",
            );
            return;
        }
        self.account_read(cx, Request::Probe { portfolios, index })
    }
    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, mut s: Orders) {
        if s.revision < self.account.snapshot_revision {
            return;
        }
        self.account.skip_logged.rearm_namespace(
            "order:",
            s.skipped.is_empty() && !s.unidentified,
            s.rows.keys().map(String::as_str),
        );
        for o in cx.store.orders() {
            if self
                .account
                .versions
                .get(&o.id)
                .is_some_and(|v| *v > s.revision)
            {
                s.rows.insert(o.id.clone(), o.clone());
                s.proofs.remove(&o.id);
                if let Some(p) = self.account.proofs.get(&o.id) {
                    s.proofs.insert(o.id.clone(), p.clone());
                }
            }
        }
        // A snapshot whose only rows lack any identity says nothing about other orders.
        let unmapped =
            s.unidentified || s.rows.is_empty() && s.skipped.values().any(Option::is_none);
        for o in cx.store.orders() {
            if (unmapped || s.skipped.contains_key(&o.id)) && !s.rows.contains_key(&o.id) {
                s.rows.insert(o.id.clone(), o.clone());
            }
        }
        s.rows
            .retain(|id, o| !self.account.terminal.contains(id) || o.status != OrderStatus::Open);
        self.account.foreign = s
            .skipped
            .into_iter()
            .filter_map(|(id, symbol)| symbol.filter(|s| !s.is_empty()).map(|s| (id, s)))
            .filter(|(id, _)| !s.rows.contains_key(id))
            .collect();
        for o in s.rows.values() {
            self.trading_observed_order(cx, o);
        }
        self.account.proofs = s.proofs;
        self.account.proofs.retain(|id, _| {
            s.rows
                .get(id)
                .is_some_and(|o| o.status == OrderStatus::Open)
        });
        self.account
            .versions
            .retain(|id, _| s.rows.contains_key(id) || self.account.terminal.contains(id));
        let mut rows = s.rows.into_values().collect::<Vec<_>>();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        cx.store.set_orders(rows);
        self.account.committed_orders_revision =
            self.account.committed_orders_revision.max(s.revision);
    }
    pub(crate) fn private_failed(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        match req {
            Request::Portfolios | Request::Probe { .. } | Request::DetectCfm { .. } => {
                if self.account.startup_retry.is_none() && self.account.startup_failures < 5 {
                    self.account.startup_failures += 1;
                    let nominal = Backoff::new(1_000.0, 5, f64::INFINITY)
                        .delay(u32::from(self.account.startup_failures));
                    let delay = cx
                        .jittered(nominal)
                        .max(self.account.backoff_until - cx.now);
                    let t = cx.set_timer(delay);
                    self.account.startup_retry = Some((t, req));
                } else if self.account.startup_retry.is_none() {
                    cx.notice(
                        Level::Error,
                        code::REQUEST_FAILED,
                        format!("Coinbase account discovery failed after retries: {error}"),
                    );
                }
            }
            Request::Balance => self.account.inflight[0] = false,
            Request::SpotBalance => self.account.inflight[1] = false,
            Request::Positions { .. } => self.account.inflight[2] = false,
            Request::Orders(_) => self.account.inflight[3] = false,
            Request::OrderDetails { id, socket } => self.fail_details(cx, &id, socket),
            Request::KeyPermissions { .. } => {
                if self.account.startup_retry.is_none() && self.account.startup_failures < 3 {
                    self.account.startup_failures += 1;
                    let nominal = Backoff::new(1_000.0, 5, f64::INFINITY)
                        .delay(u32::from(self.account.startup_failures));
                    let delay = cx
                        .jittered(nominal)
                        .max(self.account.backoff_until - cx.now);
                    let t = cx.set_timer(delay);
                    self.account.startup_retry = Some((t, req));
                } else if self.account.startup_retry.is_none() {
                    self.read_key_unverified(cx, &format!("could not be verified ({error})"));
                }
            }
        }
    }
    pub(crate) fn private_on_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        socket: SocketId,
        text: &str,
    ) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        let parsed: Result<wire::UserFrame<'_>, _> = serde_json::from_str(text);
        let Ok(frame) = parsed else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Coinbase invalid private frame",
            );
            return true;
        };
        if frame.kind == "error" {
            cx.notice(
                Level::Warn,
                code::SUBSCRIPTION_REJECTED,
                format!("Coinbase private subscription rejected: {}", frame.message),
            );
            self.private_disconnect(cx);
            return true;
        }
        if !matches!(
            frame.channel.as_ref(),
            "user" | "heartbeats" | "subscriptions"
        ) {
            return true;
        }
        let Some(seq) = frame.sequence_num else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Coinbase private sequence missing",
            );
            self.private_disconnect(cx);
            return true;
        };
        if let Some(prior) = self.account.sequence {
            if seq <= prior {
                return true;
            }
            if prior.checked_add(1) != Some(seq) {
                cx.notice(
                    Level::Warn,
                    code::STREAM_UNAVAILABLE,
                    "Coinbase private sequence gap",
                );
                self.private_disconnect(cx);
                return true;
            }
        } else if seq != 0 {
            cx.notice(
                Level::Warn,
                code::STREAM_UNAVAILABLE,
                "Coinbase private initial sequence missing",
            );
            self.private_disconnect(cx);
            return true;
        }
        self.account.sequence = Some(seq);
        self.account.supervisor.received(cx.now);
        if frame.channel != "user" {
            return true;
        }
        let Some(raw_events) = frame.events else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Coinbase user events missing",
            );
            self.private_disconnect(cx);
            return true;
        };
        let Ok(parsed_events) = serde_json::from_str::<Vec<wire::UserEvent<'_>>>(raw_events.get())
        else {
            cx.notice(
                Level::Warn,
                code::STREAM_MALFORMED,
                "Coinbase invalid user events",
            );
            self.private_disconnect(cx);
            return true;
        };
        if parsed_events.len() > 1000 {
            self.private_disconnect(cx);
            return true;
        }
        let mut frame_rows = 0;
        for event in parsed_events {
            let Some(raw) = event.orders else { continue };
            frame_rows += raw.len();
            if frame_rows > MAX_ROWS || !matches!(event.kind.as_ref(), "snapshot" | "update") {
                self.private_disconnect(cx);
                return true;
            }
            let snapshot = event.kind == "snapshot";
            if snapshot {
                let revision = self.account.revision;
                let s = self.account.snapshot.get_or_insert_with(|| Snapshot {
                    revision,
                    ..Default::default()
                });
                s.raw_count += raw.len();
                s.complete = raw.len() < 50;
                self.account.wire_snapshot_complete |= s.complete;
                if s.raw_count > MAX_ROWS {
                    self.private_disconnect(cx);
                    return true;
                }
            }
            for text in raw {
                match self.map_user_order(cx, text.get(), snapshot) {
                    Ok(Some(mapped)) if snapshot => self.stage_user_order(mapped),
                    Ok(Some(mapped)) => {
                        self.apply_user_order(cx, mapped, self.account.wire_snapshot_complete)
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let skipped = skipped_row(cx.store, text.get());
                        if !snapshot
                            && let Some((id, _)) = &skipped
                            && cx.store.order(id).is_none()
                            && !self
                                .account
                                .snapshot
                                .as_ref()
                                .is_some_and(|s| s.rows.contains_key(id))
                        {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Coinbase ignored unmappable order update {id}: {error}"),
                            );
                            continue;
                        }
                        let skipped = skipped.filter(|_| snapshot);
                        if let Some(s) = &mut self.account.snapshot {
                            if let Some((id, symbol)) = skipped {
                                self.account.skip_logged.report(
                                    cx,
                                    format!("order:{id}"),
                                    skip_level(order_kept(cx.store, &self.account.terminal, &id)),
                                    format!("Coinbase skipped unmappable order {id}: {error}"),
                                );
                                s.skipped.insert(id, symbol);
                                continue;
                            }
                            s.invalid = true;
                        }
                        cx.notice(Level::Warn, code::ROW_SKIPPED, error);
                    }
                }
            }
            self.finish_user_snapshot(cx);
        }
        self.account.payload = cx.now;
        true
    }
    fn stage_user_order(&mut self, mapped: Mapped) {
        let Some(s) = &mut self.account.snapshot else {
            return;
        };
        if mapped.average > 0.0 {
            s.fill_values.insert(
                mapped.order.id.clone(),
                mapped.order.filled * mapped.average,
            );
        }
        if let Some(p) = mapped.proof {
            s.proofs.insert(mapped.order.id.clone(), p);
        }
        s.rows.insert(mapped.order.id.clone(), mapped.order);
    }
    fn finish_user_snapshot(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self
            .account
            .snapshot
            .as_ref()
            .is_some_and(|s| s.complete && s.pending == 0)
        {
            return;
        }
        let s = self.account.snapshot.take().unwrap();
        if s.invalid {
            cx.notice(
                Level::Warn,
                code::SNAPSHOT_DISCARDED,
                "Coinbase rejected partial user snapshot",
            );
            if !self.account.has_snapshot {
                // Initial snapshot rows supply execution baselines. Reconnect
                // rather than process live cumulative fills without those rows.
                self.private_disconnect(cx);
                return;
            }
            self.account_poll(cx, true);
            return;
        }
        self.account.has_snapshot = true;
        for o in s.rows.values() {
            if self
                .account
                .stream_totals
                .get(&o.id)
                .is_none_or(|prior| o.filled > *prior)
            {
                self.stream_baseline(
                    cx,
                    &o.id,
                    o.filled,
                    s.fill_values.get(&o.id).copied().unwrap_or(0.0),
                );
            }
        }
        let revision = s.revision;
        let mut changed = cx
            .store
            .orders()
            .iter()
            .map(|o| o.id.clone())
            .collect::<HashSet<_>>();
        changed.extend(s.rows.keys().cloned());
        self.commit_orders(
            cx,
            Orders {
                rows: s.rows,
                proofs: s.proofs,
                fill_values: s.fill_values,
                skipped: s.skipped,
                unidentified: false,
                invalid: false,
                revision,
                kind: self.account.kind.unwrap_or(ProductKind::Spot),
                status: 0,
                page: 1,
                cursor: String::new(),
                cursors: HashSet::new(),
            },
        );
        self.account.revision = self.account.revision.wrapping_add(1);
        self.account.snapshot_revision = self.account.revision;
        for id in changed {
            self.account.versions.insert(id, self.account.revision);
        }
    }
    fn apply_user_order(&mut self, cx: &mut Cx<'_, Pending>, mapped: Mapped, live: bool) {
        let mut o = mapped.order;
        if cx.store.order(&o.id).is_none() && cx.store.orders().len() >= MAX_ROWS {
            cx.notice(Level::Warn, code::ROW_SKIPPED, "Coinbase live order limit");
            self.account_poll(cx, true);
            return;
        }
        let old = self
            .account
            .stream_totals
            .get(&o.id)
            .copied()
            .or_else(|| {
                self.account
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.rows.get(&o.id))
                    .map(|o| o.filled)
            })
            .or_else(|| {
                (live
                    || o.client_order_id
                        .as_ref()
                        .is_some_and(|id| self.account.pending_clients.contains(id)))
                .then_some(0.0)
            });
        if old.is_some_and(|n| o.filled < n) {
            return;
        }
        if self.account.terminal.contains(&o.id) && o.status == OrderStatus::Open {
            o.status = cx
                .store
                .order(&o.id)
                .map_or(OrderStatus::Canceled, |prior| {
                    if prior.status == OrderStatus::Closed {
                        OrderStatus::Closed
                    } else {
                        OrderStatus::Canceled
                    }
                });
        }
        // Equal-quantity receipts refresh the native quote without a new Fill.
        // Mapping has already validated this cumulative average/quantity pair.
        let Ok(mut anchor) = Cursor::seed_with_average(o.filled, None, Some(mapped.average)) else {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                "Coinbase invalid incremental execution value",
            );
            self.account_poll(cx, true);
            return;
        };
        if let Some(previous) = old
            && o.filled > previous
        {
            let old_value = if previous == 0.0 {
                Some(0.0)
            } else {
                self.account.stream_values.get(&o.id).copied().or_else(|| {
                    self.account
                        .snapshot
                        .as_ref()
                        .and_then(|s| s.fill_values.get(&o.id))
                        .copied()
                })
            };
            let Some(old_value) = old_value else {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    "Coinbase fill baseline lacks cumulative execution value",
                );
                self.account_poll(cx, true);
                return;
            };
            let prepared = Cursor::seed(previous, Some(old_value)).and_then(|cursor| {
                cursor.prepare_average(o.filled, Some(mapped.average), None, Arithmetic::Binary)
            });
            let Ok(Some(prepared)) = prepared else {
                cx.notice(
                    Level::Warn,
                    code::ROW_SKIPPED,
                    "Coinbase invalid incremental execution value",
                );
                self.account_poll(cx, true);
                return;
            };
            anchor = prepared.next();
            cx.fill(Fill {
                id: None,
                order_id: o.id.clone(),
                symbol: o.symbol.clone(),
                side: o.side,
                price: prepared.price(),
                amount: prepared.amount(),
            });
        }
        self.stream_baseline(cx, &o.id, anchor.quantity(), anchor.value().unwrap_or(0.0));
        self.trading_observed_order(cx, &o);
        if let Some(client) = &o.client_order_id {
            self.account.pending_clients.remove(client);
        }
        self.private_order_changed(&o.id);
        if let Some(proof) = mapped.proof {
            self.account.proofs.insert(o.id.clone(), proof);
        }
        if o.status != OrderStatus::Open {
            let native = o
                .extra
                .as_ref()
                .and_then(|e| e.filled_in_placement_units)
                .unwrap();
            self.account
                .terminal_fills
                .insert(o.id.clone(), (self.account.revision, native));
            if self.account.terminal.insert(o.id.clone()) {
                self.account.terminal_fifo.push_back(o.id.clone());
            }
            while self.account.terminal_fifo.len() > MAX_ROWS {
                if let Some(id) = self.account.terminal_fifo.pop_front() {
                    self.account.terminal.remove(&id);
                    self.account.terminal_fills.remove(&id);
                    self.account.versions.remove(&id);
                    self.account.proofs.remove(&id);
                }
            }
        }
        if o.filled > old.unwrap_or(0.0) {
            self.trading_observed_fill(cx, &o, old.unwrap_or(0.0));
        }
        if let Some(s) = &mut self.account.snapshot {
            if let Some(p) = self.account.proofs.get(&o.id) {
                s.proofs.insert(o.id.clone(), p.clone());
            }
            s.rows.insert(o.id.clone(), o.clone());
        }
        if !cx
            .store
            .order(&o.id)
            .is_some_and(|prior| prior.filled > o.filled)
        {
            cx.store.upsert_order(o);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn spot() -> safe_cex_engine::Store {
        let mut store = safe_cex_engine::Store::default();
        store.set_markets(vec![Market {
            id: "BTC-USD".into(),
            symbol: "BTC-USD".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 100.0,
                },
                leverage: MinMax { min: 1.0, max: 1.0 },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: Some(false),
            is_inverse: None,
            is_spot: Some(true),
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        store
    }
    fn row(extra: &str) -> String {
        format!(
            r#"{{"order_id":"order-a","product_id":"BTC-USD","status":"OPEN","side":"BUY","order_type":"LIMIT","filled_size":"0","order_configuration":{{"limit_limit_gtc":{{"base_size":"1.000","limit_price":"100.00"}}}}{extra}}}"#
        )
    }
    #[test]
    fn ordinary_spot_edit_proof_retains_native_strings() {
        let (_, proof) = map_order(&spot(), &row(""), 1000.0).unwrap();
        let proof = proof.unwrap();
        assert_eq!(proof.price, "100.00");
        assert_eq!(proof.size, "1.000");
    }
    #[test]
    fn top_level_trigger_cannot_hide_behind_limit_configuration() {
        let (o, proof) = map_order(&spot(), &row(r#", "stop_price":"99""#), 1000.0).unwrap();
        assert_eq!(o.kind, OrderType::Limit);
        assert!(proof.is_none());
    }
    #[test]
    fn contradictory_native_price_or_size_does_not_grant_edit_proof() {
        for extra in [
            r#", "limit_price":"101""#,
            r#", "leaves_quantity":"2""#,
            r#", "attached_order_id":"child""#,
        ] {
            let (_, proof) = map_order(&spot(), &row(extra), 1000.0).unwrap();
            assert!(proof.is_none());
        }
    }
}
