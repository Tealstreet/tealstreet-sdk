//! Concrete native request/format/batch boundaries, including malformed venue rows.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted placement remained pending");
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/native_reply_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn order() -> safe_cex_types::PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}),
    )
    .unwrap()
}
#[test]
fn native_nonce_client_id_and_fallback_match_source_uuid() {
    let expected = source()["uuid"].as_str().unwrap().to_owned();
    assert_eq!(expected, "fixturenonce");
    let (venue, rt, _, wire) = support::fixture_with(false);
    *rt.id.borrow_mut() = Some("fixture-_nonce".into());
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/trade/place_order".into(),
        json!({"code":0,"data":{}}),
    );
    let ids = drive(&rt, venue.base().place_order(venue.as_ref(), order())).unwrap();
    assert_eq!(
        ids,
        vec![format!("ts_{expected}")],
        "native generated client ID must use utils/uuid"
    );
    let requests = wire.requests.lock().unwrap();
    let request = requests
        .iter()
        .find(|r| r.url.ends_with("/place_order"))
        .unwrap();
    assert_eq!(
        request.headers["nonce"], expected,
        "native REST nonce must strip both separators"
    );
    let body: Value = match request.body.as_ref().unwrap() {
        safe_cex_transport::RestBody::Untransformed(v) => v.clone(),
        safe_cex_transport::RestBody::Transformed(t) => match t.value() {
            Value::String(s) => serde_json::from_str(s).unwrap(),
            v => v.clone(),
        },
        safe_cex_transport::RestBody::Encoded(b) => serde_json::from_slice(b).unwrap(),
    };
    assert_eq!(body["clientId"], format!("ts_{expected}"));
    drop(requests);
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/trade/batch_order".into(),
        json!({"code":0,"data":{"successList":[{}],"failureList":[]}}),
    );
    let ids = drive(
        &rt,
        venue
            .base()
            .place_orders(venue.as_ref(), vec![order(), order()]),
    )
    .unwrap();
    assert_eq!(
        ids,
        vec![expected],
        "batch missing-ID fallback uses utils/uuid"
    );
    venue.dispose().unwrap();
}
#[test]
fn null_batch_rows_match_actual_source_errors_prefix_and_next_chunk() {
    for case in source()["cases"].as_array().unwrap() {
        let (venue, rt, _, wire) = support::fixture_with(false);
        *rt.id.borrow_mut() = Some("fixture-_nonce".into());
        wire.bodies.lock().unwrap().insert(
            "/api/v1/futures/trade/batch_order".into(),
            json!({"code":0,"data":case["reply"]}),
        );
        let errors = Rc::new(RefCell::new(vec![]));
        let sink = errors.clone();
        venue.base().on(
            "error",
            Rc::new(move |_, v| {
                sink.borrow_mut().push(v.as_str().unwrap().to_owned());
                Ok(())
            }),
        );
        let ids = drive(
            &rt,
            venue
                .base()
                .place_orders(venue.as_ref(), (0..7).map(|_| order()).collect()),
        )
        .unwrap();
        assert_eq!(
            json!(ids),
            case["ids"],
            "malformed batch must retain only source IDs: {case}"
        );
        assert_eq!(
            json!(
                errors
                    .borrow()
                    .iter()
                    .map(|s| s.strip_prefix("Exchange Error: ").unwrap())
                    .collect::<Vec<_>>()
            ),
            case["errors"],
            "exact caught TypeError/chunk behavior: {case}"
        );
        assert_eq!(
            wire.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.url.ends_with("/batch_order"))
                .count(),
            case["calls"].as_u64().unwrap() as usize,
            "later chunks still run"
        );
        venue.dispose().unwrap();
    }
}
