//! Shared, host-independent public-key admission statements. No account creation,
//! network access, or venue activation. Callers bind statements to their pending
//! account and use their existing admission deadline and provider ownership.
use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation, PayloadPart, Provider, SignError, SignRequest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Ethereum,
    Ed25519,
    Stark,
    Schnorr,
}
impl Family {
    /// Classification only; this does not enable a venue or signing mode.
    /// AsterDEX callers must select the v3 credential policy explicitly.
    pub fn for_venue(venue: &str) -> Option<Self> {
        match venue {
            "hyperliquid" | "hyperliquidspot" | "grvt" | "txflow" | "polymarket" => {
                Some(Self::Ethereum)
            }
            "backpack" | "orderly" | "kodiakfi" | "mode" | "what" | "woofi" => Some(Self::Ed25519),
            "paradex" => Some(Self::Stark),
            "lighter" | "lighter-robinhood" => Some(Self::Schnorr),
            _ => None,
        }
    }
    pub fn algorithm(self) -> Algorithm {
        match self {
            Self::Ethereum => Algorithm::Secp256k1Digest,
            Self::Ed25519 => Algorithm::Ed25519,
            Self::Stark => Algorithm::StarkDigest,
            Self::Schnorr => Algorithm::SchnorrDigest,
        }
    }
    fn public_operation(self, key: KeyRef) -> Operation {
        match self {
            Self::Ethereum => Operation::PublicAddress { key },
            _ => Operation::PublicKey {
                algorithm: self.algorithm(),
                key,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    #[serde(rename = "tealstreet-admission-v1")]
    Admission,
}

/// A closed descriptor, rather than a client-supplied digest or transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Statement {
    kind: Kind,
    attempt: String,
    identity_digest: [u8; 32],
    nonce: [u8; 32],
    issued_at_ms: u64,
}
impl Statement {
    pub fn new(
        attempt: &str,
        identity_digest: [u8; 32],
        nonce: [u8; 32],
        issued_at_ms: u64,
    ) -> Result<Self, SignError> {
        let value = Self {
            kind: Kind::Admission,
            attempt: attempt.into(),
            identity_digest,
            nonce,
            issued_at_ms,
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<(), SignError> {
        if self.attempt.is_empty()
            || self.attempt.len() > 128
            || !self
                .attempt
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
    pub fn from_json(text: &str) -> Result<Self, SignError> {
        if text.len() > 2048 {
            return Err(SignError::InvalidRequest);
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            kind: Kind,
            attempt: String,
            identity_digest: [u8; 32],
            nonce: [u8; 32],
            issued_at_ms: u64,
        }
        let value: Wire = serde_json::from_str(text).map_err(|_| SignError::InvalidRequest)?;
        let Kind::Admission = value.kind;
        Self::new(
            &value.attempt,
            value.identity_digest,
            value.nonce,
            value.issued_at_ms,
        )
    }
    pub fn current_at(&self, now_ms: u64, lifetime_ms: u64) -> bool {
        now_ms
            .checked_sub(self.issued_at_ms)
            .is_some_and(|age| age <= lifetime_ms)
    }
    /// Length-delimited attempt followed by fixed-width binary fields.
    pub fn digest(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"tealstreet/admission/challenge/v1\0");
        hash.update((self.attempt.len() as u32).to_be_bytes());
        hash.update(self.attempt.as_bytes());
        hash.update(self.identity_digest);
        hash.update(self.nonce);
        hash.update(self.issued_at_ms.to_be_bytes());
        hash.finalize().into()
    }
    /// Existing provider algorithms, with an admission-only message grammar.
    pub fn message(&self, family: Family) -> Vec<u8> {
        let digest = self.digest();
        match family {
            Family::Ethereum => {
                let keccak = crate::ethereum::keccak256;
                let domain = [
                    keccak(b"EIP712Domain(string name,string version)"),
                    keccak(b"Tealstreet Admission"),
                    keccak(b"1"),
                ]
                .concat();
                let structure = [keccak(b"Admission(bytes32 challenge)"), digest].concat();
                crate::eip712::hash_prehashed(keccak(&domain), keccak(&structure)).to_vec()
            }
            Family::Ed25519 => [b"tealstreet/admission/ed25519/v1\0".as_slice(), &digest].concat(),
            Family::Stark => {
                let mut value = crate::sha256(
                    &[b"tealstreet/admission/stark/v1\0".as_slice(), &digest].concat(),
                );
                // The existing Stark signer requires a hash below 2^251.
                value[0] &= 3;
                value.to_vec()
            }
            // Word-aligned for the existing Lighter hash-to-Fp5 signer. The
            // first u64 cannot be a venue transaction's u32 chain ID.
            Family::Schnorr => [b"TSJOIN01".as_slice(), &digest].concat(),
        }
    }
    pub fn sign_request(
        &self,
        family: Family,
        key: &KeyRef,
        entropy: &[u8; 32],
    ) -> Result<SignRequest, SignError> {
        self.validate()?;
        let message = self.message(family);
        let payload = if family == Family::Schnorr {
            [entropy.as_slice(), &message].concat()
        } else {
            message
        };
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: family.algorithm(),
                key: key.clone(),
                payload: vec![PayloadPart::Bytes { bytes: payload }],
            }],
        };
        request.validate()?;
        Ok(request)
    }
    /// Trusted local/device provider only. Remote channels use sign_request and
    /// retain their existing timeout, owner capture and result validation.
    pub fn sign(
        &self,
        family: Family,
        provider: &Provider,
        key: &KeyRef,
        entropy: &[u8; 32],
    ) -> Result<Vec<u8>, SignError> {
        immediate(provider, self.sign_request(family, key, entropy)?)
    }
}

/// A validated public value, obtained from the canonical account identity.
/// No secret export or resident-provider mutation is needed for a comparison.
pub struct PublicKey {
    family: Family,
    bytes: Vec<u8>,
}
crate::redacted_debug!(PublicKey);
impl PublicKey {
    pub fn new(family: Family, bytes: &[u8]) -> Result<Self, SignError> {
        let operation = family.public_operation(KeyRef("admission".into()));
        if bytes.iter().all(|b| *b == 0) || !crate::signer::valid_public_value(&operation, bytes) {
            return Err(SignError::InvalidResult);
        }
        Ok(Self {
            family,
            bytes: bytes.to_vec(),
        })
    }
    pub fn family(&self) -> Family {
        self.family
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn from_provider(
        family: Family,
        provider: &Provider,
        key: &KeyRef,
    ) -> Result<Self, SignError> {
        let bytes = immediate(
            provider,
            SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![family.public_operation(key.clone())],
            },
        )?;
        Self::new(family, &bytes)
    }
    pub fn matches_provider(&self, provider: &Provider, key: &KeyRef) -> Result<bool, SignError> {
        Ok(Self::from_provider(self.family, provider, key)?.bytes == self.bytes)
    }
    pub fn verify(&self, statement: &Statement, signature: &[u8]) -> bool {
        if statement.validate().is_err() {
            return false;
        }
        let message = statement.message(self.family);
        match self.family {
            Family::Ethereum => crate::ethereum::signed_by(
                &message,
                signature,
                &format!("0x{}", crate::hex(&self.bytes)),
            ),
            Family::Ed25519 => {
                crate::ed25519::verify(&message, &self.bytes, signature).unwrap_or(false)
            }
            Family::Stark => crate::starknet::verify(&message, &self.bytes, signature),
            Family::Schnorr => {
                crate::lighter::schnorr_verify(&message, &self.bytes, signature).unwrap_or(false)
            }
        }
    }
}

fn immediate(provider: &Provider, request: SignRequest) -> Result<Vec<u8>, SignError> {
    request.validate()?;
    if !provider.immediate() {
        return Err(SignError::Unavailable);
    }
    let mut result = provider
        .sign_ready(&request)
        .ok_or(SignError::Unavailable)??;
    request.validate_result(&result)?;
    if !result.headers.is_empty() || result.values.len() != 1 {
        return Err(SignError::InvalidResult);
    }
    Ok(result.values.remove(0))
}
