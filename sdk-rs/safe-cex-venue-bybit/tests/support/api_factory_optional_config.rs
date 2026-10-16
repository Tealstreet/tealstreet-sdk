use super::*;

pub fn collect_native() -> Vec<Value> {
    let inputs = [
        ("network-absent-config", false, "", false),
        ("signature-absent-config", true, "", true),
        ("network-omitted-method", false, "/private", false),
        ("network-empty-method", false, "/private", false),
    ];
    assert_eq!(
        inputs.len(),
        4,
        "factory_optional_config_native_helper_floor"
    );
    let mut results = Vec::new();
    let mut totals = [0_usize; 2];
    for (id, signature_error, url, expected) in inputs {
        // An unclassified typed request is the predicate counterpart; no absence bit is claimed.
        let request = RestRequest {
            url: url.into(),
            ..Default::default()
        };
        assert!(
            request.method.is_empty(),
            "factory_optional_config_native_unclassified_method {id}"
        );
        let failure = if signature_error {
            RestFailure {
                code: Some("ERR_BAD_RESPONSE".into()),
                transport_error: None,
                message: "signature".into(),
                response: Some(Box::new(RestResponse {
                    status: 400,
                    reason: Some("Bad Request".into()),
                    headers: Default::default(),
                    body: RestResponseBody::Transformed(json!({"ret_msg":"error sign!"})),
                })),
                request: Box::new(request),
                context: None,
            }
        } else {
            RestFailure::from_transport(
                TransportError::Network(NetworkFailure::from_code(
                    "network".into(),
                    Some("ERR_NETWORK".into()),
                )),
                request,
            )
        };
        let predicate = safe_cex_venue_bybit::api::should_retry_bybit_request(&failure).unwrap();
        assert_eq!(
            predicate, expected,
            "factory_optional_config_native_pure_value {id}"
        );
        totals[usize::from(!predicate)] += 1;
        results.push(json!({"id":id,"predicate":predicate}));
    }
    assert_eq!(
        totals,
        [1, 3],
        "factory_optional_config_native_branch_floors"
    );
    assert_eq!(
        results.len(),
        4,
        "factory_optional_config_native_complete_before_source"
    );
    results
}

pub fn compare_source(native: &[Value], source: &Value) {
    assert_eq!(source["declaredHelperCases"], 4);
    assert_eq!(source["helperCases"].as_array().unwrap().len(), 4);
    assert_eq!(native.len(), 4);
    for (index, expected_id) in [
        "network-absent-config",
        "signature-absent-config",
        "network-omitted-method",
        "network-empty-method",
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(native[index]["id"], expected_id);
        assert_eq!(source["helperCases"][index]["id"], expected_id);
        // Source signatureRejectedResponse refuses an error without config; a native
        // RestFailure always carries its request, so that absence has no native state.
        if expected_id == "signature-absent-config" {
            assert_eq!(
                (
                    &native[index]["predicate"],
                    &source["helperCases"][index]["predicate"]
                ),
                (&json!(true), &json!(false)),
                "factory_optional_config_absent_config_boundary"
            );
            continue;
        }
        assert_eq!(
            native[index]["predicate"], source["helperCases"][index]["predicate"],
            "factory_optional_config_source_pure_comparison {expected_id}"
        );
    }
}
