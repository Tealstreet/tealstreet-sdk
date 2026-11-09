//! Native canonical work. Only the trusted provider presents keys or signs.
use super::{Params, RequestParts, signs};
use safe_cex_engine::signing::{
    Context, Identity, Operation, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};

/// Immutable native intent plus opaque provider references; no material custody.
pub struct Http {
    venue: &'static str,
    identity: Identity,
    method: Method,
    path: &'static str,
    params: Params,
    prepared: Option<Params>,
}
impl Http {
    pub fn new(identity: Identity, method: Method, path: &'static str, params: Params) -> Self {
        Self {
            venue: "bingx",
            identity,
            method,
            path,
            params,
            prepared: None,
        }
    }

    /// Spot shares the wire/signing contract but retains its own provider scope.
    pub fn spot(identity: Identity, method: Method, path: &'static str, params: Params) -> Self {
        Self {
            venue: "bingxspot",
            ..Self::new(identity, method, path, params)
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
            venue: self.venue.into(),
            method: self.method.as_str().into(),
            host: "open-api.bingx.com".into(),
            path: self.path.into(),
        };
        if signs(self.path) {
            let stamp = dispatch.stamp_ms();
            let params = self.params.clone().num("timestamp", stamp);
            let work = hmac_sha256_work(
                &self.identity.credential,
                self.identity.signing_ref()?,
                vec![PayloadPart::Bytes {
                    bytes: params.query().into_bytes(),
                }],
                context,
                SignReceipt {
                    nonce: None,
                    stamp_ms: Some(stamp),
                    expiry_ms: None,
                },
            );
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
        let params = self.prepared.take().ok_or(SignError::InvalidRequest)?;
        let mut signed = None;
        if signs(self.path) {
            if result.values[1].len() != 32 {
                return Err(SignError::InvalidResult);
            }
            let signature =
                safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex);
            signed = Some(signature);
        }
        let (url, headers, body) = params.render(
            self.method,
            self.path,
            vec![
                ("X-SOURCE-KEY", "Tealstreet".into()),
                ("X-BX-APIKEY", key.into()),
            ],
        );
        // BingX takes the signature last, in the query and in a JSON body alike.
        let (url, body) = match signed {
            Some(signature) => super::with_signature_last(self.method, url, body, &signature),
            None => (url, body),
        };
        http.url = url;
        http.headers = headers;
        http.body = body;
        Ok(())
    }

    // BingX refuses a timestamp past its 5000 ms recvWindow (also the maximum): re-sign a held
    // request well before, so the network still has its share of the window.
    fn freshness_ms(&self) -> f64 {
        2_000.
    }
}
