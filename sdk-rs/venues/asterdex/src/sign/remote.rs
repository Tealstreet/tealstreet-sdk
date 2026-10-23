//! P8 remote recipes over the same digests the local V3 and Code paths sign.
use super::{code_digest, encode_standard_payload};
use safe_cex_crypto::ethereum;
use safe_cex_engine::signing::{Algorithm, ExtraKey, SignError};
use safe_cex_engine::signing_remote::{RemoteRecipe, parse_inputs};
use std::collections::BTreeMap;

fn address(value: &str) -> Result<&str, SignError> {
    super::is_address(value)
        .then_some(value)
        .ok_or(SignError::Refused)
}

fn nonce(value: &str) -> Result<u64, SignError> {
    (!value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
        .ok_or(SignError::Refused)
}

/// `asterdex.v3.1` inputs; any other field is refused.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StandardInputs {
    payload: String,
    user: String,
    signer: String,
    nonce: String,
}

/// `asterdex.code.1` inputs; any other field is refused.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeInputs {
    query: String,
    testnet: bool,
}

/// Business params a wallet key may sign: orders, cancels, leverage, margin
/// mode, position mode and reads. Transfer and withdraw shapes (`asset`,
/// `amount`, addresses) are absent, so they are never signed remotely.
const SIGNED_PARAMS: &[&str] = &[
    "symbol",
    "side",
    "positionSide",
    "type",
    "timeInForce",
    "quantity",
    "price",
    "reduceOnly",
    "newClientOrderId",
    "stopPrice",
    "closePosition",
    "activationPrice",
    "callbackRate",
    "workingType",
    "priceProtect",
    "newOrderRespType",
    "stpMode",
    "goodTillDate",
    "orderId",
    "origClientOrderId",
    "orderIdList",
    "origClientOrderIdList",
    "batchOrders",
    "leverage",
    "marginType",
    "dualSidePosition",
    "multiAssetsMargin",
    "startTime",
    "endTime",
    "limit",
    "fromId",
    "incomeType",
    "countdownTime",
    "builder",
    "feeRate",
];

/// `asterdex.v3.1`: the EIP-191 digest of `abi(payload, user, signer, nonce)`,
/// where `payload` is the key-sorted JSON of the request's string params.
pub struct Standard;
impl RemoteRecipe for Standard {
    fn id(&self) -> &'static str {
        "asterdex.v3.1"
    }
    fn algorithm(&self) -> Algorithm {
        Algorithm::Secp256k1Digest
    }
    fn role(&self) -> Option<ExtraKey> {
        None
    }
    fn canonicalize(&self, inputs: &str) -> Result<Vec<u8>, SignError> {
        let inputs: StandardInputs = parse_inputs(inputs)?;
        let payload = inputs.payload.as_str();
        // The payload must already be the canonical sorted form, with the
        // freshness fields every standard V3 request carries.
        let params: BTreeMap<String, String> =
            serde_json::from_str(payload).map_err(|_| SignError::Refused)?;
        if serde_json::to_string(&params).ok().as_deref() != Some(payload)
            || !params.contains_key("timestamp")
            || !params
                .get("recvWindow")
                .and_then(|window| window.parse::<u64>().ok())
                .is_some_and(|window| (1..=60_000).contains(&window))
            || params.keys().any(|key| {
                !matches!(key.as_str(), "timestamp" | "recvWindow")
                    && !SIGNED_PARAMS.contains(&key.as_str())
            })
        {
            return Err(SignError::Refused);
        }
        let encoded = encode_standard_payload(
            payload,
            address(&inputs.user)?,
            address(&inputs.signer)?,
            nonce(&inputs.nonce)?,
        )
        .map_err(|_| SignError::Refused)?;
        Ok(ethereum::ethereum_message_hash(&ethereum::keccak256(&encoded)).to_vec())
    }
    fn stamp_ms(&self, inputs: &str) -> Result<f64, SignError> {
        let inputs: StandardInputs = parse_inputs(inputs)?;
        let params: BTreeMap<String, String> =
            serde_json::from_str(&inputs.payload).map_err(|_| SignError::Refused)?;
        params
            .get("timestamp")
            .and_then(|stamp| stamp.parse::<u64>().ok())
            .map(|stamp| stamp as f64)
            .ok_or(SignError::Refused)
    }
}

/// `asterdex.code.1`: the Code EIP-712 `Message(string msg)` digest over the
/// raw signed query, which must end with its chain, user, signer and nonce.
pub struct Code;
impl RemoteRecipe for Code {
    fn id(&self) -> &'static str {
        "asterdex.code.1"
    }
    fn algorithm(&self) -> Algorithm {
        Algorithm::Secp256k1Digest
    }
    fn role(&self) -> Option<ExtraKey> {
        None
    }
    fn canonicalize(&self, inputs: &str) -> Result<Vec<u8>, SignError> {
        let CodeInputs { query, testnet } = parse_inputs(inputs)?;
        let query = query.as_str();
        // Raw business values may contain '&', so only the fixed tail the venue
        // appends last is parsed: asterChain, user, signer, nonce.
        let mut tail = query.rsplitn(5, '&');
        let mut field = |name: &str| {
            tail.next()
                .and_then(|pair| pair.strip_prefix(name))
                .and_then(|pair| pair.strip_prefix('='))
                .ok_or(SignError::Refused)
        };
        let nonce_text = field("nonce")?;
        let signer = field("signer")?;
        let user = field("user")?;
        let chain = field("asterChain")?;
        if chain != if testnet { "Testnet" } else { "Mainnet" } {
            return Err(SignError::Refused);
        }
        address(user)?;
        address(signer)?;
        nonce(nonce_text)?;
        // The business part before the tail is only allowlisted key=value pairs,
        // each key once, so no reserved or fund-moving key hides in a value.
        let business = tail.next();
        let mut seen = std::collections::BTreeSet::new();
        for pair in business.into_iter().flat_map(|part| part.split('&')) {
            let (key, value) = pair.split_once('=').ok_or(SignError::Refused)?;
            if !SIGNED_PARAMS.contains(&key)
                || !seen.insert(key)
                || value.bytes().any(|b| b.is_ascii_control())
            {
                return Err(SignError::Refused);
            }
        }
        Ok(code_digest(query, testnet).to_vec())
    }
    fn stamp_ms(&self, inputs: &str) -> Result<f64, SignError> {
        // The Code nonce is milliseconds x1000 plus a sequence (sign.rs next_nonce).
        let CodeInputs { query, .. } = parse_inputs(inputs)?;
        let text = query
            .rsplit_once("&nonce=")
            .map(|(_, nonce)| nonce)
            .or_else(|| query.strip_prefix("nonce="))
            .ok_or(SignError::Refused)?;
        Ok((nonce(text)? / 1000) as f64)
    }
}

/// Verifier for Aster V3/Code results: the recovered signer must be the
/// account's configured public signer address.
pub fn remote_verifier(
    signer: &str,
) -> Result<safe_cex_engine::signing_remote::RemoteVerifier, String> {
    let checksummed = ethereum::get_address(signer)?;
    let expected: [u8; 20] = (0..20)
        .map(|i| u8::from_str_radix(&checksummed[2 + i * 2..4 + i * 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Invalid Aster signer address")?
        .try_into()
        .map_err(|_| "Invalid Aster signer address")?;
    Ok(Box::new(move |digest, signature| {
        ethereum::recover_digest_address(digest, signature) == Some(expected)
    }))
}

pub static STANDARD: Standard = Standard;
pub static CODE: Code = Code;
pub static REMOTE_RECIPES: &[&dyn RemoteRecipe] = &[&STANDARD, &CODE];

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        user: String,
        signer: String,
        nonce: String,
        payload: String,
        hash: String,
        code_query: String,
        code_hash: String,
        signature: String,
        testnet: bool,
    }
    fn case_stamp(payload: &str) -> f64 {
        let params: BTreeMap<String, String> = serde_json::from_str(payload).unwrap();
        params["timestamp"].parse().unwrap()
    }
    fn corpus() -> Vec<Case> {
        #[derive(serde::Deserialize)]
        struct Corpus {
            cases: Vec<Case>,
        }
        serde_json::from_str::<Corpus>(include_str!("../../tests/fixtures/signing.json"))
            .unwrap()
            .cases
    }

    #[test]
    fn recipes_rebuild_the_independent_vector_digests() {
        for case in corpus() {
            // Serializer-stress vectors carry a free-text `note` param the venue
            // never signs; the allowlist refuses them, so only the digest check
            // for real request shapes applies.
            if !(case.name.starts_with("documented") || case.name.starts_with("empty")) {
                let standard = serde_json::json!({
                    "payload": case.payload, "user": case.user,
                    "signer": case.signer, "nonce": case.nonce
                });
                assert_eq!(
                    STANDARD.canonicalize(&standard.to_string()),
                    Err(SignError::Refused)
                );
                let code = serde_json::json!({ "query": case.code_query, "testnet": case.testnet });
                assert_eq!(
                    CODE.canonicalize(&code.to_string()),
                    Err(SignError::Refused)
                );
                continue;
            }
            let standard = STANDARD
                .canonicalize(
                    &serde_json::json!({
                        "payload": case.payload, "user": case.user,
                        "signer": case.signer, "nonce": case.nonce
                    })
                    .to_string(),
                )
                .unwrap();
            assert_eq!(
                format!("0x{}", safe_cex_crypto::hex(&standard)),
                case.hash,
                "{}",
                case.name
            );
            let code = CODE
                .canonicalize(
                    &serde_json::json!({ "query": case.code_query, "testnet": case.testnet })
                        .to_string(),
                )
                .unwrap();
            assert_eq!(
                format!("0x{}", safe_cex_crypto::hex(&code)),
                case.code_hash,
                "{}",
                case.name
            );
            assert_eq!(code, code_digest(&case.code_query, case.testnet));
        }
    }

    #[test]
    fn remote_verifier_recovers_the_configured_signer() {
        let case = corpus().remove(0);
        let digest = STANDARD
            .canonicalize(
                &serde_json::json!({
                    "payload": case.payload, "user": case.user,
                    "signer": case.signer, "nonce": case.nonce
                })
                .to_string(),
            )
            .unwrap();
        // Wire signatures are 0x r||s||v with v 27/28; providers return v 0/1.
        let hex = case.signature.trim_start_matches("0x");
        let mut signature: Vec<u8> = (0..65)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect();
        signature[64] -= 27;
        assert!(remote_verifier(&case.signer).unwrap()(&digest, &signature));
        assert!(!remote_verifier(&case.user).unwrap()(&digest, &signature));
        assert!(remote_verifier("0x12").is_err());
    }

    #[test]
    fn recipes_refuse_inputs_outside_their_allowlists() {
        let case = corpus().remove(0);
        let standard = serde_json::json!({
            "payload": case.payload, "user": case.user, "signer": case.signer, "nonce": case.nonce
        });
        for (field, value) in [
            ("payload", serde_json::json!(r#"{"b":"1","a":"2"}"#)),
            (
                "payload",
                serde_json::json!(r#"{"recvWindow":"600000","symbol":"X","timestamp":"1"}"#),
            ),
            (
                "payload",
                serde_json::json!(
                    r#"{"amount":"1","asset":"USDT","recvWindow":"1","timestamp":"1"}"#
                ),
            ),
            (
                "payload",
                serde_json::json!(r#"{"recvWindow":"1","symbol":"X"}"#),
            ),
            (
                "payload",
                serde_json::json!(r#"{"recvWindow":"1","signature":"x","timestamp":"1"}"#),
            ),
            ("user", serde_json::json!("0x12")),
            ("nonce", serde_json::json!("1e3")),
            ("extra", serde_json::json!(1)),
        ] {
            let mut inputs = standard.clone();
            inputs[field] = value;
            assert_eq!(
                STANDARD.canonicalize(&inputs.to_string()),
                Err(SignError::Refused),
                "{field}"
            );
        }
        let code = serde_json::json!({ "query": case.code_query, "testnet": case.testnet });
        for (field, value) in [
            ("testnet", serde_json::json!(!case.testnet)),
            (
                "query",
                serde_json::json!(format!("{}&signature=0x1", case.code_query)),
            ),
            (
                "query",
                serde_json::json!(case.code_query.replace("nonce=", "nonce=x")),
            ),
            ("query", serde_json::json!("symbol=BTCUSDT")),
            (
                "query",
                serde_json::json!(format!("asset=USDT&amount=1&{}", case.code_query)),
            ),
            (
                "query",
                serde_json::json!(format!("nonce=1&{}", case.code_query)),
            ),
            ("path", serde_json::json!("/fapi/v3/order")),
        ] {
            let mut inputs = code.clone();
            inputs[field] = value;
            assert_eq!(
                CODE.canonicalize(&inputs.to_string()),
                Err(SignError::Refused),
                "{field}"
            );
        }
        assert_eq!(
            STANDARD.stamp_ms(&standard.to_string()),
            Ok(case_stamp(&case.payload))
        );
        assert_eq!(
            CODE.stamp_ms(&code.to_string()),
            Ok((case.nonce.parse::<u64>().unwrap() / 1000) as f64)
        );
        assert_eq!(
            REMOTE_RECIPES.iter().map(|r| r.id()).collect::<Vec<_>>(),
            ["asterdex.v3.1", "asterdex.code.1"]
        );
    }
}
