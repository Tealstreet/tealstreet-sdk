use safe_cex_transport::{DirectTransport, TransportProvider};

#[test]
fn direct_ws_connection_returns_no_capability_for_distinct_urls() {
    for url in [
        "wss://native-one.invalid/ws",
        "https://native-two.invalid/path?key=2",
    ] {
        assert!(
            DirectTransport.ws_connection(url).is_none(),
            "direct source callback must return no socket capability"
        );
    }
}
