#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    closed_positions::{ClosedPositionSource, resolve_closed_positions},
    native::native_runtime,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::HistoricalTradeOptions;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Waker},
};

fn source_profiles() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/closed_position_timing_source.cjs"
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

#[test]
fn real_resolver_invokes_selected_fetch_before_waiter_poll() {
    let profiles = source_profiles();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        for name in ["venue", "derived", "independent", "rejected", "unavailable"] {
            let trace = Rc::new(RefCell::new(Vec::new()));
            let runtime = native_runtime();
            let mut source = ClosedPositionSource {
                runtime,
                fetch_closed_pnl: {
                    let trace = trace.clone();
                    Rc::new(move |options| {
                        assert_eq!(options.symbol, "BTC");
                        trace.borrow_mut().push("venue");
                        trace.borrow_mut().push("done");
                        Box::pin(async move {
                            if name == "rejected" {
                                Err(ExchangeError::new("fetch rejected"))
                            } else {
                                Ok(vec![])
                            }
                        })
                    })
                },
                derive_closed_positions: Some({
                    let trace = trace.clone();
                    Rc::new(move |_| {
                        trace.borrow_mut().extend(["derived", "done"]);
                        Box::pin(async { Ok(vec![]) })
                    })
                }),
                closed_positions_share_ledger_source: (name == "independent").then_some(false),
            };
            if name == "unavailable" {
                source.derive_closed_positions = None;
            }
            let options: HistoricalTradeOptions =
                serde_json::from_value(json!({"from":11,"to":22,"symbol":"BTC","limit":3}))
                    .unwrap();
            let mut waiter = resolve_closed_positions(
                &source,
                options,
                matches!(name, "derived" | "independent" | "unavailable"),
            );
            assert_eq!(
                json!(*trace.borrow()),
                profiles[name]["before"],
                "CLOSED_EAGER {name}"
            );
            assert!(
                waiter
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending(),
                "source Promise await fence: {name}"
            );
            let result = waiter.await;
            assert_eq!(json!(*trace.borrow()), profiles[name]["after"]);
            if name == "rejected" {
                assert_eq!(
                    result.unwrap_err().message,
                    profiles[name]["error"].as_str().unwrap()
                );
            } else {
                let result = result.unwrap();
                assert_eq!(json!(result.entries), profiles[name]["result"]["entries"]);
                assert_eq!(
                    json!(result.provenance),
                    profiles[name]["result"]["provenance"]
                );
                assert_eq!(
                    result.independently_verifiable,
                    profiles[name]["result"]["independentlyVerifiable"]
                        .as_bool()
                        .unwrap()
                );
                assert_eq!(
                    result.unavailable_reason.as_deref(),
                    profiles[name]["result"]["unavailableReason"].as_str()
                );
            }
        }
    });
}

#[test]
fn real_resolver_retains_held_fetch_after_waiter_and_source_drop() {
    let profiles = source_profiles();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let source = ClosedPositionSource {
            runtime: native_runtime(),
            fetch_closed_pnl: {
                let trace = trace.clone();
                Rc::new(move |_| -> ExchangeFuture<'static, _> {
                    trace.borrow_mut().push("venue");
                    let trace = trace.clone();
                    Box::pin(async move {
                        tokio::task::yield_now().await;
                        trace.borrow_mut().push("done");
                        Ok(vec![])
                    })
                })
            },
            derive_closed_positions: None,
            closed_positions_share_ledger_source: None,
        };
        let options = serde_json::from_value(json!({"from":11,"to":22,"symbol":"BTC"})).unwrap();
        let waiter = resolve_closed_positions(&source, options, false);
        assert_eq!(
            json!(*trace.borrow()),
            profiles["held"]["before"],
            "CLOSED_DETACHED eager prefix"
        );
        drop(waiter);
        drop(source);
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            json!(*trace.borrow()),
            profiles["held"]["after"],
            "CLOSED_DETACHED continuation survives caller drop"
        );
    });
}
