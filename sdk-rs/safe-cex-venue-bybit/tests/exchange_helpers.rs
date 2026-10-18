use safe_cex_venue_bybit::{
    exchange_errors::string_from_unknown,
    exchange_helpers::*,
    order_format::{get_bybit_trigger_by, get_bybit_trigger_direction_price},
};
use serde_json::{Map, Value};
fn number(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "HELPER native {label}"
    );
}
fn optional(actual: Option<f64>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "HELPER native absent {label}");
    } else {
        number(actual.unwrap(), expected, label);
    }
}
fn string(actual: Option<&str>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "HELPER native absent {label}");
    } else {
        assert_eq!(actual, expected.as_str(), "HELPER native string {label}");
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn exchange_helpers_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-helper-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let scalars = fixture["scalars"].as_array().unwrap();
    assert_eq!(scalars.len(), 319, "HELPER independent scalar floor");
    let mut compared = 0;
    let mut families = [0usize; 3];
    for case in scalars {
        let family = case["family"].as_str().unwrap();
        families[match family {
            "linear" => 0,
            "inverse" => 1,
            "spot" => 2,
            _ => panic!("unexpected family"),
        }] += 1;
        let raw = case["raw"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let label = case["name"].as_str().unwrap();
        let expected = &case["result"];
        match label {
            "stringOrUndefined" => {
                string(string_or_undefined(raw.as_ref()), &expected["ok"], label)
            }
            "numberOrUndefined" => {
                optional(number_or_undefined(raw.as_ref()), &expected["ok"], label)
            }
            "stringFromUnknown" => string(
                string_from_unknown(raw.as_ref().unwrap_or(&Value::Null)),
                &expected["ok"],
                label,
            ),
            "isBybitPositionTpslMode" => assert_eq!(
                is_bybit_position_tpsl_mode(raw.as_ref()),
                expected["ok"].as_bool().unwrap(),
                "HELPER native mode"
            ),
            "parseBybitFiniteNumber" => {
                let actual = parse_bybit_finite_number(raw.as_ref());
                if let Some(message) = expected["message"].as_str() {
                    assert_eq!(expected["error"], "TypeError");
                    assert_eq!(
                        actual.err().as_deref(),
                        Some(message),
                        "HELPER native parse error"
                    );
                } else {
                    optional(actual.unwrap(), &expected["ok"], label);
                }
            }
            "isBybitOrderAlreadyGoneCancelMessage" | "isBybitOrderNotModifiedMessage" => {
                let actual = if label == "isBybitOrderAlreadyGoneCancelMessage" {
                    is_bybit_order_already_gone_cancel_message(raw.as_ref())
                } else {
                    is_bybit_order_not_modified_message(raw.as_ref())
                };
                if let Some(message) = expected["message"].as_str() {
                    assert_eq!(expected["error"], "TypeError");
                    assert_eq!(
                        actual.err().as_deref(),
                        Some(message),
                        "HELPER native message error"
                    );
                } else {
                    assert_eq!(
                        actual.unwrap(),
                        expected["ok"].as_bool().unwrap(),
                        "HELPER native message predicate"
                    );
                }
            }
            _ => panic!("unexpected source helper {label}"),
        }
        compared += 1;
    }
    assert_eq!(families, [203, 87, 29], "HELPER source family inventory");
    assert_eq!(compared, 319, "HELPER scalar compared floor");
    let coins = fixture["coins"].as_array().unwrap();
    assert_eq!(coins.len(), 16, "HELPER independent coin floor");
    let mut coin_compared = 0;
    for case in coins {
        let rows = serde_json::from_value::<Vec<Map<String, Value>>>(case["rows"].clone()).unwrap();
        let label = case["name"].as_str().unwrap();
        let actual = match label {
            "sumBybitCoinFields" => sum_bybit_coin_fields(&rows, &["a", "b"]),
            "deriveBybitAvailableBalanceFromCoins" => {
                derive_bybit_available_balance_from_coins(&rows)
            }
            _ => panic!("unexpected coin helper"),
        };
        let expected = &case["result"];
        if let Some(message) = expected["message"].as_str() {
            assert_eq!(expected["error"], "TypeError");
            assert_eq!(
                actual.err().as_deref(),
                Some(message),
                "HELPER native coin error"
            );
        } else {
            optional(actual.unwrap(), &expected["ok"], label);
        }
        coin_compared += 1;
    }
    assert_eq!(coin_compared, 16, "HELPER coin comparisons");
    let first = fixture["first"].as_array().unwrap();
    assert_eq!(first.len(), 6, "HELPER first finite floor");
    for case in first {
        let values = case["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let text = v.as_str().unwrap();
                if text == "undefined" {
                    None
                } else {
                    Some(text.parse::<f64>().unwrap())
                }
            })
            .collect::<Vec<_>>();
        number(
            first_finite_number(&values),
            &case["result"],
            "firstFiniteNumber",
        );
    }
    let triggers = fixture["triggers"].as_array().unwrap();
    assert_eq!(triggers.len(), 24, "HELPER inverse trigger floor");
    let mut trigger_compared = 0;
    for case in triggers {
        let opts =
            serde_json::from_value::<safe_cex_types::PlaceOrderOpts>(case["opts"].clone()).unwrap();
        let ticker =
            serde_json::from_value::<safe_cex_types::Ticker>(case["ticker"].clone()).unwrap();
        // Distinct native axes must be present before invoking either selector.
        assert_eq!(
            (ticker.last, ticker.mark, ticker.index.unwrap()),
            (17.25, 23.5, 31.75),
            "HELPER decoded native ticker axes"
        );
        string(
            get_bybit_trigger_by(&opts),
            &case["by"],
            "inverse triggerBy",
        );
        number(
            get_bybit_trigger_direction_price(&opts, &ticker),
            &case["price"],
            "inverse trigger direction",
        );
        trigger_compared += 2;
    }
    assert_eq!(trigger_compared, 48, "HELPER inverse selector comparisons");
    assert_eq!(
        fixture["boundary"]["sourceSum"]["undefined"], true,
        "HELPER null record source empty-fields witness"
    );
    assert!(
        serde_json::from_value::<Vec<Map<String, Value>>>(fixture["boundary"]["rows"].clone())
            .is_err(),
        "HELPER typed record input refusal"
    );
}
