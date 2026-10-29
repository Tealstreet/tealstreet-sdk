//! The Rust twin of packages/safe-cex/src/exchanges/rust/remoteSignerProvider.ts:
//! engine sign work becomes today's ws-auth `RemoteSignerRequest`, so the
//! browser's existing signing host answers it unchanged.
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use tokio::time::Instant;

use base64::Engine as _;
use safe_cex_engine::signing::{
    Algorithm, Capability, Encoding as PartEncoding, KeyRef, Operation, OwnedSetupExecution,
    PayloadPart, SignError, SignFuture, SignReceipt, SignRequest, SignResult, Signer,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::link::{Failure, SignerLink};
use crate::policy::{Encoding, ExtraData, Policy, Wire};

const MAX_PAYLOAD: usize = 1_048_576;
const MAX_OPERATIONS: usize = 32;
const MAX_PARTS: usize = 256;
const LIGHTER_SIGNATURE: usize = 80;
/// `WebSocketRemoteSigner.MAX_CONSECUTIVE_FAILURES` and `FATAL_MIN_STREAK_MS`.
pub const MAX_CONSECUTIVE_FAILURES: u32 = 10;
pub const FATAL_MIN_STREAK: std::time::Duration = std::time::Duration::from_millis(30_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Family {
    Hmac,
    DoubleSha256,
    Hyperliquid,
    Lighter,
    Paradex,
}
impl Family {
    pub(crate) fn of(venue: &str) -> Self {
        match venue {
            "bitunix" => Self::DoubleSha256,
            "hyperliquid" | "hyperliquidspot" => Self::Hyperliquid,
            "lighter" | "lighter-robinhood" => Self::Lighter,
            "paradex" => Self::Paradex,
            _ => Self::Hmac,
        }
    }
    fn algorithm(self) -> Algorithm {
        match self {
            Self::Hmac => Algorithm::HmacSha256,
            Self::DoubleSha256 => Algorithm::Sha256SecretSuffix,
            Self::Hyperliquid => Algorithm::Secp256k1Digest,
            Self::Lighter => Algorithm::SchnorrDigest,
            Self::Paradex => Algorithm::StarkDigest,
        }
    }
}

/// One owned identity: opaque engine refs plus the public key text the device knows.
pub(crate) struct Owned {
    pub credential: KeyRef,
    pub signing: KeyRef,
    pub public: Vec<u8>,
    pub passphrase: Option<(KeyRef, Vec<u8>)>,
}

#[derive(Default)]
struct Streak {
    count: u32,
    started: Option<Instant>,
    fired: bool,
}

pub(crate) struct Inner {
    pub venue: String,
    pub family: Family,
    pub policy: Policy,
    pub account_name: Option<String>,
    pub link: Arc<SignerLink>,
    pub primary: Owned,
    bound: Mutex<Option<Vec<u8>>>,
    streak: Mutex<Streak>,
    on_fatal: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

fn refused<T>() -> Result<T, SignError> {
    Err(SignError::Refused)
}

fn nanoid() -> String {
    const ALPHABET: &[u8; 64] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";
    let mut bytes = [0u8; 21];
    getrandom::getrandom(&mut bytes).expect("OS entropy");
    bytes
        .iter()
        .map(|byte| ALPHABET[(byte & 63) as usize] as char)
        .collect()
}

fn hex_bytes(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// `0x` then exactly `length` bytes of hex.
fn prefixed(value: &Value, length: usize) -> Result<Vec<u8>, SignError> {
    let text = value.as_str().ok_or(SignError::Refused)?;
    let body = text.strip_prefix("0x").ok_or(SignError::Refused)?;
    if body.len() != length * 2 {
        return refused();
    }
    hex_bytes(body).ok_or(SignError::Refused)
}

/// One 32-byte MAC in the family's declared presentation; anything else refuses.
fn mac(value: &Value, encoding: Encoding) -> Result<Vec<u8>, SignError> {
    let text = value.as_str().ok_or(SignError::Refused)?;
    let base64_body = |text: &str, data: usize, pad: &str| {
        text.len() == data + pad.len()
            && text.ends_with(pad)
            && text[..data]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/')
    };
    match encoding {
        Encoding::Hex => {
            if text.len() != 64 {
                return refused();
            }
            hex_bytes(text).ok_or(SignError::Refused)
        }
        Encoding::Base64 => {
            if !base64_body(text, 43, "=") {
                return refused();
            }
            // Strict decoding refuses nonzero unused pad bits.
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(text)
                .map_err(|_| SignError::Refused)?;
            if bytes.len() != 32 {
                return refused();
            }
            Ok(bytes)
        }
        Encoding::HexBase64 => {
            if !base64_body(text, 86, "==") {
                return refused();
            }
            let inner = base64::engine::general_purpose::STANDARD
                .decode(text)
                .map_err(|_| SignError::Refused)?;
            if inner.len() != 64
                || !inner
                    .iter()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            {
                return refused();
            }
            hex_bytes(std::str::from_utf8(&inner).map_err(|_| SignError::Refused)?)
                .ok_or(SignError::Refused)
        }
    }
}

/// A canonical decimal safe integer.
fn stamp(text: &str) -> Option<f64> {
    let canonical = text == "0"
        || (!text.is_empty()
            && !text.starts_with('0')
            && text.bytes().all(|byte| byte.is_ascii_digit()));
    let value: f64 = text.parse().ok()?;
    (canonical && value <= 9_007_199_254_740_991.).then_some(value)
}

/// Decode only the declared prepared WS grammar; never draw a new timestamp.
fn ws_metadata(
    wire: Option<&Wire>,
    payload: &str,
    key: &str,
    receipt: SignReceipt,
    credential_delimited: bool,
    suffix_delimited: bool,
) -> Result<Option<Value>, SignError> {
    let Some(wire) = wire else { return Ok(None) };
    if receipt.stamp_ms.is_some() && !credential_delimited && !suffix_delimited {
        return Ok(None);
    }
    match (wire.extra_data, &wire.seconds_suffix) {
        (Some(ExtraData::SecondsTimestamp), Some((method, path))) => {
            let Some(prefix) = payload.strip_suffix(&format!("{method}{path}")) else {
                return Ok(None);
            };
            let timestamp = stamp(prefix).ok_or(SignError::Refused)?;
            if timestamp * 1000. > 9_007_199_254_740_991.
                || receipt
                    .stamp_ms
                    .is_some_and(|stamp| stamp != timestamp * 1000.)
            {
                return refused();
            }
            Ok(Some(json!({ "timestamp": timestamp as u64 })))
        }
        (Some(ExtraData::BybitExpiry), _) => {
            let Some(rest) = payload.strip_prefix("GET/realtime") else {
                return Ok(None);
            };
            let expiry = stamp(rest).ok_or(SignError::Refused)?;
            if receipt.expiry_ms.is_some_and(|expected| expected != expiry) {
                return refused();
            }
            Ok(Some(json!({ "expires": expiry as u64 })))
        }
        (Some(ExtraData::BinanceSpot), _) => {
            let Some(rest) = payload.strip_prefix(&format!("apiKey={key}&recvWindow=")) else {
                return Ok(None);
            };
            let tail: Vec<&str> = rest.split("&timestamp=").collect();
            let [window, time] = tail.as_slice() else {
                return refused();
            };
            let (Some(window), Some(time)) = (stamp(window), stamp(time)) else {
                return refused();
            };
            if receipt.stamp_ms.is_some_and(|stamp| stamp != time) {
                return refused();
            }
            Ok(Some(
                json!({ "apiKey": key, "recvWindow": window as u64, "timestamp": time as u64 }),
            ))
        }
        _ => Ok(None),
    }
}

/// Exactly one bytes part, at most `max` long.
fn single_bytes(parts: &[PayloadPart], max: usize) -> Result<&[u8], SignError> {
    match parts {
        [PayloadPart::Bytes { bytes }] if bytes.len() <= max => Ok(bytes),
        _ => refused(),
    }
}

fn utf8(bytes: &[u8]) -> Result<String, SignError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| SignError::Refused)
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

impl Inner {
    pub(crate) fn new(
        venue: String,
        policy: Policy,
        account_name: Option<String>,
        link: Arc<SignerLink>,
        primary: Owned,
    ) -> Self {
        Self {
            family: Family::of(&venue),
            venue,
            policy,
            account_name,
            link,
            primary,
            bound: Mutex::new(None),
            streak: Mutex::new(Streak::default()),
            on_fatal: Mutex::new(None),
        }
    }

    pub(crate) fn set_fatal(&self, callback: Box<dyn FnOnce() + Send>) {
        *self.on_fatal.lock().unwrap_or_else(|p| p.into_inner()) = Some(callback);
    }

    fn record_failure(&self) {
        let fire = {
            let mut streak = self.streak.lock().unwrap_or_else(|p| p.into_inner());
            if streak.count == 0 {
                streak.started = Some(Instant::now());
            }
            streak.count += 1;
            let long = streak
                .started
                .is_some_and(|started| started.elapsed() >= FATAL_MIN_STREAK);
            if streak.count >= MAX_CONSECUTIVE_FAILURES && long && !streak.fired {
                streak.fired = true;
                true
            } else {
                false
            }
        };
        if fire
            && let Some(callback) = self
                .on_fatal
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take()
        {
            callback();
        }
    }

    fn record_success(&self) {
        self.streak.lock().unwrap_or_else(|p| p.into_inner()).count = 0;
    }

    /// One ws-auth round trip; returns `wsAuthSignature`. Failures count toward
    /// the account's fatal streak exactly where the TS signer counts them.
    async fn ask(
        &self,
        id: String,
        exchange_name: &str,
        to_sign: String,
        output_format: &str,
        extra_data: Option<Value>,
    ) -> Result<Value, SignError> {
        let mut ws_auth = Map::new();
        ws_auth.insert("toSign".into(), to_sign.into());
        ws_auth.insert("outputFormat".into(), output_format.into());
        if let Some(extra) = extra_data {
            ws_auth.insert("extraData".into(), extra);
        }
        let mut request = Map::new();
        request.insert("id".into(), id.clone().into());
        request.insert("type".into(), "ws-auth".into());
        request.insert("exchangeName".into(), exchange_name.into());
        if let Some(name) = &self.account_name {
            request.insert("accountName".into(), name.clone().into());
        }
        request.insert("wsAuth".into(), ws_auth.into());
        let response = match self.link.round_trip(&id, &Value::Object(request)).await {
            Ok(response) => response,
            Err(Failure::Suspended | Failure::Aborted) => return Err(SignError::Unavailable),
            Err(Failure::Duplicate) => return refused(),
            Err(Failure::TimedOut) => {
                self.record_failure();
                return Err(SignError::TimedOut);
            }
            Err(Failure::Unavailable | Failure::SendFailed) => {
                self.record_failure();
                return Err(SignError::Unavailable);
            }
        };
        let error = response.get("error");
        if error.is_some_and(|error| error.as_str() != Some("")) {
            self.record_failure();
            return refused();
        }
        self.record_success();
        if response.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return refused();
        }
        Ok(response
            .get("wsAuthSignature")
            .cloned()
            .unwrap_or(Value::Null))
    }

    async fn device(&self, to_sign: String, output_format: &str) -> Result<Value, SignError> {
        self.ask(nanoid(), &self.venue, to_sign, output_format, None)
            .await
    }

    fn bound(&self) -> Option<Vec<u8>> {
        self.bound.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// One provider binds one device key; a different one is never adopted.
    fn bind(&self, key: &[u8]) -> Result<(), SignError> {
        let mut bound = self.bound.lock().unwrap_or_else(|p| p.into_inner());
        if bound.as_deref().is_some_and(|bound| bound != key) {
            return refused();
        }
        *bound = Some(key.to_vec());
        Ok(())
    }

    fn owns_signing(&self, key: &KeyRef) -> bool {
        *key == self.primary.signing
    }

    pub(crate) async fn sign(
        self: Arc<Self>,
        request: SignRequest,
    ) -> Result<SignResult, SignError> {
        let receipt = request.receipt;
        if request.operations.is_empty() || request.operations.len() > MAX_OPERATIONS {
            return refused();
        }
        let operations = request.operations;
        let values = match self.family {
            Family::Hyperliquid => self.sign_wallet(&operations).await?,
            Family::Lighter => self.sign_lighter(&operations).await?,
            Family::Paradex => self.sign_stark(&operations).await?,
            Family::DoubleSha256 => self.sign_double_sha256(&operations).await?,
            Family::Hmac => self.sign_hmac(&operations, receipt).await?,
        };
        Ok(SignResult::new(values, receipt))
    }

    /// Every keyed operation names the primary signing ref.
    fn keyed_by_signing(&self, operations: &[Operation]) -> Result<(), SignError> {
        for operation in operations {
            let key = match operation {
                Operation::Sign { key, .. }
                | Operation::PublicAddress { key }
                | Operation::PublicKey { key, .. }
                | Operation::Present { key } => key,
                Operation::Sha256 { .. } => continue,
            };
            if !self.owns_signing(key) {
                return refused();
            }
        }
        Ok(())
    }

    async fn sign_wallet(&self, operations: &[Operation]) -> Result<Vec<Vec<u8>>, SignError> {
        self.keyed_by_signing(operations)?;
        match operations {
            [Operation::PublicAddress { .. }] => {
                let public = utf8(&self.primary.public)?;
                let address = prefixed(&self.device(public, "hyperliquid-address").await?, 20)?;
                self.bind(&address)?;
                Ok(vec![address])
            }
            [
                Operation::Sha256 { payload: statement },
                Operation::PublicAddress { .. },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    payload,
                    ..
                },
            ] => {
                let bound = self.bound().ok_or(SignError::Refused)?;
                let preimage = single_bytes(statement, MAX_PAYLOAD)?.to_vec();
                if single_bytes(payload, 32)?.len() != 32 {
                    return refused();
                }
                let signature =
                    prefixed(&self.device(utf8(&preimage)?, "hyperliquid-l1").await?, 65)?;
                Ok(vec![sha256(&preimage), bound, signature])
            }
            _ => refused(),
        }
    }

    async fn sign_lighter(&self, operations: &[Operation]) -> Result<Vec<Vec<u8>>, SignError> {
        let [Operation::Sha256 { payload }, signs @ ..] = operations else {
            return refused();
        };
        if signs.is_empty()
            || !signs.iter().all(|sign| {
                matches!(sign, Operation::Sign { algorithm: Algorithm::SchnorrDigest, key, .. } if self.owns_signing(key))
            })
        {
            return refused();
        }
        let preimage = single_bytes(payload, MAX_PAYLOAD)?.to_vec();
        let joined = prefixed(
            &self.device(utf8(&preimage)?, "lighter-tx").await?,
            signs.len() * LIGHTER_SIGNATURE,
        )?;
        let mut values = vec![sha256(&preimage)];
        values.extend(joined.chunks(LIGHTER_SIGNATURE).map(<[u8]>::to_vec));
        Ok(values)
    }

    async fn sign_stark(&self, operations: &[Operation]) -> Result<Vec<Vec<u8>>, SignError> {
        self.keyed_by_signing(operations)?;
        match operations {
            [
                Operation::PublicKey {
                    algorithm: Algorithm::StarkDigest,
                    ..
                },
            ] => {
                let public = utf8(&self.primary.public)?;
                let key = prefixed(&self.device(public, "paradex-public").await?, 32)?;
                self.bind(&key)?;
                Ok(vec![key])
            }
            [Operation::Sha256 { payload }, signs @ ..] => {
                if self.bound().is_none()
                    || signs.is_empty()
                    || signs.len() > 10
                    || !signs.iter().all(|sign| {
                        matches!(
                            sign,
                            Operation::Sign {
                                algorithm: Algorithm::StarkDigest,
                                ..
                            }
                        )
                    })
                {
                    return refused();
                }
                let preimage = single_bytes(payload, MAX_PAYLOAD)?.to_vec();
                let joined = prefixed(
                    &self.device(utf8(&preimage)?, "paradex-stark").await?,
                    64 * signs.len(),
                )?;
                let mut values = vec![sha256(&preimage)];
                values.extend(joined.chunks(64).map(<[u8]>::to_vec));
                Ok(values)
            }
            _ => refused(),
        }
    }

    /// `[sha256 bytes|own credential, sign sha256SecretSuffix result{0,hex}, present credential]`.
    async fn sign_double_sha256(
        &self,
        operations: &[Operation],
    ) -> Result<Vec<Vec<u8>>, SignError> {
        let [
            Operation::Sha256 { payload: digest },
            Operation::Sign {
                algorithm: Algorithm::Sha256SecretSuffix,
                key,
                payload: reference,
            },
            Operation::Present { key: presented },
        ] = operations
        else {
            return refused();
        };
        if !self.owns_signing(key)
            || *presented != self.primary.credential
            || !matches!(
                reference.as_slice(),
                [PayloadPart::Result {
                    index: 0,
                    encoding: PartEncoding::Hex
                }]
            )
            || digest.is_empty()
            || digest.len() > MAX_PARTS
        {
            return refused();
        }
        let credential = &self.primary.public;
        let mut preimage = Vec::new();
        for part in digest {
            match part {
                PayloadPart::Credential { key } if *key == self.primary.credential => {
                    preimage.extend_from_slice(credential)
                }
                PayloadPart::Bytes { bytes } => preimage.extend_from_slice(bytes),
                _ => return refused(),
            }
            if preimage.len() + credential.len() > MAX_PAYLOAD {
                return refused();
            }
        }
        let outer = mac(
            &self
                .device(utf8(&preimage)?, "bitunix-double-sha256")
                .await?,
            Encoding::Hex,
        )?;
        Ok(vec![sha256(&preimage), outer, credential.clone()])
    }

    async fn sign_hmac(
        &self,
        operations: &[Operation],
        receipt: SignReceipt,
    ) -> Result<Vec<Vec<u8>>, SignError> {
        enum Work {
            Present(Vec<u8>),
            Sign {
                payload: String,
                extra: Option<Value>,
            },
        }
        let primary = &self.primary;
        let mut total = 0usize;
        let mut work = Vec::with_capacity(operations.len());
        let mut presents_passphrase = false;
        let mut signs = false;
        for operation in operations {
            match operation {
                Operation::Present { key } => {
                    let material = if *key == primary.credential {
                        &primary.public
                    } else {
                        match &primary.passphrase {
                            Some((passphrase, value)) if passphrase == key => {
                                presents_passphrase = true;
                                value
                            }
                            _ => return refused(),
                        }
                    };
                    total += material.len();
                    if total > MAX_PAYLOAD {
                        return refused();
                    }
                    work.push(Work::Present(material.clone()));
                }
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key,
                    payload,
                } => {
                    if !self.owns_signing(key) || payload.len() > MAX_PARTS {
                        return refused();
                    }
                    let mut parts = Vec::with_capacity(payload.len());
                    for part in payload {
                        let bytes: &[u8] = match part {
                            PayloadPart::Credential { key } if *key == primary.credential => {
                                &primary.public
                            }
                            PayloadPart::Bytes { bytes } => bytes,
                            _ => return refused(),
                        };
                        total += bytes.len();
                        if total > MAX_PAYLOAD {
                            return refused();
                        }
                        parts.push(bytes);
                    }
                    let text = utf8(&parts.concat())?;
                    let credential_delimited =
                        payload.len() == 3 && matches!(payload[1], PayloadPart::Credential { .. });
                    let wire = self.policy.wire.as_ref();
                    let suffix_delimited = match wire.and_then(|wire| {
                        (wire.extra_data == Some(ExtraData::SecondsTimestamp))
                            .then_some(wire.seconds_suffix.as_ref())
                            .flatten()
                    }) {
                        Some((method, path)) => {
                            payload.len() == 3
                                && payload
                                    .iter()
                                    .all(|part| matches!(part, PayloadPart::Bytes { .. }))
                                && parts[1] == method.as_bytes()
                                && parts[2] == path.as_bytes()
                        }
                        None => false,
                    };
                    let public = utf8(&primary.public)?;
                    let extra = ws_metadata(
                        wire,
                        &text,
                        &public,
                        receipt,
                        credential_delimited,
                        suffix_delimited,
                    )?;
                    signs = true;
                    work.push(Work::Sign {
                        payload: text,
                        extra,
                    });
                }
                _ => return refused(),
            }
        }
        if presents_passphrase && !signs {
            return refused();
        }
        let encoding = self.policy.encoding;
        let mut values = Vec::with_capacity(work.len());
        for item in work {
            match item {
                Work::Present(bytes) => values.push(bytes),
                Work::Sign { payload, extra } => {
                    let wire = self.policy.wire.as_ref();
                    let (id, exchange_name) = match (&extra, wire) {
                        (Some(_), Some(wire)) => (
                            wire.request_id.clone().unwrap_or_else(nanoid),
                            wire.exchange_name.as_str(),
                        ),
                        _ => (nanoid(), self.venue.as_str()),
                    };
                    let signature = self
                        .ask(id, exchange_name, payload, encoding.wire(), extra)
                        .await?;
                    values.push(mac(&signature, encoding)?);
                }
            }
        }
        Ok(values)
    }
}

/// The installed engine provider for one hosted account.
pub struct RemoteProvider {
    pub(crate) inner: Arc<Inner>,
}

impl Signer for RemoteProvider {
    fn sign(&self, request: SignRequest) -> SignFuture {
        Box::pin(self.inner.clone().sign(request))
    }

    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        if self.inner.owns_signing(key) && algorithm == self.inner.family.algorithm() {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }

    fn owned_public_address_capability(&self) -> Capability {
        match self.inner.family {
            Family::Hyperliquid | Family::Paradex => Capability::Supported,
            _ => Capability::Unsupported,
        }
    }

    fn create_public_address_setup(
        &self,
        request: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        let admitted = match (self.inner.family, request.operations.as_slice()) {
            (Family::Hyperliquid, [Operation::PublicAddress { key }]) => {
                self.inner.owns_signing(key)
            }
            (
                Family::Paradex,
                [
                    Operation::PublicKey {
                        algorithm: Algorithm::StarkDigest,
                        key,
                    },
                ],
            ) => self.inner.owns_signing(key),
            _ => false,
        };
        if !admitted
            || request.receipt != Default::default()
            || request.context != Default::default()
        {
            return None;
        }
        Some(Box::new(Setup {
            work: Some(Box::pin(self.inner.clone().sign(request))),
            running: None,
            started: false,
        }))
    }
}

type Work = Pin<Box<dyn Future<Output = Result<SignResult, SignError>> + Send>>;

/// The address or public-key binding at startup, over the same device wire.
/// Dropping the future withdraws its pending request, so join is immediate.
struct Setup {
    work: Option<Work>,
    running: Option<Work>,
    started: bool,
}

impl OwnedSetupExecution for Setup {
    fn start(&mut self) -> Result<(), SignError> {
        if self.started {
            return Err(SignError::InvalidRequest);
        }
        self.started = true;
        self.running = self.work.take();
        Ok(())
    }
    fn cancel(&mut self) -> Result<(), SignError> {
        self.started = true;
        self.work = None;
        self.running = None;
        Ok(())
    }
    fn poll_result(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Result<SignResult, SignError>> {
        let Some(running) = &mut self.running else {
            return Poll::Pending;
        };
        let result = running.as_mut().poll(cx);
        if result.is_ready() {
            self.running = None;
        }
        result
    }
    fn poll_joined(&mut self, _: &mut std::task::Context<'_>) -> Poll<Result<(), SignError>> {
        if self.started && self.running.is_none() {
            Poll::Ready(Ok(()))
        } else {
            Poll::Pending
        }
    }
}
