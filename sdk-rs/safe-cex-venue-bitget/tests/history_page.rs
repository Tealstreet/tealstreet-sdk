use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_number_string};
use safe_cex_venue_bitget::history_reads::fetch_fills_page;
use serde_json::{Value, json};
use std::{
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
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture read unexpectedly pending"),
    }
}
fn decode(v: &Value) -> Option<Value> {
    if v.get("$undefined").is_some() {
        return None;
    }
    if let Some(number) = v.get("$number").and_then(Value::as_str) {
        return Some(
            serde_json::from_str(match number {
                "Infinity" => "1e400",
                "-Infinity" => "-1e400",
                number => number,
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
fn encode(v: &Value) -> Value {
    match v {
        Value::Number(_) => {
            let n = safe_cex_num::js_number(Some(v)).unwrap();
            json!({"$number":if n==0.0&&n.is_sign_negative(){"-0".into()}else{js_number_string(n)}})
        }
        Value::Array(a) => Value::Array(a.iter().map(encode).collect()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), encode(v))).collect()),
        v => v.clone(),
    }
}
fn check(oracle: &Value) {
    for case in oracle["cases"].as_array().unwrap() {
        let data = decode(&case["data"]);
        let params = decode(&case["params"]).unwrap();
        let mut calls = Vec::new();
        let failure = case.get("failure").map(|f| {
            let mut e = ExchangeError::new(f["message"].as_str().unwrap());
            e.name = f["name"].as_str().unwrap().into();
            e.context = Some(json!({"retained":"read failure metadata"}));
            Box::new(e) as Box<dyn Error>
        });
        let identity = failure
            .as_ref()
            .map(|e| std::ptr::from_ref(e.as_ref()) as *const ());
        let result = run(fetch_fills_page(params, |url, params| {
            calls.push(json!({"url":url,"params":encode(&params)}));
            std::future::ready(match failure {
                Some(e) => Err(e),
                None => Ok(data),
            })
        }));
        let expected = &case["output"];
        if expected.get("value").is_some() {
            assert_eq!(
                expected["sameRows"],
                case["data"]
                    .get("data")
                    .and_then(|d| d.get("fillList"))
                    .is_some_and(Value::is_array),
                "_fetchFillsPage source returns the original list {}",
                case["name"]
            );
        }
        assert_eq!(
            calls.len(),
            1,
            "_fetchFillsPage exactly one read {}",
            case["name"]
        );
        assert_eq!(
            calls[0]["url"], case["calls"][0]["url"],
            "_fetchFillsPage endpoint {}",
            case["name"]
        );
        assert_eq!(
            calls[0]["params"], case["calls"][0]["params"],
            "_fetchFillsPage params {}",
            case["name"]
        );
        assert_eq!(
            case["calls"][0]["sameParams"], true,
            "source passes caller params by reference"
        );
        if let Some(expected) = expected.get("error") {
            let error =
                result.expect_err(&format!("_fetchFillsPage expected error {}", case["name"]));
            let name = if let Some(e) = error.downcast_ref::<JsCoercionError>() {
                e.name
            } else {
                error.downcast_ref::<ExchangeError>().unwrap().name.as_str()
            };
            assert_eq!(
                name, expected["name"],
                "_fetchFillsPage error name {}",
                case["name"]
            );
            assert_eq!(
                error.downcast_ref::<JsCoercionError>().map_or_else(
                    || error
                        .downcast_ref::<ExchangeError>()
                        .unwrap()
                        .message
                        .as_str(),
                    |error| error.message
                ),
                expected["message"],
                "_fetchFillsPage error message {}",
                case["name"]
            );
            if let Some(identity) = identity {
                assert_eq!(
                    std::ptr::from_ref(error.as_ref()) as *const (),
                    identity,
                    "_fetchFillsPage propagates original read rejection"
                );
                assert_eq!(
                    error.downcast_ref::<ExchangeError>().unwrap().context,
                    Some(json!({"retained":"read failure metadata"}))
                );
                assert_eq!(expected["sameFailure"], true);
            }
        } else {
            let (rows, end_id) = result.unwrap_or_else(|error| {
                panic!("_fetchFillsPage expected success {}: {error}", case["name"])
            });
            let mut value = json!({"rows":encode(&Value::Array(rows)),"endId":{"$undefined":true}});
            if let Some(id) = end_id {
                value["endId"] = json!(id);
            }
            assert_eq!(
                value, expected["value"],
                "_fetchFillsPage value {}",
                case["name"]
            );
        }
    }
    assert_eq!(
        oracle["alias"]["afterInput"],
        json!([{"id":"after-input"},{"id":"added-input"}]),
        "source page rows observe response mutation"
    );
    assert_eq!(
        oracle["alias"]["afterOutput"],
        json!([{"id":"after-output"},{"id":"added-input"},{"id":"added-output"}]),
        "source response observes returned rows mutation"
    );
    let mut input = json!({"data":{"fillList":[{"id":"before"}]}});
    let (mut rows, _) = run(fetch_fills_page::<Box<dyn Error>, _>(json!({}), |_, _| {
        std::future::ready(Ok(Some(input.clone())))
    }))
    .unwrap();
    input["data"]["fillList"][0]["id"] = json!("after-input");
    input["data"]["fillList"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":"added-input"}));
    assert_eq!(
        rows.len(),
        1,
        "CHOSEN response array growth does not change returned snapshot"
    );
    assert_eq!(
        rows[0]["id"], "before",
        "CHOSEN page rows are owned snapshots"
    );
    rows[0]["id"] = json!("after-output");
    rows.push(json!({"id":"added-output"}));
    assert_eq!(
        input["data"]["fillList"][1]["id"], "added-input",
        "CHOSEN returned array growth does not change response snapshot"
    );
    assert_eq!(
        input["data"]["fillList"][0]["id"], "after-input",
        "CHOSEN response does not observe snapshot mutation"
    );
    for witness in oracle["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(witness["json"].as_str().unwrap()).is_err(),
            "page UTF8 ingress refusal"
        );
        assert!(
            witness["endIdJson"].as_str().unwrap().contains("\\ud"),
            "source endId retains lone surrogate"
        );
        assert!(
            witness["rowsJson"].as_str().unwrap().contains("\\ud"),
            "source rows retain lone surrogate"
        );
    }
}
#[test]
fn frozen_fills_page() {
    check(&serde_json::from_str(include_str!("history-page-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_fills_page() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-page-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let oracle: Value = serde_json::from_slice(&result.stdout).unwrap();
    check(&oracle);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("history-page-oracle.json")).unwrap(),
        "fills page source drift"
    );
}
#[test]
fn dropped_page_drops_pending_read() {
    use std::{cell::Cell, rc::Rc};
    struct Guard(Rc<Cell<bool>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = Rc::new(Cell::new(false));
    let flag = dropped.clone();
    let mut future = Box::pin(fetch_fills_page::<Box<dyn Error>, _>(
        json!({}),
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
    assert!(dropped.get(), "page read stays owned by caller future");
}
