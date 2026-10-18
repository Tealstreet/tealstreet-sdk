//! Inert UTA detection; this module never enables or owns a trading transport.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// ts: KUCOIN_UTA_ACCOUNT_MODE_ROUTE
pub const KUCOIN_UTA_ACCOUNT_MODE_ROUTE: &[(&str, &str)] = &[
    ("baseUrl", "https://api.kucoin.com"),
    ("method", "GET"),
    ("path", "/api/ua/v1/account/mode"),
];
// ts: KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE
pub const KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE: &str = "200000";
// ts: KUCOIN_UTA_ACCOUNT_MODES
pub const KUCOIN_UTA_ACCOUNT_MODES: &[&str] = &["CLASSIC", "UNIFIED"];
// ts: KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED
pub const KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED: bool = false;

// ts: KucoinAccountMode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KucoinAccountMode {
    Classic,
    Unified,
}
// Private Serde companion; the public entry retains its admission checks.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "KucoinAccountMode")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum KucoinAccountModeWire {
    Classic,
    Unified,
}

impl<'de> Deserialize<'de> for KucoinAccountMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        KucoinAccountModeWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            text,
        ))
    }
}

// ts: KucoinAccountModeDetection
#[derive(Debug, Clone, PartialEq)]
pub enum KucoinAccountModeDetection {
    Identified {
        account_mode: KucoinAccountMode,
        classic_sub_account: Vec<f64>,
        unified_sub_account: Vec<f64>,
    },
    Unknown,
    InvalidEnvelope,
    InvalidSuccessCode,
    InvalidData,
    InvalidSelfAccountMode,
}
impl Serialize for KucoinAccountModeDetection {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        // Serialize native numeric events directly. A json! intermediate erases
        // infinities before the shared ingress output check can observe them.
        let (status, reason) = match self {
            Self::Identified {
                account_mode,
                classic_sub_account,
                unified_sub_account,
            } => {
                let mut record = s.serialize_struct("KucoinAccountModeDetection", 4)?;
                record.serialize_field("status", "identified")?;
                record.serialize_field("accountMode", account_mode)?;
                record.serialize_field("classicSubAccount", classic_sub_account)?;
                record.serialize_field("unifiedSubAccount", unified_sub_account)?;
                return record.end();
            }
            Self::Unknown => ("unknown", "unknown-self-account-mode"),
            Self::InvalidEnvelope => ("malformed", "invalid-envelope"),
            Self::InvalidSuccessCode => ("malformed", "invalid-success-code"),
            Self::InvalidData => ("malformed", "invalid-data"),
            Self::InvalidSelfAccountMode => ("malformed", "invalid-self-account-mode"),
        };
        let mut record = s.serialize_struct("KucoinAccountModeDetection", 2)?;
        record.serialize_field("status", status)?;
        record.serialize_field("reason", reason)?;
        record.end()
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KucoinAccountMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Classic, Unified });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for KucoinAccountModeDetection {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {
            Identified { account_mode, classic_sub_account, unified_sub_account },
            Unknown, InvalidEnvelope, InvalidSuccessCode, InvalidData, InvalidSelfAccountMode
        });
    }
}
// Private Serde companion; the public entry retains its admission checks.
#[derive(Deserialize)]
#[serde(remote = "KucoinAccountModeDetectionWire")]
#[serde(tag = "status")]
enum KucoinAccountModeDetectionWire {
    #[serde(rename = "identified")]
    Identified {
        #[serde(rename = "accountMode")]
        account_mode: KucoinAccountMode,
        #[serde(rename = "classicSubAccount")]
        classic_sub_account: Vec<f64>,
        #[serde(rename = "unifiedSubAccount")]
        unified_sub_account: Vec<f64>,
    },
    #[serde(rename = "unknown")]
    Unknown { reason: String },
    #[serde(rename = "malformed")]
    Malformed { reason: String },
}

impl<'de> Deserialize<'de> for KucoinAccountModeDetection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                match KucoinAccountModeDetectionWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )? {
                    KucoinAccountModeDetectionWire::Identified {
                        account_mode,
                        classic_sub_account,
                        unified_sub_account,
                    } => Ok(Self::Identified {
                        account_mode,
                        classic_sub_account,
                        unified_sub_account,
                    }),
                    KucoinAccountModeDetectionWire::Unknown { reason }
                        if reason == "unknown-self-account-mode" =>
                    {
                        Ok(Self::Unknown)
                    }
                    KucoinAccountModeDetectionWire::Malformed { reason } => match reason.as_str() {
                        "invalid-envelope" => Ok(Self::InvalidEnvelope),
                        "invalid-success-code" => Ok(Self::InvalidSuccessCode),
                        "invalid-data" => Ok(Self::InvalidData),
                        "invalid-self-account-mode" => Ok(Self::InvalidSelfAccountMode),
                        _ => Err(serde::de::Error::custom("invalid malformed reason")),
                    },
                    _ => Err(serde::de::Error::custom("invalid unknown reason")),
                }
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: KucoinClassicRuntimeDecision
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KucoinClassicRuntimeDecision {
    Classic,
    Unified,
    Unknown,
    Malformed,
}
impl Serialize for KucoinClassicRuntimeDecision {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let (eligible, reason, route) = match self {
            Self::Classic => (
                true,
                "classic-account-mode-confirmed",
                "existing-classic-runtime",
            ),
            Self::Unified => (false, "unified-account-mode-unsupported", "blocked"),
            Self::Unknown => (false, "account-mode-unknown", "blocked"),
            Self::Malformed => (false, "account-mode-response-malformed", "blocked"),
        };
        json!({"eligible":eligible,"reason":reason,"route":route,"utaRuntimeActivationEnabled":KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED}).serialize(s)
    }
}
// Private Serde companion; the public entry retains its admission checks.
#[derive(Deserialize)]
#[serde(remote = "KucoinClassicRuntimeDecisionWire")]
#[serde(rename_all = "camelCase")]
struct KucoinClassicRuntimeDecisionWire {
    eligible: bool,
    reason: String,
    route: String,
    uta_runtime_activation_enabled: bool,
}

impl<'de> Deserialize<'de> for KucoinClassicRuntimeDecision {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wire = KucoinClassicRuntimeDecisionWire::deserialize(
            safe_cex_types::strict_decode::object(d),
        )?;
        if !wire.uta_runtime_activation_enabled {
            match (wire.eligible, wire.reason.as_str(), wire.route.as_str()) {
                (true, "classic-account-mode-confirmed", "existing-classic-runtime") => {
                    return Ok(Self::Classic);
                }
                (false, "unified-account-mode-unsupported", "blocked") => return Ok(Self::Unified),
                (false, "account-mode-unknown", "blocked") => return Ok(Self::Unknown),
                (false, "account-mode-response-malformed", "blocked") => {
                    return Ok(Self::Malformed);
                }
                _ => {}
            }
        }
        Err(serde::de::Error::custom("invalid Classic runtime decision"))
    }
}

// ts: isPlainRecord
fn is_plain_record(value: &Value) -> bool {
    value.is_object()
}
// ts: hasExactEnumerableDataKeys
fn has_exact_enumerable_data_keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|map| {
        map.len() == expected.len() && expected.iter().all(|key| map.contains_key(*key))
    })
}
// ts: decodeSubaccountIds
fn decode_subaccount_ids(value: &Value) -> Option<Vec<f64>> {
    value
        .as_array()?
        .iter()
        .map(|id| {
            let id = id.as_f64()?;
            (id > 0.0 && id <= 9_007_199_254_740_991.0 && id.fract() == 0.0).then_some(id)
        })
        .collect()
}
// ts: decodeKucoinAccountMode
pub fn decode_kucoin_account_mode(input: &Value) -> KucoinAccountModeDetection {
    use KucoinAccountModeDetection as D;
    if !is_plain_record(input) || !has_exact_enumerable_data_keys(input, &["code", "data"]) {
        return D::InvalidEnvelope;
    }
    if input["code"] != KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE {
        return D::InvalidSuccessCode;
    }
    let data = &input["data"];
    if !is_plain_record(data)
        || !has_exact_enumerable_data_keys(
            data,
            &["selfAccountMode", "unifiedSubAccount", "classicSubAccount"],
        )
    {
        return D::InvalidData;
    }
    let Some(mode) = data["selfAccountMode"]
        .as_str()
        .filter(|mode| !mode.is_empty())
    else {
        return D::InvalidSelfAccountMode;
    };
    let Some(unified_sub_account) = decode_subaccount_ids(&data["unifiedSubAccount"]) else {
        return D::InvalidData;
    };
    let Some(classic_sub_account) = decode_subaccount_ids(&data["classicSubAccount"]) else {
        return D::InvalidData;
    };
    let account_mode = match mode {
        "CLASSIC" => KucoinAccountMode::Classic,
        "UNIFIED" => KucoinAccountMode::Unified,
        _ => return D::Unknown,
    };
    D::Identified {
        account_mode,
        classic_sub_account,
        unified_sub_account,
    }
}
// ts: decideKucoinClassicRuntime
pub fn decide_kucoin_classic_runtime(
    detection: &KucoinAccountModeDetection,
) -> KucoinClassicRuntimeDecision {
    use KucoinAccountModeDetection as D;
    match detection {
        D::Identified {
            account_mode: KucoinAccountMode::Classic,
            ..
        } => KucoinClassicRuntimeDecision::Classic,
        D::Identified { .. } => KucoinClassicRuntimeDecision::Unified,
        D::Unknown => KucoinClassicRuntimeDecision::Unknown,
        _ => KucoinClassicRuntimeDecision::Malformed,
    }
}
