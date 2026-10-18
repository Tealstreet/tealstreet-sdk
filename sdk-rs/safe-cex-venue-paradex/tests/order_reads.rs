use safe_cex_venue_paradex::order_reads::{OrderReadEffects, fetch_orders};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
struct Failure {
    tag: &'static str,
    message: String,
}
type Error = Rc<Failure>;
struct Held<T> {
    ready: Rc<Cell<bool>>,
    outcome: Option<Result<T, Error>>,
}
impl<T: Unpin> Future for Held<T> {
    type Output = Result<T, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.outcome.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
fn definitions() -> Vec<Value> {
    let statuses = json!([
        "NEW",
        "OPEN",
        "PENDING",
        "UNTRIGGERED",
        "CLOSED",
        "CANCELED",
        "FILLED",
        "new",
        "constructor",
        "",
        null,
        13
    ]);
    let base:Vec<Value>=statuses.as_array().unwrap().iter().enumerate().map(|(i,s)|json!({"id":format!("row-{i}"),"status":s,"price":(101.25+i as f64*7.0).to_string(),"size":(3.5+i as f64*2.0).to_string()})).collect();
    let data = json!({"results":base});
    let mut defs = Vec::new();
    let mut add = |body: Option<Value>, extra: Value| {
        let mut v = json!({"body":body,"undefined":body.is_none(),"jwt":true,"init":false,"getError":"","initError":false,"throwLog":-1,"mapError":-1,"stringifyError":false,"describeError":false});
        for (k, value) in extra.as_object().unwrap() {
            v[k] = value.clone();
        }
        defs.push(v);
    };
    add(Some(data.clone()), json!({}));
    add(
        Some(json!({"results":base.iter().rev().collect::<Vec<_>>()})),
        json!({}),
    );
    add(
        Some(json!({"results":[base[1],base[1],base[0]]})),
        json!({}),
    );
    for raw in [
        None,
        Some("null"),
        Some("{}"),
        Some("false"),
        Some("0"),
        Some("\"\""),
        Some("{\"results\":null}"),
        Some("{\"results\":false}"),
        Some("{\"results\":0}"),
        Some("{\"results\":\"\"}"),
        Some("{\"results\":[]}"),
        Some("{\"results\":true}"),
        Some("{\"results\":\"abc\"}"),
        Some("{\"results\":{}}"),
        Some("{\"results\":[null]}"),
        Some("{\"results\":[{},7,false,\"x\"]}"),
    ] {
        add(raw.map(|s| serde_json::from_str(s).unwrap()), json!({}));
    }
    for init in [false, true] {
        for jwt in [false, true] {
            add(Some(data.clone()), json!({"init":init,"jwt":jwt}));
        }
    }
    for extra in [
        json!({"getError":"sync"}),
        json!({"getError":"async"}),
        json!({"init":true,"initError":true}),
        json!({"throwLog":0}),
        json!({"throwLog":1}),
        json!({"throwLog":2}),
        json!({"mapError":0}),
        json!({"mapError":2}),
        json!({"getError":"async","throwLog":1}),
        json!({"getError":"async","throwLog":2}),
        json!({"getError":"async","stringifyError":true}),
        json!({"getError":"async","describeError":true}),
        json!({"jwt":false,"throwLog":0}),
    ] {
        add(Some(data.clone()), extra);
    }
    defs
}
#[test]
fn actual_typescript_order_reads() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "orders_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    let expected = definitions();
    assert_eq!(cases.len(), 36, "orders_source_cases");
    assert_eq!(expected.len(), 36, "orders_independent_cases");
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        s => s,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let mut completed = 0;
    let mut native_outputs = 0;
    for (index, c) in cases.iter().enumerate() {
        let d = &expected[index];
        let undefined = c.get("data").unwrap().is_null();
        let body = c["data"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        assert_eq!(
            json!(undefined),
            d["undefined"],
            "orders_live_input_content {index}"
        );
        assert_eq!(json!(body), d["body"], "orders_live_input_content {index}");
        for key in [
            "jwt",
            "init",
            "getError",
            "initError",
            "throwLog",
            "mapError",
            "stringifyError",
            "describeError",
        ] {
            assert_eq!(
                c.get(key),
                d.get(key),
                "orders_live_input_content {index} {key}"
            );
        }
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let initialized = Rc::new(Cell::new(false));
        let settled = Rc::new(Cell::new(false));
        let original = Rc::new(Failure {
            tag: "original",
            message: String::new(),
        });
        let logger = Rc::new(Failure {
            tag: "logger",
            message: String::new(),
        });
        let replacement = Rc::new(Failure {
            tag: "replacement",
            message: String::new(),
        });
        let mut logs = 0;
        let mut mapped = 0;
        let mut effects = OrderReadEffects {
            log: |text: String, severity: &'static str| {
                trace.borrow_mut().push(json!(["log", text, severity]));
                let fail = logs == c["throwLog"].as_i64().unwrap();
                logs += 1;
                if fail { Err(logger.clone()) } else { Ok(()) }
            },
            stringify_error: |e: &Error| {
                if e.tag == "type" {
                    return Ok(format!("TypeError: {}", e.message));
                }
                trace.borrow_mut().push(json!(["stringify", e.tag]));
                if e.tag == "original" && c["stringifyError"] == true {
                    Err(replacement.clone())
                } else {
                    Ok(e.tag.to_string())
                }
            },
            describe_error: |e: &Error| {
                if e.tag == "original" && c["describeError"] == true {
                    trace.borrow_mut().push(json!(["stringify", "original"]));
                    Ok(Some("original".into()))
                } else if e.tag == "type" {
                    Ok(Some("{}".into()))
                } else {
                    Ok(Some(format!("{{\"tag\":\"{}\"}}", e.tag)))
                }
            },
            coercion_error: |e: safe_cex_num::JsCoercionError| {
                Rc::new(Failure {
                    tag: "type",
                    message: e.message.into(),
                })
            },
        };
        let init = (c["init"] == true).then(|| Held {
            ready: initialized.clone(),
            outcome: Some(if c["initError"] == true {
                Err(original.clone())
            } else {
                Ok(())
            }),
        });
        let mut pending = Box::pin(fetch_orders(
            init,
            || {
                trace.borrow_mut().push(json!(["jwt"]));
                c["jwt"] == true
            },
            |endpoint| {
                trace.borrow_mut().push(json!(["get", endpoint]));
                if c["getError"] == "sync" {
                    Err(original.clone())
                } else {
                    Ok(Held {
                        ready: settled.clone(),
                        outcome: Some(if c["getError"] == "async" {
                            Err(original.clone())
                        } else {
                            Ok(body.clone().map(Into::into))
                        }),
                    })
                }
            },
            |row| {
                let row = row
                    .clone()
                    .into_value()
                    .expect("finite source fixture trace");
                trace.borrow_mut().push(json!(["map", row["id"]]));
                let fail = mapped == c["mapError"].as_i64().unwrap();
                mapped += 1;
                if fail {
                    Err(original.clone())
                } else {
                    Ok((
                        row["id"].as_str().unwrap().to_owned(),
                        row["price"].as_str().unwrap().to_owned(),
                        row["size"].as_str().unwrap().to_owned(),
                    ))
                }
            },
            &mut effects,
        ));
        let mut cx = Context::from_waker(Waker::noop());
        let mut result = pending.as_mut().poll(&mut cx);
        assert_eq!(
            json!(*trace.borrow()),
            c["before"],
            "orders_before_init {index}"
        );
        if result.is_pending() {
            initialized.set(true);
            result = pending.as_mut().poll(&mut cx);
        }
        assert_eq!(
            json!(*trace.borrow()),
            c["initialized"],
            "orders_before_get {index}"
        );
        if result.is_pending() {
            settled.set(true);
            result = pending.as_mut().poll(&mut cx);
        }
        let Poll::Ready(result) = result else {
            panic!("orders_settlement")
        };
        match result {
            Ok(rows) => {
                assert_eq!(
                    c["outcome"].get("ok"),
                    Some(&json!(true)),
                    "orders_expected_success {index}"
                );
                let source = c["outcome"]["rows"].as_array().unwrap();
                assert_eq!(
                    rows.len(),
                    source.len(),
                    "orders_native_cardinality {index}"
                );
                let mut compared = 0;
                for (i, row) in rows.iter().enumerate() {
                    assert_eq!(row.0, source[i]["id"], "orders_native_id {index}");
                    assert_eq!(row.1, source[i]["price"], "orders_native_price {index}");
                    assert_eq!(row.2, source[i]["size"], "orders_native_size {index}");
                    compared += 1;
                }
                assert_eq!(compared, source.len(), "orders_completed_rows");
                native_outputs += compared;
            }
            Err(e) => {
                assert_eq!(
                    c["outcome"].get("ok"),
                    Some(&json!(false)),
                    "orders_expected_error {index}"
                );
                assert_eq!(e.tag, c["outcome"]["tag"], "orders_native_error {index}");
                match e.tag {
                    "original" => assert!(Rc::ptr_eq(&e, &original)),
                    "logger" => assert!(Rc::ptr_eq(&e, &logger)),
                    "replacement" => assert!(Rc::ptr_eq(&e, &replacement)),
                    "type" => assert_eq!(e.message, c["outcome"]["message"]),
                    _ => unreachable!(),
                }
            }
        }
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "orders_native_trace {index}"
        );
        completed += 1;
    }
    assert_eq!(completed, 36, "orders_completed_cases");
    assert_eq!(native_outputs, 19, "orders_native_total");
    assert_eq!(
        corpus["boundaries"]["inherited"],
        json!(["row-0"]),
        "orders_source_inherited_results"
    );
    assert_eq!(
        corpus["boundaries"]["getter"],
        json!(["row-1"]),
        "orders_source_getter_results"
    );
    assert_eq!(
        corpus["boundaries"]["getterReads"], 3,
        "orders_source_getter_reads"
    );
    assert_eq!(corpus["boundaries"]["surrogateCodeUnit"], 0xd800);
    assert!(
        safe_cex_num::numeric_ingress::js_from_str::<String>(
            corpus["boundaries"]["surrogateRaw"].as_str().unwrap()
        )
        .is_err(),
        "orders_utf8_refusal"
    );
}
