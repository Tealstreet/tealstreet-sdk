use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Position;
use safe_cex_venue_binance::pm_position_mode::change_binance_pm_position_mode;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_position_mode_matches_source_through_production_store() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/pm-position-mode.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 1280);
    for (index, case) in cases.iter().enumerate() {
        let store = DefaultStore::new();
        store
            .set_setting("isHedged", case["initial"].as_bool().unwrap())
            .unwrap();
        let positions = case["positions"].as_array().unwrap().iter().enumerate().map(|(id, value)| {
            let mut position: Position = serde_json::from_value(json!({"id":id.to_string(),"symbol":"BTCUSDT","side":"long","entryPrice":1,"notional":2,"leverage":3,"unrealizedPnl":4,"contracts":0,"liquidationPrice":6})).unwrap();
            position.contracts = value.as_f64().unwrap_or_else(|| safe_cex_num::number_from_string(value.as_str().unwrap()));
            position
        }).collect();
        Store::update(
            &store,
            StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let trace = Rc::new(RefCell::new(vec![]));
        let notified = trace.clone();
        let unsubscribe = Store::subscribe(
            &store,
            Box::new(move |data, _| {
                notified
                    .borrow_mut()
                    .push(json!(["notify", data.options.is_hedged]));
                Ok(())
            }),
        );
        let mut emitter_error = ExchangeError::new("emitter failed");
        emitter_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emitter-code"
        )));
        let outcome = {
            let mut future = std::pin::pin!(change_binance_pm_position_mode(
                &store,
                case["hedged"].as_bool().unwrap(),
                |url, body| {
                    trace
                        .borrow_mut()
                        .push(json!(["post", url, body["dualSidePosition"]]));
                    let mut error = ExchangeError::new("request failed");
                    error.response = Some(json!({"data":{"msg":case["msg"]}}));
                    std::future::ready(if case["reject"] == true {
                        Err(error)
                    } else {
                        Ok(())
                    })
                },
                |message| {
                    trace.borrow_mut().push(json!(["emit", message]));
                    if case["emitterThrows"] == true {
                        Err(emitter_error.clone())
                    } else {
                        Ok(())
                    }
                }
            ));
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("ready POST must settle"),
            }
        };
        let error = outcome.err().map(|error| json!({"name":error.name,"message":error.message,"code":observed_error_code(&error.code),"same":error==emitter_error}));
        assert_eq!(
            json!({"trace":*trace.borrow(),"error":error,"isHedged":store.options().is_hedged}),
            case["expected"],
            "PM position mode source effects case {index}"
        );
        unsubscribe();
    }
}

#[test]
fn pending_position_mode_post_does_not_commit_and_is_dropped_with_caller() {
    struct PendingPost(Rc<Cell<bool>>);
    impl Future for PendingPost {
        type Output = Result<(), ExchangeError>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for PendingPost {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let store = DefaultStore::new();
    let calls = Cell::new(0);
    let dropped = Rc::new(Cell::new(false));
    let mut future = Box::pin(change_binance_pm_position_mode(
        &store,
        true,
        |_, _| {
            calls.set(calls.get() + 1);
            PendingPost(dropped.clone())
        },
        |_| panic!("pending POST must not emit"),
    ));
    for _ in 0..2 {
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(
            !store.options().is_hedged,
            "setting commits only after POST success"
        );
    }
    assert_eq!(calls.get(), 1, "one POST across repeated polls");
    drop(future);
    assert!(dropped.get(), "caller drop cancels owned POST future");
    assert!(!store.options().is_hedged);
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_notification_cutover_matches_source_catch_and_fanout() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-store.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let store = DefaultStore::new();
        store
            .set_setting("isHedged", case["initial"].as_bool().unwrap())
            .unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let first_trace = trace.clone();
        let throws = case["listenerThrows"] == true;
        let first = Store::subscribe(
            &store,
            Box::new(move |data, _| {
                first_trace
                    .borrow_mut()
                    .push(json!(["first", data.options.is_hedged]));
                if throws {
                    Err(safe_cex_types::StoreSubscriberError::new(
                        "subscriber failed",
                    ))
                } else {
                    Ok(())
                }
            }),
        );
        let second_trace = trace.clone();
        let second = Store::subscribe(
            &store,
            Box::new(move |data, _| {
                second_trace
                    .borrow_mut()
                    .push(json!(["second", data.options.is_hedged]));
                Ok(())
            }),
        );
        let mut sentinel = ExchangeError::new("emitter failed");
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emitter identity"
        )));
        let mut future = std::pin::pin!(change_binance_pm_position_mode(
            &store,
            true,
            |url, body| {
                trace
                    .borrow_mut()
                    .push(json!(["post", url, body["dualSidePosition"]]));
                std::future::ready(Ok(()))
            },
            |message| {
                trace.borrow_mut().push(json!(["emit", message]));
                if case["emitterThrows"] == true {
                    Err(sentinel.clone())
                } else {
                    Ok(())
                }
            }
        ));
        let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("ready notification mutation");
        };
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "PM notification catch commits and skips second subscriber"
        );
        assert_eq!(
            result.is_err(),
            case["rejected"] == true,
            "PM only emitter rejection escapes"
        );
        if let Err(error) = result {
            assert_eq!(error, sentinel, "PM emitter error identity");
        }
        assert!(
            store.options().is_hedged,
            "PM notification failure retains committed setting"
        );
        first();
        second();
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
