use safe_cex_types::{OrderSide, OrderType, PositionSide};
use safe_cex_venue_weex::order_routing::{
    WeexBatchOrderFields, infer_position_side, is_algo_order, is_batchable_regular_order,
};
use serde_json::Value;
use std::{path::Path, process::Command};

fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

fn string(value: &Value) -> Option<String> {
    (value["kind"] == "string")
        .then(|| String::from_utf16(&units(&value["units"])).ok())
        .flatten()
}

fn order_type(value: &Value) -> Option<OrderType> {
    match string(value)?.as_str() {
        "limit" => Some(OrderType::Limit),
        "market" => Some(OrderType::Market),
        "conditional" => Some(OrderType::Conditional),
        "stop_market" => Some(OrderType::StopLoss),
        "take_profit_market" => Some(OrderType::TakeProfit),
        "trailing_stop_market" => Some(OrderType::TrailingStopLoss),
        _ => None,
    }
}

fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap())
}

fn defined(value: Option<&Value>) -> bool {
    value.is_some_and(|value| value["kind"] != "undefined")
}

fn exactly_true(value: Option<&Value>) -> bool {
    value.is_some_and(|value| value["kind"] == "boolean" && value["value"] == true)
}

fn truthy(value: &Value) -> bool {
    match value["kind"].as_str().unwrap() {
        "undefined" | "null" => false,
        "boolean" => value["value"].as_bool().unwrap(),
        "number" => {
            let n = number(value);
            n != 0.0 && !n.is_nan()
        }
        "string" => !units(&value["units"]).is_empty(),
        "bigint" => value["text"] != "0",
        "object" | "function" | "symbol" => true,
        other => panic!("unexpected primitive projection {other}"),
    }
}

#[test]
fn order_routing_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/order-routing-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let type_names = [
        "Limit",
        "Market",
        "Conditional",
        "StopLoss",
        "TakeProfit",
        "TrailingStopLoss",
    ];
    let mut batch_ids: Vec<String> = type_names
        .iter()
        .map(|name| format!("base_{name}"))
        .collect();
    for field in ["triggerPrice", "stopLoss", "takeProfit"] {
        for suffix in [
            "zero",
            "negative_zero",
            "nan",
            "infinity",
            "negative_infinity",
        ] {
            batch_ids.push(format!("{field}_{suffix}"));
        }
    }
    for field in ["tpTriggerType", "slTriggerType"] {
        for suffix in ["LastPrice", "MarkPrice", "IndexPrice"] {
            batch_ids.push(format!("{field}_{suffix}"));
        }
    }
    for field in ["reduceOnly", "close"] {
        for suffix in ["undefined", "false", "true"] {
            batch_ids.push(format!("{field}_{suffix}"));
        }
    }
    batch_ids.extend(
        [
            "price_zero",
            "price_nan",
            "amount_infinity",
            "unrelated_trigger",
            "both_flags_false",
            "all_guards",
            "triggerPrice_null",
            "tpTriggerType_null",
            "reduceOnly_number",
            "close_string",
            "unknown_type",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    let batches = corpus["batches"].as_array().unwrap();
    assert_eq!(batches.len(), 44, "routing::batch_count");
    let (mut compared, mut batch_true) = (0, 0);
    for (index, case) in batches.iter().enumerate() {
        let id = &batch_ids[index];
        assert_eq!(case["id"], *id, "routing::batch_identity");
        let input = &case["input"];
        if id == "triggerPrice_negative_zero" {
            assert_eq!(
                number(&input["triggerPrice"]).to_bits(),
                0x8000000000000000,
                "routing::negative_zero_input"
            );
        }
        let fields = WeexBatchOrderFields {
            order_type: order_type(&input["type"]),
            trigger_price_defined: defined(input.get("triggerPrice")),
            stop_loss_defined: defined(input.get("stopLoss")),
            take_profit_defined: defined(input.get("takeProfit")),
            reduce_only_is_true: exactly_true(input.get("reduceOnly")),
            close_is_true: exactly_true(input.get("close")),
            tp_trigger_type_defined: defined(input.get("tpTriggerType")),
            sl_trigger_type_defined: defined(input.get("slTriggerType")),
        };
        let result = is_batchable_regular_order(&fields);
        assert_eq!(
            result,
            case["result"].as_bool().unwrap(),
            "routing::batch::{id}"
        );
        batch_true += usize::from(result);
        compared += 1;
    }
    assert_eq!(batch_true, 13, "routing::batch_true_count");

    let id_names = [
        "plain",
        "double",
        "prefix",
        "suffix",
        "single",
        "triplet",
        "empty",
        "lone",
        "lone_plain",
        "split_unicode",
        "fullwidth",
    ];
    let algo_ids: Vec<_> = type_names
        .iter()
        .copied()
        .chain(["unknown"])
        .flat_map(|kind| id_names.iter().map(move |id| format!("{kind}_{id}")))
        .collect();
    let algos = corpus["algos"].as_array().unwrap();
    assert_eq!(algos.len(), 77, "routing::algo_count");
    let mut algo_true = 0;
    for (index, case) in algos.iter().enumerate() {
        let id = &algo_ids[index];
        assert_eq!(case["id"], *id, "routing::algo_identity");
        let order_id = units(&case["units"]);
        if id == "Limit_lone" {
            assert_eq!(order_id[0], 0xd800, "routing::lone_surrogate_input");
        }
        let result = is_algo_order(order_type(&case["type"]), &order_id);
        assert_eq!(
            result,
            case["result"].as_bool().unwrap(),
            "routing::algo::{id}"
        );
        algo_true += usize::from(result);
        compared += 1;
    }
    assert_eq!(algo_true, 59, "routing::algo_true_count");

    let reduce_ids = [
        "undefined",
        "false",
        "true",
        "zero",
        "negative_zero",
        "nan",
        "one",
        "empty",
        "string",
        "null",
        "object",
        "function",
        "symbol",
        "bigint_zero",
        "bigint_one",
    ];
    let position_ids: Vec<_> = ["Buy", "Sell", "unknown"]
        .into_iter()
        .flat_map(|side| reduce_ids.iter().map(move |id| format!("{side}_{id}")))
        .collect();
    let positions = corpus["positions"].as_array().unwrap();
    assert_eq!(positions.len(), 45, "routing::position_count");
    let mut long = 0;
    for (index, case) in positions.iter().enumerate() {
        let id = &position_ids[index];
        assert_eq!(case["id"], *id, "routing::position_identity");
        let side = match string(&case["side"]).as_deref() {
            Some("buy") => Some(OrderSide::Buy),
            Some("sell") => Some(OrderSide::Sell),
            _ => None,
        };
        let result = infer_position_side(side, truthy(&case["reduceOnly"]));
        let expected = match case["result"].as_str().unwrap() {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            other => panic!("position {other}"),
        };
        assert_eq!(result, expected, "routing::position::{id}");
        long += usize::from(result == PositionSide::Long);
        compared += 1;
    }
    assert_eq!(long, 22, "routing::position_long_count");
    assert_eq!(compared, 166, "routing::compared");
}
