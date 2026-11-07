//! Live gateway chart history supports GET, and omits the JSON-RPC response ID.
use safe_cex_engine::{Action, Method};
use safe_cex_venue_coinbase::drb::Transport;
use serde_json::json;

// Reuse the foundation's transport probe without re-running its test module.
include!("support/drb_transport_probe.rs");

#[test]
fn chart_history_uses_get_and_correlates_its_identifierless_http_response() {
    let mut transport = Transport::default();
    let mut id = 0;
    let actions = run(1000., |cx| {
        let context = chart_context(cx);
        id = transport.http(cx, &context, "public/get_tradingview_chart_data", json!({"instrument_name":"BTC_USDC-PERPETUAL", "resolution":"1", "start_timestamp":1794009600000u64,"end_timestamp":1794013200000u64}), None, ()).unwrap().0;
    });
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert_eq!(request.method, Method::Get);
    assert!(request.body.is_none());
    assert!(request.url.contains("resolution=1"));
    assert!(request.url.contains("start_timestamp=1794009600000"));
    assert!(request.headers.iter().all(|(h, _)| *h != "Authorization"));
    let body = r#"{"jsonrpc":"2.0","result":{"status":"no_data"}}"#;
    assert_eq!(
        transport.response_at(id, 200, body, 1001.).unwrap(),
        json!({"status":"no_data"})
    );
    assert!(transport.response_at(id, 200, body, 1002.).is_err());
}

#[test]
fn chart_get_still_rejects_an_explicit_wrong_id() {
    let mut transport = Transport::default();
    let mut id = 0;
    run(1000., |cx| {
        let context = chart_context(cx);
        id = transport
            .http(
                cx,
                &context,
                "public/get_tradingview_chart_data",
                json!({}),
                None,
                (),
            )
            .unwrap()
            .0;
    });
    assert!(
        transport
            .response_at(id, 200, r#"{"jsonrpc":"2.0","id":999,"result":{}}"#, 1001.)
            .is_err()
    );
}
