//! Inert futures leverage descriptors and strict response decoding; no live route imports this module.
use crate::v3_contracts::{WooV3Endpoint, WooV3HttpMethod};
use crate::v3_request::WooV3RequestInput;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

// ts: WooV3AccountPositionMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3AccountPositionMode {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3AccountPositionMode")]
enum AcceptedWooV3AccountPositionModeWire {
    #[serde(rename = "ONE_WAY")]
    OneWay,
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode,
}
impl<'de> Deserialize<'de> for WooV3AccountPositionMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3AccountPositionModeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountPositionMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{OneWay,HedgeMode});
    }
}
// ts: WooV3FuturesMarginMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3FuturesMarginMode {
    #[serde(rename = "CROSS")]
    Cross,
    #[serde(rename = "ISOLATED")]
    Isolated,
}

#[derive(Deserialize)]
#[serde(remote = "WooV3FuturesMarginMode")]
enum AcceptedWooV3FuturesMarginModeWire {
    #[serde(rename = "CROSS")]
    Cross,
    #[serde(rename = "ISOLATED")]
    Isolated,
}
impl<'de> Deserialize<'de> for WooV3FuturesMarginMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        AcceptedWooV3FuturesMarginModeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3FuturesMarginMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{Cross,Isolated});
    }
}
fn provenance_source<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if s == "decoded-v3-account-info" {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("unsupported provenance source"))
    }
}
fn account_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "MARGIN" | "SPOT") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("unsupported account mode"))
    }
}
fn detail_side<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let s = String::deserialize(d)?;
    if matches!(s.as_str(), "BOTH" | "LONG" | "SHORT") {
        Ok(s)
    } else {
        Err(serde::de::Error::custom("unsupported position side"))
    }
}

// ts: WooV3AccountModeProvenance
#[derive(Debug, Clone)]
pub struct WooV3AccountModeProvenance {
    pub source: String,
    pub reconciliation_id: String,
    pub account_mode: String,
    pub position_mode: WooV3AccountPositionMode,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AccountModeProvenance")]
struct WooV3AccountModeProvenanceFields {
    #[serde(rename = "source", deserialize_with = "provenance_source")]
    source: String,
    #[serde(rename = "reconciliationId")]
    reconciliation_id: String,
    #[serde(rename = "accountMode", deserialize_with = "account_mode")]
    account_mode: String,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
}
impl Serialize for WooV3AccountModeProvenance {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3AccountModeProvenanceFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3AccountModeProvenance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3AccountModeProvenanceFields::deserialize(
                    safe_cex_types::strict_decode::object(d),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AccountModeProvenance {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{source,reconciliation_id,account_mode,position_mode});
    }
}

// ts: WooV3SetLeverageInput
#[derive(Debug, Clone)]
pub struct WooV3SetLeverageInput {
    pub symbol: String,
    pub margin_mode: WooV3FuturesMarginMode,
    pub leverage: f64,
    pub account: WooV3AccountModeProvenance,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3SetLeverageInput")]
struct WooV3SetLeverageInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "marginMode")]
    margin_mode: WooV3FuturesMarginMode,
    #[serde(rename = "leverage")]
    leverage: f64,
    #[serde(rename = "account")]
    account: WooV3AccountModeProvenance,
}
impl Serialize for WooV3SetLeverageInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3SetLeverageInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3SetLeverageInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3SetLeverageInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3SetLeverageInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,margin_mode,leverage,account});
    }
}

// ts: WooV3GetLeverageInput
#[derive(Debug, Clone)]
pub struct WooV3GetLeverageInput {
    pub symbol: String,
    pub margin_mode: WooV3FuturesMarginMode,
    pub position_mode: WooV3AccountPositionMode,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3GetLeverageInput")]
struct WooV3GetLeverageInputFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "marginMode")]
    margin_mode: WooV3FuturesMarginMode,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
}
impl Serialize for WooV3GetLeverageInput {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3GetLeverageInputFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3GetLeverageInput {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3GetLeverageInputFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3GetLeverageInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,margin_mode,position_mode});
    }
}

// ts: WooV3GetLeverageRequest
#[derive(Debug, Clone)]
pub struct WooV3GetLeverageRequest {
    pub request: WooV3RequestInput,
    pub target: WooV3GetLeverageInput,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3GetLeverageRequest")]
struct WooV3GetLeverageRequestFields {
    #[serde(rename = "request")]
    request: WooV3RequestInput,
    #[serde(rename = "target")]
    target: WooV3GetLeverageInput,
}
impl Serialize for WooV3GetLeverageRequest {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3GetLeverageRequestFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3GetLeverageRequest {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3GetLeverageRequestFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3GetLeverageRequest {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{request,target});
    }
}

// ts: WooV3LeverageTarget
#[derive(Debug, Clone)]
pub struct WooV3LeverageTarget {
    pub symbol: String,
    pub margin_mode: WooV3FuturesMarginMode,
    pub position_mode: WooV3AccountPositionMode,
    pub leverage: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3LeverageTarget")]
struct WooV3LeverageTargetFields {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "marginMode")]
    margin_mode: WooV3FuturesMarginMode,
    #[serde(rename = "positionMode")]
    position_mode: WooV3AccountPositionMode,
    #[serde(rename = "leverage")]
    leverage: f64,
}
impl Serialize for WooV3LeverageTarget {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3LeverageTargetFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3LeverageTarget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3LeverageTargetFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3LeverageTarget {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{symbol,margin_mode,position_mode,leverage});
    }
}

// ts: WooV3LeverageDetail
#[derive(Debug, Clone)]
pub struct WooV3LeverageDetail {
    pub position_side: String,
    pub leverage: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3LeverageDetail")]
struct WooV3LeverageDetailFields {
    #[serde(rename = "positionSide", deserialize_with = "detail_side")]
    position_side: String,
    #[serde(rename = "leverage")]
    leverage: f64,
}
impl Serialize for WooV3LeverageDetail {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3LeverageDetailFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3LeverageDetail {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3LeverageDetailFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3LeverageDetail {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct{position_side,leverage});
    }
}

// ts: WooV3LeverageReadback
#[derive(Debug, Clone)]
pub enum WooV3LeverageReadback {
    OneWay {
        symbol: String,
        margin_mode: WooV3FuturesMarginMode,
        details: [WooV3LeverageDetail; 1],
        timestamp: f64,
    },
    HedgeMode {
        symbol: String,
        margin_mode: WooV3FuturesMarginMode,
        details: [WooV3LeverageDetail; 2],
        timestamp: f64,
    },
}
fn one_way_details<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<[WooV3LeverageDetail; 1], D::Error> {
    let rows = <[WooV3LeverageDetail; 1]>::deserialize(d)?;
    if rows[0].position_side == "BOTH" {
        Ok(rows)
    } else {
        Err(serde::de::Error::custom("one-way detail must be BOTH"))
    }
}
fn hedge_details<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<[WooV3LeverageDetail; 2], D::Error> {
    let rows = <[WooV3LeverageDetail; 2]>::deserialize(d)?;
    if rows[0].position_side == "LONG" && rows[1].position_side == "SHORT" {
        Ok(rows)
    } else {
        Err(serde::de::Error::custom(
            "hedge details must be LONG then SHORT",
        ))
    }
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3LeverageReadback", tag = "positionMode")]
enum WooV3LeverageReadbackFields {
    #[serde(rename = "ONE_WAY")]
    OneWay {
        symbol: String,
        #[serde(rename = "marginMode")]
        margin_mode: WooV3FuturesMarginMode,
        #[serde(deserialize_with = "one_way_details")]
        details: [WooV3LeverageDetail; 1],
        timestamp: f64,
    },
    #[serde(rename = "HEDGE_MODE")]
    HedgeMode {
        symbol: String,
        #[serde(rename = "marginMode")]
        margin_mode: WooV3FuturesMarginMode,
        #[serde(deserialize_with = "hedge_details")]
        details: [WooV3LeverageDetail; 2],
        timestamp: f64,
    },
}
impl Serialize for WooV3LeverageReadback {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WooV3LeverageReadbackFields::serialize(self, s)
    }
}
impl<'de> Deserialize<'de> for WooV3LeverageReadback {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3LeverageReadbackFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3LeverageReadback {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,enum{OneWay{symbol,margin_mode,details,timestamp},HedgeMode{symbol,margin_mode,details,timestamp}});
    }
}

// ts: WooV3SetLeverageBuildResult
#[derive(Debug, Clone)]
pub enum WooV3SetLeverageBuildResult {
    Ready {
        request: WooV3RequestInput,
        target: WooV3LeverageTarget,
        scope_evidence: String,
    },
    Blocked {
        blocker: String,
        target: WooV3LeverageTarget,
    },
}
// Build results are native operation outputs, with literal combinations enforced by the builder.
// ts: WooV3AccountMutationResult
#[derive(Debug, Clone)]
pub struct WooV3AccountMutationResult {
    pub accepted: bool,
    pub timestamp: f64,
    pub target: WooV3AccountMutationTarget,
    pub state_evidence: String,
}
// Supporting anonymous target union for WooV3AccountMutationResult.
#[derive(Debug, Clone)]
pub enum WooV3AccountMutationTarget {
    FuturesLeverage(WooV3LeverageTarget),
    PositionMode {
        position_mode: WooV3AccountPositionMode,
    },
}

// ts: WooV3AccountConfigResponseError
#[derive(Debug, Clone)]
pub struct WooV3AccountConfigResponseError {
    pub message: String,
    pub name: String,
    pub code: Option<f64>,
    pub gateway_response: JsObjectMap<JsNumberInput>,
    pub contract_status: String,
}
impl WooV3AccountConfigResponseError {
    // ts: constructor
    pub fn new(
        message: String,
        gateway_response: &JsObjectMap<JsNumberInput>,
        code: Option<f64>,
    ) -> Self {
        Self {
            message,
            name: "WooV3AccountConfigResponseError".into(),
            code,
            gateway_response: gateway_response.clone(),
            contract_status: "vendor-error-envelope-undocumented".into(),
        }
    }
}
// Native exception transport distinguishes the concrete gateway class from validation Errors.
#[derive(Debug, Clone)]
pub enum WooV3AccountWriteError {
    Validation(String),
    Gateway(WooV3AccountConfigResponseError),
}
impl From<String> for WooV3AccountWriteError {
    fn from(value: String) -> Self {
        Self::Validation(value)
    }
}

// Supporting fixed metadata record for WOO_V3_ACCOUNT_WRITE_PARITY.
#[derive(Debug, Clone, Copy)]
pub struct WooV3AccountWriteParity {
    pub request_and_response_fixtures: bool,
    pub runtime_imported: bool,
    pub credentialed_demo_lifecycle: bool,
    pub readback_required: bool,
    pub leverage_readback: &'static str,
    pub default_margin_mode: &'static str,
    pub cross_one_way_leverage: &'static str,
    pub cross_hedge_leverage: &'static str,
    pub isolated_leverage: &'static str,
    pub position_mode_preconditions: &'static str,
    pub mutation_response_state: &'static str,
}
// ts: WOO_V3_ACCOUNT_WRITE_PARITY
pub const WOO_V3_ACCOUNT_WRITE_PARITY: WooV3AccountWriteParity = WooV3AccountWriteParity {
    request_and_response_fixtures: true,
    runtime_imported: false,
    credentialed_demo_lifecycle: false,
    readback_required: true,
    leverage_readback: "strict-one-way-both-or-hedge-long-short",
    default_margin_mode: "excluded-official-response-unusable",
    cross_one_way_leverage: "descriptor-ready-request-state-only",
    cross_hedge_leverage: "blocked-resulting-leg-scope-requires-demo-evidence",
    isolated_leverage: "blocked-outside-safe-cex-cross-margin-contract",
    position_mode_preconditions: "complete-reconciled-v3-positions-zero-and-pending-orders-empty",
    mutation_response_state: "request-target-only",
};

// ts: requireRecord
pub fn require_record(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<JsObjectMap<JsNumberInput>, String> {
    value
        .and_then(|v| v.clone().decode().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
// ts: requireBoolean
pub fn require_boolean(value: Option<&JsNumberInput>, path: &str) -> Result<bool, String> {
    value
        .and_then(|v| v.clone().decode().ok())
        .ok_or_else(|| format!("{path} must be a boolean"))
}
// ts: requireInteger
pub fn require_integer(
    value: Option<&JsNumberInput>,
    path: &str,
    minimum: f64,
) -> Result<f64, String> {
    let n = value.and_then(|v| v.clone().decode::<f64>().ok());
    // A NaN minimum makes `n < minimum` false even for a finite n.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    n.filter(|n| {
        n.is_finite() && n.trunc() == *n && n.abs() <= 9_007_199_254_740_991.0 && !(*n < minimum)
    })
    .ok_or_else(|| {
        format!(
            "{path} must be a safe integer of at least {}",
            safe_cex_num::js_number_string(minimum)
        )
    })
}
// ts: requireString
pub fn require_string(value: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{path} must be a non-empty string"))
}
// ts: requireEnum
pub fn require_enum(
    value: Option<&JsNumberInput>,
    values: &[&str],
    path: &str,
) -> Result<String, String> {
    value
        .and_then(|v| v.clone().decode::<String>().ok())
        .filter(|s| values.contains(&s.as_str()))
        .ok_or_else(|| format!("{path} must be one of {}", values.join(", ")))
}
// ts: requireExactKeys
pub fn require_exact_keys(
    input: &JsObjectMap<JsNumberInput>,
    expected: &[&str],
    path: &str,
) -> Result<(), String> {
    let mut actual = input.keys().map(String::as_str).collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    let sort = |a: &&str, b: &&str| a.encode_utf16().cmp(b.encode_utf16());
    actual.sort_by(sort);
    expected.sort_by(sort);
    if actual != expected {
        return Err(format!(
            "{path} must contain exactly {}",
            expected.join(", ")
        ));
    }
    Ok(())
}
// ts: requirePerpetualSymbol
pub fn require_perpetual_symbol(symbol: &str) -> Result<String, String> {
    let parts = symbol.split('_').collect::<Vec<_>>();
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    if parts.len() != 3 || parts[0] != "PERP" || !valid(parts[1]) || !valid(parts[2]) {
        return Err("WOO V3 symbol must use PERP_BASE_QUOTE form".into());
    }
    Ok(symbol.into())
}
fn position_mode(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3AccountPositionMode, String> {
    match require_enum(value, &["ONE_WAY", "HEDGE_MODE"], path)?.as_str() {
        "ONE_WAY" => Ok(WooV3AccountPositionMode::OneWay),
        _ => Ok(WooV3AccountPositionMode::HedgeMode),
    }
}
fn margin_mode(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3FuturesMarginMode, String> {
    match require_enum(value, &["CROSS", "ISOLATED"], path)?.as_str() {
        "CROSS" => Ok(WooV3FuturesMarginMode::Cross),
        _ => Ok(WooV3FuturesMarginMode::Isolated),
    }
}
// Typed source symbols are string-only here; regex coercion of arrays/executable values is excluded.
fn input_symbol(value: Option<&JsNumberInput>) -> Result<String, String> {
    let text = value
        .and_then(|v| v.clone().decode::<String>().ok())
        .ok_or("WOO V3 symbol must use PERP_BASE_QUOTE form")?;
    require_perpetual_symbol(&text)
}
// ts: validateAccountProvenance
pub fn validate_account_provenance(
    input: Option<&JsNumberInput>,
) -> Result<WooV3AccountModeProvenance, String> {
    let row = require_record(input, "WOO V3 account-mode provenance")?;
    let source = require_enum(
        row.get("source"),
        &["decoded-v3-account-info"],
        "WOO V3 account-mode provenance.source",
    )?;
    Ok(WooV3AccountModeProvenance {
        source,
        reconciliation_id: require_string(
            row.get("reconciliationId"),
            "WOO V3 account-mode provenance.reconciliationId",
        )?,
        account_mode: require_enum(
            row.get("accountMode"),
            &["MARGIN", "SPOT"],
            "WOO V3 account-mode provenance.accountMode",
        )?,
        position_mode: position_mode(
            row.get("positionMode"),
            "WOO V3 account-mode provenance.positionMode",
        )?,
    })
}
fn mode_text(value: WooV3AccountPositionMode) -> &'static str {
    match value {
        WooV3AccountPositionMode::OneWay => "ONE_WAY",
        WooV3AccountPositionMode::HedgeMode => "HEDGE_MODE",
    }
}
fn margin_text(value: WooV3FuturesMarginMode) -> &'static str {
    match value {
        WooV3FuturesMarginMode::Cross => "CROSS",
        WooV3FuturesMarginMode::Isolated => "ISOLATED",
    }
}
fn captured_string(s: &str) -> JsNumberInput {
    serde_json::from_str(&serde_json::to_string(s).expect("string serialization"))
        .expect("string ingress")
}
fn request_target_query(target: &WooV3GetLeverageInput) -> JsObjectMap<JsNumberInput> {
    [
        ("symbol", captured_string(&target.symbol)),
        (
            "marginMode",
            captured_string(margin_text(target.margin_mode)),
        ),
        (
            "positionMode",
            captured_string(mode_text(target.position_mode)),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect()
}
// ts: buildWooV3GetLeverageRequest
pub fn build_woo_v3_get_leverage_request(
    input: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3GetLeverageRequest, String> {
    let target = WooV3GetLeverageInput {
        symbol: input_symbol(input.get("symbol"))?,
        margin_mode: margin_mode(input.get("marginMode"), "WOO V3 leverage read marginMode")?,
        position_mode: position_mode(
            input.get("positionMode"),
            "WOO V3 leverage read positionMode",
        )?,
    };
    Ok(WooV3GetLeverageRequest {
        request: WooV3RequestInput {
            method: WooV3HttpMethod::Get,
            path: WooV3Endpoint::FuturesLeverage,
            query: Some(request_target_query(&target)),
            body: None,
        },
        target,
    })
}
// ts: buildWooV3SetLeverageRequest
pub fn build_woo_v3_set_leverage_request(
    input: &JsObjectMap<JsNumberInput>,
) -> Result<WooV3SetLeverageBuildResult, String> {
    let symbol = input_symbol(input.get("symbol"))?;
    let margin_mode = margin_mode(
        input.get("marginMode"),
        "WOO V3 futures leverage marginMode",
    )?;
    let leverage = require_integer(input.get("leverage"), "WOO V3 futures leverage", 1.0)?;
    let account = validate_account_provenance(input.get("account"))?;
    let target = WooV3LeverageTarget {
        symbol,
        margin_mode,
        position_mode: account.position_mode,
        leverage,
    };
    let blocker = if account.account_mode != "MARGIN" {
        Some("futures-margin-account-mode-not-proven")
    } else if margin_mode == WooV3FuturesMarginMode::Isolated {
        Some("isolated-margin-outside-safe-cex-contract")
    } else if account.position_mode == WooV3AccountPositionMode::HedgeMode {
        Some("cross-hedge-resulting-leg-scope-requires-demo-evidence")
    } else {
        None
    };
    if let Some(blocker) = blocker {
        return Ok(WooV3SetLeverageBuildResult::Blocked {
            blocker: blocker.into(),
            target,
        });
    }
    let body = [
        ("symbol", captured_string(&target.symbol)),
        ("marginMode", captured_string("CROSS")),
        (
            "leverage",
            serde_json::from_str(&safe_cex_num::js_number_string(leverage))
                .expect("validated safe integer ingress"),
        ),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect();
    Ok(WooV3SetLeverageBuildResult::Ready {
        request: WooV3RequestInput {
            method: WooV3HttpMethod::Put,
            path: WooV3Endpoint::FuturesLeverage,
            query: None,
            body: Some(body),
        },
        target,
        scope_evidence: "official-cross-setter-ignores-positionMode-one-way-account-provenance"
            .into(),
    })
}
fn response_failure(
    response: &JsObjectMap<JsNumberInput>,
    name: &str,
) -> Result<WooV3AccountWriteError, String> {
    let code = response
        .get("code")
        .map(|v| require_integer(Some(v), &format!("WOO V3 {name}.code"), 0.0))
        .transpose()?;
    let message = match response.get("message") {
        None => format!("WOO V3 {name} failed with an undocumented error envelope"),
        Some(v) => require_string(Some(v), &format!("WOO V3 {name}.message"))?,
    };
    Ok(WooV3AccountWriteError::Gateway(
        WooV3AccountConfigResponseError::new(message, response, code),
    ))
}
// ts: decodeSuccess
pub fn decode_success(
    input: Option<&JsNumberInput>,
    name: &str,
) -> Result<f64, WooV3AccountWriteError> {
    let response = require_record(input, &format!("WOO V3 {name} response"))?;
    if !require_boolean(response.get("success"), &format!("WOO V3 {name}.success"))? {
        return Err(response_failure(&response, name)?);
    }
    require_exact_keys(
        &response,
        &["success", "timestamp"],
        &format!("WOO V3 {name} success response"),
    )?;
    Ok(require_integer(
        response.get("timestamp"),
        &format!("WOO V3 {name}.timestamp"),
        0.0,
    )?)
}
// ts: decodeWooV3GetLeverageResponse
pub fn decode_woo_v3_get_leverage_response(
    input: Option<&JsNumberInput>,
    build: &WooV3GetLeverageRequest,
) -> Result<WooV3LeverageReadback, WooV3AccountWriteError> {
    if build.request.method != WooV3HttpMethod::Get
        || build.request.path != WooV3Endpoint::FuturesLeverage
    {
        return Err(
            "WOO V3 futures leverage read response requires its matching request"
                .to_string()
                .into(),
        );
    }
    let response = require_record(input, "WOO V3 get futures leverage response")?;
    if !require_boolean(
        response.get("success"),
        "WOO V3 get futures leverage.success",
    )? {
        return Err(response_failure(&response, "get futures leverage")?);
    }
    require_exact_keys(
        &response,
        &["success", "data", "timestamp"],
        "WOO V3 get futures leverage success response",
    )?;
    let timestamp = require_integer(
        response.get("timestamp"),
        "WOO V3 get futures leverage.timestamp",
        0.0,
    )?;
    let data = require_record(response.get("data"), "WOO V3 get futures leverage.data")?;
    require_exact_keys(
        &data,
        &["symbol", "marginMode", "positionMode", "details"],
        "WOO V3 get futures leverage.data",
    )?;
    let symbol = require_perpetual_symbol(&require_string(
        data.get("symbol"),
        "WOO V3 get futures leverage.data.symbol",
    )?)?;
    let margin_mode = margin_mode(
        data.get("marginMode"),
        "WOO V3 get futures leverage.data.marginMode",
    )?;
    let position_mode = position_mode(
        data.get("positionMode"),
        "WOO V3 get futures leverage.data.positionMode",
    )?;
    if symbol != build.target.symbol
        || margin_mode != build.target.margin_mode
        || position_mode != build.target.position_mode
    {
        return Err(
            "WOO V3 get futures leverage response scope must match its request"
                .to_string()
                .into(),
        );
    }
    let details = data
        .get("details")
        .and_then(|v| v.clone().decode::<Vec<JsNumberInput>>().ok())
        .ok_or_else(|| "WOO V3 get futures leverage.data.details must be an array".to_string())?;
    let details = details
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let path = format!("WOO V3 get futures leverage.data.details[{i}]");
            let row = require_record(Some(v), &path)?;
            require_exact_keys(&row, &["positionSide", "leverage"], &path)?;
            Ok(WooV3LeverageDetail {
                position_side: require_enum(
                    row.get("positionSide"),
                    &["BOTH", "LONG", "SHORT"],
                    &format!("{path}.positionSide"),
                )?,
                leverage: require_integer(row.get("leverage"), &format!("{path}.leverage"), 1.0)?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    if position_mode == WooV3AccountPositionMode::OneWay {
        if details.len() != 1 || details[0].position_side != "BOTH" {
            return Err(
                "WOO V3 one-way leverage readback must contain exactly one unique BOTH detail"
                    .to_string()
                    .into(),
            );
        }
        return Ok(WooV3LeverageReadback::OneWay {
            symbol,
            margin_mode,
            details: [WooV3LeverageDetail {
                position_side: "BOTH".into(),
                leverage: details[0].leverage,
            }],
            timestamp,
        });
    }
    if details.len() != 2
        || details.iter().filter(|v| v.position_side == "LONG").count() != 1
        || details
            .iter()
            .filter(|v| v.position_side == "SHORT")
            .count()
            != 1
    {
        return Err(
            "WOO V3 hedge leverage readback must contain exactly one unique LONG and SHORT detail"
                .to_string()
                .into(),
        );
    }
    let long = details
        .iter()
        .find(|v| v.position_side == "LONG")
        .expect("validated LONG");
    let short = details
        .iter()
        .find(|v| v.position_side == "SHORT")
        .expect("validated SHORT");
    Ok(WooV3LeverageReadback::HedgeMode {
        symbol,
        margin_mode,
        details: [
            WooV3LeverageDetail {
                position_side: "LONG".into(),
                leverage: long.leverage,
            },
            WooV3LeverageDetail {
                position_side: "SHORT".into(),
                leverage: short.leverage,
            },
        ],
        timestamp,
    })
}
// ts: decodeWooV3SetLeverageResponse
pub fn decode_woo_v3_set_leverage_response(
    input: Option<&JsNumberInput>,
    request: &WooV3RequestInput,
    target: &WooV3LeverageTarget,
) -> Result<WooV3AccountMutationResult, WooV3AccountWriteError> {
    if request.method != WooV3HttpMethod::Put || request.path != WooV3Endpoint::FuturesLeverage {
        return Err(
            "WOO V3 futures leverage response requires its matching request"
                .to_string()
                .into(),
        );
    }
    Ok(WooV3AccountMutationResult {
        accepted: true,
        timestamp: decode_success(input, "set futures leverage")?,
        target: WooV3AccountMutationTarget::FuturesLeverage(target.clone()),
        state_evidence: "request-target-only-response-does-not-echo-resulting-state".into(),
    })
}
