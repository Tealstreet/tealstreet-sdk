//! Synced API and private-upgrade clocks through the production WEEX binding.
#[path = "support/binding_runtime.rs"]
#[allow(dead_code)]
mod binding_runtime;
use base64::Engine;
use binding_runtime::*;
use hmac::{Hmac, Mac};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::relay::UpgradeHeaders;
use serde_json::json;
struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
fn verify(timestamp: &str, signature: &str, path: &str) -> f64 {
    let mut mac = Hmac::<sha2::Sha256>::new_from_slice(b"synthetic-secret").unwrap();
    mac.update(format!("{timestamp}{path}").as_bytes());
    assert_eq!(
        signature,
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
    );
    timestamp.parse().unwrap()
}
#[test]
fn rest_and_private_reopen_read_synced_virtual_time() {
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 90_000.0);
    let h = harness(options(), |http| {
        http.ok(
            "GET",
            "/capi/v3/account/accountConfig",
            json!({"canTrade":true}),
        );
        http.ok("GET", "/capi/v3/account/balance", json!([]));
        http.ok("GET", "/capi/v3/account/position/allPosition", json!([]));
        for name in ["OPEN_ORDERS", "OPEN_ALGO_ORDERS", "SYMBOL_CONFIG"] {
            let path = safe_cex_venue_weex::types::ENDPOINTS
                .iter()
                .find(|(key, _)| *key == name)
                .unwrap()
                .1;
            http.ok("GET", path, json!([]));
        }
    });
    h.runtime.now.set((VIRTUAL_CLOCK.now)());
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    assert!(
        h.drive(h.venue().get_account())
            .unwrap()
            .user_id
            .starts_with("weex:")
    );
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let requests = h.http.calls("/capi/v3/account/accountConfig");
    assert_eq!(requests.len(), 1);
    let stamp = verify(
        &requests[0].headers["ACCESS-TIMESTAMP"],
        &requests[0].headers["ACCESS-SIGN"],
        "GET/capi/v3/account/accountConfig",
    );
    let mut checks = vec![("REST", stamp, before, after)];
    let socket = h.venue().base().get_private_sockets()[0].clone();
    for (i, offset) in [90_000.0, 180_000.0].into_iter().enumerate() {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + offset);
        let before = VIRTUAL_CLOCK.get_current_time_ms();
        socket.connect_and_subscribe(i > 0).unwrap();
        h.runtime.pump();
        let after = VIRTUAL_CLOCK.get_current_time_ms();
        let dials = h.runtime.dials.borrow();
        let options = &dials.last().unwrap().1;
        assert!(options.headers_required);
        let Some(UpgradeHeaders::Static(headers)) = &options.headers else {
            panic!("upgrade headers");
        };
        let timestamp = verify(
            &headers["ACCESS-TIMESTAMP"],
            &headers["ACCESS-SIGN"],
            "/v3/ws/private",
        );
        checks.push(("WS", timestamp, before, after));
    }
    h.venue().dispose().unwrap();
    assert!(
        checks
            .iter()
            .all(|(_, stamp, before, after)| stamp >= before && stamp <= after),
        "all REST/WS stamps must use their synced intervals: {checks:?}"
    );
}
