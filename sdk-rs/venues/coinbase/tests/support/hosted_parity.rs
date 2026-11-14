//! Real trading fixtures compare every native JWT graph with the device capture.
use safe_cex_engine::Engine;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, PayloadPart, Provider, SignError,
    SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_venue_coinbase::{Coinbase, Options};
use safe_cex_venues::{
    EngineOptions,
    hosted_signing::{self, Input},
};
use std::sync::Arc;

struct Parity {
    local: Provider,
    ids: Identities,
}
impl Parity {
    fn run(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        let local = self
            .local
            .sign_ready(request)
            .ok_or(SignError::Unavailable)??;
        let [Operation::Present { key }, Operation::Sign { payload, .. }] =
            request.operations.as_slice()
        else {
            panic!("Coinbase graph");
        };
        let [PayloadPart::Bytes { bytes }] = payload.as_slice() else {
            panic!("Coinbase JWS bytes");
        };
        let primary = self
            .ids
            .primary
            .as_ref()
            .filter(|id| id.credential == *key)
            .or_else(|| {
                self.ids
                    .secondary
                    .as_ref()
                    .filter(|id| id.credential == *key)
            })
            .unwrap()
            .clone();
        let ids = Identities {
            primary: Some(primary),
            secondary: None,
        };
        let (work, kind) = hosted_signing::prepare(
            "coinbase",
            &ids,
            Input::WsAuth {
                to_sign: String::from_utf8(bytes.clone()).unwrap(),
                output_format: serde_json::from_str("\"coinbase-jws\"").unwrap(),
            },
        )?;
        assert_eq!(
            serde_json::to_value(&work.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap()
        );
        let hosted = self.local.sign_ready(&work).unwrap()?;
        assert_eq!(local.values, hosted.values, "hosted/local JWT bytes");
        hosted_signing::finish(kind, &hosted)?;
        Ok(local)
    }
}
impl Signer for Parity {
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
        self.local.can_sign(key, algorithm)
    }
}
pub fn engine(mut options: Options) -> Engine<Coinbase> {
    if std::env::var_os("COINBASE_LEGACY_CONTROL").is_some() {
        return safe_cex_engine::test_support::seeded_engine(Coinbase::new(options).unwrap());
    }
    let mut setup: EngineOptions = serde_json::from_str("{}").unwrap();
    setup.key = Some(std::mem::take(&mut options.key));
    setup.secret = Some(std::mem::take(&mut options.secret));
    setup.public_data_only = options.public_data_only;
    setup.read_credentials = options.read_credentials.take();
    let (provider, ids) =
        safe_cex_venues::prepare_local_signer_for("coinbase", &mut setup).unwrap();
    let primary = setup.coinbase_signing.take().zip(ids.primary.clone());
    let secondary = setup
        .coinbase_read_signing
        .take()
        .zip(ids.secondary.clone());
    let venue = Coinbase::with_signing(options, primary, secondary).unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine
        .set_signer(Some(Arc::new(Parity {
            local: provider,
            ids: ids.clone(),
        })))
        .unwrap();
    engine.set_identities(ids).unwrap();
    engine
}
