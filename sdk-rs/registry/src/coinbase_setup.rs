//! Coinbase's existing CDP parser owns local keys; venue state gets public bindings.
use super::*;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, Identity, KeyRef, Operation, PayloadPart, Provider,
    SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_venue_coinbase::sign::{Credentials, JwtAlgorithm};
use zeroize::Zeroizing;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    primary: Option<Slot>,
    secondary: Option<Slot>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Slot {
    #[serde(default)]
    key: String,
    #[serde(default)]
    secret: String,
    stable_id: Option<String>,
    passphrase: Option<String>,
    secret_encoding: Option<String>,
    trim_credential: Option<bool>,
}
impl Drop for Slot {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.key.zeroize();
        self.secret.zeroize();
        if let Some(value) = &mut self.passphrase {
            value.zeroize();
        }
    }
}
struct Owned {
    slots: Vec<(Identity, Credentials)>,
}
impl Owned {
    fn run(&self, work: &SignRequest) -> Result<SignResult, SignError> {
        work.validate()?;
        let [
            Operation::Present { key },
            Operation::Sign {
                algorithm,
                key: signing,
                payload,
            },
        ] = work.operations.as_slice()
        else {
            return Err(SignError::InvalidRequest);
        };
        let (identity, credentials) = self
            .slots
            .iter()
            .find(|(id, _)| &id.credential == key)
            .ok_or(SignError::MissingKey)?;
        if identity.signing_ref()? != signing
            || *algorithm != algorithm_for(credentials.algorithm())
        {
            return Err(SignError::InvalidRequest);
        }
        let [PayloadPart::Bytes { bytes }] = payload.as_slice() else {
            return Err(SignError::InvalidRequest);
        };
        let input = std::str::from_utf8(bytes).map_err(|_| SignError::InvalidRequest)?;
        let (binding, _) = safe_cex_venue_coinbase::sign::inspect_input(input)
            .map_err(|_| SignError::InvalidRequest)?;
        if binding.key_name != credentials.key() || binding.algorithm != credentials.algorithm() {
            return Err(SignError::InvalidRequest);
        }
        let signature = credentials
            .sign_input(input)
            .map_err(|_| SignError::InvalidRequest)?;
        Ok(SignResult::new(
            vec![credentials.key().as_bytes().to_vec(), signature.to_vec()],
            work.receipt,
        ))
    }
}
fn algorithm_for(algorithm: JwtAlgorithm) -> Algorithm {
    match algorithm {
        JwtAlgorithm::Es256 => Algorithm::Es256,
        JwtAlgorithm::EdDsa => Algorithm::Ed25519,
    }
}
impl Signer for Owned {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.run(&request)))
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.run(request))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        if self.slots.iter().any(|(id, c)| {
            id.signing_ref().ok() == Some(key) && algorithm_for(c.algorithm()) == algorithm
        }) {
            Capability::Supported
        } else {
            Capability::Unsupported
        }
    }
}
fn install(
    setup: Setup,
    options: &mut EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let mut provider = Owned { slots: vec![] };
    let mut ids = Identities::default();
    options.coinbase_signing = None;
    options.coinbase_read_signing = None;
    // Match Coinbase::new: without private primary authority, neither slot is used.
    if construction::signer_setup_public_data_only(options)
        || setup
            .primary
            .as_ref()
            .is_none_or(|slot| slot.key.is_empty())
    {
        return Ok((std::sync::Arc::new(provider), ids));
    }
    for (name, slot) in [("primary", setup.primary), ("secondary", setup.secondary)] {
        let Some(mut slot) = slot else {
            continue;
        };
        if slot.key.is_empty() && slot.secret.is_empty() {
            continue;
        }
        if slot.trim_credential == Some(true)
            || slot
                .secret_encoding
                .as_deref()
                .is_some_and(|s| !matches!(s, "utf8" | "pem" | "base64"))
        {
            return Err(invalid("Unsupported Coinbase signer setup metadata"));
        }
        let credentials = Credentials::new(&slot.key, &slot.secret).map_err(invalid)?;
        let identity = Identity::new(
            KeyRef(format!("coinbase/{name}/credential")),
            Some(KeyRef(format!("coinbase/{name}/signing"))),
        )
        .with_stable_id(slot.stable_id.take());
        identity
            .validate()
            .map_err(|_| invalid("Invalid Coinbase signing identity"))?;
        let binding = credentials.binding();
        if name == "primary" {
            ids.primary = Some(identity.clone());
            options.coinbase_signing = Some(binding);
        } else {
            ids.secondary = Some(identity.clone());
            options.coinbase_read_signing = Some(binding);
        }
        provider.slots.push((identity, credentials));
    }
    Ok((std::sync::Arc::new(provider), ids))
}
pub(super) fn raw(options: &mut EngineOptions) -> Result<(Provider, Identities), VenueError> {
    let primary = options.key.take().map(|key| Slot {
        key,
        secret: options.secret.take().unwrap_or_default(),
        stable_id: None,
        passphrase: options.passphrase.take(),
        secret_encoding: None,
        trim_credential: None,
    });
    let secondary = options.read_credentials.take().map(|mut read| {
        let slot = Slot {
            key: std::mem::take(&mut read.key),
            secret: std::mem::take(&mut read.secret),
            stable_id: None,
            passphrase: read.passphrase.take(),
            secret_encoding: None,
            trim_credential: None,
        };
        read.erase_secrets();
        slot
    });
    drop(Zeroizing::new(options.secret.take().unwrap_or_default()));
    install(Setup { primary, secondary }, options)
}
pub(super) fn json(
    input: String,
    options: &mut EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let input = Zeroizing::new(input);
    let setup =
        serde_json::from_str(&input).map_err(|_| invalid("Invalid Coinbase local signer setup"))?;
    install(setup, options)
}
