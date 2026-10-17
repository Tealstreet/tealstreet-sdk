use crate::portfolios_balances::{PortfolioFailure, PortfolioThrown};
use safe_cex_exchange::{js_runtime::value::Thrown, runtime::ExchangeError};
use std::rc::Rc;

pub(super) fn failure(error: ExchangeError) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_native(error))
}

pub(super) fn from_thrown(error: Thrown) -> PortfolioFailure {
    let mut native = error.to_exchange_error();
    if error.native.is_none() {
        // These imported Axios fields drive source catch policies. The shared
        // generic thrown conversion supplies name/message; retain this host's
        // finite HTTP metadata before entering the native policy bodies.
        let property = |key| {
            error
                .value
                .get(key)
                .ok()
                .filter(|v| !v.is_undefined())
                .and_then(|v| safe_cex_exchange::js_runtime::value::to_json(&v).ok())
        };
        native.response = property("response");
        native.config = property("config");

        native.is_axios_error = error
            .value
            .get("isAxiosError")
            .is_ok_and(|value| value.truthy());
    }
    failure(native)
}

pub(super) fn exchange_error(error: &PortfolioFailure) -> ExchangeError {
    if let Some(native) = error.native() {
        return native.clone();
    }
    if let Some(store) = error.store_mutation() {
        return store.clone().into();
    }
    let value = error.value.as_ref();
    let message = value.and_then(|v| v.get("message")).or(value);
    let mut result = ExchangeError::new(
        safe_cex_num::js_string(message).unwrap_or_else(|_| "Coinbase operation failed".into()),
    );
    if let Some(name) = value
        .and_then(|v| v.get("name"))
        .and_then(serde_json::Value::as_str)
    {
        result.name = name.into();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_http_error_metadata_survives_the_venue_carrier() {
        let mut original = ExchangeError::new("scripted HTTP failure");
        original.response =
            Some(serde_json::json!({"status": 503, "data": {"message":"unavailable"}}));
        original.code = Some(safe_cex_exchange::js_runtime::value::from_json(
            &serde_json::json!("ERR_BAD_RESPONSE"),
        ));
        original.store_subscriber_ids = vec![17, 29];
        let carrier = from_thrown(Thrown::from(original.clone()));
        assert_eq!(carrier.value.as_ref().unwrap()["response"]["status"], 503);
        assert_eq!(exchange_error(&carrier), original);
    }
}
