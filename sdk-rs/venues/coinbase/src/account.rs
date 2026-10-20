use crate::{Coinbase, Pending, ProductKind, account_wire as wire};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketChannel, SocketId, TimerId};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const ROOT: &str = "/api/v3/brokerage";
const STATUSES: [&str; 5] = ["PENDING", "OPEN", "QUEUED", "CANCEL_QUEUED", "EDIT_QUEUED"];
const MAX_ROWS: usize = 10000;
#[derive(Clone)]
#[expect(dead_code, reason = "consumed by the following trading phase")]
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
        rows: Vec<Position>,
        invalid: bool,
    },
    Orders(Orders),
}
#[derive(Default)]
struct Snapshot {
    revision: u64,
    rows: HashMap<String, Order>,
    proofs: HashMap<String, Editable>,
    fill_values: HashMap<String, f64>,
    raw_count: usize,
    invalid: bool,
}
#[derive(Default)]
pub(crate) struct Account {
    pub kind: Option<ProductKind>,
    pub portfolio_id: Option<String>,
    pub socket: Option<SocketId>,
    pub proofs: HashMap<String, Editable>,
    ready: bool,
    opened: bool,
    received: f64,
    payload: f64,
    sequence: Option<u64>,
    watchdog: Option<TimerId>,
    reconnect: Option<TimerId>,
    reconnect_failures: u8,
    tick: Option<TimerId>,
    write_timer: Option<TimerId>,
    startup_retry: Option<(TimerId, Request)>,
    startup_failures: u8,
    inflight: [bool; 4],
    last_orders: f64,
    backoff_until: f64,
    revision: u64,
    snapshot_revision: u64,
    versions: HashMap<String, u64>,
    totals: HashMap<String, f64>,
    fill_values: HashMap<String, f64>,
    terminal: HashSet<String>,
    terminal_fifo: VecDeque<String>,
    snapshot: Option<Snapshot>,
    has_snapshot: bool,
    pending_clients: HashSet<String>,
}
#[expect(dead_code, reason = "consumed by the following trading phase")]
impl Account {
    pub(crate) fn editable(&self, id: &str) -> Option<&Editable> {
        self.proofs.get(id)
    }
    pub(crate) fn is_intx(&self) -> bool {
        self.kind == Some(ProductKind::Intx)
    }
}
fn decimal(s: &str) -> Result<f64, String> {
    let parsed: wire::Decimal = serde_json::from_str(&serde_json::to_string(s).unwrap())
        .map_err(|_| "Coinbase invalid decimal")?;
    Ok(parsed.0)
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
fn quote(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
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
        product.strip_suffix("-USDC")
            .filter(|_| !matches!(product, "USDT-USDC" | "EURC-USDC"))
            .map(|base|format!("{base}-USD"))
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
fn multiplier(m: &Market) -> Result<f64, String> {
    if m.is_expiring == Some(true) {
        m.contract_size
            .filter(|n| n.is_finite() && *n > 0.0)
            .ok_or("Coinbase missing CFM contract size".into())
    } else {
        Ok(1.0)
    }
}
struct Mapped {
    order: Order,
    proof: Option<Editable>,
    average: f64,
}
#[cfg_attr(not(test), expect(dead_code, reason = "consumed by the following query phase"))]
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
            price = r.average_filled_price.map_or(0.0, |n| n.0);
        }
        amount = if !config.base_size.is_empty() {
            unsigned(&config.base_size)?
        } else {
            let average = if kind == OrderType::Market {
                r.average_filled_price.map_or(0.0, |n| n.0)
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
        amount = filled + remaining;
        if kind == OrderType::StopLoss && !r.stop_price.is_empty() {
            price = unsigned(&r.stop_price)?
        } else if !r.limit_price.is_empty() {
            price = unsigned(&r.limit_price)?
        }
        if kind == OrderType::Limit
            && matches!(r.order_type.as_ref(), "LIMIT" | "Limit")
            && m.contract_size == Some(1.0)
            && m.is_spot == Some(true)
            && m.is_expiring != Some(true)
            && scale == 1.0
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
    let average = r.avg_price.or(r.average_filled_price).map_or(0.0, |n| n.0);
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
    })
}
impl Coinbase {
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            return;
        }
        self.account_read(cx, Request::Portfolios);
    }
    fn account_read(&mut self, cx: &mut Cx<'_, Pending>, req: Request) {
        let id = self.account.portfolio_id.as_deref().unwrap_or("");
        let (path, at) = match &req {
            Request::Portfolios => (format!("{ROOT}/portfolios"), None),
            Request::Probe { portfolios, index } => (
                format!("{ROOT}/portfolios/{}", portfolios[*index].uuid),
                None,
            ),
            Request::DetectCfm { .. } => (format!("{ROOT}/cfm/balance_summary"), None),
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
                    format!("&cursor={}", quote(&s.cursor))
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
        if let Err(e) = self.request(cx, Method::Get, path, None, at, Pending::Account(req)) {
            if let Some(req) = retry {
                self.private_failed(cx, req, &e)
            } else {
                cx.log(LogLevel::Warning, e);
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
        self.account.tick = Some(cx.set_timer(1000.0));
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.opened = false;
        self.account.received = cx.now;
        self.account.payload = cx.now;
        self.account.sequence = None;
        self.account.snapshot = None;
        self.account.has_snapshot = false;
        self.account.socket = Some(cx.connect_channel(
            "wss://advanced-trade-ws-user.coinbase.com",
            SocketChannel::Private,
        ));
        self.account.watchdog = Some(cx.set_timer(5000.0));
    }
    pub(crate) fn private_on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) -> bool {
        if self.account.socket != Some(socket) {
            return false;
        }
        self.account.opened = true;
        self.account.received = cx.now;
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
                .and_then(|c| c.jwt(cx.now, None));
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
                    cx.log(LogLevel::Error, e);
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
        if let Some(t) = self.account.watchdog.take() {
            cx.clear_timer(t)
        }
        self.account.opened = false;
        self.account.snapshot = None;
        self.account.sequence = None;
        if self.account.reconnect.is_none() {
            let delay = (2000.0 * 2f64.powi(i32::from(self.account.reconnect_failures.min(4))))
                .min(30000.0);
            self.account.reconnect_failures = self.account.reconnect_failures.saturating_add(1);
            self.account.reconnect = Some(cx.set_timer(delay))
        }
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
                        rows: vec![],
                        invalid: false,
                    },
                )
            }
        }
        if orders && !self.account.inflight[3] {
            self.account.inflight[3] = true;
            self.account.last_orders = cx.now;
            self.account_read(
                cx,
                Request::Orders(Orders {
                    rows: HashMap::new(),
                    proofs: HashMap::new(),
                    fill_values: HashMap::new(),
                    invalid: false,
                    revision: self.account.revision,
                    kind,
                    status: 0,
                    page: 1,
                    cursor: String::new(),
                    cursors: HashSet::new(),
                }),
            )
        }
    }
    #[expect(dead_code, reason = "consumed by the following trading phase")]
    pub(crate) fn private_placement_started(&mut self, client_id: &str) -> Result<(), String> {
        if self.account.pending_clients.len() >= 1000 {
            return Err("Coinbase unresolved placement limit".into());
        }
        self.account.pending_clients.insert(client_id.into());
        Ok(())
    }
    #[expect(dead_code, reason = "consumed by the following trading phase")]
    pub(crate) fn private_placement_finished(&mut self, client_id: &str) {
        self.account.pending_clients.remove(client_id);
    }
    #[expect(dead_code, reason = "consumed by the following trading phase")]
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
    #[expect(dead_code, reason = "consumed by the following trading phase")]
    pub(crate) fn private_order_canceled(&mut self, cx: &mut Cx<'_, Pending>, id: &str) {
        self.private_order_changed(id);
        if self.account.terminal.insert(id.into()) {
            self.account.terminal_fifo.push_back(id.into());
        }
        while self.account.terminal_fifo.len() > MAX_ROWS {
            if let Some(old) = self.account.terminal_fifo.pop_front() {
                self.account.terminal.remove(&old);
                self.account.versions.remove(&old);
                self.account.totals.remove(&old);
                self.account.fill_values.remove(&old);
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
        if self.account.reconnect == Some(timer) {
            self.account.reconnect = None;
            self.private_connect(cx);
            return true;
        }
        if self.account.watchdog == Some(timer) {
            self.account.watchdog = None;
            let limit = if self.account.opened {
                30000.0
            } else {
                10000.0
            };
            if cx.now - self.account.received >= limit
                || (self.account.snapshot.is_some() && cx.now - self.account.payload >= 30000.0)
            {
                cx.log(
                    LogLevel::Warning,
                    "Coinbase private stream watchdog expired",
                );
                self.private_disconnect(cx)
            } else {
                self.account.watchdog = Some(cx.set_timer(5000.0))
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
            let gap = if self.account.opened && self.account.snapshot.is_none() {
                10000.0
            } else {
                3000.0
            };
            self.account_poll(cx, cx.now - self.account.last_orders >= gap);
            self.account.tick = Some(cx.set_timer(1000.0));
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
                    if rows.len() >= MAX_ROWS {
                        return Err("Coinbase positions exceed limit".into());
                    }
                    let p = self.map_position(cx, text)?;
                    if rows.iter().any(|r| r.id == p.id) {
                        return Err("Coinbase duplicate position".into());
                    }
                    rows.push(p);
                }
                Request::Orders(s) => {
                    if s.rows.len() >= MAX_ROWS {
                        return Err("Coinbase orders exceed limit".into());
                    }
                    let mapped = map_order_detail(cx.store, text, cx.now)?;
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
            cx.log(LogLevel::Warning, e)
        }
    }
    fn map_position(&self, cx: &Cx<'_, Pending>, text: &str) -> Result<Position, String> {
        let (symbol, side, size, entry, mark, pnl, leverage, liq) = if self.account.kind
            == Some(ProductKind::Intx)
        {
            let r: wire::IntxPosition<'_> =
                serde_json::from_str(text).map_err(|e| e.to_string())?;
            let side = match r.position_side.as_ref() {
                "POSITION_SIDE_LONG" => PositionSide::Long,
                "POSITION_SIDE_SHORT" => PositionSide::Short,
                _ => return Err("Coinbase invalid position side".into()),
            };
            let size = r.net_size.0.abs();
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
        response: Response<'_>,
    ) {
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
                cx.log(LogLevel::Error, "Coinbase credentials rejected");
                cx.deactivate();
                return;
            }
            if matches!(response.status, 403 | 404) {
                match req {
                    Request::Probe { portfolios, index } if response.status == 403 => {
                        self.next_probe(cx, portfolios, index + 1);
                        return;
                    }
                    Request::DetectCfm { portfolio } => {
                        self.select_portfolio(cx, portfolio, ProductKind::Spot);
                        return;
                    }
                    Request::Portfolios => {
                        cx.log(LogLevel::Error, "Coinbase credentials rejected");
                        cx.deactivate();
                        return;
                    }
                    _ => {}
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
        let result: Result<(), String> = (|| {
            match req {
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
                            cx.store.positions().iter().map(|p| p.unrealized_pnl).sum(),
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
                    let mut coins = vec![];
                    let mut seen = HashSet::new();
                    for b in r.breakdown.spot_positions {
                        if b.asset.is_empty()
                            || !seen.insert(b.asset.clone())
                            || b.total_balance_crypto.0 < 0.0
                            || b.available_to_trade_crypto.0 < 0.0
                            || b.available_to_trade_crypto.0 > b.total_balance_crypto.0
                        {
                            return Err("Coinbase invalid spot holding".into());
                        }
                        coins.push(CurrencyBalance {
                            currency: b.asset,
                            total: b.total_balance_crypto.0,
                            free: b.available_to_trade_crypto.0,
                            used: b.total_balance_crypto.0 - b.available_to_trade_crypto.0,
                            upnl: 0.0,
                        });
                    }
                    cx.store.set_currency_balances(coins);
                    if self.account.kind == Some(ProductKind::Spot) {
                        cx.store.set_balance(Balance::default());
                    }
                }
                Request::Positions { mut rows, invalid } => {
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
                    for m in cx.store.markets() {
                        if m.is_spot != Some(true) && !rows.iter().any(|p| p.symbol == m.symbol) {
                            rows.push(Position {
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
                    let upnl = rows.iter().map(|p| p.unrealized_pnl).sum::<f64>();
                    for p in &rows {
                        cx.store.update_setting(&p.symbol, |s| {
                            s.leverage = p.leverage;
                            s.margin_type = Some(MarginType::Cross)
                        });
                    }
                    cx.store.set_positions(rows);
                    if self.account.kind == Some(ProductKind::Intx) && cx.store.loaded().balance {
                        let mut b = cx.store.balance().clone();
                        b.upnl = upnl;
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
                        self.commit_orders(cx, s);
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            if let Some(req) = retry {
                self.private_failed(cx, req, &e);
            } else {
                cx.log(LogLevel::Warning, e);
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
            cx.log(LogLevel::Error, "Coinbase no permitted portfolio found");
            return;
        }
        self.account_read(cx, Request::Probe { portfolios, index })
    }
    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, mut s: Orders) {
        if s.revision < self.account.snapshot_revision {
            return;
        }
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
        s.rows
            .retain(|id, o| !self.account.terminal.contains(id) || o.status != OrderStatus::Open);
        for o in s.rows.values() {
            if !self
                .account
                .versions
                .get(&o.id)
                .is_some_and(|v| *v > s.revision)
                && let Some(value) = s.fill_values.get(&o.id)
            {
                self.account.fill_values.insert(o.id.clone(), *value);
            }
            self.account
                .totals
                .entry(o.id.clone())
                .and_modify(|q| *q = q.max(o.filled))
                .or_insert(o.filled);
        }
        self.account.proofs = s.proofs;
        self.account.proofs.retain(|id, _| {
            s.rows
                .get(id)
                .is_some_and(|o| o.status == OrderStatus::Open)
        });
        self.account
            .fill_values
            .retain(|id, _| s.rows.contains_key(id) || self.account.terminal.contains(id));
        self.account
            .totals
            .retain(|id, _| s.rows.contains_key(id) || self.account.terminal.contains(id));
        self.account
            .versions
            .retain(|id, _| s.rows.contains_key(id) || self.account.terminal.contains(id));
        let mut rows = s.rows.into_values().collect::<Vec<_>>();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        cx.store.set_orders(rows);
    }
    pub(crate) fn private_failed(&mut self, cx: &mut Cx<'_, Pending>, req: Request, error: &str) {
        cx.log(LogLevel::Warning, error);
        match req {
            Request::Portfolios | Request::Probe { .. } | Request::DetectCfm { .. } => {
                if self.account.startup_retry.is_none() && self.account.startup_failures < 5 {
                    self.account.startup_failures += 1;
                    let t = cx.set_timer(
                        (1000.0 * 2f64.powi(i32::from(self.account.startup_failures)))
                            .max(self.account.backoff_until - cx.now),
                    );
                    self.account.startup_retry = Some((t, req));
                }
            }
            Request::Balance => self.account.inflight[0] = false,
            Request::SpotBalance => self.account.inflight[1] = false,
            Request::Positions { .. } => self.account.inflight[2] = false,
            Request::Orders(_) => self.account.inflight[3] = false,
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
            cx.log(LogLevel::Warning, "Coinbase invalid private frame");
            return true;
        };
        if frame.kind == "error" {
            cx.log(
                LogLevel::Warning,
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
            cx.log(LogLevel::Warning, "Coinbase private sequence missing");
            self.private_disconnect(cx);
            return true;
        };
        if let Some(prior) = self.account.sequence {
            if seq <= prior {
                return true;
            }
            if prior.checked_add(1) != Some(seq) {
                cx.log(LogLevel::Warning, "Coinbase private sequence gap");
                self.private_disconnect(cx);
                return true;
            }
        } else if seq != 0 {
            cx.log(
                LogLevel::Warning,
                "Coinbase private initial sequence missing",
            );
            self.private_disconnect(cx);
            return true;
        }
        self.account.sequence = Some(seq);
        self.account.received = cx.now;
        if frame.channel != "user" {
            return true;
        }
        let Some(raw_events) = frame.events else {
            cx.log(LogLevel::Warning, "Coinbase user events missing");
            self.private_disconnect(cx);
            return true;
        };
        let Ok(parsed_events) = serde_json::from_str::<Vec<wire::UserEvent<'_>>>(raw_events.get())
        else {
            cx.log(LogLevel::Warning, "Coinbase invalid user events");
            self.private_disconnect(cx);
            return true;
        };
        if parsed_events.len() > 1000 {
            self.private_disconnect(cx);
            return true;
        }
        let mut events = vec![];
        let mut frame_rows = 0;
        for event in parsed_events {
            let Some(raw) = event.orders else { continue };
            frame_rows += raw.len();
            if frame_rows > MAX_ROWS {
                self.private_disconnect(cx);
                return true;
            }
            if !matches!(event.kind.as_ref(), "snapshot" | "update") {
                cx.log(LogLevel::Warning, "Coinbase unknown user event");
                self.private_disconnect(cx);
                return true;
            }
            let mut rows = vec![];
            let mut invalid = false;
            for text in &raw {
                match map_order_detail(cx.store, text.get(), cx.now) {
                    Ok(mapped) => rows.push(mapped),
                    Err(e) => {
                        cx.log(LogLevel::Warning, e);
                        invalid = true;
                        if let Some(s) = &mut self.account.snapshot {
                            s.invalid = true
                        }
                    }
                }
            }
            events.push((event.kind.into_owned(), raw.len(), rows, invalid));
        }
        self.account.sequence = Some(seq);
        self.account.received = cx.now;
        self.account.payload = cx.now;
        for (kind, count, rows, invalid) in events {
            if kind == "snapshot" {
                let revision = self.account.revision;
                let s = self.account.snapshot.get_or_insert_with(|| Snapshot {
                    revision,
                    ..Default::default()
                });
                s.invalid |= invalid;
                s.raw_count += count;
                if s.raw_count > MAX_ROWS {
                    cx.log(LogLevel::Warning, "Coinbase user snapshot exceeds limit");
                    self.private_disconnect(cx);
                    return true;
                }
                for mapped in rows {
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
                if count < 50 {
                    let s = self.account.snapshot.take().unwrap();
                    if !s.invalid {
                        self.account.reconnect_failures = 0;
                        self.account.has_snapshot = true;
                        for o in s.rows.values() {
                            self.account
                                .totals
                                .entry(o.id.clone())
                                .and_modify(|q| *q = q.max(o.filled))
                                .or_insert(o.filled);
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
                    } else {
                        cx.log(LogLevel::Warning, "Coinbase rejected partial user snapshot");
                        self.account_poll(cx, true);
                    }
                }
            } else {
                for mapped in rows {
                    self.apply_user_order(cx, mapped)
                }
            }
        }
        true
    }
    fn apply_user_order(&mut self, cx: &mut Cx<'_, Pending>, mapped: Mapped) {
        let mut o = mapped.order;
        let old = self
            .account
            .totals
            .get(&o.id)
            .copied()
            .or_else(|| cx.store.order(&o.id).map(|o| o.filled))
            .or_else(|| {
                self.account
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.rows.get(&o.id))
                    .map(|o| o.filled)
            })
            .or_else(|| {
                (self.account.has_snapshot
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
        if let Some(previous) = old {
            let delta = o.filled - previous;
            if delta > 0.0 {
                let new_value = mapped.average * o.filled;
                let old_value = if previous == 0.0 {
                    Some(0.0)
                } else {
                    self.account.fill_values.get(&o.id).copied().or_else(|| {
                        self.account
                            .snapshot
                            .as_ref()
                            .and_then(|s| s.fill_values.get(&o.id))
                            .copied()
                    })
                };
                let Some(old_value) = old_value else {
                    cx.log(
                        LogLevel::Warning,
                        "Coinbase fill baseline lacks cumulative execution value",
                    );
                    self.account_poll(cx, true);
                    return;
                };
                let price = (new_value - old_value) / delta;
                if !price.is_finite() || price <= 0.0 {
                    cx.log(
                        LogLevel::Warning,
                        "Coinbase invalid incremental execution value",
                    );
                    self.account_poll(cx, true);
                    return;
                }
                if price > 0.0 {
                    cx.fill(Fill {
                        order_id: o.id.clone(),
                        symbol: o.symbol.clone(),
                        side: o.side,
                        price,
                        amount: delta,
                    });
                }
            }
        }
        if mapped.average > 0.0 {
            self.account
                .fill_values
                .insert(o.id.clone(), mapped.average * o.filled);
        }
        self.account.totals.insert(o.id.clone(), o.filled);
        if let Some(client) = &o.client_order_id {
            self.account.pending_clients.remove(client);
        }
        self.private_order_changed(&o.id);
        if let Some(proof) = mapped.proof {
            self.account.proofs.insert(o.id.clone(), proof);
        }
        if o.status != OrderStatus::Open {
            if self.account.terminal.insert(o.id.clone()) {
                self.account.terminal_fifo.push_back(o.id.clone());
            }
            while self.account.terminal_fifo.len() > MAX_ROWS {
                if let Some(id) = self.account.terminal_fifo.pop_front() {
                    self.account.terminal.remove(&id);
                    self.account.versions.remove(&id);
                    self.account.totals.remove(&id);
                    self.account.fill_values.remove(&id);
                    self.account.proofs.remove(&id);
                }
            }
        }
        if let Some(s) = &mut self.account.snapshot {
            if let Some(p) = self.account.proofs.get(&o.id) {
                s.proofs.insert(o.id.clone(), p.clone());
            }
            s.rows.insert(o.id.clone(), o.clone());
        }
        cx.store.upsert_order(o);
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
