//! Inert frame classification. JSON.parse values retain JS numbers, UTF-16 and object identity.
use crate::{
    json_key_scanner::{JsonDuplicateKeyScanner, JsonKeyScanResult, decode_json_string},
    starbase::{CoinbaseStarbaseHeartbeatSignal, CoinbaseStarbaseProtocolError},
    starbase_public::{
        CoinbaseStarbaseBookInterval, CoinbaseStarbaseChartResolution,
        CoinbaseStarbaseInstrumentStateCurrency, CoinbaseStarbasePublicChannel,
        is_supported_public_channel, validate_context,
    },
};
use indexmap::IndexMap;
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

/// Native data: primitive clones copy, object/array clones share identity.
/// Undefined is retained at native model boundaries and is never parsed from JSON.
#[derive(Debug, Clone)]
pub enum CoinbaseStarbaseFrameValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Array(Rc<RefCell<Vec<Self>>>),
    Object(Rc<RefCell<IndexMap<Vec<u16>, Self>>>),
}
impl CoinbaseStarbaseFrameValue {
    pub fn from_json(value: &Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(*value),
            Value::Number(value) => Self::Number(value.as_f64().expect("JSON number")),
            Value::String(value) => Self::String(value.encode_utf16().collect()),
            Value::Array(value) => Self::Array(Rc::new(RefCell::new(
                value.iter().map(Self::from_json).collect(),
            ))),
            Value::Object(value) => Self::Object(Rc::new(RefCell::new(
                value
                    .iter()
                    .map(|(key, value)| (key.encode_utf16().collect(), Self::from_json(value)))
                    .collect(),
            ))),
        }
    }
    pub fn get(&self, key: &str) -> Option<Self> {
        let Self::Object(value) = self else {
            return None;
        };
        value
            .borrow()
            .get(&key.encode_utf16().collect::<Vec<_>>())
            .cloned()
    }
    pub fn own_keys(&self) -> Vec<Vec<u16>> {
        let Self::Object(value) = self else {
            return Vec::new();
        };
        let mut indices = Vec::new();
        let mut strings = Vec::new();
        for key in value.borrow().keys() {
            let index = String::from_utf16(key).ok().and_then(|text| {
                let n = text.parse::<u32>().ok()?;
                (n < u32::MAX && n.to_string() == text).then_some(n)
            });
            if let Some(index) = index {
                indices.push((index, key.clone()));
            } else {
                strings.push(key.clone());
            }
        }
        indices.sort_by_key(|(index, _)| *index);
        indices
            .into_iter()
            .map(|(_, key)| key)
            .chain(strings)
            .collect()
    }
}
type FrameValue = CoinbaseStarbaseFrameValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinbaseStarbaseUnknownFrameReason {
    DuplicateJsonKey,
    InvalidJson,
    InvalidPublicTestResponse,
    InvalidPublicWebsocketEnvelope,
    UnsupportedRawFrame,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinbaseStarbaseUnsupportedSubscriptionReason {
    ChannelNotDesiredAcknowledgedPublicSubscription,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinbaseStarbaseFrameClassificationScope {
    Unclassified,
}
#[derive(Debug, Clone)]
pub struct CoinbaseStarbasePublicSubscriptionPayload {
    pub channel: CoinbaseStarbasePublicChannel,
    pub data: FrameValue,
}
#[derive(Debug, Clone)]
pub struct CoinbaseStarbasePublicTestError {
    pub code: f64,
    pub data: Option<FrameValue>,
    pub message: Vec<u16>,
}
#[derive(Debug, Clone)]
pub enum CoinbaseStarbasePublicTestOutcome {
    Result { version: Vec<u16> },
    Error(CoinbaseStarbasePublicTestError),
}
// ts: CoinbaseStarbasePublicFrameClassification
#[derive(Debug, Clone)]
pub enum CoinbaseStarbasePublicFrameClassification {
    PublicSubscription {
        channel: CoinbaseStarbasePublicChannel,
        payload: CoinbaseStarbasePublicSubscriptionPayload,
    },
    Heartbeat {
        signal: CoinbaseStarbaseHeartbeatSignal,
    },
    PublicTestResponse {
        id: f64,
        outcome: CoinbaseStarbasePublicTestOutcome,
    },
    UnsupportedSubscription {
        channel: Vec<u16>,
        reason: CoinbaseStarbaseUnsupportedSubscriptionReason,
        scope: CoinbaseStarbaseFrameClassificationScope,
    },
    Unknown {
        reason: CoinbaseStarbaseUnknownFrameReason,
    },
}
type Classification = CoinbaseStarbasePublicFrameClassification;
use CoinbaseStarbaseUnknownFrameReason as UnknownReason;
fn unknown(reason: UnknownReason) -> Classification {
    Classification::Unknown { reason }
}
fn string_eq(value: Option<FrameValue>, expected: &str) -> bool {
    matches!(value, Some(FrameValue::String(value)) if value.iter().copied().eq(expected.encode_utf16()))
}
fn string(value: Option<FrameValue>) -> Option<Vec<u16>> {
    match value {
        Some(FrameValue::String(value)) => Some(value),
        _ => None,
    }
}
fn number(value: Option<FrameValue>) -> Option<f64> {
    match value {
        Some(FrameValue::Number(value)) => Some(value),
        _ => None,
    }
}
fn safe_integer(value: Option<FrameValue>) -> Option<f64> {
    number(value)
        .filter(|n| n.is_finite() && n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0)
}
fn exact_keys(value: &FrameValue, keys: &[&str]) -> bool {
    let actual = value.own_keys();
    actual.len() == keys.len()
        && actual.iter().all(|key| {
            keys.iter()
                .any(|expected| key.iter().copied().eq(expected.encode_utf16()))
        })
}
fn supported_channel(raw: &[u16]) -> Option<CoinbaseStarbasePublicChannel> {
    let channel = String::from_utf16(raw).ok()?;
    if !is_supported_public_channel(&channel) {
        return None;
    }
    let parts: Vec<_> = channel.split('.').collect();
    match parts.as_slice() {
        [kind, instrument, interval] => {
            let interval = if *interval == "100ms" {
                CoinbaseStarbaseBookInterval::V100Ms
            } else {
                CoinbaseStarbaseBookInterval::Agg2
            };
            if *kind == "ticker" {
                Some(CoinbaseStarbasePublicChannel::Ticker {
                    instrument_name: (*instrument).into(),
                    interval,
                })
            } else {
                Some(CoinbaseStarbasePublicChannel::Book {
                    instrument_name: (*instrument).into(),
                    interval,
                })
            }
        }
        ["chart", "trades", instrument, resolution] => {
            let resolution = match *resolution {
                "1" => CoinbaseStarbaseChartResolution::V1,
                "3" => CoinbaseStarbaseChartResolution::V3,
                "5" => CoinbaseStarbaseChartResolution::V5,
                "15" => CoinbaseStarbaseChartResolution::V15,
                "30" => CoinbaseStarbaseChartResolution::V30,
                "60" => CoinbaseStarbaseChartResolution::V60,
                "120" => CoinbaseStarbaseChartResolution::V120,
                "360" => CoinbaseStarbaseChartResolution::V360,
                "720" => CoinbaseStarbaseChartResolution::V720,
                "1D" => CoinbaseStarbaseChartResolution::V1D,
                _ => unreachable!(),
            };
            Some(CoinbaseStarbasePublicChannel::Chart {
                instrument_name: (*instrument).into(),
                resolution,
            })
        }
        ["instrument", "state", "future", currency] => {
            Some(CoinbaseStarbasePublicChannel::InstrumentState {
                currency: match *currency {
                    "BTC" => CoinbaseStarbaseInstrumentStateCurrency::Btc,
                    "ETH" => CoinbaseStarbaseInstrumentStateCurrency::Eth,
                    "USDC" => CoinbaseStarbaseInstrumentStateCurrency::Usdc,
                    "any" => CoinbaseStarbaseInstrumentStateCurrency::Any,
                    _ => unreachable!(),
                },
            })
        }
        _ => unreachable!(),
    }
}

// ts: classifyNotification
pub fn classify_notification(
    frame: &FrameValue,
    acknowledged: &[Vec<u16>],
    desired: &[Vec<u16>],
) -> Classification {
    let invalid = || unknown(UnknownReason::InvalidPublicWebsocketEnvelope);
    if !exact_keys(frame, &["jsonrpc", "method", "params"])
        || !string_eq(frame.get("jsonrpc"), "2.0")
    {
        return invalid();
    }
    let Some(params @ FrameValue::Object(_)) = frame.get("params") else {
        return invalid();
    };
    if string_eq(frame.get("method"), "heartbeat") {
        if !exact_keys(&params, &["type"]) {
            return invalid();
        }
        let signal = if string_eq(params.get("type"), "heartbeat") {
            CoinbaseStarbaseHeartbeatSignal::Heartbeat
        } else if string_eq(params.get("type"), "test_request") {
            CoinbaseStarbaseHeartbeatSignal::TestRequest
        } else {
            return invalid();
        };
        return Classification::Heartbeat { signal };
    }
    if !string_eq(frame.get("method"), "subscription") || !exact_keys(&params, &["channel", "data"])
    {
        return invalid();
    }
    let Some(channel) = string(params.get("channel")).filter(|v| !v.is_empty()) else {
        return invalid();
    };
    let public_channel = supported_channel(&channel);
    if public_channel.is_none() || !desired.contains(&channel) || !acknowledged.contains(&channel) {
        return Classification::UnsupportedSubscription { channel, reason: CoinbaseStarbaseUnsupportedSubscriptionReason::ChannelNotDesiredAcknowledgedPublicSubscription, scope: CoinbaseStarbaseFrameClassificationScope::Unclassified };
    }
    let channel = public_channel.expect("supported public channel");
    Classification::PublicSubscription {
        payload: CoinbaseStarbasePublicSubscriptionPayload {
            channel: channel.clone(),
            data: params.get("data").expect("own data"),
        },
        channel,
    }
}

// Native overlay for the source parsePublicTestError body: same record/key/number
// validation, extending the existing finite-JSON API to UTF-16 and shared data.
fn parse_test_error(value: &FrameValue) -> Option<CoinbaseStarbasePublicTestError> {
    if !matches!(value, FrameValue::Object(_)) {
        return None;
    }
    let keys = value.own_keys();
    if !keys.iter().all(|key| {
        ["code", "data", "message"]
            .iter()
            .any(|allowed| key.iter().copied().eq(allowed.encode_utf16()))
    }) {
        return None;
    }
    let code = safe_integer(value.get("code"))?;
    let message = string(value.get("message")).filter(|v| !v.is_empty())?;
    Some(CoinbaseStarbasePublicTestError {
        code,
        message,
        data: value.get("data"),
    })
}
// ts: classifyPublicTestResponse
pub fn classify_public_test_response(frame: &FrameValue, pending: &[f64]) -> Classification {
    let invalid = || unknown(UnknownReason::InvalidPublicTestResponse);
    let id = safe_integer(frame.get("id"));
    if !string_eq(frame.get("jsonrpc"), "2.0")
        || id.is_none_or(|id| id <= 0.0 || !pending.contains(&id))
        || frame.get("method").is_some()
        || frame.get("params").is_some()
    {
        return invalid();
    }
    let has_result = frame.get("result").is_some();
    let has_error = frame.get("error").is_some();
    if has_result == has_error {
        return invalid();
    }
    let value_key = if has_result { "result" } else { "error" };
    if !frame.own_keys().iter().all(|key| {
        [
            "jsonrpc", "id", value_key, "testnet", "usDiff", "usIn", "usOut",
        ]
        .iter()
        .any(|allowed| key.iter().copied().eq(allowed.encode_utf16()))
    }) {
        return invalid();
    }
    if frame
        .get("testnet")
        .is_some_and(|value| !matches!(value, FrameValue::Bool(_)))
    {
        return invalid();
    }
    for (key, minimum) in [("usIn", 1.0), ("usOut", 1.0), ("usDiff", 0.0)] {
        if frame.get(key).is_some() && safe_integer(frame.get(key)).is_none_or(|n| n < minimum) {
            return invalid();
        }
    }
    let us_in = safe_integer(frame.get("usIn"));
    let us_out = safe_integer(frame.get("usOut"));
    let us_diff = safe_integer(frame.get("usDiff"));
    if let (Some(start), Some(end)) = (us_in, us_out) {
        if end < start || us_diff.is_some_and(|diff| end - start != diff) {
            return invalid();
        }
    }
    let outcome = if has_result {
        let result = frame.get("result").expect("own result");
        if !matches!(result, FrameValue::Object(_)) || !exact_keys(&result, &["version"]) {
            return invalid();
        }
        let Some(version) = string(result.get("version")).filter(|v| !v.is_empty()) else {
            return invalid();
        };
        CoinbaseStarbasePublicTestOutcome::Result { version }
    } else {
        let Some(error) = parse_test_error(&frame.get("error").expect("own error")) else {
            return invalid();
        };
        CoinbaseStarbasePublicTestOutcome::Error(error)
    };
    Classification::PublicTestResponse {
        id: id.expect("validated id"),
        outcome,
    }
}

// Runtime JSON.parse bridge after the source's duplicate-key/grammar guard. This
// preserves values that serde_json cannot carry and has no independent ingress API.
struct ParsedJson<'a> {
    raw: &'a [u16],
    index: usize,
}
impl ParsedJson<'_> {
    fn whitespace(&mut self) {
        while self
            .raw
            .get(self.index)
            .is_some_and(|c| matches!(c, 32 | 10 | 13 | 9))
        {
            self.index += 1;
        }
    }
    fn string(&mut self) -> Result<Vec<u16>, ()> {
        let start = self.index;
        if self.raw.get(start) != Some(&34) {
            return Err(());
        }
        self.index += 1;
        while let Some(&unit) = self.raw.get(self.index) {
            if unit == 34 {
                self.index += 1;
                return decode_json_string(&self.raw[start..self.index]);
            }
            self.index += if unit == 92 { 2 } else { 1 };
        }
        Err(())
    }
    fn value(&mut self) -> Result<FrameValue, ()> {
        self.whitespace();
        match self.raw.get(self.index).copied().ok_or(())? {
            34 => self.string().map(FrameValue::String),
            123 => {
                self.index += 1;
                self.whitespace();
                let mut values = IndexMap::new();
                if self.raw.get(self.index) == Some(&125) {
                    self.index += 1;
                    return Ok(FrameValue::Object(Rc::new(RefCell::new(values))));
                }
                loop {
                    self.whitespace();
                    let key = self.string()?;
                    self.whitespace();
                    if self.raw.get(self.index) != Some(&58) {
                        return Err(());
                    }
                    self.index += 1;
                    values.insert(key, self.value()?);
                    self.whitespace();
                    match self.raw.get(self.index) {
                        Some(125) => {
                            self.index += 1;
                            break;
                        }
                        Some(44) => self.index += 1,
                        _ => return Err(()),
                    }
                }
                Ok(FrameValue::Object(Rc::new(RefCell::new(values))))
            }
            91 => {
                self.index += 1;
                self.whitespace();
                let mut values = Vec::new();
                if self.raw.get(self.index) == Some(&93) {
                    self.index += 1;
                    return Ok(FrameValue::Array(Rc::new(RefCell::new(values))));
                }
                loop {
                    values.push(self.value()?);
                    self.whitespace();
                    match self.raw.get(self.index) {
                        Some(93) => {
                            self.index += 1;
                            break;
                        }
                        Some(44) => self.index += 1,
                        _ => return Err(()),
                    }
                }
                Ok(FrameValue::Array(Rc::new(RefCell::new(values))))
            }
            116 => {
                self.index += 4;
                Ok(FrameValue::Bool(true))
            }
            102 => {
                self.index += 5;
                Ok(FrameValue::Bool(false))
            }
            110 => {
                self.index += 4;
                Ok(FrameValue::Null)
            }
            45 | 48..=57 => {
                let start = self.index;
                while self
                    .raw
                    .get(self.index)
                    .is_some_and(|c| matches!(c, 45 | 43 | 46 | 48..=57 | 69 | 101))
                {
                    self.index += 1;
                }
                let text = String::from_utf16(&self.raw[start..self.index]).map_err(|_| ())?;
                text.parse::<f64>().map(FrameValue::Number).map_err(|_| ())
            }
            _ => Err(()),
        }
    }
}
// ts: classifyCoinbaseStarbasePublicWebsocketFrame
pub fn classify_coinbase_starbase_public_websocket_frame(
    raw: Option<&[u16]>,
    context: &serde_json::Map<String, Value>,
) -> Result<Classification, CoinbaseStarbaseProtocolError> {
    validate_context(&Value::Object(context.clone()))?;
    let Some(raw) = raw else {
        return Ok(unknown(UnknownReason::UnsupportedRawFrame));
    };
    match JsonDuplicateKeyScanner::new(raw).scan() {
        JsonKeyScanResult::Duplicate => return Ok(unknown(UnknownReason::DuplicateJsonKey)),
        JsonKeyScanResult::Invalid => return Ok(unknown(UnknownReason::InvalidJson)),
        JsonKeyScanResult::Valid => {}
    }
    let mut parser = ParsedJson { raw, index: 0 };
    let parsed = match parser.value() {
        Ok(value) => value,
        Err(()) => return Ok(unknown(UnknownReason::InvalidJson)),
    };
    if !matches!(parsed, FrameValue::Object(_)) {
        return Ok(unknown(UnknownReason::InvalidPublicWebsocketEnvelope));
    }
    if parsed.get("method").is_some() {
        let channels = |key: &str| {
            context[key]
                .as_array()
                .expect("validated list")
                .iter()
                .map(|v| {
                    v.as_str()
                        .expect("validated channel")
                        .encode_utf16()
                        .collect()
                })
                .collect::<Vec<Vec<u16>>>()
        };
        return Ok(classify_notification(
            &parsed,
            &channels("acknowledgedPublicChannels"),
            &channels("desiredPublicChannels"),
        ));
    }
    if parsed.get("id").is_some() {
        let pending = context["pendingPublicTestRequestIds"]
            .as_array()
            .expect("validated ids")
            .iter()
            .map(|v| v.as_f64().expect("validated id"))
            .collect::<Vec<_>>();
        return Ok(classify_public_test_response(&parsed, &pending));
    }
    Ok(unknown(UnknownReason::InvalidPublicWebsocketEnvelope))
}
