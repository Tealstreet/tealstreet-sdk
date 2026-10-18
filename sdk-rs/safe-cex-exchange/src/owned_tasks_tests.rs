use super::*;
use crate::runtime::{ExchangeError, ExchangeFuture};
use serde_json::Value;
use std::task::Waker;

type Observer = Rc<RefCell<Option<Box<dyn Fn()>>>>;
type Job = Rc<RefCell<Option<Work>>>;
#[derive(Default)]
struct Runtime {
    mono: Rc<Cell<f64>>,
    wall: Cell<f64>,
    jobs: RefCell<Vec<Job>>,
    delays: RefCell<Vec<u64>>,
    errors: RefCell<Vec<String>>,
    inline: Cell<bool>,
    discard_spawn: Cell<bool>,
    retain_ready: Cell<bool>,
    cancel_handle_drop: Cell<bool>,
    retained: RefCell<Vec<Work>>,
    cancel_observer: Observer,
    report_observer: Observer,
    trace: Rc<RefCell<Vec<String>>>,
    registered_sleeps: Rc<Cell<usize>>,
    next_sleep: RefCell<Option<ExchangeFuture<'static, ()>>>,
}
struct Task {
    drop_cancels: bool,
    job: Job,
    observer: Observer,
}
impl ExchangeTask for Task {
    fn cancel(&self) {
        let future = self.job.borrow_mut().take();
        if let Some(observer) = self.observer.borrow().as_ref() {
            observer();
        }
        drop(future);
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        if self.drop_cancels {
            self.cancel();
        }
    }
}
impl Runtime {
    fn pump(&self) {
        let jobs = self.jobs.borrow().clone();
        for job in jobs {
            let future = job.borrow_mut().take();
            if let Some(mut future) = future {
                if future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    *job.borrow_mut() = Some(future);
                } else if self.retain_ready.get() {
                    self.retained.borrow_mut().push(future);
                }
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.mono.get()
    }
    fn now_ms(&self) -> f64 {
        self.wall.get()
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.delays.borrow_mut().push(delay);
        if let Some(sleep) = self.next_sleep.borrow_mut().take() {
            return sleep;
        }
        let trace = self.trace.clone();
        let registered = self.registered_sleeps.clone();
        let mut registration = None;
        let clock = self.mono.clone();
        let deadline = clock.get() + delay as f64;
        let mut yielded = false;
        Box::pin(std::future::poll_fn(move |cx| {
            if !yielded {
                registered.set(registered.get() + 1);
                registration = Some(SleepRegistration(registered.clone()));
                trace.borrow_mut().push("rearm".into());
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if clock.get() >= deadline {
                drop(registration.take());
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, future: Work) -> Box<dyn ExchangeTask> {
        let job = Rc::new(RefCell::new(Some(future)));
        self.jobs.borrow_mut().push(job.clone());
        if self.discard_spawn.get() {
            let future = job.borrow_mut().take();
            drop(future);
        } else if self.inline.get() {
            self.pump();
        }
        Box::new(Task {
            drop_cancels: self.cancel_handle_drop.get(),
            job,
            observer: self.cancel_observer.clone(),
        })
    }
    fn console_values(&self, _: &str, _: &[crate::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "owned-test".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
    }
    fn report_raw_exception(
        &self,
        _: &crate::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.errors.borrow_mut().push(error.to_string());
        self.trace.borrow_mut().push("report".into());
        if let Some(observer) = self.report_observer.borrow().as_ref() {
            observer();
        }
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker request")
    }
}
struct SleepRegistration(Rc<Cell<usize>>);
impl Drop for SleepRegistration {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}
struct Capture(Rc<()>);
impl Drop for Capture {
    fn drop(&mut self) {
        assert!(Rc::strong_count(&self.0) >= 1);
    }
}
fn pending_capture(value: Rc<()>) -> Work {
    let capture = Capture(value);
    Box::pin(async move {
        std::future::pending::<()>().await;
        drop(capture);
    })
}

#[test]
fn cancellation_removes_registration_before_host_and_releases_unpolled_capture() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    let weak_owner = Rc::downgrade(&owner);
    *runtime.cancel_observer.borrow_mut() = Some(Box::new(move || {
        assert_eq!(
            weak_owner.upgrade().unwrap().len(),
            0,
            "owned cancellation removes registry before host cancellation"
        );
    }));
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let operation = owner.start(&runtime, |_| pending_capture(capture), |_| {});
    assert_eq!(owner.len(), 1);
    operation.cancel();
    assert!(!operation.is_active());
    assert!(
        weak.upgrade().is_none(),
        "owned cancellation releases unpolled captured work"
    );
}

#[test]
fn handle_drop_keeps_siblings_but_owner_drop_cancels_pending_work() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    let first = Rc::new(());
    let first_weak = Rc::downgrade(&first);
    let sibling = Rc::new(());
    let sibling_weak = Rc::downgrade(&sibling);
    let a = owner.start(&runtime, |_| pending_capture(first), |_| {});
    let b = owner.start(&runtime, |_| pending_capture(sibling), |_| {});
    runtime.pump();
    drop(b);
    a.cancel();
    assert!(
        first_weak.upgrade().is_none(),
        "cancel one releases only that task"
    );
    assert!(
        sibling_weak.upgrade().is_some(),
        "dropping returned handle must retain sibling work"
    );
    assert_eq!(owner.len(), 1);
    drop(owner);
    assert!(
        sibling_weak.upgrade().is_none(),
        "dropping actual owner cancels sibling work"
    );
}

#[test]
fn inline_completion_and_inline_close_never_install_stale_handles() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    runtime.inline.set(true);
    let operation = owner.start(&runtime, |_| Box::pin(async {}), |_| {});
    assert!(!operation.is_active());
    assert_eq!(owner.len(), 0);
    let weak = Rc::downgrade(&owner);
    let capture = Rc::new(());
    let capture_weak = Rc::downgrade(&capture);
    let operation = owner.start(
        &runtime,
        |_| {
            Box::pin(async move {
                let capture = Capture(capture);
                weak.upgrade().unwrap().close();
                std::future::pending::<()>().await;
                drop(capture);
            })
        },
        |_| {},
    );
    assert!(!operation.is_active());
    assert_eq!(owner.len(), 0);
    assert!(
        capture_weak.upgrade().is_none(),
        "inline disposal drops pending body before spawn returns"
    );
}

#[test]
fn cleanup_can_replace_task_without_registry_borrow_or_clobber() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let weak = Rc::downgrade(&owner);
    let host = runtime.clone();
    let replacement = Rc::new(RefCell::new(None));
    let saved = replacement.clone();
    let operation = owner.start(
        runtime.as_ref(),
        |_| Box::pin(std::future::pending()),
        move |_| {
            *saved.borrow_mut() = Some(weak.upgrade().unwrap().start(
                host.as_ref(),
                |_| Box::pin(std::future::pending()),
                |_| {},
            ));
        },
    );
    operation.cancel();
    operation.cancel();
    assert_eq!(
        owner.len(),
        1,
        "old cancellation preserves replacement registration"
    );
    assert!(replacement.borrow().as_ref().unwrap().is_active());
    owner.close();
    assert_eq!(owner.len(), 0);
}

#[test]
fn interval_uses_monotonic_capture_before_callback_and_yields_when_overdue() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let records = seen.clone();
    let clock = runtime.mono.clone();
    let operation = owner
        .timer(runtime.clone(), 10, true, move || {
            records.borrow_mut().push(clock.get());
            clock.set(clock.get() + 25.0);
            Ok(())
        })
        .unwrap();
    runtime.pump();
    runtime.wall.set(1_000_000.0);
    runtime.pump();
    assert!(
        seen.borrow().is_empty(),
        "wall jump cannot fire monotonic interval"
    );
    runtime.mono.set(10.0);
    runtime.pump();
    assert_eq!(*seen.borrow(), vec![10.0]);
    assert_eq!(
        *runtime.delays.borrow(),
        vec![10, 0],
        "next sleep derives from pre-callback capture, not completion"
    );
    runtime.wall.set(-1_000_000.0);
    runtime.pump();
    assert_eq!(
        *seen.borrow(),
        vec![10.0, 35.0],
        "slow callback rebases interval at next actual pre-callback capture"
    );
    operation.cancel();
    assert_eq!(owner.len(), 0);
}

#[test]
fn interval_self_cancel_during_callback_prevents_next_sleep() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let handle = Rc::new(RefCell::new(None::<OwnedOperation>));
    let callback_handle = handle.clone();
    let operation = owner
        .timer(runtime.clone(), 5, true, move || {
            callback_handle.borrow().as_ref().unwrap().cancel();
            Ok(())
        })
        .unwrap();
    *handle.borrow_mut() = Some(operation);
    runtime.pump();
    runtime.mono.set(5.0);
    runtime.pump();
    assert_eq!(
        owner.len(),
        0,
        "self cancellation removes actual interval registration"
    );
    assert_eq!(
        *runtime.delays.borrow(),
        vec![5],
        "cancelled callback must not schedule another sleep"
    );
}

#[test]
fn interval_reports_callback_error_and_retains_next_tick() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let operation = owner
        .timer(runtime.clone(), 5, true, || {
            Err(ExchangeError::new("callback failed"))
        })
        .unwrap();
    runtime.pump();
    runtime.mono.set(5.0);
    runtime.pump();
    assert_eq!(*runtime.errors.borrow(), vec!["callback failed"]);
    assert!(
        operation.is_active(),
        "reported interval callback failure retains reinsertion"
    );
    operation.cancel();
}

#[test]
fn exchange_owned_sibling_survives_outer_handle_and_disposal_releases_it() {
    use crate::exchange::{BaseExchange, ExchangeSettings};
    use crate::native_store::DefaultStore;
    use safe_cex_types::ExchangeName;
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoin,
        serde_json::from_value(serde_json::json!({"key":"k", "secret":"s"})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let release = Rc::new(Cell::new(false));
    let gate = release.clone();
    let effects = Rc::new(Cell::new(0));
    let count = effects.clone();
    let sibling = base.start_owned_operation(async move {
        let capture = Capture(capture);
        std::future::poll_fn(|_| {
            if gate.get() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await;
        count.set(count.get() + 1);
        drop(capture);
    });
    runtime.pump();
    drop(sibling);
    assert!(
        weak.upgrade().is_some(),
        "outer return does not cancel exchange-owned sibling"
    );
    release.set(true);
    runtime.pump();
    assert_eq!(
        effects.get(),
        1,
        "released sibling performs effects after outer return"
    );
    assert!(weak.upgrade().is_none());
    let capture = Rc::new(());
    let weak = Rc::downgrade(&capture);
    let pending = base.start_owned_operation(pending_capture(capture));
    base.dispose().unwrap();
    assert!(!pending.is_active());
    assert!(
        weak.upgrade().is_none(),
        "exchange disposal releases outstanding aggregation captures"
    );
}

#[test]
fn exchange_dispose_error_still_cancels_every_owned_task_and_rejects_new_work() {
    use crate::exchange::{BaseExchange, ExchangeSettings};
    use crate::native_store::DefaultStore;
    use safe_cex_types::ExchangeName;
    use std::sync::Arc;
    for poll_before_dispose in [false, true] {
        let runtime = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        let base = BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let _unsubscribe = store.subscribe(Arc::new(|_, _| {
            Err(safe_cex_types::StoreSubscriberError::with_response(
                "reset subscriber failed",
                serde_json::json!({"code": "reset-failure"}),
            ))
        }));
        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let work = base.start_owned_operation(pending_capture(capture));
        let ticks = Rc::new(Cell::new(0));
        let count = ticks.clone();
        let interval = base
            .start_interval(5, move || {
                count.set(count.get() + 1);
                Ok(())
            })
            .unwrap();
        let timeout = base
            .start_timeout(5, || panic!("disposed timeout fired"))
            .unwrap();
        if poll_before_dispose {
            runtime.pump();
        }
        let error = base.dispose().unwrap_err();
        assert_eq!(
            error.message, "reset subscriber failed",
            "DISPOSE-ERROR reset failure reaches caller"
        );
        assert_eq!(
            error.response_json_view(),
            Some(serde_json::json!({"code": "reset-failure"})),
            "DISPOSE-ERROR failure metadata reaches caller"
        );
        assert!(base.is_disposed());
        assert!(
            !work.is_active() && !interval.is_active() && !timeout.is_active(),
            "DISPOSE-ERROR all owned registrations cancelled before returning error"
        );
        assert!(
            weak.upgrade().is_none(),
            "DISPOSE-ERROR unpolled and pending captures released"
        );
        let capture = Rc::new(());
        let weak = Rc::downgrade(&capture);
        let late = base.start_owned_operation(pending_capture(capture));
        assert!(
            !late.is_active() && weak.upgrade().is_none(),
            "DISPOSE-ERROR closed owner rejects and drops new work"
        );
        runtime.mono.set(20.0);
        runtime.pump();
        assert_eq!(
            ticks.get(),
            0,
            "DISPOSE-ERROR cancelled interval never fires"
        );
        assert!(
            runtime
                .jobs
                .borrow()
                .iter()
                .all(|job| job.borrow().is_none()),
            "DISPOSE-ERROR host futures released"
        );
        assert_eq!(
            base.dispose().unwrap_err().message,
            "reset subscriber failed"
        );
    }
}

#[test]
fn exchange_listener_error_rearms_interval_and_releases_timeout() {
    use crate::exchange::{BaseExchange, ExchangeSettings};
    use crate::native_store::DefaultStore;
    use safe_cex_types::ExchangeName;
    for repeat in [false, true] {
        let runtime = Rc::new(Runtime::default());
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(serde_json::json!({"key":"", "secret":""})).unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        ));
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        base.on(
            "fill",
            Rc::new(move |_, _| {
                count.set(count.get() + 1);
                Err(ExchangeError::new("fill listener failed"))
            }),
        );
        let weak_base = Rc::downgrade(&base);
        let capture = Rc::new(());
        let weak_capture = Rc::downgrade(&capture);
        let callback = move || {
            let _capture = &capture;
            weak_base
                .upgrade()
                .unwrap()
                .emit("fill", crate::js_runtime::value::Js::Null)
        };
        let operation = if repeat {
            base.start_interval(5, callback)
        } else {
            base.start_timeout(5, callback)
        }
        .unwrap();
        runtime.pump();
        runtime.mono.set(5.0);
        runtime.pump();
        assert_eq!(
            calls.get(),
            1,
            "LISTENER-TIMER callback reached native listener"
        );
        assert_eq!(
            *runtime.errors.borrow(),
            ["fill listener failed"],
            "LISTENER-TIMER failure visibly reported exactly once"
        );
        assert_eq!(
            operation.is_active(),
            repeat,
            "LISTENER-TIMER interval rearmed and timeout unregistered after listener error"
        );
        assert_eq!(
            weak_capture.upgrade().is_some(),
            repeat,
            "LISTENER-TIMER completed timeout releases callback capture"
        );
        runtime.mono.set(10.0);
        runtime.pump();
        assert_eq!(
            calls.get(),
            if repeat { 2 } else { 1 },
            "LISTENER-TIMER interval survives error and timeout stays completed"
        );
        base.dispose().unwrap();
        assert!(
            !operation.is_active() && weak_capture.upgrade().is_none(),
            "LISTENER-TIMER disposal releases rearmed interval"
        );
    }
}

#[test]
fn actual_node_interval_reinsertion_corpus() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/owned_timer_source.cjs"
        ))
        .output()
        .expect("run one Node process for the interval corpus");
    assert!(
        output.status.success(),
        "actual Node interval oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        5,
        "NODE-INTERVAL complete actual-source corpus"
    );
    assert!(!corpus["architecture"].as_str().unwrap().is_empty());
    assert!(!corpus["node"].as_str().unwrap().is_empty());
    assert_eq!(corpus["sourceSha256"].as_str().unwrap().len(), 64);
    for row in corpus["rows"].as_array().unwrap() {
        let owner = Rc::new(OwnedTasks::default());
        let runtime = Rc::new(Runtime::default());
        let captured = row["captured"].as_f64().unwrap();
        let period = row["period"].as_u64().unwrap();
        let cost = row["cost"].as_f64().unwrap();
        runtime.mono.set(captured - period as f64);
        let clock = runtime.mono.clone();
        let cancels = row["cancel"].as_bool().unwrap_or(false);
        let throws = row["throws"].as_bool().unwrap_or(false);
        let handle = Rc::new(RefCell::new(None::<OwnedOperation>));
        let callback_handle = handle.clone();
        let trace = runtime.trace.clone();
        let operation = owner
            .timer(runtime.clone(), period, true, move || {
                trace.borrow_mut().push("callback".into());
                clock.set(clock.get() + cost);
                if cancels {
                    callback_handle.borrow().as_ref().unwrap().cancel();
                }
                if throws {
                    Err(ExchangeError::new("source callback failure"))
                } else {
                    Ok(())
                }
            })
            .unwrap();
        *handle.borrow_mut() = Some(operation.clone());
        runtime.pump();
        runtime.trace.borrow_mut().clear();
        runtime.mono.set(captured);
        runtime.pump();
        assert_eq!(
            operation.is_active(),
            !row["inserted"].is_null(),
            "NODE-INTERVAL native reinsertion identity: {}",
            row["name"]
        );
        if let Some(deadline) = row["inserted"]["deadline"].as_f64() {
            assert_eq!(
                *runtime.delays.borrow().last().unwrap(),
                (deadline - row["afterCallback"].as_f64().unwrap()).max(0.0) as u64,
                "NODE-INTERVAL native pre-callback deadline: {}",
                row["name"]
            );
        }
        assert_eq!(
            runtime.errors.borrow().first().map(String::as_str),
            row["error"].as_str(),
            "NODE-INTERVAL native callback error: {}",
            row["name"]
        );
        assert_eq!(
            serde_json::json!(*runtime.trace.borrow()),
            row["trace"],
            "NODE-INTERVAL callback rearm report order: {}",
            row["name"]
        );
        operation.cancel();
    }
}

#[test]
fn callback_reporter_sees_rearmed_interval_and_can_cancel_it() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let trace = runtime.trace.clone();
    let operation = owner
        .timer(runtime.clone(), 5, true, move || {
            trace.borrow_mut().push("callback".into());
            Err(ExchangeError::new("callback failed"))
        })
        .unwrap();
    let weak_runtime = Rc::downgrade(&runtime);
    let cancel = operation.clone();
    *runtime.report_observer.borrow_mut() = Some(Box::new(move || {
        let runtime = weak_runtime.upgrade().unwrap();
        assert_eq!(
            *runtime.trace.borrow(),
            vec!["callback", "rearm", "report"],
            "INTERVAL-ORDER rearm precedes reentrant reporter"
        );
        assert_eq!(
            *runtime.delays.borrow(),
            vec![5, 5],
            "INTERVAL-ORDER next sleep exists during report"
        );
        assert_eq!(
            runtime.registered_sleeps.get(),
            1,
            "INTERVAL-ORDER host timer registered before reporter"
        );
        cancel.cancel();
    }));
    runtime.pump();
    runtime.trace.borrow_mut().clear();
    runtime.mono.set(5.0);
    runtime.pump();
    assert!(
        !operation.is_active(),
        "INTERVAL-ORDER reporter cancellation removes registration"
    );
    assert_eq!(owner.len(), 0);
    assert_eq!(
        runtime.registered_sleeps.get(),
        0,
        "INTERVAL-ORDER reporter cancellation releases primed host timer"
    );
    runtime.mono.set(10.0);
    runtime.pump();
    assert_eq!(
        runtime.errors.borrow().len(),
        1,
        "INTERVAL-ORDER reporter cancellation prevents subsequent callback"
    );
}

#[test]
fn shared_result_outlives_dropped_waiter_and_is_cached() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    let operation = crate::shared_operation::start(&owner, &runtime, |_| async { Ok(83_u64) });
    let mut first = operation.wait();
    let mut second = operation.wait();
    #[derive(Default)]
    struct Counter(std::sync::atomic::AtomicUsize);
    impl std::task::Wake for Counter {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let counter = std::sync::Arc::new(Counter::default());
    let waker = Waker::from(counter.clone());
    let mut cx = Context::from_waker(&waker);
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    drop(first);
    assert!(
        operation.is_active(),
        "shared_native_waiter_drop_retains_work"
    );
    runtime.pump();
    assert_eq!(
        counter.0.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "shared_native_same_waker_drop_preserves_sibling"
    );
    assert_eq!(
        second.as_mut().poll(&mut cx),
        Poll::Ready(Ok(83)),
        "shared_native_joined_result"
    );
    assert!(
        !operation.is_active(),
        "shared_native_completed_registration_removed"
    );
    operation.cancel();
    assert_eq!(
        operation.wait().as_mut().poll(&mut cx),
        Poll::Ready(Ok(83)),
        "shared_native_cached_result"
    );
}
#[test]
fn shared_result_owner_cancels_unpolled_work_visibly() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    let polled = Rc::new(Cell::new(false));
    let sentinel = Rc::new(());
    let weak = Rc::downgrade(&sentinel);
    let operation = crate::shared_operation::start(&owner, &runtime, {
        let polled = polled.clone();
        move |_| async move {
            let _retained = sentinel;
            polled.set(true);
            std::future::pending::<Result<u64, ExchangeError>>().await
        }
    });
    let mut waiter = operation.wait();
    let mut cx = Context::from_waker(Waker::noop());
    assert!(waiter.as_mut().poll(&mut cx).is_pending());
    owner.close();
    assert!(!polled.get(), "shared_native_unpolled_work_stays_unpolled");
    assert!(
        weak.upgrade().is_none(),
        "shared_native_cancel_releases_work"
    );
    let Poll::Ready(Err(error)) = waiter.as_mut().poll(&mut cx) else {
        panic!("shared_native_visible_cancellation")
    };
    assert_eq!(
        error.message, "Shared operation cancelled by its owner",
        "shared_native_cancellation_cause"
    );
    assert_eq!(owner.len(), 0, "shared_native_cancel_clears_registry");
}
#[test]
fn shared_result_closed_owner_skips_builder_and_returns_cancelled() {
    let owner = Rc::new(OwnedTasks::default());
    owner.close();
    let runtime = Runtime::default();
    let called = Rc::new(Cell::new(false));
    let operation = crate::shared_operation::start(&owner, &runtime, {
        let called = called.clone();
        move |_| {
            called.set(true);
            async { Ok(89_u64) }
        }
    });
    assert!(!called.get(), "shared_native_closed_owner_skips_builder");
    assert!(
        !operation.is_active(),
        "shared_native_closed_owner_inactive"
    );
    let mut waiter = operation.wait();
    assert!(
        matches!(
            waiter
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(_))
        ),
        "shared_native_closed_owner_visible_error"
    );
}
#[test]
fn shared_result_publishes_handle_before_inline_poll() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime {
        inline: Cell::new(true),
        ..Default::default()
    };
    let published = Rc::new(RefCell::new(None::<crate::SharedOperation<u64>>));
    let operation = crate::shared_operation::start(&owner, &runtime, {
        let published = published.clone();
        move |operation| {
            *published.borrow_mut() = Some(operation);
            async move {
                assert!(
                    published.borrow().as_ref().unwrap().is_active(),
                    "shared_native_handle_precedes_inline_poll"
                );
                Ok(97)
            }
        }
    });
    assert!(
        !operation.is_active(),
        "shared_native_inline_completion_inactive"
    );
    assert_eq!(
        operation
            .wait()
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(97)),
        "shared_native_inline_completion_retained"
    );
}

#[test]
fn shared_result_owner_drop_cancels_without_a_retained_owner() {
    let owner = Rc::new(OwnedTasks::default());
    let weak_owner = Rc::downgrade(&owner);
    let runtime = Runtime::default();
    let operation = crate::shared_operation::start(&owner, &runtime, {
        let weak_owner = weak_owner.clone();
        move |_| async move {
            assert!(
                weak_owner.upgrade().is_some(),
                "shared_native_owner_live_before_drop"
            );
            std::future::pending::<Result<u64, ExchangeError>>().await
        }
    });
    runtime.pump();
    drop(owner);
    assert!(
        weak_owner.upgrade().is_none(),
        "shared_native_owner_not_retained"
    );
    assert!(
        !operation.is_active(),
        "shared_native_dropped_owner_inactive"
    );
    assert!(
        matches!(
            operation
                .wait()
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Err(_))
        ),
        "shared_native_owner_drop_visible_cancellation"
    );
}

#[derive(Default)]
struct SharedWakeCounter(std::sync::atomic::AtomicUsize);
impl std::task::Wake for SharedWakeCounter {
    fn wake(self: std::sync::Arc<Self>) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

#[test]
fn shared_result_waiter_drop_releases_its_waker_before_task_completion() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Runtime::default();
    let operation = crate::shared_operation::start(&owner, &runtime, |_| async { Ok(89_u64) });
    let mut cancelled = operation.wait();
    let counter = std::sync::Arc::new(SharedWakeCounter::default());
    let weak = std::sync::Arc::downgrade(&counter);
    let waker = Waker::from(counter.clone());
    assert!(
        cancelled
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(counter);
    drop(waker);
    assert!(
        weak.upgrade().is_some(),
        "shared_native_pending_waiter_retains_waker"
    );
    drop(cancelled);
    assert!(
        weak.upgrade().is_none(),
        "shared_native_waiter_drop_releases_waker_immediately"
    );
    assert!(
        operation.is_active(),
        "shared_native_waker_release_precedes_task_end"
    );
    assert_eq!(
        owner.len(),
        1,
        "shared_native_task_registry_survives_waiter_drop"
    );
    runtime.pump();
    assert_eq!(
        operation
            .wait()
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(89)),
        "shared_native_waiter_drop_keeps_result"
    );
}

#[test]
fn shared_result_runtime_task_drop_settles_waiters_with_owner_still_live() {
    for poll_task in [false, true] {
        let owner = Rc::new(OwnedTasks::default());
        let runtime = Runtime::default();
        let sentinel = Rc::new(());
        let weak = Rc::downgrade(&sentinel);
        let operation = crate::shared_operation::start(&owner, &runtime, move |_| async move {
            let _retained = sentinel;
            std::future::pending::<Result<u64, ExchangeError>>().await
        });
        let mut waiter = operation.wait();
        let counter = std::sync::Arc::new(SharedWakeCounter::default());
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(waiter.as_mut().poll(&mut cx).is_pending());
        if poll_task {
            runtime.pump();
        }
        assert!(operation.is_active(), "shared_native_discard_starts_live");
        assert!(
            weak.upgrade().is_some(),
            "shared_native_discard_starts_retained"
        );
        let job = runtime.jobs.borrow()[0].clone();
        let task = job.borrow_mut().take().unwrap();
        drop(task);
        assert!(
            weak.upgrade().is_none(),
            "shared_native_task_discard_releases_capture {poll_task}"
        );
        assert!(
            !operation.is_active(),
            "shared_native_task_discard_inactive {poll_task}"
        );
        assert_eq!(
            owner.len(),
            0,
            "shared_native_task_discard_clears_registry {poll_task}"
        );
        assert_eq!(
            counter.0.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "shared_native_task_discard_wakes_waiter_once {poll_task}"
        );
        let Poll::Ready(Err(error)) = waiter.as_mut().poll(&mut cx) else {
            panic!("shared_native_task_discard_visible_error {poll_task}");
        };
        assert_eq!(
            error.message, "Shared operation cancelled by its owner",
            "shared_native_task_discard_retains_cause {poll_task}"
        );
        let next = crate::shared_operation::start(&owner, &runtime, |_| async { Ok(97_u64) });
        runtime.pump();
        assert_eq!(
            next.wait().as_mut().poll(&mut cx),
            Poll::Ready(Ok(97)),
            "shared_native_task_discard_keeps_owner_open {poll_task}"
        );
    }
}

#[test]
fn detached_pending_work_retains_owner_until_completion() {
    let runtime = Rc::new(Runtime::default());
    runtime.cancel_handle_drop.set(true);
    let ready = Rc::new(Cell::new(false));
    let gate = ready.clone();
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let operation = OwnedTasks::start_detached(
        runtime.as_ref(),
        Box::pin(async move {
            std::future::poll_fn(move |_| {
                if gate.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            called.set(called.get() + 1);
        }),
    );
    let owner = operation.owner.clone();
    assert!(
        owner.upgrade().is_some(),
        "native detached task owns its pending registry"
    );
    runtime.pump();
    assert_eq!(
        calls.get(),
        0,
        "native pending detached work has not completed"
    );
    drop(operation);
    assert!(
        owner.upgrade().is_some(),
        "native pending detached owner survives caller handle drop"
    );
    ready.set(true);
    runtime.pump();
    assert_eq!(
        calls.get(),
        1,
        "native detached pending work completes once"
    );
    assert!(
        owner.upgrade().is_none(),
        "native detached completion releases registry owner"
    );
}

#[test]
fn detached_inline_ready_releases_owner_before_spawn_returns() {
    let runtime = Rc::new(Runtime::default());
    runtime.inline.set(true);
    runtime.cancel_handle_drop.set(true);
    let calls = Rc::new(Cell::new(0));
    let called = calls.clone();
    let operation = OwnedTasks::start_detached(
        runtime.as_ref(),
        Box::pin(async move {
            called.set(called.get() + 1);
        }),
    );
    assert_eq!(
        calls.get(),
        1,
        "native detached inline task completes during spawn"
    );
    assert!(
        operation.owner.upgrade().is_none(),
        "native inline completed detached owner is released"
    );
    assert!(
        !operation.is_active(),
        "native inline completion installs no stale handle"
    );
    runtime.pump();
    assert_eq!(calls.get(), 1, "native inline completion is not repolled");
}

#[test]
fn detached_host_discard_before_poll_releases_owner_and_captured_value() {
    struct Captured(Rc<Cell<usize>>);
    impl Drop for Captured {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let runtime = Rc::new(Runtime::default());
    runtime.discard_spawn.set(true);
    runtime.cancel_handle_drop.set(true);
    let drops = Rc::new(Cell::new(0));
    let captured = Captured(drops.clone());
    let operation = OwnedTasks::start_detached(
        runtime.as_ref(),
        Box::pin(async move {
            let _captured = captured;
            std::future::pending::<()>().await;
        }),
    );
    assert_eq!(
        drops.get(),
        1,
        "native host discard drops unpolled captured value once"
    );
    assert!(
        operation.owner.upgrade().is_none(),
        "native host discard releases detached registry"
    );
    assert!(
        !operation.is_active(),
        "native discarded host task installs no registration"
    );
}

#[test]
fn detached_completed_driver_retention_does_not_retain_owner() {
    let runtime = Rc::new(Runtime::default());
    runtime.retain_ready.set(true);
    runtime.cancel_handle_drop.set(true);
    let operation = OwnedTasks::start_detached(runtime.as_ref(), Box::pin(async {}));
    let owner = operation.owner.clone();
    assert!(
        owner.upgrade().is_some(),
        "native queued detached owner exists before poll"
    );
    runtime.pump();
    assert_eq!(
        runtime.retained.borrow().len(),
        1,
        "native host deliberately retains completed driver"
    );
    assert!(
        owner.upgrade().is_none(),
        "native completed driver releases detached lease before host drop"
    );
    runtime.retained.borrow_mut().clear();
}

#[test]
fn primed_ready_sleep_is_not_repolled_and_callback_yields() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let clock = runtime.mono.clone();
    let stop = Rc::downgrade(&owner);
    let operation = owner
        .timer(runtime.clone(), 5, true, move || {
            seen.set(seen.get() + 1);
            clock.set(clock.get() + 5.0);
            if seen.get() == 2 {
                stop.upgrade().unwrap().close();
            }
            Ok(())
        })
        .unwrap();
    runtime.pump();
    let mut completed = false;
    *runtime.next_sleep.borrow_mut() = Some(Box::pin(std::future::poll_fn(move |_| {
        assert!(
            !completed,
            "INTERVAL-READY completed sleep must not be repolled"
        );
        completed = true;
        Poll::Ready(Ok(()))
    })));
    runtime.mono.set(5.0);
    runtime.pump();
    assert_eq!(
        calls.get(),
        1,
        "INTERVAL-READY next callback requires a scheduler turn"
    );
    runtime.pump();
    assert_eq!(calls.get(), 2, "INTERVAL-READY next turn runs callback");
    assert!(!operation.is_active());
}

#[test]
fn primed_sleep_failure_follows_callback_failure_and_removes_task() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let operation = owner
        .timer(runtime.clone(), 5, true, || {
            Err(ExchangeError::new("callback failure"))
        })
        .unwrap();
    runtime.pump();
    *runtime.next_sleep.borrow_mut() =
        Some(Box::pin(async { Err(ExchangeError::new("sleep failure")) }));
    runtime.mono.set(5.0);
    runtime.pump();
    assert_eq!(
        *runtime.errors.borrow(),
        vec!["callback failure", "sleep failure"],
        "INTERVAL-READY callback failure precedes sleep failure"
    );
    assert!(
        !operation.is_active(),
        "INTERVAL-READY failed sleep removes task"
    );
    assert_eq!(owner.len(), 0);
}

#[test]
fn priming_poll_cancellation_does_not_reinsert_task() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let operation = owner
        .timer(runtime.clone(), 5, true, move || {
            seen.set(seen.get() + 1);
            Ok(())
        })
        .unwrap();
    runtime.pump();
    let cancel = operation.clone();
    *runtime.next_sleep.borrow_mut() = Some(Box::pin(std::future::poll_fn(move |_| {
        cancel.cancel();
        Poll::Pending
    })));
    runtime.mono.set(5.0);
    runtime.pump();
    assert!(
        !operation.is_active(),
        "INTERVAL-PRIME cancellation removes registration"
    );
    assert_eq!(owner.len(), 0);
    runtime.mono.set(10.0);
    runtime.pump();
    assert_eq!(
        calls.get(),
        1,
        "INTERVAL-PRIME cancellation prevents later callback"
    );
}

#[test]
fn detached_timeout_survives_an_unrelated_disposal_and_handle_drop() {
    let runtime = Rc::new(Runtime::default());
    runtime.cancel_handle_drop.set(true);
    let unrelated = Rc::new(OwnedTasks::default());
    let count = Rc::new(Cell::new(0));
    let captured = Rc::new(());
    let weak = Rc::downgrade(&captured);
    let called = count.clone();
    let operation = start_detached_timeout(runtime.clone(), 250, move || {
        assert_eq!(Rc::strong_count(&captured), 1);
        called.set(called.get() + 1);
        Ok(())
    })
    .unwrap();
    unrelated.close();
    drop(operation);
    runtime.pump();
    assert!(
        weak.upgrade().is_some(),
        "detached_timer_native_capture_pending"
    );
    runtime.mono.set(250.);
    runtime.pump();
    assert_eq!(count.get(), 1, "detached_timer_native_after_disposal");
    assert!(
        weak.upgrade().is_none(),
        "detached_timer_native_capture_released"
    );
    runtime.pump();
    assert_eq!(count.get(), 1, "detached_timer_native_exactly_once");
}

#[test]
fn cancelling_detached_timeout_releases_its_sleep_and_callback() {
    let runtime = Rc::new(Runtime::default());
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    let captured = Rc::new(());
    let weak = Rc::downgrade(&captured);
    let operation = start_detached_timeout(runtime.clone(), 250, move || {
        observed.set(true);
        assert_eq!(Rc::strong_count(&captured), 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(
        runtime.registered_sleeps.get(),
        0,
        "detached_timer_native_queued_before_poll"
    );
    runtime.pump();
    assert_eq!(
        runtime.registered_sleeps.get(),
        1,
        "detached_timer_native_sleep_after_poll"
    );
    operation.cancel();
    assert_eq!(
        runtime.registered_sleeps.get(),
        0,
        "detached_timer_native_cancel_sleep"
    );
    assert!(
        weak.upgrade().is_none(),
        "detached_timer_native_cancel_capture"
    );
    runtime.mono.set(250.);
    runtime.pump();
    assert!(!called.get(), "detached_timer_native_cancel_no_callback");
}

#[test]
fn eager_registry_starts_now_and_cancel_releases_pending_body_before_pump() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let host: Rc<dyn ExchangeRuntime> = runtime.clone();
    let prefix = Rc::new(Cell::new(false));
    let captured = Rc::new(());
    let weak = Rc::downgrade(&captured);
    let seen = prefix.clone();
    let cleanup = Rc::new(Cell::new(0));
    let cleaned = cleanup.clone();
    let operation = owner.start_eager(
        &host,
        move |operation| {
            Box::pin(async move {
                assert!(
                    operation.is_active(),
                    "eager body is registered before prefix"
                );
                seen.set(true);
                std::future::pending::<()>().await;
                drop(captured);
            })
        },
        move |cancelled| {
            assert!(cancelled);
            cleaned.set(cleaned.get() + 1);
        },
    );
    assert!(prefix.get(), "eager registry prefix before runtime polling");
    assert!(weak.upgrade().is_some());
    assert_eq!(owner.len(), 1);
    operation.cancel();
    assert!(
        weak.upgrade().is_none(),
        "eager cancellation synchronously drops pending body"
    );
    assert_eq!(cleanup.get(), 1);
    assert_eq!(owner.len(), 0);
    runtime.pump();
    assert_eq!(cleanup.get(), 1);
    #[cfg(all(feature = "native", not(target_arch = "wasm32")))]
    assert_eq!(
        crate::js_runtime::jobs::pending_non_timer(host),
        0,
        "eager cancelled driver is reaped after wake"
    );
}

#[test]
fn eager_registry_inline_close_and_ready_completion_leave_no_stale_slot() {
    for inline in [false, true] {
        let owner = Rc::new(OwnedTasks::default());
        let runtime = Rc::new(Runtime::default());
        runtime.inline.set(inline);
        let host: Rc<dyn ExchangeRuntime> = runtime.clone();
        let operation = owner.start_eager(&host, |_| Box::pin(async {}), |_| {});
        assert!(!operation.is_active());
        assert_eq!(owner.len(), 0);
        let weak_owner = Rc::downgrade(&owner);
        let captured = Rc::new(());
        let weak = Rc::downgrade(&captured);
        let operation = owner.start_eager(
            &host,
            move |_| {
                Box::pin(async move {
                    weak_owner.upgrade().unwrap().close();
                    std::future::pending::<()>().await;
                    drop(captured);
                })
            },
            |_| {},
        );
        assert!(!operation.is_active());
        assert!(
            weak.upgrade().is_none(),
            "reentrant close drops running eager body"
        );
        assert_eq!(owner.len(), 0);
        runtime.pump();
    }
}

#[test]
fn eager_registry_host_discard_releases_registration_and_pending_capture() {
    let owner = Rc::new(OwnedTasks::default());
    let runtime = Rc::new(Runtime::default());
    let host: Rc<dyn ExchangeRuntime> = runtime.clone();
    let captured = Rc::new(());
    let weak = Rc::downgrade(&captured);
    let operation = owner.start_eager(&host, |_| pending_capture(captured), |_| {});
    assert!(operation.is_active());
    let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
    for job in jobs {
        let future = job.borrow_mut().take();
        drop(future);
    }
    assert!(
        !operation.is_active(),
        "discard of shared producer driver releases slot"
    );
    assert!(weak.upgrade().is_none());
    assert_eq!(owner.len(), 0);
}
