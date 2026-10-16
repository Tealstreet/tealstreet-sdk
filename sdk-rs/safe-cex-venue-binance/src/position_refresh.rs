//! Private position scheduling/fetch bodies at configured runtime boundaries.
use std::{cell::Cell, future::Future, rc::Rc};

pub struct PositionFetchState {
    pub is_disposed: crate::class_flags::LiveFlag,
    pub pending_events: Cell<bool>,
    pub scheduled: Cell<bool>,
    pub in_flight: Cell<bool>,
    pub last_fetch_time: Cell<f64>,
    pub throttle_ms: Cell<f64>,
}
/// Resolves the owner's CURRENT execute method and starts its job. Async job
/// rejection must remain discarded here; only read/invocation failure is Err.
/// The registered timer closure returns void on success and preserves throws.
pub type PositionFetchStarter<E> = Rc<dyn Fn() -> Result<(), E>>;
pub type PositionTimerCallback<E> = Rc<dyn Fn() -> Result<(), E>>;

// ts: BinancePrivateWebsocket#throttledFetchPositions
/// Configured Date.now/paused/setTimeout/start callbacks are fallible. State is
/// typed boolean/numeric data cells; timer tickets are ignored as in the source.
pub fn throttle_binance_positions<E: 'static, T>(
    state: &Rc<PositionFetchState>,
    now: impl FnOnce() -> Result<f64, E>,
    paused: impl FnOnce() -> Result<bool, E>,
    set_timer: impl FnOnce(PositionTimerCallback<E>, f64) -> Result<T, E>,
    start_execute: PositionFetchStarter<E>,
) -> Result<(), E> {
    let now = now()?;
    if state.is_disposed.get() || paused()? {
        state.pending_events.set(false);
        return Ok(());
    }
    if state.scheduled.get() {
        return Ok(());
    }
    let elapsed = now - state.last_fetch_time.get();
    if elapsed >= state.throttle_ms.get() {
        start_execute()?;
    } else {
        state.scheduled.set(true);
        let callback_state = state.clone();
        let callback: PositionTimerCallback<E> = Rc::new(move || {
            callback_state.scheduled.set(false);
            if !callback_state.is_disposed.get() && callback_state.pending_events.get() {
                start_execute()?;
            }
            Ok(())
        });
        set_timer(callback, state.throttle_ms.get() - elapsed)?;
    }
    Ok(())
}

// ts: BinancePMPrivateWebsocket#throttledFetchPositions
/// PM has no background-pause read and leaves pending events intact on disposal.
pub fn throttle_binance_pm_positions<E: 'static, T>(
    state: &Rc<PositionFetchState>,
    now: impl FnOnce() -> Result<f64, E>,
    set_timer: impl FnOnce(PositionTimerCallback<E>, f64) -> Result<T, E>,
    start_execute: PositionFetchStarter<E>,
) -> Result<(), E> {
    let now = now()?;
    if state.is_disposed.get() {
        return Ok(());
    }
    if state.scheduled.get() {
        return Ok(());
    }
    let elapsed = now - state.last_fetch_time.get();
    if elapsed >= state.throttle_ms.get() {
        start_execute()?;
    } else {
        state.scheduled.set(true);
        let callback_state = state.clone();
        let callback: PositionTimerCallback<E> = Rc::new(move || {
            callback_state.scheduled.set(false);
            if !callback_state.is_disposed.get() && callback_state.pending_events.get() {
                start_execute()?;
            }
            Ok(())
        });
        set_timer(callback, state.throttle_ms.get() - elapsed)?;
    }
    Ok(())
}

// ts: BinancePrivateWebsocket#executePositionFetch
/// Numeric position versions compare with JS === (NaN unequal, signed zeros
/// equal). Version/clock preparation is outside try; fetch/version/store errors
/// are swallowed, then finally clears in-flight and invokes a live retry starter.
/// Caller retains/polls the future through settlement. Drop/unwind cleanup and
/// ownership of discarded asynchronous jobs remain outside this body port.
pub async fn execute_binance_position_fetch<P, E, F>(
    state: &PositionFetchState,
    paused: impl FnOnce() -> Result<bool, E>,
    mut get_version: impl FnMut() -> Result<f64, E>,
    now: impl FnOnce() -> Result<f64, E>,
    fetch_positions: impl FnOnce() -> Result<F, E>,
    apply_positions: impl FnOnce(P) -> Result<(), E>,
    start_throttle: impl FnOnce() -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<P, E>>,
{
    if state.in_flight.get() || state.is_disposed.get() || paused()? {
        return Ok(());
    }
    let event_version = get_version()?;
    state.in_flight.set(true);
    state.last_fetch_time.set(now()?);
    state.pending_events.set(false);
    let _ = async {
        let positions = fetch_positions()?.await?;
        if !state.is_disposed.get() && event_version == get_version()? {
            apply_positions(positions)?;
        }
        Ok::<(), E>(())
    }
    .await;
    state.in_flight.set(false);
    if !state.is_disposed.get() && state.pending_events.get() {
        start_throttle()?;
    }
    Ok(())
}

// ts: BinancePMPrivateWebsocket#executePositionFetch
/// PM deliberately has no initial disposal/in-flight/pause guard, version read,
/// finally reset or automatic retry. All clock/fetch/store errors are swallowed.
/// Caller retains/polls through settlement; dropped-future/job ownership is pending.
pub async fn execute_binance_pm_position_fetch<P, E, F>(
    state: &PositionFetchState,
    now: impl FnOnce() -> Result<f64, E>,
    fetch_positions: impl FnOnce() -> Result<F, E>,
    apply_positions: impl FnOnce(P) -> Result<(), E>,
) where
    F: Future<Output = Result<P, E>>,
{
    let _ = async {
        state.last_fetch_time.set(now()?);
        state.pending_events.set(false);
        let positions = fetch_positions()?.await?;
        if !state.is_disposed.get() {
            apply_positions(positions)?;
        }
        Ok::<(), E>(())
    }
    .await;
}
