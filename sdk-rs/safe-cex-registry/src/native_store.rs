use crate::native_value::NativeValue;
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_typed, to_typed},
};
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
            .spawn_classified(
                safe_cex_exchange::runtime::RuntimeTaskKind::Timer,
                Box::pin(async move {
                    match sleep.await {
                        Ok(()) => callback(),
                        Err(error) => runtime.report_exception(
                            &error,
                            &json!({"module":"native account Store timer"}).into(),
                        ),
                    }
                }),
            )
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
        self.runtime.report_exception(
            error,
            &json!({"module":"native account Store delivery"}).into(),
        );
    }
}

pub(super) struct NativeStoreDelivery {
    active: Cell<bool>,
    exchange: Weak<dyn Exchange>,
    store: Weak<dyn Store>,
    preparer: RefCell<StorePayloadPreparer>,
    post: Rc<dyn Fn(NativeValue)>,
    throttle: Rc<StoreUpdateThrottle<NativeStoreTimers, Js>>,
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
                throttle: StoreUpdateThrottle::new_with_argument(
                    Rc::new(NativeStoreTimers {
                        runtime: exchange.base().runtime.clone(),
                    }),
                    250.0 * multiplier,
                    Rc::new(move |data| {
                        if let Some(delivery) = delivery.upgrade() {
                            delivery.deliver(data, false)?;
                        }
                        Ok(())
                    }),
                ),
            }
        })
    }
    fn deliver(&self, data: Js, full: bool) -> Result<(), ExchangeError> {
        if !self.active.get() {
            return Ok(());
        }
        let (Some(store), Some(exchange)) = (self.store.upgrade(), self.exchange.upgrade()) else {
            return Ok(());
        };
        let data = data.spread_object()?;
        // Only the diagnostic processor needs a typed field. Model rows remain
        // their original objects all the way through preparation and post capture.
        let mut statistics = to_typed(&data.get("messageHandlerStats")?)?;
        MessageHandlerStatsPostprocessor.process(&mut statistics, exchange.as_ref())?;
        data.set("messageHandlerStats", from_typed(&statistics)?)?;
        let payload = self.preparer.borrow_mut().prepare(
            &data,
            store.get_markets_version(),
            store.get_tickers_version(),
            exchange.base().options().hosted_mode == Some(true),
            Some(full),
        )?;
        let value = NativeValue::from_js(&payload)?;
        (self.post)(value);
        Ok(())
    }
    pub fn update(&self, data: &dyn std::any::Any) -> Result<(), ExchangeError> {
        if self.active.get() {
            let data = data
                .downcast_ref::<Js>()
                .ok_or_else(|| ExchangeError::new("Unsupported native Store state"))?;
            self.throttle.call(data.clone())?;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<(), ExchangeError> {
        self.throttle.cancel();
        if let Some(exchange) = self.exchange.upgrade()
            && !exchange.base().is_disposed()
            && let Some(store) = self.store.upgrade()
        {
            self.deliver(
                store::facade(store)?.call_method("serialize", vec![])?,
                true,
            )?;
        }
        Ok(())
    }
    pub(super) fn reset_replay_throttle(&self) {
        self.throttle.cancel();
    }
    pub fn cancel(&self) {
        self.active.set(false);
        self.throttle.cancel();
    }
}

#[cfg(test)]
mod construction_order_tests {
    use super::*;
    use crate::{
        native_host::{hyperliquid_rate_limiter, native_venue_host},
        rust_venues::rust_constructor,
    };
    use safe_cex_exchange::{js_runtime::store, native_store::DefaultStore};
    use safe_cex_types::ExchangeName;

    #[test]
    fn actual_native_delivery_preserves_all_model_construction_orders() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
            tokio::task::LocalSet::new().run_until(async {
                let output=std::process::Command::new("node")
                    .arg(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/native_store_order_source.mjs"))
                    .output().unwrap();
                assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
                let source: serde_json::Value=serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(source["cases"].as_array().unwrap().len(),6);
                for (index,case) in source["cases"].as_array().unwrap().iter().enumerate() {
                    let store: Rc<dyn Store>=Rc::new(DefaultStore::new());
                    let host=native_venue_host(hyperliquid_rate_limiter());
                    let exchange=rust_constructor(&ExchangeName::Binance,host).unwrap().construct(
                        ExchangeName::Binance,serde_json::from_value(json!({"key":"","secret":"","hostedMode":true})).unwrap(),store.clone()).unwrap();
                    let posts=Rc::new(RefCell::new(Vec::new()));
                    let captured=posts.clone();
                    let delivery=NativeStoreDelivery::new(&exchange,&store,Rc::new(move|value|captured.borrow_mut().push(value)));
                    let callback=delivery.clone();
                    let unsubscribe=store.subscribe_native(Box::new(move|data,_| callback.update(data).map_err(Into::into)));
                    let facade=store::facade(store.clone()).unwrap();
                    facade.call_method("update",vec![NativeValue::from(case["input"].clone()).to_js()]).unwrap();
                    let actual=posts.borrow();
                    assert_eq!(actual.len(),1,"MODEL-CONSTRUCTION-ORDER leading delivery {index}");
                    let expected=NativeValue::from(case["expected"].clone());
                    assert_eq!(actual[0],expected,"MODEL-CONSTRUCTION-ORDER actual native constructor/Store/worker payload {index}");
                    drop(actual);unsubscribe();delivery.cancel();exchange.dispose().unwrap();
                }
            })
        );
    }
}
