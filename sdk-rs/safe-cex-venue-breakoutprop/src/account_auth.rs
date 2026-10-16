//! Account lookup and validation over the existing session/transport seams.
use crate::types::UserResponse;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_truthy, numeric_ingress::JsNumberInput};
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
            // CHOSEN shared declared wire shape rather than a parallel dynamic user graph.
            let data: UserResponse = input
                .decode()
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            let user = data.user_details.first().ok_or_else(|| {
                let mut e =
                    ExchangeError::new("Cannot read properties of undefined (reading 'accounts')");
                e.name = "TypeError".into();
                e
            })?;
            let valid: Vec<_> = user
                .accounts
                .iter()
                .filter(|a| a.account_status == "FULL_TRADING")
                .collect();
            let selected = if valid.is_empty() {
                return Err(ExchangeError::new("No valid accounts found"));
            } else if valid.len() > 1 {
                let subaccount = subaccount.filter(|s| !s.is_empty()).ok_or_else(|| {
                    ExchangeError::new("Multiple accounts found. Please specify subaccount number ")
                })?;
                valid
                    .iter()
                    .find(|a| a.account.contains(&subaccount))
                    .copied()
                    .ok_or_else(|| ExchangeError::new("Subaccount for number not found"))?
            } else {
                valid[0]
            };
            *cache.borrow_mut() = Some(selected.account.clone());
            Ok(selected.account.clone())
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
