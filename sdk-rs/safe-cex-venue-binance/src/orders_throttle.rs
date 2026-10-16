//! Order debounce and deferred-refresh bodies at configured runtime boundaries.
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

/// Undefined, numeric browser timer IDs (including NaN/-0), JSON data, or opaque
/// registry handles. Clones retain object/handle identity, including handle 0.
#[derive(Clone, Debug)]
pub enum OrderTimer {
    Undefined,
    Number(f64),
    Json(Rc<Value>),
    Handle(Rc<u64>),
}
impl OrderTimer {
    fn is_truthy(&self) -> bool {
        match self {
            Self::Undefined => false,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::Json(value) => safe_cex_num::js_truthy(Some(value)),
            Self::Handle(_) => true,
        }
    }
}
pub struct OrderThrottleState {
    pub timer: RefCell<OrderTimer>,
    pub is_disposed: Cell<bool>,
}
pub type FetchOrdersCallback = Rc<dyn Fn()>;
/// Snapshot of unique own enumerable string properties in JS enumeration order.
/// Handles preserve aliased native values; get_loaded supplies this snapshot,
/// including any extra properties, from its current owner. Executable property
/// access outside configured read callbacks, symbol keys and concrete wiring
/// remain pending. The helper only overwrites/appends the string key "orders".
pub type LoadedProperties = Vec<(String, Rc<Value>)>;
pub struct OrdersRefreshPatch<O> {
    pub orders: O,
    pub loaded: LoadedProperties,
}
fn mark_orders_loaded(loaded: &mut LoadedProperties) {
    if let Some((_, value)) = loaded.iter_mut().find(|(key, _)| key == "orders") {
        *value = Rc::new(Value::Bool(true));
    } else {
        loaded.push(("orders".into(), Rc::new(Value::Bool(true))));
    }
}

// ts: BinancePrivateWebsocket#throttledFetchOrders
/// get_fetch reads the current method AFTER cancellation. set_timer registers
/// that exact callback handle, rather than a new wrapper or late method lookup.
pub fn throttle_binance_orders<E>(
    state: &OrderThrottleState,
    clear_timer: impl FnOnce(OrderTimer) -> Result<(), E>,
    get_fetch: impl FnOnce() -> Result<FetchOrdersCallback, E>,
    set_timer: impl FnOnce(FetchOrdersCallback, f64) -> Result<OrderTimer, E>,
) -> Result<(), E> {
    if state.timer.borrow().is_truthy() {
        let previous = state.timer.borrow().clone();
        clear_timer(previous)?;
    }
    let callback = get_fetch()?;
    let timer = set_timer(callback, 10_000.0)?;
    *state.timer.borrow_mut() = timer;
    Ok(())
}

// ts: BinancePrivateWebsocket#fetchThrottledOrders
/// fetch_orders models both synchronous invocation and the awaited result.
/// get_update captures the current Store.update method AND receiver before
/// get_loaded reads the live loaded properties. Its invocation errors are caught.
pub async fn fetch_throttled_binance_orders<O, E, F, U>(
    state: &OrderThrottleState,
    fetch_orders: impl FnOnce() -> Result<F, E>,
    snapshot_stale: impl FnOnce() -> Result<bool, E>,
    get_update: impl FnOnce() -> Result<U, E>,
    get_loaded: impl FnOnce() -> Result<LoadedProperties, E>,
) where
    F: Future<Output = Result<O, E>>,
    U: FnOnce(OrdersRefreshPatch<O>) -> Result<(), E>,
{
    *state.timer.borrow_mut() = OrderTimer::Undefined;
    if !state.is_disposed.get() {
        let _ = async {
            let orders = fetch_orders()?.await?;
            if state.is_disposed.get() || snapshot_stale()? {
                return Ok::<(), E>(());
            }
            let update = get_update()?;
            let mut loaded = get_loaded()?;
            mark_orders_loaded(&mut loaded);
            update(OrdersRefreshPatch { orders, loaded })?;
            Ok(())
        }
        .await;
    }
}

// ts: BinancePMPrivateWebsocket#throttledFetchOrders
/// get_fetch reads the current method AFTER cancellation. set_timer registers
/// that exact callback handle, rather than a new wrapper or late method lookup.
pub fn throttle_binance_pm_orders<E>(
    state: &OrderThrottleState,
    clear_timer: impl FnOnce(OrderTimer) -> Result<(), E>,
    get_fetch: impl FnOnce() -> Result<FetchOrdersCallback, E>,
    set_timer: impl FnOnce(FetchOrdersCallback, f64) -> Result<OrderTimer, E>,
) -> Result<(), E> {
    if state.timer.borrow().is_truthy() {
        let previous = state.timer.borrow().clone();
        clear_timer(previous)?;
    }
    let callback = get_fetch()?;
    let timer = set_timer(callback, 10_000.0)?;
    *state.timer.borrow_mut() = timer;
    Ok(())
}

// ts: BinancePMPrivateWebsocket#fetchThrottledOrders
/// fetch_orders models both synchronous invocation and the awaited result.
/// get_update captures the current Store.update method AND receiver before
/// get_loaded reads the live loaded properties. Its invocation errors are caught.
pub async fn fetch_throttled_binance_pm_orders<O, E, F, U>(
    state: &OrderThrottleState,
    fetch_orders: impl FnOnce() -> Result<F, E>,
    snapshot_stale: impl FnOnce() -> Result<bool, E>,
    get_update: impl FnOnce() -> Result<U, E>,
    get_loaded: impl FnOnce() -> Result<LoadedProperties, E>,
) where
    F: Future<Output = Result<O, E>>,
    U: FnOnce(OrdersRefreshPatch<O>) -> Result<(), E>,
{
    *state.timer.borrow_mut() = OrderTimer::Undefined;
    if !state.is_disposed.get() {
        let _ = async {
            let orders = fetch_orders()?.await?;
            if state.is_disposed.get() || snapshot_stale()? {
                return Ok::<(), E>(());
            }
            let update = get_update()?;
            let mut loaded = get_loaded()?;
            mark_orders_loaded(&mut loaded);
            update(OrdersRefreshPatch { orders, loaded })?;
            Ok(())
        }
        .await;
    }
}
