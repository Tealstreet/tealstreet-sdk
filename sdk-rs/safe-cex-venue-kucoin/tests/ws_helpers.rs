use safe_cex_venue_kucoin::ws_helpers::{
    KucoinPublicRole, get_candle_handler_topic, normalize_millis, normalize_timestamp,
    parse_number, snapshot_shard_symbols,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
// Keep heartbeat proofs in the existing venue-oracle websocket target.
#[path = "support/ws_ping.rs"]
mod ping;

#[test]
fn live_socket_number_and_timestamp_helpers() {
    let mut cases = vec![json!({})];
    for value in [
        json!(null),
        json!(false),
        json!(true),
        json!(""),
        json!("  "),
        json!("0x10"),
        json!("-0"),
        json!("12tail"),
        json!("1e21"),
        json!("\u{feff}1700000000123"),
        json!("\u{85}12"),
        json!("Infinity"),
        json!("-Infinity"),
        json!("NaN"),
        json!([]),
        json!(["12"]),
        json!([1, 2]),
        json!({}),
        json!({"toString":null}),
        json!(-1.5),
        json!("100000000000000001"),
    ] {
        cases.push(json!({"value":value}));
    }
    for boundary in [
        0_i64,
        10_000_000_000,
        100_000_000_000_000,
        100_000_000_000_000_000,
    ] {
        for offset in [-128_i64, -1, 0, 1, 128] {
            cases.push(json!({"value":(boundary+offset).to_string()}));
        }
    }
    let market = json!({"id":"XBTUSDTM","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10000},"leverage":{"min":1,"max":100}}});
    let mut markets = vec![];
    for index in 0..9 {
        let mut m = market.clone();
        m["id"] = json!(if index % 3 == 0 {
            String::new()
        } else {
            format!("id{index}")
        });
        m["symbol"] = json!(if index == 0 {
            String::new()
        } else {
            format!("symbol{index}")
        });
        markets.push(m);
    }
    for count in [-1.0, 0.0, 1.0, 2.0, 3.0, 2.5] {
        for index in [-1.0, 0.0, 1.0, 2.0] {
            for topic in [
                "/market/candles:BTC-USDT_1min",
                "plain",
                "a:",
                "a:b",
                "a:b_c_d",
                "a:b_c:d_e",
            ] {
                cases.push(
                    json!({"markets":markets,"shardCount":count,"shardIndex":index,"topic":topic}),
                );
            }
        }
    }
    for c in &mut cases {
        if c.get("topic").is_none() {
            c["topic"] = json!("a:b_c");
        }
    }
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-helpers.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        reference["roles"],
        json!([KucoinPublicRole::Detail, KucoinPublicRole::Snapshot]),
        "KucoinPublicRole"
    );
    assert_eq!(reference["cases"].as_array().unwrap().len(), cases.len());
    for (c, expected) in cases.iter().zip(reference["cases"].as_array().unwrap()) {
        let value = c.get("value");
        let store = safe_cex_exchange::native_store::DefaultStore::new();
        safe_cex_store::Store::update(
            &store,
            safe_cex_store::StoreUpdate {
                markets: c
                    .get("markets")
                    .map(|m| serde_json::from_value(m.clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let shards = snapshot_shard_symbols(
            &store,
            c["shardIndex"].as_f64().unwrap_or(f64::NAN),
            c["shardCount"].as_f64().unwrap_or(f64::NAN),
        );
        let topic = match get_candle_handler_topic(c["topic"].as_str().unwrap()) {
            Ok(t) => json!({"ok":t}),
            Err(e) => json!({"error":e}),
        };
        let parsed = match parse_number(value) {
            Ok(n) => json!({"ok":n.map_or(Value::Null,tag)}),
            Err(e) => json!({"error":e}),
        };
        let actual = json!({"snapshotShardSymbols":{"ok":shards},"getCandleHandlerTopic":topic,"parseNumber":parsed,"normalizeTimestamp":result(normalize_timestamp(value,1700000000123.0)),"normalizeMillis":result(normalize_millis(value,1700000000123.0))});
        compare(&actual, expected);
    }
}
#[test]
fn socket_role_rejects_widened_union_shapes() {
    for value in [
        json!({"detail":null}),
        json!(["detail"]),
        json!(null),
        json!(false),
        json!(1),
        json!("Detail"),
        json!("detail "),
        json!("\u{feff}detail"),
        json!("other"),
    ] {
        assert!(
            <KucoinPublicRole as Deserialize>::deserialize(value.clone()).is_err(),
            "accepted {value}"
        );
    }
    for (value, expected) in [
        ("detail", KucoinPublicRole::Detail),
        ("snapshot", KucoinPublicRole::Snapshot),
    ] {
        assert_eq!(
            <KucoinPublicRole as Deserialize>::deserialize(json!(value)).unwrap(),
            expected,
            "native public role identity"
        );
    }
}
fn result(value: Result<f64, String>) -> Value {
    match value {
        Ok(v) => json!({"ok":tag(v)}),
        Err(e) => json!({"error":e}),
    }
}
fn tag(v: f64) -> Value {
    if v.is_nan() {
        json!("NaN")
    } else if v == f64::INFINITY {
        json!("Infinity")
    } else if v == f64::NEG_INFINITY {
        json!("-Infinity")
    } else if v == 0.0 && v.is_sign_negative() {
        json!("-0")
    } else {
        json!(v)
    }
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, a) in a {
                compare(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}

fn batch_reference(cases: &Value) -> Value {
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/ws-batch.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn live_subscription_batch_wire_order_and_clock_calls() {
    let mut cases = Vec::new();
    for count in [0, 1, 2, 99, 100, 101, 199, 200, 201, 380, 401] {
        for channels in [
            vec![],
            vec![""],
            vec!["/snapshot", "/ticker", "/snapshot"],
            vec!["a:\"\n", "\u{feff}😀"],
        ] {
            for subscribe in [true, false] {
                for fail_at in [0, 1, 2, 4] {
                    let symbols: Vec<_> = (0..count)
                        .map(|i| {
                            if i % 3 == 0 {
                                String::new()
                            } else {
                                format!("s:{i},😀")
                            }
                        })
                        .collect();
                    cases.push(json!({"subscribe":subscribe,"channels":channels,"symbols":symbols,"failAt":fail_at}));
                }
            }
        }
    }
    // One empty symbol skips; two empty symbols join to a comma and DO send.
    for count in [1, 2, 100, 101] {
        cases.push(json!({"subscribe":true,"channels":["x"],"symbols":vec!["";count],"failAt":0}));
    }
    let expected = batch_reference(&json!(cases));
    assert_eq!(
        expected.as_array().unwrap().len(),
        cases.len(),
        "KUCOIN-CARDINALITY websocket source case count"
    );
    for (case, expected) in cases.iter().zip(expected.as_array().unwrap()) {
        let channels: Vec<String> = serde_json::from_value(case["channels"].clone()).unwrap();
        let symbols: Vec<String> = serde_json::from_value(case["symbols"].clone()).unwrap();
        let mut frames = Vec::new();
        let mut clock_calls = 0;
        let result = safe_cex_venue_kucoin::ws_helpers::send_channel_op(
            case["subscribe"].as_bool().unwrap(),
            &channels,
            &symbols,
            || {
                let now = 1700000000000.0 + f64::from(clock_calls);
                clock_calls += 1;
                now
            },
            |frame| {
                frames.push(frame.to_owned());
                if frames.len() == case["failAt"].as_u64().unwrap() as usize {
                    Err("send refused")
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(
            json!({"frames":frames,"clockCalls":clock_calls,"error":result.err()}),
            *expected,
            "subscription batch wire order and clock calls: {case}"
        );
    }
}

#[test]
fn subscription_batch_lone_surrogate_boundary() {
    // Preserve the JS input escape instead of replacing its code unit before Node.
    let raw = r#"[{"subscribe":true,"channels":["x"],"symbols":["\ud800"],"failAt":0}]"#;
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/ws-batch.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        reference[0]["frames"][0]
            .as_str()
            .unwrap()
            .contains(r#"x:\ud800"#)
    );
    assert!(
        serde_json::from_str::<Vec<Value>>(raw).is_err(),
        "shared UTF-8 ingress must refuse a lone surrogate"
    );
}

#[test]
fn subscription_batch_source_boundary_control() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_subscription_batch_wire_order_and_clock_calls",
            "--nocapture",
        ])
        .env("KUCOIN_BATCH_MUTATION", "boundary")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("subscription batch wire order and clock calls:"),
        "{text}"
    );
}

#[test]
fn frame_cardinality_supplement_retains_each_channel() {
    let case =
        json!({"subscribe":true,"channels":["first","second"],"symbols":["BTC","ETH"],"failAt":0});
    let source = batch_reference(&json!([case]));
    assert_eq!(
        source.as_array().unwrap().len(),
        1,
        "KUCOIN-CARDINALITY frame source cases"
    );
    assert_eq!(
        source[0]["frames"].as_array().unwrap().len(),
        2,
        "KUCOIN-CARDINALITY frame source total"
    );
    let mut frames = vec![];
    let mut clock_calls = 0;
    safe_cex_venue_kucoin::ws_helpers::send_channel_op(
        true,
        &["first".into(), "second".into()],
        &["BTC".into(), "ETH".into()],
        || {
            let now = 1700000000000.0 + f64::from(clock_calls);
            clock_calls += 1;
            now
        },
        |frame| {
            frames.push(frame.to_owned());
            Ok::<_, String>(())
        },
    )
    .unwrap();
    assert_eq!(
        frames.len(),
        2,
        "KUCOIN-CARDINALITY frame native retained frames"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        let native: Value = serde_json::from_str(&frames[index]).unwrap();
        assert_eq!(native["topic"], ["first:BTC,ETH", "second:BTC,ETH"][index]);
        assert_eq!(frames[index], source[0]["frames"][index]);
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY frame native comparisons"
    );
}

#[test]
fn shard_cardinality_supplement_retains_each_native_symbol() {
    let first = json!({"id":"first-id","symbol":"FIRST","base":"A","quote":"B","active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":10},"leverage":{"min":1,"max":2}}});
    let mut second = first.clone();
    second["id"] = json!("second-id");
    second["symbol"] = json!("SECOND");
    let markets = json!([first, second]);
    let cases = json!([{"markets":markets,"shardCount":1,"shardIndex":0,"topic":"a:b_c"}]);
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-helpers.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"].as_array().unwrap().len(),
        1,
        "KUCOIN-CARDINALITY shard source cases"
    );
    assert_eq!(
        source["cases"][0]["snapshotShardSymbols"]["ok"]
            .as_array()
            .unwrap()
            .len(),
        2,
        "KUCOIN-CARDINALITY shard source total"
    );
    let store = safe_cex_exchange::native_store::DefaultStore::new();
    safe_cex_store::Store::update(
        &store,
        safe_cex_store::StoreUpdate {
            markets: Some(serde_json::from_value(markets).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let native = snapshot_shard_symbols(&store, 0.0, 1.0);
    assert_eq!(
        native.len(),
        2,
        "KUCOIN-CARDINALITY shard native retained symbols"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        assert_eq!(native[index], ["first-id", "second-id"][index]);
        assert_eq!(
            native[index],
            source["cases"][0]["snapshotShardSymbols"]["ok"][index]
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY shard native comparisons"
    );
}
