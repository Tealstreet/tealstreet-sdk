//! Account-read methods from paradex.exchange.ts. Callbacks bind the existing
//! xhr.get, template-literal error coercion and log capabilities; API/auth
//! composition remains a separate pending row.
use crate::{constants::ENDPOINTS, types::AccountResponse};
use safe_cex_types::{ExchangeAccount, PositionMode};
use std::future::Future;

// ts: ParadexExchange.getPositionMode
pub async fn get_position_mode() -> PositionMode {
    PositionMode::OneWay
}

// ts: ParadexExchange.getAccount
pub async fn get_account<E, F: Future<Output = Result<AccountResponse, E>>>(
    get: impl FnOnce(&'static str) -> Result<F, E>,
    stringify_error: impl FnOnce(&E) -> Result<String, E>,
    log: impl FnOnce(String, &'static str) -> Result<(), E>,
) -> Result<ExchangeAccount, E> {
    let endpoint = ENDPOINTS
        .iter()
        .find(|(key, _)| *key == "ACCOUNT")
        .unwrap()
        .1;
    // Both a synchronous xhr.get throw and a rejected promise enter the catch.
    let result = match get(endpoint) {
        Ok(pending) => pending.await,
        Err(error) => Err(error),
    };
    match result {
        Ok(data) => Ok(ExchangeAccount {
            user_id: data.account_number,
            affiliate_id: Some(None),
            sub_id: None,
        }),
        Err(error) => {
            // A coercion or logger failure replaces the caught error in source.
            let text = stringify_error(&error)?;
            log(format!("Error fetching account: {text}"), "error")?;
            Err(error)
        }
    }
}
