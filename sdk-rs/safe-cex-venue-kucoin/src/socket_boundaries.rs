pub fn listen_ohlcv<A, B, C, R, E>(
    opts: A,
    callback: B,
    capture: impl FnOnce(&str, &str) -> Result<C, E>,
    invoke: impl FnOnce(C, A, B) -> Result<R, E>,
) -> Result<R, E> {
    let callee = capture("publicWebsocket", "listenOHLCV")?;
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

pub fn listen_ohlcv_spot<V, A, B, C, R, E>(
    opts: A,
    callback: B,
    provider: impl FnOnce() -> Result<V, E>,
    truthy: impl FnOnce(&V) -> bool,
    capture: impl FnOnce(&str, &str) -> Result<C, E>,
    invoke: impl FnOnce(C, A, B) -> Result<R, E>,
) -> Result<R, E> {
    if truthy(&provider()?) {
        let callee = capture("externalPublicDataProvider", "subscribeCandle")?;
        return invoke(callee, opts, callback);
    }
    let callee = capture("publicWebsocket", "listenOHLCV")?;
    invoke(callee, opts, callback)
}

pub fn listen_order_book_spot<V, A, B, C, R, E>(
    symbol: A,
    callback: B,
    provider: impl FnOnce() -> Result<V, E>,
    truthy: impl FnOnce(&V) -> bool,
    capture: impl FnOnce(&str, &str) -> Result<C, E>,
    invoke: impl FnOnce(C, A, B) -> Result<R, E>,
) -> Result<R, E> {
    if truthy(&provider()?) {
        let callee = capture("externalPublicDataProvider", "subscribeOrderbook")?;
        return invoke(callee, symbol, callback);
    }
    let callee = capture("publicWebsocket", "listenOrderBook")?;
    invoke(callee, symbol, callback)
}

pub fn dispose_futures<'a, S: 'a, R: 'a, E: 'a>(
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    for_each: impl FnOnce(std::rc::Rc<dyn Fn(S) -> Result<R, E> + 'a>) -> Result<(), E>,
    dispose_snapshot: std::rc::Rc<dyn Fn(S) -> Result<R, E> + 'a>,
    assign_empty: impl FnOnce(Vec<S>) -> Result<(), E>,
) -> Result<(), E> {
    invoke("super.dispose")?;
    invoke("publicWebsocket.dispose")?;
    for_each(std::rc::Rc::new(move |socket| dispose_snapshot(socket)))?;
    assign_empty(Vec::new())?;
    invoke("privateWebsocket.dispose")?;
    Ok(())
}

pub fn dispose_spot<E>(mut invoke: impl FnMut(&str) -> Result<(), E>) -> Result<(), E> {
    invoke("super.dispose")?;
    invoke("publicWebsocket.dispose")?;
    invoke("privateWebsocket.dispose")?;
    Ok(())
}
