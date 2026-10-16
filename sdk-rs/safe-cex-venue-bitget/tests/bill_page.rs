use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, JsJsonValue, js_number_string};
use safe_cex_venue_bitget::history_reads::fetch_bill_page;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    error::Error,
    future::Future,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
fn number(n: f64) -> Value {
    json!({"$number":if n==0.0&&n.is_sign_negative(){"-0".into()}else{js_number_string(n)},"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn encode(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"$undefined":true}),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::String(v) => json!(v),
        JsJsonValue::Number(v) => number(*v),
        JsJsonValue::Array(v) => Value::Array(v.iter().map(encode).collect()),
        JsJsonValue::Object(v) => {
            Value::Object(v.iter().map(|(k, v)| (k.clone(), encode(v))).collect())
        }
        JsJsonValue::BigInt(_) => panic!("fixture unexpected bigint"),
    }
}
fn decode(v: &Value) -> Option<Value> {
    if v.get("$undefined").is_some() {
        return None;
    }
    if let Some(n) = v.get("$number").and_then(Value::as_str) {
        return Some(
            serde_json::from_str(match n {
                "Infinity" => "1e400",
                "-Infinity" => "-1e400",
                n => n,
            })
            .unwrap(),
        );
    }
    Some(match v {
        Value::Array(a) => {
            Value::Array(a.iter().map(|v| decode(v).unwrap_or(Value::Null)).collect())
        }
        Value::Object(o) => Value::Object(
            o.iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        v => v.clone(),
    })
}
fn check(oracle: &Value) {
    for c in oracle["cases"].as_array().unwrap() {
        let opts = decode(&c["opts"]);
        let data = decode(&c["data"]);
        let id = decode(&c["marketId"]);
        let calls = RefCell::new(Vec::new());
        let failure = RefCell::new(Some(Box::new(ExchangeError {
            name: "AxiosError".into(),
            message: "original read failure".into(),
            metadata: Box::default(),
        }) as Box<dyn Error>));
        let identity = std::ptr::from_ref(failure.borrow().as_ref().unwrap().as_ref()) as *const ();
        let result = run(fetch_bill_page::<Box<dyn Error>, _>(
            c["publicOnly"].as_bool().unwrap(),
            c["product"].as_str().unwrap(),
            opts.as_ref(),
            |symbol| {
                calls
                    .borrow_mut()
                    .push(json!({"resolve":encode(&JsJsonValue::from(symbol.clone()))}));
                if c["fail"] == "resolve" {
                    Err(failure.borrow_mut().take().unwrap())
                } else {
                    Ok(id)
                }
            },
            |url, params| {
                calls
                    .borrow_mut()
                    .push(json!({"url":url,"params":encode(&params)}));
                std::future::ready(if c["fail"] == "read" {
                    Err(failure.borrow_mut().take().unwrap())
                } else {
                    Ok(data)
                })
            },
        ));
        let actual_calls = Value::Array(calls.into_inner());
        let expected_calls = c["calls"].clone();
        assert_eq!(
            actual_calls, expected_calls,
            "_fetchBillPage request/call ordering {}",
            c["name"]
        );
        if let Some(e) = c["output"].get("error") {
            let error = result.expect_err(&format!("_fetchBillPage error required {}", c["name"]));
            let (name, message) = if let Some(e) = error.downcast_ref::<JsCoercionError>() {
                (e.name, e.message)
            } else {
                let e = error.downcast_ref::<ExchangeError>().unwrap();
                (e.name.as_str(), e.message.as_str())
            };
            assert_eq!(name, e["name"], "_fetchBillPage error name {}", c["name"]);
            assert_eq!(
                message, e["message"],
                "_fetchBillPage error message {}",
                c["name"]
            );
            if e["sameFailure"] == true {
                assert_eq!(
                    std::ptr::from_ref(error.as_ref()) as *const (),
                    identity,
                    "_fetchBillPage original failure identity {}",
                    c["name"]
                );
            }
        } else {
            let rows =
                result.unwrap_or_else(|e| panic!("_fetchBillPage success {}: {e}", c["name"]));
            let actual=Value::Array(rows.iter().map(|r|{
 let mut v=json!({"symbol":r.symbol,"incomeType":r.income_type,"asset":r.asset,"timestamp":number(r.timestamp),"tranId":r.tran_id,"income":number(r.income)});
 if let Some(category)=&r.category{v["category"]=serde_json::to_value(category).unwrap();}v
 }).collect());
            let mut expected = c["output"]["value"].clone();
            if c["name"]
                .as_str()
                .unwrap()
                .starts_with("inherited-category:")
            {
                for row in expected.as_array_mut().unwrap() {
                    let category = row.as_object_mut().unwrap().remove("category").unwrap();
                    if c["name"] == "inherited-category:__proto__" {
                        assert_eq!(
                            category,
                            json!({}),
                            "prototype object survives source output"
                        );
                    } else {
                        assert!(
                            category.get("$function").is_some(),
                            "inherited callable observed before serialization"
                        );
                    }
                }
                assert!(
                    rows.iter().all(|r| r.category.is_none()),
                    "CHOSEN inherited category refused while record retained"
                );
            }
            assert_eq!(
                actual, expected,
                "_fetchBillPage mapped records {}",
                c["name"]
            );
        }
    }
    assert_eq!(
        oracle["alias"]["afterInput"],
        json!({"tag":"after-input"}),
        "source request observes nested caller mutation"
    );
    assert_eq!(
        oracle["alias"]["afterOutput"],
        json!({"tag":"after-output"}),
        "source caller observes nested request mutation"
    );
    let mut opts = json!({"startTime":{"tag":"before"}});
    let mut captured = None;
    run(fetch_bill_page::<Box<dyn Error>, _>(
        false,
        "usdt-futures",
        Some(&opts),
        |_| unreachable!(),
        |_, params| {
            captured = Some(params);
            std::future::ready(Ok(None))
        },
    ))
    .unwrap();
    opts["startTime"]["tag"] = json!("after-input");
    let mut params = captured.unwrap();
    assert_eq!(
        encode(&params)["startTime"],
        json!({"tag":"before"}),
        "CHOSEN request owns nested input snapshot"
    );
    if let JsJsonValue::Object(fields) = &mut params {
        fields.iter_mut().find(|(k, _)| k == "startTime").unwrap().1 =
            JsJsonValue::from(json!({"tag":"after-output"}));
    }
    assert_eq!(
        opts["startTime"],
        json!({"tag":"after-input"}),
        "CHOSEN request mutation does not change caller snapshot"
    );
    for witness in oracle["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(witness["json"].as_str().unwrap()).is_err(),
            "bill page UTF8 representation refusal"
        );
        assert!(
            witness["output"].as_str().unwrap().contains("\\ud"),
            "source bill id preserves surrogate"
        );
    }
}
#[test]
fn frozen_bill_page() {
    check(&serde_json::from_str(include_str!("bill-page-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_bill_page() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/bill-page-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    check(&oracle);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("bill-page-oracle.json")).unwrap(),
        "bill page source drift"
    );
}

#[test]
fn dropped_bill_page_drops_its_read() {
    use std::{cell::Cell, rc::Rc};
    struct Guard(Rc<Cell<bool>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let flag = dropped.clone();
    let mut future = Box::pin(fetch_bill_page::<Box<dyn Error>, _>(
        false,
        "usdt-futures",
        None,
        |_| panic!("no symbol resolver"),
        move |_, _| async move {
            let _guard = Guard(flag);
            std::future::pending::<Result<Option<Value>, Box<dyn Error>>>().await
        },
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(!dropped.get());
    drop(future);
    assert!(
        dropped.get(),
        "bill page owns and drops pending read, without a shared latch"
    );
}

#[test]
fn bill_page_numeric_capture_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/bill-page-capture.cjs"
        ))
        .arg("--numeric-probe")
        .output()
        .expect("bill page numeric capture requires Node on PATH");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captured: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, value, bits) in [
        (
            "negativeNaN",
            f64::from_bits(0xfff8_0000_0000_0000),
            "7ff8000000000000",
        ),
        ("positiveNaN", f64::NAN, "7ff8000000000000"),
        ("negativeZero", -0.0, "8000000000000000"),
        ("positiveZero", 0.0, "0000000000000000"),
        ("finite", 1.25, "3ff4000000000000"),
        ("negativeFinite", -2.5, "c004000000000000"),
        ("positiveInfinity", f64::INFINITY, "7ff0000000000000"),
        ("negativeInfinity", f64::NEG_INFINITY, "fff0000000000000"),
    ] {
        assert_eq!(
            captured[name]["bits"], bits,
            "bill page source numeric identity {name}"
        );
        assert_eq!(
            number(value)["bits"],
            bits,
            "bill page Rust numeric identity {name}"
        );
        assert_eq!(
            captured[name],
            number(value),
            "bill page numeric tag identity {name}"
        );
    }
}
