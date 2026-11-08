//! Frozen exchange actions signed under a genuine selected, address-bound nonce claim.
use super::{Signature, agent_digest};
use safe_cex_crypto::{ethereum, hex};
use safe_cex_engine::signing::{
    Algorithm, Context, Identity, KeyRef, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{Action, DispatchContext, HttpRequest, Method, SignStep};
use serde::Serialize;
use serde_json::value::RawValue;

const HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("x-api-version", "2026-08-16"),
];

pub struct Exchange {
    signing: KeyRef,
    claim: safe_cex_engine::nonce::NonceClaim,
    address: [u8; 20],
    signer: String,
    json: Box<RawValue>,
    vault: Option<String>,
    user: String,
    prepared: Option<(SignRequest, [u8; 32], [u8; 32])>,
}
impl Exchange {
    pub fn new(
        identity: &Identity,
        claim: safe_cex_engine::nonce::NonceClaim,
        action: &impl Serialize,
        vault: Option<String>,
        user: String,
    ) -> Result<Self, SignError> {
        let signing = identity.signing_ref()?.clone();
        let address = identity.verified_signing_address()?;
        let signer = ethereum::get_address(&format!("0x{}", hex(&address)))
            .map_err(|_| SignError::InvalidRequest)?;
        if let Some(vault) = &vault {
            ethereum::get_address(vault).map_err(|_| SignError::InvalidRequest)?;
        }
        let json =
            serde_json::value::to_raw_value(action).map_err(|_| SignError::InvalidRequest)?;
        super::statement::connection_id(json.get(), vault.as_deref(), 0)?;
        ethereum::get_address(&user).map_err(|_| SignError::InvalidRequest)?;
        Ok(Self {
            signing,
            claim,
            address,
            signer,
            json,
            vault,
            user,
            prepared: None,
        })
    }
    pub fn url(&self) -> &'static str {
        "https://rpc.txflow-testnet.xyz/exchange"
    }

    fn attest(&self, request: &HttpRequest) -> Result<(), SignError> {
        if request.method != Method::Post
            || request.url != self.url()
            || request.body.is_some()
            || !request.signer_headers.is_empty()
            || !request
                .headers
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .eq(HEADERS.iter().copied())
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}
impl SignStep for Exchange {
    fn nonce_claim(&self) -> Option<&safe_cex_engine::nonce::NonceClaim> {
        Some(&self.claim)
    }
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        HEADERS
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        self.attest(request)?;
        let nonce = dispatch
            .reserve_nonce(&self.claim)
            .map_err(|error| match error {
                safe_cex_engine::nonce::NonceError::InvalidTime
                | safe_cex_engine::nonce::NonceError::InvalidClaim => SignError::InvalidRequest,
                _ => SignError::Unavailable,
            })?;
        let hash = super::statement::connection_id(self.json.get(), self.vault.as_deref(), nonce)
            .map_err(|_| SignError::InvalidRequest)?;
        let digest = agent_digest(hash);
        // A remote device re-derives the digest from this statement; local
        // providers only hash it, which binds the result to the same statement.
        let statement =
            super::statement::encode(&self.json, nonce, self.vault.as_deref(), &self.user)?;
        let commitment = safe_cex_crypto::sha256(statement.as_bytes());
        let work = SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(dispatch.stamp_ms()),
                nonce: Some(nonce),
                expiry_ms: None,
            },
            context: Context {
                venue: "txflow".into(),
                method: "POST".into(),
                host: "rpc.txflow-testnet.xyz".into(),
                path: "/exchange".into(),
            },
            operations: vec![
                Operation::Sha256 {
                    payload: vec![PayloadPart::Bytes {
                        bytes: statement.into_bytes(),
                    }],
                },
                Operation::PublicAddress {
                    key: self.signing.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    key: self.signing.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: digest.to_vec(),
                    }],
                },
            ],
        };
        work.validate()?;
        self.prepared = Some((work.clone(), digest, commitment));
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let (work, digest, commitment) = self.prepared.take().ok_or(SignError::InvalidResult)?;
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        self.attest(request)?;
        work.validate_result(result)?;
        if result.values[0].as_slice() != commitment
            || result.values[1].as_slice() != self.address
            || !ethereum::signed_by(&digest, &result.values[2], &self.signer)
        {
            return Err(SignError::InvalidResult);
        }
        let signature = &result.values[2];
        // EIP-2 canonical signature; do not forward a high-S device response.
        const HALF_ORDER: [u8; 32] = [
            0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46,
            0x68, 0x1b, 0x20, 0xa0,
        ];
        if signature[32..64] > HALF_ORDER[..] || !matches!(signature[64], 0 | 1 | 27 | 28) {
            return Err(SignError::InvalidResult);
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Payload<'a> {
            action: &'a RawValue,
            nonce: u64,
            signature: Signature,
            #[serde(skip_serializing_if = "Option::is_none")]
            vault_address: Option<&'a str>,
            signature_chain_id: u32,
            confirm: &'static str,
        }
        let body = serde_json::to_string(&Payload {
            action: &self.json,
            nonce: work.receipt.nonce.ok_or(SignError::InvalidResult)?,
            signature: Signature {
                r: format!("0x{}", hex(&signature[..32])),
                s: format!("0x{}", hex(&signature[32..64])),
                v: match signature[64] {
                    0 | 1 => signature[64] + 27,
                    value => value,
                },
            },
            vault_address: self.vault.as_deref(),
            signature_chain_id: 2511,
            confirm: "executed",
        })
        .map_err(|_| SignError::InvalidResult)?;
        request.body = Some(body);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        20_000.
    }
}
