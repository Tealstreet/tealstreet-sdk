mod virtual_time {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../test-support/virtual_time.rs"
    ));
}
use super::*;
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask};
use safe_cex_venue_bybit::private_snapshot_refresh::{
    PrivateSnapshotRefresh, schedule_set_trading_stop_snapshot_refresh,
};
use std::{future::Future, pin::Pin};

type RuntimeWork = Pin<Box<dyn Future<Output = ()>>>;
type Slot = Rc<RefCell<Option<RuntimeWork>>>;
struct RuntimeTask(Slot);
impl ExchangeTask for RuntimeTask {
    fn cancel(&self) {
        let work = self.0.borrow_mut().take();
        drop(work);
    }
}
struct Sleep {
    due: f64,
    active: Cell<bool>,
}
struct SleepDrop(Rc<Sleep>);
impl Drop for SleepDrop {
    fn drop(&mut self) {
        self.0.active.set(false);
    }
}
#[derive(Default)]
struct Runtime {
    sleep_wakers: virtual_time::SleepWakers,
    mono: Rc<Cell<f64>>,
    wall: Cell<f64>,
    tasks: RefCell<Vec<Slot>>,
    sleeps: RefCell<Vec<Rc<Sleep>>>,
    errors: RefCell<Vec<String>>,
}
impl Runtime {
    fn pump(&self) {
        self.sleep_wakers.wake_due(self.mono.get());
        for _ in 0..100 {
            let signal = std::sync::Arc::new(virtual_time::WakeSignal::default());
            let waker = signal.waker();
            let tasks = self.tasks.borrow().clone();
            let before = tasks.len();
            let mut completed = 0;
            for slot in tasks {
                let work = slot.borrow_mut().take();
                if let Some(mut work) = work {
                    if work
                        .as_mut()
                        .poll(&mut Context::from_waker(&waker))
                        .is_pending()
                    {
                        *slot.borrow_mut() = Some(work);
                    } else {
                        completed += 1;
                    }
                }
            }
            if completed == 0 && self.tasks.borrow().len() == before && !signal.take() {
                return;
            }
        }
        panic!("SNAPSHOT-DEBOUNCE native timer task progress bound");
    }
    fn timer_count(&self) -> usize {
        self.tasks
            .borrow()
            .iter()
            .filter(|slot| slot.borrow().is_some())
            .count()
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.wall.get()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.mono.get()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        let sleep = Rc::new(Sleep {
            due: self.mono.get() + delay as f64,
            active: Cell::new(true),
        });
        self.sleeps.borrow_mut().push(sleep.clone());
        let guard = SleepDrop(sleep.clone());
        let mono = self.mono.clone();
        let sleeper = self.sleep_wakers.register(sleep.due);
        Box::pin(std::future::poll_fn(move |cx| {
            let _retained = &guard;
            if mono.get() >= sleep.due {
                Poll::Ready(Ok(()))
            } else {
                *sleeper.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, work: RuntimeWork) -> Box<dyn ExchangeTask> {
        let slot = Rc::new(RefCell::new(Some(work)));
        self.tasks.borrow_mut().push(slot.clone());
        Box::new(RuntimeTask(slot))
    }
    fn console(&self, _: &str, _: &str) {}
    fn unique_id(&self) -> String {
        "snapshot-debounce-test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.
    }
    fn report_raw_exception(&self, _: &Value, _: &Value) {
        panic!("unexpected raw exception in snapshot debounce runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.errors.borrow_mut().push(error.to_string());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async {
            Err(ExchangeError::new(
                "unexpected snapshot debounce worker request",
            ))
        })
    }
}
#[derive(Default)]
struct Hooks {
    debounce: RefCell<Option<Weak<PrivateSnapshotRefresh<Adapter>>>>,
    trace: RefCell<Vec<Value>>,
    invoked: RefCell<Vec<V>>,
    observed: Cell<usize>,
    observations: RefCell<Vec<Value>>,
    held: RefCell<Vec<V>>,
    hold: Cell<bool>,
    rejected: Cell<bool>,
    reenter: Cell<bool>,
    cancel_inside: Cell<bool>,
    report_reenter: Cell<bool>,
    report_throws: Cell<bool>,
    sync_throw: Cell<bool>,
}
struct Adapter {
    host: Rc<Host>,
    runtime: Rc<Runtime>,
    hooks: Rc<Hooks>,
    error: V,
    report_error: V,
}
impl Adapter {
    fn debounce(&self) -> Rc<PrivateSnapshotRefresh<Self>> {
        self.hooks
            .debounce
            .borrow()
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
    }
    fn report(&self, error: V) -> Result<V, V> {
        self.hooks.trace.borrow_mut().push(json!([
            "report",
            self.runtime.mono.get(),
            String::from_utf16(&text(&field(&error, "message"))).unwrap()
        ]));
        if self.hooks.report_reenter.replace(false) {
            let returned = self.debounce().call().unwrap();
            let current = self
                .hooks
                .invoked
                .borrow()
                .first()
                .is_some_and(|p| Rc::ptr_eq(p, &returned));
            assert_eq!(
                current,
                !self.hooks.sync_throw.get(),
                "snapshot_native_report_reentry_published_result"
            );
            assert_eq!(
                matches!(returned.as_ref(), Node::U),
                self.hooks.sync_throw.get(),
                "snapshot_native_report_reentry_prior_undefined"
            );
            self.hooks.trace.borrow_mut().push(json!([
                "report-return-current",
                self.runtime.mono.get(),
                current,
                matches!(returned.as_ref(), Node::U)
            ]));
        }
        if self.hooks.report_throws.get() {
            Err(self.report_error.clone())
        } else {
            Ok(u())
        }
    }
}

fn fetch_times(x: &Fixture) -> Vec<f64> {
    x.host
        .hooks
        .trace
        .borrow()
        .iter()
        .filter(|event| event[0] == "fetch")
        .map(|event| event[1].as_f64().unwrap())
        .collect()
}
fn reports(x: &Fixture) -> usize {
    x.host
        .hooks
        .trace
        .borrow()
        .iter()
        .filter(|event| event[0] == "report")
        .count()
}
fn assert_json_numbers(a: &Value, b: &Value, label: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{label}: binary64"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label}: array length");
            for (a, b) in a.iter().zip(b) {
                assert_json_numbers(a, b, label);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{label}: object length");
            for (key, value) in a {
                assert_json_numbers(value, b.get(key).expect("source field"), label);
            }
        }
        _ => assert_eq!(a, b, "{label}"),
    }
}
#[test]
#[ignore = "requires installed maintained TypeScript and Lodash"]
fn private_snapshot_refresh_native_source_oracle() {
    let mut cases = vec![];
    for kind in [
        "single",
        "trailing",
        "max-wait",
        "cancel",
        "cancel-reschedule",
        "clock-rollback",
        "disposed",
        "public-only",
        "overlap",
        "rejected",
        "reentrant",
        "cancel-inside",
    ] {
        let x = Fixture::new("snapshot refused");
        set_field(&x.host.host.owner, "isDisposed", b(kind == "disposed"));
        set_field(
            &x.host.host.owner,
            "isPublicDataOnly",
            b(kind == "public-only"),
        );
        x.host.hooks.hold.set(kind == "overlap");
        x.host.hooks.rejected.set(kind == "rejected");
        x.host.hooks.reenter.set(kind == "reentrant");
        x.host.hooks.cancel_inside.set(kind == "cancel-inside");
        x.call();
        if kind == "trailing" {
            x.advance(1200.);
            x.call();
        }
        if kind == "max-wait" {
            for time in [1200., 1400., 1600., 1800., 2000.] {
                x.advance(time);
                x.call();
            }
        }
        if kind == "clock-rollback" {
            x.advance(1100.);
            x.host.runtime.wall.set(900.);
            x.call();
        }
        if kind == "cancel" {
            x.debounce.cancel();
        }
        if kind == "cancel-reschedule" {
            x.debounce.cancel();
            x.advance(1100.);
            x.call();
        }
        x.advance(2250.);
        if kind == "overlap" {
            x.call();
            x.advance(2500.);
            assert_eq!(
                x.host.hooks.held.borrow().len(),
                2,
                "snapshot_native_overlapping_inflight_count"
            );
            x.release(false);
            x.advance(2501.);
        }
        let expected: &[f64] = match kind {
            "single" | "rejected" | "cancel-inside" => &[1250.],
            "trailing" => &[1450.],
            "max-wait" => &[2200.],
            "cancel" | "disposed" | "public-only" => &[],
            "cancel-reschedule" => &[1350.],
            "clock-rollback" => &[1100.],
            "overlap" => &[1250., 2500.],
            "reentrant" => &[1250., 1500.],
            _ => unreachable!(),
        };
        assert_eq!(
            fetch_times(&x),
            expected,
            "snapshot_native_independent_invocation_times {kind}"
        );
        assert_eq!(
            reports(&x),
            usize::from(kind == "rejected"),
            "snapshot_native_report_count {kind}"
        );
        let invocations = match kind {
            "cancel" => 0,
            "overlap" | "reentrant" => 2,
            _ => 1,
        };
        assert_eq!(
            x.host.hooks.invoked.borrow().len(),
            invocations,
            "snapshot_native_fixed_invocation_floor {kind}"
        );
        assert_eq!(
            x.host.hooks.observations.borrow().len(),
            invocations,
            "snapshot_native_fixed_observation_floor {kind}"
        );
        assert_eq!(
            x.host.runtime.timer_count(),
            0,
            "snapshot_native_finished_timer_count {kind}"
        );
        cases.push(x.capture(kind, "variant"));
    }
    assert_eq!(cases.len(), 12, "snapshot_native_schedule_case_floor");
    let mut supplements = vec![];
    for kind in [
        "promise-rejection-reentry",
        "sync-throw-reentry",
        "reporter-rejection",
    ] {
        let x = Fixture::new(if kind == "sync-throw-reentry" {
            "synchronous snapshot refused"
        } else {
            "snapshot refused"
        });
        x.host.hooks.sync_throw.set(kind == "sync-throw-reentry");
        x.host.hooks.rejected.set(true);
        x.host
            .hooks
            .report_reenter
            .set(kind != "reporter-rejection");
        x.host.hooks.report_throws.set(kind == "reporter-rejection");
        x.call();
        x.advance(2250.);
        let expected = if kind == "reporter-rejection" {
            vec![1250.]
        } else {
            vec![1250., 1500.]
        };
        assert_eq!(
            fetch_times(&x),
            expected,
            "snapshot_native_reporter_invocation_times {kind}"
        );
        assert_eq!(
            reports(&x),
            expected.len(),
            "snapshot_native_reporter_floor {kind}"
        );
        assert_eq!(
            x.host.hooks.invoked.borrow().len(),
            expected.len(),
            "snapshot_native_reporter_result_floor {kind}"
        );
        let mut captured = x.capture(kind, "kind");
        captured.as_object_mut().unwrap().remove("timers");
        supplements.push(captured);
    }
    assert_eq!(supplements.len(), 3, "snapshot_native_reporter_case_floor");
    let mut identities = vec![];
    for kind in [
        "flush-new-result",
        "flush-pending-replacement",
        "cancel-retains-result",
        "flush-reentrant",
    ] {
        let x = Fixture::new("snapshot refused");
        x.host.hooks.hold.set(true);
        x.host.hooks.reenter.set(kind == "flush-reentrant");
        assert!(
            matches!(x.debounce.call().unwrap().as_ref(), Node::U),
            "snapshot_native_initial_undefined"
        );
        x.advance(1100.);
        let first = x.debounce.flush();
        assert_eq!(
            x.host.hooks.invoked.borrow().len(),
            1,
            "snapshot_native_flush_eager"
        );
        assert!(
            Rc::ptr_eq(&first, &x.host.hooks.invoked.borrow()[0]),
            "snapshot_native_flush_original_promise"
        );
        assert!(
            Rc::ptr_eq(&x.debounce.call().unwrap(), &first),
            "snapshot_native_call_cached_promise"
        );
        if kind == "flush-pending-replacement" {
            let second = x.debounce.flush();
            assert_eq!(
                x.host.hooks.invoked.borrow().len(),
                2,
                "snapshot_native_second_flush_invokes"
            );
            assert!(
                Rc::ptr_eq(&second, &x.host.hooks.invoked.borrow()[1]),
                "snapshot_native_second_flush_original_promise"
            );
            assert!(
                !Rc::ptr_eq(&first, &second),
                "snapshot_native_distinct_async_identities"
            );
        } else if kind == "cancel-retains-result" {
            x.debounce.cancel();
            assert!(
                Rc::ptr_eq(&x.debounce.flush(), &first),
                "snapshot_native_cancel_retains_result"
            );
        }
        x.release(false);
        x.drain();
        x.advance(1500.);
        x.release(false);
        x.drain();
        let expected = match kind {
            "flush-new-result" | "flush-reentrant" => vec![1100., 1350.],
            "flush-pending-replacement" => vec![1100., 1100.],
            "cancel-retains-result" => vec![1100.],
            _ => unreachable!(),
        };
        assert_eq!(
            fetch_times(&x),
            expected,
            "snapshot_native_physical_flush_timer_behavior {kind}"
        );
        assert_eq!(
            x.host.hooks.invoked.borrow().len(),
            expected.len(),
            "snapshot_native_flush_invocation_floor"
        );
        assert_eq!(
            x.host.hooks.observations.borrow().len(),
            expected.len(),
            "snapshot_native_flush_observation_floor"
        );
        identities.push(x.capture(kind, "kind"));
    }
    assert_eq!(identities.len(), 4, "snapshot_native_identity_case_floor");
    let mut lifetimes = vec![];
    for kind in [
        "disposed-pending-success",
        "disposed-pending-rejection",
        "public-only-pending-success",
    ] {
        let x = Fixture::new("pending snapshot refused");
        x.host.hooks.hold.set(true);
        x.call();
        x.advance(1250.);
        assert_eq!(
            x.host.hooks.held.borrow().len(),
            1,
            "snapshot_native_started_helper_floor"
        );
        assert!(
            x.host.hooks.observations.borrow().is_empty(),
            "snapshot_native_started_helper_pending"
        );
        if kind == "public-only-pending-success" {
            set_field(&x.host.host.owner, "isPublicDataOnly", b(true));
        } else {
            set_field(&x.host.host.owner, "isDisposed", b(true));
            x.debounce.cancel();
        }
        x.release(kind == "disposed-pending-rejection");
        x.drain();
        assert_eq!(
            fetch_times(&x),
            vec![1250.],
            "snapshot_native_post_disposal_no_second_helper"
        );
        assert_eq!(
            reports(&x),
            usize::from(kind == "disposed-pending-rejection"),
            "snapshot_native_post_disposal_report_floor"
        );
        assert_eq!(
            x.host.hooks.invoked.borrow().len(),
            1,
            "snapshot_native_post_disposal_promise_retained"
        );
        assert_eq!(
            x.host.hooks.observations.borrow().len(),
            1,
            "snapshot_native_post_disposal_result_retained"
        );
        let mut captured = x.capture(kind, "kind");
        captured.as_object_mut().unwrap().remove("timers");
        lifetimes.push(captured);
    }
    assert_eq!(lifetimes.len(), 3, "snapshot_native_lifetime_case_floor");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let output = std::process::Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-venue-bybit/scripts/snapshot-refresh-source.cjs"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "snapshot source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (key, native, count) in [
        ("cases", cases, 12),
        ("supplements", supplements, 3),
        ("identities", identities, 4),
        ("lifetimes", lifetimes, 3),
    ] {
        assert_eq!(
            source[key].as_array().unwrap().len(),
            count,
            "snapshot_source_fixed_case_floor {key}"
        );
        assert_json_numbers(&json!(native), &source[key], key);
    }
}
macro_rules! forward {
    ($($name:ident($($arg:ident:$ty:ty),*)->$result:ty;)*) => {$ (
        fn $name(&self,$($arg:$ty),*)->$result { self.host.$name($($arg),*) }
    )*};
}
impl SnapshotHost for Adapter {
    type Value = V;
    type Error = V;
    forward! {
        get(owner:&V,key:&str)->Result<V,V>;
        optional(owner:&V,key:&str)->Result<V,V>;
        set(owner:&V,key:&str,value:V)->Result<(),V>;
        truthy(value:&V)->bool;
        equal(a:&V,b:&V)->bool;
        undefined()->V;
        literal(value:&str)->V;
        boolean(value:bool)->V;
        number(value:f64)->V;
        numeric(value:&V)->Result<f64,V>;
        object(fields:Vec<(&'static str,V)>)->V;
        array(values:Vec<V>)->V;
        values(array:&V)->Result<Vec<V>,V>;
        is_array(value:&V)->bool;
        spread(value:V,fields:Vec<(&'static str,V)>)->Result<V,V>;
        intrinsic(name:&str,args:Vec<V>)->Result<V,V>;
        callback(owner:V,code:&'static str,captures:Vec<V>)->Result<V,V>;
        error(message:V)->Result<V,V>;
        error_value(error:V)->V;
    }
    fn select(&self, owner: V, key: &str) -> Result<V, V> {
        if matches!(
            key,
            "fetchAndSetPrivateSnapshotHelper"
                | "emitAndLogError"
                | "debouncedPrivateSnapshotRefresh"
        ) {
            Ok(self.host.function(owner, &format!("refresh:{key}"), vec![]))
        } else {
            self.host.select(owner, key)
        }
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, V> {
        let Node::Fn {
            code,
            owner: receiver,
            ..
        } = selected.as_ref()
        else {
            return self.host.apply(selected, args);
        };
        if code.starts_with("refresh:") {
            assert!(
                Rc::ptr_eq(receiver, &self.host.owner),
                "snapshot_native_selected_receiver_identity"
            );
        }
        match code.as_str() {
            "refresh:fetchAndSetPrivateSnapshotHelper" => {
                assert!(args.is_empty(), "snapshot_native_helper_no_arguments");
                self.hooks
                    .trace
                    .borrow_mut()
                    .push(json!(["fetch", self.runtime.mono.get()]));
                if self.hooks.reenter.replace(false) {
                    self.debounce().call().unwrap();
                }
                if self.hooks.cancel_inside.get() {
                    self.debounce().cancel();
                }
                if self.hooks.sync_throw.get() {
                    return Err(self.error.clone());
                }
                if self.hooks.hold.get() {
                    let value = Rc::new(Node::Promise(Promise {
                        state: RefCell::new(None),
                        listeners: RefCell::new(vec![]),
                    }));
                    self.hooks.held.borrow_mut().push(value.clone());
                    return Ok(value);
                }
                let result = if self.hooks.rejected.get() {
                    Err(self.error.clone())
                } else {
                    Ok(u())
                };
                Ok(self.host.launch(Box::pin(async move { result })))
            }
            "refresh:emitAndLogError" => {
                assert_eq!(args.len(), 1, "snapshot_native_reporter_argument_count");
                assert!(
                    Rc::ptr_eq(&args[0], &self.error),
                    "snapshot_native_original_error_identity"
                );
                self.report(args[0].clone())
            }
            "refresh:debouncedPrivateSnapshotRefresh" => {
                assert!(args.is_empty(), "snapshot_native_schedule_no_arguments");
                Ok(self.debounce().call().unwrap())
            }
            _ => self.host.apply(selected, args),
        }
    }
    fn launch(&self, body: Operation<V, V>) -> V {
        let result = self.host.launch(body);
        self.hooks.invoked.borrow_mut().push(result.clone());
        result
    }
    fn wait(&self, value: V) -> Operation<V, V> {
        self.host.wait(value)
    }
}
struct Fixture {
    host: Rc<Adapter>,
    debounce: Rc<PrivateSnapshotRefresh<Adapter>>,
}
impl Fixture {
    fn new(error_message: &str) -> Self {
        let runtime = Rc::new(Runtime::default());
        runtime.mono.set(1000.);
        runtime.wall.set(1000.);
        let host = Host::new(&json!({}));
        set_field(&host.owner, "isPublicDataOnly", b(false));
        let hooks = Rc::new(Hooks::default());
        let host = Rc::new(Adapter {
            host,
            runtime,
            hooks,
            error: Rc::new(Node::Error {
                name: "Error".encode_utf16().collect(),
                message: error_message.encode_utf16().collect(),
            }),
            report_error: Rc::new(Node::Error {
                name: "Error".encode_utf16().collect(),
                message: "reporting listener refused".encode_utf16().collect(),
            }),
        });
        let debounce = PrivateSnapshotRefresh::new(
            host.clone(),
            host.host.owner.clone(),
            host.runtime.clone(),
        );
        *host.hooks.debounce.borrow_mut() = Some(Rc::downgrade(&debounce));
        Self { host, debounce }
    }
    fn drain(&self) {
        for _ in 0..256 {
            if self.host.host.queue.lock().unwrap().is_empty() {
                break;
            }
            self.host.host.turn();
        }
        assert!(
            self.host.host.queue.lock().unwrap().is_empty(),
            "snapshot_native_microtask_bound"
        );
        let invoked = self.host.hooks.invoked.borrow();
        let mut index = self.host.hooks.observed.get();
        while let Some(value) = invoked.get(index) {
            let Node::Promise(promise) = value.as_ref() else {
                panic!("snapshot_native_async_result");
            };
            let state = promise.state.borrow().clone();
            let Some(state) = state else {
                break;
            };
            let observed = match state {
                Ok(value) => {
                    assert!(
                        matches!(value.as_ref(), Node::U),
                        "snapshot_native_async_resolves_undefined"
                    );
                    json!(["resolved-observation", self.host.runtime.mono.get()])
                }
                Err(error) => {
                    assert!(
                        Rc::ptr_eq(&error, &self.host.report_error),
                        "snapshot_native_reporter_rejection_identity"
                    );
                    json!([
                        "rejected-observation",
                        self.host.runtime.mono.get(),
                        "reporting listener refused"
                    ])
                }
            };
            self.host.hooks.observations.borrow_mut().push(observed);
            index += 1;
        }
        self.host.hooks.observed.set(index);
        self.host.runtime.pump();
    }
    fn call(&self) {
        self.host
            .hooks
            .trace
            .borrow_mut()
            .push(json!(["call", self.host.runtime.mono.get()]));
        let returned = schedule_set_trading_stop_snapshot_refresh(
            self.host.as_ref(),
            self.host.host.owner.clone(),
        )
        .unwrap();
        assert!(
            matches!(returned.as_ref(), Node::U),
            "snapshot_native_schedule_undefined"
        );
        self.host.runtime.pump();
        self.drain();
    }
    fn advance(&self, to: f64) {
        let rt = &self.host.runtime;
        assert!(to >= rt.mono.get());
        rt.pump();
        for _ in 0..1000 {
            let next = rt
                .sleeps
                .borrow()
                .iter()
                .filter(|s| s.active.get() && s.due > rt.mono.get() && s.due <= to)
                .map(|s| s.due)
                .min_by(f64::total_cmp);
            let next = next.unwrap_or(to);
            rt.wall.set(rt.wall.get() + next - rt.mono.get());
            rt.mono.set(next);
            rt.pump();
            self.drain();
            if next == to {
                return;
            }
        }
        panic!("snapshot_native_advance_bound");
    }
    fn release(&self, rejected: bool) {
        let held = std::mem::take(&mut *self.host.hooks.held.borrow_mut());
        for value in held {
            let Node::Promise(p) = value.as_ref() else {
                panic!("snapshot_native_held_promise");
            };
            *p.state.borrow_mut() = Some(if rejected {
                Err(self.host.error.clone())
            } else {
                Ok(u())
            });
            for waker in std::mem::take(&mut *p.listeners.borrow_mut()) {
                waker.wake();
            }
        }
    }
    fn capture(&self, kind: &str, key: &str) -> Value {
        let mut rejected = 0;
        for value in self.host.hooks.invoked.borrow().iter() {
            let Node::Promise(promise) = value.as_ref() else {
                panic!("snapshot_native_callback_result_is_promise");
            };
            match promise.state.borrow().as_ref() {
                Some(Ok(value)) => assert!(
                    matches!(value.as_ref(), Node::U),
                    "snapshot_native_resolved_result_undefined"
                ),
                Some(Err(error)) => {
                    rejected += 1;
                    assert!(
                        Rc::ptr_eq(error, &self.host.report_error),
                        "snapshot_native_rejected_result_identity"
                    );
                }
                None => panic!("snapshot_native_callback_result_unsettled"),
            }
        }
        assert_eq!(
            rejected,
            usize::from(kind == "reporter-rejection"),
            "snapshot_native_rejected_result_count {kind}"
        );
        assert!(
            self.host.runtime.errors.borrow().is_empty(),
            "snapshot_native_runtime_error_floor"
        );
        assert_eq!(
            self.host.hooks.observed.get(),
            self.host.hooks.invoked.borrow().len(),
            "snapshot_native_all_results_observed"
        );
        json!({key:kind,"trace":self.host.hooks.trace.borrow().clone(),"timers":self.host.runtime.timer_count(),"observations":self.host.hooks.observations.borrow().clone()})
    }
}

#[test]
fn pending_sleep_registers_the_latest_waker_and_cancels_on_drop() {
    let runtime = Runtime::default();
    let first = std::sync::Arc::new(virtual_time::WakeSignal::default());
    let latest = std::sync::Arc::new(virtual_time::WakeSignal::default());
    let mut future = runtime.sleep(10);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&first.waker()))
            .is_pending()
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_pending()
    );
    runtime.mono.set(runtime.mono.get() + 9.0);
    runtime.pump();
    assert!(
        !first.take() && !latest.take(),
        "sleep woke before deadline"
    );
    runtime.mono.set(runtime.mono.get() + 1.0);
    runtime.pump();
    assert!(
        !first.take() && latest.take(),
        "expired sleep must wake latest waiter"
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_ready()
    );
    let mut cancelled = runtime.sleep(10);
    assert!(
        cancelled
            .as_mut()
            .poll(&mut Context::from_waker(&latest.waker()))
            .is_pending()
    );
    drop(cancelled);
    runtime.mono.set(runtime.mono.get() + 10.0);
    runtime.pump();
    assert!(!latest.take(), "dropped sleep must not wake");
}
