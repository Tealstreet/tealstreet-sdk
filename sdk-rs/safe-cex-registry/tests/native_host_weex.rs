//! WEEX through the production registry host, including its required shared limiter.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::rc::Rc;
use support::*;
#[test]
fn weex_constructs_through_native_registry_and_signs_its_account_read() {
    run_local(async {
        let h = venue_harness(
            safe_cex_types::ExchangeName::Weex,
            Rc::new(|request| {
                if ![
                    "/capi/v3/account/accountConfig",
                    "/capi/v3/account/balance",
                    "/capi/v3/account/position/allPosition",
                ]
                .contains(&request.path.as_str())
                {
                    return None;
                }
                assert_eq!(request.method, "GET");
                assert_eq!(
                    request.headers.get("access-key").map(String::as_str),
                    Some("synthetic-weex-key")
                );
                assert_eq!(
                    request.headers.get("access-passphrase").map(String::as_str),
                    Some("synthetic-passphrase")
                );
                let timestamp = request.headers.get("access-timestamp").expect("timestamp");
                assert!(timestamp.parse::<u64>().unwrap() > 0);
                let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
                mac.update(format!("{timestamp}GET{}", request.path).as_bytes());
                use base64::Engine;
                let signature =
                    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
                assert_eq!(
                    request.headers.get("access-sign"),
                    Some(&signature),
                    "exact native wire preimage"
                );
                Some(if request.path.ends_with("accountConfig") {
                    json!({"canTrade":true})
                } else {
                    json!([])
                })
            }),
            json!({"key":"synthetic-weex-key","secret":SECRET,"passphrase":"synthetic-passphrase"}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(h.exchange.class_name(), "WeexExchange");
        assert_eq!(h.exchange.base().get_all_sockets().len(), 2);
        let account = h.exchange.get_account().await.unwrap();
        let expected: String = Sha256::digest(b"synthetic-weex-key")
            .iter()
            .take(12)
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(account.user_id, format!("weex:{expected}"));
        assert_eq!(
            h.seen
                .borrow()
                .iter()
                .filter(|request| request.path.ends_with("accountConfig"))
                .count(),
            1
        );
        h.exchange.dispose().unwrap();
    });
}
