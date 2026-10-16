use super::*;
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture future unexpectedly pending"),
    }
}
fn numeric_json(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numeric_json).collect()),
        Value::Object(o) => {
            Value::Object(o.into_iter().map(|(k, v)| (k, numeric_json(v))).collect())
        }
        v => v,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn rest_market_requests_and_failures_match_live_inverse_and_spot() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/rest-market-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, case) in cases.iter().enumerate() {
        let calls = Rc::new(RefCell::new(vec![]));
        let captured = calls.clone();
        let mut index = 0;
        let mut emissions = vec![];
        let result = ready(fetch_markets_helper(
            case["category"].as_str().unwrap(),
            case["variant"] == "bybitinverse",
            case["publicOnly"] == true,
            |url, params| {
                captured
                    .borrow_mut()
                    .push(json!({"url":url,"params":params}));
                let response = case["responses"][index].clone();
                index += 1;
                std::future::ready(if let Some(error) = response.get("throw") {
                    Err(error.as_str().unwrap().to_owned())
                } else {
                    Ok(response)
                })
            },
            |v| {
                emissions.push(v.cloned().unwrap_or(Value::Null));
                Ok(())
            },
        ));
        assert_eq!(
            *calls.borrow(),
            *case["calls"].as_array().unwrap(),
            "request order case{i}"
        );
        assert_eq!(
            emissions,
            *case["emissions"].as_array().unwrap(),
            "emissions case{i}"
        );
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.err().as_deref(),
                error.as_str(),
                "error case{i}: {case}"
            );
        } else {
            assert_eq!(
                numeric_json(
                    serde_json::to_value(result.unwrap_or_else(|e| panic!("case{i}: {e}")))
                        .unwrap()
                ),
                numeric_json(case["result"].clone()),
                "result case{i}"
            );
        }
    }
    assert_eq!(cases.len(), 191);
}
#[test]
fn dropping_pending_market_read_does_not_issue_later_requests() {
    let calls = Rc::new(RefCell::new(0));
    let captured = calls.clone();
    let mut future = Box::pin(fetch_markets_helper(
        "inverse",
        true,
        false,
        move |_, _| {
            *captured.borrow_mut() += 1;
            std::future::pending::<Result<Value, String>>()
        },
        |_| Ok(()),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(*calls.borrow(), 1);
    drop(future);
    assert_eq!(*calls.borrow(), 1);
}
