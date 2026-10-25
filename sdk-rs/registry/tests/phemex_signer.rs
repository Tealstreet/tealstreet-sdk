//! Original construction controls; the Phemex provider arm is not enabled yet.
#![cfg(all(feature = "phemex", feature = "dispatch-signing"))]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_engine::{Action, Engine, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
const NOW: f64 = 1_700_000_000_000.;
fn provider() -> (Provider, Identities) {
    let (local, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("key".into()),
            secret: Secret::new("secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    (std::sync::Arc::new(local), identities)
}
struct Registered;
impl Visit for Registered {
    type Output = (Identities, bool);
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        (engine.identities().clone(), engine.signer().is_some())
    }
}
struct Start;
impl Visit for Start {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(NOW);
        safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
    }
}
fn entropy() -> Option<safe_cex_engine::entropy::Entropy> {
    Some(safe_cex_engine::test_support::seeded_entropy(17))
}
#[test]
fn opaque_phemex_setup_accepts_the_owned_provider_and_registers_exact_refs() {
    let (local, refs) = provider();
    let mut input: EngineOptions = serde_json::from_str("{}").unwrap();
    input.signing = Some(refs.clone());
    let (registered, present) =
        build_with_signer("phemex", input, entropy(), Some(local), Registered)
            .expect("full Phemex custody cutover must accept opaque provider setup");
    assert!(present);
    assert_eq!(registered, refs);
}
#[test]
fn missing_provider_is_refused_before_any_venue_work() {
    let (_, refs) = provider();
    let mut input: EngineOptions = serde_json::from_str("{}").unwrap();
    input.signing = Some(refs);
    assert!(build_with_signer("phemex", input, entropy(), None, Start).is_err());
}
#[test]
fn trusted_legacy_inputs_preserve_the_existing_signed_startup_wire() {
    let mut input: EngineOptions = serde_json::from_str("{}").unwrap();
    input.key = Some("key".into());
    input.secret = Some("secret".into());
    let output = build("phemex", input, entropy(), Start).unwrap();
    let requests: Vec<_> = output
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://api.phemex.com/public/products");
    assert_eq!(
        requests[0].headers[0],
        ("X-PHEMEX-ACCESS-TOKEN", "key".into())
    );
    assert_eq!(
        requests[0].headers[1],
        ("X-PHEMEX-REQUEST-EXPIRY", "1700000060".into())
    );
    assert_eq!(
        requests[0].headers[3],
        ("Content-Type", "application/json".into())
    );
}
