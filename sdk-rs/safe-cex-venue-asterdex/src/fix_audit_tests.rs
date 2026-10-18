use super::*;
use safe_cex_exchange::{
    js_runtime::{client::JsHttpHost, promise::JsFuture, value::from_json},
    native::{NativeRuntime, native_replay_runtime},
    native_store::DefaultStore,
    replay_clock,
};
use safe_cex_store::{Store, StoreUpdate};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    task::{Context, Poll, Waker},
};

#[derive(Default)]
struct Gate {
    open: Cell<bool>,
    waker: RefCell<Option<Waker>>,
}
impl Gate {
    fn release(&self) {
        self.open.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() {
            waker.wake();
        }
    }
}
struct Http {
    runtime: Rc<NativeRuntime>,
    gates: RefCell<BTreeMap<String, Rc<Gate>>>,
    requests: RefCell<Vec<String>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let url = config.get("url").unwrap().to_display();
        self.requests.borrow_mut().push(url.clone());
        let gate = self
            .gates
            .borrow()
            .iter()
            .find(|(path, _)| url.contains(path.as_str()))
            .map(|(_, gate)| gate.clone());
        let data = if url.contains("/time") {
            json!({"serverTime":1_800_000_001_000_u64})
        } else if url.contains("/openInterest") {
            json!({"symbol":"BTCUSDT","openInterest":"7"})
        } else if url.contains("/listenKey") {
            json!({"listenKey":"fixture-listen-key"})
        } else if url.contains("/account") {
            json!({"totalWalletBalance":"100","availableBalance":"70","totalUnrealizedProfit":"5","totalInitialMargin":"12","totalOpenOrderInitialMargin":"3"})
        } else {
            json!([])
        };
        Box::pin(async move {
            if let Some(gate) = gate {
                std::future::poll_fn(move |cx| {
                    if gate.open.get() {
                        Poll::Ready(())
                    } else {
                        *gate.waker.borrow_mut() = Some(cx.waker().clone());
                        Poll::Pending
                    }
                })
                .await;
            }
            Ok(Js::object(vec![
                ("config", config),
                ("data", from_json(&data)),
                ("status", Js::Number(200.0)),
                ("statusText", Js::str("OK")),
                ("headers", Js::object(vec![])),
            ]))
        })
    }
}
struct Fixture {
    venue: Rc<AsterDEXExchange>,
    runtime: Rc<NativeRuntime>,
    store: Rc<DefaultStore>,
    http: Rc<Http>,
}
impl Fixture {
    fn new() -> Self {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        replay_clock::activate(runtime.replay_clock.as_ref().unwrap()).unwrap();
        let http = Rc::new(Http {
            runtime: runtime.clone(),
            gates: RefCell::default(),
            requests: RefCell::default(),
        });
        let host = http.clone();
        let store = Rc::new(DefaultStore::new());
        let venue = AsterDEXExchange::new(ExchangeName::Asterdex, serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret","tuning":{"consoleLogging":false}})).unwrap(), store.clone(), AsterDEXDependencies {
            runtime: runtime.clone(), http: Rc::new(move |_| Ok(host.clone())), external_public_data: None,
        }).unwrap();
        Self {
            venue,
            runtime,
            store,
            http,
        }
    }
    fn hold(&self, path: &str) -> Rc<Gate> {
        let gate = Rc::new(Gate::default());
        self.http
            .gates
            .borrow_mut()
            .insert(path.into(), gate.clone());
        gate
    }
    fn tickers(&self) {
        let ticker = serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":0,"ask":0,"last":1,"mark":0,"index":0,"percentage":0,"fundingRate":0,"fundingTime":0,"openInterest":0,"volume":0,"quoteVolume":0})).unwrap();
        self.store
            .update(
                StoreUpdate {
                    tickers: Some(vec![ticker]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
    }
    async fn advance(&self, elapsed: f64) {
        replay_clock::advance(self.runtime.replay_clock.as_ref().unwrap(), elapsed)
            .await
            .unwrap();
        turns().await;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
        safe_cex_store::virtual_clock::set_thread_host_time(None);
    }
}
async fn turns() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
}
fn run(test: impl Future<Output = ()>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, test);
}

#[test]
fn clock_settles_and_clears_its_slot_after_disposal() {
    run(async {
        let f = Fixture::new();
        let gate = f.hold("/time");
        let clock = f.venue.rt.shared.extra.clock.get().unwrap();
        let mut starting = Box::pin(clock.start(false, crate::types::AsterDEXAPIVersion::V1));
        let mut pending = Box::pin(clock.sync_client_server_time(false, None));
        let mut offset = Box::pin(clock.force_sync_and_get_offset(false, None));
        turns().await;
        assert!(clock.is_syncing());
        assert_eq!(
            f.http.requests.borrow().len(),
            1,
            "coalesced requests share the actual HTTP promise"
        );
        assert!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        f.venue.dispose().unwrap();
        gate.release();
        turns().await;
        assert!(
            matches!(
                pending
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ),
            "ASTER-CLOCK-SETTLEMENT in-flight source Promise must settle after dispose"
        );
        assert!(
            matches!(
                starting
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ),
            "start waiter must settle too"
        );
        assert!(
            matches!(
                offset
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Ready(Ok(0.0))
            ),
            "force-sync waiter must settle too"
        );
        assert!(!clock.is_syncing(), "finally must clear the shared slot");
        assert_eq!(
            clock.get_time_offset(),
            0.0,
            "stopped generation must not write clock state"
        );
        f.advance(600_000.0).await;
        assert_eq!(
            f.http.requests.borrow().len(),
            1,
            "late start must not install an interval"
        );
    });
}

#[test]
fn open_interest_teardown_cancels_late_effects_without_another_cycle() {
    run(async {
        let f = Fixture::new();
        f.tickers();
        let gate = f.hold("/openInterest");
        f.venue
            .rt
            .this
            .call_method("startOpenInterestPolling", vec![])
            .unwrap();
        turns().await;
        assert!(
            f.http
                .requests
                .borrow()
                .iter()
                .any(|url| url.contains("/openInterest"))
        );
        f.venue.dispose().unwrap();
        f.tickers();
        gate.release();
        turns().await;
        assert_eq!(
            f.store.tickers().unwrap()[0].open_interest,
            0.0,
            "accepted native teardown suppresses late Store effects"
        );
        let count = f.http.requests.borrow().len();
        f.advance(30_000.0).await;
        assert_eq!(f.http.requests.borrow().len(), count);
    });
}

#[test]
fn position_repair_waits_before_dispose_and_cleans_flight_on_teardown() {
    run(async {
        let f = Fixture::new();
        let gate = f.hold("/positionRisk");
        let repairs = &f.venue.rt.shared.extra.private.get().unwrap().repairs;
        repairs.run_position_repair();
        turns().await;
        assert!(
            repairs.state().position_in_flight,
            "repair finally must wait for the held response"
        );
        f.venue.dispose().unwrap();
        turns().await;
        assert!(!repairs.state().position_in_flight);
        gate.release();
        turns().await;
        assert!(!repairs.state().position_in_flight);
        assert!(f.store.positions().unwrap().is_empty());
        assert!(!f.store.loaded().unwrap().positions);
    });
}

#[test]
fn private_timers_use_fixed_delays_and_are_cleared_by_source_dispose() {
    run(async {
        let f = Fixture::new();
        let stream = f.venue.rt.shared.extra.private.get().unwrap();
        stream.listen_key.fetch_listen_key().await.unwrap();
        stream.repairs.schedule_balance_reconcile(0.0).unwrap();
        stream.repairs.schedule_order_reconcile().unwrap();
        let state = stream.listen_key.state();
        assert!(state.refresh_timer && state.watchdog_timer);
        assert!(stream.repairs.state().balance_timer && stream.repairs.state().order_timer);
        f.venue.dispose().unwrap();
        let state = stream.listen_key.state();
        assert!(!state.refresh_timer && !state.watchdog_timer);
        let state = stream.repairs.state();
        assert!(!state.balance_timer && !state.order_timer);
        let count = f.http.requests.borrow().len();
        f.advance(4_300_000.0).await;
        assert_eq!(f.http.requests.borrow().len(), count);
        assert!(f.store.orders().unwrap().is_empty() && f.store.positions().unwrap().is_empty());
    });
}

#[test]
fn signature_diagnostics_keep_error_class_without_secret_material() {
    run(async {
        let f = Fixture::new();
        let key = "0xDEAD_private_fixture_secret";
        let error = f
            .venue
            .rt
            .shared
            .globals
            .call_method(
                "generateAsterDEXCodeMessageSignature",
                vec![Js::str("x=1"), Js::str(key), Js::Bool(false)],
            )
            .unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        let message = error.value.get("message").unwrap().to_display();
        assert!(!message.contains(key) && !message.contains("private_fixture_secret"));
    });
}

#[test]
fn lifecycle_probes_execute_the_maintained_whole_typescript_classes() {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fix-audit-source.cjs"),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let value: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(
        value["clock"],
        json!({"settled":true,"slot":false,"offset":0})
    );
    assert_eq!(value["delays"], json!([1_800_000, 300_000, 1000, 10_000]));
    assert_eq!(
        value["signature"],
        json!({"name":"TypeError","sourceLeaksKey":true})
    );
}
