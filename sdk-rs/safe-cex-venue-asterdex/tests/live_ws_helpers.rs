use safe_cex_venue_asterdex::ws_helpers::public_connection_url;
use safe_cex_venue_asterdex::ws_helpers::public_depth_snapshot;
use safe_cex_venue_asterdex::ws_helpers::public_mark_price_update;
use safe_cex_venue_asterdex::ws_helpers::public_stream_trade_update;
use safe_cex_venue_asterdex::ws_helpers::public_ticker_update;
use serde_json::{Value, json};

fn number(value: f64) -> Value {
    let bits = if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    };
    json!({"numberBits":format!("{bits:016x}")})
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_ticker_update_matches_typescript() {
    use safe_cex_num::{ProtoMember, TableHit};
    use safe_cex_types::Ticker;

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-ticker-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(oracle["captureArchitecture"].as_str().is_some());
    assert!(oracle["captureNodeMajor"].as_u64().is_some());
    for case in oracle["cases"].as_array().unwrap() {
        let data = &case["input"];
        let ticker = case["ticker"].as_bool().unwrap().then(|| {
            serde_json::from_value::<Ticker>(json!({
                "id":"ticker-id", "symbol":data["s"], "bid":0, "ask":0, "last":0,
                "mark":0, "index":0, "percentage":0, "openInterest":0,
                "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0
            }))
            .unwrap()
        });
        let entries = ticker
            .as_ref()
            .map(|ticker| (data["s"].as_str().unwrap(), ticker))
            .into_iter()
            .collect::<Vec<_>>();
        let update = public_ticker_update(
            &safe_cex_exchange::js_runtime::value::from_json(&data),
            &entries,
        )
        .unwrap();
        let mut actual = json!({"updates":[],"events":[]});
        if let Some(update) = update {
            let (target_type, target_string, event_target) = match update.ticker {
                TableHit::Mapped(_) => (
                    "object",
                    "[object Object]".to_owned(),
                    json!({"symbol":safe_cex_exchange::js_runtime::value::to_json(&update.symbol).unwrap()}),
                ),
                TableHit::Inherited(ProtoMember::Prototype) => {
                    ("object", "[object Object]".to_owned(), json!({}))
                }
                TableHit::Inherited(member) => {
                    ("function", member.js_string().to_owned(), Value::Null)
                }
                TableHit::Missing => unreachable!(),
            };
            let mut patch = serde_json::Map::new();
            for (key, value) in [
                ("last", update.patch.last),
                ("percentage", update.patch.percentage),
                ("volume", update.patch.volume),
                ("quoteVolume", update.patch.quote_volume),
            ] {
                if let Some(value) = value {
                    patch.insert(key.to_owned(), number(value));
                }
            }
            actual["updates"] = json!([{
                "targetType":target_type,
                "targetString":target_string,
                "update":patch
            }]);
            actual["events"] = json!([[
                "ticker",
                safe_cex_exchange::js_runtime::value::to_json(&update.symbol).unwrap(),
                event_target
            ]]);
        }
        same(
            &actual,
            &case["output"],
            &format!("handleTickerUpdate field identity {}", case["name"]),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_ticker_update_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_ticker_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TICKER_CONTROL", "swap-percentage")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTickerUpdate field identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_ticker_update_oracle_rejects_signed_zero_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_ticker_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TICKER_CAPTURE_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTickerUpdate field identity \"signed-zero-and-finite\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_ticker_update_oracle_rejects_finite_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_ticker_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TICKER_CAPTURE_CONTROL", "erase-finite")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTickerUpdate field identity \"signed-zero-and-finite\""),
        "{diagnostics}"
    );
}

fn same(actual: &Value, expected: &Value, context: &str) {
    assert_eq!(actual, expected, "{context}");
}

fn same_json_numbers(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => actual.as_f64() == expected.as_f64(),
        (Value::Array(actual), Value::Array(expected)) => {
            actual.len() == expected.len()
                && actual
                    .iter()
                    .zip(expected)
                    .all(|(actual, expected)| same_json_numbers(actual, expected))
        }
        (Value::Object(actual), Value::Object(expected)) => {
            actual.len() == expected.len()
                && actual.iter().all(|(key, actual)| {
                    expected
                        .get(key)
                        .is_some_and(|expected| same_json_numbers(actual, expected))
                })
        }
        _ => actual == expected,
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_connection_url_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-url-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in cases.as_array().unwrap() {
        let actual = public_connection_url(
            case["testnet"].as_bool().unwrap(),
            case["requestId"].as_str().unwrap(),
        );
        assert_eq!(
            actual,
            case["url"].as_str().unwrap(),
            "public connectionURL {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_connection_url_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_connection_url_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_URL_CONTROL", "wrong-query-key")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("public connectionURL"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_depth_snapshot_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-depth-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(oracle["captureArchitecture"].as_str().is_some());
    assert!(oracle["captureNodeMajor"].as_u64().is_some());
    let cases = oracle["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 15, "handleDepthSnapshot pinned source cases");
    let mut native_calls = 0;
    let mut expected_callbacks = 0;
    let bypass_source_callback_count =
        std::env::var_os("ASTERDEX_DEPTH_BYPASS_SOURCE_CALLBACK_COUNT").is_some();
    let mut callback_comparisons = 0;
    for index in 0..15 {
        let case = &cases[index];
        let book = public_depth_snapshot(&safe_cex_exchange::js_runtime::value::from_json(
            &case["input"],
        ));
        let callbacks = case["output"]["callbacks"].as_array().unwrap();
        let expected_callback_count = match case["name"].as_str().unwrap() {
            "sorted-and-callback-isolation" => 2,
            "fractional-levels" | "hex-number" | "book-without-ticker" => 1,
            _ => 0,
        };
        if !bypass_source_callback_count {
            assert_eq!(
                callbacks.len(),
                expected_callback_count,
                "handleDepthSnapshot source callback count {}",
                case["name"]
            );
        }
        expected_callbacks += expected_callback_count;
        match book {
            Some(book) => {
                let actual = serde_json::to_value(book).unwrap();
                for callback in callbacks {
                    assert!(
                        same_json_numbers(&actual, callback),
                        "handleDepthSnapshot result {}: actual={actual} expected={callback}",
                        case["name"]
                    );
                    callback_comparisons += 1;
                }
                let expected = &case["output"]["tickerUpdates"];
                assert_eq!(
                    expected.as_array().unwrap().len(),
                    usize::from(case["ticker"].as_bool().unwrap()),
                    "handleDepthSnapshot ticker update count {index}"
                );
                if case["ticker"].as_bool().unwrap() {
                    assert!(same_json_numbers(
                        &expected[0]["update"]["bid"],
                        &actual["bids"][0]["price"]
                    ));
                    assert!(same_json_numbers(
                        &expected[0]["update"]["ask"],
                        &actual["asks"][0]["price"]
                    ));
                }
            }
            None => {
                assert!(
                    callbacks.is_empty(),
                    "handleDepthSnapshot result {}",
                    case["name"]
                );
                assert!(
                    case["output"]["tickerUpdates"]
                        .as_array()
                        .unwrap()
                        .is_empty(),
                    "handleDepthSnapshot invalid frame has no ticker patch {}",
                    case["name"]
                );
            }
        }
        native_calls += 1;
    }
    assert_eq!(
        native_calls, 15,
        "handleDepthSnapshot fixed native comparison count"
    );
    assert_eq!(
        expected_callbacks, 5,
        "handleDepthSnapshot fixed source callback count"
    );
    assert_eq!(
        callback_comparisons, expected_callbacks,
        "handleDepthSnapshot fixed callback comparison count"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_depth_snapshot_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_depth_snapshot_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_DEPTH_CONTROL", "allow-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains(r#"handleDepthSnapshot source callback count "zero-quantity""#),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_depth_snapshot_rejects_dropped_callback_payload() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_depth_snapshot_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_DEPTH_CONTROL", "empty-callbacks")
        .env("ASTERDEX_DEPTH_BYPASS_SOURCE_CALLBACK_COUNT", "1")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleDepthSnapshot fixed callback comparison count"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_stream_trade_update_matches_typescript() {
    use safe_cex_num::{ProtoMember, TableHit};
    use safe_cex_types::Ticker;

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-trade-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(oracle["captureArchitecture"].as_str().is_some());
    assert!(oracle["captureNodeMajor"].as_u64().is_some());
    for case in oracle["cases"].as_array().unwrap() {
        let trade: safe_cex_venue_asterdex::types::AsterDEXStreamTrade =
            serde_json::from_value(case["input"].clone()).unwrap();
        let ticker = case["ticker"].as_bool().unwrap().then(|| {
            serde_json::from_value::<Ticker>(json!({
                "id":"ticker-id", "symbol":trade.s.clone(), "bid":0, "ask":0, "last":0,
                "mark":0, "index":0, "percentage":0, "openInterest":0,
                "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0
            }))
            .unwrap()
        });
        let entries = ticker
            .as_ref()
            .map(|ticker| vec![(trade.s.as_str(), ticker)])
            .unwrap_or_default();
        let update = public_stream_trade_update(&trade, &entries);
        let (updates, ticker_hit) = match update.ticker {
            TableHit::Mapped(_) => (
                vec![json!({
                    "targetType":"object",
                    "targetString":"[object Object]",
                    "update":{"last":number(update.ticker_last)}
                })],
                json!({"kind":"mapped"}),
            ),
            TableHit::Inherited(member) => (
                vec![json!({
                    "targetType":if member == ProtoMember::Prototype {"object"} else {"function"},
                    "targetString":member.js_string(),
                    "update":{"last":number(update.ticker_last)}
                })],
                json!({"kind":"inherited","member":member.js_string()}),
            ),
            TableHit::Missing => (Vec::new(), json!({"kind":"missing"})),
        };
        let actual = json!({
            "updates":updates,
            "trades":[{"name":"trade","trade":{
                "symbol":update.event.symbol,
                "price":number(update.event.price),
                "qty":number(update.event.qty),
                "timestamp":number(update.event.timestamp),
                "side":update.event.side
            }}],
            "tickerHit":ticker_hit
        });
        let expected = case["output"].clone();
        same(
            &actual,
            &expected,
            &format!("handleTradeUpdate field identity {}", case["name"]),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_stream_trade_update_oracle_rejects_source_side_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_stream_trade_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TRADE_CONTROL", "swap-side")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTradeUpdate field identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_stream_trade_update_oracle_rejects_signed_zero_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_stream_trade_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TRADE_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTradeUpdate field identity \"taker-buy-no-ticker\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_stream_trade_update_oracle_rejects_finite_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_stream_trade_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_TRADE_CONTROL", "erase-finite")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleTradeUpdate field identity \"maker-sell\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_mark_price_update_matches_typescript() {
    use safe_cex_num::{ProtoMember, TableHit};
    use safe_cex_types::Ticker;

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ws-mark-price-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(oracle["captureArchitecture"].as_str().is_some());
    assert!(oracle["captureNodeMajor"].as_u64().is_some());
    for case in oracle["cases"].as_array().unwrap() {
        let data = &case["input"];
        let ticker = case["ticker"].as_bool().unwrap().then(|| {
            serde_json::from_value::<Ticker>(json!({
                "id":"ticker-id", "symbol":data["s"], "bid":0, "ask":0, "last":0,
                "mark":0, "index":0, "percentage":0, "openInterest":0,
                "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0
            }))
            .unwrap()
        });
        let entries = ticker
            .as_ref()
            .map(|ticker| vec![(data["s"].as_str().unwrap(), ticker)])
            .unwrap_or_default();
        let plan = public_mark_price_update(
            &safe_cex_exchange::js_runtime::value::from_json(&data),
            &entries,
        )
        .unwrap();
        let mut actual = json!({"updates":[],"events":[],"tickerHit":{"kind":"missing"}});
        let expected = case["output"].clone();
        match plan {
            None => {}
            Some(plan) => {
                let (target_type, target_string, event_ticker, ticker_hit) = match plan.ticker {
                    TableHit::Mapped(_) => (
                        "object",
                        "[object Object]",
                        json!({"symbol":safe_cex_exchange::js_runtime::value::to_json(&plan.symbol).unwrap()}),
                        json!({"kind":"mapped"}),
                    ),
                    TableHit::Inherited(member) if member == ProtoMember::Prototype => (
                        "object",
                        "[object Object]",
                        json!({}),
                        json!({"kind":"inherited","member":member.js_string()}),
                    ),
                    TableHit::Inherited(member) => (
                        "function",
                        member.js_string(),
                        Value::Null,
                        json!({"kind":"inherited","member":member.js_string()}),
                    ),
                    TableHit::Missing => unreachable!(),
                };
                let mut update = serde_json::Map::new();
                if let Some(value) = plan.patch.mark {
                    update.insert("mark".to_owned(), number(value));
                }
                if let Some(value) = plan.patch.index {
                    update.insert("index".to_owned(), number(value));
                }
                if let Some(value) = plan.patch.funding_rate {
                    update.insert("fundingRate".to_owned(), number(value));
                }
                if let Some(value) = plan.patch.funding_time {
                    update.insert("fundingTime".to_owned(), number(value.as_f64().unwrap()));
                }
                actual["updates"] = json!([{
                    "targetType":target_type,
                    "targetString":target_string,
                    "update":update
                }]);
                actual["events"] = json!([[
                    "markPrice",
                    safe_cex_exchange::js_runtime::value::to_json(&plan.symbol).unwrap(),
                    event_ticker
                ]]);
                actual["tickerHit"] = ticker_hit.clone();
            }
        }
        same(
            &actual,
            &expected,
            &format!("handleMarkPriceUpdate field identity {}", case["name"]),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_mark_price_update_oracle_rejects_source_index_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_mark_price_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARK_PRICE_CONTROL", "swap-index")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleMarkPriceUpdate field identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_mark_price_update_oracle_rejects_signed_zero_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_mark_price_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARK_PRICE_CAPTURE_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleMarkPriceUpdate field identity \"signed-zero-and-finite\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_mark_price_update_oracle_rejects_finite_bit_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_mark_price_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARK_PRICE_CAPTURE_CONTROL", "erase-finite")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleMarkPriceUpdate field identity \"signed-zero-and-finite\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_kline_update_matches_typescript() {
    use safe_cex_venue_asterdex::ws_helpers::public_kline_update;

    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/public-ws-kline-oracle.cjs"
    );
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(oracle["captureArchitecture"].is_string());
    assert!(oracle["captureNodeMajor"].is_u64());
    for case in oracle["cases"].as_array().unwrap() {
        let mut data = if let Some(input) = case["inputText"].as_str() {
            serde_json::from_str(input).unwrap()
        } else {
            case["input"].clone()
        };
        if case["nativeNegativeZero"] == "k.t" {
            data["k"]["t"] = Value::Number(serde_json::Number::from_f64(-0.0).unwrap());
            assert_eq!(
                data["k"]["t"].as_f64().unwrap().to_bits(),
                (-0.0_f64).to_bits()
            );
        }
        let streams = case["callbackStreams"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>();
        let actual = match public_kline_update(
            &safe_cex_exchange::js_runtime::value::from_json(&data),
            &streams,
        ) {
            Ok(None) => json!([]),
            Ok(Some(update)) => {
                let timestamp = match update.candle.timestamp {
                    None => json!({"kind":"undefined"}),
                    Some(Value::Number(value)) => json!({
                        "kind":"number",
                        "bits":format!("{:016x}", value.as_f64().unwrap().to_bits())
                    }),
                    Some(value) => json!({"kind":"value","value":value}),
                };
                json!([{
                    "stream":update.stream,
                    "timestamp":timestamp,
                    "openBits":format!("{:016x}", update.candle.open.to_bits()),
                    "highBits":format!("{:016x}", update.candle.high.to_bits()),
                    "lowBits":format!("{:016x}", update.candle.low.to_bits()),
                    "closeBits":format!("{:016x}", update.candle.close.to_bits()),
                    "volumeBits":format!("{:016x}", update.candle.volume.to_bits()),
                }])
            }
            Err(error) => {
                let error = error.to_exchange_error();
                json!({"name":error.name,"message":error.message})
            }
        };
        let expected = if let Some(error) = case.get("error") {
            error.clone()
        } else {
            case["result"].clone()
        };
        same(
            &actual,
            &expected,
            &format!("handleKlineUpdate field identity {}", case["name"]),
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_kline_update_oracle_rejects_source_field_swap() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_kline_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_PUBLIC_KLINE_CONTROL", "swap-close")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleKlineUpdate field identity \"all-fields\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_kline_update_oracle_rejects_signed_zero_capture_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_kline_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_KLINE_CAPTURE_CONTROL", "erase-zero")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleKlineUpdate field identity \"signed-zero-and-nan\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_kline_update_oracle_rejects_finite_capture_collapse() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_kline_update_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_KLINE_CAPTURE_CONTROL", "erase-finite")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("handleKlineUpdate field identity \"all-fields\""),
        "{diagnostics}"
    );
}
