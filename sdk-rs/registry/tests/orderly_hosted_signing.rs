#![cfg(all(feature = "orderly", feature = "hosted-signing"))]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::{
    signing::{
        Algorithm, Identities, Identity, KeyRef, Operation, PayloadPart, SignError, SignResult,
    },
    signing_capture::Capture,
};
use safe_cex_venues::hosted_signing::{self, Input};
use serde::Deserialize;
use serde_json::{Value, json};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const ALIASES: [&str; 4] = ["kodiakfi", "mode", "what", "woofi"];
const STAMP: f64 = 1_700_000_000_123.;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    public_key: String,
    vectors: Vec<Vector>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    name: String,
    kind: String,
    stamp_ms: f64,
    payload: String,
    signature: String,
}
fn golden() -> TestResult<Golden> {
    Ok(serde_json::from_str(include_str!(
        "../../../packages/safe-cex/src/__tests__/fixtures/orderly-device-signing.json"
    ))?)
}
fn fixture() -> TestResult<(LocalSigner, Identities)> {
    let mut signer = LocalSigner::new();
    signer.insert(
        KeyRef("owned-public".into()),
        golden()?.public_key.into_bytes(),
    );
    signer.insert_declared(
        KeyRef("owned-signing".into()),
        vec![7; 32],
        Algorithm::Ed25519,
    )?;
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("owned-public".into()),
            Some(KeyRef("owned-signing".into())),
        )),
        secondary: None,
    };
    Ok((signer, ids))
}
fn descriptor(venue: &str, row: &Vector) -> Value {
    json!({"version":"orderly/1", "venue":venue, "slot":"primary",
        "kind":row.kind, "stampMs":row.stamp_ms, "payload":row.payload})
}
fn input(value: Value, ws: bool) -> TestResult<Input> {
    Ok(serde_json::from_value(if ws {
        json!({"kind":"wsAuth", "toSign":value.to_string(), "outputFormat":"orderly-ed25519"})
    } else {
        json!({"kind":"http", "payload":value.to_string(), "stampMs":STAMP, "revision":"0"})
    })?)
}
fn read() -> TestResult<Vector> {
    golden()?
        .vectors
        .into_iter()
        .find(|row| row.name == "read")
        .ok_or("missing read fixture".into())
}
fn prepared(
    ids: &Identities,
    value: Value,
) -> TestResult<(
    safe_cex_engine::signing::SignRequest,
    hosted_signing::OutputKind,
)> {
    Ok(hosted_signing::prepare("mode", ids, input(value, true)?)?)
}

#[test]
fn orderly_device_rest_and_ws_match_local_ed25519_bytes() -> TestResult {
    let (signer, ids) = fixture()?;
    for venue in ALIASES {
        for row in golden()?.vectors {
            for wire_ws in [false, true] {
                let mut encoded = input(descriptor(venue, &row), wire_ws)?;
                if let Input::Http { stamp_ms, .. } = &mut encoded {
                    *stamp_ms = row.stamp_ms;
                }
                let (work, kind) = hosted_signing::prepare(venue, &ids, encoded)?;
                assert_eq!(work.context.venue, venue);
                assert_eq!(work.receipt.stamp_ms, Some(row.stamp_ms));
                assert!(work.receipt.nonce.is_none() && work.receipt.expiry_ms.is_none());
                let [
                    Operation::Present { key },
                    Operation::Sign {
                        algorithm,
                        key: signing,
                        payload,
                    },
                ] = work.operations.as_slice()
                else {
                    return Err("unexpected graph".into());
                };
                assert_eq!(key.0, "owned-public");
                assert_eq!(signing.0, "owned-signing");
                assert_eq!(*algorithm, Algorithm::Ed25519);
                assert_eq!(
                    payload,
                    &vec![PayloadPart::Bytes {
                        bytes: row.payload.as_bytes().to_vec()
                    }]
                );
                let result = signer.sign_now(&work)?;
                work.validate_result(&result)?;
                let output = hosted_signing::finish(kind, &result)?;
                let signature = if wire_ws {
                    let encoded = output.signature.ok_or("missing batch")?;
                    let batch: Value = serde_json::from_str(&encoded)?;
                    assert!(batch["credential"] == golden()?.public_key);
                    assert!(output.credential.is_none());
                    batch["signature"]
                        .as_str()
                        .ok_or("missing signature")?
                        .to_owned()
                } else {
                    assert!(output.credential.as_deref() == Some(golden()?.public_key.as_str()));
                    output.signature.ok_or("missing signature")?
                };
                assert_eq!(
                    signature, row.signature,
                    "frozen browser golden: {}",
                    row.name
                );
                assert!(signature.ends_with("=="));
                assert!(output.headers.is_none());
            }
        }
    }
    Ok(())
}

#[test]
fn orderly_device_rejects_unknown_fields_foreign_slot_and_hmac_format() -> TestResult {
    let (_, ids) = fixture()?;
    let valid = descriptor("mode", &read()?);
    prepared(&ids, valid.clone())?;
    for (field, value) in [
        ("unknown", json!(true)),
        ("slot", json!("secondary")),
        ("slot", json!("foreign")),
        ("venue", json!("woofi")),
        ("version", json!("orderly/2")),
        ("kind", json!("digest")),
    ] {
        let mut bad = valid.clone();
        bad[field] = value;
        assert!(prepared(&ids, bad).is_err(), "accepted invalid {field}");
    }
    for format in ["hex", "base64", "hex-base64", "bitunix-double-sha256"] {
        let input = serde_json::from_value(
            json!({"kind":"wsAuth", "toSign":valid.to_string(), "outputFormat":format}),
        )?;
        assert!(hosted_signing::prepare("mode", &ids, input).is_err());
    }
    for venue in ["unknown", "binance"] {
        assert!(hosted_signing::prepare(venue, &ids, input(valid.clone(), true)?).is_err());
    }
    for malformed in [
        r#"{"version":"orderly/1","version":"orderly/1","venue":"mode","slot":"primary","kind":"http","stampMs":1700000000123,"payload":"1700000000123GET/v1/orders"}"#,
        r#"["orderly/1","mode","primary","http",1700000000123,"1700000000123GET/v1/orders"]"#,
    ] {
        let input = serde_json::from_value(
            json!({"kind":"wsAuth", "toSign":malformed, "outputFormat":"orderly-ed25519"}),
        )?;
        assert!(hosted_signing::prepare("mode", &ids, input).is_err());
    }
    Ok(())
}

#[test]
fn orderly_device_rejects_malformed_key_signature_and_stamp() -> TestResult {
    let (signer, ids) = fixture()?;
    let valid = descriptor("mode", &read()?);
    let (work, kind) = prepared(&ids, valid.clone())?;
    let signed = signer.sign_now(&work)?;
    hosted_signing::finish(kind, &signed)?;
    let mut hmac_only = LocalSigner::new();
    hmac_only.insert(
        KeyRef("owned-public".into()),
        golden()?.public_key.into_bytes(),
    );
    hmac_only.insert(KeyRef("owned-signing".into()), vec![7; 32]);
    assert!(hmac_only.sign_now(&work).is_err());
    let mut wrong_width = LocalSigner::new();
    wrong_width.insert(
        KeyRef("owned-public".into()),
        golden()?.public_key.into_bytes(),
    );
    wrong_width.insert_declared(
        KeyRef("owned-signing".into()),
        vec![7; 31],
        Algorithm::Ed25519,
    )?;
    assert!(wrong_width.sign_now(&work).is_err());
    for key in [
        "",
        "invalid",
        "ed25519:0OIl",
        "ed25519:111",
        "ed25519:111111111111111111111111111111111",
        "ed25519:11111111111111111111111111111111\n",
    ] {
        let mut bad = signed.clone();
        bad.values[0] = key.as_bytes().to_vec();
        assert!(hosted_signing::finish(kind, &bad).is_err());
    }
    for length in [0, 32, 63, 65] {
        let mut bad = signed.clone();
        bad.values[1] = vec![0; length];
        assert!(hosted_signing::finish(kind, &bad).is_err());
    }
    for length in [0, 1, 3] {
        let bad = SignResult::new(vec![vec![0; 64]; length], signed.receipt);
        assert!(hosted_signing::finish(kind, &bad).is_err());
    }
    let mut with_headers = signed.clone();
    with_headers
        .headers
        .push(("orderly-signature".into(), "foreign".into()));
    assert!(hosted_signing::finish(kind, &with_headers).is_err());
    for value in [
        json!(-1),
        json!(-0.0),
        json!(1.5),
        json!(9_007_199_254_740_992_u64),
        Value::Null,
        json!("1700000000123"),
    ] {
        let mut bad = valid.clone();
        bad["stampMs"] = value;
        assert!(prepared(&ids, bad).is_err());
    }
    let mut wrong_stamp = valid.clone();
    wrong_stamp["stampMs"] = json!(STAMP + 1.);
    assert!(prepared(&ids, wrong_stamp).is_err());
    assert!(
        hosted_signing::prepare(
            "mode",
            &ids,
            input(
                {
                    let mut mismatch = valid.clone();
                    mismatch["stampMs"] = json!(STAMP + 1.);
                    mismatch["payload"] = json!(format!("{:.0}GET/v1/orders", STAMP + 1.));
                    mismatch
                },
                false
            )?
        )
        .is_err()
    );
    for payload in [
        "",
        "1700000000123get/v1/orders",
        "1700000000123GEThttps://foreign.invalid/v1/orders",
    ] {
        let mut bad = valid.clone();
        bad["payload"] = json!(payload);
        assert!(prepared(&ids, bad).is_err());
    }
    let mut wrong_ws = valid;
    wrong_ws["kind"] = json!("wsAuth");
    assert!(prepared(&ids, wrong_ws).is_err());
    Ok(())
}

#[test]
fn orderly_device_never_accepts_server_key_refs() -> TestResult {
    let (_, ids) = fixture()?;
    let valid = descriptor("mode", &read()?);
    prepared(&ids, valid.clone())?;
    for field in [
        "key",
        "signing",
        "credential",
        "identities",
        "accountId",
        "algorithm",
    ] {
        let mut bad = valid.clone();
        bad[field] = json!("foreign-ref");
        assert!(prepared(&ids, bad).is_err());
    }
    assert!(prepared(&Identities::default(), valid.clone()).is_err());
    let missing_signing = Identities {
        primary: Some(Identity::new(KeyRef("owned-public".into()), None)),
        secondary: None,
    };
    assert!(prepared(&missing_signing, valid.clone()).is_err());
    let mut oversized = valid;
    oversized["payload"] = json!("x".repeat(1024 * 1024 + 1));
    assert!(prepared(&ids, oversized).is_err());
    Ok(())
}

#[test]
fn orderly_capture_fences_clock_changes_cancellation_timeout_and_duplicate_completion() -> TestResult
{
    let (signer, ids) = fixture()?;
    let mut capture = Capture::default();
    let (work, kind) = prepared(&ids, descriptor("mode", &read()?))?;
    let pending = capture.prepare(work, kind, 0., STAMP, 0, 5000)?;
    let signed = signer.sign_now(&pending.work)?;
    assert!(
        capture
            .finish(pending.id, 1., STAMP + 1000., 1, Ok(signed))
            .is_err()
    );
    assert!(capture.is_empty());
    let corrected = golden()?
        .vectors
        .into_iter()
        .find(|row| row.name == "corrected-clock")
        .ok_or("missing corrected fixture")?;
    for failure in ["complete", "cancel", "timeout", "refused", "malformed"] {
        let (work, kind) = prepared(&ids, descriptor("mode", &corrected))?;
        let pending = capture.prepare(work, kind, 1., STAMP + 1000., 1, 5000)?;
        let mut signed = signer.sign_now(&pending.work)?;
        if failure == "cancel" {
            assert!(capture.cancel(pending.id));
        }
        if failure == "malformed" {
            signed.values.pop();
        }
        let result = if failure == "refused" {
            Err(SignError::Refused)
        } else {
            Ok(signed)
        };
        let finished = capture.finish(
            pending.id,
            if failure == "timeout" { 5001. } else { 2. },
            STAMP + 1000.,
            1,
            result,
        );
        if failure == "complete" {
            let (kind, signed) = finished?;
            hosted_signing::finish(kind, &signed)?;
        } else {
            assert!(finished.is_err());
        }
        assert!(
            capture
                .finish(pending.id, 3., STAMP + 1000., 1, Err(SignError::Refused))
                .is_err()
        );
        assert!(capture.is_empty());
    }
    Ok(())
}

#[test]
fn orderly_admission_remains_closed_for_every_alias_and_internal_family() -> TestResult {
    let options = serde_json::from_value(json!({}))?;
    for venue in ALIASES.into_iter().chain(["orderly"]) {
        assert!(!hosted_signing::admitted(venue, &options));
        assert!(!safe_cex_venues::remote_signing_policy(venue).enabled);
    }
    Ok(())
}

#[test]
fn orderly_refusal_is_not_sent_and_never_echoes_the_descriptor() -> TestResult {
    let (_, ids) = fixture()?;
    let mut bad = descriptor("mode", &read()?);
    bad["payload"] = json!("private-payload-canary");
    let error = hosted_signing::prepare("mode", &ids, input(bad, true)?)
        .err()
        .ok_or("invalid descriptor was accepted")?;
    let refusal = error.venue_error();
    assert_eq!(refusal.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(!refusal.message.contains("private-payload-canary"));
    Ok(())
}
