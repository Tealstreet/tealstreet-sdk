use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::*,
};
use safe_cex_store::Store;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Runtime(Cell<u64>);
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("notification fixture does not spawn")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        let id = self.0.get();
        self.0.set(id + 1);
        id.to_string()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { unreachable!() })
    }
}
fn fixture() -> (Rc<BaseExchange>, Rc<DefaultStore>) {
    let store = Rc::new(DefaultStore::new());
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"k","secret":"s","tuning":{"consoleLogging":false}}))
            .unwrap(),
        store.clone(),
        Rc::new(Runtime::default()),
        ExchangeSettings::default(),
    ));
    (base, store)
}
fn failure(message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "E_LISTENER"
    )));
    error.config = Some(json!({"url":"/fixture"}));
    error.request = Some(json!({"method":"POST"}));
    error.stack = Some("original stack".into());
    error.context = Some(json!({"topic":"update"}));
    error
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready"),
    }
}

#[test]
fn real_store_commit_preserves_listener_failure_and_stops_both_fanouts() {
    let (base, store) = fixture();
    let expected = failure("update failure");
    let original = expected.clone();
    let events = Rc::new(RefCell::new(Vec::new()));
    let seen = events.clone();
    base.on(
        "update",
        Rc::new(move |_, data| {
            assert_eq!(
                safe_cex_exchange::js_runtime::value::to_json(data).unwrap()["options"]["isHedged"],
                true,
                "notification sees committed setting"
            );
            seen.borrow_mut().push("first");
            Err(original.clone())
        }),
    );
    let seen = events.clone();
    base.on(
        "update",
        Rc::new(move |_, _| {
            seen.borrow_mut().push("later listener");
            Ok(())
        }),
    );
    let seen = events.clone();
    let _cleanup = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            seen.borrow_mut().push("later subscriber");
            Ok(())
        }),
    );
    let error = store.set_setting("isHedged", true).unwrap_err();
    assert!(
        store.options().unwrap().is_hedged,
        "mutation must remain committed after listener failure"
    );
    assert_eq!(
        *events.borrow(),
        ["first"],
        "listener throw must stop later listeners and subscribers"
    );
    let safe_cex_store::StoreMutationError::Notification(notification) = &error else {
        panic!("notification origin retained")
    };
    assert_eq!(
        notification
            .cause()
            .source()
            .unwrap()
            .downcast_ref::<ExchangeError>(),
        Some(&expected),
        "opaque source retains complete ExchangeError"
    );
    let mut recovered_expected = expected.clone();
    recovered_expected
        .store_subscriber_ids
        .push(notification.subscriber_id());
    let original_cause = notification.cause().clone();
    let recovered = ExchangeError::from(error);
    assert!(
        std::ptr::eq(
            recovered
                .source()
                .expect("native listener cause survives conversion"),
            original_cause.source().unwrap(),
        ),
        "native Store conversion preserves original listener pointer"
    );
    recovered_expected.store_subscriber_cause = Some(original_cause);
    assert_eq!(
        recovered, recovered_expected,
        "recover original listener name and metadata"
    );
    store.set_setting("isHedged", true).unwrap();
    assert_eq!(
        *events.borrow(),
        ["first"],
        "unchanged setting does not notify"
    );
}

#[test]
fn non_exchange_subscriber_response_survives_overlay_recovery() {
    let (base, store) = fixture();
    let market = serde_json::from_value(json!({
        "id":"BTC", "symbol":"BTC", "base":"BTC", "quote":"USDT", "active":true,
        "precision":{"amount":1,"price":1},
        "limits":{"amount":{"min":1,"max":5},"leverage":{"min":1,"max":100}}
    }))
    .unwrap();
    Store::update(
        store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let response = json!({"data":{"msg":"preferred"}});
    let expected = response.clone();
    let _cleanup = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            Err(safe_cex_types::StoreSubscriberError::with_response(
                "fallback",
                response.clone(),
            ))
        }),
    );
    let error = ExchangeError::from(store.set_setting("isHedged", true).unwrap_err());
    assert_eq!(
        error.response,
        Some(expected),
        "canonical subscriber response must survive non-ExchangeError projection"
    );
    let messages = Rc::new(RefCell::new(Vec::new()));
    let seen = messages.clone();
    base.on(
        "error",
        Rc::new(move |_, value| {
            seen.borrow_mut()
                .push(String::from_utf16_lossy(value.as_str().unwrap()));
            Ok(())
        }),
    );
    ready(base.load_account_market_overlay(Box::pin(async { Ok(None) }))).unwrap();
    assert_eq!(
        *messages.borrow(),
        ["Exchange Error: preferred"],
        "overlay catch must prefer preserved response.data.msg"
    );
}

#[test]
fn once_is_removed_before_throw_and_captured_list_survives_removal() {
    let (base, _) = fixture();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let events = seen.clone();
    base.once(
        "event",
        Rc::new(move |_, _| {
            events.borrow_mut().push("once");
            Err(failure("once"))
        }),
    );
    let events = seen.clone();
    let weak = Rc::downgrade(&base);
    base.on(
        "event",
        Rc::new(move |_, _| {
            events.borrow_mut().push("remove");
            weak.upgrade().unwrap().off("event", None);
            Ok(())
        }),
    );
    let events = seen.clone();
    base.on(
        "event",
        Rc::new(move |_, _| {
            events.borrow_mut().push("captured");
            Ok(())
        }),
    );
    assert_eq!(
        base.emit(
            "event",
            safe_cex_exchange::js_runtime::value::from_json(&Value::Null)
        )
        .unwrap_err()
        .name,
        "TypeError"
    );
    base.emit(
        "event",
        safe_cex_exchange::js_runtime::value::from_json(&Value::Null),
    )
    .unwrap();
    base.emit(
        "event",
        safe_cex_exchange::js_runtime::value::from_json(&Value::Null),
    )
    .unwrap();
    assert_eq!(
        *seen.borrow(),
        ["once", "remove", "captured"],
        "throw removes once; off does not erase captured callbacks"
    );
}

fn price_order(fail: &str) -> Value {
    let (base, _) = fixture();
    let events = Rc::new(RefCell::new(Vec::new()));
    let expected = failure(&format!("{fail} failure"));
    for event in ["log", "error"] {
        let seen = events.clone();
        let error = expected.clone();
        let throws = event == fail;
        base.on(
            event,
            Rc::new(move |_, _| {
                seen.borrow_mut().push(event.to_owned());
                if throws { Err(error.clone()) } else { Ok(()) }
            }),
        );
        let seen = events.clone();
        base.on(
            event,
            Rc::new(move |_, _| {
                seen.borrow_mut().push(format!("{event}-later"));
                Ok(())
            }),
        );
    }
    // This is the consumer contract from actual findBestPrice, not an implementation
    // of the worker. Its worker-owned witness must consume these fallible operations.
    let result: Result<(), ExchangeError> = (|| {
        base.log("no market or ticker found", "info")?;
        events.borrow_mut().push("stop".into());
        base.emit(
            "error",
            safe_cex_exchange::js_runtime::value::from_json(&json!("missing market")),
        )?;
        Err(ExchangeError::new("missing"))
    })();
    let error = result.unwrap_err();
    assert_eq!(error, expected, "ordered emit preserves original error");
    json!({"kind":"price","fail":fail,"events":*events.borrow(),"name":error.name,"message":error.message})
}
fn listener_rows() -> Vec<Value> {
    let mut rows = Vec::new();
    for event in ["log", "error", "update"] {
        let (base, _) = fixture();
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        let error = failure(&format!("{event} failure"));
        base.on(
            event,
            Rc::new(move |_, _| {
                seen.borrow_mut().push("first");
                Err(error.clone())
            }),
        );
        let seen = events.clone();
        base.on(
            event,
            Rc::new(move |_, _| {
                seen.borrow_mut().push("second");
                Ok(())
            }),
        );
        let error = base
            .emit(
                event,
                safe_cex_exchange::js_runtime::value::from_json(&Value::Null),
            )
            .expect_err("source throw must stop later emitter listeners");
        assert_eq!(
            *events.borrow(),
            ["first"],
            "source throw must stop later emitter listeners"
        );
        rows.push(json!({"kind":"fanout","event":event,"events":*events.borrow(),"name":error.name,"message":error.message}));
    }
    rows.push(price_order("log"));
    rows.push(price_order("error"));
    for log_fails in [false, true] {
        let (base, _) = fixture();
        let events = Rc::new(RefCell::new(Vec::new()));
        let seen = events.clone();
        base.on(
            "chaser:complete",
            Rc::new(move |_, _| {
                seen.borrow_mut().push("complete");
                Err(failure("complete failure"))
            }),
        );
        let seen = events.clone();
        base.on(
            "chaser:complete",
            Rc::new(move |_, _| {
                seen.borrow_mut().push("complete-later");
                Ok(())
            }),
        );
        let seen = events.clone();
        base.on(
            "log",
            Rc::new(move |_, _| {
                seen.borrow_mut().push("log");
                if log_fails {
                    Err(failure("log failure"))
                } else {
                    Ok(())
                }
            }),
        );
        let result: Result<(), ExchangeError> = ready(async {
            if let Err(error) = base.emit(
                "chaser:complete",
                safe_cex_exchange::js_runtime::value::from_json(&json!({})),
            ) {
                base.log(
                    &format!(
                        "chaser:complete listener threw: {}: {}",
                        error.name, error.message
                    ),
                    "info",
                )?;
            }
            Ok(())
        });
        rows.push(json!({"kind":"complete","logFails":log_fails,"events":*events.borrow(),"error":result.err().map(|e|e.message)}));
    }
    rows
}
#[test]
fn caller_order_distinguishes_log_before_stop_from_error_after_stop() {
    assert_eq!(
        price_order("log")["events"],
        json!(["log"]),
        "log throw must precede stop"
    );
    assert_eq!(
        price_order("error")["events"],
        json!(["log", "log-later", "stop", "error"]),
        "error throw must follow stop"
    );
    assert_eq!(listener_rows().len(), 7);
}
#[test]
#[ignore = "mandatory core include-ignored CI installs frozen TinyEmitter and TypeScript"]
fn listener_fanout_and_worker_order_match_actual_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/listener-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "listener source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        listener_rows(),
        expected,
        "actual TinyEmitter and worker listener ordering"
    );
}

#[test]
fn store_notification_conversion_retains_subscriber_identity_on_both_paths() {
    use safe_cex_store::{StoreMutationError, StoreNotificationError};
    use safe_cex_types::StoreSubscriberError;
    let mut original = failure("nested notification");
    original.store_subscriber_ids.push(17);
    let original_cause: StoreSubscriberError = original.clone().into();
    let recovered = ExchangeError::from(StoreMutationError::Notification(
        StoreNotificationError::from_subscriber(42, original_cause.clone()),
    ));
    assert!(
        std::ptr::eq(
            recovered.source().expect("native recovered source exists"),
            original_cause.source().unwrap()
        ),
        "native recovered source preserves exact original cause"
    );
    assert_eq!(
        recovered.store_subscriber_ids,
        [17, 42],
        "recovered ExchangeError retains inner and outer subscriber identity"
    );
    let mut expected = original.clone();
    expected.store_subscriber_ids.push(42);
    expected.store_subscriber_cause = Some(original_cause);
    assert_eq!(
        recovered, expected,
        "subscriber identity does not replace cause metadata"
    );
    assert_eq!(
        original.store_subscriber_ids,
        [17],
        "conversion does not mutate original cause"
    );

    let response = json!({"data":{"msg":"canonical response"}});
    let projected = ExchangeError::from(StoreMutationError::Notification(
        StoreNotificationError::from_subscriber(
            91,
            StoreSubscriberError::with_response("fallback", response.clone()),
        ),
    ));
    assert_eq!(
        projected.store_subscriber_ids,
        [91],
        "projected subscriber error retains subscriber identity"
    );
    assert_eq!(
        projected.response,
        Some(response),
        "subscriber identity preserves response"
    );
    let ordinary = ExchangeError::from(StoreMutationError::InvalidOperation("bad status"));
    assert!(
        ordinary.store_subscriber_ids.is_empty(),
        "non-notification errors invent no subscriber identity"
    );
}
