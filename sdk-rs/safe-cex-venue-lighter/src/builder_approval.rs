use crate::account_tier::has_non_zero_lighter_integrator_fee;
use crate::builder_fee::{LIGHTER_INTEGRATOR_MAKER_FEE_VALUE, LIGHTER_INTEGRATOR_TAKER_FEE_VALUE};
use crate::deployment::get_lighter_api_base_url;
use safe_cex_num::js_trim;
use safe_cex_transport::TransportRegistry;
use safe_cex_types::ExchangeName;
use serde::Serialize;

// ts: LighterBuilderApprovalFeeCaps
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterBuilderApprovalFeeCaps {
    pub max_perps_maker_fee: f64,
    pub max_perps_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub max_spot_taker_fee: f64,
}

// ts: DEFAULT_LIGHTER_APPROVAL_FEE_CAPS
pub const DEFAULT_LIGHTER_APPROVAL_FEE_CAPS: LighterBuilderApprovalFeeCaps =
    LighterBuilderApprovalFeeCaps {
        max_perps_maker_fee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        max_perps_taker_fee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
        max_spot_maker_fee: LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        max_spot_taker_fee: LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
    };

// ts: DEFAULT_LIGHTER_APPROVAL_FEE_VALUE
pub const DEFAULT_LIGHTER_APPROVAL_FEE_VALUE: f64 = LIGHTER_INTEGRATOR_TAKER_FEE_VALUE;

// ts: approvalFeeCapsHaveNonZeroFees
pub fn approval_fee_caps_have_non_zero_fees(fee_caps: &LighterBuilderApprovalFeeCaps) -> bool {
    has_non_zero_lighter_integrator_fee(
        Some(fee_caps.max_perps_maker_fee),
        Some(fee_caps.max_perps_taker_fee),
    ) || has_non_zero_lighter_integrator_fee(
        Some(fee_caps.max_spot_maker_fee),
        Some(fee_caps.max_spot_taker_fee),
    )
}

// ts: getBaseUrl
pub fn get_base_url(
    exchange_name: Option<&ExchangeName>,
    override_base_url: Option<&str>,
    testnet: bool,
    registry: &TransportRegistry,
) -> String {
    if let Some(value) = override_base_url.filter(|value| !js_trim(value).is_empty()) {
        let value = js_trim(value);
        // Source removes exactly one trailing slash, even for repeated slashes.
        return value.strip_suffix('/').unwrap_or(value).to_owned();
    }
    get_lighter_api_base_url(exchange_name, Some(testnet), registry)
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterBuilderApprovalFeeCaps", rename_all = "camelCase")]
struct LighterBuilderApprovalFeeCapsWire {
    max_perps_maker_fee: f64,
    max_perps_taker_fee: f64,
    max_spot_maker_fee: f64,
    max_spot_taker_fee: f64,
}
impl<'de> serde::Deserialize<'de> for LighterBuilderApprovalFeeCaps {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                LighterBuilderApprovalFeeCapsWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: LighterBuilderApprovalContext
#[derive(Debug, Clone, PartialEq)]
pub struct LighterBuilderApprovalContext {
    pub account_index: num_bigint::BigInt,
    pub api_key_index: f64,
    pub approval_expiry_ms: f64,
    pub integrator_account_index: num_bigint::BigInt,
    pub l1_address: Option<String>,
    pub max_perps_maker_fee: f64,
    pub max_perps_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub nonce: num_bigint::BigInt,
    pub signer_chain_id: f64,
    pub testnet: bool,
}

// ts: LighterBuilderApprovalCheckResult.approvalStatus
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LighterBuilderApprovalStatus {
    Approved,
    Missing,
    Expired,
    InsufficientFeeCap,
    AccountTierIneligible,
}
#[derive(serde::Deserialize)]
#[serde(remote = "LighterBuilderApprovalStatus", rename_all = "snake_case")]
enum LighterBuilderApprovalStatusWire {
    Approved,
    Missing,
    Expired,
    InsufficientFeeCap,
    AccountTierIneligible,
}
impl<'de> serde::Deserialize<'de> for LighterBuilderApprovalStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        LighterBuilderApprovalStatusWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}

// ts: LighterBuilderApprovalCheckResult
#[derive(Debug, Clone, PartialEq)]
pub struct LighterBuilderApprovalCheckResult {
    pub account_index: num_bigint::BigInt,
    pub account_tier: Option<crate::account_tier::LighterAccountTier>,
    pub approved: bool,
    pub approval_expiry_ms: Option<f64>,
    pub approval_status: LighterBuilderApprovalStatus,
    pub integrator_account_index: num_bigint::BigInt,
    pub l1_address: Option<String>,
    pub max_perps_maker_fee: Option<f64>,
    pub max_perps_taker_fee: Option<f64>,
    pub max_spot_maker_fee: Option<f64>,
    pub max_spot_taker_fee: Option<f64>,
}

// ts: LighterBuilderApprovalResult
#[derive(Debug, Clone, PartialEq)]
pub struct LighterBuilderApprovalResult {
    pub message: String,
    pub success: bool,
    pub tx_hash: Option<String>,
}

// ts: getDefaultIntegratorAccountIndex
pub fn get_default_integrator_account_index(
    exchange_name: Option<&ExchangeName>,
    testnet: bool,
) -> Result<num_bigint::BigInt, &'static str> {
    let raw =
        crate::builder_fee::get_default_integrator_account_index(Some(testnet), exchange_name)
            .map(|v| serde_json::Value::String(v.to_owned()));
    // Both TS helpers guard the same constant string with trim and ASCII digits.
    // The builder-fee parser preserves arbitrary precision without a Number hop.
    crate::builder_fee::parse_positive_integer_big_int(raw.as_ref())
        .ok_or("Lighter integrator account index is not configured for this network")
}

// ts: createLighterL1SignatureHelperUrl.params
#[derive(Debug, Clone)]
pub struct LighterL1SignatureHelperUrlParams<'a> {
    pub account_index: &'a num_bigint::BigInt,
    pub api_key_index: f64,
    pub approval_expiry_ms: f64,
    pub chain_id: f64,
    pub expected_wallet_address: Option<&'a str>,
    pub integrator_account_index: &'a num_bigint::BigInt,
    pub max_perps_maker_fee: f64,
    pub max_perps_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub nonce: &'a num_bigint::BigInt,
    pub origin: &'a str,
    pub testnet: bool,
}

// ts: createLighterL1SignatureHelperUrl
pub fn create_lighter_l1_signature_helper_url(
    params: &LighterL1SignatureHelperUrlParams<'_>,
) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("nonce", &params.nonce.to_string());
    query.append_pair("accountIndex", &params.account_index.to_string());
    query.append_pair(
        "apiKeyIndex",
        &safe_cex_num::js_number_string(params.api_key_index),
    );
    query.append_pair(
        "integratorAccountIndex",
        &params.integrator_account_index.to_string(),
    );
    query.append_pair(
        "maxPerpsTakerFee",
        &safe_cex_num::js_number_string(params.max_perps_taker_fee),
    );
    query.append_pair(
        "maxPerpsMakerFee",
        &safe_cex_num::js_number_string(params.max_perps_maker_fee),
    );
    query.append_pair(
        "maxSpotTakerFee",
        &safe_cex_num::js_number_string(params.max_spot_taker_fee),
    );
    query.append_pair(
        "maxSpotMakerFee",
        &safe_cex_num::js_number_string(params.max_spot_maker_fee),
    );
    query.append_pair(
        "approvalExpiry",
        &safe_cex_num::js_number_string(params.approval_expiry_ms),
    );
    query.append_pair("chainId", &safe_cex_num::js_number_string(params.chain_id));
    query.append_pair("testnet", if params.testnet { "true" } else { "false" });
    if let Some(wallet) = params
        .expected_wallet_address
        .filter(|value| !value.is_empty())
    {
        query.append_pair("wallet", wallet);
    }
    format!("{}/trade/lighter-l1sig?{}", params.origin, query.finish())
}

// ts: toNumber
pub fn to_number(
    value: Option<&serde_json::Value>,
) -> Result<Option<f64>, safe_cex_num::JsCoercionError> {
    let parsed = safe_cex_num::js_number(value)?;
    Ok(parsed.is_finite().then_some(parsed))
}

// ts: toBigInt.value
// JSON wire objects plus the non-JSON primitive alternatives used by callers.
#[derive(Debug, Clone, Copy)]
pub enum LighterApprovalValue<'a> {
    Json(Option<&'a serde_json::Value>),
    BigInt(&'a num_bigint::BigInt),
    Number(f64),
}

// ts: toBigInt
pub fn to_big_int(value: LighterApprovalValue<'_>) -> Option<num_bigint::BigInt> {
    let text = match value {
        LighterApprovalValue::Json(None | Some(serde_json::Value::Null)) => return None,
        LighterApprovalValue::Json(value) => safe_cex_num::js_string(value).ok()?,
        LighterApprovalValue::BigInt(value) => value.to_string(),
        LighterApprovalValue::Number(value) => safe_cex_num::js_number_string(value),
    };
    // Source catches both String coercion and BigInt conversion failures.
    safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::String(text)).ok()
}

// ts: toHex10.value
#[derive(Debug, Clone, Copy)]
pub enum LighterApprovalInteger<'a> {
    BigInt(&'a num_bigint::BigInt),
    Number(f64),
}

// ts: toHex10
pub fn to_hex_10(
    value: LighterApprovalInteger<'_>,
    field_name: &str,
) -> Result<String, safe_cex_num::JsBigIntError> {
    let normalized = match value {
        LighterApprovalInteger::BigInt(value) => value.clone(),
        LighterApprovalInteger::Number(value) => {
            safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::Number(value))?
        }
    };
    let refusal = if normalized < num_bigint::BigInt::from(0) {
        Some(format!("{field_name} must be non-negative"))
    } else if normalized > num_bigint::BigInt::from(u64::MAX) {
        Some(format!("{field_name} exceeds uint64 range"))
    } else {
        None
    };
    if let Some(message) = refusal {
        return Err(safe_cex_num::JsBigIntError {
            kind: safe_cex_num::JsBigIntErrorKind::HostError("Error"),
            message_utf16: message.encode_utf16().collect(),
            message,
        });
    }
    let digits = normalized.to_str_radix(16);
    Ok(format!("0x{digits:0>16}"))
}

// ts: buildLighterApproveIntegratorL1Message.params
#[derive(Debug, Clone, Copy)]
pub struct LighterApproveIntegratorL1MessageParams<'a> {
    pub nonce: &'a num_bigint::BigInt,
    pub account_index: &'a num_bigint::BigInt,
    pub api_key_index: f64,
    pub integrator_account_index: &'a num_bigint::BigInt,
    pub max_perps_taker_fee: f64,
    pub max_perps_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub approval_expiry_ms: &'a num_bigint::BigInt,
    pub chain_id: f64,
}

// ts: buildLighterApproveIntegratorL1Message
pub fn build_lighter_approve_integrator_l1_message(
    params: &LighterApproveIntegratorL1MessageParams<'_>,
) -> Result<String, safe_cex_num::JsBigIntError> {
    use LighterApprovalInteger::{BigInt, Number};
    let mut message = String::from("Approve Integrator\n\n");
    for (label, field, value) in [
        ("nonce", "nonce", BigInt(params.nonce)),
        (
            "account index",
            "accountIndex",
            BigInt(params.account_index),
        ),
        ("api key index", "apiKeyIndex", Number(params.api_key_index)),
        (
            "integrator account index",
            "integratorAccountIndex",
            BigInt(params.integrator_account_index),
        ),
        (
            "max perps taker fee",
            "maxPerpsTakerFee",
            Number(params.max_perps_taker_fee),
        ),
        (
            "max perps maker fee",
            "maxPerpsMakerFee",
            Number(params.max_perps_maker_fee),
        ),
        (
            "max spot taker fee",
            "maxSpotTakerFee",
            Number(params.max_spot_taker_fee),
        ),
        (
            "max spot maker fee",
            "maxSpotMakerFee",
            Number(params.max_spot_maker_fee),
        ),
        (
            "approval expiry",
            "approvalExpiryMs",
            BigInt(params.approval_expiry_ms),
        ),
        ("chainId", "chainId", Number(params.chain_id)),
    ] {
        let hex = to_hex_10(value, field)?;
        message.push_str(label);
        message.push_str(": ");
        message.push_str(&hex);
        message.push('\n');
    }
    message.push_str("Only sign this message for a trusted client!");
    Ok(message)
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterBuilderApprovalFeeCaps {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { max_perps_maker_fee, max_perps_taker_fee, max_spot_maker_fee, max_spot_taker_fee });
    }
}
