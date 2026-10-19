//! Exercise the source private class through the production realm/socket bridge.
use crate::binding_test_runtime as host;
use safe_cex_exchange::js_runtime::value::Js;

use crate::{
    binding::{HyperliquidDependencies, RuntimeClock},
    exchange::HyperliquidExchange,
    rate_limit::RateLimiter,
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{runtime::ExchangeSigners, socket_lifecycle::SocketSlot};
use serde_json::json;
use std::{rc::Rc, sync::Arc};

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn both_constructed_families_keep_original_js_arguments_and_start_async_calls_eagerly() {
    use safe_cex_exchange::{exchange::Exchange, js_runtime::promise::Promise};
    use std::cell::{Cell, RefCell};
    for spot in [false, true] {
        let runtime = Rc::new(host::Host::default());
        let dependencies = HyperliquidDependencies {
            runtime: runtime.clone(),
            http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(runtime.clone())),
                1200.0,
                60_000.0,
            )),
            signers: ExchangeSigners::default(),
            external_public_data: None,
        };
        let options = serde_json::from_value(json!({"key":"", "secret":""})).unwrap();
        let store = Rc::new(DefaultStore::new());
        let (exchange, binding): (Rc<dyn Exchange>, _) = if spot {
            let exchange =
                crate::spot_binding::HyperliquidSpotExchange::new(options, store, dependencies)
                    .unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            (exchange, binding)
        } else {
            let exchange = HyperliquidExchange::new(options, store, dependencies).unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            (exchange, binding)
        };
        let mut context = binding.realm.clone();
        let captured = Rc::new(RefCell::new(Vec::new()));
        let observed = captured.clone();
        let _cleanup = exchange.base().on_args(
            "identity",
            Rc::new(move |_, args| {
                *observed.borrow_mut() = args.to_vec();
                Ok(())
            }),
        );
        let array = Js::array(vec![Js::Number(7.0)]);
        let nested = Js::object(vec![("array", array.clone())]);
        let expected_receiver = nested.clone();
        let callback = Js::function("callback", move |receiver, _| {
            assert!(receiver.same_reference(&expected_receiver));
            Ok(receiver)
        });
        let cause = Js::object(vec![("cause", Js::utf16(&[0xd800]))]);
        let iterable = Js::array(vec![nested.clone()]);
        let iterator = iterable.iterator().unwrap();
        let args = vec![
            nested.clone(),
            array.clone(),
            callback.clone(),
            cause.clone(),
            iterator.clone(),
        ];
        let emitter = binding.instance.get("emitter").unwrap();
        emitter
            .call_method(
                "emit",
                std::iter::once(Js::str("identity"))
                    .chain(args.clone())
                    .collect(),
            )
            .unwrap();
        assert_eq!(captured.borrow().len(), args.len());
        for (actual, original) in captured.borrow().iter().zip(&args) {
            assert!(actual.same_reference(original));
        }
        array.set("0", Js::Number(8.0)).unwrap();
        assert_eq!(
            captured.borrow()[0]
                .get("array")
                .unwrap()
                .get("0")
                .unwrap()
                .as_f64(),
            Some(8.0)
        );
        assert!(
            captured.borrow()[2]
                .call(nested.clone(), vec![])
                .unwrap()
                .same_reference(&nested)
        );
        assert!(
            captured.borrow()[4]
                .call_method("next", vec![])
                .unwrap()
                .get("value")
                .unwrap()
                .same_reference(&nested)
        );
        let returned_cause = cause.clone();
        let throwing = context.native("throwing", 0, move |_, _, _| {
            Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                returned_cause.clone(),
            ))
        });
        assert!(
            context
                .call(&throwing, &nested, &[])
                .unwrap_err()
                .value
                .same_reference(&cause)
        );
        let prefix = Rc::new(Cell::new(0));
        let suffix = Rc::new(Cell::new(0));
        let before = prefix.clone();
        let after = suffix.clone();
        let receiver = binding.instance.clone();
        binding
            .instance
            .define(
                "setLeverage",
                context.native("setLeverage", 2, move |context, this, args| {
                    assert!(this.same_reference(&receiver));
                    assert_eq!(args[0].to_display(), "BTC");
                    before.set(before.get() + 1);
                    let after = after.clone();
                    let sleep = context.runtime().sleep(5);
                    Ok(Js::Promise(Promise::spawn(
                        &context.runtime(),
                        Box::pin(async move {
                            sleep
                                .await
                                .map_err(safe_cex_exchange::js_runtime::value::Thrown::from)?;
                            after.set(after.get() + 1);
                            Ok(Js::Undefined)
                        }),
                    )))
                }),
            )
            .unwrap();
        let waiter = exchange.set_leverage("BTC".into(), 3.0, None);
        assert_eq!(
            prefix.get(),
            1,
            "public async prefix must run before the waiter is polled: spot={spot}"
        );
        assert_eq!(suffix.get(), 0);
        drop(waiter);
        runtime.advance(5.0);
        assert_eq!(
            suffix.get(),
            1,
            "dropping the waiter does not cancel source async work: spot={spot}"
        );
        exchange.dispose().unwrap();
    }
}

#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn private_no_super_dispose_closes_wire_without_force_closing_base() {
    let runtime = Rc::new(host::Host::default());
    let options = serde_json::from_value(json!({
        "key": "0x14791697260e4c9a71f18484c9f997b308e59325",
        "secret": "",
        "applicationId": "0x14791697260e4c9a71f18484c9f997b308e59325"
    }))
    .unwrap();
    let exchange = HyperliquidExchange::new(
        options,
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: runtime.clone(),
            http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(
                Arc::new(RuntimeClock::new(runtime.clone())),
                1200.0,
                60000.0,
            )),
            signers: ExchangeSigners::default(),
            external_public_data: None,
        },
    )
    .unwrap();
    let (mut realm, instance) = {
        let binding = exchange.binding.borrow();
        let binding = binding.as_ref().unwrap();
        (binding.realm.clone(), binding.instance.clone())
    };
    let private = realm.get_str(&instance, "privateWebsocket").unwrap();
    realm
        .call_method(&private, "connectAndSubscribe", &[Js::Bool(false)])
        .unwrap();
    runtime.settle();
    let base = exchange.base.get_socket(SocketSlot::Private).unwrap();
    let wire = runtime.sockets.borrow()[0].clone();
    assert_eq!(wire.closes.get(), 0);
    assert!(!base.is_force_closed());
    realm.call_method(&private, "dispose", &[]).unwrap();
    assert_eq!(
        wire.closes.get(),
        1,
        "source ws.close closes the current native wire"
    );
    assert!(
        !base.is_force_closed(),
        "source private dispose has no super call and must not set forceClosed"
    );
    assert!(
        !exchange.base.is_disposed(),
        "private no-super dispose leaves its parent active"
    );

    // The actual source class is loaded independently; changing its ws.close
    // to forceClose must fail that program's behavioral assertions.
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/private-close-source.cjs"
    );
    let source = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source["forceClosed"], false);
    assert_eq!(source["isDisposed"], false);
    assert_eq!(source["closes"], wire.closes.get());
    let control = std::process::Command::new("node")
        .arg(script)
        .env("SAFE_CEX_HL_CLOSE_CONTROL", "1")
        .output()
        .unwrap();
    assert!(
        !control.status.success(),
        "controlled source must reject forceClose"
    );
    assert!(
        String::from_utf8_lossy(&control.stderr)
            .contains("actual private dispose does not call forceClose")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[allow(clippy::arc_with_non_send_sync)]
fn constructed_startup_race_releases_canceled_timeout_reactions() {
    use safe_cex_exchange::{
        js_runtime::promise::{Promise, await_js},
        native,
    };
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        for replay in [false, true] {
            let runtime = if replay {
                native::native_replay_runtime(1_700_000_000_000.)
            } else {
                native::native_runtime()
            };
            let dependencies = HyperliquidDependencies {
                runtime: runtime.clone(),
                http: host::Routes::new(vec![]),
                limiter: Rc::new(RateLimiter::new(
                    Arc::new(RuntimeClock::new(runtime.clone())),
                    1200.,
                    60_000.,
                )),
                signers: ExchangeSigners::default(),
                external_public_data: None,
            };
            let exchange = HyperliquidExchange::new(
                serde_json::from_value(json!({"key":"", "secret":"", "publicDataOnly":true}))
                    .unwrap(),
                Rc::new(DefaultStore::new()),
                dependencies,
            )
            .unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            let runtime_dyn = binding.realm.runtime();
            let operation = Promise::pending(&runtime_dyn);
            let result = binding
                .instance
                .call_method(
                    "withStartupTimeout",
                    vec![
                        Js::Promise(operation.clone()),
                        Js::Number(60_000.),
                        Js::str("source startup"),
                        Js::Null,
                    ],
                )
                .unwrap();
            let original = Js::object(vec![("result", Js::Number(-0.))]);
            operation.settle(Ok(original.clone()));
            let value = tokio::time::timeout(
                std::time::Duration::from_secs(1),
                await_js(&runtime_dyn, result),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(value.same_reference(&original));
            for _ in 0..8 {
                native::replay_checkpoint(&runtime);
                tokio::task::yield_now().await;
            }
            assert_eq!(
                native::replay_pending_jobs(&runtime),
                0,
                "HL_STARTUP_CANCELED_TIMEOUT replay={replay}"
            );
            assert_eq!(
                native::replay_raw_tasks(&runtime)["blocking"],
                0,
                "no unknown producer survives startup"
            );
            safe_cex_store::virtual_clock::set_thread_host_time(None);
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
#[expect(
    clippy::arc_with_non_send_sync,
    reason = "limiter clocks are local runtime capabilities"
)]
async fn observe_constructed_ping_timer(clear: bool) {
    use safe_cex_exchange::{exchange::Exchange, native};
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ping-loop-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 2);
    for replay in [false, true] {
        for (index, member) in ["publicWebsocket", "privateWebsocket"]
            .into_iter()
            .enumerate()
        {
            assert_eq!(source[index]["timers"], 2);
            assert_eq!(source[index]["canceled"], true);
            assert_eq!(source[index]["fired"], 0);
            let runtime = native::native_replay_runtime(1_700_000_000_000.);
            if replay {
                safe_cex_exchange::replay_clock::activate(runtime.replay_clock.as_ref().unwrap())
                    .unwrap();
            }
            let exchange = HyperliquidExchange::new(
                serde_json::from_value(json!({"key":"", "secret":"", "publicDataOnly":true}))
                    .unwrap(),
                Rc::new(DefaultStore::new()),
                HyperliquidDependencies {
                    runtime: runtime.clone(),
                    http: host::Routes::new(vec![]),
                    limiter: Rc::new(RateLimiter::new(
                        Arc::new(RuntimeClock::new(runtime.clone())),
                        1200.,
                        60_000.,
                    )),
                    signers: ExchangeSigners::default(),
                    external_public_data: None,
                },
            )
            .unwrap();
            let binding = exchange.binding.borrow().as_ref().unwrap().clone();
            let mut realm = binding.realm.clone();
            let socket = realm.get_str(&binding.instance, member).unwrap();
            realm.call_method(&socket, "startPingLoop", &[]).unwrap();
            for _ in 0..8 {
                native::replay_checkpoint(&runtime);
                tokio::task::yield_now().await;
            }
            let before = native::replay_raw_tasks(&runtime);
            assert!(
                before["unknown"].as_u64().unwrap()
                    + before["promiseDrivers"].as_u64().unwrap()
                    + before["waitingTimers"].as_u64().unwrap()
                    > 0,
                "actual ping timer must own runtime work: {before}"
            );
            if clear {
                realm.call_method(&socket, "stopPingLoop", &[]).unwrap();
                assert!(matches!(
                    realm.get_str(&socket, "hlPingTimeoutId").unwrap(),
                    Js::Undefined
                ));
            }
            for _ in 0..8 {
                native::replay_checkpoint(&runtime);
                tokio::task::yield_now().await;
            }
            let after = native::replay_raw_tasks(&runtime);
            assert_eq!(
                after["blocking"], 0,
                "HL_PING_TIMER_OWNERSHIP clear={clear} replay={replay} member={member}: {after}"
            );
            assert_eq!(
                native::replay_pending_jobs(&runtime),
                0,
                "scheduled pings are not finite Promise work"
            );
            if clear {
                assert_eq!(
                    after["waitingTimers"], 0,
                    "HL_PING_CLEAR_TIMEOUT actual sleep canceled"
                );
                assert_eq!(
                    after["promiseDrivers"], 0,
                    "HL_PING_CLEAR_TIMEOUT timer owner released"
                );
            } else {
                realm.call_method(&socket, "stopPingLoop", &[]).unwrap();
            }
            exchange.dispose().unwrap();
            safe_cex_store::virtual_clock::set_thread_host_time(None);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn constructed_ping_loop_is_owned_and_classified() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, observe_constructed_ping_timer(false));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn constructed_ping_loop_clear_timeout_releases_timer_work() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, observe_constructed_ping_timer(true));
}
