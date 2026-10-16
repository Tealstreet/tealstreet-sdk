//! Account/runtime method bodies with configured transport, state and callbacks.
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue, js_string, parse_float};
use std::{cell::Cell, future::Future};

fn endpoint(name: &str) -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .expect("Backpack endpoint")
        .1
}

// ts: BackpackExchange.cancelOrder
pub fn cancel_order<'a, F>(
    symbol: &str,
    order_id: &str,
    market_id: impl FnOnce(&str) -> Result<Option<String>, ExchangeError>,
    delete: impl FnOnce(&'static str, JsJsonValue) -> Result<F, ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, ()>
where
    F: Future<Output = Result<(), ExchangeError>> + 'a,
{
    let response = market_id(symbol).and_then(|id| {
        let id = id.ok_or_else(|| ExchangeError::new(format!("Market not found: {symbol}")))?;
        delete(
            endpoint("ORDER"),
            JsJsonValue::Object(vec![(
                "data".into(),
                JsJsonValue::Object(vec![
                    ("symbol".into(), JsJsonValue::String(id)),
                    ("orderId".into(), JsJsonValue::String(order_id.into())),
                ]),
            )]),
        )
    });
    let catch = |error: ExchangeError| {
        log(
            &format!("Error canceling order: {}", error.js_error_string()),
            "error",
        )?;
        Err(error)
    };
    match response {
        Err(error) => Box::pin(std::future::ready(catch(error))),
        Ok(response) => Box::pin(async move {
            match response.await {
                Ok(()) => Ok(()),
                Err(error) => catch(error),
            }
        }),
    }
}

fn balance_field(data: &JsJsonValue, name: &str) -> Result<f64, ExchangeError> {
    let value = property(data, name)?;
    if matches!(value, JsJsonValue::Null | JsJsonValue::Undefined) {
        return Ok(0.0);
    }
    let text = js_string(Some(&crate::signing::string_input(value))).map_err(|error| {
        let mut result = ExchangeError::new(error.message);
        result.name = error.name.into();
        result
    })?;
    Ok(parse_float(&text))
}

// ts: BackpackExchange.fetchBalance
pub fn fetch_balance<'a, F>(
    read: impl FnOnce(&'static str) -> Result<F, ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, safe_cex_types::Balance>
where
    F: Future<Output = Result<JsJsonValue, ExchangeError>> + 'a,
{
    let catch = |error: ExchangeError| {
        log(
            &format!("Error fetching balance: {}", error.js_error_string()),
            "error",
        )?;
        Err(error)
    };
    let response = match read(endpoint("CAPITAL_COLLATERAL")) {
        Ok(response) => response,
        Err(error) => return Box::pin(std::future::ready(catch(error))),
    };
    Box::pin(async move {
        let result = response.await.and_then(|data| {
            let total = balance_field(&data, "netEquity")?;
            let free = balance_field(&data, "netEquityAvailable")?;
            let used = balance_field(&data, "netEquityLocked")?;
            let upnl = balance_field(&data, "pnlUnrealized")?;
            Ok(safe_cex_types::Balance {
                total,
                free,
                used,
                upnl,
                info: None,
                currencies: None,
                coins: None,
            })
        });
        match result {
            Ok(balance) => Ok(balance),
            Err(error) => catch(error),
        }
    })
}

pub type ValidationFailure = std::rc::Rc<std::cell::RefCell<JsJsonValue>>;
pub struct ValidationResult<P> {
    pub error: JsJsonValue,
    /// None is absent; Some(None) is an own undefined property after balance rejection.
    pub balance: Option<Option<P>>,
}

fn optional_property<'a>(value: &'a JsJsonValue, name: &str) -> &'a JsJsonValue {
    match value {
        JsJsonValue::Object(fields) => fields
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map_or(&JsJsonValue::Undefined, |(_, value)| value),
        _ => &JsJsonValue::Undefined,
    }
}
fn validation_message(error: &JsJsonValue) -> JsJsonValue {
    let response = optional_property(error, "response");
    let data = optional_property(response, "data");
    let message = optional_property(data, "msg");
    if truthy(message) {
        message.clone()
    } else {
        optional_property(error, "message").clone()
    }
}
fn validation_catch<P, E>(
    error: ValidationFailure,
    emit: impl FnOnce(&JsJsonValue) -> Result<(), E>,
) -> Result<ValidationResult<P>, E> {
    let message = validation_message(&error.borrow());
    emit(&message)?;
    // Source reads again after emitting; an emitter may have changed the error.
    let message = validation_message(&error.borrow());
    Ok(ValidationResult {
        error: if truthy(&message) {
            message
        } else {
            JsJsonValue::String("Invalid API key or secret".into())
        },
        balance: None,
    })
}

// ts: BackpackExchange.validateAccount
pub fn validate_account<'a, P: 'a, E: 'a, F, B>(
    read: impl FnOnce(&'static str) -> Result<F, ValidationFailure>,
    balance: impl FnOnce() -> Result<B, ValidationFailure> + 'a,
    emit: impl FnOnce(&JsJsonValue) -> Result<(), E> + 'a,
) -> std::pin::Pin<Box<dyn Future<Output = Result<ValidationResult<P>, E>> + 'a>>
where
    F: Future<Output = Result<(), ValidationFailure>> + 'a,
    B: Future<Output = Result<P, ValidationFailure>> + 'a,
{
    let response = match read(endpoint("CAPITAL")) {
        Ok(response) => response,
        Err(error) => return Box::pin(std::future::ready(validation_catch(error, emit))),
    };
    Box::pin(async move {
        if let Err(error) = response.await {
            return validation_catch(error, emit);
        }
        let response = match balance() {
            Ok(response) => response,
            Err(error) => return validation_catch(error, emit),
        };
        Ok(ValidationResult {
            error: JsJsonValue::String(String::new()),
            balance: Some(response.await.ok()),
        })
    })
}

fn series_member<R, E>(
    index: &mut usize,
    length: &mut impl FnMut() -> Result<usize, E>,
    element: &mut impl FnMut(usize) -> Result<Option<R>, E>,
) -> Result<Option<R>, E> {
    while *index < length()? {
        let current = *index;
        *index += 1;
        if let Some(element) = element(current)? {
            return Ok(Some(element));
        }
    }
    Ok(None)
}

// ts: BackpackExchange.cancelOrders
pub fn cancel_orders<'a, T: 'a, E: 'a, R, C>(
    mut length: impl FnMut() -> Result<usize, E> + 'a,
    mut element: impl FnMut(usize) -> Result<Option<R>, E> + 'a,
    mut cancel: impl FnMut(T) -> Result<C, E> + 'a,
) -> std::pin::Pin<Box<dyn Future<Output = Result<(), E>> + 'a>>
where
    R: Future<Output = Result<T, E>> + 'a,
    C: Future<Output = Result<(), E>> + 'a,
{
    // Actual p-iteration reads length/presence/first item before its first await.
    let mut index = 0;
    let first = series_member(&mut index, &mut length, &mut element);
    Box::pin(async move {
        let mut current = first?;
        while let Some(item) = current {
            // Element rejection is outside the source callback's catch.
            let item = item.await?;
            if let Ok(response) = cancel(item) {
                let _ignored = response.await;
            }
            current = series_member(&mut index, &mut length, &mut element)?;
        }
        Ok(())
    })
}

// ts: BackpackExchange.cancelAllOrders
pub fn cancel_all_orders<'a, T: 'a, R, C>(
    length: impl FnMut() -> Result<usize, ExchangeError> + 'a,
    element: impl FnMut(usize) -> Result<Option<R>, ExchangeError> + 'a,
    mut market_id: impl FnMut(T) -> Result<String, ExchangeError> + 'a,
    mut delete: impl FnMut(&'static str, JsJsonValue) -> Result<C, ExchangeError> + 'a,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, ()>
where
    R: Future<Output = Result<T, ExchangeError>> + 'a,
    C: Future<Output = Result<(), ExchangeError>> + 'a,
{
    let action = cancel_orders(length, element, move |market| {
        let id = market_id(market)?;
        delete(
            endpoint("ORDERS"),
            JsJsonValue::Object(vec![(
                "data".into(),
                JsJsonValue::Object(vec![("symbol".into(), JsJsonValue::String(id))]),
            )]),
        )
    });
    Box::pin(async move {
        match action.await {
            Ok(()) => Ok(()),
            Err(error) => {
                log(
                    &format!("Error canceling all orders: {}", error.js_error_string()),
                    "error",
                )?;
                Err(error)
            }
        }
    })
}

// ts: BackpackExchange.dispose
pub fn dispose<E>(
    disposed: &Cell<bool>,
    public: impl FnOnce() -> Result<(), E>,
    private: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    disposed.set(true);
    public()?;
    private()
}

fn leverage_error(
    error: ExchangeError,
    emit: impl FnOnce(&str, ExchangeError) -> Result<(), ExchangeError>,
    mut log: impl FnMut(&str, &str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let message = format!("Failed to set leverage: {}", error.js_error_string());
    emit("error", ExchangeError::new(message.clone()))?;
    log(&message, "error")?;
    Err(error)
}

pub struct PositionsUpdate<P> {
    pub positions: P,
}

// ts: BackpackExchange.setLeverage
pub fn set_leverage<'a, P: 'a, F, R>(
    account_leverage_limit: &'a Cell<f64>,
    leverage: f64,
    patch: impl FnOnce(&'static str, JsJsonValue) -> Result<F, ExchangeError>,
    mut log: impl FnMut(&str, &str) -> Result<(), ExchangeError> + 'a,
    emit: impl FnOnce(&str, ExchangeError) -> Result<(), ExchangeError> + 'a,
    positions: impl FnOnce() -> Result<R, ExchangeError> + 'a,
    update: impl FnOnce(PositionsUpdate<P>) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, ()>
where
    F: Future<Output = Result<(), ExchangeError>> + 'a,
    R: Future<Output = Result<P, ExchangeError>> + 'a,
{
    // The symbol and side arguments are unused in the source. The leverage
    // request is constructed eagerly; factory errors enter the catch eagerly.
    let response = match patch(
        account_endpoint(),
        JsJsonValue::Object(vec![(
            "leverageLimit".into(),
            JsJsonValue::String(safe_cex_num::js_number_string(leverage)),
        )]),
    ) {
        Ok(response) => response,
        Err(error) => return Box::pin(std::future::ready(leverage_error(error, emit, log))),
    };
    Box::pin(async move {
        let result = async {
            response.await?;
            account_leverage_limit.set(leverage);
            log(
                &format!(
                    "Account leverage limit set to {}x",
                    safe_cex_num::js_number_string(leverage)
                ),
                "info",
            )?;
            let positions = positions()?.await?;
            update(PositionsUpdate { positions })
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) => leverage_error(error, emit, log),
        }
    })
}

// ts: BackpackExchange.cancelSymbolOrders
pub fn cancel_symbol_orders<'a, F>(
    symbol: &str,
    market_id: impl FnOnce(&str) -> Result<Option<String>, ExchangeError>,
    delete: impl FnOnce(&'static str, JsJsonValue) -> Result<F, ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, ()>
where
    F: Future<Output = Result<(), ExchangeError>> + 'a,
{
    let result = market_id(symbol).and_then(|id| {
        id.map(|id| {
            delete(
                crate::constants::ENDPOINTS
                    .iter()
                    .find(|(name, _)| *name == "ORDERS")
                    .expect("Backpack ORDERS endpoint")
                    .1,
                JsJsonValue::Object(vec![(
                    "data".into(),
                    JsJsonValue::Object(vec![("symbol".into(), JsJsonValue::String(id))]),
                )]),
            )
        })
        .transpose()
    });
    let catch = |error: ExchangeError| {
        log(
            &format!("Error canceling symbol orders: {}", error.js_error_string()),
            "error",
        )?;
        Err(error)
    };
    match result {
        Ok(None) => Box::pin(std::future::ready(Ok(()))),
        Err(error) => Box::pin(std::future::ready(catch(error))),
        Ok(Some(response)) => Box::pin(async move {
            match response.await {
                Ok(()) => Ok(()),
                Err(error) => catch(error),
            }
        }),
    }
}

fn leverage_limit(data: &JsJsonValue) -> Result<f64, ExchangeError> {
    let value = property(data, "leverageLimit")?;
    // Reuse the existing prepared-data lowering for shared String coercion.
    // Undefined and null both yield NaN here, so their spelling is unobserved.
    let text = js_string(Some(&crate::signing::string_input(value))).map_err(|error| {
        let mut result = ExchangeError::new(error.message);
        result.name = error.name.into();
        result
    })?;
    let value = parse_float(&text);
    Ok(if value == 0.0 || value.is_nan() {
        1.0
    } else {
        value
    })
}

fn property<'a>(data: &'a JsJsonValue, name: &str) -> Result<&'a JsJsonValue, ExchangeError> {
    Ok(match data {
        JsJsonValue::Undefined | JsJsonValue::Null => {
            let kind = if matches!(data, JsJsonValue::Null) {
                "null"
            } else {
                "undefined"
            };
            let mut error = ExchangeError::new(format!(
                "Cannot read properties of {kind} (reading '{name}')"
            ));
            error.name = "TypeError".into();
            return Err(error);
        }
        JsJsonValue::Object(fields) => fields
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map_or(&JsJsonValue::Undefined, |(_, value)| value),
        _ => &JsJsonValue::Undefined,
    })
}

fn truthy(value: &JsJsonValue) -> bool {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => false,
        JsJsonValue::Bool(value) => *value,
        JsJsonValue::Number(value) => *value != 0.0 && !value.is_nan(),
        JsJsonValue::String(value) => !value.is_empty(),
        JsJsonValue::BigInt(value) => *value != 0.into(),
        JsJsonValue::Array(_) | JsJsonValue::Object(_) => true,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountIdentity {
    pub user_id: JsJsonValue,
    pub affiliate_id: JsJsonValue,
}

// ts: BackpackExchange.getAccount
pub fn get_account<'a, F>(
    read: impl FnOnce(&'static str) -> F,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, AccountIdentity>
where
    F: Future<Output = Result<JsJsonValue, ExchangeError>> + 'a,
{
    let response = read(account_endpoint());
    Box::pin(async move {
        let result = response.await.and_then(|data| {
            let account_id = property(&data, "accountId")?;
            let user_id = if truthy(account_id) {
                account_id
            } else {
                property(&data, "userId")?
            };
            Ok(AccountIdentity {
                user_id: user_id.clone(),
                affiliate_id: JsJsonValue::Null,
            })
        });
        match result {
            Ok(account) => Ok(account),
            Err(error) => {
                log(
                    &format!("Error fetching account: {}", error.js_error_string()),
                    "error",
                )?;
                Err(error)
            }
        }
    })
}

fn account_endpoint() -> &'static str {
    crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == "ACCOUNT")
        .expect("Backpack ACCOUNT endpoint")
        .1
}

// ts: BackpackExchange.getPositionMode
pub fn get_position_mode() -> ExchangeFuture<'static, safe_cex_types::PositionMode> {
    // Preserve the source's advertised UI mode, despite its single upstream net position.
    Box::pin(std::future::ready(Ok(safe_cex_types::PositionMode::Hedged)))
}

// ts: BackpackExchange.changePositionMode
pub fn change_position_mode(
    _hedged: bool,
    emit: impl FnOnce(&str, ExchangeError) -> Result<(), ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError>,
) -> ExchangeFuture<'static, ()> {
    unsupported_mode(
        "Backpack only supports Hedged position mode - this cannot be changed",
        emit,
        log,
    )
}

// ts: BackpackExchange.changeMarginType
pub fn change_margin_type(
    _isolated: bool,
    _symbol: Option<&str>,
    emit: impl FnOnce(&str, ExchangeError) -> Result<(), ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError>,
) -> ExchangeFuture<'static, ()> {
    unsupported_mode(
        "Backpack only supports Cross margin mode - this cannot be changed",
        emit,
        log,
    )
}

fn unsupported_mode(
    message: &str,
    emit: impl FnOnce(&str, ExchangeError) -> Result<(), ExchangeError>,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError>,
) -> ExchangeFuture<'static, ()> {
    // Source has no await: side effects happen at construction, and callback
    // exceptions become rejected promises rather than synchronous throws.
    let result = emit("error", ExchangeError::new(message)).and_then(|()| log(message, "warning"));
    Box::pin(std::future::ready(result))
}

// ts: BackpackExchange.fetchAccountInfo
pub fn fetch_account_info<'a, F>(
    account_leverage_limit: &'a Cell<f64>,
    read: impl FnOnce(&'static str) -> F,
    log: impl FnOnce(&str, &str) -> Result<(), ExchangeError> + 'a,
) -> ExchangeFuture<'a, ()>
where
    F: Future<Output = Result<JsJsonValue, ExchangeError>> + 'a,
{
    // Source calls xhr.get before its first await, not on a later poll.
    let response = read(account_endpoint());
    Box::pin(async move {
        match response.await.and_then(|data| leverage_limit(&data)) {
            Ok(limit) => {
                account_leverage_limit.set(limit);
                Ok(())
            }
            Err(error) => {
                // ts-bug/source compatibility: the catch swallows the read/coercion
                // failure and RETAINS prior leverage; it does not reset a default.
                // A failure from logging itself still escapes, with metadata intact.
                log(
                    &format!("Error fetching account info: {}", error.js_error_string()),
                    "warning",
                )
            }
        }
    })
}

// ts: BackpackExchange.listenOHLCV
pub fn listen_ohlcv<O, C, R, E>(
    opts: &O,
    callback: &C,
    listen: impl FnOnce(&O, &C) -> Result<R, E>,
) -> Result<R, E> {
    listen(opts, callback)
}
// ts: BackpackExchange.listenOrderBook
pub fn listen_order_book<S, C, R, E>(
    symbol: &S,
    callback: &C,
    listen: impl FnOnce(&S, &C) -> Result<R, E>,
) -> Result<R, E> {
    listen(symbol, callback)
}
