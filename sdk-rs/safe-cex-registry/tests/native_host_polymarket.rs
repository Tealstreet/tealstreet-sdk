//! Registry constructor and production reqwest host, using only local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;

#[test]
fn polymarket_registry_derives_credentials_and_reads_signed_balance() {
    support::run_local(async {
        let h = support::venue_harness(
            ExchangeName::Polymarket,
            Rc::new(|request| match request.path.as_str() {
                "/time" => Some(json!(1700000000)),
                "/auth/derive-api-key" => Some(json!({
                    "apiKey":"local-account-key", "secret":"c2VjcmV0",
                    "passphrase":"local-account-passphrase"
                })),
                "/balance-allowance" => {
                    Some(json!({"balance":"123456789", "allowance":"999999999"}))
                }
                _ => None,
            }),
            json!({"key":format!("0x{}", "01".repeat(32)), "secret":"unused"}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        let validation = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            h.exchange.validate_account(),
        )
        .await
        .unwrap_or_else(|_| {
            panic!(
                "native validation timed out; requests={:?}",
                h.seen.borrow()
            )
        })
        .unwrap();
        assert_eq!(validation.error, "".into(), "native account validation");
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
        );
        let seen = h.seen.borrow();
        let l1 = seen
            .iter()
            .find(|r| r.path == "/auth/derive-api-key")
            .unwrap();
        assert_eq!(l1.method, "GET");
        assert_eq!(
            l1.headers["poly_address"],
            "0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1"
        );
        assert_eq!(l1.headers["poly_timestamp"], "1700000000");
        assert_eq!(l1.headers["poly_nonce"], "0");
        assert_eq!(l1.headers["poly_signature"].len(), 132);
        let balance = seen
            .iter()
            .find(|r| r.path == "/balance-allowance")
            .unwrap();
        assert_eq!(balance.method, "GET");
        assert_eq!(balance.headers["poly_api_key"], "local-account-key");
        assert_eq!(
            balance.headers["poly_passphrase"],
            "local-account-passphrase"
        );
        assert_eq!(balance.headers["poly_signature"].len(), 44);
        assert!(balance.query.contains("asset_type=COLLATERAL"));
        assert!(balance.query.contains("signature_type=0"));
        h.exchange.base().dispose().unwrap();
    });
}
