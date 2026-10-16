use std::{cell::Cell, rc::Rc};

/// Configured timer registry handles; None represents the source's unset field.
/// Cancellation does not erase the field. Class/registry/socket binding remains
/// pending; configured callbacks preserve original failures through Result.
#[derive(Default)]
pub struct PrivateTimerState {
    pub retry_timer: Cell<Option<u64>>,
    pub refresh_timer: Cell<Option<u64>>,
}
pub type RefreshCallback = Rc<dyn Fn()>;

// ts: BinancePrivateWebsocket.dispose (binance.ws-private.ts)
pub fn dispose_binance_private<E>(
    state: &PrivateTimerState,
    mut clear_timer: impl FnMut(Option<u64>) -> Result<(), E>,
    dispose_base: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    clear_timer(state.retry_timer.get())?;
    clear_timer(state.refresh_timer.get())?;
    dispose_base()
}

// ts: BinancePMPrivateWebsocket.dispose
pub fn dispose_binance_pm_private<E>(
    state: &PrivateTimerState,
    mut clear_timer: impl FnMut(Option<u64>) -> Result<(), E>,
    dispose_base: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    clear_timer(state.retry_timer.get())?;
    clear_timer(state.refresh_timer.get())?;
    dispose_base()
}

// ts: BinancePrivateWebsocket#scheduleListenKeyRefresh
/// start_update resolves the owner's current updateListenKey when fired. It
/// starts the async update at that configured boundary; this body discards its
/// return. set_timer is the configured one-shot runtime, retaining the callback.
pub fn schedule_binance_listen_key_refresh<E>(
    state: &PrivateTimerState,
    delay_ms: f64,
    clear_timer: impl FnOnce(Option<u64>) -> Result<(), E>,
    set_timer: impl FnOnce(f64, RefreshCallback) -> Result<u64, E>,
    start_update: RefreshCallback,
) -> Result<(), E> {
    clear_timer(state.refresh_timer.get())?;
    let callback: RefreshCallback = Rc::new(move || start_update());
    state
        .refresh_timer
        .set(Some(set_timer(delay_ms, callback)?));
    Ok(())
}

// ts: BinancePMPrivateWebsocket#scheduleListenKeyRefresh
pub fn schedule_binance_pm_listen_key_refresh<E>(
    state: &PrivateTimerState,
    delay_ms: f64,
    clear_timer: impl FnOnce(Option<u64>) -> Result<(), E>,
    set_timer: impl FnOnce(f64, RefreshCallback) -> Result<u64, E>,
    start_update: RefreshCallback,
) -> Result<(), E> {
    clear_timer(state.refresh_timer.get())?;
    let callback: RefreshCallback = Rc::new(move || start_update());
    state
        .refresh_timer
        .set(Some(set_timer(delay_ms, callback)?));
    Ok(())
}

// ts: BinancePrivateWebsocket#detachSocket
/// Capture the previous socket once. Optional method/handler lookup is bound by
/// remove_listener on EACH invocation; ready-state reads retain short-circuit
/// order and happen after removals. Neither owner socket nor handlers are reset.
pub fn detach_binance_private_socket<S, E>(
    get_socket: impl FnOnce() -> Result<Option<S>, E>,
    mut remove_listener: impl FnMut(&S, &'static str) -> Result<(), E>,
    mut ready_state: impl FnMut(&S) -> Result<f64, E>,
    close: impl FnOnce(&S) -> Result<(), E>,
) -> Result<(), E> {
    let Some(previous) = get_socket()? else {
        return Ok(());
    };
    remove_listener(&previous, "message")?;
    remove_listener(&previous, "close")?;
    remove_listener(&previous, "open")?;
    if ready_state(&previous)? == 1.0 || ready_state(&previous)? == 0.0 {
        close(&previous)?;
    }
    Ok(())
}

// ts: BinancePMPrivateWebsocket#detachSocket
pub fn detach_binance_pm_private_socket<S, E>(
    get_socket: impl FnOnce() -> Result<Option<S>, E>,
    mut remove_listener: impl FnMut(&S, &'static str) -> Result<(), E>,
    mut ready_state: impl FnMut(&S) -> Result<f64, E>,
    close: impl FnOnce(&S) -> Result<(), E>,
) -> Result<(), E> {
    let Some(previous) = get_socket()? else {
        return Ok(());
    };
    remove_listener(&previous, "message")?;
    remove_listener(&previous, "close")?;
    remove_listener(&previous, "open")?;
    if ready_state(&previous)? == 1.0 || ready_state(&previous)? == 0.0 {
        close(&previous)?;
    }
    Ok(())
}
