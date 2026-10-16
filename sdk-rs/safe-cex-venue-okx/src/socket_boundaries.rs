pub fn listen_ohlcv<A, B, C, R, E>(
    opts: A,
    callback: B,
    capture: impl FnOnce(&str, &str) -> Result<C, E>,
    invoke: impl FnOnce(C, A, B) -> Result<R, E>,
) -> Result<R, E> {
    let callee = capture("businessWebsocket", "listenOHLCV")?;
    invoke(callee, opts, callback)
}

pub fn listen_order_book<A, B, C, R, E>(
    symbol: A,
    callback: B,
    capture: impl FnOnce(&str, &str) -> Result<C, E>,
    invoke: impl FnOnce(C, A, B) -> Result<R, E>,
) -> Result<R, E> {
    let callee = capture("publicWebsocket", "listenOrderBook")?;
    invoke(callee, symbol, callback)
}

pub fn dispose<E>(mut invoke: impl FnMut(&str) -> Result<(), E>) -> Result<(), E> {
    invoke("super.dispose")?;
    invoke("publicWebsocket.dispose")?;
    invoke("privateWebsocket.dispose")?;
    invoke("businessWebsocket.dispose")?;
    Ok(())
}
