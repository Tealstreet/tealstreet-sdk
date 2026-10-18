use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::ExchangeName;
use safe_cex_venue_asterdex::{
    types::AsterDEXAPIVersion,
    virtual_clock::{AsterDEXClockRealm, ClockHttp, ClockOperation, aster_dex_virtual_clock},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn poll<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(&Waker::noop().clone()))
}
type Work = Pin<Box<dyn Future<Output = ()>>>;
struct Job {
    cancelled: Cell<bool>,
    future: RefCell<Option<Work>>,
}
type CancelHook = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;
struct Task(Rc<Job>, CancelHook);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.cancelled.set(true);
        self.0.future.borrow_mut().take();
        let hook = self.1.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
    }
}
#[derive(Default)]
struct Runtime {
    cancel_hook: CancelHook,
    wall: Cell<f64>,
    mono: Rc<Cell<f64>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    sleeps: Rc<RefCell<Vec<Rc<Cell<bool>>>>>,
    logs: RefCell<Vec<String>>,
}
impl Runtime {
    fn pump(&self) {
        // A bounded deterministic microtask floor, not a claim about arbitrary
        // host scheduler progress. Scenarios require at most four chained awaits.
        for _ in 0..32 {
            let jobs = self.jobs.borrow().clone();
            for job in jobs {
                if job.cancelled.get() {
                    continue;
                }
                let Some(mut future) = job.future.borrow_mut().take() else {
                    continue;
                };
                if poll(future.as_mut()).is_pending() && !job.cancelled.get() {
                    *job.future.borrow_mut() = Some(future);
                }
            }
        }
    }
    fn timers(&self) -> usize {
        self.sleeps
            .borrow()
            .iter()
            .filter(|active| active.get())
            .count()
    }
}
struct Sleep {
    mono: Rc<Cell<f64>>,
    deadline: f64,
    active: Rc<Cell<bool>>,
}
impl Future for Sleep {
    type Output = Result<(), ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.mono.get() >= self.deadline {
            self.active.set(false);
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
impl Drop for Sleep {
    fn drop(&mut self) {
        self.active.set(false);
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
        let active = Rc::new(Cell::new(true));
        self.sleeps.borrow_mut().push(active.clone());
        Box::pin(Sleep {
            mono: self.mono.clone(),
            deadline: self.mono.get() + delay as f64,
            active,
        })
    }
    fn spawn_local(&self, future: Work) -> Box<dyn ExchangeTask> {
        let job = Rc::new(Job {
            cancelled: Cell::new(false),
            future: RefCell::new(Some(future)),
        });
        self.jobs.borrow_mut().push(job.clone());
        Box::new(Task(job, self.cancel_hook.clone()))
    }
    fn console_values(&self, _: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        self.logs.borrow_mut().push(message.into());
    }
    fn unique_id(&self) -> String {
        "clock".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected clock task error: {error}");
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
type Response = Rc<RefCell<Option<Result<Value, ExchangeError>>>>;
#[derive(Default)]
struct Http {
    requests: RefCell<Vec<(String, u64)>>,
    pending: RefCell<VecDeque<Response>>,
}
struct Waiting(Response);
impl Future for Waiting {
    type Output = Result<Value, ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .borrow_mut()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}
impl ClockHttp for Http {
    fn get(&self, url: String, timeout: u64) -> ExchangeFuture<'static, Value> {
        self.requests.borrow_mut().push((url, timeout));
        let response = Rc::new(RefCell::new(None));
        self.pending.borrow_mut().push_back(response.clone());
        Box::pin(Waiting(response))
    }
    fn with_public_cors_proxy(&self, url: &str) -> Result<String, ExchangeError> {
        Ok(format!("proxy:{url}"))
    }
}
fn fixture() -> (Rc<Runtime>, Rc<Http>, Rc<BaseExchange>, AsterDEXClockRealm) {
    let runtime = Rc::new(Runtime::default());
    runtime.wall.set(1000.0);
    let http = Rc::new(Http::default());
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    ));
    let realm = AsterDEXClockRealm::new(base.clone(), http.clone());
    (runtime, http, base, realm)
}
enum Operation {
    Unit(ClockOperation<()>),
    Offset(ClockOperation<f64>),
}
struct Snapshot {
    offset: f64,
    current: f64,
    needs: bool,
    timers: usize,
    in_flight: bool,
    requests: usize,
}
struct Trace {
    snapshots: Vec<Snapshot>,
    requests: Vec<(String, u64)>,
    operations: Vec<(String, bool, Option<f64>)>,
    logs: Vec<String>,
}
fn run(scenario: &Value) -> Trace {
    let (runtime, http, base, realm) = fixture();
    let clock = aster_dex_virtual_clock(&realm);
    let mut operations = Vec::new();
    let mut snapshots = Vec::new();
    for step in scenario["steps"].as_array().unwrap() {
        let op = step["op"].as_str().unwrap();
        let testnet = step["testnet"].as_bool().unwrap_or(false);
        let preferred = step
            .get("version")
            .map(|v| serde_json::from_value::<AsterDEXAPIVersion>(v.clone()).unwrap());
        match op {
            "sync" | "force" | "start" => {
                let operation = match op {
                    "sync" => Operation::Unit(clock.sync_client_server_time(testnet, preferred)),
                    "force" => {
                        Operation::Offset(clock.force_sync_and_get_offset(testnet, preferred))
                    }
                    _ => Operation::Unit(
                        clock.start(testnet, preferred.unwrap_or(AsterDEXAPIVersion::V1)),
                    ),
                };
                if !step["discard"].as_bool().unwrap_or(false) {
                    operations.push((op.to_owned(), operation));
                }
            }
            "stop" => clock.stop(),
            "wall" => runtime.wall.set(step["value"].as_f64().unwrap()),
            "tick" => runtime.mono.set(step["value"].as_f64().unwrap()),
            "resolve" => {
                let response = http
                    .pending
                    .borrow_mut()
                    .pop_front()
                    .expect("pending HTTP response");
                *response.borrow_mut() = Some(if step["error"].as_bool().unwrap_or(false) {
                    Err(ExchangeError::new("offline"))
                } else {
                    Ok(step["data"].clone())
                });
            }
            "snapshot" => snapshots.push(Snapshot {
                offset: clock.get_time_offset(),
                current: clock.get_current_time_ms(),
                needs: clock.needs_sync(),
                timers: runtime.timers(),
                in_flight: clock.is_syncing(),
                requests: http.requests.borrow().len(),
            }),
            _ => panic!("unknown clock operation"),
        }
        runtime.pump();
    }
    let operations = operations
        .into_iter()
        .map(|(name, op)| match op {
            Operation::Unit(mut op) => {
                let done = match poll(Pin::new(&mut op)) {
                    Poll::Ready(result) => {
                        result.unwrap();
                        true
                    }
                    Poll::Pending => false,
                };
                (name, done, None)
            }
            Operation::Offset(mut op) => match poll(Pin::new(&mut op)) {
                Poll::Ready(result) => (name, true, Some(result.unwrap())),
                Poll::Pending => (name, false, None),
            },
        })
        .collect();
    let trace = Trace {
        snapshots,
        requests: http.requests.borrow().clone(),
        operations,
        logs: runtime.logs.borrow().clone(),
    };
    clock.stop();
    base.dispose().unwrap();
    trace
}
fn same_number(actual: f64, expected: &Value, label: &str) {
    let expected = if expected.as_str() == Some("NaN") {
        f64::NAN
    } else {
        expected.as_f64().unwrap()
    };
    assert!(
        actual.to_bits() == expected.to_bits() || actual.is_nan() && expected.is_nan(),
        "{label}: native {actual:?}, source {expected:?}"
    );
}

#[test]
#[ignore = "requires the live TypeScript source and real Dayjs"]
fn clock_lifecycle_matches_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/clock-lifecycle-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("clock-lifecycle-cases.json")).unwrap();
    // Independent, literal expectations; iteration 1 stops at these five scenarios.
    const NAMES: [&str; 5] = [
        "fallback-v3-direct-then-proxy",
        "failed-refresh-retains-offset",
        "stop-restart-coalesces-stale-request",
        "discarded-start-still-installs-interval",
        "truthy-invalid-dayjs-does-not-fallback",
    ];
    const SNAPSHOTS: [usize; 5] = [2, 4, 6, 4, 2];
    const REQUESTS: [usize; 5] = [4, 5, 3, 2, 1];
    const OPERATIONS: [usize; 5] = [1, 2, 4, 0, 1];
    assert_eq!(cases.len(), 5);
    assert_eq!(expected.len(), 5);
    let actual: Vec<Trace> = cases.iter().map(run).collect();
    // Every decoded native value is checked before any URL/log boundary assertion.
    for (i, (trace, source)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(cases[i]["name"], NAMES[i]);
        assert_eq!(source["name"], NAMES[i]);
        assert_eq!(trace.snapshots.len(), SNAPSHOTS[i]);
        assert_eq!(source["snapshots"].as_array().unwrap().len(), SNAPSHOTS[i]);
        for (native, snapshot) in trace
            .snapshots
            .iter()
            .zip(source["snapshots"].as_array().unwrap())
        {
            same_number(
                native.offset,
                &snapshot["offset"],
                &format!("{} offset/generation", NAMES[i]),
            );
            same_number(native.current, &snapshot["current"], NAMES[i]);
            assert_eq!(
                native.needs,
                snapshot["needs"].as_bool().unwrap(),
                "{} needsSync",
                NAMES[i]
            );
            assert_eq!(
                native.timers,
                snapshot["timers"].as_u64().unwrap() as usize,
                "{} interval ownership",
                NAMES[i]
            );
            assert_eq!(native.in_flight, snapshot["inFlight"].as_bool().unwrap());
            assert_eq!(
                native.requests,
                snapshot["requests"].as_u64().unwrap() as usize
            );
        }
        assert_eq!(trace.operations.len(), OPERATIONS[i]);
        assert_eq!(
            source["operations"].as_array().unwrap().len(),
            OPERATIONS[i]
        );
        for ((kind, done, value), operation) in trace
            .operations
            .iter()
            .zip(source["operations"].as_array().unwrap())
        {
            assert_eq!(kind, operation["kind"].as_str().unwrap());
            assert_eq!(*done, operation["done"].as_bool().unwrap());
            if let Some(value) = value {
                same_number(*value, &operation["value"], NAMES[i]);
            }
        }
    }
    for (i, (trace, source)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(trace.requests.len(), REQUESTS[i]);
        assert_eq!(source["requests"].as_array().unwrap().len(), REQUESTS[i]);
        for ((url, timeout), request) in trace
            .requests
            .iter()
            .zip(source["requests"].as_array().unwrap())
        {
            assert_eq!(
                url,
                request["url"].as_str().unwrap(),
                "{} fetch fallback order",
                NAMES[i]
            );
            assert_eq!(*timeout, 5000);
            assert_eq!(request["timeout"], 5000);
        }
        let logs: Vec<_> = source["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(trace.logs, logs, "{} console", NAMES[i]);
    }
}

#[test]
fn clock_realm_shares_one_clock_and_isolates_other_realms() {
    let (runtime, http, base, realm) = fixture();
    let first = aster_dex_virtual_clock(&realm);
    let again = aster_dex_virtual_clock(&realm);
    assert!(Rc::ptr_eq(&first, &again));
    let other = AsterDEXClockRealm::new(base.clone(), http.clone());
    assert!(!Rc::ptr_eq(&first, &aster_dex_virtual_clock(&other)));
    drop(first.force_sync_and_get_offset(false, None));
    *http.pending.borrow_mut().pop_front().unwrap().borrow_mut() =
        Some(Ok(json!({"serverTime":1100})));
    runtime.pump();
    assert_eq!(again.get_time_offset(), 100.0);
    assert_eq!(aster_dex_virtual_clock(&other).get_time_offset(), 0.0);
    first.stop();
    base.dispose().unwrap();
}

#[test]
fn clock_stop_releases_timer_slot_before_reentrant_host_cancellation() {
    let (runtime, http, base, realm) = fixture();
    let clock = aster_dex_virtual_clock(&realm);
    drop(clock.start(false, AsterDEXAPIVersion::V1));
    *http.pending.borrow_mut().pop_front().unwrap().borrow_mut() =
        Some(Ok(json!({"serverTime":1100})));
    runtime.pump();
    assert_eq!(runtime.timers(), 1);
    let weak = Rc::downgrade(&clock);
    *runtime.cancel_hook.borrow_mut() = Some(Box::new(move || weak.upgrade().unwrap().stop()));
    clock.stop();
    assert_eq!(runtime.timers(), 0);
    assert_eq!(clock.get_time_offset(), 0.0);
    assert!(!clock.is_syncing());
    base.dispose().unwrap();
}

#[test]
fn clock_owner_disposal_cancels_work_without_settling_completion() {
    let (runtime, http, base, realm) = fixture();
    let clock = aster_dex_virtual_clock(&realm);
    let mut operation = clock.force_sync_and_get_offset(false, None);
    assert!(clock.is_syncing());
    base.dispose().unwrap();
    *http.pending.borrow_mut().pop_front().unwrap().borrow_mut() =
        Some(Ok(json!({"serverTime":1100})));
    runtime.pump();
    assert!(
        poll(Pin::new(&mut operation)).is_pending(),
        "documented native owner cancellation boundary"
    );
    assert!(clock.is_syncing());
    assert_eq!(clock.get_time_offset(), 0.0);
    assert_eq!(runtime.timers(), 0);
}
