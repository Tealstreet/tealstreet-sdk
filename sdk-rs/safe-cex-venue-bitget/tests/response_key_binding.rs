//! Actual API response interceptor over the constructed production exchange.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_types::ExchangeName;
use serde_json::json;

#[test]
fn truthy_primitive_response_preserves_source_in_operator_error() {
    for body in [json!(17), json!(true), json!("refused")] {
        let h = harness(ExchangeName::Bitget, options(), |http| {
            http.route("GET", "/api/v2/spot/account/info", 200.0, body.clone());
        });
        let error = h.drive(h.venue().get_account()).unwrap_err();
        assert_eq!(error.name, "TypeError", "response body {body}");
        let shown = body
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| body.to_string());
        assert_eq!(
            error.message,
            format!("Cannot use 'in' operator to search for 'data' in {shown}")
        );
        let calls = h.http.calls("/api/v2/spot/account/info");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].headers["ACCESS-SIGN"].is_string());
    }
}
