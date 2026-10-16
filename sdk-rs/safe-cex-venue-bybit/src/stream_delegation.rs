//! Exchange bodies that delegate to their concrete public/private sockets.
//! Generic handles retain receiver, argument, callback, return and error identity.
fn listen<S, O, C, R, E>(
    property: &'static str,
    method: &'static str,
    options: O,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<R, E> {
    let receiver = websocket(property)?;
    call(receiver, method, options, callback)
}

pub fn listen_ohlcv<S, O, C, R, E>(
    options: O,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(
        "publicWebsocket",
        "listenOHLCV",
        options,
        callback,
        websocket,
        call,
    )
}

pub fn listen_inverse_ohlcv<S, O, C, R, E>(
    options: O,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(
        "publicInverseWebsocket",
        "listenOHLCV",
        options,
        callback,
        websocket,
        call,
    )
}

/// The source bulk wrapper deliberately discards the socket method's return.
pub fn listen_ohlcvs<S, O, C, R, E>(
    options: O,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<(), E> {
    listen(
        "publicWebsocket",
        "listenOHLCVs",
        options,
        callback,
        websocket,
        call,
    )?;
    Ok(())
}

pub fn listen_inverse_ohlcvs<S, O, C, R, E>(
    options: O,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<(), E> {
    listen(
        "publicInverseWebsocket",
        "listenOHLCVs",
        options,
        callback,
        websocket,
        call,
    )?;
    Ok(())
}

pub fn listen_order_book<S, C, R, E>(
    symbol: &str,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, &str, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(
        "publicWebsocket",
        "listenOrderBook",
        symbol,
        callback,
        websocket,
        call,
    )
}

pub fn listen_inverse_order_book<S, C, R, E>(
    symbol: &str,
    callback: C,
    websocket: impl FnOnce(&str) -> Result<S, E>,
    call: impl FnOnce(S, &str, &str, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(
        "publicInverseWebsocket",
        "listenOrderBook",
        symbol,
        callback,
        websocket,
        call,
    )
}

// ts: BybitExchange#publicSocketFor —
// (getCategoryForSymbol(symbol) === 'inverse' && this.inversePublicWebsocket) || this.publicWebsocket
fn public_socket_for<S, E>(
    symbol: &str,
    websocket: &mut impl FnMut(&str) -> Result<S, E>,
    secondary: &mut impl FnMut() -> Result<Option<S>, E>,
) -> Result<S, E> {
    if crate::inverse_symbol::is_bybit_inverse_symbol(symbol)
        && !crate::options::is_bybit_option_symbol(symbol)
        && let Some(socket) = secondary()?
    {
        return Ok(socket);
    }
    websocket("publicWebsocket")
}

/// The bybit connection routes an inverse symbol to its inverse public socket.
pub fn listen_primary_ohlcv<S, O, C, R, E>(
    options: O,
    symbol: &str,
    callback: C,
    mut websocket: impl FnMut(&str) -> Result<S, E>,
    mut secondary: impl FnMut() -> Result<Option<S>, E>,
    call: impl FnOnce(S, &str, O, C) -> Result<R, E>,
) -> Result<R, E> {
    let socket = public_socket_for(symbol, &mut websocket, &mut secondary)?;
    call(socket, "listenOHLCV", options, callback)
}

pub fn listen_primary_order_book<S, C, R, E>(
    symbol: &str,
    callback: C,
    mut websocket: impl FnMut(&str) -> Result<S, E>,
    mut secondary: impl FnMut() -> Result<Option<S>, E>,
    call: impl FnOnce(S, &str, &str, C) -> Result<R, E>,
) -> Result<R, E> {
    let socket = public_socket_for(symbol, &mut websocket, &mut secondary)?;
    call(socket, "listenOrderBook", symbol, callback)
}

/// Symbols group by socket in first-seen order; each socket gets `{ ...opts, symbols }`
/// and the source discards every socket method's return.
#[allow(clippy::too_many_arguments)]
pub fn listen_primary_ohlcvs<S, O, C: Clone, R, E>(
    symbols: Result<Vec<String>, E>,
    callback: C,
    mut websocket: impl FnMut(&str) -> Result<S, E>,
    mut secondary: impl FnMut() -> Result<Option<S>, E>,
    same: impl Fn(&S, &S) -> bool,
    with_symbols: impl Fn(Vec<String>) -> O,
    mut call: impl FnMut(S, &str, O, C) -> Result<R, E>,
) -> Result<(), E> {
    let mut by_socket: Vec<(S, Vec<String>)> = Vec::new();
    for symbol in symbols? {
        let socket = public_socket_for(&symbol, &mut websocket, &mut secondary)?;
        match by_socket.iter_mut().find(|(seen, _)| same(seen, &socket)) {
            Some((_, group)) => group.push(symbol),
            None => by_socket.push((socket, vec![symbol])),
        }
    }
    for (socket, group) in by_socket {
        call(
            socket,
            "listenOHLCVs",
            with_symbols(group),
            callback.clone(),
        )?;
    }
    Ok(())
}

fn dispose_sockets<S, R, E>(
    public_property: &str,
    base_dispose: impl FnOnce() -> Result<R, E>,
    mut websocket: impl FnMut(&str) -> Result<S, E>,
    mut dispose: impl FnMut(S) -> Result<R, E>,
) -> Result<(), E> {
    base_dispose()?;
    let public = websocket(public_property)?;
    dispose(public)?;
    let private = websocket("privateWebsocket")?;
    dispose(private)?;
    Ok(())
}

pub fn dispose_exchange<S, R, E>(
    base_dispose: impl FnOnce() -> Result<R, E>,
    websocket: impl FnMut(&str) -> Result<S, E>,
    dispose: impl FnMut(S) -> Result<R, E>,
) -> Result<(), E> {
    dispose_sockets("publicWebsocket", base_dispose, websocket, dispose)
}

pub fn dispose_inverse_exchange<S, R, E>(
    base_dispose: impl FnOnce() -> Result<R, E>,
    websocket: impl FnMut(&str) -> Result<S, E>,
    dispose: impl FnMut(S) -> Result<R, E>,
) -> Result<(), E> {
    dispose_sockets("publicInverseWebsocket", base_dispose, websocket, dispose)
}

/// `keys` supplies a live iterator. A snapshot would lose keys added by cleanup.
#[allow(clippy::too_many_arguments)]
pub fn dispose_primary_exchange<S, R, E, I: Iterator<Item = Result<String, E>>>(
    cancel_debounced_snapshot: impl FnOnce() -> Result<R, E>,
    keys: impl FnOnce() -> Result<I, E>,
    mut clear_timer: impl FnMut(String) -> Result<R, E>,
    base_dispose: impl FnOnce() -> Result<R, E>,
    websocket: impl FnMut(&str) -> Result<S, E>,
    dispose: impl FnMut(S) -> Result<R, E>,
) -> Result<(), E> {
    cancel_debounced_snapshot()?;
    for key in keys()? {
        clear_timer(key?)?;
    }
    dispose_sockets("publicWebsocket", base_dispose, websocket, dispose)
}
