use safe_cex_exchange::{exchange::*, runtime::*};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_binance::public_start::*;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "test-id".into()
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
        panic!("unexpected worker")
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn public_start_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-start.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 198);
    for (index, c) in cases.iter().enumerate() {
        let store = Rc::new(DefaultStore::new());
        Store::update(store.as_ref(),StoreUpdate{
   loaded:Some(serde_json::from_value(c["loaded"].clone()).unwrap()),
   balance:Some(serde_json::from_value(json!({"used":1,"free":2,"total":3,"upnl":4,"info":{"old":true},"currencies":{},"coins":{}})).unwrap()),
   orders:Some(vec![serde_json::from_value(json!({"id":"o","status":"open","symbol":"BTCUSDT","type":"limit","side":"buy","price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":1})).unwrap()]),
   positions:Some(vec![serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":"long","entryPrice":1,"notional":2,"leverage":3,"unrealizedPnl":4,"contracts":5,"liquidationPrice":6})).unwrap()]),
   ..Default::default()
  },false).unwrap();
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Binance,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store.clone(),
            Rc::new(Runtime),
            ExchangeSettings::default(),
        ));
        if c["disposed"] == true {
            base.dispose().unwrap();
        }
        let events = Rc::new(RefCell::new(Vec::new()));
        let log_events = events.clone();
        let logs = Rc::new(RefCell::new(Vec::new()));
        let captured = logs.clone();
        let log_store = store.clone();
        let log_base = Rc::downgrade(&base);
        let action = c["action"].as_str().unwrap().to_owned();
        base.on_args(
            "log",
            Rc::new(move |_, v| {
                let v: Vec<serde_json::Value> = v
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
                captured.borrow_mut().push(serde_json::json!(v));
                log_events.borrow_mut().push("log");
                if action == "loaded" {
                    let mut loaded = Store::loaded(log_store.as_ref()).clone();
                    loaded.markets = true;
                    loaded.tickers = true;
                    Store::update(
                        log_store.as_ref(),
                        StoreUpdate {
                            loaded: Some(loaded),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                }
                if action == "dispose" {
                    log_base.upgrade().unwrap().dispose().unwrap();
                }
                Ok(())
            }),
        );
        let snapshot = || json!({"orders":Store::orders(store.as_ref()).to_vec(),"positions":Store::positions(store.as_ref()).to_vec(),"balance":Store::balance(store.as_ref()).clone(),"loaded":Store::loaded(store.as_ref()).clone()});
        let before = snapshot();
        let updates = Rc::new(RefCell::new(0));
        let captured = updates.clone();
        let update_events = events.clone();
        let _unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, from_worker| {
                assert!(!from_worker);
                *captured.borrow_mut() += 1;
                update_events.borrow_mut().push("update");
                Ok(())
            }),
        );
        match c["variant"].as_str().unwrap() {
            "binance" => start_binance_public_data_only(&base),
            "binancepm" => start_binance_pm_public_data_only(&base),
            "binancespot" => start_binance_spot_public_data_only(&base),
            _ => unreachable!(),
        }
        .unwrap();
        assert_eq!(
            *updates.borrow(),
            c["updates"].as_array().unwrap().len(),
            "update count case {index}"
        );
        assert_eq!(
            json!(*events.borrow()),
            c["events"],
            "event order case {index}"
        );
        let mut expected = if c["disposed"] == true {
            before
        } else {
            c["updates"].as_array().unwrap().last().unwrap().clone()
        };
        expected["balance"] = serde_json::to_value(
            serde_json::from_value::<Balance>(expected["balance"].clone()).unwrap(),
        )
        .unwrap();
        assert_eq!(snapshot(), expected, "startup state case {index}");
        assert_eq!(
            logs.borrow().len(),
            c["logs"].as_array().unwrap().len(),
            "log count case {index}"
        );
        if c["disposed"] == false {
            assert_eq!(
                logs.borrow()[0],
                json!([
                    format!("[binance] {}", c["logs"][0].as_str().unwrap()),
                    "info"
                ]),
                "log payload case {index}"
            );
        }
    }
}

#[test]
fn startup_preserves_notification_identity_and_log_failure_order() {
    use safe_cex_store::StoreMutationError;
    for start in [
        start_binance_public_data_only,
        start_binance_pm_public_data_only,
        start_binance_spot_public_data_only,
    ] {
        let store = Rc::new(DefaultStore::new());
        let base = BaseExchange::new(
            ExchangeName::Binance,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store.clone(),
            Rc::new(Runtime),
            ExchangeSettings::default(),
        );
        let cause = StoreSubscriberError::new("subscriber sentinel")
            .with_cause(Rc::new(ExchangeError::new("opaque original")));
        let failure = cause.clone();
        let first = Store::subscribe(store.as_ref(), Box::new(move |_, _| Err(failure.clone())));
        let second_calls = Rc::new(RefCell::new(0));
        let calls = second_calls.clone();
        let second = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                *calls.borrow_mut() += 1;
                Ok(())
            }),
        );
        let error = start(&base).unwrap_err();
        let Some(StoreMutationError::Notification(notification)) =
            error.downcast_ref::<StoreMutationError>()
        else {
            panic!("startup retains original StoreMutationError notification");
        };
        assert_eq!(
            notification.cause(),
            &cause,
            "startup retains opaque subscriber cause identity"
        );
        let id = notification.subscriber_id();
        let repeated = start(&base).unwrap_err();
        let Some(StoreMutationError::Notification(repeated)) =
            repeated.downcast_ref::<StoreMutationError>()
        else {
            panic!("startup notification variant retained");
        };
        assert_eq!(
            repeated.subscriber_id(),
            id,
            "startup retains subscriber id"
        );
        assert_eq!(
            *second_calls.borrow(),
            0,
            "startup skips later subscriber after throw"
        );
        assert!(
            store.loaded().orders && store.loaded().positions && store.loaded().balance,
            "startup commits before notification failure"
        );
        first();
        second();

        let mut sentinel = ExchangeError::new("log sentinel");
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "log-code"
        )));
        let failure = sentinel.clone();
        base.on("log", Rc::new(move |_, _| Err(failure.clone())));
        let calls = second_calls.clone();
        let unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                *calls.borrow_mut() += 1;
                Ok(())
            }),
        );
        let error = start(&base).expect_err("startup propagates log failure before Store mutation");
        assert_eq!(
            error.downcast_ref::<ExchangeError>(),
            Some(&sentinel),
            "startup log failure identity"
        );
        assert_eq!(
            *second_calls.borrow(),
            0,
            "startup log failure precedes Store mutation"
        );
        unsubscribe();
    }
}

#[test]
fn startup_box_retains_cause_beyond_structural_subscriber_ids() {
    use safe_cex_store::StoreMutationError;
    use std::error::Error;
    for start in [
        start_binance_public_data_only,
        start_binance_pm_public_data_only,
        start_binance_spot_public_data_only,
    ] {
        for recovered in [false, true] {
            let store = Rc::new(DefaultStore::new());
            let base = BaseExchange::new(
                ExchangeName::Binance,
                serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
                store.clone(),
                Rc::new(Runtime),
                ExchangeSettings::default(),
            );
            let mut original = ExchangeError::new("original callback");
            original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "callback-code"
            )));
            original.store_subscriber_ids = vec![41, 73];
            let cause: Rc<dyn Error> = if recovered {
                Rc::new(original.clone())
            } else {
                Rc::new(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "opaque callback",
                ))
            };
            let subscriber_error = StoreSubscriberError::with_response(
                "subscriber wrapper",
                json!({"data":{"msg":"wrapper response"}}),
            )
            .with_cause(cause.clone());
            let unsubscribe = Store::subscribe(
                store.as_ref(),
                Box::new(move |_, _| Err(subscriber_error.clone())),
            );
            let error = start(&base).unwrap_err();
            let Some(mutation @ StoreMutationError::Notification(notification)) =
                error.downcast_ref::<StoreMutationError>()
            else {
                panic!("startup retains Store mutation variant after ID landing");
            };
            assert!(
                notification
                    .cause()
                    .source()
                    .is_some_and(|retained| std::ptr::eq(retained, cause.as_ref())),
                "startup retains original callback pointer beyond subscriber ID"
            );
            let converted = ExchangeError::from(mutation.clone());
            let mut expected_ids = if recovered { vec![41, 73] } else { vec![] };
            expected_ids.push(notification.subscriber_id());
            assert_eq!(
                converted.store_subscriber_ids, expected_ids,
                "shared conversion appends inner-to-outer IDs"
            );
            assert!(
                converted
                    .source()
                    .is_some_and(|retained| std::ptr::eq(retained, cause.as_ref())),
                "shared conversion retains original opaque callback cause"
            );
            if recovered {
                assert_eq!(
                    observed_error_code(&converted.code),
                    observed_error_code(&original.code),
                    "recovered callback metadata retained"
                );
                assert_eq!(converted.message, original.message);
                assert_eq!(
                    cause
                        .downcast_ref::<ExchangeError>()
                        .unwrap()
                        .store_subscriber_ids,
                    vec![41, 73],
                    "conversion leaves original callback IDs unchanged"
                );
            } else {
                assert_eq!(
                    notification
                        .cause()
                        .source()
                        .unwrap()
                        .downcast_ref::<std::io::Error>()
                        .unwrap()
                        .kind(),
                    std::io::ErrorKind::PermissionDenied,
                    "opaque callback type survives startup"
                );
                assert_eq!(
                    converted.response_json_view(),
                    Some(json!({"data":{"msg":"wrapper response"}}))
                );
            }
            unsubscribe();
        }
    }
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
