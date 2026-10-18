#[path = "support/runtime.rs"]
mod fixture_runtime;
use safe_cex_exchange::{
    closed_positions::*,
    loop_util::*,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{ClosedPnlEntry, DataProvenance, HistoricalTradeOptions};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture"),
    }
}
fn loop_ready(
    runtime: &fixture_runtime::Runtime,
    future: ExchangeFuture<'static, ()>,
) -> Result<(), ExchangeError> {
    runtime.advance(0);
    let mut future = future;
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("loop promise settled after runtime checkpoint"),
    }
}
#[test]
fn native_loop_warning_dedupe_sleep_then_visibility_and_defer() {
    let trace = Rc::new(RefCell::new(vec![]));
    let queued = Rc::new(RefCell::new(Vec::<Rc<dyn Fn()>>::new()));
    let visible = Rc::new(Cell::new(false));
    let runtime = Rc::new(fixture_runtime::Runtime::default());
    let host = LoopHost {
        runtime: runtime.clone(),
        sleep: {
            let trace = trace.clone();
            let visible = visible.clone();
            Rc::new(move |delay| {
                trace.borrow_mut().push(format!("sleep:{delay}"));
                visible.set(true);
                Box::pin(async { Ok(()) })
            })
        },
        document_visible: {
            let trace = trace.clone();
            let visible = visible.clone();
            Rc::new(move || {
                trace.borrow_mut().push("visibility".into());
                visible.get()
            })
        },
        request_animation_frame: {
            let trace = trace.clone();
            let queued = queued.clone();
            Rc::new(move |callback| {
                trace.borrow_mut().push("raf".into());
                queued.borrow_mut().push(callback);
                Ok(())
            })
        },
        set_timeout_zero: {
            let trace = trace.clone();
            let queued = queued.clone();
            Rc::new(move |callback| {
                trace.borrow_mut().push("timeout:0".into());
                queued.borrow_mut().push(callback);
                Ok(())
            })
        },
        warn: {
            let trace = trace.clone();
            Rc::new(move |warning| trace.borrow_mut().push(format!("warn:{warning}")))
        },
    };
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let callback: Rc<dyn Fn()> = Rc::new(move || count.set(count.get() + 1));
    let mut source = vec![u16::from(b'x'); 200];
    source.push(1);
    loop_ready(&runtime, loop_once(callback.clone(), &source, None, &host)).unwrap();
    assert_eq!(calls.get(), 0, "native deferred callback");
    assert!(trace.borrow()[0].starts_with("warn:[safe-cex] loop() called without a minInterval"));
    assert_eq!(&trace.borrow()[1..], &["sleep:5000", "visibility", "raf"]);
    assert!(Rc::ptr_eq(&queued.borrow()[0], &callback));
    queued.borrow_mut().remove(0)();
    assert_eq!(calls.get(), 1);
    source[200] = 2;
    trace.borrow_mut().clear();
    loop_ready(
        &runtime,
        loop_once(callback.clone(), &source, Some(f64::NAN), &host),
    )
    .unwrap();
    assert_eq!(*trace.borrow(), vec!["sleep:5000", "visibility", "raf"]);
    let mut hidden = host.clone();
    hidden.document_visible = Rc::new(|| false);
    trace.borrow_mut().clear();
    loop_ready(
        &runtime,
        loop_once(callback.clone(), &source, Some(12.5), &hidden),
    )
    .unwrap();
    assert_eq!(*trace.borrow(), vec!["sleep:12.5", "timeout:0"]);
    trace.borrow_mut().clear();
    loop_ready(
        &runtime,
        loop_once(callback.clone(), &source, Some(f64::INFINITY), &hidden),
    )
    .unwrap();
    assert!(trace.borrow().is_empty());
    let mut rejected = host;
    rejected.sleep = Rc::new(|_| Box::pin(async { Err("sleep rejected".into()) }));
    assert_eq!(
        loop_ready(&runtime, loop_once(callback, &source, Some(1.0), &rejected))
            .unwrap_err()
            .message,
        "sleep rejected"
    );
}
fn entry(provenance: &str) -> ClosedPnlEntry {
    serde_json::from_value(serde_json::json!({"provenance":provenance,"symbol":"BTC","notional":1.5,"entryPrice":2.5,"exitPrice":3.5,"unrealizedPnl":4.5,"side":"buy","fees":5.5,"time":6.5,"closedTime":7.5,"account":"a","contracts":8.5,"leverage":9.5})).unwrap()
}
fn opts() -> HistoricalTradeOptions {
    serde_json::from_value(serde_json::json!({"from":11,"to":22,"symbol":"BTC","limit":3})).unwrap()
}
fn closed_ready<T>(
    runtime: &fixture_runtime::Runtime,
    mut future: ExchangeFuture<'static, T>,
) -> Result<T, ExchangeError> {
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..8 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
            return result;
        }
        runtime.advance(0);
    }
    panic!("closed position promise did not settle");
}
#[test]
fn native_closed_positions_provenance_independence_and_one_fetch() {
    let calls = Rc::new(RefCell::new(vec![]));
    let runtime = Rc::new(fixture_runtime::Runtime::default());
    let mut source = ClosedPositionSource {
        runtime: runtime.clone(),
        fetch_closed_pnl: {
            let calls = calls.clone();
            Rc::new(move |options| {
                calls.borrow_mut().push(("venue", options));
                Box::pin(async { Ok(vec![entry("derived")]) })
            })
        },
        derive_closed_positions: Some({
            let calls = calls.clone();
            Rc::new(move |options| {
                calls.borrow_mut().push(("derived", options));
                Box::pin(async { Ok(vec![entry("venue")]) })
            })
        }),
        closed_positions_share_ledger_source: None,
    };
    let venue = closed_ready(&runtime, resolve_closed_positions(&source, opts(), false)).unwrap();
    assert_eq!(venue.provenance, DataProvenance::Venue);
    assert!(venue.independently_verifiable);
    assert_eq!(
        venue.entries[0].provenance,
        DataProvenance::Derived,
        "venue preserves entry provenance"
    );
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(calls.borrow()[0].0, "venue");
    assert_eq!(calls.borrow()[0].1, opts());
    let derived = closed_ready(&runtime, resolve_closed_positions(&source, opts(), true)).unwrap();
    assert_eq!(derived.provenance, DataProvenance::Derived);
    assert!(!derived.independently_verifiable);
    assert_eq!(derived.entries[0].provenance, DataProvenance::Derived);
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(calls.borrow()[1].0, "derived");
    source.closed_positions_share_ledger_source = Some(false);
    assert!(
        closed_ready(&runtime, resolve_closed_positions(&source, opts(), true))
            .unwrap()
            .independently_verifiable
    );
    source.derive_closed_positions = None;
    let missing = closed_ready(&runtime, resolve_closed_positions(&source, opts(), true)).unwrap();
    assert!(missing.entries.is_empty());
    assert!(!missing.independently_verifiable);
    assert_eq!(
        missing.unavailable_reason.as_deref(),
        Some("venue reports no closed positions and no derivation is implemented")
    );
    assert_eq!(calls.borrow().len(), 3);
    source.fetch_closed_pnl = Rc::new(|_| -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        Box::pin(async { Err(ExchangeError::new("venue rejected")) })
    });
    assert_eq!(
        closed_ready(&runtime, resolve_closed_positions(&source, opts(), false))
            .unwrap_err()
            .message,
        "venue rejected"
    );
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
mod loop_timing {
    use super::*;
    use safe_cex_exchange::native::native_runtime;
    use serde_json::{Value, json};

    fn source() -> Value {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/loop_timing_source.cjs"
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

    fn host(
        runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
        trace: &Rc<RefCell<Vec<String>>>,
        queued: &Rc<RefCell<Vec<Rc<dyn Fn()>>>>,
    ) -> LoopHost {
        let sleeping = trace.clone();
        let scheduling = trace.clone();
        let callbacks = queued.clone();
        let warning = trace.clone();
        LoopHost {
            runtime,
            sleep: Rc::new(move |delay| {
                sleeping.borrow_mut().push(format!("sleep:{delay}"));
                Box::pin(async {
                    tokio::task::yield_now().await;
                    Ok(())
                })
            }),
            document_visible: Rc::new(|| false),
            request_animation_frame: Rc::new(|_| panic!("hidden document")),
            set_timeout_zero: Rc::new(move |callback| {
                scheduling.borrow_mut().push("timeout:0".into());
                callbacks.borrow_mut().push(callback);
                Ok(())
            }),
            warn: Rc::new(move |_| warning.borrow_mut().push("warn".into())),
        }
    }

    #[test]
    fn real_loop_starts_sleep_before_poll_and_retains_dropped_continuation() {
        let expected = source();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let runtime = native_runtime();
            let trace = Rc::new(RefCell::new(Vec::new()));
            let queued = Rc::new(RefCell::new(Vec::new()));
            let host = host(runtime, &trace, &queued);
            let called = trace.clone();
            let callback: Rc<dyn Fn()> =
                Rc::new(move || called.borrow_mut().push("callback".into()));
            let waiter = loop_once(callback, &[], Some(100.0), &host);
            assert_eq!(
                json!(*trace.borrow()),
                expected["pending"]["before"],
                "LOOP_EAGER sleep starts at invocation"
            );
            drop(waiter);
            for _ in 0..16 {
                tokio::task::yield_now().await;
            }
            assert_eq!(
                json!(*trace.borrow()),
                expected["pending"]["after"],
                "LOOP_EAGER dropped waiter retains scheduler continuation"
            );
            assert_eq!(queued.borrow().len(), 1);
            queued.borrow_mut().remove(0)();
            assert_eq!(json!(*trace.borrow()), expected["pending"]["complete"]);
        });
    }

    #[test]
    fn real_loop_infinity_suppresses_all_host_effects() {
        let expected = source();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            let runtime = native_runtime();
            let trace = Rc::new(RefCell::new(Vec::new()));
            let queued = Rc::new(RefCell::new(Vec::new()));
            let host = host(runtime, &trace, &queued);
            loop_once(
                Rc::new(|| panic!("disabled loop")),
                &[],
                Some(f64::INFINITY),
                &host,
            )
            .await
            .unwrap();
            assert_eq!(
                json!(*trace.borrow()),
                expected["disabled"]["complete"],
                "LOOP_DISABLED infinity never sleeps or schedules"
            );
            assert!(queued.borrow().is_empty());
        });
    }
    #[test]
    fn real_loop_ready_sleep_keeps_await_order_and_rejection_skips_scheduling() {
        let expected = source();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async {
            for profile in ["ready", "visible", "fallback", "rejected"] {
                let runtime = native_runtime();
                let trace = Rc::new(RefCell::new(Vec::new()));
                let queued = Rc::new(RefCell::new(Vec::new()));
                let mut host = host(runtime, &trace, &queued);
                let sleeping = trace.clone();
                let rejected = profile == "rejected";
                host.sleep = Rc::new(move |delay| {
                    sleeping.borrow_mut().push(format!("sleep:{delay}"));
                    Box::pin(async move {
                        if rejected {
                            Err("sleep rejected".into())
                        } else {
                            Ok(())
                        }
                    })
                });
                if profile == "visible" {
                    let seeing = trace.clone();
                    host.document_visible = Rc::new(move || {
                        seeing.borrow_mut().push("visibility".into());
                        true
                    });
                    let scheduling = trace.clone();
                    let callbacks = queued.clone();
                    host.request_animation_frame = Rc::new(move |callback| {
                        scheduling.borrow_mut().push("raf".into());
                        callbacks.borrow_mut().push(callback);
                        Ok(())
                    });
                }
                let called = trace.clone();
                let interval = match profile {
                    "visible" => Some(12.5),
                    "fallback" => None,
                    _ => Some(100.0),
                };
                let source = "loop timing unique fallback"
                    .encode_utf16()
                    .collect::<Vec<_>>();
                let waiter = loop_once(
                    Rc::new(move || called.borrow_mut().push("callback".into())),
                    &source,
                    interval,
                    &host,
                );
                assert_eq!(
                    json!(*trace.borrow()),
                    expected[profile]["before"],
                    "LOOP_AWAIT no synchronous continuation for {profile}"
                );
                let result = waiter.await;
                assert_eq!(
                    json!(result.err().map(|error| error.message)),
                    expected[profile]["error"]
                );
                assert_eq!(json!(*trace.borrow()), expected[profile]["after"]);
                assert_eq!(queued.borrow().len(), usize::from(!rejected));
                for callback in queued.borrow_mut().drain(..) {
                    callback();
                }
                assert_eq!(json!(*trace.borrow()), expected[profile]["complete"]);
            }
        });
    }
}
