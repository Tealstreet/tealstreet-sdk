use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_paradex::{leverage_reads::fetch_leverage_settings, types::MarginResponse};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<&'static str>;
struct Held {
    ready: Rc<Cell<bool>>,
    result: Option<Result<Option<MarginResponse>, Error>>,
}
impl Future for Held {
    type Output = Result<Option<MarginResponse>, Error>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.ready.get() {
            Poll::Ready(self.result.take().unwrap())
        } else {
            Poll::Pending
        }
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn definitions() -> Vec<Value> {
    let config = |market: &str, leverage: Value| json!({"market":market,"leverage":leverage,"margin_type":"CROSS","isolated_margin_leverage":51.25});
    let body = |configs: Value| json!({"account":"account-A","margin_methodology":"cross","configs":configs});
    let base = body(json!([
        config("BTC", json!(5)),
        config("ETH", json!(7.5)),
        config("BTC", json!(11)),
        config("ZERO", json!(0))
    ]))
    .to_string();
    let mut out = Vec::new();
    let mut add = |raw: Option<String>, extra: Value| {
        let mut v =
            json!({"raw":raw,"getError":"","throwLog":-1,"setError":-1,"stringifyError":false});
        for (k, v2) in extra.as_object().unwrap() {
            v[k] = v2.clone();
        }
        out.push(v);
    };
    add(Some(base.clone()), json!({}));
    add(Some(body(json!([])).to_string()), json!({}));
    add(None, json!({}));
    add(Some("null".into()), json!({}));
    for raw in [
        "0",
        "-0",
        "-0.0",
        "-1e-9999",
        "1e400",
        "-1e400",
        "9007199254740993",
        "1.0000000000000002",
    ] {
        add(
            Some(
                body(json!([config("numeric", json!("TOKEN"))]))
                    .to_string()
                    .replace("\"TOKEN\"", raw),
            ),
            json!({}),
        );
    }
    for extra in [
        json!({"getError":"sync"}),
        json!({"getError":"async"}),
        json!({"throwLog":0}),
        json!({"throwLog":1}),
        json!({"getError":"async","throwLog":1}),
        json!({"getError":"async","stringifyError":true}),
        json!({"setError":0}),
        json!({"setError":1}),
        json!({"setError":2}),
        json!({"setError":0,"throwLog":1}),
    ] {
        add(Some(base.clone()), extra);
    }
    out
}
#[test]
fn actual_typescript_leverage_reads() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/leverage_reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "leverage_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 22, "leverage_source_cases");
    let expected = definitions();
    assert_eq!(expected.len(), 22);
    let counts = [
        3, 2, 2, 2, 2, 2, 2, 2, 3, 2, 3, 3, 2, 2, 2, 3, 2, 2, 2, 2, 3, 2,
    ];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        s => s,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let mut total = 0;
    let mut completed = 0;
    let mut all_sets = 0;
    for (i, c) in cases.iter().enumerate() {
        // Compare JSON token spelling as well as fields: numeric ingress is under test.
        let normalize = |raw: &Value| {
            raw.as_str().map(|s| {
                safe_cex_num::numeric_ingress::js_deserialize_passthrough(
                    &mut serde_json::Deserializer::from_str(s),
                )
                .unwrap()
            })
        };
        assert_eq!(
            normalize(c.get("raw").expect("leverage_required_raw")),
            normalize(&expected[i]["raw"]),
            "leverage_live_input_content {i}"
        );
        if let (Some(a), Some(b)) = (c["raw"].as_str(), expected[i]["raw"].as_str()) {
            let source_number = a
                .split("\"leverage\":")
                .nth(1)
                .map(|s| s.split(',').next().unwrap());
            let native_number = b
                .split("\"leverage\":")
                .nth(1)
                .map(|s| s.split(',').next().unwrap());
            assert_eq!(
                source_number, native_number,
                "leverage_live_numeric_token {i}"
            );
        }
        for key in ["getError", "throwLog", "setError", "stringifyError"] {
            assert_eq!(
                c.get(key),
                expected[i].get(key),
                "leverage_live_input_content {i} {key}"
            );
        }
        let input = match c["raw"].as_str() {
            None | Some("null") => None,
            Some(raw) => Some(js_from_str::<MarginResponse>(raw).unwrap()),
        };
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let state = Rc::new(RefCell::new(vec![
            ("stale".to_string(), 29.0),
            ("BTC".to_string(), 31.0),
        ]));
        let ready = Rc::new(Cell::new(false));
        let original = Rc::new("original");
        let logger = Rc::new("logger");
        let replacement = Rc::new("replacement");
        let mut logs = 0;
        let mut sets = 0;
        let mut pending = Box::pin(fetch_leverage_settings(
            |endpoint| {
                trace.borrow_mut().push(json!(["get", endpoint]));
                if c["getError"] == "sync" {
                    Err(original.clone())
                } else {
                    Ok(Held {
                        ready: ready.clone(),
                        result: Some(if c["getError"] == "async" {
                            Err(original.clone())
                        } else {
                            Ok(input)
                        }),
                    })
                }
            },
            |key, value| {
                trace
                    .borrow_mut()
                    .push(json!(["set",key,{"kind":"number","bits":bits(value)}]));
                let fail = sets == c["setError"].as_i64().unwrap();
                sets += 1;
                if fail {
                    return Err(original.clone());
                }
                let mut state = state.borrow_mut();
                if let Some(entry) = state.iter_mut().find(|(k, _)| k == key) {
                    entry.1 = value;
                } else {
                    state.push((key.to_owned(), value));
                }
                Ok(())
            },
            |e: &Error| {
                trace.borrow_mut().push(json!(["stringify", **e]));
                if **e == "original" && c["stringifyError"] == true {
                    Err(replacement.clone())
                } else {
                    Ok(e.to_string())
                }
            },
            |text, severity| {
                trace.borrow_mut().push(json!(["log", text, severity]));
                let fail = logs == c["throwLog"].as_i64().unwrap();
                logs += 1;
                if fail { Err(logger.clone()) } else { Ok(()) }
            },
        ));
        let mut cx = Context::from_waker(Waker::noop());
        let mut result = pending.as_mut().poll(&mut cx);
        let before = trace.borrow().clone();
        if result.is_pending() {
            ready.set(true);
            result = pending.as_mut().poll(&mut cx);
        }
        let Poll::Ready(result) = result else {
            panic!("leverage_settlement")
        };
        drop(pending);
        match result {
            Ok(()) => {
                assert_eq!(
                    c["outcome"].get("ok"),
                    Some(&json!(true)),
                    "leverage_expected_success {i}"
                );
                assert_eq!(c["outcome"].get("undefined"), Some(&json!(true)));
            }
            Err(e) => {
                assert_eq!(
                    c["outcome"].get("ok"),
                    Some(&json!(false)),
                    "leverage_expected_error {i}"
                );
                assert_eq!(**e, c["outcome"]["tag"]);
                assert!(
                    Rc::ptr_eq(&e, &logger) || Rc::ptr_eq(&e, &replacement),
                    "leverage_replacement_identity"
                );
            }
        }
        let source = c["state"].as_array().unwrap();
        assert_eq!(source.len(), counts[i], "leverage_source_state_count {i}");
        assert_eq!(
            state.borrow().len(),
            counts[i],
            "leverage_native_state_count {i}"
        );
        let mut compared = 0;
        for (j, (key, value)) in state.borrow().iter().enumerate() {
            assert_eq!(
                key,
                source[j][0].as_str().unwrap(),
                "leverage_native_key {i}"
            );
            assert_eq!(
                bits(*value),
                source[j][1]["bits"],
                "leverage_native_value {i}"
            );
            compared += 1;
        }
        assert_eq!(compared, counts[i], "leverage_completed_entries");
        assert_eq!(json!(before), c["before"], "leverage_before_settlement {i}");
        assert_eq!(
            json!(*trace.borrow()),
            c["trace"],
            "leverage_native_trace {i}"
        );
        total += compared;
        completed += 1;
        all_sets += sets;
    }
    assert_eq!(total, 50, "leverage_native_total");
    assert_eq!(completed, 22, "leverage_completed_cases");
    assert_eq!(all_sets, 16, "leverage_set_calls");
    assert_eq!(
        corpus["boundary"]["state"][2],
        json!(["typed-string",{"kind":"string","value":"5"}]),
        "leverage_source_unchecked_string"
    );
    assert!(
        js_from_str::<MarginResponse>(corpus["boundary"]["raw"].as_str().unwrap()).is_err(),
        "leverage_typed_numeric_refusal"
    );
    let shapes = corpus["shapes"].as_array().unwrap();
    assert_eq!(shapes.len(), 5, "leverage_shape_boundaries");
    let inputs = [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!(0)),
        Some(json!("")),
    ];
    for (i, shape) in shapes.iter().enumerate() {
        let raw: Value = serde_json::from_str(shape["raw"].as_str().unwrap()).unwrap();
        let mut expected = json!({"account":"account-A","margin_methodology":"cross"});
        if let Some(value) = &inputs[i] {
            expected["configs"] = value.clone();
        }
        assert_eq!(raw, expected, "leverage_boundary_live_content");
        assert_eq!(
            shape["outcome"].get("ok"),
            Some(&json!(true)),
            "leverage_source_falsy_configs"
        );
        assert_eq!(
            shape["trace"][2],
            json!(["log", "Margin API returned invalid data", "warning"])
        );
        assert!(
            js_from_str::<MarginResponse>(shape["raw"].as_str().unwrap()).is_err(),
            "leverage_typed_configs_refusal"
        );
    }
}
