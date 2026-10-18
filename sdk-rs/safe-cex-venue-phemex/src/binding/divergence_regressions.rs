//! Exact FIX-audit rows, exercised on constructed production receivers/clients.
use super::test_support as support;
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, from_json},
};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{cell::Cell, rc::Rc};
use support::{Harness, harness, options};
const ACCOUNT: &str = "/g-accounts/accountPositions";
const TICKERS: &str = "/md/v3/ticker/24hr/all";
fn venue() -> Harness {
    harness(ExchangeName::Phemex, options(), |http| {
        http.ok("GET", ACCOUNT, json!({"code":0,"data":{"account":{"userID":123,"accountBalanceRv":"100","totalUsedBalanceRv":"20"},"positions":[]}}));
        http.ok("GET", TICKERS, json!({"error":{"code":17}}));
        http.ok(
            "GET",
            "/public/products",
            json!({"data":{"perpProductsV2":[]}}),
        );
    })
}
// Observe the original capability, forwarding the exact receiver, arguments,
// return value and thrown value. The HTTP client keeps its real interceptors.
fn observe(receiver: &Js, member: &str) -> Rc<Cell<usize>> {
    let original = receiver.get(member).unwrap();
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    receiver
        .set(
            member,
            Js::function("observe-original", move |this, args| {
                seen.set(seen.get() + 1);
                original.call(this, args)
            }),
        )
        .unwrap();
    calls
}
#[test]
fn ticker_sdk_starts_http_before_poll() {
    let h = venue();
    let release = h.http.hold(TICKERS);
    let reads = observe(&h.exchange.rt.this.get("xhr").unwrap(), "get");
    let wait = h.venue().fetch_tickers();
    assert_eq!(
        reads.get(),
        1,
        "PHEMEX eager typed member invokes original HTTP client before poll"
    );
    h.runtime.pump();
    assert_eq!(h.http.calls(TICKERS).len(), 1);
    assert_eq!(h.http.calls(TICKERS)[0].method, "GET");
    drop(wait);
    release.set(true);
    h.runtime.pump();
    h.venue().dispose().unwrap();
}
#[test]
fn unit_sdk_starts_source_parallel_reads_before_poll() {
    let h = venue();
    let release = h.http.hold("/public/products");
    let reads = observe(&h.exchange.rt.this, "initExternalPublicData");
    let wait = h.venue().start();
    assert_eq!(
        reads.get(),
        1,
        "PHEMEX eager unit member invokes original initializer before poll"
    );
    assert!(
        h.http.calls("/public/products").is_empty(),
        "preserve source initializer await fence"
    );
    h.runtime.pump();
    assert_eq!(h.http.calls("/public/products").len(), 1);
    assert_eq!(
        h.http.calls(TICKERS).len(),
        1,
        "source Promise.all starts sibling ticker read"
    );
    h.venue().dispose().unwrap();
    drop(wait);
    release.set(true);
    h.runtime.pump();
}
#[test]
fn discarded_ticker_waiter_keeps_error_emission_after_http() {
    let h = venue();
    let release = h.http.hold(TICKERS);
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, value| {
            assert_eq!(
                value.to_js_string().unwrap().as_ref(),
                "[object Object]"
                    .encode_utf16()
                    .collect::<Vec<_>>()
                    .as_slice(),
                "shared source error formatting"
            );
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    drop(h.venue().fetch_tickers());
    h.runtime.pump();
    assert_eq!(count.get(), 0, "response is genuinely pending");
    release.set(true);
    h.runtime.pump();
    assert_eq!(
        count.get(),
        1,
        "PHEMEX discarded SDK waiter retains source error continuation"
    );
    h.venue().dispose().unwrap();
}
fn socket(h: &Harness) -> Js {
    h.exchange.rt.this.get("privateWebsocket").unwrap()
}
#[test]
fn reconciliation_retains_original_receiver_and_pending_store_commit() {
    let h = venue();
    let original = socket(&h);
    let parent = original.get("parent").unwrap();
    assert!(parent.strict_equals(&h.exchange.rt.this));
    assert!(
        original
            .get("store")
            .unwrap()
            .strict_equals(&parent.get("store").unwrap())
    );
    original
        .set("positionsAndBalanceSyncTimeout", Js::Number(9.0))
        .unwrap();
    original
        .set("positionsAndBalanceSyncSymbols", Js::array(vec![]))
        .unwrap();
    let release = h.http.hold(ACCOUNT);
    let reads = observe(&parent, "fetchPositionsAndBalance");
    let promise = original
        .call_method("syncPositionsAndBalance", vec![])
        .unwrap();
    assert!(
        original
            .get("positionsAndBalanceSyncTimeout")
            .unwrap()
            .is_undefined()
    );
    assert_eq!(
        original
            .get("positionsAndBalanceSyncSymbols")
            .unwrap()
            .array_values()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        reads.get(),
        1,
        "PHEMEX sync invokes original parent read before return"
    );
    h.runtime.pump();
    assert_eq!(
        h.http.calls(ACCOUNT).len(),
        2,
        "both actual settlement requests start through client interceptors"
    );
    assert!(!h.store.loaded().unwrap().balance);
    drop(promise);
    h.runtime.pump();
    assert!(
        !h.store.loaded().unwrap().balance,
        "account read remains pending"
    );
    release.set(true);
    h.runtime.pump();
    assert!(
        h.store.loaded().unwrap().balance && h.store.loaded().unwrap().positions,
        "PHEMEX discarded reconciliation Promise retains Store continuation"
    );
    assert_eq!(
        h.store.balance().unwrap().total.to_bits(),
        200.0_f64.to_bits()
    );
    assert_eq!(
        h.store.balance().unwrap().used.to_bits(),
        40.0_f64.to_bits()
    );
    assert!(
        original.strict_equals(&socket(&h)),
        "receiver identity stays fixed"
    );
    h.venue().dispose().unwrap();
}
#[test]
fn reconciliation_silently_fulfills_store_failure_on_real_binding() {
    let h = venue();
    let original = socket(&h);
    // A real subscriber failure escapes the native Store mutation into the
    // installed Promise's source catch; no Store algorithm is substituted.
    let failures = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = failures.clone();
    let unsubscribe = h.store.subscribe(std::sync::Arc::new(move |_, _| {
        seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(safe_cex_types::StoreSubscriberError::new(
            "opaque-store-error",
        ))
    }));
    let promise = original
        .call_method("syncPositionsAndBalance", vec![])
        .unwrap();
    let result = h.drive(async {
        await_js(&h.exchange.rt.exchange_runtime(), promise)
            .await
            .map_err(|e| e.to_exchange_error())
    });
    assert!(
        result
            .expect("PHEMEX source silent catch fulfills despite Store observer failure")
            .is_undefined(),
        "PHEMEX source silent catch fulfills the installed receiver Promise"
    );
    assert_eq!(
        failures.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the original Store actually notified its failing subscriber"
    );
    assert!(h.runtime.reports.borrow().is_empty());
    unsubscribe();
    h.venue().dispose().unwrap();
}
#[test]
fn reconciliation_checks_disposal_after_pending_account_read() {
    let h = venue();
    let release = h.http.hold(ACCOUNT);
    let original = socket(&h);
    drop(
        original
            .call_method("syncPositionsAndBalance", vec![])
            .unwrap(),
    );
    h.runtime.pump();
    assert_eq!(h.http.calls(ACCOUNT).len(), 2);
    h.venue().dispose().unwrap();
    release.set(true);
    h.runtime.pump();
    assert!(
        !h.store.loaded().unwrap().balance && !h.store.loaded().unwrap().positions,
        "PHEMEX post-await disposal guard suppresses Store commit"
    );
    assert_eq!(
        h.store.balance().unwrap().total.to_bits(),
        0.0_f64.to_bits()
    );
}
#[test]
fn reconciliation_clears_queued_symbols_even_when_disposed() {
    let h = venue();
    let original = socket(&h);
    h.venue().dispose().unwrap();
    original
        .set(
            "positionsAndBalanceSyncSymbols",
            from_json(&json!(["BTCUSDT"])),
        )
        .unwrap();
    original
        .set("positionsAndBalanceSyncTimeout", Js::Number(12.0))
        .unwrap();
    let promise = original
        .call_method("syncPositionsAndBalance", vec![])
        .unwrap();
    assert!(
        original
            .get("positionsAndBalanceSyncTimeout")
            .unwrap()
            .is_undefined(),
        "PHEMEX source pre-try symbol clearing resets timer"
    );
    assert!(
        original
            .get("positionsAndBalanceSyncSymbols")
            .unwrap()
            .array_values()
            .unwrap()
            .is_empty(),
        "PHEMEX source pre-try symbol clearing survives disposal"
    );
    assert!(h.http.calls(ACCOUNT).is_empty());
    assert!(
        h.drive(async {
            await_js(&h.exchange.rt.exchange_runtime(), promise)
                .await
                .map_err(|e| e.to_exchange_error())
        })
        .unwrap()
        .is_undefined()
    );
}

#[test]
fn maintained_source_confirms_prefix_retention_and_silent_catch() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-timing-source.cjs"
        ))
        .output()
        .expect("maintained TypeScript oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(observed, json!({"ticker":1,"reconciliation":3}));
}
