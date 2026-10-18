use safe_cex_exchange::{exchange::BaseExchange, runtime::ExchangeError};
use safe_cex_num::{adjust, js_number_string};
use safe_cex_types::{
    OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PlaceOrderOpts, PositionSide,
};
use serde_json::{Map, Value};

// ts: BinanceExchange.formatCreateOrder return shape
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BinanceOrderPayloads {
    pub regular: Vec<Map<String, Value>>,
    pub algo: Vec<Map<String, Value>>,
}

// ts: BinanceExchange.formatCreateOrder
pub fn format_futures_create_order(
    base: &BaseExchange,
    opts: &PlaceOrderOpts,
    mut uuid: impl FnMut() -> String,
) -> Result<BinanceOrderPayloads, ExchangeError> {
    let market = base
        .store
        .markets_by_symbol()?
        .get(&opts.symbol)
        .cloned()
        .ok_or_else(|| ExchangeError::new(format!("Market {} not found", opts.symbol)))?;
    let is_hedged = base.store.options()?.is_hedged;
    if opts.r#type == OrderType::TrailingStopLoss {
        let ticker = base.store.tickers_by_symbol()?.get(&opts.symbol).cloned();
        let positions = base.store.positions()?.to_vec();
        let algo = crate::trailing_orders::format_create_trailing_stop_loss_order(
            opts,
            Some(&market),
            ticker.as_ref(),
            &positions,
            is_hedged,
            uuid,
        )
        .map_err(ExchangeError::new)?;
        return Ok(BinanceOrderPayloads {
            regular: vec![],
            algo,
        });
    }
    let stop = matches!(opts.r#type, OrderType::StopLoss | OrderType::TakeProfit);
    let amount = adjust(opts.amount, market.precision.amount);
    let has_amount = amount != 0.0 && !amount.is_nan();
    let price = opts
        .price
        .filter(|n| *n != 0.0 && !n.is_nan())
        .filter(|_| opts.r#type != OrderType::Market)
        .map(|n| adjust(n, market.precision.price));
    let reduce_only = !is_hedged && opts.reduce_only == Some(true);
    let wants_close = stop && (opts.close == Some(true) || !has_amount);
    let opposite = if opts.side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    let existing = base
        .store
        .positions()?
        .iter()
        .any(|p| p.symbol == opts.symbol && p.side == opposite && p.contracts.abs() > 0.0);
    let use_close = wants_close && existing;
    let mut regular = Vec::new();
    let mut algo = Vec::new();
    let mut request = Map::new();
    if stop {
        insert(&mut request, "algoType", "CONDITIONAL");
    }
    insert(&mut request, "symbol", opts.symbol.clone());
    insert(
        &mut request,
        "positionSide",
        crate::order_logic::get_order_position_side(is_hedged, opts),
    );
    insert(&mut request, "side", side(opts.side));
    insert_type(&mut request, opts.r#type);
    if has_amount && !use_close {
        insert(&mut request, "quantity", js_number_string(amount));
    }
    if let Some(price) = price.filter(|n| *n != 0.0 && !n.is_nan()) {
        insert(
            &mut request,
            if stop { "triggerPrice" } else { "price" },
            js_number_string(price),
        );
    }
    if stop {
        if use_close {
            insert(&mut request, "closePosition", "true");
        }
        if !use_close && reduce_only {
            insert(&mut request, "reduceOnly", "true");
        }
        insert(
            &mut request,
            "workingType",
            working_type(opts, opts.r#type == OrderType::TakeProfit),
        );
        insert(
            &mut request,
            "clientAlgoId",
            format!("x-s3vFFRQp{}", uuid()),
        );
        if wants_close && !existing && !has_amount {
            return base.reject_preflight(
                "Cannot place full stop order without existing position or specified amount".into(),
            );
        }
        algo.push(request);
    } else {
        if opts.r#type == OrderType::Limit
            && let Some((name, _)) =
                crate::binance_types::TIME_IN_FORCE
                    .iter()
                    .rfind(|(_, value)| {
                        *value
                            == opts
                                .time_in_force
                                .unwrap_or(OrderTimeInForce::GoodTillCancel)
                    })
        {
            insert(&mut request, "timeInForce", *name);
        }
        if reduce_only {
            insert(&mut request, "reduceOnly", "true");
        }
        let max = market.limits.amount.max;
        let lots = if amount > max {
            (amount / max).ceil()
        } else {
            1.0
        };
        let rest = if amount > max {
            adjust(amount % max, market.precision.amount)
        } else {
            0.0
        };
        let lot_size = adjust((amount - rest) / lots, market.precision.amount);
        if (1.0..=9_007_199_254_740_991.0).contains(&lots) {
            for _ in 0..lots as usize {
                let mut payload = request.clone();
                insert(&mut payload, "quantity", js_number_string(lot_size));
                regular.push(payload);
            }
        }
        if rest != 0.0 && !rest.is_nan() {
            let mut payload = request.clone();
            insert(&mut payload, "quantity", js_number_string(rest));
            regular.push(payload);
        }
        let client_order_id = base.resolve_caller_client_order_id(opts)?;
        base.assert_single_native_client_order_id(client_order_id.as_deref(), regular.len())?;
        for payload in &mut regular {
            insert(
                payload,
                "newClientOrderId",
                client_order_id
                    .clone()
                    .unwrap_or_else(|| format!("x-s3vFFRQp{}", uuid())),
            );
        }
    }
    for (price, kind) in [
        (opts.stop_loss, OrderType::StopLoss),
        (opts.take_profit, OrderType::TakeProfit),
    ] {
        if let Some(price) = price.filter(|n| *n != 0.0 && !n.is_nan()) {
            let mut opposite_opts = opts.clone();
            opposite_opts.side = if opts.side == OrderSide::Buy {
                OrderSide::Sell
            } else {
                OrderSide::Buy
            };
            opposite_opts.r#type = kind;
            opposite_opts.reduce_only = Some(true);
            let mut payload = Map::new();
            insert(&mut payload, "algoType", "CONDITIONAL");
            insert(&mut payload, "symbol", opts.symbol.clone());
            insert(
                &mut payload,
                "positionSide",
                crate::order_logic::get_order_position_side(is_hedged, &opposite_opts),
            );
            insert(&mut payload, "side", side(opposite_opts.side));
            insert_type(&mut payload, kind);
            insert(
                &mut payload,
                "triggerPrice",
                js_number_string(adjust(price, market.precision.price)),
            );
            if has_amount {
                insert(&mut payload, "quantity", js_number_string(amount));
            }
            if !is_hedged {
                insert(&mut payload, "reduceOnly", "true");
            }
            insert(
                &mut payload,
                "workingType",
                working_type(opts, kind == OrderType::TakeProfit),
            );
            insert(
                &mut payload,
                "clientAlgoId",
                format!("x-s3vFFRQp{}", uuid()),
            );
            algo.push(payload);
        }
    }
    Ok(BinanceOrderPayloads { regular, algo })
}
fn insert(payload: &mut Map<String, Value>, key: &str, value: impl Into<String>) {
    payload.insert(key.into(), Value::String(value.into()));
}
fn insert_type(payload: &mut Map<String, Value>, kind: OrderType) {
    if let Some((name, _)) = crate::binance_types::ORDER_TYPE
        .iter()
        .rfind(|(_, value)| *value == kind)
    {
        insert(payload, "type", *name);
    }
}
fn side(side: OrderSide) -> &'static str {
    if side == OrderSide::Buy {
        "BUY"
    } else {
        "SELL"
    }
}
fn working_type(opts: &PlaceOrderOpts, take_profit: bool) -> &'static str {
    if if take_profit {
        opts.tp_trigger_type == Some(OrderStopTriggerType::MarkPrice)
    } else {
        opts.sl_trigger_type != Some(OrderStopTriggerType::LastPrice)
    } {
        "MARK_PRICE"
    } else {
        "CONTRACT_PRICE"
    }
}
