use crate::{
    subscriptions::{
        WeexSubscriptionRuntime, WeexSubscriptionState, resubscribe_topics, subscribe_ticker_topics,
    },
    types::PRIVATE_WS_REQUEST_PATH,
};

/// Partial control fields shared by the declared websocket open/ping/log algorithms.
#[derive(Debug, Clone)]
pub struct WeexWebsocketControlState {
    pub is_disposed: bool,
    pub subscription_id: f64,
    pub ping_at: f64,
    pub has_logged_unsupported: bool,
}

/// Send binds source JSON.stringify/sendWithLog; snapshot starts the source call without awaiting it.
pub struct WeexWebsocketControlRuntime<H, E> {
    pub performance_now: fn(&mut H, &mut WeexWebsocketControlState) -> Result<f64, E>,
    pub send_with_log:
        fn(&mut H, &mut WeexWebsocketControlState, &str, Option<&[&str]>, f64) -> Result<(), E>,
    pub snapshot: fn(&mut H, &mut WeexWebsocketControlState, f64) -> Result<(), E>,
    pub emit_reconnected: fn(&mut H, &mut WeexWebsocketControlState) -> Result<(), E>,
    pub log_warning: fn(&mut H, &mut WeexWebsocketControlState, &[u16]) -> Result<(), E>,
}

/// Credential getters and the already-ported signing helper are bound by the caller.
/// Generic values preserve absent or malformed key/passphrase values without asserting strings.
pub struct WeexAuthHeaderRuntime<H, V, E> {
    pub time_ms: fn(&mut H) -> Result<f64, E>,
    pub credential: fn(&mut H, &str) -> Result<V, E>,
    pub string: fn(&mut H, &[u16]) -> V,
    pub sign: fn(&mut H, V, &[u16]) -> Result<V, E>,
}

pub fn create_auth_headers<H, V, E>(
    host: &mut H,
    runtime: &WeexAuthHeaderRuntime<H, V, E>,
) -> Result<[(&'static str, V); 5], E> {
    let timestamp = safe_cex_num::js_number_string((runtime.time_ms)(host)?);
    let user_agent = (runtime.string)(host, &"Tealstreet".encode_utf16().collect::<Vec<_>>());
    let key = (runtime.credential)(host, "key")?;
    let passphrase = (runtime.credential)(host, "passphrase")?;
    let timestamp_value = (runtime.string)(host, &timestamp.encode_utf16().collect::<Vec<_>>());
    let secret = (runtime.credential)(host, "secret")?;
    let message: Vec<u16> = timestamp
        .encode_utf16()
        .chain(PRIVATE_WS_REQUEST_PATH.encode_utf16())
        .collect();
    let signature = (runtime.sign)(host, secret, &message)?;
    Ok([
        ("User-Agent", user_agent),
        ("ACCESS-KEY", key),
        ("ACCESS-PASSPHRASE", passphrase),
        ("ACCESS-TIMESTAMP", timestamp_value),
        ("ACCESS-SIGN", signature),
    ])
}

pub fn private_subscribe<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    let id = state.subscription_id;
    state.subscription_id += 1.0;
    (runtime.send_with_log)(
        host,
        state,
        "SUBSCRIBE",
        Some(&["account", "positions", "orders", "fill"]),
        id,
    )
}

pub fn log_unsupported_transport<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    reason: Option<&[u16]>,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    if state.has_logged_unsupported {
        return Ok(());
    }
    state.has_logged_unsupported = true;
    let default_reason: Vec<u16> =
        "browser WebSocket clients cannot attach WEEX private-auth headers"
            .encode_utf16()
            .collect();
    let message: Vec<u16> = "WEEX private WebSocket disabled: "
        .encode_utf16()
        .chain(reason.unwrap_or(&default_reason).iter().copied())
        .chain("; continuing with the REST safety loop".encode_utf16())
        .collect();
    (runtime.log_warning)(host, state, &message)
}

pub fn private_on_open<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    if state.is_disposed {
        return Ok(());
    }
    private_subscribe(host, state, runtime)?;
    (runtime.snapshot)(host, state, 10.0)?;
    (runtime.emit_reconnected)(host, state)
}

fn ping<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    if state.is_disposed {
        return Ok(());
    }
    state.ping_at = (runtime.performance_now)(host, state)?;
    let id = state.subscription_id;
    state.subscription_id += 1.0;
    (runtime.send_with_log)(host, state, "PING", None, id)
}

pub fn private_ping<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    ping(host, state, runtime)
}

pub fn public_ping<H, E>(
    host: &mut H,
    state: &mut WeexWebsocketControlState,
    runtime: &WeexWebsocketControlRuntime<H, E>,
) -> Result<(), E> {
    ping(host, state, runtime)
}

pub fn public_on_open<H, E>(
    host: &mut H,
    state: &mut WeexSubscriptionState,
    runtime: &WeexSubscriptionRuntime<H, E>,
    emit_reconnected: impl FnOnce(&mut H, &mut WeexSubscriptionState) -> Result<(), E>,
) -> Result<(), E> {
    if state.is_disposed {
        return Ok(());
    }
    state.operation_timestamps = Vec::new();
    state.has_logged_limit = false;
    subscribe_ticker_topics(host, state, runtime)?;
    resubscribe_topics(host, state, runtime)?;
    emit_reconnected(host, state)
}
