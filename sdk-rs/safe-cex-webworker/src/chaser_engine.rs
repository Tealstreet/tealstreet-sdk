//! Actual placement, reconciliation, completion and monitoring bodies. Missing
//! partial-cancel and optional-fill ABI are explicit venue leaf dependencies.
use crate::{
    chaser::{ChaserOrderInstance, placement_filled},
    chaser_manager::{self, ChaserOrderManager, ChaserSupervisor},
    task_types::{CHASER_UNCONFIRMED_ORDER_STOP_REASON, ChaserCompleteEvent},
};
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::js_runtime::value::{Js, to_typed};
use safe_cex_exchange::{
    client_order_id::ClientOrderLookup,
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_exchange::{eager::start_eager, js_runtime::promise::await_js};
use safe_cex_types::{
    ModelField, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PlaceOrderOpts,
    PositionSide,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[derive(Debug, Clone)]
pub enum ChaserCancel {
    // `Order` is boxed to keep this enum's storage bounded as Order grows —
    // the same reason safe-cex-types boxes GatewayTickerData's singular Ticker
    // variant. ChaserCancel is webworker-internal and carries no Serialize or
    // Deserialize, so this cannot change a wire shape, and `Full(o)` patterns
    // still read `o.id` through Box deref.
    Full(Box<Order>),
    ById { id: String, symbol: String },
}
pub type FillLookup =
    Rc<dyn Fn(Rc<dyn Exchange>, Vec<String>) -> ExchangeFuture<'static, Vec<(String, f64)>>>;
pub trait ChaserHost: WorkerPortTimers + 'static {
    fn construct_place(
        &self,
        exchange: Rc<dyn Exchange>,
        options: PlaceOrderOpts,
    ) -> ExchangeFuture<'static, Vec<String>>;
    fn construct_lookup(
        &self,
        exchange: Rc<dyn Exchange>,
        symbol: String,
        client_id: String,
    ) -> ExchangeFuture<'static, ClientOrderLookup>;
    fn construct_cancel(
        &self,
        exchange: Rc<dyn Exchange>,
        orders: Vec<ChaserCancel>,
    ) -> ExchangeFuture<'static, ()>;
    /// Pure property lookup. None is missing/noncallable; successful empty is not None.
    fn fill_lookup(&self, exchange: Rc<dyn Exchange>) -> Option<FillLookup>;
    fn construct_sleep(
        &self,
        exchange: Rc<dyn Exchange>,
        delay_ms: f64,
    ) -> ExchangeFuture<'static, ()>;
    fn error_properties(&self, error: &ExchangeError) -> Value;
    fn console_error(&self, error: &ExchangeError) -> Result<(), ExchangeError>;
    fn emit_completion(
        &self,
        exchange: Rc<dyn Exchange>,
        event: ChaserCompleteEvent,
    ) -> Result<(), ExchangeError>;
    /// Own the future and settle/report it, never drop detached source work.
    fn spawn(&self, future: ExchangeFuture<'static, ()>);
}
pub struct ChaserEngine<H: ChaserHost> {
    pub host: Rc<H>,
    pub manager: Rc<ChaserOrderManager>,
}
impl<H: ChaserHost> ChaserEngine<H> {
    pub fn new(host: Rc<H>, manager: Rc<ChaserOrderManager>) -> Rc<Self> {
        Rc::new(Self { host, manager })
    }
    fn emit_error(instance: &ChaserOrderInstance, message: &str) -> Result<(), ExchangeError> {
        instance.exchange.base().emit(
            "error",
            safe_cex_exchange::js_runtime::value::Js::str(message),
        )
    }
    fn cancel_input(
        instance: &ChaserOrderInstance,
        id: &str,
    ) -> Result<ChaserCancel, ExchangeError> {
        let order = instance
            .exchange
            .base()
            .store
            .orders()?
            .iter()
            .find(|o| o.id == id)
            .cloned();
        Ok(order
            .map(|o| ChaserCancel::Full(Box::new(o)))
            .unwrap_or_else(|| ChaserCancel::ById {
                id: id.into(),
                symbol: instance.options().borrow().symbol.clone(),
            }))
    }
    // ts: cleanupInstance. Execute all pre-await work at construction time.
    pub fn cleanup_instance(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        let owns = instance.claim_complete_emission();
        instance.dispose();
        let list = self.manager.instances();
        let index = list.borrow().iter().position(|i| Rc::ptr_eq(i, &instance));
        if let Some(index) = index {
            list.borrow_mut().remove(index);
        }
        let ids = instance.current_order_ids();
        if !ids.is_empty() {
            instance.set_current_order_ids(vec![]);
        }
        let first = ids
            .first()
            .map(|id| {
                Self::cancel_input(&instance, id).map(|input| {
                    self.host
                        .construct_cancel(instance.exchange.clone(), vec![input])
                })
            })
            .transpose();
        // Source cleanupInstance catches a failure evaluating its cancel-loop
        // input, then still emits completion (ChaserOrderManager.ts:884-903).
        let immediate = if !matches!(&first, Ok(Some(_))) && owns {
            Some(self.emit_complete(instance.clone()))
        } else {
            None
        };
        let engine = self.clone();
        Box::pin(async move {
            if let Ok(Some(first)) = first {
                let _ = first.await;
                for id in ids.iter().skip(1) {
                    let Ok(input) = Self::cancel_input(&instance, id) else {
                        break;
                    };
                    let cancel = engine
                        .host
                        .construct_cancel(instance.exchange.clone(), vec![input]);
                    let _ = cancel.await;
                }
                if owns {
                    engine.emit_complete(instance).await?;
                }
            } else if let Some(complete) = immediate {
                complete.await?;
            }
            Ok(())
        })
    }
    // ts: emitComplete. Capture stop reason/IDs before reentrant fill updates.
    pub fn emit_complete(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        let runtime = instance.exchange.base().runtime.clone();
        let reason = instance.state().borrow().stop_reason.clone();
        let ids: Vec<String> = instance
            .ever_tracked_order_ids()
            .borrow()
            .iter()
            .cloned()
            .collect();
        let cached: Result<(), ExchangeError> = (|| {
            for id in &ids {
                let order = instance
                    .exchange
                    .base()
                    .store
                    .orders()?
                    .iter()
                    .find(|o| &o.id == id)
                    .cloned();
                if let Some(order) = order {
                    instance.set_filled_amount(&order.id, placement_filled(&order));
                }
            }
            Ok(())
        })();
        if let Err(error) = cached {
            return start_eager(&runtime, async move { Err(error) });
        }
        let method = self.host.fill_lookup(instance.exchange.clone());
        let eligible = instance
            .options()
            .borrow()
            .client_id
            .as_ref()
            .is_some_and(|s| !s.is_empty())
            && !ids.is_empty()
            && fill_tracked(&instance.exchange.base().name);
        let pending = if eligible {
            method.map(|f| f(instance.exchange.clone(), ids))
        } else {
            None
        };
        let engine = self.clone();
        if let Some(pending) = pending {
            start_eager(&runtime, async move {
                let result = pending.await;
                await_js(&instance.exchange.base().runtime, Js::Undefined)
                    .await
                    .map_err(|e| e.to_exchange_error())?;
                let reconciled = match result {
                    Ok(fills) => {
                        for (id, n) in fills {
                            instance.set_filled_amount(&id, n);
                        }
                        true
                    }
                    Err(e) => {
                        chaser_manager::log(
                            &format!(
                                "chaser:complete fill reconciliation failed: {}",
                                e.js_error_string()
                            ),
                            &instance,
                        )?;
                        false
                    }
                };
                engine.emit_complete_event(instance, &reason, reconciled)
            })
        } else {
            let result = self.emit_complete_event(instance, &reason, false);
            start_eager(&runtime, async move { result })
        }
    }
    fn emit_complete_event(
        &self,
        instance: Rc<ChaserOrderInstance>,
        reason: &str,
        reconciled: bool,
    ) -> Result<(), ExchangeError> {
        let options = instance.options();
        let options = options.borrow().clone();
        let filled = if reconciled {
            instance.filled_size()
        } else {
            safe_cex_num::math_max(
                instance.filled_size(),
                chaser_manager::bybit_demo_position_fill(&instance)?,
            )
        };
        let event = ChaserCompleteEvent {
            id: instance.id().into(),
            client_id: options.client_id,
            symbol: options.symbol,
            side: options.side,
            reduce_only: options.reduce_only == Some(true),
            size: options.size,
            filled,
            stop_reason: reason.into(),
        };
        if let Err(e) = self.host.emit_completion(instance.exchange.clone(), event) {
            chaser_manager::log(
                &format!("chaser:complete listener threw: {}", e.js_error_string()),
                &instance,
            )?;
        }
        Ok(())
    }
    fn lookup(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
        id: String,
    ) -> ExchangeFuture<'static, Option<Order>> {
        let symbol = instance.options().borrow().symbol.clone();
        let pending = self
            .host
            .construct_lookup(instance.exchange.clone(), symbol, id);
        Box::pin(async move {
            match pending.await {
                Ok(ClientOrderLookup::Found(order)) => Ok(Some(*order)),
                Ok(ClientOrderLookup::NotFound { .. }) => Ok(None),
                Err(e) => {
                    chaser_manager::log(
                        &format!("client order id lookup failed: {}", e.js_error_string()),
                        &instance,
                    )?;
                    Ok(None)
                }
            }
        })
    }
    // ts: findLostPlacement. A stop at the next iteration performs one final lookup.
    pub fn find_lost_placement(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
        id: String,
        delays: Vec<f64>,
    ) -> ExchangeFuture<'static, Option<Order>> {
        enum First {
            Lookup(ExchangeFuture<'static, Option<Order>>, bool),
            Sleep(ExchangeFuture<'static, ()>),
            Empty,
        }
        let first = match delays.first() {
            None => First::Empty,
            Some(_) if instance.is_stopped() => {
                First::Lookup(self.lookup(instance.clone(), id.clone()), true)
            }
            // `!(*delay > 0.)` is NOT `*delay <= 0.`: a NaN delay must take the
            // immediate-lookup branch, as it does in the source. The rewritten
            // comparison would fall through to the sleep branch instead.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            Some(delay) if !(*delay > 0.) => {
                First::Lookup(self.lookup(instance.clone(), id.clone()), false)
            }
            Some(delay) => {
                First::Sleep(self.host.construct_sleep(instance.exchange.clone(), *delay))
            }
        };
        let engine = self.clone();
        Box::pin(async move {
            match first {
                First::Empty => return Ok(None),
                First::Lookup(f, stop) => {
                    let order = f.await?;
                    if stop || order.is_some() {
                        return Ok(order);
                    }
                }
                First::Sleep(f) => {
                    f.await?;
                    let order = engine.lookup(instance.clone(), id.clone()).await?;
                    if order.is_some() {
                        return Ok(order);
                    }
                }
            }
            for delay in delays.into_iter().skip(1) {
                if instance.is_stopped() {
                    return engine.lookup(instance, id).await;
                }
                if delay > 0. {
                    engine
                        .host
                        .construct_sleep(instance.exchange.clone(), delay)
                        .await?;
                }
                let order = engine.lookup(instance.clone(), id.clone()).await?;
                if order.is_some() {
                    return Ok(order);
                }
            }
            Ok(None)
        })
    }
    // ts: tryPlaceOrders. Pre-placement guards/mint/log execute before the first await.
    pub fn try_place_orders(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
        tries: f64,
    ) -> ExchangeFuture<'static, Vec<String>> {
        let prefix: Result<
            Option<(Option<String>, ExchangeFuture<'static, Vec<String>>)>,
            ExchangeError,
        > = (|| {
            let max = if instance.exchange.base().name == safe_cex_types::ExchangeName::Binance {
                20.
            } else {
                10.
            };
            if instance.is_stopped() {
                chaser_manager::log("tryPlaceOrders early-exit: instance is stopped", &instance)?;
                return Ok(None);
            }
            let size = instance.remaining_size();
            if size == 0. || size.is_nan() {
                chaser_manager::log("size is 0", &instance)?;
                instance.mark_complete(Some("No remaining size. User likely cancelled order"));
                Self::emit_error(
                    &instance,
                    "Chaser has no remaining size. User likely cancelled order",
                )?;
                return Err(ExchangeError::new("size is 0"));
            }
            if tries >= max {
                chaser_manager::log(
                    &format!("failed to place orders after {} tries", num(max)),
                    &instance,
                )?;
                Self::emit_error(
                    &instance,
                    &format!("Chaser failed to place orders after {} tries", num(max)),
                )?;
                instance.mark_complete(Some(&format!(
                    "Failed to place orders after {} tries",
                    num(max)
                )));
                chaser_manager::check_and_mark_if_complete(&instance)?;
                self.host.spawn(self.cleanup_instance(instance.clone()));
                return Ok(None);
            }
            instance.set_fresh_order(tries == 1.);
            let price = chaser_manager::find_best_price(&instance)?;
            chaser_manager::log(
                &format!(
                    "tryPlaceOrders try={} price={} size={} tickDist={}",
                    num(tries),
                    num(price),
                    num(instance.remaining_size()),
                    num(instance.min_tick_distance())
                ),
                &instance,
            )?;
            let options = instance.options();
            let options = options.borrow().clone();
            let mut order = PlaceOrderOpts {
                symbol: options.symbol,
                side: options.side,
                r#type: OrderType::Limit,
                price: Some(price),
                amount: instance.remaining_size(),
                reduce_only: options.reduce_only,
                time_in_force: Some(OrderTimeInForce::PostOnly),
                trigger_price: None,
                stop_loss: None,
                take_profit: None,
                trigger_type: None,
                tp_trigger_type: None,
                sl_trigger_type: None,
                client_order_id: None,
                max_slippage_pct: None,
                close: None,
                implied_volatility: None,
            };
            let id = instance.exchange.base().mint_client_order_id(&order)?;
            if id.as_ref().is_some_and(|id| !id.is_empty()) {
                order.client_order_id = id.clone();
            }
            let placement = self.host.construct_place(instance.exchange.clone(), order);
            Ok(Some((id, placement)))
        })();
        let engine = self.clone();
        let runtime = instance.exchange.base().runtime.clone();
        start_eager(&runtime, async move {
            let Some((client_id, placement)) = prefix? else {
                return Ok(vec![]);
            };
            let mut ids = vec![];
            let mut error = None;
            let placed = placement.await;
            await_js(&instance.exchange.base().runtime, Js::Undefined)
                .await
                .map_err(|e| e.to_exchange_error())?;
            let result = match placed {
                Ok(returned) => {
                    ids = returned;
                    chaser_manager::log(
                        &format!(
                            "placeOrder returned: {}",
                            serde_json::to_string(&ids).expect("string ids")
                        ),
                        &instance,
                    )
                }
                Err(e) => Err(e),
            };
            if let Err(e) = result {
                engine.host.console_error(&e)?;
                chaser_manager::log(
                    &format!("placeOrder exception: {}", e.js_error_string()),
                    &instance,
                )?;
                error = Some(e);
            }
            if let Some(id) = client_id.filter(|id| !id.is_empty()) {
                if ids.is_empty() {
                    let lost = if let Some(e) = &error {
                        safe_cex_store::remaining_utils::is_unknown_outcome(
                            &engine.host.error_properties(e),
                        )
                        .map_err(ExchangeError::new)?
                    } else {
                        false
                    };
                    if lost || error.is_none() {
                        let order = engine
                            .find_lost_placement(
                                instance.clone(),
                                id,
                                if lost {
                                    vec![1000., 2000., 4000., 8000.]
                                } else {
                                    vec![0.]
                                },
                            )
                            .await;
                        await_js(&instance.exchange.base().runtime, Js::Undefined)
                            .await
                            .map_err(|e| e.to_exchange_error())?;
                        if let Some(order) = order? {
                            if !matches!(
                                order.status,
                                ModelField::Value(OrderStatus::Closed | OrderStatus::Canceled)
                            ) {
                                ids = vec![order.id];
                            } else {
                                instance.set_filled_amount(&order.id, placement_filled(&order));
                                if instance.is_stopped() {
                                    return Ok(vec![]);
                                }
                            }
                        } else if lost {
                            let message = format!(
                                "Chaser for {} stopped: could not confirm whether its order was placed — check open orders",
                                instance.options().borrow().symbol
                            );
                            chaser_manager::log(&message, &instance)?;
                            Self::emit_error(&instance, &message)?;
                            if !instance.is_stopped() {
                                instance.mark_complete(Some(CHASER_UNCONFIRMED_ORDER_STOP_REASON));
                                chaser_manager::check_and_mark_if_complete(&instance)?;
                                engine.host.spawn(engine.cleanup_instance(instance.clone()));
                            }
                            return Ok(vec![]);
                        }
                    }
                }
            }
            if instance.is_stopped() && !ids.is_empty() {
                chaser_manager::log(
                    &format!(
                        "isStopped after placeOrder; cancelling just-placed {}",
                        serde_json::to_string(&ids).expect("string ids")
                    ),
                    &instance,
                )?;
                for id in ids {
                    let symbol = instance.options().borrow().symbol.clone();
                    let _ = engine
                        .host
                        .construct_cancel(
                            instance.exchange.clone(),
                            vec![ChaserCancel::ById { id, symbol }],
                        )
                        .await;
                    await_js(&instance.exchange.base().runtime, Js::Undefined)
                        .await
                        .map_err(|e| e.to_exchange_error())?;
                }
                return Ok(vec![]);
            }
            if instance.is_stopped() {
                return Ok(vec![]);
            }
            if !ids.is_empty() {
                return Ok(ids);
            }
            instance.set_min_tick_distance(instance.min_tick_distance() + 1.);
            let delay =
                if instance.exchange.base().name == safe_cex_types::ExchangeName::Hyperliquid {
                    500.
                } else {
                    100.
                };
            let slept = engine
                .host
                .construct_sleep(instance.exchange.clone(), delay)
                .await;
            await_js(&instance.exchange.base().runtime, Js::Undefined)
                .await
                .map_err(|e| e.to_exchange_error())?;
            slept?;
            if instance.is_stopped() {
                return Ok(vec![]);
            }
            engine.try_place_orders(instance, tries + 1.).await
        })
    }
}
fn num(n: f64) -> String {
    safe_cex_num::js_number_string(n)
}
fn fill_tracked(name: &safe_cex_types::ExchangeName) -> bool {
    matches!(
        name,
        safe_cex_types::ExchangeName::Hyperliquid
            | safe_cex_types::ExchangeName::Grvt
            | safe_cex_types::ExchangeName::Bybitdemo
    )
}

impl<H: ChaserHost> ChaserEngine<H> {
    // ts: processInstance. Log prefix executes before returning the startup future.
    pub fn process_instance(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        let message = format!(
            "processInstance starting for {}",
            instance.options().borrow().symbol
        );
        let prefix = chaser_manager::log(&message, &instance);
        let waiting = prefix.map(|_| {
            let exchange = instance.exchange.clone();
            prime_future(Box::pin(async move {
                chaser_manager::wait_for_bybit_demo_private_ws(exchange.as_ref()).await
            }))
        });
        let engine = self.clone();
        Box::pin(async move {
            let reason = waiting?.await?;
            if let Some(reason) = reason {
                let message = format!(
                    "Chaser for {} is reconciling over REST: the Bybit Demo private WebSocket is not running - {reason}. Fills are recovered from REST fill history and position size, so completion can lag by a poll interval. A VPN or Tealstreet IP routing usually restores the private socket.",
                    instance.options().borrow().symbol
                );
                chaser_manager::log(&message, &instance)?;
                instance.exchange.base().emit(
                    "warning",
                    safe_cex_exchange::js_runtime::value::Js::str(&message),
                )?;
            }
            if instance.is_stopped() {
                engine.cleanup_instance(instance).await?;
                return Ok(());
            }
            let ids = engine.try_place_orders(instance.clone(), 1.).await?;
            instance.set_current_order_ids(ids);
            chaser_manager::log(
                &format!(
                    "initial orderIds: {}",
                    serde_json::to_string(&instance.current_order_ids()).expect("string ids")
                ),
                &instance,
            )?;
            engine
                .host
                .construct_sleep(instance.exchange.clone(), 1000.)
                .await?;
            if !instance.current_order_ids().is_empty() {
                let fill = fill_tracked(&instance.exchange.base().name);
                chaser_manager::log(
                    &format!("monitoring started, useFillBasedTracking={fill}"),
                    &instance,
                )?;
                let monitor = Rc::new(ChaserMonitor {
                    engine: engine.clone(),
                    instance: instance.clone(),
                    use_fills: fill,
                    cleaned: Cell::new(false),
                    price_timer: RefCell::new(None),
                    queued_timer: RefCell::new(None),
                    update_listener: RefCell::new(None),
                    fill_listener: RefCell::new(None),
                    has_seen_orders: Cell::new(false),
                    missing_updates: Cell::new(0),
                    queued_check: Cell::new(false),
                    last_orders_id: RefCell::new(Js::Number(-1.0)),
                    last_positions_id: RefCell::new(Js::Number(-1.0)),
                    last_price_check: Cell::new(0.),
                });
                let dispose = monitor.clone();
                instance.register_disposer(Box::new(move || {
                    dispose.cleanup();
                    Ok(())
                }));
                let on_update = monitor.clone();
                let listener = instance.exchange.base().on(
                    "update",
                    Rc::new(move |_, data| {
                        let future = on_update.on_update(data);
                        on_update.spawn_caught(future, "store update");
                        Ok(())
                    }),
                );
                *monitor.update_listener.borrow_mut() = Some(listener);
                if fill {
                    let on_fill = monitor.clone();
                    let listener = instance.exchange.base().on(
                        "fill",
                        Rc::new(move |_, data| {
                            on_fill.on_fill(data);
                            Ok(())
                        }),
                    );
                    *monitor.fill_listener.borrow_mut() = Some(listener);
                }
                return Ok(());
            }
            if !instance.is_stopped() {
                let message = format!(
                    "Chaser for {} could not start - no order was placed",
                    instance.options().borrow().symbol
                );
                instance.exchange.base().emit(
                    "warning",
                    safe_cex_exchange::js_runtime::value::Js::str(&message),
                )?;
                instance.mark_complete(Some("No order placed"));
            }
            engine.cleanup_instance(instance).await
        })
    }
}
impl<H: ChaserHost> ChaserSupervisor for ChaserEngine<H> {
    fn construct_supervision(
        &self,
        manager: Rc<ChaserOrderManager>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        Self::new(self.host.clone(), manager).process_instance(instance)
    }
    fn construct_cleanup(
        &self,
        manager: Rc<ChaserOrderManager>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        Self::new(self.host.clone(), manager).cleanup_instance(instance)
    }
    fn construct_completion(
        &self,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        Self::new(self.host.clone(), self.manager.clone()).emit_complete(instance)
    }
    fn spawn(&self, future: ExchangeFuture<'static, ()>) {
        self.host.spawn(future)
    }
}
struct ChaserMonitor<H: ChaserHost> {
    engine: Rc<ChaserEngine<H>>,
    instance: Rc<ChaserOrderInstance>,
    use_fills: bool,
    cleaned: Cell<bool>,
    price_timer: RefCell<Option<H::Timer>>,
    queued_timer: RefCell<Option<H::Timer>>,
    update_listener: RefCell<Option<String>>,
    fill_listener: RefCell<Option<String>>,
    has_seen_orders: Cell<bool>,
    missing_updates: Cell<u32>,
    queued_check: Cell<bool>,
    last_orders_id: RefCell<Js>,
    last_positions_id: RefCell<Js>,
    last_price_check: Cell<f64>,
}
impl<H: ChaserHost> ChaserMonitor<H> {
    fn clear_timer(&self, slot: &RefCell<Option<H::Timer>>) {
        let timer = slot.borrow().clone();
        if let Some(timer) = timer.filter(|t| self.engine.host.timer_is_truthy(t)) {
            self.engine.host.clear_timeout(&timer);
            *slot.borrow_mut() = None;
        }
    }
    fn cleanup(&self) {
        if self.cleaned.replace(true) {
            return;
        }
        self.clear_timer(&self.price_timer);
        self.clear_timer(&self.queued_timer);
        if let Some(id) = self.update_listener.borrow().clone() {
            self.instance.exchange.base().off("update", Some(&id));
        }
        if self.use_fills {
            if let Some(id) = self.fill_listener.borrow().clone() {
                self.instance.exchange.base().off("fill", Some(&id));
            }
        }
    }
    fn spawn_caught(self: &Rc<Self>, future: ExchangeFuture<'static, ()>, phase: &'static str) {
        let engine = self.engine.clone();
        let instance = self.instance.clone();
        self.engine.host.spawn(Box::pin(async move {
            if let Err(error) = future.await {
                engine
                    .manager
                    .handle_supervisor_error(instance, error.into(), phase, engine.clone())
                    .await?;
            }
            Ok(())
        }));
    }
    fn apply_fill_state(self: &Rc<Self>, orders: &[Order]) -> ExchangeFuture<'static, bool> {
        for order in orders {
            let post_only = order.status == OrderStatus::Canceled
                && order.extra.as_ref().and_then(|v| v.reject_reason)
                    == Some(safe_cex_types::OrderRejectReason::PostOnly);
            if !post_only {
                self.instance.subtract_filled_size(order);
            }
        }
        if orders.iter().all(|o| o.status == OrderStatus::Closed) {
            let result = chaser_manager::log(
                "all orders are closed (executed) - marking ourselves completed",
                &self.instance,
            );
            if let Err(e) = result {
                return Box::pin(async move { Err(e) });
            }
            self.instance.mark_complete(None);
            self.cleanup();
            let cleanup = self.engine.cleanup_instance(self.instance.clone());
            return Box::pin(async move {
                cleanup.await?;
                Ok(true)
            });
        }
        if self.instance.is_stopped() {
            self.cleanup();
            let cleanup = self.engine.cleanup_instance(self.instance.clone());
            return Box::pin(async move {
                cleanup.await?;
                Ok(true)
            });
        }
        Box::pin(async { Ok(false) })
    }
    fn update_orders(self: &Rc<Self>, orders: Vec<Order>) -> ExchangeFuture<'static, ()> {
        if self.instance.pending_orders_update() {
            self.queued_check.set(true);
            let apply = self.apply_fill_state(&orders);
            return Box::pin(async move {
                apply.await?;
                Ok(())
            });
        }
        self.instance.set_pending_orders_update(true);
        enum Prefix {
            Stop(ExchangeFuture<'static, ()>),
            Continue(Option<f64>, f64, ExchangeFuture<'static, bool>),
        }
        let prefix: Result<Prefix, ExchangeError> = (|| {
            let price = orders
                .iter()
                .find(|o| o.status == OrderStatus::Open)
                .and_then(|o| o.price.value().copied());
            let last = chaser_manager::find_best_price(&self.instance)?;
            let options = self.instance.options();
            let options = options.borrow().clone();
            if options.reduce_only == Some(true) {
                let closing = if options.side == OrderSide::Buy {
                    PositionSide::Short
                } else {
                    PositionSide::Long
                };
                let exists = self
                    .instance
                    .exchange
                    .base()
                    .store
                    .positions()?
                    .iter()
                    .any(|p| p.symbol == options.symbol && p.side == closing && p.notional != 0.);
                if !exists {
                    self.instance.mark_complete(None);
                    self.cleanup();
                    return Ok(Prefix::Stop(
                        self.engine.cleanup_instance(self.instance.clone()),
                    ));
                }
            }
            Ok(Prefix::Continue(
                price,
                last,
                self.apply_fill_state(&orders),
            ))
        })();
        let prefix: Result<Prefix, ExchangeError> = match prefix {
            Ok(prefix) => Ok(prefix),
            Err(error) => {
                self.finish_update();
                return Box::pin(async move { Err(error) });
            }
        };
        let monitor = self.clone();
        Box::pin(async move {
            let result:Result<(),ExchangeError>=async{let(price,last,apply)=match prefix?{Prefix::Stop(f)=>{f.await?;return Ok(())},Prefix::Continue(p,l,f)=>(p,l,f)};let stopped=apply.await?;
    if monitor.instance.fresh_order(){monitor.instance.set_fresh_order(false);if orders.iter().all(|o|o.status==OrderStatus::Canceled){monitor.instance.set_min_tick_distance(monitor.instance.min_tick_distance()+1.);}}
    if stopped{return Ok(())}
    let side=monitor.instance.options().borrow().side;if price.is_some_and(|p|p!=0.&&!p.is_nan()&&((side==OrderSide::Buy&&last<p)||(side==OrderSide::Sell&&last>p))){return Ok(())}
    if monitor.instance.last_partial_fill_at()>0.&&monitor.engine.host.now_ms()-monitor.instance.last_partial_fill_at()<1000.{chaser_manager::log(&format!("hold steady: {}ms since last partial fill",num(monitor.engine.host.now_ms()-monitor.instance.last_partial_fill_at())),&monitor.instance)?;return Ok(())}
    if price==Some(last){return Ok(())} // Option preserves source undefined !== any numeric last.
    monitor.instance.increment_attempts();if monitor.instance.is_stopped(){monitor.cleanup();monitor.engine.cleanup_instance(monitor.instance.clone()).await?;return Ok(())}
    let cancel:Vec<Order>=orders.into_iter().filter(|o|!matches!(o.status,ModelField::Value(OrderStatus::Closed|OrderStatus::Canceled))).collect();if !cancel.is_empty(){chaser_manager::log("cancelling orders",&monitor.instance)?;if let Err(e)=monitor.engine.host.construct_cancel(monitor.instance.exchange.clone(),cancel.into_iter().map(|o| ChaserCancel::Full(Box::new(o))).collect()).await{let message=if e.message.is_empty(){e.js_error_string()}else{e.message};let expected=message.contains("already canceled")||message.contains("already filled")||message.contains("never placed");if monitor.use_fills&&expected{chaser_manager::log(&format!("cancel failed (order may be filled): {message}"),&monitor.instance)?;}else{monitor.cleanup();ChaserEngine::<H>::emit_error(&monitor.instance,"Chaser failed to cancel orders, stopping")?;monitor.instance.mark_complete(Some("Failed to cancel orders"));chaser_manager::log("failed to cancel orders",&monitor.instance)?;return Err(ExchangeError::new("Failed to cancel orders"))}}}
    if monitor.use_fills{let reconcile:Result<(),ExchangeError>=async{let method=monitor.engine.host.fill_lookup(monitor.instance.exchange.clone()).ok_or_else(||ExchangeError::new("fill reconciliation is unavailable"))?;let fills=method(monitor.instance.exchange.clone(),monitor.instance.current_order_ids()).await?;for(id,n)in fills{chaser_manager::log(&format!("REST fills check: orderId={id}, filledAmount={}",num(n)),&monitor.instance)?;monitor.instance.set_filled_amount(&id,n);}if monitor.instance.is_stopped(){chaser_manager::log("fully filled after REST check - marking ourselves completed",&monitor.instance)?;monitor.cleanup();monitor.engine.cleanup_instance(monitor.instance.clone()).await?;}Ok(())}.await;
     if let Err(e)=reconcile{let message=format!("Chaser reconciliation failed after cancellation; stopped before replacement: {}",e.js_error_string());chaser_manager::log(&message,&monitor.instance)?;monitor.instance.mark_complete(Some("Failed to reconcile fills after cancellation"));ChaserEngine::<H>::emit_error(&monitor.instance,&message)?;monitor.engine.cleanup_instance(monitor.instance.clone()).await?;return Ok(())}
     if monitor.instance.is_stopped(){return Ok(())}
    }
    if monitor.instance.is_stopped()||monitor.cleaned.get(){return Ok(())}
    let ids=monitor.engine.try_place_orders(monitor.instance.clone(),1.).await?;monitor.instance.set_current_order_ids(ids);monitor.has_seen_orders.set(false);monitor.engine.host.construct_sleep(monitor.instance.exchange.clone(),1000.).await?;chaser_manager::log("created new orders",&monitor.instance)?;Ok(())
   }.await;
            monitor.finish_update();
            result
        })
    }
    fn finish_update(self: &Rc<Self>) {
        self.instance.set_pending_orders_update(false);
        if self.queued_check.get() && !self.cleaned.get() && !self.instance.is_stopped() {
            self.queued_check.set(false);
            self.schedule_queued();
        }
    }
    fn handle_tracked(
        self: &Rc<Self>,
        orders: Vec<Order>,
        confirm: bool,
    ) -> ExchangeFuture<'static, ()> {
        if !orders.is_empty() {
            self.has_seen_orders.set(true);
            self.missing_updates.set(0);
            return self.update_orders(orders);
        }
        let prefix: Result<Option<ExchangeFuture<'static, ()>>, ExchangeError> = (|| {
            if !self.instance.pending_orders_update() && self.has_seen_orders.get() && confirm {
                let count = self.missing_updates.get() + 1;
                self.missing_updates.set(count);
                if count < 2 {
                    chaser_manager::log(
                        "tracked orders missing once; waiting for confirmation",
                        &self.instance,
                    )?;
                    return Ok(None);
                }
                chaser_manager::log(
                    "tracked orders absent across confirmed order updates; stopping safely",
                    &self.instance,
                )?;
                self.instance
                    .mark_complete(Some("Tracked orders disappeared from store"));
                let message = format!(
                    "Chaser for {} stopped: its order left the order book (filled or cancelled externally); any resting remainder was cancelled",
                    self.instance.options().borrow().symbol
                );
                self.instance.exchange.base().emit(
                    "warning",
                    safe_cex_exchange::js_runtime::value::Js::str(&message),
                )?;
                self.cleanup();
                return Ok(Some(self.engine.cleanup_instance(self.instance.clone())));
            }
            Ok(None)
        })();
        Box::pin(async move {
            if let Some(f) = prefix? {
                f.await?;
            }
            Ok(())
        })
    }
    fn current(&self) -> Result<Vec<Order>, ExchangeError> {
        chaser_manager::get_current_orders(&self.instance)
    }
    fn filter(&self, orders: Vec<Order>) -> Vec<Order> {
        let ids = self.instance.current_order_ids();
        orders.into_iter().filter(|o| ids.contains(&o.id)).collect()
    }
    fn run_price_check(
        self: &Rc<Self>,
        orders: Vec<Order>,
        confirm: bool,
    ) -> ExchangeFuture<'static, ()> {
        self.last_price_check.set(self.engine.host.now_ms());
        let prefix: Result<Option<ExchangeFuture<'static, ()>>, ExchangeError> = (|| {
            let options = self.instance.options();
            let options = options.borrow().clone();
            if let Some(terminate) = options.terminate {
                let store = &self.instance.exchange.base().store;
                let mut ticker = store.tickers_by_symbol()?.get(&options.symbol).cloned();
                if ticker.is_none() {
                    ticker = {
                        if matches!(
                            safe_cex_num::js_table_lookup::<&str, ()>(&[], &options.symbol),
                            safe_cex_num::TableHit::Inherited(_)
                        ) {
                            None
                        } else {
                            store
                                .tickers()?
                                .iter()
                                .find(|t| t.symbol == options.symbol)
                                .cloned()
                        }
                    };
                }
                if let Some(price) = ticker.map(|t| t.last).filter(|p| *p != 0. && !p.is_nan()) {
                    if self.instance.should_terminate(price) {
                        let kind = match terminate.kind {
                            crate::task_types::ChaserTerminateKind::Price => "price",
                            crate::task_types::ChaserTerminateKind::Percent => "percent",
                            crate::task_types::ChaserTerminateKind::Usd => "usd",
                        };
                        chaser_manager::log(
                            &format!(
                                "terminating: live={} breached {kind}={}",
                                num(price),
                                num(terminate.value)
                            ),
                            &self.instance,
                        )?;
                        self.instance
                            .mark_complete(Some("Terminated by length condition"));
                        self.cleanup();
                        return Ok(Some(self.engine.cleanup_instance(self.instance.clone())));
                    }
                }
            }
            if chaser_manager::has_reached_target_position(&self.instance)? {
                chaser_manager::log(
                    "target position reached - marking ourselves completed",
                    &self.instance,
                )?;
                self.instance.mark_complete(None);
                self.cleanup();
                return Ok(Some(self.engine.cleanup_instance(self.instance.clone())));
            }
            Ok(None)
        })();

        (match prefix {
            Ok(Some(f)) => f,
            Ok(None) => self.handle_tracked(orders, confirm),
            Err(e) => Box::pin(async move { Err(e) }),
        }) as _
    }
    fn schedule_queued(self: &Rc<Self>) {
        if self.cleaned.get()
            || self
                .queued_timer
                .borrow()
                .as_ref()
                .is_some_and(|t| self.engine.host.timer_is_truthy(t))
        {
            return;
        }
        let monitor = self.clone();
        let timer = self.engine.host.set_timeout(
            Rc::new(move || {
                *monitor.queued_timer.borrow_mut() = None;
                if monitor.cleaned.get() || monitor.instance.is_stopped() {
                    return;
                }
                let f = match monitor.current() {
                    Ok(orders) => monitor.run_price_check(orders, false),
                    Err(error) => Box::pin(async move { Err(error) }),
                };
                monitor.spawn_caught(f, "queued order update");
            }),
            50.,
        );
        *self.queued_timer.borrow_mut() = Some(timer);
    }
    fn schedule_ticker(self: &Rc<Self>) {
        if self.cleaned.get()
            || self
                .price_timer
                .borrow()
                .as_ref()
                .is_some_and(|t| self.engine.host.timer_is_truthy(t))
        {
            return;
        }
        let wait = safe_cex_num::math_max(
            50. - (self.engine.host.now_ms() - self.last_price_check.get()),
            0.,
        );
        let monitor = self.clone();
        let timer = self.engine.host.set_timeout(
            Rc::new(move || {
                *monitor.price_timer.borrow_mut() = None;
                let f = if monitor.instance.is_stopped() {
                    let prefix = chaser_manager::log(
                        "instance is stopped from outside, stopping queued price check",
                        &monitor.instance,
                    );
                    if prefix.is_ok() {
                        monitor.instance.mark_complete(None);
                        monitor.cleanup();
                        let cleanup = monitor.engine.cleanup_instance(monitor.instance.clone());
                        Box::pin(async move {
                            prefix?;
                            cleanup.await
                        }) as ExchangeFuture<'static, ()>
                    } else {
                        Box::pin(async move { prefix })
                    }
                } else {
                    match monitor.current() {
                        Ok(orders) => monitor.run_price_check(orders, false),
                        Err(error) => Box::pin(async move { Err(error) }),
                    }
                };
                monitor.spawn_caught(f, "queued price update");
            }),
            wait,
        );
        *self.price_timer.borrow_mut() = Some(timer);
    }
    fn on_update(self: &Rc<Self>, data: &Js) -> ExchangeFuture<'static, ()> {
        let prefix: Result<Option<ExchangeFuture<'static, ()>>, ExchangeError> = (|| {
            if self.instance.is_stopped() {
                chaser_manager::log(
                    "instance is stopped from outside, unsubbing",
                    &self.instance,
                )?;
                self.instance.mark_complete(None);
                self.cleanup();
                return Ok(Some(self.engine.cleanup_instance(self.instance.clone())));
            }
            let ids = data.get("updateIds")?;
            let orders_id = ids.get("orders")?;
            let order_update = !orders_id.strict_equals(&self.last_orders_id.borrow());
            *self.last_orders_id.borrow_mut() = orders_id;
            let positions_id = ids.get("positions")?;
            let position_update = !positions_id.strict_equals(&self.last_positions_id.borrow());
            *self.last_positions_id.borrow_mut() = positions_id;
            let decode = || {
                to_typed::<Vec<Order>>(&data.get("orders")?)
                    .map_err(|error| error.to_exchange_error())
            };
            if order_update || position_update {
                self.clear_timer(&self.price_timer);
                self.clear_timer(&self.queued_timer);
                return Ok(Some(
                    self.run_price_check(self.filter(decode()?), order_update),
                ));
            }
            if self.engine.host.now_ms() - self.last_price_check.get() >= 50. {
                return Ok(Some(self.run_price_check(self.filter(decode()?), false)));
            }
            self.schedule_ticker();
            Ok(None)
        })();
        Box::pin(async move {
            if let Some(f) = prefix? {
                f.await?;
            }
            Ok(())
        })
    }
    fn on_fill(self: &Rc<Self>, data: &Js) {
        let result: Result<(), ExchangeError> = (|| {
            let id = data.get("orderId")?;
            let Some(id) = id.as_str().filter(|id| !id.is_empty()) else {
                return Ok(());
            };
            let id = String::from_utf16(id)
                .map_err(|_| ExchangeError::new("fill orderId requires UTF-8"))?;
            if !self.instance.current_order_ids().iter().any(|i| i == &id)
                || !data
                    .get("symbol")?
                    .strict_equals(&Js::str(&self.instance.options().borrow().symbol))
            {
                return Ok(());
            }
            let amount = data.get("amount")?.to_number()?;
            chaser_manager::log(
                &format!("fill event received: orderId={id}, amount={}", num(amount)),
                &self.instance,
            )?;
            self.instance.add_fill_amount(&id, amount);
            if self.instance.is_stopped() {
                chaser_manager::log(
                    "fully filled via fill events - marking ourselves completed",
                    &self.instance,
                )?;
                self.cleanup();
                self.engine
                    .host
                    .spawn(self.engine.cleanup_instance(self.instance.clone()));
            }
            Ok(())
        })();
        if let Err(error) = result {
            let f = self.engine.manager.handle_supervisor_error(
                self.instance.clone(),
                error.into(),
                "fill update",
                self.engine.clone(),
            );
            self.engine.host.spawn(f);
        }
    }
}

fn prime_future<T: 'static>(mut future: ExchangeFuture<'static, T>) -> ExchangeFuture<'static, T> {
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(result) => Box::pin(async move { result }),
        std::task::Poll::Pending => future,
    }
}
