use safe_cex_num::JsCoercionError;
use serde::Deserialize;
use serde::Serialize;
mod support;
use safe_cex_venue_lighter::account_tier::*;
use serde_json::{Value, json};
use std::process::Command;

fn number(value: Option<&Value>) -> Option<f64> {
    value.map(|v| match v.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        Some(other) => panic!("unknown tag {other}"),
        None => v.as_f64().unwrap(),
    })
}
fn tier_name(tier: LighterAccountTier) -> &'static str {
    match tier {
        LighterAccountTier::Standard => "standard",
        LighterAccountTier::Premium => "premium",
        LighterAccountTier::Plus => "plus",
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn lighter_account_tier_matches_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/tier-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("tiers", 3),
        ("normalized", 27),
        ("eligible", 4),
        ("payloads", 958),
        ("errors", 99),
        ("fees", 81),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "{name} scalar/opaque corpus inventory"
        );
    }

    for literal in oracle["tiers"].as_array().unwrap() {
        let invalid = serde_json::json!({literal.as_str().unwrap():null});
        assert!(
            LighterAccountTier::deserialize(invalid).is_err(),
            "LighterAccountTier public qualified decoder rejects object"
        );
    }

    support::assert_string_only::<LighterAccountTier>(&oracle["tiers"], "LighterAccountTier");

    assert_eq!(
        json!(support::enum_inventory::<LighterAccountTier>()),
        oracle["tiers"],
        "LighterAccountTier accepted wire inventory"
    );
    let tiers = [
        LighterAccountTier::Standard,
        LighterAccountTier::Premium,
        LighterAccountTier::Plus,
    ];
    assert_eq!(
        serde_json::to_value(tiers).unwrap(),
        oracle["tiers"],
        "LighterAccountTier"
    );
    for tier in tiers {
        assert_eq!(serde_json::to_value(tier).unwrap(), json!(tier_name(tier)));
    }
    assert_eq!(
        json!({
            "LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE,
            "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
            "LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE": LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE,
        }),
        oracle["constants"],
        "account-tier messages"
    );
    for case in oracle["normalized"].as_array().unwrap() {
        assert_eq!(
            outcome(
                normalize_lighter_account_tier(case.get("input")).map(|value| value.map(tier_name))
            ),
            case["result"],
            "normalizeLighterAccountTier {case}"
        );
        let decoded = serde_json::from_value::<LighterAccountTier>(case["input"].clone());
        if let Ok(tier) = decoded.as_ref() {
            assert_eq!(
                Some(tier_name(*tier)),
                case["input"].as_str(),
                "LighterAccountTier native decoded identity {case}"
            );
        }
        assert_eq!(
            decoded.is_ok(),
            oracle["tiers"].as_array().unwrap().contains(&case["input"]),
            "LighterAccountTier exact decoding {case}"
        );
        assert_eq!(
            as_record(case.get("input")).unwrap_or(&Value::Null),
            &case["record"],
            "asRecord native value identity {case}"
        );
    }
    for case in oracle["eligible"].as_array().unwrap() {
        let tier = case
            .get("tier")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        assert_eq!(
            is_lighter_account_tier_eligible_for_partner_attribution(tier),
            case["expected"].as_bool().unwrap(),
            "isLighterAccountTierEligibleForPartnerAttribution {case}"
        );
    }
    for case in oracle["payloads"].as_array().unwrap() {
        assert_eq!(
            outcome(
                extract_lighter_account_tier_from_payload(case.get("input"))
                    .map(|value| value.map(tier_name))
            ),
            case["result"],
            "extractLighterAccountTierFromPayload {case}"
        );
    }
    for case in oracle["errors"].as_array().unwrap() {
        assert_eq!(
            outcome(is_lighter_partner_attribution_tier_rejection(
                case.get("code"),
                case.get("message")
            )),
            case["tier"],
            "isLighterPartnerAttributionTierRejection {case}"
        );
        assert_eq!(
            outcome(is_lighter_partner_attribution_approval_missing_rejection(
                case.get("code"),
                case.get("message")
            )),
            case["approval"],
            "isLighterPartnerAttributionApprovalMissingRejection {case}"
        );
    }
    for case in oracle["fees"].as_array().unwrap() {
        assert_eq!(
            has_non_zero_lighter_integrator_fee(
                number(case.get("integratorMakerFee")),
                number(case.get("integratorTakerFee"))
            ),
            case["expected"].as_bool().unwrap(),
            "hasNonZeroLighterIntegratorFee {case}"
        );
    }
}

fn outcome<T: Serialize>(result: Result<T, JsCoercionError>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
    }
}
