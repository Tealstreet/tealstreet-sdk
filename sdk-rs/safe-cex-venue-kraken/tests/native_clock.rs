//! Futures and Spot sign with virtualClock, independently of the host wall clock.
#[allow(dead_code)]
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_types::ExchangeName;
use serde_json::json;
#[test]
fn concrete_futures_and_spot_sign_synced_nonces() {
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let h = support::harness(name.clone(), support::options(), |http| {
            http.ok(
                "GET",
                "/v3/accounts",
                json!({"result":"success","accounts":{}}),
            );
            http.ok(
                "POST",
                "/0/private/Balance",
                json!({"error":[],"result":{}}),
            );
        });
        h.runtime.now.set(123.0);
        let before = VIRTUAL_CLOCK.get_current_time_ms() * 1000.0;
        h.drive(h.venue().validate_account()).unwrap();
        let after = VIRTUAL_CLOCK.get_current_time_ms() * 1000.0 + 999.0;
        let calls = h.http.calls(if name == ExchangeName::Kraken {
            "/v3/accounts"
        } else {
            "/0/private/Balance"
        });
        assert!(!calls.is_empty(), "private request sent");
        for call in calls {
            let nonce: f64 = if name == ExchangeName::Kraken {
                call.headers["Nonce"].as_str().unwrap().parse().unwrap()
            } else {
                call.data
                    .as_str()
                    .unwrap()
                    .split('&')
                    .find_map(|p| p.strip_prefix("nonce="))
                    .unwrap()
                    .parse()
                    .unwrap()
            };
            assert!(
                nonce >= before && nonce <= after,
                "synced {name:?} nonce {nonce} outside {before}..={after}"
            );
        }
        h.venue().dispose().unwrap();
    }
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
}
