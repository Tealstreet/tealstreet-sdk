//! WOO through the registry's production native host over local HTTP.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use hmac::{Hmac, Mac};
use serde_json::json;
use std::rc::Rc;
use support::*;

#[test]
fn woo_constructs_through_native_registry_and_signs_its_account_read() {
    run_local(async {
        let h=venue_harness(safe_cex_types::ExchangeName::Woo,Rc::new(|request| {
            if request.path!="/v3/accountinfo" {return None;}
            assert_eq!(request.method,"GET");
            assert_eq!(request.headers.get("x-api-key").map(String::as_str),Some("synthetic-woo-key"));
            let timestamp=request.headers.get("x-api-timestamp").expect("native timestamp header");
            assert!(timestamp.parse::<u64>().unwrap()>0);
            let mut mac=Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
            mac.update(format!("{timestamp}GET/v3/accountinfo").as_bytes());
            let expected:String=mac.finalize().into_bytes().iter().map(|b|format!("{b:02x}")).collect();
            assert_eq!(request.headers.get("x-api-signature"),Some(&expected),"HMAC verified over the exact native request preimage");
            Some(json!({"success":true,"data":{"applicationId":"synthetic-app","referrerID":"synthetic-referrer","accountMode":"FUTURES","positionMode":"ONE_WAY"},"timestamp":1}))
        }),json!({"key":"synthetic-woo-key","secret":SECRET}),None,safe_cex_exchange::native::native_runtime()).await;
        assert_eq!(h.exchange.name(), &safe_cex_types::ExchangeName::Woo);
        assert_eq!(h.exchange.class_name(), "WooExchange");
        let account = h.exchange.get_account().await.unwrap();
        assert_eq!(account.user_id, "synthetic-app");
        assert_eq!(
            account.affiliate_id,
            Some(Some("synthetic-referrer".into()))
        );
        assert_eq!(
            h.seen
                .borrow()
                .iter()
                .filter(|request| request.path == "/v3/accountinfo")
                .count(),
            1
        );
        h.exchange.dispose().unwrap();
    });
}
