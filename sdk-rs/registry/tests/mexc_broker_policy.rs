#[test]
fn mexc_broker_header_protection_is_scoped_to_its_native_family() {
    let policy = safe_cex_venues::remote_signing_policy("mexc");
    let expected: &[&str] = if cfg!(feature = "mexc") {
        &[
            "ApiKey",
            "Request-Time",
            "Signature",
            "Recv-Window",
            "Content-Type",
            "source",
        ]
    } else {
        &[
            "ApiKey",
            "Request-Time",
            "Signature",
            "Recv-Window",
            "Content-Type",
        ]
    };
    assert!(policy.enabled && policy.payload_supported && policy.payload_rest_only);
    assert_eq!(policy.protected_headers, Some(expected));
}
