use super::*;
fn history_corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/historical_fills.cjs"
            ))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "history oracle {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn option_string(object: &J, key: &str) -> Option<String> {
    if let J::Object(fields) = object
        && let Some((_, J::String(v))) = fields.iter().find(|(k, _)| k == key)
    {
        Some(v.clone())
    } else {
        None
    }
}
fn option_number(object: &J, key: &str) -> Option<f64> {
    if let J::Object(fields) = object
        && let Some((_, J::Number(v))) = fields.iter().find(|(k, _)| k == key)
    {
        Some(*v)
    } else {
        None
    }
}
#[test]
fn historical_fills_native() {
    let rows = history_corpus()["cases"].as_array().unwrap();
    assert_eq!(rows.len(), 13, "history source floor");
    let mut count = 0;
    for row in rows {
        let variant = row["variant"].as_u64().unwrap();
        let host = Rc::new(Host::default());
        host.record_timeouts.set(true);
        let store = fixture(&host);
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(json!({"key":if variant == 11 { "" } else { "key" },"secret":"secret","passphrase":"pass","publicDataOnly":variant == 10}))
                    .unwrap(),
                store,
                host.clone(),
                Default::default(),
            ),
            host.clone(),
        )
        .unwrap();
        let fill = json!({"symbol":"BTC-USDC","side":"BUY","createdAt":1700000000000.0,"fillPrice":"83.25","fillSize":"2.25","fee":"-0.75","orderId":"rest","token":"USDC"});
        let fills = match variant {
            12 => Value::Null,
            2 => json!({"data":{"fills":[]}}),
            3 => {
                let mut second = fill.clone();
                second["createdAt"] = json!(1700000001000.0);
                second["orderId"] = json!("rest-2");
                json!({"result":{"list":[fill,second]}})
            }
            8 => json!({"data":[null]}),
            9 => {
                let mut first = fill.clone();
                first["side"] = json!("\u{feff}BUY\u{feff}");
                json!({"data":{"rows":[first,fill]}})
            }
            _ => json!({"data":{"fills":[fill]}}),
        };
        host.responses
            .borrow_mut()
            .insert("/v3/fills", J::from(fills));
        host.responses
            .borrow_mut()
            .insert("/v3/history-orders", J::from(json!({"data":{"orders":[]}})));
        if variant == 1 || variant == 7 {
            host.request_errors.borrow_mut().insert(
                "/v3/fills",
                ExchangeError::new(if variant == 1 {
                    "fills unavailable"
                } else {
                    "fills down"
                }),
            );
        }
        if variant == 1 {
            let mut fallback = fill;
            fallback["side"] = json!("\u{feff}SHORT\u{feff}");
            fallback["cumSuccessFillSize"] = json!("4.25");
            fallback["fillSize"] = json!("1.25");
            fallback["avgPrice"] = json!("91.25");
            fallback["fillPrice"] = json!("");
            fallback["cumSuccessFillValue"] = json!("425");
            fallback["fillFee"] = json!("-0.125");
            host.responses.borrow_mut().insert(
                "/v3/history-orders",
                J::from(json!({"body":{"result":{"rows":[fallback]}}})),
            );
        }
        if variant == 7 {
            host.request_errors
                .borrow_mut()
                .insert("/v3/history-orders", ExchangeError::new("history down"));
        }
        venue
            .record_ws_execution(&native::capture(&native::input(&row["cached"])))
            .unwrap();
        host.operations.borrow_mut().clear();
        let opts = native::input(&row["options"]);
        let opts = ExecutionsOptions {
            symbol: option_string(&opts, "symbol"),
            order_id: option_string(&opts, "orderId"),
            from: option_number(&opts, "from"),
            to: option_number(&opts, "to"),
            limit: option_number(&opts, "limit"),
        };
        let result = futures::executor::block_on(
            venue.fetch_historical_fills(&opts, &native::input(&row["configured"])),
        )
        .unwrap();
        native::check_fields(
            &J::Array(result.iter().map(execution).collect()),
            &row["result"],
            "fetchHistoricalFills native executions",
        );
        native::check_fields(
            &J::Array(host.operations.borrow().clone()),
            &row["operations"],
            "fetchHistoricalFills native operations",
        );
        if variant == 0 {
            assert_eq!(result.len(), 1);
            assert_eq!(
                result[0].order_id.as_deref(),
                Some("rest"),
                "REST rows exclude WS cache"
            );
        }
        if variant == 10 || variant == 11 {
            assert!(result.is_empty(), "public-only history is empty");
            assert!(
                host.operations.borrow().is_empty(),
                "public-only history makes no requests"
            );
        }
        count += 1;
    }
    assert_eq!(count, 13, "historical comparison floor");
}
