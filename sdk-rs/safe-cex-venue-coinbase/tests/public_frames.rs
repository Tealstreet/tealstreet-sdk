use safe_cex_venue_coinbase::starbase::is_record;
use safe_cex_venue_coinbase::starbase_public::{has_exact_keys, is_supported_public_channel};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/public-frames.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn cases_matching(predicate: impl Fn(&Value) -> bool, expected: usize) -> Vec<&'static Value> {
    let all = oracle().as_array().expect("public-frame oracle array");
    assert_eq!(all.len(), 66, "public-frame source corpus total");
    let selected = all
        .iter()
        .filter(|case| predicate(case))
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), expected, "public-frame selected case count");
    selected
}

fn expected_channel_inputs() -> Vec<String> {
    let mut inputs = vec![
        "".to_owned(),
        "ticker".to_owned(),
        "book".to_owned(),
        "chart.trades".to_owned(),
        "instrument.state.future".to_owned(),
        "constructor".to_owned(),
        "__proto__".to_owned(),
        "ticker.SOL_USDC-PERPETUAL.100ms".to_owned(),
        "ticker.BTC_USDC-PERPETUAL.200ms".to_owned(),
        "book.ETH_USDC-PERPETUAL.agg3".to_owned(),
        "chart.trades.BTC_USDC-PERPETUAL.2".to_owned(),
        "instrument.state.future.toString".to_owned(),
        "instrument.state.future.BTC.extra".to_owned(),
        "ticker. BTC_USDC-PERPETUAL.100ms".to_owned(),
        "ticker.BTC_USDC-PERPETUAL.\u{feff}100ms".to_owned(),
        "ticker.BTC_USDC-PERPETUAL.100ms\u{0085}".to_owned(),
    ];
    for instrument in ["BTC_USDC-PERPETUAL", "ETH_USDC-PERPETUAL"] {
        for category in ["ticker", "book"] {
            for interval in ["100ms", "agg2"] {
                inputs.push(format!("{category}.{instrument}.{interval}"));
            }
        }
        for resolution in ["1", "3", "5", "15", "30", "60", "120", "360", "720", "1D"] {
            inputs.push(format!("chart.trades.{instrument}.{resolution}"));
        }
    }
    for currency in ["BTC", "ETH", "USDC", "any"] {
        inputs.push(format!("instrument.state.future.{currency}"));
    }
    assert_eq!(inputs.len(), 48, "independent channel inventory total");
    inputs
}

fn assert_channel_inventory(cases: &[&Value]) {
    let expected = expected_channel_inputs();
    assert_eq!(cases.len(), expected.len(), "channel corpus cardinality");
    for (case, input) in cases.iter().zip(expected) {
        assert_eq!(case["input"], input, "channel corpus case identity");
    }
}

#[test]
fn supported_public_channel_matches_live_source() {
    let cases = cases_matching(
        |case| case.get("kind").is_none() && case.get("input").is_some(),
        48,
    );
    assert_channel_inventory(&cases);
    for case in cases {
        assert_eq!(
            is_supported_public_channel(case["input"].as_str().unwrap()),
            case["result"].as_bool().unwrap(),
            "channel {:?}",
            case["input"]
        );
    }
}

#[test]
fn exact_public_frame_keys_match_live_source() {
    let cases = cases_matching(|case| case.get("label").is_some(), 7);
    let expected = [
        ("ordinary", json!({"a":1,"b":2}), json!(["a", "b"])),
        ("different-order", json!({"b":2,"a":1}), json!(["a", "b"])),
        (
            "numeric-index-keys",
            json!({"2":"two","10":"ten","name":"value"}),
            json!(["name", "2", "10"]),
        ),
        (
            "prototype-named-own-keys",
            json!({"__proto__":"own","constructor":"own"}),
            json!(["__proto__", "constructor"]),
        ),
        ("extra-key", json!({"a":1,"b":2,"c":3}), json!(["a", "b"])),
        ("missing-key", json!({"a":1}), json!(["a", "b"])),
        ("wrong-same-size", json!({"a":1,"c":3}), json!(["a", "b"])),
    ];
    assert_eq!(cases.len(), expected.len(), "exact-keys corpus total");
    for (case, (label, value, keys)) in cases.iter().zip(expected) {
        assert_eq!(case["label"], label, "exact-keys case identity");
        assert_eq!(case["value"], value, "exact-keys input identity");
        assert_eq!(case["expected"], keys, "exact-keys key inventory");
    }
    for case in cases {
        let value = case["value"].as_object().unwrap();
        let expected = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|key| key.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            has_exact_keys(value, &expected),
            case["result"].as_bool().unwrap(),
            "{} keys",
            case["label"]
        );
    }
}

#[test]
fn public_frame_record_predicate_matches_live_source() {
    let cases = cases_matching(|case| case["kind"] == "record", 11);
    let expected = [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!(true)),
        Some(json!(0)),
        Some(json!("")),
        Some(json!("x")),
        Some(json!([])),
        Some(json!([1])),
        Some(json!({})),
        Some(json!({"x":1})),
    ];
    assert_eq!(cases.len(), expected.len(), "record predicate corpus total");
    for (case, expected_input) in cases.iter().zip(expected) {
        assert_eq!(
            case.get("input"),
            expected_input.as_ref(),
            "record case identity"
        );
    }
    for case in cases {
        assert_eq!(
            is_record(case.get("input")),
            case["result"].as_bool().unwrap(),
            "record {:?}",
            case.get("input")
        );
    }
}
