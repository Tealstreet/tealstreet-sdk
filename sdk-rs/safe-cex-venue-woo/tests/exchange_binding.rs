//! The production WooExchange driven end to end over a scripted transport.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)] // Shared support: each test crate uses a subset.
mod binding_harness;
use binding_harness::{harness, options};
use safe_cex_types::ExchangeName;
use serde_json::json;

#[test]
fn constructs_from_real_options() {
    let h = harness(ExchangeName::Woo, options(), |_| {});
    assert_eq!(*h.venue().base().name(), ExchangeName::Woo);
}

#[test]
fn get_account_reads_account_info() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok(
            "GET",
            "/v3/accountinfo",
            json!({"applicationId": "app-1", "referrerID": "ref-9", "accountMode": "FUTURES", "positionMode": "ONE_WAY"}),
        );
    });
    let account = h.drive(h.venue().get_account()).unwrap();
    assert_eq!(
        serde_json::to_value(account).unwrap(),
        json!({"userId": "app-1", "affiliateId": "ref-9"})
    );
}
