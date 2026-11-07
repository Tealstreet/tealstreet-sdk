//! Atomic Kraken recipes. Native callers and socket authority are adopted separately.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_engine::credential_selection::{CredentialRole, Selection};
use safe_cex_engine::request_context::Credential;
use safe_cex_engine::signing::{
    Algorithm, Context, Encoding, Identities, Identity, KeyRef, Operation, PayloadPart, SignError,
    SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::{Action, CredentialSlot, DispatchContext, HttpRequest, Method, SignStep};

const FORM: &str = "application/x-www-form-urlencoded";
const FUTURES_HEADERS: &[(&str, &str)] = &[("Content-Type", FORM)];
const SPOT_HEADERS: &[(&str, &str)] = &[("Content-Type", FORM), ("Accept", "application/json")];

/// One selection and its non-Clone nonce authority, captured before command admission.
pub struct Selected {
    selection: Selection,
    identity: Identity,
    claim: safe_cex_engine::nonce::NonceClaim,
    spot: bool,
}
impl Selected {
    pub fn futures(
        ids: &Identities,
        role: CredentialRole,
    ) -> Result<Self, safe_cex_engine::nonce::NonceSelectionError> {
        let slots = super::setup::futures_slots(ids);
        let (selection, identity, claim) =
            slots.select_with_nonce(role, &safe_cex_engine::nonce::KRAKEN)?;
        Ok(Self {
            selection,
            identity: identity.clone(),
            claim,
            spot: false,
        })
    }
    pub fn spot(
        ids: &Identities,
        role: CredentialRole,
    ) -> Result<Self, safe_cex_engine::nonce::NonceSelectionError> {
        let slots = super::setup::spot_slots(ids);
        let (selection, identity, claim) =
            slots.select_with_nonce(role, &safe_cex_engine::nonce::KRAKEN)?;
        Ok(Self {
            selection,
            identity: identity.clone(),
            claim,
            spot: true,
        })
    }
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn credential(&self) -> Credential {
        self.selection.credential(self.identity.credential.clone())
    }
    pub fn futures_http(
        self,
        method: Method,
        path: &str,
        form: &str,
        testnet: bool,
        freshness_ms: f64,
    ) -> Result<Http, SignError> {
        if self.spot {
            return Err(SignError::InvalidRequest);
        }
        Http::futures(
            &self.identity,
            self.claim,
            method,
            path,
            form,
            testnet,
            freshness_ms,
        )
    }
    pub fn spot_http(self, path: &str, form: &str, freshness_ms: f64) -> Result<Http, SignError> {
        if !self.spot {
            return Err(SignError::InvalidRequest);
        }
        Http::spot(&self.identity, self.claim, path, form, freshness_ms)
    }
}

pub struct Http {
    identity: Identity,
    claim: safe_cex_engine::nonce::NonceClaim,
    method: Method,
    url: String,
    body: Option<String>,
    path: String,
    form: String,
    spot: bool,
    freshness_ms: f64,
    prepared: Option<(SignReceipt, Option<String>)>,
}
impl Http {
    /// Consume the claim minted by a retained read queue or a selected direct request.
    pub fn futures(
        identity: &Identity,
        claim: safe_cex_engine::nonce::NonceClaim,
        method: Method,
        path: &str,
        form: &str,
        testnet: bool,
        freshness_ms: f64,
    ) -> Result<Self, SignError> {
        let canonical = path.strip_prefix("/derivatives").unwrap_or(path);
        if !(canonical.starts_with("/api/v3/") || canonical.starts_with("/api/history/"))
            || path.contains('?')
            || (claim.slot() == CredentialSlot::Secondary
                && (method != Method::Get || canonical.starts_with("/api/history/")))
        {
            return Err(SignError::InvalidRequest);
        }
        let origin = if testnet {
            "https://demo-futures.kraken.com"
        } else {
            "https://futures.kraken.com"
        };
        let query = if method == Method::Get && !form.is_empty() {
            format!("?{form}")
        } else {
            String::new()
        };
        let url = format!("{origin}{path}{query}");
        let body = (method != Method::Get).then(|| form.to_owned());
        Self::new(
            identity,
            claim,
            method,
            url,
            body,
            path,
            form,
            false,
            freshness_ms,
        )
    }
    /// Spot never accepts a Secondary claim, including one minted outside its factory.
    pub fn spot(
        identity: &Identity,
        claim: safe_cex_engine::nonce::NonceClaim,
        path: &str,
        form: &str,
        freshness_ms: f64,
    ) -> Result<Self, SignError> {
        if claim.slot() != CredentialSlot::Primary
            || !path.starts_with("/0/private/")
            || path.contains('?')
        {
            return Err(SignError::InvalidRequest);
        }
        Self::new(
            identity,
            claim,
            Method::Post,
            format!("https://api.kraken.com{path}"),
            Some(form.into()),
            path,
            form,
            true,
            freshness_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        identity: &Identity,
        claim: safe_cex_engine::nonce::NonceClaim,
        method: Method,
        url: String,
        body: Option<String>,
        path: &str,
        form: &str,
        spot: bool,
        freshness_ms: f64,
    ) -> Result<Self, SignError> {
        identity.validate().map_err(|_| SignError::InvalidRequest)?;
        if !freshness_ms.is_finite() || freshness_ms <= 0. {
            return Err(SignError::InvalidRequest);
        }
        Ok(Self {
            identity: identity.clone(),
            claim,
            method,
            url,
            body,
            path: path.into(),
            form: form.into(),
            spot,
            freshness_ms,
            prepared: None,
        })
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }
    fn work(&self, nonce: u64, stamp_ms: f64) -> Result<(Option<String>, SignRequest), SignError> {
        let context = Context {
            venue: if self.spot { "krakenspot" } else { "kraken" }.into(),
            method: self.method.as_str().into(),
            host: self
                .url
                .strip_prefix("https://")
                .and_then(|url| url.split('/').next())
                .ok_or(SignError::InvalidRequest)?
                .into(),
            path: self.path.clone(),
        };
        build_http_work(
            &self.identity,
            context,
            &self.form,
            self.spot,
            SignReceipt {
                stamp_ms: Some(stamp_ms),
                nonce: Some(nonce),
                ..Default::default()
            },
        )
    }

    fn attest(&self, request: &HttpRequest, body: Option<&str>) -> Result<(), SignError> {
        if request.method != self.method
            || request.url != self.url
            || request.body.as_deref() != body
            || !request.signer_headers.is_empty()
            || !request
                .headers
                .iter()
                .map(|(k, v)| (*k, v.as_str()))
                .eq(self.unsigned_http_headers().iter().copied())
        {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
}
impl SignStep for Http {
    fn nonce_claim(&self) -> Option<&safe_cex_engine::nonce::NonceClaim> {
        Some(&self.claim)
    }
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        if self.spot {
            SPOT_HEADERS
        } else {
            FUTURES_HEADERS
        }
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        self.attest(request, self.body.as_deref())?;
        let nonce = dispatch
            .reserve_nonce(&self.claim)
            .map_err(|error| match error {
                safe_cex_engine::nonce::NonceError::InvalidTime
                | safe_cex_engine::nonce::NonceError::InvalidClaim => SignError::InvalidRequest,
                _ => SignError::Unavailable,
            })?;
        let (body, work) = self.work(nonce, dispatch.stamp_ms())?;
        let receipt = work.receipt;
        request.body = body.clone();
        self.prepared = Some((receipt, body));
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(request) = action else {
            return Err(SignError::InvalidRequest);
        };
        let (receipt, body) = self.prepared.as_ref().ok_or(SignError::InvalidResult)?;
        self.attest(request, body.as_deref())?;
        if result.receipt != *receipt
            || !result.headers.is_empty()
            || result.values.len() != 3
            || result.values[1].len() != 32
            || result.values[2].len() != 64
        {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
        if key.is_empty() || key.contains(['\r', '\n']) {
            return Err(SignError::InvalidResult);
        }
        let signature = STANDARD.encode(&result.values[2]);
        request.headers = if self.spot {
            vec![
                ("API-Key", key.into()),
                ("API-Sign", signature),
                ("Content-Type", FORM.into()),
                ("Accept", "application/json".into()),
            ]
        } else {
            vec![
                ("APIKey", key.into()),
                ("Authent", signature),
                (
                    "Nonce",
                    receipt.nonce.ok_or(SignError::InvalidResult)?.to_string(),
                ),
                ("Content-Type", FORM.into()),
            ]
        };
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        self.freshness_ms
    }
}
pub(super) fn build_http_work(
    identity: &Identity,
    context: Context,
    form: &str,
    spot: bool,
    receipt: SignReceipt,
) -> Result<(Option<String>, SignRequest), SignError> {
    let nonce_text = receipt.nonce.ok_or(SignError::InvalidRequest)?.to_string();
    let body = if spot {
        Some(format!(
            "{}{}nonce={nonce_text}",
            form,
            if form.is_empty() { "" } else { "&" }
        ))
    } else {
        (context.method != "GET").then(|| form.to_owned())
    };
    let input = if spot {
        format!("{nonce_text}{}", body.as_deref().unwrap())
    } else {
        format!(
            "{form}{nonce_text}{}",
            context
                .path
                .strip_prefix("/derivatives")
                .unwrap_or(&context.path)
        )
    };
    let mut work = hash_work(
        &identity.credential,
        identity.signing_ref()?,
        input.into_bytes(),
        spot.then_some(context.path.as_bytes()),
        receipt,
    );
    work.context = context;
    Ok((body, work))
}

fn hash_work(
    credential: &KeyRef,
    signing: &KeyRef,
    input: Vec<u8>,
    prefix: Option<&[u8]>,
    receipt: SignReceipt,
) -> SignRequest {
    let mut payload = Vec::new();
    if let Some(path) = prefix {
        payload.push(PayloadPart::Bytes {
            bytes: path.to_vec(),
        });
    }
    payload.push(PayloadPart::Result {
        index: 1,
        encoding: Encoding::Raw,
    });
    SignRequest {
        receipt,
        operations: vec![
            Operation::Present {
                key: credential.clone(),
            },
            Operation::Sha256 {
                payload: vec![PayloadPart::Bytes { bytes: input }],
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key: signing.clone(),
                payload,
            },
        ],
        context: Context::default(),
    }
}
/// Pure atomic work only; the socket owner binds challenge and physical epoch.
pub fn challenge_work(ids: &Identities, challenge: &str) -> Result<SignRequest, SignError> {
    if challenge.is_empty() {
        return Err(SignError::InvalidRequest);
    }
    let slots = super::setup::futures_slots(ids);
    let (_, identity) = slots
        .select(CredentialRole::SessionAuth)
        .map_err(|_| SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    Ok(hash_work(
        &identity.credential,
        identity.signing_ref()?,
        challenge.as_bytes().to_vec(),
        None,
        SignReceipt::default(),
    ))
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
