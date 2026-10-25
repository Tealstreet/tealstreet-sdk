//! Binance canonicalisation recipes. Cryptographic operations and credentials
//! are owned by the injected Signer; this module never receives a secret.
pub use safe_cex_engine::signing::Identity as Credentials;
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
pub use safe_cex_engine::url::Query;
use safe_cex_engine::{Action, SignStep, num};
pub const RECV_WINDOW_MS: u32 = 20_000;

/// Exact-byte work for the trusted hosted-signing facade. Serialization and
/// identity selection belong to the caller; these helpers never execute crypto.
pub mod hosted_signing {
    use super::*;

    pub fn http_work(
        identity: &Identity,
        exact_encoded_query: String,
        stamp_ms: f64,
        context: Context,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &identity.credential,
            identity.signing_ref()?,
            vec![bytes(exact_encoded_query.into_bytes())],
            context,
            SignReceipt {
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
            },
        ))
    }

    pub fn ws_work(
        identity: &Identity,
        exact_preimage: String,
        context: Context,
    ) -> Result<SignRequest, SignError> {
        Ok(hmac_sha256_work(
            &identity.credential,
            identity.signing_ref()?,
            vec![bytes(exact_preimage.into_bytes())],
            context,
            SignReceipt::default(),
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use safe_cex_crypto::signer::LocalSigner;
        use safe_cex_engine::signing::{Algorithm, KeyRef};

        fn identity() -> Identity {
            Identity::new(
                KeyRef("primary.credential".into()),
                Some(KeyRef("primary.signing".into())),
            )
        }

        fn mac(work: &SignRequest) -> String {
            let (signer, _) = LocalSigner::from_json(
                r#"{"primary":{"key":"primary-key+%é","secret":"fixture-secret"}}"#.into(),
            )
            .unwrap();
            let signed = signer.sign_now(work).unwrap();
            assert_eq!(signed.values[0], "primary-key+%é".as_bytes());
            safe_cex_crypto::hex(&signed.values[1])
        }

        #[test]
        fn exact_query_keeps_reserved_field_positions_repeated_values_and_percent_spelling() {
            let id = identity();
            let context = Context {
                venue: "binance".into(),
                method: "POST".into(),
                host: "fapi.binance.com".into(),
                path: "/fapi/v1/order".into(),
            };
            let query = "symbol=BTCUSDT&timestamp=1709209849123&recvWindow=5000&note=a%2Bb%20%C3%A9&value=first&value=second";
            let work = http_work(&id, query.into(), 1_709_209_849_123., context.clone()).unwrap();
            assert_eq!(work.context, context);
            assert_eq!(
                work.receipt,
                SignReceipt {
                    stamp_ms: Some(1_709_209_849_123.),
                    expiry_ms: None
                }
            );
            assert_eq!(
                work.operations,
                vec![
                    Operation::Present {
                        key: id.credential.clone(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: id.signing_ref().unwrap().clone(),
                        payload: vec![PayloadPart::Bytes {
                            bytes: query.as_bytes().to_vec()
                        }],
                    },
                ]
            );
            // Fixed independent Python stdlib HMAC-SHA256 vector.
            assert_eq!(
                mac(&work),
                "2cf174dc789e88cf3d35b79599e83e61296a773686ae0e347d73ed710ba00ac0"
            );
            assert!(
                http_work(&identity(), String::new(), 1_000.5, Context::default())
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }

        #[test]
        fn exact_ws_preimage_does_not_draw_a_new_stamp_or_rewrite_key_escaping() {
            let id = identity();
            let work = ws_work(
                &id,
                "apiKey=primary-key%2B%25%C3%A9&timestamp=1709209849123".into(),
                Context::default(),
            )
            .unwrap();
            assert_eq!(work.receipt, SignReceipt::default());
            assert_eq!(
                work.operations,
                vec![
                    Operation::Present {
                        key: id.credential.clone(),
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: id.signing_ref().unwrap().clone(),
                        payload: vec![PayloadPart::Bytes {
                            bytes: b"apiKey=primary-key%2B%25%C3%A9&timestamp=1709209849123"
                                .to_vec()
                        }],
                    },
                ]
            );
            assert_eq!(
                mac(&work),
                "47dbb20a2c36798579807e450312ce25eedb069b35edac2115ff85334800b390"
            );
        }
    }
}

pub(crate) struct Http {
    credentials: Credentials,
    query: Query,
    recv_window: Option<f64>,
    signed: bool,
    header_policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
}
impl Http {
    pub fn new(
        credentials: Credentials,
        query: Query,
        recv_window: Option<f64>,
        signed: bool,
    ) -> Self {
        Self {
            credentials,
            query,
            recv_window,
            signed,
            header_policy: Default::default(),
        }
    }
    pub fn with_header_policy(
        mut self,
        policy: safe_cex_engine::signing_headers::ResultHeaderPolicy,
    ) -> Self {
        if self.signed {
            self.header_policy = policy;
        }
        self
    }
}
fn bytes(bytes: impl Into<Vec<u8>>) -> PayloadPart {
    PayloadPart::Bytes {
        bytes: bytes.into(),
    }
}
fn request_context(url: &str, method: &str) -> Context {
    let (host, path) = url
        .split_once("://")
        .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
        .unwrap_or(("", ""));
    Context {
        venue: "binance".into(),
        method: method.into(),
        host: host.into(),
        path: format!("/{path}"),
    }
}
fn credential(value: &[u8]) -> Result<&str, SignError> {
    let value = std::str::from_utf8(value).map_err(|_| SignError::InvalidResult)?;
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    Ok(value)
}
fn hex(bytes: &[u8]) -> String {
    safe_cex_crypto::hex(bytes)
}
impl SignStep for Http {
    fn result_header_policy(&self) -> safe_cex_engine::signing_headers::ResultHeaderPolicy {
        self.header_policy
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        context: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let now = context.stamp_ms();
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let context = request_context(&http.url, http.method.as_str());
        if self.signed {
            let query = self
                .query
                .clone()
                .num("timestamp", now)
                .num(
                    "recvWindow",
                    self.recv_window.unwrap_or(f64::from(RECV_WINDOW_MS)),
                )
                .into_string();
            http.url.push('?');
            http.url.push_str(&query);
            return hosted_signing::http_work(&self.credentials, query, now, context);
        }
        Ok(SignRequest {
            receipt: SignReceipt::default(),
            operations: vec![Operation::Present {
                key: self.credentials.credential.clone(),
            }],
            context,
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let key = credential(&result.values[0])?;
        if self.signed {
            http.url.push_str("&signature=");
            http.url.push_str(&hex(&result.values[1]));
        }
        http.headers.push(("X-MBX-APIKEY", key.into()));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        let window = self.recv_window.unwrap_or(f64::from(RECV_WINDOW_MS));
        if window.is_nan() {
            1.
        } else {
            window.clamp(1., 5000.)
        }
    }
}

pub(crate) struct SpotSubscribe {
    credentials: Credentials,
    id: String,
    timestamp: f64,
}
impl SpotSubscribe {
    pub fn new(credentials: Credentials, id: String) -> Self {
        Self {
            credentials,
            id,
            timestamp: 0.,
        }
    }
}
impl SignStep for SpotSubscribe {
    fn prepare(
        &mut self,
        _: &mut Action,
        context: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let now = context.stamp_ms();
        self.timestamp = now;
        Ok(hmac_sha256_work(
            &self.credentials.credential,
            self.credentials.signing_ref()?,
            vec![
                bytes(b"apiKey=".to_vec()),
                PayloadPart::Credential {
                    key: self.credentials.credential.clone(),
                },
                bytes(
                    format!("&recvWindow=5000&timestamp={}", num::format(self.timestamp))
                        .into_bytes(),
                ),
            ],
            Context {
                venue: "binancespot".into(),
                method: "WS".into(),
                path: "userDataStream.subscribe.signature".into(),
                ..Default::default()
            },
            SignReceipt {
                stamp_ms: Some(self.timestamp),
                expiry_ms: None,
            },
        ))
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::WsSend { text, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        // Struct declaration order preserves the existing serialized wire bytes.
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Params<'a> {
            api_key: &'a str,
            recv_window: u32,
            timestamp: f64,
            signature: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Subscription<'a> {
            id: &'a str,
            method: &'a str,
            params: Params<'a>,
        }
        let signature = hex(&result.values[1]);
        *text = serde_json::to_string(&Subscription {
            id: &self.id,
            method: "userDataStream.subscribe.signature",
            params: Params {
                api_key: credential(&result.values[0])?,
                recv_window: 5000,
                timestamp: self.timestamp,
                signature: &signature,
            },
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

#[cfg(test)]
pub(crate) mod oracle {
    // Signed REST requests: Binance HMACs the query string and takes every
    // parameter, including POST/PUT/DELETE data, in the query.
    use safe_cex_crypto::{Encoding, hmac_sha256};

    pub const RECV_WINDOW_MS: u32 = 20_000;

    #[derive(Clone)]
    pub struct LegacyCredentials {
        pub key: String,
        pub secret: String,
    }

    pub use safe_cex_engine::url::Query;

    pub trait Signed {
        /// Append `timestamp`, `recvWindow` and the HMAC-SHA256 `signature`.
        fn signed(
            self,
            credentials: &LegacyCredentials,
            now_ms: f64,
            recv_window: Option<f64>,
        ) -> String;
    }

    impl Signed for Query {
        fn signed(
            self,
            credentials: &LegacyCredentials,
            now_ms: f64,
            recv_window: Option<f64>,
        ) -> String {
            let mut query = self
                .num("timestamp", now_ms.floor())
                .num(
                    "recvWindow",
                    recv_window.unwrap_or(f64::from(RECV_WINDOW_MS)),
                )
                .into_string();
            let signature = hmac_sha256(
                credentials.secret.as_bytes(),
                query.as_bytes(),
                Encoding::Hex,
            );
            query.push_str("&signature=");
            query.push_str(&signature);
            query
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn signs_the_documented_example() {
            // developers.binance.com, "SIGNED Endpoint Examples for POST /fapi/v1/order".
            #[derive(serde::Deserialize)]
            struct Vector {
                secret: String,
                payload: String,
                signature: String,
            }
            let vector: Vector = serde_json::from_str(include_str!(
                "../../../crypto/tests/fixtures/hmac/binance.json"
            ))
            .unwrap();
            let credentials = LegacyCredentials {
                key: String::new(),
                secret: vector.secret,
            };
            let payload = vector.payload.as_str();
            assert_eq!(
                hmac_sha256(
                    credentials.secret.as_bytes(),
                    payload.as_bytes(),
                    Encoding::Hex
                ),
                vector.signature
            );
            let signed =
                Query::new()
                    .str("symbol", "BTCUSDT")
                    .signed(&credentials, 1591702613943.7, None);
            assert!(
                signed.starts_with(
                    "symbol=BTCUSDT&timestamp=1591702613943&recvWindow=20000&signature="
                )
            );
        }

        #[test]
        fn percent_encodes_json_parameters() {
            let query = Query::new().str("batchOrders", r#"[{"a":"b c"}]"#);
            assert_eq!(
                query.as_str(),
                "batchOrders=%5B%7B%22a%22%3A%22b%20c%22%7D%5D"
            );
        }
    }

    safe_cex_crypto::erase_credentials_on_drop!(LegacyCredentials, key, secret);

    #[cfg(test)]
    mod security_receive_window {
        use super::*;

        #[test]
        fn configured_receive_window_is_signed_without_changing_supported_values() {
            let c = LegacyCredentials {
                key: "k".into(),
                secret: "s".into(),
            };
            for (window, expected) in [
                (None, 20_000.0),
                (Some(500.0), 500.0),
                (Some(0.0), 0.0),
                (Some(500.125), 500.125),
                (Some(60_000.0), 60_000.0),
            ] {
                let query = Query::new()
                    .str("symbol", "BTCUSDT")
                    .signed(&c, 1000.0, window);
                let (payload, signature) = query.split_once("&signature=").unwrap();
                assert!(payload.ends_with(&format!("recvWindow={expected}")));
                assert_eq!(
                    signature,
                    hmac_sha256(b"s", payload.as_bytes(), Encoding::Hex)
                );
            }
        }
    }
}
