use crate::request_weight::Failure;
use safe_cex_types::StoreDataLoaded;
use std::{cell::Cell, future::Future, rc::Rc};

pub struct OrdersRetryState {
    pub last_retry_at: Cell<f64>,
    pub is_disposed: crate::class_flags::LiveFlag,
    pub snapshot_stale: crate::class_flags::LiveFlag,
}

// ts: BinanceExchange#retryOrdersSnapshot
pub async fn retry_futures_orders_snapshot<O, F, U>(
    state: &Rc<OrdersRetryState>,
    mut now: impl FnMut() -> Result<f64, Failure>,
    fetch: impl FnOnce() -> Result<F, Failure>,
    get_update: impl FnOnce() -> Result<U, Failure>,
    get_loaded: impl FnOnce() -> Result<StoreDataLoaded, Failure>,
) -> Result<(), Failure>
where
    F: Future<Output = Result<O, Failure>>,
    U: FnOnce(O, StoreDataLoaded) -> Result<(), Failure>,
{
    if now()? - state.last_retry_at.get() < 15_000.0 {
        return Ok(());
    }
    state.last_retry_at.set(now()?);
    let orders = fetch()?.await?;
    if state.is_disposed.get() || state.snapshot_stale.get() {
        return Ok(());
    }
    let update = get_update()?;
    let mut loaded = get_loaded()?;
    loaded.orders = true;
    update(orders, loaded)
}

// ts: BinancePMExchange#retryOrdersSnapshot
pub async fn retry_pm_orders_snapshot<O, F, U>(
    state: &Rc<OrdersRetryState>,
    mut now: impl FnMut() -> Result<f64, Failure>,
    fetch: impl FnOnce() -> Result<F, Failure>,
    get_update: impl FnOnce() -> Result<U, Failure>,
    get_loaded: impl FnOnce() -> Result<StoreDataLoaded, Failure>,
) -> Result<(), Failure>
where
    F: Future<Output = Result<O, Failure>>,
    U: FnOnce(O, StoreDataLoaded) -> Result<(), Failure>,
{
    if now()? - state.last_retry_at.get() < 15_000.0 {
        return Ok(());
    }
    state.last_retry_at.set(now()?);
    let orders = fetch()?.await?;
    if state.is_disposed.get() || state.snapshot_stale.get() {
        return Ok(());
    }
    let update = get_update()?;
    let mut loaded = get_loaded()?;
    loaded.orders = true;
    update(orders, loaded)
}
