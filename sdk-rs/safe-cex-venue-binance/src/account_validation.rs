use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_string, js_truthy, parse_float};
use safe_cex_types::Balance;
use serde_json::Value;
use std::future::Future;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountValidationClient {
    Primary,
    Secondary,
    Spot,
}

/// Valid always has the source's balance property; None is its undefined value.
/// Invalid has only the error property, which can carry an uncoerced JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum AccountValidation<B> {
    Valid { balance: Option<B> },
    Invalid { error: Value },
}

/// Decoded fetchBalance/fetchPositionsAndBalance result. An absent container or
/// balance models the best-effort undefined result; B is forwarded without copying.
#[derive(Debug, Clone, PartialEq)]
pub struct FetchedAccountBalance<B> {
    pub balance: Option<B>,
}

// ts: BinanceExchange.validateAccount
/// Configured GET and fallible emission are class-composition inputs. Read the
/// secondary-key option after the primary request settles, never before it.
pub async fn validate_binance_account<F>(
    mut get: impl FnMut(AccountValidationClient, &'static str) -> F,
    secondary_key_present: impl FnOnce() -> bool,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<AccountValidation<Balance>, ExchangeError>
where
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    let balance = match get(AccountValidationClient::Primary, "/fapi/v3/account").await {
        Ok(data) => match parse_account_balance(data.as_ref()) {
            Ok(balance) => balance,
            Err(error) => return primary_failure(error, &mut emit_error),
        },
        Err(error) => return primary_failure(error, &mut emit_error),
    };
    if secondary_key_present()
        && let Err(error) = get(AccountValidationClient::Secondary, "/fapi/v3/account").await
    {
        return secondary_failure(error);
    }
    Ok(AccountValidation::Valid { balance })
}

// ts: BinancePMExchange.validateAccount
/// The primary and secondary GET callbacks are configured PAPI clients.
pub async fn validate_binance_pm_account<B, F, G>(
    mut get: impl FnMut(AccountValidationClient, &'static str) -> F,
    secondary_key_present: impl FnOnce() -> bool,
    fetch_balance: impl FnOnce() -> G,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<AccountValidation<B>, ExchangeError>
where
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
    G: Future<Output = Result<Option<FetchedAccountBalance<B>>, ExchangeError>>,
{
    if let Err(error) = get(AccountValidationClient::Primary, "/papi/v1/account").await {
        return primary_failure(error, &mut emit_error);
    }
    if secondary_key_present()
        && let Err(error) = get(AccountValidationClient::Secondary, "/papi/v1/account").await
    {
        return secondary_failure(error);
    }
    let balance = fetch_balance().await.ok().flatten().and_then(|r| r.balance);
    Ok(AccountValidation::Valid { balance })
}

// ts: BinanceSpotExchange.validateAccount
/// GET is configured xhr; fetch_balance is fetchPositionsAndBalance's decoded
/// result. Its rejection is swallowed, while emission failures escape.
pub async fn validate_binance_spot_account<B, F, G>(
    get: impl FnOnce(AccountValidationClient, &'static str) -> F,
    fetch_balance: impl FnOnce() -> G,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<AccountValidation<B>, ExchangeError>
where
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
    G: Future<Output = Result<Option<FetchedAccountBalance<B>>, ExchangeError>>,
{
    if let Err(error) = get(AccountValidationClient::Spot, "/api/v3/account").await {
        return primary_failure(error, &mut emit_error);
    }
    let balance = fetch_balance().await.ok().flatten().and_then(|r| r.balance);
    Ok(AccountValidation::Valid { balance })
}

fn parse_account_balance(data: Option<&Value>) -> Result<Option<Balance>, ExchangeError> {
    let parse = |key: &str| {
        js_string(data.and_then(|data| data.get(key)))
            .map(|text| parse_float(&text))
            .map_err(coercion_error)
    };
    let total = parse("totalWalletBalance")?;
    if !total.is_finite() {
        return Ok(None);
    }
    let zero_if_falsy = |n: f64| if n == 0.0 || n.is_nan() { 0.0 } else { n };
    let free = zero_if_falsy(parse("availableBalance")?);
    let upnl = zero_if_falsy(parse("totalUnrealizedProfit")?);
    Ok(Some(Balance {
        total,
        free,
        used: total - free,
        upnl,
        info: None,
        currencies: None,
        coins: None,
    }))
}

fn error_message(error: &ExchangeError) -> Value {
    error
        .response_json_view()
        .as_ref()
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("msg"))
        .filter(|msg| js_truthy(Some(msg)))
        .cloned()
        .unwrap_or_else(|| Value::String(error.message.clone()))
}

fn primary_failure<B>(
    error: ExchangeError,
    emit_error: &mut impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<AccountValidation<B>, ExchangeError> {
    emit_error(error_message(&error))?;
    let message = error_message(&error);
    Ok(AccountValidation::Invalid {
        error: if js_truthy(Some(&message)) {
            message
        } else {
            Value::String("Invalid API key or secret".into())
        },
    })
}

fn secondary_failure<B>(error: ExchangeError) -> Result<AccountValidation<B>, ExchangeError> {
    let message = error_message(&error);
    let text = js_string(Some(&message)).map_err(coercion_error)?;
    Ok(AccountValidation::Invalid {
        error: Value::String(format!("Secondary Key: {text}")),
    })
}

fn coercion_error(error: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut result = ExchangeError::new(error.message);
    result.name = error.name.into();
    result
}
