//! Paradex Stark signing at dispatch: `/auth` headers and order bodies are
//! hashed with the release stamp, signed by the provider's sealed key and
//! verified against it before anything binds to the request.
use super::{CURVE_ORDER, Credentials};
use alloy_primitives::U256;
use safe_cex_engine::signing::{
    Algorithm, Context, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};
use serde::Serialize;
use starknet_crypto::Felt;

const AUTH_LIFETIME_S: u64 = 300;
const FRESHNESS_MS: f64 = 20_000.;

/// The signed fields of one order or amendment.
pub struct Preimage<'a> {
    pub market: &'a str,
    pub side: &'a str,
    pub kind: &'a str,
    pub size: &'a str,
    pub price: Option<&'a str>,
    pub id: Option<&'a str>,
}

/// A frozen native order body whose only dispatch-time fields are its
/// signature and signature timestamp.
pub trait SignedBody: Serialize + Send {
    fn preimage(&self) -> Preimage<'_>;
    fn sign(&mut self, signature: String, timestamp: u64);
}

/// What one Stark recipe signs with: the sealed key and its account facts.
struct Signer {
    credentials: Credentials,
    host: &'static str,
}
impl Signer {
    fn new(credentials: &Credentials, testnet: bool) -> Self {
        Self {
            credentials: credentials.clone(),
            host: if testnet {
                "api.testnet.paradex.trade"
            } else {
                "api.prod.paradex.trade"
            },
        }
    }
    fn work(
        &self,
        stamp_ms: f64,
        method: Method,
        path: &str,
        hashes: &[Felt],
    ) -> Result<SignRequest, SignError> {
        let work = SignRequest {
            receipt: SignReceipt {
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
                nonce: None,
            },
            context: Context {
                venue: "paradex".into(),
                method: method.as_str().into(),
                host: self.host.into(),
                path: path.into(),
            },
            operations: hashes
                .iter()
                .map(|hash| Operation::Sign {
                    algorithm: Algorithm::StarkDigest,
                    key: self.credentials.signing.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: hash.to_bytes_be().to_vec(),
                    }],
                })
                .collect(),
        };
        work.validate()?;
        Ok(work)
    }
    /// Every value is a canonical 64-byte `r || s` with `0 < r, s < n` that
    /// verifies for its own hash under the sealed public key.
    fn signatures(
        &self,
        work: &SignRequest,
        hashes: &[Felt],
        result: &SignResult,
    ) -> Result<Vec<String>, SignError> {
        work.validate_result(result)?;
        if result.values.len() != hashes.len() {
            return Err(SignError::InvalidResult);
        }
        let order = U256::from_str_radix(CURVE_ORDER, 10).expect("curve order");
        hashes
            .iter()
            .zip(&result.values)
            .map(|(hash, value)| {
                let (r, s) = value
                    .split_first_chunk::<32>()
                    .and_then(|(r, rest)| Some((r, <&[u8; 32]>::try_from(rest).ok()?)))
                    .ok_or(SignError::InvalidResult)?;
                let (r_int, s_int) = (U256::from_be_bytes(*r), U256::from_be_bytes(*s));
                if r_int == U256::ZERO || s_int == U256::ZERO || r_int >= order || s_int >= order {
                    return Err(SignError::InvalidResult);
                }
                let (r_felt, s_felt) = (Felt::from_bytes_be(r), Felt::from_bytes_be(s));
                if !starknet_crypto::verify(&self.credentials.public, hash, &r_felt, &s_felt)
                    .unwrap_or(false)
                {
                    return Err(SignError::InvalidResult);
                }
                Ok(Credentials::signature_text(r, s))
            })
            .collect()
    }
}

/// `POST /auth/{subkey}`: the signed headers follow the account header.
pub struct Auth {
    signer: Signer,
    account: String,
    url: String,
    prepared: Option<(SignRequest, Felt, u64, u64)>,
}
impl Auth {
    pub fn new(credentials: &Credentials, testnet: bool, url: String) -> Self {
        Self {
            signer: Signer::new(credentials, testnet),
            account: credentials.address().into(),
            url,
            prepared: None,
        }
    }
    /// The unsigned headers the request is created with, for `account`.
    pub fn headers(&self, account: &str) -> Vec<(&'static str, String)> {
        vec![
            ("Content-Type", "application/json".into()),
            ("Accept", "application/json".into()),
            ("PARADEX-STARKNET-ACCOUNT", account.into()),
        ]
    }
    fn attest(&self, action: &Action) -> Result<(), SignError> {
        match action {
            Action::Http(http)
                if http.method == Method::Post
                    && http.url == self.url
                    && http.body.is_none()
                    && http.headers == self.headers(&self.account)
                    && http.signer_headers.is_empty() =>
            {
                Ok(())
            }
            _ => Err(SignError::InvalidRequest),
        }
    }
}
impl SignStep for Auth {
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.attest(action)?;
        let stamp = dispatch.stamp_ms();
        let timestamp = stamp as u64 / 1000;
        let expiration = timestamp
            .checked_add(AUTH_LIFETIME_S)
            .ok_or(SignError::InvalidRequest)?;
        let hash = self
            .signer
            .credentials
            .auth_hash(timestamp, expiration)
            .map_err(|_| SignError::InvalidRequest)?;
        let work = self.signer.work(stamp, Method::Post, "/v1/auth", &[hash])?;
        self.prepared = Some((work.clone(), hash, timestamp, expiration));
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let (work, hash, timestamp, expiration) =
            self.prepared.take().ok_or(SignError::InvalidResult)?;
        self.attest(action)?;
        let signature = self
            .signer
            .signatures(&work, &[hash], result)?
            .pop()
            .ok_or(SignError::InvalidResult)?;
        let Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.headers.extend([
            ("PARADEX-STARKNET-SIGNATURE", signature),
            ("PARADEX-TIMESTAMP", timestamp.to_string()),
            ("PARADEX-SIGNATURE-EXPIRATION", expiration.to_string()),
        ]);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        FRESHNESS_MS
    }
}

/// One order write: a single body, or a native batch array whose legs share
/// one release stamp.
pub struct Orders<T> {
    signer: Signer,
    legs: Vec<T>,
    batch: bool,
    method: Method,
    url: String,
    path: String,
    unsigned: String,
    prepared: Option<(SignRequest, Vec<Felt>, u64)>,
}
impl<T: SignedBody> Orders<T> {
    pub fn new(
        credentials: &Credentials,
        testnet: bool,
        method: Method,
        url: String,
        path: String,
        legs: Vec<T>,
        batch: bool,
    ) -> Result<Self, SignError> {
        if legs.is_empty() || (!batch && legs.len() != 1) {
            return Err(SignError::InvalidRequest);
        }
        let unsigned = body(&legs, batch)?;
        Ok(Self {
            signer: Signer::new(credentials, testnet),
            legs,
            batch,
            method,
            url,
            path,
            unsigned,
            prepared: None,
        })
    }
    /// The unsigned body the request is created with.
    pub fn unsigned_body(&self) -> &str {
        &self.unsigned
    }
    fn attest(&self, action: &Action) -> Result<(), SignError> {
        match action {
            Action::Http(http)
                if http.method == self.method
                    && http.url == self.url
                    && http.body.as_deref() == Some(self.unsigned.as_str())
                    && http.signer_headers.is_empty() =>
            {
                Ok(())
            }
            _ => Err(SignError::InvalidRequest),
        }
    }
}
fn body<T: Serialize>(legs: &[T], batch: bool) -> Result<String, SignError> {
    if batch {
        serde_json::to_string(legs)
    } else {
        serde_json::to_string(&legs[0])
    }
    .map_err(|_| SignError::InvalidRequest)
}
impl<T: SignedBody> SignStep for Orders<T> {
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.attest(action)?;
        let stamp = dispatch.stamp_ms();
        let now = stamp as u64;
        let hashes = self
            .legs
            .iter()
            .map(|leg| self.signer.credentials.order_hash(now, &leg.preimage()))
            .collect::<Result<Vec<_>, String>>()
            .map_err(|_| SignError::InvalidRequest)?;
        let work = self.signer.work(stamp, self.method, &self.path, &hashes)?;
        self.prepared = Some((work.clone(), hashes, now));
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let (work, hashes, now) = self.prepared.take().ok_or(SignError::InvalidResult)?;
        self.attest(action)?;
        // The whole batch verifies before any leg is signed or a body binds.
        let signatures = self.signer.signatures(&work, &hashes, result)?;
        let mut legs = std::mem::take(&mut self.legs);
        for (leg, signature) in legs.iter_mut().zip(signatures) {
            leg.sign(signature, now);
        }
        let signed = body(&legs, self.batch);
        self.legs = legs;
        let Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.body = Some(signed?);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        FRESHNESS_MS
    }
}
