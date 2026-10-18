//! Actual constructed venue → signed Axios client → terminal transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod binding_harness;
use binding_harness::{harness, options};
use safe_cex_types::ExchangeName;
use serde_json::json;

#[test]
fn nonfinite_leverage_reaches_the_v1_signer_before_form_encoding() {
    const PATH: &str = "/v1/client/futures_leverage";
    for (number, spelling) in [(f64::NAN, "NaN"), (f64::INFINITY, "Infinity")] {
        let h = harness(ExchangeName::Woo, options(), |http| {
            http.ok("POST", PATH, json!({"success": true}));
        });
        let result = h.drive(h.venue().set_leverage("BTCUSDT".into(), number, None));
        let calls = h.http.calls(PATH);
        assert_eq!(
            calls.len(),
            1,
            "source sends the native {spelling} through the signer; result={result:?}; reports={:?}",
            h.runtime.reports.borrow()
        );
        assert_eq!(
            calls[0].data.as_str(),
            Some(
                format!(
                    "leverage={spelling}&margin_mode=CROSS&position_side=BOTH&symbol=PERP_BTC_USDT"
                )
                .as_str()
            ),
            "signer must retain the Number until qs form encoding"
        );
    }
}
