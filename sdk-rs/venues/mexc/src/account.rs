//! Private stream and paged account reconciliation. Live revisions win over
//! snapshots that started before them; failed pages never replace store data.
use crate::{Connection, MexcFutures, Pending, WS, sign, wire::*};
use safe_cex_engine::Level;
use safe_cex_engine::coverage::{RowReports, SnapshotBuilder};
use safe_cex_engine::error::{ErrorKind, VenueError};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::*;
use safe_cex_engine::socket::{Config, Supervisor, Tick};
use safe_cex_engine::{Cx, Fill, FillFinancials, Method, Response, SocketId, TimerId};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
    Snapshot(Box<Snapshot>),
}
pub struct Snapshot {
    pub symbol: Option<String>,
    pub scope: Scope,
    pub revision: u64,
    pub page: u32,
    pub page_count: usize,
    /// A structural failure (scope mismatch, held catalogue exposure): nothing commits.
    pub bad: bool,
    pub orders: SnapshotBuilder<String, Order>,
    pub positions: SnapshotBuilder<String, Position>,
    pub assets: SnapshotBuilder<String, (String, Balance)>,
    pub full: bool,
    pub native_ids: Vec<(String, String)>,
}
#[derive(Default)]
pub struct Account {
    pub(crate) supervisor: Supervisor,
    authenticated: bool,
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
    pub stop_refresh: HashMap<String, TimerId>,
    pub pending_stop_refresh: BTreeSet<String>,
    pub in_flight: HashSet<Scope>,
    pub balance_revision: u64,
    pub position_revision: u64,
    pub mode_revision: u64,
    pub staged: Vec<Order>,
    pub stage_mask: u8,
    pub full_failed: bool,
    pub needs_full: bool,
    pub mode_ready: bool,
    pub auth_rejected: bool,
    pub fills: HashMap<String, f64>,
    pub paused_until: f64,
    /// `coin:`/`position:`/`order:` + identity (empty when unreadable) of skips already logged.
    skip_logged: RowReports<String>,
}

impl Account {
    pub(crate) fn new() -> Self {
        Self {
            supervisor: Supervisor::new(Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 20_000.0,
                ping_interval_ms: Some(8_000.0),
                ..Config::default()
            }),
            ..Self::default()
        }
    }
}

/// Ids (by `key`) of accepted rows whose identity no rejection protects: they recovered.
fn mapped_ids<T>(builder: &SnapshotBuilder<String, T>, key: impl Fn(&T) -> &String) -> Vec<String> {
    builder
        .rows()
        .iter()
        .map(key)
        .filter(|id| builder.allows_absence(id))
        .cloned()
        .collect()
}

impl MexcFutures {
    pub fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, command: u32, symbol: String) {
        if cx.now < self.account.paused_until {
            Self::fail_local_read(cx, "MEXC account reads are rate limited");
            return;
        }
        let Some(m) = cx.store.market(&symbol) else {
            Self::fail_local_read(cx, "Market not found");
            return;
        };
        if self.credentials.is_none() {
            Self::fail_local_read(cx, "MEXC leverage requires credentials");
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
        self.connect_private(cx);
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
        self.snapshot_orders(cx, scope, full, None);
    }
    fn snapshot_orders(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        scope: Scope,
        full: bool,
        symbol: Option<String>,
    ) {
        if self.account.auth_rejected
            || self.account.in_flight.contains(&scope)
            || cx.now < self.account.paused_until
        {
            return;
        }
        self.account.in_flight.insert(scope);
        let snapshot = Box::new(Snapshot {
            symbol,
            scope,
            revision: self.account.revision,
            page: 1,
            page_count: 0,
            bad: false,
            orders: SnapshotBuilder::default(),
            positions: SnapshotBuilder::default(),
            assets: SnapshotBuilder::default(),
            full,
            native_ids: vec![],
        });
        self.snapshot_page(cx, snapshot);
    }
    fn snapshot_page(&mut self, cx: &mut Cx<'_, Pending>, snapshot: Box<Snapshot>) {
        let mut path = match snapshot.scope {
            Scope::Assets => "/api/v1/private/account/assets",
            Scope::Positions => "/api/v1/private/position/open_positions",
            Scope::Regular => "/api/v1/private/order/list/open_orders",
            Scope::Plan => "/api/v1/private/planorder/list/orders",
            Scope::Stop => "/api/v1/private/stoporder/list/orders",
        }
        .to_owned();
        let mut params = vec![];
        if let Some(symbol) = &snapshot.symbol {
            let market = cx.store.market(symbol).expect("scoped snapshot market");
            if snapshot.scope == Scope::Regular {
                path.push('/');
                path.push_str(&market.id);
            } else {
                params.push(("symbol", market.id.clone()));
            }
        }
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
            &path,
            sign::query(params),
            true,
            Pending::Account(Request::Snapshot(snapshot)),
        );
    }
    fn connect_private(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.authenticated = false;
        self.account.supervisor.connecting(cx);
        self.account.private.socket = Some(cx.connect(
            safe_cex_engine::SocketSpec::new(WS).channel(safe_cex_engine::SocketChannel::Private),
        ));
    }
    pub fn account_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        let Some(identity) = self.credentials.clone() else {
            return;
        };
        let slots = safe_cex_engine::credential_selection::Slots::new(
            Some(identity),
            None,
            Default::default(),
        );
        let Ok((_, identity)) =
            slots.select(safe_cex_engine::credential_selection::CredentialRole::SessionAuth)
        else {
            return;
        };
        self.account.private.open = true;
        self.account.supervisor.opened(cx);
        cx.send(socket, r#"{"method":"ping"}"#);
        cx.send_signed(
            socket,
            String::new(),
            Box::new(sign::dispatch::Login::new(identity.clone(), socket)),
        );
    }

    pub fn account_message(&mut self, cx: &mut Cx<'_, Pending>, text: &str) {
        self.account.supervisor.received(cx.now);
        let Ok(m) = serde_json::from_str::<Message<'_>>(text) else {
            return;
        };
        if m.channel == "rs.login" {
            if m.success != Some(false)
                && (m.success == Some(true) || m.data.is_some_and(|d| d.get() == r#""success""#))
            {
                if self.account.authenticated {
                    return;
                }
                self.account.authenticated = true;
                self.account.supervisor.authenticated(cx);
                if let Some(socket) = self.account.private.socket {
                    cx.send(socket,r#"{"method":"personal.filter","param":{"filters":[{"filter":"order"},{"filter":"order.deal"},{"filter":"position"},{"filter":"asset"},{"filter":"plan.order"},{"filter":"stop.order"},{"filter":"stop.planorder"}]}}"#);
                }
                if self.account.mode_ready {
                    self.refresh_account(cx, true);
                }
            } else {
                cx.notice(
                    Level::Error,
                    code::AUTH_REJECTED,
                    "MEXC private ws login failed, double check your key and secret",
                );
                if let Some(socket) = self.account.private.socket {
                    cx.close(socket);
                    self.account_close(cx, socket);
                }
            }
            return;
        }
        if m.channel == "push.personal.asset" {
            if self.account.auth_rejected {
                return;
            }
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
                        cx.notice(
                            Level::Error,
                            code::ORDER_REJECTED,
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
                    cx.fill_with_financials(
                        Fill {
                            id: None,
                            order_id: raw.order_id.to_string(),
                            symbol,
                            side: side(raw.side),
                            price: raw.price,
                            amount: raw.vol * m.contract_size.unwrap_or(1.0),
                        },
                        FillFinancials {
                            realized_pnl: raw.profit.or(raw.realized_pnl),
                            fee: raw.fee.map(f64::abs),
                        },
                    );
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
    fn reconcile_snapshot_orders(
        &self,
        cx: &Cx<'_, Pending>,
        rows: &mut Vec<Order>,
        revision: u64,
    ) {
        rows.retain_mut(|order| {
            // An open-order endpoint may lag a terminal stream update. Keep a
            // returned regular row terminal while preserving cumulative fills;
            // a tombstone without a current row must never resurrect an order.
            if matches!(order.kind, OrderType::Limit | OrderType::Market)
                && cx.store.order(&order.id).is_some_and(|current| {
                    matches!(current.status, OrderStatus::Canceled | OrderStatus::Closed)
                })
            {
                let current = cx.store.order(&order.id).unwrap();
                let status = if order.status == OrderStatus::Closed {
                    OrderStatus::Closed
                } else {
                    current.status
                };
                *order = Self::monotonic_order(cx, order.clone());
                order.status = status;
                return true;
            }
            !self.account.terminal.contains_key(&order.id)
                && self
                    .account
                    .order_updates
                    .get(&order.id)
                    .is_none_or(|&updated| updated <= revision)
        });
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
            reduce_only: raw.reduce_only || scope == Scope::Stop || matches!(raw.side, 2.0 | 4.0),
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
                // An unreadable asset keeps its stored balance.
                Err(_) => {
                    let currency = serde_json::from_str::<RowIdentity<'_>>(item)
                        .ok()
                        .map(|row| row.currency.into_owned())
                        .filter(|currency| !currency.is_empty());
                    let message = match &currency {
                        Some(currency) => format!("MEXC skipped malformed {currency} asset row"),
                        None => "MEXC skipped asset row without a currency".into(),
                    };
                    let key = format!("coin:{}", currency.as_deref().unwrap_or_default());
                    self.log_skip(cx, key, Level::Warn, message);
                    snapshot.assets.reject(currency);
                }
            },
            Scope::Positions => match serde_json::from_str::<PositionRow<'_>>(item) {
                Ok(r) => {
                    let symbol = compact(&r.symbol);
                    if r.hold_vol != 0.0 && cx.store.market(&symbol).is_none() {
                        if self.malformed_markets.contains(&symbol) {
                            snapshot.bad = true;
                            cx.notice(Level::Warn, code::SNAPSHOT_DISCARDED, "MEXC owned position market unavailable; preserving previous snapshot");
                        } else {
                            cx.notice(
                                Level::Warn,
                                code::ROW_SKIPPED,
                                format!("Skipping MEXC position on uncatalogued market {symbol}"),
                            );
                        }
                    } else if let Some(p) = Self::map_position(cx, &r) {
                        snapshot
                            .native_ids
                            .push((p.id.clone(), r.position_id.to_string()));
                        snapshot.positions.push(p);
                    }
                }
                // An unreadable position keeps its stored legs.
                Err(_) => {
                    let symbol = serde_json::from_str::<RowIdentity<'_>>(item)
                        .ok()
                        .map(|row| compact(&row.symbol))
                        .filter(|symbol| !symbol.is_empty());
                    let Some(symbol) = symbol else {
                        let level = if cx.store.positions().is_empty() {
                            Level::Error
                        } else {
                            Level::Warn
                        };
                        let message = "MEXC skipped position row without a symbol";
                        self.log_skip(cx, "position:".into(), level, message);
                        snapshot.positions.reject(None);
                        return;
                    };
                    if cx.store.market(&symbol).is_none()
                        && self.malformed_markets.contains(&symbol)
                    {
                        snapshot.bad = true;
                        cx.notice(
                            Level::Warn,
                            code::SNAPSHOT_DISCARDED,
                            "MEXC owned position market unavailable; preserving previous snapshot",
                        );
                        return;
                    }
                    let level = if cx.store.positions().iter().any(|p| p.symbol == symbol) {
                        Level::Warn
                    } else {
                        Level::Error
                    };
                    let message = format!("MEXC skipped malformed {symbol} position row");
                    self.log_skip(cx, format!("position:{symbol}"), level, message);
                    for id in [
                        symbol.clone(),
                        format!("{symbol}:long"),
                        format!("{symbol}:short"),
                    ] {
                        snapshot.positions.reject(Some(id));
                    }
                }
            },
            scope => match serde_json::from_str::<OrderRow<'_>>(item) {
                Ok(r) => {
                    if let Some(o) = self.map_order(cx, &r, scope) {
                        if snapshot
                            .symbol
                            .as_ref()
                            .is_some_and(|symbol| *symbol != o.symbol)
                        {
                            snapshot.bad = true;
                        } else {
                            snapshot.orders.push(o);
                        }
                    } else {
                        let id = if scope == Scope::Regular {
                            if r.order_id.is_empty() {
                                &r.id
                            } else {
                                &r.order_id
                            }
                        } else if r.id.is_empty() {
                            &r.order_id
                        } else {
                            &r.id
                        };
                        // A typed but unmappable row is not proof that a known order
                        // vanished; one without an id could be any of them.
                        self.reject_order(cx, snapshot, id, &r.symbol);
                    }
                }
                Err(_) => {
                    let row = serde_json::from_str::<RowIdentity<'_>>(item).ok();
                    let id = row.as_ref().map_or("", |row| {
                        let (first, second) = if scope == Scope::Regular {
                            (&row.order_id, &row.id)
                        } else {
                            (&row.id, &row.order_id)
                        };
                        if first.is_empty() { second } else { first }
                    });
                    let symbol = row.as_ref().map_or("", |row| &row.symbol);
                    self.reject_order(cx, snapshot, id, symbol);
                }
            },
        }
    }
    fn reject_order(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        snapshot: &mut Snapshot,
        id: &str,
        symbol: &str,
    ) {
        if id.is_empty() {
            let message = "MEXC skipped order row without an id";
            self.log_skip(cx, "order:".into(), Level::Warn, message);
            snapshot.orders.reject(None);
        } else {
            let message = format!("MEXC skipped unmappable order {id} on {symbol}");
            self.log_skip(cx, format!("order:{id}"), Level::Warn, message);
            snapshot.orders.reject(Some(id.to_owned()));
        }
    }
    /// Logs a skipped row once per `key` until it maps again; polls would otherwise repeat it.
    fn log_skip(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        key: String,
        level: Level,
        message: impl Into<String>,
    ) {
        self.account.skip_logged.report(cx, key, level, message);
    }
    /// Re-arms the skip logs of `prefix`: all after a clean read, else those `ids` mapped.
    fn rearm_skips(&mut self, prefix: &str, clean: bool, ids: Vec<String>) {
        self.account
            .skip_logged
            .rearm_namespace(prefix, clean, ids.iter().map(String::as_str));
    }
    pub fn account_startup_refusal(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &Request,
        response: &Response<'_>,
    ) -> bool {
        if let (true, Request::Mode | Request::Snapshot(_)) = (self.account.auth_rejected, request)
        {
            return true;
        }
        // Initial mode and plan-list credential refusals cannot recover by polling.
        let read_key = self.options.identities.secondary.is_some();
        let initial_plan = if let Request::Snapshot(snapshot) = request {
            snapshot.scope == Scope::Plan && !cx.store.loaded().orders
        } else {
            false
        };
        let startup_read =
            (matches!(request, Request::Mode) && !self.account.mode_ready) || initial_plan;
        if startup_read && let Some(error) = auth_rejection(response.body, read_key) {
            self.account.auth_rejected = true;
            self.account.in_flight.clear();
            self.account.staged.clear();
            self.account.stage_mask = 0;
            self.account.full_failed = true;
            if let Some(timer) = self.account.poll.take() {
                cx.clear_timer(timer);
            }
            if let Some(timer) = self.account.asset_refresh.take() {
                cx.clear_timer(timer);
            }
            cx.notice(Level::Error, code::AUTH_REJECTED, &error.message);
            let refused = error.kind == Some(ErrorKind::AuthInvalid);
            cx.startup_failed(error);
            // As Bybit: a refused trading key also stops the private socket's login loop.
            if refused && !read_key {
                cx.deactivate();
            }
            return true;
        }
        false
    }
    pub fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if self.account_startup_refusal(cx, &request, &response) {
            return;
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
                command: _,
                symbol,
                revision,
                rows,
                bad,
            } => {
                if bad || rows.is_empty() {
                    self.private_fail(cx, "Invalid MEXC leverage response", false);
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
                self.private_done(
                    cx,
                    serde_json::to_string(&ResultRails {
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
                    .unwrap(),
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
                            let mapped = mapped_ids(&s.assets, |(currency, _)| currency);
                            self.rearm_skips("coin:", s.assets.rejected() == 0, mapped);
                            // Rejected rows keep their stored balances, which the choice may use.
                            let current = cx
                                .store
                                .currency_balances()
                                .unwrap_or_default()
                                .iter()
                                .map(|c| (c.currency.clone(), c.balance.clone()))
                                .collect::<Vec<_>>();
                            let assets = s.assets.commit(current, |(currency, _)| currency.clone());
                            let chosen = assets
                                .iter()
                                .find(|(a, b)| {
                                    matches!(a.as_str(), "USDT" | "USDC")
                                        && (b.free != 0.0 || b.total != 0.0)
                                })
                                .or_else(|| {
                                    assets
                                        .iter()
                                        .find(|(a, _)| matches!(a.as_str(), "USDT" | "USDC"))
                                })
                                .or_else(|| assets.first());
                            cx.store
                                .set_balance(chosen.map(|(_, b)| b.clone()).unwrap_or_default());
                            // Duplicate rows keep the last one, as the TS reducer does.
                            let currencies: BTreeMap<String, Balance> = assets
                                .into_iter()
                                .filter(|(currency, _)| !currency.is_empty())
                                .collect();
                            cx.store.set_currency_balances(
                                currencies
                                    .into_iter()
                                    .map(|(currency, balance)| CurrencyBalance {
                                        currency,
                                        balance,
                                    })
                                    .collect(),
                            );
                        }
                    }
                    Scope::Positions => {
                        let mapped = mapped_ids(&s.positions, |p| &p.symbol);
                        self.rearm_skips("position:", s.positions.rejected() == 0, mapped);
                        // Rejected rows keep their stored legs; newer stream rows still win.
                        let mut rows = s
                            .positions
                            .commit(cx.store.positions().iter().cloned(), |p| p.id.clone());
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
                        let mapped = mapped_ids(&s.orders, |o| &o.id);
                        self.rearm_skips("order:", s.orders.rejected() == 0, mapped);
                        // Rejected rows keep the stored open orders of this scope and symbol.
                        let current: Vec<Order> = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| {
                                o.status == OrderStatus::Open
                                    && s.symbol.as_ref().is_none_or(|symbol| *symbol == o.symbol)
                                    && match scope {
                                        Scope::Plan => self.account.plan_ids.contains(&o.id),
                                        Scope::Stop => self.account.stop_ids.contains(&o.id),
                                        _ => {
                                            !self.account.plan_ids.contains(&o.id)
                                                && !self.account.stop_ids.contains(&o.id)
                                        }
                                    }
                            })
                            .cloned()
                            .collect();
                        let mut orders =
                            std::mem::take(&mut s.orders).commit(current, |o| o.id.clone());
                        self.reconcile_snapshot_orders(cx, &mut orders, s.revision);
                        for o in &orders {
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
                            self.account.staged.extend(orders);
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
                            if s.symbol.is_none() {
                                self.account.needs_full = false;
                            }
                            self.reconcile_snapshot_orders(cx, &mut rows, s.revision);
                            for o in cx.store.orders() {
                                // A symbol snapshot cannot retire another market's rows.
                                if s.symbol.as_ref().is_some_and(|symbol| *symbol != o.symbol) {
                                    rows.push(o.clone());
                                    continue;
                                }
                                if o.status == OrderStatus::Open
                                    && !rows.iter().any(|r| r.id == o.id)
                                    && ((s.symbol.is_some()
                                        && !self.account.plan_ids.contains(&o.id)
                                        && !self.account.stop_ids.contains(&o.id))
                                        || self
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
                            let mut rows = orders;
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
    pub fn schedule_stop_refresh(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if let Some(timer) = self.account.stop_refresh.remove(symbol) {
            cx.clear_timer(timer);
        }
        self.account.pending_stop_refresh.remove(symbol);
        self.account
            .stop_refresh
            .insert(symbol.to_owned(), cx.set_timer(1500.0));
    }
    // Order batches share staging and revision fences. Serialize scoped batches
    // with full-account polls so neither can commit a partial view of the other.
    pub fn refresh_stop_orders(&mut self, cx: &mut Cx<'_, Pending>) {
        if cx.now < self.account.paused_until
            || !cx.store.loaded().orders
            || self.account.needs_full
            || [Scope::Regular, Scope::Plan, Scope::Stop]
                .iter()
                .any(|scope| self.account.in_flight.contains(scope))
        {
            return;
        }
        let Some(symbol) = self.account.pending_stop_refresh.pop_first() else {
            return;
        };
        self.account.staged.clear();
        self.account.stage_mask = 0;
        self.account.full_failed = false;
        for scope in [Scope::Regular, Scope::Plan, Scope::Stop] {
            self.snapshot_orders(cx, scope, true, Some(symbol.clone()));
        }
    }
    pub fn refresh_account(&mut self, cx: &mut Cx<'_, Pending>, full: bool) {
        if self.account.auth_rejected {
            return;
        }
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
            self.account.poll = Some(cx.set_poll_timer(2000.0));
        }
    }
    pub fn account_failed(&mut self, cx: &mut Cx<'_, Pending>, request: Request, error: &str) {
        if let (true, Request::Mode | Request::Snapshot(_)) = (self.account.auth_rejected, &request)
        {
            return;
        }
        if let Request::Leverage { .. } = &request {
            self.private_fail(cx, error, true);
        } else {
            cx.retried_failure("mexc.account:1339", error);
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
        if !self.account.auth_rejected && self.account.poll.is_none() {
            self.account.poll = Some(cx.set_poll_timer(2000.0));
        }
    }
    pub fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.poll == Some(timer) {
            self.account.poll = None;
            if self.account.mode_ready {
                self.refresh_account(cx, false);
            } else if cx.now >= self.account.paused_until && !self.account.auth_rejected {
                self.signed(
                    cx,
                    Method::Get,
                    "/api/v1/private/position/position_mode",
                    String::new(),
                    false,
                    Pending::Account(Request::Mode),
                );
                self.account.poll = Some(cx.set_poll_timer(2000.0));
            }
            if self.account.poll.is_none() {
                self.account.poll = Some(cx.set_poll_timer(2000.0));
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
        let symbol = self
            .account
            .stop_refresh
            .iter()
            .find_map(|(symbol, &pending)| (pending == timer).then(|| symbol.clone()));
        if let Some(symbol) = symbol {
            self.account.stop_refresh.remove(&symbol);
            self.account.pending_stop_refresh.insert(symbol);
            self.refresh_stop_orders(cx);
        }
        match self.account.supervisor.on_timer(cx, timer) {
            Tick::Reconnect => self.connect_private(cx),
            Tick::Ping => {
                if let Some(socket) = self.account.private.socket {
                    cx.send(socket, r#"{"method":"ping"}"#);
                }
            }
            Tick::TimedOut(_) => {
                if let Some(socket) = self.account.private.socket {
                    cx.close(socket);
                    self.account_close(cx, socket);
                }
            }
            _ => (),
        }
    }
    pub fn account_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.private.socket != Some(socket) {
            return;
        }
        self.account.private.open = false;
        self.account.private.socket = None;
        self.account.supervisor.retry(cx);
    }
}
pub fn side(n: f64) -> OrderSide {
    if matches!(n, 3.0 | 4.0) {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    }
}

pub(crate) fn auth_scope(pending: &Pending) -> Option<safe_cex_engine::AccountReadScope> {
    use safe_cex_engine::AccountReadScope::*;
    Some(match pending {
        Pending::Account(Request::Mode) => Settings,
        Pending::Account(Request::Snapshot(s)) => match s.scope {
            Scope::Assets => Balance,
            Scope::Positions => Positions,
            Scope::Regular | Scope::Plan | Scope::Stop => Orders,
        },
        _ => return None,
    })
}
impl MexcFutures {
    /// One order endpoint's rejection holds back its siblings' acceptance.
    pub(crate) fn account_auth(
        &self,
        pending: &Pending,
        response: &Response<'_>,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        let outcome = self.account_auth_evidence(pending, response);
        match pending {
            Pending::Account(Request::Snapshot(s)) => match s.scope {
                Scope::Regular => self.orders_auth.report(0, outcome),
                Scope::Plan => self.orders_auth.report(1, outcome),
                Scope::Stop => self.orders_auth.report(2, outcome),
                Scope::Assets | Scope::Positions => outcome,
            },
            _ => outcome,
        }
    }
    fn account_auth_evidence(
        &self,
        pending: &Pending,
        response: &Response<'_>,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        use safe_cex_engine::AuthOutcome::{Accepted, Rejected};
        auth_scope(pending)?;
        let e: Envelope<'_> = serde_json::from_str(response.body).ok()?;
        if e.success != Some(true)
            && matches!(
                e.code,
                401.0 | 402.0 | 406.0 | 602.0 | 701.0 | 702.0 | 703.0 | 704.0
            )
        {
            return Some(Rejected);
        }
        if response.status != 200 || e.success != Some(true) || e.code != 0.0 {
            return None;
        }
        match pending {
            Pending::Account(Request::Mode) => e
                .data
                .is_some_and(|d| matches!(serde_json::from_str::<u8>(d.get()), Ok(1 | 2)))
                .then_some(Accepted),
            Pending::Account(Request::Snapshot(_)) => safe_cex_engine::AuthOutcome::accepted_list(
                response,
                ArrayAt::EnvelopeField("data"),
            ),
            _ => None,
        }
    }
}

/// MEXC's documented key refusals. Only a parsed venue code counts: a bare HTTP
/// 401/403 may come from a CDN, WAF or proxy and is retried.
fn auth_rejection(body: &str, read_key: bool) -> Option<VenueError> {
    let envelope = serde_json::from_str::<Envelope<'_>>(body).ok()?;
    let (kind, reason) = match envelope.code {
        401.0 | 602.0 => (ErrorKind::AuthInvalid, "rejected"),
        402.0 => (ErrorKind::AuthExpired, "expired"),
        406.0 | 701.0 | 702.0 | 703.0 | 704.0 => (ErrorKind::AuthPermission, "permission denied"),
        _ => return None,
    };
    let which = if read_key {
        "read-only API key"
    } else {
        "API key"
    };
    let code = envelope.code.to_string();
    let venue_message = envelope.message.into_owned();
    let mut error = VenueError::new(format!("MEXC {which} {reason} ({code}): {venue_message}"))
        .kind(kind)
        .venue_code(code);
    if !venue_message.is_empty() {
        error = error.venue_message(venue_message);
    }
    Some(error)
}

impl MexcFutures {
    pub(crate) fn queue_trade(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        payload: String,
        pending: Pending,
    ) {
        use safe_cex_engine::credential_selection::{CredentialRole, Slots};
        let slots = Slots::new(
            self.credentials.clone(),
            self.options.identities.secondary.clone(),
            Default::default(),
        );
        let role = if method == Method::Get {
            CredentialRole::Read { scope: None }
        } else {
            CredentialRole::Mutation
        };
        let Ok((_, identity)) = slots.select(role) else {
            return;
        };
        let step = crate::sign::dispatch::Http::new(
            identity.clone(),
            method,
            path.into(),
            payload.clone(),
            self.options.recv_window,
        );
        let url = step.url();
        let headers = vec![("Content-Type", "application/json".into())];
        let request = cx.request(
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
        cx.sign_http(request, Box::new(step));
    }
}

#[cfg(test)]
mod auth_envelopes {
    use super::*;
    use safe_cex_engine::AuthOutcome::{Accepted, Rejected};

    fn pending(scope: Scope, rejected_rows: bool) -> Pending {
        let mut snapshot = Snapshot {
            symbol: None,
            scope,
            revision: 0,
            page: 1,
            page_count: 0,
            bad: rejected_rows,
            orders: SnapshotBuilder::default(),
            positions: SnapshotBuilder::default(),
            assets: SnapshotBuilder::default(),
            full: true,
            native_ids: vec![],
        };
        if rejected_rows {
            snapshot.orders.reject(None);
        }
        Pending::Account(Request::Snapshot(Box::new(snapshot)))
    }

    fn response(body: &str) -> Response<'_> {
        Response {
            status: 200,
            headers: "",
            body,
        }
    }

    #[test]
    fn malformed_success_cannot_clear_a_rejected_endpoint_before_a_sibling_succeeds() {
        for body in [
            r#"{"success":true,"code":0}"#,
            r#"{"success":true,"code":0,"data":null}"#,
            r#"{"success":true,"code":0,"data":{}}"#,
            r#"{"success":true,"code":0,"data":false}"#,
        ] {
            let venue = MexcFutures::new(crate::Options::default());
            let plan = pending(Scope::Plan, false);
            let regular = pending(Scope::Regular, false);
            assert_eq!(
                venue.account_auth(&plan, &response(r#"{"success":false,"code":701}"#)),
                Some(Rejected)
            );
            assert_eq!(venue.account_auth(&plan, &response(body)), None, "{body}");
            assert_eq!(
                venue.account_auth(
                    &regular,
                    &response(r#"{"success":true,"code":0,"data":[]}"#)
                ),
                None
            );
            assert_eq!(
                venue.account_auth(&plan, &response(r#"{"success":true,"code":0,"data":[]}"#)),
                Some(Accepted)
            );
        }
    }

    #[test]
    fn held_or_rejected_rows_do_not_override_typed_envelope_evidence() {
        let venue = MexcFutures::new(crate::Options::default());
        let plan = pending(Scope::Plan, true);
        assert_eq!(
            venue.account_auth(&plan, &response(r#"{"success":true,"code":0,"data":[]}"#)),
            Some(Accepted)
        );
        assert_eq!(
            venue.account_auth(
                &plan,
                &response(r#"{"success":false,"code":701,"data":[]}"#)
            ),
            Some(Rejected)
        );
    }
}
