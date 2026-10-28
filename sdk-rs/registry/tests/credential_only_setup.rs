//! Credential-only trusted setup (docs/plans/RUST_CREDENTIAL_ONLY_SETUP.md).
use safe_cex_venues::EngineOptions;

/// C8: injected (remote, delegated or hosted) identities keep requiring a
/// signing ref, so an omitted or null one is still refused where the host
/// parses engine options ("Invalid engine options").
#[test]
fn c8_injected_identities_without_a_signing_ref_are_refused_at_parse() {
    for primary in [
        r#"{"credential":"primary.credential"}"#,
        r#"{"credential":"primary.credential","signing":null}"#,
    ] {
        for slot in ["primary", "secondary"] {
            let options = format!(r#"{{"signing":{{"{slot}":{primary}}}}}"#);
            assert!(
                serde_json::from_str::<EngineOptions>(&options).is_err(),
                "{options}"
            );
        }
    }
    let complete = r#"{"signing":{"primary":{"credential":"primary.credential","signing":"primary.signing"}}}"#;
    assert!(serde_json::from_str::<EngineOptions>(complete).is_ok());
}

struct Accept;
impl safe_cex_venues::Visit for Accept {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, _engine: safe_cex_engine::Engine<V>) {}
}

/// K4 (setup half): the non-validating crates without dispatch signing
/// never receive a setup Identity, so D1 cannot move their terminal. ApeX
/// Omni, the eighth, is pinned after the socket sign-failure fix lane.
#[test]
fn k4_non_validating_crates_without_dispatch_signing_refuse_injected_identities() {
    for venue in ["breakoutprop", "grvt", "orderly", "polymarket"] {
        for primary in [
            r#"{"credential":"primary.credential","signing":""}"#,
            r#"{"credential":"","signing":"primary.signing"}"#,
        ] {
            let options: EngineOptions =
                serde_json::from_str(&format!(r#"{{"signing":{{"primary":{primary}}}}}"#)).unwrap();
            let error = safe_cex_venues::build(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Accept,
            )
            .expect_err("injected identities are refused");
            assert_eq!(
                error.message, "Injected signing is not supported for this venue yet",
                "{venue}"
            );
        }
    }
}

#[cfg(feature = "hyperliquid")]
#[test]
fn k4_hyperliquid_injected_refs_still_require_the_installed_provider_after_cutover() {
    for venue in ["hyperliquid", "hyperliquidspot"] {
        for primary in [
            r#"{"credential":"primary.credential","signing":""}"#,
            r#"{"credential":"","signing":"primary.signing"}"#,
            r#"{"credential":"primary.credential","signing":"primary.signing"}"#,
        ] {
            let options: EngineOptions =
                serde_json::from_str(&format!(r#"{{"signing":{{"primary":{primary}}}}}"#)).unwrap();
            let error = safe_cex_venues::build(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Accept,
            )
            .expect_err("refs alone cannot install a signing provider");
            assert_eq!(
                error.message,
                "Hyperliquid requires its primary signing provider"
            );
            assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
            assert_eq!(error.outcome, None);
        }
    }
}

#[cfg(feature = "paradex")]
#[test]
fn k4_paradex_injected_refs_still_require_the_installed_provider_after_cutover() {
    for primary in [
        r#"{"credential":"primary.credential","signing":""}"#,
        r#"{"credential":"","signing":"primary.signing"}"#,
        r#"{"credential":"primary.credential","signing":"primary.signing"}"#,
    ] {
        let options: EngineOptions =
            serde_json::from_str(&format!(r#"{{"signing":{{"primary":{primary}}}}}"#)).unwrap();
        let error = safe_cex_venues::build(
            "paradex",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Accept,
        )
        .expect_err("refs alone cannot install a signing provider");
        assert_eq!(
            error.message,
            "Paradex requires its primary signing provider"
        );
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
    }
}

#[cfg(feature = "lighter")]
#[test]
fn k4_lighter_injected_refs_still_require_the_installed_provider_after_cutover() {
    for primary in [
        r#"{"credential":"primary.credential","signing":""}"#,
        r#"{"credential":"","signing":"primary.signing"}"#,
        r#"{"credential":"primary.credential","signing":"primary.signing"}"#,
    ] {
        let options: EngineOptions =
            serde_json::from_str(&format!(r#"{{"signing":{{"primary":{primary}}}}}"#)).unwrap();
        let error = safe_cex_venues::build(
            "lighter",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Accept,
        )
        .expect_err("refs alone cannot install a signing provider");
        assert_eq!(
            error.message,
            "Lighter requires its installed primary signer"
        );
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        // Construction has no admitted command or write to project.
        assert_eq!(error.outcome, None);
    }
}

/// K5: the whole construction record treats an absent signing ref exactly
/// like today's empty one, for `Input::Secret` (and `AsterLocalAuth`).
#[test]
fn k5_construction_resolves_absent_and_empty_signing_identically() {
    use safe_cex_engine::signing::KeyRef;
    for venue in ["binance", "bybit", "asterdex", "okx"] {
        let base = r#"{"publicMode":{"requested":false},"signing":{"primary":{"credential":"primary.credential","signing":"primary.signing"}}}"#;
        let construction = |signing: Option<KeyRef>| {
            let mut options: EngineOptions = serde_json::from_str(base).unwrap();
            options
                .signing
                .as_mut()
                .unwrap()
                .primary
                .as_mut()
                .unwrap()
                .set_signing_ref(signing);
            serde_json::to_string(&options.construction(venue)).unwrap()
        };
        assert_eq!(
            construction(None),
            construction(Some(KeyRef(String::new()))),
            "{venue}"
        );
    }
}
