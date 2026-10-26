//! Exact native HTTP bytes; the provider alone presents and signs material.
use safe_cex_engine::signing::{
    Context, Identity, PayloadPart, SignError, SignReceipt, SignRequest, SignResult,
    hmac_sha256_work,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep, StampFloor};

pub const HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("X-Referer", "Tealstreet"),
];

pub struct Http {
    identity: Identity,
    method: Method,
    url: String,
    body: Option<String>,
    window: u32,
    floor: Option<StampFloor>,
    receipt: Option<SignReceipt>,
}

impl Http {
    pub fn new(
        identity: Identity,
        method: Method,
        url: String,
        body: Option<String>,
        window: u32,
        floor: Option<StampFloor>,
    ) -> Self {
        Self {
            identity,
            method,
            url,
            body,
            window,
            floor,
            receipt: None,
        }
    }

    fn attest(&self, outbound: &Action) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        if http.method != self.method
            || http.url != self.url
            || http.body != self.body
            || !http.signer_headers.is_empty()
            || http.headers.len() != HEADERS.len()
            || http
                .headers
                .iter()
                .zip(HEADERS)
                .any(|((name, value), (n, v))| name != n || value != v)
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
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.receipt = None;
        self.attest(outbound)?;
        self.identity
            .validate()
            .map_err(|_| SignError::InvalidRequest)?;
        let (host, path) = ["fapi.bitrue.com", "fapiws-auth.bitrue.com"]
            .into_iter()
            .find_map(|host| {
                self.url
                    .strip_prefix(&format!("https://{host}/"))
                    .map(|_| (host, &self.url[8 + host.len()..]))
            })
            .ok_or(SignError::InvalidRequest)?;
        let stamp = dispatch.stamp_after(self.floor)?;
        let receipt = SignReceipt {
            nonce: None,
            stamp_ms: Some(stamp),
            expiry_ms: None,
        };
        let work = hmac_sha256_work(
            &self.identity.credential,
            self.identity.signing_ref()?,
            vec![PayloadPart::Bytes {
                bytes: format!(
                    "{stamp:.0}{}{path}{}",
                    self.method.as_str(),
                    self.body.as_deref().unwrap_or("")
                )
                .into_bytes(),
            }],
            Context {
                venue: "bitrue".into(),
                method: self.method.as_str().into(),
                host: host.into(),
                path: path.into(),
            },
            receipt,
        );
        self.receipt = Some(receipt);
        Ok(work)
    }

    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        self.attest(outbound)?;
        let receipt = self.receipt.take().ok_or(SignError::InvalidRequest)?;
        if result.receipt != receipt
            || result.values.len() != 2
            || result.values[1].len() != 32
            || !result.headers.is_empty()
        {
            return Err(SignError::InvalidResult);
        }
        let key = std::str::from_utf8(&result.values[0])
            .ok()
            .filter(|s| !s.is_empty() && !s.contains(['\r', '\n']))
            .ok_or(SignError::InvalidResult)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.headers.extend([
            ("X-CH-APIKEY", key.to_owned()),
            (
                "X-CH-TS",
                format!("{:.0}", receipt.stamp_ms.ok_or(SignError::InvalidResult)?),
            ),
            ("recvWindow", self.window.to_string()),
            (
                "X-CH-SIGN",
                safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Hex),
            ),
        ]);
        Ok(())
    }

    fn freshness_ms(&self) -> f64 {
        5_000.
    }
}
