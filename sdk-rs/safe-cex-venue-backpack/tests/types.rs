#![allow(non_snake_case)] // Source type names keep native-field witnesses mechanically traceable.
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_backpack::types::*;
use serde_json::{Value, json};
use std::process::Command;
include!("type_probes.rs.inc");
include!("cardinality.inc");

// Independent source-field inventory (backpack.types.ts), deliberately not
// generated from the live oracle. Keep the one-hot supplement that detects
// symmetric serde Boolean swaps, even when corpus cardinality stays unchanged.
const BOOLEAN_FIELDS: &[(&str, &[&str])] = &[
    ("BackpackAsset", &["depositEnabled", "withdrawEnabled"]),
    ("BackpackMarket", &["visible"]),
    ("BackpackTrade", &["isBuyerMaker"]),
    ("BackpackOrder", &["reduceOnly"]),
    ("BackpackOrderRequest", &["postOnly", "reduceOnly"]),
    ("BackpackFill", &["isMaker"]),
    ("BackpackWsTradeUpdate", &["m"]),
    ("BackpackWsKlineUpdate", &["X"]),
    ("BackpackWsOrderUpdate", &["r", "m"]),
    (
        "BackpackAccount",
        &[
            "autoBorrowSettlements",
            "autoLend",
            "autoRealizePnl",
            "autoRepayBorrows",
            "liquidating",
        ],
    ),
];

fn assert_one_hot(name: &str, label: &str, native: &Value) {
    let active = label.strip_prefix("boolean ").unwrap();
    let fields = BOOLEAN_FIELDS
        .iter()
        .find(|(record, _)| *record == name)
        .expect("independently enumerated Boolean record")
        .1;
    assert!(
        fields.contains(&active),
        "TYPES-CONTENT independent Boolean field {name} {active}"
    );
    for field in fields {
        assert_eq!(
            native[*field],
            *field == active,
            "TYPES-CONTENT independent one-hot {name} {active} field {field}"
        );
    }
}

// Native fields/bits are captured independently before the deliberate wire projection.
fn wire<T: serde::Serialize>(v: &T) -> String {
    safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(
        serde_json::to_value(v).unwrap(),
    ))
    .unwrap()
    .unwrap()
}

fn decode(name: &str, raw: &str, route: usize) -> Result<(Value, String), serde_json::Error> {
    match name {
        "BackpackApiResponseRetained" => {
            use safe_cex_num::numeric_ingress::JsNumberInput;
            let v: BackpackApiResponse<JsNumberInput> = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            let data: BackpackTime = v.data.clone().expect("present retained data").decode()?;
            Ok((json!({"data": probe_BackpackTime(&data)}), wire(&v)))
        }
        "BackpackAsset" => {
            let v: BackpackAsset = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackAsset(&v), wire(&v)))
        }
        "BackpackMarket" => {
            let v: BackpackMarket = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackMarket(&v), wire(&v)))
        }
        "BackpackTicker" => {
            let v: BackpackTicker = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackTicker(&v), wire(&v)))
        }
        "BackpackDepth" => {
            let v: BackpackDepth = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackDepth(&v), wire(&v)))
        }
        "BackpackTrade" => {
            let v: BackpackTrade = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackTrade(&v), wire(&v)))
        }
        "BackpackKline" => {
            let v: BackpackKline = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackKline(&v), wire(&v)))
        }
        "BackpackMarkPrice" => {
            let v: BackpackMarkPrice = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackMarkPrice(&v), wire(&v)))
        }
        "BackpackBalance" => {
            let v: BackpackBalance = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackBalance(&v), wire(&v)))
        }
        "BackpackOrder" => {
            let v: BackpackOrder = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackOrder(&v), wire(&v)))
        }
        "BackpackOrderRequest" => {
            let v: BackpackOrderRequest = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackOrderRequest(&v), wire(&v)))
        }
        "BackpackFill" => {
            let v: BackpackFill = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackFill(&v), wire(&v)))
        }
        "BackpackCollateral" => {
            let v: BackpackCollateral = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackCollateral(&v), wire(&v)))
        }
        "BackpackPosition" => {
            let v: BackpackPosition = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackPosition(&v), wire(&v)))
        }
        "BackpackWsSubscribeMessage" => {
            let v: BackpackWsSubscribeMessage = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsSubscribeMessage(&v), wire(&v)))
        }
        "BackpackWsUnsubscribeMessage" => {
            let v: BackpackWsUnsubscribeMessage = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsUnsubscribeMessage(&v), wire(&v)))
        }
        "BackpackWsSignature" => {
            let v: BackpackWsSignature = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsSignature(&v), wire(&v)))
        }
        "BackpackWsTickerUpdate" => {
            let v: BackpackWsTickerUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsTickerUpdate(&v), wire(&v)))
        }
        "BackpackWsDepthUpdate" => {
            let v: BackpackWsDepthUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsDepthUpdate(&v), wire(&v)))
        }
        "BackpackWsTradeUpdate" => {
            let v: BackpackWsTradeUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsTradeUpdate(&v), wire(&v)))
        }
        "BackpackWsKlineUpdate" => {
            let v: BackpackWsKlineUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsKlineUpdate(&v), wire(&v)))
        }
        "BackpackWsBookTickerUpdate" => {
            let v: BackpackWsBookTickerUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsBookTickerUpdate(&v), wire(&v)))
        }
        "BackpackWsOrderUpdate" => {
            let v: BackpackWsOrderUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsOrderUpdate(&v), wire(&v)))
        }
        "BackpackWsPositionUpdate" => {
            let v: BackpackWsPositionUpdate = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackWsPositionUpdate(&v), wire(&v)))
        }
        "BackpackError" => {
            let v: BackpackError = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackError(&v), wire(&v)))
        }
        "BackpackApiResponse" => {
            let v: BackpackApiResponse<f64> = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackApiResponse(&v), wire(&v)))
        }
        "BackpackTime" => {
            let v: BackpackTime = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackTime(&v), wire(&v)))
        }
        "BackpackStatus" => {
            let v: BackpackStatus = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackStatus(&v), wire(&v)))
        }
        "BackpackAccount" => {
            let v: BackpackAccount = match route {
                0 => js_from_str(raw)?,
                1 => js_from_value(serde_json::from_str(raw)?)?,
                _ => js_from_value_ref(&serde_json::from_str(raw)?)?,
            };
            Ok((probe_BackpackAccount(&v), wire(&v)))
        }
        _ => panic!("unregistered source interface {name}"),
    }
}

#[test]
fn actual_typescript_records_match_decoded_fields_and_bits() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/types_oracle.cjs"
        ))
        .output()
        .expect("execute actual TypeScript type witness");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).expect("oracle output");
    assert_eq!(oracle["interfaces"], 28, "source interface census");
    assert_eq!(
        oracle["cases"].as_array().unwrap().len(),
        1545,
        "TYPES-CARDINALITY source cases"
    );
    let one_hot: Vec<_> = oracle["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["label"].as_str().unwrap().starts_with("boolean "))
        .collect();
    assert_eq!(
        one_hot.len(),
        17,
        "TYPES-CONTENT independent Boolean supplement size"
    );
    for (name, fields) in BOOLEAN_FIELDS {
        for field in *fields {
            let label = format!("boolean {field}");
            let found: Vec<_> = one_hot
                .iter()
                .filter(|case| case["name"] == *name && case["label"] == label)
                .collect();
            assert_eq!(
                found.len(),
                1,
                "TYPES-CONTENT independent supplement identity {name} {field}"
            );
            let case = found[0];
            assert_eq!(
                case["accept"], true,
                "TYPES-CONTENT supplement admission {name} {field}"
            );
            assert_one_hot(
                name,
                &label,
                &serde_json::from_str::<Value>(case["raw"].as_str().unwrap()).unwrap(),
            );
            assert_one_hot(name, &label, &case["native"]);
        }
    }
    for (name, accepted, refused) in [
        ("BackpackAccount", 16, 43),
        ("BackpackApiResponse", 11, 9),
        ("BackpackApiResponseRetained", 4, 0),
        ("BackpackAsset", 5, 25),
        ("BackpackBalance", 3, 15),
        ("BackpackCollateral", 3, 125),
        ("BackpackDepth", 7, 23),
        ("BackpackError", 7, 7),
        ("BackpackFill", 14, 44),
        ("BackpackKline", 3, 37),
        ("BackpackMarkPrice", 7, 19),
        ("BackpackMarket", 14, 119),
        ("BackpackOrder", 33, 130),
        ("BackpackOrderRequest", 30, 128),
        ("BackpackPosition", 7, 70),
        ("BackpackStatus", 5, 14),
        ("BackpackTicker", 7, 39),
        ("BackpackTime", 7, 3),
        ("BackpackTrade", 12, 19),
        ("BackpackWsBookTickerUpdate", 8, 29),
        ("BackpackWsDepthUpdate", 20, 36),
        ("BackpackWsKlineUpdate", 13, 43),
        ("BackpackWsOrderUpdate", 19, 75),
        ("BackpackWsPositionUpdate", 15, 53),
        ("BackpackWsSignature", 11, 13),
        ("BackpackWsSubscribeMessage", 12, 26),
        ("BackpackWsTickerUpdate", 8, 45),
        ("BackpackWsTradeUpdate", 13, 25),
        ("BackpackWsUnsubscribeMessage", 4, 13),
    ] {
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(
            cases
                .iter()
                .filter(|c| c["name"] == name && c["accept"] == true)
                .count(),
            accepted,
            "TYPES-CARDINALITY source accepted {name}"
        );
        assert_eq!(
            cases
                .iter()
                .filter(|c| c["name"] == name && c["accept"] == false)
                .count(),
            refused,
            "TYPES-CARDINALITY source refused {name}"
        );
    }
    let mut accepted_comparisons = [0; 3];
    let mut refused_comparisons = [0; 3];
    for key in ["node", "arch", "platform"] {
        assert!(
            !oracle["runtime"][key]
                .as_str()
                .expect("runtime metadata")
                .is_empty()
        );
    }
    eprintln!("TypeScript oracle runtime: {}", oracle["runtime"]);
    // Exercise reusable source records before their containing records, so a
    // per-record control reports that record rather than its first consumer.
    // Keep every case and all three ingress routes; this is not a case filter.
    let mut cases: Vec<_> = oracle["cases"].as_array().unwrap().iter().collect();
    cases.sort_by_key(|case| match case["name"].as_str().unwrap() {
        "BackpackWsSignature" | "BackpackError" | "BackpackTime" => 0,
        _ => 1,
    });
    let mut wire_checks = Vec::new();
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        let label = case["label"].as_str().unwrap();
        for route in 0..3 {
            let result = decode(name, raw, route);
            if case["accept"] == true {
                let (native, wire) =
                    result.unwrap_or_else(|e| panic!("{name} {label} route {route}: {e}"));
                assert_native_cardinality(
                    &case["native"],
                    &native,
                    &format!("{name} {label} route {route}"),
                );
                assert_eq!(
                    native, case["native"],
                    "decoded native fields/bits {name} {label} route {route}"
                );
                if label.starts_with("boolean ") {
                    assert_one_hot(name, label, &native);
                }
                accepted_comparisons[route] += 1;
                wire_checks.push((wire, case["wire"].as_str().unwrap(), name, label, route));
            } else {
                if route == 0 && !case["lone"].as_array().unwrap().is_empty() {
                    let (native, _) = result.expect("typed String retains the record");
                    typed_utf16::assert_projection(raw, &native);
                } else {
                    assert!(
                        result.is_err(),
                        "typed/Value refusal {name} {label} route {route}"
                    );
                }
                refused_comparisons[route] += 1;
            }
        }
    }
    assert_eq!(
        accepted_comparisons, [318; 3],
        "TYPES-CARDINALITY completed accepted comparisons"
    );
    assert_eq!(
        refused_comparisons, [1227; 3],
        "TYPES-CARDINALITY completed refusal comparisons"
    );
    assert_eq!(wire_checks.len(), 954, "TYPES-CARDINALITY wire comparisons");
    // Finish every native-value assertion before considering wire projections;
    // wire key-order differences must not mask a supplement's swap control.
    let mut wire_comparisons = 0;
    for (wire, expected, name, label, route) in wire_checks {
        assert_eq!(
            wire, expected,
            "JSON.stringify wire {name} {label} route {route}"
        );
        wire_comparisons += 1;
    }
    assert_eq!(
        wire_comparisons, 954,
        "TYPES-CARDINALITY completed wire comparisons"
    );
    eprintln!(
        "{} live cases across 28 interfaces, three ingress routes",
        oracle["cases"].as_array().unwrap().len()
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
