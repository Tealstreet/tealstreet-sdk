use safe_cex_transport::{NetworkFailure, RestFailure, RestRequest, RestResponse, TransportError};

#[test]
#[ignore = "live installed axios-retry in mandatory core isolated-dependency CI step"]
fn live_network_eligibility_matches_installed_axios_retry() {
    let output = std::process::Command::new("node")
        .arg("scripts/network-retry-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Compare the source list independently of the generated cases: an upstream
    // removal must not silently remove the only witness for a stale Rust denial.
    let provider = syn::parse_file(include_str!("../../safe-cex-transport/src/provider.rs"))
        .expect("provider source must parse");
    let declared = provider
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Const(item) if item.ident == "NETWORK_DENIED_CODES" => Some(item),
            _ => None,
        })
        .expect("network denylist must remain explicitly inventoried");
    let syn::Expr::Reference(reference) = declared.expr.as_ref() else {
        panic!("network denylist must be a literal array reference");
    };
    let syn::Expr::Array(array) = reference.expr.as_ref() else {
        panic!("network denylist must be a literal array");
    };
    let rust_codes: std::collections::BTreeSet<String> = array
        .elems
        .iter()
        .map(|expr| {
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(code),
                ..
            }) = expr
            else {
                panic!("network denylist entries must be string literals");
            };
            code.value()
        })
        .collect();
    let installed_codes: std::collections::BTreeSet<String> = oracle["denied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|code| code.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        rust_codes, installed_codes,
        "network denylist must match installed is-retry-allowed in both directions"
    );
    assert_eq!(
        oracle["cases"].as_array().unwrap().len(),
        2 * (8 + rust_codes.len()),
        "NETWORK-CARDINALITY source cases"
    );
    for case in oracle["cases"].as_array().unwrap() {
        let code = case["code"].as_str().map(str::to_owned);
        let mut failure = RestFailure::from_transport(
            TransportError::Network(NetworkFailure::from_code("fixture".into(), code)),
            RestRequest::default(),
        );
        if case["response"] == true {
            failure.response = Some(Box::new(RestResponse {
                status: 400,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
            }));
        }
        assert_eq!(
            failure.is_retryable_network(),
            case["expected"].as_bool().unwrap(),
            "originating error evidence must match axios-retry: {case}"
        );
    }
}
#[test]
fn network_retry_eligibility_cannot_be_supplied_as_a_boolean() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/network_retry_bool.rs");
    cases.compile_fail("tests/ui/rest_body_no_default.rs");
    cases.compile_fail("tests/ui/network_retry_from_bool.rs");
}

#[test]
fn remote_signer_hosts_must_handle_both_operations() {
    trybuild::TestCases::new().compile_fail("tests/ui/signer_http_only.rs");
}

#[test]
fn transformed_request_data_requires_shared_transform_provenance() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/transformed_data_private.rs");
    cases.compile_fail("tests/ui/transformed_data_no_default.rs");
}

#[test]
fn response_provenance_has_no_implicit_default_or_byte_view() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/response_body_no_default.rs");
    cases.compile_fail("tests/ui/response_body_no_byte_deref.rs");
    cases.pass("tests/ui/response_body_explicit.rs");
}
