//! Constructed production client signing, with actual source API interceptor expectations.
use crate::native_query_fixture as fixture;
use safe_cex_num::JsJsonValue;
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(runtime: &fixture::Runtime, future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.now.set(runtime.now.get() + 1000.0);
        runtime.pump();
    }
    panic!("production query read remained pending");
}

#[test]
fn native_query_signatures_match_source_locale_sort_at_every_level() {
    let (venue, runtime, _, wire) = fixture::fixture_with(false);
    *runtime.id.borrow_mut() = Some("fixture-_nonce".into());
    let inputs = [
        json!({"Zeta":11,"alpha":13,"_b":17}),
        json!({"ids":[0,1,2,3,4,5,6,7,8,9,10,11],"nested":{"Zeta":"U","alpha":"L","_b":"P"}}),
    ];
    let mut samples = Vec::new();
    for params in inputs {
        let config = JsJsonValue::from(json!({"params":params.clone()}));
        drive(&runtime, venue.xhr().get("/api/v1/futures/account", config)).unwrap();
        let requests = wire.requests.lock().unwrap();
        let request = requests.last().unwrap();
        let timestamp = request.headers["timestamp"].parse::<f64>().unwrap();
        samples.push(json!({"params":params,"nonce":request.headers["nonce"],
            "timeBits":format!("{:016x}",timestamp.to_bits()),"actual":request.headers["sign"]}));
    }
    assert_eq!(
        wire.requests.lock().unwrap().len(),
        2,
        "two actual signed client reads"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/api-oracle.cjs"))
        .arg("--native-query")
        .arg(serde_json::to_string(&samples).unwrap())
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(expected.len(), 2, "actual source interceptor corpus");
    for (sample, source) in samples.iter().zip(expected) {
        assert_eq!(
            sample["actual"], source["sign"],
            "production native query signature uses source locale ordering: {}",
            sample["params"]
        );
    }
}
