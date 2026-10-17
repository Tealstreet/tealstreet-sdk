use crate::native_value::NativeValue;
use safe_cex_exchange::{
    delivery_timers::WorkerPortTimers,
    message_stats::MessageHandlerStatsPostprocessor,
    store_delivery::{StoreUpdateThrottle, StoreUpdateThrottleHost},
    store_payload::StorePayloadPreparer,
};
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::Store;
use safe_cex_types::StoreData;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

struct NativeStoreTimers {
    runtime: Rc<dyn ExchangeRuntime>,
}
impl WorkerPortTimers for NativeStoreTimers {
    type Timer = Rc<dyn ExchangeTask>;
    fn now_ms(&self) -> f64 {
        self.runtime.now_ms()
    }
    fn set_timeout(&self, callback: Rc<dyn Fn()>, delay_ms: f64) -> Self::Timer {
        let delay = if delay_ms.is_finite() && delay_ms > 0.0 {
            delay_ms as u64
        } else {
            0
        };
        let sleep = self.runtime.sleep(delay);
        let runtime = self.runtime.clone();
        self.runtime
            .spawn_local(Box::pin(async move {
                match sleep.await {
                    Ok(()) => callback(),
                    Err(error) => runtime
                        .report_exception(&error, &json!({"module":"native account Store timer"})),
                }
            }))
            .into()
    }
    fn clear_timeout(&self, timer: &Self::Timer) {
        timer.cancel();
    }
    fn timer_is_truthy(&self, _: &Self::Timer) -> bool {
        true
    }
}
impl StoreUpdateThrottleHost for NativeStoreTimers {
    fn report_store_timer_error(&self, error: &ExchangeError) {
        self.runtime
            .report_exception(error, &json!({"module":"native account Store delivery"}));
    }
}

pub(super) struct NativeStoreDelivery {
    active: Cell<bool>,
    exchange: Weak<dyn Exchange>,
    store: Weak<dyn Store>,
    preparer: RefCell<StorePayloadPreparer>,
    post: Rc<dyn Fn(NativeValue)>,
    throttle: Rc<StoreUpdateThrottle<NativeStoreTimers>>,
}
impl NativeStoreDelivery {
    pub fn new(
        exchange: &Rc<dyn Exchange>,
        store: &Rc<dyn Store>,
        post: Rc<dyn Fn(NativeValue)>,
    ) -> Rc<Self> {
        let options = exchange.base().options();
        let multiplier = options
            .tuning
            .as_ref()
            .and_then(|t| t.data_update_throttle_multiplier)
            .filter(|value| *value != 0.0 && !value.is_nan())
            .unwrap_or(1.0);
        Rc::new_cyclic(|weak: &Weak<Self>| {
            let delivery = weak.clone();
            Self {
                active: Cell::new(true),
                exchange: Rc::downgrade(exchange),
                store: Rc::downgrade(store),
                preparer: RefCell::default(),
                post,
                throttle: StoreUpdateThrottle::new(
                    Rc::new(NativeStoreTimers {
                        runtime: exchange.base().runtime.clone(),
                    }),
                    250.0 * multiplier,
                    Rc::new(move |data| {
                        if let Some(delivery) = delivery.upgrade() {
                            delivery.deliver(data.borrow().clone(), false)?;
                        }
                        Ok(())
                    }),
                ),
            }
        })
    }
    fn deliver(&self, mut data: StoreData, full: bool) -> Result<(), ExchangeError> {
        if !self.active.get() {
            return Ok(());
        }
        let (Some(store), Some(exchange)) = (self.store.upgrade(), self.exchange.upgrade()) else {
            return Ok(());
        };
        MessageHandlerStatsPostprocessor.process(&mut data, exchange.as_ref())?;
        let payload = self.preparer.borrow_mut().prepare(
            &data,
            store.get_markets_version(),
            store.get_tickers_version(),
            exchange.base().options().hosted_mode == Some(true),
            Some(full),
        );
        let value = NativeValue::encode(&payload)?;
        (self.post)(value);
        Ok(())
    }
    pub fn update(&self, data: &StoreData) -> Result<(), ExchangeError> {
        if self.active.get() {
            self.throttle.call(Rc::new(RefCell::new(data.clone())))?;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<(), ExchangeError> {
        self.throttle.cancel();
        if let Some(exchange) = self.exchange.upgrade()
            && !exchange.base().is_disposed()
            && let Some(store) = self.store.upgrade()
        {
            self.deliver(store.serialize(), true)?;
        }
        Ok(())
    }
    pub fn cancel(&self) {
        self.active.set(false);
        self.throttle.cancel();
    }
}
