//! Account identity from the supplied parsed REST read.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use safe_cex_types::ExchangeAccount;
use serde_json::Value;
use std::future::Future;

pub async fn get_account<E, F>(read: impl FnOnce(&'static str) -> F) -> Result<ExchangeAccount, E>
where
    E: From<JsCoercionError> + From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let url = crate::protocol::ENDPOINTS
        .iter()
        .find(|entry| entry.name == "ACCOUNT_SPOT")
        .ok_or_else(|| ExchangeError::new("Bitget protocol table is missing ACCOUNT_SPOT"))?
        .path;
    let response = read(url).await?;
    let body = response
        .as_ref()
        .filter(|value| !value.is_null())
        .ok_or(JsCoercionError {
            name: "TypeError",
            message: if response.is_none() {
                "Cannot read properties of undefined (reading 'data')"
            } else {
                "Cannot read properties of null (reading 'data')"
            },
        })?;
    let data = body.get("data");
    let data = data
        .filter(|value| !value.is_null())
        .ok_or(JsCoercionError {
            name: "TypeError",
            message: if data.is_none() {
                "Cannot read properties of undefined (reading 'parentId')"
            } else {
                "Cannot read properties of null (reading 'parentId')"
            },
        })?;
    let parent = data.get("parentId");
    let user = data.get("userId");
    let user_id = js_string(if js_truthy(parent) { parent } else { user })?;
    let sub_id = if js_truthy(parent) {
        Some(js_string(user)?)
    } else {
        None
    };
    let inviter = data.get("inviterId");
    let affiliate_id = if js_truthy(inviter) {
        Some(js_string(inviter)?)
    } else {
        None
    };
    Ok(ExchangeAccount {
        user_id,
        sub_id: Some(sub_id),
        affiliate_id: Some(affiliate_id),
    })
}
