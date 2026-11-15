//! Closed v1 `d` payloads on the existing sign-req/sign-res envelopes.
use super::{Family, Statement, identity::Venue};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_engine::signing::SignError;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
const MAX_BYTES: usize = 4096;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
#[derive(Clone, Deserialize, Serialize)]
enum Kind {
    #[serde(rename = "join-proof")]
    JoinProof,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    id: String,
    #[serde(rename = "type")]
    kind: Kind,
    exchange_name: Venue,
    account_name: String,
    instance_id: String,
    join_proof: Challenge,
}
#[derive(Clone, Serialize)]
struct Challenge {
    version: u8,
    statement: Statement,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    id: String,
    #[serde(rename = "type", deserialize_with = "super::string_discriminant")]
    kind: Kind,
    #[serde(deserialize_with = "super::string_discriminant")]
    exchange_name: Venue,
    account_name: String,
    instance_id: String,
    join_proof: InputChallenge,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputChallenge {
    version: u8,
    statement: Box<RawValue>,
}
impl Request {
    pub fn new(
        id: &str,
        venue: Venue,
        account: &str,
        instance: &str,
        statement: Statement,
    ) -> Result<Self, SignError> {
        label(id, 128)?;
        label(account, 512)?;
        label(instance, 128)?;
        if statement.issued_at_ms > MAX_SAFE_INTEGER {
            return Err(SignError::InvalidRequest);
        }
        Ok(Self {
            id: id.into(),
            kind: Kind::JoinProof,
            exchange_name: venue,
            account_name: account.into(),
            instance_id: instance.into(),
            join_proof: Challenge {
                version: 1,
                statement,
            },
        })
    }
    /// Preserve raw JSON until this decoder has checked duplicate/unknown fields.
    pub fn from_json(input: &str) -> Result<Self, SignError> {
        bounded(input)?;
        let raw: Input = serde_json::from_str(input).map_err(|_| SignError::InvalidRequest)?;
        let Kind::JoinProof = raw.kind;
        if raw.join_proof.version != 1 {
            return Err(SignError::InvalidRequest);
        }
        Self::new(
            &raw.id,
            raw.exchange_name,
            &raw.account_name,
            &raw.instance_id,
            Statement::from_json(raw.join_proof.statement.get())?,
        )
    }
    pub fn statement(&self) -> &Statement {
        &self.join_proof.statement
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn account_name(&self) -> &str {
        &self.account_name
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn venue(&self) -> Venue {
        self.exchange_name
    }
}
#[derive(Clone, Deserialize, Serialize)]
pub enum Refusal {
    #[serde(rename = "Join proof refused")]
    Refused,
    #[serde(rename = "Join proof unavailable")]
    Unavailable,
}
#[derive(Serialize)]
pub struct Signed {
    id: String,
    #[serde(rename = "joinProof")]
    proof: Signature,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Signature {
    version: u8,
    signature: String,
}
#[derive(Serialize)]
pub struct Refused {
    id: String,
    error: Refusal,
}
#[derive(Serialize)]
#[serde(untagged)]
pub enum Response {
    Signed(Signed),
    Refused(Refused),
}
#[derive(Deserialize)]
#[serde(untagged)]
enum ParsedResponse {
    Signed(InputSigned),
    Refused(InputRefused),
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputSigned {
    id: String,
    #[serde(rename = "joinProof")]
    proof: Signature,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputRefused {
    id: String,
    #[serde(deserialize_with = "super::string_discriminant")]
    error: Refusal,
}
impl Response {
    pub fn signed(id: &str, family: Family, bytes: &[u8]) -> Result<Self, SignError> {
        label(id, 128)?;
        if bytes.len() != width(family) {
            return Err(SignError::InvalidResult);
        }
        Ok(Self::Signed(Signed {
            id: id.into(),
            proof: Signature {
                version: 1,
                signature: STANDARD.encode(bytes),
            },
        }))
    }
    pub fn refused(id: &str, error: Refusal) -> Result<Self, SignError> {
        label(id, 128)?;
        Ok(Self::Refused(Refused {
            id: id.into(),
            error,
        }))
    }
    /// Shape validation only; callers must still run LocalProof::verify.
    pub fn from_json(input: &str, family: Family) -> Result<Self, SignError> {
        bounded(input)?;
        match serde_json::from_str(input).map_err(|_| SignError::InvalidResult)? {
            ParsedResponse::Signed(value) => {
                if value.proof.version != 1 {
                    return Err(SignError::InvalidResult);
                }
                let bytes = STANDARD
                    .decode(&value.proof.signature)
                    .map_err(|_| SignError::InvalidResult)?;
                if STANDARD.encode(&bytes) != value.proof.signature {
                    return Err(SignError::InvalidResult);
                }
                Self::signed(&value.id, family, &bytes)
            }
            ParsedResponse::Refused(value) => Self::refused(&value.id, value.error),
        }
    }
    pub fn id(&self) -> &str {
        match self {
            Self::Signed(value) => &value.id,
            Self::Refused(value) => &value.id,
        }
    }
    pub fn signature(&self) -> Option<Vec<u8>> {
        match self {
            Self::Signed(value) => STANDARD.decode(&value.proof.signature).ok(),
            Self::Refused(_) => None,
        }
    }
}
fn width(family: Family) -> usize {
    match family {
        Family::Ethereum => 65,
        Family::Schnorr => 80,
        _ => 64,
    }
}
fn bounded(value: &str) -> Result<(), SignError> {
    if value.len() > MAX_BYTES {
        Err(SignError::InvalidRequest)
    } else {
        Ok(())
    }
}
fn label(value: &str, max: usize) -> Result<(), SignError> {
    if value.is_empty()
        || value.len() > max
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        Err(SignError::InvalidRequest)
    } else {
        Ok(())
    }
}
