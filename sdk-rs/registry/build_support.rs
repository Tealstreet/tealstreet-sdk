//! Host-only compiler for the JSON shared with the TypeScript resolver.
use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Input {
    Key,
    Secret,
    RemoteSigner,
    PayloadSigner,
    ApplicationId,
    Vault,
    ParadexAddress,
    SubkeyAddress,
    ApiKeyIndex,
    AsterLocalAuth,
    CanSign,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Rule {
    can_sign: Option<Vec<Vec<Input>>>,
    private_eligible: Option<Vec<Vec<Input>>>,
    explicit_private_error: Option<String>,
    #[serde(default)]
    bridge: BridgeRule,
    #[serde(default)]
    signer_setup: CredentialSetupPolicy,
    #[serde(default)]
    remote_signing: RemoteSigningPolicy,
    /// Family-specific trusted setup mapping, declared rather than name-matched.
    #[serde(default)]
    setup_mapping: Option<SetupMapping>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SetupMapping {
    #[serde(rename = "mexc")]
    Mexc,
    Asterdex,
    #[serde(rename = "apexomni")]
    ApexOmni,
    Lighter,
    Hyperliquid,
    Paradex,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct RemoteSigningPolicy {
    enabled: bool,
    payload_supported: bool,
    payload_rest_only: bool,
    protected_headers: Option<Vec<String>>,
    wire: Option<RemoteSigningWire>,
    signature_encoding: SignatureEncoding,
    passphrase_header: Option<String>,
}

#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SignatureEncoding {
    #[default]
    Hex,
    Base64,
    #[serde(rename = "hex-base64")]
    Base64Hex,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoteSigningWire {
    exchange_name: String,
    extra_data: Option<RemoteExtraData>,
    request_id: Option<String>,
    seconds_suffix: Option<SecondsSuffix>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum RemoteExtraData {
    BybitExpiry,
    BinanceSpot,
    SecondsTimestamp,
}
/// The signed literal after a seconds login stamp, e.g. `GET` + `/user/verify`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SecondsSuffix {
    method: String,
    path: String,
}
impl SecondsSuffix {
    fn valid(&self) -> bool {
        (1..=16).contains(&self.method.len())
            && self.method.bytes().all(|byte| byte.is_ascii_uppercase())
            && (2..=256).contains(&self.path.len())
            && self.path.starts_with('/')
            && self.path.bytes().all(|byte| byte.is_ascii_graphic())
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct CredentialSetupPolicy {
    primary: Option<IdentitySetupPolicy>,
    secondary: Option<IdentitySetupPolicy>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct IdentitySetupPolicy {
    trim_credential: Option<bool>,
    trim_signing: Option<bool>,
    passphrase: Option<SetupFieldPolicy>,
    secret_encoding: Option<KeyEncoding>,
    extra: Option<ExtraKeyPolicy>,
    signing_algorithm: Option<SigningAlgorithm>,
    signing: SigningPolicy,
    source: PrimarySource,
}

/// Scalar-only primary acceptance; `required` is today's behaviour.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum PrimarySource {
    #[default]
    Required,
    ScalarOnly,
}

/// Credential-only acceptance; `required` is today's behaviour.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SigningPolicy {
    #[default]
    Required,
    Optional,
}

/// Only asymmetric algorithms may be declared; the provider refuses others.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SigningAlgorithm {
    Ed25519,
    Es256,
    Secp256k1Digest,
    StarkDigest,
    SchnorrDigest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum KeyEncoding {
    Utf8,
    Base64,
    Hex,
    Pem,
    Base64Text,
    Base58,
    Base64Url,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtraKeyPolicy {
    key: ExtraKey,
    #[serde(default)]
    field: ExtraFieldPolicy,
    #[serde(default)]
    encoding: Option<KeyEncoding>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ExtraFieldPolicy {
    #[default]
    Optional,
    RequiredForPrivate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ExtraKey {
    ZklinkSeed,
    L1Wallet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SetupFieldPolicy {
    Optional,
    RequiredForPrivate,
    RequiredWithPrimaryFallback,
}

#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
struct BridgeRule {
    spot: bool,
    demo: DemoRule,
    primary_credentials_only: bool,
    payload_signing: bool,
    api_key_index_required: bool,
    isolated_book_subscriptions: bool,
    dispatch_signing: bool,
    read_credentials: ReadCredentialsPolicy,
}

/// What trusted setup does with a family's read key; absent installs it.
#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ReadCredentialsPolicy {
    #[default]
    Install,
    ValidateOnly,
    PublicRead,
}

#[derive(Default, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum DemoRule {
    #[default]
    Never,
    Always,
    HyroMode,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rules {
    default: Rule,
    venues: BTreeMap<String, Rule>,
}

fn conditions(value: &Option<Vec<Vec<Input>>>) -> String {
    let Some(groups) = value else {
        return "None".into();
    };
    let groups = groups
        .iter()
        .map(|group| {
            let inputs = group
                .iter()
                .map(|input| format!("Input::{input:?}"))
                .collect::<Vec<_>>()
                .join(",");
            format!("&[{inputs}]")
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("Some(&[{groups}])")
}

fn rule(value: &Rule) -> String {
    let error = match &value.explicit_private_error {
        Some(error) => format!("Some({error:?})"),
        None => "None".into(),
    };
    let bridge = &value.bridge;
    let bridge = format!(
        "BridgeRule {{ spot: {}, demo: DemoRule::{:?}, primary_credentials_only: {}, payload_signing: {}, api_key_index_required: {}, isolated_book_subscriptions: {}, dispatch_signing: {}, read_credentials: ReadCredentialsPolicy::{:?} }}",
        bridge.spot,
        bridge.demo,
        bridge.primary_credentials_only,
        bridge.payload_signing,
        bridge.api_key_index_required,
        bridge.isolated_book_subscriptions,
        bridge.dispatch_signing,
        bridge.read_credentials
    );
    let slot = |value: &Option<IdentitySetupPolicy>| match value {
        Some(value) => {
            let passphrase = value.passphrase.as_ref().map_or_else(
                || "None".into(),
                |policy| format!("Some(SetupFieldPolicy::{policy:?})"),
            );
            let secret_encoding = value.secret_encoding.as_ref().map_or_else(
                || "None".into(),
                |encoding| format!("Some(safe_cex_engine::signing::KeyEncoding::{encoding:?})"),
            );
            let extra = value.extra.as_ref().map_or_else(
                || "None".into(),
                |extra| {
                    let encoding = extra.encoding.as_ref().map_or_else(
                        || "None".into(),
                        |encoding| format!("Some(safe_cex_engine::signing::KeyEncoding::{encoding:?})"),
                    );
                    format!(
                        "Some(safe_cex_engine::signing::ExtraKeyPolicy {{ key: safe_cex_engine::signing::ExtraKey::{:?}, field: safe_cex_engine::signing::ExtraFieldPolicy::{:?}, encoding: {encoding} }})",
                        extra.key, extra.field
                    )
                },
            );
            let signing_algorithm = value.signing_algorithm.as_ref().map_or_else(
                || "None".into(),
                |algorithm| format!("Some(safe_cex_engine::signing::Algorithm::{algorithm:?})"),
            );
            format!(
                "Some(IdentitySetupPolicy {{ trim_credential: {:?}, trim_signing: {:?}, passphrase: {passphrase}, secret_encoding: {secret_encoding}, extra: {extra}, signing_algorithm: {signing_algorithm}, signing: safe_cex_engine::signing::SigningPolicy::{:?}, source: safe_cex_engine::signing::PrimarySource::{:?} }})",
                value.trim_credential, value.trim_signing, value.signing, value.source
            )
        }
        None => "None".into(),
    };
    let signer_setup = format!(
        "CredentialSetupPolicy {{ primary: {}, secondary: {} }}",
        slot(&value.signer_setup.primary),
        slot(&value.signer_setup.secondary),
    );
    let protected = value.remote_signing.protected_headers.as_ref().map_or_else(
        || "None".into(),
        |names| {
            format!(
                "Some(&[{}])",
                names
                    .iter()
                    .map(|name| format!("{name:?}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        },
    );
    let remote_signing = format!(
        "RemoteSigningPolicy {{ enabled: {}, payload_supported: {}, payload_rest_only: {}, protected_headers: {protected}, signature_encoding: SignatureEncoding::{:?}, passphrase_header: {} }}",
        value.remote_signing.enabled,
        value.remote_signing.payload_supported,
        value.remote_signing.payload_rest_only,
        value.remote_signing.signature_encoding,
        value
            .remote_signing
            .passphrase_header
            .as_ref()
            .map_or_else(|| "None".into(), |header| format!("Some({header:?})")),
    );
    let setup_mapping = value.setup_mapping.as_ref().map_or_else(
        || "None".into(),
        |mapping| format!("Some(SetupMapping::{mapping:?})"),
    );
    format!(
        "Rule {{ can_sign: {}, private_eligible: {}, explicit_private_error: {error}, bridge: {bridge}, signer_setup: {signer_setup}, remote_signing: {remote_signing}, setup_mapping: {setup_mapping} }}",
        conditions(&value.can_sign),
        conditions(&value.private_eligible),
    )
}

pub fn generate(json: &str) -> Result<String, String> {
    let rules: Rules = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if rules.default.can_sign.is_none() || rules.default.private_eligible.is_none() {
        return Err("default signing and private conditions are required".into());
    }
    for policy in std::iter::once(&rules.default).chain(rules.venues.values()) {
        if policy
            .remote_signing
            .passphrase_header
            .as_ref()
            .is_some_and(|name| {
                name.is_empty()
                    || name.len() > 256
                    || !name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
                    })
            })
        {
            return Err("invalid passphrase presentation header declaration".into());
        }
        if policy.remote_signing.enabled
            && (!policy.bridge.dispatch_signing
                || policy.remote_signing.protected_headers.is_none()
                || (policy.remote_signing.payload_supported
                    && !policy.remote_signing.payload_rest_only))
        {
            return Err(
                "delegated signing requires dispatch, header and private-feed policy".into(),
            );
        }
        if let Some(wire) = &policy.remote_signing.wire
            && (wire.exchange_name.is_empty()
                || wire.exchange_name.len() > 64
                || !wire
                    .exchange_name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase())
                || wire.request_id.as_ref().is_some_and(|id| {
                    id.is_empty()
                        || id.len() > 256
                        || id.bytes().any(|byte| byte.is_ascii_control())
                })
                || (wire.request_id.is_some()
                    && !matches!(wire.extra_data, Some(RemoteExtraData::BinanceSpot)))
                || matches!(wire.extra_data, Some(RemoteExtraData::SecondsTimestamp))
                    != wire.seconds_suffix.is_some()
                || wire
                    .seconds_suffix
                    .as_ref()
                    .is_some_and(|suffix| !suffix.valid()))
        {
            return Err("invalid delegated wire declaration".into());
        }
        if let Some(names) = &policy.remote_signing.protected_headers
            && (names.len() > 32
                || names.iter().any(|name| {
                    name.is_empty()
                        || name.len() > 256
                        || !name.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
                        })
                }))
        {
            return Err("invalid protected callback header declaration".into());
        }
    }
    let mut output = format!("const DEFAULT_RULE: Rule = {};\n", rule(&rules.default));
    output.push_str("fn venue_rule(venue: &str) -> Option<&'static Rule> { match venue {\n");
    for (venue, value) in rules.venues {
        output.push_str(&format!("{venue:?} => Some(&{}),\n", rule(&value)));
    }
    output.push_str("_ => None,\n} }\n");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::generate;

    #[test]
    fn signer_setup_rules_emit_constant_explicit_and_undeclared_fields() {
        let output = generate(r#"{
            "default":{"canSign":[],"privateEligible":[]},
            "venues":{
                "mexc":{"signerSetup":{"primary":{"trimCredential":true},"secondary":{"trimCredential":false}}},
                "empty":{"signerSetup":{"primary":{}}},
                "evm":{"signerSetup":{"primary":{"signingAlgorithm":"secp256k1Digest"}}}
            }
        }"#).unwrap();
        assert!(output.contains("CredentialSetupPolicy { primary: Some(IdentitySetupPolicy { trim_credential: Some(true), trim_signing: None, passphrase: None, secret_encoding: None, extra: None, signing_algorithm: None, signing: safe_cex_engine::signing::SigningPolicy::Required, source: safe_cex_engine::signing::PrimarySource::Required }), secondary: Some(IdentitySetupPolicy { trim_credential: Some(false), trim_signing: None, passphrase: None, secret_encoding: None, extra: None, signing_algorithm: None, signing: safe_cex_engine::signing::SigningPolicy::Required, source: safe_cex_engine::signing::PrimarySource::Required }) }"));
        assert!(output.contains("CredentialSetupPolicy { primary: Some(IdentitySetupPolicy { trim_credential: None, trim_signing: None, passphrase: None, secret_encoding: None, extra: None, signing_algorithm: None, signing: safe_cex_engine::signing::SigningPolicy::Required, source: safe_cex_engine::signing::PrimarySource::Required }), secondary: None }"));
        assert!(output.contains("CredentialSetupPolicy { primary: Some(IdentitySetupPolicy { trim_credential: None, trim_signing: None, passphrase: None, secret_encoding: None, extra: None, signing_algorithm: Some(safe_cex_engine::signing::Algorithm::Secp256k1Digest), signing: safe_cex_engine::signing::SigningPolicy::Required, source: safe_cex_engine::signing::PrimarySource::Required }), secondary: None }"));
        // Lighter's Schnorr key is declarable for Sign typing.
        assert!(generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{"v":{"signerSetup":{"primary":{"signingAlgorithm":"schnorrDigest"}}}}}"#).unwrap().contains("Algorithm::SchnorrDigest"));
        // MAC algorithms are never declarable.
        assert!(generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{"v":{"signerSetup":{"primary":{"signingAlgorithm":"hmacSha256"}}}}}"#).is_err());
        assert!(output.contains("CredentialSetupPolicy { primary: None, secondary: None }"));
    }

    #[test]
    fn required_and_fallback_fields_are_compiled_without_runtime_rules() {
        let output = generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
            "v":{"signerSetup":{"primary":{"passphrase":"requiredForPrivate"},"secondary":{"passphrase":"requiredWithPrimaryFallback"}}},
            "optional":{"signerSetup":{"primary":{"passphrase":"optional"}}}
        }}"#).unwrap();
        for variant in [
            "RequiredForPrivate",
            "RequiredWithPrimaryFallback",
            "Optional",
        ] {
            assert!(output.contains(&format!("Some(SetupFieldPolicy::{variant})")));
        }
        for field in [r#"{"passphrase":"invented"}"#, r#"{"passphrase":true}"#] {
            let rules = format!(
                r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"signerSetup":{{"primary":{field}}}}}}}}}"#
            );
            assert!(generate(&rules).is_err());
        }
    }

    // P6b R3: the l1Wallet role compiles like zklinkSeed.
    #[test]
    fn l1_wallet_extra_key_compiles() {
        let output = generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
            "p":{"signerSetup":{"primary":{"extra":{"key":"l1Wallet","field":"requiredForPrivate","encoding":"hex"}}}}
        }}"#).unwrap();
        assert!(output.contains("extra: Some(safe_cex_engine::signing::ExtraKeyPolicy { key: safe_cex_engine::signing::ExtraKey::L1Wallet, field: safe_cex_engine::signing::ExtraFieldPolicy::RequiredForPrivate, encoding: Some(safe_cex_engine::signing::KeyEncoding::Hex) })"));
    }

    #[test]
    fn declared_encoding_and_extra_key_compile_and_unknown_values_fail_closed() {
        let output = generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
            "v":{"signerSetup":{"primary":{"secretEncoding":"base64Text","extra":{"key":"zklinkSeed","field":"requiredForPrivate","encoding":"hex"}}}},
            "o":{"signerSetup":{"primary":{"extra":{"key":"zklinkSeed"}}}}
        }}"#).unwrap();
        assert!(output.contains("secret_encoding: Some(safe_cex_engine::signing::KeyEncoding::Base64Text), extra: Some(safe_cex_engine::signing::ExtraKeyPolicy { key: safe_cex_engine::signing::ExtraKey::ZklinkSeed, field: safe_cex_engine::signing::ExtraFieldPolicy::RequiredForPrivate, encoding: Some(safe_cex_engine::signing::KeyEncoding::Hex) })"));
        assert!(output.contains("secret_encoding: None, extra: Some(safe_cex_engine::signing::ExtraKeyPolicy { key: safe_cex_engine::signing::ExtraKey::ZklinkSeed, field: safe_cex_engine::signing::ExtraFieldPolicy::Optional, encoding: None })"));
        let base58 = r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{"v":{"signerSetup":{"primary":{"secretEncoding":"base58"}}}}}"#;
        assert!(generate(base58).unwrap().contains("KeyEncoding::Base58"));
        let url = r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{"v":{"signerSetup":{"primary":{"secretEncoding":"base64Url"}}}}}"#;
        assert!(generate(url).unwrap().contains("KeyEncoding::Base64Url"));
        for field in [
            r#"{"secretEncoding":"base32"}"#,
            r#"{"extra":{"key":"unknownRole"}}"#,
            r#"{"extra":{"key":"zklinkSeed","name":"x"}}"#,
            r#"{"extra":{"key":"zklinkSeed","field":"requiredWithPrimaryFallback"}}"#,
            r#"{"extra":{"key":"zklinkSeed","field":null}}"#,
        ] {
            let rules = format!(
                r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"signerSetup":{{"primary":{field}}}}}}}}}"#
            );
            assert!(generate(&rules).is_err());
        }
    }

    #[test]
    fn read_credentials_policy_compiles_and_unknown_values_fail_closed() {
        let output = generate(
            r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
            "validated":{"bridge":{"readCredentials":"validateOnly"}},
            "public":{"bridge":{"readCredentials":"publicRead"}},
            "plain":{}
        }}"#,
        )
        .unwrap();
        for variant in ["ValidateOnly", "PublicRead", "Install"] {
            assert!(output.contains(&format!(
                "read_credentials: ReadCredentialsPolicy::{variant}"
            )));
        }
        for value in [r#""ignore""#, "true", "null"] {
            let rules = format!(
                r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"bridge":{{"readCredentials":{value}}}}}}}}}"#
            );
            assert!(generate(&rules).is_err(), "{value}");
        }
    }

    #[test]
    fn declared_signing_trim_compiles_per_slot() {
        let output = generate(r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{
            "bitunix":{"signerSetup":{"primary":{"trimCredential":true,"trimSigning":true},"secondary":{"trimCredential":true,"trimSigning":true}}}
        }}"#).unwrap();
        assert!(output.contains("IdentitySetupPolicy { trim_credential: Some(true), trim_signing: Some(true), passphrase: None, secret_encoding: None, extra: None, signing_algorithm: None, signing: safe_cex_engine::signing::SigningPolicy::Required, source: safe_cex_engine::signing::PrimarySource::Required }"));
        let rules = r#"{"default":{"canSign":[],"privateEligible":[]},"venues":{"v":{"signerSetup":{"primary":{"trimSigning":"yes"}}}}}"#;
        assert!(generate(rules).is_err());
    }

    #[test]
    fn malformed_signer_setup_rules_fail_closed_at_build_time() {
        for setup in [
            r#"{"primry":{}}"#,
            r#"{"primary":{"trimCredental":true}}"#,
            r#"{"secondary":{"trimCredential":"false"}}"#,
            r#"{"secondary":{"secret":"not-a-policy-field"}}"#,
        ] {
            let rules = format!(
                r#"{{"default":{{"canSign":[],"privateEligible":[]}},"venues":{{"v":{{"signerSetup":{setup}}}}}}}"#
            );
            assert!(generate(&rules).is_err());
        }
    }
}
