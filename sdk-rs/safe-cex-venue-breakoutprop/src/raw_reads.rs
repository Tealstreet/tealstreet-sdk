//! Raw REST read ordering and source error selection, over injected host hooks.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_number_output, js_truthy,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough, js_from_value},
};
use safe_cex_transport::provider::RestRequest;
use serde::Serialize;
use serde_json::{Value, json};
use std::future::Future;

#[derive(Serialize)]
#[serde(transparent)]
struct Raw {
    value: Value,
}
impl JsNumberOutput for Raw {
    fn check_number_output(&self) {
        js_number_output!(self,struct {} passthrough {value});
    }
}
fn project(input: JsNumberInput, key: &str) -> Result<JsNumberInput, ExchangeError> {
    let raw = input
        .decode_with(|d| js_deserialize_passthrough(d).map(|value| Raw { value }))
        .map_err(|e| ExchangeError::new(e.to_string()))?
        .value;
    if raw.is_null() {
        return Err(ExchangeError::new(format!(
            "Cannot read properties of null (reading '{key}')"
        )));
    }
    let field = raw.get(key);
    let rows = match field {
        Some(Value::Array(rows)) => rows,
        None => {
            return Err(ExchangeError::new(
                "Cannot read properties of undefined (reading 'filter')",
            ));
        }
        Some(Value::Null) => {
            return Err(ExchangeError::new(
                "Cannot read properties of null (reading 'filter')",
            ));
        }
        _ => {
            return Err(ExchangeError::new(format!(
                "data.{key}.filter is not a function"
            )));
        }
    };
    let mut filtered = Vec::new();
    for row in rows {
        if row.is_null() {
            return Err(ExchangeError::new(
                "Cannot read properties of null (reading 'symbol')",
            ));
        }
        if row.get("symbol").and_then(Value::as_str) != Some("XBTUSD") {
            filtered.push(row.clone());
        }
    }
    js_from_value(json!({key:filtered})).map_err(|e| ExchangeError::new(e.to_string()))
}
fn read<S, A, R>(
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    report: &dyn Fn(Value) -> Result<(), ExchangeError>,
    endpoint: &'static str,
    filter: Option<&'static str>,
) -> impl Future<Output = Result<Option<JsNumberInput>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let pending = session();
    async move {
        let result = async {
            pending.await?;
            let account_code = account().await?;
            let input = request(RestRequest {
                method: "GET".into(),
                url: endpoint.into(),
                params: Some(json!({"accountCode":account_code})),
                ..RestRequest::default()
            })
            .await?;
            if let Some(key) = filter {
                project(input, key)
            } else {
                Ok(input)
            }
        }
        .await;
        match result {
            Ok(data) => Ok(Some(data)),
            Err(error) => {
                let message = error
                    .response
                    .as_ref()
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.get("msg"));
                let selected = if js_truthy(message) {
                    message.cloned().unwrap()
                } else {
                    Value::String(error.message.clone())
                };
                report(selected)?;
                Ok(None)
            }
        }
    }
}

pub fn fetch_instruments_raw<S, A, R>(
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    report: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Option<JsNumberInput>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    read(
        session,
        account,
        request,
        report,
        "instruments/query",
        Some("instruments"),
    )
}
pub fn fetch_instrument_details_raw<S, A, R>(
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    report: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Option<JsNumberInput>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    read(
        session,
        account,
        request,
        report,
        "accounts/{accountCode}/instruments/query",
        Some("instrumentDetails"),
    )
}
pub fn fetch_portfolio_raw<S, A, R>(
    session: impl FnOnce() -> S,
    account: impl FnOnce() -> A,
    request: impl FnOnce(RestRequest) -> R,
    report: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Option<JsNumberInput>, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    A: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    read(
        session,
        account,
        request,
        report,
        "accounts/{accountCode}/portfolio",
        None,
    )
}
