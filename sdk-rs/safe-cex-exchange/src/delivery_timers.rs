use std::rc::Rc;

/// Timers enqueue callbacks and preserve cloned identity and binary64 delays.
/// Truthiness/cancellation never invoke callbacks; teardown releases callbacks
/// so retained host/callback registries cannot form an Rc cycle.
pub trait WorkerPortTimers {
    type Timer: Clone;
    fn now_ms(&self) -> f64;
    fn set_timeout(&self, callback: Rc<dyn Fn()>, delay_ms: f64) -> Self::Timer;
    fn clear_timeout(&self, timer: &Self::Timer);
    fn timer_is_truthy(&self, timer: &Self::Timer) -> bool;
}
