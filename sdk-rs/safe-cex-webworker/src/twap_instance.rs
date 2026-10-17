//! TwapInstance lifecycle. The native host binds worker delivery, timers and
//! chaser-manager operations; those bindings are not substitute manager ports.
use crate::chaser::ChaserOrderInstance;
use crate::task_types::ChaserOrderOptions;
use crate::task_types::TwapOptions;
use crate::task_types::TwapState;
use crate::task_types::TwapStatus;
use crate::task_types::TwapTimeoutAction;
use crate::twap_manager::ManagedTwap;
use crate::twap_manager::SerializedTwap;
use crate::twap_manager::TwapManager;
use indexmap::IndexMap;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_types::{OrderSide, OrderType, PlaceOrderOpts};
use std::{cell::RefCell, rc::Rc};

/// Host primitives must execute their construction prefix synchronously and
/// release RefCell borrows before callbacks. Spawn polls/retains detached work;
/// it reports unhandled errors rather than silently dropping them. Timer IDs
/// retain identity, and hosts release retained callbacks at worker teardown.
pub trait TwapHost: WorkerPortTimers {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime>;
    fn manager(&self) -> Rc<TwapManager>;
    /// Worker postMessage({op: 'twap:stateUpdate', data: states}).
    fn post_state(&self, states: IndexMap<String, SerializedTwap>) -> Result<(), ExchangeError>;
    fn place_order(
        &self,
        exchange: Rc<dyn Exchange>,
        order: PlaceOrderOpts,
    ) -> Result<ExchangeFuture<'static, Vec<String>>, ExchangeError>;
    fn emit_log(
        &self,
        exchange: Rc<dyn Exchange>,
        message: String,
        severity: &'static str,
    ) -> Result<(), ExchangeError>;
    fn add_chaser(&self, instance: Rc<ChaserOrderInstance>) -> Result<(), ExchangeError>;
    fn find_chaser(&self, id: &str) -> Option<Rc<ChaserOrderInstance>>;
    fn remove_chaser(&self, id: &str) -> Result<(), ExchangeError>;
    fn same_timer(&self, first: &Self::Timer, second: &Self::Timer) -> bool;
    fn spawn(&self, future: ExchangeFuture<'static, ()>);
}

pub type UpdateStateCallback<H> =
    Rc<dyn Fn(&TwapInstance<H>, Rc<RefCell<TwapState>>) -> Result<(), ExchangeError>>;

#[derive(Default)]
pub struct TwapStateUpdate {
    pub status: Option<TwapStatus>,
    pub orders_executed: Option<f64>,
    pub next_order_at: Option<f64>,
    pub started_at: Option<f64>,
    pub mark_for_cleanup: Option<bool>,
}

pub struct TwapInstance<H: TwapHost> {
    /// This exact record is registered in TwapManager, never a copied snapshot.
    pub managed: Rc<RefCell<ManagedTwap>>,
    pub exchange: Rc<dyn Exchange>,
    pub host: Rc<H>,
    pub update_state_callback: UpdateStateCallback<H>,
    pub timer_id: RefCell<Option<H::Timer>>,
    pub tracked_chaser_order_ids: RefCell<Vec<String>>,
    pub slice_timers: RefCell<Vec<H::Timer>>,
}
impl<H: TwapHost + 'static> TwapInstance<H> {
    // ts: constructor. ID generation precedes state creation, even on error.
    pub fn new(
        exchange: Rc<dyn Exchange>,
        host: Rc<H>,
        options: Rc<RefCell<TwapOptions>>,
        update_state_callback: UpdateStateCallback<H>,
        state: Option<Rc<RefCell<TwapState>>>,
        id: Option<String>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let id = id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| host.runtime().unique_id());
        let state = match state {
            Some(state) => state,
            None => {
                let input = options.borrow().clone();
                Rc::new(RefCell::new(crate::twap::create_new_state(
                    &input,
                    host.runtime().as_ref(),
                )?))
            }
        };
        Ok(Rc::new(Self {
            managed: Rc::new(RefCell::new(ManagedTwap { id, options, state })),
            exchange,
            host,
            update_state_callback,
            timer_id: RefCell::new(None),
            tracked_chaser_order_ids: RefCell::new(vec![]),
            slice_timers: RefCell::new(vec![]),
        }))
    }
    pub fn state(&self) -> Rc<RefCell<TwapState>> {
        self.managed.borrow().state.clone()
    }
    pub fn options(&self) -> Rc<RefCell<TwapOptions>> {
        self.managed.borrow().options.clone()
    }
    pub fn id(&self) -> String {
        self.managed.borrow().id.clone()
    }

    // ts: updateState. Replace state, invoke callback, then serialize current manager.
    pub fn update_state(&self, update: TwapStateUpdate) -> Result<(), ExchangeError> {
        let mut state = self.state().borrow().clone();
        if let Some(v) = update.status {
            state.status = v;
        }
        if let Some(v) = update.orders_executed {
            state.orders_executed = v;
        }
        if let Some(v) = update.next_order_at {
            state.next_order_at = v;
        }
        if let Some(v) = update.started_at {
            state.started_at = Some(v);
        }
        if let Some(v) = update.mark_for_cleanup {
            state.mark_for_cleanup = Some(v);
        }
        let state = Rc::new(RefCell::new(state));
        self.managed.borrow_mut().state = state.clone();
        (self.update_state_callback)(self, state)?;
        self.host.post_state(self.host.manager().serialize_state())
    }

    fn order(&self, amount: f64, price: Option<f64>) -> PlaceOrderOpts {
        let options = self.options();
        let options = options.borrow();
        PlaceOrderOpts {
            symbol: options.active_market.symbol.clone(),
            side: options.side,
            r#type: if price.is_some() {
                OrderType::Limit
            } else {
                OrderType::Market
            },
            amount,
            price,
            reduce_only: Some(options.reduce_only),
            trigger_price: None,
            stop_loss: None,
            take_profit: None,
            time_in_force: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            client_order_id: None,
            max_slippage_pct: None,
            close: None,
            implied_volatility: None,
        }
    }
    // Prefix wrapper: captures options and invokes placement before any polling.
    fn fire_remainder(self: &Rc<Self>, amount: f64) -> ExchangeFuture<'static, ()> {
        if amount <= 0.0 {
            return Box::pin(async { Ok(()) });
        }
        let placement = self
            .host
            .place_order(self.exchange.clone(), self.order(amount, None));
        Box::pin(async move {
            if let Ok(placement) = placement {
                let _ = placement.await;
            }
            Ok(())
        })
    }
    // ts: placeOrder. All pre-await construction executes during this call.
    pub fn place_order(self: &Rc<Self>, amount: f64) -> ExchangeFuture<'static, ()> {
        let result = (|| {
            let options = self.options();
            if options.borrow().use_chaser_orders {
                let o = options.borrow();
                let input = ChaserOrderOptions {
                    symbol: o.active_market.symbol.clone(),
                    size: amount,
                    side: o.side,
                    max_attempts: 1000.0,
                    reduce_only: Some(o.reduce_only),
                    delay: o.chaser_delay,
                    origin: None,
                    account_name: None,
                    terminate: None,
                    client_id: None,
                };
                drop(o);
                let chaser = Rc::new(ChaserOrderInstance::new(
                    self.exchange.clone(),
                    Rc::new(RefCell::new(input)),
                ));
                let id = chaser.id().to_owned();
                self.tracked_chaser_order_ids.borrow_mut().push(id.clone());
                self.host.add_chaser(chaser)?;
                // Re-read options after manager.add, which can invoke host callbacks.
                if self.options().borrow().timeout_pct.is_some() {
                    self.schedule_slice_timeout(id, amount)?;
                }
                return Ok(None);
            }
            let limit = options.borrow().use_limit_orders;
            let price = if limit {
                let tickers = self.exchange.base().store.tickers();
                let current_options = self.options();
                let o = current_options.borrow();
                let ticker = tickers.iter().find(|t| t.symbol == o.active_market.symbol);
                let best = ticker
                    .map(|t| {
                        if o.side == OrderSide::Buy {
                            t.bid
                        } else {
                            t.ask
                        }
                    })
                    .unwrap_or(0.0);
                let price = if truthy(best) {
                    best
                } else {
                    ticker.map(|t| t.last).filter(|n| truthy(*n)).unwrap_or(0.0)
                };
                truthy(price).then_some(price)
            } else {
                None
            };
            self.host
                .place_order(self.exchange.clone(), self.order(amount, price))
                .map(Some)
        })();
        Box::pin(async move {
            match result {
                Ok(Some(future)) => future.await.map(|_| ()),
                Ok(None) => Ok(()),
                Err(e) => Err(e),
            }
        })
    }

    // ts: scheduleSliceTimeout. Actions captured now, instance state read at fire.
    pub fn schedule_slice_timeout(
        self: &Rc<Self>,
        chaser_id: String,
        _amount: f64,
    ) -> Result<(), ExchangeError> {
        let options = self.options();
        let o = options.borrow();
        let slice_ms = crate::twap::slice_interval_ms(&o);
        let pct = o.timeout_pct.unwrap_or(50.0);
        let timeout_action = o.timeout_action.unwrap_or(TwapTimeoutAction::Market);
        let fail_action = o.fail_action.unwrap_or(TwapTimeoutAction::Market);
        let delay = safe_cex_num::math_max(0.0, slice_ms * (pct / 100.0));
        drop(o);
        let slot = Rc::new(RefCell::new(None));
        let token = slot.clone();
        let instance = self.clone();
        let timer = self.host.set_timeout(
            Rc::new(move || {
                let timer = token
                    .borrow()
                    .clone()
                    .expect("host enqueues setTimeout callback");
                let index = instance
                    .slice_timers
                    .borrow()
                    .iter()
                    .position(|t| instance.host.same_timer(t, &timer));
                if let Some(index) = index {
                    instance.slice_timers.borrow_mut().remove(index);
                }
                let Some(chaser) = instance.host.find_chaser(&chaser_id) else {
                    return;
                };
                let remaining = chaser.remaining_size();
                if remaining <= 0.0 {
                    return;
                }
                let action = if !chaser.is_stopped() {
                    timeout_action
                } else {
                    fail_action
                };
                if let Err(error) = instance.host.remove_chaser(chaser.id()) {
                    instance.host.runtime().report_exception(
                        &error,
                        &serde_json::json!({"method":"scheduleSliceTimeout"}),
                    );
                    return;
                }
                if action == TwapTimeoutAction::Market {
                    let future = instance.fire_remainder(remaining);
                    instance.host.spawn(future);
                }
            }),
            delay,
        );
        *slot.borrow_mut() = Some(timer.clone());
        self.slice_timers.borrow_mut().push(timer);
        Ok(())
    }

    // ts: start. Source async prefix executes immediately, continuation after await.
    pub fn start(self: &Rc<Self>) -> ExchangeFuture<'static, Option<bool>> {
        let prefix = (|| {
            let status = self.state().borrow().status;
            if matches!(status, TwapStatus::Completed | TwapStatus::Paused) {
                return Ok(None);
            }
            let state = self.state();
            let order_size = {
                let mut s = state.borrow_mut();
                if s.orders_sizes.is_empty() {
                    None
                } else {
                    Some(s.orders_sizes.remove(0))
                }
            };
            let options = self.options();
            let o = options.borrow();
            let pace = o.duration / o.orders_count;
            let randomness = o.randomness_percentage;
            drop(o);
            let rand = safe_cex_num::lodash_random(
                &safe_cex_num::JsPrimitive::Number(-randomness),
                &safe_cex_num::JsPrimitive::Number(randomness),
                &safe_cex_num::JsPrimitive::Undefined,
                || self.host.runtime().random(),
            )
            .map_err(|e| ExchangeError::new(e.message))?;
            let rand = safe_cex_num::multiply(rand, 0.01);
            let wait = pace * 60.0 * 1000.0;
            let wait_random = wait + wait * rand;
            let next_order_at = self.host.now_ms() + wait_random;
            if self.state().borrow().status == TwapStatus::Idle {
                self.update_state(TwapStateUpdate {
                    status: Some(TwapStatus::Running),
                    next_order_at: Some(self.host.now_ms()),
                    started_at: Some(self.host.now_ms()),
                    ..Default::default()
                })?;
            }
            let executed = self.state().borrow().orders_executed;
            Ok(Some((order_size, wait_random, next_order_at, executed)))
        })();
        let (size, wait, next, executed) = match prefix {
            Ok(Some(p)) => p,
            Ok(None) => return Box::pin(async { Ok(None) }),
            Err(e) => return Box::pin(async { Err(e) }),
        };
        if let Some(size) = size.filter(|n| truthy(*n)) {
            let placement = self.place_order(size);
            let instance = self.clone();
            Box::pin(async move {
                if let Err(error) = placement.await {
                    let symbol = instance.options().borrow().active_market.symbol.clone();
                    instance.host.emit_log(
                        instance.exchange.clone(),
                        format!("TWAP {symbol} slice failed: {}", error.message),
                        "error",
                    )?;
                }
                instance.finish_start(executed + 1.0, next, wait)
            })
        } else {
            let result = self.finish_start(executed, next, wait);
            Box::pin(async move { result })
        }
    }
    fn finish_start(
        self: &Rc<Self>,
        executed: f64,
        next: f64,
        wait: f64,
    ) -> Result<Option<bool>, ExchangeError> {
        if self.state().borrow().orders_sizes.is_empty() {
            self.update_state(TwapStateUpdate {
                status: Some(TwapStatus::Completed),
                orders_executed: Some(executed),
                ..Default::default()
            })?;
            self.host.manager().remove(&self.id());
            return Ok(Some(false));
        }
        // ts-bug: do not guard against pause/terminate while placement was pending.
        self.update_state(TwapStateUpdate {
            orders_executed: Some(executed),
            next_order_at: Some(next),
            ..Default::default()
        })?;
        let instance = self.clone();
        let timer = self.host.set_timeout(
            Rc::new(move || {
                let future = instance.start();
                instance
                    .host
                    .spawn(Box::pin(async move { future.await.map(|_| ()) }));
            }),
            wait,
        );
        *self.timer_id.borrow_mut() = Some(timer);
        Ok(Some(true))
    }
    pub fn pause(&self) -> ExchangeFuture<'static, ()> {
        let result = (|| {
            if self.state().borrow().status == TwapStatus::Running {
                self.update_state(TwapStateUpdate {
                    status: Some(TwapStatus::Paused),
                    ..Default::default()
                })?;
                let timer = self.timer_id.borrow().clone();
                if let Some(timer) = timer.filter(|t| self.host.timer_is_truthy(t)) {
                    self.host.clear_timeout(&timer);
                }
            }
            Ok(())
        })();
        Box::pin(async move { result })
    }
    pub fn resume(self: &Rc<Self>) -> ExchangeFuture<'static, ()> {
        let result = (|| {
            if self.state().borrow().status == TwapStatus::Paused {
                let previous = self.state().borrow().next_order_at;
                let next = if previous > self.host.now_ms() {
                    self.state().borrow().next_order_at
                } else {
                    self.host.now_ms()
                };
                let delay = next - self.host.now_ms();
                let instance = self.clone();
                let timer = self.host.set_timeout(
                    Rc::new(move || {
                        let future = instance.start();
                        instance
                            .host
                            .spawn(Box::pin(async move { future.await.map(|_| ()) }));
                    }),
                    delay,
                );
                *self.timer_id.borrow_mut() = Some(timer);
                self.update_state(TwapStateUpdate {
                    status: Some(TwapStatus::Running),
                    ..Default::default()
                })?;
            }
            Ok(())
        })();
        Box::pin(async move { result })
    }
    pub fn terminate(&self) -> ExchangeFuture<'static, ()> {
        let result = (|| {
            if self.state().borrow().status != TwapStatus::Completed {
                self.update_state(TwapStateUpdate {
                    status: Some(TwapStatus::Completed),
                    ..Default::default()
                })?;
                let timer = self.timer_id.borrow().clone();
                if let Some(timer) = timer.filter(|t| self.host.timer_is_truthy(t)) {
                    self.host.clear_timeout(&timer);
                }
                let mut index = 0;
                loop {
                    let timer = self.slice_timers.borrow().get(index).cloned();
                    let Some(timer) = timer else {
                        break;
                    };
                    self.host.clear_timeout(&timer);
                    index += 1;
                }
                self.slice_timers.borrow_mut().clear();
            }
            self.host.manager().remove(&self.id());
            Ok(())
        })();
        Box::pin(async move { result })
    }
    pub fn cleanup(&self) -> ExchangeFuture<'static, ()> {
        let result = self.update_state(TwapStateUpdate {
            mark_for_cleanup: Some(true),
            ..Default::default()
        });
        Box::pin(async move { result })
    }
}
fn truthy(number: f64) -> bool {
    number != 0.0 && !number.is_nan()
}
