#[path = "support/depth_corpus.rs"]
mod depth_corpus;
use depth_corpus::DepthCorpus;
use safe_cex_webworker::task_types::{ChaserOrderOptions, TwapOptions, TwapState};
use serde_json::{Value, json};
fn sample(name: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_types_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    source["records"][name]["sample"].clone()
}
fn owned_zero(name: &str) -> Value {
    let mut value = sample(name);
    value["size"] = json!("NUMBER_PROBE");
    serde_json::from_str(
        &serde_json::to_string(&value)
            .unwrap()
            .replace("\"NUMBER_PROBE\"", "-0"),
    )
    .unwrap()
}
#[test]
fn chaser_options_owned_integer_zero_is_preserved() {
    let value = owned_zero("ChaserOrderOptions");
    assert_eq!(
        value["size"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    let output: ChaserOrderOptions = serde_json::from_value(value).unwrap();
    assert_eq!(
        output.size.to_bits(),
        (-0.0_f64).to_bits(),
        "REPAIRED CHASER OPTIONS OWNED INTEGER ZERO"
    );
}
#[test]
fn twap_options_owned_integer_zero_is_preserved() {
    let value = owned_zero("TwapOptions");
    assert_eq!(
        value["size"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    let output: TwapOptions = serde_json::from_value(value).unwrap();
    assert_eq!(
        output.size.to_bits(),
        (-0.0_f64).to_bits(),
        "REPAIRED TWAP OPTIONS OWNED INTEGER ZERO"
    );
}
#[test]
fn twap_state_owned_integer_zero_is_preserved() {
    let value = owned_zero("TwapState");
    assert_eq!(
        value["size"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    let output: TwapState = serde_json::from_value(value).unwrap();
    assert_eq!(
        output.size.to_bits(),
        (-0.0_f64).to_bits(),
        "REPAIRED TWAP STATE OWNED INTEGER ZERO"
    );
}

use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::de::DeserializeOwned;
fn source_graph() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/task_domain_numeric_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn routes<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(
    raw: &str,
    value: &Value,
    native: &Value,
) -> Vec<(&'static str, Result<T, serde_json::Error>)> {
    vec![
        ("bare raw", serde_json::from_str(raw)),
        ("bare slice", serde_json::from_slice(raw.as_bytes())),
        ("bare owned", serde_json::from_value(value.clone())),
        ("bare borrowed", T::deserialize(value)),
        ("shared raw", js_from_str(raw)),
        ("shared owned", js_from_value(value.clone())),
        ("shared borrowed", js_from_value_ref(value)),
        ("native owned", serde_json::from_value(native.clone())),
        ("native borrowed", T::deserialize(native)),
    ]
}
fn matrix<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(
    source: &Value,
    name: &str,
    read: impl Fn(&T, &str) -> f64,
) -> usize {
    let record = &source["records"][name];
    let mut comparisons = 0;
    for field in record["fields"].as_array().unwrap() {
        let field = field.as_str().unwrap();
        for (token, bits) in source["numbers"].as_object().unwrap() {
            let expected = u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap();
            let mut sample = record["sample"].clone();
            *sample.pointer_mut(field).unwrap() = json!("NUMERIC_PROBE");
            let raw = serde_json::to_string(&sample)
                .unwrap()
                .replace("\"NUMERIC_PROBE\"", token);
            let value: Value = serde_json::from_str(&raw).unwrap();
            let mut native = value.clone();
            if expected == (-0.0_f64).to_bits() {
                *native.pointer_mut(field).unwrap() = json!(-0.0);
            }
            for (route, result) in routes::<T>(&raw, &value, &native) {
                assert!(
                    result.is_ok(),
                    "TASK-DOMAIN ACCEPTANCE {name} {field} {token} {route}: {}",
                    result.err().map(|e| e.to_string()).unwrap_or_default()
                );
                assert_eq!(
                    read(&result.unwrap(), field).to_bits(),
                    expected,
                    "TASK-DOMAIN BITS {name} {field} {token} {route}"
                );
                comparisons += 1;
            }
        }
    }
    comparisons
}
#[test]
fn complete_task_graphs_match_source_numeric_bits() {
    let source = source_graph();
    let mut comparisons = 0;
    comparisons +=
        matrix::<ChaserOrderOptions>(&source, "ChaserOrderOptions", |v, path| match path {
            "/size" => v.size,
            "/maxAttempts" => v.max_attempts,
            "/delay" => v.delay,
            "/terminate/value" => v.terminate.as_ref().unwrap().value,
            "/terminate/anchorPrice" => v.terminate.as_ref().unwrap().anchor_price,
            _ => panic!("TASK-DOMAIN uncovered source path {path}"),
        });
    comparisons += matrix::<TwapOptions>(&source, "TwapOptions", |v, path| match path {
        "/activeMarket/precision/amount" => v.active_market.precision.amount,
        "/activeMarket/precision/price" => v.active_market.precision.price,
        "/activeMarket/limits/amount/min" => v.active_market.limits.amount.min,
        "/activeMarket/limits/amount/max" => {
            safe_cex_num::numeric_value_to_number(&v.active_market.limits.amount.max)
        }
        "/activeMarket/limits/leverage/min" => v.active_market.limits.leverage.min,
        "/activeMarket/limits/leverage/max" => v.active_market.limits.leverage.max,
        "/activeMarket/limits/price/min" => v.active_market.limits.price.as_ref().unwrap().min,
        "/activeMarket/option/expiry" => v.active_market.option.as_ref().unwrap().expiry,
        "/activeMarket/option/strike" => v.active_market.option.as_ref().unwrap().strike,
        "/activeMarket/contractSize" => v.active_market.contract_size.unwrap(),
        "/activeMarket/info/nested/0" => v
            .active_market
            .info
            .as_ref()
            .unwrap()
            .get("nested")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_f64()
            .unwrap(),
        "/activeMarket/takerFee" => v.active_market.taker_fee.unwrap(),
        "/activeMarket/makerFee" => v.active_market.maker_fee.unwrap(),
        "/size" => v.size,
        "/duration" => v.duration,
        "/ordersCount" => v.orders_count,
        "/chaserDelay" => v.chaser_delay,
        "/randomnessPercentage" => v.randomness_percentage,
        "/timeoutPct" => v.timeout_pct.unwrap(),
        _ => panic!("TASK-DOMAIN uncovered source path {path}"),
    });
    comparisons += matrix::<TwapState>(&source, "TwapState", |v, path| match path {
        "/activeMarket/precision/amount" => v.active_market.precision.amount,
        "/activeMarket/precision/price" => v.active_market.precision.price,
        "/activeMarket/limits/amount/min" => v.active_market.limits.amount.min,
        "/activeMarket/limits/amount/max" => {
            safe_cex_num::numeric_value_to_number(&v.active_market.limits.amount.max)
        }
        "/activeMarket/limits/leverage/min" => v.active_market.limits.leverage.min,
        "/activeMarket/limits/leverage/max" => v.active_market.limits.leverage.max,
        "/activeMarket/limits/price/min" => v.active_market.limits.price.as_ref().unwrap().min,
        "/activeMarket/option/expiry" => v.active_market.option.as_ref().unwrap().expiry,
        "/activeMarket/option/strike" => v.active_market.option.as_ref().unwrap().strike,
        "/activeMarket/contractSize" => v.active_market.contract_size.unwrap(),
        "/activeMarket/info/nested/0" => v
            .active_market
            .info
            .as_ref()
            .unwrap()
            .get("nested")
            .unwrap()
            .as_array()
            .unwrap()[0]
            .as_f64()
            .unwrap(),
        "/activeMarket/takerFee" => v.active_market.taker_fee.unwrap(),
        "/activeMarket/makerFee" => v.active_market.maker_fee.unwrap(),
        "/size" => v.size,
        "/ordersSizes/0" => v.orders_sizes[0],
        "/ordersCount" => v.orders_count,
        "/ordersExecuted" => v.orders_executed,
        "/nextOrderAt" => v.next_order_at,
        "/createdAt" => v.created_at.unwrap(),
        "/startedAt" => v.started_at.unwrap(),
        _ => panic!("TASK-DOMAIN uncovered source path {path}"),
    });
    assert_eq!(comparisons, 3960, "TASK-DOMAIN comparison inventory");
}

#[test]
fn retained_market_info_preserves_presence_and_nested_numbers() {
    let source = source_graph();
    macro_rules! check {
        ($ty:ty) => {
            for present_null in [false, true] {
                let mut value = source["records"][stringify!($ty)]["sample"].clone();
                if present_null {
                    value["activeMarket"]["info"] = Value::Null;
                } else {
                    value["activeMarket"]
                        .as_object_mut()
                        .unwrap()
                        .remove("info");
                }
                let raw = serde_json::to_string(&value).unwrap();
                for (route, result) in routes::<$ty>(&raw, &value, &value) {
                    let result = result.unwrap();
                    assert_eq!(
                        result.active_market.info,
                        if present_null {
                            Some(safe_cex_types::retained_json::RetainedJsonInput::Null)
                        } else {
                            None
                        },
                        "TASK-DOMAIN retained info presence {} {route}",
                        stringify!($ty)
                    );
                }
            }
        };
    }
    check!(TwapOptions);
    check!(TwapState);
}
#[test]
fn complete_task_graph_depth_boundary() {
    let source = source_graph();
    fn check<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(
        source: &Value,
        name: &str,
        corpus: &mut DepthCorpus,
    ) {
        for depth in [140, 20] {
            let mut value = source["records"][name]["sample"].clone();
            let mut nested = json!(1);
            for _ in 0..depth {
                nested = json!([nested]);
            }
            value["ignored"] = nested;
            let raw = serde_json::to_string(&value).unwrap();
            corpus.record(&value, "/ignored", depth);
            for (route, result) in routes::<T>(&raw, &value, &value) {
                if depth == 20 {
                    assert!(result.is_ok(), "TASK-DOMAIN ordinary depth {name} {route}");
                } else {
                    assert!(
                        result.is_err(),
                        "TASK-DOMAIN shared depth refusal {name} {route}"
                    );
                    assert!(
                        result
                            .err()
                            .unwrap()
                            .to_string()
                            .contains("recursion limit")
                    );
                }
            }
        }
    }
    let mut corpus = DepthCorpus::default();
    check::<ChaserOrderOptions>(&source, "ChaserOrderOptions", &mut corpus);
    check::<TwapOptions>(&source, "TwapOptions", &mut corpus);
    check::<TwapState>(&source, "TwapState", &mut corpus);
    corpus.verify();
}

#[test]
fn task_domain_source_inventory_fails_closed_on_new_shapes() {
    for (flag, message) in [
        (
            "--inherited-number",
            "TASK-DOMAIN unsupported interface heritage",
        ),
        (
            "--unsupported-type",
            "TASK-DOMAIN unsupported shape unknown",
        ),
        (
            "--extra-number",
            "TASK-DOMAIN complete numeric graph inventory",
        ),
    ] {
        let output = std::process::Command::new("node")
            .args([
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/task_domain_numeric_oracle.mjs"
                ),
                flag,
            ])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "TASK-DOMAIN source control survived {flag}"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(message),
            "TASK-DOMAIN source control wrong failure {flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
