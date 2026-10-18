use safe_cex_venue_coinbase::{
    starbase::CoinbaseStarbaseHeartbeatSignal, starbase_public::*, starbase_public_frames::*,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{process::Command, rc::Rc, sync::OnceLock};
const INPUT: &[u8] = include_bytes!("fixtures/frame-classification.json");
type Native = CoinbaseStarbaseFrameValue;
type Classification = CoinbaseStarbasePublicFrameClassification;
fn oracle() -> &'static Value {
    static OUTPUT: OnceLock<Value> = OnceLock::new();
    OUTPUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/frame-classification.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live frame classifier oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn bits(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", actual.to_bits()),
        expected.as_str().unwrap(),
        "{label}"
    );
}
fn data(actual: &Native, expected: &Value) {
    fn visit(actual: &Native, tokens: &mut std::slice::Iter<'_, Value>) {
        let expected = tokens.next().expect("source payload token");
        match actual {
            Native::Undefined => panic!("JSON.parse cannot produce undefined"),
            Native::Null => assert_eq!(expected["type"], "null", "decoded native null payload"),
            Native::Bool(value) => {
                assert_eq!(expected["type"], "bool", "decoded native boolean kind");
                assert_eq!(
                    *value,
                    expected["value"].as_bool().unwrap(),
                    "decoded native boolean payload"
                );
            }
            Native::Number(value) => {
                assert_eq!(expected["type"], "number", "decoded native number kind");
                bits(
                    *value,
                    &expected["bits"],
                    "decoded native payload number bits",
                );
            }
            Native::String(value) => {
                assert_eq!(expected["type"], "string", "decoded native string kind");
                assert_eq!(
                    *value,
                    units(&expected["units"]),
                    "decoded native payload UTF16 units"
                );
            }
            Native::Array(value) => {
                assert_eq!(expected["type"], "array", "decoded native array kind");
                assert_eq!(
                    value.borrow().len(),
                    expected["length"].as_u64().unwrap() as usize,
                    "decoded native array size"
                );
                for value in value.borrow().iter() {
                    visit(value, tokens);
                }
            }
            Native::Object(value) => {
                assert_eq!(expected["type"], "object", "decoded native object kind");
                let expected = expected["keys"].as_array().unwrap();
                let keys = actual.own_keys();
                assert_eq!(keys.len(), expected.len(), "decoded native object size");
                for (key, expected) in keys.iter().zip(expected) {
                    assert_eq!(
                        *key,
                        units(expected),
                        "decoded native own-key order and UTF16 units"
                    );
                }
                for key in keys {
                    visit(&value.borrow()[&key], tokens);
                }
            }
        }
    }
    let mut tokens = expected.as_array().unwrap().iter();
    visit(actual, &mut tokens);
    assert!(
        tokens.next().is_none(),
        "native payload consumes independent source tokens"
    );
}
fn channel(actual: &CoinbaseStarbasePublicChannel, expected: &Value) {
    let parts: Vec<_> = expected.as_str().unwrap().split('.').collect();
    match actual {
        CoinbaseStarbasePublicChannel::Ticker {
            instrument_name,
            interval,
        }
        | CoinbaseStarbasePublicChannel::Book {
            instrument_name,
            interval,
        } => {
            assert_eq!(
                parts[0],
                if matches!(actual, CoinbaseStarbasePublicChannel::Ticker { .. }) {
                    "ticker"
                } else {
                    "book"
                },
                "decoded native channel variant"
            );
            assert_eq!(instrument_name, parts[1], "decoded native instrument");
            assert_eq!(
                *interval,
                if parts[2] == "100ms" {
                    CoinbaseStarbaseBookInterval::V100Ms
                } else {
                    CoinbaseStarbaseBookInterval::Agg2
                },
                "decoded native book interval"
            );
        }
        CoinbaseStarbasePublicChannel::Chart {
            instrument_name,
            resolution,
        } => {
            assert_eq!(
                &parts[..2],
                ["chart", "trades"],
                "decoded native chart variant"
            );
            assert_eq!(instrument_name, parts[2], "decoded native chart instrument");
            let expected = match parts[3] {
                "1" => CoinbaseStarbaseChartResolution::V1,
                "3" => CoinbaseStarbaseChartResolution::V3,
                "5" => CoinbaseStarbaseChartResolution::V5,
                "15" => CoinbaseStarbaseChartResolution::V15,
                "30" => CoinbaseStarbaseChartResolution::V30,
                "60" => CoinbaseStarbaseChartResolution::V60,
                "120" => CoinbaseStarbaseChartResolution::V120,
                "360" => CoinbaseStarbaseChartResolution::V360,
                "720" => CoinbaseStarbaseChartResolution::V720,
                "1D" => CoinbaseStarbaseChartResolution::V1D,
                _ => panic!("unknown source resolution"),
            };
            assert_eq!(*resolution, expected, "decoded native chart resolution");
        }
        CoinbaseStarbasePublicChannel::InstrumentState { currency } => {
            assert_eq!(
                &parts[..3],
                ["instrument", "state", "future"],
                "decoded native instrument-state variant"
            );
            let expected = match parts[3] {
                "BTC" => CoinbaseStarbaseInstrumentStateCurrency::Btc,
                "ETH" => CoinbaseStarbaseInstrumentStateCurrency::Eth,
                "USDC" => CoinbaseStarbaseInstrumentStateCurrency::Usdc,
                "any" => CoinbaseStarbaseInstrumentStateCurrency::Any,
                _ => panic!("unknown source currency"),
            };
            assert_eq!(*currency, expected, "decoded native state currency");
        }
    }
}
fn classification(actual: &Classification, expected: &Value) {
    match actual {
        Classification::PublicSubscription {
            channel: ch,
            payload,
        } => {
            assert_eq!(
                expected["kind"], "public-subscription",
                "decoded native classification kind"
            );
            channel(ch, &expected["channel"]);
            channel(&payload.channel, &expected["payload"]["channel"]);
            data(&payload.data, &expected["payload"]["data"]);
        }
        Classification::Heartbeat { signal } => {
            assert_eq!(
                expected["kind"], "heartbeat",
                "decoded native classification kind"
            );
            assert_eq!(
                *signal,
                if expected["signal"] == "heartbeat" {
                    CoinbaseStarbaseHeartbeatSignal::Heartbeat
                } else {
                    CoinbaseStarbaseHeartbeatSignal::TestRequest
                },
                "decoded native heartbeat signal"
            );
        }
        Classification::PublicTestResponse { id, outcome } => {
            assert_eq!(
                expected["kind"], "public-test-response",
                "decoded native classification kind"
            );
            bits(*id, &expected["idBits"], "decoded native response ID");
            match outcome {
                CoinbaseStarbasePublicTestOutcome::Result { version } => {
                    assert_eq!(
                        expected["outcome"], "result",
                        "decoded native result outcome"
                    );
                    assert_eq!(
                        *version,
                        units(&expected["version"]),
                        "decoded native version UTF16 units"
                    );
                }
                CoinbaseStarbasePublicTestOutcome::Error(error) => {
                    assert_eq!(expected["outcome"], "error", "decoded native error outcome");
                    bits(
                        error.code,
                        &expected["error"]["codeBits"],
                        "decoded native error code",
                    );
                    assert_eq!(
                        error.message,
                        units(&expected["error"]["message"]),
                        "decoded native error message units"
                    );
                    assert_eq!(
                        error.data.is_some(),
                        expected["error"]["hasData"].as_bool().unwrap(),
                        "decoded native error data presence"
                    );
                    if let Some(value) = &error.data {
                        data(value, &expected["error"]["data"]);
                    }
                }
            }
        }
        Classification::UnsupportedSubscription {
            channel,
            reason,
            scope,
        } => {
            assert_eq!(
                expected["kind"], "unsupported-subscription",
                "decoded native classification kind"
            );
            assert_eq!(
                *channel,
                units(&expected["channelUnits"]),
                "decoded native unsupported channel"
            );
            assert_eq!(*reason,CoinbaseStarbaseUnsupportedSubscriptionReason::ChannelNotDesiredAcknowledgedPublicSubscription);
            assert_eq!(
                expected["reason"],
                "channel-not-a-desired-acknowledged-public-subscription"
            );
            assert_eq!(
                *scope,
                CoinbaseStarbaseFrameClassificationScope::Unclassified
            );
            assert_eq!(expected["scope"], "unclassified");
        }
        Classification::Unknown { reason } => {
            assert_eq!(
                expected["kind"], "unknown",
                "decoded native classification kind"
            );
            let expected = match expected["reason"].as_str().unwrap() {
                "duplicate-json-key" => CoinbaseStarbaseUnknownFrameReason::DuplicateJsonKey,
                "invalid-json" => CoinbaseStarbaseUnknownFrameReason::InvalidJson,
                "invalid-public-test-response" => {
                    CoinbaseStarbaseUnknownFrameReason::InvalidPublicTestResponse
                }
                "invalid-public-websocket-envelope" => {
                    CoinbaseStarbaseUnknownFrameReason::InvalidPublicWebsocketEnvelope
                }
                "unsupported-raw-frame" => CoinbaseStarbaseUnknownFrameReason::UnsupportedRawFrame,
                _ => panic!("unknown source reason"),
            };
            assert_eq!(*reason, expected, "decoded native unknown reason");
        }
    }
}
#[test]
fn native_notification_response_and_raw_frame_classification_match_live_source() {
    assert_eq!(
        format!("{:x}", Sha256::digest(INPUT)),
        "62b02b0753be9f326ec91450cff51b756f4e320c6a94c1f8f3440493166a93d3",
        "independent frame corpus content"
    );
    let input: Value = serde_json::from_slice(INPUT).unwrap();
    for (group, floor) in [("notifications", 31), ("responses", 64), ("raw", 69)] {
        let cases = input[group].as_array().unwrap();
        let expected = oracle()[group].as_array().unwrap();
        assert_eq!(cases.len(), floor, "independent input floor");
        assert_eq!(expected.len(), floor, "independent oracle floor");
        for (case, expected) in cases.iter().zip(expected) {
            if group == "raw" {
                let raw = case["units"].as_array().map(|_| units(&case["units"]));
                match classify_coinbase_starbase_public_websocket_frame(
                    raw.as_deref(),
                    case["context"].as_object().unwrap(),
                ) {
                    Ok(actual) => {
                        assert!(
                            expected.get("ok").is_some(),
                            "native classifier success branch: {}",
                            case["label"]
                        );
                        classification(&actual, &expected["ok"]);
                    }
                    Err(error) => {
                        assert!(
                            expected.get("error").is_some(),
                            "native classifier context error branch"
                        );
                        assert_eq!(
                            error.name,
                            expected["error"]["name"].as_str().unwrap(),
                            "native context error name"
                        );
                        assert_eq!(
                            error.message,
                            expected["error"]["message"].as_str().unwrap(),
                            "native context error message"
                        );
                    }
                }
            } else {
                let frame = Native::from_json(&case["frame"]);
                let actual = if group == "notifications" {
                    let list = |key: &str| {
                        case[key]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().encode_utf16().collect())
                            .collect::<Vec<Vec<u16>>>()
                    };
                    classify_notification(&frame, &list("ack"), &list("desired"))
                } else {
                    let ids = case["pending"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect::<Vec<_>>();
                    classify_public_test_response(&frame, &ids)
                };
                classification(&actual, &expected["ok"]);
            }
        }
    }
}
#[test]
fn native_payload_aliases_array_aliases_and_primitive_copies_match_source() {
    let ch = "ticker.BTC_USDC-PERPETUAL.100ms";
    let channels = vec![ch.encode_utf16().collect::<Vec<_>>()];
    for (index, initial) in [json!({"marker":1}), json!([1]), json!(3)]
        .into_iter()
        .enumerate()
    {
        let frame = Native::from_json(
            &json!({"jsonrpc":"2.0","method":"subscription","params":{"channel":ch,"data":initial}}),
        );
        let input = frame.get("params").unwrap().get("data").unwrap();
        let Classification::PublicSubscription { mut payload, .. } =
            classify_notification(&frame, &channels, &channels)
        else {
            panic!("native subscription kind")
        };
        match (&mut payload.data, &input) {
            (Native::Object(actual), Native::Object(original)) => {
                assert!(Rc::ptr_eq(actual, original), "native payload object alias");
                actual
                    .borrow_mut()
                    .insert("added".encode_utf16().collect(), Native::Number(88.0));
            }
            (Native::Array(actual), Native::Array(original)) => {
                assert!(Rc::ptr_eq(actual, original), "native payload array alias");
                actual.borrow_mut().push(Native::Number(88.0));
            }
            (actual, Native::Number(_)) => *actual = Native::Number(99.0),
            _ => panic!("native alias type"),
        }
        if index < 2 {
            assert_eq!(oracle()["aliases"][index]["same"], true);
        }
        data(&input, &oracle()["aliases"][index]["input"]);
    }
    let frame = Native::from_json(
        &json!({"jsonrpc":"2.0","id":7,"error":{"code":-0.0,"message":"native-message","data":{"marker":1}}}),
    );
    let Native::Object(original) = frame.get("error").unwrap().get("data").unwrap() else {
        unreachable!()
    };
    let Classification::PublicTestResponse {
        outcome: CoinbaseStarbasePublicTestOutcome::Error(error),
        ..
    } = classify_public_test_response(&frame, &[7.0])
    else {
        panic!("native error outcome")
    };
    let Native::Object(actual) = error.data.unwrap() else {
        unreachable!()
    };
    assert!(
        Rc::ptr_eq(&actual, &original),
        "native error data object alias"
    );
    actual
        .borrow_mut()
        .insert("added".encode_utf16().collect(), Native::Number(88.0));
    data(&Native::Object(original), &oracle()["aliases"][3]["input"]);
    assert_eq!(oracle()["aliases"][3]["same"], true);
}
