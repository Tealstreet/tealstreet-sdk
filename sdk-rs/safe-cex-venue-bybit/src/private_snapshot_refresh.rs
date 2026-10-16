//! Maintained private-snapshot field bodies over retained values and global timers.
use crate::core_snapshot_readers::SnapshotHost;
use safe_cex_exchange::{
    OwnedOperation,
    runtime::{ExchangeError, ExchangeRuntime},
    start_detached_timeout,
};
use std::{cell::RefCell, rc::Rc};

const PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS: f64 = 250.;
const PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS: f64 = 1200.;

struct TimerSlot {
    generation: u64,
    operation: Option<OwnedOperation>,
}
struct State<V> {
    next_generation: u64,
    pending_call: bool,
    last_call: Option<f64>,
    last_invoke: f64,
    timer: Option<TimerSlot>,
    result: V,
}
pub struct PrivateSnapshotRefresh<H: SnapshotHost> {
    host: Rc<H>,
    owner: H::Value,
    runtime: Rc<dyn ExchangeRuntime>,
    state: RefCell<State<H::Value>>,
}
impl<H: SnapshotHost> PrivateSnapshotRefresh<H> {
    pub fn new(host: Rc<H>, owner: H::Value, runtime: Rc<dyn ExchangeRuntime>) -> Rc<Self> {
        let result = host.undefined();
        Rc::new(Self {
            host,
            owner,
            runtime,
            state: RefCell::new(State {
                next_generation: 0,
                pending_call: false,
                last_call: None,
                last_invoke: 0.,
                timer: None,
                result,
            }),
        })
    }
    fn should_invoke(state: &State<H::Value>, time: f64) -> bool {
        let Some(last_call) = state.last_call else {
            return true;
        };
        let elapsed = time - last_call;
        elapsed >= PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS
            || elapsed < 0.
            || time - state.last_invoke >= PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS
    }
    fn invoke(&self, time: f64) -> H::Value {
        {
            let mut state = self.state.borrow_mut();
            state.pending_call = false;
            state.last_invoke = time;
        }
        let h = self.host.clone();
        let owner = self.owner.clone();
        let result = self.host.launch(Box::pin(async move {
            if h.truthy(&h.get(&owner, "isDisposed")?)
                || h.truthy(&h.get(&owner, "isPublicDataOnly")?)
            {
                return Ok(h.undefined());
            }
            let refreshed = async {
                let selected = h.select(owner.clone(), "fetchAndSetPrivateSnapshotHelper")?;
                let work = h.apply(selected, vec![])?;
                h.wait(work).await?;
                Ok(())
            }
            .await;
            if let Err(error) = refreshed {
                let selected = h.select(owner, "emitAndLogError")?;
                h.apply(selected, vec![h.error_value(error)])?;
            }
            Ok(h.undefined())
        }));
        self.state.borrow_mut().result = result.clone();
        result
    }
    fn start_timer(self: &Rc<Self>, delay: f64) -> Result<(), ExchangeError> {
        let generation = {
            let mut state = self.state.borrow_mut();
            let generation = state.next_generation;
            state.next_generation = generation
                .checked_add(1)
                .expect("snapshot timer generation exhausted");
            state.timer = Some(TimerSlot {
                generation,
                operation: None,
            });
            generation
        };
        let this = self.clone();
        let started = start_detached_timeout(
            self.runtime.clone(),
            delay.max(1.).floor() as u64,
            move || this.expired(),
        );
        let mut state = self.state.borrow_mut();
        let current = state
            .timer
            .as_ref()
            .is_some_and(|slot| slot.generation == generation);
        match started {
            Ok(timer) if current && timer.is_active() => {
                state.timer.as_mut().expect("current timer").operation = Some(timer);
            }
            Ok(timer) => {
                if current {
                    state.timer = None;
                }
                drop(state);
                timer.cancel();
            }
            Err(error) => {
                if current {
                    state.timer = None;
                }
                return Err(error);
            }
        }
        Ok(())
    }
    fn trailing_edge(&self, time: f64) -> H::Value {
        let pending = {
            let mut state = self.state.borrow_mut();
            state.timer = None;
            state.pending_call
        };
        if pending {
            self.invoke(time)
        } else {
            self.state.borrow().result.clone()
        }
    }
    fn expired(self: &Rc<Self>) -> Result<(), ExchangeError> {
        let time = self.runtime.now_ms();
        let remaining = {
            let state = self.state.borrow();
            if Self::should_invoke(&state, time) {
                None
            } else {
                Some(
                    (PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS
                        - (time - state.last_call.expect("pending call time")))
                    .min(PRIVATE_SNAPSHOT_REFRESH_MAX_WAIT_MS - (time - state.last_invoke)),
                )
            }
        };
        if let Some(delay) = remaining {
            self.start_timer(delay)
        } else {
            self.trailing_edge(time);
            Ok(())
        }
    }
    pub fn call(self: &Rc<Self>) -> Result<H::Value, ExchangeError> {
        let time = self.runtime.now_ms();
        let (invoking, has_timer, timer) = {
            let mut state = self.state.borrow_mut();
            let invoking = Self::should_invoke(&state, time);
            state.pending_call = true;
            state.last_call = Some(time);
            (
                invoking,
                state.timer.is_some(),
                state.timer.as_ref().and_then(|slot| slot.operation.clone()),
            )
        };
        if invoking {
            if !has_timer {
                self.state.borrow_mut().last_invoke = time;
                self.start_timer(PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS)?;
                return Ok(self.state.borrow().result.clone());
            }
            if let Some(timer) = timer {
                timer.cancel();
            }
            self.start_timer(PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS)?;
            return Ok(self.invoke(time));
        }
        if !has_timer {
            self.start_timer(PRIVATE_SNAPSHOT_REFRESH_DEBOUNCE_MS)?;
        }
        Ok(self.state.borrow().result.clone())
    }
    pub fn cancel(&self) {
        let timer = {
            let mut state = self.state.borrow_mut();
            state.last_invoke = 0.;
            state.pending_call = false;
            state.last_call = None;
            state.timer.take().and_then(|slot| slot.operation)
        };
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
    pub fn flush(&self) -> H::Value {
        if self.state.borrow().timer.is_none() {
            return self.state.borrow().result.clone();
        }
        self.trailing_edge(self.runtime.now_ms())
    }
}
pub fn schedule_set_trading_stop_snapshot_refresh<H: SnapshotHost>(
    h: &H,
    owner: H::Value,
) -> Result<H::Value, H::Error> {
    let callback = h.select(owner, "debouncedPrivateSnapshotRefresh")?;
    h.apply(callback, vec![])?;
    Ok(h.undefined())
}
