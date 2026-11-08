#![cfg(feature = "polymarket")]
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer, remote_signing_policy};

struct Reject;
impl Visit for Reject {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {
        panic!("unsupported custody must refuse before engine construction");
    }
}

struct PublicOnly;
impl Visit for PublicOnly {
    type Output = ();
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) {
        assert!(!engine.account_startup());
    }
}

fn construct_public(value: serde_json::Value) {
    for injected in [false, true] {
        let options = serde_json::from_value(value.clone()).unwrap();
        let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
        if injected {
            build_with_signer("polymarket", options, entropy, None, PublicOnly)
        } else {
            build("polymarket", options, entropy, PublicOnly)
        }
        .unwrap();
    }
}

#[test]
fn inferred_public_mode_ignores_unused_remote_signing() {
    construct_public(serde_json::json!({
        "key": "", "accountStartup": true, "remoteSigning": true,
        "publicMode": {"requested": null, "remoteSigner": true}
    }));
}

#[test]
fn inferred_public_mode_ignores_unused_payload_signer() {
    construct_public(serde_json::json!({
        "key": "", "accountStartup": true, "payloadSigner": true,
        "publicMode": {"requested": null, "payloadSigner": true}
    }));
}

#[test]
fn inferred_public_mode_without_signing_flags_constructs() {
    construct_public(serde_json::json!({"key": "", "publicMode": {"requested": null}}));
}

#[test]
fn inferred_private_mode_keeps_custody_refusal() {
    for (flag, intent) in [
        ("remoteSigning", "remoteSigner"),
        ("payloadSigner", "payloadSigner"),
    ] {
        let mut value = serde_json::json!({
            "key": "12".repeat(32), "publicMode": {"requested": null}
        });
        value[flag] = true.into();
        value["publicMode"][intent] = true.into();
        for injected in [false, true] {
            let options = serde_json::from_value(value.clone()).unwrap();
            let error = if injected {
                build_with_signer("polymarket", options, None, None, Reject)
            } else {
                build("polymarket", options, None, Reject)
            }
            .unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::Unsupported));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert!(
                error
                    .message
                    .contains("user WebSocket requires the API secret")
            );
        }
    }
}

#[test]
fn signature_only_custody_cannot_supply_user_websocket_credentials() {
    for mode in ["remoteSigning", "payloadSigner"] {
        for injected in [false, true] {
            let mut value =
                serde_json::json!({"key": "not-a-local-key", "secret": "unused-local-material"});
            value[mode] = true.into();
            if injected {
                value["signing"] = serde_json::json!({"primary": {"credential": "opaque-credential", "signing": "opaque-signing"}});
            }
            let options: EngineOptions = serde_json::from_value(value).unwrap();
            let error = if injected {
                build_with_signer("polymarket", options, None, None, Reject)
            } else {
                build("polymarket", options, None, Reject)
            }
            .unwrap_err();
            assert_eq!(error.kind, Some(ErrorKind::Unsupported));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert!(
                error
                    .message
                    .contains("user WebSocket requires the API secret")
            );
            assert!(!error.message.contains("unused-local-material"));
        }
    }
}

#[test]
fn construction_policy_explicitly_refuses_both_callback_modes() {
    let policy = remote_signing_policy("polymarket");
    assert!(!policy.enabled);
    assert!(!policy.payload_supported);
}

#[test]
fn explicit_public_mode_still_constructs_without_signing_material() {
    for public in [
        serde_json::json!({"publicDataOnly": true}),
        serde_json::json!({"publicMode": {"requested": true}}),
    ] {
        for flag in ["remoteSigning", "payloadSigner"] {
            let mut value = public.clone();
            value[flag] = true.into();
            construct_public(value);
        }
    }
}
