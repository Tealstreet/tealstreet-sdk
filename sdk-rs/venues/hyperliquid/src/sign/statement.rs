//! The structured L1 preimage a remote signer re-derives its digest from.
//! The device never signs a digest it did not compute from an allowed action.
use super::{action_hash_encoded, agent_digest};
use safe_cex_engine::signing::SignError;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq, Serializer};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

/// L1 trading actions only. Fund movement is user-signed and never allowed here.
pub const ALLOWED_ACTIONS: &[&str] = &[
    "order",
    "cancel",
    "cancelByCloid",
    "modify",
    "batchModify",
    "updateLeverage",
    "updateIsolatedMargin",
    "scheduleCancel",
];
const MAX_STATEMENT: usize = 256 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Encoded<'a> {
    kind: &'static str,
    action: &'a RawValue,
    nonce: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    vault_address: Option<&'a str>,
    is_mainnet: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Decoded<'a> {
    kind: &'a str,
    #[serde(borrow)]
    action: &'a RawValue,
    nonce: u64,
    #[serde(default)]
    vault_address: Option<&'a str>,
    #[serde(default)]
    expires_after: Option<u64>,
    is_mainnet: bool,
}

/// The canonical statement for one prepared L1 action.
pub fn encode(
    action: &RawValue,
    nonce: u64,
    vault: Option<&str>,
    mainnet: bool,
) -> Result<String, SignError> {
    serde_json::to_string(&Encoded {
        kind: "l1",
        action,
        nonce,
        vault_address: vault,
        is_mainnet: mainnet,
    })
    .map_err(|_| SignError::InvalidRequest)
}

/// The signed-in device account a statement must belong to.
pub struct Account<'a> {
    pub mainnet: bool,
    pub vault: Option<&'a str>,
}

/// Re-derive the phantom-agent digest from a statement, refusing anything
/// outside the L1 trading allowlist or another network or vault.
pub fn derive(statement: &str, account: &Account<'_>) -> Result<[u8; 32], SignError> {
    if statement.len() > MAX_STATEMENT {
        return Err(SignError::InvalidRequest);
    }
    let decoded: Decoded<'_> =
        serde_json::from_str(statement).map_err(|_| SignError::InvalidRequest)?;
    // Expiring actions are never issued by the engine; refuse rather than guess.
    if decoded.kind != "l1" || decoded.expires_after.is_some() {
        return Err(SignError::Unsupported);
    }
    if decoded.is_mainnet != account.mainnet {
        return Err(SignError::InvalidRequest);
    }
    let same_vault = match (decoded.vault_address, account.vault) {
        (None, None) => true,
        (Some(stated), Some(own)) => stated.eq_ignore_ascii_case(own),
        _ => false,
    };
    if !same_vault {
        return Err(SignError::InvalidRequest);
    }
    let action: Node =
        serde_json::from_str(decoded.action.get()).map_err(|_| SignError::InvalidRequest)?;
    let Node::Map(fields) = &action else {
        return Err(SignError::InvalidRequest);
    };
    let kinds: Vec<_> = fields.iter().filter(|(key, _)| key == "type").collect();
    match kinds.as_slice() {
        [(_, Node::Str(kind))] if ALLOWED_ACTIONS.contains(&kind.as_str()) => {}
        _ => return Err(SignError::Unsupported),
    }
    let encoded = rmp_serde::to_vec_named(&action).map_err(|_| SignError::InvalidRequest)?;
    let hash = action_hash_encoded(&encoded, decoded.vault_address, decoded.nonce)
        .map_err(|_| SignError::InvalidRequest)?;
    Ok(agent_digest(hash, decoded.is_mainnet))
}

/// JSON in its original key order, so msgpack matches the venue's own encoding.
#[derive(Debug, PartialEq)]
enum Node {
    Null,
    Bool(bool),
    Unsigned(u64),
    Signed(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Node>),
    Map(Vec<(String, Node)>),
}

// serde_json's arbitrary_precision presents numbers as this single-entry map.
const NUMBER_TOKEN: &str = "$serde_json::private::Number";

impl Node {
    fn number(text: &str) -> Option<Self> {
        if let Ok(value) = text.parse::<u64>() {
            Some(Self::Unsigned(value))
        } else if let Ok(value) = text.parse::<i64>() {
            Some(Self::Signed(value))
        } else {
            text.parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(Self::Float)
        }
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }
    fn visit_unit<E>(self) -> Result<Node, E> {
        Ok(Node::Null)
    }
    fn visit_bool<E>(self, value: bool) -> Result<Node, E> {
        Ok(Node::Bool(value))
    }
    fn visit_u64<E>(self, value: u64) -> Result<Node, E> {
        Ok(Node::Unsigned(value))
    }
    fn visit_i64<E>(self, value: i64) -> Result<Node, E> {
        Ok(u64::try_from(value).map_or(Node::Signed(value), Node::Unsigned))
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Node, E> {
        value
            .is_finite()
            .then_some(Node::Float(value))
            .ok_or_else(|| E::custom("non-finite number"))
    }
    fn visit_str<E>(self, value: &str) -> Result<Node, E> {
        Ok(Node::Str(value.into()))
    }
    fn visit_string<E>(self, value: String) -> Result<Node, E> {
        Ok(Node::Str(value))
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Node, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Node::Seq(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Node, A::Error> {
        let mut fields: Vec<(String, Node)> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            if fields.is_empty() && key == NUMBER_TOKEN {
                let text: String = map.next_value()?;
                return Node::number(&text)
                    .ok_or_else(|| serde::de::Error::custom("invalid number"));
            }
            if fields.iter().any(|(seen, _)| *seen == key) {
                return Err(serde::de::Error::custom("duplicate field"));
            }
            let value = map.next_value()?;
            fields.push((key, value));
        }
        Ok(Node::Map(fields))
    }
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NodeVisitor)
    }
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_unit(),
            Self::Bool(value) => serializer.serialize_bool(*value),
            Self::Unsigned(value) => serializer.serialize_u64(*value),
            Self::Signed(value) => serializer.serialize_i64(*value),
            Self::Float(value) => serializer.serialize_f64(*value),
            Self::Str(value) => serializer.serialize_str(value),
            Self::Seq(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Self::Map(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}
