#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, Ticker};
use safe_cex_venue_blofin::tickers::*;
use serde_json::{Value, json};
use std::{process::Command, rc::Rc};
fn bits(value: f64) -> Value {
    json!({"number":if value.is_nan(){"nan".into()}else{format!("{:016x}",value.to_bits())}})
}
fn info(value: safe_cex_types::retained_json::RetainedJsonInput) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Float(v) => bits(v),
        Input::Lexeme(v) => bits(v.parse().unwrap()),
        Input::Array(v) => Value::Array(v.into_iter().map(info).collect()),
        Input::Object(v) => Value::Object(
            v.into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), info(v)))
                .collect(),
        ),
        Input::Text(v) => Value::String(String::from_utf16(&v).unwrap()),
        Input::Bool(v) => Value::Bool(v),
        Input::Null => Value::Null,
        Input::Undefined => panic!("source corpus has no native undefined in JSON info"),
    }
}

fn native(ticker: Ticker) -> Value {
    assert!(ticker.mid_price.is_none(), "ticker_mid_price_absent");
    let mut value = json!({"id":ticker.id,"symbol":ticker.symbol,"bid":bits(ticker.bid),"ask":bits(ticker.ask),
        "last":bits(ticker.last),"mark":bits(ticker.mark),"index":bits(ticker.index),"percentage":bits(ticker.percentage),
        "fundingRate":bits(ticker.funding_rate),"fundingTime":bits(ticker.funding_time),"volume":bits(ticker.volume),
        "quoteVolume":bits(ticker.quote_volume),"openInterest":bits(ticker.open_interest)});
    if let Some(raw) = ticker.info {
        value["info"] = info(raw);
    }
    value
}
#[test]
fn live_tickers_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tickers_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_tickers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("tickers", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("tickers capture header: {header}");
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let raw = case["raw"].as_str().unwrap();
            let input: TickerInput = match path {
                0 => js_from_str(raw),
                1 => js_from_value(serde_json::from_str(raw).unwrap()),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()),
            }
            .expect("ticker_ingress_retained");
            let markets: Vec<Market> = js_from_value(corpus["markets"].clone()).unwrap();
            let fallback: Ticker = js_from_value(corpus["fallbackRaw"].clone()).unwrap();
            let fallback_rows: Vec<Ticker> = case
                .get("fallbackRows")
                .map(|v| js_from_value(v.clone()).unwrap())
                .unwrap_or_else(|| vec![fallback]);
            let store = DefaultStore::new();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(markets),
                    tickers: Some(fallback_rows),
                    ..Default::default()
                },
                false,
            )
            .expect("ticker fixture store update");
            let mut logs = Vec::new();
            let sentinel = Rc::new("host:unique".to_owned());
            let result = format_tickers(&input, &store, |message| {
                logs.push(message.to_owned());
                if case["hostMutates"] == true {
                    let mut updated: Ticker = js_from_value(corpus["fallbackRaw"].clone()).unwrap();
                    updated.id = "after-log".into();
                    updated.symbol = "after-log-symbol".into();
                    updated.last = 37.125;
                    Store::update(
                        &store,
                        StoreUpdate {
                            tickers: Some(vec![updated]),
                            ..Default::default()
                        },
                        false,
                    )
                    .expect("reentrant fixture update");
                }
                if case["hostThrows"] == true {
                    Err(sentinel.clone())
                } else {
                    Ok(())
                }
            });
            if let Ok(rows) = &result {
                pin.native(i, "/result/ok", rows.len());
            }
            if case["result"].get("error").is_some() {
                assert!(
                    result
                        .as_ref()
                        .err()
                        .is_some_and(|e| matches!(e, safe_cex_venue_blofin::reads::ReadError::Emitter(e) if Rc::ptr_eq(e, &sentinel))),
                    "ticker_host_error_identity"
                );
            }
            if case["fallbackAlias"] == true {
                assert!(
                    result
                        .as_ref()
                        .is_ok_and(|rows| rows.as_ptr() != store.tickers().unwrap().as_ptr()),
                    "ticker_owned_fallback_boundary"
                );
            }
            let actual = match result {
                Ok(v) => json!({"ok":v.into_iter().map(native).collect::<Vec<_>>()}),
                Err(safe_cex_venue_blofin::reads::ReadError::Emitter(e)) => {
                    json!({"error":"Error","message":e.as_str(),"identity":Rc::ptr_eq(&e,&sentinel)})
                }
                Err(e) => panic!("unexpected non-emitter fixture failure: {e:?}"),
            };
            assert_eq!(
                json!(
                    store
                        .tickers()
                        .unwrap()
                        .iter()
                        .cloned()
                        .map(native)
                        .collect::<Vec<_>>()
                ),
                case["finalTickers"],
                "ticker_post_emit_store"
            );
            if case["inherited"] == true {
                assert_eq!(
                    case["result"]["ok"][0]["id"],
                    json!({"undefined":true}),
                    "source_ticker_inherited_id"
                );
                assert_eq!(
                    case["result"]["ok"][0]["symbol"],
                    json!({"undefined":true}),
                    "source_ticker_inherited_symbol"
                );
                assert_eq!(
                    actual,
                    json!({"ok":[corpus["fallback"].clone()]}),
                    "ticker_inherited_typed_fallback"
                );
                assert_eq!(
                    logs,
                    vec!["Inherited market cannot produce a typed ticker"],
                    "ticker_inherited_error_log"
                );
            } else {
                assert_eq!(
                    actual, case["result"],
                    "ticker_native_fields case={i} path={path} raw={raw}"
                );
                assert_eq!(json!(logs), case["logs"], "ticker_error_trace case={i}");
            }
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    for index in 0..pin.count("/boundaries") {
        let raw = &corpus["boundaries"][index];
        let result = js_from_str::<TickerInput>(raw.as_str().unwrap());
        if index == 0 {
            let input = result.expect("ticker opaque native UTF-16 ingress");
            assert_eq!(
                input.retained().as_array().unwrap()[0]
                    .get("instId")
                    .unwrap()
                    .as_text(),
                Some([0xd800].as_slice()),
                "ticker_retained_input_units"
            );
            // A retained row reaches the source mapper; typed Market IDs remain
            // strict and produce the existing fallback when lookup is impossible.
            let store = DefaultStore::new();
            let mut logs = Vec::new();
            assert!(
                format_tickers(&input, &store, |message| {
                    logs.push(message.to_owned());
                    Ok::<_, ()>(())
                })
                .unwrap()
                .is_empty()
            );
            assert_eq!(logs.len(), 1, "ticker named-ID refusal remains visible");
        } else {
            assert!(result.is_err(), "ticker_numeric_ingress_depth_boundary");
        }
        pin.hit("boundaries-raw-1");
    }
    pin.finish("boundaries-raw-1", pin.count("/boundaries"));
}
