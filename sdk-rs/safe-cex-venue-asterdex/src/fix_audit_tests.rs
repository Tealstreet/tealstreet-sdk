use super::{AsterDEXDependencies, AsterDEXExchange};
use safe_cex_exchange::exchange::Exchange;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_exchange::{
    js_runtime::{client::JsHttpHost, promise::JsFuture, value::from_json},
    native::{NativeRuntime, native_replay_runtime},
    native_store::DefaultStore,
    replay_clock,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
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
    rejection: RefCell<Option<Js>>,
    depth: RefCell<Option<serde_json::Value>>,
}
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let url = config.get("url").unwrap().to_display();
        self.requests.borrow_mut().push(url.clone());
        if let Some(value) = self.rejection.borrow().clone() {
            return Box::pin(async move {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(value))
            });
        }
        let gate = self
            .gates
            .borrow()
            .iter()
            .find(|(path, _)| url.contains(path.as_str()))
            .map(|(_, gate)| gate.clone());
        let data = if url.contains("/depth") {
            assert_eq!(
                config
                    .get("params")
                    .unwrap()
                    .get("symbol")
                    .unwrap()
                    .to_display(),
                "BTCUSDT"
            );
            self.depth
                .borrow()
                .clone()
                .expect("scripted depth response")
        } else if url.contains("/time") {
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
            rejection: RefCell::default(),
            depth: RefCell::default(),
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

#[test]
fn factory_enhances_primitive_rejections_before_sync_position_mode_observes_them() {
    run(async {
        for original in [
            Js::str("bare rejection"),
            Js::Number(42.0),
            Js::Bool(false),
            Js::utf16(&[0xd800]),
        ] {
            let f = Fixture::new();
            *f.http.rejection.borrow_mut() = Some(original.clone());
            let pending = f
                .venue
                .rt
                .this
                .call_method("syncPositionMode", vec![])
                .unwrap();
            let error = safe_cex_exchange::js_runtime::promise::await_js(
                &f.venue.rt.exchange_runtime(),
                pending,
            )
            .await
            .unwrap_err();
            assert_eq!(
                error.value.get("message").unwrap().to_display(),
                "[asterdex] Request setup error: undefined",
                "ASTER-FACTORY-ENHANCER must run before the venue catch"
            );
            assert_eq!(error.value.get("name").unwrap().to_display(), "Error");
            assert!(matches!(&error.value, Js::Object(object) if object.class == "Error"));
            assert!(error.value.get("isAxiosError").unwrap().truthy());
            assert!(
                error
                    .value
                    .get("originalError")
                    .unwrap()
                    .strict_equals(&original)
            );
            assert!(
                error
                    .value
                    .get("enhancedMessage")
                    .unwrap()
                    .strict_equals(&error.value.get("message").unwrap())
            );
            let timestamp = error
                .value
                .get("context")
                .unwrap()
                .get("timestamp")
                .unwrap();
            assert_eq!(timestamp.to_display(), "2027-01-15T08:00:00.000Z");
            error
                .value
                .set("enhancedMessage", Js::str("changed enhancement"))
                .unwrap();
            assert_eq!(
                error.value.get("message").unwrap().to_display(),
                "changed enhancement",
                "source enhanced message remains a live getter"
            );
        }
    });
}

#[test]
fn factory_enhancement_matches_the_two_actual_source_venues() {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../safe-cex-exchange/tests/factory-error-source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let result: serde_json::Value = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(result, json!({"asterdex":4,"woo":4}));
}

#[test]
fn live_poll_and_reconcile_deadlines_match_source_before_disposal() {
    run(async {
        let f = Fixture::new();
        f.tickers();
        f.venue
            .rt
            .this
            .call_method("startOpenInterestPolling", vec![])
            .unwrap();
        turns().await;
        assert_eq!(f.store.tickers().unwrap()[0].open_interest, 7.0);
        let stream = f.venue.rt.shared.extra.private.get().unwrap();
        stream.repairs.schedule_balance_reconcile(0.0).unwrap();
        stream.repairs.schedule_order_reconcile().unwrap();
        f.advance(999.0).await;
        assert!(
            !f.http
                .requests
                .borrow()
                .iter()
                .any(|url| url.contains("/account"))
        );
        f.advance(1000.0).await;
        assert_eq!(
            f.http
                .requests
                .borrow()
                .iter()
                .filter(|url| url.contains("/account"))
                .count(),
            1
        );
        f.advance(5000.0).await;
        stream.repairs.schedule_order_reconcile().unwrap();
        f.advance(10_000.0).await;
        assert!(
            !f.http
                .requests
                .borrow()
                .iter()
                .any(|url| url.contains("/openOrders"))
        );
        f.advance(14_999.0).await;
        assert_eq!(
            f.http
                .requests
                .borrow()
                .iter()
                .filter(|url| url.contains("/openInterest"))
                .count(),
            1
        );
        f.advance(15_000.0).await;
        assert_eq!(
            f.http
                .requests
                .borrow()
                .iter()
                .filter(|url| url.contains("/openInterest"))
                .count(),
            2,
            "source poll repeats after 15s"
        );
        assert_eq!(
            f.http
                .requests
                .borrow()
                .iter()
                .filter(|url| url.contains("/openOrders"))
                .count(),
            1,
            "replacement debounce fires at 15s, never the replaced 10s deadline"
        );
    });
}

#[test]
fn browser_approval_export_has_no_constructed_native_entrypoint() {
    run(async {
        let f = Fixture::new();
        assert!(
            f.venue
                .rt
                .this
                .get("checkAsterBuilderApproved")
                .unwrap()
                .is_undefined()
        );
        assert!(
            f.venue
                .rt
                .shared
                .globals
                .get("checkAsterBuilderApproved")
                .unwrap()
                .is_undefined()
        );
        assert!(
            f.http.requests.borrow().is_empty(),
            "missing browser ApprovalHost remains an explicit exported-host residual"
        );
    });
}

// Keep fixture teardown under an explicit test-only module for raw-file audits.
#[cfg(test)]
mod fixture_cleanup {
    use super::Fixture;
    use safe_cex_exchange::exchange::Exchange;

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.venue.dispose().unwrap();
            safe_cex_store::virtual_clock::set_thread_host_time(None);
        }
    }
}

fn book_sort_source(side: &str) -> Vec<serde_json::Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/book-sort-source.cjs"
        ))
        .arg(side)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn sort_number_bits(number: f64) -> String {
    if number.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", number.to_bits())
    }
}

fn rest_book_sort_matches_source(side: &str) {
    let cases = book_sort_source(side);
    assert_eq!(cases.len(), 10, "actual source book-sort profile floor");
    run(async {
        let f = Fixture::new();
        for case in cases {
            *f.http.depth.borrow_mut() = Some(case["depth"].clone());
            let book: safe_cex_types::OrderBook = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                f.venue.invoke(
                    "fetchOrderBook",
                    vec![Js::str("BTCUSDT"), Js::Number(1000.0)],
                ),
            )
            .await
            .expect("ASTER-REST-SORT must settle without a comparator panic")
            .unwrap();
            let levels = if side == "bids" {
                &book.bids
            } else {
                &book.asks
            };
            let actual = levels
                .iter()
                .map(|level| {
                    json!({
                        "price":sort_number_bits(level.price),
                        "amount":sort_number_bits(level.amount),
                        "total":sort_number_bits(level.total)
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(
                json!(actual),
                case[side],
                "ASTER-REST-SORT {side} profile {} retains source order and cumulative totals",
                case["name"]
            );
        }
        assert_eq!(
            f.http
                .requests
                .borrow()
                .iter()
                .filter(|url| url.ends_with("/fapi/v1/depth"))
                .count(),
            10
        );
        f.venue.dispose().unwrap();
    });
}

#[test]
fn rest_book_bids_match_source_non_total_comparator() {
    rest_book_sort_matches_source("bids");
}

#[test]
fn rest_book_asks_match_source_non_total_comparator() {
    rest_book_sort_matches_source("asks");
}
