//! Pure account-tier helpers from lighter.account-tier.ts.
use safe_cex_num::{JsCoercionError, js_string, js_trim};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: LighterAccountTier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LighterAccountTier {
    Standard,
    Premium,
    Plus,
}
// ts: LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE
pub const LIGHTER_PARTNER_ATTRIBUTION_TIER_REQUIREMENT_MESSAGE: &str = "Lighter requires Plus or Premium account tier for Tealstreet partner-attributed orders with non-zero integrator fees. Switch this account to Plus or Premium, then retry.";
// ts: LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE
pub const LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE: &str = "Lighter requires Plus or Premium account tier before approving Tealstreet partner attribution with non-zero integrator fees. Switch this account to Plus or Premium, then retry.";
// ts: LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE
pub const LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_MISSING_MESSAGE: &str = "Tealstreet partner attribution is not yet approved for this Lighter account. Open Account Settings, edit this Lighter account, then click \"Approve Builder Fee\" and retry.";

// ts: asRecord
pub fn as_record(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| v.is_object() || v.is_array())
}
// ts: isLighterAccountTierEligibleForPartnerAttribution
pub fn is_lighter_account_tier_eligible_for_partner_attribution(
    tier: Option<LighterAccountTier>,
) -> bool {
    matches!(
        tier,
        Some(LighterAccountTier::Premium | LighterAccountTier::Plus)
    )
}
// ts: hasNonZeroLighterIntegratorFee
pub fn has_non_zero_lighter_integrator_fee(
    integrator_maker_fee: Option<f64>,
    integrator_taker_fee: Option<f64>,
) -> bool {
    let maker_fee = integrator_maker_fee.unwrap_or(0.0);
    let taker_fee = integrator_taker_fee.unwrap_or(0.0);
    (maker_fee.is_finite() && maker_fee != 0.0) || (taker_fee.is_finite() && taker_fee != 0.0)
}

// Require a string before applying serde's variant table. Its default unit-enum
// decoder also accepts tagged objects, which the TypeScript literal union rejects.
#[derive(Deserialize)]
#[serde(remote = "LighterAccountTier", rename_all = "lowercase")]
enum LighterAccountTierWire {
    Standard,
    Premium,
    Plus,
}
impl<'de> Deserialize<'de> for LighterAccountTier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        LighterAccountTierWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

// Apply this venue callsite's nullish fallback BEFORE shared String(any).
fn nullish_string(value: Option<&Value>) -> Result<String, JsCoercionError> {
    match value {
        None | Some(Value::Null) => Ok(String::new()),
        value => js_string(value),
    }
}

// ts: normalizeLighterAccountTier
pub fn normalize_lighter_account_tier(
    value: Option<&Value>,
) -> Result<Option<LighterAccountTier>, JsCoercionError> {
    let input = nullish_string(value)?;
    Ok(match js_trim(&input).to_lowercase().as_str() {
        "std" | "standard" => Some(LighterAccountTier::Standard),
        "premium" => Some(LighterAccountTier::Premium),
        "plus" => Some(LighterAccountTier::Plus),
        _ => None,
    })
}

// ts: extractLighterAccountTierFromPayload
pub fn extract_lighter_account_tier_from_payload(
    payload: Option<&Value>,
) -> Result<Option<LighterAccountTier>, JsCoercionError> {
    let Some(root) = as_record(payload) else {
        return Ok(None);
    };
    let data = as_record(root.get("data"));
    let account =
        as_record(root.get("account")).or_else(|| as_record(data.and_then(|v| v.get("account"))));
    for record in [Some(root), data, account].into_iter().flatten() {
        for key in [
            "user_tier_name",
            "userTierName",
            "user_tier",
            "userTier",
            "account_type_name",
            "accountTypeName",
            "account_type",
            "accountType",
        ] {
            if let Some(value) = record.get(key).filter(|v| !v.is_null()) {
                return normalize_lighter_account_tier(Some(value));
            }
        }
    }
    normalize_lighter_account_tier(None)
}

// ts: isLighterPartnerAttributionTierRejection
pub fn is_lighter_partner_attribution_tier_rejection(
    code: Option<&Value>,
    message: Option<&Value>,
) -> Result<bool, JsCoercionError> {
    if js_trim(&nullish_string(code)?) == "21520" {
        return Ok(true);
    }
    let message = nullish_string(message)?.to_lowercase();
    Ok(message.contains("standard")
        && (message.contains("integrator") || message.contains("partner attribution"))
        && (message.contains("plus")
            || message.contains("premium")
            || message.contains("account type")))
}

// ts: isLighterPartnerAttributionApprovalMissingRejection
pub fn is_lighter_partner_attribution_approval_missing_rejection(
    code: Option<&Value>,
    message: Option<&Value>,
) -> Result<bool, JsCoercionError> {
    if js_trim(&nullish_string(code)?) == "21149" {
        return Ok(true);
    }
    let message = nullish_string(message)?.to_lowercase();
    Ok(message.contains("integrator")
        && (message.contains("not approved") || message.contains("is not approved")))
}
