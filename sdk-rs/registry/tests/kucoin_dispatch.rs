#![cfg(all(feature = "kucoin", feature = "kucoinspot"))]
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Venue};
use safe_cex_venues::{
    EngineOptions, Visit, build, build_with_signer, local_signer_from_capsule,
    prepare_local_signer_for,
};

const NOW: f64 = 1_700_000_000_123.;
const INPUT: &str = r#"{"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-pass","readCredentials":{"key":"read-key","secret":"read-secret"},"extra":{"kucoinBroker":{"partner":" partner ","brokerKey":" broker-material ","brokerName":" broker "}}}"#;
struct Inspect;
impl Visit for Inspect {
    type Output = Vec<HttpRequest>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(NOW);
        engine.handle(NOW, Event::Command(Command::FetchAccount { command: 71 }));
        safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(http) => Some(http),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn real_registry_native_and_capsule_paths_preserve_both_mac_graphs_and_slots() {
    for venue in ["kucoin", "kucoinspot"] {
        for capsule in [false, true] {
            let entropy = Some(safe_cex_engine::test_support::seeded_entropy(17));
            let packets = if capsule {
                let mut options: EngineOptions = serde_json::from_str(
                    r#"{"extra":{"kucoinBroker":{"partner":" partner ","brokerName":" broker "}}}"#,
                )
                .unwrap();
                let (provider, ids) = local_signer_from_capsule(venue,
                    r#"{"primary":{"key":"fixture-key","secret":"fixture-secret","secretEncoding":"base64","passphrase":"fixture-pass","extra":{"kucoinBroker":{"value":" broker-material "}}},"secondary":{"key":"read-key","secret":"read-secret","secretEncoding":"base64"}}"#.into(), &mut options).unwrap();
                options.signing = Some(ids);
                build_with_signer(venue, options, entropy, Some(provider), Inspect).unwrap()
            } else {
                build(
                    venue,
                    serde_json::from_str(INPUT).unwrap(),
                    entropy,
                    Inspect,
                )
                .unwrap()
            };
            let identity = packets
                .iter()
                .find(|p| p.url.ends_with("/user-info"))
                .unwrap();
            let value = |packet: &HttpRequest, name| {
                packet
                    .headers
                    .iter()
                    .find(|(key, _)| *key == name)
                    .unwrap()
                    .1
                    .clone()
            };
            assert_eq!(value(identity, "KC-API-KEY"), "read-key");
            for packet in packets
                .iter()
                .filter(|p| p.headers.iter().any(|(name, _)| *name == "KC-API-KEY"))
            {
                let read = value(packet, "KC-API-KEY") == "read-key";
                let secret = if read {
                    "read-secret"
                } else {
                    "fixture-secret"
                };
                let origin = if venue == "kucoin" {
                    "https://api-futures.kucoin.com"
                } else {
                    "https://api.kucoin.com"
                };
                let path = packet.url.strip_prefix(origin).unwrap();
                assert_eq!(value(packet, "KC-API-TIMESTAMP"), "1700000000123");
                assert_eq!(
                    value(packet, "KC-API-SIGN"),
                    hmac_sha256(
                        secret.as_bytes(),
                        format!("1700000000123{}{path}", packet.method.as_str()).as_bytes(),
                        Encoding::Base64
                    )
                );
                assert_eq!(
                    value(packet, "KC-API-PASSPHRASE"),
                    hmac_sha256(secret.as_bytes(), b"fixture-pass", Encoding::Base64)
                );
                assert_eq!(
                    value(packet, "KC-API-PARTNER-SIGN"),
                    hmac_sha256(
                        b"broker-material",
                        format!("1700000000123partner{}", value(packet, "KC-API-KEY")).as_bytes(),
                        Encoding::Base64
                    )
                );
            }
            if venue == "kucoinspot" {
                assert!(
                    packets
                        .iter()
                        .filter(|p| !p.url.ends_with("/user-info"))
                        .all(|p| p
                            .headers
                            .iter()
                            .all(|(name, _)| !name.starts_with("KC-API-")))
                );
            }
        }
    }
}

#[test]
fn trusted_setup_consumes_all_raw_material_and_refuses_missing_passphrases() {
    for venue in ["kucoin", "kucoinspot"] {
        let mut options: EngineOptions = serde_json::from_str(INPUT).unwrap();
        let (_, ids) = prepare_local_signer_for(venue, &mut options).unwrap();
        assert!(
            options.key.is_none()
                && options.secret.is_none()
                && options.passphrase.is_none()
                && options.read_credentials.is_none()
        );
        assert!(
            options
                .extra
                .as_ref()
                .unwrap()
                .kucoin_broker
                .as_ref()
                .unwrap()
                .broker_key
                .is_empty()
        );
        let encoded = serde_json::to_string(&ids).unwrap();
        for material in [
            "fixture-key",
            "fixture-secret",
            "fixture-pass",
            "broker-material",
            "read-secret",
        ] {
            assert!(!encoded.contains(material));
        }
        for input in [
            r#"{"key":"key","secret":"secret"}"#,
            r#"{"key":"key","secret":"secret","passphrase":"pass","readCredentials":{"key":"read","secret":""}}"#,
        ] {
            assert!(build(venue, serde_json::from_str(input).unwrap(), None, Inspect).is_err());
        }
    }
}
