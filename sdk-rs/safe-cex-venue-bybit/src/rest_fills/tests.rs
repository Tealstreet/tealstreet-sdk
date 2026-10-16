use super::*;
use std::{
    pin::pin,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut f = pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(r) => r,
        _ => panic!("unexpected pending fixture"),
    }
}
fn norm(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(norm).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        v => v,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn historical_fill_pagination_and_recovery_match_live_methods() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/rest-fill-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, c) in cases.iter().enumerate() {
        let opts: ExecutionsOptions = serde_json::from_value(c["opts"].clone()).unwrap();
        let mut calls = vec![];
        let mut diagnostics = vec![];
        let mut fallback_calls = vec![];
        let mut index = 0;
        let mut fallback_index = 0;
        let result = ready(fetch_historical_fills(
            &opts,
            c["publicOnly"] == true,
            if c["variant"] == "bybitinverse" {
                "inverse"
            } else {
                "linear"
            },
            c["variant"].as_str().unwrap(),
            |params| {
                calls.push(json!({"url":"/v5/execution/list","params":params}));
                let data = c["responses"]
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| json!({"retMsg":"OK","result":{"list":[]}}));
                index += 1;
                std::future::ready(if let Some(e) = data.get("throw") {
                    Err(e.as_str().unwrap().into())
                } else {
                    Ok(data)
                })
            },
            |lookback| {
                fallback_calls.push(json!(lookback));
                let data = c["fallbacks"]
                    .get(fallback_index)
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                fallback_index += 1;
                std::future::ready(if let Some(e) = data.get("throw") {
                    Err(e.as_str().unwrap().into())
                } else {
                    Ok(serde_json::from_value(data).unwrap())
                })
            },
            |channel, msg| diagnostics.push(json!([channel, msg])),
        ));
        assert_eq!(
            norm(json!(calls)),
            norm(c["calls"].clone()),
            "requests case{i}: {c}"
        );
        assert_eq!(
            json!(diagnostics),
            c["diagnostics"],
            "diagnostics case{i}: {c}"
        );
        assert_eq!(
            norm(json!(fallback_calls)),
            norm(c["fallbackCalls"].clone()),
            "fallbacks case{i}"
        );
        assert_eq!(
            norm(serde_json::to_value(result.unwrap()).unwrap()),
            norm(c["result"].clone()),
            "fills case{i}: {c}"
        );
    }
    assert_eq!(cases.len(), 112);
}

#[test]
fn dropping_pending_fill_read_never_starts_recovery() {
    use std::cell::Cell;
    let reads = Cell::new(0);
    let recoveries = Cell::new(0);
    let opts: ExecutionsOptions = serde_json::from_value(json!({"symbol":"BTCUSD"})).unwrap();
    let mut future = Box::pin(fetch_historical_fills(
        &opts,
        false,
        "linear",
        "bybit",
        |_| {
            reads.set(reads.get() + 1);
            std::future::pending::<Result<Value, String>>()
        },
        |_| {
            recoveries.set(recoveries.get() + 1);
            std::future::ready(Ok(vec![]))
        },
        |_, _| panic!("pending primary read must not emit recovery diagnostics"),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(reads.get(), 1);
    drop(future);
    assert_eq!(reads.get(), 1);
    assert_eq!(recoveries.get(), 0);
}

#[test]
fn dropping_pending_fill_fallback_never_starts_extended_lookup() {
    use std::cell::Cell;
    let recoveries = Cell::new(0);
    let opts: ExecutionsOptions = serde_json::from_value(json!({"symbol":"BTCUSD"})).unwrap();
    let mut future = Box::pin(fetch_historical_fills(
        &opts,
        false,
        "linear",
        "bybit",
        |_| std::future::ready(Ok(json!({"retMsg":"OK","result":{"list":[]}}))),
        |_| {
            recoveries.set(recoveries.get() + 1);
            std::future::pending::<Result<Vec<Execution>, String>>()
        },
        |_, _| {},
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(recoveries.get(), 1);
    drop(future);
    assert_eq!(recoveries.get(), 1);
}
