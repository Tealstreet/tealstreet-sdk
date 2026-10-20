mod support;
use safe_cex_engine::Event;
use safe_cex_venue_bybit::Options;
use support::*;

#[test]
fn ticker_failure_cannot_block_authenticated_account_snapshot() {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    });
    h.respond("/v5/account/fee-rate", 200, FEE_RATES);
    h.respond("/v5/account/info", 200, ACCOUNT_INFO);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    let mut body: serde_json::Value = serde_json::from_str(INSTRUMENTS).unwrap();
    body["result"]["list"]
        .as_array_mut()
        .unwrap()
        .insert(0, serde_json::json!({"broken":true}));
    h.respond("/v5/market/instruments-info", 200, &body.to_string());
    assert!(h.engine.store().loaded().markets);
    assert!(!h.engine.store().loaded().tickers);
    assert!(h.has_request("/v5/order/realtime"));
    assert!(h.has_request("/v5/position/list"));
    assert!(h.has_request("/v5/account/wallet-balance"));
    assert!(
        h.logs
            .iter()
            .any(|line| line.contains("Skipped malformed public discovery row"))
    );
    let ticker = h.take("/v5/market/tickers");
    h.event(Event::HttpFailed {
        request: ticker.id,
        error: "scripted outage",
    });
    assert!(h.has_request("/v5/account/wallet-balance"));
    h.advance(1000.0);
    assert!(h.has_request("/v5/market/tickers"));
}

#[test]
fn catalogue_failure_uses_shared_exponential_backoff() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    for delay in [1000.0, 2000.0, 4000.0] {
        let read = h.take("/v5/market/instruments-info");
        h.event(Event::HttpFailed {
            request: read.id,
            error: "scripted outage",
        });
        assert!(!h.has_request("/v5/market/instruments-info"));
        h.advance(delay);
        assert!(h.has_request("/v5/market/instruments-info"));
    }
    h.respond("/v5/market/instruments-info", 200, INSTRUMENTS);
    assert!(h.engine.store().loaded().markets);
}
