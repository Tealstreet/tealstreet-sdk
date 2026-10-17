#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
#[path = "../../safe-cex-exchange/tests/support/store.rs"]
mod test_store;
use runtime::Runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::ExchangeError,
};
use safe_cex_types::ExchangeName;
use safe_cex_webworker::chaser_manager::{
    get_bybit_demo_dead_private_ws_reason, wait_for_bybit_demo_private_ws,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
fn venue(name: ExchangeName, store: Rc<test_store::TestStore>, runtime: Rc<Runtime>) -> Rc<Venue> {
    Rc::new(Venue {
        base: BaseExchange::new(
            name,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store,
            runtime,
            Default::default(),
        ),
        capabilities: Default::default(),
    })
}
fn output(result: Result<Option<String>, ExchangeError>) -> Value {
    match result {
        Ok(reason) => json!({"reason":reason}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    }
}
fn health(text: &Value) -> Option<Value> {
    text.as_str()
        .map(|text| serde_json::from_str(text).unwrap())
}
#[test]
fn diagnostic_and_grace_wait_match_live_health_states_and_coercion_errors() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_health_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    let rows = data["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1328);
    let mut cx = Context::from_waker(Waker::noop());
    for (index, row) in rows.iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        runtime.advance(0);
        let store = Rc::new(test_store::TestStore::new());
        let states = row["states"].as_array().unwrap();
        store.data.borrow_mut().socket_health = health(&states[0]);
        let exchange = venue(
            serde_json::from_value(row["name"].clone()).unwrap(),
            store.clone(),
            runtime.clone(),
        );
        assert_eq!(
            output(get_bybit_demo_dead_private_ws_reason(exchange.as_ref())),
            row["direct"],
            "HEALTH-DIAGNOSTIC row {index}"
        );
        let mut future = Box::pin(wait_for_bybit_demo_private_ws(exchange.as_ref()));
        let mut sleeps = Vec::new();
        let result = loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(result) => break result,
                Poll::Pending => {
                    assert!(sleeps.len() < 4, "GRACE-BOUND row {index}");
                    let start = runtime.now.get();
                    runtime.advance(start + 499);
                    assert!(
                        future.as_mut().poll(&mut cx).is_pending(),
                        "GRACE-DELAY row {index}"
                    );
                    sleeps.push(500);
                    let next = sleeps.len().min(states.len() - 1);
                    // Also proves no store borrow survives suspension.
                    store.data.borrow_mut().socket_health = health(&states[next]);
                    runtime.advance(start + 500);
                }
            }
        };
        assert_eq!(output(result), row["result"], "GRACE-RESULT row {index}");
        assert_eq!(json!(sleeps), row["sleeps"], "GRACE-SLEEPS row {index}");
        assert!(
            runtime.tasks.borrow().is_empty(),
            "wait must not spawn detached work"
        );
    }
    // JSON materialization omits inherited/named-array fields. Source sees true
    // initialization and returns immediately, Rust sees no own true field.
    for (name, health) in [("inherited", json!({})), ("namedArray", json!([]))] {
        assert_eq!(data["witness"][name], Value::Null);
        assert_eq!(data["witness"][format!("{name}Wait")], Value::Null);
        let runtime = Rc::new(Runtime::default());
        runtime.advance(0);
        let store = Rc::new(test_store::TestStore::new());
        store.data.borrow_mut().socket_health = Some(health);
        let exchange = venue(ExchangeName::Bybitdemo, store, runtime.clone());
        assert_eq!(
            get_bybit_demo_dead_private_ws_reason(exchange.as_ref())
                .unwrap()
                .as_deref(),
            Some("it has never delivered a message")
        );
        let mut future = Box::pin(wait_for_bybit_demo_private_ws(exchange.as_ref()));
        for time in [0, 500, 1000, 1500] {
            runtime.advance(time);
            assert!(future.as_mut().poll(&mut cx).is_pending());
        }
        runtime.advance(2000);
        assert!(matches!(
            future.as_mut().poll(&mut cx),
            Poll::Ready(Ok(Some(_)))
        ));
    }
    let raw = data["witness"]["surrogateInput"].as_str().unwrap();
    assert!(raw.contains("\\ud800"));
    assert!(
        serde_json::from_str::<Value>(raw).is_err(),
        "CHOSEN UTF-8 ingress refusal, not invalid JSON"
    );
    assert!(
        data["witness"]["surrogateOutput"]
            .as_str()
            .unwrap()
            .contains("\\ud800")
    );
}
#[test]
fn dropping_grace_wait_releases_exchange_and_holds_no_store_borrow() {
    let runtime = Rc::new(Runtime::default());
    runtime.advance(0);
    let store = Rc::new(test_store::TestStore::new());
    store.data.borrow_mut().socket_health = Some(json!({}));
    let exchange = venue(ExchangeName::Bybitdemo, store.clone(), runtime.clone());
    let weak = Rc::downgrade(&exchange);
    let mut future = Box::pin(wait_for_bybit_demo_private_ws(exchange.as_ref()));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    store.data.borrow_mut().socket_health = Some(json!({"privateInitialized":true}));
    drop(future);
    drop(exchange);
    assert!(weak.upgrade().is_none());
    assert!(runtime.tasks.borrow().is_empty());
}
