//! Account snapshots commit only if no newer socket event has changed that axis.
use crate::sign::{self, Params};
use crate::wire::*;
use crate::{BitunixFutures, Feed, Pending, REST};
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, TimerId, json};
use std::collections::HashMap;

pub const ACCOUNT: &str = "/api/v1/futures/account";
pub const POSITIONS: &str = "/api/v1/futures/position/get_pending_positions";
pub const ORDERS: &str = "/api/v1/futures/trade/get_pending_orders";
pub const TPSL: &str = "/api/v1/futures/tpsl/get_pending_orders";
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Balance,
    Positions,
    Orders,
    Tpsl,
}
pub struct Read {
    pub axis: Axis,
    pub generation: u64,
    pub serial: u64,
    pub after: Option<u32>,
    pub positions: Vec<Position>,
    pub identities: Vec<(String, String)>,
    pub orders: Vec<Order>,
    pub native: Vec<(String, NativeOrder)>,
    pub invalid: bool,
    pub baselines: Vec<(String, (f64, f64))>,
}
#[derive(Clone, Default)]
pub struct NativeOrder {
    pub raw_id: String,
    pub position_id: String,
    pub full_position: bool,
    pub attached: bool,
    pub tp: Option<f64>,
    pub sl: Option<f64>,
    pub tp_stop: Option<StopTriggerType>,
    pub sl_stop: Option<StopTriggerType>,
    pub tp_execution: String,
    pub sl_execution: String,
    pub tp_limit: f64,
    pub sl_limit: f64,
}
#[derive(Default)]
pub struct AccountState {
    pub generations: [u64; 3],
    pub serials: [u64; 4],
    pub initial_orders: [Option<Read>; 2],
    pub terminal: HashMap<String, f64>,
    pub timers: [Option<TimerId>; 3],
    pub tick: Option<TimerId>,
    pub count: u32,
    pub position_ids: HashMap<String, String>,
    pub ws_position_ids: HashMap<String, String>,
    pub native: HashMap<String, NativeOrder>,
    pub tifs: HashMap<String, TimeInForce>,
    pub reduces: HashMap<String, bool>,
    pub filled: HashMap<String, (f64, f64)>,
    pub login_retry: Option<TimerId>,
}
impl Axis {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Balance => 0,
            Self::Positions => 1,
            Self::Orders | Self::Tpsl => 2,
        }
    }
}
pub fn order_side(s: &str) -> OrderSide {
    if s.eq_ignore_ascii_case("BUY") {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    }
}
pub fn order_type(s: &str) -> OrderType {
    match s.to_ascii_uppercase().as_str() {
        "MARKET" => OrderType::Market,
        "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        _ => OrderType::Limit,
    }
}
pub fn tif(s: &str) -> TimeInForce {
    match s.to_ascii_uppercase().as_str() {
        "IOC" => TimeInForce::ImmediateOrCancel,
        "FOK" => TimeInForce::FillOrKill,
        "POST_ONLY" => TimeInForce::PostOnly,
        _ => TimeInForce::GoodTillCancel,
    }
}
pub fn trigger(s: &str) -> StopTriggerType {
    if s.to_ascii_uppercase().starts_with("LAST") {
        StopTriggerType::LastPrice
    } else {
        StopTriggerType::MarkPrice
    }
}
fn position_key(symbol: &str, side: PositionSide) -> String {
    format!(
        "{symbol}:{}",
        if side == PositionSide::Long {
            "long"
        } else {
            "short"
        }
    )
}

impl BitunixFutures {
    pub(crate) fn private_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.credentials.is_none() {
            cx.store.set_balance(Balance::default());
            cx.store.set_positions(vec![]);
            cx.store.set_orders(vec![]);
            return;
        }
        let settings: Vec<_> = cx
            .store
            .markets()
            .iter()
            .enumerate()
            .map(|(i, _)| {
                (
                    i as u32,
                    MarketSetting {
                        leverage: 10.0,
                        margin_type: Some(MarginType::Cross),
                        ..MarketSetting::default()
                    },
                )
            })
            .collect();
        cx.store.set_settings(settings);
        self.connect(cx, Feed::Private);
        self.read_axis(cx, Axis::Balance);
        self.read_axis(cx, Axis::Positions);
        self.read_axis(cx, Axis::Orders);
        self.account.tick = Some(cx.set_timer(5_000.0));
    }
    pub(crate) fn signed_request(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        params: Params,
        body: Option<String>,
        pending: Pending,
    ) {
        let Some(c) = &self.credentials else {
            self.fail_private(cx, pending, "Bitunix credentials required");
            return;
        };
        let mut headers = match sign::headers(c, &params, body.as_deref().unwrap_or(""), cx.now) {
            Ok(h) => h,
            Err(e) => {
                self.fail_private(cx, pending, &e);
                return;
            }
        };
        headers.extend(self.headers());
        let query = params.query();
        let url = format!(
            "{REST}{path}{}{}",
            if query.is_empty() { "" } else { "?" },
            query
        );
        if matches!(&pending,Pending::Private(r) if r.axis!=Axis::Balance)
            || matches!(&pending, Pending::History(_))
        {
            cx.request_streamed(
                method,
                url,
                headers,
                json::ArrayAt::AnyFieldEnvelope(&[
                    "data",
                    "tradeList",
                    "userTrades",
                    "positionList",
                    "positions",
                    "orderList",
                    "list",
                    "rows",
                ]),
                pending,
            );
        } else {
            cx.request(method, url, headers, body, pending);
        }
    }
    pub(crate) fn read_axis(&mut self, cx: &mut Cx<'_, Pending>, axis: Axis) {
        self.read_axis_for(cx, axis, None);
    }
    pub(crate) fn read_axis_for(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        axis: Axis,
        after: Option<u32>,
    ) {
        if self.credentials.is_none() {
            return;
        }
        let path = match axis {
            Axis::Balance => ACCOUNT,
            Axis::Positions => POSITIONS,
            Axis::Orders => ORDERS,
            Axis::Tpsl => TPSL,
        };
        let params = if axis == Axis::Balance {
            Params::default().str("marginCoin", "USDT")
        } else {
            Params::default()
        };
        let slot = match axis {
            Axis::Balance => 0,
            Axis::Positions => 1,
            Axis::Orders => 2,
            Axis::Tpsl => 3,
        };
        self.account.serials[slot] += 1;
        let read = Read {
            after,
            serial: self.account.serials[slot],
            axis,
            generation: self.account.generations[axis.index()],
            positions: vec![],
            identities: vec![],
            orders: vec![],
            native: vec![],
            invalid: false,
            baselines: vec![],
        };
        self.signed_request(cx, Method::Get, path, params, None, Pending::Private(read));
    }
    pub(crate) fn fail_private(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Candles { command } => cx.done(command, Err(error.into())),
            Pending::Trade(request) => self.trade_failed(cx, request, error),
            Pending::History(read) => self.history_failed(cx, read, error),
            Pending::Private(read) => {
                if let Some(command) = read.after {
                    self.prepared(cx, command, false);
                }
                cx.log(LogLevel::Error, error);
            }
            _ => cx.log(LogLevel::Error, error),
        }
    }
    pub(crate) fn login(&self, cx: &mut Cx<'_, Pending>, socket: safe_cex_engine::SocketId) {
        let Some(c) = &self.credentials else {
            return;
        };
        match safe_cex_crypto::uuid::uuid() {
            Ok(nonce) => {
                let timestamp = safe_cex_engine::num::format(cx.now.floor());
                let trimmed = sign::Credentials {
                    key: c.key.trim().into(),
                    secret: c.secret.trim().into(),
                };
                let sign = sign::signature(&trimmed, &nonce, &timestamp, "", "");
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Login<'a> {
                    api_key: &'a str,
                    timestamp: &'a str,
                    nonce: &'a str,
                    sign: &'a str,
                }
                let args = [Login {
                    api_key: c.key.trim(),
                    timestamp: &timestamp,
                    nonce: &nonce,
                    sign: &sign,
                }];
                cx.send(
                    socket,
                    format!(
                        r#"{{"op":"login","args":{}}}"#,
                        serde_json::to_string(&args).unwrap()
                    ),
                );
            }
            Err(e) => cx.log(LogLevel::Error, e),
        }
    }
    pub(crate) fn debounce(&mut self, cx: &mut Cx<'_, Pending>, axis: Axis) {
        let i = axis.index();
        if let Some(t) = self.account.timers[i].take() {
            cx.clear_timer(t);
        }
        self.account.timers[i] = Some(cx.set_timer(1000.0));
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.login_retry == Some(timer) {
            self.account.login_retry = None;
            if let Some(s) = self
                .sockets
                .iter()
                .find(|s| matches!(s.feed, Feed::Private) && s.opened)
            {
                self.login(cx, s.id);
            }
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account
                .terminal
                .retain(|_, at| cx.now - *at < 300_000.0);
            self.account.filled.retain(|id, _| {
                self.account.terminal.contains_key(id) || cx.store.order(id).is_some()
            });
            self.read_axis(cx, Axis::Balance);
            self.account.count += 1;
            if self.account.count.is_multiple_of(32) {
                for a in [Axis::Positions, Axis::Orders, Axis::Tpsl] {
                    self.read_axis(cx, a);
                }
            }
            self.account.tick = Some(cx.set_timer(5_000.0));
            return true;
        }
        for axis in [Axis::Balance, Axis::Positions, Axis::Orders] {
            let i = axis.index();
            if self.account.timers[i] == Some(timer) {
                self.account.timers[i] = None;
                self.read_axis(cx, axis);
                if axis == Axis::Orders {
                    self.read_axis(cx, Axis::Tpsl);
                }
                return true;
            }
        }
        false
    }
    pub(crate) fn account_item(&mut self, cx: &mut Cx<'_, Pending>, read: &mut Read, item: &str) {
        let result = match read.axis {
            Axis::Positions => serde_json::from_str::<PositionRow<'_>>(item).map(|p| {
                if p.symbol.is_empty() {
                    read.invalid = true;
                    return;
                }
                let qty = p.quantity();
                let side = if p.side.eq_ignore_ascii_case("BUY")
                    || p.side.eq_ignore_ascii_case("LONG")
                    || (p.side.eq_ignore_ascii_case("BOTH") && qty >= 0.0)
                {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                };
                let mode = if p.position_mode.eq_ignore_ascii_case("ONE_WAY") {
                    PositionMode::Oneway
                } else {
                    PositionMode::Hedged
                };
                let id = if mode == PositionMode::Oneway {
                    p.symbol.to_string()
                } else {
                    position_key(&p.symbol, side)
                };
                let setting = cx.store.setting(&p.symbol);
                let margin = match p.margin_mode.to_ascii_uppercase().as_str() {
                    "ISOLATION" | "ISOLATED" => MarginType::Isolated,
                    "CROSS" => MarginType::Cross,
                    _ => setting
                        .and_then(|s| s.margin_type)
                        .unwrap_or(MarginType::Cross),
                };
                let key = position_key(&p.symbol, side);
                if !p.position_id.is_empty() {
                    read.identities.push((key, p.position_id.to_string()));
                }
                if qty != 0.0 {
                    read.positions.push(Position {
                        id,
                        symbol: p.symbol.to_string(),
                        side,
                        entry_price: p.entry(),
                        notional: p.notional(),
                        leverage: if p.leverage.0 != 0.0 {
                            p.leverage.0
                        } else {
                            setting.map_or(10.0, |s| s.leverage)
                        },
                        margin_type: Some(margin),
                        position_mode: Some(mode),
                        unrealized_pnl: p.unrealized_pnl.0,
                        // Reported net of fees AND funding. Never subtract either again.
                        realized_pnl: Some(p.realized_pnl.0),
                        contracts: qty.abs(),
                        liquidation_price: p.liq_price.0,
                        margin_size: None,
                        last_update: None,
                    });
                }
                let _ = (p.fee, p.funding, &p.event);
            }),
            Axis::Orders | Axis::Tpsl => serde_json::from_str::<OrderRow<'_>>(item).map(|o| {
                if o.order_id.is_empty() || o.symbol.is_empty() {
                    read.invalid = true;
                    return;
                }
                if self.account.terminal.contains_key(o.order_id.as_ref()) {
                    return;
                }
                if read.axis == Axis::Orders {
                    read.baselines.push((
                        o.order_id.to_string(),
                        (
                            o.trade_qty.0,
                            o.trade_qty.0
                                * if o.average_price.0 > 0.0 {
                                    o.average_price.0
                                } else {
                                    o.price.0
                                },
                        ),
                    ));
                }
                let (orders, native) = self.map_orders(cx, &o, read.axis == Axis::Tpsl, false);
                read.orders.extend(orders);
                read.native.extend(native);
            }),
            Axis::Balance => return,
        };
        if result.is_err() {
            read.invalid = true;
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Response<'_>,
    ) {
        let body = match data(response.body) {
            Ok(b) if (200..300).contains(&response.status) && !read.invalid => b,
            _ => {
                cx.log(
                    LogLevel::Warning,
                    "Bitunix account read failed; keeping last snapshot",
                );
                self.debounce(cx, read.axis);
                return;
            }
        };
        let slot = match read.axis {
            Axis::Balance => 0,
            Axis::Positions => 1,
            Axis::Orders => 2,
            Axis::Tpsl => 3,
        };
        if read.serial != self.account.serials[slot] {
            return;
        }
        if read.generation != self.account.generations[read.axis.index()] {
            self.debounce(cx, read.axis);
            return;
        }
        if !cx.store.loaded().orders && matches!(read.axis, Axis::Orders | Axis::Tpsl) {
            let slot = usize::from(read.axis == Axis::Tpsl);
            self.account.initial_orders[slot] = Some(read);
            if self.account.initial_orders.iter().all(Option::is_some) {
                for slot in 0..2 {
                    if self.account.initial_orders[slot]
                        .as_ref()
                        .is_some_and(|r| r.generation != self.account.generations[2])
                    {
                        self.account.initial_orders[slot] = None;
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                }
                let regular = self.account.initial_orders[0].take().unwrap();
                let tpsl = self.account.initial_orders[1].take().unwrap();
                self.commit_orders(cx, regular);
                self.commit_orders(cx, tpsl);
            }
            return;
        }
        match read.axis {
            Axis::Balance => {
                let result =
                    extract_account(body, 0).ok_or_else(|| "missing Bitunix account".to_string());
                match result {
                    Ok(a) => {
                        let previous = cx.store.hedged();
                        let hedged = !a.position_mode.eq_ignore_ascii_case("ONE_WAY");
                        cx.store.set_hedged(hedged);
                        cx.store.set_balance(Balance {
                            free: a.available.unwrap_or_default().0,
                            used: a.margin.unwrap_or_default().0,
                            total: a.available.unwrap_or_default().0
                                + a.margin.unwrap_or_default().0
                                + a.frozen.unwrap_or_default().0,
                            upnl: if a.cross_unrealized_pnl.is_some()
                                || a.isolation_unrealized_pnl.is_some()
                            {
                                a.cross_unrealized_pnl.unwrap_or_default().0
                                    + a.isolation_unrealized_pnl.unwrap_or_default().0
                            } else {
                                a.unrealized_pnl.unwrap_or_default().0
                            },
                        });
                        if previous != hedged {
                            self.debounce(cx, Axis::Positions);
                            self.debounce(cx, Axis::Orders);
                        }
                    }
                    Err(e) => cx.log(LogLevel::Warning, e.to_string()),
                }
            }
            Axis::Positions => {
                for p in &read.positions {
                    cx.store.update_setting(&p.symbol, |s| {
                        s.leverage = p.leverage;
                        s.margin_type = p.margin_type;
                    });
                }
                self.account.position_ids = read.identities.into_iter().collect();
                cx.store.set_positions(read.positions);
                self.read_axis(cx, Axis::Tpsl);
            }
            Axis::Orders | Axis::Tpsl => {
                self.commit_orders(cx, read);
            }
        }
    }
    fn commit_orders(&mut self, cx: &mut Cx<'_, Pending>, mut read: Read) {
        let tpsl = read.axis == Axis::Tpsl;
        let optimistic: std::collections::HashSet<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| matches!(o.status, OrderStatus::Creating | OrderStatus::Canceling))
            .map(|o| o.id.clone())
            .collect();
        read.orders.retain(|o| !optimistic.contains(&o.id));
        read.native.retain(|(id, _)| !optimistic.contains(id));
        let mut orders: Vec<_> = cx
            .store
            .orders()
            .iter()
            .filter(|o| {
                optimistic.contains(&o.id)
                    || self.account.native.get(&o.id).is_some_and(|n| {
                        !n.attached
                            && (o.kind == OrderType::StopLoss || o.kind == OrderType::TakeProfit)
                    }) != tpsl
            })
            .cloned()
            .collect();
        let retained: std::collections::HashSet<_> = orders.iter().map(|o| o.id.clone()).collect();
        self.account.native.retain(|id, _| retained.contains(id));
        for (id, baseline) in read.baselines {
            let previous = self.account.filled.entry(id).or_insert(baseline);
            if baseline.0 > previous.0 {
                *previous = baseline;
            }
        }
        orders.extend(read.orders);
        self.account.native.extend(read.native);
        cx.store.set_orders(orders);
    }
    pub(crate) fn map_orders(
        &self,
        cx: &Cx<'_, Pending>,
        o: &OrderRow<'_>,
        tpsl: bool,
        ws: bool,
    ) -> (Vec<Order>, Vec<(String, NativeOrder)>) {
        let raw = o.order_id.to_string();
        let reduce = o
            .reduce_only
            .as_ref()
            .and_then(Supported::boolean)
            .unwrap_or_else(|| {
                self.account
                    .reduces
                    .get(o.client_id.as_ref())
                    .copied()
                    .unwrap_or(false)
            });
        let side = if tpsl {
            cx.store
                .positions()
                .iter()
                .find(|p| {
                    self.account
                        .position_ids
                        .get(&position_key(&p.symbol, p.side))
                        .is_some_and(|id| id == o.position_id.as_ref())
                })
                .map_or_else(
                    || order_side(&o.side),
                    |p| {
                        if p.side == PositionSide::Long {
                            OrderSide::Sell
                        } else {
                            OrderSide::Buy
                        }
                    },
                )
        } else {
            order_side(&o.side)
        };
        let kind = order_type(if o.order_type.is_empty() {
            &o.kind
        } else {
            &o.order_type
        });
        let amount = o.qty.0;
        let filled = if ws {
            o.deal_amount.unwrap_or_default().0
        } else {
            o.trade_qty.0
        };
        let native = NativeOrder {
            raw_id: raw.clone(),
            position_id: o.position_id.to_string(),
            full_position: o.kind.eq_ignore_ascii_case("POSITION_TPSL"),
            attached: false,
            tp: o.tp_price.map(|n| n.0),
            sl: o.sl_price.map(|n| n.0),
            tp_stop: Some(trigger(&o.tp_stop_type)),
            sl_stop: Some(trigger(&o.sl_stop_type)),
            tp_execution: o.tp_order_type.to_ascii_uppercase(),
            sl_execution: o.sl_order_type.to_ascii_uppercase(),
            tp_limit: o.tp_order_price.0,
            sl_limit: o.sl_order_price.0,
        };
        let base = Order {
            id: raw.clone(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: o.symbol.to_string(),
            kind,
            side,
            price: o.price.0,
            amount,
            filled,
            remaining: amount - filled,
            reduce_only: reduce,
            close: None,
            client_order_id: if o.client_id.is_empty() {
                None
            } else {
                Some(o.client_id.to_string())
            },
            time_in_force: Some(
                self.account
                    .tifs
                    .get(o.client_id.as_ref())
                    .copied()
                    .unwrap_or_else(|| tif(&o.effect)),
            ),
            last_update: if time_ms(&o.mtime) != 0.0 {
                time_ms(&o.mtime)
            } else {
                cx.now
            },
            trigger_type: Some(StopTriggerType::MarkPrice),
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        let mut orders = vec![];
        let mut natives = vec![];
        if !tpsl {
            orders.push(base.clone());
            natives.push((raw.clone(), native.clone()));
        }
        for (price, qty, kind, leg, stop) in [
            (
                native.tp,
                o.tp_qty.0,
                OrderType::TakeProfit,
                "tp",
                native.tp_stop,
            ),
            (
                native.sl,
                o.sl_qty.0,
                OrderType::StopLoss,
                "sl",
                native.sl_stop,
            ),
        ] {
            let Some(price) = price else {
                continue;
            };
            let both = native.tp.is_some() && native.sl.is_some();
            let id = if !tpsl {
                format!("{raw}__{leg}")
            } else if both {
                format!(
                    "{raw}__{}__{leg}",
                    if o.position_id.is_empty() {
                        "position"
                    } else {
                        &o.position_id
                    }
                )
            } else {
                raw.clone()
            };
            let mut row = base.clone();
            row.id = id.clone();
            row.kind = kind;
            row.price = price;
            row.amount = if tpsl { qty } else { amount };
            row.filled = 0.0;
            row.remaining = row.amount;
            row.side = if tpsl { side } else { side.opposite() };
            row.reduce_only = if tpsl { !ws && reduce || ws } else { true };
            row.trigger_type = stop;
            row.time_in_force = if tpsl {
                None
            } else {
                Some(TimeInForce::GoodTillCancel)
            };
            let mut n = native.clone();
            n.attached = !tpsl;
            orders.push(row);
            natives.push((id, n));
        }
        (orders, natives)
    }
    pub(crate) fn private_message(&mut self, cx: &mut Cx<'_, Pending>, m: &Message<'_>) {
        let Some(raw) = m.data else {
            return;
        };
        let text = raw.get();
        match if m.op.is_empty() {
            m.ch.as_ref()
        } else {
            m.op.as_ref()
        } {
            "login" => {
                #[derive(serde::Deserialize)]
                struct Login {
                    result: bool,
                }
                if !serde_json::from_str::<Login>(text).is_ok_and(|l| l.result) {
                    cx.log(
                        LogLevel::Error,
                        "Bitunix private ws login failed, double check your key and secret",
                    );
                    if let Some(t) = self.account.login_retry.take() {
                        cx.clear_timer(t);
                    }
                    self.account.login_retry = Some(cx.set_timer(2000.0));
                }
            }
            "balance" => {
                self.account.generations[0] += 1;
                self.debounce(cx, Axis::Balance);
            }
            "position" => {
                if let Ok(p) = serde_json::from_str::<PositionRow<'_>>(text) {
                    let side = if p.side.eq_ignore_ascii_case("BUY")
                        || p.side.eq_ignore_ascii_case("LONG")
                    {
                        PositionSide::Long
                    } else {
                        PositionSide::Short
                    };
                    let key = position_key(&p.symbol, side);
                    if p.event == "CLOSE" {
                        self.account.ws_position_ids.remove(&key);
                    } else if !p.position_id.is_empty() {
                        self.account
                            .ws_position_ids
                            .insert(key, p.position_id.to_string());
                    }
                }
                self.account.generations[1] += 1;
                self.debounce(cx, Axis::Positions);
            }
            "order" | "tpsl" => {
                let tpsl = m.ch == "tpsl";
                if let Ok(o) = serde_json::from_str::<OrderRow<'_>>(text) {
                    let raw = o.order_id.as_ref();
                    if raw.is_empty() || o.symbol.is_empty() {
                        cx.log(LogLevel::Warning, "Bitunix order event missing identity");
                        return;
                    }
                    let (filled, updated, closed) = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|row| {
                            row.id == raw
                                || self
                                    .account
                                    .native
                                    .get(&row.id)
                                    .is_some_and(|meta| meta.raw_id == raw)
                        })
                        .fold(
                            (
                                self.account.filled.get(raw).map_or(0.0, |v| v.0),
                                0.0_f64,
                                false,
                            ),
                            |(filled, updated, closed), row| {
                                (
                                    filled.max(row.filled),
                                    updated.max(row.last_update),
                                    closed || row.status == OrderStatus::Closed,
                                )
                            },
                        );
                    let timestamp = time_ms(&o.mtime);
                    let stale_timestamp = timestamp > 0.0 && timestamp < updated;
                    let cumulative = o.deal_amount.map(|value| value.0);
                    let advancing_execution = !tpsl
                        && matches!(o.event.as_ref(), "CLOSE" | "UPDATE")
                        && cumulative.is_some_and(|value| value > filled);
                    // Fence stale events before touching raw protection metadata too.
                    // A late terminal execution can still advance a canceled order.
                    if (!tpsl
                        && cumulative
                            .or_else(|| (o.event == "CREATE").then_some(0.0))
                            .is_some_and(|value| value < filled))
                        || (stale_timestamp && !advancing_execution)
                        || (closed && o.event == "CLOSE" && o.order_status != "FILLED")
                    {
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                    if self.account.terminal.contains_key(raw)
                        && o.event != "CLOSE"
                        && !(o.event == "UPDATE"
                            && !tpsl
                            && cumulative.is_some_and(|value| value > filled))
                    {
                        self.debounce(cx, Axis::Orders);
                        return;
                    }
                    if !stale_timestamp {
                        for n in self.account.native.values_mut().filter(|n| n.raw_id == raw) {
                            if let Some(p) = o.tp_price {
                                n.tp = Some(p.0);
                            }
                            if let Some(p) = o.sl_price {
                                n.sl = Some(p.0);
                            }
                            if !o.tp_stop_type.is_empty() {
                                n.tp_stop = Some(trigger(&o.tp_stop_type));
                            }
                            if !o.sl_stop_type.is_empty() {
                                n.sl_stop = Some(trigger(&o.sl_stop_type));
                            }
                            if !o.tp_order_type.is_empty() {
                                n.tp_execution = o.tp_order_type.to_ascii_uppercase();
                            }
                            if !o.sl_order_type.is_empty() {
                                n.sl_execution = o.sl_order_type.to_ascii_uppercase();
                            }
                            if o.tp_order_price.0 != 0.0 {
                                n.tp_limit = o.tp_order_price.0;
                            }
                            if o.sl_order_price.0 != 0.0 {
                                n.sl_limit = o.sl_order_price.0;
                            }
                        }
                    }
                    if o.event == "CREATE" && !self.account.terminal.contains_key(raw) {
                        let (rows, native) = self.map_orders(cx, &o, tpsl, true);
                        if !o.client_id.is_empty() && !o.effect.is_empty() {
                            self.account
                                .tifs
                                .insert(o.client_id.to_string(), tif(&o.effect));
                        }
                        for row in rows {
                            cx.store.upsert_order(row);
                        }
                        self.account.native.extend(native);
                    } else if o.event != "CREATE" {
                        if o.event == "CLOSE" {
                            self.account.terminal.insert(raw.to_owned(), cx.now);
                        }
                        let ids: Vec<_> = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|r| {
                                r.id == raw
                                    || tpsl
                                        && self
                                            .account
                                            .native
                                            .get(&r.id)
                                            .is_some_and(|n| n.raw_id == raw)
                            })
                            .map(|r| r.id.clone())
                            .collect();
                        for id in ids {
                            cx.store.update_order(&id, |r| {
                                let timestamp = time_ms(&o.mtime);
                                if timestamp > r.last_update {
                                    r.last_update = timestamp;
                                }
                                if o.event == "CLOSE" {
                                    if o.order_status == "FILLED" {
                                        r.status = OrderStatus::Closed;
                                    } else if r.status != OrderStatus::Closed {
                                        r.status = OrderStatus::Canceled;
                                    }
                                    if !tpsl {
                                        r.remaining = 0.0;
                                    }
                                }
                                if tpsl {
                                    let qty = if r.kind == OrderType::TakeProfit {
                                        o.tp_qty.0
                                    } else {
                                        o.sl_qty.0
                                    };
                                    if qty > 0.0 {
                                        r.amount = qty;
                                        r.remaining = qty;
                                    }
                                    let stop = if r.kind == OrderType::TakeProfit {
                                        &o.tp_stop_type
                                    } else {
                                        &o.sl_stop_type
                                    };
                                    if !stop.is_empty() {
                                        r.trigger_type = Some(trigger(stop));
                                    }
                                    if let Some(p) = if r.kind == OrderType::TakeProfit {
                                        o.tp_price
                                    } else {
                                        o.sl_price
                                    } {
                                        r.price = p.0;
                                    }
                                } else {
                                    if !stale_timestamp && o.price.0 != 0.0 {
                                        r.price = o.price.0;
                                    }
                                    if !stale_timestamp && o.qty.0 != 0.0 {
                                        r.amount = o.qty.0;
                                    }
                                    if let Some(cumulative) = cumulative {
                                        r.filled = r.filled.max(cumulative);
                                    }
                                    if o.event != "CLOSE"
                                        && !matches!(
                                            r.status,
                                            OrderStatus::Closed | OrderStatus::Canceled
                                        )
                                    {
                                        r.remaining = r.amount - r.filled;
                                    }
                                }
                            });
                        }
                    }
                    if !tpsl {
                        let previous = self
                            .account
                            .filled
                            .entry(o.order_id.to_string())
                            .or_insert((0.0, 0.0));
                        let cumulative = cumulative.unwrap_or(previous.0);
                        let delta = cumulative - previous.0;
                        if delta > 0.0 {
                            let cost = if o.average_price.0 > 0.0 {
                                o.average_price.0 * cumulative
                            } else {
                                previous.1 + delta * o.price.0
                            };
                            cx.fill(Fill {
                                order_id: o.order_id.to_string(),
                                symbol: o.symbol.to_string(),
                                side: order_side(&o.side),
                                price: (cost - previous.1) / delta,
                                amount: delta,
                            });
                            *previous = (cumulative, cost);
                        }
                    }
                }
                self.account.generations[2] += 1;
                self.debounce(cx, Axis::Orders);
            }
            _ => {}
        }
    }
}

#[derive(serde::Deserialize)]
struct AccountWrapper<'a> {
    #[serde(borrow, default)]
    data: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    result: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    payload: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    list: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    rows: Option<&'a serde_json::value::RawValue>,
}
fn extract_account(text: &str, depth: u8) -> Option<Account<'_>> {
    if depth > 4 {
        return None;
    }
    if text.trim_start().starts_with('[') {
        let rows: Vec<&serde_json::value::RawValue> = serde_json::from_str(text).ok()?;
        let mut first = None;
        for row in rows {
            if let Some(a) = extract_account(row.get(), depth + 1) {
                if a.margin_coin.eq_ignore_ascii_case("USDT") {
                    return Some(a);
                }
                if first.is_none() {
                    first = Some(a);
                }
            }
        }
        return first;
    }
    if let Ok(a) = serde_json::from_str::<Account<'_>>(text)
        && (!a.margin_coin.is_empty()
            || !a.position_mode.is_empty()
            || a.available.is_some()
            || a.margin.is_some()
            || a.frozen.is_some()
            || a.cross_unrealized_pnl.is_some()
            || a.isolation_unrealized_pnl.is_some())
    {
        return Some(a);
    }
    let w: AccountWrapper<'_> = serde_json::from_str(text).ok()?;
    [w.data, w.result, w.payload, w.list, w.rows]
        .into_iter()
        .flatten()
        .find_map(|v| extract_account(v.get(), depth + 1))
}
