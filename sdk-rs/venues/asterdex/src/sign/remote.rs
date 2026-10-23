//! P8 remote recipes over the same digests the local V3 and Code paths sign.
use super::{code_digest, encode_standard_payload};
use safe_cex_crypto::ethereum;
use safe_cex_engine::signing::{Algorithm, ExtraKey, SignError};
use safe_cex_engine::signing_remote::RemoteRecipe;
use std::collections::BTreeMap;

fn address(value: Option<&serde_json::Value>) -> Result<&str, SignError> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| super::is_address(value))
        .ok_or(SignError::Refused)
}

fn nonce(value: Option<&serde_json::Value>) -> Result<u64, SignError> {
    value
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 20 && value.bytes().all(|b| b.is_ascii_digit())
        })
        .and_then(|value| value.parse().ok())
        .ok_or(SignError::Refused)
}

fn only(inputs: &serde_json::Value, fields: &[&str]) -> Result<(), SignError> {
    let object = inputs.as_object().ok_or(SignError::Refused)?;
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(SignError::Refused);
    }
    Ok(())
}

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
    fn canonicalize(&self, inputs: &serde_json::Value) -> Result<Vec<u8>, SignError> {
        only(inputs, &["payload", "user", "signer", "nonce"])?;
        let payload = inputs
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .ok_or(SignError::Refused)?;
        // The payload must already be the canonical sorted form, with the
        // freshness fields every standard V3 request carries.
        let params: BTreeMap<String, String> =
            serde_json::from_str(payload).map_err(|_| SignError::Refused)?;
        if serde_json::to_string(&params).ok().as_deref() != Some(payload)
            || !params.contains_key("timestamp")
            || !params.contains_key("recvWindow")
            || ["user", "signer", "nonce", "signature"]
                .iter()
                .any(|key| params.contains_key(*key))
        {
            return Err(SignError::Refused);
        }
        let encoded = encode_standard_payload(
            payload,
            address(inputs.get("user"))?,
            address(inputs.get("signer"))?,
            nonce(inputs.get("nonce"))?,
        )
        .map_err(|_| SignError::Refused)?;
        Ok(ethereum::ethereum_message_hash(&ethereum::keccak256(&encoded)).to_vec())
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
    fn canonicalize(&self, inputs: &serde_json::Value) -> Result<Vec<u8>, SignError> {
        only(inputs, &["query", "testnet"])?;
        let query = inputs
            .get("query")
            .and_then(serde_json::Value::as_str)
            .ok_or(SignError::Refused)?;
        let testnet = inputs
            .get("testnet")
            .and_then(serde_json::Value::as_bool)
            .ok_or(SignError::Refused)?;
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
        address(Some(&serde_json::Value::from(user)))?;
        address(Some(&serde_json::Value::from(signer)))?;
        nonce(Some(&serde_json::Value::from(nonce_text)))?;
        Ok(code_digest(query, testnet).to_vec())
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
            let standard = STANDARD
                .canonicalize(&serde_json::json!({
                    "payload": case.payload, "user": case.user,
                    "signer": case.signer, "nonce": case.nonce
                }))
                .unwrap();
            assert_eq!(
                format!("0x{}", safe_cex_crypto::hex(&standard)),
                case.hash,
                "{}",
                case.name
            );
            let code = CODE
                .canonicalize(
                    &serde_json::json!({ "query": case.code_query, "testnet": case.testnet }),
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
            .canonicalize(&serde_json::json!({
                "payload": case.payload, "user": case.user,
                "signer": case.signer, "nonce": case.nonce
            }))
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
                STANDARD.canonicalize(&inputs),
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
            ("path", serde_json::json!("/fapi/v3/order")),
        ] {
            let mut inputs = code.clone();
            inputs[field] = value;
            assert_eq!(
                CODE.canonicalize(&inputs),
                Err(SignError::Refused),
                "{field}"
            );
        }
        assert_eq!(
            REMOTE_RECIPES.iter().map(|r| r.id()).collect::<Vec<_>>(),
            ["asterdex.v3.1", "asterdex.code.1"]
        );
    }
}
