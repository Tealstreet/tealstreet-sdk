use super::api_factory_runtime::*;
use safe_cex_transport::RestAdapter;
use serde_json::{Value, json};
use std::{
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    task::{Context, Waker},
};
const DIAGNOSTIC: &str = "Bybit API key environment mismatch: this key is valid for Bybit Demo Trading, but this account is configured for live Bybit. Create a live Bybit API key or switch the account to Bybit Demo Trading. [invalid key]";
fn run(kind: ProbeKind, drop_first: bool) -> Value {
    let gate = Rc::new(Gate::default());
    let f = fixture(
        Mode::Probe {
            kind,
            gate: gate.clone(),
        },
        None,
    );
    let mut first = Some(f.api.execute_with_receipt(request()));
    let mut second = f.api.execute_with_receipt(request());
    let observer_wakes = Arc::new(WakeCount::default());
    let observer_waker = Waker::from(observer_wakes.clone());
    let mut cx = Context::from_waker(&observer_waker);
    assert!(
        first.as_mut().unwrap().as_mut().poll(&mut cx).is_pending(),
        "factory_probe_native_initiator_pending"
    );
    assert!(
        second.as_mut().poll(&mut cx).is_pending(),
        "factory_probe_native_joiner_pending"
    );
    f.runtime.pump();
    assert_eq!(
        f.owner.base.auth_failure_streak(),
        2,
        "factory_probe_native_original_auth_before_probe"
    );
    assert_eq!(
        f.wire.requests.borrow().len(),
        3,
        "factory_probe_native_concurrent_attempts"
    );
    assert_eq!(
        f.wire.probe_count.get(),
        1,
        "factory_probe_native_singleflight"
    );
    assert_eq!(gate.active.get(), 1, "factory_probe_native_live_transport");
    assert_eq!(
        f.runtime.active_jobs(),
        1,
        "factory_probe_native_owned_task"
    );
    if drop_first {
        drop(first.take());
        assert_eq!(
            f.runtime.active_jobs(),
            1,
            "factory_probe_native_observer_drop_keeps_task"
        );
    }
    let wakes = observer_wakes.0.load(Ordering::SeqCst);
    let task_wakes = f.wakes.0.load(Ordering::SeqCst);
    gate.release();
    assert!(
        f.wakes.0.load(Ordering::SeqCst) > task_wakes,
        "factory_probe_native_transport_wakes_runtime"
    );
    f.runtime.pump();
    assert_eq!(
        observer_wakes.0.load(Ordering::SeqCst) - wakes,
        if drop_first { 1 } else { 2 },
        "factory_probe_native_same_waker_observers"
    );
    assert_eq!(
        gate.active.get(),
        0,
        "factory_probe_native_completed_transport_released"
    );
    assert_eq!(
        f.runtime.active_jobs(),
        0,
        "factory_probe_native_completed_task_released"
    );
    let mut results = Vec::new();
    if let Some(first) = first {
        results.push(drive(first, &f.runtime).unwrap());
    }
    results.push(drive(second, &f.runtime).unwrap());
    results.push(drive(f.api.execute_with_receipt(request()), &f.runtime).unwrap());
    let expected = if matches!(kind, ProbeKind::Mismatch) {
        DIAGNOSTIC
    } else {
        "invalid key"
    };
    for result in &results {
        assert_eq!(result.response.body.json_value().unwrap()["retCode"], 10003);
        assert_eq!(
            result.response.body.json_value().unwrap()["retMsg"],
            expected,
            "factory_probe_native_cached_message"
        );
    }
    assert_eq!(
        results.len(),
        if drop_first { 2 } else { 3 },
        "factory_probe_native_observed_results"
    );
    assert_eq!(
        f.owner.base.auth_failure_streak(),
        3,
        "factory_probe_native_probe_auth_bypassed"
    );
    assert_eq!(
        f.owner.accesses.borrow().len(),
        4,
        "factory_probe_native_three_auth_calls_and_one_owned_start"
    );
    assert_eq!(
        f.clock.get_current_time_ms(),
        1000.0,
        "factory_probe_native_parsed_clock_bypassed"
    );
    assert_eq!(
        f.clock_fetches.load(Ordering::SeqCst),
        0,
        "factory_probe_native_fallback_clock_bypassed"
    );
    assert_eq!(
        f.wire.probe_count.get(),
        1,
        "factory_probe_native_cached_no_reprobe"
    );
    assert_eq!(
        f.wire.requests.borrow().len(),
        4,
        "factory_probe_native_total_attempts"
    );
    assert_eq!(
        f.host.failures.get(),
        usize::from(matches!(kind, ProbeKind::Failed)),
        "factory_probe_native_failure_route"
    );
    assert!(
        f.runtime.console.borrow().is_empty(),
        "factory_probe_native_no_console"
    );
    json!({"probes":f.wire.probe_count.get(),"authFailures":f.owner.base.auth_failure_streak(),"authSuccesses":f.owner.accesses.borrow().len()-usize::from(f.owner.base.auth_failure_streak())-1,"clockSyncs":f.clock_fetches.load(Ordering::SeqCst),"responseMessages":results.iter().map(|value|value.response.body.json_value().unwrap()["retMsg"].clone()).collect::<Vec<_>>(),"observerDropped":drop_first})
}
fn cancelled_owner(drop_owner: bool) {
    let gate = Rc::new(Gate::default());
    let f = fixture(
        Mode::Probe {
            kind: ProbeKind::Mismatch,
            gate: gate.clone(),
        },
        None,
    );
    let mut first = f.api.execute_with_receipt(request());
    let mut second = f.api.execute_with_receipt(request());
    let observer_wakes = Arc::new(WakeCount::default());
    let observer_waker = Waker::from(observer_wakes.clone());
    let mut cx = Context::from_waker(&observer_waker);
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    f.runtime.pump();
    assert_eq!(
        f.wire.probe_count.get(),
        1,
        "factory_probe_native_cancel_started"
    );
    assert_eq!(gate.active.get(), 1);
    let weak = Rc::downgrade(&f.owner);
    let wakes = observer_wakes.0.load(Ordering::SeqCst);
    if drop_owner {
        drop(f.owner);
        assert!(
            weak.upgrade().is_none(),
            "factory_probe_native_owner_not_retained"
        );
    } else {
        f.owner.base.dispose().unwrap();
    }
    assert_eq!(
        f.runtime.active_jobs(),
        0,
        "factory_probe_native_dispose_cancels_owned_task"
    );
    assert_eq!(
        gate.active.get(),
        0,
        "factory_probe_native_dispose_releases_transport"
    );
    assert_eq!(
        observer_wakes.0.load(Ordering::SeqCst) - wakes,
        2,
        "factory_probe_native_dispose_wakes_both_observers"
    );
    for error in [
        drive(first, &f.runtime).unwrap_err(),
        drive(second, &f.runtime).unwrap_err(),
    ] {
        assert_eq!(
            error.message, "Shared operation cancelled by its owner",
            "factory_probe_native_visible_owner_cancellation"
        );
        assert!(!error.is_retryable_network());
    }
    let settled_wakes = observer_wakes.0.load(Ordering::SeqCst);
    let settled_task_wakes = f.wakes.0.load(Ordering::SeqCst);
    gate.release();
    f.runtime.pump();
    assert_eq!(
        observer_wakes.0.load(Ordering::SeqCst),
        settled_wakes,
        "factory_probe_native_cancelled_transport_not_woken"
    );
    assert_eq!(
        f.wakes.0.load(Ordering::SeqCst),
        settled_task_wakes,
        "factory_probe_native_cancelled_task_not_woken"
    );
    assert_eq!(
        f.wire.requests.borrow().len(),
        3,
        "factory_probe_native_no_dispatch_after_dispose"
    );
}
pub fn collect_native() -> Vec<Value> {
    let inputs = [
        ("shared-mismatch-cached", ProbeKind::Mismatch, false),
        (
            "shared-no-mismatch-cached-probe-auth-clock-skipped",
            ProbeKind::NoMismatch,
            false,
        ),
        ("shared-probe-failed-cached", ProbeKind::Failed, false),
        (
            "caller-abort-after-response-does-not-cancel-shared-probe",
            ProbeKind::Mismatch,
            true,
        ),
    ];
    let cases = inputs
        .into_iter()
        .map(|(id, kind, drop_first)| json!({"id":id,"native":run(kind,drop_first)}))
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 4, "factory_probe_native_inventory");
    assert_eq!(
        cases
            .iter()
            .map(|value| value["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "shared-mismatch-cached",
            "shared-no-mismatch-cached-probe-auth-clock-skipped",
            "shared-probe-failed-cached",
            "caller-abort-after-response-does-not-cancel-shared-probe"
        ],
        "factory_probe_native_ids"
    );
    cancelled_owner(false);
    cancelled_owner(true);
    cases
}
pub fn compare_source(native: &[Value]) {
    assert_eq!(native.len(), 4, "factory_probe_native_before_source");
    let source = source("create-api-env-probe-boundaries.cjs", 4);
    for i in 0..4 {
        assert_eq!(native[i]["id"], source["cases"][i]["id"]);
        for field in ["probes", "authFailures", "authSuccesses", "clockSyncs"] {
            assert_eq!(
                native[i]["native"][field], source["cases"][i]["source"][field],
                "factory_probe_source_effects {i}/{field}"
            );
        }
        if i == 3 {
            assert_eq!(source["cases"][i]["source"]["callerAbortRequested"], true);
            assert_eq!(source["cases"][i]["source"]["callerAbortRejected"], false);
            assert_eq!(native[i]["native"]["observerDropped"], true);
            assert_eq!(
                native[i]["native"]["responseMessages"],
                json!([
                    source["cases"][i]["source"]["responseMessages"][1],
                    source["cases"][i]["source"]["responseMessages"][2]
                ]),
                "native future drop differs from source AbortSignal; sibling/shared work survives"
            );
        } else {
            assert_eq!(
                native[i]["native"]["responseMessages"],
                source["cases"][i]["source"]["responseMessages"]
            );
        }
        assert_eq!(
            source["cases"][i]["source"]["sourceFactoryDisposeHook"], false,
            "native owner disposal is an explicit ownership boundary"
        );
    }
}
