use safe_cex_exchange::runtime::*;
use safe_cex_venue_binance::{class_jobs::*, private_event_value::*};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
#[derive(Default)]
struct Runtime {
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    reported: RefCell<Vec<String>>,
}
impl Runtime {
    fn pump(&self) {
        let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn monotonic_now_ms(&self) -> f64 {
        1000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.tasks.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "jobs fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &serde_json::Value) {
        self.reported.borrow_mut().push(error.message.clone());
    }
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }
}
struct Counter(AtomicUsize);
impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn eager_prefix_shared_native_settlement_and_all_waiters() {
    let runtime = Rc::new(Runtime::default());
    let trace = Rc::new(RefCell::new(vec![]));
    let ready = Rc::new(Cell::new(false));
    let original = record([("value", number(4.2))]);
    let value = original.clone();
    let seen = trace.clone();
    let signal = ready.clone();
    let mut one = start(
        runtime.clone(),
        Box::pin(async move {
            seen.borrow_mut().push("prefix");
            poll_fn(move |_| {
                if signal.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            seen.borrow_mut().push("settled");
            Ok(value)
        }),
    );
    assert_eq!(
        *trace.borrow(),
        ["prefix"],
        "native class job starts synchronous prefix before returning"
    );
    assert_eq!(
        runtime.tasks.borrow().len(),
        1,
        "independent retained driver count"
    );
    let mut two = one.clone();
    let a = Arc::new(Counter(AtomicUsize::new(0)));
    let b = Arc::new(Counter(AtomicUsize::new(0)));
    let aw = Waker::from(a.clone());
    let bw = Waker::from(b.clone());
    assert!(
        Pin::new(&mut one)
            .poll(&mut Context::from_waker(&aw))
            .is_pending()
    );
    assert!(
        Pin::new(&mut two)
            .poll(&mut Context::from_waker(&bw))
            .is_pending()
    );
    ready.set(true);
    runtime.pump();
    assert_eq!(
        *trace.borrow(),
        ["prefix", "settled"],
        "native prefix runs once across all settlement handles"
    );
    assert_eq!(a.0.load(Ordering::SeqCst), 1, "native first waiter woken");
    assert_eq!(b.0.load(Ordering::SeqCst), 1, "native second waiter woken");
    for join in [&mut one, &mut two] {
        let Poll::Ready(Ok(value)) = Pin::new(join).poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("native completed settlement")
        };
        assert!(
            value.same(&original),
            "native shared original object result identity"
        );
        assert!(matches!(value.read("value").unwrap(),EventValue::Number(v)if v==4.2));
    }
    assert_eq!(
        runtime.tasks.borrow().len(),
        0,
        "independent completed driver count"
    );
}
#[test]
fn dropped_join_does_not_stop_driver_and_prefix_rejection_is_settlement() {
    let runtime = Rc::new(Runtime::default());
    let ran = Rc::new(Cell::new(0));
    let ready = Rc::new(Cell::new(false));
    let signal = ready.clone();
    let result = ran.clone();
    let join = start(
        runtime.clone(),
        Box::pin(async move {
            result.set(1);
            poll_fn(move |_| {
                if signal.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            result.set(2);
            Ok(number(3.6))
        }),
    );
    assert_eq!(ran.get(), 1, "native eager prefix before join drop");
    drop(join);
    ready.set(true);
    runtime.pump();
    assert_eq!(ran.get(), 2, "native discarded join retains suspended work");
    let primary = Rc::new(ExchangeError::new("class prefix fixture"));
    let e = primary.clone();
    let mut join = start(
        runtime.clone(),
        Box::pin(async move { Err::<EventValue, _>(e) }),
    );
    let Poll::Ready(Err(actual)) =
        Pin::new(&mut join).poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("native rejected settlement")
    };
    assert!(
        Rc::ptr_eq(&actual, &primary),
        "native async prefix rejection original identity"
    );
    assert_eq!(
        runtime.tasks.borrow().len(),
        1,
        "independent rejected settlement queues one observation checkpoint"
    );
    runtime.pump();
    assert!(
        runtime.reported.borrow().is_empty(),
        "native handled rejection suppresses queued report"
    );
}

#[test]
fn detached_rejection_reports_after_queued_reaction() {
    let runtime = Rc::new(Runtime::default());
    let error = Rc::new(ExchangeError::new("detached marker"));
    let work = start(
        runtime.clone(),
        Box::pin(async move { Err::<(), _>(error) }),
    );
    detach(runtime.clone(), work);
    assert!(
        runtime.reported.borrow().is_empty(),
        "native detached error waits for reaction"
    );
    runtime.pump();
    assert_eq!(
        *runtime.reported.borrow(),
        ["detached marker"],
        "native discarded rejection reaches host exactly once"
    );
    runtime.pump();
    assert_eq!(
        runtime.reported.borrow().len(),
        1,
        "native no duplicate detached report"
    );
}

#[test]
fn bare_dropped_rejected_settlement_reports_once_after_queue() {
    let runtime = Rc::new(Runtime::default());
    let error = Rc::new(ExchangeError::new("bare rejected marker"));
    drop(start(
        runtime.clone(),
        Box::pin(async move { Err::<(), _>(error) }),
    ));
    assert!(
        runtime.reported.borrow().is_empty(),
        "native bare rejection queues host report"
    );
    runtime.pump();
    assert_eq!(
        *runtime.reported.borrow(),
        ["bare rejected marker"],
        "native bare dropped rejection reports exactly once"
    );
    runtime.pump();
    assert_eq!(
        runtime.reported.borrow().len(),
        1,
        "native bare rejection does not double report"
    );
}

#[test]
fn retained_unpolled_rejection_reports_before_handle_release() {
    let runtime = Rc::new(Runtime::default());
    let error = Rc::new(ExchangeError::new("retained rejected marker"));
    let retained = start(
        runtime.clone(),
        Box::pin(async move { Err::<(), _>(error) }),
    );
    assert!(
        runtime.reported.borrow().is_empty(),
        "native retained rejection queues observation check"
    );
    runtime.pump();
    assert_eq!(
        *runtime.reported.borrow(),
        ["retained rejected marker"],
        "native retained unobserved rejection reports before release"
    );
    drop(retained);
    runtime.pump();
    assert_eq!(
        runtime.reported.borrow().len(),
        1,
        "native retained rejection not reported twice after release"
    );
}
