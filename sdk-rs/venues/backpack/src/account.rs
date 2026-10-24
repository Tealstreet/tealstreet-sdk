//! Account snapshots are committed only if their domain has not changed since
//! the request began. Executions are independent from order-state updates.
use crate::sign::{Credentials, Params};
use crate::wire::*;
use crate::{Backpack, Connection, Pending, REST, WS};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::SnapshotBuilder;
use safe_cex_engine::fence::StaleGuard;
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::Tick;
use safe_cex_engine::{Cx, Fill, Method, OrderSnapshotToken, Response, SocketId, TimerId};
use std::collections::{HashMap, HashSet, VecDeque};

const STALE_SNAPSHOT_LIMIT: u8 = 2;
const SKIP_LOG_LIMIT: usize = 4_096;

#[derive(Clone, Copy)]
pub enum Next {
    Positions,
    Balance,
    Orders,
    Finish,
    None,
}
pub enum Request {
    Info {
        revision: u64,
    },
    Positions {
        rows: SnapshotBuilder<String, Position>,
        /// An owned row on a catalogue-malformed market keeps the last snapshot.
        hold: bool,
        revision: u64,
        next: Next,
    },
    Balance {
        revision: u64,
        recalculate: bool,
        next: Next,
    },
    Orders {
        rows: SnapshotBuilder<String, Order>,
        repairs: Vec<crate::trading::Repair>,
        /// Native mapping-time decisions, applied only after whole-read admission.
        unpriced: Vec<(String, bool)>,
        /// A working order on a catalogue-malformed market keeps the last snapshot.
        hold: bool,
        token: OrderSnapshotToken,
        next: Next,
    },
}

pub struct Account {
    pub credentials: Option<Credentials>,
    pub socket: Connection,
    pub leverage: f64,
    pub leverage_revision: u64,
    pub positions_revision: u64,
    pub balance_revision: u64,
    balance_guard: StaleGuard,
    tick: Option<TimerId>,
    orders_timer: Option<TimerId>,
    fill_timer: Option<TimerId>,
    last_fill_read: f64,
    order_times: HashMap<String, f64>,
    position_times: HashMap<String, f64>,
    executions: HashSet<String>,
    execution_order: VecDeque<String>,
    /// Trigger orders labelled before any price existed; relabelled at the seed.
    pub unpriced_triggers: HashSet<String>,
    /// Symbols (empty when unidentified) whose skipped position row was already logged.
    position_skips: HashSet<String>,
    /// Order ids (empty when unidentified) whose skipped row was already logged.
    order_skips: HashSet<String>,
}
impl Default for Account {
    fn default() -> Self {
        Self {
            credentials: None,
            socket: Connection::default(),
            leverage: 1.0,
            leverage_revision: 0,
            positions_revision: 0,
            balance_revision: 0,
            balance_guard: StaleGuard::default(),
            tick: None,
            orders_timer: None,
            fill_timer: None,
            last_fill_read: f64::NEG_INFINITY,
            order_times: HashMap::new(),
            position_times: HashMap::new(),
            executions: HashSet::new(),
            execution_order: VecDeque::new(),
            unpriced_triggers: HashSet::new(),
            position_skips: HashSet::new(),
            order_skips: HashSet::new(),
        }
    }
}

/// A skipped row is an error only when the store has no copy for it to keep.
/// What a row that failed to decode still says about its identity.
#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct RowIdentity {
    #[serde(deserialize_with = "lenient")]
    id: String,
    #[serde(deserialize_with = "lenient")]
    symbol: String,
    #[serde(deserialize_with = "lenient")]
    status: String,
}
/// A string field, or empty when it holds anything else; one bad field keeps the rest.
fn lenient<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<'a> {
        Text(#[serde(borrow)] Str<'a>),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Field::deserialize(d)? {
        Field::Text(text) => text.into_owned(),
        Field::Other(_) => String::new(),
    })
}
fn skip_level(stored: bool) -> Level {
    if stored { Level::Warn } else { Level::Error }
}
/// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
fn log_skip(
    cx: &mut Cx<'_, Pending>,
    logged: &mut HashSet<String>,
    key: &str,
    level: Level,
    message: impl Into<String>,
) {
    if logged.len() >= SKIP_LOG_LIMIT {
        logged.clear();
    }
    if logged.insert(key.to_owned()) {
        cx.notice(level, code::ROW_SKIPPED, message);
    }
}
pub fn side(text: &str) -> OrderSide {
    if text == "Bid" {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub fn status(text: &str) -> OrderStatus {
    match text {
        "Filled" => OrderStatus::Closed,
        "Cancelled" | "Expired" | "TriggerFailed" => OrderStatus::Canceled,
        _ => OrderStatus::Open,
    }
}
pub fn tif(text: &str) -> TimeInForce {
    match text {
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "PO" => TimeInForce::PostOnly,
        _ => TimeInForce::GoodTillCancel,
    }
}

pub fn trigger_type(text: &str) -> Option<StopTriggerType> {
    match text {
        "MarkPrice" => Some(StopTriggerType::MarkPrice),
        "LastPrice" => Some(StopTriggerType::LastPrice),
        "IndexPrice" => Some(StopTriggerType::IndexPrice),
        _ => None,
    }
}

pub fn kind(
    store: &safe_cex_engine::Store,
    symbol: &str,
    side: OrderSide,
    native: &str,
    trigger: Option<f64>,
) -> OrderType {
    if !native.eq_ignore_ascii_case("Market") {
        return OrderType::Limit;
    }
    let Some(price) = trigger else {
        return OrderType::Market;
    };
    // A placeholder ticker has no last price yet; mark is the next best reference.
    let last = store
        .ticker(symbol)
        .map(|t| if t.last > 0.0 { t.last } else { t.mark })
        .unwrap_or(0.0);
    if last == 0.0
        || (side == OrderSide::Sell && price < last)
        || (side == OrderSide::Buy && price > last)
    {
        OrderType::StopLoss
    } else {
        OrderType::TakeProfit
    }
}

fn unpriced(store: &safe_cex_engine::Store, symbol: &str) -> bool {
    store
        .ticker(symbol)
        .is_none_or(|t| t.last <= 0.0 && t.mark <= 0.0)
}

pub fn order(row: OrderRow<'_>, store: &safe_cex_engine::Store) -> Result<Order, &'static str> {
    let quantity = row.base_quantity()?;
    let side = side(&row.side);
    let trigger = row.trigger_price.map(|n| n.0);
    let amount = if trigger.is_some() {
        row.trigger_quantity.map_or(quantity, |n| n.0)
    } else {
        quantity
    };
    Ok(Order {
        id: row.id.into_owned(),
        parent_id: None,
        status: status(&row.status),
        kind: kind(store, &row.symbol, side, &row.order_type, trigger),
        symbol: row.symbol.into_owned(),
        side,
        price: trigger.unwrap_or(row.price),
        amount,
        filled: row.executed_quantity,
        remaining: amount - row.executed_quantity,
        reduce_only: row.reduce_only.unwrap_or(trigger.is_some()),
        close: None,
        client_order_id: row.client_id.map(|id| id.to_string()),
        time_in_force: Some(if row.post_only {
            TimeInForce::PostOnly
        } else {
            tif(&row.time_in_force)
        }),
        last_update: row.created_at,
        trigger_type: row.trigger_by.as_deref().and_then(trigger_type),
        take_profit: None,
        stop_loss: None,
        extra: (trigger.is_some() && row.order_type.eq_ignore_ascii_case("Limit")).then_some(
            OrderExtra {
                conditional_order_price: Some(row.price),
                ..Default::default()
            },
        ),
    })
}

#[allow(clippy::too_many_arguments)]
fn position(
    symbol: String,
    quantity: f64,
    entry: f64,
    notional: f64,
    upnl: f64,
    liquidation: f64,
    leverage: f64,
    now: f64,
) -> Position {
    let side = if quantity >= 0.0 {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    Position {
        id: format!(
            "{symbol}-{}",
            if quantity >= 0.0 { "long" } else { "short" }
        ),
        symbol,
        side,
        entry_price: entry,
        notional: if notional == 0.0 {
            (quantity * entry).abs()
        } else {
            notional
        },
        leverage,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Hedged),
        unrealized_pnl: upnl,
        realized_pnl: None,
        contracts: quantity.abs(),
        liquidation_price: liquidation,
        margin_size: None,
        last_update: Some(now),
    }
}

impl Backpack {
    pub(crate) fn order_stream_time(&self, id: &str) -> f64 {
        self.account.order_times.get(id).copied().unwrap_or(0.0)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        params: Params,
        body: Option<String>,
        pending: Pending,
        stream: bool,
    ) -> Result<(), String> {
        let read_credentials = self
            .options
            .read_credentials
            .as_ref()
            .filter(|_| method == Method::Get)
            .map(|read| {
                let mut c = Credentials::new(read.key.clone(), &read.secret)?;
                c.window = self
                    .account
                    .credentials
                    .as_ref()
                    .map_or(60000, |primary| primary.window);
                Ok::<_, String>(c)
            })
            .transpose()?;
        let credentials = read_credentials
            .as_ref()
            .or(self.account.credentials.as_ref())
            .ok_or("Backpack credentials unavailable")?;
        let headers = credentials.headers(
            instruction(method, path),
            std::slice::from_ref(&params),
            cx.now,
        );
        let query = if method == Method::Get {
            crate::sign::query(&params)
        } else {
            String::new()
        };
        let url = format!(
            "{REST}{path}{}",
            if query.is_empty() {
                String::new()
            } else {
                format!("?{query}")
            }
        );
        if stream {
            cx.request_streamed(method, url, headers, ArrayAt::Root, pending);
        } else {
            cx.request(method, url, headers, body, pending);
        }
        Ok(())
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.options.public_data_only || self.options.key.as_ref().is_none_or(String::is_empty) {
            return;
        }
        self.account.credentials = match (&self.options.key, &self.options.secret) {
            (Some(key), Some(secret)) => match Credentials::new(key.clone(), secret) {
                Ok(mut c) => {
                    if let Some(window) = self
                        .options
                        .recv_window
                        .filter(|n| n.is_finite() && *n > 0.0)
                    {
                        c.window = window.min(60000.0) as u32;
                    }
                    Some(c)
                }
                Err(e) => {
                    cx.notice(Level::Error, code::CONFIG_INVALID, e);
                    None
                }
            },
            _ => None,
        };
        if self.account.credentials.is_some() {
            cx.store.set_hedged(true);
            self.leverage_settings(cx);
            self.account_info(cx);
        }
    }
    fn account_info(&self, cx: &mut Cx<'_, Pending>) {
        let _ = self.signed(
            cx,
            Method::Get,
            "/api/v1/account",
            Params::new(),
            None,
            Pending::Account(Request::Info {
                revision: self.account.leverage_revision,
            }),
            false,
        );
    }
    pub(crate) fn positions_read(&mut self, cx: &mut Cx<'_, Pending>, next: Next) {
        self.account.positions_revision += 1;
        let _ = self.signed(
            cx,
            Method::Get,
            "/api/v1/position",
            Params::new(),
            None,
            Pending::Account(Request::Positions {
                rows: SnapshotBuilder::default(),
                hold: false,
                revision: self.account.positions_revision,
                next,
            }),
            true,
        );
    }
    fn balance_read(&mut self, cx: &mut Cx<'_, Pending>, recalculate: bool, next: Next) {
        self.account.balance_revision += 1;
        let _ = self.signed(
            cx,
            Method::Get,
            "/api/v1/capital/collateral",
            Params::new(),
            None,
            Pending::Account(Request::Balance {
                revision: self.account.balance_revision,
                recalculate,
                next,
            }),
            false,
        );
    }
    fn orders_read(&mut self, cx: &mut Cx<'_, Pending>, next: Next) {
        // Starting a newer native read supersedes every older read.
        cx.store.invalidate_order_snapshots();
        let Some(token) = cx.store.begin_orders_snapshot() else {
            cx.notice(
                Level::Error,
                code::SNAPSHOT_DISCARDED,
                "Backpack order snapshot authority exhausted",
            );
            self.next_account(cx, next);
            return;
        };
        let _ = self.signed(
            cx,
            Method::Get,
            "/api/v1/orders",
            Params::new(),
            None,
            Pending::Account(Request::Orders {
                rows: SnapshotBuilder::default(),
                repairs: vec![],
                unpriced: vec![],
                hold: false,
                token,
                next,
            }),
            true,
        );
    }
    fn next_account(&mut self, cx: &mut Cx<'_, Pending>, next: Next) {
        match next {
            Next::Positions => self.positions_read(cx, Next::Balance),
            Next::Balance => self.balance_read(cx, true, Next::Orders),
            Next::Orders => self.orders_read(cx, Next::Finish),
            Next::Finish => {
                if self.account.socket.socket.is_none()
                    && !self.account.socket.supervisor.is_waiting()
                {
                    self.private_connect(cx);
                }
                if let Some(t) = self.account.tick.take() {
                    cx.clear_timer(t);
                }
                self.account.tick = Some(cx.set_poll_timer(10000.0));
            }
            Next::None => {}
        }
    }
    pub(crate) fn leverage_settings(&self, cx: &mut Cx<'_, Pending>) {
        let symbols: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for symbol in symbols {
            cx.store.update_setting(&symbol, |s| {
                s.leverage = self.account.leverage;
                s.margin_type = Some(MarginType::Cross);
            });
        }
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: &mut Request,
        item: &str,
    ) {
        match pending {
            Request::Positions { rows, hold, .. } => {
                let Ok(r) = serde_json::from_str::<PositionRow<'_>>(item) else {
                    self.reject_position(cx, rows, hold, item);
                    return;
                };
                self.account.position_skips.remove(r.symbol.as_ref());
                if r.net_quantity != 0.0 && cx.store.market(&r.symbol).is_none() {
                    // A malformed catalogue row holds the snapshot; a filtered market is skipped.
                    if self.report_skipped_market(cx, &r.symbol) {
                        *hold = true;
                    } else {
                        self.report_unlisted_market(cx, &r.symbol);
                    }
                } else if r.net_quantity != 0.0 {
                    rows.push(position(
                        r.symbol.into_owned(),
                        r.net_quantity,
                        r.entry_price,
                        r.net_exposure_notional,
                        r.pnl_unrealized,
                        r.est_liquidation_price,
                        self.account.leverage,
                        cx.now,
                    ));
                }
            }
            Request::Orders {
                rows,
                repairs,
                unpriced: pending_unpriced,
                hold,
                ..
            } => {
                let Ok(r) = serde_json::from_str::<OrderRow<'_>>(item) else {
                    self.reject_order(cx, rows, hold, item);
                    return;
                };
                if cx.store.market(&r.symbol).is_none() {
                    // Only a working order on a malformed market holds the snapshot.
                    if self.report_skipped_market(cx, &r.symbol) {
                        *hold |= status(&r.status) == OrderStatus::Open;
                    } else {
                        self.report_unlisted_market(cx, &r.symbol);
                    }
                } else {
                    // Prepare a candidate without queuing any side effect. The native
                    // amount still has to pass the shared strict order mapper below.
                    let repair = if r.trigger_price.is_some() && r.trigger_by.is_none() {
                        self.trigger_repair(cx, &r).ok()
                    } else {
                        None
                    };
                    let Ok(mapped) = order(r, cx.store) else {
                        self.reject_order(cx, rows, hold, item);
                        return;
                    };
                    // A row counts as mapped only after quantity admission succeeds.
                    self.account.order_skips.remove(mapped.id.as_str());
                    if let Some(repair) = repair {
                        repairs.push(repair);
                    }
                    let pending =
                        matches!(mapped.kind, OrderType::StopLoss | OrderType::TakeProfit)
                            && unpriced(cx.store, &mapped.symbol);
                    pending_unpriced.push((mapped.id.clone(), pending));
                    rows.push(mapped);
                }
            }
            _ => {}
        }
    }
    /// A malformed position row keeps its symbol's stored legs, or every leg when unnamed.
    /// Its quantity is unknown, so a catalogue-malformed market holds the snapshot.
    fn reject_position(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &mut SnapshotBuilder<String, Position>,
        hold: &mut bool,
        item: &str,
    ) {
        let symbol = serde_json::from_str::<RowIdentity>(item)
            .unwrap_or_default()
            .symbol;
        if !symbol.is_empty() && cx.store.market(&symbol).is_none() {
            if self.report_skipped_market(cx, &symbol) {
                *hold = true;
            } else {
                self.report_unlisted_market(cx, &symbol);
            }
            return;
        }
        if symbol.is_empty() {
            let level = skip_level(!cx.store.positions().is_empty());
            let message = "Backpack skipped malformed position row without a symbol";
            log_skip(cx, &mut self.account.position_skips, "", level, message);
            rows.reject(None);
            return;
        }
        let ids = ["long", "short"].map(|side| format!("{symbol}-{side}"));
        let level = skip_level(ids.iter().any(|id| cx.store.position(id).is_some()));
        let message = format!("Backpack skipped malformed position row {symbol}");
        log_skip(
            cx,
            &mut self.account.position_skips,
            &symbol,
            level,
            message,
        );
        for id in ids {
            rows.reject(Some(id));
        }
    }
    /// A malformed order row keeps its stored copy, or every stored order when it has no id.
    /// One on a catalogue-malformed market holds the snapshot unless its status reads terminal.
    fn reject_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        rows: &mut SnapshotBuilder<String, Order>,
        hold: &mut bool,
        item: &str,
    ) {
        let row = serde_json::from_str::<RowIdentity>(item).unwrap_or_default();
        if !row.symbol.is_empty() && cx.store.market(&row.symbol).is_none() {
            if self.report_skipped_market(cx, &row.symbol) {
                *hold |= row.status.is_empty() || status(&row.status) == OrderStatus::Open;
            } else {
                self.report_unlisted_market(cx, &row.symbol);
            }
            return;
        }
        let id = row.id;
        if id.is_empty() {
            let level = skip_level(!cx.store.orders().is_empty());
            let message = "Backpack skipped malformed order row without an id";
            log_skip(cx, &mut self.account.order_skips, "", level, message);
            rows.reject(None);
            return;
        }
        let level = skip_level(cx.store.order(&id).is_some());
        let message = format!("Backpack skipped malformed order row {id}");
        log_skip(cx, &mut self.account.order_skips, &id, level, message);
        rows.reject(Some(id));
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.account_failed(
                cx,
                pending,
                &format!("Backpack HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let failure_next = match &pending {
            Request::Info { .. } => Next::Positions,
            Request::Positions { next, .. }
            | Request::Balance { next, .. }
            | Request::Orders { next, .. } => *next,
        };
        let result = (|| {
            match pending {
                Request::Info { revision } => {
                    let r: AccountRow =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    if revision == self.account.leverage_revision {
                        self.account.leverage = if r.leverage_limit > 0.0 {
                            r.leverage_limit
                        } else {
                            1.0
                        };
                        self.leverage_settings(cx);
                    }
                    self.positions_read(cx, Next::Balance);
                }
                Request::Positions {
                    rows,
                    hold,
                    revision,
                    next,
                } => {
                    if hold {
                        return Err("Backpack invalid positions".into());
                    }
                    if revision == self.account.positions_revision {
                        if rows.rejected() == 0 {
                            self.account.position_skips.clear();
                        }
                        // A rejected row cannot prove its position flat; keep the stored copy.
                        let rows =
                            rows.commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
                        cx.store.set_positions(rows);
                    }
                    self.next_account(cx, next);
                }
                Request::Balance {
                    revision,
                    recalculate,
                    next,
                } => {
                    let r: Collateral =
                        serde_json::from_str(response.body).map_err(|e| e.to_string())?;
                    let current = revision == self.account.balance_revision;
                    if self.account.balance_guard.admit_issued(
                        current,
                        revision,
                        STALE_SNAPSHOT_LIMIT,
                    ) {
                        cx.store.set_balance(Balance {
                            used: r.net_equity_locked,
                            free: r.net_equity_available,
                            total: r.net_equity,
                            upnl: if recalculate {
                                cx.store.positions().iter().map(|p| p.unrealized_pnl).sum()
                            } else {
                                r.pnl_unrealized
                            },
                        });
                        if !current {
                            self.balance_read(cx, recalculate, Next::None);
                        }
                    }
                    self.next_account(cx, next);
                }
                Request::Orders {
                    rows,
                    repairs,
                    unpriced,
                    hold,
                    token,
                    next,
                } => {
                    if hold {
                        return Err("Backpack invalid orders".into());
                    }
                    let account = &mut self.account;
                    let accepted = cx.store.commit_orders_snapshot([token], |current| {
                        if rows.rejected() == 0 {
                            account.order_skips.clear();
                        }
                        for (id, pending) in unpriced {
                            if pending {
                                account.unpriced_triggers.insert(id);
                            } else {
                                account.unpriced_triggers.remove(&id);
                            }
                        }
                        rows.commit(current.iter().cloned(), |o| o.id.clone())
                    });
                    if accepted {
                        for repair in repairs {
                            self.start_repair(cx, repair);
                        }
                    }
                    self.next_account(cx, next);
                }
            }
            Ok::<(), String>(())
        })();
        if let Err(e) = result {
            cx.notice(Level::Error, code::STREAM_MALFORMED, e);
            self.next_account(cx, failure_next);
            if self.account.tick.is_none() {
                self.account.tick = Some(cx.set_poll_timer(10000.0));
            }
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        error: &str,
    ) {
        cx.retried_failure("backpack.account:613", error);
        let next = match pending {
            Request::Info { .. } => Next::Positions,
            Request::Positions { next, .. } => next,
            Request::Orders { next, .. } | Request::Balance { next, .. } => next,
        };
        self.next_account(cx, next);
    }

    pub(crate) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(c) = &self.account.credentials else {
            return;
        };
        #[derive(serde::Serialize)]
        struct Subscribe {
            method: &'static str,
            params: [&'static str; 2],
            signature: [String; 4],
        }
        let signature = c.ws_auth(cx.now);
        cx.send(
            socket,
            serde_json::to_string(&Subscribe {
                method: "SUBSCRIBE",
                params: ["account.orderUpdate", "account.positionUpdate"],
                signature,
            })
            .unwrap(),
        );
        self.account.socket.open = true;
        self.account.socket.ping_at = cx.now;
        self.account.socket.supervisor.opened(cx);
        cx.send(socket, "{\"method\":\"PING\"}");
        if cx.store.loaded().orders {
            self.orders_read(cx, Next::None);
            self.positions_read(cx, Next::None);
            self.balance_read(cx, false, Next::None);
        }
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
    ) -> Result<(), String> {
        self.account.socket.supervisor.received(cx.now);
        let envelope: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if let Some(error) = envelope.error {
            if error.code == 4002 {
                return Ok(());
            }
            if let Some(socket) = self.account.socket.socket {
                cx.close(socket);
                self.private_close(cx, socket);
            }
            return Err(format!(
                "Backpack private socket {}: {}",
                error.code, error.message
            ));
        }
        let Some(raw) = envelope.data else {
            return Ok(());
        };
        if envelope.stream.starts_with("account.orderUpdate") {
            let d: OrderEvent<'_> = serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
            if cx.store.market(&d.s).is_none() {
                self.report_skipped_market(cx, &d.s);
                return Ok(());
            }
            let amount = d.trigger_quantity.map_or(d.q, |n| n.0);
            let trigger = d.trigger_price.map(|n| n.0);
            let side = side(&d.side);
            let price = trigger
                .or(d.p.as_ref().map(|n| n.0))
                .or(d.execution_price.as_ref().map(|n| n.0))
                .unwrap_or(0.0);
            cx.store.invalidate_order_snapshots();
            if self
                .account
                .order_times
                .get(d.i.as_ref())
                .is_none_or(|t| *t <= d.event_time)
            {
                self.account
                    .order_times
                    .insert(d.i.to_string(), d.event_time);
                let order_kind = kind(cx.store, &d.s, side, &d.o, trigger);
                if matches!(order_kind, OrderType::StopLoss | OrderType::TakeProfit)
                    && unpriced(cx.store, &d.s)
                {
                    self.account.unpriced_triggers.insert(d.i.to_string());
                }
                cx.store.upsert_order(Order {
                    id: d.i.to_string(),
                    parent_id: None,
                    status: status(&d.status),
                    symbol: d.s.to_string(),
                    kind: order_kind,
                    side,
                    price,
                    amount,
                    filled: d.z,
                    remaining: amount - d.z,
                    reduce_only: d.r || trigger.is_some(),
                    close: None,
                    client_order_id: d.c.as_ref().map(ClientId::text),
                    time_in_force: Some(if d.y {
                        TimeInForce::PostOnly
                    } else {
                        tif(&d.f)
                    }),
                    last_update: (d.event_time / 1000.0).floor(),
                    trigger_type: d.trigger_by.as_deref().and_then(trigger_type),
                    take_profit: None,
                    stop_loss: None,
                    extra: (trigger.is_some() && d.o.eq_ignore_ascii_case("Limit")).then_some(
                        OrderExtra {
                            conditional_order_price: d.p.as_ref().map(|n| n.0),
                            ..Default::default()
                        },
                    ),
                });
            }
            if d.e == "orderFill"
                && let Some(executed) = d.l.filter(|n| n.0 > 0.0)
            {
                let key = match d.t {
                    Some(t) => format!("{}:{t}", d.i),
                    None => format!("{}:{}:{}", d.i, d.event_time, d.z),
                };
                if self.account.executions.insert(key.clone()) {
                    self.account.execution_order.push_back(key);
                    if self.account.execution_order.len() > 4096
                        && let Some(old) = self.account.execution_order.pop_front()
                    {
                        self.account.executions.remove(&old);
                    }
                    cx.fill(Fill {
                        id: None,
                        order_id: d.i.to_string(),
                        symbol: d.s.to_string(),
                        side,
                        price: d.execution_price.map_or(price, |n| n.0),
                        amount: executed.0,
                    });
                    self.account.balance_revision += 1;
                    self.account.positions_revision += 1;
                    self.fill_refresh(cx);
                }
            }
            if self.account.orders_timer.is_none() {
                self.account.orders_timer = Some(cx.set_timer(10000.0));
            }
        } else if envelope.stream.starts_with("account.positionUpdate") {
            let d: PositionEvent<'_> =
                serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
            if cx.store.market(&d.s).is_none() {
                self.report_skipped_market(cx, &d.s);
                return Ok(());
            }
            let now = (d.event_time / 1000.0).floor();
            if self
                .account
                .position_times
                .get(d.s.as_ref())
                .is_some_and(|t| *t > d.event_time)
            {
                return Ok(());
            }
            self.account
                .position_times
                .insert(d.s.to_string(), d.event_time);
            self.account.positions_revision += 1;
            self.account.balance_revision += 1;
            cx.store.update_positions(|p| {
                if p.symbol == d.s {
                    p.contracts = 0.0;
                }
            });
            if d.q != 0.0 {
                let leverage = if d.f > 0.0 { (1.0 / d.f).round() } else { 1.0 };
                cx.store.update_setting(&d.s, |s| {
                    s.leverage = leverage;
                    s.margin_type = Some(MarginType::Cross);
                });
                cx.store.upsert_position(position(
                    d.s.into_owned(),
                    d.q,
                    d.entry.or(d.b).map_or(0.0, |n| n.0),
                    d.n,
                    d.upnl,
                    d.l,
                    leverage,
                    now,
                ));
            }
        }
        Ok(())
    }
    fn fill_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now - self.account.last_fill_read >= 500.0 {
            self.account.last_fill_read = cx.now;
            self.positions_read(cx, Next::None);
            self.balance_read(cx, false, Next::None);
        } else if self.account.fill_timer.is_none() {
            self.account.fill_timer =
                Some(cx.set_timer(500.0 - (cx.now - self.account.last_fill_read)));
        }
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            self.account_info(cx);
        } else if self.account.orders_timer == Some(timer) {
            self.account.orders_timer = None;
            self.orders_read(cx, Next::None);
        } else if self.account.fill_timer == Some(timer) {
            self.account.fill_timer = None;
            self.fill_refresh(cx);
        } else {
            match self.account.socket.supervisor.on_timer(cx, timer) {
                Tick::Unhandled => return false,
                Tick::Reconnect => self.private_connect(cx),
                Tick::Ping => {
                    if let Some(socket) = self.account.socket.socket {
                        self.account.socket.ping_at = cx.now;
                        cx.send(socket, "{\"method\":\"PING\"}");
                    }
                }
                Tick::TimedOut(_) => {
                    if let Some(socket) = self.account.socket.socket {
                        cx.close(socket);
                        self.private_close(cx, socket);
                    }
                }
                Tick::Handled => {}
            }
        }
        true
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.socket.supervisor.connecting(cx);
        self.account.socket.socket = Some(cx.connect(
            safe_cex_engine::SocketSpec::new(WS).channel(safe_cex_engine::SocketChannel::Private),
        ));
    }
    pub(crate) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) {
            return;
        }
        self.account.socket.socket = None;
        self.account.socket.open = false;
        self.account.socket.supervisor.retry(cx);
    }
}

fn instruction(method: Method, path: &str) -> &'static str {
    match (method, path) {
        (Method::Get, "/api/v1/account") => "accountQuery",
        (Method::Patch, "/api/v1/account") => "accountUpdate",
        (Method::Get, "/api/v1/position") => "positionQuery",
        (Method::Get, "/api/v1/capital/collateral") => "collateralQuery",
        (Method::Get, "/api/v1/orders") => "orderQueryAll",
        (Method::Post, "/api/v1/order" | "/api/v1/orders") => "orderExecute",
        (Method::Delete, "/api/v1/order") => "orderCancel",
        (Method::Delete, "/api/v1/orders") => "orderCancelAll",
        (Method::Get, "/wapi/v1/history/fills") => "fillHistoryQueryAll",
        (Method::Get, "/wapi/v1/history/orders") => "orderHistoryQueryAll",
        (Method::Get, "/wapi/v1/history/funding") => "fundingHistoryQueryAll",
        _ => unreachable!("unregistered Backpack instruction"),
    }
}

impl crate::Backpack {
    /// A trigger read before any price defaults to StopLoss; relabel it from
    /// the seeded price once that exists.
    pub(crate) fn relabel_unpriced_triggers(&mut self, cx: &mut Cx<'_, Pending>) {
        for id in std::mem::take(&mut self.account.unpriced_triggers) {
            let Some(order) = cx.store.order(&id) else {
                continue;
            };
            if unpriced(cx.store, &order.symbol) {
                continue;
            }
            let relabelled = kind(
                cx.store,
                &order.symbol,
                order.side,
                "Market",
                Some(order.price),
            );
            cx.store.update_order(&id, |order| order.kind = relabelled);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_classification_uses_mark_while_the_ticker_has_no_last_price() {
        let mut store = safe_cex_engine::Store::default();
        let ticker = |last: f64, mark: f64| Ticker {
            id: "ETH_USDC_PERP".into(),
            symbol: "ETH_USDC_PERP".into(),
            last,
            mark,
            ..Default::default()
        };
        let classify = |store: &safe_cex_engine::Store| {
            kind(
                store,
                "ETH_USDC_PERP",
                OrderSide::Sell,
                "Market",
                Some(2100.0),
            )
        };
        store.set_tickers(vec![ticker(0.0, 2000.0)]);
        assert_eq!(classify(&store), OrderType::TakeProfit);
        store.set_tickers(vec![ticker(2200.0, 2000.0)]);
        assert_eq!(classify(&store), OrderType::StopLoss);
        store.set_tickers(vec![ticker(0.0, 0.0)]);
        assert_eq!(classify(&store), OrderType::StopLoss);
    }
}
