//! Pure acknowledgement and subscription operations; no socket/timer ownership.
use crate::v3_ws_commands::{
    WooV3WsCommand, WooV3WsCommandName, assert_topics_match_channel, build_woo_v3_subscribe,
};
use crate::v3_ws_session::{require_boolean, require_integer};
use crate::v3_ws_topics::{
    WOO_V3_WS_LIMITS, WooV3WsChannel, WooV3WsTopicDescriptor, parse_woo_v3_ws_topic,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{borrow::Cow, collections::HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(usize)]
pub enum WooV3WsBlockerCode {
    #[serde(rename = "command-failure-envelope-undocumented")]
    CommandFailureEnvelopeUndocumented,
    #[serde(rename = "command-request-id-type-undocumented")]
    CommandRequestIdTypeUndocumented,
    #[serde(rename = "listen-key-rotation-overlap-undocumented")]
    ListenKeyRotationOverlapUndocumented,
    #[serde(rename = "private-account-replacement-semantics-undocumented")]
    PrivateAccountReplacementSemanticsUndocumented,
    #[serde(rename = "private-balance-replacement-semantics-undocumented")]
    PrivateBalanceReplacementSemanticsUndocumented,
    #[serde(rename = "private-position-replacement-semantics-undocumented")]
    PrivatePositionReplacementSemanticsUndocumented,
    #[serde(rename = "execution-status-lifecycle-incomplete")]
    ExecutionStatusLifecycleIncomplete,
    #[serde(rename = "algo-shape-and-lifecycle-incomplete")]
    AlgoShapeAndLifecycleIncomplete,
}
impl WooV3WsBlockerCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CommandFailureEnvelopeUndocumented => "command-failure-envelope-undocumented",
            Self::CommandRequestIdTypeUndocumented => "command-request-id-type-undocumented",
            Self::ListenKeyRotationOverlapUndocumented => {
                "listen-key-rotation-overlap-undocumented"
            }
            Self::PrivateAccountReplacementSemanticsUndocumented => {
                "private-account-replacement-semantics-undocumented"
            }
            Self::PrivateBalanceReplacementSemanticsUndocumented => {
                "private-balance-replacement-semantics-undocumented"
            }
            Self::PrivatePositionReplacementSemanticsUndocumented => {
                "private-position-replacement-semantics-undocumented"
            }
            Self::ExecutionStatusLifecycleIncomplete => "execution-status-lifecycle-incomplete",
            Self::AlgoShapeAndLifecycleIncomplete => "algo-shape-and-lifecycle-incomplete",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "command-failure-envelope-undocumented" => {
                Some(Self::CommandFailureEnvelopeUndocumented)
            }
            "command-request-id-type-undocumented" => Some(Self::CommandRequestIdTypeUndocumented),
            "listen-key-rotation-overlap-undocumented" => {
                Some(Self::ListenKeyRotationOverlapUndocumented)
            }
            "private-account-replacement-semantics-undocumented" => {
                Some(Self::PrivateAccountReplacementSemanticsUndocumented)
            }
            "private-balance-replacement-semantics-undocumented" => {
                Some(Self::PrivateBalanceReplacementSemanticsUndocumented)
            }
            "private-position-replacement-semantics-undocumented" => {
                Some(Self::PrivatePositionReplacementSemanticsUndocumented)
            }
            "execution-status-lifecycle-incomplete" => {
                Some(Self::ExecutionStatusLifecycleIncomplete)
            }
            "algo-shape-and-lifecycle-incomplete" => Some(Self::AlgoShapeAndLifecycleIncomplete),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsBlockerCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("unknown WooV3WsBlockerCode literal"))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsBlockerCode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { CommandFailureEnvelopeUndocumented, CommandRequestIdTypeUndocumented, ListenKeyRotationOverlapUndocumented, PrivateAccountReplacementSemanticsUndocumented, PrivateBalanceReplacementSemanticsUndocumented, PrivatePositionReplacementSemanticsUndocumented, ExecutionStatusLifecycleIncomplete, AlgoShapeAndLifecycleIncomplete });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(usize)]
pub enum WsBlockerRequirement {
    #[serde(rename = "official-contract")]
    OfficialContract,
    #[serde(rename = "credentialed-demo")]
    CredentialedDemo,
}
impl WsBlockerRequirement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OfficialContract => "official-contract",
            Self::CredentialedDemo => "credentialed-demo",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "official-contract" => Some(Self::OfficialContract),
            "credentialed-demo" => Some(Self::CredentialedDemo),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WsBlockerRequirement {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("unknown WsBlockerRequirement literal"))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WsBlockerRequirement {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { OfficialContract, CredentialedDemo });
    }
}

// ts: WooV3WsBlocker
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsBlocker {
    pub code: WooV3WsBlockerCode,
    pub reason: Cow<'static, str>,
    pub requires: WsBlockerRequirement,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsBlocker")]
struct WooV3WsBlockerFields {
    #[serde(rename = "code")]
    code: WooV3WsBlockerCode,
    #[serde(rename = "reason")]
    reason: Cow<'static, str>,
    #[serde(rename = "requires")]
    requires: WsBlockerRequirement,
}
impl Serialize for WooV3WsBlocker {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsBlockerFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsBlocker {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsBlockerFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsBlocker {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, reason, requires });
    }
}

// ts: WooV3WsCommandAck
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsCommandAck {
    pub id: String,
    pub cmd: WooV3WsCommandName,
    pub success: bool,
    pub time: f64,
    pub data: Vec<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsCommandAck")]
struct WooV3WsCommandAckFields {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "cmd")]
    cmd: WooV3WsCommandName,
    #[serde(rename = "success", deserialize_with = "true_literal")]
    success: bool,
    #[serde(rename = "time")]
    time: f64,
    #[serde(rename = "data")]
    data: Vec<String>,
}
impl Serialize for WooV3WsCommandAck {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsCommandAckFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsCommandAck {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsCommandAckFields::deserialize(safe_cex_types::strict_decode::object(d))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsCommandAck {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, cmd, success, time, data });
    }
}

// ts: WooV3WsSubscriptionState
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsSubscriptionState {
    pub channel: WooV3WsChannel,
    pub generation: f64,
    pub intended_topics: Vec<String>,
    pub acknowledged_topics: Vec<String>,
    pub incremental_books_require_fresh_snapshot: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsSubscriptionState")]
struct WooV3WsSubscriptionStateFields {
    #[serde(rename = "channel")]
    channel: WooV3WsChannel,
    #[serde(rename = "generation")]
    generation: f64,
    #[serde(rename = "intendedTopics")]
    intended_topics: Vec<String>,
    #[serde(rename = "acknowledgedTopics")]
    acknowledged_topics: Vec<String>,
    #[serde(rename = "incrementalBooksRequireFreshSnapshot")]
    incremental_books_require_fresh_snapshot: bool,
}
impl Serialize for WooV3WsSubscriptionState {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WooV3WsSubscriptionStateFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsSubscriptionState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        JsNumberInput::deserialize(d)?
            .decode_with(|d| {
                WooV3WsSubscriptionStateFields::deserialize(safe_cex_types::strict_decode::object(
                    d,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsSubscriptionState {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { channel, generation, intended_topics, acknowledged_topics, incremental_books_require_fresh_snapshot });
    }
}

// ts: WOO_V3_WS_BLOCKERS
pub const WOO_V3_WS_BLOCKERS: [WooV3WsBlocker; 8] = [
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::CommandFailureEnvelopeUndocumented,
        reason: Cow::Borrowed(
            "WOO documents only success=true command acknowledgements, not the complete failure envelope",
        ),
        requires: WsBlockerRequirement::OfficialContract,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::CommandRequestIdTypeUndocumented,
        reason: Cow::Borrowed(
            "WOO shows an untyped request id placeholder and a string response id; builders use the documented string response subset without claiming numeric ids are invalid",
        ),
        requires: WsBlockerRequirement::OfficialContract,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::ListenKeyRotationOverlapUndocumented,
        reason: Cow::Borrowed(
            "creating a new listen key immediately expires the old key, but reconnect/rotation overlap is not defined",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::PrivateAccountReplacementSemanticsUndocumented,
        reason: Cow::Borrowed(
            "account pushes are periodic and event-driven, but WOO does not define snapshot versus patch replacement semantics",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::PrivateBalanceReplacementSemanticsUndocumented,
        reason: Cow::Borrowed(
            "balance pushes contain an array, but WOO does not state whether omitted tokens are unchanged or removed",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::PrivatePositionReplacementSemanticsUndocumented,
        reason: Cow::Borrowed(
            "position pushes contain an array, but WOO does not state whether omitted positions are unchanged or closed",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::ExecutionStatusLifecycleIncomplete,
        reason: Cow::Borrowed(
            "the execution page shows selected status transitions and reject variants but no exhaustive status lifecycle",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
    WooV3WsBlocker {
        code: WooV3WsBlockerCode::AlgoShapeAndLifecycleIncomplete,
        reason: Cow::Borrowed(
            "the algo page shows one BRACKET row without an exhaustive per-algo optional-field or lifecycle contract",
        ),
        requires: WsBlockerRequirement::CredentialedDemo,
    },
];
// ts: WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WsCommandIdContractStatus {
    pub emitted_subset: &'static str,
    pub blocker: &'static WooV3WsBlocker,
}
pub const WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS: WsCommandIdContractStatus =
    WsCommandIdContractStatus {
        emitted_subset: "documented-response-string",
        blocker: &WOO_V3_WS_BLOCKERS[1],
    };
fn true_literal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    if bool::deserialize(d)? {
        Ok(true)
    } else {
        Err(serde::de::Error::custom("literal must be true"))
    }
}
fn record(input: Option<&JsNumberInput>, path: &str) -> Result<JsObjectMap<JsNumberInput>, String> {
    input
        .and_then(|v| v.clone().decode::<JsObjectMap<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an object"))
}
fn text(input: Option<&JsNumberInput>) -> Option<String> {
    input.and_then(|v| v.clone().decode::<String>().ok())
}
fn string(input: Option<&JsNumberInput>, path: &str) -> Result<String, String> {
    text(input)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{path} must be a non-empty string"))
}
fn topics(input: Option<&JsNumberInput>, path: &str) -> Result<Vec<String>, String> {
    let values = input
        .and_then(|v| v.clone().decode::<Vec<JsNumberInput>>().ok())
        .ok_or_else(|| format!("{path} must be an array"))?;
    let topics = values
        .iter()
        .enumerate()
        .map(|(i, v)| string(Some(v), &format!("{path}[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    if topics.iter().collect::<HashSet<_>>().len() != topics.len() {
        return Err(format!("{path} must not contain duplicate topics"));
    }
    Ok(topics)
}
fn channel(descriptor: &WooV3WsTopicDescriptor) -> WooV3WsChannel {
    match descriptor {
        WooV3WsTopicDescriptor::Private { .. } => WooV3WsChannel::Private,
        _ => WooV3WsChannel::Public,
    }
}
// ts: decodeWooV3WsCommandAck
pub fn decode_woo_v3_ws_command_ack(
    input: Option<&JsNumberInput>,
    expected: Option<&WooV3WsCommand>,
) -> Result<WooV3WsCommandAck, String> {
    let path = "WOO V3 websocket command acknowledgement";
    let frame = record(input, path)?;
    let id = string(frame.get("id"), &format!("{path}.id"))?;
    let cmd=text(frame.get("cmd")).and_then(|s|WooV3WsCommandName::parse(&s)).ok_or_else(||format!("{path}.cmd must be one of SUBSCRIBE, UN_SUBSCRIBE, UN_SUBSCRIBE_ALL, LIST_SUBSCRIPTION"))?;
    if !require_boolean(frame.get("success"), &format!("{path}.success"))? {
        return Err(WOO_V3_WS_BLOCKERS[0].reason.to_string());
    }
    let data = topics(frame.get("data"), &format!("{path}.data"))?;
    let descriptors = data
        .iter()
        .map(|s| parse_woo_v3_ws_topic(Some(&Value::String(s.clone()))))
        .collect::<Result<Vec<_>, _>>()?;
    if descriptors.iter().any(|d| {
        descriptors
            .first()
            .is_some_and(|first| channel(d) != channel(first))
    }) {
        return Err(format!("{path} mixes public and private topics"));
    }
    let result = WooV3WsCommandAck {
        id,
        cmd,
        success: true,
        time: require_integer(frame.get("time"), &format!("{path}.time"), 0.0)?,
        data,
    };
    if let Some(expected) = expected {
        if result.id != expected.wire.id || result.cmd != expected.wire.cmd {
            return Err(
                "WOO V3 websocket acknowledgement identity does not match its command".into(),
            );
        }
        if let Some(first) = descriptors.first() {
            if channel(first) != expected.channel {
                return Err(format!(
                    "WOO V3 websocket acknowledgement returned {} topics for a {} command",
                    channel(first).as_str(),
                    expected.channel.as_str()
                ));
            }
        }
        if let Some(params) = &expected.wire.params {
            if result.data != *params {
                return Err(
                    "WOO V3 websocket acknowledgement topics do not exactly match its command"
                        .into(),
                );
            }
        }
    }
    Ok(result)
}
// ts: createWooV3WsSubscriptionState
pub fn create_woo_v3_ws_subscription_state(
    channel: WooV3WsChannel,
    topics_input: Option<&JsNumberInput>,
) -> Result<WooV3WsSubscriptionState, String> {
    let topics = topics(topics_input, "WOO V3 intended websocket topics")?;
    if topics.len() > usize::from(WOO_V3_WS_LIMITS.maximum_topics_per_connection) {
        return Err(format!(
            "WOO V3 websocket connection supports at most {} topics",
            WOO_V3_WS_LIMITS.maximum_topics_per_connection
        ));
    }
    assert_topics_match_channel(&topics, channel, "WOO V3 intended websocket topics")?;
    let incremental_books_require_fresh_snapshot = channel == WooV3WsChannel::Public
        && topics.iter().any(|s| s.starts_with("orderbookupdate"));
    Ok(WooV3WsSubscriptionState {
        channel,
        generation: 0.0,
        intended_topics: topics,
        acknowledged_topics: Vec::new(),
        incremental_books_require_fresh_snapshot,
    })
}
// ts: planWooV3WsResubscribe
pub fn plan_woo_v3_ws_resubscribe(
    state: &WooV3WsSubscriptionState,
    id_prefix_input: Option<&JsNumberInput>,
) -> Result<Vec<WooV3WsCommand>, String> {
    let prefix = string(id_prefix_input, "WOO V3 resubscribe id prefix")?;
    let mut commands = Vec::new();
    for chunk in state
        .intended_topics
        .chunks(usize::from(WOO_V3_WS_LIMITS.maximum_topics_per_command))
    {
        let id = Value::String(format!("{prefix}-{}", commands.len() + 1));
        commands.push(build_woo_v3_subscribe(state.channel, Some(&id), chunk)?);
    }
    Ok(commands)
}
// ts: acknowledgeWooV3WsSubscription
pub fn acknowledge_woo_v3_ws_subscription(
    state: &WooV3WsSubscriptionState,
    command: &WooV3WsCommand,
    acknowledgement: Option<&JsNumberInput>,
) -> Result<WooV3WsSubscriptionState, String> {
    if command.channel != state.channel {
        return Err(format!(
            "WOO V3 {} command cannot update {} subscription state",
            command.channel.as_str(),
            state.channel.as_str()
        ));
    }
    if command.wire.cmd != WooV3WsCommandName::Subscribe {
        return Err("WOO V3 subscription state only accepts SUBSCRIBE acknowledgements".into());
    }
    let ack = decode_woo_v3_ws_command_ack(acknowledgement, Some(command))?;
    let mut acknowledged = state
        .acknowledged_topics
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    for topic in &ack.data {
        if !state.intended_topics.contains(topic) {
            return Err(format!("WOO V3 acknowledged unexpected topic {topic}"));
        }
        acknowledged.insert(topic.clone());
    }
    let mut next = state.clone();
    next.acknowledged_topics = state
        .intended_topics
        .iter()
        .filter(|t| acknowledged.contains(*t))
        .cloned()
        .collect();
    Ok(next)
}
// ts: reconnectWooV3WsSubscription
pub fn reconnect_woo_v3_ws_subscription(
    state: &WooV3WsSubscriptionState,
) -> WooV3WsSubscriptionState {
    WooV3WsSubscriptionState {
        channel: state.channel,
        generation: state.generation + 1.0,
        intended_topics: state.intended_topics.clone(),
        acknowledged_topics: Vec::new(),
        incremental_books_require_fresh_snapshot: state.channel == WooV3WsChannel::Public
            && state
                .intended_topics
                .iter()
                .any(|s| s.starts_with("orderbookupdate")),
    }
}
