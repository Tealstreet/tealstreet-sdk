//! The structured L1 preimage a remote signer re-derives its digest from.
//! The device never signs a digest it did not compute from an allowed action.
use super::{action_hash_encoded, agent_digest};
use safe_cex_engine::signing::SignError;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
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

/// A statement's fields, read from one Node parse with the strictness of a
/// derived deny_unknown_fields struct: known keys once, required keys present.
struct Decoded<'a> {
    kind: &'a str,
    action: &'a Node,
    nonce: u64,
    vault_address: Option<&'a str>,
    expires_after: Option<u64>,
    is_mainnet: bool,
}

impl<'a> Decoded<'a> {
    fn read(statement: &'a Node) -> Option<Self> {
        let Node::Map(fields) = statement else {
            return None;
        };
        let (mut kind, mut action, mut nonce, mut is_mainnet) = (None, None, None, None);
        let (mut vault_address, mut expires_after) = (None, None);
        for (key, value) in fields {
            match (key.as_str(), value) {
                ("kind", Node::Str(value)) => kind = Some(value.as_str()),
                ("action", value) => action = Some(value),
                ("nonce", Node::Unsigned(value)) => nonce = Some(*value),
                ("vaultAddress", Node::Null) | ("expiresAfter", Node::Null) => {}
                ("vaultAddress", Node::Str(value)) => vault_address = Some(value.as_str()),
                ("expiresAfter", Node::Unsigned(value)) => expires_after = Some(*value),
                ("isMainnet", Node::Bool(value)) => is_mainnet = Some(*value),
                _ => return None,
            }
        }
        Some(Self {
            kind: kind?,
            action: action?,
            nonce: nonce?,
            vault_address,
            expires_after,
            is_mainnet: is_mainnet?,
        })
    }
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
    let statement: Node = serde_json::from_str(statement).map_err(|_| SignError::InvalidRequest)?;
    let decoded = Decoded::read(&statement).ok_or(SignError::InvalidRequest)?;
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
    let action = decoded.action;
    let Node::Map(fields) = action else {
        return Err(SignError::InvalidRequest);
    };
    let kinds: Vec<_> = fields.iter().filter(|(key, _)| key == "type").collect();
    match kinds.as_slice() {
        [(_, Node::Str(kind))] if ALLOWED_ACTIONS.contains(&kind.as_str()) => {}
        _ => return Err(SignError::Unsupported),
    }
    let hash = action_hash_encoded(&action.msgpack(), decoded.vault_address, decoded.nonce)
        .map_err(|_| SignError::InvalidRequest)?;
    Ok(agent_digest(hash, decoded.is_mainnet))
}

/// The venue's msgpack of an action serialized to `json`: rmp_serde's encoding
/// of the same struct, since JSON keeps field order and every wire field is a
/// string, bool, unsigned integer, option or sequence.
pub(crate) fn msgpack(json: &str) -> Result<Vec<u8>, SignError> {
    let action: Node = serde_json::from_str(json).map_err(|_| SignError::InvalidRequest)?;
    Ok(action.msgpack())
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

// serde_json's exact-number feature presents numbers as this single-entry map.
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

// rmp_serde's exact encoding, hand-rolled so the HL family carries no second
// rmp serializer: smallest int, str, array and map headers; floats always f64.
impl Node {
    fn msgpack(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out);
        out
    }
    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Null => out.push(0xc0),
            Self::Bool(value) => out.push(if *value { 0xc3 } else { 0xc2 }),
            Self::Unsigned(value) => write_unsigned(out, *value),
            Self::Signed(value) => write_signed(out, *value),
            Self::Float(value) => {
                out.push(0xcb);
                out.extend_from_slice(&value.to_be_bytes());
            }
            Self::Str(value) => write_str(out, value),
            Self::Seq(items) => {
                write_header(out, items.len(), 0x90, 0xdc);
                items.iter().for_each(|item| item.write(out));
            }
            Self::Map(fields) => {
                write_header(out, fields.len(), 0x80, 0xde);
                for (key, value) in fields {
                    write_str(out, key);
                    value.write(out);
                }
            }
        }
    }
}

fn write_unsigned(out: &mut Vec<u8>, value: u64) {
    match value {
        0..0x80 => out.push(value as u8),
        0x80..0x100 => out.extend_from_slice(&[0xcc, value as u8]),
        0x100..0x1_0000 => {
            out.push(0xcd);
            out.extend_from_slice(&(value as u16).to_be_bytes());
        }
        0x1_0000..0x1_0000_0000 => {
            out.push(0xce);
            out.extend_from_slice(&(value as u32).to_be_bytes());
        }
        _ => {
            out.push(0xcf);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

fn write_signed(out: &mut Vec<u8>, value: i64) {
    match value {
        0.. => write_unsigned(out, value as u64),
        -32..0 => out.push(value as u8),
        -128..-32 => out.extend_from_slice(&[0xd0, value as u8]),
        -32_768..-128 => {
            out.push(0xd1);
            out.extend_from_slice(&(value as i16).to_be_bytes());
        }
        -2_147_483_648..-32_768 => {
            out.push(0xd2);
            out.extend_from_slice(&(value as i32).to_be_bytes());
        }
        _ => {
            out.push(0xd3);
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
}

fn write_str(out: &mut Vec<u8>, value: &str) {
    match value.len() {
        len @ 0..32 => out.push(0xa0 | len as u8),
        len @ 32..256 => out.extend_from_slice(&[0xd9, len as u8]),
        len @ 256..0x1_0000 => {
            out.push(0xda);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        len => {
            out.push(0xdb);
            out.extend_from_slice(&(len as u32).to_be_bytes());
        }
    }
    out.extend_from_slice(value.as_bytes());
}

/// Fix, 16-bit or 32-bit length header; `wide` is the 16-bit marker (32-bit is next).
fn write_header(out: &mut Vec<u8>, len: usize, fix: u8, wide: u8) {
    match len {
        0..16 => out.push(fix | len as u8),
        16..0x1_0000 => {
            out.push(wide);
            out.extend_from_slice(&(len as u16).to_be_bytes());
        }
        _ => {
            out.push(wide + 1);
            out.extend_from_slice(&(len as u32).to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Node;
    use serde::ser::{SerializeMap, SerializeSeq, Serializer};

    /// rmp_serde's encoding of a Node, the oracle for the hand-rolled writer.
    struct Oracle<'a>(&'a Node);
    impl serde::Serialize for Oracle<'_> {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            match self.0 {
                Node::Null => serializer.serialize_unit(),
                Node::Bool(value) => serializer.serialize_bool(*value),
                Node::Unsigned(value) => serializer.serialize_u64(*value),
                Node::Signed(value) => serializer.serialize_i64(*value),
                Node::Float(value) => serializer.serialize_f64(*value),
                Node::Str(value) => serializer.serialize_str(value),
                Node::Seq(items) => {
                    let mut seq = serializer.serialize_seq(Some(items.len()))?;
                    for item in items {
                        seq.serialize_element(&Oracle(item))?;
                    }
                    seq.end()
                }
                Node::Map(fields) => {
                    let mut map = serializer.serialize_map(Some(fields.len()))?;
                    for (key, value) in fields {
                        map.serialize_entry(key, &Oracle(value))?;
                    }
                    map.end()
                }
            }
        }
    }

    /// The serde-derived reader the Node reader replaced, kept as its oracle.
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    #[allow(dead_code)]
    struct DerivedDecoded<'a> {
        kind: &'a str,
        #[serde(borrow)]
        action: &'a serde_json::value::RawValue,
        nonce: u64,
        #[serde(default)]
        vault_address: Option<&'a str>,
        #[serde(default)]
        expires_after: Option<u64>,
        is_mainnet: bool,
    }

    #[test]
    fn statement_fields_are_as_strict_as_the_derived_reader() {
        let base = r#""kind":"l1","action":{"type":"order"},"nonce":7,"isMainnet":true"#;
        let cases = [
            format!("{{{base}}}"),
            format!(r#"{{{base},"vaultAddress":"0xabc"}}"#),
            format!(r#"{{{base},"vaultAddress":null,"expiresAfter":null}}"#),
            format!(r#"{{{base},"expiresAfter":9}}"#),
            format!(r#"{{{base},"expiresAfter":-1}}"#),
            format!(r#"{{{base},"vaultAddress":7}}"#),
            format!(r#"{{{base},"other":1}}"#),
            format!(r#"{{{base},"nonce":8}}"#),
            r#"{"kind":"l1","action":{},"nonce":7}"#.into(),
            r#"{"kind":"l1","nonce":7,"isMainnet":true}"#.into(),
            r#"{"kind":1,"action":{},"nonce":7,"isMainnet":true}"#.into(),
            r#"{"kind":"l1","action":{},"nonce":-7,"isMainnet":true}"#.into(),
            r#"{"kind":"l1","action":{},"nonce":7.5,"isMainnet":true}"#.into(),
            r#"{"kind":"l1","action":{},"nonce":7,"isMainnet":"true"}"#.into(),
            r#"{"kind":"l1","action":null,"nonce":7,"isMainnet":false}"#.into(),
            r#"[1]"#.into(),
        ];
        for case in &cases {
            let node: Option<Node> = serde_json::from_str(case).ok();
            let read = node.as_ref().and_then(super::Decoded::read).is_some();
            let derived = serde_json::from_str::<DerivedDecoded<'_>>(case).is_ok();
            assert_eq!(read, derived, "{case}");
        }
    }

    #[test]
    fn statement_msgpack_matches_rmp_serde_at_every_header_boundary() {
        let mut nodes = vec![Node::Null, Node::Bool(false), Node::Bool(true)];
        for value in [
            0,
            127,
            128,
            255,
            256,
            65_535,
            65_536,
            u32::MAX as u64,
            1 << 32,
            u64::MAX,
        ] {
            nodes.push(Node::Unsigned(value));
        }
        for value in [
            -1,
            -32,
            -33,
            -128,
            -129,
            -32_768,
            -32_769,
            i32::MIN as i64,
            i32::MIN as i64 - 1,
            i64::MIN,
        ] {
            nodes.push(Node::Signed(value));
        }
        for value in [0.5, -1.25, 1e300, f64::MIN_POSITIVE] {
            nodes.push(Node::Float(value));
        }
        for len in [0, 31, 32, 255, 256, 65_535, 65_536] {
            nodes.push(Node::Str("x".repeat(len)));
        }
        for len in [0, 15, 16, 65_535, 65_536] {
            nodes.push(Node::Seq((0..len).map(|_| Node::Null).collect()));
            nodes.push(Node::Map(
                (0..len)
                    .map(|i| (i.to_string(), Node::Unsigned(i as u64)))
                    .collect(),
            ));
        }
        nodes.push(Node::Map(vec![
            ("type".into(), Node::Str("order".into())),
            (
                "orders".into(),
                Node::Seq(vec![Node::Map(vec![("a".into(), Node::Signed(-7))])]),
            ),
        ]));
        for node in &nodes {
            assert_eq!(
                node.msgpack(),
                rmp_serde::to_vec_named(&Oracle(node)).unwrap(),
                "{node:?}"
            );
        }
    }
}
