//! Public websocket lifecycle over socket and timer host capabilities.
// ts: ParadexPublicWebSocket.connectAndSubscribe
pub fn connect_and_subscribe<U, S, E>(
    read_disposed: impl FnOnce() -> Result<bool, E>,
    read_url: impl FnOnce() -> Result<U, E>,
    create_socket: impl FnOnce(U) -> Result<S, E>,
    write_socket: impl FnOnce(S) -> Result<(), E>,
    mut add_listener: impl FnMut(&'static str) -> Result<(), E>,
) -> Result<(), E> {
    if !read_disposed()? {
        write_socket(create_socket(read_url()?)?)?;
        add_listener("open")?;
        add_listener("message")?;
        add_listener("close")?;
    }
    Ok(())
}

// ts: ParadexPublicWebSocket.ping
pub fn ping<'a, T, E: 'a>(
    mut read_connected: impl FnMut() -> Result<bool, E> + 'a,
    read_disposed: impl FnOnce() -> Result<bool, E> + 'a,
    send: impl FnOnce(&'static str) -> Result<(), E>,
    call_ping: impl FnOnce() -> Result<(), E> + 'a,
    schedule: impl FnOnce(Box<dyn FnOnce() -> Result<(), E> + 'a>, u32) -> Result<T, E>,
    write_timer: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    if read_connected()? {
        send(r#"{"type":"ping"}"#)?;
    }
    let callback = Box::new(move || {
        if !read_disposed()? && read_connected()? {
            call_ping()?;
        }
        Ok(())
    });
    write_timer(schedule(callback, 30_000)?)?;
    Ok(())
}

// ts: ParadexPublicWebSocket.dispose
pub fn dispose<T, E>(
    mut read_ping: impl FnMut() -> Result<Option<T>, E>,
    mut clear_timeout: impl FnMut(Option<T>) -> Result<(), E>,
    write_ping: impl FnOnce(Option<T>) -> Result<(), E>,
    visit_retry: impl FnOnce(&mut dyn FnMut(T) -> Result<(), E>) -> Result<(), E>,
    clear_retry: impl FnOnce() -> Result<(), E>,
    visit_poll: impl FnOnce(&mut dyn FnMut(T) -> Result<(), E>) -> Result<(), E>,
    clear_poll: impl FnOnce() -> Result<(), E>,
    base_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if read_ping()?.is_some() {
        clear_timeout(read_ping()?)?;
        write_ping(None)?;
    }
    visit_retry(&mut |timer| clear_timeout(Some(timer)))?;
    clear_retry()?;
    visit_poll(&mut |timer| clear_timeout(Some(timer)))?;
    clear_poll()?;
    base_dispose()
}

// ts: ParadexPublicWebSocket.onOpen
pub fn on_open<S, E>(
    read_disposed: impl FnOnce() -> Result<bool, E>,
    mut log: impl FnMut(&'static str, &'static str) -> Result<(), E>,
    ping: impl FnOnce() -> Result<(), E>,
    clear_sent: impl FnOnce() -> Result<(), E>,
    clear_pending: impl FnOnce() -> Result<(), E>,
    read_connected: impl FnOnce() -> Result<bool, E>,
    read_socket: impl FnOnce() -> Result<Option<S>, E>,
    close: impl FnOnce(S) -> Result<(), E>,
    subscribe: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if !read_disposed()? {
        log("Paradex public WebSocket connected", "info")?;
        ping()?;
        clear_sent()?;
        clear_pending()?;
        if !read_connected()? {
            log(
                "WebSocket disconnected in onOpen, closing connection",
                "warning",
            )?;
            if let Some(socket) = read_socket()? {
                close(socket)?;
            }
            return Ok(());
        }
        subscribe()?;
    }
    Ok(())
}

// ts: ParadexPublicWebSocket#hasActiveSubscriptionInterest
pub fn has_active_subscription_interest<E>(
    channel: &[u16],
    read_market_length: impl FnOnce() -> Result<f64, E>,
    desired_has: impl FnOnce(&[u16]) -> Result<bool, E>,
    market_some: impl FnOnce(&[u16]) -> Result<bool, E>,
    orderbook_size: impl FnOnce(&[u16]) -> Result<Option<f64>, E>,
) -> Result<bool, E> {
    let equal = |ascii: &str| channel.iter().copied().eq(ascii.encode_utf16());
    if equal("markets_summary") {
        return Ok(read_market_length()? > 0.0);
    }
    let prefixed = |ascii: &str| channel.starts_with(&ascii.encode_utf16().collect::<Vec<_>>());
    let symbol = channel.split(|unit| *unit == 46).nth(1).unwrap_or(&[]);
    if prefixed("trades.") {
        return Ok(!symbol.is_empty() && (desired_has(symbol)? || market_some(symbol)?));
    }
    if prefixed("bbo.") {
        return Ok(!symbol.is_empty() && market_some(symbol)?);
    }
    if prefixed("order_book.") {
        return Ok(!symbol.is_empty()
            && orderbook_size(symbol)?.is_some_and(|size| size != 0.0 && !size.is_nan()));
    }
    Ok(false)
}
