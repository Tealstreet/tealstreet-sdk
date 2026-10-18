//! One record/replay service per worker module, matching perfFixture.ts.
use safe_cex_exchange::{
    delivery_timers::WorkerPortTimers,
    js_runtime::{structured_clone::structured_clone, value::Js},
    runtime::ExchangeError,
};
use std::{cell::{Cell, RefCell}, rc::{Rc, Weak}};

pub const MAX_FRAMES: usize = 20_000;
pub type ReplayPost = Rc<dyn Fn(Js) -> Result<(), ExchangeError>>;

pub trait PerfFixtureHost: WorkerPortTimers {
    fn report_perf_timer_error(&self, error: &ExchangeError);
}

pub struct PerfFixture<H: PerfFixtureHost> {
    host: Weak<H>,
    mode: RefCell<Js>,
    frames: RefCell<Js>,
    recorded_at: Cell<f64>,
    fixture: RefCell<Js>,
    interval: Cell<f64>,
    looping: Cell<bool>,
    cursor: Cell<f64>,
    timer: RefCell<Option<H::Timer>>,
    closed: Cell<bool>,
}

impl<H: PerfFixtureHost + 'static> PerfFixture<H> {
    pub fn new(host: &Rc<H>) -> Rc<Self> {
        Rc::new(Self { host: Rc::downgrade(host), mode: RefCell::new(Js::str("off")), frames: RefCell::new(Js::array(vec![])), recorded_at: Cell::new(0.), fixture: RefCell::new(Js::Null), interval: Cell::new(50.), looping: Cell::new(true), cursor: Cell::new(0.), timer: RefCell::new(None), closed: Cell::new(false) })
    }
    fn host(&self) -> Result<Rc<H>, ExchangeError> {
        self.host.upgrade().ok_or_else(|| ExchangeError::new("Perf fixture host released"))
    }
    pub fn is_recording(&self) -> bool { !self.closed.get() && self.mode.borrow().strict_equals(&Js::str("record")) }
    pub fn is_replaying(&self) -> bool { !self.closed.get() && self.mode.borrow().strict_equals(&Js::str("replay")) }
    pub fn configure(&self, cfg: Js) -> Result<(), ExchangeError> {
        if self.closed.get() { return Err(ExchangeError::new("Perf fixture module disposed")); }
        let next = if cfg.is_nullish() { Js::Undefined } else { cfg.get("mode")? };
        let next = if next.is_nullish() { Js::str("off") } else { next };
        if next.strict_equals(&self.mode.borrow()) && !next.strict_equals(&Js::str("replay")) { return Ok(()); }
        self.stop_replay();
        *self.mode.borrow_mut() = next.clone();
        if next.strict_equals(&Js::str("record")) {
            *self.frames.borrow_mut() = Js::array(vec![]);
            self.recorded_at.set(0.);
        }
        if next.strict_equals(&Js::str("replay")) {
            let fixture = cfg.get("fixture")?;
            *self.fixture.borrow_mut() = if fixture.is_nullish() { Js::Null } else { fixture };
            let interval = cfg.get("intervalMs")?;
            let interval = if interval.is_nullish() { 50. } else { interval.to_number()? };
            self.interval.set(if interval.is_nan() { f64::NAN } else { interval.max(1.) });
            self.looping.set(!cfg.get("loop")?.strict_equals(&Js::Bool(false)));
            self.cursor.set(0.);
        }
        Ok(())
    }
    pub fn record(&self, msg: &Js) -> Result<(), ExchangeError> {
        if !self.is_recording() { return Ok(()); }
        let frames = self.frames.borrow().clone();
        if frames.get("length")?.to_number()? >= MAX_FRAMES as f64 { return Ok(()); }
        let now = self.host()?.now_ms();
        if self.recorded_at.get() == 0. { self.recorded_at.set(now); }
        // The maintained recorder deliberately drops unclonable outbound messages.
        let Ok(snapshot) = structured_clone(msg) else { return Ok(()); };
        let frame = Js::object(vec![("atMs", Js::Number(now - self.recorded_at.get())), ("msg", snapshot)]);
        let frames = self.frames.borrow().clone();
        frames.get("push")?.call(frames, vec![frame])?;
        Ok(())
    }
    pub fn take(&self) -> Result<Js, ExchangeError> {
        let recorded_at = self.recorded_at.get();
        let frames = self.frames.borrow().clone();
        let length = frames.get("length")?;
        let duration = if length.truthy() {
            frames.get(&safe_cex_num::js_number_string(length.to_number()? - 1.))?.get("atMs")?
        } else { Js::Number(0.) };
        Ok(Js::object(vec![("version", Js::Number(1.)), ("recordedAtMs", Js::Number(recorded_at)), ("durationMs", duration), ("frames", self.frames.borrow().clone())]))
    }
    pub fn stop_replay(&self) {
        let timer = self.timer.borrow().clone();
        if let Some(host) = self.host.upgrade() {
            if let Some(timer) = timer.filter(|timer| host.timer_is_truthy(timer)) {
                host.clear_timeout(&timer);
                self.timer.borrow_mut().take();
            }
        }
    }
    pub fn start_replay(self: &Rc<Self>, post: ReplayPost) -> Result<(), ExchangeError> {
        if !self.is_replaying() { return Ok(()); }
        let fixture = self.fixture.borrow().clone();
        let frames = if fixture.is_nullish() { Js::Undefined } else { fixture.get("frames")? };
        let length = if frames.is_nullish() { Js::Undefined } else { frames.get("length")? };
        if !length.truthy() { return Ok(()); }
        self.stop_replay();
        self.schedule(post)
    }
    fn schedule(self: &Rc<Self>, post: ReplayPost) -> Result<(), ExchangeError> {
        let weak = Rc::downgrade(self);
        let timer = self.host()?.set_timeout(Rc::new(move || {
            if let Some(service) = weak.upgrade() {
                if let Err(error) = service.tick(post.clone()) {
                    if let Some(host) = service.host.upgrade() { host.report_perf_timer_error(&error); }
                }
            }
        }), self.interval.get());
        *self.timer.borrow_mut() = Some(timer);
        Ok(())
    }
    fn tick(self: &Rc<Self>, post: ReplayPost) -> Result<(), ExchangeError> {
        let fixture = self.fixture.borrow().clone();
        if !self.is_replaying() || !fixture.truthy() { return Ok(()); }
        let frame = fixture.get("frames")?.get(&safe_cex_num::js_number_string(self.cursor.get()))?;
        if frame.truthy() {
            // A throwing frame message/post is swallowed by the source tick.
            let _ = (|| -> Result<(), ExchangeError> { post(frame.get("msg")?) })();
        }
        self.cursor.set(self.cursor.get() + 1.);
        let fixture = self.fixture.borrow().clone();
        if self.cursor.get() >= fixture.get("frames")?.get("length")?.to_number()? {
            if !self.looping.get() { return Ok(()); }
            self.cursor.set(0.);
        }
        self.schedule(post)
    }
    pub fn dispose(&self) {
        self.stop_replay();
        self.closed.set(true);
        *self.mode.borrow_mut() = Js::str("off");
        *self.frames.borrow_mut() = Js::array(vec![]);
        *self.fixture.borrow_mut() = Js::Null;
    }
}
impl<H: PerfFixtureHost> Drop for PerfFixture<H> {
    fn drop(&mut self) {
        if let Some(host) = self.host.upgrade() {
            if let Some(timer) = self.timer.get_mut().take().filter(|timer| host.timer_is_truthy(timer)) { host.clear_timeout(&timer); }
        }
    }
}
