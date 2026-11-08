#![cfg(all(
    any(feature = "kucoin", feature = "kucoinspot"),
    feature = "hosted-signing"
))]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{ExtraKey, Identities, Identity, KeyRef, SignError};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    let mut id = Identity::new(KeyRef("public".into()), Some(KeyRef("signing".into())))
        .with_passphrase(Some(KeyRef("passphrase".into())));
    id.extra
        .insert(ExtraKey::KucoinBroker, KeyRef("broker".into()));
    for (key, value) in [
        ("public", "fixture-key"),
        ("signing", "fixture-secret"),
        ("passphrase", "fixture-passphrase"),
        ("broker", "fixture-broker"),
    ] {
        signer.insert(KeyRef(key.into()), value.as_bytes().to_vec());
    }
    (
        signer,
        Identities {
            primary: Some(id),
            secondary: None,
        },
    )
}

fn kucoin_hosted_batch_keeps_passphrase_and_broker_in_device_custody(venue: &str) {
    let (signer, ids) = fixture();
    let format: OutputFormat = serde_json::from_str("\"kucoin-batch\"").unwrap();
    let descriptor = r#"{"payload":"1700000000123POST/api/v1/orders{\"size\":1}","brokerPrefix":"1700000000123partner"}"#;
    let (work, kind) = hosted_signing::prepare(
        venue,
        &ids,
        Input::WsAuth {
            to_sign: descriptor.into(),
            output_format: format,
        },
    )
    .unwrap();
    work.validate().unwrap();
    assert_eq!(work.context.venue, venue);
    assert_eq!(work.operations.len(), 4);
    let result = signer.sign_now(&work).unwrap();
    let output = hosted_signing::finish(kind, &result).unwrap();
    let batch: serde_json::Value =
        serde_json::from_str(output.signature.as_ref().unwrap()).unwrap();
    assert_eq!(batch["credential"], "fixture-key");
    assert_eq!(
        batch["macs"],
        serde_json::json!(
            result.values[1..]
                .iter()
                .map(|mac| safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64))
                .collect::<Vec<_>>()
        )
    );
    let encoded = serde_json::to_string(&work).unwrap();
    for material in ["fixture-secret", "fixture-passphrase", "fixture-broker"] {
        assert!(!encoded.contains(material));
    }
    assert!(output.credential.is_none());
}

fn kucoin_missing_device_passphrase_refuses_before_provider_work(venue: &str) {
    let (_, mut ids) = fixture();
    ids.primary.as_mut().unwrap().passphrase = None;
    assert!(matches!(
        hosted_signing::prepare(
            venue,
            &ids,
            Input::Http {
                payload: r#"{"payload":"1700000000123GET/api/v1/positions"}"#.into(),
                stamp_ms: 1_700_000_000_123.,
                revision: "0".into(),
            }
        ),
        Err(SignError::Unavailable)
    ));
}

fn kucoin_captures_are_clock_fenced_and_refuse_malformed_or_unknown_work(venue: &str) {
    use safe_cex_engine::signing_capture::Capture;
    let (signer, ids) = fixture();
    let http = |stamp: f64| Input::Http {
        payload: format!(r#"{{"payload":"{}GET/api/v1/positions"}}"#, stamp as u64),
        stamp_ms: stamp,
        revision: "0".into(),
    };
    let stamp = 1_700_000_000_123.;
    let mut capture = Capture::default();
    let (work, kind) = hosted_signing::prepare(venue, &ids, http(stamp)).unwrap();
    let first = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
    let old = signer.sign_now(&first.work).unwrap();
    assert!(
        capture
            .finish(first.id, 1., stamp + 1000., 1, Ok(old))
            .is_err()
    );
    assert!(capture.is_empty());
    let (work, kind) = hosted_signing::prepare(venue, &ids, http(stamp + 1000.)).unwrap();
    let next = capture
        .prepare(work, kind, 1., stamp + 1000., 1, 5000)
        .unwrap();
    let mut signed = signer.sign_now(&next.work).unwrap();
    signed.values.pop();
    assert!(
        capture
            .finish(next.id, 2., stamp + 1000., 1, Ok(signed))
            .is_err()
    );
    assert!(capture.is_empty());
    for payload in [
        r#"{"payload":"p","unknown":"x"}"#,
        r#"{"payload":"p","payload":"q"}"#,
        r#"{"payload":""}"#,
        r#"{"payload":"p","slot":"foreign"}"#,
        r#"{"payload":"p","brokerPrefix":""}"#,
    ] {
        assert!(
            hosted_signing::prepare(
                venue,
                &ids,
                Input::Http {
                    payload: payload.into(),
                    stamp_ms: stamp,
                    revision: "0".into()
                }
            )
            .is_err()
        );
    }
    assert!(hosted_signing::prepare("unknown", &ids, http(stamp)).is_err());
    assert!(
        hosted_signing::prepare(
            venue,
            &ids,
            Input::WsAuth {
                to_sign: "raw-passphrase-must-not-be-admitted".into(),
                output_format: OutputFormat::Base64,
            }
        )
        .is_err()
    );
}

#[test]
fn hosted_signing_for_each_enabled_kucoin_family() {
    for (enabled, venue) in [
        (cfg!(feature = "kucoin"), "kucoin"),
        (cfg!(feature = "kucoinspot"), "kucoinspot"),
    ] {
        if enabled {
            kucoin_hosted_batch_keeps_passphrase_and_broker_in_device_custody(venue);
            kucoin_missing_device_passphrase_refuses_before_provider_work(venue);
            kucoin_captures_are_clock_fenced_and_refuse_malformed_or_unknown_work(venue);
        }
    }
}
