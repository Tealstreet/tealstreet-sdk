//! Original constructor-installed API client with a real BaseExchange listener.
use super::{HyperliquidDependencies, HyperliquidExchange, RuntimeClock};
use crate::{binding_test_runtime as host, rate_limit::RateLimiter};
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown},
    native_store::DefaultStore,
};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::Arc,
};

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn api_signing_audit_listener_failure_rejects_before_transport() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-signing-audit.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 6);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        for row in source {
            let runtime = Rc::new(host::Host::with_native(safe_cex_exchange::native::native_runtime()));
            let routes = host::Routes::new(vec![]);
            let options = json!({"key":"","secret":"not-a-valid-key","applicationId":"0x1111111111111111111111111111111111111111"});
            let exchange = HyperliquidExchange::new(
                serde_json::from_value(options.clone()).unwrap(), Rc::new(DefaultStore::new()),
                HyperliquidDependencies {runtime: runtime.clone(), http: routes.clone(),
                    limiter: Rc::new(RateLimiter::new(Arc::new(RuntimeClock::new(runtime.clone())), 1200., 60_000.)),
                    signers: Default::default(), external_public_data: None},
            ).unwrap();
            let sentinel = Js::object(vec![("marker", Js::str("listener-origin")), ("units", Js::utf16(&[0xd800])), ("zero", Js::Number(-0.))]);
            let failure = Thrown::new(sentinel).to_exchange_error();
            let expected = failure.clone();
            let calls = Rc::new(Cell::new(0));
            let observer = calls.clone();
            let messages = Rc::new(RefCell::new(Vec::new()));
            let message_observer = messages.clone();
            let throws = row["throws"].as_bool().unwrap();
            exchange.base.on("error", Rc::new(move |_, value| {message_observer.borrow_mut().push(value.clone()); observer.set(observer.get() + 1); if throws {Err(failure.clone())} else {Ok(())}}));
            let mut r = exchange.binding.borrow().as_ref().unwrap().realm.clone();
            let factory = r.import("createExchangeAPI").unwrap();
            let js_options = r.encode_json(&options);
            let client = r.call(&factory, &Js::Undefined, &[js_options]).unwrap();
            let body = r.encode_json(&json!({"action":{"type":row["kind"]},"nonce":1}));
            let pending = r.call_method(&client, "post", &[Js::str("/exchange"), body.clone()]).unwrap();
            assert!(matches!(&pending, Js::Promise(_)));
            let concurrent = r.call_method(&client, "post", &[Js::str("/exchange"), body]).unwrap();
            assert!(matches!(&concurrent, Js::Promise(_)));
            let first = r.await_value(pending).await;
            let second = r.await_value(concurrent).await;
            for result in [first, second] {
                assert_eq!(result.is_err(), row["rejected"], "HL_API_SIGNING native listener rejection for {}", row["kind"]);
                if let Err(error) = result {
                    assert_eq!(r.exchange_error(&error).origin, expected.origin, "HL_API_SIGNING original callback cause");
                    assert_eq!(row["exact"], true);
                }
            }
            assert_eq!(calls.get(), row["calls"].as_u64().unwrap());
            assert_eq!(routes.requests.borrow().len(), row["adapters"].as_u64().unwrap() as usize, "HL_API_SIGNING transport must follow callback success only");
            let expected_message: Vec<u16> = row["message"].as_str().unwrap().encode_utf16().collect();
            for value in messages.borrow().iter() {
                assert_eq!(value.as_str().unwrap().as_ref(), expected_message.as_slice(), "HL_API_SIGNING listener message");
            }
        }
    });
}
