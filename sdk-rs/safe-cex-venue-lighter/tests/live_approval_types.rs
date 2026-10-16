use safe_cex_venue_lighter::account_tier::LighterAccountTier;
use safe_cex_venue_lighter::builder_approval::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Command;
mod support;
fn number(v: &Value) -> f64 {
    match v.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(s) => panic!("unknown number {s}"),
        None => v.as_f64().unwrap(),
    }
}
fn fields(expected: &Value, values: &[(&str, &str, bool)], name: &str) {
    let actual: Vec<_> = values
        .iter()
        .map(|(name, typ, optional)| json!({"name":name,"type":typ,"optional":optional}))
        .collect();
    assert_eq!(json!(actual), *expected, "{name} field schema");
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_approval_types_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/approval-types-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["fields"].as_array().unwrap().len(),
        36,
        "TYPE-CARDINALITY fields source cases"
    );

    support::assert_string_only::<LighterBuilderApprovalStatus>(
        &oracle["statuses"],
        "LighterBuilderApprovalStatus",
    );
    assert_eq!(
        json!(support::enum_inventory::<LighterBuilderApprovalStatus>()),
        oracle["statuses"],
        "LighterBuilderApprovalStatus accepted inventory"
    );
    let statuses = [
        LighterBuilderApprovalStatus::Approved,
        LighterBuilderApprovalStatus::Missing,
        LighterBuilderApprovalStatus::Expired,
        LighterBuilderApprovalStatus::InsufficientFeeCap,
        LighterBuilderApprovalStatus::AccountTierIneligible,
    ];
    assert_eq!(
        serde_json::to_value(statuses).unwrap(),
        oracle["statuses"],
        "LighterBuilderApprovalStatus exhaustive inventory"
    );
    for status in statuses {
        let literal = match status {
            LighterBuilderApprovalStatus::Approved => "approved",
            LighterBuilderApprovalStatus::Missing => "missing",
            LighterBuilderApprovalStatus::Expired => "expired",
            LighterBuilderApprovalStatus::InsufficientFeeCap => "insufficient_fee_cap",
            LighterBuilderApprovalStatus::AccountTierIneligible => "account_tier_ineligible",
        };
        assert_eq!(serde_json::to_value(status).unwrap(), json!(literal));
    }
    for value in oracle["statuses"].as_array().unwrap() {
        let parsed = LighterBuilderApprovalStatus::deserialize(value.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            *value,
            "LighterBuilderApprovalStatus decoded literal"
        );
        for wrong in [
            json!({value.as_str().unwrap():null}),
            json!({value.as_str().unwrap():{}}),
            json!({value.as_str().unwrap():[]}),
        ] {
            assert!(
                LighterBuilderApprovalStatus::deserialize(wrong).is_err(),
                "LighterBuilderApprovalStatus public decoder rejects tagged objects"
            );
        }
    }
    fields(
        &oracle["schemas"]["LighterBuilderApprovalContext"],
        &[
            ("accountIndex", "bigint", false),
            ("apiKeyIndex", "number", false),
            ("approvalExpiryMs", "number", false),
            ("integratorAccountIndex", "bigint", false),
            ("l1Address", "string", true),
            ("maxPerpsMakerFee", "number", false),
            ("maxPerpsTakerFee", "number", false),
            ("maxSpotMakerFee", "number", false),
            ("maxSpotTakerFee", "number", false),
            ("nonce", "bigint", false),
            ("signerChainId", "number", false),
            ("testnet", "boolean", false),
        ],
        "LighterBuilderApprovalContext",
    );
    fields(
        &oracle["schemas"]["LighterBuilderApprovalCheckResult"],
        &[
            ("accountIndex", "bigint", false),
            ("accountTier", "LighterAccountTier", true),
            ("approved", "boolean", false),
            ("approvalExpiryMs", "number", true),
            (
                "approvalStatus",
                "'approved' | 'missing' | 'expired' | 'insufficient_fee_cap' | 'account_tier_ineligible'",
                false,
            ),
            ("integratorAccountIndex", "bigint", false),
            ("l1Address", "string", true),
            ("maxPerpsMakerFee", "number", true),
            ("maxPerpsTakerFee", "number", true),
            ("maxSpotMakerFee", "number", true),
            ("maxSpotTakerFee", "number", true),
        ],
        "LighterBuilderApprovalCheckResult",
    );
    fields(
        &oracle["schemas"]["LighterBuilderApprovalResult"],
        &[
            ("message", "string", false),
            ("success", "boolean", false),
            ("txHash", "string", true),
        ],
        "LighterBuilderApprovalResult",
    );
    for case in oracle["fields"].as_array().unwrap() {
        let n = number(&case["number"]);
        let big = num_bigint::BigInt::parse_bytes(case["bigint"].as_str().unwrap().as_bytes(), 10)
            .unwrap();
        let present = case["present"].as_bool().unwrap();
        let text = "\u{feff}\u{85}address/hash/message";
        let value = LighterBuilderApprovalContext {
            account_index: big.clone(),
            api_key_index: n,
            approval_expiry_ms: n,
            integrator_account_index: big.clone(),
            l1_address: present.then(|| text.to_owned()),
            max_perps_maker_fee: n,
            max_perps_taker_fee: n,
            max_spot_maker_fee: n,
            max_spot_taker_fee: n,
            nonce: big.clone(),
            signer_chain_id: n,
            testnet: present,
        };
        let LighterBuilderApprovalContext {
            account_index,
            api_key_index,
            approval_expiry_ms,
            integrator_account_index,
            l1_address,
            max_perps_maker_fee,
            max_perps_taker_fee,
            max_spot_maker_fee,
            max_spot_taker_fee,
            nonce,
            signer_chain_id,
            testnet,
        } = value;
        assert_eq!(
            account_index.to_string(),
            big.to_string(),
            "LighterBuilderApprovalContext.accountIndex bigint"
        );
        assert_eq!(
            api_key_index.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.apiKeyIndex binary64"
        );
        assert_eq!(
            approval_expiry_ms.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.approvalExpiryMs binary64"
        );
        assert_eq!(
            integrator_account_index.to_string(),
            big.to_string(),
            "LighterBuilderApprovalContext.integratorAccountIndex bigint"
        );
        assert_eq!(l1_address.as_deref(), present.then_some(text));
        assert_eq!(
            max_perps_maker_fee.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.maxPerpsMakerFee binary64"
        );
        assert_eq!(
            max_perps_taker_fee.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.maxPerpsTakerFee binary64"
        );
        assert_eq!(
            max_spot_maker_fee.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.maxSpotMakerFee binary64"
        );
        assert_eq!(
            max_spot_taker_fee.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.maxSpotTakerFee binary64"
        );
        assert_eq!(
            nonce.to_string(),
            big.to_string(),
            "LighterBuilderApprovalContext.nonce bigint"
        );
        assert_eq!(
            signer_chain_id.to_bits(),
            n.to_bits(),
            "LighterBuilderApprovalContext.signerChainId binary64"
        );
        assert_eq!(testnet, present);
        let value = LighterBuilderApprovalCheckResult {
            account_index: big.clone(),
            account_tier: present.then_some(LighterAccountTier::Plus),
            approved: present,
            approval_expiry_ms: present.then_some(n),
            approval_status: LighterBuilderApprovalStatus::AccountTierIneligible,
            integrator_account_index: big.clone(),
            l1_address: present.then(|| text.to_owned()),
            max_perps_maker_fee: present.then_some(n),
            max_perps_taker_fee: present.then_some(n),
            max_spot_maker_fee: present.then_some(n),
            max_spot_taker_fee: present.then_some(n),
        };
        let LighterBuilderApprovalCheckResult {
            account_index,
            account_tier,
            approved,
            approval_expiry_ms,
            approval_status,
            integrator_account_index,
            l1_address,
            max_perps_maker_fee,
            max_perps_taker_fee,
            max_spot_maker_fee,
            max_spot_taker_fee,
        } = value;
        assert_eq!(
            account_index.to_string(),
            big.to_string(),
            "LighterBuilderApprovalCheckResult.accountIndex bigint"
        );
        assert_eq!(account_tier, present.then_some(LighterAccountTier::Plus));
        assert_eq!(approved, present);
        assert_eq!(
            approval_expiry_ms.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterBuilderApprovalCheckResult.approvalExpiryMs binary64"
        );
        assert_eq!(
            approval_status,
            LighterBuilderApprovalStatus::AccountTierIneligible
        );
        assert_eq!(
            integrator_account_index.to_string(),
            big.to_string(),
            "LighterBuilderApprovalCheckResult.integratorAccountIndex bigint"
        );
        assert_eq!(l1_address.as_deref(), present.then_some(text));
        assert_eq!(
            max_perps_maker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterBuilderApprovalCheckResult.maxPerpsMakerFee binary64"
        );
        assert_eq!(
            max_perps_taker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterBuilderApprovalCheckResult.maxPerpsTakerFee binary64"
        );
        assert_eq!(
            max_spot_maker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterBuilderApprovalCheckResult.maxSpotMakerFee binary64"
        );
        assert_eq!(
            max_spot_taker_fee.map(f64::to_bits),
            present.then(|| n.to_bits()),
            "LighterBuilderApprovalCheckResult.maxSpotTakerFee binary64"
        );
        let value = LighterBuilderApprovalResult {
            message: text.to_owned(),
            success: present,
            tx_hash: present.then(|| text.to_owned()),
        };
        let LighterBuilderApprovalResult {
            message,
            success,
            tx_hash,
        } = value;
        assert_eq!(message, text);
        assert_eq!(success, present);
        assert_eq!(tx_hash.as_deref(), present.then_some(text));
    }
}

#[test]
fn native_record_clone_retains_field_identity() {
    macro_rules! identity {
        ($typ:ident, {$($field:ident: $value:expr),+}) => {{
            let source = $typ { $($field: $value),+ };
            let actual = source.clone();
            $(assert_eq!(actual.$field, source.$field, concat!(stringify!($typ), ".", stringify!($field), " native clone identity"));)+
            actual
        }};
    }
    for flags in 0..8 {
        identity!(LighterBuilderApprovalContext, {account_index: num_bigint::BigInt::from(1001u64), api_key_index: 102.25, approval_expiry_ms: 103.25, integrator_account_index: num_bigint::BigInt::from(1004u64), l1_address: Some("identity:LighterBuilderApprovalContext.l1_address".to_owned()), max_perps_maker_fee: 106.25, max_perps_taker_fee: 107.25, max_spot_maker_fee: 108.25, max_spot_taker_fee: 109.25, nonce: num_bigint::BigInt::from(1010u64), signer_chain_id: 111.25, testnet: flags & 4 != 0});
        identity!(LighterBuilderApprovalCheckResult, {account_index: num_bigint::BigInt::from(1001u64), account_tier: Some(LighterAccountTier::Plus), approved: flags & 4 != 0, approval_expiry_ms: Some(104.25), approval_status: LighterBuilderApprovalStatus::AccountTierIneligible, integrator_account_index: num_bigint::BigInt::from(1006u64), l1_address: Some("identity:LighterBuilderApprovalCheckResult.l1_address".to_owned()), max_perps_maker_fee: Some(108.25), max_perps_taker_fee: Some(109.25), max_spot_maker_fee: Some(110.25), max_spot_taker_fee: Some(111.25)});
        identity!(LighterBuilderApprovalResult, {message: "identity:LighterBuilderApprovalResult.message".to_owned(), success: flags & 2 != 0, tx_hash: Some("identity:LighterBuilderApprovalResult.tx_hash".to_owned())});
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn source_cardinality_controls_fail_by_name() {
    let (field, assertion) = ("fields", "TYPE-CARDINALITY fields source cases");
    {
        for mode in ["empty", "subset"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "lighter_approval_types_match_live_typescript",
                    "--ignored",
                    "--nocapture",
                ])
                .env(
                    "LIGHTER_TYPE_CARDINALITY_MUTATION",
                    format!("{field}:{mode}"),
                )
                .output()
                .unwrap();
            let log = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.status.code(), Some(101), "{log}");
            assert!(log.contains(assertion), "{log}");
        }
    }
}
