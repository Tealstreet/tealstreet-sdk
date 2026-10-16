use safe_cex_venue_coinbase::starbase_cutover::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};
#[path = "support/schema.rs"]
mod schema;
#[path = "support/wire_identity.rs"]
mod wire_identity;
use wire_identity::{
    NativeWireIdentity, assert_native_identity_and_fields, assert_wire_corpus_content,
    complete_identity_calls, counted_identity, reset_complete_identity_calls,
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/cutover.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn check<T: DeserializeOwned + Serialize + NativeWireIdentity>(
    name: &str,
    identity: impl Fn(&T, &Value),
) {
    assert_eq!(
        schema::wire_schema(include_str!("../src/starbase_cutover.rs"), name),
        oracle()["schemas"][name],
        "{name} declaration"
    );
    let cases = oracle()["types"][name].as_array().unwrap();
    let expected_count = match name {
        "CoinbaseStarbaseImplementationStatus" => 8,
        "CoinbaseStarbaseVerificationStatus" => 8,
        "CoinbaseStarbaseCutoverEvidence" => 105,
        "CoinbaseStarbaseHttpShadowBlocker" => 14,
        "CoinbaseStarbaseWebsocketBlocker" => 12,
        "CoinbaseStarbaseCutoverBlocker" => 34,
        "CoinbaseStarbaseHttpShadowReadiness" => 85,
        "CoinbaseStarbaseCutoverPlan" => 223,
        _ => panic!("unclassified Coinbase cutover corpus: {name}"),
    };
    assert_eq!(cases.len(), expected_count, "{name}: source case total");
    assert!(!cases.is_empty(), "{name}: missing oracle cases");
    reset_complete_identity_calls();
    let identity = counted_identity(identity);
    let mut decoded_cases = Vec::new();
    for case in cases {
        let value = serde_json::from_value::<T>(case["value"].clone());
        assert_eq!(
            value.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{name} {}",
            case["name"]
        );
        if let Ok(value) = value {
            if case["name"].as_str().unwrap().ends_with(": complete") {
                assert_native_identity_and_fields(&value, &case["value"], name, &identity);
            } else {
                value.assert_native_wire_identity(&case["value"], name);
            }
            decoded_cases.push((value, case));
        }
    }
    let expected_identity_calls = match name {
        "CoinbaseStarbaseHttpShadowReadiness" => 11,
        "CoinbaseStarbaseCutoverEvidence" => 12,
        "CoinbaseStarbaseCutoverPlan" => 27,
        _ => 1,
    };
    assert_eq!(
        complete_identity_calls(),
        expected_identity_calls,
        "{name}: native identity assertion count"
    );
    // Every native assertion, including the execution floor, precedes wire checks.
    assert_wire_corpus_content(name, &oracle()["types"][name]);
    for (value, case) in decoded_cases {
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            case["value"],
            "{name} {}",
            case["name"]
        );
    }
}
macro_rules! wire {
    ($test:ident,$ty:ident,$identity:expr) => {
        #[test]
        fn $test() {
            check::<$ty>(stringify!($ty), $identity);
        }
    };
}
wire!(
    cutover_implementation_status,
    CoinbaseStarbaseImplementationStatus,
    |value, input| match (value, input.as_str().unwrap()) {
        (CoinbaseStarbaseImplementationStatus::Implemented, "implemented")
        | (CoinbaseStarbaseImplementationStatus::NotImplemented, "not-implemented") => (),
        _ => panic!("implementation status field identity"),
    }
);
wire!(
    cutover_verification_status,
    CoinbaseStarbaseVerificationStatus,
    |value, input| match (value, input.as_str().unwrap()) {
        (CoinbaseStarbaseVerificationStatus::Verified, "verified")
        | (CoinbaseStarbaseVerificationStatus::Unverified, "unverified") => (),
        _ => panic!("verification status field identity"),
    }
);
wire!(
    cutover_evidence,
    CoinbaseStarbaseCutoverEvidence,
    |value, input| {
        let verification = |value| match value {
            CoinbaseStarbaseVerificationStatus::Verified => "verified",
            CoinbaseStarbaseVerificationStatus::Unverified => "unverified",
        };
        let implementation = |value| match value {
            CoinbaseStarbaseImplementationStatus::Implemented => "implemented",
            CoinbaseStarbaseImplementationStatus::NotImplemented => "not-implemented",
        };
        assert_eq!(
            verification(value.credentialed_http_authentication),
            input["credentialedHttpAuthentication"]
        );
        assert_eq!(
            verification(value.credentialed_http_read_parity),
            input["credentialedHttpReadParity"]
        );
        assert_eq!(
            verification(value.credentialed_websocket_session_lifecycle),
            input["credentialedWebsocketSessionLifecycle"]
        );
        assert_eq!(
            verification(value.credentialed_write_canary),
            input["credentialedWriteCanary"]
        );
        assert_eq!(verification(value.cutover_runbook), input["cutoverRunbook"]);
        assert_eq!(
            implementation(value.http_authentication_contract),
            input["httpAuthenticationContract"]
        );
        assert_eq!(
            implementation(value.private_http_response_contracts),
            input["privateHttpResponseContracts"]
        );
        assert_eq!(
            implementation(value.public_http_response_contracts),
            input["publicHttpResponseContracts"]
        );
    }
);
wire!(
    cutover_http_blockers,
    CoinbaseStarbaseHttpShadowBlocker,
    |value, input| {
        assert_eq!(
            *value,
            CoinbaseStarbaseHttpShadowBlocker::CredentialedHttpAuthenticationUnverified
        );
        assert_eq!(
            input.as_str().unwrap(),
            "credentialed-http-authentication-unverified"
        );
    }
);
wire!(
    cutover_websocket_blockers,
    CoinbaseStarbaseWebsocketBlocker,
    |value, input| {
        assert_eq!(
            *value,
            CoinbaseStarbaseWebsocketBlocker::CredentialedWebsocketSessionLifecycleUnverified
        );
        assert_eq!(
            input.as_str().unwrap(),
            "credentialed-websocket-session-lifecycle-unverified"
        );
    }
);
wire!(
    cutover_all_blockers,
    CoinbaseStarbaseCutoverBlocker,
    |value, input| {
        assert_eq!(
            *value,
            CoinbaseStarbaseCutoverBlocker::CredentialedHttpAuthenticationUnverified
        );
        assert_eq!(
            input.as_str().unwrap(),
            "credentialed-http-authentication-unverified"
        );
    }
);
wire!(
    cutover_http_readiness,
    CoinbaseStarbaseHttpShadowReadiness,
    |value, input| {
        assert_eq!(value.eligible, input["eligible"]);
        let classification = match value.product_classification {
            CoinbaseStarbaseTestProductClassification::Spot => "spot",
            CoinbaseStarbaseTestProductClassification::Cfm => "cfm",
            CoinbaseStarbaseTestProductClassification::Intx => "intx",
        };
        assert_eq!(classification, input["productClassification"]);
        let blocker = match value.blockers[0] {
            CoinbaseStarbaseHttpShadowBlocker::CredentialedHttpAuthenticationUnverified => {
                "credentialed-http-authentication-unverified"
            }
            CoinbaseStarbaseHttpShadowBlocker::HttpAuthenticationContractNotImplemented => {
                "http-authentication-contract-not-implemented"
            }
            CoinbaseStarbaseHttpShadowBlocker::PrivateHttpResponseContractsNotImplemented => {
                "private-http-response-contracts-not-implemented"
            }
            CoinbaseStarbaseHttpShadowBlocker::ProductOutsideIntxMigration => {
                "product-outside-intx-migration"
            }
            CoinbaseStarbaseHttpShadowBlocker::PublicHttpResponseContractsNotImplemented => {
                "public-http-response-contracts-not-implemented"
            }
        };
        assert_eq!(blocker, input["blockers"][0]);
    }
);
wire!(cutover_plan, CoinbaseStarbaseCutoverPlan, |value, input| {
    assert_eq!(
        value.product_binding.required,
        CoinbaseStarbaseCutoverPlanProductBindingRequired::TrustedRuntimeIntxPortfolioBinding
    );
    assert_eq!(
        value.product_binding.status,
        CoinbaseStarbaseCutoverPlanProductBindingStatus::Unbound
    );
    assert_eq!(
        input["productBinding"]["required"],
        "trusted-runtime-intx-portfolio-binding"
    );
    assert_eq!(input["productBinding"]["status"], "unbound");
});
#[test]
fn cutover_type_inventory() {
    assert_eq!(oracle()["schemas"].as_object().unwrap().len(), 8);
}

#[test]
fn cutover_blocker_order_for_every_evidence_combination() {
    for case in oracle()["collectors"].as_array().unwrap() {
        let evidence: CoinbaseStarbaseCutoverEvidence =
            serde_json::from_value(case["evidence"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(collect_websocket_blockers(&evidence)).unwrap(),
            case["websocket"],
            "websocket {}",
            case["evidence"]
        );
        for http in case["http"].as_array().unwrap() {
            let product = serde_json::from_value(http["product"].clone()).unwrap();
            assert_eq!(
                serde_json::to_value(collect_http_shadow_blockers(product, &evidence)).unwrap(),
                http["blockers"],
                "http {} {}",
                http["product"],
                case["evidence"]
            );
        }
    }
}

#[test]
fn cutover_validation_own_key_order_and_empty_key_truthiness() {
    for case in oracle()["validation"].as_array().unwrap() {
        let input = case["inputJson"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let result = match validate_evidence(input.as_ref()) {
            Ok(()) => serde_json::json!({"ok":true}),
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, case["result"], "{}", case["name"]);
    }
}

#[test]
fn cutover_test_only_readiness_validation_precedence_and_plan_omission() {
    for case in oracle()["readiness"].as_array().unwrap() {
        let mut options = serde_json::Map::new();
        if let Some(input) = case["inputJson"].as_str() {
            options.insert("evidence".into(), serde_json::from_str(input).unwrap());
        }
        if let Some(product) = case.get("product") {
            options.insert("productClassification".into(), product.clone());
        }
        let result = match evaluate_coinbase_starbase_http_shadow_readiness_for_tests_only(&options)
        {
            Ok(value) => serde_json::json!({"ok":value}),
            Err(error) => serde_json::json!({"error":error}),
        };
        assert_eq!(result, case["result"], "{case}");
    }
}

#[test]
fn cutover_production_selector_uses_fixed_evidence_and_closed_plan() {
    let select: fn() -> Result<
        CoinbaseStarbaseCutoverPlan,
        safe_cex_venue_coinbase::starbase::CoinbaseStarbaseProtocolError,
    > = select_coinbase_starbase_cutover_plan;
    assert_eq!(
        serde_json::to_value(select().unwrap()).unwrap(),
        oracle()["selectedPlan"]
    );
}
