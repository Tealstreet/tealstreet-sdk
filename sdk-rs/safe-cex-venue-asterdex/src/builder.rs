//! Pure Aster builder approval and fee helpers; no private revenue derivation.
use crate::tables::ASTERDEX_ENDPOINTS;
use safe_cex_num::{JsCoercionError, js_number, js_trim};
use serde_json::{Value, json};

pub fn get_base_url(testnet: bool, base_url: Option<&str>) -> String {
    if let Some(base) = base_url
        .filter(|s| !s.is_empty())
        .map(js_trim)
        .filter(|s| !s.is_empty())
    {
        return base.strip_suffix('/').unwrap_or(base).to_owned();
    }
    let key = if testnet {
        "REST.TESTNET"
    } else {
        "REST.PRODUCTION"
    };
    ASTERDEX_ENDPOINTS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| (*v).to_owned())
        .unwrap_or_default()
}

pub fn extract_builder_rows(payload: Option<&Value>) -> Vec<Value> {
    match payload {
        Some(Value::Array(rows)) => rows.clone(),
        Some(value) => value
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

pub fn parse_fee_rate(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let parsed = js_number(value)?;
    Ok(parsed.is_finite().then_some(parsed))
}

pub fn get_provider_error_code(error: Option<&Value>) -> Option<f64> {
    error
        .filter(|e| e.is_object() || e.is_array())
        .and_then(|e| e.get("code"))
        .and_then(Value::as_f64)
}

/// `error_instance_message` preserves a host Error message before JSON erases it.
pub fn get_provider_error_message(
    error: Option<&Value>,
    error_instance_message: Option<&str>,
    fallback: &str,
) -> String {
    if let Some(message) = error_instance_message {
        return message.to_owned();
    }
    if let Some(Value::String(message)) = error
        && !js_trim(message).is_empty()
    {
        return message.clone();
    }
    error
        .filter(|e| e.is_object() || e.is_array())
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

pub fn build_aster_approve_builder_typed_data(
    builder_address: &str,
    builder_name: Option<&str>,
    max_fee_rate: &str,
    nonce: &str,
    testnet: bool,
    user_address: &str,
) -> Value {
    json!({
        "domain":{"name":"AsterSignTransaction","version":"1","chainId":56,"verifyingContract":"0x0000000000000000000000000000000000000000"},
        "types":{
            "EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"},{"name":"chainId","type":"uint256"},{"name":"verifyingContract","type":"address"}],
            "ApproveBuilder":[{"name":"Builder","type":"string"},{"name":"MaxFeeRate","type":"string"},{"name":"BuilderName","type":"string"},{"name":"AsterChain","type":"string"},{"name":"User","type":"string"},{"name":"Nonce","type":"uint256"}]
        },
        "primaryType":"ApproveBuilder",
        "message":{"Builder":builder_address,"MaxFeeRate":max_fee_rate,"BuilderName":builder_name.unwrap_or(""),"AsterChain":crate::api_helpers::get_aster_dex_aster_chain(testnet),"User":user_address,"Nonce":nonce}
    })
}

fn present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterBuilderFeeConfig {
    #[serde(rename = "builderAddress")]
    pub builder_address: String,
    #[serde(rename = "feeRateMaker")]
    pub fee_rate_maker: String,
    #[serde(rename = "feeRateTaker")]
    pub fee_rate_taker: String,
}
#[derive(serde::Deserialize)]
struct AsterBuilderFeeConfigWire {
    #[serde(rename = "builderAddress")]
    builder_address: String,
    #[serde(rename = "feeRateMaker")]
    fee_rate_maker: String,
    #[serde(rename = "feeRateTaker")]
    fee_rate_taker: String,
}

impl<'de> serde::Deserialize<'de> for AsterBuilderFeeConfig {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterBuilderFeeConfigWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            builder_address: w.builder_address,
            fee_rate_maker: w.fee_rate_maker,
            fee_rate_taker: w.fee_rate_taker,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterBuilderApprovalCheckResult {
    #[serde(rename = "approved")]
    pub approved: bool,
    #[serde(rename = "approvalStatus")]
    pub approval_status: AsterBuilderApprovalStatus,
    #[serde(rename = "builderAddress")]
    pub builder_address: String,
    #[serde(rename = "error")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub error: Option<String>,
    #[serde(rename = "maxFeeRate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_fee_rate: Option<String>,
}
#[derive(serde::Deserialize)]
struct AsterBuilderApprovalCheckResultWire {
    #[serde(rename = "approved")]
    approved: bool,
    #[serde(rename = "approvalStatus")]
    approval_status: AsterBuilderApprovalStatus,
    #[serde(rename = "builderAddress")]
    builder_address: String,
    #[serde(rename = "error")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    error: Option<String>,
    #[serde(rename = "maxFeeRate")]
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    max_fee_rate: Option<String>,
}

impl<'de> serde::Deserialize<'de> for AsterBuilderApprovalCheckResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterBuilderApprovalCheckResultWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            approved: w.approved,
            approval_status: w.approval_status,
            builder_address: w.builder_address,
            error: w.error,
            max_fee_rate: w.max_fee_rate,
        })
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct AsterBuilderApprovalResult {
    #[serde(rename = "message")]
    pub message: String,
    #[serde(rename = "success")]
    pub success: bool,
}
#[derive(serde::Deserialize)]
struct AsterBuilderApprovalResultWire {
    #[serde(rename = "message")]
    message: String,
    #[serde(rename = "success")]
    success: bool,
}

impl<'de> serde::Deserialize<'de> for AsterBuilderApprovalResult {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = <AsterBuilderApprovalResultWire as serde::Deserialize>::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        Ok(Self {
            message: w.message,
            success: w.success,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum AsterBuilderApprovalStatus {
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "insufficient_fee_cap")]
    InsufficientFeeCap,
    #[serde(rename = "unverified")]
    Unverified,
}
#[derive(serde::Deserialize)]
enum AsterBuilderApprovalStatusWire {
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "missing")]
    Missing,
    #[serde(rename = "insufficient_fee_cap")]
    InsufficientFeeCap,
    #[serde(rename = "unverified")]
    Unverified,
}

impl<'de> serde::Deserialize<'de> for AsterBuilderApprovalStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(d)?;
        let wire = <AsterBuilderApprovalStatusWire as serde::Deserialize>::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(text),
        )?;
        Ok(match wire {
            AsterBuilderApprovalStatusWire::Approved => Self::Approved,
            AsterBuilderApprovalStatusWire::Missing => Self::Missing,
            AsterBuilderApprovalStatusWire::InsufficientFeeCap => Self::InsufficientFeeCap,
            AsterBuilderApprovalStatusWire::Unverified => Self::Unverified,
        })
    }
}

pub const TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET: &str =
    "0x79a52a6658c23cF7e7Fe4fE43Ff91c5195Eb86Ca";
pub const TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET: &str = "REPLACE_WITH_TESTNET_BUILDER_ADDRESS";
pub const DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER: &str = "0.0001";
pub const DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER: &str = "0.0001";
pub const ASTER_BUILDER_FEE_RATE_CAP: &str = "0.001";

pub fn is_aster_maker_only_order(
    opts: impl Into<safe_cex_types::PreparedOrderOpts>,
    post_only: Option<bool>,
    api_order_type: &str,
) -> bool {
    let opts = opts.into();
    opts.r#type == safe_cex_types::OrderType::Limit
        && matches!(api_order_type, "LIMIT" | "STOP" | "TAKE_PROFIT")
        && (opts.time_in_force == Some(safe_cex_types::OrderTimeInForce::PostOnly)
            || post_only == Some(true))
}
pub fn get_aster_builder_fee_rate_for_order<'a>(
    opts: impl Into<safe_cex_types::PreparedOrderOpts>,
    post_only: Option<bool>,
    api_order_type: &str,
    config: &'a AsterBuilderFeeConfig,
) -> Option<&'a str> {
    let rate = if is_aster_maker_only_order(opts, post_only, api_order_type) {
        &config.fee_rate_maker
    } else {
        &config.fee_rate_taker
    };
    (rate != "0").then_some(rate.as_str())
}

// ts: parseScaledDecimal
pub fn parse_scaled_decimal(value: &str) -> Option<num_bigint::BigInt> {
    let (integer, fraction) = value.split_once('.').unwrap_or((value, ""));
    if integer.is_empty()
        || !integer.bytes().all(|c| c.is_ascii_digit())
        || (integer.len() > 1 && integer.starts_with('0'))
        || (value.contains('.') && fraction.is_empty())
        || fraction.len() > 18
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    // Both inputs are validated decimal digits; the shared BigInt conversion cannot throw.
    let integer = safe_cex_num::js_bigint(Some(&Value::String(integer.to_owned()))).ok()?;
    let fractional = format!("{fraction:0<18}");
    let fractional = safe_cex_num::js_bigint(Some(&Value::String(fractional))).ok()?;
    Some(integer * 1_000_000_000_000_000_000_u64 + fractional)
}

// ts: normalizeDecimalString
pub fn normalize_decimal_string(value: &str) -> Option<String> {
    let trimmed = js_trim(value);
    let scaled = parse_scaled_decimal(trimmed)?;
    if scaled == num_bigint::BigInt::from(0) {
        return Some("0".into());
    }
    let (integer, fraction) = trimmed.split_once('.').unwrap_or((trimmed, ""));
    let fraction = fraction.trim_end_matches('0');
    Some(if fraction.is_empty() {
        integer.to_owned()
    } else {
        format!("{integer}.{fraction}")
    })
}

// ts: assertFeeRate
pub fn assert_fee_rate(value: &str, field_name: &str) -> Result<String, String> {
    let normalized = normalize_decimal_string(value)
        .ok_or_else(|| format!("Invalid Aster builder {field_name}: expected a decimal string"))?;
    let scaled = parse_scaled_decimal(&normalized);
    let cap = parse_scaled_decimal(ASTER_BUILDER_FEE_RATE_CAP);
    if scaled.is_none() || cap.is_none() || scaled > cap {
        return Err(format!(
            "Invalid Aster builder {field_name}: maximum supported fee rate is {ASTER_BUILDER_FEE_RATE_CAP}"
        ));
    }
    Ok(normalized)
}

// ts: getAsterBuilderApprovalFeeRate
pub fn get_aster_builder_approval_fee_rate(config: &AsterBuilderFeeConfig) -> &str {
    let maker = parse_scaled_decimal(&config.fee_rate_maker).unwrap_or_default();
    let taker = parse_scaled_decimal(&config.fee_rate_taker).unwrap_or_default();
    if maker >= taker {
        &config.fee_rate_maker
    } else {
        &config.fee_rate_taker
    }
}

// ts: buildAsterApprovalRequest
pub async fn build_aster_approval_request(
    target_url: &str,
    base_url: Option<&str>,
    registry: &safe_cex_transport::TransportRegistry,
) -> Result<(std::collections::BTreeMap<String, String>, String), safe_cex_transport::TransportError>
{
    if base_url.is_some_and(|base| !base.is_empty()) {
        return Ok((Default::default(), target_url.to_owned()));
    }
    let proxy = registry
        .get_transport(None)
        .private_proxy(&safe_cex_types::ExchangeName::Asterdex);
    let Some(proxy) = proxy else {
        return Ok((Default::default(), target_url.to_owned()));
    };
    // No venue state is held across this await; dropping this caller owns no shared cleanup.
    let headers = proxy.resolve_headers().await?;
    if headers.is_empty() {
        return Ok((headers, target_url.to_owned()));
    }
    Ok((headers, format!("{}/{}", proxy.url(), target_url)))
}

// ts: resolveBuilderAddress
pub fn resolve_builder_address(
    options: &safe_cex_types::ExchangeOptions,
) -> Result<Option<String>, String> {
    let raw = options
        .extra
        .as_ref()
        .and_then(|extra| extra.aster_builder_address.as_deref())
        .filter(|address| !address.is_empty())
        .unwrap_or(if options.testnet.unwrap_or(false) {
            TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET
        } else {
            TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET
        });
    let trimmed = js_trim(raw);
    if trimmed.is_empty() || trimmed.starts_with("REPLACE_WITH_") {
        return Ok(None);
    }
    // ref-bug: ethers accepts oversized ICAP; retain the shared primitive's
    // spelling rather than imposing a venue-local 20-byte output restriction.
    let checksummed = safe_cex_crypto::ethereum::get_address(trimmed)
        .map_err(|_| "Invalid Aster builder address: expected an EVM address".to_owned())?;
    if checksummed == "0x0000000000000000000000000000000000000000" {
        return Err("Invalid Aster builder address: zero address is not allowed".into());
    }
    Ok(Some(checksummed))
}

// ts: getAsterBuilderApprovalConfig
pub fn get_aster_builder_approval_config(
    options: &safe_cex_types::ExchangeOptions,
) -> Result<Option<Value>, String> {
    let Some(config) = resolve_aster_builder_fee_config(options)? else {
        return Ok(None);
    };
    let max_fee_rate = get_aster_builder_approval_fee_rate(&config);
    Ok(Some(json!({
        "builderAddress": config.builder_address,
        "feeRateMaker": config.fee_rate_maker,
        "feeRateTaker": config.fee_rate_taker,
        "maxFeeRate": max_fee_rate,
    })))
}

// ts: resolveAsterBuilderFeeConfig
pub fn resolve_aster_builder_fee_config(
    options: &safe_cex_types::ExchangeOptions,
) -> Result<Option<AsterBuilderFeeConfig>, String> {
    let extra = options.extra.as_ref();
    let fee_rate_maker = assert_fee_rate(
        extra
            .and_then(|e| e.aster_builder_fee_rate_maker.as_deref())
            .unwrap_or(DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER),
        "maker fee rate",
    )?;
    let fee_rate_taker = assert_fee_rate(
        extra
            .and_then(|e| e.aster_builder_fee_rate_taker.as_deref())
            .unwrap_or(DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER),
        "taker fee rate",
    )?;
    if fee_rate_maker == "0" && fee_rate_taker == "0" {
        return Ok(None);
    }
    let Some(builder_address) = resolve_builder_address(options)? else {
        return Ok(None);
    };
    Ok(Some(AsterBuilderFeeConfig {
        builder_address,
        fee_rate_maker,
        fee_rate_taker,
    }))
}
