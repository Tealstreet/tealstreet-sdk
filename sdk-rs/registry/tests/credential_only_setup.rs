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
