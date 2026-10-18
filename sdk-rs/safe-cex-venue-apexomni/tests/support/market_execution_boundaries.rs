use super::*;

fn boundary_rows(history: bool) -> Value {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/market-execution-boundaries.cjs"
    ));
    command.env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH);
    if history {
        command.arg("history");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "APEX-BOUNDARY maintained source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        capture["rows"].as_array().unwrap().len(),
        if history { 21 } else { 32 },
        "APEX-BOUNDARY independent captured floor"
    );
    capture
}

fn venue(host: &Rc<Host>, store: Rc<DefaultStore>) -> Rc<ApexOmniExchange> {
    ApexOmniExchange::new(
        BaseExchange::new(
            ExchangeName::Apexomni,
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            store,
            host.clone(),
            ExchangeSettings::default(),
        ),
        host.clone(),
    )
    .unwrap()
}

#[test]
fn market_filter_errors_precede_typed_output_admission() {
    let capture = boundary_rows(false);
    let asset_rows = capture["assetRows"].as_array().unwrap();
    assert_eq!(
        asset_rows.len(),
        24,
        "independent source strict market fields"
    );
    for (index, row) in asset_rows.iter().enumerate() {
        assert_eq!(
            row["field"],
            ["id", "symbol", "base", "quote", "active"][index / 5]
        );
        assert_eq!(row["result"]["values"].as_array().unwrap().len(), 3);
    }
    let mut count = 0;
    for row in capture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["method"] == "fetchMarketsHelper")
    {
        let label = format!("APEX-BOUNDARY markets/{}", row["label"].as_str().unwrap());
        let host = Rc::new(Host::default());
        host.responses
            .borrow_mut()
            .insert("/v3/symbols", native::input(&row["payload"]));
        let store = fixture(&host);
        if row["label"] == "missing-config" {
            store
                .update_zk_link_config(Some(
                    json!({"id":"old","l2Key":"old-key","contractConfig":{"stale":true}}),
                ))
                .unwrap();
        }
        let exchange = venue(&host, store.clone());
        host.operations.borrow_mut().clear();
        let actual = futures::executor::block_on(exchange.fetch_markets_helper());
        if row["label"] == "nonboolean" || row["label"] == "numeric-id" {
            let nonboolean = row["label"] == "nonboolean";
            let markets = actual.expect("typed row is visibly omitted");
            assert!(
                markets.is_empty(),
                "all refused rows produce a fresh empty result"
            );
            let operations = host.operations.borrow();
            let warnings = operations
                .iter()
                .filter_map(|operation| match operation {
                    J::Array(values) if values.first() == Some(&s("log")) => Some(values),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(warnings.len(), 1, "one visible typed row refusal");
            assert_eq!(warnings[0][2], s("warning"));
            let J::String(message) = &warnings[0][1] else {
                panic!("warning text")
            };
            assert!(
                message.contains(if nonboolean {
                    "active=String(\"truthy\")"
                } else {
                    "id=Number(17.0)"
                }),
                "{message}"
            );
            drop(operations);
            let J::Array(source) = native::input(&row["result"]["value"]) else {
                panic!("source markets")
            };
            assert_eq!(source.len(), 1, "{label} source keeps one whole Market");
            let J::Object(fields) = &source[0] else {
                panic!("source Market")
            };
            let (key, value) = if nonboolean {
                ("active", s("truthy"))
            } else {
                ("id", n(17.0))
            };
            assert!(
                fields.contains(&(key.into(), value)),
                "{label} source raw field value and kind"
            );
        } else {
            match actual {
                Ok(markets) => {
                    assert_eq!(row["result"]["ok"], true, "{label} native expected error");
                    native::check_fields(
                        &J::Array(markets.iter().map(market).collect()),
                        &row["result"]["value"],
                        &format!("{label} native markets"),
                    );
                }
                Err(error) => {
                    assert_eq!(
                        row["result"]["ok"], false,
                        "{label} native unexpected error"
                    );
                    assert_eq!(
                        error.name,
                        row["result"]["name"].as_str().unwrap(),
                        "{label} native error kind"
                    );
                    assert_eq!(
                        error.message,
                        row["result"]["message"].as_str().unwrap(),
                        "{label} native error precedence"
                    );
                }
            }
        }
        // Native Store effects are inspected directly, including every rejected helper result.
        if row["label"] == "missing-config" {
            native::native_eq(
                &J::from(store.zk_link_config().as_deref().cloned().unwrap()),
                &J::from(json!({"id":"old","l2Key":"old-key"})),
                &format!("{label} native stale config removed before error"),
            );
            let J::Object(source_config) = native::input(&row["zk"]) else {
                panic!("source config")
            };
            assert!(
                source_config.contains(&("contractConfig".into(), J::Undefined)),
                "{label} source retains own undefined property"
            );
            assert_eq!(
                source_config.len(),
                3,
                "{label} source retained config fields"
            );
        } else {
            native::check_fields(
                &J::from(store.zk_link_config().as_deref().cloned().unwrap()),
                &row["zk"],
                &format!("{label} native config retained before filter error"),
            );
        }
        let J::Array(source_operations) = native::input(&row["operations"]) else {
            panic!("source operations")
        };
        let source_requests: Vec<_> = source_operations.into_iter().filter(|operation| {
            !matches!(operation, J::Array(fields) if fields.first() == Some(&s("store:zk")))
        }).collect();
        let typed_refusal = matches!(row["label"].as_str(), Some("nonboolean" | "numeric-id"));
        native::native_eq(
            &J::Array(host.operations.borrow().iter().filter(|operation| {
                !typed_refusal || !matches!(operation,J::Array(values) if values.first()==Some(&s("log")))
            }).cloned().collect()),
            &J::Array(source_requests),
            &format!("{label} native request sequence"),
        );
        count += 1;
    }
    assert_eq!(count, 17, "APEX-BOUNDARY independent market floor");
}

#[test]
fn websocket_coercion_failures_keep_source_type_error() {
    let capture = boundary_rows(false);
    let mut count = 0;
    for row in capture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["method"] == "recordWsExecutionError")
    {
        let label = format!(
            "APEX-BOUNDARY WS coercion/{}",
            row["label"].as_str().unwrap()
        );
        let host = Rc::new(Host::default());
        let exchange = venue(&host, fixture(&host));
        host.operations.borrow_mut().clear();
        let error = exchange
            .record_ws_execution(&native::input(&row["input"]))
            .expect_err("APEX-BOUNDARY native coercion must throw");
        assert_eq!(error.name, "TypeError", "{label} native error kind");
        assert_eq!(
            error.name,
            row["result"]["name"].as_str().unwrap(),
            "{label} native/source error kind"
        );
        assert_eq!(
            error.message, "Cannot convert object to primitive value",
            "{label} native error message"
        );
        assert_eq!(
            error.message,
            row["result"]["message"].as_str().unwrap(),
            "{label} native/source error message"
        );
        assert!(
            exchange.recent_ws_executions.borrow().is_empty(),
            "{label} native cache unchanged"
        );
        native::check_fields(
            &J::Array(host.operations.borrow().clone()),
            &row["operations"],
            &format!("{label} native operations"),
        );
        native::check(
            &J::Array(vec![]),
            &row["cache"],
            &format!("{label} source cache unchanged"),
        );
        count += 1;
    }
    assert_eq!(count, 7, "APEX-BOUNDARY independent WS coercion floor");
}

fn executions_assets(history: bool) {
    let capture = boundary_rows(history);
    let mut count = 0;
    let mut refusals = 0;
    for row in capture["rows"].as_array().unwrap().iter().filter(|row| {
        row["method"]
            == if history {
                "fetchHistoricalFills"
            } else {
                "recordWsExecution"
            }
    }) {
        let label = format!(
            "APEX-BOUNDARY {}/{} cache={}",
            row["method"].as_str().unwrap(),
            row["label"].as_str().unwrap(),
            row["hasCache"]
        );
        let unsupported = matches!(
            row["label"].as_str().unwrap(),
            "number" | "array" | "object" | "empty-array" | "boolean"
        );
        let host = Rc::new(Host::default());
        host.record_timeouts.set(history);
        let exchange = venue(&host, fixture(&host));
        if row["hasCache"] == true {
            exchange
                .record_ws_execution(&native::input(&row["cached"]))
                .unwrap();
        }
        let cache_before = exchange.recent_ws_executions.borrow().clone();
        host.operations.borrow_mut().clear();
        let result = if history {
            let inputs = if row.get("inputs").is_some() {
                native::input(&row["inputs"])
            } else {
                J::Array(vec![native::input(&row["input"])])
            };
            host.responses.borrow_mut().insert(
                "/v3/fills",
                obj(vec![("data", obj(vec![("fills", inputs)]))]),
            );
            let options = ExecutionsOptions {
                symbol: Some("BTCUSDC".into()),
                limit: Some(row["options"]["limit"].as_f64().unwrap_or(100.0)),
                order_id: None,
                from: row["options"]["from"].as_f64(),
                to: None,
            };
            futures::executor::block_on(exchange.fetch_historical_fills(&options, &J::Undefined))
        } else {
            exchange
                .record_ws_execution(&native::input(&row["input"]))
                .map(|()| exchange.recent_ws_executions.borrow().clone())
        };
        if unsupported {
            refusals += 1;
            if history {
                let actual = result.expect("APEX-BOUNDARY history catches typed refusal");
                assert_eq!(
                    actual.len(),
                    if row["hasCache"] == true { 1 } else { 0 },
                    "{label} native whole REST result refused"
                );
                native::native_eq(
                    &J::Array(actual.iter().map(execution).collect()),
                    &J::Array(cache_before.iter().map(execution).collect()),
                    &format!("{label} native cached fallback"),
                );
                let operations = host.operations.borrow();
                assert_eq!(
                    operations.len(),
                    2,
                    "{label} native request and refusal warning"
                );
                let message = if row["hasCache"] == true {
                    "Execution history fetch failed (shared typed record refuses a non-string field), serving 1 cached ws fills"
                } else {
                    "Execution history fetch failed: shared typed record refuses a non-string field"
                };
                native::native_eq(
                    &operations[1],
                    &J::Array(vec![s("log"), s(message), s("warning")]),
                    &format!("{label} native refusal visible warning"),
                );
                let J::Array(source_operations) = native::input(&row["operations"]) else {
                    panic!("source operations")
                };
                native::native_eq(
                    &J::Array(operations[..1].to_vec()),
                    &J::Array(source_operations),
                    &format!("{label} native request before refusal"),
                );
            } else {
                let error =
                    result.expect_err("APEX-BOUNDARY native commissionAsset refuses raw nonstring");
                assert_eq!(error.name, "Error", "{label} native refusal kind");
                assert_eq!(
                    error.message, "shared typed record refuses a non-string field",
                    "{label} native refusal cause"
                );
                assert!(
                    exchange.recent_ws_executions.borrow().is_empty(),
                    "{label} native refusal leaves cache empty"
                );
            }
            let J::Array(source) = native::input(&row["result"]) else {
                panic!("source executions")
            };
            assert_eq!(source.len(), 1, "{label} source retains whole execution");
            let J::Object(fields) = &source[0] else {
                panic!("source execution")
            };
            let J::Object(input_fields) = &native::input(&row["input"]) else {
                panic!("source input")
            };
            let asset = &input_fields
                .iter()
                .find(|(key, _)| key == "token")
                .unwrap()
                .1;
            assert!(
                fields.contains(&("commissionAsset".into(), asset.clone())),
                "{label} source raw asset value and kind"
            );
        } else {
            let actual = result
                .unwrap_or_else(|error| panic!("{label} native unexpected refusal: {error:?}"));
            native::check_fields(
                &J::Array(actual.iter().map(execution).collect()),
                &row["result"],
                &format!("{label} native executions"),
            );
            native::check_fields(
                &J::Array(host.operations.borrow().clone()),
                &row["operations"],
                &format!("{label} native operations"),
            );
        }
        if history {
            native::check_fields(
                &J::Array(
                    exchange
                        .recent_ws_executions
                        .borrow()
                        .iter()
                        .map(execution)
                        .collect(),
                ),
                &row["cache"],
                &format!("{label} native cache unchanged"),
            );
        }
        count += 1;
    }
    assert_eq!(
        count,
        if history { 21 } else { 8 },
        "APEX-BOUNDARY independent execution floor"
    );
    assert_eq!(
        refusals,
        if history { 10 } else { 5 },
        "APEX-BOUNDARY independently selected refusal floor"
    );
}

#[test]
fn websocket_assets_preserve_strings_and_refuse_raw_nonstrings() {
    executions_assets(false);
}

#[test]
fn historical_assets_refuse_rest_records_with_explicit_cached_fallback() {
    executions_assets(true);
}
