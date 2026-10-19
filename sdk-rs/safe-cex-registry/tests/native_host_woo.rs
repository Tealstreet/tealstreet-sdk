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

#[test]
fn woo_constructed_funding_timer_drains_fires_and_cancels_on_replay_clock() {
    use safe_cex_exchange::{
        native::{native_replay_runtime, replay_raw_tasks},
        replay_clock,
    };
    run_local(async {
        let runtime = native_replay_runtime(1_800_000_000_000.0);
        let h = venue_harness(
            safe_cex_types::ExchangeName::Woo,
            Rc::new(|r| match r.path.as_str() {
                "/v1/public/info" => Some(json!({"success":true,"rows":[]})),
                "/v1/public/futures" => Some(json!({"success":true,"rows":[]})),
                "/v1/public/funding_rates" => Some(json!({"success":true,"rows":[]})),
                _ => None,
            }),
            json!({"key":"","secret":"","publicDataOnly":true}),
            None,
            runtime.clone(),
        )
        .await;
        h.exchange.start().await.unwrap();
        eventually("WOO socket", || !h.peers.borrow().is_empty()).await;
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        let receipt = replay_raw_tasks(&runtime);
        assert_eq!(
            receipt["unknown"], 0,
            "WOO constructed startup timer must use core ownership: {receipt}"
        );
        assert_eq!(receipt["blocking"], 0, "WOO startup drain: {receipt}");
        replay_clock::activate(runtime.replay_clock.as_ref().unwrap()).unwrap();
        replay_clock::advance(runtime.replay_clock.as_ref().unwrap(), 60_000.0)
            .await
            .unwrap();
        eventually("WOO funding timer callback", || {
            h.seen
                .borrow()
                .iter()
                .any(|r| r.path == "/v1/public/funding_rates")
        })
        .await;
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        h.exchange.dispose().unwrap();
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        let before = h.seen.borrow().len();
        replay_clock::advance(runtime.replay_clock.as_ref().unwrap(), 120_000.0)
            .await
            .unwrap();
        for _ in 0..32 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            h.seen.borrow().len(),
            before,
            "disposed funding interval never runs"
        );
        assert_eq!(replay_raw_tasks(&runtime)["blocking"], 0);
    });
}
