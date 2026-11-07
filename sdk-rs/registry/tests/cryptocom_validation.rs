//! Registry validation precedes venue construction; direct-venue controls live
//! in Crypto.com's request_context_private_funnel target.
#![cfg(feature = "cryptocom")]

use safe_cex_engine::{Engine, ErrorKind, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use std::cell::Cell;

const SECONDARY_ERROR: &str = "Secondary read credentials require a nonempty key and secret";

struct Constructed<'a>(&'a Cell<bool>);
impl Visit for Constructed<'_> {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        self.0.set(true);
    }
}

fn options(public_only: bool, primary: bool, read_key: &str, read_secret: &str) -> EngineOptions {
    serde_json::from_value(serde_json::json!({
        "publicDataOnly": public_only,
        "key": primary.then_some("fixture-primary-key"),
        "secret": primary.then_some("fixture-primary-material"),
        "readCredentials": {"key": read_key, "secret": read_secret}
    }))
    .unwrap()
}

#[test]
fn invalid_secondary_is_refused_before_registry_constructs_or_starts_cryptocom() {
    for public_only in [false, true] {
        for primary in [false, true] {
            for (key, secret) in [
                ("", "fixture-read-material"),
                ("fixture-read-key", ""),
                ("", ""),
            ] {
                for injected_route in [false, true] {
                    let visited = Cell::new(false);
                    let options = options(public_only, primary, key, secret);
                    let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                    let error = if injected_route {
                        build_with_signer(
                            "cryptocom",
                            options,
                            entropy,
                            None,
                            Constructed(&visited),
                        )
                    } else {
                        build("cryptocom", options, entropy, Constructed(&visited))
                    }
                    .unwrap_err();
                    assert!(
                        !visited.get(),
                        "construction refusal must precede the venue visitor"
                    );
                    assert_eq!(error.message, SECONDARY_ERROR);
                    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
                    assert!(
                        error.outcome.is_none(),
                        "construction has no command outcome"
                    );
                    assert!(error.venue_code.is_none());
                    assert!(error.venue_message.is_none());
                }
            }
        }
    }
}

#[test]
fn valid_or_absent_secondary_reaches_the_registry_visitor() {
    for public_only in [false, true] {
        for with_secondary in [false, true] {
            let visited = Cell::new(false);
            let mut options = options(
                public_only,
                true,
                "fixture-read-key",
                "fixture-read-material",
            );
            if !with_secondary {
                options.read_credentials = None;
            }
            build(
                "cryptocom",
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Constructed(&visited),
            )
            .unwrap();
            assert!(visited.get());
        }
    }
}

#[test]
fn validate_only_false_status_keeps_the_same_registry_refusal() {
    for public_only in [false, true] {
        for primary in [false, true] {
            for injected_route in [false, true] {
                let visited = Cell::new(false);
                let options = serde_json::from_value(serde_json::json!({
                    "publicDataOnly": public_only,
                    "key": primary.then_some("fixture-primary-key"),
                    "secret": primary.then_some("fixture-primary-material"),
                    "readCredentialsValid": false
                }))
                .unwrap();
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                let error = if injected_route {
                    build_with_signer("cryptocom", options, entropy, None, Constructed(&visited))
                } else {
                    build("cryptocom", options, entropy, Constructed(&visited))
                }
                .unwrap_err();
                assert!(!visited.get());
                assert_eq!(
                    error.message,
                    if injected_route {
                        SECONDARY_ERROR
                    } else {
                        "Unexpected read credential validation result"
                    }
                );
                assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
                assert!(
                    error.outcome.is_none(),
                    "construction has no command outcome"
                );
            }
        }
    }
}

#[test]
fn trusted_setup_consumes_secondary_without_installing_or_retaining_it() {
    for public_only in [false, true] {
        let mut options = options(
            public_only,
            true,
            "fixture-read-key",
            "fixture-read-material",
        );
        let (provider, identities) =
            safe_cex_venues::prepare_local_signer_for("cryptocom", &mut options).unwrap();
        assert!(options.key.is_none());
        assert!(options.secret.is_none());
        assert!(options.read_credentials.is_none());
        assert_eq!(options.read_credentials_valid, Some(true));
        assert!(identities.primary.is_some());
        assert!(identities.secondary.is_none());
        options.signing = Some(identities);
        let visited = Cell::new(false);
        build_with_signer(
            "cryptocom",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(provider),
            Constructed(&visited),
        )
        .unwrap();
        assert!(visited.get());
    }
}

#[test]
fn injected_secondary_identity_is_refused_before_venue_construction() {
    let mut source = options(false, true, "fixture-read-key", "fixture-read-material");
    let (provider, mut identities) = safe_cex_venues::prepare_local_signer(&mut source).unwrap();
    assert!(identities.secondary.is_some());
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(std::mem::take(&mut identities));
    let visited = Cell::new(false);
    let error = build_with_signer(
        "cryptocom",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Some(provider),
        Constructed(&visited),
    )
    .unwrap_err();
    assert!(!visited.get());
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert!(error.outcome.is_none());
}
