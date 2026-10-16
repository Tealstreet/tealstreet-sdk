//! Local CDP signing field bodies. WebCrypto key handles and primitive signing
//! operations are imported capabilities; cache/payload/promise policy is native.
use crate::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    utils::{
        CoinbaseSignatureAlgorithm, base64_url_encode, base64_url_encode_string,
        normalize_coinbase_api_credentials,
    },
};
use serde_json::{Value, json};
use std::{any::Any, cell::RefCell, rc::Rc};
pub type CdpCryptoKey = Rc<dyn Any>;
pub type CdpAsync<T> = Result<Promise<Result<T, PortfolioFailure>>, PortfolioFailure>;
#[derive(Default)]
pub struct CdpSigningCache {
    pub crypto_key: RefCell<Option<CdpCryptoKey>>,
    pub key_secret: RefCell<String>,
}
#[derive(Debug, Clone)]
pub struct CdpImportKey {
    pub format: &'static str,
    pub der: Vec<u8>,
    pub name: &'static str,
    pub named_curve: &'static str,
    pub extractable: bool,
    pub usages: Vec<&'static str>,
}
#[derive(Debug, Clone)]
pub struct CdpJwtHeader {
    pub algorithm: CoinbaseSignatureAlgorithm,
    pub typ: &'static str,
    pub key_id: String,
    pub nonce: String,
}
#[derive(Debug, Clone)]
pub struct CdpJwtPayload {
    pub issuer: &'static str,
    pub audience: Option<Vec<&'static str>>,
    pub not_before: f64,
    pub expires: f64,
    pub subject: String,
    pub uri: Option<String>,
}
/// Native claims plus the bytes handed to the imported primitive. Claims retain
/// JS nonfinite numbers before JSON.stringify turns them into null.
#[derive(Debug, Clone)]
pub struct CdpSigningInput {
    pub header: CdpJwtHeader,
    pub payload: CdpJwtPayload,
    pub message: Vec<u8>,
}
pub trait CdpSigningHost {
    /// Source pemToPkcs8Der capability, including its original exception carrier.
    /// Do not reconstruct a metadata-free error from the older pure Rust leaf.
    fn prepare_pkcs8(&self, secret: &str) -> Result<Vec<u8>, PortfolioFailure>;
    fn import_key(&self, request: CdpImportKey) -> CdpAsync<CdpCryptoKey>;
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure>;
    fn uuid(&self) -> Result<String, PortfolioFailure>;
    fn sign_ed25519(
        &self,
        input: CdpSigningInput,
        seed: Vec<u8>,
    ) -> Result<Vec<u8>, PortfolioFailure>;
    fn sign_ecdsa(
        &self,
        name: &'static str,
        hash: &'static str,
        key: CdpCryptoKey,
        input: CdpSigningInput,
    ) -> CdpAsync<Vec<u8>>;
}
pub struct CdpSigningReceiver<H> {
    pub host: H,
    pub cache: Rc<CdpSigningCache>,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn error(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn stringify(value: Value) -> String {
    safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(value))
        .expect("native scalar JWT record")
        .expect("JWT object")
}
pub fn get_signing_key<H: CdpSigningHost + 'static>(
    r: Rc<CdpSigningReceiver<H>>,
    secret: String,
) -> Promise<Result<CdpCryptoKey, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let cached = r.cache.crypto_key.borrow().clone();
            if r.cache.key_secret.borrow().as_str() == secret {
                if let Some(key) = cached {
                    return Ok(key);
                }
            }
            let der = r.host.prepare_pkcs8(&secret)?;
            let key = r
                .host
                .import_key(CdpImportKey {
                    format: "pkcs8",
                    der,
                    name: "ECDSA",
                    named_curve: "P-256",
                    extractable: false,
                    usages: vec!["sign"],
                })?
                .await?;
            *r.cache.crypto_key.borrow_mut() = Some(key);
            *r.cache.key_secret.borrow_mut() = secret;
            Ok(r.cache
                .crypto_key
                .borrow()
                .clone()
                .expect("assigned imported key"))
        }),
        &spawn,
    )
}
pub fn sign<H: CdpSigningHost + 'static>(
    r: Rc<CdpSigningReceiver<H>>,
    key: String,
    secret: String,
    uri: Option<String>,
) -> Promise<Result<String, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            let credentials = normalize_coinbase_api_credentials(&key, &secret)
                .map_err(|message| error("Error", &message))?;
            let now = r.host.current_time_ms()?;
            let header = CdpJwtHeader {
                algorithm: credentials.algorithm,
                typ: "JWT",
                key_id: credentials.key_name.clone(),
                nonce: r.host.uuid()?,
            };
            let payload = CdpJwtPayload {
                issuer: if credentials.algorithm == CoinbaseSignatureAlgorithm::EdDsa {
                    "cdp"
                } else {
                    "coinbase-cloud"
                },
                audience: (credentials.algorithm == CoinbaseSignatureAlgorithm::EdDsa)
                    .then(|| vec!["cdp_service"]),
                not_before: (now / 1000.0).floor(),
                expires: (now / 1000.0).floor() + 120.0,
                subject: credentials.key_name,
                uri: uri.filter(|v| !v.is_empty()),
            };
            // Insertion order exactly follows the two source object literals.
            let header_json = json!({"alg":if header.algorithm==CoinbaseSignatureAlgorithm::EdDsa {"EdDSA"}else{"ES256"},"typ":header.typ,"kid":header.key_id,"nonce":header.nonce});
            let mut payload_json = serde_json::Map::new();
            payload_json.insert("iss".into(), Value::from(payload.issuer));
            if let Some(audience) = &payload.audience {
                payload_json.insert("aud".into(), json!(audience));
            }
            payload_json.insert("nbf".into(), json!(payload.not_before));
            payload_json.insert("exp".into(), json!(payload.expires));
            payload_json.insert("sub".into(), json!(payload.subject));
            if let Some(uri) = &payload.uri {
                payload_json.insert("uri".into(), json!(uri));
            }
            let signing_input = format!(
                "{}.{}",
                base64_url_encode_string(&stringify(header_json)),
                base64_url_encode_string(&stringify(Value::Object(payload_json)))
            );
            let input = CdpSigningInput {
                header,
                payload,
                message: signing_input.as_bytes().to_vec(),
            };
            let signature = if credentials.algorithm == CoinbaseSignatureAlgorithm::EdDsa {
                r.host.sign_ed25519(
                    input,
                    credentials.ed25519_seed.expect("normalized Ed25519 seed"),
                )?
            } else {
                let key = get_signing_key(r.clone(), credentials.key_secret).await?;
                r.host.sign_ecdsa("ECDSA", "SHA-256", key, input)?.await?
            };
            Ok(format!("{signing_input}.{}", base64_url_encode(&signature)))
        }),
        &spawn,
    )
}
