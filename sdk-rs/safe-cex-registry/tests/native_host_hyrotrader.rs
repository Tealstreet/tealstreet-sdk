//! HyroTrader goes through the actual Rust registry and native HTTP signer.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::{native_host, rust_venues::rust_constructor};
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::rc::Rc;

#[test]
fn hyrotrader_native_registry_preserves_configuration_and_signed_reads() {
    let name = ExchangeName::Hyrotrader;
    let host = native_host::native_venue_host(native_host::hyperliquid_rate_limiter());
    let constructor =
        rust_constructor(&name, host).expect("live HyroTrader has a Rust registry constructor");
    if [
        &*safe_cex_venue_bybit::types::HYROTRADER_PROD_URL,
        &*safe_cex_venue_bybit::types::HYROTRADER_SIMULATOR_URL,
    ]
    .iter()
    .all(|urls| urls["livenet"].as_str().is_none_or(str::is_empty))
    {
        let result = constructor.construct(
            name,
            serde_json::from_value(json!({"key":"native-bybit-key","secret":support::SECRET}))
                .unwrap(),
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        );
        let error = match result {
            Ok(_) => panic!("unconfigured HyroTrader must refuse before IO"),
            Err(error) => error,
        };
        assert_eq!(
            error.message,
            "HyroTrader production REST URL is not configured. Set NEXT_PUBLIC_HYROTRADER_PROD_REST_URL, or NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL while v1 is simulator-backed."
        );
        return;
    }
    support::run_local(async {
        let h = support::venue_harness(
            name.clone(),
            Rc::new(|request| {
                (request.path == "/v5/order/realtime").then(
                    || json!({"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}),
                )
            }),
            json!({"key":"native-bybit-key","secret":support::SECRET}),
            None,
            safe_cex_exchange::native::native_runtime(),
        )
        .await;
        assert_eq!(h.exchange.class_name(), "HyrotraderExchange");
        assert!(!h.exchange.base().is_demo);
        let orders = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            h.exchange.fetch_orders(None),
        )
        .await
        .expect("native Hyro orders settle")
        .unwrap();
        assert!(orders.is_empty());
        let seen = h.seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].path, "/v5/order/realtime");
        assert!(seen[0].query.contains("category=linear"));
        use hmac::Mac;
        let mut mac =
            hmac::Hmac::<sha2::Sha256>::new_from_slice(support::SECRET.as_bytes()).unwrap();
        let preimage = format!(
            "{}{}{}{}",
            seen[0].headers["x-bapi-timestamp"],
            seen[0].headers["x-bapi-api-key"],
            seen[0].headers["x-bapi-recv-window"],
            seen[0].query
        );
        mac.update(preimage.as_bytes());
        let signature: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(seen[0].headers["x-bapi-sign"], signature);
        drop(seen);
        h.exchange.dispose().unwrap();
    });
}
