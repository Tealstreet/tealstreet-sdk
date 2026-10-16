use std::future::Future;

pub async fn validate_account<V: Clone, C, E, F, B>(
    request: impl FnOnce(&str) -> Result<F, E>,
    fetch_balance: impl FnOnce() -> Result<B, E>,
    capture_emitter: impl FnOnce() -> Result<C, E>,
    mut error_value: impl FnMut(&E, &str) -> Result<V, E>,
    invoke_emitter: impl FnOnce(C, V) -> Result<(), E>,
    truthy: impl Fn(&V) -> bool,
    literal: impl Fn(&str) -> V,
    undefined: impl Fn() -> V,
    object: impl Fn(Vec<(&'static str, V)>) -> V,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
    B: Future<Output = Result<V, E>>,
{
    let checked = async { request("BALANCE")?.await }.await;
    if let Err(error) = checked {
        let emitter = capture_emitter()?;
        let message = error_value(&error, "response-or-message")?;
        invoke_emitter(emitter, message)?;
        let message = error_value(&error, "response-or-message")?;
        let message = if truthy(&message) {
            message
        } else {
            error_value(&error, "message")?
        };
        return Ok(object(vec![("error", message)]));
    }
    let balance = async { fetch_balance()?.await }
        .await
        .unwrap_or_else(|_| undefined());
    Ok(object(vec![("error", literal("")), ("balance", balance)]))
}

pub async fn validate_eu_account<V, C, E, F>(
    request: impl FnOnce(&str) -> Result<F, E>,
    capture_emitter: impl FnOnce() -> Result<C, E>,
    mut error_value: impl FnMut(&E, &str) -> Result<V, E>,
    invoke_emitter: impl FnOnce(C, V) -> Result<(), E>,
    truthy: impl Fn(&V) -> bool,
    literal: impl Fn(&str) -> V,
    object: impl Fn(Vec<(&'static str, V)>) -> V,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    match async { request("ACCOUNT_CONFIG")?.await }.await {
        Ok(_) => Ok(object(vec![("error", literal(""))])),
        Err(error) => {
            let emitter = capture_emitter()?;
            let message = error_value(&error, "response-or-message")?;
            invoke_emitter(emitter, message)?;
            let message = error_value(&error, "response-or-message")?;
            let message = if truthy(&message) {
                message
            } else {
                error_value(&error, "message")?
            };
            Ok(object(vec![("error", message)]))
        }
    }
}

pub async fn fetch_positions<V, E, F>(
    symbols: V,
    fetch: impl FnOnce(&str, V) -> Result<F, E>,
    positions: impl FnOnce(V, &str) -> Result<V, E>,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    let response = fetch("fetchBalanceAndPositions", symbols)?.await?;
    positions(response, "positions")
}

pub async fn fetch_balance<V, E, F>(
    fetch: impl FnOnce(&str) -> Result<F, E>,
    balance: impl FnOnce(V, &str) -> Result<V, E>,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    let response = fetch("fetchBalanceAndOpenPositions")?.await?;
    balance(response, "balance")
}

pub async fn fetch_tickers<V, E, F>(
    fetch_raw: impl FnOnce(&str) -> Result<F, E>,
    parse_tickers: impl FnOnce(V) -> Result<V, E>,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    let raw = fetch_raw("fetchTickersRaw")?.await?;
    parse_tickers(raw)
}

pub async fn get_or_fetch_position_mode_and_account_level<V: Clone, C, E, F>(
    force: Option<V>,
    boolean: impl Fn(bool) -> V,
    mut cache: impl FnMut(&str) -> Result<V, E>,
    truthy: impl Fn(&V) -> bool,
    request: impl FnOnce(&str) -> Result<F, E>,
    decode: impl FnOnce(V) -> Result<(V, V), E>,
    strict_equal_string: impl Fn(&V, &str) -> bool,
    mut assign: impl FnMut(&str, V) -> Result<(), E>,
    parse_float: impl FnOnce(V) -> Result<V, E>,
    literal: impl Fn(&str) -> V,
    object: impl Fn(Vec<(&'static str, V)>) -> V,
    mut capture: impl FnMut(&str) -> Result<C, E>,
    mut error_value: impl FnMut(&E) -> Result<V, E>,
    invoke_emitter: impl FnOnce(C, V) -> Result<(), E>,
    new_error: impl FnOnce(C, V) -> Result<E, E>,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    let force = force.unwrap_or_else(|| boolean(false));
    if truthy(&cache("positionMode")?) && truthy(&cache("accountLevel")?) && !truthy(&force) {
        return Ok(object(vec![
            ("positionMode", cache("positionMode")?),
            ("accountLevel", cache("accountLevel")?),
        ]));
    }
    let loaded = async {
        let (pos_mode, account_level) = decode(request("ACCOUNT_CONFIG")?.await?)?;
        let position_mode = if strict_equal_string(&pos_mode, "long_short_mode") {
            literal("hedged")
        } else {
            literal("oneway")
        };
        assign("positionMode", position_mode)?;
        let level = if truthy(&account_level) {
            account_level
        } else {
            literal("2")
        };
        assign("accountLevel", parse_float(level)?)?;
        Ok(object(vec![
            ("positionMode", cache("positionMode")?),
            ("accountLevel", cache("accountLevel")?),
        ]))
    }
    .await;
    match loaded {
        Ok(value) => Ok(value),
        Err(error) => {
            let emitter = capture("emitAndLogError")?;
            let message = error_value(&error)?;
            invoke_emitter(emitter, message)?;
            let constructor = capture("Error")?;
            let message = error_value(&error)?;
            Err(new_error(constructor, message)?)
        }
    }
}
