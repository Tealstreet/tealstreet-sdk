#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::OrderStopTriggerType;
use safe_cex_venue_blofin::utility::*;
use serde_json::{Value, json};
use std::process::Command;
fn optional(value: Option<f64>) -> Value {
    value.map_or(
        json!({"undefined":true}),
        |v| json!({"number":if v.is_nan(){"nan".into()}else{format!("{:016x}",v.to_bits())}}),
    )
}
#[test]
fn live_utility_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/utility_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_utility: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("utility", &corpus);
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
    eprintln!("utility capture header: {header}");
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        let name = case["name"].as_str().unwrap();
        for path in 0..3 {
            let input: Option<UtilityInput> = case["raw"].as_str().map(|raw| match path {
                0 => js_from_str(raw).unwrap(),
                1 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            });
            let result = match name {
                "getNormalizedInstType" => {
                    get_normalized_inst_type(input.as_ref().unwrap()).map(Value::String)
                }
                "isBlofinLinearSwapMarket" => {
                    is_blofin_linear_swap_market(input.as_ref().unwrap()).map(Value::Bool)
                }
                "isKnownNonSwapInstrument" => {
                    is_known_non_swap_instrument(input.as_ref().unwrap()).map(Value::Bool)
                }
                "parsePositivePrice" => parse_positive_price(input.as_ref()).map(optional),
                "mapBlofinTriggerType" => Ok(match map_blofin_trigger_type(input.as_ref()) {
                    Some(OrderStopTriggerType::LastPrice) => json!("last_price"),
                    Some(OrderStopTriggerType::MarkPrice) => json!("mark_price"),
                    Some(OrderStopTriggerType::IndexPrice) => json!("index_price"),
                    None => json!({"undefined":true}),
                }),
                "toBlofinTriggerType" => Ok(json!(to_blofin_trigger_type(input.as_ref()))),
                "isAlreadyGoneCancelRow" => {
                    is_already_gone_cancel_row(input.as_ref()).map(Value::Bool)
                }
                "isAlgoOrder" => Ok(json!(is_algo_order(input.as_ref()))),
                _ => unreachable!(),
            };
            let actual = match result {
                Ok(v) => json!({"ok":v}),
                Err(e) => json!({"error":e.name,"message":e.message}),
            };
            assert_eq!(
                actual, case["result"],
                "utility_{name}_native case={i} path={path} raw={}",
                case["raw"]
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
            js_from_str::<UtilityInput>(raw.as_str().unwrap()).is_err(),
            "utility_ingress_boundary"
        );
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
}
