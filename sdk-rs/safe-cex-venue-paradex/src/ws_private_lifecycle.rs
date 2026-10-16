//! Private websocket lifecycle over socket and timer host capabilities.
// ts: ParadexPrivateWebSocket.connectAndSubscribe
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

// ts: ParadexPrivateWebSocket.ping
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

// ts: ParadexPrivateWebSocket.dispose
pub fn dispose<T, E>(
    mut read_ping: impl FnMut() -> Result<Option<T>, E>,
    mut read_orders: impl FnMut() -> Result<Option<T>, E>,
    mut clear_timeout: impl FnMut(Option<T>) -> Result<(), E>,
    write_ping: impl FnOnce(Option<T>) -> Result<(), E>,
    write_orders: impl FnOnce(Option<T>) -> Result<(), E>,
    base_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if read_ping()?.is_some() {
        clear_timeout(read_ping()?)?;
        write_ping(None)?;
    }
    if read_orders()?.is_some() {
        clear_timeout(read_orders()?)?;
        write_orders(None)?;
    }
    base_dispose()
}

// ts: ParadexPrivateWebSocket#throttledFetchOrders
pub fn throttled_fetch_orders<T, C, E>(
    mut read_timeout: impl FnMut() -> Result<Option<T>, E>,
    clear_timeout: impl FnOnce(Option<T>) -> Result<(), E>,
    read_fetch_callback: impl FnOnce() -> Result<C, E>,
    schedule: impl FnOnce(C, u32) -> Result<T, E>,
    write_timeout: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    if read_timeout()?.is_some() {
        clear_timeout(read_timeout()?)?;
    }
    write_timeout(schedule(read_fetch_callback()?, 1_000)?)
}

// ts: ParadexPrivateWebSocket#subscribeToPrivateChannels
pub struct PrivateSubscription {
    pub jsonrpc: &'static str,
    pub method: &'static str,
    pub channel: &'static str,
    pub id: f64,
}

// ts: ParadexPrivateWebSocket#subscribeToPrivateChannels
pub fn subscribe_to_private_channels<E>(
    mut read_message_id: impl FnMut() -> Result<f64, E>,
    mut write_message_id: impl FnMut(f64) -> Result<(), E>,
    mut send: impl FnMut(PrivateSubscription) -> Result<(), E>,
) -> Result<(), E> {
    for channel in ["orders.ALL", "positions", "balance_events"] {
        let id = read_message_id()?;
        write_message_id(id + 1.0)?;
        send(PrivateSubscription {
            jsonrpc: "2.0",
            method: "subscribe",
            channel,
            id,
        })?;
    }
    Ok(())
}
