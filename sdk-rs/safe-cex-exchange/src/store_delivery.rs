use crate::{delivery_timers::WorkerPortTimers, runtime::ExchangeError};
use safe_cex_types::StoreData;
use std::{cell::RefCell, rc::Rc};

pub type StoreFrame = Rc<RefCell<StoreData>>;
pub trait StoreUpdateThrottleHost: WorkerPortTimers + 'static {
    fn report_store_timer_error(&self, error: &ExchangeError);
}

struct ThrottleState<T> {
    timer: Option<T>,
    last_args: Option<StoreFrame>,
    last_call: Option<f64>,
    last_invoke: f64,
}
/// lodash throttle with default leading=true, trailing=true, maxWait=wait.
/// Its callback reaches the real worker store_update through Weak, not a second
/// keyed throttle. Binary64 timer delays and wall-clock rollback are preserved.
pub struct StoreUpdateThrottle<H: StoreUpdateThrottleHost> {
    host: Rc<H>,
    pub wait_ms: f64,
    state: RefCell<ThrottleState<H::Timer>>,
    callback: Rc<dyn Fn(StoreFrame) -> Result<(), ExchangeError>>,
}
impl<H: StoreUpdateThrottleHost> StoreUpdateThrottle<H> {
    pub fn new(
        host: Rc<H>,
        wait: f64,
        callback: Rc<dyn Fn(StoreFrame) -> Result<(), ExchangeError>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            host,
            wait_ms: if wait.is_nan() || wait == 0.0 {
                0.0
            } else {
                wait
            },
            state: RefCell::new(ThrottleState {
                timer: None,
                last_args: None,
                last_call: None,
                last_invoke: 0.0,
            }),
            callback,
        })
    }
    fn should_invoke(&self, time: f64) -> bool {
        let s = self.state.borrow();
        s.last_call.is_none_or(|last| {
            time - last >= self.wait_ms || time - last < 0.0 || time - s.last_invoke >= self.wait_ms
        })
    }
    fn set_timer(self: &Rc<Self>, delay: f64) -> H::Timer {
        let weak = Rc::downgrade(self);
        self.host.set_timeout(
            Rc::new(move || {
                if let Some(throttle) = weak.upgrade() {
                    if let Err(error) = throttle.timer_expired() {
                        throttle.host.report_store_timer_error(&error);
                    }
                }
            }),
            delay,
        )
    }
    fn invoke(&self, time: f64) -> Result<(), ExchangeError> {
        let args = {
            let mut s = self.state.borrow_mut();
            let args = s.last_args.take();
            s.last_invoke = time;
            args
        };
        let args = args.ok_or_else(|| {
            ExchangeError::new("Store update arguments cleared before invocation")
        })?;
        (self.callback)(args)
    }
    fn timer_expired(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let time = self.host.now_ms();
        if self.should_invoke(time) {
            self.state.borrow_mut().timer = None;
            let has_args = self.state.borrow().last_args.is_some();
            if has_args {
                return self.invoke(time);
            }
            self.state.borrow_mut().last_args = None;
        } else {
            let remaining = {
                let s = self.state.borrow();
                safe_cex_num::math_min(
                    self.wait_ms - (time - s.last_call.unwrap()),
                    self.wait_ms - (time - s.last_invoke),
                )
            };
            let timer = self.set_timer(remaining);
            self.state.borrow_mut().timer = Some(timer);
        }
        Ok(())
    }
    pub fn call(self: &Rc<Self>, data: StoreFrame) -> Result<(), ExchangeError> {
        let time = self.host.now_ms();
        let invoking = self.should_invoke(time);
        {
            let mut s = self.state.borrow_mut();
            s.last_args = Some(data);
            s.last_call = Some(time);
        }
        let timer = self.state.borrow().timer.clone();
        if invoking {
            if let Some(timer) = timer {
                self.host.clear_timeout(&timer);
                let timer = self.set_timer(self.wait_ms);
                self.state.borrow_mut().timer = Some(timer);
                return self.invoke(time);
            }
            self.state.borrow_mut().last_invoke = time;
            let timer = self.set_timer(self.wait_ms);
            self.state.borrow_mut().timer = Some(timer);
            return self.invoke(time);
        }
        if timer.is_none() {
            let timer = self.set_timer(self.wait_ms);
            self.state.borrow_mut().timer = Some(timer);
        }
        Ok(())
    }
    pub fn cancel(&self) {
        let timer = self.state.borrow().timer.clone();
        if let Some(timer) = timer {
            self.host.clear_timeout(&timer);
        }
        let mut s = self.state.borrow_mut();
        s.last_invoke = 0.0;
        s.last_args = None;
        s.last_call = None;
        s.timer = None;
    }
}
impl<H: StoreUpdateThrottleHost> Drop for StoreUpdateThrottle<H> {
    fn drop(&mut self) {
        if let Some(timer) = self.state.get_mut().timer.take() {
            self.host.clear_timeout(&timer);
        }
    }
}
