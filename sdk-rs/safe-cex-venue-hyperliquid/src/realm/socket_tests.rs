//! Exercise the source private class through the production realm/socket bridge.
use super::Value;
use crate::binding_test_runtime as host;
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
        .call_method(&private, "connectAndSubscribe", &[Value::Bool(false)])
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
                    Value::Undefined
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
