#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::*;
use safe_cex_venue_blofin::order_history::*;
use serde_json::{Value, json};
use std::process::Command;
fn bits(value: f64) -> Value {
    json!({"number": if value.is_nan() { "nan".into() } else { format!("{:016x}", value.to_bits()) }})
}
fn optional(value: Option<f64>) -> Value {
    value.map_or(json!({"undefined":true}), bits)
}
fn native(order: HistoricalOrder) -> Value {
    // Check native variants and fields; serde names are not the oracle.
    let side = match order.side {
        OrderSide::Buy => "buy",
        OrderSide::Sell => "sell",
    };
    let ty = match order.r#type {
        OrderType::Limit => "limit",
        OrderType::Market => "market",
        OrderType::Conditional => "conditional",
        other => panic!("native_history_type {other:?}"),
    };
    let status = match order.status {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        other => panic!("native_history_status {other:?}"),
    };
    let position = match order.position_side.unwrap() {
        ExecutionPositionSide::Side(PositionSide::Long) => "long",
        ExecutionPositionSide::Side(PositionSide::Short) => "short",
        ExecutionPositionSide::Both(BothPositionSide::Both) => "both",
    };
    assert!(
        order.realized_pnl.is_none()
            && order.fees.is_none()
            && order.fee_asset.is_none()
            && order.rebate.is_none()
            && order.rebate_asset.is_none(),
        "history_extra_fields_absent"
    );
    json!({"id":order.id,"symbol":order.symbol,"side":side,"type":ty,"status":status,
        "price":optional(order.price),"averagePrice":optional(order.average_price),
        "amount":bits(order.amount),"filled":bits(order.filled),"reduceOnly":order.reduce_only.unwrap(),
        "positionSide":position,"createdTime":bits(order.created_time),"updatedTime":optional(order.updated_time)})
}
#[test]
fn live_history_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/history_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_history: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("history", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("history capture header: {header}");
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str();
            let input: Option<HistoryInput> = raw.map(|raw| match path {
                0 => js_from_str(raw).unwrap(),
                1 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let mut trace = Vec::new();
            let result = match case["op"].as_str().unwrap() {
                "number" => to_number(input.as_ref())
                    .map(bits)
                    .map_err(safe_cex_exchange::runtime::ExchangeError::from),
                "optional" => to_optional_number(input.as_ref())
                    .map(optional)
                    .map_err(safe_cex_exchange::runtime::ExchangeError::from),
                "order" => map_blofin_historical_order(input.as_ref().unwrap(), |id| {
                    trace.push(id.to_owned());
                    Ok(case["resolved"].as_str().map(str::to_owned))
                })
                .map(|v| v.map_or(Value::Null, native))
                .map_err(safe_cex_exchange::runtime::ExchangeError::from),
                _ => unreachable!(),
            };
            if case["inherited"].is_object() {
                let field = case["inherited"]["field"].as_str().unwrap();
                assert_eq!(
                    case["result"]["ok"][field],
                    json!({"inherited":case["inherited"]["name"]}),
                    "source_history_inherited_identity"
                );
                assert!(result.is_err(), "history_inherited_refusal");
                assert_eq!(
                    result.unwrap_err().name,
                    "RepresentationError",
                    "history_inherited_refusal"
                );
            } else {
                let actual = match result {
                    Ok(v) => json!({"ok":v}),
                    Err(e) => json!({"error":e.name,"message":e.message}),
                };
                let assertion = match case["op"].as_str().unwrap() {
                    "number" => "history_to_number_native",
                    "optional" => "history_optional_native",
                    _ => "history_order_native",
                };
                assert_eq!(
                    actual, case["result"],
                    "{assertion} case={i} path={path} raw={raw:?}"
                );
            }
            assert_eq!(
                json!(trace),
                case["trace"],
                "history_callback_order case={i}"
            );
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    for index in 0..pin.count("/boundaries") {
        let raw = &corpus["boundaries"][index];
        assert!(
            js_from_str::<HistoryInput>(raw.as_str().unwrap()).is_err(),
            "history_ingress_boundary"
        );
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
}
