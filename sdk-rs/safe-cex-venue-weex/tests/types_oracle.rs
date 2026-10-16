#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_types::{
    MarginType, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce, OrderType,
    PositionMode, PositionSide,
};
use safe_cex_venue_weex::types::*;
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn native_forward<T: Copy + std::fmt::Debug + PartialEq>(
    name: &str,
    actual: &[(&str, T)],
    expected: &Value,
    decode: impl Fn(&str) -> T,
) {
    let expected: Vec<(&str, T)> = expected[name]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry[0].as_str().unwrap(),
                decode(entry[1].as_str().unwrap()),
            )
        })
        .collect();
    assert_eq!(actual, expected, "constant::{name}::native");
}
fn native_reverse<T: Copy + std::fmt::Debug + PartialEq>(
    name: &str,
    actual: &[(T, &str)],
    expected: &Value,
    decode: impl Fn(&str) -> T,
) {
    let expected: Vec<(T, &str)> = expected[name]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                decode(entry[0].as_str().unwrap()),
                entry[1].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(actual, expected, "constant::{name}::native");
}

#[test]
fn constants_match_actual_typescript() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/types-oracle.cjs");
    let output = Command::new("node")
        .arg(script)
        .output()
        .expect("node is required");
    assert!(
        output.status.success(),
        "TypeScript oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).expect("oracle JSON");
    cardinality::check(&expected, "types");
    assert!(expected["header"]["arch"].is_string(), "capture_arch");
    assert!(
        expected["header"]["nodeMajor"].is_number(),
        "capture_node_major"
    );
    let expected = &expected["constants"];
    // Native variants are decoded independently of shared Serialize/Deserialize.
    native_forward("ORDER_SIDE", ORDER_SIDE, expected, |value| match value {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        other => panic!("unknown source enum label {other}"),
    });
    native_reverse(
        "REVERSE_ORDER_SIDE",
        REVERSE_ORDER_SIDE,
        expected,
        |value| match value {
            "buy" => OrderSide::Buy,
            "sell" => OrderSide::Sell,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward(
        "POSITION_SIDE",
        POSITION_SIDE,
        expected,
        |value| match value {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_reverse(
        "REVERSE_POSITION_SIDE",
        REVERSE_POSITION_SIDE,
        expected,
        |value| match value {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward("ORDER_TYPE", ORDER_TYPE, expected, |value| match value {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        "conditional" => OrderType::Conditional,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        "trailing_stop_market" => OrderType::TrailingStopLoss,
        other => panic!("unknown source enum label {other}"),
    });
    native_reverse(
        "REVERSE_ORDER_TYPE",
        REVERSE_ORDER_TYPE,
        expected,
        |value| match value {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "conditional" => OrderType::Conditional,
            "stop_market" => OrderType::StopLoss,
            "take_profit_market" => OrderType::TakeProfit,
            "trailing_stop_market" => OrderType::TrailingStopLoss,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward(
        "TIME_IN_FORCE",
        TIME_IN_FORCE,
        expected,
        |value| match value {
            "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => OrderTimeInForce::FillOrKill,
            "PostOnly" => OrderTimeInForce::PostOnly,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_reverse(
        "REVERSE_TIME_IN_FORCE",
        REVERSE_TIME_IN_FORCE,
        expected,
        |value| match value {
            "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
            "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
            "FillOrKill" => OrderTimeInForce::FillOrKill,
            "PostOnly" => OrderTimeInForce::PostOnly,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward(
        "ORDER_STATUS",
        ORDER_STATUS,
        expected,
        |value| match value {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            "creating" => OrderStatus::Creating,
            "updating" => OrderStatus::Updating,
            "canceling" => OrderStatus::Canceling,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward("MARGIN_TYPE", MARGIN_TYPE, expected, |value| match value {
        "cross" => MarginType::Cross,
        "isolated" => MarginType::Isolated,
        other => panic!("unknown source enum label {other}"),
    });
    native_reverse(
        "REVERSE_MARGIN_TYPE",
        REVERSE_MARGIN_TYPE,
        expected,
        |value| match value {
            "cross" => MarginType::Cross,
            "isolated" => MarginType::Isolated,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward(
        "POSITION_MODE",
        POSITION_MODE,
        expected,
        |value| match value {
            "oneway" => PositionMode::OneWay,
            "hedged" => PositionMode::Hedged,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_reverse(
        "REVERSE_POSITION_MODE",
        REVERSE_POSITION_MODE,
        expected,
        |value| match value {
            "oneway" => PositionMode::OneWay,
            "hedged" => PositionMode::Hedged,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_forward(
        "ORDER_TRIGGER_TYPE",
        ORDER_TRIGGER_TYPE,
        expected,
        |value| match value {
            "last_price" => OrderStopTriggerType::LastPrice,
            "mark_price" => OrderStopTriggerType::MarkPrice,
            "index_price" => OrderStopTriggerType::IndexPrice,
            other => panic!("unknown source enum label {other}"),
        },
    );
    native_reverse(
        "REVERSE_ORDER_TRIGGER_TYPE",
        REVERSE_ORDER_TRIGGER_TYPE,
        expected,
        |value| match value {
            "last_price" => OrderStopTriggerType::LastPrice,
            "mark_price" => OrderStopTriggerType::MarkPrice,
            "index_price" => OrderStopTriggerType::IndexPrice,
            other => panic!("unknown source enum label {other}"),
        },
    );
    let actual = json!({
        "RECV_WINDOW": { "type": "number", "bits": format!("{:016x}", RECV_WINDOW.to_bits()) },
        "BASE_URL": BASE_URL,
        "BASE_WS_URL": BASE_WS_URL,
        "PRIVATE_WS_REQUEST_PATH": PRIVATE_WS_REQUEST_PATH,
        "ENDPOINTS": ENDPOINTS,
        "PUBLIC_ENDPOINTS": PUBLIC_ENDPOINTS,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "INTERVAL": INTERVAL,
        "SUPPORTED_INTERVALS": SUPPORTED_INTERVALS,
        "SUPPORTED_WS_INTERVALS": SUPPORTED_WS_INTERVALS,
        "ORDER_SIDE": ORDER_SIDE,
        "REVERSE_ORDER_SIDE": REVERSE_ORDER_SIDE,
        "POSITION_SIDE": POSITION_SIDE,
        "REVERSE_POSITION_SIDE": REVERSE_POSITION_SIDE,
        "ORDER_TYPE": ORDER_TYPE,
        "REVERSE_ORDER_TYPE": REVERSE_ORDER_TYPE,
        "TIME_IN_FORCE": TIME_IN_FORCE,
        "REVERSE_TIME_IN_FORCE": REVERSE_TIME_IN_FORCE,
        "ORDER_STATUS": ORDER_STATUS,
        "MARGIN_TYPE": MARGIN_TYPE,
        "REVERSE_MARGIN_TYPE": REVERSE_MARGIN_TYPE,
        "POSITION_MODE": POSITION_MODE,
        "REVERSE_POSITION_MODE": REVERSE_POSITION_MODE,
        "ORDER_TRIGGER_TYPE": ORDER_TRIGGER_TYPE,
        "REVERSE_ORDER_TRIGGER_TYPE": REVERSE_ORDER_TRIGGER_TYPE,
    });
    let expected = expected.as_object().expect("exports object");
    let actual = actual.as_object().expect("constants object");
    assert_eq!(actual.len(), expected.len(), "export_inventory");
    let inventory: Value = serde_json::from_str(include_str!("cardinality-census.json")).unwrap();
    let mut compared = 0;
    for (name, expected_value) in expected {
        assert_eq!(actual.get(name), Some(expected_value), "constant::{name}");
        if let Some(count) = inventory["types"]["constantLengths"].get(name) {
            assert_eq!(
                actual[name].as_array().unwrap().len() as u64,
                count.as_u64().unwrap(),
                "cardinality::types::{name}::native_entries"
            );
        }
        assert_eq!(
            cardinality::shape(&actual[name]),
            inventory["types"]["constantShapes"][name],
            "cardinality::types::{name}::native_shape"
        );
        compared += 1;
    }
    assert_eq!(compared, 26, "cardinality::types::compared");
}
