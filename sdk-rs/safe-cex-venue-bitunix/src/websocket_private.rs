//! Private heartbeat, debounce timers and asynchronous store reconciliation.
use safe_cex_num::JsJsonValue;
use std::future::Future;

pub fn ping<E>(
    disposed: bool,
    performance_now: impl FnOnce() -> Result<f64, E>,
    set_ping_at: impl FnOnce(f64) -> Result<(), E>,
    send_with_log: impl FnOnce(JsJsonValue) -> Result<(), E>,
    handle_pong: impl FnOnce() -> Result<(), E>,
    track_received: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if !disposed {
        set_ping_at(performance_now()?)?;
        send_with_log(JsJsonValue::Object(vec![(
            "op".into(),
            JsJsonValue::String("ping".into()),
        )]))?;
        handle_pong()?;
        track_received()?;
    }
    Ok(())
}

fn debounce<T, H, E>(
    (field, handler): (&'static str, &'static str),
    (mut read, active): (
        impl FnMut(&'static str) -> Result<Option<T>, E>,
        impl FnOnce(&Option<T>) -> bool,
    ),
    (clear, get_handler): (
        impl FnOnce(Option<T>) -> Result<(), E>,
        impl FnOnce(&'static str) -> Result<H, E>,
    ),
    (schedule, set): (
        impl FnOnce(H, f64) -> Result<T, E>,
        impl FnOnce(&'static str, T) -> Result<(), E>,
    ),
) -> Result<(), E> {
    if active(&read(field)?) {
        clear(read(field)?)?;
    }
    let timer = schedule(get_handler(handler)?, 1000.0)?;
    set(field, timer)
}

pub fn throttled_fetch_balance<T, H, E>(
    read: impl FnMut(&'static str) -> Result<Option<T>, E>,
    active: impl FnOnce(&Option<T>) -> bool,
    clear: impl FnOnce(Option<T>) -> Result<(), E>,
    get_handler: impl FnOnce(&'static str) -> Result<H, E>,
    schedule: impl FnOnce(H, f64) -> Result<T, E>,
    set: impl FnOnce(&'static str, T) -> Result<(), E>,
) -> Result<(), E> {
    debounce(
        ("balanceThrottleTimeout", "fetchThrottledBalance"),
        (read, active),
        (clear, get_handler),
        (schedule, set),
    )
}

pub fn throttled_fetch_orders<T, H, E>(
    read: impl FnMut(&'static str) -> Result<Option<T>, E>,
    active: impl FnOnce(&Option<T>) -> bool,
    clear: impl FnOnce(Option<T>) -> Result<(), E>,
    get_handler: impl FnOnce(&'static str) -> Result<H, E>,
    schedule: impl FnOnce(H, f64) -> Result<T, E>,
    set: impl FnOnce(&'static str, T) -> Result<(), E>,
) -> Result<(), E> {
    debounce(
        ("ordersThrottleTimeout", "fetchThrottledOrders"),
        (read, active),
        (clear, get_handler),
        (schedule, set),
    )
}

pub fn throttled_fetch_positions<T, H, E>(
    read: impl FnMut(&'static str) -> Result<Option<T>, E>,
    active: impl FnOnce(&Option<T>) -> bool,
    clear: impl FnOnce(Option<T>) -> Result<(), E>,
    get_handler: impl FnOnce(&'static str) -> Result<H, E>,
    schedule: impl FnOnce(H, f64) -> Result<T, E>,
    set: impl FnOnce(&'static str, T) -> Result<(), E>,
) -> Result<(), E> {
    debounce(
        ("positionsThrottleTimeout", "fetchThrottledPositions"),
        (read, active),
        (clear, get_handler),
        (schedule, set),
    )
}

pub async fn fetch_throttled_balance<B, P, L, E, F>(
    reset_timeout: impl FnOnce(&'static str) -> Result<(), E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    fetch: impl FnOnce() -> F,
    read_loaded: impl FnOnce() -> Result<L, E>,
    update: impl FnOnce(B, P, L, [(&'static str, bool); 2]) -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<(B, P), E>>,
{
    reset_timeout("balanceThrottleTimeout")?;
    if !is_disposed()? {
        let (balance, positions) = fetch().await?;
        let loaded = read_loaded()?;
        update(
            balance,
            positions,
            loaded,
            [("balance", true), ("positions", true)],
        )?;
    }
    Ok(())
}

pub async fn fetch_throttled_orders<O, L, E, F>(
    reset_timeout: impl FnOnce(&'static str) -> Result<(), E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    fetch: impl FnOnce() -> F,
    read_loaded: impl FnOnce() -> Result<L, E>,
    update: impl FnOnce(&'static str, O, L, (&'static str, bool)) -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<O, E>>,
{
    reset_timeout("ordersThrottleTimeout")?;
    if !is_disposed()? {
        let orders = fetch().await?;
        let loaded = read_loaded()?;
        update("orders", orders, loaded, ("orders", true))?;
    }
    Ok(())
}

pub async fn fetch_throttled_positions<P, L, E, F>(
    reset_timeout: impl FnOnce(&'static str) -> Result<(), E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    fetch: impl FnOnce() -> F,
    read_loaded: impl FnOnce() -> Result<L, E>,
    update: impl FnOnce(&'static str, P, L, (&'static str, bool)) -> Result<(), E>,
) -> Result<(), E>
where
    F: Future<Output = Result<P, E>>,
{
    reset_timeout("positionsThrottleTimeout")?;
    if !is_disposed()? {
        let positions = fetch().await?;
        let loaded = read_loaded()?;
        update("positions", positions, loaded, ("positions", true))?;
    }
    Ok(())
}

pub async fn send_login<S, Sync, Delay, Sign>(
    clock: impl FnOnce() -> Result<f64, String>,
    nonce: impl FnOnce() -> Result<String, String>,
    key: impl FnOnce() -> Result<String, String>,
    sync: impl FnOnce() -> Sync,
    sleep: impl FnOnce(f64) -> Delay,
    remote_signer: impl FnOnce() -> Result<Option<S>, String>,
    secret: impl FnOnce() -> Result<String, String>,
    id: impl FnOnce() -> Result<String, String>,
    account_name: impl FnOnce(&S) -> Result<JsJsonValue, String>,
    sign: impl FnOnce(S, JsJsonValue) -> Sign,
    report: impl FnOnce(String) -> Result<(), String>,
    send: impl FnOnce(JsJsonValue) -> Result<(), String>,
) -> Result<(), String>
where
    Sync: Future<Output = Result<(), String>>,
    Delay: Future<Output = Result<(), String>>,
    Sign: Future<Output = Result<serde_json::Value, String>>,
{
    use safe_cex_num::{js_number_string, js_string, js_trim, js_truthy};
    let timestamp = clock()?;
    let nonce = nonce()?;
    let api_key = key()?;
    let api_key = js_trim(&api_key).to_owned();
    sync().await?;
    sleep(25.0).await?;
    let to_sign = format!("{nonce}{}{api_key}", js_number_string(timestamp));
    let signature = if let Some(signer) = remote_signer()? {
        let request_id = id()?;
        let account = account_name(&signer)?;
        let result = sign(
            signer,
            JsJsonValue::Object(vec![
                ("id".into(), JsJsonValue::String(request_id)),
                ("type".into(), JsJsonValue::String("ws-auth".into())),
                ("exchangeName".into(), JsJsonValue::String("bitunix".into())),
                ("accountName".into(), account),
                (
                    "wsAuth".into(),
                    JsJsonValue::Object(vec![
                        ("toSign".into(), JsJsonValue::String(to_sign)),
                        (
                            "outputFormat".into(),
                            JsJsonValue::String("bitunix-double-sha256".into()),
                        ),
                        (
                            "extraData".into(),
                            JsJsonValue::Object(vec![
                                ("timestamp".into(), JsJsonValue::Number(timestamp)),
                                ("nonce".into(), JsJsonValue::String(nonce.clone())),
                            ]),
                        ),
                    ]),
                ),
            ]),
        )
        .await?;
        if result.is_null() {
            return Err("Cannot read properties of null (reading 'error')".into());
        }
        if js_truthy(result.get("error")) || !js_truthy(result.get("wsAuthSignature")) {
            let reason = result.get("error").filter(|v| !v.is_null()).map_or_else(
                || Ok("no signature returned".to_owned()),
                |v| js_string(Some(v)).map_err(|e| e.message.to_owned()),
            )?;
            report(format!("Remote WS signing unavailable: {reason}"))?;
            return Ok(());
        }
        JsJsonValue::from(result["wsAuthSignature"].clone())
    } else {
        let digest = crate::utils::sha256_hex(&to_sign);
        let secret = secret()?;
        JsJsonValue::String(crate::utils::sha256_hex(&format!(
            "{digest}{}",
            js_trim(&secret)
        )))
    };
    send(JsJsonValue::Object(vec![
        ("op".into(), JsJsonValue::String("login".into())),
        (
            "args".into(),
            JsJsonValue::Array(vec![JsJsonValue::Object(vec![
                ("apiKey".into(), JsJsonValue::String(api_key)),
                ("timestamp".into(), JsJsonValue::Number(timestamp)),
                ("nonce".into(), JsJsonValue::String(nonce)),
                ("sign".into(), signature),
            ])]),
        ),
    ]))
}
