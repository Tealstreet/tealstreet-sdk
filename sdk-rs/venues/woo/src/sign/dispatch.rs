//! Dispatch-time Woo wire recipes; only the provider holds signing material.
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, SocketId};

fn work(
    identity: &Identity,
    payload: String,
    context: Context,
    stamp: f64,
) -> Result<SignRequest, SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    Ok(hmac_sha256_work(
        &identity.credential,
        identity.signing_ref()?,
        vec![PayloadPart::Bytes {
            bytes: payload.into_bytes(),
        }],
        context,
        SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp),
            expiry_ms: None,
        },
    ))
}
fn values(result: &SignResult, receipt: SignReceipt) -> Result<(&str, String), SignError> {
    if result.receipt != receipt || result.values.len() != 2 || result.values[1].len() != 32 {
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

pub struct Http {
    identity: Identity,
    origin: &'static str,
    method: Method,
    path: String,
    params: Vec<(String, String)>,
    body: Option<String>,
    prepared: Option<(SignReceipt, super::Signed)>,
}
impl Http {
    pub fn new(
        identity: Identity,
        origin: &'static str,
        method: Method,
        path: String,
        params: Vec<(String, String)>,
        body: Option<String>,
    ) -> Self {
        Self {
            identity,
            origin,
            method,
            path,
            params,
            body,
            prepared: None,
        }
    }
    pub fn unsigned(&self) -> (String, Vec<(&'static str, String)>, Option<String>) {
        let (wire, _) = super::prepare(
            "",
            0.,
            self.method,
            &self.path,
            &self.params,
            self.body.clone(),
        );
        (
            format!("{}{}", self.origin, wire.path),
            Vec::new(),
            wire.body,
        )
    }
    fn matches(&self, action: &Action) -> bool {
        let Action::Http(http) = action else {
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
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        if !self.matches(action) {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let (wire, payload) = super::prepare(
            "",
            stamp,
            self.method,
            &self.path,
            &self.params,
            self.body.clone(),
        );
        let work = work(
            &self.identity,
            payload,
            Context {
                venue: "woo".into(),
                method: self.method.as_str().into(),
                host: self.origin.trim_start_matches("https://").into(),
                path: self.path.clone(),
            },
            stamp,
        )?;
        self.prepared = Some((work.receipt, wire));
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if !self.matches(action) {
            return Err(SignError::InvalidRequest);
        }
        let (receipt, mut wire) = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = values(result, receipt)?;
        wire.headers[0].1 = key.into();
        wire.headers[2].1 = signature;
        let Action::Http(http) = action else {
            return Err(SignError::InvalidRequest);
        };
        http.headers = wire.headers;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        2_000.
    }
}

pub struct WsAuth {
    identity: Identity,
    socket: SocketId,
    prepared: Option<SignReceipt>,
}
impl WsAuth {
    pub fn new(identity: Identity, socket: SocketId) -> Self {
        Self {
            identity,
            socket,
            prepared: None,
        }
    }
    fn matches(&self, action: &Action) -> bool {
        matches!(action, Action::WsSend { socket, text } if *socket == self.socket && text.is_empty())
    }
}
impl SignStep for WsAuth {
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        if !self.matches(action) {
            return Err(SignError::InvalidRequest);
        }
        let stamp = dispatch.stamp_ms();
        let work = work(
            &self.identity,
            format!("|{}", safe_cex_engine::num::format(stamp)),
            Context {
                venue: "woo".into(),
                method: "WS".into(),
                host: String::new(),
                path: "auth".into(),
            },
            stamp,
        )?;
        self.prepared = Some(work.receipt);
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        if !self.matches(action) {
            return Err(SignError::InvalidRequest);
        }
        let receipt = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let (key, signature) = values(result, receipt)?;
        let text = super::websocket_text(
            key,
            signature,
            receipt.stamp_ms.ok_or(SignError::InvalidResult)?,
        );
        let Action::WsSend { text: output, .. } = action else {
            return Err(SignError::InvalidRequest);
        };
        *output = text;
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        2_000.
    }
}
