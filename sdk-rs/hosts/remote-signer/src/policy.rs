//! The remote-signing declaration of each venue, read from the same
//! construction-rules.json the SDK and the registry use.
use std::sync::OnceLock;

use serde_json::Value;

const RULES: &str =
    include_str!("../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Hex,
    Base64,
    /// BloFin: base64 of the MAC's lowercase hex text.
    HexBase64,
}
impl Encoding {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Hex => "hex",
            Self::Base64 => "base64",
            Self::HexBase64 => "hex-base64",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtraData {
    BybitExpiry,
    BinanceSpot,
    SecondsTimestamp,
}

#[derive(Clone, Debug)]
pub struct Wire {
    pub exchange_name: String,
    pub extra_data: Option<ExtraData>,
    pub request_id: Option<String>,
    /// `(method, path)` of a seconds login stamp's signed suffix.
    pub seconds_suffix: Option<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassphraseRule {
    Optional,
    RequiredForPrivate,
    Other,
}

#[derive(Clone, Debug)]
pub struct Policy {
    pub enabled: bool,
    pub encoding: Encoding,
    pub passphrase_header: Option<String>,
    pub wire: Option<Wire>,
    pub trim_credential: bool,
    pub passphrase: Option<PassphraseRule>,
}

fn rules() -> &'static Value {
    static PARSED: OnceLock<Value> = OnceLock::new();
    PARSED.get_or_init(|| serde_json::from_str(RULES).expect("construction-rules.json parses"))
}

/// None when the venue is not declared or declares a shape this signer cannot speak.
pub fn policy(venue: &str) -> Option<Policy> {
    let rule = rules().get("venues")?.get(venue)?;
    let remote = rule.get("remoteSigning")?;
    let text =
        |value: &Value, field: &str| value.get(field).and_then(Value::as_str).map(str::to_owned);
    let encoding = match remote.get("signatureEncoding").and_then(Value::as_str) {
        None | Some("hex") => Encoding::Hex,
        Some("base64") => Encoding::Base64,
        Some("hex-base64") => Encoding::HexBase64,
        Some(_) => return None,
    };
    let wire = match remote.get("wire") {
        None => None,
        Some(wire) => Some(Wire {
            exchange_name: text(wire, "exchangeName")?,
            extra_data: match wire.get("extraData").and_then(Value::as_str) {
                None => None,
                Some("bybitExpiry") => Some(ExtraData::BybitExpiry),
                Some("binanceSpot") => Some(ExtraData::BinanceSpot),
                Some("secondsTimestamp") => Some(ExtraData::SecondsTimestamp),
                Some(_) => return None,
            },
            request_id: text(wire, "requestId"),
            seconds_suffix: match wire.get("secondsSuffix") {
                None => None,
                Some(suffix) => Some((text(suffix, "method")?, text(suffix, "path")?)),
            },
        }),
    };
    if wire.as_ref().is_some_and(|wire| {
        (wire.extra_data == Some(ExtraData::SecondsTimestamp)) != wire.seconds_suffix.is_some()
    }) {
        return None;
    }
    let primary = rule
        .get("signerSetup")
        .and_then(|setup| setup.get("primary"));
    Some(Policy {
        enabled: remote.get("enabled").and_then(Value::as_bool) == Some(true),
        encoding,
        passphrase_header: text(remote, "passphraseHeader"),
        wire,
        trim_credential: primary
            .and_then(|primary| primary.get("trimCredential"))
            .and_then(Value::as_bool)
            == Some(true),
        passphrase: primary
            .and_then(|primary| primary.get("passphrase"))
            .and_then(Value::as_str)
            .map(|rule| match rule {
                "optional" => PassphraseRule::Optional,
                "requiredForPrivate" => PassphraseRule::RequiredForPrivate,
                _ => PassphraseRule::Other,
            }),
    })
}
