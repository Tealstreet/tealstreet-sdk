mod support;
use safe_cex_exchange::http::HttpClient;
use safe_cex_transport::*;
use safe_cex_venue_polymarket::{public_reads::*, types::*};
use serde_json::{Value, json};
use std::{
    process::Command,
    sync::{Arc, Mutex},
};
use support::{client, response, run};

enum Records {
    Markets(Vec<PolymarketClobMarket>),
    Book(PolymarketOrderBook),
    Books(Vec<PolymarketOrderBook>),
    Prices(Vec<PolymarketPrice>),
}
async fn invoke(name: &str, http: &HttpClient, ids: &[String]) -> Result<Records, RestFailure> {
    match name {
        "getMarkets" => get_markets(http).await.map(Records::Markets),
        "getOrderBook" => get_order_book(http, ids.first().map_or("", String::as_str))
            .await
            .map(Records::Book),
        "getOrderBooks" => get_order_books(http, ids).await.map(Records::Books),
        "getPrices" => get_prices(http, ids).await.map(Records::Prices),
        _ => panic!("unexpected source method"),
    }
}
fn string(actual: &str, expected: &Value, label: &str) {
    assert_eq!(actual, expected.as_str().unwrap(), "record_native_{label}");
}
fn timestamp(actual: Option<f64>, bits: &Value) {
    assert_eq!(
        actual.map(|v| format!("{:016x}", v.to_bits())),
        bits.as_str().map(str::to_owned),
        "record_native_timestamp"
    );
}
fn book(actual: &PolymarketOrderBook, expected: &Value, bits: &Value, empty: bool) -> usize {
    string(&actual.asset_id, &expected["asset_id"], "asset_id");
    let (bid_count, ask_count) = if empty { (0, 0) } else { (2, 1) };
    assert_eq!(actual.bids.len(), bid_count, "record_native_bid_count");
    assert_eq!(actual.asks.len(), ask_count, "record_native_ask_count");
    let mut levels = 0;
    for (key, side, count) in [
        ("bids", &actual.bids, bid_count),
        ("asks", &actual.asks, ask_count),
    ] {
        assert_eq!(
            expected[key].as_array().unwrap().len(),
            count,
            "record_source_level_count"
        );
        for index in 0..count {
            let level = side.get(index).expect("record_native_level_present");
            string(&level.price, &expected[key][index]["price"], "level_price");
            string(&level.size, &expected[key][index]["size"], "level_size");
            levels += 1;
        }
    }
    timestamp(actual.timestamp, bits);
    assert_eq!(
        actual.hash.as_deref(),
        expected["hash"].as_str(),
        "record_native_hash"
    );
    levels
}
fn compare_native(records: &Records, case: &Value) -> (usize, usize) {
    let empty = case["iteration"] == 1;
    let expected = &case["output"];
    let count = if empty { 0 } else { 2 };
    match records {
        Records::Markets(rows) => {
            assert_eq!(rows.len(), count, "record_native_market_count");
            assert_eq!(
                expected.as_array().unwrap().len(),
                count,
                "record_source_market_count"
            );
            for index in 0..count {
                let row = rows.get(index).expect("record_native_market_present");
                let expected = &expected[index];
                string(&row.condition_id, &expected["condition_id"], "condition_id");
                string(&row.token_id, &expected["token_id"], "market_token_id");
                string(&row.outcome, &expected["outcome"], "outcome");
                string(
                    &row.minimum_order_size,
                    &expected["minimum_order_size"],
                    "minimum_order_size",
                );
                string(
                    &row.minimum_tick_size,
                    &expected["minimum_tick_size"],
                    "minimum_tick_size",
                );
                assert_eq!(
                    row.active,
                    expected["active"].as_bool().unwrap(),
                    "record_native_active"
                );
                assert_eq!(
                    row.closed,
                    expected["closed"].as_bool().unwrap(),
                    "record_native_closed"
                );
            }
            (count, 0)
        }
        Records::Book(row) => (1, book(row, expected, &case["timestamps"][0], empty)),
        Records::Books(rows) => {
            assert_eq!(rows.len(), count, "record_native_book_count");
            assert_eq!(
                expected.as_array().unwrap().len(),
                count,
                "record_source_book_count"
            );
            let mut levels = 0;
            for index in 0..count {
                levels += book(
                    rows.get(index).expect("record_native_book_present"),
                    &expected[index],
                    &case["timestamps"][index],
                    false,
                );
            }
            (count, levels)
        }
        Records::Prices(rows) => {
            assert_eq!(rows.len(), count, "record_native_price_count");
            assert_eq!(
                expected.as_array().unwrap().len(),
                count,
                "record_source_price_count"
            );
            for index in 0..count {
                let row = rows.get(index).expect("record_native_price_present");
                string(
                    &row.token_id,
                    &expected[index]["token_id"],
                    "price_token_id",
                );
                string(&row.price, &expected[index]["price"], "price");
                timestamp(row.timestamp, &case["timestamps"][index]);
            }
            (count, 0)
        }
    }
}
fn request(actual: &RestRequest, case: &Value) {
    let calls = case["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1, "record_source_request_count");
    let expected = &calls[0];
    assert_eq!(actual.method, expected["verb"], "record_native_method");
    assert_eq!(actual.url, expected["args"][0], "record_native_url");
    if actual.method == "post" {
        assert!(actual.params.is_none(), "record_native_no_params");
        // Native response assertions above precede this transmitted body assertion.
        assert_eq!(
            actual.body,
            Some(RestBody::Encoded(
                serde_json::to_vec(&expected["args"][1]).unwrap()
            )),
            "record_wire_batch_body"
        );
    } else {
        assert_eq!(
            actual.params.as_ref(),
            expected["args"]
                .get(1)
                .and_then(|config| config.get("params")),
            "record_native_params"
        );
        assert!(actual.body.is_none(), "record_native_no_body");
    }
}
#[test]
fn record_reads_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/record_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    eprintln!("polymarket record reads oracle: one Node invocation for the entire corpus");
    assert!(
        output.status.success(),
        "record_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("capture header: {}", corpus["capture"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36, "record_source_case_count");
    let (mut accepted, mut refused, mut rows, mut levels) = (0, 0, 0, 0);
    let names = ["getMarkets", "getOrderBook", "getOrderBooks", "getPrices"];
    // Fixed 4 x 9 traversal stays load-bearing when source length guards are removed.
    for method in 0..4 {
        for iteration in 0..9 {
            let case = cases
                .get(method * 9 + iteration)
                .expect("record_source_case_present");
            assert_eq!(case["name"], names[method], "record_source_method_order");
            assert_eq!(case["iteration"], iteration, "record_source_iteration");
            assert_eq!(
                case["accepted"],
                !(matches!(iteration, 4..=6) || (method == 0 && iteration == 7)),
                "record_source_boundary"
            );
            let ids: Vec<String> = case["input"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_owned())
                .collect();
            let raw = case["raw"].as_str().unwrap();
            let mut bodies = vec![RestResponseBody::Encoded(raw.as_bytes().to_vec())];
            if case["encodedOnly"] != true {
                let value: Value = serde_json::from_str(raw).unwrap();
                bodies.push(RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value.clone())
                        .unwrap(),
                ));
                bodies.push(RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
                ));
            }
            assert_eq!(
                bodies.len(),
                if method == 0 && iteration == 7 { 1 } else { 3 },
                "record_provenance_count"
            );
            for body in bodies {
                let calls = Arc::new(Mutex::new(vec![]));
                let result = run(invoke(
                    names[method],
                    &client(Ok(response(body.clone())), calls.clone()),
                    &ids,
                ));
                if case["accepted"] == true {
                    let records = result.unwrap_or_else(|e| {
                        panic!("record_native_accept:{method}:{iteration}: {e:?}")
                    });
                    let (compared_rows, compared_levels) = compare_native(&records, case);
                    rows += compared_rows;
                    levels += compared_levels;
                    accepted += 1;
                } else {
                    let error = result.err().expect("record_typed_refusal");
                    assert_eq!(
                        error.response.unwrap().body,
                        body,
                        "record_decode_response_retained"
                    );
                    assert_eq!(
                        error.request.url, case["calls"][0]["args"][0],
                        "record_decode_request_retained"
                    );
                    assert!(
                        error.code.is_none()
                            && error.transport_error.is_none()
                            && error.context.is_none(),
                        "record_decode_no_network_cause"
                    );
                    refused += 1;
                }
                let calls = calls.lock().unwrap();
                assert_eq!(calls.len(), 1, "record_native_request_count");
                request(&calls[0], case);
            }
        }
    }
    assert_eq!(accepted, 69, "record_accepted_comparisons");
    assert_eq!(refused, 37, "record_refused_comparisons");
    assert_eq!(rows, 102, "record_performed_rows");
    assert_eq!(levels, 135, "record_performed_levels");
    let failures = corpus["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 4, "record_source_failure_count");
    for index in 0..4 {
        assert_eq!(
            failures[index]["name"], names[index],
            "record_source_failure_method"
        );
        assert_eq!(
            failures[index]["preserved"], true,
            "record_source_failure_identity"
        );
        let expected = RestFailure {
            code: Some("ERR_NETWORK".into()),
            transport_error: None,
            message: "fixture failure".into(),
            response: Some(Box::new(RestResponse {
                status: 429,
                ..response(RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"error":"limit"}),
                    )
                    .unwrap(),
                ))
            })),
            request: Box::new(RestRequest {
                url: "/changed".into(),
                ..Default::default()
            }),
            context: Some(Box::new(json!({"id":7}))),
        };
        let calls = Arc::new(Mutex::new(vec![]));
        let error = run(invoke(
            names[index],
            &client(Err(expected.clone()), calls.clone()),
            &["x".into(), "y".into()],
        ))
        .err()
        .expect("record_native_failure");
        assert_eq!(error, expected, "record_failure_metadata");
        let calls = calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "record_failure_request_count");
        request(&calls[0], &failures[index]);
    }
}
