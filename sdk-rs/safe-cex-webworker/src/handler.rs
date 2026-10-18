use crate::{
    error_post::{WorkerErrorPost, WorkerErrorReport},
    payload::StoreWorkerPayload,
};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeTask},
};
use serde_json::{Map, Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

struct Timer {
    id: Cell<u64>,
    armed: Cell<bool>,
    task: RefCell<Option<Box<dyn ExchangeTask>>>,
}
impl Default for Timer {
    fn default() -> Self {
        Self {
            id: Cell::new(0),
            armed: Cell::new(false),
            task: RefCell::new(None),
        }
    }
}
impl Timer {
    fn cancel(&self) {
        self.armed.set(false);
        self.id.set(self.id.get().wrapping_add(1));
        let task = self.task.borrow_mut().take();
        if let Some(task) = task {
            task.cancel();
        }
    }
}
pub struct BaseHandler {
    exchange: Rc<dyn Exchange>,
    error_post: WorkerErrorPost,
    replay: Rc<dyn Fn(StoreWorkerPayload)>,
    name: String,
    pending: RefCell<Vec<StoreWorkerPayload>>,
    check: Timer,
    timeout: Timer,
    first_pending_at: Cell<Option<f64>>,
    last_report_at: Cell<Option<f64>>,
    suppressed: Cell<u64>,
}
struct TimerGuard {
    handler: Weak<BaseHandler>,
    check: bool,
    id: u64,
    settled: Rc<Cell<bool>>,
    fired: bool,
}
impl Drop for TimerGuard {
    fn drop(&mut self) {
        self.settled.set(true);
        if let Some(handler) = self.handler.upgrade() {
            let timer = if self.check {
                &handler.check
            } else {
                &handler.timeout
            };
            if timer.id.get() == self.id {
                if !self.fired {
                    timer.armed.set(false);
                }
                let task = timer.task.borrow_mut().take();
                drop(task);
            }
        }
    }
}
impl BaseHandler {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        name: String,
        error_post: WorkerErrorPost,
        replay: Rc<dyn Fn(StoreWorkerPayload)>,
    ) -> Rc<Self> {
        Rc::new(Self {
            exchange,
            error_post,
            replay,
            name,
            pending: RefCell::default(),
            check: Timer::default(),
            timeout: Timer::default(),
            first_pending_at: Cell::new(None),
            last_report_at: Cell::new(None),
            suppressed: Cell::new(0),
        })
    }
    pub fn handle(&self, payload: StoreWorkerPayload) {
        (self.replay)(payload);
    }
    pub fn buffer_until_markets_loaded(
        self: &Rc<Self>,
        payload: StoreWorkerPayload,
    ) -> Result<bool, ExchangeError> {
        if !self.exchange.base().store.markets()?.is_empty() {
            self.reset_pending_timeout_report_episode();
            return Ok(false);
        }
        if self.first_pending_at.get().is_none() {
            self.first_pending_at
                .set(Some(self.exchange.base().runtime.now_ms()));
        }
        self.pending.borrow_mut().push(payload);
        self.schedule_pending_markets_check();
        if !self.timeout.armed.get() {
            self.arm(false, 30_000);
        }
        Ok(true)
    }
    fn arm(self: &Rc<Self>, is_check: bool, delay: u64) {
        let timer = if is_check { &self.check } else { &self.timeout };
        let id = timer.id.get().wrapping_add(1);
        timer.id.set(id);
        timer.armed.set(true);
        let settled = Rc::new(Cell::new(false));
        let weak = Rc::downgrade(self);
        let guard = TimerGuard {
            handler: weak.clone(),
            check: is_check,
            id,
            settled: settled.clone(),
            fired: false,
        };
        let sleep = self.exchange.base().runtime.sleep(delay);
        let task = self
            .exchange
            .base()
            .runtime
            .spawn_local(Box::pin(async move {
                let mut guard = guard;
                if sleep.await.is_err() {
                    return;
                }
                guard.fired = true;
                if let Some(handler) = weak.upgrade() {
                    let timer = if is_check {
                        &handler.check
                    } else {
                        &handler.timeout
                    };
                    if timer.id.get() != id {
                        return;
                    }
                    let handle = timer.task.borrow_mut().take();
                    drop(handle);
                    if is_check {
                        timer.armed.set(false);
                        if handler.pending.borrow().is_empty() {
                            return;
                        }
                        let markets = match handler.exchange.base().store.markets() {
                            Ok(markets) => markets,
                            Err(error) => {
                                handler.exchange.base().runtime.report_exception(
                                    &error.into(),
                                    &json!({"method":"bufferUntilMarketsLoaded"}).into(),
                                );
                                return;
                            }
                        };
                        if !markets.is_empty() {
                            handler.flush_pending();
                        } else {
                            handler.schedule_pending_markets_check();
                        }
                    } else {
                        if let Err(error) = handler.pending_timed_out() {
                            handler.exchange.base().runtime.report_exception(
                                &error,
                                &json!({"method":"bufferUntilMarketsLoaded"}).into(),
                            );
                        }
                    }
                }
            }));
        if !settled.get() && timer.id.get() == id {
            *timer.task.borrow_mut() = Some(task);
        }
    }
    fn schedule_pending_markets_check(self: &Rc<Self>) {
        if self.check.task.borrow().is_some() || self.pending.borrow().is_empty() {
            return;
        }
        self.arm(true, 500);
    }
    fn flush_pending(&self) {
        self.clear_pending_timers();
        self.reset_pending_timeout_report_episode();
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        for payload in pending {
            self.handle(payload);
        }
    }
    fn clear_pending_timers(&self) {
        self.check.cancel();
        self.timeout.cancel();
        self.first_pending_at.set(None);
    }
    fn pending_timed_out(&self) -> Result<(), ExchangeError> {
        let count = self.pending.borrow().len();
        if count > 0 {
            let now = self.exchange.base().runtime.now_ms();
            if self
                .last_report_at
                .get()
                .is_none_or(|last| now - last >= 300_000.0)
            {
                let message = format!(
                    "Markets still not loaded after 30s, dropping {count} pending subscriptions"
                );
                self.exchange
                    .base()
                    .runtime
                    .console("error", &format!("[{}] {message}", self.name));
                let error = self.error_post.create("MarketsNotLoadedError", &message)?;
                let tags = json!({"exchange":self.exchange.base().name,"module":"BaseHandler","worker_handler":self.name,"worker_error_family":"markets_not_loaded"});
                let mut extra = self.get_pending_telemetry()?;
                extra["suppressedPendingTimeoutReports"] = json!(self.suppressed.get());
                self.error_post.send(WorkerErrorReport {
                    message,
                    error,
                    tags,
                    extra,
                })?;
                self.last_report_at.set(Some(now));
                self.suppressed.set(0);
            } else {
                self.suppressed.set(self.suppressed.get() + 1);
            }
            self.pending.borrow_mut().clear();
        }
        self.clear_pending_timers();
        Ok(())
    }
    fn get_pending_telemetry(&self) -> Result<Value, ExchangeError> {
        let mut counts = Map::new();
        for payload in self.pending.borrow().iter() {
            let key = serde_json::to_value(payload.op)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
            let count = counts.get(&key).and_then(Value::as_u64).unwrap_or(0);
            counts.insert(key, json!(count + 1));
        }
        let base = self.exchange.base();
        let mut options = Map::new();
        if let Some(value) = base.options().testnet {
            options.insert("testnet".into(), json!(value));
        }
        if let Some(value) = base.options().public_data_only {
            options.insert("publicDataOnly".into(), json!(value));
        }
        let mut health = Map::new();
        if let Some(source) = base.store.socket_health()? {
            for key in [
                "publicInitialized",
                "privateInitialized",
                "publicHealthy",
                "privateHealthy",
                "publicCloseCode",
                "privateCloseCode",
                "publicCloseStreak",
                "privateCloseStreak",
                "publicReconnectPaused",
                "privateReconnectPaused",
            ] {
                if let Some(value) = source.get(key).filter(|value| {
                    !matches!(
                        value,
                        safe_cex_types::retained_json::RetainedJsonInput::Undefined
                    )
                }) {
                    health.insert(
                        key.into(),
                        value
                            .clone()
                            .into_value()
                            .map_err(|error| ExchangeError::new(error.to_string()))?,
                    );
                }
            }
        }
        Ok(
            json!({"marketCount":base.store.markets()?.len(),"loadedMarkets":base.store.loaded()?.markets,
            "pendingCount":self.pending.borrow().len(),"pendingAgeMs":self.first_pending_at.get().map_or(0.0,|first|base.runtime.now_ms()-first),
            "pendingOpCounts":counts,"exchangeOptions":options,"socketHealth":health}),
        )
    }
    fn reset_pending_timeout_report_episode(&self) {
        self.last_report_at.set(None);
        self.suppressed.set(0);
    }
    pub fn dispose(&self) {
        self.clear_pending_timers();
        self.reset_pending_timeout_report_episode();
        self.pending.borrow_mut().clear();
    }
}
impl Drop for BaseHandler {
    fn drop(&mut self) {
        self.check.cancel();
        self.timeout.cancel();
    }
}
