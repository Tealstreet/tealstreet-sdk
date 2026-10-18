use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_venue_asterdex::{clock_helpers::*, exchange_helpers::*};
use serde_json::{Map, Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

fn captured_bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    }
}

struct OpenInterestAdapter {
    responses: RefCell<VecDeque<Result<RestResponse, RestFailure>>>,
    requests: RefCell<Vec<RestRequest>>,
}

impl RestAdapter for OpenInterestAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(request);
        let response = self
            .responses
            .borrow_mut()
            .pop_front()
            .expect("oracle response");
        Box::pin(async move { response })
    }
}

fn run_ready<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    match Pin::as_mut(&mut future).poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("mock REST operation unexpectedly suspended"),
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_open_interest_map_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["fetchOpenInterest"].as_array().unwrap() {
        let responses = case["responses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|response| {
                if let Some(message) = response.get("error").and_then(Value::as_str) {
                    Err(RestFailure::pre_transmit(
                        message.to_owned(),
                        RestRequest::default(),
                    ))
                } else {
                    Ok(RestResponse {
                        status: 200,
                        reason: None,
                        headers: Default::default(),
                        body: if response["data"].as_str() == Some("") {
                            safe_cex_transport::RestResponseBody::Encoded(Vec::new())
                        } else {
                            safe_cex_transport::RestResponseBody::Encoded(
                                serde_json::to_vec(&response["data"]).unwrap(),
                            )
                        },
                    })
                }
            })
            .collect::<VecDeque<_>>();
        let adapter = OpenInterestAdapter {
            responses: RefCell::new(responses),
            requests: RefCell::new(Vec::new()),
        };
        let symbols = case["symbols"].as_array().map(|values| {
            values
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        });
        let mut warnings = Vec::new();
        let actual = run_ready(fetch_open_interest_map(
            &adapter,
            "/fapi/v1/openInterest",
            symbols.as_deref(),
            case["allow"].as_bool().unwrap_or(true),
            |warning| warnings.push(warning),
        ));
        let entries = actual.iter().map(|(key, value)| {
            let tagged = value.as_f64().filter(|number| !number.is_finite() || (*number == 0.0 && number.is_sign_negative()))
                .map(|number| json!({"number":if number == 0.0 {"-0".to_owned()} else {number.to_string()}}))
                .unwrap_or_else(|| value.clone());
            json!([key, tagged])
        }).collect::<Vec<_>>();
        same(
            &json!(entries),
            &case["entries"],
            &format!("fetchOpenInterestMap entries {}", case["name"]),
        );
        let actual_requests = adapter
            .requests
            .borrow()
            .iter()
            .map(|request| {
                let mut config = serde_json::Map::new();
                if let Some(params) = &request.params {
                    config.insert("params".into(), params.clone());
                }
                config.insert("timeout".into(), json!(request.timeout_ms));
                json!({"url":request.url,"config":config})
            })
            .collect::<Vec<_>>();
        same(
            &json!(actual_requests),
            &case["requests"],
            &format!("fetchOpenInterestMap requests {}", case["name"]),
        );
        same(
            &json!(
                warnings
                    .iter()
                    .map(|message| json!({"message":message,"severity":"Warning"}))
                    .collect::<Vec<_>>()
            ),
            &case["logs"],
            &format!("fetchOpenInterestMap logs {}", case["name"]),
        );
        assert_eq!(
            adapter.responses.borrow().len(),
            case["unconsumed"].as_u64().unwrap() as usize,
            "fetchOpenInterestMap consumption {}",
            case["name"]
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_open_interest_map_rejects_source_fallback_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fetch_open_interest_map_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_OPEN_INTEREST_CONTROL", "always-fallback")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchOpenInterestMap requests \"bulk succeeds\""),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_open_interest_map_records_lone_surrogate_key_finding() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let witness = &data["fetchOpenInterestLoneSurrogate"];
    assert_eq!(
        witness["typeof"], "string",
        "TS map key before JSON serialization"
    );
    assert_eq!(
        witness["size"], 1,
        "TS retains escaped lone-surrogate symbol"
    );
    let wire = witness["inputJson"].as_str().unwrap();
    assert!(
        wire.contains("\\ud800"),
        "double-escaped source witness: {wire}"
    );
    assert!(
        serde_json::from_str::<Value>(wire).is_err(),
        "Rust String ingress refuses lone surrogate"
    );

    let adapter = OpenInterestAdapter {
        responses: RefCell::new(VecDeque::from([Ok(RestResponse {
            status: 200,
            reason: None,
            headers: Default::default(),
            body: safe_cex_transport::RestResponseBody::Encoded(wire.as_bytes().to_vec()),
        })])),
        requests: RefCell::new(Vec::new()),
    };
    let actual = run_ready(fetch_open_interest_map(
        &adapter,
        "/fapi/v1/openInterest",
        None,
        false,
        |_| {},
    ));
    assert!(
        actual.is_empty(),
        "FINDING: current Rust response representation loses the TS Map entry"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn position_mode_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-oracle.cjs"
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
        let data = if case["kind"] == "undefined" {
            None
        } else {
            Some(&case["data"])
        };
        let actual = match parse_position_mode_response(data) {
            Ok(value) => json!({"value": value}),
            Err((name, message)) => json!({"error": {"name": name, "message": message}}),
        };
        let expected = if let Some(error) = case.get("error") {
            json!({"error": error})
        } else {
            json!({"value": case["value"]})
        };
        same(
            &actual,
            &expected,
            &format!("fetchPositionMode response {case}"),
        );
        assert_eq!(
            case["actualRequestPath"], "/fapi/v1/positionSide/dual",
            "fetchPositionMode endpoint {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn get_account_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/get-account-oracle.cjs"
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
        let input = &case["input"];
        let alias = input["account"]
            .get("accountAlias")
            .and_then(Value::as_str)
            .unwrap_or("");
        let key = input.get("key").and_then(Value::as_str);
        let actual = serde_json::to_value(get_account_from_alias(alias, key)).unwrap();
        same(
            &actual,
            &case["result"],
            &format!("getAccount output {case}"),
        );
        let expected_path = if input["apiVersion"] == "v3" {
            "/fapi/v3/account"
        } else {
            "/fapi/v4/account"
        };
        assert_eq!(case["path"], expected_path, "getAccount endpoint {case}");
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn get_account_oracle_rejects_nullish_fallback_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "get_account_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_GET_ACCOUNT_CONTROL", "nullish-fallback")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("getAccount output"), "{diagnostics}");
}

#[test]
fn leverage_brackets_preserves_map_key_identity() {
    for key in [json!({ "market": "BTCUSDT" }), json!(["BTCUSDT"])] {
        let response = json!([
            { "symbol": key, "brackets": [{ "initialLeverage": 125 }] },
            { "symbol": key, "brackets": [{ "initialLeverage": 75 }] },
        ]);
        let (entries, message, severity) = parse_leverage_brackets_response(Ok(&response));
        assert_eq!(
            entries,
            vec![
                (Some(key.clone()), Some(json!(125))),
                (Some(key), Some(json!(75))),
            ],
            "distinct object/array Map keys must retain both entries"
        );
        assert_eq!(message, "Loaded leverage brackets for 2 symbols");
        assert_eq!(severity.as_str(), "info");
    }

    let response = json!([
        { "brackets": [{ "initialLeverage": 125 }] },
        { "symbol": null, "brackets": [{ "initialLeverage": 100 }] },
        { "brackets": [{ "initialLeverage": 75 }] },
        { "symbol": null, "brackets": [{ "initialLeverage": 50 }] },
    ]);
    let (entries, _, _) = parse_leverage_brackets_response(Ok(&response));
    assert_eq!(
        entries,
        vec![
            (None, Some(json!(75))),
            (Some(Value::Null), Some(json!(50)))
        ],
        "undefined and null are separate keys; duplicates overwrite in place"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn leverage_brackets_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/leverage-brackets-oracle.cjs"
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
        let input = &case["input"];
        let response = if let Some(error) = input.get("error").and_then(Value::as_str) {
            Err(error)
        } else {
            Ok(&input["response"])
        };
        let (entries, log_message, log_severity) = parse_leverage_brackets_response(response);
        let actual = json!({
            "result": entries,
            "log": [log_message, log_severity.as_str()],
        });
        let expected_trace = case["trace"].as_array().unwrap();
        let expected_log = expected_trace
            .iter()
            .find(|entry| entry[0] == "log")
            .unwrap();
        same(
            &actual,
            &json!({
                "result": case["result"],
                "log": [expected_log[1], expected_log[2]],
            }),
            &format!("fetchLeverageBrackets response {case}"),
        );
        assert_eq!(
            expected_trace[0][1],
            format!(
                "/fapi/{}/leverageBracket",
                case["apiVersion"].as_str().unwrap()
            ),
            "fetchLeverageBrackets endpoint {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn order_book_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-book-oracle.cjs"
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
        let input = &case["input"];
        let levels = |side: &str| {
            input["depth"][side]
                .as_array()
                .unwrap()
                .iter()
                .map(|level| {
                    (
                        level[0].as_str().unwrap().to_owned(),
                        level[1].as_str().unwrap().to_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let book = transform_order_book(&levels("bids"), &levels("asks"));
        let actual = json!({
            "result": serde_json::to_value(&book).unwrap(),
            "negativeZeroPrices": {
                "bids": book.bids.iter().map(|level| level.price.to_bits() == (-0.0_f64).to_bits()).collect::<Vec<_>>(),
                "asks": book.asks.iter().map(|level| level.price.to_bits() == (-0.0_f64).to_bits()).collect::<Vec<_>>(),
            },
        });
        same(
            &actual,
            &json!({
                "result": case["result"],
                "negativeZeroPrices": case["negativeZeroPrices"],
            }),
            &format!("fetchOrderBook result {case}"),
        );
        assert_eq!(
            case["request"],
            json!([
                format!("/fapi/{}/depth", input["apiVersion"].as_str().unwrap()),
                {"params": {"symbol": input["symbol"], "limit": input["limit"]}}
            ]),
            "fetchOrderBook request {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn order_book_oracle_rejects_source_sort_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "order_book_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_ORDER_BOOK_CONTROL", "ascending-bids")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchOrderBook result"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_ohlcv_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/fetch-ohlcv-oracle.cjs"
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
        assert!(case["captureArchitecture"].is_string());
        let input = &case["input"];
        let rows = input["rows"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let fields = row.as_array().unwrap();
                safe_cex_venue_asterdex::types::AsterDEXKlineResponse(
                    f64::from_bits(
                        u64::from_str_radix(case["rowTimestampBits"][index].as_str().unwrap(), 16)
                            .unwrap(),
                    ),
                    fields[1].as_str().unwrap().to_owned(),
                    fields[2].as_str().unwrap().to_owned(),
                    fields[3].as_str().unwrap().to_owned(),
                    fields[4].as_str().unwrap().to_owned(),
                    fields[5].as_str().unwrap().to_owned(),
                    fields[6].as_f64().unwrap(),
                    fields[7].as_str().unwrap().to_owned(),
                    fields[8].as_f64().unwrap(),
                    fields[9].as_str().unwrap().to_owned(),
                    fields[10].as_str().unwrap().to_owned(),
                    fields[11].as_str().unwrap().to_owned(),
                )
            })
            .collect::<Vec<safe_cex_venue_asterdex::types::AsterDEXKlineResponse>>();
        let rows = safe_cex_exchange::js_runtime::value::from_typed(&rows).unwrap();
        let candles: Vec<safe_cex_types::Candle> =
            safe_cex_exchange::js_runtime::value::to_typed(&transform_klines(&rows).unwrap())
                .unwrap();
        let actual = serde_json::to_value(&candles).unwrap();
        same(
            &actual,
            &case["result"],
            &format!("fetchOHLCV output {case}"),
        );
        let actual_bits = candles
            .iter()
            .map(|candle| {
                [
                    candle.timestamp,
                    candle.open,
                    candle.high,
                    candle.low,
                    candle.close,
                    candle.volume,
                ]
                .map(|number| format!("{:016x}", captured_bits(number)))
            })
            .collect::<Vec<_>>();
        same(
            &serde_json::to_value(actual_bits).unwrap(),
            &case["bits"],
            &format!("fetchOHLCV numeric bits {case}"),
        );
        let limit = input
            .get("limit")
            .filter(|value| value.as_f64().is_some_and(|number| number != 0.0))
            .cloned()
            .unwrap_or(json!(500));
        let mut params = json!({
            "symbol": input["symbol"],
            "interval": input["interval"],
            "limit": limit,
        });
        if let Some(from) = input
            .get("from")
            .filter(|value| value.as_f64().is_some_and(|number| number != 0.0))
        {
            params["startTime"] = from.clone();
        }
        assert_eq!(
            case["request"],
            json!([
                format!("/fapi/{}/klines", input["apiVersion"].as_str().unwrap()),
                {"params": params}
            ]),
            "fetchOHLCV request parameters"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_ohlcv_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fetch_ohlcv_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_OHLCV_CONTROL", "wrong-open-field")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("fetchOHLCV output"), "{diagnostics}");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn historical_fills_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/historical-fills-oracle.cjs"
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
        assert!(case["captureArchitecture"].is_string());
        let input = &case["input"];
        let skipped_before_request = input["publicDataOnly"] == true
            || input["opts"]
                .get("symbol")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty);
        if skipped_before_request {
            assert!(case["result"].as_array().unwrap().is_empty());
            assert!(case.get("request").is_none());
            assert!(case["logs"].as_array().unwrap().is_empty());
        }
        if input["reject"] == true {
            assert!(case["result"].as_array().unwrap().is_empty());
            assert!(case.get("request").is_some());
            assert_eq!(
                case["logs"][0][0],
                "Failed to fetch executions: oracle transport rejection"
            );
            assert_eq!(case["logs"][0][1], "error");
        }
        if case["compareMapper"] == true {
            let trades = input["response"]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .enumerate()
                        .map(|(index, row)| {
                            let mut numeric = row.clone();
                            for field in ["id", "orderId", "time"] {
                                numeric[field] = json!(0);
                            }
                            let mut trade: safe_cex_venue_asterdex::types::AsterDEXUserTradeResponse =
                                serde_json::from_value(numeric).unwrap();
                            trade.id = f64::from_bits(u64::from_str_radix(
                                case["rowNumberBits"][index]["id"].as_str().unwrap(),
                                16,
                            ).unwrap());
                            trade.order_id = f64::from_bits(u64::from_str_radix(
                                case["rowNumberBits"][index]["orderId"].as_str().unwrap(),
                                16,
                            ).unwrap());
                            trade.time = f64::from_bits(u64::from_str_radix(
                                case["rowNumberBits"][index]["time"].as_str().unwrap(),
                                16,
                            ).unwrap());
                            trade
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let executions = historical_fills_from_trades(&trades);
            same(
                &serde_json::to_value(&executions).unwrap(),
                &case["result"],
                &format!("fetchHistoricalFills output {case}"),
            );
            let actual_bits = executions
                .iter()
                .map(|row| {
                    json!({
                        "timestamp": format!("{:016x}", captured_bits(row.timestamp)),
                        "price": format!("{:016x}", captured_bits(row.price)),
                        "qty": format!("{:016x}", captured_bits(row.qty)),
                        "commission": row.commission.map(|value| format!("{:016x}", captured_bits(value))),
                        "realizedPnl": row.realized_pnl.map(|value| format!("{:016x}", captured_bits(value))),
                    })
                })
                .collect::<Vec<_>>();
            same(
                &serde_json::to_value(actual_bits).unwrap(),
                &case["bits"],
                &format!("fetchHistoricalFills numeric bits {case}"),
            );
        }

        if let Some(request) = case.get("request") {
            let opts = &input["opts"];
            let mut params = json!({
                "symbol": opts["symbol"],
                "limit": opts.get("limit").and_then(Value::as_f64).unwrap_or(500.0).min(1000.0),
            });
            for (option, parameter) in [
                ("orderId", "orderId"),
                ("from", "startTime"),
                ("to", "endTime"),
            ] {
                if let Some(value) = opts.get(option).filter(|value| match value {
                    Value::Null => false,
                    Value::Bool(value) => *value,
                    Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
                    Value::String(value) => !value.is_empty(),
                    _ => true,
                }) {
                    params[parameter] = value.clone();
                }
            }
            same(
                request,
                &json!([
                    format!("/fapi/{}/userTrades", input["apiVersion"].as_str().unwrap()),
                    {"params": params}
                ]),
                "fetchHistoricalFills request",
            );
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn historical_fills_oracle_rejects_source_side_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "historical_fills_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_HISTORICAL_FILLS_CONTROL", "wrong-side")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchHistoricalFills output"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_balance_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/fetch-balance-oracle.cjs"
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
        assert!(case["captureArchitecture"].is_string());
        let balance = balance_from_account_response(&case["input"]["account"]).unwrap();
        let actual = serde_json::to_value(&balance).unwrap();
        same(
            &actual,
            &case["result"],
            &format!("fetchBalance result {case}"),
        );
        let bits = [
            ("total", balance.total),
            ("free", balance.free),
            ("used", balance.used),
            ("upnl", balance.upnl),
        ]
        .into_iter()
        .map(|(key, value)| {
            (
                key.to_owned(),
                Value::String(format!("{:016x}", captured_bits(value))),
            )
        })
        .collect::<serde_json::Map<_, _>>();
        assert_eq!(
            Value::Object(bits),
            case["bits"],
            "fetchBalance binary64 bits {case}"
        );
        let expected_path = if case["input"]["apiVersion"] == "v3" {
            "/fapi/v3/account"
        } else {
            "/fapi/v4/account"
        };
        assert_eq!(
            case["request"], expected_path,
            "fetchBalance endpoint {case}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn fetch_balance_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fetch_balance_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_BALANCE_CONTROL", "nullish-fallback")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("fetchBalance result"), "{diagnostics}");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn leverage_brackets_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "leverage_brackets_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_LEVERAGE_BRACKETS_CONTROL", "last-bracket")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchLeverageBrackets response"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn position_mode_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "position_mode_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MODE_CONTROL", "reject-false")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchPositionMode response"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn symbol_max_leverage_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/symbol-max-leverage-oracle.cjs"
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
        let markets: Vec<_> = case["marketRows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let max = match &row["max"] {
                    Value::Null => None,
                    Value::Object(marker)
                        if marker.get("undefined") == Some(&Value::Bool(true)) =>
                    {
                        None
                    }
                    Value::Object(marker) if marker.contains_key("number") => {
                        Some(match marker["number"].as_str().unwrap() {
                            "NaN" => f64::NAN,
                            "Infinity" => f64::INFINITY,
                            "-Infinity" => f64::NEG_INFINITY,
                            "-0" => -0.0,
                            other => panic!("unknown number marker {other}"),
                        })
                    }
                    value => value.as_f64(),
                };
                (row["symbol"].as_str().unwrap().to_owned(), max)
            })
            .collect();
        match fetch_symbol_max_leverage(&markets, case["symbol"].as_str().unwrap()) {
            Ok(actual) => {
                assert!(
                    case.get("result").is_some(),
                    "symbol max leverage output {case}"
                );
                same(
                    &number(actual),
                    &case["result"],
                    &format!("fetchSymbolMaxLeverage result {case}"),
                );
            }
            Err(message) => {
                assert_eq!(
                    case["error"]["name"], "Error",
                    "fetchSymbolMaxLeverage error {case}"
                );
                assert_eq!(
                    case["error"]["message"], message,
                    "fetchSymbolMaxLeverage message {case}"
                );
                assert_eq!(
                    case["logs"][0][0],
                    format!("Failed to fetch max leverage: {message}"),
                    "fetchSymbolMaxLeverage catch logging {case}"
                );
            }
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn symbol_max_leverage_oracle_rejects_nullish_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "symbol_max_leverage_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_LEVERAGE_CONTROL", "nullish")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchSymbolMaxLeverage result"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn stp_mode_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/stp-mode-oracle.cjs"
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
        if case["operation"] == "changeStpMode" {
            let input = case.get("input");
            let actual = match build_change_stp_mode_request(input) {
                Ok(request) => json!({"request": request}),
                Err((name, message)) => json!({"error": {"name": name, "message": message}}),
            };
            let expected = if let Some(error) = case.get("error") {
                json!({"error": error})
            } else {
                json!({"request": {
                    "method": "POST",
                    "path": case["requestPath"],
                    "body": case["requestBody"]
                }})
            };
            same(&actual, &expected, &format!("changeStpMode request {case}"));
            if case.get("error").is_some() {
                if case["error"]["name"] == "Error" {
                    assert_eq!(
                        case["emits"][0], case["error"]["message"],
                        "changeStpMode emit {case}"
                    );
                } else {
                    assert!(
                        case["emits"].as_array().unwrap().is_empty(),
                        "coercion failure precedes emit: {case}"
                    );
                }
                assert!(
                    case.get("requestPath").is_none(),
                    "changeStpMode must not submit invalid mode: {case}"
                );
            } else {
                assert_eq!(
                    case["requestPath"], "/fapi/v3/stpMode",
                    "changeStpMode path {case}"
                );
                assert_eq!(
                    case["requestBody"]["stpMode"], case["input"],
                    "changeStpMode body {case}"
                );
            }
            continue;
        }
        let data = if case["data"].get("undefined").and_then(Value::as_bool) == Some(true) {
            None
        } else {
            Some(&case["data"])
        };
        let actual = match parse_stp_mode_response(data) {
            Ok(mode) => json!({"value": mode}),
            Err((name, message)) => json!({"error": {"name": name, "message": message}}),
        };
        let expected = if let Some(error) = case.get("error") {
            json!({"error": error})
        } else {
            json!({"value": case["value"]})
        };
        same(&actual, &expected, &format!("fetchStpMode response {case}"));
        assert_eq!(
            case["requestPath"], "/fapi/v3/stpMode",
            "fetchStpMode endpoint {case}"
        );
        if case.get("error").is_some() {
            assert_eq!(
                case["emits"][0], case["error"]["message"],
                "fetchStpMode emit {case}"
            );
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn change_stp_mode_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "stp_mode_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_CHANGE_STP_CONTROL", "accept-invalid")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("changeStpMode request"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn change_margin_type_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/change-margin-type-oracle.cjs"
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
        let version = serde_json::from_value(case["apiVersion"].clone()).unwrap();
        let actual = match build_change_margin_type_request(
            case["isolated"].as_bool().unwrap(),
            case["symbol"].as_str(),
            version,
        ) {
            Ok(request) => json!({"request": request}),
            Err((name, message)) => json!({"error": {"name": name, "message": message}}),
        };
        let expected = if let Some(error) = case.get("error") {
            json!({"error": error})
        } else {
            json!({"request": {
                "method": "POST",
                "path": case["requestPath"],
                "body": case["requestBody"]
            }})
        };
        same(
            &actual,
            &expected,
            &format!("changeMarginType request {case}"),
        );
        if let Some(error) = case.get("error") {
            let message = error["message"].as_str().unwrap();
            assert_eq!(
                case["logs"][0][0],
                format!("Failed to change margin type: {message}"),
                "changeMarginType log {case}"
            );
            assert!(
                case.get("requestPath").is_none(),
                "invalid symbol must not submit: {case}"
            );
        } else {
            assert_eq!(
                case["requestPath"],
                format!("/fapi/{}/marginType", case["apiVersion"].as_str().unwrap()),
                "changeMarginType path {case}"
            );
            assert_eq!(
                case["requestBody"]["symbol"], case["symbol"],
                "changeMarginType symbol {case}"
            );
            assert_eq!(
                case["requestBody"]["marginType"],
                if case["isolated"] == true {
                    "ISOLATED"
                } else {
                    "CROSSED"
                },
                "changeMarginType type {case}"
            );
        }
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn change_margin_type_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "change_margin_type_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARGIN_CONTROL", "accept-empty-symbol")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("changeMarginType request"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn stp_mode_oracle_rejects_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "stp_mode_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_STP_CONTROL", "drop-first-mode")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchStpMode response"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn exchange_helpers_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for c in data["accountModes"].as_array().unwrap() {
        assert_eq!(
            json!(get_account_position_mode(c["isHedged"].as_bool().unwrap())),
            c["output"],
            "getAccountPositionMode {c}"
        );
    }
    for c in data["mapperStrings"].as_array().unwrap() {
        let wire = c["inputJson"].as_str().unwrap();
        let scalar = serde_json::from_str::<String>(c["symbolJson"].as_str().unwrap());
        let output = if c["kind"] == "ticker" {
            serde_json::from_str(wire).map(|input| json!(transform_ticker(&input, None).unwrap()))
        } else {
            serde_json::from_str(wire).map(|input| json!(transform_position(&input, &[]).unwrap()))
        };
        if scalar.is_err() {
            assert!(output.is_err(), "shared UTF8 refusal for valid mapper JSON");
            assert!(
                c["outputJson"]
                    .as_str()
                    .unwrap()
                    .contains(c["symbolJson"].as_str().unwrap()),
                "TS retains lone surrogate"
            );
        } else {
            let expected: Value = serde_json::from_str(c["outputJson"].as_str().unwrap()).unwrap();
            same(
                &output.unwrap(),
                &expected,
                "Unicode mapper positive corpus",
            );
        }
    }
    for c in data["aliases"].as_array().unwrap() {
        let kind = c["kind"].as_str().unwrap();
        assert_eq!(c["sameReference"], true, "{kind} live info reference");
        assert_eq!(
            c["afterSource"],
            json!({"source":"SOURCE_AFTER","info":"SOURCE_AFTER","symbol":"BTC"})
        );
        assert_eq!(
            c["afterInfo"],
            json!({"source":"INFO_AFTER","info":"INFO_AFTER","symbol":"BTC"})
        );
        match kind {
            "market" => {
                let mut input: safe_cex_venue_asterdex::types::AsterDEXMarketResponse =
                    serde_json::from_value(c["input"].clone()).unwrap();
                let mut output =
                    transform_market(&input, &std::collections::BTreeMap::new()).unwrap();
                input.symbol = "SOURCE_AFTER".to_owned();
                assert_eq!(output.symbol, "BTC", "{kind} mapped scalar snapshot");
                assert_eq!(
                    output.info.as_ref().unwrap()["symbol"],
                    "BTC",
                    "{kind} owned info snapshot"
                );
                output.info.as_mut().unwrap()["symbol"] = json!("INFO_AFTER");
                assert_eq!(
                    input.symbol, "SOURCE_AFTER",
                    "{kind} typed info mutation cannot reach source"
                );
                assert_eq!(output.info.as_ref().unwrap()["symbol"], "INFO_AFTER");
            }
            "ticker" => {
                let mut input: safe_cex_venue_asterdex::types::AsterDEXTickerResponse =
                    serde_json::from_value(c["input"].clone()).unwrap();
                let mut output = transform_ticker(&input, None).unwrap();
                input.symbol = "SOURCE_AFTER".to_owned();
                assert_eq!(output.symbol, "BTC", "{kind} mapped scalar snapshot");
                assert_eq!(
                    output.info.as_ref().unwrap()["symbol"],
                    "BTC",
                    "{kind} owned info snapshot"
                );
                output.info.as_mut().unwrap()["symbol"] = json!("INFO_AFTER");
                assert_eq!(
                    input.symbol, "SOURCE_AFTER",
                    "{kind} typed info mutation cannot reach source"
                );
                assert_eq!(output.info.as_ref().unwrap()["symbol"], "INFO_AFTER");
            }
            "position" => {
                let mut input = c["input"].clone();
                let mut output = transform_position(&input, &[]).unwrap();
                input["symbol"] = json!("SOURCE_AFTER");
                assert_eq!(output.symbol, "BTC", "{kind} mapped scalar snapshot");
                assert_eq!(
                    output.info.as_ref().unwrap()["symbol"],
                    "BTC",
                    "{kind} owned info snapshot"
                );
                output.info.as_mut().unwrap()["symbol"] = json!("INFO_AFTER");
                assert_eq!(
                    input["symbol"], "SOURCE_AFTER",
                    "{kind} typed info mutation cannot reach source"
                );
                assert_eq!(output.info.as_ref().unwrap()["symbol"], "INFO_AFTER");
            }
            _ => panic!("unknown alias kind {kind}"),
        }
    }
    for c in data["markets"].as_array().unwrap() {
        let input = serde_json::from_value(c["input"].clone()).unwrap();
        let mut brackets = std::collections::BTreeMap::new();
        if !c["leverage"].is_null() {
            let value = c["leverage"].as_f64().unwrap_or_else(|| {
                match c["leverage"]["number"].as_str().unwrap() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    "-0" => -0.0,
                    v => panic!("unknown number {v}"),
                }
            });
            brackets.insert(
                "BTC".to_owned(),
                safe_cex_exchange::js_runtime::value::Js::Number(value),
            );
        }
        let market = transform_market(&input, &brackets).unwrap();
        let mut actual = json!(market);
        actual["precision"]["price"] = number(market.precision.price);
        actual["precision"]["amount"] = number(market.precision.amount);
        actual["limits"]["amount"]["min"] = number(market.limits.amount.min);
        actual["limits"]["amount"]["max"] = number(market.limits.amount.max);
        actual["limits"]["leverage"]["max"] = number(market.limits.leverage.max);
        same(
            &number(market.precision.price),
            &c["output"]["precision"]["price"],
            "transformMarket native price precision identity",
        );
        same(
            &number(market.precision.amount),
            &c["output"]["precision"]["amount"],
            "transformMarket native amount precision identity",
        );
        same(&actual, &c["output"], &format!("transformMarket {c}"));
    }
    for c in data["marketSurrogates"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<safe_cex_venue_asterdex::types::AsterDEXMarketResponse>(
                c["inputJson"].as_str().unwrap()
            )
            .is_err(),
            "shared UTF8 refusal for valid market JSON"
        );
        assert!(
            c["outputJson"]
                .as_str()
                .unwrap()
                .contains(c["symbolJson"].as_str().unwrap()),
            "TS transformMarket retains lone surrogate"
        );
    }
    for c in data["marketNonfiniteInfo"].as_array().unwrap() {
        let mut wire = c["input"].clone();
        wire["pricePrecision"] = json!(0);
        let mut input: safe_cex_venue_asterdex::types::AsterDEXMarketResponse =
            serde_json::from_value(wire).unwrap();
        input.price_precision = if c["precision"]["number"] == "NaN" {
            f64::NAN
        } else {
            f64::NEG_INFINITY
        };
        let result = transform_market(&input, &std::collections::BTreeMap::new());
        if input.price_precision.is_nan() {
            assert!(
                result.unwrap_err().1.contains("refuses NaN"),
                "native market metadata refuses NaN visibly"
            );
            continue;
        }
        let market = result.unwrap();
        let retained = safe_cex_num::numeric_ingress::js_value_as_f64(
            &market.info.as_ref().unwrap()["pricePrecision"],
        )
        .unwrap();
        assert_eq!(
            retained.to_bits(),
            input.price_precision.to_bits(),
            "native market metadata bits"
        );
        let mut actual = json!(market);
        actual["precision"]["price"] = number(market.precision.price);
        actual["precision"]["amount"] = number(market.precision.amount);
        same(
            &number(market.precision.price),
            &c["output"]["precision"]["price"],
            "transformMarket native price precision identity",
        );
        same(
            &number(market.precision.amount),
            &c["output"]["precision"]["amount"],
            "transformMarket native amount precision identity",
        );
        let expected = c["output"].clone();
        actual["info"]["pricePrecision"] = number(retained);
        same(&actual, &expected, "transformMarket nonfinite full output");
        assert_eq!(
            c["output"]["info"]["pricePrecision"], c["precision"],
            "TS retains nonfinite info"
        );
    }
    for c in data["numericInfo"].as_array().unwrap() {
        let field = c["field"].as_str().unwrap();
        let mut wire = c["input"].clone();
        wire[field] = json!(0);
        let value = match c["raw"]["number"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => unreachable!(),
        };
        let result = if c["kind"] == "ticker" {
            let mut input: safe_cex_venue_asterdex::types::AsterDEXTickerResponse =
                serde_json::from_value(wire).unwrap();
            input.close_time = value;
            transform_ticker(&input, None)
                .map(|output| output.info.unwrap())
                .map_err(|e| e.to_string())
        } else {
            safe_cex_types::retained_json::RetainedJsonInput::Float(value)
                .into_value()
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    wire[field] = raw;
                    transform_position(&wire, &[])
                        .map(|output| output.info.unwrap())
                        .map_err(|e| e.1)
                })
        };
        assert_eq!(
            c["raw"],
            number(value),
            "TS in-memory nonfinite numeric info"
        );
        if value.is_nan() {
            assert!(
                result.unwrap_err().to_string().contains("refuses NaN"),
                "native metadata NaN refusal"
            );
            continue;
        }
        let info = result.unwrap();
        assert_eq!(
            safe_cex_num::numeric_ingress::js_value_as_f64(&info[field])
                .unwrap()
                .to_bits(),
            value.to_bits(),
            "native {} metadata bits {field}",
            c["kind"]
        );
    }
    for c in data["tickers"].as_array().unwrap() {
        let input = serde_json::from_value(c["input"].clone()).unwrap();
        let mut map = std::collections::BTreeMap::new();
        if !c["oi"].is_null() {
            let oi = c["oi"].as_f64().unwrap_or_else(|| {
                if c["oi"]["number"] == "NaN" {
                    f64::NAN
                } else {
                    f64::INFINITY
                }
            });
            map.insert("BTC".to_owned(), oi);
        }
        let t = transform_ticker(&input, Some(&map)).unwrap();
        let mut actual = json!(t);
        for (k, v) in [
            ("last", t.last),
            ("percentage", t.percentage),
            ("openInterest", t.open_interest),
            ("volume", t.volume),
            ("quoteVolume", t.quote_volume),
        ] {
            actual[k] = number(v);
        }
        same(&actual, &c["output"], &format!("transformTicker {c}"));
    }
    for c in data["positions"].as_array().unwrap() {
        let input = serde_json::from_value(c["input"].clone()).unwrap();
        let base = transform_position(&input, &[]).unwrap();
        let stored = c["stored"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| {
                let mut p = base.clone();
                p.id = value["id"].as_str().unwrap().to_owned();
                p.realized_pnl = (value["realizedPnl"].as_f64()).into();
                p
            })
            .collect::<Vec<_>>();
        let p = transform_position(&input, &stored).unwrap();
        let mut actual = json!(p);
        for (k, v) in [
            ("contracts", p.contracts),
            ("entryPrice", p.entry_price),
            ("notional", p.notional),
            ("leverage", p.leverage),
            ("unrealizedPnl", p.unrealized_pnl),
            ("liquidationPrice", p.liquidation_price),
            ("realizedPnl", p.realized_pnl.unwrap()),
        ] {
            actual[k] = number(v);
        }
        same(&actual, &c["output"], &format!("transformPosition {c}"));
    }
    for c in data["algoLimits"].as_array().unwrap() {
        let market: safe_cex_types::Market=serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":1},"limits":{"amount":{"min":0,"max":1},"leverage":{"min":1,"max":10}},"info":c.get("info").cloned().unwrap_or(Value::Null)})).unwrap();
        let actual = match get_max_algo_orders_limit(Some(&market)) {
            Ok(value) => json!({"value":value}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        same(&actual, &c["output"], &format!("getMaxAlgoOrdersLimit {c}"));
    }
    for c in data["canSign"].as_array().unwrap() {
        let options = serde_json::from_value(c["options"].clone()).unwrap();
        let version = serde_json::from_value(c["version"].clone()).unwrap();
        assert_eq!(
            json!(can_sign(&options, &version, c["remote"].as_bool().unwrap())),
            c["output"],
            "canSign {c}"
        );
    }
    for c in data["paths"].as_array().unwrap() {
        let version = serde_json::from_value(c["version"].clone()).unwrap();
        let over = c
            .get("override")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            json!(get_fapi_path(
                c["endpoint"].as_str().unwrap(),
                &version,
                over
            )),
            c["output"],
            "getFapiPath {c}"
        );
        assert_eq!(
            json!(get_account_endpoint(&version)),
            c["account"],
            "getAccountEndpoint {c}"
        );
    }
    for c in data["orderTypes"].as_array().unwrap() {
        let actual = match map_order_type(serde_json::from_value(c["input"].clone()).unwrap()) {
            Ok(value) => json!({"value":value}),
            Err(message) => json!({"error":{"name":"Error","message":message}}),
        };
        same(&actual, &c["output"], &format!("mapOrderType {c}"));
    }
    for c in data["timeframes"].as_array().unwrap() {
        assert_eq!(
            json!(map_timeframe(
                serde_json::from_value(c["input"].clone()).unwrap()
            )),
            c["output"],
            "mapTimeframe {c}"
        );
    }
    for c in data["ticks"].as_array().unwrap() {
        let market: safe_cex_types::Market=serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.25,"amount":1},"limits":{"amount":{"min":0,"max":1},"leverage":{"min":1,"max":10}},"info":c["market"]["info"]})).unwrap();
        let actual = match get_tick_size(&market) {
            Ok(value) => json!({"value":number(value)}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        same(&actual, &c["output"], &format!("getTickSize {c}"));
    }
    for c in data["capacities"].as_array().unwrap() {
        let market: safe_cex_types::Market=serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":1},"limits":{"amount":{"min":0,"max":1},"leverage":{"min":1,"max":10}},"info":c.get("info").cloned().unwrap_or(Value::Null)})).unwrap();
        let orders: Vec<safe_cex_types::Order> =
            serde_json::from_value(c["orders"].clone()).unwrap();
        let added =
            c["added"]
                .as_f64()
                .unwrap_or_else(|| match c["added"]["number"].as_str().unwrap() {
                    "NaN" => f64::NAN,
                    "-0" => -0.0,
                    _ => f64::INFINITY,
                });
        let actual = match assert_algo_order_capacity("BTC", added, Some(&market), &orders) {
            Ok(()) => json!({"value":null}),
            Err((name, message)) => json!({"error":{"name":name,"message":message}}),
        };
        same(
            &actual,
            &c["output"],
            &format!("assertAlgoOrderCapacity {c}"),
        );
    }
    for c in data["counts"].as_array().unwrap() {
        let orders: Vec<safe_cex_types::Order> =
            serde_json::from_value(c["orders"].clone()).unwrap();
        assert_eq!(
            json!(get_open_algo_orders_count("BTC", &orders)),
            c["output"],
            "getOpenAlgoOrdersCount {c}"
        );
    }
    for c in data["side"].as_array().unwrap() {
        let opts: safe_cex_types::PlaceOrderOpts =
            serde_json::from_value(c["opts"].clone()).unwrap();
        assert_eq!(
            json!(get_order_position_side(
                &opts,
                c["isHedged"].as_bool().unwrap()
            )),
            c["output"],
            "getOrderPositionSide {c}"
        );
    }
    for c in data["interest"].as_array().unwrap() {
        let mut map = Map::new();
        map.insert("existing".into(), json!(3));
        let result = collect_open_interest_from_payload(c.get("input"), &mut map);
        let actual = match result {
            Ok(()) => json!({"value":null}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        if c.get("error").is_some() {
            assert_eq!(
                actual["error"], c["error"],
                "collectOpenInterestFromPayload {c}"
            );
        } else {
            assert_eq!(
                actual,
                json!({"value": c["value"]}),
                "collectOpenInterestFromPayload {c}"
            );
        }
        let entries = map
            .iter()
            .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
            .collect::<Vec<_>>();
        let expected = c["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v[0].as_str().unwrap().to_owned(), v[1].as_f64().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            entries, expected,
            "collectOpenInterestFromPayload partial state/order {c}"
        );
    }
    for c in data["algo"].as_array().unwrap() {
        assert_eq!(
            json!(is_algo_order(
                serde_json::from_value(c["type"].clone()).unwrap(),
                c.get("info")
            )),
            c["output"],
            "isAlgoOrder {c}"
        );
    }
    for c in data["unknownErrors"].as_array().unwrap() {
        let actual =
            match is_aster_dex_unknown_order_error(c.get("input"), c["nativeMessage"].as_str()) {
                Ok(value) => json!({"value":value}),
                Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
            };
        assert_eq!(actual, c["output"], "isAsterDEXUnknownOrderError {c}");
    }
    for c in data["serverTimes"].as_array().unwrap() {
        assert_eq!(
            parse_server_time(c.get("input")),
            c["output"].as_f64(),
            "parseServerTime {c}"
        );
    }
    for c in data["clockPaths"].as_array().unwrap() {
        assert_eq!(
            json!(get_time_path_candidates(
                serde_json::from_value(c["input"].clone()).unwrap()
            )),
            c["output"],
            "getTimePathCandidates {c}"
        );
    }
    for c in data["reverseStatuses"].as_array().unwrap() {
        use safe_cex_num::{ProtoMember, TableHit};
        let actual = match reverse_map_order_status(c["input"].as_str().unwrap()) {
            TableHit::Mapped(status) => json!({"mapped":status}),
            TableHit::Inherited(member) => json!({
                "inherited":format!("{member:?}"),
                "type":if member == ProtoMember::Prototype {"object"} else {"function"}
            }),
            TableHit::Missing => panic!("reverseMapOrderStatus must always apply fallback"),
        };
        assert_eq!(actual, c["output"], "reverseMapOrderStatus {c}");
    }
    for c in data["reverseStatusSurrogates"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(c["inputJson"].as_str().unwrap()).is_err(),
            "shared UTF-8 representation refusal for valid JSON string"
        );
        assert_eq!(
            c["output"], "open",
            "TypeScript accepts the lone surrogate before fallback"
        );
    }
    for c in data["reverseTypes"].as_array().unwrap() {
        use safe_cex_num::{ProtoMember, TableHit};
        let actual = match reverse_map_order_type(c["input"].as_str().unwrap()) {
            TableHit::Mapped(value) => json!({"mapped":value}),
            TableHit::Inherited(member) => {
                json!({"inherited":format!("{member:?}"),"type":if member==ProtoMember::Prototype {"object"} else {"function"}})
            }
            TableHit::Missing => panic!("reverseMapOrderType must apply passthrough"),
        };
        assert_eq!(actual, c["output"], "reverseMapOrderType {c}");
    }
    for c in data["filterValues"].as_array().unwrap() {
        let actual = match extract_filter_value(
            c["filters"].as_array().unwrap(),
            "LOT_SIZE",
            c["field"].as_str().unwrap(),
        ) {
            Ok(value) => json!({"value":number(value)}),
            Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
        };
        same(&actual, &c["output"], &format!("extractFilterValue {c}"));
    }
    for c in data["lookupSurrogates"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(c["inputJson"].as_str().unwrap()).is_err(),
            "UTF8 representation refusal for valid JSON"
        );
        assert_eq!(
            c["typeOutputJson"], c["inputJson"],
            "TS preserves lone surrogate in reverseMapOrderType"
        );
        assert_eq!(
            c["filterOutput"], 3,
            "TS accepts lone surrogate in extractFilterValue"
        );
    }
    for c in data["tif"].as_array().unwrap() {
        assert_eq!(
            json!(reverse_map_time_in_force(c["input"].as_str().unwrap())),
            c["output"],
            "reverseMapTimeInForce {c}"
        );
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_trades_match_typescript_closed_output_finding() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-trades-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["request"]["url"], "/fapi/v1/trades",
        "fetchPublicTrades endpoint"
    );
    assert_eq!(data["request"]["params"]["symbol"], data["symbol"]);
    assert_eq!(data["request"]["params"]["limit"], 37);
    let trades = data["trades"].as_array().unwrap();
    let outputs = data["output"].as_array().unwrap();
    let source_trade_count = 3;
    assert_eq!(
        trades.len(),
        source_trade_count,
        "fetchPublicTrades source trade count"
    );
    assert_eq!(
        outputs.len(),
        source_trade_count,
        "fetchPublicTrades TypeScript output count"
    );
    assert_eq!(
        trades.len(),
        outputs.len(),
        "fetchPublicTrades source/output cardinality"
    );
    let mut native_trade_count = 0;
    for (index, (trade, mapped)) in trades.iter().zip(outputs).enumerate() {
        let source: safe_cex_venue_asterdex::types::AsterDEXTradeResponse =
            serde_json::from_value(trade.clone()).unwrap();
        let result = map_public_trade(&source, data["symbol"].as_str().unwrap());
        let expected_side = if source.is_buyer_maker { "SELL" } else { "BUY" };
        assert!(result.is_err(), "fetchPublicTrades closed side boundary");
        let error = result.unwrap_err();
        assert_eq!(error.0, "RepresentationError", "fetchPublicTrades side");
        assert!(
            error.1.contains("side"),
            "fetchPublicTrades side: {error:?}"
        );
        assert_eq!(
            mapped["side"], expected_side,
            "fetchPublicTrades side value"
        );
        assert_eq!(
            data["sideTypes"][index], "string",
            "fetchPublicTrades typeof before serialization"
        );
        assert_eq!(mapped["id"], safe_cex_num::js_number_string(source.id));
        assert_eq!(mapped["symbol"], data["symbol"]);
        assert_eq!(mapped["side"].as_str().map(|_| "string"), Some("string"));
        assert!(
            mapped.get("side").is_some(),
            "TypeScript retains the full trade record"
        );
        native_trade_count += 1;
    }
    assert_eq!(
        native_trade_count, source_trade_count,
        "fetchPublicTrades native cardinality"
    );
    assert_eq!(
        native_trade_count,
        outputs.len(),
        "fetchPublicTrades native/output cardinality"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn public_trades_oracle_rejects_lowercase_source_mutation() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "public_trades_match_typescript_closed_output_finding",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_TRADE_CONTROL", "lowercase-side")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("fetchPublicTrades side value"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn worker_hook_matches_typescript_truthiness() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/worker-hook-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["cases"].as_array().unwrap() {
        let arg = case["args"].as_array().unwrap().first();
        let first_arg = arg.filter(|value| value.get("undefined") != Some(&Value::Bool(true)));
        let mut actual_calls = Vec::new();
        let result = after_method_called_in_worker(
            case["method"].as_str().unwrap(),
            first_arg,
            |key, value| {
                actual_calls.push(json!([key, value]));
                Ok(())
            },
        );
        assert!(result.is_ok(), "afterMethodCalledInWorker {case}");
        let expected_calls = case["calls"].as_array().unwrap();
        assert_eq!(
            &actual_calls, expected_calls,
            "afterMethodCalledInWorker {case}"
        );
    }
}

#[test]
fn worker_hook_propagates_store_failure_after_mutation() {
    let mut hedged = false;
    let error = after_method_called_in_worker(
        "changePositionMode",
        Some(&Value::Bool(true)),
        |key, value| {
            assert_eq!(key, "isHedged");
            hedged = value;
            Err(safe_cex_store::StoreMutationError::Notification(
                safe_cex_store::StoreNotificationError::from_subscriber(
                    17,
                    safe_cex_types::StoreSubscriberError::new("subscriber failed"),
                ),
            )
            .into())
        },
    )
    .unwrap_err();
    assert!(hedged, "the store mutation precedes notification failure");
    assert_eq!(error.to_string(), "subscriber failed");
    assert_eq!(error.store_subscriber_ids, vec![17]);
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn worker_hook_oracle_rejects_inverted_source_control() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "worker_hook_matches_typescript_truthiness",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_WORKER_CONTROL", "invert")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("afterMethodCalledInWorker"),
        "{diagnostics}"
    );
}

fn number(value: f64) -> Value {
    if value.is_nan() {
        json!({"number":"NaN"})
    } else if value == f64::INFINITY {
        json!({"number":"Infinity"})
    } else if value == f64::NEG_INFINITY {
        json!({"number":"-Infinity"})
    } else if value == 0.0 && value.is_sign_negative() {
        json!({"number":"-0"})
    } else {
        json!(value)
    }
}
fn same(actual: &Value, expected: &Value, context: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{context}"),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{context}");
            for (k, v) in a {
                same(
                    v,
                    b.get(k).unwrap_or_else(|| panic!("missing {k}: {context}")),
                    context,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{context}");
            for (a, b) in a.iter().zip(b) {
                same(a, b, context);
            }
        }
        _ => assert_eq!(actual, expected, "{context}"),
    }
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn reverse_status_oracle_rejects_swapped_function_identity() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_STATUS_CONTROL", "swapped")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "swapped function identity must fail: {diagnostics}"
    );
    assert!(
        diagnostics.contains("reverseMapOrderStatus")
            && diagnostics.contains("ToString")
            && diagnostics.contains("Constructor"),
        "control must reach identity comparison: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn filter_oracle_rejects_own_only_lookup() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_FILTER_CONTROL", "own-only")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "own-only lookup must fail: {diagnostics}"
    );
    assert!(
        diagnostics.contains("extractFilterValue") && diagnostics.contains("constructor"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn market_oracle_rejects_nullish_leverage_fallback() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARKET_CONTROL", "nullish")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("transformMarket"), "{diagnostics}");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn market_oracle_rejects_nonfinite_precision_correction() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MARKET_CONTROL", "nonfinite-precision")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        // Native assertions precede the full-output comparison.
        diagnostics.contains("transformMarket native price precision identity"),
        "{diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn account_mode_oracle_rejects_inverted_flag() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange_helpers_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_MODE_CONTROL", "inverted")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(
        diagnostics.contains("getAccountPositionMode"),
        "{diagnostics}"
    );
}

fn tagged_order(order: &safe_cex_types::Order) -> Value {
    let mut value = json!(order);
    for (key, number_value) in [
        ("price", order.price.to_number()),
        ("amount", order.amount),
        ("filled", order.filled),
        ("remaining", order.remaining),
        ("lastUpdate", order.last_update),
    ] {
        value[key] = number(number_value);
    }
    value
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn order_mapper_matches_typescript() {
    let process = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        process.status.success(),
        "{}",
        String::from_utf8_lossy(&process.stderr)
    );
    let data: Value = serde_json::from_slice(&process.stdout).unwrap();
    for case in data["orders"].as_array().unwrap() {
        let input = serde_json::from_value(case["input"].clone()).unwrap();
        same(
            &tagged_order(&transform_order(&input).unwrap()),
            &case["output"],
            &format!("transformOrder {case}"),
        );
    }
    for case in data["orderRefusals"].as_array().unwrap() {
        let input = serde_json::from_value(case["input"].clone()).unwrap();
        let error = transform_order(&input).unwrap_err();
        assert_eq!(error.0, "RepresentationError");
        assert!(error.1.contains(case["field"].as_str().unwrap()));
        let key = case["key"].as_str().unwrap();
        let field = case["field"].as_str().unwrap();
        let serialized: Value = serde_json::from_str(case["serialized"].as_str().unwrap()).unwrap();
        assert_eq!(
            serialized["id"], "123",
            "TS still returns the whole usable record"
        );
        match safe_cex_num::js_table_lookup::<&str, Value>(&[], key) {
            safe_cex_num::TableHit::Inherited(member) => {
                assert_eq!(
                    case["coercion"],
                    member.js_string(),
                    "transformOrder inherited identity {key}"
                );
                if key == "__proto__" {
                    assert_eq!(case["typeof"], "object");
                    assert_eq!(
                        serialized[field],
                        json!({}),
                        "FINDING prototype object survives JSON"
                    );
                } else {
                    assert_eq!(case["typeof"], "function");
                    assert!(
                        serialized.get(field).is_none(),
                        "function field alone omitted"
                    );
                }
            }
            _ => {
                assert_eq!(case["typeof"], "string");
                assert_eq!(serialized[field], key, "unknown string survives TS output");
            }
        }
    }
    for case in data["orderNumericInfo"].as_array().unwrap() {
        let field = case["field"].as_str().unwrap();
        let value = match case["raw"]["number"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => unreachable!(),
        };
        let mut wire = case["input"].clone();
        wire[field] = json!(0);
        let mut input: safe_cex_venue_asterdex::types::AsterDEXOrderResponse =
            serde_json::from_value(wire).unwrap();
        match field {
            "orderId" => input.order_id = value,
            "time" => input.time = value,
            "updateTime" => input.update_time = value,
            "workingTime" => input.working_time = value,
            _ => unreachable!(),
        }
        let result = transform_order(&input);
        if value.is_nan() {
            assert!(
                result.unwrap_err().1.contains("refuses NaN"),
                "native order metadata NaN refusal"
            );
            continue;
        }
        let output = result.unwrap();
        let retained =
            safe_cex_num::numeric_ingress::js_value_as_f64(&output.info.as_ref().unwrap()[field])
                .unwrap();
        assert_eq!(
            retained.to_bits(),
            value.to_bits(),
            "native order metadata bits {field}"
        );
        let expected = case["output"].clone();
        assert_eq!(
            expected["info"][field],
            number(value),
            "live pre-JSON info number"
        );
        let mut actual = tagged_order(&output);
        actual["info"][field] = number(retained);
        same(
            &actual,
            &expected,
            &format!("transformOrder nonfinite {field}"),
        );
    }
    for case in data["orderSignedZeroIngress"].as_array().unwrap() {
        let field = case["field"].as_str().unwrap();
        let raw: Value = serde_json::from_str(case["inputJson"].as_str().unwrap()).unwrap();
        for constructed in [false, true] {
            let mut raw = raw.clone();
            if constructed {
                raw[field] = serde_json::json!(-0.0_f64);
            }
            assert_eq!(raw[field].as_f64().unwrap().to_bits(), (-0.0_f64).to_bits());
            let input: safe_cex_venue_asterdex::types::AsterDEXOrderResponse =
                serde_json::from_value(raw).unwrap();
            let value = match field {
                "orderId" => input.order_id,
                "time" => input.time,
                "updateTime" => input.update_time,
                "workingTime" => input.working_time,
                _ => unreachable!(),
            };
            assert_eq!(
                format!("{:016x}", value.to_bits()),
                case["bits"].as_str().unwrap(),
                "transformOrder signed-zero ingress {field} constructed={constructed}"
            );
        }
    }
    for case in data["orderOverflowIngress"].as_array().unwrap() {
        let raw: Value = serde_json::from_str(case["inputJson"].as_str().unwrap()).unwrap();
        let field = case["field"].as_str().unwrap();
        // Candidate ingress ordering; numeric bit witnesses above are independently required.
        let input: safe_cex_venue_asterdex::types::AsterDEXOrderResponse =
            if std::env::var("ASTERDEX_INGRESS_CONTROL").as_deref() == Ok("direct") {
                serde_json::from_str(case["inputJson"].as_str().unwrap())
            } else {
                serde_json::from_value(raw)
            }
            .unwrap_or_else(|error| panic!("transformOrder Value-first ingress {field}: {error}"));
        let value = match field {
            "orderId" => input.order_id,
            "time" => input.time,
            "updateTime" => input.update_time,
            "workingTime" => input.working_time,
            _ => unreachable!(),
        };
        assert_eq!(
            number(value),
            case["raw"],
            "transformOrder Value-first ingress {field}"
        );
        let output = transform_order(&input).unwrap();
        let expected = case["output"].clone();
        assert_eq!(expected["info"][field], case["raw"]);
        let retained =
            safe_cex_num::numeric_ingress::js_value_as_f64(&output.info.as_ref().unwrap()[field])
                .unwrap();
        assert_eq!(
            retained.to_bits(),
            value.to_bits(),
            "native order overflow metadata bits {field}"
        );
        let mut actual = tagged_order(&output);
        actual["info"][field] = number(retained);
        same(&actual, &expected, "transformOrder overflow ingress output");
    }
    for case in data["orderStrings"].as_array().unwrap() {
        let input = serde_json::from_str::<safe_cex_venue_asterdex::types::AsterDEXOrderResponse>(
            case["inputJson"].as_str().unwrap(),
        );
        if serde_json::from_str::<String>(case["symbolJson"].as_str().unwrap()).is_err() {
            assert!(
                input.is_err(),
                "valid JSON lone surrogate representation refusal"
            );
            assert!(
                case["outputJson"]
                    .as_str()
                    .unwrap()
                    .contains(case["symbolJson"].as_str().unwrap())
            );
        } else {
            same(
                &json!(transform_order(&input.unwrap()).unwrap()),
                &serde_json::from_str(case["outputJson"].as_str().unwrap()).unwrap(),
                "transformOrder Unicode",
            );
        }
    }
    let case = &data["orderAlias"];
    assert_eq!(case["sameReference"], true);
    assert_eq!(
        case["afterSource"],
        json!({"source":"SOURCE_AFTER","info":"SOURCE_AFTER","symbol":"BTC"})
    );
    assert_eq!(
        case["afterInfo"],
        json!({"source":"INFO_AFTER","info":"INFO_AFTER","symbol":"BTC"})
    );
    let mut input: safe_cex_venue_asterdex::types::AsterDEXOrderResponse =
        serde_json::from_value(case["input"].clone()).unwrap();
    let mut output = transform_order(&input).unwrap();
    input.symbol = "SOURCE_AFTER".to_owned();
    assert_eq!(output.symbol, "BTC");
    assert_eq!(output.info.as_ref().unwrap()["symbol"], "BTC");
    output.info.as_mut().unwrap()["symbol"] = json!("INFO_AFTER");
    assert_eq!(input.symbol, "SOURCE_AFTER");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn order_oracle_rejects_zero_stop_price() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "order_mapper_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_ORDER_CONTROL", "zero-stop")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{diagnostics}");
    assert!(diagnostics.contains("transformOrder"), "{diagnostics}");
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn order_mapper_accepts_direct_typed_overflow_ingress() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "order_mapper_matches_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_INGRESS_CONTROL", "direct")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0), "{diagnostics}");
    assert!(
        diagnostics.contains("1 passed"),
        "direct-ingress comparison must execute: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn state_versions_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let decode = |value: &Value| match value["number"].as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => value.as_f64().unwrap(),
    };
    let version_states = data["versionStates"].as_array().unwrap();
    assert_eq!(
        version_states.len(),
        11,
        "state version pinned source case count"
    );
    let mut native_state_checks = 0;
    let mut native_state_steps = 0;
    for case_index in 0..11 {
        let case = &version_states[case_index];
        let steps = case["steps"].as_array().unwrap();
        assert_eq!(
            steps.len(),
            8,
            "state version source steps case {case_index}"
        );
        let mut position = decode(&case["position"]);
        let mut balance = decode(&case["balance"]);
        for step_index in 0..8 {
            let step = &steps[step_index];
            let method = step["method"].as_str().unwrap();
            let returned = match method {
                "getPositionStateVersion" => number(get_position_state_version(position)),
                "getBalanceStateVersion" => number(get_balance_state_version(balance)),
                "markPositionStateUpdatedByWebsocket" => {
                    mark_position_state_updated_by_websocket(&mut position);
                    json!({"undefined":true})
                }
                "markBalanceStateUpdatedByWebsocket" => {
                    number(mark_balance_state_updated_by_websocket(&mut balance))
                }
                _ => panic!("unexpected version method"),
            };
            same(&returned, &step["returned"], method);
            same(&number(position), &step["position"], method);
            same(&number(balance), &step["balance"], method);
            native_state_checks += 3;
            native_state_steps += 1;
        }
    }
    assert_eq!(
        native_state_steps, 88,
        "state version fixed native step count"
    );
    assert_eq!(
        native_state_checks, 264,
        "state version fixed native comparison count"
    );
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn state_version_oracle_rejects_postincrement_return() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "state_versions_match_typescript",
            "--include-ignored",
            "--nocapture",
        ])
        .env("ASTERDEX_VERSION_CONTROL", "postincrement")
        .output()
        .unwrap();
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "version source control: {diagnostics}"
    );
    assert!(
        diagnostics.contains("markBalanceStateUpdatedByWebsocket")
            && !diagnostics.contains("control source missing"),
        "control reaches named comparison: {diagnostics}"
    );
}

#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn mapper_metadata_retains_native_numbers() {
    use safe_cex_venue_asterdex::types::*;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = data["numericMetadata"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        18 * 5 - 2,
        "18 native cells; two unbounded positive-infinite precision cases excluded"
    );
    let mut cells = std::collections::BTreeSet::new();
    for (iteration, case) in cases.iter().enumerate() {
        let kind = case["kind"].as_str().unwrap();
        let pointer = case["pointer"].as_str().unwrap();
        cells.insert((kind, pointer));
        let bits = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
        let value = f64::from_bits(bits);
        let mut wire = case["input"].clone();
        *wire.pointer_mut(pointer).unwrap() = json!(0);
        macro_rules! mapped {
            ($ty:ty, $input:ident, $map:expr, $( $path:literal => $field:ident $(.$nested:tt)* ),+ $(,)?) => {{
                let mut $input: $ty = serde_json::from_value(wire).unwrap();
                match pointer { $( $path => $input.$field$(.$nested)* = value, )+ _ => panic!("uncovered native cell {kind}{pointer}") }
                $map
            }};
        }
        let result: Result<Value, String> = match kind {
            "ticker" => mapped!(AsterDEXTickerResponse, input,
                transform_ticker(&input,None).map(|o|o.info.unwrap()).map_err(|e|e.to_string()),
                "/openTime"=>open_time, "/closeTime"=>close_time, "/firstId"=>first_id, "/lastId"=>last_id, "/count"=>count),
            "position" => safe_cex_types::retained_json::RetainedJsonInput::Float(value)
                .into_value()
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    *wire.pointer_mut(pointer).unwrap() = raw;
                    transform_position(&wire, &[])
                        .map(|o| o.info.unwrap())
                        .map_err(|e| e.1)
                }),
            "order" => mapped!(AsterDEXOrderResponse, input,
                transform_order(&input).map(|o|o.info.unwrap()).map_err(|e|e.1),
                "/orderId"=>order_id, "/time"=>time, "/updateTime"=>update_time, "/workingTime"=>working_time),
            "market" => {
                let mut input: AsterDEXMarketResponse = serde_json::from_value(wire).unwrap();
                match pointer {
                    "/deliveryDate" => input.delivery_date = Some(value),
                    "/onboardDate" => input.onboard_date = Some(value),
                    "/pricePrecision" => input.price_precision = value,
                    "/quantityPrecision" => input.quantity_precision = value,
                    "/baseAssetPrecision" => input.base_asset_precision = value,
                    "/quotePrecision" => input.quote_precision = value,
                    "/settlePlan" => input.settle_plan = Some(value),
                    "/filters/0/limit" => input.filters[0].limit = Some(value),
                    _ => panic!("uncovered market cell {pointer}"),
                }
                transform_market(&input, &Default::default())
                    .map(|o| o.info.unwrap())
                    .map_err(|e| e.1)
            }
            _ => panic!("unknown mapper"),
        };
        if value.is_nan() {
            assert!(
                result.unwrap_err().contains("refuses NaN"),
                "visible native metadata NaN refusal {kind}{pointer}"
            );
        } else {
            let info = result.unwrap();
            let retained =
                safe_cex_num::numeric_ingress::js_value_as_f64(info.pointer(pointer).unwrap())
                    .unwrap();
            assert_eq!(
                retained.to_bits(),
                bits,
                "native metadata bits {kind}{pointer} iteration {iteration}"
            );
        }
    }
    assert_eq!(
        cells.len(),
        18,
        "independent native cell floor ends at case 87"
    );
}
