//! Private stream and paged account reconciliation. Live revisions win over
//! snapshots that started before them; failed pages never replace store data.
use crate::{Connection, HEARTBEAT_MS, MexcFutures, Pending, WS, sign, wire::*};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Default)]
pub struct Rails {
    pub long: f64,
    pub short: f64,
    pub isolated: bool,
}
impl Rails {
    fn initial() -> Self {
        Self {
            long: 20.0,
            short: 20.0,
            isolated: false,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Assets,
    Positions,
    Regular,
    Plan,
    Stop,
}
pub enum Request {
    Leverage {
        command: u32,
        symbol: String,
        revision: u64,
        rows: Vec<LeverageRow>,
        bad: bool,
    },
    Mode,
    Snapshot(Snapshot),
}
pub struct Snapshot {
    pub scope: Scope,
    pub revision: u64,
    pub page: u32,
    pub page_count: usize,
    pub bad: bool,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub assets: Vec<(String, Balance)>,
    pub full: bool,
    pub native_ids: Vec<(String, String)>,
}
#[derive(Default)]
pub struct Account {
    pub private: Connection,
    pub native_ids: HashMap<String, String>,
    pub settings_revision: HashMap<String, u64>,
    pub rail_revision: HashMap<(String, PositionSide), u64>,
    pub revision: u64,
    pub order_updates: HashMap<String, u64>,
    pub position_updates: HashMap<String, u64>,
    pub terminal: HashMap<String, (u64, f64)>,
    pub rails: HashMap<String, Rails>,
    pub stop_ids: HashSet<String>,
    pub plan_ids: HashSet<String>,
    pub placed: HashMap<String, f64>,
    pub poll: Option<TimerId>,
    pub asset_refresh: Option<TimerId>,
    pub stop_refresh: Option<TimerId>,
    pub in_flight: HashSet<Scope>,
    pub balance_revision: u64,
    pub position_revision: u64,
    pub mode_revision: u64,
    pub staged: Vec<Order>,
    pub stage_mask: u8,
    pub full_failed: bool,
    pub needs_full: bool,
    pub mode_ready: bool,
    pub fills: HashMap<String, f64>,
    pub paused_until: f64,
}

impl MexcFutures {
    pub fn signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        payload: String,
        streamed: bool,
        pending: Pending,
    ) {
        let Some(c) = &self.credentials else {
            return;
        };
        let headers = c.headers(cx.now, &payload);
        let url = if method == Method::Get && !payload.is_empty() {
            format!("{}{path}?{payload}", crate::REST)
        } else {
            format!("{}{path}", crate::REST)
        };
        if streamed {
            cx.request_streamed(
                method,
                url,
                headers,
                ArrayAt::EnvelopeField("data"),
                pending,
            );
        } else {
            cx.request(
                method,
                url,
                headers,
                if method == Method::Get {
                    None
                } else {
                    Some(payload)
                },
                pending,
            );
        }
    }
    pub fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, command: u32, symbol: String) {
        if cx.now < self.account.paused_until {
            cx.done(command, Err("MEXC account reads are rate limited".into()));
            return;
        }
        let Some(m) = cx.store.market(&symbol) else {
            cx.done(command, Err("Market not found".into()));
            return;
        };
        if self.credentials.is_none() {
            cx.done(command, Err("MEXC leverage requires credentials".into()));
            return;
        }
        let query = sign::query([("symbol", m.id.clone())]);
        self.signed(
            cx,
            Method::Get,
            "/api/v1/private/position/leverage",
            query,
            true,
            Pending::Account(Request::Leverage {
                command,
                symbol,
                revision: self.account.revision,
                rows: vec![],
                bad: false,
            }),
        );
    }
    pub fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            return;
        }
        for m in cx.store.markets().to_vec() {
            self.account
                .rails
                .insert(m.symbol.clone(), Rails::initial());
            cx.store.update_setting(&m.symbol, |s| {
                s.leverage = 20.0;
                s.buy_leverage = Some(20.0);
                s.sell_leverage = Some(20.0);
                s.margin_type = Some(MarginType::Cross);
            });
        }
        cx.store.set_hedged(true);
        self.account.private.socket =
            Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Private));
        self.signed(
            cx,
            Method::Get,
            "/api/v1/private/position/position_mode",
            String::new(),
            false,
            Pending::Account(Request::Mode),
        );
    }
    pub fn snapshot_account(&mut self, cx: &mut Cx<'_, Pending>, scope: Scope, full: bool) {
        if self.account.in_flight.contains(&scope) || cx.now < self.account.paused_until {
            return;
        }
        self.account.in_flight.insert(scope);
        let snapshot = Snapshot {
            scope,
            revision: self.account.revision,
            page: 1,
            page_count: 0,
            bad: false,
            orders: vec![],
            positions: vec![],
            assets: vec![],
            full,
            native_ids: vec![],
        };
        self.snapshot_page(cx, snapshot);
    }
    fn snapshot_page(&self, cx: &mut Cx<'_, Pending>, snapshot: Snapshot) {
        let path = match snapshot.scope {
            Scope::Assets => "/api/v1/private/account/assets",
            Scope::Positions => "/api/v1/private/position/open_positions",
            Scope::Regular => "/api/v1/private/order/list/open_orders",
            Scope::Plan => "/api/v1/private/planorder/list/orders",
            Scope::Stop => "/api/v1/private/stoporder/list/orders",
        };
        let mut params = vec![];
        if matches!(snapshot.scope, Scope::Regular | Scope::Plan | Scope::Stop) {
            params.extend([
                ("page_num", snapshot.page.to_string()),
                ("page_size", "100".into()),
            ]);
        }
        if matches!(snapshot.scope, Scope::Plan | Scope::Stop) {
            params.extend([
                (
                    "start_time",
                    safe_cex_engine::num::format(cx.now - 89.0 * 86400000.0),
                ),
                ("end_time", safe_cex_engine::num::format(cx.now)),
            ]);
            params.push(if snapshot.scope == Scope::Plan {
                ("states", "1".into())
            } else {
                ("is_finished", "0".into())
            });
        }
        self.signed(
            cx,
            Method::Get,
            path,
            sign::query(params),
            true,
            Pending::Account(Request::Snapshot(snapshot)),
        );
    }
    pub fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let c = self.credentials.as_ref().unwrap();
        self.account.private.open = true;
        self.account.private.last_received = cx.now;
        self.account.private.attempts = 0;
        cx.send(socket, r#"{"method":"ping"}"#);
        self.account.private.heartbeat = Some(cx.set_timer(HEARTBEAT_MS));
        #[derive(serde::Serialize)]
        struct Param<'a> {
            #[serde(rename = "apiKey")]
            key: &'a str,
            #[serde(rename = "reqTime")]
            time: String,
            signature: String,
        }
        #[derive(serde::Serialize)]
        struct Login<'a> {
            method: &'static str,
            param: Param<'a>,
        }
        cx.send(
            socket,
            serde_json::to_string(&Login {
                method: "login",
                param: Param {
                    key: &c.key,
                    time: safe_cex_engine::num::format(cx.now),
                    signature: c.signature(cx.now, ""),
                },
            })
            .unwrap(),
        );
    }
    pub fn account_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        self.account.private.last_received = cx.now;
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if m.channel == "rs.login" {
            if m.success == Some(true) || m.data.is_some_and(|d| d.get() == r#""success""#) {
                if let Some(socket) = self.account.private.socket {
                    cx.send(socket,r#"{"method":"personal.filter","param":{"filters":[{"filter":"order"},{"filter":"order.deal"},{"filter":"position"},{"filter":"asset"},{"filter":"plan.order"},{"filter":"stop.order"},{"filter":"stop.planorder"}]}}"#);
                }
                if self.account.mode_ready {
                    self.refresh_account(cx, true);
                }
            } else {
                cx.log(
                    LogLevel::Error,
                    "MEXC private ws login failed, double check your key and secret",
                );
            }
            return;
        }
        if m.channel == "push.personal.asset" {
            if let Some(t) = self.account.asset_refresh.take() {
                cx.clear_timer(t);
            }
            self.account.asset_refresh = Some(cx.set_timer(1000.0));
            return;
        }
        let Some(data) = m.data else {
            return;
        };
        let mut apply = |row: &str| self.private_row(cx, &m.channel, row);
        if data.get().trim_start().starts_with('[') {
            let _ = safe_cex_engine::json::each::<&serde_json::value::RawValue>(data.get(), |r| {
                apply(r.get())
            });
        } else {
            apply(data.get());
        }
    }
    fn private_row(&mut self, cx: &mut Cx<'_, Pending>, channel: &str, row: &str) {
        self.account.revision += 1;
        let revision = self.account.revision;
        match channel {
            "push.personal.order"
            | "push.personal.plan.order"
            | "push.personal.trigger.order"
            | "push.personal.stop.order"
            | "push.personal.stop.planorder" => {
                let scope = if channel == "push.personal.order" {
                    Scope::Regular
                } else if channel.contains("stop.") {
                    Scope::Stop
                } else {
                    Scope::Plan
                };
                let Ok(raw) = serde_json::from_str::<OrderRow<'_>>(row) else {
                    return;
                };
                let error = raw.error_code;
                if let Some(order) = self.map_order(cx, &raw, scope) {
                    if order.status == OrderStatus::Canceled && error > 0.0 && order.filled == 0.0 {
                        cx.log(
                            LogLevel::Error,
                            format!("MEXC canceled order {}: errorCode {error}", order.id),
                        );
                    }
                    self.apply_order(cx, order, scope, revision);
                }
            }
            "push.personal.position" => {
                let Ok(raw) = serde_json::from_str::<PositionRow<'_>>(row) else {
                    return;
                };
                if let Some(p) = Self::map_position(cx, &raw) {
                    self.account
                        .native_ids
                        .insert(p.id.clone(), raw.position_id.to_string());
                    self.update_rails(cx, &p);
                    self.account.position_updates.insert(p.id.clone(), revision);
                    self.account.position_revision = revision;
                    cx.store.upsert_position(p);
                }
            }
            "push.personal.order.deal" => {
                let Ok(raw) = serde_json::from_str::<DealRow<'_>>(row) else {
                    return;
                };
                let symbol = compact(&raw.symbol);
                let Some(m) = cx.store.market(&symbol) else {
                    return;
                };
                let key = if !raw.trade_id.is_empty() {
                    raw.trade_id.to_string()
                } else {
                    raw.id.to_string()
                };
                if !key.is_empty() && self.account.fills.insert(key, cx.now).is_some() {
                    return;
                }
                if raw.vol > 0.0 {
                    cx.fill(Fill {
                        order_id: raw.order_id.to_string(),
                        symbol,
                        side: side(raw.side),
                        price: raw.price,
                        amount: raw.vol * m.contract_size.unwrap_or(1.0),
                    });
                }
            }
            "push.personal.position.mode" => {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Mode {
                    position_mode: u8,
                }
                if let Ok(mode) = serde_json::from_str::<Mode>(row) {
                    self.account.mode_revision = revision;
                    cx.store.set_hedged(mode.position_mode == 1);
                }
            }
            _ => {}
        }
    }
    fn monotonic_order(cx: &Cx<'_, Pending>, mut order: Order) -> Order {
        if let Some(current) = cx.store.order(&order.id) {
            if order.filled < current.filled {
                let status = order.status;
                order = current.clone();
                if current.status != OrderStatus::Closed
                    && matches!(status, OrderStatus::Canceled | OrderStatus::Closed)
                {
                    order.status = status;
                }
            }
            if current.status == OrderStatus::Closed {
                order.status = OrderStatus::Closed;
            }
        }
        order
    }
    fn commit_order_snapshot(cx: &mut Cx<'_, Pending>, rows: Vec<Order>) {
        let rows = rows
            .into_iter()
            .map(|o| {
                if matches!(o.kind, OrderType::Limit | OrderType::Market) {
                    Self::monotonic_order(cx, o)
                } else {
                    o
                }
            })
            .collect();
        cx.store.set_orders(rows);
    }
    pub fn apply_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut order: Order,
        scope: Scope,
        revision: u64,
    ) {
        if scope == Scope::Regular {
            order = Self::monotonic_order(cx, order);
        }
        if order.status == OrderStatus::Open && self.account.terminal.contains_key(&order.id) {
            // A fill can precede cancellation but arrive after its HTTP ack.
            // Retain its cumulative quantity without reopening the order.
            if scope == Scope::Regular
                && let Some(current) = cx.store.order(&order.id)
                && order.filled > current.filled
                && matches!(current.status, OrderStatus::Canceled | OrderStatus::Closed)
            {
                order.status = current.status;
            } else {
                return;
            }
        }
        self.account
            .order_updates
            .insert(order.id.clone(), revision);
        self.account.placed.remove(&order.id);
        if scope == Scope::Stop {
            self.account.stop_ids.insert(order.id.clone());
        }
        if scope == Scope::Plan {
            self.account.plan_ids.insert(order.id.clone());
        }
        if matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed) {
            self.account
                .terminal
                .insert(order.id.clone(), (revision, cx.now));
        }
        cx.store.upsert_order(order);
    }
    pub fn update_rails(&mut self, cx: &mut Cx<'_, Pending>, p: &Position) {
        self.account
            .settings_revision
            .insert(p.symbol.clone(), self.account.revision);
        self.account
            .rail_revision
            .insert((p.symbol.clone(), p.side), self.account.revision);
        let r = self
            .account
            .rails
            .entry(p.symbol.clone())
            .or_insert_with(Rails::initial);
        if p.side == PositionSide::Long {
            r.long = p.leverage;
        } else {
            r.short = p.leverage;
        }
        r.isolated = p.margin_type == Some(MarginType::Isolated);
        cx.store.update_setting(&p.symbol, |s| {
            s.leverage = p.leverage;
            s.buy_leverage = Some(r.long);
            s.sell_leverage = Some(r.short);
            s.margin_type = p.margin_type;
        });
    }
    pub fn map_position(cx: &Cx<'_, Pending>, raw: &PositionRow<'_>) -> Option<Position> {
        let symbol = compact(&raw.symbol);
        let m = cx.store.market(&symbol)?;
        let contracts = raw.hold_vol * m.contract_size.unwrap_or(1.0);
        let notional = contracts.abs() * raw.hold_avg_price;
        let leverage = if raw.leverage > 0.0 {
            raw.leverage
        } else {
            m.limits.leverage.min
        };
        Some(Position {
            id: if raw.position_mode == 2.0 || (raw.position_mode == 0.0 && !cx.store.hedged()) {
                symbol.clone()
            } else {
                format!(
                    "{symbol}:{}",
                    if raw.position_type == 2.0 {
                        "short"
                    } else {
                        "long"
                    }
                )
            },
            symbol,
            side: if raw.position_type == 2.0 {
                PositionSide::Short
            } else {
                PositionSide::Long
            },
            entry_price: raw.hold_avg_price,
            notional,
            leverage,
            margin_type: Some(if raw.open_type == 1.0 {
                MarginType::Isolated
            } else {
                MarginType::Cross
            }),
            position_mode: Some(
                if raw.position_mode == 2.0 || (raw.position_mode == 0.0 && !cx.store.hedged()) {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                },
            ),
            contracts,
            unrealized_pnl: raw.unrealized,
            realized_pnl: Some(raw.realised),
            liquidation_price: raw.liquidate_price,
            margin_size: Some(if raw.im > 0.0 {
                raw.im
            } else {
                notional / leverage
            }),
            last_update: Some(millis(raw.update_time)),
        })
    }
    pub fn map_order(
        &self,
        cx: &Cx<'_, Pending>,
        raw: &OrderRow<'_>,
        scope: Scope,
    ) -> Option<Order> {
        let symbol = compact(&raw.symbol);
        let m = cx.store.market(&symbol)?;
        let id = if scope == Scope::Regular {
            if raw.order_id.is_empty() {
                &raw.id
            } else {
                &raw.order_id
            }
        } else {
            if raw.id.is_empty() {
                &raw.order_id
            } else {
                &raw.id
            }
        };
        if id.is_empty() {
            return None;
        }
        let side = if scope == Scope::Stop {
            if raw.position_type == 1.0 {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            }
        } else {
            side(raw.side)
        };
        let full = scope == Scope::Stop && raw.vol_type == 2.0;
        let native = if full { 0.0 } else { raw.vol };
        let amount = native * m.contract_size.unwrap_or(1.0);
        let filled_native = if scope == Scope::Regular {
            raw.deal_vol
        } else {
            0.0
        };
        let filled = filled_native * m.contract_size.unwrap_or(1.0);
        let kind = match scope {
            Scope::Regular => {
                if raw.order_type == 5.0 {
                    OrderType::Market
                } else {
                    OrderType::Limit
                }
            }
            Scope::Plan => {
                if (side == OrderSide::Sell && raw.trigger_type == 1.0)
                    || (side == OrderSide::Buy && raw.trigger_type == 2.0)
                {
                    OrderType::TakeProfit
                } else {
                    OrderType::StopLoss
                }
            }
            _ => {
                if raw.trigger_side == 1.0
                    || (raw.trigger_side == 0.0 && raw.take_profit_price.unwrap_or(0.0) > 0.0)
                {
                    OrderType::TakeProfit
                } else {
                    OrderType::StopLoss
                }
            }
        };
        let price = if scope == Scope::Regular {
            raw.price
        } else if scope == Scope::Stop {
            if kind == OrderType::TakeProfit {
                raw.take_profit_price.unwrap_or(0.0)
            } else {
                raw.stop_loss_price.unwrap_or(0.0)
            }
        } else {
            raw.trigger_price
        };
        let status = if raw.state == 3.0 {
            OrderStatus::Closed
        } else if (scope == Scope::Regular && matches!(raw.state, 4.0 | 5.0))
            || (scope != Scope::Regular && matches!(raw.state, 2.0 | 4.0 | 5.0))
        {
            OrderStatus::Canceled
        } else {
            OrderStatus::Open
        };
        let previous = cx.store.order(id);
        let trigger = |value| match value {
            Some(1.0) => Some(StopTriggerType::LastPrice),
            Some(2.0) => Some(StopTriggerType::MarkPrice),
            Some(3.0) => Some(StopTriggerType::IndexPrice),
            _ => None,
        };
        let stop_trend = if scope == Scope::Stop {
            raw.trend
        } else {
            None
        };
        let plan_trend = if scope == Scope::Plan {
            raw.trend
        } else {
            None
        };
        let tp_reference = trigger(raw.profit_trend.or(stop_trend).or(
            if kind == OrderType::TakeProfit {
                plan_trend
            } else {
                None
            },
        ))
        .or_else(|| {
            previous
                .and_then(|o| o.extra.as_ref())
                .and_then(|e| e.tp_trigger_type)
        });
        let sl_reference = trigger(raw.loss_trend.or(stop_trend).or(
            if kind == OrderType::StopLoss {
                plan_trend
            } else {
                None
            },
        ))
        .or_else(|| {
            previous
                .and_then(|o| o.extra.as_ref())
                .and_then(|e| e.sl_trigger_type)
        });
        Some(Order {
            id: id.to_string(),
            parent_id: None,
            status,
            symbol,
            kind,
            side,
            price,
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: raw.reduce_only
                || scope != Scope::Regular
                || matches!(raw.side, 2.0 | 4.0),
            close: None,
            client_order_id: None,
            time_in_force: Some(match raw.order_type {
                2.0 => TimeInForce::PostOnly,
                3.0 => TimeInForce::ImmediateOrCancel,
                4.0 => TimeInForce::FillOrKill,
                _ => TimeInForce::GoodTillCancel,
            }),
            last_update: millis(if raw.update_time != 0.0 {
                raw.update_time
            } else {
                raw.create_time
            }),
            trigger_type: match scope {
                Scope::Plan => trigger(raw.trend).or_else(|| previous.and_then(|o| o.trigger_type)),
                Scope::Stop => {
                    if kind == OrderType::TakeProfit {
                        tp_reference
                    } else {
                        sl_reference
                    }
                }
                _ => None,
            },
            take_profit: if scope == Scope::Regular {
                match raw.take_profit_price {
                    Some(p) => (p > 0.0).then_some(p),
                    None => previous.and_then(|o| o.take_profit),
                }
            } else {
                None
            },
            stop_loss: if scope == Scope::Regular {
                match raw.stop_loss_price {
                    Some(p) => (p > 0.0).then_some(p),
                    None => previous.and_then(|o| o.stop_loss),
                }
            } else {
                None
            },
            extra: Some(OrderExtra {
                take_profit_known: raw.take_profit_price.is_some()
                    || previous
                        .and_then(|o| o.extra.as_ref())
                        .is_some_and(|e| e.take_profit_known),
                stop_loss_known: raw.stop_loss_price.is_some()
                    || previous
                        .and_then(|o| o.extra.as_ref())
                        .is_some_and(|e| e.stop_loss_known),
                tp_trigger_type: tp_reference,
                sl_trigger_type: sl_reference,
                remaining_native: Some((native - filled_native).max(0.0)),
                filled_in_placement_units: if scope == Scope::Regular {
                    Some(filled_native)
                } else {
                    None
                },
                ..Default::default()
            }),
        })
    }
    pub fn account_item(&mut self, cx: &mut Cx<'_, Pending>, snapshot: &mut Snapshot, item: &str) {
        snapshot.page_count += 1;
        match snapshot.scope {
            Scope::Assets => match serde_json::from_str::<Asset<'_>>(item) {
                Ok(r) => {
                    let total = r.equity.or(r.cash_balance).unwrap_or(r.available_balance);
                    snapshot.assets.push((
                        r.currency.to_string(),
                        Balance {
                            free: r.available_balance,
                            total,
                            used: (total - r.available_balance).max(r.frozen_balance).max(0.0),
                            upnl: r.unrealized,
                        },
                    ));
                }
                Err(_) => snapshot.bad = true,
            },
            Scope::Positions => match serde_json::from_str::<PositionRow<'_>>(item) {
                Ok(r) => {
                    if let Some(p) = Self::map_position(cx, &r) {
                        snapshot
                            .native_ids
                            .push((p.id.clone(), r.position_id.to_string()));
                        snapshot.positions.push(p);
                    }
                }
                Err(_) => snapshot.bad = true,
            },
            scope => match serde_json::from_str::<OrderRow<'_>>(item) {
                Ok(r) => {
                    if let Some(o) = self.map_order(cx, &r, scope) {
                        snapshot.orders.push(o);
                    } else {
                        snapshot.bad = true;
                    }
                }
                Err(_) => snapshot.bad = true,
            },
        }
    }
    pub fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if response.status == 429 {
            self.account.paused_until = cx.now + 10_000.0;
        }
        let result = if response.status == 200 {
            Envelope::checked(response.body)
        } else {
            Err(format!("MEXC HTTP {}", response.status))
        };
        let envelope = match result {
            Ok(e) => e,
            Err(e) => {
                self.account_failed(cx, request, &e);
                return;
            }
        };
        match request {
            Request::Leverage {
                command,
                symbol,
                revision,
                rows,
                bad,
            } => {
                if bad || rows.is_empty() {
                    cx.done(command, Err("Invalid MEXC leverage response".into()));
                    return;
                }
                let mut rails = self
                    .account
                    .rails
                    .get(&symbol)
                    .copied()
                    .unwrap_or_else(Rails::initial);
                for row in &rows {
                    if row.position_type == 1.0 {
                        rails.long = row.leverage;
                    } else if row.position_type == 2.0 {
                        rails.short = row.leverage;
                    }
                    rails.isolated = row.open_type == 1.0;
                }
                if self
                    .account
                    .settings_revision
                    .get(&symbol)
                    .is_none_or(|&v| v <= revision)
                {
                    self.account.revision += 1;
                    self.account
                        .settings_revision
                        .insert(symbol.clone(), self.account.revision);
                    for row in &rows {
                        let side = if row.position_type == 1.0 {
                            PositionSide::Long
                        } else {
                            PositionSide::Short
                        };
                        self.account
                            .rail_revision
                            .insert((symbol.clone(), side), self.account.revision);
                    }
                    self.account.rails.insert(symbol.clone(), rails);
                    cx.store.update_setting(&symbol, |s| {
                        s.leverage = rails.long;
                        s.buy_leverage = Some(rails.long);
                        s.sell_leverage = Some(rails.short);
                        s.margin_type = Some(if rails.isolated {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        });
                    });
                    let cap = rows
                        .iter()
                        .filter_map(|r| r.max_leverage_view.or(r.max_leverage))
                        .filter(|&n| n > 0.0)
                        .reduce(f64::min);
                    if let Some(cap) = cap {
                        cx.store.update_markets(|m| {
                            if m.symbol == symbol {
                                m.limits.leverage.max = cap;
                            }
                        });
                    }
                }
                let rails = self.account.rails.get(&symbol).copied().unwrap_or(rails);
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct ResultRails {
                    buy_leverage: f64,
                    sell_leverage: f64,
                    margin_type: MarginType,
                    position_mode: PositionMode,
                }
                cx.done(
                    command,
                    Ok(serde_json::to_string(&ResultRails {
                        buy_leverage: rails.long,
                        sell_leverage: rails.short,
                        margin_type: if rails.isolated {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        },
                        position_mode: if cx.store.hedged() {
                            PositionMode::Hedged
                        } else {
                            PositionMode::Oneway
                        },
                    })
                    .unwrap()),
                );
            }
            Request::Mode => {
                let mode = match envelope
                    .payload()
                    .and_then(|p| serde_json::from_str::<u8>(p).map_err(|e| e.to_string()))
                {
                    Ok(mode @ (1 | 2)) => mode,
                    _ => {
                        self.account_failed(cx, Request::Mode, "Invalid MEXC position mode");
                        return;
                    }
                };
                if self.account.mode_revision == 0 {
                    cx.store.set_hedged(mode == 1);
                }
                self.account.mode_ready = true;
                self.refresh_account(cx, true);
            }
            Request::Snapshot(mut s) => {
                if s.bad {
                    self.account_failed(cx, Request::Snapshot(s), "Malformed MEXC snapshot row");
                    return;
                }
                if s.page_count == 100
                    && matches!(s.scope, Scope::Regular | Scope::Plan | Scope::Stop)
                {
                    if s.page >= 1000 {
                        self.account_failed(
                            cx,
                            Request::Snapshot(s),
                            "MEXC snapshot pagination did not terminate",
                        );
                        return;
                    }
                    s.page += 1;
                    s.page_count = 0;
                    self.snapshot_page(cx, s);
                    return;
                }
                self.account.in_flight.remove(&s.scope);
                match s.scope {
                    Scope::Assets => {
                        if self.account.balance_revision <= s.revision {
                            let chosen = s
                                .assets
                                .iter()
                                .find(|(a, b)| {
                                    matches!(a.as_str(), "USDT" | "USDC")
                                        && (b.free != 0.0 || b.total != 0.0)
                                })
                                .or_else(|| {
                                    s.assets
                                        .iter()
                                        .find(|(a, _)| matches!(a.as_str(), "USDT" | "USDC"))
                                })
                                .or_else(|| s.assets.first());
                            cx.store
                                .set_balance(chosen.map(|(_, b)| b.clone()).unwrap_or_default());
                        }
                    }
                    Scope::Positions => {
                        let mut rows = s.positions;
                        rows.retain(|p| {
                            self.account
                                .position_updates
                                .get(&p.id)
                                .is_none_or(|&v| v <= s.revision)
                        });
                        rows.extend(
                            cx.store
                                .positions()
                                .iter()
                                .filter(|p| {
                                    self.account
                                        .position_updates
                                        .get(&p.id)
                                        .is_some_and(|&v| v > s.revision)
                                })
                                .cloned(),
                        );
                        for (id, native) in s.native_ids {
                            if self
                                .account
                                .position_updates
                                .get(&id)
                                .is_none_or(|&v| v <= s.revision)
                            {
                                self.account.native_ids.insert(id, native);
                            }
                        }
                        let allowed: HashSet<_> = rows
                            .iter()
                            .filter(|p| {
                                self.account
                                    .settings_revision
                                    .get(&p.symbol)
                                    .is_none_or(|&v| v <= s.revision)
                            })
                            .map(|p| p.symbol.clone())
                            .collect();
                        for p in &rows {
                            if allowed.contains(&p.symbol) {
                                self.update_rails(cx, p);
                            }
                        }
                        rows.retain(|p| p.contracts > 0.0);
                        cx.store.set_positions(rows);
                    }
                    scope => {
                        s.orders.retain(|o| {
                            !self.account.terminal.contains_key(&o.id)
                                && self
                                    .account
                                    .order_updates
                                    .get(&o.id)
                                    .is_none_or(|&v| v <= s.revision)
                        });
                        for o in &s.orders {
                            if scope == Scope::Stop {
                                self.account.stop_ids.insert(o.id.clone());
                            }
                            if scope == Scope::Plan {
                                self.account.plan_ids.insert(o.id.clone());
                            }
                        }
                        if s.full {
                            if self.account.full_failed {
                                return;
                            }
                            self.account.staged.extend(s.orders);
                            self.account.stage_mask |= match scope {
                                Scope::Regular => 1,
                                Scope::Plan => 2,
                                _ => 4,
                            };
                            if self.account.stage_mask != 7 {
                                return;
                            }
                            let mut rows = std::mem::take(&mut self.account.staged);
                            self.account.stage_mask = 0;
                            self.account.needs_full = false;
                            rows.retain(|o| {
                                !self.account.terminal.contains_key(&o.id)
                                    && self
                                        .account
                                        .order_updates
                                        .get(&o.id)
                                        .is_none_or(|&v| v <= s.revision)
                            });
                            for o in cx.store.orders() {
                                if o.status == OrderStatus::Open
                                    && !rows.iter().any(|r| r.id == o.id)
                                    && (self
                                        .account
                                        .order_updates
                                        .get(&o.id)
                                        .is_some_and(|&v| v > s.revision)
                                        || self.account.placed.contains_key(&o.id))
                                {
                                    rows.push(o.clone());
                                }
                            }
                            Self::commit_order_snapshot(cx, rows);
                        } else {
                            let mut rows = s.orders;
                            for o in cx.store.orders() {
                                let is_stop = self.account.stop_ids.contains(&o.id)
                                    || self.account.plan_ids.contains(&o.id);
                                if o.status == OrderStatus::Open
                                    && !rows.iter().any(|r| r.id == o.id)
                                    && (is_stop
                                        || cx.now - o.last_update <= 15_000.0
                                        || self
                                            .account
                                            .order_updates
                                            .get(&o.id)
                                            .is_some_and(|&v| v > s.revision))
                                {
                                    rows.push(o.clone());
                                }
                            }
                            Self::commit_order_snapshot(cx, rows);
                        }
                    }
                }
            }
        }
    }
    pub fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>, full: bool) {
        self.account.needs_full |= full || !cx.store.loaded().orders;
        self.snapshot_account(cx, Scope::Assets, false);
        self.snapshot_account(cx, Scope::Positions, false);
        let full = self.account.needs_full;
        let order_inflight = [Scope::Regular, Scope::Plan, Scope::Stop]
            .iter()
            .any(|s| self.account.in_flight.contains(s));
        if !order_inflight {
            if full {
                self.account.staged.clear();
                self.account.stage_mask = 0;
                self.account.full_failed = false;
            }
            self.snapshot_account(cx, Scope::Regular, full);
            if full {
                self.snapshot_account(cx, Scope::Plan, true);
                self.snapshot_account(cx, Scope::Stop, true);
            }
        }
        if self.account.poll.is_none() {
            self.account.poll = Some(cx.set_timer(2000.0));
        }
    }
    pub fn account_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let Request::Leverage { command, .. } = &request {
            cx.done(*command, Err(error.into()));
        }
        if let Request::Snapshot(s) = request {
            self.account.in_flight.remove(&s.scope);
            if s.full {
                self.account.stage_mask = 0;
                self.account.full_failed = true;
                self.account.needs_full = true;
                self.account.staged.clear();
            }
        }
        cx.log(LogLevel::Error, error);
        if self.account.poll.is_none() {
            self.account.poll = Some(cx.set_timer(2000.0));
        }
    }
    pub fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.poll == Some(timer) {
            self.account.poll = None;
            if self.account.mode_ready {
                self.refresh_account(cx, false);
            } else if cx.now >= self.account.paused_until {
                self.signed(
                    cx,
                    Method::Get,
                    "/api/v1/private/position/position_mode",
                    String::new(),
                    false,
                    Pending::Account(Request::Mode),
                );
                self.account.poll = Some(cx.set_timer(2000.0));
            }
            if self.account.poll.is_none() {
                self.account.poll = Some(cx.set_timer(2000.0));
            }
            self.account
                .terminal
                .retain(|_, (_, at)| cx.now - *at < 30_000.0);
            self.account.fills.retain(|_, at| cx.now - *at < 300_000.0);
            self.account.placed.retain(|_, at| cx.now - *at < 20_000.0);
        }
        if self.account.asset_refresh == Some(timer) {
            self.account.asset_refresh = None;
            self.snapshot_account(cx, Scope::Assets, false);
        }
        if self.account.stop_refresh == Some(timer) {
            self.account.stop_refresh = None;
            self.refresh_account(cx, true);
        }
        if self.account.private.reconnect == Some(timer) {
            self.account.private.reconnect = None;
            self.account.private.socket =
                Some(cx.connect_channel(WS, safe_cex_engine::SocketChannel::Private));
        }
        if self.account.private.heartbeat == Some(timer) {
            self.account.private.heartbeat = None;
            if let Some(socket) = self.account.private.socket {
                if cx.now - self.account.private.last_received >= crate::SILENCE_MS {
                    cx.close(socket);
                    self.account_close(cx, socket);
                } else {
                    cx.send(socket, r#"{"method":"ping"}"#);
                    self.account.private.heartbeat = Some(cx.set_timer(HEARTBEAT_MS));
                }
            }
        }
    }
    pub fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.private.socket != Some(socket) {
            return;
        }
        self.account.private.open = false;
        self.account.private.socket = None;
        if let Some(t) = self.account.private.heartbeat.take() {
            cx.clear_timer(t);
        }
        self.account.private.attempts += 1;
        self.account.private.reconnect = Some(cx.set_timer(
            (500.0 * 2f64.powi(self.account.private.attempts.min(6) as i32)).min(30_000.0),
        ));
    }
}
pub fn side(n: f64) -> OrderSide {
    if matches!(n, 3.0 | 4.0) {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}
