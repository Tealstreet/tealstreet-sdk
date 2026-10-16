use safe_cex_exchange::runtime::ExchangeError;
use serde_json::Value;
use std::future::Future;

/// The source always returns its own userId property. None is undefined;
/// Some(Value::Null) is an explicit null. No wire serialization is implied.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountIdentity {
    pub user_id: Option<Value>,
}

// ts: BinanceExchange.getAccount
/// GET is the configured xhr.get operation returning decoded data. This body
/// accepts JSON values and undefined; executable getters/custom iterators and
/// concrete client/class wiring remain outside the decoded callback boundary.
pub async fn get_binance_account<F>(
    get: impl FnOnce(&'static str) -> F,
) -> Result<AccountIdentity, ExchangeError>
where
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    let data = get("/fapi/v3/balance").await?;
    let first = match data {
        Some(Value::Array(items)) => items.into_iter().next(),
        // The string iterator yields characters whose accountAlias is undefined.
        Some(Value::String(text)) if !text.is_empty() => {
            return Ok(AccountIdentity { user_id: None });
        }
        Some(Value::String(_)) => None,
        None => return Err(iterator_nullish("undefined")),
        Some(Value::Null) => return Err(iterator_nullish("null")),
        _ => {
            return Err(type_error(
                "(intermediate value) is not iterable (cannot read property Symbol(Symbol.iterator))",
            ));
        }
    };
    account_identity(first)
}

// ts: BinancePMExchange.getAccount
/// GET is configured papiXhr.get; the shared source-array validator runs before
/// first-row destructuring. Callback rejection metadata escapes unchanged.
pub async fn get_binance_pm_account<F>(
    get: impl FnOnce(&'static str) -> F,
) -> Result<AccountIdentity, ExchangeError>
where
    F: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    let data = get("/papi/v1/balance").await?;
    crate::response_shape::require_binance_pm_array_response("/papi/v1/balance", data.as_ref())
        .map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        })?;
    let Some(Value::Array(items)) = data else {
        unreachable!("shared validator admitted an array");
    };
    account_identity(items.into_iter().next())
}

/// Decoded fetchPositionsAndBalance container. Present includes primitive
/// non-nullish results whose positions property is undefined. P is moved without
/// serialization, so production Position arrays retain their native f64 values.
#[derive(Debug, Clone, PartialEq)]
pub enum FetchedPositions<P> {
    Undefined,
    Null,
    Present { positions: Option<P> },
}

// ts: BinanceSpotExchange.fetchPositions
/// fetch is configured fetchPositionsAndBalance returning its decoded result.
/// Forward its positions value unchanged; preserve nullish-container TypeError.
pub async fn fetch_binance_spot_positions<P, F>(
    fetch: impl FnOnce() -> F,
) -> Result<Option<P>, ExchangeError>
where
    F: Future<Output = Result<FetchedPositions<P>, ExchangeError>>,
{
    match fetch().await? {
        FetchedPositions::Undefined => Err(positions_nullish("undefined")),
        FetchedPositions::Null => Err(positions_nullish("null")),
        FetchedPositions::Present { positions } => Ok(positions),
    }
}

fn account_identity(first: Option<Value>) -> Result<AccountIdentity, ExchangeError> {
    let user_id = match first {
        None => return Err(alias_nullish("undefined")),
        Some(Value::Null) => return Err(alias_nullish("null")),
        Some(Value::Object(mut record)) => record.remove("accountAlias"),
        Some(_) => None,
    };
    Ok(AccountIdentity { user_id })
}

fn type_error(message: impl Into<String>) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
fn iterator_nullish(kind: &str) -> ExchangeError {
    type_error(format!(
        "Cannot destructure property 'Symbol(Symbol.iterator)' of '(intermediate value)' as it is {kind}."
    ))
}
fn alias_nullish(kind: &str) -> ExchangeError {
    type_error(format!(
        "Cannot read properties of {kind} (reading 'accountAlias')"
    ))
}
fn positions_nullish(kind: &str) -> ExchangeError {
    type_error(format!(
        "Cannot destructure property 'positions' of '(intermediate value)' as it is {kind}."
    ))
}
