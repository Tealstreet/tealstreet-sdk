#![cfg(all(
    feature = "hyperliquid",
    feature = "paradex",
    feature = "polymarket",
    feature = "bitrue",
    feature = "asterdex"
))]

use safe_cex_engine::signing::{SignError, SignFuture, SignRequest, Signer};
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use serde_json::{Value, json};
use std::sync::Arc;

struct Mode;
impl Visit for Mode {
    type Output = bool;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> bool {
        engine.account_startup()
    }
}

struct NoIo;
impl Signer for NoIo {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("construction must not call the signer");
    }
    fn sign_ready(
        &self,
        _: &SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, SignError>> {
        panic!("construction must not call the signer");
    }
}

fn options(value: Value) -> EngineOptions {
    serde_json::from_value(value).expect("fixture options")
}

#[test]
fn keyless_payload_accounts_remain_public_through_both_entrypoints() {
    for venue in ["hyperliquid", "hyperliquidspot", "paradex"] {
        for key in [Value::Null, json!("")] {
            for requested in [Value::Null, json!(false)] {
                let value = json!({"key": key, "accountStartup": true, "payloadSigner": true,
                    "publicMode": {"requested": requested, "payloadSigner": true}});
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                assert!(!build(venue, options(value.clone()), entropy, Mode).expect(venue));
                let mut injected = value;
                injected["key"] = Value::Null;
                injected["signing"] = json!({});
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                assert!(
                    !build_with_signer(
                        venue,
                        options(injected),
                        entropy,
                        Some(Arc::new(NoIo)),
                        Mode
                    )
                    .expect(venue)
                );
            }
        }
    }
}

#[test]
fn keyless_hyperliquid_spot_remote_signer_keeps_the_declared_private_mode() {
    for key in [Value::Null, json!("")] {
        for requested in [Value::Null, json!(false)] {
            let value = json!({"key": key, "accountStartup": true, "remoteSigning": true,
                "publicMode": {"requested": requested, "remoteSigner": true},
                "signing": {"primary": {"credential": "", "signing": "opaque-agent"}}});
            let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
            assert!(
                build_with_signer(
                    "hyperliquidspot",
                    options(value),
                    entropy,
                    Some(Arc::new(NoIo)),
                    Mode
                )
                .expect("valid delegated Spot account")
            );
        }
    }
}

#[test]
fn outside_beta_native_accounts_refuse_before_public_or_private_custody() {
    // restored by #3502; #3317 dropped a live venue without a product decision.
    for venue in ["polymarket"] {
        for flag in ["remoteSigning", "payloadSigner"] {
            for public in [true, false] {
                let mut value = json!({"key": "unused-key", "secret": "unused-fixture",
                    "publicDataOnly": public});
                value[flag] = json!(true);
                for injected in [false, true] {
                    let error = if injected {
                        safe_cex_venues::build_with_signer_in_context(
                            safe_cex_venues::ConstructionContext::Hosted,
                            venue,
                            options(value.clone()),
                            None,
                            Some(Arc::new(NoIo)),
                            Mode,
                        )
                    } else {
                        safe_cex_venues::build_in_context(
                            safe_cex_venues::ConstructionContext::Hosted,
                            venue,
                            options(value.clone()),
                            None,
                            Mode,
                        )
                    }
                    .expect_err("OUT venue must refuse");
                    assert_eq!(error.kind, Some(ErrorKind::Unsupported), "{venue}");
                    assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}");
                }
            }
        }
    }
}

#[cfg(all(
    feature = "kraken",
    feature = "apexomni",
    feature = "cryptocom",
    feature = "mexc",
    feature = "orderly"
))]
#[test]
fn keyed_missing_callbacks_keep_adoption_and_configuration_refusals() {
    for venue in [
        "kraken",
        "krakenspot",
        "apexomni",
        "cryptocom",
        "mexc",
        "kodiakfi",
        "mode",
        "what",
        "woofi",
        "asterdex",
    ] {
        for version in ["v1", "v3"] {
            if venue != "asterdex" && version == "v3" {
                continue;
            }
            for (flag, intent) in [
                ("remoteSigning", "remoteSigner"),
                ("payloadSigner", "payloadSigner"),
            ] {
                let mut value = json!({"key": "configured-key", "secret": "", "asterdex": {"apiVersion": version},
                    "publicMode": {"requested": null}});
                value[flag] = json!(true);
                value["publicMode"][intent] = json!(true);
                for injected in [false, true] {
                    let error = if injected {
                        build_with_signer(venue, options(value.clone()), None, None, Mode)
                    } else {
                        build(venue, options(value.clone()), None, Mode)
                    }
                    .expect_err("missing callback provider");
                    assert_eq!(
                        error.kind,
                        Some(ErrorKind::InvalidInput),
                        "{venue}/{version}/{flag}"
                    );
                    assert_eq!(
                        error.message, "Delegated signing requires a configured callback provider",
                        "{venue}/{version}/{flag}"
                    );
                    assert_eq!(
                        error.outcome,
                        Some(Outcome::NotSent),
                        "{venue}/{version}/{flag}"
                    );
                    if !injected {
                        let mut public = value.clone();
                        public["publicMode"]["requested"] = json!(true);
                        let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                        assert!(
                            !build(venue, options(public), entropy, Mode)
                                .expect("explicit public mode ignores unused callbacks")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn valid_provider_admits_asterdex_private_signing_after_hosted_adoption() {
    use safe_cex_venues::{ConstructionContext, build_with_signer_in_context};

    for context in [ConstructionContext::Client, ConstructionContext::Hosted] {
        let value = json!({"remoteSigning": true,
            "publicMode": {"requested": false, "remoteSigner": true},
            "signing": {"primary": {"credential": "configured-key", "signing": "opaque-agent"}}});
        // #3302 (1bc6ca7c41) enables AsterDEX RemoteSigner custody; the former
        // Unsupported expectation becomes private construction with provider/entropy.
        // Missing-callback and V3 PayloadSigner refusal controls remain unchanged.
        assert!(
            build_with_signer_in_context(
                context,
                "asterdex",
                options(value),
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Some(Arc::new(NoIo)),
                Mode,
            )
            .expect("configured AsterDEX provider is admitted"),
            "{context:?}: RemoteSigner admission must keep private startup"
        );
    }
}

#[cfg(feature = "apexomni")]
#[test]
fn valid_provider_admits_apexomni_private_signing_after_hosted_adoption() {
    use safe_cex_venues::{ConstructionContext, build_with_signer_in_context};

    // Master #3300 enabled RemoteSigner custody. A valid provider must now
    // construct a private engine, rather than use the disabled-family refusal.
    for context in [ConstructionContext::Client, ConstructionContext::Hosted] {
        let value = json!({"remoteSigning": true,
            "publicMode": {"requested": false, "remoteSigner": true},
            "signing": {"primary": {"credential": "configured-key", "signing": "opaque-agent",
                "passphrase": "opaque-passphrase", "extra": {"zklinkSeed": "opaque-zklink"}}}});
        let error = build_with_signer_in_context(
            context,
            "apexomni",
            options(value.clone()),
            None,
            Some(Arc::new(NoIo)),
            Mode,
        )
        .expect_err("admitted custody still requires engine entropy");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput), "{context:?}");
        assert_eq!(error.message, "Engine entropy providers are required");
        assert!(
            build_with_signer_in_context(
                context,
                "apexomni",
                options(value),
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Some(Arc::new(NoIo)),
                Mode,
            )
            .expect("configured ApexOmni provider is admitted"),
            "{context:?}: RemoteSigner admission must keep private startup"
        );
    }
}

#[cfg(feature = "mexc")]
#[test]
fn valid_provider_admits_mexc_private_signing_after_hosted_adoption() {
    for (flag, intent) in [
        ("remoteSigning", "remoteSigner"),
        ("payloadSigner", "payloadSigner"),
    ] {
        let mut value = json!({"publicMode": {"requested": false},
            "signing": {"primary": {"credential": "configured-key", "signing": "opaque-agent"}}});
        value[flag] = json!(true);
        value["publicMode"][intent] = json!(true);
        assert!(
            build_with_signer(
                "mexc",
                options(value),
                Some(safe_cex_engine::test_support::seeded_entropy(17)),
                Some(Arc::new(NoIo)),
                Mode
            )
            .expect("configured MEXC provider is admitted")
        );
    }
}

#[test]
fn client_beta_families_construct_public_engines() {
    for venue in ["bitrue", "polymarket"] {
        for value in [
            json!({"publicMode": {"requested": null}}),
            json!({"publicDataOnly": true}),
            json!({"key": "unused-key", "publicMode": {"requested": true}}),
        ] {
            let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
            assert!(
                !build(venue, options(value), entropy, Mode).expect("client family construction")
            );
        }
    }
}

#[test]
fn explicit_client_context_constructs_public_engines_with_unused_callback_intent() {
    use safe_cex_venues::{ConstructionContext, build_in_context, build_with_signer_in_context};
    for venue in ["bitrue", "polymarket"] {
        for flag in ["remoteSigning", "payloadSigner"] {
            for requested in [Value::Null, json!(true)] {
                let mut value = json!({"key": "", "accountStartup": true,
                    "publicMode": {"requested": requested}});
                value[flag] = json!(true);
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                assert!(
                    !build_in_context(
                        ConstructionContext::Client,
                        venue,
                        options(value.clone()),
                        entropy,
                        Mode
                    )
                    .expect("public client")
                );
                value["key"] = Value::Null;
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                assert!(
                    !build_with_signer_in_context(
                        ConstructionContext::Client,
                        venue,
                        options(value),
                        entropy,
                        None,
                        Mode
                    )
                    .expect("public client without provider")
                );
            }
        }
    }
}

#[cfg(all(
    feature = "kraken",
    feature = "coinbase",
    feature = "txflow",
    feature = "orderly"
))]
#[test]
fn disabled_private_payload_checks_callback_configuration_before_admission() {
    use safe_cex_venues::{ConstructionContext, build_in_context, build_with_signer_in_context};
    for venue in [
        "kraken",
        "krakenspot",
        "coinbase",
        "coinbasespot",
        "txflow",
        "kodiakfi",
        "mode",
        "what",
        "woofi",
    ] {
        for context in [ConstructionContext::Client, ConstructionContext::Hosted] {
            for local in ["", "unused-local"] {
                for remote in [false, true] {
                    let value = json!({"key": "configured-key", "secret": local,
                        "payloadSigner": true, "remoteSigning": remote, "testnet": true,
                        "publicMode": {"requested": false, "payloadSigner": true,
                            "remoteSigner": remote}});
                    for injected in [false, true] {
                        let result = if injected {
                            build_with_signer_in_context(
                                context,
                                venue,
                                options(value.clone()),
                                None,
                                Some(Arc::new(NoIo)),
                                Mode,
                            )
                        } else {
                            build_in_context(context, venue, options(value.clone()), None, Mode)
                        };
                        let error = result.expect_err("unadopted private custody");
                        // Full Hosted roster admission precedes callback validation.
                        // Client TxFlow retains the existing codec/provider errors.
                        let hosted_out =
                            context == ConstructionContext::Hosted && venue == "txflow";
                        assert_eq!(
                            error.kind,
                            Some(if injected || hosted_out {
                                ErrorKind::Unsupported
                            } else {
                                ErrorKind::InvalidInput
                            }),
                            "{venue}/{context:?}"
                        );
                        assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}/{context:?}");
                        assert_eq!(
                            error.message,
                            if hosted_out {
                                format!("unsupported venue: {venue}")
                            } else if injected {
                                match venue {
                                    "coinbase" | "coinbasespot" => format!(
                                        "Rust {venue} payloadSigner requires a configured injected signing provider"
                                    ),
                                    "kraken" => {
                                        "Rust kraken has no REST-only PayloadSigner lifecycle"
                                            .to_owned()
                                    }
                                    "kodiakfi" | "mode" | "what" | "woofi" => {
                                        "Orderly injected private signing admission is closed"
                                            .to_owned()
                                    }
                                    _ => format!(
                                        "Rust {venue} delegated signing is unsupported in this build. Remove payloadSigner and configure local signing credentials."
                                    ),
                                }
                            } else {
                                "Delegated signing requires a configured callback provider"
                                    .to_owned()
                            }
                        );
                    }
                }
            }
        }
    }
}

#[cfg(any(feature = "mexc", feature = "bitunix"))]
fn normalized_keyless_callbacks(venue: &str) {
    for key in ["", "   ", "\u{0085}\u{2003}"] {
        for (flag, intent) in [
            ("remoteSigning", "remoteSigner"),
            ("payloadSigner", "payloadSigner"),
        ] {
            for requested in [Value::Null, json!(false)] {
                let mut value = json!({"key": key, "secret": "", "accountStartup": true,
                    "publicMode": {"requested": requested}});
                value[flag] = json!(true);
                value["publicMode"][intent] = json!(true);
                let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
                let public = build(venue, options(value), entropy, Mode)
                    .expect("normalized keyless callbacks keep public mode");
                assert!(!public, "{venue}/{flag}/{key:?}/{requested}");
            }
        }
    }
}

#[cfg(feature = "mexc")]
#[test]
// TS #3320 fd43a168: exchanges/rust/localSignerSetup.ts:63-96 normalizes before derivePublicMode.
fn review2_normalized_keyless_mexc_callbacks_are_public() {
    normalized_keyless_callbacks("mexc");
}

#[cfg(feature = "bitunix")]
#[test]
// TS #3320 fd43a168: exchanges/rust/localSignerSetup.ts:63-96 normalizes before derivePublicMode.
fn review2_normalized_keyless_bitunix_callbacks_are_public() {
    normalized_keyless_callbacks("bitunix");
}

#[cfg(all(feature = "grvt", feature = "kucoin", feature = "kucoinspot"))]
#[test]
// TS #3320 fd43a168: exchanges/rust/delegatedSignerSetup.ts:74-86 refuses these modes before callbacks.
fn review2_grvt_kucoin_payload_refusal_precedes_provider_validation() {
    use safe_cex_venues::{ConstructionContext, build_in_context, build_with_signer_in_context};
    for venue in ["grvt", "kucoin", "kucoinspot"] {
        for context in [ConstructionContext::Client, ConstructionContext::Hosted] {
            for requested in [Value::Null, json!(false)] {
                let value = json!({"key": "configured-key", "secret": "", "payloadSigner": true,
                    "publicMode": {"requested": requested, "payloadSigner": true}});
                for entry in ["raw", "injected-missing", "injected-ready"] {
                    let result = match entry {
                        "raw" => {
                            build_in_context(context, venue, options(value.clone()), None, Mode)
                        }
                        "injected-missing" => build_with_signer_in_context(
                            context,
                            venue,
                            options(value.clone()),
                            None,
                            None,
                            Mode,
                        ),
                        _ => build_with_signer_in_context(
                            context,
                            venue,
                            options(value.clone()),
                            None,
                            Some(Arc::new(NoIo)),
                            Mode,
                        ),
                    };
                    let error = result.expect_err("TS refuses unsupported payload mode first");
                    assert_eq!(error.kind, Some(ErrorKind::Unsupported), "{venue}/{entry}");
                    assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}/{entry}");
                    assert!(
                        error.message.contains("PayloadSigner"),
                        "{venue}: {}",
                        error.message
                    );
                }
            }
        }
    }
}

#[cfg(all(feature = "kraken", feature = "coinbase"))]
#[test]
// TS #3320 fd43a168: exchanges/rust/betaVenues.ts:84-95 validates callbacks before disabled modes.
fn review2_kraken_coinbase_provider_validation_precedes_payload_refusal() {
    use safe_cex_venues::{ConstructionContext, build_in_context, build_with_signer_in_context};
    for venue in ["kraken", "krakenspot", "coinbase", "coinbasespot"] {
        for context in [ConstructionContext::Client, ConstructionContext::Hosted] {
            for requested in [Value::Null, json!(false)] {
                let value = json!({"key": "configured-key", "secret": "", "payloadSigner": true,
                    "publicMode": {"requested": requested, "payloadSigner": true}});
                for entry in ["raw", "injected-missing", "injected-ready"] {
                    let ready = entry == "injected-ready";
                    let result = match entry {
                        "raw" => {
                            build_in_context(context, venue, options(value.clone()), None, Mode)
                        }
                        "injected-missing" => build_with_signer_in_context(
                            context,
                            venue,
                            options(value.clone()),
                            None,
                            None,
                            Mode,
                        ),
                        _ => build_with_signer_in_context(
                            context,
                            venue,
                            options(value.clone()),
                            None,
                            Some(Arc::new(NoIo)),
                            Mode,
                        ),
                    };
                    let error = result.expect_err("private disabled-family custody");
                    assert_eq!(
                        error.kind,
                        Some(if ready {
                            ErrorKind::Unsupported
                        } else {
                            ErrorKind::InvalidInput
                        }),
                        "{venue}/{entry}"
                    );
                    assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}/{entry}");
                    if !ready {
                        assert_eq!(
                            error.message,
                            "Delegated signing requires a configured callback provider",
                            "{venue}/{entry}"
                        );
                    }
                }
            }
        }
    }
}
