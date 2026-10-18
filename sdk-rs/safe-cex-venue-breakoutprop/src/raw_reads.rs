//! Raw REST read ordering and source error selection, over injected host hooks.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_truthy, numeric_ingress::JsNumberInput};
use safe_cex_transport::provider::RestRequest;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};
use std::future::Future;

fn project(input: JsNumberInput, key: &str) -> Result<JsNumberInput, ExchangeError> {
    let raw: Input = input
        .decode()
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    if raw.is_nullish() {
        return Err(ExchangeError::new(format!(
            "Cannot read properties of {} (reading '{key}')",
            if raw.is_null() { "null" } else { "undefined" }
        )));
    }
    let rows = match raw.get(key) {
        Some(Input::Array(rows)) => rows,
        None | Some(Input::Undefined) => {
            return Err(ExchangeError::new(
                "Cannot read properties of undefined (reading 'filter')",
            ));
        }
        Some(Input::Null) => {
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
        if row.is_nullish() {
            return Err(ExchangeError::new(format!(
                "Cannot read properties of {} (reading 'symbol')",
                if row.is_null() { "null" } else { "undefined" }
            )));
        }
        if !row
            .get("symbol")
            .is_some_and(|value| value.text_equals("XBTUSD"))
        {
            filtered.push(row.clone());
        }
    }
    Ok(Input::Object(vec![(key.encode_utf16().collect(), Input::Array(filtered))]).into())
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
                let response_view = error.response_json_view();
                let message = response_view
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
