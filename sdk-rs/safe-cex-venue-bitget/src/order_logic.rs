//! Pure order decisions. Exchange state lookups are explicit inputs to these functions.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{
    OrderSide, OrderStopTriggerType, OrderType, PlaceOrderOpts, Position, PositionMode,
    PositionSide, PreparedOrderOpts,
};
use serde::Serialize;

// ts: BitgetExchange.getOrderSideV2
pub fn get_order_side_v2(
    side: OrderSide,
    reduce_only: Option<bool>,
    mode: PositionMode,
) -> (OrderSide, Option<&'static str>) {
    let mut side = side;
    let mut trade_side = None;
    if mode == PositionMode::Hedged {
        if reduce_only == Some(true) {
            trade_side = Some("close");
            side = if side == OrderSide::Buy {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
        } else {
            trade_side = Some("open");
        }
    }
    (side, trade_side)
}

// ts: BitgetExchange.getOrderSideUta
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UtaOrderSide {
    pub side: OrderSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<PositionSide>,
}

// ts: BitgetExchange.getOrderSideUta
pub fn get_order_side_uta(
    side: OrderSide,
    reduce_only: Option<bool>,
    mode: PositionMode,
) -> UtaOrderSide {
    if mode != PositionMode::Hedged {
        return UtaOrderSide {
            side,
            pos_side: None,
        };
    }
    let pos_side = if reduce_only == Some(true) {
        if side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        }
    } else if side == OrderSide::Buy {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    UtaOrderSide {
        side,
        pos_side: Some(pos_side),
    }
}

// ts: BitgetExchange.getUtaStrategyOrderSide
pub fn get_uta_strategy_order_side(side: OrderSide, mode: PositionMode) -> UtaOrderSide {
    if mode != PositionMode::Hedged {
        return UtaOrderSide {
            side,
            pos_side: None,
        };
    }
    UtaOrderSide {
        side,
        pos_side: Some(if side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        }),
    }
}

// ts: BitgetExchange.getUtaStrategyTriggerBy
pub fn get_uta_strategy_trigger_by(trigger_type: Option<OrderStopTriggerType>) -> &'static str {
    if trigger_type == Some(OrderStopTriggerType::LastPrice) {
        "market"
    } else {
        "mark"
    }
}

// ts: BitgetExchange.getAlgoOrderSide
pub fn get_algo_order_side(
    order_type: OrderType,
    side: OrderSide,
    mode: PositionMode,
) -> Result<&'static str, ExchangeError> {
    if matches!(
        order_type,
        OrderType::TakeProfit | OrderType::StopLoss | OrderType::TrailingStopLoss
    ) {
        return Ok(if mode == PositionMode::Hedged {
            if side == OrderSide::Buy {
                "short"
            } else {
                "long"
            }
        } else if side == OrderSide::Buy {
            "sell"
        } else {
            "buy"
        });
    }
    Err(ExchangeError::new(format!(
        "Unknown algo order side: {} - {}",
        enum_text(order_type),
        enum_text(side)
    )))
}

fn enum_text(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .expect("domain enum serialization")
        .as_str()
        .unwrap_or("undefined")
        .to_owned()
}

// ts: BitgetExchange.isVirtualTPSLOrder
pub fn is_virtual_tpsl_order(id: &str) -> bool {
    id.ends_with("__stop_loss") || id.ends_with("__take_profit")
}

// ts: BitgetExchange.isAlgoOrder
pub fn is_algo_order(order_type: OrderType) -> bool {
    order_type == OrderType::StopLoss
        || order_type == OrderType::TakeProfit
        || order_type == OrderType::TrailingStopLoss
}

// ts: BitgetExchange.planTypeComparableAmount
pub fn plan_type_comparable_amount(amount: f64, step: Option<f64>) -> f64 {
    let Some(step) = step.filter(|step| *step != 0.0 && !step.is_nan()) else {
        return amount;
    };
    adjust(amount, step)
}

// ts: BitgetExchange.floorToLot
pub fn floor_to_lot(value: f64, step: Option<f64>) -> f64 {
    let Some(step) = step.filter(|step| *step != 0.0 && !step.is_nan()) else {
        return value;
    };
    let rounded = adjust(value, step);
    if rounded > value {
        adjust(rounded - step, step)
    } else {
        rounded
    }
}

// ts: BitgetExchange.selectPlanTypeAndAmount
pub fn select_plan_type_and_amount(
    opts: impl Into<PreparedOrderOpts>,
    positions: &[Position],
    amount_step: Option<f64>,
    mut emit_and_log_error: impl FnMut(String),
) -> Result<(&'static str, f64), ExchangeError> {
    let opts = opts.into();
    if opts.r#type == OrderType::TrailingStopLoss {
        return Ok(("track_plan", opts.amount));
    }
    let mut sized_amount = None;
    if opts.amount != 0.0 {
        let found_position = positions.iter().find(|position| {
            position.symbol == opts.symbol
                && position.side
                    == if opts.side == OrderSide::Buy {
                        PositionSide::Short
                    } else {
                        PositionSide::Long
                    }
                && position.contracts > 0.0
        });
        if let Some(position) = found_position {
            let comparable = plan_type_comparable_amount(opts.amount, amount_step);
            let floored = floor_to_lot(position.contracts, amount_step);
            // JS Math.min propagates NaN; Rust f64::min discards it.
            let clamped = if comparable.is_nan() || floored.is_nan() {
                f64::NAN
            } else {
                comparable.min(floored)
            };
            if clamped > 0.0 {
                sized_amount = Some(clamped);
            }
        }
    }
    if let Some(amount) = sized_amount {
        if opts.r#type == OrderType::StopLoss {
            return Ok(("loss_plan", amount));
        }
        if opts.r#type == OrderType::TakeProfit {
            return Ok(("profit_plan", amount));
        }
    } else {
        if opts.amount != 0.0 {
            let candidates = positions
                .iter()
                .filter(|position| position.symbol == opts.symbol)
                .map(|position| {
                    format!(
                        "{}:{}",
                        enum_text(position.side),
                        js_number_string(position.contracts)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            emit_and_log_error(format!(
                "Sized {} stop became a full-position stop: no open {} position to size {} (requested {}) against, positions seen [{}]",
                opts.symbol,
                if opts.side == OrderSide::Buy {
                    "short"
                } else {
                    "long"
                },
                js_number_string(plan_type_comparable_amount(opts.amount, amount_step)),
                js_number_string(opts.amount),
                if candidates.is_empty() {
                    "none"
                } else {
                    &candidates
                }
            ));
        }
        if opts.r#type == OrderType::StopLoss {
            return Ok(("pos_loss", 0.0));
        }
        if opts.r#type == OrderType::TakeProfit {
            return Ok(("pos_profit", 0.0));
        }
    }
    Err(ExchangeError::new(format!(
        "Unknown order type: {}",
        enum_text(opts.r#type)
    )))
}

// ts: BitgetExchange.buildUtaStrategyPlaceOptsFromOrder
pub fn build_uta_strategy_place_opts_from_order(
    order: &safe_cex_types::Order,
) -> PreparedOrderOpts {
    PreparedOrderOpts {
        symbol: order.symbol.clone(),
        r#type: order.r#type.into_property(),
        side: order.side.into_property(),
        price: order.price.into_value(),
        amount: order.amount,
        reduce_only: order.reduce_only.into_property(),
        time_in_force: (order.time_in_force).into_value(),
        tp_trigger_type: (order.tp_trigger_type).into_value(),
        sl_trigger_type: (order.sl_trigger_type).into_value(),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
        implied_volatility: None,
    }
}

// ts: BitgetExchange.getCancelPlanType
pub fn get_cancel_plan_type(
    order: &safe_cex_types::Order,
    positions: &[Position],
    amount_step: Option<f64>,
    emit_and_log_error: impl FnMut(String),
) -> Result<safe_cex_num::TableHit<&'static str>, ExchangeError> {
    use safe_cex_num::{TableHit, js_table_lookup};
    let raw = order.info.as_ref().and_then(|info| info.get("planType"));
    if raw.is_some_and(|value| value.is_truthy()) {
        let key = safe_cex_num::js_retained_string(raw).map_err(|error| ExchangeError {
            name: error.name.into(),
            message: error.message.into(),
            metadata: Box::default(),
        })?;
        let table = [
            ("psl", "pos_loss"),
            ("ptp", "pos_profit"),
            ("pos_loss", "pos_loss"),
            ("pos_profit", "pos_profit"),
            ("loss_plan", "loss_plan"),
            ("profit_plan", "profit_plan"),
            ("track_plan", "track_plan"),
            ("sl", "loss_plan"),
            ("tp", "profit_plan"),
        ];
        // ts-bug: BitgetExchange.getCancelPlanType returns inherited members
        // instead of taking the immutable-order fallback. Preserve their identity.
        match js_table_lookup(&table, &key) {
            TableHit::Mapped(value) => return Ok(TableHit::Mapped(*value)),
            TableHit::Inherited(member) => return Ok(TableHit::Inherited(member)),
            TableHit::Missing => {}
        }
    }
    if order.r#type == OrderType::TrailingStopLoss {
        return Ok(TableHit::Mapped("track_plan"));
    }
    let sized = plan_type_comparable_amount(order.amount, amount_step) > 0.0;
    if order.r#type == OrderType::StopLoss {
        return Ok(TableHit::Mapped(if sized {
            "loss_plan"
        } else {
            "pos_loss"
        }));
    }
    if order.r#type == OrderType::TakeProfit {
        return Ok(TableHit::Mapped(if sized {
            "profit_plan"
        } else {
            "pos_profit"
        }));
    }
    let opts = build_uta_strategy_place_opts_from_order(order);
    select_plan_type_and_amount(&opts, positions, amount_step, emit_and_log_error)
        .map(|(plan, _)| TableHit::Mapped(plan))
}

// ts: BitgetExchange.getOrderPlanTypeAndAmount
/// The source union retains the original argument, including existing Order metadata.
#[derive(Clone, Copy)]
pub enum PlanSelectionOptions<'a> {
    Place(&'a PlaceOrderOpts),
    Order(&'a safe_cex_types::Order),
}
impl<'a> PlanSelectionOptions<'a> {
    fn fields(
        self,
    ) -> (
        &'a str,
        safe_cex_types::ModelField<OrderType>,
        safe_cex_types::ModelField<OrderSide>,
        f64,
    ) {
        match self {
            Self::Place(opts) => (
                &opts.symbol,
                opts.r#type.into(),
                opts.side.into(),
                opts.amount,
            ),
            Self::Order(opts) => (&opts.symbol, opts.r#type, opts.side, opts.amount),
        }
    }
}

// ts: BitgetExchange.getOrderPlanTypeAndAmount
// Receiver operations stay lazy; an error keeps its original identity.
pub async fn get_order_plan_type_and_amount<'a, R, E, F>(
    opts: PlanSelectionOptions<'a>,
    store_positions: impl FnOnce() -> Vec<Position>,
    resolve_positions: impl FnOnce(&str, PositionSide) -> F,
    select: impl FnOnce(PlanSelectionOptions<'a>, &[Position]) -> Result<R, E>,
) -> Result<R, E>
where
    F: std::future::Future<Output = Result<Vec<Position>, E>>,
{
    let (symbol, kind, side, amount) = opts.fields();
    let positions = if amount == 0.0 || kind == OrderType::TrailingStopLoss {
        store_positions()
    } else {
        resolve_positions(
            symbol,
            if side == OrderSide::Buy {
                PositionSide::Short
            } else {
                PositionSide::Long
            },
        )
        .await?
    };
    select(opts, &positions)
}
