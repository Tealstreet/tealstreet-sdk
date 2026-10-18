use crate::chaser::ChaserOrderInstance;
use safe_cex_types::{ExchangeName, OrderSide, PositionSide};

// ts: ChaserOrderManager.hasReachedTargetPosition
pub fn has_reached_target_position(instance: &ChaserOrderInstance) -> Result<bool, ExchangeError> {
    let options = instance.options();
    let options = options.borrow();
    if instance.exchange.base().name != ExchangeName::Bybitdemo
        || options.reduce_only == Some(true)
        || instance.target_position_contracts().is_none()
    {
        return Ok(false);
    }
    let target_side = if options.side == OrderSide::Buy {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let positions = instance.exchange.base().store.positions()?;
    let contracts = positions
        .iter()
        .find(|p| p.symbol == options.symbol && p.side == target_side)
        .map_or(0.0, |p| p.contracts);
    let contracts = if contracts.is_nan() {
        0.0
    } else {
        contracts.abs()
    };
    Ok(instance
        .target_position_contracts()
        .is_some_and(|target| contracts >= target - 1e-12))
}

// ts: ChaserOrderManager.bybitDemoPositionFill
pub fn bybit_demo_position_fill(instance: &ChaserOrderInstance) -> Result<f64, ExchangeError> {
    if instance.exchange.base().name != ExchangeName::Bybitdemo {
        return Ok(0.0);
    }
    let Some(target) = instance.target_position_contracts() else {
        return Ok(0.0);
    };
    let options = instance.options();
    let options = options.borrow();
    let starting_contracts = target - options.size;
    let target_side = if options.side == OrderSide::Buy {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let positions = instance.exchange.base().store.positions()?;
    let contracts = positions
        .iter()
        .find(|p| p.symbol == options.symbol && p.side == target_side)
        .map_or(0.0, |p| p.contracts);
    let delta = if contracts.is_nan() {
        0.0
    } else {
        contracts.abs()
    } - starting_contracts;
    Ok(safe_cex_num::math_min(
        options.size,
        safe_cex_num::math_max(0.0, delta),
    ))
}

// ts: ChaserOrderManager.getBybitDemoDeadPrivateWsReason
pub fn get_bybit_demo_dead_private_ws_reason(
    exchange: &dyn safe_cex_exchange::exchange::Exchange,
) -> Result<Option<String>, safe_cex_exchange::runtime::ExchangeError> {
    if exchange.base().name != ExchangeName::Bybitdemo {
        return Ok(None);
    }
    let health = exchange.base().store.socket_health()?;
    let Some(health) = health
        .as_ref()
        .filter(|health| safe_cex_num::js_truthy(Some(health)))
    else {
        return Ok(None);
    };
    if health.get("privateReconnectPaused") == Some(&serde_json::Value::Bool(true)) {
        let suffix = match health.get("privateCloseCode") {
            None => String::new(),
            Some(code) => format!(
                " (last close code {})",
                safe_cex_num::js_string(Some(code)).map_err(|error| {
                    let mut converted =
                        safe_cex_exchange::runtime::ExchangeError::new(error.message);
                    converted.name = error.name.into();
                    converted
                })?
            ),
        };
        return Ok(Some(format!(
            "reconnects are paused after repeated failures{suffix}"
        )));
    }
    if health.get("privateInitialized") != Some(&serde_json::Value::Bool(true)) {
        return Ok(Some("it has never delivered a message".into()));
    }
    Ok(None)
}

// ts: ChaserOrderManager.waitForBybitDemoPrivateWs
pub async fn wait_for_bybit_demo_private_ws(
    exchange: &dyn safe_cex_exchange::exchange::Exchange,
) -> Result<Option<String>, safe_cex_exchange::runtime::ExchangeError> {
    let mut reason = get_bybit_demo_dead_private_ws_reason(exchange)?;
    for _attempt in 1..=4 {
        if reason.is_none() {
            break;
        }
        let paused = exchange
            .base()
            .store
            .socket_health()?
            .as_ref()
            .and_then(|health| health.get("privateReconnectPaused"))
            == Some(&serde_json::Value::Bool(true));
        if paused {
            return Ok(reason);
        }
        // The caller owns this future and its sleep. Dropping it retains no task,
        // registration or marker, and no store Ref is held across the await.
        exchange.base().runtime.sleep(500).await?;
        reason = get_bybit_demo_dead_private_ws_reason(exchange)?;
    }
    Ok(reason)
}

pub fn log(
    message: &str,
    instance: &ChaserOrderInstance,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    instance.exchange.base().emit(
        "log",
        safe_cex_exchange::js_runtime::value::Js::str(&format!(
            "[CHASER{}] {message}",
            instance.id()
        )),
    )
}

pub fn get_current_orders(
    instance: &ChaserOrderInstance,
) -> Result<Vec<safe_cex_types::Order>, ExchangeError> {
    let ids = instance.current_order_ids();
    Ok(instance
        .exchange
        .base()
        .store
        .orders()?
        .into_iter()
        .filter(|order| ids.contains(&order.id))
        .collect())
}

pub fn check_and_mark_if_complete(
    instance: &ChaserOrderInstance,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    let options = instance.options();
    let (reduce_only, symbol, side) = {
        let options = options.borrow();
        (
            options.reduce_only == Some(true),
            options.symbol.clone(),
            options.side,
        )
    };
    if reduce_only {
        let closing_side = if side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        if !instance
            .exchange
            .base()
            .store
            .positions()?
            .iter()
            .any(|position| {
                position.symbol == symbol
                    && position.side == closing_side
                    && position.notional != 0.0
            })
        {
            instance.mark_complete(None);
            return Ok(());
        }
    }
    if !instance.current_order_ids().is_empty() {
        let orders = get_current_orders(instance)?;
        let ids = orders
            .iter()
            .map(|order| {
                use safe_cex_types::ModelField::{Absent, Undefined, Value};
                use safe_cex_types::OrderStatus;
                let status = match order.status {
                    Value(OrderStatus::Open) => "open",
                    Value(OrderStatus::Closed) => "closed",
                    Value(OrderStatus::Canceled) => "canceled",
                    Value(OrderStatus::Creating) => "creating",
                    Value(OrderStatus::Updating) => "updating",
                    Value(OrderStatus::Canceling) => "canceling",
                    Absent | Undefined => "undefined",
                };
                format!("{}:{status}", order.id)
            })
            .collect::<Vec<_>>()
            .join(",");
        log(&format!("my order ids {ids}"), instance)?;
        // CHOSEN: these are owned Store snapshots. A reentrant log listener can
        // mutate source order references, but cannot mutate this copied selection.
        if !orders.is_empty()
            && orders
                .iter()
                .all(|order| order.status == safe_cex_types::OrderStatus::Closed)
        {
            instance.mark_complete(None);
            instance.set_current_order_ids(Vec::new());
        }
    }
    Ok(())
}

// ts: ChaserOrderManager.findBestPrice
pub fn find_best_price(
    instance: &ChaserOrderInstance,
) -> Result<f64, safe_cex_exchange::runtime::ExchangeError> {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_num::sizing::{get_ticker_predicted_best_ask, get_ticker_predicted_best_bid};
    use safe_cex_num::{TableHit, adjust, math_max, math_min};
    let symbol = instance.options().borrow().symbol.clone();
    let store = &instance.exchange.base().store;
    let market = table_or_array(
        store.markets_by_symbol()?.get(&symbol).cloned(),
        &symbol,
        || {
            Ok(store
                .markets()?
                .iter()
                .find(|m| m.symbol == symbol)
                .cloned())
        },
    )?;
    let ticker = table_or_array(
        store.tickers_by_symbol()?.get(&symbol).cloned(),
        &symbol,
        || {
            Ok(store
                .tickers()?
                .iter()
                .find(|t| t.symbol == symbol)
                .cloned())
        },
    )?;
    if !market.is_truthy_by(|_| true) || !ticker.is_truthy_by(|_| true) {
        log("no market or ticker found", instance)?;
        instance.mark_complete(Some("No market or ticker found"));
        let symbol = instance.options().borrow().symbol.clone();
        instance.exchange.base().emit(
            "error",
            safe_cex_exchange::js_runtime::value::Js::str(&format!(
                "Chaser no market or ticker found for {symbol}"
            )),
        )?;
        let symbol = instance.options().borrow().symbol.clone();
        return Err(ExchangeError::new(format!(
            "No market or ticker found for {symbol}"
        )));
    }
    let TableHit::Mapped(market) = market else {
        // ts-bug: inherited markets pass the guard, then precision.price throws.
        let mut error = ExchangeError::new("Cannot read properties of undefined (reading 'price')");
        error.name = "TypeError".into();
        return Err(error);
    };
    let (best_bid, best_ask) = match ticker {
        TableHit::Mapped(ticker) => (
            get_ticker_predicted_best_bid(&ticker, Some(&market)),
            get_ticker_predicted_best_ask(&ticker, Some(&market)),
        ),
        // ts-bug: inherited ticker fields are undefined, producing NaN in arithmetic.
        TableHit::Inherited(_) => (f64::NAN, f64::NAN),
        TableHit::Missing => {
            return Err(ExchangeError::new(format!(
                "No market or ticker found for {symbol}"
            )));
        }
    };
    let precision = market.precision.price;
    let side = instance.options().borrow().side;
    let mut raw = if side == OrderSide::Buy {
        best_bid - precision * instance.min_tick_distance()
    } else {
        best_ask + precision * instance.min_tick_distance()
    };
    if side == OrderSide::Buy && best_ask > 0.0 {
        raw = math_min(raw, best_ask - precision);
    }
    if side == OrderSide::Sell && best_bid > 0.0 {
        raw = math_max(raw, best_bid + precision);
    }
    Ok(adjust(math_max(raw, precision), precision))
}

fn table_or_array<T>(
    own: Option<T>,
    symbol: &str,
    fallback: impl FnOnce() -> Result<Option<T>, ExchangeError>,
) -> Result<safe_cex_num::TableHit<T>, ExchangeError> {
    use safe_cex_num::TableHit;
    if let Some(value) = own {
        return Ok(TableHit::Mapped(value));
    }
    Ok(
        match safe_cex_num::js_table_lookup::<&str, ()>(&[], symbol) {
            TableHit::Inherited(member) => TableHit::Inherited(member),
            _ => fallback()?.map_or(TableHit::Missing, TableHit::Mapped),
        },
    )
}

use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use std::{cell::RefCell, rc::Rc};

/// Explicit construction/spawn bindings for the native manager engine. Each construct
/// method executes the source async prefix immediately, captures business errors
/// in the returned Result future, and returns before settlement. No default
/// implementation supplies cancellation, fill reconciliation or supervision.
/// Hosts own detached work and report rejection rather than silently dropping it.
pub trait ChaserSupervisor: 'static {
    fn construct_supervision(
        &self,
        manager: Rc<ChaserOrderManager>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()>;
    fn construct_cleanup(
        &self,
        manager: Rc<ChaserOrderManager>,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()>;
    fn construct_completion(
        &self,
        instance: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()>;
    fn spawn(&self, future: ExchangeFuture<'static, ()>);
}

/// Canonical native Error values and owned JSON throws. This does not model JS
/// Error prototype tests or arbitrary throwing String/getter coercions.
pub enum SupervisorFailure {
    Error(ExchangeError),
    Value(serde_json::Value),
}
impl From<ExchangeError> for SupervisorFailure {
    fn from(error: ExchangeError) -> Self {
        Self::Error(error)
    }
}

/// Membership and dispatch portion of the source class. The live dense list
/// retains instance identity. Release list/state borrows before calling methods;
/// all manager borrows are released before reentrant callbacks/dependencies.
/// processInstance, cleanupInstance and emitComplete are implemented by ChaserEngine; hosts bind that engine or their foreign equivalents explicitly.
#[derive(Default)]
pub struct ChaserOrderManager {
    chaser_order_instances: Rc<RefCell<Vec<Rc<ChaserOrderInstance>>>>,
}
impl ChaserOrderManager {
    pub fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }

    // ts: add. Push before executing the supervisor's async construction prefix.
    pub fn add(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
        supervisor: Rc<dyn ChaserSupervisor>,
    ) {
        self.chaser_order_instances
            .borrow_mut()
            .push(instance.clone());
        let processing = supervisor.construct_supervision(self.clone(), instance.clone());
        let manager = self.clone();
        let on_error = supervisor.clone();
        supervisor.spawn(Box::pin(async move {
            if let Err(error) = processing.await {
                manager
                    .handle_supervisor_error(instance, error.into(), "startup", on_error)
                    .await?;
            }
            Ok(())
        }));
    }

    // ts: handleSupervisorError. Execute log/mark/error/cleanup prefix in that order.
    pub fn handle_supervisor_error(
        self: &Rc<Self>,
        instance: Rc<ChaserOrderInstance>,
        error: SupervisorFailure,
        phase: &str,
        supervisor: Rc<dyn ChaserSupervisor>,
    ) -> ExchangeFuture<'static, ()> {
        let prefix = (|| {
            let detail = match error {
                SupervisorFailure::Error(error) => error.message,
                SupervisorFailure::Value(value) => {
                    safe_cex_num::js_string(Some(&value)).map_err(|e| {
                        let mut error = ExchangeError::new(e.message);
                        error.name = e.name.into();
                        error
                    })?
                }
            };
            let symbol = instance.options().borrow().symbol.clone();
            let message = format!("Chaser supervisor failed during {phase} for {symbol}: {detail}");
            log(&message, &instance)?;
            if !instance.is_stopped() {
                instance.mark_complete(Some(&format!("Chaser {phase} failed")));
            }
            instance.exchange.base().emit(
                "error",
                safe_cex_exchange::js_runtime::value::Js::str(&message),
            )?;
            Ok::<_, ExchangeError>(supervisor.construct_cleanup(self.clone(), instance))
        })();
        Box::pin(async move { prefix?.await })
    }

    // ts: cleanupExchange. Snapshot the matching list before any callback.
    pub fn cleanup_exchange(
        self: &Rc<Self>,
        exchange: Rc<dyn Exchange>,
        reason: Option<&str>,
        supervisor: Rc<dyn ChaserSupervisor>,
    ) -> Result<(), ExchangeError> {
        let instances: Vec<_> = self
            .chaser_order_instances
            .borrow()
            .iter()
            .filter(|instance| Rc::ptr_eq(&instance.exchange, &exchange))
            .cloned()
            .collect();
        for instance in instances {
            if !instance.is_stopped() {
                instance.mark_complete(Some(
                    reason.unwrap_or("Exchange disposed while chase was active"),
                ));
                let symbol = instance.options().borrow().symbol.clone();
                exchange.base().emit("warning", safe_cex_exchange::js_runtime::value::Js::str(&format!(
                    "Chaser for {symbol} was stopped because the exchange connection was disposed"
                )))?;
            }
            let cleanup = supervisor.construct_cleanup(self.clone(), instance);
            supervisor.spawn(cleanup);
        }
        Ok(())
    }

    // ts: remove. First ID match; completion inspection precedes cleanup.
    pub fn remove(
        self: &Rc<Self>,
        id: &str,
        supervisor: Rc<dyn ChaserSupervisor>,
    ) -> Result<(), ExchangeError> {
        let instance = self
            .chaser_order_instances
            .borrow()
            .iter()
            .find(|instance| instance.id() == id)
            .cloned();
        if let Some(instance) = instance {
            instance.mark_complete(Some("User cancelled"));
            check_and_mark_if_complete(&instance)?;
            let cleanup = supervisor.construct_cleanup(self.clone(), instance);
            supervisor.spawn(cleanup);
        }
        Ok(())
    }

    // ts: removeKeepOrder. Re-find by object identity after disposer reentrancy.
    pub fn remove_keep_order(self: &Rc<Self>, id: &str, supervisor: Rc<dyn ChaserSupervisor>) {
        let instance = self
            .chaser_order_instances
            .borrow()
            .iter()
            .find(|instance| instance.id() == id)
            .cloned();
        let Some(instance) = instance else {
            return;
        };
        instance.mark_complete(Some(crate::task_types::CHASER_KEPT_ORDER_STOP_REASON));
        instance.set_current_order_ids(Vec::new());
        instance.dispose();
        let index = self
            .chaser_order_instances
            .borrow()
            .iter()
            .position(|other| Rc::ptr_eq(other, &instance));
        if let Some(index) = index {
            self.chaser_order_instances.borrow_mut().remove(index);
        }
        if instance.claim_complete_emission() {
            let completion = supervisor.construct_completion(instance);
            supervisor.spawn(completion);
        }
    }

    // ts: get instances. Return the same live list, not a selected/snapshot copy.
    pub fn instances(&self) -> Rc<RefCell<Vec<Rc<ChaserOrderInstance>>>> {
        self.chaser_order_instances.clone()
    }
}
// ts: chaserOrderManagerInstance. Shared per native host thread.
pub fn chaser_order_manager_instance() -> Rc<ChaserOrderManager> {
    // Function-local static storage remains shared across calls on each thread.
    thread_local! {
        static CHASER_ORDER_MANAGER_INSTANCE: Rc<ChaserOrderManager> = ChaserOrderManager::new();
    }
    CHASER_ORDER_MANAGER_INSTANCE.with(Rc::clone)
}
