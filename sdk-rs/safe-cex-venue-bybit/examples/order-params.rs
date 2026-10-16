use safe_cex_venue_bybit::order_params::*;
use serde_json::{Value, json};
use std::{
    borrow::Borrow,
    io::{self, Read},
};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let result: Vec<_> = cases.into_iter().map(|case| {
        let opts: BybitPlaceOrderOpts = match serde_json::from_value(case["opts"].clone()) {
            Ok(opts) => opts, Err(_) => return json!({"accepted":false}),
        };
        assert_native_options(&opts, &case["opts"]);
        match get_bybit_order_execution_params(&opts, case["category"].as_str().unwrap()) {
            Err(_) => json!({"accepted":false}),
            Ok(params) => {
                let plain = BybitPlaceOrderOpts::from((opts.borrow() as &safe_cex_types::PlaceOrderOpts).clone());
                json!({"accepted":true,"params":params,"single":has_bybit_single_order_params(&opts),"roundTrip":opts,"plainHasExtension":!matches!(plain,BybitPlaceOrderOpts::Plain(_))})
            }
        }
    }).collect();
    println!("{}", serde_json::to_string(&result).unwrap());
}

// Inspect native fields before any serializer can undo a decoder permutation.
fn assert_native_options(opts: &BybitPlaceOrderOpts, wire: &Value) {
    use safe_cex_types::{
        OrderSide, OrderStopTriggerType, OrderTimeInForce, OrderType, PlaceOrderOpts,
    };
    let base: &PlaceOrderOpts = opts.borrow();
    assert_eq!(
        base.symbol,
        wire["symbol"].as_str().unwrap(),
        "native symbol"
    );
    assert_eq!(
        base.client_order_id.as_deref(),
        wire["clientOrderId"].as_str(),
        "native clientOrderId"
    );
    assert_eq!(
        base.amount,
        wire["amount"].as_f64().unwrap(),
        "native amount"
    );
    for (field, actual) in [
        ("price", base.price),
        ("triggerPrice", base.trigger_price),
        ("stopLoss", base.stop_loss),
        ("takeProfit", base.take_profit),
        ("maxSlippagePct", base.max_slippage_pct),
    ] {
        assert_eq!(actual, wire[field].as_f64(), "native {field}");
    }
    for (field, actual) in [("reduceOnly", base.reduce_only), ("close", base.close)] {
        assert_eq!(actual, wire[field].as_bool(), "native {field}");
    }
    let kind = match base.r#type {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    };
    assert_eq!(Some(kind), wire["type"].as_str(), "native type");
    let side = match base.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    assert_eq!(Some(side), wire["side"].as_str(), "native side");
    let trigger = |v: OrderStopTriggerType| match v {
        OrderStopTriggerType::MarkPrice => "mark_price",
        OrderStopTriggerType::LastPrice => "last_price",
        OrderStopTriggerType::IndexPrice => "index_price",
    };
    for (field, actual) in [
        ("triggerType", base.trigger_type),
        ("tpTriggerType", base.tp_trigger_type),
        ("slTriggerType", base.sl_trigger_type),
    ] {
        assert_eq!(actual.map(trigger), wire[field].as_str(), "native {field}");
    }
    let tif = base.time_in_force.map(|v| match v {
        OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
        OrderTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
        OrderTimeInForce::FillOrKill => "FillOrKill",
        OrderTimeInForce::PostOnly => "PostOnly",
    });
    assert_eq!(tif, wire["timeInForce"].as_str(), "native timeInForce");
    match opts {
        BybitPlaceOrderOpts::Plain(_) => {
            assert!(wire.get("bybitBbo").is_none());
            assert!(wire.get("bybitSlippageToleranceTicks").is_none());
        }
        BybitPlaceOrderOpts::Bbo(value) => {
            let side = match value.side_type {
                BybitBboSideType::Queue => "Queue",
                BybitBboSideType::Counterparty => "Counterparty",
            };
            assert_eq!(
                Some(side),
                wire["bybitBbo"]["sideType"].as_str(),
                "native BBO side"
            );
            assert_eq!(
                value.level,
                BybitBboLevel::try_from(wire["bybitBbo"]["level"].as_f64().unwrap()).unwrap(),
                "native BBO level"
            );
        }
        BybitPlaceOrderOpts::TickSize(value) => assert_eq!(
            value.ticks,
            wire["bybitSlippageToleranceTicks"].as_f64().unwrap(),
            "native ticks"
        ),
    }
}
