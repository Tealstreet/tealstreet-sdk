//! Scheduling observations through the actual public native exchange and Store.
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::PositionMode;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn drive<T>(runtime: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..64 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.pump();
    }
    panic!("bounded native mode operation did not settle")
}

#[test]
fn mode_change_logs_run_in_the_call_prefix_before_the_caller_can_poll() {
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/divergence_timing_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["prefix"].as_array().unwrap().len(), 3);
    assert_eq!(source["settled"].as_array().unwrap().len(), 4);
    let (venue, runtime, store, wire) = support::fixture();
    Store::update(store.as_ref(), StoreUpdate {
        fetched_leverage: Some(serde_json::from_value(json!({"BTCUSDT":{"buyLeverage":3,"sellLeverage":5,"marginType":"cross","positionMode":"oneway"}})).unwrap()),
        ..Default::default()
    }, false).unwrap();
    wire.bodies.borrow_mut().insert(
        "/api/v1/account/set-position-mode".into(),
        json!({"code":"0","msg":"success","data":{"positionMode":"long_short_mode"}}),
    );
    let logs = Rc::new(RefCell::new(Vec::<Value>::new()));
    let seen = logs.clone();
    venue.base().on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(json!(
                args.iter()
                    .map(|arg| String::from_utf16(&arg.to_js_string().unwrap()).unwrap())
                    .collect::<Vec<_>>()
            ));
            Ok(())
        }),
    );
    let pending = venue.change_position_mode(true, None);
    assert_eq!(
        json!(&*logs.borrow()),
        source["prefix"],
        "MODE-PREFIX actual public call must emit the three maintained diagnostics before returning its future"
    );
    drive(&runtime, pending).unwrap();
    assert_eq!(json!(&*logs.borrow()), source["settled"]);
    assert_eq!(
        store.fetched_leverage()["BTCUSDT"].position_mode,
        Some(PositionMode::Hedged)
    );
    assert_eq!(wire.requests.borrow().len(), 1);
    venue.dispose().unwrap();
}
