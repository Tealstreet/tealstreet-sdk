//! Exact native HMAC intent; only the provider presents credentials and signs.
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, SocketId};

#[derive(Clone, Copy)]
struct Prepared {
    expiry: u64,
    receipt: SignReceipt,
}
impl Prepared {
    fn at(dispatch: &DispatchContext<'_>) -> Result<Self, SignError> {
        let expiry = super::expiry(dispatch.now_ms);
        let receipt = SignReceipt {
            nonce: None,
            stamp_ms: None,
            expiry_ms: Some(expiry as f64 * 1000.),
        };
        receipt.validate()?;
        Ok(Self { expiry, receipt })
    }
    fn values<'a>(&self, result: &'a SignResult) -> Result<(&'a str, String), SignError> {
        if result.receipt != self.receipt
            || result.values.len() != 2
            || result.values[1].len() != 32
        {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0])
            .ok()
            .filter(|key| {
                !key.is_empty()
                    && key.len() <= safe_cex_engine::signing_headers::MAX_VALUE_BYTES
                    && !key.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
            })
            .ok_or(SignError::InvalidResult)?;
        Ok((key, safe_cex_crypto::hex(&result.values[1])))
    }
}

pub struct Http {
    identity: Identity,
    origin: &'static str,
    method: Method,
    path: String,
    query: String,
    body: Option<String>,
    prepared: Option<Prepared>,
}
impl Http {
    pub fn new(
        identity: Identity,
        testnet: bool,
        method: Method,
        path: String,
        query: String,
        body: Option<String>,
    ) -> Self {
        Self {
            identity,
            origin: if testnet {
                "https://testnet-api.phemex.com"
            } else {
                "https://api.phemex.com"
            },
            method,
            path,
            query,
            body,
            prepared: None,
        }
    }
    pub fn unsigned(&self) -> (String, Vec<(&'static str, String)>, Option<String>) {
        (
            format!(
                "{}{}{}{}",
                self.origin,
                self.path,
                if self.query.is_empty() { "" } else { "?" },
                self.query
            ),
            Vec::new(),
            self.body.clone(),
        )
    }
    fn matches(&self, outbound: &Action) -> bool {
        let Action::Http(http) = outbound else {
            return false;
        };
        let (url, headers, body) = self.unsigned();
        http.method == self.method
            && http.url == url
            && http.headers == headers
            && http.body == body
    }
}
impl SignStep for Http {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let prepared = Prepared::at(dispatch)?;
        let payload = format!(
            "{}{}{}{}",
            self.path,
            super::decoded_query(&self.query),
            prepared.expiry,
            self.body.as_deref().unwrap_or("")
        );
        let work = hmac_sha256_work(
            &self.identity,
            vec![PayloadPart::Bytes {
                bytes: payload.into_bytes(),
            }],
            Context {
                venue: "phemex".into(),
                method: self.method.as_str().into(),
                host: self.origin.trim_start_matches("https://").into(),
                path: self.path.clone(),
            },
            prepared.receipt,
        );
        self.prepared = Some(prepared);
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let prepared = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = prepared.values(result)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.headers = vec![
            ("X-PHEMEX-ACCESS-TOKEN", key.into()),
            ("X-PHEMEX-REQUEST-EXPIRY", prepared.expiry.to_string()),
            ("X-PHEMEX-REQUEST-SIGNATURE", signature),
            ("Content-Type", "application/json".into()),
        ];
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}

pub struct WsAuth {
    identity: Identity,
    socket: SocketId,
    id: u64,
    prepared: Option<Prepared>,
}
impl WsAuth {
    pub fn new(identity: Identity, socket: SocketId, id: u64) -> Self {
        Self {
            identity,
            socket,
            id,
            prepared: None,
        }
    }
    pub fn unsigned(&self) -> String {
        format!(r#"{{"method":"user.auth","params":[],"id":{}}}"#, self.id)
    }
    fn matches(&self, outbound: &Action) -> bool {
        matches!(outbound,Action::WsSend{socket,text} if *socket==self.socket && *text==self.unsigned())
    }
}
impl SignStep for WsAuth {
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let prepared = Prepared::at(dispatch)?;
        let work = hmac_sha256_work(
            &self.identity,
            vec![
                PayloadPart::Credential {
                    key: self.identity.credential.clone(),
                },
                PayloadPart::Bytes {
                    bytes: prepared.expiry.to_string().into_bytes(),
                },
            ],
            Context {
                venue: "phemex".into(),
                method: "WS".into(),
                host: String::new(),
                path: "user.auth".into(),
            },
            prepared.receipt,
        );
        self.prepared = Some(prepared);
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if !self.matches(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let prepared = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = prepared.values(result)?;
        #[derive(serde::Serialize)]
        struct Auth<'a> {
            method: &'static str,
            params: (&'static str, &'a str, String, u64),
            id: u64,
        }
        let text = serde_json::to_string(&Auth {
            method: "user.auth",
            params: ("API", key, signature, prepared.expiry),
            id: self.id,
        })
        .map_err(|_| SignError::InvalidResult)?;
        let Action::WsSend { text: out, .. } = outbound else {
            return Err(SignError::InvalidRequest);
        };
        *out = text;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
