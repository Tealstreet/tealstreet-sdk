//! `/v3/symbols` is signed with a complete identity, so a credentialed
//! account's market load depends on the provider. A refusal must surface and
//! reschedule startup through the ordinary read-failure path, never stall.
mod support;
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, LogEvent, Notice};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use std::sync::Arc;
use support::*;

/// Declares the fixture's capabilities but refuses every signature.
struct Refusing(Arc<dyn Signer>);
impl Signer for Refusing {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(Err(SignError::Refused)))
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(Err(SignError::Refused))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}

#[test]
fn refused_symbols_signature_reports_and_retries_startup_without_sending() {
    let (provider, identities) = fixture_provider(&Material {
        key: "key",
        signing: "c2VjcmV0",
        passphrase: Some("pass"),
        seed: None,
    });
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Options::default()
    }));
    e.set_identities(identities).unwrap();
    e.set_signer(Some(Arc::new(Refusing(provider)))).unwrap();
    e.start(0.0);
    let actions: Vec<_> = e.prepared(0.0).collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    let message = |code: &str, text: &str| {
        actions.iter().any(|a| matches!(a,
            Action::Notice(Notice::Event(LogEvent { code: c, message, .. })) if *c == code && message.contains(text)))
    };
    assert!(message("request.failed", "refused"));
    assert!(message("request.failed", "retry scheduled"));
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}
