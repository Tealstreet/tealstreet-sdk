//! Borrowed PerpetualV2 account/order wire records. Rp/Rv values are unscaled.
use crate::wire::{Str, number, optional_number};
use safe_cex_engine::Store;
use safe_cex_engine::model::*;
use serde::{Deserialize, Deserializer};

pub fn integer<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Integer<'a> {
        Number(u64),
        Text(#[serde(borrow)] Str<'a>),
    }
    match Integer::deserialize(d)? {
        Integer::Number(n) => Ok(n),
        Integer::Text(s) => s.parse().map_err(serde::de::Error::custom),
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Context {
    pub mode: PositionMode,
    pub margin_type: MarginType,
    pub long_leverage: f64,
    pub short_leverage: f64,
}
pub fn order_side(value: &str) -> Option<OrderSide> {
    match value {
        "Buy" => Some(OrderSide::Buy),
        "Sell" => Some(OrderSide::Sell),
        _ => None,
    }
}
pub fn trigger(value: &str) -> Option<StopTriggerType> {
    match value {
        "ByMarkPrice" => Some(StopTriggerType::MarkPrice),
        "ByLastPrice" => Some(StopTriggerType::LastPrice),
        "ByIndexPrice" => Some(StopTriggerType::IndexPrice),
        _ => None,
    }
}
#[derive(Deserialize)]
pub struct AccountRow<'a> {
    #[serde(borrow)]
    pub currency: Str<'a>,
    #[serde(rename = "accountBalanceRv", deserialize_with = "number")]
    pub total: f64,
    #[serde(rename = "totalUsedBalanceRv", deserialize_with = "number")]
    pub used: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, borrow)]
    pub currency: Str<'a>,
    #[serde(default, borrow)]
    pub pos_mode: Str<'a>,
    #[serde(default, borrow)]
    pub pos_side: Str<'a>,
    #[serde(default, borrow, alias = "Side")]
    pub side: Str<'a>,
    #[serde(deserialize_with = "number", alias = "sizeRq")]
    pub size: f64,
    #[serde(deserialize_with = "number")]
    pub leverage_rr: f64,
    #[serde(default)]
    pub cross_margin: bool,
    #[serde(default, deserialize_with = "optional_number")]
    pub avg_entry_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub avg_entry_price_rp: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub mark_price_rp: f64,
    #[serde(default, deserialize_with = "number")]
    pub liquidation_price_rp: f64,
    #[serde(default, deserialize_with = "integer")]
    pub transact_time_ns: u64,
}
impl PositionRow<'_> {
    pub fn map(
        &self,
        store: &Store,
        previous: Option<Context>,
    ) -> Option<(Context, Option<Position>)> {
        let market = store.market(&self.symbol.to_ascii_uppercase())?;
        if !self.currency.is_empty() && self.currency != market.quote {
            return None;
        }
        let mode = match self.pos_mode.as_ref() {
            "Hedged" => PositionMode::Hedged,
            "OneWay" => PositionMode::Oneway,
            _ => previous?.mode,
        };
        let leverage = self.leverage_rr.abs();
        let margin_type = if self.leverage_rr < 0. || self.cross_margin {
            MarginType::Cross
        } else {
            MarginType::Isolated
        };
        let mut context = previous.unwrap_or(Context {
            mode,
            margin_type,
            long_leverage: 0.,
            short_leverage: 0.,
        });
        context.mode = mode;
        context.margin_type = margin_type;
        match self.pos_side.as_ref() {
            "Long" => context.long_leverage = leverage,
            "Short" => context.short_leverage = leverage,
            "Merged" => {
                context.long_leverage = leverage;
                context.short_leverage = leverage;
            }
            _ => return None,
        }
        if self.size == 0. {
            return Some((context, None));
        }
        let side = if self.pos_side == "Long" || self.side == "Buy" {
            PositionSide::Long
        } else if self.pos_side == "Short" || self.side == "Sell" {
            PositionSide::Short
        } else {
            return None;
        };
        let entry = self.avg_entry_price.or(self.avg_entry_price_rp)?;
        if entry <= 0. || self.liquidation_price_rp < 0. {
            return None;
        }
        let contracts = self.size.abs();
        let last = store
            .ticker(&market.symbol)
            .map(|t| t.last)
            .filter(|p| p.is_finite() && *p > 0.)
            .unwrap_or(if self.mark_price_rp > 0. {
                self.mark_price_rp
            } else {
                entry
            });
        let upnl = contracts
            * if side == PositionSide::Long {
                last - entry
            } else {
                entry - last
            };
        Some((
            context,
            Some(Position {
                id: if mode == PositionMode::Oneway {
                    market.symbol.clone()
                } else {
                    format!(
                        "{}:{}",
                        market.symbol,
                        if side == PositionSide::Long {
                            "long"
                        } else {
                            "short"
                        }
                    )
                },
                symbol: market.symbol.clone(),
                side,
                entry_price: entry,
                notional: contracts * entry,
                leverage,
                margin_type: Some(margin_type),
                position_mode: Some(mode),
                unrealized_pnl: upnl,
                realized_pnl: None,
                contracts,
                liquidation_price: self.liquidation_price_rp,
                margin_size: None,
                last_update: (self.transact_time_ns > 0)
                    .then_some(self.transact_time_ns as f64 / 1e6),
            }),
        ))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(rename = "orderID", alias = "orderId", borrow)]
    pub order_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(default, borrow)]
    pub pos_side: Str<'a>,
    #[serde(default, borrow, rename = "ordType", alias = "orderType")]
    pub kind: Str<'a>,
    #[serde(default, borrow, rename = "ordStatus", alias = "orderStatus")]
    pub status: Str<'a>,
    #[serde(default, borrow, rename = "clOrdID", alias = "clOrdId")]
    pub client_order_id: Str<'a>,
    #[serde(default, deserialize_with = "optional_number")]
    pub order_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub order_qty_rq: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub exec_qty: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub cum_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub cum_qty_rq: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub leaves_qty: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub leaves_qty_rq: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub price_rp: f64,
    #[serde(default, deserialize_with = "number")]
    pub stop_px_rp: f64,
    #[serde(default, deserialize_with = "number")]
    pub stop_loss_rp: f64,
    #[serde(default, deserialize_with = "number")]
    pub take_profit_rp: f64,
    #[serde(default, borrow)]
    pub stop_direction: Str<'a>,
    #[serde(default, borrow)]
    pub peg_price_type: Str<'a>,
    #[serde(default, borrow, rename = "trigger", alias = "triggerType")]
    pub trigger_type: Str<'a>,
    #[serde(default, borrow)]
    pub sl_trigger: Str<'a>,
    #[serde(default, borrow)]
    pub tp_trigger: Str<'a>,
    #[serde(default, borrow)]
    pub time_in_force: Str<'a>,
    #[serde(default, borrow)]
    pub exec_inst: Str<'a>,
    #[serde(default, deserialize_with = "boolean")]
    pub reduce_only: bool,
    #[serde(default, borrow, rename = "execID")]
    pub execution_id: Str<'a>,
    #[serde(default, borrow)]
    pub exec_status: Str<'a>,
    #[serde(default, borrow)]
    pub trade_type: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub exec_price_rp: f64,
    #[serde(default, deserialize_with = "integer")]
    pub transact_time_ns: u64,
    #[serde(default, deserialize_with = "integer")]
    pub action_time_ns: u64,
}
fn boolean<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum B<'a> {
        Bool(bool),
        Text(#[serde(borrow)] Str<'a>),
    }
    match B::deserialize(d)? {
        B::Bool(b) => Ok(b),
        B::Text(s) => match s.as_ref() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(serde::de::Error::custom("invalid boolean")),
        },
    }
}
impl OrderRow<'_> {
    pub fn cumulative(&self) -> Option<f64> {
        self.cum_qty.or(self.cum_qty_rq)
    }
    pub fn leaves(&self) -> Option<f64> {
        self.leaves_qty.or(self.leaves_qty_rq)
    }

    pub fn version(&self) -> u64 {
        self.transact_time_ns.max(self.action_time_ns)
    }
    pub fn terminal(&self) -> bool {
        matches!(
            self.status.to_ascii_lowercase().as_str(),
            "filled" | "canceled" | "cancelled" | "rejected" | "deactivated" | "expired"
        )
    }
    pub fn status(&self) -> Option<OrderStatus> {
        match self.status.to_ascii_lowercase().as_str() {
            "created" | "new" | "untriggered" | "partiallyfilled" | "partially_filled" => {
                Some(OrderStatus::Open)
            }
            "triggered" | "filled" => Some(OrderStatus::Closed),
            "canceled" | "cancelled" | "rejected" | "deactivated" | "expired" => {
                Some(OrderStatus::Canceled)
            }
            _ => None,
        }
    }
    /// A status or order type this engine does not model (a newer venue type,
    /// or one placed outside Tealstreet). Such a row is skipped, not fatal; an
    /// unknown status never removes a stored order, since it may still be live.
    pub fn unrecognized(&self) -> bool {
        self.status().is_none()
            || !matches!(
                self.kind.as_ref(),
                "Market" | "Limit" | "Stop" | "StopLimit" | "MarketIfTouched" | "LimitIfTouched"
            )
    }
    pub fn orders(&self, now: f64) -> Option<Vec<Order>> {
        if self.order_id.is_empty() || self.symbol.is_empty() {
            return None;
        }
        let side = order_side(&self.side)?;
        let status = self.status()?;
        let mut kind = match self.kind.as_ref() {
            "Market" => OrderType::Market,
            "Limit" => OrderType::Limit,
            "Stop" | "StopLimit" | "MarketIfTouched" | "LimitIfTouched" => {
                match (self.stop_direction.as_ref(), side) {
                    ("Rising", OrderSide::Sell) | ("Falling", OrderSide::Buy) => {
                        OrderType::TakeProfit
                    }
                    ("Rising", OrderSide::Buy) | ("Falling", OrderSide::Sell) => {
                        OrderType::StopLoss
                    }
                    _ => {
                        if matches!(self.kind.as_ref(), "Stop" | "StopLimit") {
                            OrderType::StopLoss
                        } else {
                            OrderType::TakeProfit
                        }
                    }
                }
            }
            _ => return None,
        };
        if self.peg_price_type == "TrailingStopPeg" {
            kind = OrderType::TrailingStopLoss;
        }
        let amount = self.order_qty.or(self.order_qty_rq)?;
        if self
            .order_qty
            .zip(self.order_qty_rq)
            .is_some_and(|(a, b)| a != b)
            || self
                .cum_qty
                .zip(self.cum_qty_rq)
                .is_some_and(|(a, b)| a != b)
            || self
                .leaves_qty
                .zip(self.leaves_qty_rq)
                .is_some_and(|(a, b)| a != b)
        {
            return None;
        }
        let filled = self
            .cumulative()
            .or_else(|| self.leaves().map(|leaves| (amount - leaves).max(0.)))
            .unwrap_or(self.exec_qty);
        if amount < 0.
            || filled < 0.
            || self.exec_qty < 0.
            || self.leaves().is_some_and(|n| n < 0.)
            || self.price_rp < 0.
            || self.stop_px_rp < 0.
        {
            return None;
        }
        let remaining = self.leaves().unwrap_or((amount - filled).max(0.));
        let reduce_only = self.reduce_only
            || self.exec_inst.contains("ReduceOnly")
            || self.exec_inst.contains("CloseOnTrigger");
        let price = if self.stop_px_rp > 0. {
            self.stop_px_rp
        } else {
            self.price_rp
        };
        let primary = Order {
            id: self.order_id.to_string(),
            parent_id: None,
            status,
            symbol: self.symbol.to_ascii_uppercase(),
            kind,
            side,
            price,
            amount,
            filled,
            remaining,
            reduce_only,
            close: None,
            client_order_id: (!self.client_order_id.is_empty())
                .then(|| self.client_order_id.to_string()),
            time_in_force: match self.time_in_force.as_ref() {
                "GoodTillCancel" => Some(TimeInForce::GoodTillCancel),
                "ImmediateOrCancel" => Some(TimeInForce::ImmediateOrCancel),
                "FillOrKill" => Some(TimeInForce::FillOrKill),
                "PostOnly" => Some(TimeInForce::PostOnly),
                _ => None,
            },
            last_update: if self.version() > 0 {
                self.version() as f64 / 1e6
            } else {
                now
            },
            trigger_type: if matches!(
                kind,
                OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
            ) {
                trigger(&self.trigger_type)
            } else {
                None
            },
            take_profit: (self.take_profit_rp > 0.).then_some(self.take_profit_rp),
            stop_loss: (self.stop_loss_rp > 0.).then_some(self.stop_loss_rp),
            extra: matches!(self.kind.as_ref(), "StopLimit" | "LimitIfTouched").then_some(
                OrderExtra {
                    conditional_order_price: Some(self.price_rp),
                    ..OrderExtra::default()
                },
            ),
        };
        let mut orders = vec![primary];
        for (suffix, kind, price, source) in [
            (
                "stop_loss",
                OrderType::StopLoss,
                self.stop_loss_rp,
                self.sl_trigger.as_ref(),
            ),
            (
                "take_profit",
                OrderType::TakeProfit,
                self.take_profit_rp,
                self.tp_trigger.as_ref(),
            ),
        ] {
            if price > 0. {
                let mut child = orders[0].clone();
                child.id = format!("{}__{suffix}", self.order_id);
                child.parent_id = Some(self.order_id.to_string());
                child.kind = kind;
                child.side = side.opposite();
                child.price = price;
                child.amount = 0.;
                child.filled = 0.;
                child.trigger_type = trigger(source);
                orders.push(child);
            }
        }
        Some(orders)
    }
}
