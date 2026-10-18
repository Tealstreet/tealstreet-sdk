//! Account lookup and validation over the existing session/transport seams.

use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, from_retained_input, to_typed},
    runtime::ExchangeError,
};
use safe_cex_num::{
    js_truthy,
    numeric_ingress::{JsNumberInput, js_capture_output},
};
use safe_cex_transport::provider::RestRequest;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future};

pub fn invalidate_session(token: &mut Option<String>, expires_at: &mut f64) {
    *token = None;
    *expires_at = 0.0;
}

pub fn validate_account<S>(
    session: impl FnOnce() -> S,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<Value, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
{
    let pending = session();
    async move {
        if let Err(error) = pending.await {
            let data = error.response.as_ref().and_then(|r| r.get("data"));
            let message = data.and_then(|d| d.get("message"));
            logger(if js_truthy(message) {
                message.cloned().unwrap()
            } else {
                Value::String(error.message.clone())
            })?;
            let msg = data.and_then(|d| d.get("msg"));
            return Ok(
                json!({"error":if js_truthy(msg){msg.cloned().unwrap()}else if !error.message.is_empty(){Value::String(error.message)}else{json!("Invalid API key or secret")}}),
            );
        }
        Ok(json!({"error":""}))
    }
}

pub fn get_account_code<S, R>(
    cache: &RefCell<Option<String>>,
    username: String,
    subaccount: Option<String>,
    session: impl FnOnce() -> S,
    request: impl FnOnce(RestRequest) -> R,
    logger: &dyn Fn(Value) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<String, ExchangeError>>
where
    S: Future<Output = Result<String, ExchangeError>>,
    R: Future<Output = Result<JsNumberInput, ExchangeError>>,
{
    let cached = cache.borrow().clone().filter(|s| !s.is_empty());
    let pending = if cached.is_none() {
        Some(session())
    } else {
        None
    };
    async move {
        let result: Result<String, ExchangeError> = async {
            if let Some(cached) = cached {
                return Ok(cached);
            }
            pending.expect("uncached session").await?;
            let input = request(RestRequest {
                method: "GET".into(),
                url: "/users/{username}".into(),
                params: Some(json!({"username":username})),
                ..RestRequest::default()
            })
            .await?;
            let data = from_retained_input(
                js_capture_output(&input).map_err(|e| ExchangeError::new(e.to_string()))?,
            );
            let accounts = data.get("userDetails")?.get("0")?.get("accounts")?;
            let filter = accounts.get("filter")?;
            if !filter.is_callable() {
                return Err(
                    Thrown::type_error("userDetail.accounts.filter is not a function").into(),
                );
            }
            let valid = filter.call(
                accounts,
                vec![Js::function("filter", |_, args| {
                    Ok(Js::Bool(
                        args[0]
                            .get("accountStatus")?
                            .strict_equals(&Js::str("FULL_TRADING")),
                    ))
                })],
            )?;
            let count = valid.get("length")?.to_number()?;
            let selected = if count == 0.0 {
                return Err(ExchangeError::new("No valid accounts found"));
            } else if count > 1.0 {
                let subaccount = subaccount.filter(|s| !s.is_empty()).ok_or_else(|| {
                    ExchangeError::new("Multiple accounts found. Please specify subaccount number ")
                })?;
                let selected = valid.call_method(
                    "find",
                    vec![Js::function("find", move |_, args| {
                        let account = args[0].get("account")?;
                        let includes = account.get("includes")?;
                        if !includes.is_callable() {
                            return Err(Thrown::type_error(
                                "account.account.includes is not a function",
                            ));
                        }
                        includes.call(account, vec![Js::str(&subaccount)])
                    })],
                )?;
                if !selected.truthy() {
                    return Err(ExchangeError::new("Subaccount for number not found"));
                }
                selected
            } else {
                valid.get("0")?
            };
            let account: String = to_typed(&selected.get("account")?)?;
            *cache.borrow_mut() = Some(account.clone());
            Ok(account)
        }
        .await;
        if let Err(error) = &result {
            let msg = error
                .response
                .as_ref()
                .and_then(|r| r.get("data"))
                .and_then(|d| d.get("msg"));
            logger(if js_truthy(msg) {
                msg.cloned().unwrap()
            } else {
                Value::String(error.message.clone())
            })?;
        }
        result
    }
}
