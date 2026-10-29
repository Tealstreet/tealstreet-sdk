//! Compatibility inputs contain canonical bytes, never caller-chosen key refs.
//! The SDK retains its existing serializers; family recipes build atomic work.
use safe_cex_engine::signing::{
    Context, Identities, Identity, Operation, SignError, SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::signing_policy::{RemoteSigningPolicy, SignatureEncoding};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
pub enum OutputFormat {
    #[serde(rename = "hex")]
    Hex,
    #[serde(rename = "base64")]
    Base64,
    #[serde(rename = "hex-base64")]
    Base64Hex,
    /// Bitunix: `hex(sha256(hex(sha256(toSign)) ‖ material))`, never an HMAC.
    #[serde(rename = "bitunix-double-sha256")]
    BitunixDoubleSha256,
    /// Hyperliquid: `toSign` is the structured L1 statement; the device
    /// re-derives the digest and returns `0x` hex of `r || s || v`.
    #[serde(rename = "hyperliquid-l1")]
    HyperliquidL1,
    /// Hyperliquid: `toSign` is the signed-in account; returns the agent address.
    #[serde(rename = "hyperliquid-address")]
    HyperliquidAddress,
    /// Lighter: `toSign` is the transaction or auth statement; the device
    /// re-derives the signed words and returns `0x` hex of each 80-byte
    /// signature, concatenated in statement order.
    #[serde(rename = "lighter-tx")]
    LighterTx,
}

/// The signed-in account a device signs for, captured at construction.
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub key: Option<String>,
    pub vault: Option<String>,
    pub testnet: bool,
    /// Lighter: the account index, when configured as the Application ID.
    pub application_id: Option<String>,
    pub api_key_index: Option<String>,
}

pub enum Input {
    Credential {},
    Http {
        payload: String,
        stamp_ms: f64,
        revision: String,
    },
    WsAuth {
        to_sign: String,
        output_format: OutputFormat,
    },
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum InputKind {
    Credential,
    Http,
    WsAuth,
}

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "camelCase")]
enum InputField {
    Kind,
    Payload,
    StampMs,
    Revision,
    ToSign,
    OutputFormat,
}

/// Hand-written in place of serde's buffered internally-tagged enum, which
/// cost wasm size. Accepts exactly what that derive accepted: a closed map
/// keyed by `kind`, or a sequence of the tag followed by the variant's fields.
struct InputVisitor;

fn store<'de, A: serde::de::MapAccess<'de>, T: Deserialize<'de>>(
    map: &mut A,
    slot: &mut Option<T>,
    name: &'static str,
) -> Result<(), A::Error> {
    if slot.is_some() {
        return Err(serde::de::Error::duplicate_field(name));
    }
    *slot = Some(map.next_value()?);
    Ok(())
}

fn element<'de, A: serde::de::SeqAccess<'de>, T: Deserialize<'de>>(
    seq: &mut A,
    index: usize,
) -> Result<T, A::Error> {
    seq.next_element()?
        .ok_or_else(|| serde::de::Error::invalid_length(index, &"a hosted signing input"))
}

impl<'de> serde::de::Visitor<'de> for InputVisitor {
    type Value = Input;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a hosted signing input")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Input, A::Error> {
        let (mut kind, mut payload, mut stamp_ms, mut revision, mut to_sign, mut output_format) =
            (None, None, None, None, None, None);
        while let Some(field) = map.next_key()? {
            match field {
                InputField::Kind => store(&mut map, &mut kind, "kind")?,
                InputField::Payload => store(&mut map, &mut payload, "payload")?,
                InputField::StampMs => store(&mut map, &mut stamp_ms, "stampMs")?,
                InputField::Revision => store(&mut map, &mut revision, "revision")?,
                InputField::ToSign => store(&mut map, &mut to_sign, "toSign")?,
                InputField::OutputFormat => store(&mut map, &mut output_format, "outputFormat")?,
            }
        }
        let kind = kind.ok_or_else(|| serde::de::Error::missing_field("kind"))?;
        match (kind, payload, stamp_ms, revision, to_sign, output_format) {
            (InputKind::Credential, None, None, None, None, None) => Ok(Input::Credential {}),
            (InputKind::Http, Some(payload), Some(stamp_ms), Some(revision), None, None) => {
                Ok(Input::Http {
                    payload,
                    stamp_ms,
                    revision,
                })
            }
            (InputKind::WsAuth, None, None, None, Some(to_sign), Some(output_format)) => {
                Ok(Input::WsAuth {
                    to_sign,
                    output_format,
                })
            }
            _ => Err(serde::de::Error::custom("invalid hosted signing input")),
        }
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Input, A::Error> {
        Ok(match element(&mut seq, 0)? {
            InputKind::Credential => Input::Credential {},
            InputKind::Http => Input::Http {
                payload: element(&mut seq, 0)?,
                stamp_ms: element(&mut seq, 1)?,
                revision: element(&mut seq, 2)?,
            },
            InputKind::WsAuth => Input::WsAuth {
                to_sign: element(&mut seq, 0)?,
                output_format: element(&mut seq, 1)?,
            },
        })
    }
}

impl<'de> Deserialize<'de> for Input {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(InputVisitor)
    }
}

/// Formatting metadata remains bound to its one capture token.
#[derive(Clone, Copy)]
pub enum OutputKind {
    Credential,
    Http(RemoteSigningPolicy),
    /// Bitunix HTTP: the double-SHA256 work, with the MAC as its last value.
    DoubleSha256Http(RemoteSigningPolicy),
    WsAuth(OutputFormat),
    /// Lighter: this many 80-byte signatures, in statement order.
    Lighter(usize),
}

impl OutputKind {
    pub fn header_policy(self) -> safe_cex_engine::signing_headers::ResultHeaderPolicy {
        match self {
            Self::Http(policy) | Self::DoubleSha256Http(policy) => policy.header_policy(),
            _ => Default::default(),
        }
    }

    /// Value count and MAC position of the captured work's result.
    fn layout(self) -> (usize, Option<usize>) {
        match self {
            Self::Credential => (1, None),
            Self::Http(policy) if policy.passphrase_header.is_some() => (3, Some(1)),
            Self::DoubleSha256Http(_) | Self::WsAuth(OutputFormat::BitunixDoubleSha256) => {
                (3, Some(2))
            }
            Self::WsAuth(OutputFormat::HyperliquidAddress) => (1, None),
            Self::WsAuth(OutputFormat::HyperliquidL1) => (2, None),
            Self::Lighter(signatures) => (signatures, None),
            _ => (2, Some(1)),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<(String, String)>>,
}

/// Both the layout and presentation policy are captured with the canonical work.
/// This helper does not admit a venue or resolve setup's absent-only fallback.
fn http_work(
    identity: &Identity,
    payload: String,
    context: Context,
    receipt: SignReceipt,
    policy: RemoteSigningPolicy,
) -> Result<(SignRequest, OutputKind), SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    receipt.validate()?;
    let mut work = safe_cex_engine::signing::hmac_sha256_work(
        &identity.credential,
        identity.signing_ref()?,
        vec![safe_cex_engine::signing::PayloadPart::Bytes {
            bytes: payload.into_bytes(),
        }],
        context,
        receipt,
    );
    if policy.passphrase_header.is_some() {
        work.operations.push(Operation::Present {
            key: identity.passphrase.clone().ok_or(SignError::Unavailable)?,
        });
    }
    // The existing capture admission owns canonical-work bounds before any
    // provider call; descriptor construction preserves that rejection boundary.
    Ok((work, OutputKind::Http(policy)))
}

/// `[Present, Sha256(toSign), Sign Sha256SecretSuffix(hex of that digest)]`:
/// the engine's Bitunix recipe over a caller-built preimage, so the browser
/// returns the same bytes as a local-key account.
fn double_sha256_work(
    identity: &Identity,
    to_sign: String,
    context: Context,
    receipt: SignReceipt,
) -> Result<SignRequest, SignError> {
    use safe_cex_engine::signing::{Algorithm, Encoding, PayloadPart};
    receipt.validate()?;
    Ok(SignRequest {
        receipt,
        context,
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sha256 {
                payload: vec![PayloadPart::Bytes {
                    bytes: to_sign.into_bytes(),
                }],
            },
            Operation::Sign {
                algorithm: Algorithm::Sha256SecretSuffix,
                key: identity.signing_ref()?.clone(),
                payload: vec![PayloadPart::Result {
                    index: 1,
                    encoding: Encoding::Hex,
                }],
            },
        ],
    })
}

fn double_sha256(venue: &str) -> bool {
    cfg!(feature = "bitunix") && venue == "bitunix"
}

/// Families with a hosted request codec. AsterDEX V3 signs with a wallet key,
/// which must never become an HMAC key. Only an explicit setup-resolved V1
/// qualifies: inference from options cannot see material held by the setup.
pub fn admitted(venue: &str, options: &crate::EngineOptions) -> bool {
    let _ = options;
    match venue {
        #[cfg(feature = "asterdex")]
        "asterdex" => {
            options.asterdex.api_version == Some(safe_cex_venue_asterdex::sign::Version::V1)
        }
        _ => family(venue) || hyperliquid(venue) || lighter(venue),
    }
}

fn family(venue: &str) -> bool {
    let binance = cfg!(feature = "binance")
        && matches!(
            venue,
            "binance" | "binancepm" | "binanceinverse" | "binancespot"
        );
    let bybit = cfg!(feature = "bybit")
        && matches!(
            venue,
            "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader"
        );
    let okx = cfg!(feature = "okx") && matches!(venue, "okx" | "okxeu" | "okxdemo" | "okxeudemo");
    let bitget =
        cfg!(feature = "bitget") && matches!(venue, "bitget" | "bitgetusdc" | "bitgetinverse");
    binance
        || bybit
        || okx
        || bitget
        || (cfg!(feature = "bingx") && venue == "bingx")
        || double_sha256(venue)
        || (cfg!(feature = "phemex") && venue == "phemex")
        || (cfg!(feature = "asterdex") && venue == "asterdex")
}

pub fn prepare(
    venue: &str,
    identities: &Identities,
    input: Input,
) -> Result<(SignRequest, OutputKind), SignError> {
    prepare_for(venue, identities, &Account::default(), input)
}

fn lighter(venue: &str) -> bool {
    cfg!(feature = "lighter") && matches!(venue, "lighter" | "lighter-robinhood")
}

/// Lighter signs only words it re-derives from an allowlisted statement for
/// this deployment's chain and this account's API key.
#[cfg(feature = "lighter")]
fn lighter_work(
    venue: &str,
    identity: &Identity,
    account: &Account,
    context: Context,
    to_sign: &str,
) -> Result<(SignRequest, OutputKind), SignError> {
    use safe_cex_engine::signing::{Algorithm, PayloadPart};
    use safe_cex_venue_lighter::sign::{chain_id, statement};
    let decimal = |value: Option<&str>| value.and_then(|text| text.trim().parse::<u64>().ok());
    let api_key_index = decimal(account.api_key_index.as_deref())
        .and_then(|index| u8::try_from(index).ok())
        .ok_or(SignError::Unavailable)?;
    let owner = statement::Owner {
        chain_id: chain_id(venue == "lighter-robinhood", account.testnet),
        account_index: decimal(account.application_id.as_deref()),
        api_key_index,
    };
    let derived = statement::derive(to_sign, &owner).map_err(|_| SignError::InvalidRequest)?;
    let signing = identity.signing_ref()?.clone();
    // The hedged nonce is keyed by the scalar and the message, so fixed entropy
    // degrades to deterministic signing, never to a reused nonce.
    let operations = derived
        .preimages
        .into_iter()
        .map(|preimage| {
            let mut bytes = vec![0; 32];
            bytes.extend_from_slice(&preimage);
            Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key: signing.clone(),
                payload: vec![PayloadPart::Bytes { bytes }],
            }
        })
        .collect::<Vec<_>>();
    let signatures = operations.len();
    Ok((
        SignRequest {
            receipt: SignReceipt::default(),
            context,
            operations,
        },
        OutputKind::Lighter(signatures),
    ))
}

fn hyperliquid(venue: &str) -> bool {
    cfg!(feature = "hyperliquid") && matches!(venue, "hyperliquid" | "hyperliquidspot")
}

/// Wallet signing never answers a bare digest: the device derives it from an
/// allowlisted statement for this account's network and vault.
#[cfg(feature = "hyperliquid")]
fn hyperliquid_work(
    identity: &Identity,
    account: &Account,
    context: Context,
    input: Input,
) -> Result<(SignRequest, OutputKind), SignError> {
    use safe_cex_engine::signing::{Algorithm, PayloadPart};
    use safe_cex_venue_hyperliquid::sign::statement;
    let Input::WsAuth {
        to_sign,
        output_format,
    } = input
    else {
        return Err(SignError::Unsupported);
    };
    let signing = identity.signing_ref()?.clone();
    let mut operations = vec![Operation::PublicAddress {
        key: signing.clone(),
    }];
    match output_format {
        OutputFormat::HyperliquidAddress => {
            let own = account.key.as_deref().ok_or(SignError::Unavailable)?;
            if !to_sign.eq_ignore_ascii_case(own) {
                return Err(SignError::InvalidRequest);
            }
        }
        OutputFormat::HyperliquidL1 => {
            let digest = statement::derive(
                &to_sign,
                &statement::Account {
                    mainnet: !account.testnet,
                    vault: account.vault.as_deref(),
                },
            )?;
            operations.push(Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: signing,
                payload: vec![PayloadPart::Bytes {
                    bytes: digest.to_vec(),
                }],
            });
        }
        _ => return Err(SignError::InvalidRequest),
    }
    Ok((
        SignRequest {
            receipt: SignReceipt::default(),
            context,
            operations,
        },
        OutputKind::WsAuth(output_format),
    ))
}

pub fn prepare_for(
    venue: &str,
    identities: &Identities,
    account: &Account,
    input: Input,
) -> Result<(SignRequest, OutputKind), SignError> {
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let context = Context {
        venue: venue.into(),
        method: "HOSTED".into(),
        ..Default::default()
    };
    let wallet_format = matches!(
        &input,
        Input::WsAuth {
            output_format: OutputFormat::HyperliquidL1 | OutputFormat::HyperliquidAddress,
            ..
        }
    );
    if wallet_format != hyperliquid(venue) {
        return Err(if wallet_format {
            SignError::InvalidRequest
        } else {
            SignError::Unsupported
        });
    }
    // Lighter signs nothing but its own statements.
    let lighter_format = matches!(
        &input,
        Input::WsAuth {
            output_format: OutputFormat::LighterTx,
            ..
        }
    );
    if lighter_format != lighter(venue) {
        return Err(if lighter_format {
            SignError::InvalidRequest
        } else {
            SignError::Unsupported
        });
    }
    #[cfg(feature = "lighter")]
    if let Input::WsAuth { to_sign, .. } = &input
        && lighter_format
    {
        return lighter_work(venue, identity, account, context, to_sign);
    }
    #[cfg(feature = "hyperliquid")]
    if wallet_format {
        return hyperliquid_work(identity, account, context, input);
    }
    let _ = account;
    if !family(venue) {
        return Err(SignError::Unsupported);
    }
    match input {
        Input::Credential {} => Ok((
            SignRequest {
                receipt: SignReceipt::default(),
                context,
                operations: vec![Operation::Present {
                    key: identity.credential.clone(),
                }],
            },
            OutputKind::Credential,
        )),
        Input::Http {
            payload,
            stamp_ms,
            revision: _,
        } => {
            let receipt = SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
            };
            if double_sha256(venue) {
                let policy = crate::remote_signing_policy(venue);
                return Ok((
                    double_sha256_work(identity, payload, context, receipt)?,
                    OutputKind::DoubleSha256Http(policy),
                ));
            }
            http_work(
                identity,
                payload,
                context,
                receipt,
                crate::remote_signing_policy(venue),
            )
        }
        Input::WsAuth {
            to_sign,
            output_format,
        } => {
            // AsterDEX authenticates its private socket with a listen key.
            if to_sign.is_empty() || venue == "asterdex" {
                return Err(SignError::InvalidRequest);
            }
            // The output format names the scheme, so it must match the family's.
            let fused = matches!(output_format, OutputFormat::BitunixDoubleSha256);
            if fused != double_sha256(venue) {
                return Err(SignError::InvalidRequest);
            }
            if fused {
                return Ok((
                    double_sha256_work(identity, to_sign, context, SignReceipt::default())?,
                    OutputKind::WsAuth(output_format),
                ));
            }
            Ok((
                safe_cex_engine::signing::hmac_sha256_work(
                    &identity.credential,
                    identity.signing_ref()?,
                    vec![safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: to_sign.into_bytes(),
                    }],
                    context,
                    SignReceipt::default(),
                ),
                OutputKind::WsAuth(output_format),
            ))
        }
    }
}

pub fn finish(kind: OutputKind, result: &SignResult) -> Result<Output, SignError> {
    let (expected, mac_index) = kind.layout();
    if result.values.len() != expected {
        return Err(SignError::InvalidResult);
    }
    if let OutputKind::WsAuth(
        format @ (OutputFormat::HyperliquidAddress | OutputFormat::HyperliquidL1),
    ) = kind
    {
        if !result.headers.is_empty() || result.values[0].len() != 20 {
            return Err(SignError::InvalidResult);
        }
        let value = match format {
            OutputFormat::HyperliquidAddress => result.values[0].clone(),
            _ => {
                let mut signature = result.values[1].clone();
                if signature.len() != 65 {
                    return Err(SignError::InvalidResult);
                }
                match &mut signature[64] {
                    v @ (0 | 1) => *v += 27,
                    27 | 28 => {}
                    _ => return Err(SignError::InvalidResult),
                }
                signature
            }
        };
        return Ok(Output {
            credential: None,
            signature: Some(format!("0x{}", safe_cex_crypto::hex(&value))),
            headers: None,
        });
    }
    if let OutputKind::Lighter(_) = kind {
        if !result.headers.is_empty() || result.values.iter().any(|value| value.len() != 80) {
            return Err(SignError::InvalidResult);
        }
        return Ok(Output {
            credential: None,
            signature: Some(format!(
                "0x{}",
                safe_cex_crypto::hex(&result.values.concat())
            )),
            headers: None,
        });
    }
    let mut headers = Vec::new();
    if let OutputKind::Http(policy) = kind
        && let Some(header) = policy.passphrase_header
    {
        let value = std::str::from_utf8(&result.values[2]).map_err(|_| SignError::InvalidResult)?;
        if value.is_empty() {
            return Err(SignError::InvalidResult);
        }
        headers.push((header.into(), value.into()));
        safe_cex_engine::signing_headers::validate(&headers)?;
    }
    // Trusted native presentation precedes the same declared callback precedence
    // used by dispatch. Case aliases cannot bypass a protected header name.
    kind.header_policy().apply(&mut headers, &result.headers)?;
    safe_cex_engine::signing_headers::validate(&headers)?;
    let key = result.values.first().ok_or(SignError::InvalidResult)?;
    let key = std::str::from_utf8(key).map_err(|_| SignError::InvalidResult)?;
    if key.is_empty() || key.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    if matches!(kind, OutputKind::Credential) {
        return Ok(Output {
            credential: Some(key.into()),
            signature: None,
            headers: None,
        });
    }
    let mac = mac_index
        .and_then(|index| result.values.get(index))
        .filter(|mac| mac.len() == 32)
        .ok_or(SignError::InvalidResult)?;
    // The double-SHA256 inner digest is a 32-byte value of its own.
    if mac_index == Some(2) && result.values[1].len() != 32 {
        return Err(SignError::InvalidResult);
    }
    let signature = match kind {
        OutputKind::Credential => unreachable!(),
        OutputKind::Http(policy) | OutputKind::DoubleSha256Http(policy) => {
            match policy.signature_encoding {
                SignatureEncoding::Hex => safe_cex_crypto::hex(mac),
                SignatureEncoding::Base64 => {
                    safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64)
                }
            }
        }
        OutputKind::WsAuth(OutputFormat::Hex) => safe_cex_crypto::hex(mac),
        OutputKind::WsAuth(OutputFormat::Base64) => {
            safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64)
        }
        OutputKind::WsAuth(OutputFormat::Base64Hex) => safe_cex_crypto::encode(
            safe_cex_crypto::hex(mac).as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        ),
        OutputKind::WsAuth(OutputFormat::BitunixDoubleSha256) => safe_cex_crypto::hex(mac),
        OutputKind::WsAuth(
            OutputFormat::HyperliquidL1
            | OutputFormat::HyperliquidAddress
            | OutputFormat::LighterTx,
        )
        | OutputKind::Lighter(_) => {
            return Err(SignError::InvalidResult);
        }
    };
    Ok(Output {
        credential: matches!(kind, OutputKind::Http(_) | OutputKind::DoubleSha256Http(_))
            .then(|| key.into()),
        signature: Some(signature),
        headers: (!headers.is_empty()).then_some(headers),
    })
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{KeyRef, SetupFieldPolicy};
    use safe_cex_engine::signing_headers::MAX_VALUE_BYTES;

    const PAYLOAD: &str = "1709209849123GET/api/account?x=%2B&x=second";
    const MAC: &str = "SL/xd+lRj5sirx9sxjcCAiy6l78Q3WwR8844XzqxcNs=";

    fn fixture() -> (Identity, LocalSigner) {
        let identity = Identity::new(
            KeyRef("ref:owned-primary-public".into()),
            Some(KeyRef("ref:owned-primary-signing".into())),
        )
        .with_passphrase(Some(KeyRef("ref:owned-primary-presentation".into())));
        let mut signer = LocalSigner::new();
        signer.insert(identity.credential.clone(), b"public-identity".to_vec());
        signer.insert(
            identity.signing_ref().unwrap().clone(),
            b"fixture-only-provider".to_vec(),
        );
        signer.insert(
            identity.passphrase.clone().unwrap(),
            b"fixture-presentation".to_vec(),
        );
        (identity, signer)
    }

    fn prepared(identity: &Identity, policy: RemoteSigningPolicy) -> (SignRequest, OutputKind) {
        http_work(
            identity,
            PAYLOAD.into(),
            Context::default(),
            SignReceipt::default(),
            policy,
        )
        .unwrap()
    }

    fn families() -> [(&'static str, &'static [&'static str]); 2] {
        [
            ("okx", &["OK-ACCESS-PASSPHRASE"]),
            ("bitget", &["ACCESS-PASSPHRASE"]),
        ]
    }

    #[test]
    fn two_families_use_one_owned_atomic_work_and_formatter_with_explicit_admission() {
        let (identity, signer) = fixture();
        for (venue, names) in families() {
            let policy = crate::remote_signing_policy(venue);
            assert!(policy.enabled && !policy.payload_supported);
            assert_eq!(policy.signature_encoding, SignatureEncoding::Base64);
            assert_eq!(policy.passphrase_header, Some(names[0]));
            let (work, kind) = prepared(&identity, policy);
            assert_eq!(work.operations.len(), 3);
            assert!(
                matches!(&work.operations[2], Operation::Present { key } if Some(key) == identity.passphrase.as_ref())
            );
            let result = signer.sign_now(&work).unwrap();
            work.validate_result(&result).unwrap();
            let output = finish(kind, &result).unwrap();
            // Independent Python stdlib HMAC vector, not the formatter's oracle.
            assert_eq!(output.signature.as_deref(), Some(MAC));
            assert_eq!(output.credential.as_deref(), Some("public-identity"));
            assert_eq!(
                output.headers.unwrap(),
                vec![(names[0].into(), "fixture-presentation".into())]
            );
            let capture = prepare(
                venue,
                &Identities {
                    primary: Some(identity.clone()),
                    secondary: None,
                },
                Input::Credential {},
            );
            if (venue == "okx" && cfg!(feature = "okx"))
                || (venue == "bitget" && cfg!(feature = "bitget"))
            {
                assert!(capture.is_ok());
            } else {
                assert!(matches!(capture, Err(SignError::Unsupported)));
            }
        }
    }

    #[test]
    fn missing_explicit_refs_do_not_fall_back_but_setup_resolved_aliases_work() {
        let (identity, mut signer) = fixture();
        let secondary = Identity::new(
            KeyRef("ref:owned-secondary-public".into()),
            Some(KeyRef("ref:owned-secondary-signing".into())),
        );
        signer.insert(secondary.credential.clone(), b"secondary-public".to_vec());
        signer.insert(
            secondary.signing_ref().unwrap().clone(),
            b"secondary-fixture-provider".to_vec(),
        );
        for (venue, _) in families() {
            let policy = crate::remote_signing_policy(venue);
            for missing in [None, Some(KeyRef(String::new()))] {
                let bad = identity.clone().with_passphrase(missing);
                assert!(
                    http_work(
                        &bad,
                        PAYLOAD.into(),
                        Context::default(),
                        SignReceipt::default(),
                        policy
                    )
                    .is_err()
                );
            }
            let mut ids = Identities {
                primary: Some(identity.clone()),
                secondary: Some(secondary.clone()),
            };
            safe_cex_engine::signing::CredentialSetupPolicy {
                secondary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                    passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
                    ..Default::default()
                }),
                ..Default::default()
            }
            .apply_to_identities(&mut ids, false)
            .unwrap();
            let (work, kind) = prepared(ids.secondary.as_ref().unwrap(), policy);
            let result = signer.sign_now(&work).unwrap();
            let output = finish(kind, &result).unwrap();
            assert_eq!(output.credential.as_deref(), Some("secondary-public"));
            assert_eq!(output.headers.unwrap()[0].1, "fixture-presentation");
        }
    }

    #[test]
    fn malformed_presentations_or_batch_shapes_never_publish_partial_output() {
        let (identity, signer) = fixture();
        for (venue, _) in families() {
            let (work, kind) = prepared(&identity, crate::remote_signing_policy(venue));
            for invalid in [
                vec![],
                vec![0xff],
                b"injected\r\nheader".to_vec(),
                vec![0; 3],
                vec![b'x'; MAX_VALUE_BYTES + 1],
            ] {
                let mut result = signer.sign_now(&work).unwrap();
                result.values[2] = invalid;
                assert!(matches!(
                    finish(kind, &result),
                    Err(SignError::InvalidResult)
                ));
            }
            for count in [0, 1, 2, 4] {
                let result = SignResult::new(vec![vec![0x37; 32]; count], work.receipt);
                assert!(matches!(
                    finish(kind, &result),
                    Err(SignError::InvalidResult)
                ));
            }
            let mut bad_mac = signer.sign_now(&work).unwrap();
            bad_mac.values[1].pop();
            assert!(matches!(
                finish(kind, &bad_mac),
                Err(SignError::InvalidResult)
            ));
        }
    }

    #[test]
    fn declared_case_insensitive_precedence_and_combined_bounds_are_atomic() {
        let (identity, signer) = fixture();
        for (venue, protected) in families() {
            let base = crate::remote_signing_policy(venue);
            let policy = RemoteSigningPolicy {
                protected_headers: Some(protected),
                ..base
            };
            let (work, kind) = prepared(&identity, policy);
            let mut result = signer.sign_now(&work).unwrap();
            result.headers = vec![(protected[0].to_lowercase(), "callback".into())];
            assert_eq!(
                finish(kind, &result).unwrap().headers.unwrap(),
                vec![(protected[0].into(), "fixture-presentation".into())]
            );
            let (_, override_kind) = prepared(
                &identity,
                RemoteSigningPolicy {
                    protected_headers: Some(&[]),
                    ..base
                },
            );
            assert_eq!(
                finish(override_kind, &result).unwrap().headers.unwrap(),
                vec![(protected[0].into(), "callback".into())]
            );
            assert!(matches!(
                finish(
                    OutputKind::Http(RemoteSigningPolicy {
                        protected_headers: None,
                        ..base
                    }),
                    &result
                ),
                Err(SignError::InvalidResult)
            ));
            result.headers = (0..32)
                .map(|i| (format!("X-Callback-{i}"), "v".into()))
                .collect();
            assert!(matches!(
                finish(kind, &result),
                Err(SignError::InvalidResult)
            ));
        }
    }

    #[test]
    fn presentation_policy_stays_in_the_capture_and_stale_results_cannot_publish() {
        use safe_cex_engine::signing_capture::Capture;
        let (identity, signer) = fixture();
        for (venue, _) in families() {
            for (now, revision) in [(600., 7), (101., 8)] {
                let (work, kind) = prepared(&identity, crate::remote_signing_policy(venue));
                let mut captures = Capture::default();
                let pending = captures
                    .prepare_with_policy((work, kind.header_policy()), kind, 100., 1_000., 7, 500)
                    .unwrap();
                let result = signer.sign_now(&pending.work).unwrap();
                assert!(
                    captures
                        .finish(pending.id, now, 1_001., revision, Ok(result))
                        .is_err()
                );
                assert!(captures.is_empty());
            }
            let mut policy = crate::remote_signing_policy(venue);
            let (work, kind) = prepared(&identity, policy);
            let mut captures = Capture::default();
            let pending = captures
                .prepare_with_policy((work, kind.header_policy()), kind, 100., 1_000., 7, 500)
                .unwrap();
            policy.signature_encoding = SignatureEncoding::Hex;
            assert_eq!(policy.signature_encoding, SignatureEncoding::Hex);
            let result = signer.sign_now(&pending.work).unwrap();
            let (captured, result) = captures
                .finish(pending.id, 101., 1_001., 7, Ok(result))
                .unwrap();
            assert_eq!(
                finish(captured, &result).unwrap().signature.as_deref(),
                Some(MAC)
            );
            assert!(captures.is_empty());
        }
    }

    #[test]
    fn double_sha256_layout_publishes_its_last_value_and_hmac_layouts_are_unchanged() {
        let (identity, signer) = fixture();
        let to_sign = "nonce-abc1700000000api-key-xyz";
        let work = double_sha256_work(
            &identity,
            to_sign.into(),
            Context::default(),
            SignReceipt::default(),
        )
        .unwrap();
        let result = signer.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let http = OutputKind::DoubleSha256Http(crate::remote_signing_policy("bitunix"));
        let ws = OutputKind::WsAuth(OutputFormat::BitunixDoubleSha256);
        for kind in [http, ws] {
            let output = finish(kind, &result).unwrap();
            // Independent Python hashlib vector: hex(sha256(hex(sha256(m)) ‖ k)).
            assert_eq!(
                output.signature.as_deref(),
                Some("4dc47f49a21a0e908e7746858f98e4c3fdcad5f248f37944878380d174aaf38f")
            );
            assert_eq!(
                output.credential.as_deref(),
                matches!(kind, OutputKind::DoubleSha256Http(_)).then_some("public-identity")
            );
            assert!(output.headers.is_none());
            for count in [0, 1, 2, 4] {
                let short = SignResult::new(vec![vec![0x37; 32]; count], work.receipt);
                assert!(matches!(
                    finish(kind, &short),
                    Err(SignError::InvalidResult)
                ));
            }
            for index in [1, 2] {
                let mut bad = result.clone();
                bad.values[index].pop();
                assert!(matches!(finish(kind, &bad), Err(SignError::InvalidResult)));
            }
        }
        // A three-value result never publishes through an HMAC layout.
        for kind in [
            OutputKind::Http(crate::remote_signing_policy("binance")),
            OutputKind::WsAuth(OutputFormat::Hex),
        ] {
            assert!(matches!(
                finish(kind, &result),
                Err(SignError::InvalidResult)
            ));
        }
    }
}

#[cfg(test)]
mod input_parse_tests {
    use super::*;

    /// The wasm host maps every parse failure to `InvalidRequest` this way.
    fn parse(input: &str) -> Result<Input, SignError> {
        serde_json::from_str(input).map_err(|_| SignError::InvalidRequest)
    }

    fn rejected(input: &str) {
        assert!(
            matches!(parse(input), Err(SignError::InvalidRequest)),
            "{input}"
        );
    }

    #[test]
    fn each_kind_is_accepted_in_map_and_sequence_form() {
        for input in [r#"{"kind":"credential"}"#, r#"["credential"]"#] {
            assert!(matches!(parse(input), Ok(Input::Credential {})));
        }
        for input in [
            r#"{"kind":"http","payload":"p","stampMs":1000.5,"revision":"7"}"#,
            r#"{"revision":"7","stampMs":1000.5,"payload":"p","kind":"http"}"#,
            r#"["http","p",1000.5,"7"]"#,
        ] {
            assert!(matches!(
                parse(input),
                Ok(Input::Http { payload, stamp_ms, revision })
                    if payload == "p" && stamp_ms == 1000.5 && revision == "7"
            ));
        }
        for (format, input) in [
            (
                "hex",
                r#"{"kind":"wsAuth","toSign":"x","outputFormat":"hex"}"#,
            ),
            (
                "base64",
                r#"{"kind":"wsAuth","toSign":"x","outputFormat":"base64"}"#,
            ),
            ("hex-base64", r#"["wsAuth","x","hex-base64"]"#),
            (
                "bitunix-double-sha256",
                r#"{"kind":"wsAuth","toSign":"x","outputFormat":"bitunix-double-sha256"}"#,
            ),
            (
                "bitunix-double-sha256",
                r#"["wsAuth","x","bitunix-double-sha256"]"#,
            ),
            ("hyperliquid-l1", r#"["wsAuth","x","hyperliquid-l1"]"#),
            (
                "hyperliquid-address",
                r#"{"kind":"wsAuth","toSign":"x","outputFormat":"hyperliquid-address"}"#,
            ),
        ] {
            let Ok(Input::WsAuth {
                to_sign,
                output_format,
            }) = parse(input)
            else {
                panic!("{input}");
            };
            assert_eq!(to_sign, "x");
            assert_eq!(
                format,
                match output_format {
                    OutputFormat::Hex => "hex",
                    OutputFormat::Base64 => "base64",
                    OutputFormat::Base64Hex => "hex-base64",
                    OutputFormat::BitunixDoubleSha256 => "bitunix-double-sha256",
                    OutputFormat::HyperliquidL1 => "hyperliquid-l1",
                    OutputFormat::HyperliquidAddress => "hyperliquid-address",
                    OutputFormat::LighterTx => "lighter-tx",
                }
            );
        }
    }

    #[test]
    fn unknown_null_duplicate_wrong_kind_and_missing_fields_are_invalid_requests() {
        for input in [
            // Unknown fields, including another kind's field.
            r#"{"kind":"credential","key":"ref:owned-primary"}"#,
            r#"{"kind":"http","payload":"p","stampMs":1,"revision":"7","secondary":true}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"hex","stamp_ms":1}"#,
            r#"{"kind":"credential","toSign":"x"}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"hex","payload":"p"}"#,
            // Nulls are never absence.
            r#"{"kind":null}"#,
            r#"{"kind":"credential","payload":null}"#,
            r#"{"kind":"http","payload":null,"stampMs":1,"revision":"7"}"#,
            r#"{"kind":"http","payload":"p","stampMs":null,"revision":"7"}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":null}"#,
            r#"["http",null,1,"7"]"#,
            // Duplicates.
            r#"{"kind":"credential","kind":"credential"}"#,
            r#"{"kind":"http","payload":"p","payload":"p","stampMs":1,"revision":"7"}"#,
            r#"{"kind":"wsAuth","toSign":"x","toSign":"x","outputFormat":"hex"}"#,
            // Wrong or unknown kind and wrong value types.
            r#"{"kind":"present"}"#,
            r#"{"kind":"Http","payload":"p","stampMs":1,"revision":"7"}"#,
            r#"{"kind":0}"#,
            r#"{"kind":"http","payload":"p","stampMs":1,"revision":7}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"raw"}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"Bitunix-Double-Sha256"}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"bitunixDoubleSha256"}"#,
            r#"{"kind":"wsAuth","toSign":"x","outputFormat":"double-sha256"}"#,
            // Missing required fields, extra sequence elements, non-objects.
            r#"{}"#,
            r#"{"payload":"p","stampMs":1,"revision":"7"}"#,
            r#"{"kind":"http","payload":"p","stampMs":1}"#,
            r#"{"kind":"wsAuth","toSign":"x"}"#,
            r#"["http","p",1]"#,
            r#"["credential","x"]"#,
            r#"["wsAuth","x","hex",null]"#,
            r#"[]"#,
            r#""http""#,
            "null",
        ] {
            rejected(input);
        }
    }
}
