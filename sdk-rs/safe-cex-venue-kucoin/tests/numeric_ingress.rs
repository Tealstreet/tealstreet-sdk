//! Compare public Deserialize and shared whole-entry ingress against live bits.
#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_kucoin::{
    account_mode::KucoinAccountModeDetection,
    spot_types::{KucoinSpotWsServer, KucoinSpotWsTokenData, KucoinSpotWsTokenResponse},
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::process::Command;
fn source() -> Vec<Value> {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-ingress.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 72);
    rows
}
fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(
    case: &Value,
    mode: &str,
) -> Result<T, serde_json::Error> {
    let shared = mode.starts_with("js-");
    let mode = mode.strip_prefix("js-").unwrap_or(mode);
    let raw = case["raw"].as_str().unwrap();
    if mode == "slice" {
        return serde_json::from_slice(raw.as_bytes());
    }
    if mode == "raw" {
        return if shared {
            js_from_str(raw)
        } else {
            serde_json::from_str(raw)
        };
    }
    let mut value: Value = serde_json::from_str(raw).unwrap();
    if mode == "native" {
        let field = case["field"].as_u64().unwrap();
        let target = match case["kind"].as_str().unwrap() {
            "mode" => {
                &mut value[if field == 0 {
                    "classicSubAccount"
                } else {
                    "unifiedSubAccount"
                }][0]
            }
            kind => {
                let s = match kind {
                    "server" => &mut value,
                    "data" => &mut value["instanceServers"][0],
                    _ => &mut value["data"]["instanceServers"][0],
                };
                &mut s[if field == 0 {
                    "pingInterval"
                } else {
                    "pingTimeout"
                }]
            }
        };
        *target = json!(-0.0f64);
    }
    match (shared, mode) {
        (true, "borrowed") => js_from_value_ref(&value),
        (true, _) => js_from_value(value),
        (false, "borrowed") => T::deserialize(&value),
        (false, _) => serde_json::from_value(value),
    }
}
fn actual(case: &Value, mode: &str) -> Result<Vec<String>, String> {
    let pair = match case["kind"].as_str().unwrap() {
        "server" => {
            decode::<KucoinSpotWsServer>(case, mode).map(|s| [s.ping_interval, s.ping_timeout])
        }
        "data" => decode::<KucoinSpotWsTokenData>(case, mode).map(|d| {
            let s = &d.instance_servers[0];
            [s.ping_interval, s.ping_timeout]
        }),
        "response" => decode::<KucoinSpotWsTokenResponse>(case, mode).map(|r| {
            let d = r.data.unwrap();
            let s = &d.instance_servers[0];
            [s.ping_interval, s.ping_timeout]
        }),
        "mode" => decode::<KucoinAccountModeDetection>(case, mode).map(|m| match m {
            KucoinAccountModeDetection::Identified {
                classic_sub_account,
                unified_sub_account,
                ..
            } => [classic_sub_account[0], unified_sub_account[0]],
            _ => panic!("wrong decoded arm"),
        }),
        _ => panic!("unknown case"),
    };
    pair.map(|p| p.into_iter().map(numeric_bits::bits).collect())
        .map_err(|e| e.to_string())
}
#[test]
fn live_direct_numeric_ingress_retains_every_number_bit() {
    let mut comparisons = 0;
    for case in source() {
        for mode in ["raw", "slice", "owned", "borrowed"] {
            let bits = actual(&case, mode).unwrap_or_else(|error| {
                panic!("direct KuCoin numeric ingress refused {case} {mode}: {error}")
            });
            assert_eq!(
                json!(bits),
                case["bits"],
                "direct KuCoin numeric ingress bits: {case} {mode}"
            );
            comparisons += 1;
        }
        if case["literal"] == "-0" {
            assert_eq!(
                json!(actual(&case, "native").unwrap()),
                case["bits"],
                "direct native Value -0.0 bits: {case}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 296);
}
#[test]
fn numeric_type_source_control_is_not_a_noop() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_shared_numeric_ingress_retains_every_number_bit",
            "--nocapture",
        ])
        .env("KUCOIN_NUMERIC_INGRESS_MUTATION", "source-string")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("live KuCoin numeric type corpus rejected")
    );
}

#[test]
fn live_shared_numeric_ingress_retains_every_number_bit() {
    let mut comparisons = 0;
    for case in source() {
        for mode in ["js-raw", "js-owned", "js-borrowed"] {
            let bits = actual(&case, mode).unwrap_or_else(|error| {
                panic!("shared KuCoin numeric ingress refused {case} {mode}: {error}")
            });
            assert_eq!(
                json!(bits),
                case["bits"],
                "shared KuCoin numeric ingress bits: {case} {mode}"
            );
            comparisons += 1;
        }
        if case["literal"] == "-0" {
            assert_eq!(
                json!(actual(&case, "js-native").unwrap()),
                case["bits"],
                "shared native Value -0.0 bits: {case}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 224);
}

#[test]
fn live_string_ingress_refuses_only_the_declared_surrogate_boundary() {
    fn decoded<T: DeserializeOwned + Serialize + JsNumberOutput>(
        raw: &str,
        shared: bool,
    ) -> Result<T, serde_json::Error> {
        let value: T = if shared {
            js_from_str(raw)?
        } else {
            serde_json::from_str(raw)?
        };
        Ok(value)
    }
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-ingress.cjs"
        ))
        .env("KUCOIN_INGRESS_CORPUS", "strings")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 30);
    let mut refusals = 0;
    for case in cases {
        let raw = case["raw"].as_str().unwrap();
        for shared in [false, true] {
            let actual = match case["kind"].as_str().unwrap() {
                "server" => decoded::<KucoinSpotWsServer>(raw, shared).map(|v| match case["field"]
                    .as_str()
                    .unwrap()
                {
                    "endpoint" => v.endpoint,
                    "protocol" => v.protocol,
                    _ => panic!("unknown server field"),
                }),
                "data" => decoded::<KucoinSpotWsTokenData>(raw, shared).map(|v| v.token),
                "response" => decoded::<KucoinSpotWsTokenResponse>(raw, shared).map(|v| {
                    match case["field"].as_str().unwrap() {
                        "code" => v.code,
                        "msg" => v.msg.unwrap(),
                        _ => panic!("unknown response field"),
                    }
                }),
                _ => panic!("unknown string projection"),
            };
            if case["lone"] == true {
                let unit = case["units"][0].as_u64().unwrap();
                assert!((0xd800..=0xdfff).contains(&unit));
                assert!(case["stringJson"].as_str().unwrap().contains("\\ud"));
                let units: Vec<u16> = case["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                    .collect();
                assert_eq!(
                    actual.unwrap(),
                    String::from_utf16_lossy(&units),
                    "typed String replacement: {case}"
                );
                typed_utf16::retained(raw);
                refusals += 1;
            } else {
                let value = actual.unwrap();
                assert_eq!(
                    value,
                    serde_json::from_str::<String>(case["stringJson"].as_str().unwrap()).unwrap(),
                    "native decoded string identity: {case}"
                );
            }
        }
    }
    assert_eq!(refusals, 20);
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;

#[test]
fn capture_canonicalizes_only_nan() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-bits.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        assert!(
            row["captureArchitecture"]
                .as_str()
                .is_some_and(|v| !v.is_empty())
        );
        assert!(
            row["captureNodeMajor"]
                .as_u64()
                .is_some_and(|major| major > 0),
            "capture Node major recorded"
        );
        let input = row["input"].as_str().unwrap();
        let number = f64::from_bits(u64::from_str_radix(input, 16).unwrap());
        let expected = if number.is_nan() {
            "7ff8000000000000"
        } else {
            input
        };
        let label = if number.is_nan() {
            "NaN capture canonical payload"
        } else if number == 0.0 {
            "signed-zero capture identity"
        } else {
            "finite and infinity capture identity"
        };
        assert_eq!(
            numeric_bits::bits(number),
            expected,
            "{label}; Rust capture architecture {}",
            std::env::consts::ARCH
        );
        assert_eq!(
            row["bits"], expected,
            "{label}; source architecture {}",
            row["captureArchitecture"]
        );
    }
}
