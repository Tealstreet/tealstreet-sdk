//! Closed signing-only HTTP work. The supplied receipt owns the exact nonce.
use safe_cex_engine::signing::{Identity, SignError, SignReceipt, SignRequest};
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    pub(crate) kind: Kind,
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) form: String,
    pub(crate) nonce: String,
    pub(crate) stamp_ms: u64,
    pub(crate) testnet: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum Kind {
    #[serde(rename = "kraken.futures.http")]
    Futures,
    #[serde(rename = "kraken.spot.http")]
    Spot,
}
impl std::fmt::Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("KrakenDescriptor")
    }
}
impl Descriptor {
    pub fn from_json(json: &str) -> Result<Self, SignError> {
        if json.len() > 1_048_576 {
            return Err(SignError::InvalidRequest);
        }
        serde_json::from_str(json).map_err(|_| SignError::InvalidRequest)
    }
    fn check(&self, receipt: SignReceipt) -> Result<(), SignError> {
        receipt.validate()?;
        let nonce: u64 = self.nonce.parse().map_err(|_| SignError::InvalidRequest)?;
        if nonce.to_string() != self.nonce
            || receipt.nonce != Some(nonce)
            || receipt.stamp_ms != Some(self.stamp_ms as f64)
            || receipt.expiry_ms.is_some()
            || self.stamp_ms > 9_007_199_254_740_991
            || self.form.len() > 1_048_576
            || !self.allowed()
            || (self.kind == Kind::Spot && self.testnet)
        {
            return Err(SignError::InvalidRequest);
        }
        for pair in self.form.split('&').filter(|s| !s.is_empty()) {
            let (key, _) = pair.split_once('=').ok_or(SignError::InvalidRequest)?;
            // Decode keys only to exclude nonce aliases; keep the actual form bytes.
            let mut decoded = Vec::new();
            let mut bytes = key.as_bytes().iter().copied();
            while let Some(byte) = bytes.next() {
                decoded.push(match byte {
                    b'%' => {
                        let h = bytes.next().and_then(|b| (b as char).to_digit(16));
                        let l = bytes.next().and_then(|b| (b as char).to_digit(16));
                        ((h.ok_or(SignError::InvalidRequest)? << 4)
                            | l.ok_or(SignError::InvalidRequest)?) as u8
                    }
                    b'+' => b' ',
                    b => b,
                });
            }
            if decoded == b"nonce" {
                return Err(SignError::InvalidRequest);
            }
        }
        if self.form.bytes().any(|b| b < 32 || b == 127 || b >= 128) {
            return Err(SignError::InvalidRequest);
        }
        Ok(())
    }
    fn allowed(&self) -> bool {
        match (self.kind, self.method.as_str()) {
            (Kind::Futures, "GET") => matches!(
                self.path.as_str(),
                "/derivatives/api/v3/accounts"
                    | "/derivatives/api/v3/openorders"
                    | "/derivatives/api/v3/openpositions"
                    | "/derivatives/api/v3/leveragepreferences"
                    | "/derivatives/api/v3/fills"
                    | "/api/history/v3/account-log"
                    | "/api/history/v3/orders"
            ),
            (Kind::Futures, "POST") => matches!(
                self.path.as_str(),
                "/derivatives/api/v3/sendorder"
                    | "/derivatives/api/v3/cancelorder"
                    | "/derivatives/api/v3/batchorder"
                    | "/derivatives/api/v3/editorder"
            ),
            (Kind::Futures, "PUT") => self.path == "/derivatives/api/v3/leveragepreferences",
            (Kind::Spot, "POST") => matches!(
                self.path.as_str(),
                "/0/private/Balance"
                    | "/0/private/OpenOrders"
                    | "/0/private/GetWebSocketsToken"
                    | "/0/private/AddOrder"
                    | "/0/private/AmendOrder"
                    | "/0/private/CancelOrder"
            ),
            _ => false,
        }
    }
    pub fn authorize(
        &self,
        slot: safe_cex_engine::CredentialSlot,
        role: safe_cex_engine::credential_selection::CredentialRole,
    ) -> Result<(), SignError> {
        use safe_cex_engine::{CredentialSlot, credential_selection::CredentialRole};
        let read = self.kind == Kind::Futures && self.method == "GET"
            || self.kind == Kind::Spot
                && matches!(
                    self.path.as_str(),
                    "/0/private/Balance" | "/0/private/OpenOrders"
                );
        let history = self.path.starts_with("/api/history/");
        let token = self.kind == Kind::Spot && self.path == "/0/private/GetWebSocketsToken";
        let permitted = match role {
            CredentialRole::Read { .. } => read && !history,
            CredentialRole::PrimaryRead { .. } => read && slot == CredentialSlot::Primary,
            CredentialRole::Mutation => !read && !token && slot == CredentialSlot::Primary,
            CredentialRole::SessionAuth => token && slot == CredentialSlot::Primary,
        };
        if !permitted || (self.kind == Kind::Spot && slot != CredentialSlot::Primary) {
            return Err(SignError::Refused);
        }
        Ok(())
    }
    pub fn work(
        &self,
        identity: &Identity,
        receipt: SignReceipt,
    ) -> Result<SignRequest, SignError> {
        self.check(receipt)?;
        identity.validate().map_err(|_| SignError::InvalidRequest)?;
        let spot = self.kind == Kind::Spot;
        let context = safe_cex_engine::signing::Context {
            venue: if spot { "krakenspot" } else { "kraken" }.into(),
            method: self.method.clone(),
            host: if spot {
                "api.kraken.com"
            } else if self.testnet {
                "demo-futures.kraken.com"
            } else {
                "futures.kraken.com"
            }
            .into(),
            path: self.path.clone(),
        };
        super::dispatch::build_http_work(identity, context, &self.form, spot, receipt)
            .map(|(_, work)| work)
    }
    pub fn describe(work: &SignRequest) -> Result<Self, SignError> {
        use safe_cex_engine::signing::{Operation, PayloadPart};
        let [
            Operation::Present { key: credential },
            Operation::Sha256 { payload },
            Operation::Sign { key: signing, .. },
        ] = work.operations.as_slice()
        else {
            return Err(SignError::InvalidRequest);
        };
        let [PayloadPart::Bytes { bytes }] = payload.as_slice() else {
            return Err(SignError::InvalidRequest);
        };
        let input = std::str::from_utf8(bytes).map_err(|_| SignError::InvalidRequest)?;
        let nonce = work
            .receipt
            .nonce
            .ok_or(SignError::InvalidRequest)?
            .to_string();
        let spot = work.context.venue == "krakenspot";
        if !spot && work.context.venue != "kraken" {
            return Err(SignError::InvalidRequest);
        }
        let form = if spot {
            let body = input
                .strip_prefix(&nonce)
                .ok_or(SignError::InvalidRequest)?;
            if body == format!("nonce={nonce}") {
                ""
            } else {
                body.strip_suffix(&format!("&nonce={nonce}"))
                    .ok_or(SignError::InvalidRequest)?
            }
        } else {
            input
                .strip_suffix(&format!(
                    "{nonce}{}",
                    work.context
                        .path
                        .strip_prefix("/derivatives")
                        .unwrap_or(&work.context.path)
                ))
                .ok_or(SignError::InvalidRequest)?
        };
        let stamp = work.receipt.stamp_ms.ok_or(SignError::InvalidRequest)?;
        work.receipt.validate()?;
        let d = Self {
            kind: if spot { Kind::Spot } else { Kind::Futures },
            method: work.context.method.clone(),
            path: work.context.path.clone(),
            form: form.into(),
            nonce,
            stamp_ms: stamp as u64,
            testnet: work.context.host == "demo-futures.kraken.com",
        };
        let identity = Identity::new(credential.clone(), Some(signing.clone()));
        if d.work(&identity, work.receipt)? != *work {
            return Err(SignError::InvalidRequest);
        }
        Ok(d)
    }
}
#[cfg(test)]
#[path = "hosted_tests.rs"]
mod tests;

/// One trusted installation; refs never come from an operation or wire binding.
pub struct Executor {
    signer: safe_cex_crypto::signer::LocalSigner,
    identities: safe_cex_engine::signing::Identities,
    spot: bool,
    testnet: bool,
    credentials: [Option<String>; 2],
}
impl Executor {
    pub fn from_json(input: String, spot: bool, testnet: bool) -> Result<Self, SignError> {
        let policy = if spot {
            super::setup::spot_policy()
        } else {
            super::setup::futures_policy()
        };
        let (signer, identities) =
            safe_cex_crypto::signer::LocalSigner::from_json_with_policy(input, policy)?;
        if identities.primary.is_none() || spot && testnet {
            return Err(SignError::InvalidRequest);
        }
        use safe_cex_engine::signing::{Context, Operation, Signer};
        let mut credentials = [None, None];
        for (n, identity) in [identities.primary.as_ref(), identities.secondary.as_ref()]
            .into_iter()
            .enumerate()
        {
            if let Some(identity) = identity {
                let work = SignRequest {
                    receipt: SignReceipt::default(),
                    operations: vec![Operation::Present {
                        key: identity.credential.clone(),
                    }],
                    context: Context::default(),
                };
                let result = signer.sign_ready(&work).ok_or(SignError::Unavailable)??;
                let value = String::from_utf8(result.values[0].clone())
                    .map_err(|_| SignError::InvalidResult)?;
                if value.is_empty() || value.contains(['\r', '\n']) {
                    return Err(SignError::InvalidResult);
                }
                credentials[n] = Some(value);
            }
        }
        Ok(Self {
            signer,
            identities,
            spot,
            testnet,
            credentials,
        })
    }
    fn identity(&self, slot: safe_cex_engine::CredentialSlot) -> Result<&Identity, SignError> {
        match slot {
            safe_cex_engine::CredentialSlot::Primary => self.identities.primary.as_ref(),
            safe_cex_engine::CredentialSlot::Secondary if !self.spot => {
                self.identities.secondary.as_ref()
            }
            _ => None,
        }
        .ok_or(SignError::Unavailable)
    }
    pub fn binding(
        &self,
        slot: safe_cex_engine::CredentialSlot,
    ) -> Result<(String, String), SignError> {
        let identity = self.identity(slot)?;
        let subject = identity
            .stable_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or(SignError::InvalidRequest)?;
        let credential = self.credentials[match slot {
            safe_cex_engine::CredentialSlot::Primary => 0,
            safe_cex_engine::CredentialSlot::Secondary => 1,
        }]
        .as_ref()
        .ok_or(SignError::Unavailable)?
        .clone();
        Ok((subject.clone(), credential))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        d: &Descriptor,
        slot: safe_cex_engine::CredentialSlot,
        role: safe_cex_engine::credential_selection::CredentialRole,
        receipt: SignReceipt,
        now: f64,
        deadline: f64,
    ) -> Result<safe_cex_engine::signing::SignResult, SignError> {
        use safe_cex_engine::signing::Signer;
        d.validate(slot, role, receipt, now, deadline)?;
        if (d.kind == Kind::Spot) != self.spot || d.testnet != self.testnet {
            return Err(SignError::Refused);
        }
        let (_, credential) = self.binding(slot)?;
        let work = d.work(self.identity(slot)?, receipt)?;
        let result = self
            .signer
            .sign_ready(&work)
            .ok_or(SignError::Unavailable)??;
        work.validate_result(&result)?;
        d.verify(&result, &credential)?;
        Ok(result)
    }
}
impl Descriptor {
    pub fn validate(
        &self,
        slot: safe_cex_engine::CredentialSlot,
        role: safe_cex_engine::credential_selection::CredentialRole,
        receipt: SignReceipt,
        now: f64,
        deadline: f64,
    ) -> Result<(), SignError> {
        self.check(receipt)?;
        self.authorize(slot, role)?;
        if !now.is_finite()
            || !deadline.is_finite()
            || now < 0.
            || deadline < 0.
            || now > 9_007_199_254_740_991.
            || deadline > 9_007_199_254_740_991.
            || now >= deadline
            || deadline - now > 5000.
            || !(0. ..5000.).contains(&(now - self.stamp_ms as f64))
        {
            return Err(SignError::Refused);
        }
        Ok(())
    }
    pub fn verify(
        &self,
        result: &safe_cex_engine::signing::SignResult,
        credential: &str,
    ) -> Result<(), SignError> {
        use safe_cex_engine::signing::{KeyRef, Operation, PayloadPart};
        use sha2::Digest;
        let identity = Identity::new(KeyRef("credential".into()), Some(KeyRef("signing".into())));
        let work = self.work(&identity, result.receipt)?;
        work.validate_result(result)?;
        let Operation::Sha256 { payload } = &work.operations[1] else {
            unreachable!()
        };
        let PayloadPart::Bytes { bytes } = &payload[0] else {
            unreachable!()
        };
        if result.values[0] != credential.as_bytes()
            || result.values[1] != sha2::Sha256::digest(bytes)[..]
        {
            return Err(SignError::InvalidResult);
        }
        Ok(())
    }
}
