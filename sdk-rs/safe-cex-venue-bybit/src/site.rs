//! Bybit Brazil site routing; compatible variants deliberately do not opt in.
use serde_json::{Value, json};
pub const BYBIT_BRAZIL_SITE_ID: &str = "BRA_BTL";
pub type BybitSiteId = safe_cex_types::ExchangeOptionsBybitSiteId;
pub fn is_bybit_site_id_exchange(exchange: &str) -> bool {
    matches!(exchange, "bybit" | "bybitinverse" | "bybitspot")
}
pub fn normalize_bybit_site_id(exchange: &str, site_id: &Value) -> Option<BybitSiteId> {
    (is_bybit_site_id_exchange(exchange) && site_id.as_str() == Some(BYBIT_BRAZIL_SITE_ID))
        .then_some(BybitSiteId::BraBtl)
}
pub fn get_bybit_site_headers(options: &Value) -> Value {
    if options["bybitSiteId"].as_str() == Some(BYBIT_BRAZIL_SITE_ID) {
        json!({"x-site-id": BYBIT_BRAZIL_SITE_ID})
    } else {
        json!({})
    }
}
pub fn get_bybit_web_socket_options(options: &Value) -> Option<Value> {
    let headers = get_bybit_site_headers(options);
    if headers
        .as_object()
        .is_some_and(|headers| !headers.is_empty())
    {
        Some(json!({"headers": headers}))
    } else {
        None
    }
}
