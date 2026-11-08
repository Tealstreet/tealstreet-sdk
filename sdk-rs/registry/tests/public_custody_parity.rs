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
    for venue in ["bitrue", "polymarket"] {
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

#[test]
fn keyed_missing_callbacks_are_invalid_and_explicitly_not_sent() {
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
fn valid_provider_keeps_disabled_private_remote_signing_unsupported() {
    for venue in ["apexomni", "mexc", "asterdex"] {
        let value = json!({"remoteSigning": true,
            "publicMode": {"requested": false, "remoteSigner": true},
            "signing": {"primary": {"credential": "configured-key", "signing": "opaque-agent"}}});
        let error = build_with_signer(venue, options(value), None, Some(Arc::new(NoIo)), Mode)
            .expect_err("valid provider cannot enable unsupported private custody");
        assert_eq!(error.kind, Some(ErrorKind::Unsupported), "{venue}");
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{venue}");
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
