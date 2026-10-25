//! Dispatch-owned Crypto.com envelopes with exact request and receipt binding.
use super::{Params, envelope_json};
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, SignStep};

/// One signed JSON envelope: REST body or the private WS `public/auth` frame.
/// The nonce doubles as the envelope id and is drawn at each preparation.
pub struct Envelope {
    credentials: Identity,
    method: String,
    /// `None` for `public/auth`, whose envelope carries no params.
    params: Option<Params>,
    context: Context,
    unsigned: Option<Action>,
    prepared: Option<SignRequest>,
    url: String,
}
impl Envelope {
    pub fn http(credentials: Identity, method: &str, params: Params, url: &str) -> Self {
        Self::new(credentials, method, Some(params), url, "POST")
    }
    pub fn ws_auth(credentials: Identity, url: &str) -> Self {
        Self::new(credentials, "public/auth", None, url, "WS")
    }
    fn new(
        credentials: Identity,
        method: &str,
        params: Option<Params>,
        url: &str,
        verb: &str,
    ) -> Self {
        let (host, path) = url
            .split_once("://")
            .map(|(_, rest)| rest.split_once('/').unwrap_or((rest, "")))
            .unwrap_or(("", ""));
        Self {
            credentials,
            method: method.into(),
            params,
            context: Context {
                venue: "cryptocom".into(),
                method: verb.into(),
                host: host.into(),
                path: format!("/{path}"),
            },
            unsigned: None,
            prepared: None,
            url: url.into(),
        }
    }
}
impl Envelope {
    /// REST recipes carry params; the WS auth recipe never does.
    fn transport(&self, outbound: &Action) -> Result<(), SignError> {
        match (outbound, self.params.is_some()) {
            (Action::Http(_), true) | (Action::WsSend { .. }, false) => Ok(()),
            _ => Err(SignError::InvalidRequest),
        }
    }
}
impl SignStep for Envelope {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.params.is_some() {
            &[("Content-Type", "application/json")]
        } else {
            &[]
        }
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.unsigned = None;
        self.transport(outbound)?;
        self.credentials
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        if let Action::Http(http) = outbound {
            if http.method != safe_cex_engine::Method::Post
                || http.url != self.url
                || http.body.is_some()
                || !http.signer_headers.is_empty()
                || (!http.headers.is_empty()
                    && http.headers != [("Content-Type", "application/json".into())])
            {
                return Err(SignError::InvalidRequest);
            }
            http.headers = vec![("Content-Type", "application/json".into())];
        } else if let Action::WsSend { text, .. } = outbound
            && !text.is_empty()
        {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let nonce = stamp as u64;
        let signing_text = self
            .params
            .as_ref()
            .map(Params::signing_text)
            .unwrap_or_default();
        let work = hmac_sha256_work(
            &self.credentials.credential,
            self.credentials.signing_ref()?,
            vec![
                PayloadPart::Bytes {
                    bytes: format!("{}{nonce}", self.method).into_bytes(),
                },
                PayloadPart::Credential {
                    key: self.credentials.credential.clone(),
                },
                PayloadPart::Bytes {
                    bytes: format!("{signing_text}{nonce}").into_bytes(),
                },
            ],
            self.context.clone(),
            SignReceipt {
                nonce: None,
                stamp_ms: Some(stamp),
                expiry_ms: None,
            },
        );
        work.validate()?;
        self.unsigned = Some(outbound.clone());
        self.prepared = Some(work.clone());
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.transport(outbound)?;
        let work = self.prepared.as_ref().ok_or(SignError::InvalidResult)?;
        work.validate_result(result)?;
        if self.unsigned.as_ref() != Some(outbound) {
            return Err(SignError::InvalidRequest);
        }
        let nonce = work.receipt.stamp_ms.ok_or(SignError::InvalidResult)? as u64;
        let key = result
            .values
            .first()
            .and_then(|key| std::str::from_utf8(key).ok())
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let mac = result
            .values
            .get(1)
            .filter(|mac| mac.len() == 32)
            .ok_or(SignError::InvalidResult)?;
        self.transport(outbound)?;
        let body = envelope_json(
            &self.method,
            key,
            self.params.as_ref(),
            &safe_cex_crypto::hex(mac),
            nonce,
        );
        match outbound {
            Action::Http(http) => http.body = Some(body),
            Action::WsSend { text, .. } => *text = body,
            _ => return Err(SignError::InvalidRequest),
        }
        self.prepared = None;
        self.unsigned = None;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
}
