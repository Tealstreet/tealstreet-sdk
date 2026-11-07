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
    struct Accept;
    impl Visit for Accept {
        type Output = ();
        fn visit<V: Venue + 'static>(self, _: Engine<V>) {}
    }
    for public in [
        serde_json::json!({"publicDataOnly": true}),
        serde_json::json!({"publicMode": {"requested": true}}),
    ] {
        let mut value = public;
        value["remoteSigning"] = true.into();
        value["payloadSigner"] = true.into();
        build(
            "polymarket",
            serde_json::from_value(value).unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Accept,
        )
        .unwrap();
    }
}
