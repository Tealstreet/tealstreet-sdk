//! Typed borrowed private records shared by streaming, trading and queries.
use crate::wire::{Str, number, optional_number, timestamp};
use safe_cex_engine::model::*;
use safe_cex_engine::store::{MarketSetting, Store};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;

pub fn id<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id<'a> {
        Text(#[serde(borrow)] Str<'a>),
        Integer(u64),
        Signed(i64),
    }
    Ok(match Id::deserialize(d)? {
        Id::Text(s) => s,
        Id::Integer(n) => Cow::Owned(n.to_string()),
        Id::Signed(n) => Cow::Owned(n.to_string()),
    })
}
pub fn margin(value: &str) -> MarginType {
    if value == "ISOLATED" {
        MarginType::Isolated
    } else {
        MarginType::Cross
    }
}
pub fn mode(value: &str) -> PositionMode {
    if value == "SEPARATED" {
        PositionMode::Hedged
    } else {
        PositionMode::Oneway
    }
}
pub fn side(value: &str) -> Option<OrderSide> {
    match value {
        "BUY" => Some(OrderSide::Buy),
        "SELL" => Some(OrderSide::Sell),
        _ => None,
    }
}
pub fn status(value: &str) -> OrderStatus {
    match value {
        "FILLED" | "COMPLETED" => OrderStatus::Closed,
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
        "CANCELING" => OrderStatus::Canceling,
        _ => OrderStatus::Open,
    }
}
fn trigger(value: &str) -> StopTriggerType {
    if value == "MARK_PRICE" {
        StopTriggerType::MarkPrice
    } else if value == "INDEX_PRICE" {
        StopTriggerType::IndexPrice
    } else {
        StopTriggerType::LastPrice
    }
}
fn bracket_trigger(value: &str) -> Option<StopTriggerType> {
    matches!(
        value,
        "CONTRACT_PRICE" | "LAST_PRICE" | "MARK_PRICE" | "INDEX_PRICE"
    )
    .then(|| trigger(value))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(
        borrow,
        default,
        deserialize_with = "id",
        alias = "id",
        alias = "algoId"
    )]
    pub order_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, alias = "orderSide")]
    pub side: Str<'a>,
    #[serde(borrow, default, rename = "type", alias = "orderType")]
    pub kind: Str<'a>,
    #[serde(borrow, default)]
    pub plan_type: Str<'a>,
    #[serde(borrow, default)]
    pub algo_type: Str<'a>,
    #[serde(borrow, default, alias = "algoStatus")]
    pub status: Str<'a>,
    #[serde(
        deserialize_with = "number",
        alias = "quantity",
        alias = "size",
        alias = "qty"
    )]
    pub orig_qty: f64,
    #[serde(
        default,
        deserialize_with = "number",
        alias = "cumFillSize",
        alias = "fillSize"
    )]
    pub executed_qty: f64,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    #[serde(default, deserialize_with = "number")]
    pub actual_price: f64,
    #[serde(default, deserialize_with = "number", alias = "triggerPrice")]
    pub stop_price: f64,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default)]
    pub position_tpsl: bool,
    #[serde(default)]
    pub close_position: Option<bool>,
    #[serde(borrow, default, alias = "clientAlgoId")]
    pub client_order_id: Str<'a>,
    #[serde(borrow, default)]
    pub time_in_force: Str<'a>,
    #[serde(default, deserialize_with = "number", alias = "updatedTime")]
    pub update_time: f64,
    #[serde(
        default,
        deserialize_with = "number",
        alias = "createdTime",
        alias = "time"
    )]
    pub create_time: f64,
    #[serde(borrow, default, alias = "triggerPriceType")]
    pub working_type: Str<'a>,
    #[serde(borrow, default)]
    pub tp_order_type: Str<'a>,
    #[serde(borrow, default, rename = "TpWorkingType")]
    pub tp_working_type: Str<'a>,
    #[serde(borrow, default, rename = "SlWorkingType")]
    pub sl_working_type: Str<'a>,
    #[serde(default, deserialize_with = "optional_number")]
    pub tp_trigger_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub sl_trigger_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub tp_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub sl_price: Option<f64>,
}
impl OrderRow<'_> {
    /// Some algo rows carry only `planType`; TS falls back to it as well.
    fn order_type(&self) -> &str {
        if self.kind.is_empty() {
            &self.plan_type
        } else {
            &self.kind
        }
    }
    fn protective(&self) -> bool {
        self.reduce_only
            || self.position_tpsl
            || matches!(self.plan_type.as_ref(), "STOP_LOSS" | "TAKE_PROFIT")
            || matches!(self.algo_type.as_ref(), "TPSL" | "TP_SL")
    }
    pub fn normal(&self) -> Option<Order> {
        if self.order_id.is_empty()
            || self.symbol.is_empty()
            || self.order_type().is_empty()
            || self.status.is_empty()
            || self.orig_qty < 0.0
            || self.executed_qty < 0.0
        {
            return None;
        }
        let side = side(&self.side)?;
        let kind = match self.order_type() {
            "MARKET" => OrderType::Market,
            "CONDITIONAL" => OrderType::Conditional,
            "STOP" | "STOP_MARKET" | "STOP_LOSS" => OrderType::StopLoss,
            "TAKE_PROFIT" | "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
            "TRAILING_STOP_MARKET" | "TRAILING_MARKET" => OrderType::TrailingStopLoss,
            _ => OrderType::Limit,
        };
        let kind = if !self.protective()
            && matches!(
                self.order_type(),
                "STOP" | "STOP_MARKET" | "TAKE_PROFIT" | "TAKE_PROFIT_MARKET"
            ) {
            OrderType::Conditional
        } else {
            kind
        };
        let amount = self.orig_qty.abs();
        let filled = self.executed_qty.abs();
        Some(Order {
            id: self.order_id.to_string(),
            parent_id: None,
            status: status(&self.status),
            symbol: self.symbol.to_string(),
            kind,
            side,
            price: if kind == OrderType::Conditional && self.stop_price > 0.0 {
                self.stop_price
            } else if self.price != 0.0 {
                self.price
            } else if self.actual_price != 0.0 {
                self.actual_price
            } else {
                self.stop_price
            },
            amount,
            filled,
            remaining: (amount - filled).max(0.0),
            reduce_only: self.reduce_only,
            close: self.close_position,
            client_order_id: (!self.client_order_id.is_empty())
                .then(|| self.client_order_id.to_string()),
            time_in_force: Some(match self.time_in_force.as_ref() {
                "IOC" => TimeInForce::ImmediateOrCancel,
                "FOK" => TimeInForce::FillOrKill,
                "POST_ONLY" => TimeInForce::PostOnly,
                _ => TimeInForce::GoodTillCancel,
            }),
            last_update: timestamp(if self.update_time > 0.0 {
                self.update_time
            } else {
                self.create_time
            }),
            // Plain limit/market rows carry a placeholder trigger type (CONTRACT_PRICE, price 0).
            trigger_type: (!matches!(kind, OrderType::Limit | OrderType::Market)
                && !self.working_type.is_empty())
            .then(|| trigger(&self.working_type)),
            take_profit: self.tp_trigger_price.filter(|n| *n > 0.0),
            stop_loss: self.sl_trigger_price.filter(|n| *n > 0.0),
            extra: if kind == OrderType::Conditional {
                Some(OrderExtra {
                    conditional_order_price: Some(if self.price != 0.0 {
                        self.price
                    } else {
                        self.actual_price
                    }),
                    ..OrderExtra::default()
                })
            } else {
                // Regular-order bracket legs keep their venue trigger types
                // (TS: tpOrderType ?? TpWorkingType, workingType ?? SlWorkingType).
                let tp = bracket_trigger(&self.tp_order_type)
                    .or_else(|| bracket_trigger(&self.tp_working_type))
                    .filter(|_| self.tp_trigger_price.is_some_and(|n| n > 0.0));
                let sl = bracket_trigger(&self.working_type)
                    .or_else(|| bracket_trigger(&self.sl_working_type))
                    .filter(|_| self.sl_trigger_price.is_some_and(|n| n > 0.0));
                (matches!(kind, OrderType::Limit | OrderType::Market)
                    && (tp.is_some() || sl.is_some()))
                .then_some(OrderExtra {
                    tp_trigger_type: tp,
                    sl_trigger_type: sl,
                    ..OrderExtra::default()
                })
            },
        })
    }
    pub fn algo(&self) -> Vec<Order> {
        let Some(mut order) = self.normal() else {
            return vec![];
        };
        let protective = self.protective();
        if protective && self.plan_type == "STOP_LOSS" {
            order.kind = OrderType::StopLoss;
        } else if protective && self.plan_type == "TAKE_PROFIT" {
            order.kind = OrderType::TakeProfit;
        } else if !protective
            && matches!(
                self.order_type(),
                "STOP" | "STOP_MARKET" | "TAKE_PROFIT" | "TAKE_PROFIT_MARKET"
            )
        {
            order.kind = OrderType::Conditional;
        }
        // Algo rows always trigger, even when their order type reads LIMIT/MARKET.
        order.trigger_type = (!self.working_type.is_empty()).then(|| trigger(&self.working_type));
        order.extra = Some(OrderExtra {
            conditional_order_price: Some(if self.price != 0.0 {
                self.price
            } else {
                self.actual_price
            }),
            ..OrderExtra::default()
        });
        // Deliberate TS correction: conditional display/amend price is its
        // trigger; preserve execution separately for amount-only replacement.
        if order.kind == OrderType::Conditional && self.stop_price > 0.0 {
            order.price = self.stop_price;
        }
        let mut orders = vec![order.clone()];
        for (suffix, price, execution, working, kind) in [
            (
                "tp",
                self.tp_trigger_price,
                self.tp_price,
                self.tp_order_type.as_ref(),
                OrderType::TakeProfit,
            ),
            (
                "sl",
                self.sl_trigger_price,
                self.sl_price,
                self.working_type.as_ref(),
                OrderType::StopLoss,
            ),
        ] {
            if let Some(price) = price.filter(|n| *n > 0.0) {
                let mut child = order.clone();
                child.id = format!("{}__{suffix}", order.id);
                child.parent_id = Some(order.id.clone());
                child.price = price;
                child.kind = kind;
                child.reduce_only = true;
                child.filled = 0.0;
                child.remaining = child.amount;
                child.trigger_type = matches!(
                    working,
                    "CONTRACT_PRICE" | "LAST_PRICE" | "MARK_PRICE" | "INDEX_PRICE"
                )
                .then(|| trigger(working));
                child.extra = Some(OrderExtra {
                    remaining_notional: execution.filter(|n| *n != 0.0),
                    ..OrderExtra::default()
                });
                orders.push(child);
            }
        }
        orders
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, alias = "positionSide")]
    pub side: Str<'a>,
    #[serde(borrow, default, alias = "marginMode")]
    pub margin_type: Str<'a>,
    #[serde(borrow, default, alias = "separatedType")]
    pub separated_mode: Str<'a>,
    #[serde(deserialize_with = "number", alias = "positionAmt")]
    pub size: f64,
    #[serde(default, deserialize_with = "number", alias = "notional")]
    pub open_value: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub leverage: Option<f64>,
    #[serde(
        default,
        deserialize_with = "number",
        alias = "unRealizedProfit",
        alias = "unrealizedPnl"
    )]
    pub unrealize_pnl: f64,
    #[serde(default, deserialize_with = "number", alias = "realizePnl")]
    pub realized_pnl: f64,
    #[serde(default, deserialize_with = "number", alias = "liquidationPrice")]
    pub liquidate_price: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub margin_size: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub isolated_margin: Option<f64>,
    #[serde(default, deserialize_with = "number", alias = "updateTime")]
    pub updated_time: f64,
}
impl PositionRow<'_> {
    pub fn position(&self, store: &Store) -> Option<Position> {
        if self.symbol.is_empty() || !matches!(self.side.as_ref(), "LONG" | "SHORT") {
            return None;
        }
        let side = if self.side == "SHORT" {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        let setting = store.setting(&self.symbol);
        let mode = if self.separated_mode.is_empty() {
            if store.hedged() {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            }
        } else {
            mode(&self.separated_mode)
        };
        let margin = if self.margin_type.is_empty() {
            setting
                .and_then(|s| s.margin_type)
                .unwrap_or(MarginType::Cross)
        } else {
            margin(&self.margin_type)
        };
        let contracts = self.size.abs();
        Some(Position {
            id: if mode == PositionMode::Oneway {
                self.symbol.to_string()
            } else {
                format!(
                    "{}:{}",
                    self.symbol,
                    if side == PositionSide::Long {
                        "long"
                    } else {
                        "short"
                    }
                )
            },
            symbol: self.symbol.to_string(),
            side,
            entry_price: if contracts > 0.0 {
                self.open_value.abs() / contracts
            } else {
                0.0
            },
            notional: self.open_value.abs(),
            leverage: self.leverage.or(setting.map(|s| s.leverage)).unwrap_or(1.0),
            margin_type: Some(margin),
            position_mode: Some(mode),
            unrealized_pnl: self.unrealize_pnl,
            realized_pnl: Some(self.realized_pnl),
            contracts,
            liquidation_price: self.liquidate_price,
            margin_size: Some(self.margin_size.or(self.isolated_margin).unwrap_or(0.0)),
            last_update: Some(timestamp(self.updated_time)),
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub margin_type: Str<'a>,
    #[serde(borrow)]
    pub separated_type: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub cross_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub isolated_long_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub isolated_short_leverage: f64,
}
impl ConfigRow<'_> {
    pub fn setting(&self) -> MarketSetting {
        MarketSetting {
            leverage: if self.margin_type == "ISOLATED" {
                self.isolated_long_leverage
            } else {
                self.cross_leverage
            },
            buy_leverage: Some(if self.margin_type == "ISOLATED" {
                self.isolated_long_leverage
            } else {
                self.cross_leverage
            }),
            sell_leverage: Some(if self.margin_type == "ISOLATED" {
                self.isolated_short_leverage
            } else {
                self.cross_leverage
            }),
            margin_type: Some(margin(&self.margin_type)),
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow<'a> {
    #[serde(borrow, alias = "coin")]
    pub asset: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub balance: f64,
    #[serde(deserialize_with = "number")]
    pub available_balance: f64,
    #[serde(default, deserialize_with = "number")]
    pub frozen: f64,
    #[serde(default, deserialize_with = "number")]
    pub unrealize_pnl: f64,
}
impl BalanceRow<'_> {
    pub fn balance(&self) -> Balance {
        Balance {
            total: self.balance,
            free: self.available_balance,
            used: if self.frozen != 0.0 {
                self.frozen
            } else {
                (self.balance - self.available_balance).max(0.0)
            },
            upnl: self.unrealize_pnl,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfig {
    pub dual_side_position: bool,
    pub can_trade: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRow<'a> {
    #[serde(borrow, deserialize_with = "id")]
    pub id: Str<'a>,
    #[serde(borrow, deserialize_with = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub order_side: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub fill_size: f64,
    #[serde(deserialize_with = "number")]
    pub fill_value: f64,
    #[serde(default, deserialize_with = "optional_fill_number")]
    pub fill_fee: Option<f64>,
    #[serde(default, deserialize_with = "optional_fill_number")]
    pub realize_pnl: Option<f64>,
}

// Optional financial metadata must not suppress an otherwise valid execution.
fn optional_fill_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
        Other(serde::de::IgnoredAny),
    }
    let value = match N::deserialize(d)? {
        N::Bare(n) => Some(n),
        N::Text(s) => s.parse().ok(),
        N::Other(_) => None,
    };
    Ok(value.filter(|n| n.is_finite()))
}
