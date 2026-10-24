//! Aster's legacy HMAC and API-wallet signing. Exact payload policy stays here.
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_crypto::{eip712, ethereum};
use safe_cex_engine::signing::{
    Algorithm, Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use safe_cex_engine::url;
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, num};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Version {
    #[default]
    V1,
    V3,
}

impl Version {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::V1 => "/fapi/v1",
            Self::V3 => "/fapi/v3",
        }
    }
}

/// Match the SDK's explicit override and trimmed auth/account fallbacks.
pub fn resolve_version(
    configured: Option<Version>,
    user: Option<&str>,
    signer: Option<&str>,
    private_key: Option<&str>,
) -> Version {
    configured.unwrap_or_else(|| {
        if user.is_some_and(is_address)
            && signer.is_some_and(is_address)
            && private_key.is_some_and(is_private_key)
        {
            Version::V3
        } else {
            Version::V1
        }
    })
}

pub fn is_address(value: &str) -> bool {
    value.starts_with("0x")
        && value.len() == 42
        && value[2..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn is_private_key(value: &str) -> bool {
    let body = value.strip_prefix("0x").unwrap_or(value);
    body.len() == 64 && body.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Legacy eager-signing oracle for the wire vectors; production signs only
/// through `Recipe` at dispatch.
#[cfg(test)]
#[derive(Clone)]
pub struct Credentials {
    pub version: Version,
    pub key: String,
    pub secret: String,
    pub user: Option<String>,
    pub signer: Option<String>,
}

#[cfg(test)]
impl Credentials {
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_empty() {
            return Err("Missing required credentials: API key".into());
        }
        if self.version == Version::V1 {
            if is_address(&self.key) && is_private_key(&self.secret) && self.user.is_none() {
                return Err("Aster API Wallet/vault keys require the Main Wallet Address. Fill Main Wallet Address with your login wallet — do not replace the main wallet with the API/vault wallet.".into());
            }
            if self.secret.is_empty() {
                return Err("Missing required credentials: API secret".into());
            }
            return Ok(());
        }
        let user = self.user.as_deref().filter(|s| !s.is_empty());
        let signer = self.signer.as_deref().filter(|s| !s.is_empty());
        if user.is_none() || signer.is_none() || self.secret.is_empty() {
            return Err("Missing required Aster v3 auth fields: Main Wallet Address, API Wallet Address, and Generated Private Key".into());
        }
        if !is_private_key(&self.secret) {
            return Err(
                "Invalid Aster API Wallet private key format (expected 64-byte hex)".into(),
            );
        }
        ethereum::get_address(user.unwrap())?;
        ethereum::get_address(signer.unwrap())?;
        Ok(())
    }
}

/// Keep the original ordered values until the signing mode is known. Binance's
/// encoded Query cannot be shared: Aster signs raw values, in a different order.
#[derive(Clone, Debug, Default)]
pub struct Query(Vec<(String, String)>);

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn str(mut self, name: &str, value: &str) -> Self {
        if value.is_empty() {
            self.0.retain(|(key, _)| key != name);
        } else {
            if let Some((_, old)) = self.0.iter_mut().find(|(key, _)| key == name) {
                *old = value.into();
            } else {
                self.0.push((name.into(), value.into()));
            }
        }
        self
    }

    pub fn num(self, name: &str, value: f64) -> Self {
        self.str(name, &num::format(value))
    }

    pub fn opt_str(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(name, value),
            None => self,
        }
    }

    pub fn opt_num(self, name: &str, value: Option<f64>) -> Self {
        match value {
            Some(value) => self.num(name, value),
            None => self,
        }
    }

    pub fn as_str(&self) -> String {
        join(&self.0, false)
    }

    #[cfg(test)]
    pub fn signed(
        self,
        credentials: &Credentials,
        method: Method,
        path: &str,
        now_ms: f64,
        testnet: bool,
    ) -> Result<String, String> {
        credentials.validate()?;
        if !now_ms.is_finite() || now_ms < 0.0 || now_ms > (u64::MAX / 1000) as f64 {
            return Err("Invalid Aster signing clock".into());
        }
        let stamp = safe_cex_engine::time::wire_stamp_ms(now_ms);
        let mode = if credentials.version == Version::V1 {
            Mode::V1
        } else {
            let user = ethereum::get_address(credentials.user.as_deref().unwrap())?;
            let signer = ethereum::get_address(credentials.signer.as_deref().unwrap())?;
            // Aster's replay protection belongs to the agent address, across users.
            let nonce = next_nonce(&signer, stamp as u64)?;
            Mode::V3 {
                user,
                signer,
                nonce,
            }
        };
        let prepared = self.prepared(&mode, method, path, stamp, testnet)?;
        let signature = match &prepared.preimage {
            Preimage::Hmac(message) => hmac_sha256(
                credentials.secret.as_bytes(),
                message.as_bytes(),
                Encoding::Hex,
            ),
            Preimage::Digest(digest) => {
                let secret = if credentials.secret.starts_with("0x") {
                    credentials.secret.clone()
                } else {
                    format!("0x{}", credentials.secret)
                };
                ethereum::sign_digest(digest, &secret)?
            }
        };
        Ok(prepared.finish(&signature))
    }

    /// Canonical signing input for one attempt. Every mode first removes the
    /// fields a retry regenerates from its unsigned business params.
    fn prepared(
        mut self,
        mode: &Mode,
        method: Method,
        path: &str,
        stamp_ms: f64,
        testnet: bool,
    ) -> Result<Prepared, String> {
        self.0.retain(|(key, _)| {
            !matches!(
                key.as_str(),
                "signature" | "timestamp" | "recvWindow" | "user" | "signer" | "nonce"
            )
        });
        let Mode::V3 {
            user,
            signer,
            nonce,
        } = mode
        else {
            self = self.num("recvWindow", 20_000.0).num("timestamp", stamp_ms);
            return Ok(Prepared {
                preimage: Preimage::Hmac(self.as_str()),
                query: self,
                encoded: true,
            });
        };
        if code_endpoint(method, path) {
            self = self
                .str("asterChain", if testnet { "Testnet" } else { "Mainnet" })
                .str("user", user)
                .str("signer", signer)
                .str("nonce", &nonce.to_string());
            return Ok(Prepared {
                preimage: Preimage::Digest(code_digest(&self.as_str(), testnet)),
                query: self,
                encoded: false,
            });
        }
        self = self.num("recvWindow", 20_000.0).num("timestamp", stamp_ms);
        let sorted: BTreeMap<_, _> = self
            .0
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let payload = serde_json::to_string(&sorted).map_err(|e| e.to_string())?;
        let encoded = encode_standard_payload(&payload, user, signer, *nonce)?;
        let digest = ethereum::ethereum_message_hash(&ethereum::keccak256(&encoded));
        Ok(Prepared {
            preimage: Preimage::Digest(digest),
            query: self
                .str("user", user)
                .str("signer", signer)
                .str("nonce", &nonce.to_string()),
            encoded: true,
        })
    }
}

/// Fixed-nonce V3 signing for the independent wire vectors.
#[cfg(test)]
impl Query {
    fn signed_v3(
        self,
        credentials: &Credentials,
        method: Method,
        path: &str,
        now_ms: f64,
        testnet: bool,
        nonce: u64,
    ) -> Result<String, String> {
        let mode = Mode::V3 {
            user: ethereum::get_address(credentials.user.as_deref().unwrap())?,
            signer: ethereum::get_address(credentials.signer.as_deref().unwrap())?,
            nonce,
        };
        let stamp = safe_cex_engine::time::wire_stamp_ms(now_ms);
        let prepared = self.prepared(&mode, method, path, stamp, testnet)?;
        let Preimage::Digest(digest) = &prepared.preimage else {
            unreachable!("V3 always signs a digest")
        };
        let secret = format!(
            "0x{}",
            credentials
                .secret
                .strip_prefix("0x")
                .unwrap_or(&credentials.secret)
        );
        let signature = ethereum::sign_digest(digest, &secret)?;
        Ok(prepared.finish(&signature))
    }
}
#[cfg(test)]
fn code_signature(query: &str, secret: &str, testnet: bool) -> Result<String, String> {
    ethereum::sign_digest(&code_digest(query, testnet), secret)
}

/// Signing mode for one attempt. V3 addresses are checksummed public config.
enum Mode {
    V1,
    V3 {
        user: String,
        signer: String,
        nonce: u64,
    },
}

enum Preimage {
    /// HMAC-SHA256 over the raw (unencoded) query; hex signature.
    Hmac(String),
    /// secp256k1 over an EIP-191 or Code EIP-712 digest; 0x r||s||v(27/28).
    Digest([u8; 32]),
}

struct Prepared {
    preimage: Preimage,
    query: Query,
    /// Code endpoints keep the signed raw query; others use client encoding.
    encoded: bool,
}
impl Prepared {
    fn finish(self, signature: &str) -> String {
        join(&self.query.str("signature", signature).0, self.encoded)
    }
}

fn code_endpoint(method: Method, path: &str) -> bool {
    let path = path.split('?').next().unwrap_or(path);
    (method == Method::Post && path.ends_with("/fapi/v3/order"))
        || (method == Method::Get
            && (path.ends_with("/fapi/v3/builder") || path.ends_with("/fapi/v3/agent")))
}

fn next_nonce(scope: &str, now_ms: u64) -> Result<u64, String> {
    static NONCES: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let mut nonces = NONCES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| "Aster nonce state unavailable")?;
    let previous = nonces.get(scope).copied().unwrap_or(0);
    let nonce = now_ms
        .checked_mul(1000)
        .ok_or("Aster nonce overflow")?
        .max(previous.checked_add(1).ok_or("Aster nonce overflow")?);
    nonces.insert(scope.into(), nonce);
    Ok(nonce)
}

fn word(value: u64) -> [u8; 32] {
    let mut word = [0; 32];
    word[24..].copy_from_slice(&value.to_be_bytes());
    word
}

fn address_word(address: &str) -> Result<[u8; 32], String> {
    let address = ethereum::get_address(address)?;
    let mut word = [0; 32];
    for (i, pair) in address.as_bytes()[2..]
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
    {
        word[12 + i] = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| "Invalid Aster address")?,
            16,
        )
        .map_err(|_| "Invalid Aster address")?;
    }
    Ok(word)
}

fn encode_standard_payload(
    payload: &str,
    user: &str,
    signer: &str,
    nonce: u64,
) -> Result<Vec<u8>, String> {
    let mut encoded = Vec::with_capacity(160 + payload.len().div_ceil(32) * 32);
    encoded.extend(word(128));
    encoded.extend(address_word(user)?);
    encoded.extend(address_word(signer)?);
    encoded.extend(word(nonce));
    encoded.extend(word(payload.len() as u64));
    encoded.extend(payload.as_bytes());
    encoded.resize(160 + payload.len().div_ceil(32) * 32, 0);
    Ok(encoded)
}

fn code_digest(query: &str, testnet: bool) -> [u8; 32] {
    let mut domain = Vec::with_capacity(160);
    domain.extend(ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    ));
    domain.extend(ethereum::keccak256(b"AsterSignTransaction"));
    domain.extend(ethereum::keccak256(b"1"));
    domain.extend(word(if testnet { 714 } else { 1666 }));
    domain.extend([0; 32]);
    let mut message = [0; 64];
    message[..32].copy_from_slice(&ethereum::keccak256(b"Message(string msg)"));
    message[32..].copy_from_slice(&ethereum::keccak256(query.as_bytes()));
    eip712::hash_prehashed(ethereum::keccak256(&domain), ethereum::keccak256(&message))
}
fn join(params: &[(String, String)], encoded: bool) -> String {
    let mut result = String::new();
    for (key, value) in params {
        if !result.is_empty() {
            result.push('&');
        }
        if encoded {
            url::encode_http_client_into(&mut result, key);
        } else {
            result.push_str(key);
        }
        result.push('=');
        if encoded {
            url::encode_http_client_into(&mut result, value);
        } else {
            result.push_str(value);
        }
    }
    result
}

/// Source of the V3 replay nonce, called at every preparation with the
/// checksummed signer address. The engine-owned allocator replaces
/// `interim_nonce` once it lands; no second allocator exists meanwhile.
pub type NonceSource = Box<dyn FnMut(&DispatchContext<'_>, &str) -> Result<u64, SignError> + Send>;

/// Interim V3 source: the existing per-signer allocator, fed the protocol stamp.
pub fn interim_nonce() -> NonceSource {
    Box::new(|dispatch, signer| {
        next_nonce(signer, dispatch.stamp_ms() as u64).map_err(|_| SignError::Unavailable)
    })
}

/// Selected signing contract for one recipe. V3 addresses are public config.
/// The V3 identity's signing key is the 32-byte API-wallet private key, which
/// trusted setup decodes as hex with an optional `0x` prefix.
pub enum Auth {
    V1,
    V3 {
        user: String,
        signer: String,
        nonce: NonceSource,
    },
}

/// Dispatch recipe for one private Aster request: V1 HMAC, V3 EIP-191 or
/// Code EIP-712. It rebuilds the signed query at each preparation from the
/// unsigned business params and writes the full signed URL at apply.
pub struct Recipe {
    identity: Identity,
    auth: Auth,
    method: Method,
    origin: String,
    path: String,
    query: Query,
    testnet: bool,
    prepared: Option<Prepared>,
}
impl Recipe {
    /// `origin` is the REST base URL; `path` includes the version prefix.
    pub fn new(
        identity: Identity,
        auth: Auth,
        method: Method,
        origin: &str,
        path: &str,
        query: Query,
        testnet: bool,
    ) -> Result<Self, String> {
        let auth = match auth {
            Auth::V1 => Auth::V1,
            Auth::V3 {
                user,
                signer,
                nonce,
            } => Auth::V3 {
                user: ethereum::get_address(&user)?,
                signer: ethereum::get_address(&signer)?,
                nonce,
            },
        };
        Ok(Self {
            identity,
            auth,
            method,
            origin: origin.into(),
            path: path.into(),
            query,
            testnet,
            prepared: None,
        })
    }
}
impl SignStep for Recipe {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // The signing mode follows the method, so the outbound must agree.
        if http.method != self.method {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let mode = match &mut self.auth {
            Auth::V1 => Mode::V1,
            Auth::V3 {
                user,
                signer,
                nonce,
            } => Mode::V3 {
                nonce: nonce(dispatch, signer)?,
                user: user.clone(),
                signer: signer.clone(),
            },
        };
        let code = matches!(mode, Mode::V3 { .. }) && code_endpoint(self.method, &self.path);
        let prepared = self
            .query
            .clone()
            .prepared(&mode, self.method, &self.path, stamp, self.testnet)
            .map_err(|_| SignError::InvalidRequest)?;
        let (algorithm, message) = match &prepared.preimage {
            Preimage::Hmac(message) => (Algorithm::HmacSha256, message.as_bytes().to_vec()),
            Preimage::Digest(digest) => (Algorithm::Secp256k1Digest, digest.to_vec()),
        };
        self.prepared = Some(prepared);
        let (host, _) = self
            .origin
            .split_once("://")
            .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
            .unwrap_or(("", ""));
        Ok(SignRequest {
            // Code queries carry a nonce, not a signed timestamp.
            receipt: SignReceipt {
                nonce: None,
                stamp_ms: (!code).then_some(stamp),
                expiry_ms: None,
            },
            operations: vec![
                Operation::Present {
                    key: self.identity.credential.clone(),
                },
                Operation::Sign {
                    algorithm,
                    key: self.identity.signing.clone(),
                    payload: vec![PayloadPart::Bytes { bytes: message }],
                },
            ],
            context: Context {
                venue: "asterdex".into(),
                method: http.method.as_str().into(),
                host: host.into(),
                path: self.path.clone(),
            },
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let raw = result.values.get(1).ok_or(SignError::InvalidResult)?;
        let prepared = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let signature = match prepared.preimage {
            Preimage::Hmac(_) if raw.len() == 32 => safe_cex_crypto::hex(raw),
            // Wire form is ethers' 0x r||s||v with v as 27/28.
            Preimage::Digest(_) if raw.len() == 65 && raw[64] <= 1 => {
                format!("0x{}{:02x}", safe_cex_crypto::hex(&raw[..64]), raw[64] + 27)
            }
            _ => return Err(SignError::InvalidResult),
        };
        http.url = format!(
            "{}{}?{}",
            self.origin,
            self.path,
            prepared.finish(&signature)
        );
        http.headers = vec![("X-MBX-APIKEY", key.into())];
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        private_key: String,
        user: String,
        signer: String,
        // Preserve the generator's business insertion order explicitly below.
        params: BTreeMap<String, String>,
        now_ms: f64,
        nonce: String,
        payload: String,
        abi: String,
        hash: String,
        signature: String,
        code_query: String,
        code_hash: String,
        code_signature: String,
        testnet: bool,
        v1_query: String,
    }

    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }

    fn corpus() -> Vec<Case> {
        serde_json::from_str::<Corpus>(include_str!("../tests/fixtures/signing.json"))
            .unwrap()
            .cases
    }

    fn credentials(case: &Case) -> Credentials {
        Credentials {
            version: Version::V3,
            key: case.signer.clone(),
            secret: case.private_key.clone(),
            user: Some(case.user.clone()),
            signer: Some(case.signer.clone()),
        }
    }

    fn ordered_query(case: &Case) -> Query {
        let keys: &[&str] = if case.name.starts_with("documented") {
            &["symbol", "type", "side", "timeInForce", "quantity", "price"]
        } else if case.name.starts_with("decimal") {
            &["symbol", "quantity", "price", "reduceOnly", "note"]
        } else if case.name.starts_with("query") {
            &["symbol", "note"]
        } else {
            &[]
        };
        keys.iter()
            .fold(Query::new(), |q, key| q.str(key, &case.params[*key]))
    }

    #[test]
    fn independent_ethers_and_http_serializer_vectors() {
        for case in corpus() {
            let encoded = encode_standard_payload(
                &case.payload,
                &case.user,
                &case.signer,
                case.nonce.parse().unwrap(),
            )
            .unwrap();
            assert_eq!(
                format!("0x{}", safe_cex_crypto::hex(&encoded)),
                case.abi,
                "ABI {}",
                case.name
            );
            let digest = ethereum::ethereum_message_hash(&ethereum::keccak256(&encoded));
            assert_eq!(
                format!("0x{}", safe_cex_crypto::hex(&digest)),
                case.hash,
                "digest {}",
                case.name
            );
            assert_eq!(
                ethereum::sign_digest(&digest, &case.private_key).unwrap(),
                case.signature,
                "signature {}",
                case.name
            );
            assert_eq!(
                code_signature(&case.code_query, &case.private_key, case.testnet).unwrap(),
                case.code_signature,
                "Code {}",
                case.name
            );
            assert_ne!(case.code_hash, case.hash);
            let mut v1 = credentials(&case);
            v1.version = Version::V1;
            v1.secret = "documentation-secret".into();
            assert_eq!(
                ordered_query(&case)
                    .signed(
                        &v1,
                        Method::Post,
                        "/fapi/v1/order",
                        case.now_ms,
                        case.testnet
                    )
                    .unwrap(),
                case.v1_query,
                "HMAC wire {}",
                case.name
            );
            let creds = credentials(&case);
            let signed = ordered_query(&case)
                .signed_v3(
                    &creds,
                    Method::Post,
                    "/fapi/v3/order",
                    case.now_ms,
                    case.testnet,
                    case.nonce.parse().unwrap(),
                )
                .unwrap();
            assert_eq!(
                signed,
                format!("{}&signature={}", case.code_query, case.code_signature),
                "Code exact wire {}",
                case.name
            );
            // Standard V3 sorts the JSON but leaves transport fields in caller order.
            let signed = ordered_query(&case)
                .signed_v3(
                    &creds,
                    Method::Delete,
                    "/fapi/v3/order",
                    case.now_ms,
                    case.testnet,
                    case.nonce.parse().unwrap(),
                )
                .unwrap();
            assert!(
                signed.ends_with(&format!("&signature={}", case.signature)),
                "standard V3 {}",
                case.name
            );
            assert!(signed.contains("recvWindow=20000&timestamp=1748310859508&user="));
        }
    }

    #[test]
    fn signer_nonce_is_shared_monotonic_and_independent() {
        let scope = "unit-test-clock-regression";
        let first = next_nonce(scope, 100).unwrap();
        assert_eq!(first, 100_000);
        assert_eq!(next_nonce(scope, 100).unwrap(), first + 1);
        assert_eq!(next_nonce(scope, 90).unwrap(), first + 2);
        assert_eq!(next_nonce(scope, 101).unwrap(), 101_000);
        assert_eq!(next_nonce("unit-test-other-signer", 90).unwrap(), 90_000);
    }

    #[test]
    fn signing_mode_endpoint_method_boundaries() {
        for endpoint in ["order", "builder", "agent"] {
            for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
                assert_eq!(
                    code_endpoint(method, &format!("/fapi/v3/{endpoint}")),
                    (endpoint == "order" && method == Method::Post)
                        || (endpoint != "order" && method == Method::Get)
                );
            }
        }
        for endpoint in [
            "account",
            "listenKey",
            "positionRisk",
            "stpMode",
            "userTrades",
            "approveBuilder",
        ] {
            assert!(!code_endpoint(
                Method::Post,
                &format!("/fapi/v3/{endpoint}")
            ));
        }
        assert!(!code_endpoint(Method::Post, "/fapi/v1/order"));
    }

    #[test]
    fn validation_nonce_freshness_and_unsigned_fields() {
        let case = corpus().remove(0);
        let mut creds = credentials(&case);
        // Isolate this agent from concurrently running lifecycle tests.
        creds.secret = format!("0x{}", "91".repeat(32));
        let signer = ethereum::private_key_address(&creds.secret).unwrap();
        creds.key = signer.clone();
        creds.signer = Some(signer);
        let first = Query::new()
            .str("symbol", "BTCUSDT")
            .signed(&creds, Method::Post, "/fapi/v3/order", 200.0, false)
            .unwrap();
        creds.secret = creds.secret.strip_prefix("0x").unwrap().into();
        let next = Query::new()
            .str("symbol", "BTCUSDT")
            .str("signature", "stale")
            .str("timestamp", "1")
            .str("recvWindow", "1")
            .str("user", "stale")
            .str("signer", "stale")
            .str("nonce", "1")
            .signed(&creds, Method::Post, "/fapi/v3/order", 100.0, false)
            .unwrap();
        assert!(first.contains("nonce=200000&signature="));
        assert!(next.contains("nonce=200001&signature="));
        assert!(!next.contains("stale"));
        assert!(!next.contains("timestamp"));
        assert!(!next.contains("recvWindow"));
        assert_ne!(first, next);
        assert!(
            Query::new()
                .signed(&creds, Method::Get, "/fapi/v3/account", f64::NAN, false)
                .is_err()
        );
        creds.user = None;
        assert!(
            creds
                .validate()
                .unwrap_err()
                .contains("Main Wallet Address")
        );
        creds.user = Some(case.user);
        creds.secret = "bad".into();
        assert!(creds.validate().unwrap_err().contains("private key format"));
        assert!(is_address("0x0000000000000000000000000000000000000000"));
        assert!(!is_address("0000000000000000000000000000000000000000"));
        assert!(is_private_key(&"01".repeat(32)));
        assert!(!is_private_key(&format!("0X{}", "01".repeat(32))));
    }

    #[test]
    fn query_optional_empty_and_replace_keep_field_sequence() {
        assert_eq!(
            Query::new()
                .str("a", "1")
                .str("b", "")
                .str("c", "3")
                .str("a", "2")
                .opt_str("d", None)
                .opt_num("e", Some(1.25))
                .as_str(),
            "a=2&c=3&e=1.25"
        );
        assert_eq!(Query::new().str("a", "1").str("a", "").as_str(), "");
    }

    #[test]
    fn v1_hmac_oracle_matches_independent_vectors_for_both_slots() {
        let v1 = |key: &str, secret: &str| Credentials {
            version: Version::V1,
            key: key.into(),
            secret: secret.into(),
            user: None,
            signer: None,
        };
        // Independent Python hashlib/hmac vectors, using the exact native payload.
        for (credentials, expected) in [
            (
                v1("primary-key", "primary-raw-secret"),
                "29306ee689ad7468724aa8f72aed3a3bb507291d3c577faa91fda70fcf7cb3c0",
            ),
            (
                v1("secondary-key", "secondary-raw-secret"),
                "cd0441e53172012534659d4a39a53bd924f19b2c4f35e4f01ca7be577ea7372d",
            ),
        ] {
            let query = Query::new()
                .str("symbol", "BTCUSDT")
                .signed(
                    &credentials,
                    Method::Get,
                    "/fapi/v1/account",
                    1_700_000_000_000.0,
                    false,
                )
                .unwrap();
            assert_eq!(
                query,
                format!(
                    "symbol=BTCUSDT&recvWindow=20000&timestamp=1700000000000&signature={expected}"
                )
            );
        }
    }

    #[test]
    fn owners_sharing_an_agent_share_its_nonce_sequence() {
        let secret = format!("0x{}", "92".repeat(32));
        let signer = ethereum::private_key_address(&secret).unwrap();
        let credentials = |owner: &str| Credentials {
            version: Version::V3,
            key: signer.clone(),
            secret: secret.clone(),
            user: Some(owner.into()),
            signer: Some(signer.clone()),
        };
        let first = credentials("0x0000000000000000000000000000000000000111");
        let second = credentials("0x0000000000000000000000000000000000000222");
        let query = |credentials: &Credentials, now| {
            Query::new()
                .signed(credentials, Method::Get, "/fapi/v3/account", now, false)
                .unwrap()
        };
        assert!(query(&first, 1234.0).contains("nonce=1234000&signature="));
        assert!(query(&second, 1234.0).contains("nonce=1234001&signature="));
        assert!(query(&first, 1233.0).contains("nonce=1234002&signature="));
    }
}

#[cfg(test)]
safe_cex_crypto::erase_credentials_on_drop!(Credentials, key, secret);
