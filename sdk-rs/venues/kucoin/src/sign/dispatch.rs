//! Atomic native request, passphrase and optional broker MACs.
use safe_cex_engine::signing::{
    Algorithm, Context, ExtraKey, Identity, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

#[derive(Clone, Debug)]
pub struct Broker {
    pub partner: String,
    pub name: String,
    pub verify: bool,
}

pub struct Http {
    identity: Identity,
    method: Method,
    origin: &'static str,
    path: String,
    body: Option<String>,
    broker: Option<Broker>,
    receipt: Option<SignReceipt>,
}

impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        origin: &'static str,
        path: String,
        body: Option<String>,
        broker: Option<Broker>,
    ) -> Self {
        let broker = broker
            .map(|broker| Broker {
                partner: broker.partner.trim().into(),
                name: broker.name.trim().into(),
                verify: broker.verify,
            })
            .filter(|broker| {
                !broker.partner.is_empty()
                    && !broker.name.is_empty()
                    && identity.extra.contains_key(&ExtraKey::KucoinBroker)
            });
        Self {
            identity,
            method,
            origin,
            path,
            body,
            broker,
            receipt: None,
        }
    }

    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if !matches!(
            self.origin,
            "https://api-futures.kucoin.com" | "https://api.kucoin.com"
        ) || !self.path.starts_with('/')
            || self.path.starts_with("//")
            || self.path.contains(['\r', '\n'])
            || http.method != self.method
            || http.url != format!("{}{}", self.origin, crate::encoded_path(&self.path))
            || http.body != self.body
            || !http.signer_headers.is_empty()
            || http.headers.len() != super::PUBLIC_HEADERS.len()
            || http
                .headers
                .iter()
                .zip(super::PUBLIC_HEADERS)
                .any(|((name, value), (n, v))| name != n || value != v)
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}

impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        super::PUBLIC_HEADERS
    }

    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.receipt = None;
        self.attest(outbound)?;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        let passphrase = self
            .identity
            .passphrase
            .as_ref()
            .ok_or(SignError::InvalidRequest)?;
        let stamp = dispatch.stamp_ms();
        let receipt = SignReceipt {
            stamp_ms: Some(stamp),
            expiry_ms: None,
        };
        receipt.validate()?;
        let mut operations = vec![
            Operation::Present {
                key: self.identity.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: self.identity.signing.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: format!(
                        "{stamp:.0}{}{}{}",
                        self.method.as_str(),
                        self.path,
                        self.body.as_deref().unwrap_or("")
                    )
                    .into_bytes(),
                }],
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: self.identity.signing.clone(),
                payload: vec![PayloadPart::Credential {
                    key: passphrase.clone(),
                }],
            },
        ];
        if let Some(broker) = &self.broker {
            if broker.partner.contains(['\r', '\n']) || broker.name.contains(['\r', '\n']) {
                return Err(SignError::InvalidRequest);
            }
            operations.push(Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: self.identity.extra[&ExtraKey::KucoinBroker].clone(),
                payload: vec![
                    PayloadPart::Bytes {
                        bytes: format!("{stamp:.0}{}", broker.partner).into_bytes(),
                    },
                    PayloadPart::Credential {
                        key: self.identity.credential.clone(),
                    },
                ],
            });
        }
        self.receipt = Some(receipt);
        Ok(SignRequest {
            receipt,
            context: Context {
                venue: if self.origin == "https://api.kucoin.com" {
                    "kucoinspot"
                } else {
                    "kucoin"
                }
                .into(),
                method: self.method.as_str().into(),
                host: self
                    .origin
                    .strip_prefix("https://")
                    .ok_or(SignError::InvalidRequest)?
                    .into(),
                path: self.path.clone(),
            },
            operations,
        })
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let receipt = self.receipt.take().ok_or(SignError::InvalidRequest)?;
        if result.receipt != receipt
            || !result.headers.is_empty()
            || result.values.len() != if self.broker.is_some() { 4 } else { 3 }
            || result.values[1..].iter().any(|value| value.len() != 32)
        {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0])
            .ok()
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let mac = |index| {
            safe_cex_crypto::encode(&result.values[index], safe_cex_crypto::Encoding::Base64)
        };
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.headers = vec![
            ("KC-API-KEY", key.into()),
            (
                "KC-API-TIMESTAMP",
                format!("{:.0}", receipt.stamp_ms.ok_or(SignError::InvalidResult)?),
            ),
            ("KC-API-KEY-VERSION", "2".into()),
            ("KC-API-SIGN", mac(1)),
            ("KC-API-PASSPHRASE", mac(2)),
            ("Content-Type", "application/json".into()),
            ("X-CHANNEL-API-CODE", "3zexp".into()),
        ];
        if let Some(broker) = &self.broker {
            http.headers.extend([
                ("KC-API-PARTNER", broker.partner.clone()),
                ("KC-API-PARTNER-SIGN", mac(3)),
                ("KC-BROKER-NAME", broker.name.clone()),
                ("KC-API-PARTNER-VERIFY", broker.verify.to_string()),
            ]);
        }
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
