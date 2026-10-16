//! Account reads through the shared response-bearing REST adapter.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_exchange::{error_reporting::ExchangeErrorInput, exchange::BaseExchange};
use safe_cex_num::js_truthy;
use safe_cex_transport::provider::{RestAdapter, RestRequest};
use safe_cex_types::ExchangeAccount;
use serde_json::Value;

// ts: KucoinExchange.getAccount
pub async fn get_account(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
) -> Result<ExchangeAccount, ExchangeError> {
    read_account(base, xhr, false).await
}
// ts: KucoinSpotExchange.getAccount
pub async fn get_spot_account(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
) -> Result<ExchangeAccount, ExchangeError> {
    read_account(base, xhr, true).await
}
fn empty_account() -> ExchangeAccount {
    ExchangeAccount {
        user_id: String::new(),
        sub_id: Some(None),
        affiliate_id: Some(None),
    }
}
fn emit(
    base: &BaseExchange,
    message: Option<String>,
    inside_try: bool,
) -> Result<(), ExchangeError> {
    let emitted = base.emit_and_log_error(
        message.map_or(ExchangeErrorInput::Missing, ExchangeErrorInput::Message),
        None,
        None,
    );
    match emitted {
        Err(error) if inside_try => {
            let caught = text(
                error
                    .response
                    .as_ref()
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.get("msg")),
            )
            .unwrap_or_else(Some)
            .or(Some(error.message));
            base.emit_and_log_error(
                caught.map_or(ExchangeErrorInput::Missing, ExchangeErrorInput::Message),
                None,
                None,
            )
        }
        result => result,
    }
}

fn text(value: Option<&Value>) -> Result<Option<String>, String> {
    match value.filter(|v| js_truthy(Some(v))) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err("KuCoin account strings must use the shared string representation".into()),
    }
}
async fn read_account(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    spot: bool,
) -> Result<ExchangeAccount, ExchangeError> {
    let url = if spot {
        "/api/v2/user-info"
    } else {
        "/api/v1/user-info"
    };
    // No shared state is latched before the await. Dropping the caller cancels
    // this read; there is no cleanup or follow-up store mutation to release.
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: url.into(),
            ..Default::default()
        })
        .await;
    let mut inside_try = false;
    let result: Result<ExchangeAccount, Option<String>> = match response {
        Err(error) => {
            match error
                .response
                .as_ref()
                .map(|r| decode_body(&r.body))
                .transpose()
            {
                Err(message) => Err(Some(message)),
                Ok(body) => {
                    let message =
                        text(body.as_ref().and_then(|b| b.get("msg"))).unwrap_or_else(Some);
                    Err(message.or(Some(error.message)))
                }
            }
        }
        Ok(response) => match decode_body(&response.body) {
            Err(error) => Err(Some(error)),
            Ok(data) => {
                inside_try =
                    !data.is_null() && data.get("code").and_then(Value::as_str) != Some("200000");
                decode(&data, spot).map_err(Some)
            }
        },
    };
    match result {
        Ok(account) => Ok(account),
        Err(message) => {
            emit(base, message, inside_try)?;
            Ok(empty_account())
        }
    }
}
fn decode(data: &Value, spot: bool) -> Result<ExchangeAccount, String> {
    if data.is_null() {
        return Err("Cannot read properties of null (reading 'code')".into());
    }
    if data.get("code").and_then(Value::as_str) != Some("200000") {
        return Err(text(data.get("msg"))?.unwrap_or_else(|| {
            if spot {
                "Failed to fetch KuCoin Spot account summary"
            } else {
                "Failed to get account info"
            }
            .into()
        }));
    }
    let account = data.get("data");
    if !spot && account.is_none_or(Value::is_null) {
        return Err(format!(
            "Cannot read properties of {} (reading 'userId')",
            if account.is_none() {
                "undefined"
            } else {
                "null"
            }
        ));
    }
    let user = account
        .and_then(|v| v.get("userId"))
        .filter(|v| js_truthy(Some(v)))
        .or_else(|| {
            if spot {
                account.and_then(|v| v.get("uid"))
            } else {
                None
            }
        });
    Ok(ExchangeAccount {
        user_id: text(user)?.unwrap_or_default(),
        sub_id: Some(text(account.and_then(|v| v.get("subUserId")))?),
        affiliate_id: Some(text(account.and_then(|v| v.get("referrerUserId")))?),
    })
}

fn decode_body(body: &safe_cex_transport::RestResponseBody) -> Result<Value, String> {
    crate::response_default::decode(body)
}

// ts: KucoinSpotExchange.validateAccount
pub async fn validate_spot_account<F, Fut, E>(
    base: &BaseExchange,
    xhr: &dyn RestAdapter,
    fetch_balance: F,
) -> Result<safe_cex_types::ValidateAccountResult, ExchangeError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<safe_cex_types::Balance, E>>,
{
    use safe_cex_types::ValidateAccountResult;
    const FALLBACK: &str = "Failed to validate KuCoin Spot account";
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[1].1.into(),
            params: Some(serde_json::json!({"type":"trade"})),
            ..Default::default()
        })
        .await;
    let decoded = match response {
        Ok(response) => decode_body(&response.body),
        Err(error) => {
            let message = error
                .response
                .as_ref()
                .map(|r| decode_body(&r.body))
                .transpose()
                .and_then(|body| text(body.as_ref().and_then(|b| b.get("msg"))))
                .map(|message| {
                    message
                        .or_else(|| (!error.message.is_empty()).then_some(error.message))
                        .unwrap_or_else(|| FALLBACK.into())
                })
                .unwrap_or_else(|message| message);
            Err(message)
        }
    };
    let data = match decoded {
        Ok(Value::Null) => Err("Cannot read properties of null (reading 'code')".into()),
        value => value,
    };
    match data {
        Err(message) => {
            emit(base, Some(message.clone()), false)?;
            Ok(ValidateAccountResult {
                error: message.into(),
                balance: None,
            })
        }
        Ok(data) if data.get("code").and_then(Value::as_str) != Some("200000") => {
            let message = data
                .get("msg")
                .filter(|value| js_truthy(Some(value)))
                .cloned()
                .unwrap_or_else(|| Value::String(FALLBACK.into()));
            Ok(ValidateAccountResult {
                error: safe_cex_types::AccountValidationError::from_value(message),
                balance: None,
            })
        }
        Ok(_) => {
            // Match fetchBalance().catch(() => undefined): validation succeeded,
            // and an independently failed balance read does not invalidate it.
            let balance = fetch_balance().await.ok();
            Ok(ValidateAccountResult {
                error: String::new().into(),
                balance,
            })
        }
    }
}
