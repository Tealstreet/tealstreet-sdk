//! Inert V3 WebSocket topic contracts and pure builders. No socket is owned here.
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: WooV3WsPublicTopicKind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3WsPublicTopicKind {
    #[serde(rename = "orderbookupdate")]
    Orderbookupdate,
    #[serde(rename = "orderbookupdaterpi")]
    Orderbookupdaterpi,
    #[serde(rename = "orderbook10")]
    Orderbook10,
    #[serde(rename = "bbo")]
    Bbo,
    #[serde(rename = "bborpi")]
    Bborpi,
    #[serde(rename = "trade")]
    Trade,
    #[serde(rename = "ticker")]
    Ticker,
    #[serde(rename = "kline")]
    Kline,
    #[serde(rename = "indexprice")]
    Indexprice,
    #[serde(rename = "markprice")]
    Markprice,
    #[serde(rename = "estfundingrate")]
    Estfundingrate,
    #[serde(rename = "openinterest")]
    Openinterest,
}
impl WooV3WsPublicTopicKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orderbookupdate => "orderbookupdate",
            Self::Orderbookupdaterpi => "orderbookupdaterpi",
            Self::Orderbook10 => "orderbook10",
            Self::Bbo => "bbo",
            Self::Bborpi => "bborpi",
            Self::Trade => "trade",
            Self::Ticker => "ticker",
            Self::Kline => "kline",
            Self::Indexprice => "indexprice",
            Self::Markprice => "markprice",
            Self::Estfundingrate => "estfundingrate",
            Self::Openinterest => "openinterest",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orderbookupdate" => Some(Self::Orderbookupdate),
            "orderbookupdaterpi" => Some(Self::Orderbookupdaterpi),
            "orderbook10" => Some(Self::Orderbook10),
            "bbo" => Some(Self::Bbo),
            "bborpi" => Some(Self::Bborpi),
            "trade" => Some(Self::Trade),
            "ticker" => Some(Self::Ticker),
            "kline" => Some(Self::Kline),
            "indexprice" => Some(Self::Indexprice),
            "markprice" => Some(Self::Markprice),
            "estfundingrate" => Some(Self::Estfundingrate),
            "openinterest" => Some(Self::Openinterest),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsPublicTopicKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("unknown WooV3WsPublicTopicKind literal"))
    }
}

// ts: WooV3WsPrivateTopic
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3WsPrivateTopic {
    #[serde(rename = "account")]
    Account,
    #[serde(rename = "balance")]
    Balance,
    #[serde(rename = "position")]
    Position,
    #[serde(rename = "executionreport")]
    Executionreport,
    #[serde(rename = "algoexecutionreport")]
    Algoexecutionreport,
}
impl WooV3WsPrivateTopic {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::Balance => "balance",
            Self::Position => "position",
            Self::Executionreport => "executionreport",
            Self::Algoexecutionreport => "algoexecutionreport",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "account" => Some(Self::Account),
            "balance" => Some(Self::Balance),
            "position" => Some(Self::Position),
            "executionreport" => Some(Self::Executionreport),
            "algoexecutionreport" => Some(Self::Algoexecutionreport),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsPrivateTopic {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("unknown WooV3WsPrivateTopic literal"))
    }
}

// ts: WooV3WsChannel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3WsChannel {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "private")]
    Private,
}
impl WooV3WsChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsChannel {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown WooV3WsChannel literal"))
    }
}

// ts: WooV3WsKlineInterval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WooV3WsKlineInterval {
    #[serde(rename = "1m")]
    M1,
    #[serde(rename = "3m")]
    M3,
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "2h")]
    H2,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "1d")]
    D1,
    #[serde(rename = "3d")]
    D3,
    #[serde(rename = "1w")]
    W1,
    #[serde(rename = "1M")]
    Month1,
}
impl WooV3WsKlineInterval {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::M1 => "1m",
            Self::M3 => "3m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1h",
            Self::H2 => "2h",
            Self::H4 => "4h",
            Self::H6 => "6h",
            Self::H12 => "12h",
            Self::D1 => "1d",
            Self::D3 => "3d",
            Self::W1 => "1w",
            Self::Month1 => "1M",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "1m" => Some(Self::M1),
            "3m" => Some(Self::M3),
            "5m" => Some(Self::M5),
            "15m" => Some(Self::M15),
            "30m" => Some(Self::M30),
            "1h" => Some(Self::H1),
            "2h" => Some(Self::H2),
            "4h" => Some(Self::H4),
            "6h" => Some(Self::H6),
            "12h" => Some(Self::H12),
            "1d" => Some(Self::D1),
            "3d" => Some(Self::D3),
            "1w" => Some(Self::W1),
            "1M" => Some(Self::Month1),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for WooV3WsKlineInterval {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text)
            .ok_or_else(|| serde::de::Error::custom("unknown WooV3WsKlineInterval literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BookKind {
    #[serde(rename = "orderbookupdate")]
    Update,
    #[serde(rename = "orderbookupdaterpi")]
    UpdateRpi,
}
impl BookKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Update => "orderbookupdate",
            Self::UpdateRpi => "orderbookupdaterpi",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orderbookupdate" => Some(Self::Update),
            "orderbookupdaterpi" => Some(Self::UpdateRpi),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for BookKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown BookKind literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PlainKind {
    #[serde(rename = "orderbook10")]
    Orderbook10,
    #[serde(rename = "bbo")]
    Bbo,
    #[serde(rename = "bborpi")]
    Bborpi,
    #[serde(rename = "trade")]
    Trade,
    #[serde(rename = "ticker")]
    Ticker,
    #[serde(rename = "indexprice")]
    Indexprice,
    #[serde(rename = "markprice")]
    Markprice,
    #[serde(rename = "estfundingrate")]
    Estfundingrate,
    #[serde(rename = "openinterest")]
    Openinterest,
}
impl PlainKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Orderbook10 => "orderbook10",
            Self::Bbo => "bbo",
            Self::Bborpi => "bborpi",
            Self::Trade => "trade",
            Self::Ticker => "ticker",
            Self::Indexprice => "indexprice",
            Self::Markprice => "markprice",
            Self::Estfundingrate => "estfundingrate",
            Self::Openinterest => "openinterest",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "orderbook10" => Some(Self::Orderbook10),
            "bbo" => Some(Self::Bbo),
            "bborpi" => Some(Self::Bborpi),
            "trade" => Some(Self::Trade),
            "ticker" => Some(Self::Ticker),
            "indexprice" => Some(Self::Indexprice),
            "markprice" => Some(Self::Markprice),
            "estfundingrate" => Some(Self::Estfundingrate),
            "openinterest" => Some(Self::Openinterest),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for PlainKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown PlainKind literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PublicChannel {
    #[serde(rename = "public")]
    Public,
}
impl PublicChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for PublicChannel {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown PublicChannel literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrivateChannel {
    #[serde(rename = "private")]
    Private,
}
impl PrivateChannel {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "private" => Some(Self::Private),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for PrivateChannel {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown PrivateChannel literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum KlineKind {
    #[serde(rename = "kline")]
    Kline,
}
impl KlineKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kline => "kline",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "kline" => Some(Self::Kline),
            _ => None,
        }
    }
}
impl<'de> Deserialize<'de> for KlineKind {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let text = String::deserialize(decoder)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom("unknown KlineKind literal"))
    }
}

// ts: WooV3WsBookDepth
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WooV3WsBookDepth {
    D50,
    D200,
    D500,
}
impl WooV3WsBookDepth {
    pub const fn number(self) -> u16 {
        match self {
            Self::D50 => 50,
            Self::D200 => 200,
            Self::D500 => 500,
        }
    }
    pub fn from_number(value: f64) -> Option<Self> {
        if value == 50.0 {
            Some(Self::D50)
        } else if value == 200.0 {
            Some(Self::D200)
        } else if value == 500.0 {
            Some(Self::D500)
        } else {
            None
        }
    }
}
impl Serialize for WooV3WsBookDepth {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u16(self.number())
    }
}
impl<'de> Deserialize<'de> for WooV3WsBookDepth {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let number = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(f64::deserialize)
            .map_err(serde::de::Error::custom)?;
        Self::from_number(number)
            .ok_or_else(|| serde::de::Error::custom("documented depth 50, 200, or 500 required"))
    }
}

// ts: WOO_V3_WS_PUBLIC_TOPIC_KINDS
pub const WOO_V3_WS_PUBLIC_TOPIC_KINDS: [WooV3WsPublicTopicKind; 12] = [
    WooV3WsPublicTopicKind::Orderbookupdate,
    WooV3WsPublicTopicKind::Orderbookupdaterpi,
    WooV3WsPublicTopicKind::Orderbook10,
    WooV3WsPublicTopicKind::Bbo,
    WooV3WsPublicTopicKind::Bborpi,
    WooV3WsPublicTopicKind::Trade,
    WooV3WsPublicTopicKind::Ticker,
    WooV3WsPublicTopicKind::Kline,
    WooV3WsPublicTopicKind::Indexprice,
    WooV3WsPublicTopicKind::Markprice,
    WooV3WsPublicTopicKind::Estfundingrate,
    WooV3WsPublicTopicKind::Openinterest,
];
// ts: WOO_V3_WS_PRIVATE_TOPICS
pub const WOO_V3_WS_PRIVATE_TOPICS: [WooV3WsPrivateTopic; 5] = [
    WooV3WsPrivateTopic::Account,
    WooV3WsPrivateTopic::Balance,
    WooV3WsPrivateTopic::Position,
    WooV3WsPrivateTopic::Executionreport,
    WooV3WsPrivateTopic::Algoexecutionreport,
];
#[derive(Debug, Clone, Copy)]
pub struct WsLimits {
    pub maximum_connection_lifetime_ms: u32,
    pub maximum_connections_per_five_minutes_per_ip: u16,
    pub maximum_active_connections_per_ip: u16,
    pub maximum_topics_per_connection: u16,
    pub maximum_topics_per_command: u16,
    pub recommended_maximum_subscription_commands_per_second: u16,
}
// ts: WOO_V3_WS_LIMITS
pub const WOO_V3_WS_LIMITS: WsLimits = WsLimits {
    maximum_connection_lifetime_ms: 24 * 60 * 60 * 1_000,
    maximum_connections_per_five_minutes_per_ip: 100,
    maximum_active_connections_per_ip: 100,
    maximum_topics_per_connection: 100,
    maximum_topics_per_command: 20,
    recommended_maximum_subscription_commands_per_second: 200,
};
#[derive(Debug, Clone, Copy)]
pub struct WsChannelContract {
    pub livenet_url: &'static str,
    pub testnet_url: &'static str,
    pub authentication: &'static str,
    pub listen_key_endpoint: Option<&'static str>,
}
// ts: WOO_V3_WS_CHANNELS
pub const WOO_V3_WS_CHANNELS: [(WooV3WsChannel, WsChannelContract); 2] = [
    (
        WooV3WsChannel::Public,
        WsChannelContract {
            livenet_url: "wss://wss.woox.io/v3/public",
            testnet_url: "wss://wss.staging.woox.io/v3/public",
            authentication: "none",
            listen_key_endpoint: None,
        },
    ),
    (
        WooV3WsChannel::Private,
        WsChannelContract {
            livenet_url: "wss://wss.woox.io/v3/private",
            testnet_url: "wss://wss.staging.woox.io/v3/private",
            authentication: "listen-key-query",
            listen_key_endpoint: Some("/v3/account/listenKey"),
        },
    ),
];

// ts: WooV3WsTopicDescriptor
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WooV3WsTopicDescriptor {
    Book {
        channel: PublicChannel,
        kind: BookKind,
        symbol: String,
        depth: WooV3WsBookDepth,
        #[serde(rename = "includesRpi")]
        includes_rpi: bool,
        topic: String,
    },
    Kline {
        channel: PublicChannel,
        kind: KlineKind,
        symbol: String,
        interval: WooV3WsKlineInterval,
        topic: String,
    },
    Public {
        channel: PublicChannel,
        kind: PlainKind,
        symbol: String,
        topic: String,
    },
    Private {
        channel: PrivateChannel,
        kind: WooV3WsPrivateTopic,
        topic: WooV3WsPrivateTopic,
    },
}
#[derive(Deserialize)]
#[serde(remote = "WooV3WsTopicDescriptor", untagged)]
enum DescriptorWire {
    Book {
        channel: PublicChannel,
        kind: BookKind,
        symbol: String,
        depth: WooV3WsBookDepth,
        #[serde(rename = "includesRpi")]
        includes_rpi: bool,
        topic: String,
    },
    Kline {
        channel: PublicChannel,
        kind: KlineKind,
        symbol: String,
        interval: WooV3WsKlineInterval,
        topic: String,
    },
    Public {
        channel: PublicChannel,
        kind: PlainKind,
        symbol: String,
        topic: String,
    },
    Private {
        channel: PrivateChannel,
        kind: WooV3WsPrivateTopic,
        topic: WooV3WsPrivateTopic,
    },
}
impl<'de> Deserialize<'de> for WooV3WsTopicDescriptor {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(decoder)?
            .decode_with(|decoder| {
                DescriptorWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: requireString
pub fn require_string(
    value: Option<&Value>,
    path: &str,
    allow_empty: bool,
) -> Result<String, String> {
    match value.and_then(Value::as_str) {
        Some(text) if allow_empty || !text.is_empty() => Ok(text.to_owned()),
        _ => Err(format!(
            "{path} must be {}",
            if allow_empty {
                "a string"
            } else {
                "a non-empty string"
            }
        )),
    }
}
// ts: requireSymbol
pub fn require_symbol(value: Option<&Value>, path: &str) -> Result<String, String> {
    let symbol = require_string(value, path, false)?;
    let parts: Vec<_> = symbol.split('_').collect();
    let valid = parts.len() == 3
        && matches!(parts[0], "SPOT" | "PERP")
        && parts[1..].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
        });
    if !valid {
        return Err(format!(
            "{path} must be a documented WOO SPOT_ or PERP_ symbol"
        ));
    }
    Ok(symbol)
}
// ts: buildWooV3PublicTopic
pub fn build_woo_v3_public_topic(
    kind: WooV3WsPublicTopicKind,
    symbol_input: &Value,
    option: Option<&Value>,
) -> Result<WooV3WsTopicDescriptor, String> {
    let symbol = require_symbol(Some(symbol_input), "WOO V3 websocket topic symbol")?;
    let name = kind.as_str();
    if let Some(book_kind) = BookKind::parse(name) {
        let depth = option
            .and_then(Value::as_f64)
            .and_then(WooV3WsBookDepth::from_number)
            .ok_or_else(|| format!("{name} requires documented depth 50, 200, or 500"))?;
        let topic = format!("{name}@{symbol}@{}", depth.number());
        return Ok(WooV3WsTopicDescriptor::Book {
            channel: PublicChannel::Public,
            kind: book_kind,
            symbol,
            depth,
            includes_rpi: kind == WooV3WsPublicTopicKind::Orderbookupdaterpi,
            topic,
        });
    }
    if kind == WooV3WsPublicTopicKind::Kline {
        let interval = option
            .and_then(Value::as_str)
            .and_then(WooV3WsKlineInterval::parse)
            .ok_or("kline requires a documented WOO V3 interval")?;
        let topic = format!("{name}@{symbol}@{}", interval.as_str());
        return Ok(WooV3WsTopicDescriptor::Kline {
            channel: PublicChannel::Public,
            kind: KlineKind::Kline,
            symbol,
            interval,
            topic,
        });
    }
    if option.is_some() {
        return Err(format!("{name} does not accept a topic option"));
    }
    Ok(WooV3WsTopicDescriptor::Public {
        channel: PublicChannel::Public,
        kind: PlainKind::parse(name).expect("remaining public topic kind"),
        topic: format!("{name}@{symbol}"),
        symbol,
    })
}
// ts: buildWooV3PrivateTopic
pub fn build_woo_v3_private_topic(topic: &Value) -> Result<WooV3WsTopicDescriptor, String> {
    let Some(kind) = topic.as_str().and_then(WooV3WsPrivateTopic::parse) else {
        return Err(format!(
            "unknown WOO V3 private topic {}",
            safe_cex_num::js_string(Some(topic)).map_err(|error| error.message.to_owned())?
        ));
    };
    Ok(WooV3WsTopicDescriptor::Private {
        channel: PrivateChannel::Private,
        kind,
        topic: kind,
    })
}
// ts: parseWooV3WsTopic
pub fn parse_woo_v3_ws_topic(input: Option<&Value>) -> Result<WooV3WsTopicDescriptor, String> {
    let topic = require_string(input, "WOO V3 websocket topic", false)?;
    if WooV3WsPrivateTopic::parse(&topic).is_some() {
        return build_woo_v3_private_topic(&Value::String(topic));
    }
    let parts: Vec<_> = topic.split('@').collect();
    let kind = WooV3WsPublicTopicKind::parse(parts[0])
        .ok_or_else(|| format!("unknown WOO V3 websocket topic {topic}"))?;
    let symbol = || Value::String(parts[1].to_owned());
    if BookKind::parse(kind.as_str()).is_some() {
        if parts.len() != 3 {
            return Err(format!(
                "{} topic must contain symbol and depth",
                kind.as_str()
            ));
        }
        let depth = safe_cex_num::js_number(Some(&Value::String(parts[2].to_owned())))
            .map_err(|error| error.to_string())?;
        let option = serde_json::Number::from_f64(depth)
            .map(Value::Number)
            .unwrap_or(Value::Null);
        return build_woo_v3_public_topic(kind, &symbol(), Some(&option));
    }
    if kind == WooV3WsPublicTopicKind::Kline {
        if parts.len() != 3 {
            return Err("kline topic must contain symbol and interval".into());
        }
        return build_woo_v3_public_topic(
            kind,
            &symbol(),
            Some(&Value::String(parts[2].to_owned())),
        );
    }
    if parts.len() != 2 {
        return Err(format!(
            "{} topic must contain exactly one symbol",
            kind.as_str()
        ));
    }
    build_woo_v3_public_topic(kind, &symbol(), None)
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPublicTopicKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Orderbookupdate, Orderbookupdaterpi, Orderbook10, Bbo, Bborpi, Trade, Ticker, Kline, Indexprice, Markprice, Estfundingrate, Openinterest });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsPrivateTopic {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Account, Balance, Position, Executionreport, Algoexecutionreport });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsChannel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Public, Private });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsKlineInterval {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { M1, M3, M5, M15, M30, H1, H2, H4, H6, H12, D1, D3, W1, Month1 });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BookKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Update, UpdateRpi });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for PlainKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Orderbook10, Bbo, Bborpi, Trade, Ticker, Indexprice, Markprice, Estfundingrate, Openinterest });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for PublicChannel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Public });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for PrivateChannel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Private });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for KlineKind {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Kline });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsBookDepth {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { D50, D200, D500 });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3WsTopicDescriptor {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum {
            Book {channel,kind,symbol,depth,includes_rpi,topic},
            Kline {channel,kind,symbol,interval,topic},
            Public {channel,kind,symbol,topic}, Private {channel,kind,topic}
        });
    }
}
