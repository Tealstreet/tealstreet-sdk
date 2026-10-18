use safe_cex_exchange::{
    delivery_timers::WorkerPortTimers,
    js_runtime::value::Js,
    runtime::ExchangeError,
    store_delivery::{StoreUpdateThrottle, StoreUpdateThrottleHost},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Default)]
struct Host {
    now: Cell<f64>,
    next: Cell<usize>,
    timers: RefCell<Vec<(usize, Rc<dyn Fn()>)>>,
}
impl WorkerPortTimers for Host {
    type Timer = usize;
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn set_timeout(&self, callback: Rc<dyn Fn()>, _: f64) -> usize {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.timers.borrow_mut().push((id, callback));
        id
    }
    fn clear_timeout(&self, timer: &usize) {
        self.timers.borrow_mut().retain(|(id, _)| id != timer);
    }
    fn timer_is_truthy(&self, timer: &usize) -> bool {
        *timer != 0
    }
}
impl StoreUpdateThrottleHost for Host {
    fn report_store_timer_error(&self, error: &ExchangeError) {
        panic!("unexpected timer error: {error}");
    }
}

#[test]
fn native_argument_is_read_only_at_leading_and_trailing_invocations() {
    let host = Rc::new(Host::default());
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let row = Js::object(vec![("last", Js::Number(1.0))]);
    let value = row.clone();
    let state = Js::object(vec![]);
    state
        .define_accessor(
            "ticker",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(value.clone())
            })),
            None,
        )
        .unwrap();
    let output = Rc::new(RefCell::new(Vec::new()));
    let seen = output.clone();
    let expected = state.clone();
    let throttle = StoreUpdateThrottle::new_with_argument(
        host.clone(),
        250.0,
        Rc::new(move |data: Js| {
            assert!(data.strict_equals(&expected));
            seen.borrow_mut()
                .push(data.get("ticker")?.get("last")?.as_f64().unwrap());
            Ok(())
        }),
    );
    throttle.call(state.clone()).unwrap();
    for last in 2..=100 {
        row.set("last", Js::Number(last as f64)).unwrap();
        throttle.call(state.clone()).unwrap();
    }
    assert_eq!(reads.get(), 1, "burst calls must retain without projection");
    assert_eq!(*output.borrow(), [1.0]);
    host.now.set(250.0);
    let timer = host.timers.borrow_mut().remove(0).1;
    timer();
    assert_eq!(*output.borrow(), [1.0, 100.0]);
    assert_eq!(reads.get(), 2);
    throttle.call(state).unwrap();
    throttle.cancel();
    assert!(host.timers.borrow().is_empty());
}
