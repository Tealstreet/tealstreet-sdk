//! Immutable MEXC intent; credential interpolation and HMAC remain provider-owned.
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, SocketId, num};

const HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];

pub struct Http {
    identity: Identity,
    method: Method,
    path: String,
    payload: String,
    window: u32,
    stamp: Option<f64>,
}
impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        path: String,
        payload: String,
        window: Option<u32>,
    ) -> Self {
        Self {
            identity,
            method,
            path,
            payload,
            window: window.unwrap_or(20_000).clamp(1, 20_000),
            stamp: None,
        }
    }
    pub fn url(&self) -> String {
        if self.method == Method::Get && !self.payload.is_empty() {
            format!("{}{}?{}", crate::REST, self.path, self.payload)
        } else {
            format!("{}{}", crate::REST, self.path)
        }
    }
    pub fn body(&self) -> Option<String> {
        (self.method != Method::Get).then(|| self.payload.clone())
    }
    fn attest(&self, action: &Action) -> Result<(), SignError> {
        let Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        if http.method != self.method
            || http.url != self.url()
            || http.body != self.body()
            || http.headers != vec![("Content-Type", "application/json".into())]
            || !http.signer_headers.is_empty()
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}
impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        HEADERS
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp = None;
        self.attest(action)?;
        let stamp = dispatch.stamp_ms();
        let request = work(
            &self.identity,
            stamp,
            &self.payload,
            Context {
                venue: "mexc".into(),
                method: self.method.as_str().into(),
                host: "api.mexc.com".into(),
                path: self.path.clone(),
            },
        )?;
        self.stamp = Some(stamp);
        Ok(request)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(action)?;
        let stamp = self.stamp.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = values(result, stamp)?;
        let Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.headers = vec![
            ("ApiKey", key.into()),
            ("Request-Time", num::format(stamp)),
            ("Signature", signature),
            ("Recv-Window", self.window.to_string()),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        f64::from(self.window)
    }
}

pub struct Login {
    identity: Identity,
    socket: SocketId,
    stamp: Option<f64>,
}
impl Login {
    pub fn new(identity: Identity, socket: SocketId) -> Self {
        Self {
            identity,
            socket,
            stamp: None,
        }
    }
    fn attest(&self, action: &Action) -> Result<(), SignError> {
        if matches!(action, Action::WsSend { socket, text } if *socket == self.socket && text.is_empty())
        {
            Ok(())
        } else {
            Err(SignError::InvalidRequest)
        }
    }
}
impl SignStep for Login {
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.stamp = None;
        self.attest(action)?;
        let stamp = dispatch.stamp_ms();
        let request = work(
            &self.identity,
            stamp,
            "",
            Context {
                venue: "mexc".into(),
                method: "WS".into(),
                host: "contract.mexc.com".into(),
                path: "/edge".into(),
            },
        )?;
        self.stamp = Some(stamp);
        Ok(request)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(action)?;
        let stamp = self.stamp.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = values(result, stamp)?;
        #[derive(serde::Serialize)]
        struct Param<'a> {
            #[serde(rename = "apiKey")]
            key: &'a str,
            #[serde(rename = "reqTime")]
            time: String,
            signature: String,
        }
        #[derive(serde::Serialize)]
        struct Login<'a> {
            method: &'static str,
            param: Param<'a>,
        }
        let Action::WsSend { text, .. } = action else {
            return Err(SignError::InvalidRequest);
        };
        *text = serde_json::to_string(&Login {
            method: "login",
            param: Param {
                key,
                time: num::format(stamp),
                signature,
            },
        })
        .map_err(|_| SignError::InvalidResult)?;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}
fn work(
    identity: &Identity,
    stamp: f64,
    payload: &str,
    context: Context,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let receipt = SignReceipt {
        stamp_ms: Some(stamp),
        expiry_ms: None,
        nonce: None,
    };
    receipt.validate()?;
    Ok(hmac_sha256_work(
        &identity.credential,
        identity.signing_ref()?,
        vec![
            PayloadPart::Credential {
                key: identity.credential.clone(),
            },
            PayloadPart::Bytes {
                bytes: format!("{}{payload}", num::format(stamp)).into_bytes(),
            },
        ],
        context,
        receipt,
    ))
}
fn values(result: &SignResult, stamp: f64) -> Result<(&str, String), SignError> {
    if result.values.len() != 2
        || result.values[1].len() != 32
        || !result.headers.is_empty()
        || result.receipt
            != (SignReceipt {
                stamp_ms: Some(stamp),
                expiry_ms: None,
                nonce: None,
            })
    {
        return Err(SignError::InvalidResult);
    }
    let key = std::str::from_utf8(&result.values[0])
        .ok()
        .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
        .ok_or(SignError::InvalidResult)?;
    Ok((
        key,
        safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex),
    ))
}
