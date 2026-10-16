//! Pure V3 WebSocket command construction and shared JSON validation helpers.
use crate::v3_ws_topics::{
    WOO_V3_WS_LIMITS, WooV3WsChannel, WooV3WsTopicDescriptor, parse_woo_v3_ws_topic, require_string,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashSet;

// ts: WooV3WsCommandName
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3WsCommandName {
    #[serde(rename = "SUBSCRIBE")]
    Subscribe,
    #[serde(rename = "UN_SUBSCRIBE")]
    Unsubscribe,
    #[serde(rename = "UN_SUBSCRIBE_ALL")]
    UnsubscribeAll,
    #[serde(rename = "LIST_SUBSCRIPTION")]
    ListSubscriptions,
}
impl WooV3WsCommandName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subscribe => "SUBSCRIBE",
            Self::Unsubscribe => "UN_SUBSCRIBE",
            Self::UnsubscribeAll => "UN_SUBSCRIBE_ALL",
            Self::ListSubscriptions => "LIST_SUBSCRIPTION",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "SUBSCRIBE" => Some(Self::Subscribe),
            "UN_SUBSCRIBE" => Some(Self::Unsubscribe),
            "UN_SUBSCRIBE_ALL" => Some(Self::UnsubscribeAll),
            "LIST_SUBSCRIPTION" => Some(Self::ListSubscriptions),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsCommandName {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = String::deserialize(decoder)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("unknown WOO V3 command name"))
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsCommandName {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Subscribe, Unsubscribe, UnsubscribeAll, ListSubscriptions });
    }
}
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    decoder: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(decoder).map(Some)
}
// ts: WooV3WsWireCommand
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsWireCommand {
    pub id: String,
    pub cmd: WooV3WsCommandName,
    pub params: Option<Vec<String>>,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsWireCommand")]
struct WireCommandFields {
    id: String,
    cmd: WooV3WsCommandName,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    params: Option<Vec<String>>,
}
impl Serialize for WooV3WsWireCommand {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        WireCommandFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsWireCommand {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                WireCommandFields::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsWireCommand {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, cmd, params });
    }
}
// ts: WooV3WsCommand
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3WsCommand {
    pub channel: WooV3WsChannel,
    pub wire: WooV3WsWireCommand,
}
#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3WsCommand")]
struct CommandFields {
    channel: WooV3WsChannel,
    wire: WooV3WsWireCommand,
}
impl Serialize for WooV3WsCommand {
    fn serialize<S: serde::Serializer>(&self, encoder: S) -> Result<S::Ok, S::Error> {
        CommandFields::serialize(self, encoder)
    }
}
impl<'de> Deserialize<'de> for WooV3WsCommand {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                CommandFields::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsCommand {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { channel, wire });
    }
}

// ts: isRecord
pub fn is_record(value: Option<&Value>) -> bool {
    value.is_some_and(Value::is_object)
}
// ts: requireRecord
pub fn require_record<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<&'a Map<String, Value>, String> {
    if !is_record(value) {
        return Err(format!("{path} must be an object"));
    }
    Ok(value.and_then(Value::as_object).expect("validated object"))
}
// ts: requireArray
pub fn require_array<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a [Value], String> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path} must be an array"))
}
// ts: requireEnum
pub fn require_enum(value: Option<&Value>, values: &[&str], path: &str) -> Result<String, String> {
    match value.and_then(Value::as_str) {
        Some(value) if values.contains(&value) => Ok(value.to_owned()),
        _ => Err(format!("{path} must be one of {}", values.join(", "))),
    }
}
// ts: requireTopicList
pub fn require_topic_list(value: Option<&Value>, path: &str) -> Result<Vec<String>, String> {
    let topics = require_array(value, path)?
        .iter()
        .enumerate()
        .map(|(index, topic)| require_string(Some(topic), &format!("{path}[{index}]"), false))
        .collect::<Result<Vec<_>, _>>()?;
    let unique: HashSet<_> = topics.iter().collect();
    if unique.len() != topics.len() {
        return Err(format!("{path} must not contain duplicate topics"));
    }
    Ok(topics)
}
// ts: assertTopicsMatchChannel
pub fn assert_topics_match_channel(
    topics: &[String],
    channel: WooV3WsChannel,
    path: &str,
) -> Result<(), String> {
    for (index, topic) in topics.iter().enumerate() {
        let descriptor = parse_woo_v3_ws_topic(Some(&Value::String(topic.clone())))?;
        let actual_channel = match descriptor {
            WooV3WsTopicDescriptor::Book { .. }
            | WooV3WsTopicDescriptor::Kline { .. }
            | WooV3WsTopicDescriptor::Public { .. } => WooV3WsChannel::Public,
            WooV3WsTopicDescriptor::Private { .. } => WooV3WsChannel::Private,
        };
        if actual_channel != channel {
            return Err(format!(
                "{path}[{index}] is a {} topic for a {} websocket",
                actual_channel.as_str(),
                channel.as_str()
            ));
        }
    }
    Ok(())
}
// ts: buildCommand
pub fn build_command(
    channel: WooV3WsChannel,
    id_input: Option<&Value>,
    cmd: WooV3WsCommandName,
    topics: Option<&[String]>,
) -> Result<WooV3WsCommand, String> {
    let id = require_string(id_input, "WOO V3 websocket command id", false)?;
    let params = if let Some(topics) = topics {
        if topics.is_empty()
            || topics.len() > usize::from(WOO_V3_WS_LIMITS.maximum_topics_per_command)
        {
            return Err(format!(
                "WOO V3 {} requires 1-{} topics",
                cmd.as_str(),
                WOO_V3_WS_LIMITS.maximum_topics_per_command
            ));
        }
        assert_topics_match_channel(topics, channel, &format!("WOO V3 {} topics", cmd.as_str()))?;
        let unique: HashSet<_> = topics.iter().collect();
        if unique.len() != topics.len() {
            return Err(format!("WOO V3 {} topics must be unique", cmd.as_str()));
        }
        Some(topics.to_vec())
    } else {
        None
    };
    Ok(WooV3WsCommand {
        channel,
        wire: WooV3WsWireCommand { id, cmd, params },
    })
}
// ts: buildWooV3Subscribe
pub fn build_woo_v3_subscribe(
    channel: WooV3WsChannel,
    id: Option<&Value>,
    topics: &[String],
) -> Result<WooV3WsCommand, String> {
    build_command(channel, id, WooV3WsCommandName::Subscribe, Some(topics))
}
// ts: buildWooV3Unsubscribe
pub fn build_woo_v3_unsubscribe(
    channel: WooV3WsChannel,
    id: Option<&Value>,
    topics: &[String],
) -> Result<WooV3WsCommand, String> {
    build_command(channel, id, WooV3WsCommandName::Unsubscribe, Some(topics))
}
// ts: buildWooV3UnsubscribeAll
pub fn build_woo_v3_unsubscribe_all(
    channel: WooV3WsChannel,
    id: Option<&Value>,
) -> Result<WooV3WsCommand, String> {
    build_command(channel, id, WooV3WsCommandName::UnsubscribeAll, None)
}
// ts: buildWooV3ListSubscriptions
pub fn build_woo_v3_list_subscriptions(
    channel: WooV3WsChannel,
    id: Option<&Value>,
) -> Result<WooV3WsCommand, String> {
    build_command(channel, id, WooV3WsCommandName::ListSubscriptions, None)
}
