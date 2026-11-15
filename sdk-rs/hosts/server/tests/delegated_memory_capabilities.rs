#[test]
fn signing_capabilities_describe_delegated_memory_only_keys() {
    let capabilities = safe_cex_exchange_server::http::capabilities_json();
    assert_eq!(
        capabilities["transports"]["accountWorkerWebSocket"]["privateSigning"],
        "browser remote signer for COSIGNED; keys enter only in DELEGATED, memory-only"
    );
}
