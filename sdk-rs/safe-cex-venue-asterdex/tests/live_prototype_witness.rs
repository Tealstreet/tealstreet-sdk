//! Pins known TypeScript defects and live protocol effects for the websocket helpers.
use std::collections::{BTreeSet, HashMap};
use std::process::Command;

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn prototype_socket_paths_preserve_typescript_defects() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/prototype-witness.cjs");
    let positive = Command::new("node").arg(script).output().unwrap();
    assert!(
        positive.status.success(),
        "{}",
        String::from_utf8_lossy(&positive.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&positive.stdout).unwrap();
    let cases = oracle["evidence"].as_array().unwrap();
    use safe_cex_exchange::runtime::LogSeverity;
    let expired_trace = std::cell::RefCell::new(Vec::new());
    safe_cex_venue_asterdex::ws_helpers::private_listen_key_expired(
        |message, severity| {
            expired_trace
                .borrow_mut()
                .push(serde_json::json!(["log", message, severity]))
        },
        || {
            expired_trace
                .borrow_mut()
                .push(serde_json::json!("clear-refresh"))
        },
        || {
            expired_trace
                .borrow_mut()
                .push(serde_json::json!("clear-watchdog"))
        },
        || {
            expired_trace
                .borrow_mut()
                .push(serde_json::json!("reconnect"))
        },
    );
    let expired_trace = expired_trace.into_inner();
    assert_eq!(
        serde_json::Value::Array(expired_trace.clone()),
        oracle["listenKeyExpired"],
        "AsterDEXPrivateWebsocket#handleListenKeyExpired ordered effects"
    );
    assert_eq!(expired_trace[0][2], serde_json::json!(LogSeverity::Warning));
    assert_eq!(
        cases.len(),
        76,
        "twelve inherited names plus public batch controls and unsubscribe states"
    );
    use safe_cex_venue_asterdex::ws_helpers::{
        PublicEventDataDispatch, public_event_data_dispatch,
    };
    let handler_keys = [
        ("24hrTicker", ()),
        ("aggTrade", ()),
        ("markPriceUpdate", ()),
        ("kline", ()),
    ];
    for case in cases.iter().filter(|case| case["case"] == "public batch") {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            case["trace"],
            serde_json::json!(["received", ["stats", key]]),
            "AsterDEXPublicWebsocket#handleEventData stats then caught batch abort {key}"
        );
        let actual = public_event_data_dispatch(
            &safe_cex_exchange::js_runtime::value::from_json(&serde_json::json!({"e":key})),
            &handler_keys,
        )
        .unwrap();
        let PublicEventDataDispatch::InheritedHandlerFailure { event_type, member } = actual else {
            panic!("AsterDEXPublicWebsocket inherited handler {key}");
        };
        assert_eq!(event_type, key);
        let safe_cex_num::TableHit::Inherited(expected) =
            safe_cex_num::js_table_lookup::<&str, ()>(&[], key)
        else {
            panic!("inherited source member");
        };
        assert_eq!(member, expected);
    }
    let ordinary = cases
        .iter()
        .find(|case| case["case"] == "public batch ordinary")
        .unwrap();
    assert_eq!(
        ordinary["trace"],
        serde_json::json!(["received", ["stats", "aggTrade"], "handleTradeUpdate"]),
        "AsterDEXPublicWebsocket#handleEventData ordinary event continues batch"
    );
    let original =
        safe_cex_exchange::js_runtime::value::from_json(&serde_json::json!({"e":"aggTrade"}));
    let PublicEventDataDispatch::Handlers { event_type, data } =
        public_event_data_dispatch(&original, &handler_keys).unwrap()
    else {
        panic!("ordinary handler");
    };
    assert_eq!(event_type, "aggTrade");
    assert!(
        data.strict_equals(&original),
        "ordinary handler retains original input identity"
    );

    let source_control = Command::new("node")
        .arg(script)
        .env("ASTERDEX_PUBLIC_EVENT_CONTROL", "own-only")
        .output()
        .unwrap();
    assert_eq!(source_control.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&source_control.stderr)
            .contains("prototype dispatch throws and stops the public batch"),
        "source control must fail on whole public batch behavior: {}",
        String::from_utf8_lossy(&source_control.stderr)
    );
    let negative = Command::new("node")
        .args([script, "--negative-control"])
        .output()
        .unwrap();
    assert_eq!(
        negative.status.code(),
        Some(1),
        "null-prototype silent fix must fail"
    );
    assert!(
        String::from_utf8_lossy(&negative.stderr)
            .contains("prototype topic suppression must preserve the inherited truthy guard"),
        "control must fail on the topic guard: {}",
        String::from_utf8_lossy(&negative.stderr)
    );

    let stream_control = Command::new("node")
        .arg(script)
        .env("ASTERDEX_PUBLIC_STREAM_CONTROL", "own-only")
        .output()
        .unwrap();
    assert_eq!(stream_control.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&stream_control.stderr)
            .contains("handleStreamData inherited lookup preservation constructor"),
        "source mutation must fail by handleStreamData row name: {}",
        String::from_utf8_lossy(&stream_control.stderr)
    );
    assert_eq!(
        oracle["surrogateDispatch"]["streamType"], "string",
        "AsterDEXPublicWebsocket#handleStreamData source accepts escaped lone surrogate"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(
            oracle["surrogateDispatch"]["inputJson"].as_str().unwrap()
        )
        .is_err(),
        "AsterDEXPublicWebsocket#handleStreamData Rust UTF-8 ingress refuses lone surrogate"
    );
    let surrogate_supplement = oracle["surrogateObservabilitySupplement"]
        .as_array()
        .unwrap();
    assert_eq!(
        surrogate_supplement.len(),
        4,
        "handleStreamData surrogate observability supplement case count"
    );
    for (name, stream_json, length, code_units, json_symbol) in [
        (
            "lone-surrogate",
            r#""\ud800@ticker""#,
            1,
            vec![0xd800],
            r#""\ud800""#,
        ),
        ("scalar", r#""A@ticker""#, 1, vec![0x41], r#""A""#),
        (
            "paired-surrogate",
            "\"😀@ticker\"",
            2,
            vec![0xd83d, 0xde00],
            "\"😀\"",
        ),
        (
            "literal-backslash",
            r#""\\ud800@ticker""#,
            6,
            vec![0x5c, 0x55, 0x44, 0x38, 0x30, 0x30],
            r#""\\UD800""#,
        ),
    ] {
        let case = surrogate_supplement
            .iter()
            .find(|case| case["name"] == name)
            .unwrap_or_else(|| panic!("missing handleStreamData supplement case {name}"));
        assert_eq!(
            case["streamJson"], stream_json,
            "handleStreamData {name} stream input"
        );
        assert_eq!(
            case["dispatchedHandler"], "handleTickerUpdate",
            "handleStreamData {name} ticker dispatch"
        );
        assert_eq!(
            case["trace"],
            serde_json::json!([["stats", "24hrTicker"], ["handler", "24hrTicker"]]),
            "handleStreamData {name} handler effects"
        );
        assert_eq!(
            case["forwarded"],
            serde_json::json!({
                "type": "string",
                "length": length,
                "codeUnits": code_units,
                "json": json_symbol,
            }),
            "handleStreamData {name} forwarded symbol code units before serialization"
        );
    }

    use safe_cex_venue_asterdex::ws_helpers::{PublicStreamDispatch, public_stream_dispatch};
    for case in cases.iter().filter(|case| case["case"] == "public stream") {
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            public_stream_dispatch(&format!("btc@{key}"), false).unwrap(),
            PublicStreamDispatch::Ignore,
            "AsterDEXPublicWebsocket#handleStreamData inherited stream {key}"
        );
    }
    for case in oracle["streamDispatch"].as_array().unwrap() {
        let data_symbol_truthy = case["existingSymbolTruthy"].as_bool().unwrap();
        let actual =
            public_stream_dispatch(case["stream"].as_str().unwrap(), data_symbol_truthy).unwrap();
        let trace = case["trace"].as_array().unwrap();
        match trace.first().and_then(serde_json::Value::as_array) {
            Some(call) if call[0] == "depthUpdate" => {
                assert_eq!(
                    actual,
                    PublicStreamDispatch::DepthSnapshot {
                        assigned_symbol: case["data"]["s"]
                            .as_str()
                            .filter(|_| !data_symbol_truthy)
                            .map(str::to_owned),
                    },
                    "AsterDEXPublicWebsocket#handleStreamData depth route {}",
                    case["stream"]
                );
            }
            Some(call) if call[0] == "stats" => {
                let handler_call = trace[1].as_array().unwrap();
                let key = match call[0].as_str().unwrap() {
                    "stats" => match handler_call[0].as_str().unwrap() {
                        "24hrTicker" => "24hrTicker",
                        "aggTrade" => "aggTrade",
                        "markPriceUpdate" => "markPriceUpdate",
                        "kline" => "kline",
                        other => panic!("unexpected live stream handler {other}"),
                    },
                    other => panic!("unexpected stats event {other}"),
                };
                assert_eq!(
                    actual,
                    PublicStreamDispatch::Handler {
                        key,
                        assigned_symbol: case["data"]["s"]
                            .as_str()
                            .filter(|_| !data_symbol_truthy)
                            .map(str::to_owned),
                    },
                    "AsterDEXPublicWebsocket#handleStreamData handler route {}",
                    case["stream"]
                );
            }
            None => assert_eq!(
                actual,
                PublicStreamDispatch::Ignore,
                "AsterDEXPublicWebsocket#handleStreamData ignored route {}",
                case["stream"]
            ),
            Some(other) => panic!("unexpected stream trace {other:?}"),
        }
    }

    use safe_cex_venue_asterdex::ws_helpers::public_subscribe_plan;
    for case in cases.iter().filter(|case| case["case"] == "topic") {
        let key = case["key"].as_str().unwrap();
        let streams = vec![format!("btc@{key}")];
        let plan = public_subscribe_plan(true, 1, &streams, &HashMap::new())
            .unwrap()
            .expect("connected public socket should produce a subscribe plan");
        assert_eq!(
            plan.0,
            case["trace"][0][2].as_str().unwrap(),
            "AsterDEXPublicWebsocket#subscribe exact frame {key}"
        );
        let mut topics = HashMap::new();
        for (key, values) in plan.2 {
            topics.insert(key, values);
        }
        let actual: BTreeSet<_> = topics.keys().map(String::as_str).collect();
        let expected: BTreeSet<_> = case["own"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(actual, expected, "AsterDEX topic membership {key}");
    }

    use safe_cex_venue_asterdex::ws_helpers::public_unsubscribe_plan;
    let unsubscribe = cases
        .iter()
        .find(|case| case["case"] == "unsubscribe")
        .unwrap();
    let streams: Vec<String> = unsubscribe["frame"]["params"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    let plan = public_unsubscribe_plan(true, 1, &streams).unwrap().unwrap();
    assert_eq!(
        plan.0,
        unsubscribe["frameText"].as_str().unwrap(),
        "AsterDEXPublicWebsocket#unsubscribe exact frame"
    );
    assert_eq!(plan.1, unsubscribe["nextId"].as_u64().unwrap());
    assert!(
        public_unsubscribe_plan(false, 1, &streams)
            .unwrap()
            .is_none()
    );

    let streams = vec!["btc@unknown".to_owned(), "eth@unknown".to_owned()];
    let plan = public_subscribe_plan(true, 7, &streams, &HashMap::new())
        .unwrap()
        .unwrap();
    assert_eq!(plan.1, 8);
    let mut topics = HashMap::new();
    for (key, values) in plan.2 {
        topics.insert(key, values);
    }
    assert_eq!(topics.get("unknown").unwrap(), &["btc", "eth"]);
    assert!(
        public_subscribe_plan(false, 7, &streams, &HashMap::new())
            .unwrap()
            .is_none()
    );
}
