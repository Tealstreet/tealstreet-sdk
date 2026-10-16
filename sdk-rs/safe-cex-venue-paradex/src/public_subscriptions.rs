//! Public subscription control flow with host collection and transport capabilities.
use safe_cex_num::js_number_string;

// ts: ParadexPublicWebSocket#sendJsonRpcSubscription
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionMethod {
    Subscribe,
    Unsubscribe,
}

// ts: ParadexPublicWebSocket#sendJsonRpcSubscription
pub struct PublicSubscription<C> {
    pub id: f64,
    pub jsonrpc: &'static str,
    pub method: SubscriptionMethod,
    pub channel: C,
}

// ts: ParadexPublicWebSocket#sendJsonRpcSubscription
pub fn send_json_rpc_subscription<C, E>(
    method: SubscriptionMethod,
    channel: C,
    attempt: Option<f64>,
    read_connected: impl FnOnce() -> Result<bool, E>,
    read_id: impl FnOnce() -> Result<f64, E>,
    write_id: impl FnOnce(f64) -> Result<(), E>,
    set_pending: impl FnOnce(f64, &C, f64) -> Result<(), E>,
    send: impl FnOnce(PublicSubscription<C>) -> Result<(), E>,
) -> Result<(), E> {
    if !read_connected()? {
        return Ok(());
    }
    let id = read_id()?;
    write_id(id + 1.0)?;
    if method == SubscriptionMethod::Subscribe {
        set_pending(id, &channel, attempt.unwrap_or(0.0))?;
    }
    send(PublicSubscription {
        id,
        jsonrpc: "2.0",
        method,
        channel,
    })
}

fn channel(prefix: &str, symbol: &[u16]) -> Vec<u16> {
    prefix
        .encode_utf16()
        .chain(symbol.iter().copied())
        .collect()
}
fn count_or_zero(value: Option<f64>) -> f64 {
    value.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(0.0)
}

// ts: ParadexPublicWebSocket#sendTradeSubscribe
pub fn send_trade_subscribe<E>(
    symbol: &[u16],
    force: Option<bool>,
    attempt: Option<f64>,
    add_desired: impl FnOnce(&[u16]) -> Result<(), E>,
    has_sent: impl FnOnce(&[u16]) -> Result<bool, E>,
    add_sent: impl FnOnce(&[u16]) -> Result<(), E>,
    send: impl FnOnce(SubscriptionMethod, Vec<u16>, f64) -> Result<(), E>,
) -> Result<(), E> {
    if symbol.is_empty() {
        return Ok(());
    }
    add_desired(symbol)?;
    if !force.unwrap_or(false) && has_sent(symbol)? {
        return Ok(());
    }
    add_sent(symbol)?;
    send(
        SubscriptionMethod::Subscribe,
        channel("trades.", symbol),
        attempt.unwrap_or(0.0),
    )
}

// ts: ParadexPublicWebSocket.subscribeToOrderBook
pub fn subscribe_to_order_book<S, C, E>(
    symbol: S,
    read_connected: impl FnOnce() -> Result<bool, E>,
    get_channel: impl FnOnce(S) -> Result<C, E>,
    send: impl FnOnce(SubscriptionMethod, C) -> Result<(), E>,
) -> Result<(), E> {
    if read_connected()? {
        send(SubscriptionMethod::Subscribe, get_channel(symbol)?)?;
    }
    Ok(())
}

// ts: ParadexPublicWebSocket.unsubscribeFromOrderBook
pub fn unsubscribe_from_order_book<S, C, E>(
    symbol: S,
    read_connected: impl FnOnce() -> Result<bool, E>,
    get_channel: impl FnOnce(S) -> Result<C, E>,
    send: impl FnOnce(SubscriptionMethod, C) -> Result<(), E>,
) -> Result<(), E> {
    if read_connected()? {
        send(SubscriptionMethod::Unsubscribe, get_channel(symbol)?)?;
    }
    Ok(())
}

// ts: ParadexPublicWebSocket.subscribeToTrades
pub fn subscribe_to_trades<E>(
    symbol: &[u16],
    read_count: impl FnOnce(&[u16]) -> Result<Option<f64>, E>,
    write_count: impl FnOnce(&[u16], f64) -> Result<(), E>,
    send_trade: impl FnOnce(&[u16]) -> Result<(), E>,
) -> Result<(), E> {
    let count = count_or_zero(read_count(symbol)?);
    write_count(symbol, count + 1.0)?;
    send_trade(symbol)
}

// ts: ParadexPublicWebSocket.unsubscribeFromTrades
pub fn unsubscribe_from_trades<E>(
    symbol: &[u16],
    read_count: impl FnOnce(&[u16]) -> Result<Option<f64>, E>,
    delete_count: impl FnOnce(&[u16]) -> Result<(), E>,
    write_count: impl FnOnce(&[u16], f64) -> Result<(), E>,
) -> Result<(), E> {
    let count = count_or_zero(read_count(symbol)?);
    if count <= 1.0 {
        delete_count(symbol)?;
        return Ok(());
    }
    write_count(symbol, count - 1.0)
}

// ts: ParadexPublicWebSocket#cleanupOhlcvSubscription
pub fn cleanup_ohlcv_subscription<T, E>(
    key: &[u16],
    write_active: impl FnOnce(&[u16], bool) -> Result<(), E>,
    read_version: impl FnOnce(&[u16]) -> Result<Option<f64>, E>,
    write_version: impl FnOnce(&[u16], f64) -> Result<(), E>,
    has_timer: impl FnOnce(&[u16]) -> Result<bool, E>,
    read_timer: impl FnOnce(&[u16]) -> Result<Option<T>, E>,
    clear_timer: impl FnOnce(Option<T>) -> Result<(), E>,
    delete_timer: impl FnOnce(&[u16]) -> Result<(), E>,
    mut delete_state: impl FnMut(&'static str, &[u16]) -> Result<(), E>,
) -> Result<(), E> {
    write_active(key, false)?;
    write_version(key, count_or_zero(read_version(key)?) + 1.0)?;
    if has_timer(key)? {
        clear_timer(read_timer(key)?)?;
        delete_timer(key)?;
    }
    for field in [
        "ohlcvCallbacks",
        "ohlcvIntervals",
        "ohlcvLastTradeAt",
        "ohlcvLastRecoveryAt",
        "lastCandleCache",
    ] {
        delete_state(field, key)?;
    }
    Ok(())
}

// ts: ParadexPublicWebSocket#subscribeToChannels
pub fn subscribe_to_channels<M, E>(
    read_markets: impl FnOnce() -> Result<M, E>,
    mut visit_markets: impl FnMut(&M, &mut dyn FnMut(&[u16]) -> Result<(), E>) -> Result<(), E>,
    read_market_length: impl FnOnce(&M) -> Result<f64, E>,
    mut send_rpc: impl FnMut(SubscriptionMethod, Vec<u16>) -> Result<(), E>,
    mut send_trade: impl FnMut(&[u16], bool) -> Result<(), E>,
    mut log: impl FnMut(String, &'static str) -> Result<(), E>,
    mut read_book_size: impl FnMut() -> Result<f64, E>,
    visit_books: impl FnOnce(&mut dyn FnMut(&[u16], f64) -> Result<(), E>) -> Result<(), E>,
    mut subscribe_book: impl FnMut(&[u16]) -> Result<(), E>,
) -> Result<(), E> {
    let markets = read_markets()?;
    visit_markets(&markets, &mut |symbol| {
        send_rpc(SubscriptionMethod::Subscribe, channel("bbo.", symbol))
    })?;
    visit_markets(&markets, &mut |symbol| send_trade(symbol, true))?;
    send_rpc(
        SubscriptionMethod::Subscribe,
        channel("markets_summary", &[]),
    )?;
    log(
        format!(
            "Subscribed to bbo + trades for {} markets + markets_summary",
            js_number_string(read_market_length(&markets)?)
        ),
        "info",
    )?;
    if read_book_size()? > 0.0 {
        visit_books(&mut |symbol, size| {
            if size > 0.0 {
                subscribe_book(symbol)?;
            }
            Ok(())
        })?;
        log(
            format!(
                "Resubscribed to {} orderbooks",
                js_number_string(read_book_size()?)
            ),
            "info",
        )?;
    }
    Ok(())
}
