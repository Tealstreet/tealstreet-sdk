//! AsterDEX constructed through the registry and production native HTTP host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;
#[test]
fn asterdex_registry_reads_signed_account_over_native_http() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let h = support::venue_harness(
            ExchangeName::Asterdex,
            Rc::new(|request| match request.path.as_str() {
                "/fapi/v1/time" => Some(json!({"serverTime":1700000000000u64})),
                "/fapi/v4/account" => Some(json!({"accountAlias":"aster-native"})),
                _ => None,
            }),
            json!({"key":"native-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(
            h.exchange.get_account().await.unwrap().user_id,
            "aster-native"
        );
        let requests = h.seen.borrow();
        let account = requests
            .iter()
            .find(|r| r.path == "/fapi/v4/account")
            .unwrap();
        assert_eq!(
            account.headers.get("x-mbx-apikey").map(String::as_str),
            Some("native-key")
        );
        let (query, signature) = account.query.rsplit_once("&signature=").unwrap();
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        mac.update(query.as_bytes());
        let expected = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            signature, expected,
            "server verifies the actual signed wire query"
        );
        h.exchange.dispose().unwrap();
    });
}
