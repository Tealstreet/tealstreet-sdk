use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue, js_number_string};
use safe_cex_types::{Execution, ExecutionsOptions};
use safe_cex_venue_bitget::history_reads::{fetch_historical_fills, fetch_historical_uta_fills};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
fn number(value: f64) -> Value {
    json!({"$number":if value == 0.0 && value.is_sign_negative(){"-0".into()}else{js_number_string(value)},"bits":format!("{:016x}",if value.is_nan(){0x7ff8000000000000}else{value.to_bits()})})
}
fn tag(value: JsJsonValue) -> Value {
    match value {
        JsJsonValue::Undefined => json!({"$undefined":true}),
        JsJsonValue::Number(v) => number(v),
        JsJsonValue::Array(v) => Value::Array(v.into_iter().map(tag).collect()),
        JsJsonValue::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, tag(v))).collect()),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => Value::Bool(v),
        JsJsonValue::String(v) => Value::String(v),
        JsJsonValue::BigInt(_) => panic!("unexpected BigInt fixture"),
    }
}
fn decode(value: &Value) -> Option<Value> {
    if value.get("$undefined").is_some() {
        return None;
    }
    if let Some(number) = option_number(value) {
        return Some(serde_json::to_value(number).expect("finite fixture JSON number"));
    }
    Some(match value {
        Value::Object(v) => Value::Object(
            v.iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        Value::Array(v) => {
            Value::Array(v.iter().map(|v| decode(v).unwrap_or(Value::Null)).collect())
        }
        v => v.clone(),
    })
}
fn option_number(value: &Value) -> Option<f64> {
    value
        .get("bits")
        .and_then(Value::as_str)
        .map(|bits| f64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
}
fn execution(value: Execution) -> Value {
    let absent = json!({"$undefined":true});
    let optional = |v: Option<f64>| v.map(number).unwrap_or_else(|| absent.clone());
    json!({"timestamp":number(value.timestamp),"symbol":value.symbol,"id":value.id.map(Value::String).unwrap_or_else(||absent.clone()),"orderId":value.order_id.map(Value::String).unwrap_or_else(||absent.clone()),"price":number(value.price),"qty":number(value.qty),"side":value.side,"commission":optional(value.commission),"commissionAsset":value.commission_asset.map(Value::String).unwrap_or_else(||absent.clone()),"realizedPnl":optional(value.realized_pnl),"positionSide":value.position_side,"maker":value.maker.map(Value::Bool).unwrap_or_else(||absent.clone())})
}
fn check(case: &Value) {
    let c = &case["config"];
    let calls = RefCell::new(Vec::new());
    let opts = ExecutionsOptions {
        symbol: c["opts"]["symbol"].as_str().map(str::to_owned),
        order_id: c["opts"]["orderId"].as_str().map(str::to_owned),
        from: option_number(&c["opts"]["from"]),
        to: option_number(&c["opts"]["to"]),
        limit: option_number(&c["opts"]["limit"]),
    };
    let default_rows = json!([{"symbol":"BTC","cTime":"1700000000123","baseVolume":"2.5","price":"11.25","side":"sell","tradeSide":"close","profit":"3.75","fee":"-0.125","tradeId":"a"}]);
    let default_pages = json!([{"rows":default_rows}]);
    let pages = c.get("pages").unwrap_or(&default_pages).as_array().unwrap();
    let index = std::cell::Cell::new(0);
    let mut products = 0;
    let resolve = |symbol: &str| {
        calls.borrow_mut().push(json!(["resolve", symbol]));
        match symbol {
            "THROW" => Err(ExchangeError::new("resolve failure")),
            "MISSING" => Ok(None),
            "EMPTY" => Ok(Some((String::new(), String::new()))),
            "BTC" => Ok(Some(("BTCUSDT".into(), "BTC/USDT".into()))),
            _ => Ok(Some((symbol.into(), symbol.into()))),
        }
    };
    let next_page = |label: &str, params: JsJsonValue| {
        calls.borrow_mut().push(json!([label, tag(params)]));
        let i = index.get();
        let page = if c["cycle"] == true {
            pages.get(i % pages.len())
        } else {
            pages.get(i)
        };
        index.set(i + 1);
        std::future::ready(if let Some(error) = page.and_then(|p| p.get("error")) {
            let mut e = ExchangeError::new("fixture page failure");
            e.original_error = decode(error);
            Err(e)
        } else {
            Ok((
                page.and_then(|p| p["rows"].as_array())
                    .map(|rows| {
                        rows.iter()
                            .map(|r| decode(r).unwrap_or(Value::Null))
                            .collect()
                    })
                    .unwrap_or_default(),
                page.and_then(|p| p["endId"].as_str()).map(str::to_owned),
            ))
        })
    };
    let error_value = |error: &ExchangeError| {
        error
            .original_error
            .clone()
            .or_else(|| Some(json!({"message":error.message})))
    };
    let report = |message: Option<Value>| {
        calls.borrow_mut().push(json!([
            "emit",
            message
                .map(|v| tag(JsJsonValue::from(v)))
                .unwrap_or_else(|| json!({"$undefined":true}))
        ]));
        if c["reportError"] == true {
            Err(ExchangeError::new("report failure"))
        } else {
            Ok(())
        }
    };
    let result = run(fetch_historical_fills(
        c["public"] == true,
        &opts,
        || {
            calls.borrow_mut().push(json!(["detect"]));
            std::future::ready(if c["detectError"] == true {
                Err(ExchangeError::new("detect failure"))
            } else {
                calls.borrow_mut().push(json!(["uta"]));
                Ok(c["uta"] == true)
            })
        },
        |opts| {
            fetch_historical_uta_fills(
                opts,
                resolve,
                || {
                    calls.borrow_mut().push(json!(["category"]));
                    if c["categoryError"] == true {
                        Err(ExchangeError::new("category failure"))
                    } else {
                        Ok("USDT-FUTURES".into())
                    }
                },
                |params| next_page("utaRead", params),
                (error_value, report),
            )
        },
        resolve,
        || {
            calls
                .borrow_mut()
                .push(json!(["product", number(f64::from(products))]));
            products += 1;
            if c["productError"] == true {
                Err(ExchangeError::new("product failure"))
            } else {
                Ok(if products % 2 == 1 {
                    "USDT-FUTURES"
                } else {
                    "USDC-FUTURES"
                }
                .into())
            }
        },
        |params| next_page("read", params),
        (error_value, report),
    ));
    let actual = match result {
        Ok(rows) => json!({"value":rows.into_iter().map(execution).collect::<Vec<_>>()}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    };
    assert_eq!(
        actual, case["output"],
        "historical fills output {}",
        case["name"]
    );
    assert_eq!(
        *calls.borrow(),
        case["calls"].as_array().unwrap().clone(),
        "historical fills effects {}",
        case["name"]
    );
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_historical_fills() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/historical-fills-capture.cjs"
        ))
        .output()
        .expect("historical fills oracle requires Node");
    assert!(
        output.status.success(),
        "historical fills oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["cases"].as_array().unwrap().len(),
        155,
        "historical fills case inventory"
    );
    assert_eq!(
        oracle["aliases"][0]["symbol"], "ETH",
        "source options alias after detection"
    );
    assert_eq!(
        oracle["aliases"][0]["limit"],
        number(7.0),
        "source limit alias after detection"
    );
    for case in oracle["utf16"].as_array().unwrap() {
        assert_eq!(
            case["inputJson"], case["outputJson"],
            "source retains surrogate output"
        );
        assert!(
            serde_json::from_str::<String>(case["inputJson"].as_str().unwrap()).is_err(),
            "Rust shared string boundary refuses lone surrogate"
        );
    }
    for case in oracle["cases"].as_array().unwrap() {
        check(case);
    }
}

#[test]
fn historical_fills_drops_detection_and_page() {
    use std::cell::Cell;
    use std::rc::Rc;
    struct DropProbe(Rc<Cell<bool>>);
    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    for detect_pending in [true, false] {
        let dropped = Rc::new(Cell::new(false));
        let guard = DropProbe(dropped.clone());
        let mut guard = Some(guard);
        let opts = ExecutionsOptions {
            symbol: None,
            order_id: None,
            from: None,
            to: None,
            limit: None,
        };
        let detection_guard = if detect_pending { guard.take() } else { None };
        let mut future = Box::pin(fetch_historical_fills(
            false,
            &opts,
            move || async move {
                if let Some(_guard) = detection_guard {
                    std::future::pending::<()>().await;
                }
                Ok(false)
            },
            |_| -> std::future::Ready<Result<Vec<safe_cex_types::Execution>, ExchangeError>> {
                unreachable!("classic routing")
            },
            |_| unreachable!("no rows should resolve"),
            || Ok("USDT-FUTURES".into()),
            |_| {
                let guard = guard.take();
                async move {
                    let _guard = guard;
                    std::future::pending().await
                }
            },
            (
                |_| unreachable!("pending operation cannot fail"),
                |_| unreachable!("pending operation cannot report"),
            ),
        ));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(!dropped.get(), "operation must remain owned while pending");
        drop(future);
        assert!(
            dropped.get(),
            "history caller drops pending operation: detect={detect_pending}"
        );
    }
}

#[test]
fn historical_fills_preserves_escaped_error_metadata() {
    for stage in ["detect", "resolve", "report"] {
        let mut error = ExchangeError::new(format!("original {stage} failure"));
        error.name = "OriginalFailure".into();
        error.context = Some(json!({"stage":stage,"identity":[7,19,42]}));
        error.response = Some(json!({"data":{"msg":"venue detail"}}));
        error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "ORIGINAL"
        )));
        error.original_error = Some(json!({"retained":"source object"}));
        let expected = error.clone();
        let allocation = std::ptr::from_ref(error.metadata.as_ref());
        let original = RefCell::new(Some(error));
        let opts = ExecutionsOptions {
            symbol: Some("BTC".into()),
            order_id: None,
            from: None,
            to: None,
            limit: None,
        };
        let result = run(fetch_historical_fills(
            false,
            &opts,
            || {
                std::future::ready(if stage == "detect" {
                    Err(original.borrow_mut().take().unwrap())
                } else {
                    Ok(false)
                })
            },
            |_| -> std::future::Ready<Result<Vec<safe_cex_types::Execution>, ExchangeError>> {
                unreachable!("classic routing")
            },
            |_| {
                if stage == "resolve" {
                    Err(original.borrow_mut().take().unwrap())
                } else {
                    Ok(Some(("BTC".into(), "BTC".into())))
                }
            },
            || Ok("USDT-FUTURES".into()),
            |_| {
                let mut failure = ExchangeError::new("read failure");
                failure.context = Some(json!({"read":"retained"}));
                std::future::ready(Err(failure))
            },
            (
                |error| {
                    assert_eq!(stage, "report", "preflight error must not enter catch");
                    assert_eq!(
                        error.context,
                        Some(json!({"read":"retained"})),
                        "caught read metadata reaches projection"
                    );
                    Some(json!({"message":error.message}))
                },
                |message| {
                    assert_eq!(stage, "report", "preflight error must not report");
                    assert_eq!(message, Some(json!("read failure")));
                    Err(original.borrow_mut().take().unwrap())
                },
            ),
        ));
        let actual = result.expect_err("original error must escape");
        assert_eq!(actual, expected, "historical fills {stage} error metadata");
        assert_eq!(
            std::ptr::from_ref(actual.metadata.as_ref()),
            allocation,
            "historical fills {stage} original metadata allocation"
        );
    }
}
