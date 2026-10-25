//! A key without its secret can never sign. A private ApeX account is refused
//! at construction; a public one (what TS builds, since `canSign` needs both)
//! keeps public data, drops the unsignable identity and says so once.
#![cfg(all(feature = "apexomni", feature = "dispatch-signing"))]

use safe_cex_engine::{Action, Level, LogEvent, Notice};
use safe_cex_venues::{
    EngineOptions, Visit, build, build_with_signer, local_signer_from_json_for_mode,
};
use serde_json::json;

const REFUSED: &str = "ApeX requires the API secret with the API key";

struct Accept;
impl Visit for Accept {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
}

/// Starts the engine and returns its first actions with the installed primary.
struct Started;
impl Visit for Started {
    type Output = (bool, Vec<Action>);
    fn visit<V: safe_cex_engine::Venue + 'static>(
        self,
        mut engine: safe_cex_engine::Engine<V>,
    ) -> Self::Output {
        let primary = engine.identities().primary.is_some();
        engine.start(0.0);
        let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.0);
        (primary, actions)
    }
}

fn key_only(public: bool) -> EngineOptions {
    serde_json::from_value(json!({
        "key": "api-key",
        "passphrase": "pass",
        "publicDataOnly": public,
    }))
    .unwrap()
}

/// The JSON (WASM) path: trusted setup first, then construction with refs.
fn injected<T: Visit>(public: bool, visitor: T) -> Result<T::Output, safe_cex_engine::VenueError> {
    let setup = json!({ "primary": { "key": "api-key", "passphrase": "pass" } });
    let (provider, identities) =
        local_signer_from_json_for_mode("apexomni", setup.to_string(), public).unwrap();
    let mut options: EngineOptions =
        serde_json::from_value(json!({ "publicDataOnly": public })).unwrap();
    options.signing = Some(identities);
    build_with_signer(
        "apexomni",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(11)),
        Some(provider),
        visitor,
    )
}

#[test]
fn a_private_key_only_account_is_refused_at_construction_on_both_paths() {
    let entropy = Some(safe_cex_engine::test_support::seeded_entropy(11));
    let error = build("apexomni", key_only(false), entropy, Accept).unwrap_err();
    assert_eq!(error.message, REFUSED);
    assert_eq!(injected(false, Accept).unwrap_err().message, REFUSED);
}

#[test]
fn a_public_key_only_account_keeps_public_data_and_never_signs() {
    let entropy = Some(safe_cex_engine::test_support::seeded_entropy(11));
    for (primary, actions) in [
        build("apexomni", key_only(true), entropy, Started).unwrap(),
        injected(true, Started).unwrap(),
    ] {
        assert!(!primary, "the unsignable identity is dropped");
        assert!(
            actions.iter().any(|a| matches!(a, Action::Http(_))),
            "the public market read is sent"
        );
        assert!(
            !actions.iter().any(|a| matches!(a, Action::Sign { .. })),
            "nothing is signed"
        );
        let notices: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(LogEvent { level, code, .. }))
                    if *code == "config.invalid" =>
                {
                    Some(*level)
                }
                Action::Notice(Notice::StartupFailed { .. }) => Some(Level::Error),
                _ => None,
            })
            .collect();
        assert_eq!(notices, [Level::Warn], "one visible notice");
    }
}
