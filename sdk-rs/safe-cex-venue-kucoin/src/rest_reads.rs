use std::{future::Future, rc::Rc};

pub async fn validate_account<'a, V: Clone + 'a, C, E: 'a, F, B, H>(
    request: impl FnOnce(&str) -> Result<F, E>,
    mut read: impl FnMut(&V, &str) -> Result<V, E>,
    strict_equal_string: impl Fn(&V, &str) -> bool,
    truthy: impl Fn(&V) -> bool,
    fetch_balance: impl FnOnce() -> Result<B, E>,
    catch_balance: impl FnOnce(B, Rc<dyn Fn(E) -> V + 'a>) -> Result<H, E>,
    mut capture_emitter: impl FnMut() -> Result<C, E>,
    mut invoke_emitter: impl FnMut(C, V) -> Result<(), E>,
    error_value: impl FnOnce(&E) -> Result<V, E>,
    literal: impl Fn(&str) -> V,
    undefined: impl Fn() -> V,
    object: impl Fn(Vec<(&'static str, V)>) -> V,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
    B: Future<Output = Result<V, E>>,
    H: Future<Output = Result<V, E>>,
{
    let checked = async {
        let response = request("/api/v1/account-overview")?.await?;
        let data = read(&response, "data")?;
        let code = read(&data, "code")?;
        if !strict_equal_string(&code, "200000") {
            let message = read(&data, "msg")?;
            let message = if truthy(&message) {
                message
            } else {
                literal("Failed to validate account")
            };
            let emitter = capture_emitter()?;
            invoke_emitter(emitter, message.clone())?;
            return Ok(object(vec![("error", message)]));
        }
        let balance_promise = fetch_balance()?;
        let absent = undefined();
        let balance = catch_balance(balance_promise, Rc::new(move |_| absent.clone()))?.await?;
        Ok(object(vec![("error", literal("")), ("balance", balance)]))
    }
    .await;
    match checked {
        Ok(value) => Ok(value),
        Err(error) => {
            let message = error_value(&error)?;
            let message = if truthy(&message) {
                message
            } else {
                literal("Unknown error validating account")
            };
            let emitter = capture_emitter()?;
            invoke_emitter(emitter, message.clone())?;
            Ok(object(vec![("error", message)]))
        }
    }
}

pub async fn fetch_balance<V, C, E, F>(
    capture_request: impl FnOnce(&str) -> Result<C, E>,
    currency: impl FnOnce() -> Result<V, E>,
    invoke_request: impl FnOnce(C, V) -> Result<F, E>,
    mut read: impl FnMut(&V, &str) -> Result<V, E>,
    strict_equal_string: impl Fn(&V, &str) -> bool,
    truthy: impl Fn(&V) -> bool,
    capture_emitter: impl FnOnce() -> Result<C, E>,
    invoke_emitter: impl FnOnce(C, V) -> Result<(), E>,
    stored_balance: impl FnOnce() -> Result<V, E>,
    mut parse_field: impl FnMut(&V, &str, &str) -> Result<V, E>,
    literal: impl Fn(&str) -> V,
    object: impl Fn(Vec<(&'static str, V)>) -> V,
) -> Result<V, E>
where
    F: Future<Output = Result<V, E>>,
{
    let request = capture_request("BALANCE")?;
    let params = object(vec![("currency", currency()?)]);
    let response = invoke_request(request, object(vec![("params", params)]))?.await?;
    let data = read(&response, "data")?;
    let code = read(&data, "code")?;
    if !strict_equal_string(&code, "200000") {
        let emitter = capture_emitter()?;
        let message = read(&data, "msg")?;
        let message = if truthy(&message) {
            message
        } else {
            literal("Failed to fetch balance")
        };
        invoke_emitter(emitter, message)?;
        return stored_balance();
    }
    let account = read(&data, "data")?;
    if !truthy(&account) {
        return stored_balance();
    }
    Ok(object(vec![
        ("free", parse_field(&account, "availableBalance", "0")?),
        ("used", parse_field(&account, "orderMargin", "0")?),
        ("total", parse_field(&account, "accountEquity", "0")?),
        ("upnl", parse_field(&account, "unrealisedPNL", "0")?),
    ]))
}
