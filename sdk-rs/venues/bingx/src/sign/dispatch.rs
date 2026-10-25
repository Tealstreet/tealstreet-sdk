//! Native canonical work. Only the trusted provider presents keys or signs.
use super::{Params, RequestParts, signs};
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

/// Immutable native intent plus opaque provider references; no material custody.
pub struct Http {
    identity: Identity,
    method: Method,
    path: &'static str,
    params: Params,
    prepared: Option<Params>,
}
impl Http {
    pub fn new(identity: Identity, method: Method, path: &'static str, params: Params) -> Self {
        Self {
            identity,
            method,
            path,
            params,
            prepared: None,
        }
    }

    /// The unsigned transport intent used when staging this exact recipe.
    pub fn unsigned(&self) -> RequestParts {
        unsigned(self.method, self.path, &self.params)
    }
}

/// The unsigned transport intent for a request, before any identity signs it.
pub fn unsigned(method: Method, path: &'static str, params: &Params) -> RequestParts {
    params
        .clone()
        .render(method, path, vec![("X-SOURCE-KEY", "Tealstreet".into())])
}

impl SignStep for Http {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        super::unsigned_headers(self.method)
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let (url, headers, body) = self.unsigned();
        if http.method != self.method
            || http.url != url
            || http.headers != headers
            || http.body != body
        {
            return Err(SignError::InvalidRequest);
        }
        let context = Context {
            venue: "bingx".into(),
            method: self.method.as_str().into(),
            host: "open-api.bingx.com".into(),
            path: self.path.into(),
        };
        if signs(self.path) {
            let stamp = dispatch.stamp_ms();
            let params = self.params.clone().num("timestamp", stamp);
            let work = hmac_sha256_work(
                &self.identity,
                vec![PayloadPart::Bytes {
                    bytes: params.query().into_bytes(),
                }],
                context,
                SignReceipt {
                    stamp_ms: Some(stamp),
                    expiry_ms: None,
                },
            )?;
            self.prepared = Some(params);
            Ok(work)
        } else {
            self.prepared = Some(self.params.clone());
            Ok(SignRequest {
                receipt: SignReceipt::default(),
                operations: vec![Operation::Present {
                    key: self.identity.credential.clone(),
                }],
                context,
            })
        }
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let expected = if signs(self.path) { 2 } else { 1 };
        if result.values.len() != expected {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0])
            .ok()
            .filter(|key| !key.is_empty() && !key.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let mut params = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        if signs(self.path) {
            if result.values[1].len() != 32 {
                return Err(SignError::InvalidResult);
            }
            let signature =
                safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex);
            params = params.str("signature", &signature);
        }
        let (url, headers, body) = params.render(
            self.method,
            self.path,
            vec![
                ("X-SOURCE-KEY", "Tealstreet".into()),
                ("X-BX-APIKEY", key.into()),
            ],
        );
        http.url = url;
        http.headers = headers;
        http.body = body;
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
