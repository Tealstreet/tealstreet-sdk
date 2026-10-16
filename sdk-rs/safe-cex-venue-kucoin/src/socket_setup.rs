use std::future::Future;

// V and expression capabilities retain live values, identity and original errors.
// post takes a table-key selector and owns the whole live xhr.post(table.key) call.
// Callers poll immediately to match JavaScript initiation before its first await.
pub async fn connect_futures_public<V, E, F: Future<Output = Result<V, E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    post: impl FnOnce(&str) -> Result<F, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    code_ok: impl Fn(&V) -> bool,
    truthy: impl Fn(&V) -> bool,
    mut state: impl FnMut(&str) -> Result<V, E>,
    mut call: impl FnMut(&str, Option<&V>, Option<&str>) -> Result<V, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    date_now: impl FnOnce() -> Result<V, E>,
    string_value: impl FnOnce(Vec<u16>) -> V,
    retry: impl FnOnce(&E, &str) -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let result = async {
        let response = post("GET_PUBLIC_WS_TOKEN")?.await?;
        let data = get(&response, "data")?;
        if !code_ok(&get(&data, "code")?) || !truthy(&get(&data, "data")?) {
            call("emit-error", None, Some("Failed to get WebSocket token"))?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let token = get(&bucket, "token")?;
        call("set:token", Some(&token), None)?;
        let bucket = get(&data, "data")?;
        let servers = get(&bucket, "instanceServers")?;
        if !truthy(&servers) || !truthy(&get(&servers, "length")?) {
            call("emit-error", None, Some("No WebSocket servers available"))?;
            return Ok(());
        }
        let server = get(&servers, "0")?;
        let mut endpoint = template(&get(&server, "endpoint")?)?;
        endpoint.extend("?token=".encode_utf16());
        endpoint.extend(template(&state("token")?)?);
        endpoint.extend("&connectId=".encode_utf16());
        endpoint.extend(template(&date_now()?)?);
        call("set:socketEndpoint", Some(&string_value(endpoint)), None)?;
        let socket = call("create-current", None, None)?;
        call("set:ws", Some(&socket), None)?;
        for event in ["open", "message", "close"] {
            call("add-listener", None, Some(event))?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => retry(&error, "Failed to connect to WebSocket"),
    }
}

pub async fn connect_futures_private<V, E, F: Future<Output = Result<V, E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    post: impl FnOnce(&str) -> Result<F, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    code_ok: impl Fn(&V) -> bool,
    truthy: impl Fn(&V) -> bool,
    mut state: impl FnMut(&str) -> Result<V, E>,
    mut call: impl FnMut(&str, Option<&V>, Option<&str>) -> Result<V, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    date_now: impl FnOnce() -> Result<V, E>,
    string_value: impl FnOnce(Vec<u16>) -> V,
    retry: impl FnOnce(&E, &str) -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let result = async {
        let response = post("GET_PRIVATE_WS_TOKEN")?.await?;
        let data = get(&response, "data")?;
        if !code_ok(&get(&data, "code")?) || !truthy(&get(&data, "data")?) {
            call(
                "emit-error",
                None,
                Some("Failed to get private WebSocket token"),
            )?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let token = get(&bucket, "token")?;
        call("set:token", Some(&token), None)?;
        let bucket = get(&data, "data")?;
        let servers = get(&bucket, "instanceServers")?;
        if !truthy(&servers) || !truthy(&get(&servers, "length")?) {
            call(
                "emit-error",
                None,
                Some("No private WebSocket servers available"),
            )?;
            return Ok(());
        }
        let server = get(&servers, "0")?;
        let mut endpoint = template(&get(&server, "endpoint")?)?;
        endpoint.extend("?token=".encode_utf16());
        endpoint.extend(template(&state("token")?)?);
        endpoint.extend("&connectId=".encode_utf16());
        endpoint.extend(template(&date_now()?)?);
        call("set:socketEndpoint", Some(&string_value(endpoint)), None)?;
        let endpoint = state("socketEndpoint")?;
        let socket = call("create-captured", Some(&endpoint), None)?;
        call("set:ws", Some(&socket), None)?;
        for event in ["open", "message", "close"] {
            call("add-listener", None, Some(event))?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => retry(&error, "Failed to connect to private WebSocket"),
    }
}

pub async fn connect_spot_public<V, E, F: Future<Output = Result<V, E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    post: impl FnOnce(&str) -> Result<F, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    code_ok: impl Fn(&V) -> bool,
    truthy: impl Fn(&V) -> bool,
    mut state: impl FnMut(&str) -> Result<V, E>,
    mut call: impl FnMut(&str, Option<&V>, Option<&str>) -> Result<V, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    date_now: impl FnOnce() -> Result<V, E>,
    string_value: impl FnOnce(Vec<u16>) -> V,
    retry: impl FnOnce(&E, &str) -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let result = async {
        let response = post("GET_PUBLIC_WS_TOKEN")?.await?;
        let data = get(&response, "data")?;
        if !code_ok(&get(&data, "code")?) || !truthy(&get(&data, "data")?) {
            call(
                "emit-token-error",
                Some(&data),
                Some("Failed to get KuCoin Spot public WebSocket token"),
            )?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let server = get(&bucket, "instanceServers?.0")?;
        if !truthy(&get(&server, "?.endpoint")?) {
            call(
                "emit-error",
                None,
                Some("No KuCoin Spot public WebSocket servers available"),
            )?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let token = get(&bucket, "token")?;
        call("set:token", Some(&token), None)?;
        let ping = get(&server, "pingInterval")?;
        let ping = if truthy(&ping) {
            ping
        } else {
            state("pingInterval")?
        };
        call("set:pingInterval", Some(&ping), None)?;
        let mut endpoint = template(&get(&server, "endpoint")?)?;
        endpoint.extend("?token=".encode_utf16());
        endpoint.extend(template(&state("token")?)?);
        endpoint.extend("&connectId=".encode_utf16());
        endpoint.extend(template(&date_now()?)?);
        call("set:socketEndpoint", Some(&string_value(endpoint)), None)?;
        let socket = call("create-current", None, None)?;
        call("set:ws", Some(&socket), None)?;
        for event in ["open", "message", "close"] {
            call("add-listener", None, Some(event))?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => retry(&error, "Failed to connect KuCoin Spot public WebSocket"),
    }
}

pub async fn connect_spot_private<V, E, F: Future<Output = Result<V, E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    post: impl FnOnce(&str) -> Result<F, E>,
    mut get: impl FnMut(&V, &str) -> Result<V, E>,
    code_ok: impl Fn(&V) -> bool,
    truthy: impl Fn(&V) -> bool,
    mut state: impl FnMut(&str) -> Result<V, E>,
    mut call: impl FnMut(&str, Option<&V>, Option<&str>) -> Result<V, E>,
    mut template: impl FnMut(&V) -> Result<Vec<u16>, E>,
    date_now: impl FnOnce() -> Result<V, E>,
    string_value: impl FnOnce(Vec<u16>) -> V,
    retry: impl FnOnce(&E, &str) -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let result = async {
        let response = post("GET_PRIVATE_WS_TOKEN")?.await?;
        let data = get(&response, "data")?;
        if !code_ok(&get(&data, "code")?) || !truthy(&get(&data, "data")?) {
            call(
                "emit-token-error",
                Some(&data),
                Some("Failed to get KuCoin Spot private WebSocket token"),
            )?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let server = get(&bucket, "instanceServers?.0")?;
        if !truthy(&get(&server, "?.endpoint")?) {
            call(
                "emit-error",
                None,
                Some("No KuCoin Spot private WebSocket servers available"),
            )?;
            return Ok(());
        }
        let bucket = get(&data, "data")?;
        let token = get(&bucket, "token")?;
        call("set:token", Some(&token), None)?;
        let ping = get(&server, "pingInterval")?;
        let ping = if truthy(&ping) {
            ping
        } else {
            state("pingInterval")?
        };
        call("set:pingInterval", Some(&ping), None)?;
        let mut endpoint = template(&get(&server, "endpoint")?)?;
        endpoint.extend("?token=".encode_utf16());
        endpoint.extend(template(&state("token")?)?);
        endpoint.extend("&connectId=".encode_utf16());
        endpoint.extend(template(&date_now()?)?);
        call("set:socketEndpoint", Some(&string_value(endpoint)), None)?;
        let socket = call("create-current", None, None)?;
        call("set:ws", Some(&socket), None)?;
        for event in ["open", "message", "close"] {
            call("add-listener", None, Some(event))?;
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(error) => retry(&error, "Failed to connect KuCoin Spot private WebSocket"),
    }
}

pub async fn reconnect_futures_public<V, E, F: Future<Output = Result<(), E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    previous_socket: impl FnOnce() -> Result<V, E>,
    mut socket_call: impl FnMut(&V, &str) -> Result<(), E>,
    mut effect: impl FnMut(&str) -> Result<(), E>,
    connect: impl FnOnce() -> Result<F, E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let previous = previous_socket()?;
    for event in ["open", "message", "close"] {
        socket_call(&previous, event)?;
    }
    effect("ws:undefined")?;
    socket_call(&previous, "close-socket")?;
    effect("token:null")?;
    effect("socketEndpoint:null")?;
    connect()?.await
}

pub async fn reconnect_futures_private<V, E, F: Future<Output = Result<(), E>>>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    previous_socket: impl FnOnce() -> Result<V, E>,
    mut socket_call: impl FnMut(&V, &str) -> Result<(), E>,
    mut effect: impl FnMut(&str) -> Result<(), E>,
    connect: impl FnOnce() -> Result<F, E>,
) -> Result<(), E> {
    if is_disposed()? {
        return Ok(());
    }
    let previous = previous_socket()?;
    for event in ["open", "message", "close"] {
        socket_call(&previous, event)?;
    }
    effect("ws:undefined")?;
    socket_call(&previous, "close-socket")?;
    effect("token:null")?;
    effect("socketEndpoint:null")?;
    effect("subscribed:false")?;
    connect()?.await
}

pub fn public_constructor<P, V, E>(
    parent: P,
    super_call: impl FnOnce(P) -> Result<(), E>,
    initialize_own_fields: impl FnOnce() -> Result<(), E>,
    mut optional_option: impl FnMut(&str) -> Result<Option<V>, E>,
    mut default: impl FnMut(&str) -> V,
    mut assign: impl FnMut(&str, V) -> Result<(), E>,
) -> Result<(), E> {
    super_call(parent)?;
    initialize_own_fields()?;
    for (field, fallback) in [("role", "detail"), ("shardIndex", "0"), ("shardCount", "1")] {
        let value = optional_option(field)?.unwrap_or_else(|| default(fallback));
        assign(field, value)?;
    }
    Ok(())
}

pub fn create_snapshot_websockets<P, T, A, E>(
    parent: &P,
    symbol_count: f64,
    array_from: impl FnOnce(f64, &mut dyn FnMut(usize) -> Result<T, E>) -> Result<A, E>,
    mut construct: impl FnMut(&P, &str, usize, f64) -> Result<T, E>,
) -> Result<A, E> {
    let ceil = (symbol_count / 380.0).ceil();
    let shard_count = if ceil.is_nan() {
        f64::NAN
    } else {
        ceil.max(1.0)
    };
    array_from(shard_count, &mut |index| {
        construct(parent, "snapshot", index, shard_count)
    })
}

// enumerate owns Object.keys followed by whole for-of, including IteratorClose.
// Keys retain UTF-16; the native body uses ordinary String intrinsic semantics.
pub fn subscribe_spot_public<E>(
    enumerate: impl FnOnce(&mut dyn FnMut(Vec<u16>) -> Result<(), E>) -> Result<(), E>,
    mut subscribe_topic: impl FnMut(&[u16]) -> Result<(), E>,
) -> Result<(), E> {
    subscribe_topic(&"/market/ticker:all".encode_utf16().collect::<Vec<_>>())?;
    let candle = "candle_".encode_utf16().collect::<Vec<_>>();
    let book = "orderBook_".encode_utf16().collect::<Vec<_>>();
    enumerate(&mut |topic| {
        if topic.starts_with(&candle) {
            let mut parts = topic[candle.len()..].split(|unit| *unit == 95);
            let symbol = parts.next().unwrap_or_default();
            let interval = parts
                .next()
                .unwrap_or(&[117, 110, 100, 101, 102, 105, 110, 101, 100]);
            let mut ws_topic = "/market/candles:".encode_utf16().collect::<Vec<_>>();
            ws_topic.extend_from_slice(symbol);
            ws_topic.push(95);
            ws_topic.extend_from_slice(interval);
            subscribe_topic(&ws_topic)?;
        }
        if topic.starts_with(&book) {
            let mut ws_topic = "/spotMarket/level2Depth50:"
                .encode_utf16()
                .collect::<Vec<_>>();
            ws_topic.extend_from_slice(&topic[book.len()..]);
            subscribe_topic(&ws_topic)?;
        }
        Ok(())
    })
}
