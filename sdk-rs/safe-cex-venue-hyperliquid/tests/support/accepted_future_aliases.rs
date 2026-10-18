//! Fixed production Js monomorphizations only; no decoder or other-host claim.
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use crate::{
    income::IncomeFuture,
    host::{Host, test_runtime::TestRuntime},
    spot_account_reads::AccountFuture,
    startup_timeout::TimeoutCallback,
};
use std::{
    any::TypeId,
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type NativeFuture = Pin<Box<dyn Future<Output = Result<Js, Thrown>>>>;
type NativeCallback = Rc<dyn Fn(Vec<Js>) -> Result<Js, Thrown>>;

struct CountWake(AtomicUsize);
impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
struct Controlled {
    output: Option<Result<Js, Thrown>>,
    polls: Rc<Cell<usize>>,
    drops: Rc<Cell<usize>>,
}
impl Future for Controlled {
    type Output = Result<Js, Thrown>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let previous = self.polls.replace(self.polls.get() + 1);
        if previous == 0 {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(self.output.take().expect("future polled after settlement"))
        }
    }
}
impl Drop for Controlled {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
fn require_spot<
    H: crate::spot_account_reads::SpotAccountHost
        + crate::ws_ping_lifecycle::WsPingHost<Value = Js, Error = Thrown>,
>() {
}
fn require_income<
    H: crate::income::IncomeHost + crate::cross_cache::CrossCacheHost<Value = Js, Error = Thrown>,
>() {
}
fn require_timeout<H: crate::startup_timeout::StartupTimeoutHost<Value = Js, Error = Thrown>>() {}
fn assert_native(value: &Js, expected: &Js) {
    match (value, expected) {
        (Js::Number(actual), Js::Number(wanted)) => {
            assert_eq!(actual.to_bits(), wanted.to_bits())
        }
        (Js::String(actual), Js::String(wanted)) => {
            assert_eq!(actual.as_ref(), wanted.as_ref())
        }
        _ => assert!(
            value.strict_equals(expected),
            "alias changed native identity or variant"
        ),
    }
}
fn future_native<A: 'static>(
    make: fn(NativeFuture) -> A,
    as_native: fn(&mut A) -> &mut NativeFuture,
) {
    assert_eq!(TypeId::of::<A>(), TypeId::of::<NativeFuture>());
    let realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
    let object = realm.object_from(vec![("marker", Js::Number(-0.0))]);
    let error = realm.type_error("future rejection");
    let values = [
        Js::Undefined,
        Js::Number(-0.0),
        Js::Number(f64::INFINITY),
        Js::String(Rc::from([0xd800_u16, 0x0061])),
        object,
        error.value,
    ];
    for reject in [false, true] {
        for expected in &values {
            let polls = Rc::new(Cell::new(0));
            let drops = Rc::new(Cell::new(0));
            let native: NativeFuture = Box::pin(Controlled {
                output: Some(if reject {
                    Err(Thrown::new(expected.clone()))
                } else {
                    Ok(expected.clone())
                }),
                polls: polls.clone(),
                drops: drops.clone(),
            });
            let mut alias = make(native);
            let wake = Arc::new(CountWake(AtomicUsize::new(0)));
            let waker = Waker::from(wake.clone());
            let mut context = Context::from_waker(&waker);
            assert!(
                as_native(&mut alias)
                    .as_mut()
                    .poll(&mut context)
                    .is_pending()
            );
            assert_eq!(polls.get(), 1);
            assert_eq!(wake.0.load(Ordering::SeqCst), 1);
            assert_eq!(drops.get(), 0);
            let Poll::Ready(result) = as_native(&mut alias).as_mut().poll(&mut context) else {
                panic!("alias lost ready settlement");
            };
            assert_eq!(polls.get(), 2);
            match (reject, result) {
                (false, Ok(value)) => assert_native(&value, expected),
                (true, Err(error)) => assert_native(&error.value, expected),
                _ => panic!("alias swapped fulfillment and rejection"),
            }
            drop(alias);
            assert_eq!(drops.get(), 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let alias = make(Box::pin(Controlled {
        output: Some(Ok(Js::Undefined)),
        polls: Rc::new(Cell::new(0)),
        drops: drops.clone(),
    }));
    drop(alias);
    assert_eq!(
        drops.get(),
        1,
        "unpolled alias drop must release its owned future"
    );
}
#[test]
fn account_future_fixed_value_preserves_poll_result_and_drop() {
    require_spot::<Host>();
    future_native::<AccountFuture<Js, Thrown>>(|value| value, |value| value);
}
#[test]
fn income_future_static_fixed_value_preserves_poll_result_and_drop() {
    require_income::<Host>();
    future_native::<IncomeFuture<'static, Js, Thrown>>(|value| value, |value| value);
}
#[test]
fn timeout_callback_fixed_value_preserves_arguments_result_and_identity() {
    require_timeout::<Host>();
    assert_eq!(
        TypeId::of::<TimeoutCallback<Js, Thrown>>(),
        TypeId::of::<NativeCallback>()
    );
    let realm = crate::host::test_runtime::host(Rc::new(TestRuntime::default()));
    let object = realm.object_from(vec![("marker", Js::Number(-0.0))]);
    let thrown = realm.type_error("callback rejection");
    let calls = Rc::new(RefCell::new(Vec::<Vec<Js>>::new()));
    let recorded = calls.clone();
    let success = object.clone();
    let failure = thrown.clone();
    let mut native: NativeCallback = Rc::new(move |args| {
        let reject = args.is_empty();
        recorded.borrow_mut().push(args);
        if reject {
            Err(failure.clone())
        } else {
            Ok(success.clone())
        }
    });
    let alias: &mut TimeoutCallback<Js, Thrown> = &mut native;
    let back: &mut NativeCallback = alias;
    let clone: TimeoutCallback<Js, Thrown> = back.clone();
    assert!(Rc::ptr_eq(back, &clone));
    let input = vec![
        Js::Number(-0.0),
        Js::Number(f64::INFINITY),
        Js::String(Rc::from([0xd800_u16, 0x0061])),
        object.clone(),
    ];
    let output = clone(input.clone()).unwrap();
    assert_native(&output, &object);
    let rejected = back(Vec::new()).unwrap_err();
    assert_native(&rejected.value, &thrown.value);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].len(), 4);
    for (actual, expected) in calls[0].iter().zip(&input) {
        assert_native(actual, expected);
    }
    assert!(calls[1].is_empty());
}
