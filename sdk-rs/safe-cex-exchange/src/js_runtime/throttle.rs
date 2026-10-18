//! Lodash throttle's leading/trailing/maxWait scheduling for the source callback.
use super::{
    scope::{Scope, ScopeShared, scope_of},
    value::{Js, Thrown},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
struct State {
    last_call: Option<f64>,
    last_invoke: f64,
    pending: Option<(Js, Vec<Js>)>,
    timer: Option<Js>,
    result: Js,
}
struct Throttle<X> {
    weak: Weak<ScopeShared<X>>,
    function: Js,
    wait: f64,
    max_wait: f64,
    leading: bool,
    trailing: bool,
    state: RefCell<State>,
}
impl<X: 'static> Throttle<X> {
    fn should_invoke(&self, now: f64) -> bool {
        let s = self.state.borrow();
        s.last_call.is_none_or(|last| {
            now - last >= self.wait || now - last < 0. || now - s.last_invoke >= self.max_wait
        })
    }
    fn invoke(&self, now: f64) -> Result<Js, Thrown> {
        let pending = {
            let mut s = self.state.borrow_mut();
            s.last_invoke = now;
            s.pending.take()
        };
        if let Some((this, args)) = pending {
            let value = self.function.call(this, args)?;
            self.state.borrow_mut().result = value;
        }
        Ok(self.state.borrow().result.clone())
    }
    fn schedule(self: &Rc<Self>, rt: &Scope<X>, delay: f64) {
        let this = self.clone();
        let h = rt.set_timeout(Rc::new(move || this.expire()), delay);
        self.state.borrow_mut().timer = Some(h);
    }
    fn expire(self: &Rc<Self>) -> Result<(), Thrown> {
        let rt = scope_of(&self.weak)?;
        let now = rt.now();
        if self.should_invoke(now) {
            self.state.borrow_mut().timer = None;
            if self.trailing && self.state.borrow().pending.is_some() {
                self.invoke(now)?;
            } else {
                self.state.borrow_mut().pending = None;
            }
        } else {
            let remaining = {
                let s = self.state.borrow();
                (self.wait - (now - s.last_call.unwrap_or(now)))
                    .min(self.max_wait - (now - s.last_invoke))
            };
            self.schedule(&rt, remaining);
        }
        Ok(())
    }
    fn call(self: &Rc<Self>, this: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        let rt = scope_of(&self.weak)?;
        let now = rt.now();
        let invoke = self.should_invoke(now);
        {
            let mut s = self.state.borrow_mut();
            s.last_call = Some(now);
            s.pending = Some((this, args));
        }
        if invoke {
            let timer = self.state.borrow().timer.clone();
            if let Some(h) = timer {
                rt.clear_timeout(&h);
                self.schedule(&rt, self.wait);
                return self.invoke(now);
            }
            self.state.borrow_mut().last_invoke = now;
            self.schedule(&rt, self.wait);
            if self.leading {
                return self.invoke(now);
            }
        }
        if self.state.borrow().timer.is_none() {
            self.schedule(&rt, self.wait);
        }
        Ok(self.state.borrow().result.clone())
    }
}
pub(super) fn new<X: 'static>(
    rt: &Scope<X>,
    function: Js,
    wait: f64,
    leading: bool,
    trailing: bool,
) -> Js {
    let t = Rc::new(Throttle {
        weak: Rc::downgrade(&rt.shared),
        function,
        wait,
        max_wait: wait.max(0.),
        leading,
        trailing,
        state: RefCell::new(State {
            last_call: None,
            last_invoke: 0.,
            pending: None,
            timer: None,
            result: Js::Undefined,
        }),
    });
    let called = t.clone();
    let value = Js::function("throttled", move |this, args| called.call(this, args));
    let cancelled = t.clone();
    value
        .define(
            "cancel",
            Js::function("cancel", move |_, _| {
                let rt = scope_of(&cancelled.weak)?;
                let timer = {
                    let mut s = cancelled.state.borrow_mut();
                    s.last_invoke = 0.;
                    s.last_call = None;
                    s.pending = None;
                    s.timer.take()
                };
                if let Some(timer) = timer {
                    rt.clear_timeout(&timer);
                }
                Ok(Js::Undefined)
            }),
        )
        .expect("fresh throttle function");
    value
        .define(
            "flush",
            Js::function("flush", move |_, _| {
                let now = scope_of(&t.weak)?.now();
                let pending = {
                    let mut s = t.state.borrow_mut();
                    let timer = s.timer.take().is_some();
                    timer && t.trailing && s.pending.is_some()
                };
                if pending {
                    t.invoke(now)
                } else {
                    t.state.borrow_mut().pending = None;
                    Ok(t.state.borrow().result.clone())
                }
            }),
        )
        .expect("fresh throttle function");
    value
}
