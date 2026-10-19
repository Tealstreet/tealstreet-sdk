//! Account snapshots are committed only if their domain has not changed since
//! the request began. Executions are independent from order-state updates.
use crate::sign::{Credentials, Params};
use crate::wire::*;
use crate::{Backpack, Connection, Pending, REST, WS};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId};
use std::collections::{HashMap, HashSet, VecDeque};

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
        rows: Vec<Position>,
        invalid: bool,
        revision: u64,
        next: Next,
    },
    Balance {
        revision: u64,
        recalculate: bool,
        next: Next,
    },
    Orders {
        rows: Vec<Order>,
        repairs: Vec<crate::trading::Repair>,
        invalid: bool,
        revision: u64,
        next: Next,
    },
}

pub struct Account {
    pub credentials: Option<Credentials>,
    pub socket: Connection,
    pub leverage: f64,
    pub leverage_revision: u64,
    pub orders_revision: u64,
    pub positions_revision: u64,
    pub balance_revision: u64,
    tick: Option<TimerId>,
    orders_timer: Option<TimerId>,
    fill_timer: Option<TimerId>,
    last_fill_read: f64,
    order_times: HashMap<String, f64>,
    position_times: HashMap<String, f64>,
    executions: HashSet<String>,
    execution_order: VecDeque<String>,
}
impl Default for Account {
    fn default() -> Self {
        Self {
            credentials: None,
            socket: Connection::default(),
            leverage: 1.0,
            leverage_revision: 0,
            orders_revision: 0,
            positions_revision: 0,
            balance_revision: 0,
            tick: None,
            orders_timer: None,
            fill_timer: None,
            last_fill_read: f64::NEG_INFINITY,
            order_times: HashMap::new(),
            position_times: HashMap::new(),
            executions: HashSet::new(),
            execution_order: VecDeque::new(),
        }
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
    let last = store.ticker(symbol).map_or(0.0, |t| t.last);
    if last == 0.0
        || (side == OrderSide::Sell && price < last)
        || (side == OrderSide::Buy && price > last)
    {
        OrderType::StopLoss
    } else {
        OrderType::TakeProfit
    }
}

pub fn order(row: OrderRow<'_>, store: &safe_cex_engine::Store) -> Order {
    let side = side(&row.side);
    let trigger = row.trigger_price.map(|n| n.0);
    let amount = if trigger.is_some() {
        row.trigger_quantity.map_or(row.quantity, |n| n.0)
    } else {
        row.quantity
    };
    Order {
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
        time_in_force: Some(tif(&row.time_in_force)),
        last_update: row.created_at,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
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
        let credentials = self
            .account
            .credentials
            .as_ref()
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
                Ok(c) => Some(c),
                Err(e) => {
                    cx.log(LogLevel::Error, e);
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
                rows: vec![],
                invalid: false,
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
        self.account.orders_revision += 1;
        let _ = self.signed(
            cx,
            Method::Get,
            "/api/v1/orders",
            Params::new(),
            None,
            Pending::Account(Request::Orders {
                rows: vec![],
                repairs: vec![],
                invalid: false,
                revision: self.account.orders_revision,
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
                if self.account.socket.socket.is_none() && self.account.socket.reconnect.is_none() {
                    self.account.socket.socket = Some(cx.connect(WS));
                }
                if let Some(t) = self.account.tick.take() {
                    cx.clear_timer(t);
                }
                self.account.tick = Some(cx.set_timer(10000.0));
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
        let result = match pending {
            Request::Positions { rows, invalid, .. } => {
                let result = serde_json::from_str::<PositionRow<'_>>(item).map(|r| {
                    if r.net_quantity != 0.0 && cx.store.market(&r.symbol).is_some() {
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
                });
                if result.is_err() {
                    *invalid = true;
                }
                result
            }
            Request::Orders {
                rows,
                repairs,
                invalid,
                ..
            } => {
                let result = serde_json::from_str::<OrderRow<'_>>(item).map(|r| {
                    if cx.store.market(&r.symbol).is_some() {
                        if r.trigger_price.is_some()
                            && r.trigger_by.is_none()
                            && let Ok(repair) = self.trigger_repair(cx, &r)
                        {
                            repairs.push(repair);
                        }
                        rows.push(order(r, cx.store));
                    }
                });
                if result.is_err() {
                    *invalid = true;
                }
                result
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            cx.log(LogLevel::Error, format!("Backpack account decode: {e}"));
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        response: Response<'_>,
    ) {
        if response.status == 404
            && let Request::Positions { revision, next, .. } = &pending
        {
            if *revision == self.account.positions_revision {
                cx.store.set_positions(vec![]);
            }
            self.next_account(cx, *next);
            return;
        }
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
                    invalid,
                    revision,
                    next,
                } => {
                    if invalid {
                        return Err("Backpack invalid positions".into());
                    }
                    if revision == self.account.positions_revision {
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
                    if revision == self.account.balance_revision {
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
                    }
                    self.next_account(cx, next);
                }
                Request::Orders {
                    rows,
                    repairs,
                    invalid,
                    revision,
                    next,
                } => {
                    if invalid {
                        return Err("Backpack invalid orders".into());
                    }
                    if revision == self.account.orders_revision {
                        cx.store.set_orders(rows);
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
            cx.log(LogLevel::Error, e);
            self.next_account(cx, failure_next);
            if self.account.tick.is_none() {
                self.account.tick = Some(cx.set_timer(10000.0));
            }
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Request,
        error: &str,
    ) {
        cx.log(LogLevel::Error, error);
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
        self.account.socket.received = cx.now;
        self.account.socket.ping_at = cx.now;
        self.account.socket.watchdog = Some(cx.set_timer(5000.0));
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
        self.account.socket.received = cx.now;
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
        self.account.socket.attempts = 0;
        if envelope.stream.starts_with("account.orderUpdate") {
            let d: OrderEvent<'_> = serde_json::from_str(raw.get()).map_err(|e| e.to_string())?;
            if cx.store.market(&d.s).is_none() {
                return Ok(());
            }
            let amount = d.trigger_quantity.map_or(d.q, |n| n.0);
            let trigger = d.trigger_price.map(|n| n.0);
            let side = side(&d.side);
            let price = trigger
                .or(d.p.map(|n| n.0))
                .or(d.execution_price.as_ref().map(|n| n.0))
                .unwrap_or(0.0);
            self.account.orders_revision += 1;
            if self
                .account
                .order_times
                .get(d.i.as_ref())
                .is_none_or(|t| *t <= d.event_time)
            {
                self.account
                    .order_times
                    .insert(d.i.to_string(), d.event_time);
                cx.store.upsert_order(Order {
                    id: d.i.to_string(),
                    parent_id: None,
                    status: status(&d.status),
                    symbol: d.s.to_string(),
                    kind: kind(cx.store, &d.s, side, &d.o, trigger),
                    side,
                    price,
                    amount,
                    filled: d.z,
                    remaining: amount - d.z,
                    reduce_only: d.r || trigger.is_some(),
                    close: None,
                    client_order_id: d.c.as_ref().map(ClientId::text),
                    time_in_force: Some(tif(&d.f)),
                    last_update: (d.event_time / 1000.0).floor(),
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
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
        } else if self.account.socket.reconnect == Some(timer) {
            self.account.socket.reconnect = None;
            self.account.socket.socket = Some(cx.connect(WS));
        } else if self.account.socket.watchdog == Some(timer) {
            self.account.socket.watchdog = None;
            if let Some(socket) = self.account.socket.socket {
                if cx.now - self.account.socket.received >= 20000.0 {
                    cx.close(socket);
                    self.private_close(cx, socket);
                } else {
                    self.account.socket.ping_at = cx.now;
                    cx.send(socket, "{\"method\":\"PING\"}");
                    self.account.socket.watchdog = Some(cx.set_timer(5000.0));
                }
            }
        } else {
            return false;
        }
        true
    }
    pub(crate) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) {
            return;
        }
        self.account.socket.socket = None;
        self.account.socket.open = false;
        if let Some(t) = self.account.socket.watchdog.take() {
            cx.clear_timer(t);
        }
        self.account.socket.attempts += 1;
        self.account.socket.reconnect = Some(cx.set_timer(
            (1000.0 * 2f64.powi(self.account.socket.attempts.min(5) as i32 - 1)).min(30000.0),
        ));
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
