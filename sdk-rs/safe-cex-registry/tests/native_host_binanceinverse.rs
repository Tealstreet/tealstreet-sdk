//! COIN-M through the real registry constructor, native HTTP and socket slots.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use serde_json::json;
use std::rc::Rc;
use support::*;

#[test]
fn inverse_registry_constructs_real_binding_and_signs_account_read() {
    run_local(async {
        let name = serde_json::from_value(json!("binanceinverse")).unwrap();
        let h = venue_harness(name, Rc::new(|request| {
            if request.path != "/dapi/v1/account" { return None; }
            assert_eq!(request.method, "GET");
            assert_eq!(request.headers.get("x-mbx-apikey").map(String::as_str), Some("native-inverse-key"));
            let (payload, signature) = request.query.rsplit_once("&signature=").expect("signed native query");
            let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
            mac.update(payload.as_bytes());
            let expected: String = mac.finalize().into_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
            assert_eq!(signature, expected, "actual query bytes are signed exactly once");
            assert!(payload.contains("timestamp="));
            assert!(payload.contains("recvWindow=20000"));
            Some(json!({"assets":[{"asset":"USD","walletBalance":"100","availableBalance":"75","unrealizedProfit":"3"}]}))
        }), json!({"key":"native-inverse-key","secret":SECRET}), None, safe_cex_exchange::native::native_runtime()).await;
        assert_eq!(h.exchange.class_name(), "BinanceInverseExchange");
        use safe_cex_exchange::socket_lifecycle::SocketSlot;
        assert!(h.exchange.base().get_socket(SocketSlot::Public).is_some());
        assert!(h.exchange.base().get_socket(SocketSlot::Private).is_some());
        assert!(
            h.seen.borrow().is_empty(),
            "construction does not send venue requests"
        );
        let validation = h.exchange.validate_account().await.unwrap();
        assert_eq!(
            validation.error,
            safe_cex_types::AccountValidationError::from("")
        );
        let balance = validation.balance.expect("source USD aggregation");
        assert_eq!(balance.total, 100.0);
        assert_eq!(balance.free, 75.0);
        assert_eq!(balance.used, 25.0);
        assert_eq!(balance.upnl, 3.0);
        assert_eq!(h.seen.borrow().len(), 1);
        h.exchange.dispose().unwrap();
        assert!(h.exchange.base().is_disposed());
    });
}
