// ts: BinancePrivateWebsocket#detachSocket
/// Capture the socket once, resolving each removal against its current handler.
/// Read readiness after removals; preserve the source short-circuit order.
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
