#![cfg(all(
    feature = "dispatch-signing",
    any(feature = "binance", feature = "bybit")
))]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Identities, Identity, KeyRef, Operation, PayloadPart, SignError, SignReceipt,
    SignResult,
};
use safe_cex_engine::signing_capture::Capture;
use safe_cex_venues::hosted_signing::{self, Input, Output, OutputFormat};

const STAMP: f64 = 1_709_209_849_123.;
const QUERY: &str =
    "symbol=BTCUSDT&timestamp=1709209849123&recvWindow=05000&memo=%C3%A9%2B%25&x=first&x=second";
const BYBIT_QUERY: &str = "1709209849123hosted-primary05000symbol=BTCUSDT&note=%C3%A9%2B%25";
const BODY: &str =
    "1709209849123hosted-primary20000{\"symbol\":\"BTCUSDT\",\"qty\":\"0.0100\",\"memo\":\"é/+%\"}";
const ORIGINAL_WS: &str = "GET/realtime1709209800000";

fn families() -> Vec<&'static str> {
    let mut names = Vec::new();
    #[cfg(feature = "binance")]
    names.extend(["binance", "binancepm", "binanceinverse", "binancespot"]);
    #[cfg(feature = "bybit")]
    names.extend([
        "bybit",
        "bybitinverse",
        "bybitspot",
        "bybitdemo",
        "hyrotrader",
    ]);
    names
}

fn fixture() -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    let identity = |slot| Identity {
        credential: KeyRef(format!("owned/{slot}/presentation")),
        signing: KeyRef(format!("owned/{slot}/mac")),
        passphrase: None,
        stable_id: Some(format!("fixture/{slot}")),
        extra: Default::default(),
    };
    let primary = identity("primary");
    let secondary = identity("read");
    signer.insert(primary.credential.clone(), b"hosted-primary".to_vec());
    signer.insert(
        primary.signing.clone(),
        b"fixture-only-primary-secret".to_vec(),
    );
    signer.insert(secondary.credential.clone(), b"hosted-read".to_vec());
    signer.insert(
        secondary.signing.clone(),
        b"fixture-only-read-secret".to_vec(),
    );
    (
        signer,
        Identities {
            primary: Some(primary),
            secondary: Some(secondary),
        },
    )
}

fn http(payload: &str) -> Input {
    Input::Http {
        payload: payload.into(),
        stamp_ms: STAMP,
        revision: "7".into(),
    }
}

fn capture(signer: &LocalSigner, ids: &Identities, venue: &str, input: Input) -> Output {
    let (work, kind) = hosted_signing::prepare(venue, ids, input).unwrap();
    let mut captures = Capture::default();
    let prepared = captures.prepare(work, kind, 100., STAMP, 7, 5_000).unwrap();
    let result = signer.sign_now(&prepared.work).unwrap();
    let (kind, result) = captures
        .finish(prepared.id, 101., STAMP + 1., 7, Ok(result))
        .unwrap();
    let output = hosted_signing::finish(kind, &result).unwrap();
    // Result ownership stays local until formatting; Drop wipes its buffers.
    // This is not a physical-memory zeroization measurement.
    drop(result);
    assert!(captures.is_empty());
    assert!(captures.work(prepared.id).is_none());
    output
}

#[test]
fn every_compiled_alias_uses_the_owned_primary_refs_for_exact_complete_http_preimages() {
    let (signer, ids) = fixture();
    // Independent Python stdlib HMAC-SHA256 vectors; no shared implementation oracle.
    let vectors = [
        (
            QUERY,
            "bb470a87cb02a5d4ef68700a516641a9862892ab7ed8025860c5a8f007a76a69",
        ),
        (
            BYBIT_QUERY,
            "acef1429594ec26b3953662f9e40388f043c2ceb5d03ba64f682a0786a34beef",
        ),
        (
            BODY,
            "295c09f9eab4675234c00f9fd907ce56e07895fd6a9814306e86091874e0879f",
        ),
    ];
    for venue in families() {
        for (payload, expected) in vectors {
            let (work, _) = hosted_signing::prepare(venue, &ids, http(payload)).unwrap();
            assert_eq!(
                work.receipt,
                SignReceipt {
                    nonce: None,
                    stamp_ms: Some(STAMP),
                    expiry_ms: None
                }
            );
            assert_eq!(work.context.venue, venue);
            assert_eq!(
                work.operations,
                vec![
                    Operation::Present {
                        key: ids.primary.as_ref().unwrap().credential.clone()
                    },
                    Operation::Sign {
                        algorithm: Algorithm::HmacSha256,
                        key: ids.primary.as_ref().unwrap().signing.clone(),
                        payload: vec![PayloadPart::Bytes {
                            bytes: payload.as_bytes().to_vec()
                        }],
                    },
                ]
            );
            let output = capture(&signer, &ids, venue, http(payload));
            assert_eq!(output.credential.as_deref(), Some("hosted-primary"));
            assert_eq!(output.signature.as_deref(), Some(expected));
        }
        let output = capture(&signer, &ids, venue, Input::Credential {});
        assert_eq!(output.credential.as_deref(), Some("hosted-primary"));
        assert!(output.signature.is_none());
    }
}

#[test]
fn original_ws_bytes_and_expiry_survive_all_output_encodings_without_clock_restamping() {
    let (signer, ids) = fixture();
    for venue in families() {
        for (format, expected) in [
            (
                OutputFormat::Hex,
                "5f8a6451fd863f6eb88a65b577547b4e7fbce7e02e2171653562289876678541",
            ),
            (
                OutputFormat::Base64,
                "X4pkUf2GP264imW1d1R7Tn+85+AuIXFlNWIomHZnhUE=",
            ),
            (
                OutputFormat::Base64Hex,
                "NWY4YTY0NTFmZDg2M2Y2ZWI4OGE2NWI1Nzc1NDdiNGU3ZmJjZTdlMDJlMjE3MTY1MzU2MjI4OTg3NjY3ODU0MQ==",
            ),
        ] {
            let input = || Input::WsAuth {
                to_sign: ORIGINAL_WS.into(),
                output_format: format,
            };
            let (work, _) = hosted_signing::prepare(venue, &ids, input()).unwrap();
            // The caller's original expiry is older than our STAMP. It is part
            // of exact preimage bytes, not permission to invent a new expiry.
            assert_eq!(work.receipt, SignReceipt::default());
            let output = capture(&signer, &ids, venue, input());
            assert_eq!(output.signature.as_deref(), Some(expected));
            assert!(output.credential.is_none());
        }
    }
}

#[test]
fn no_primary_never_selects_secondary_and_removed_material_fails_atomically() {
    let (mut signer, mut ids) = fixture();
    let venue = families()[0];
    let primary = ids.primary.take().unwrap();
    assert!(matches!(
        hosted_signing::prepare(venue, &ids, http(QUERY)),
        Err(SignError::Unavailable)
    ));
    ids.primary = Some(primary.clone());
    let (work, kind) = hosted_signing::prepare(venue, &ids, http(QUERY)).unwrap();
    let mut captures = Capture::default();
    let prepared = captures.prepare(work, kind, 100., STAMP, 7, 500).unwrap();
    // Revocation removes the provider-owned signing buffer. It must not fall
    // back to the still-available secondary secret or publish partial Present.
    signer.remove(&primary.signing);
    let failed = signer.sign_now(&prepared.work);
    assert!(matches!(failed, Err(SignError::MissingKey)));
    assert!(matches!(
        captures.finish(prepared.id, 101., STAMP + 1., 7, failed),
        Err(SignError::MissingKey)
    ));
    assert!(captures.is_empty());
    let (read, _) = hosted_signing::prepare(venue, &ids, Input::Credential {}).unwrap();
    assert_eq!(
        signer.sign_now(&read).unwrap().values,
        vec![b"hosted-primary".to_vec()]
    );
    signer.remove(&primary.credential);
    assert!(matches!(signer.sign_now(&read), Err(SignError::MissingKey)));
}

#[test]
fn closed_schema_rejects_caller_refs_secret_exports_and_unknown_families() {
    let (_, ids) = fixture();
    for input in [
        r#"{"kind":"wsAuth","toSign":"x","outputFormat":"hex","key":"owned/read/mac"}"#,
        r#"{"kind":"wsAuth","toSign":"x","outputFormat":"raw"}"#,
        r#"{"kind":"wsAuth","toSign":"x","outputFormat":"apexomni-base64"}"#,
        r#"{"kind":"credential","key":"ref:owned-primary"}"#,
        r#"{"kind":"present","key":"ref:owned-primary"}"#,
        r#"{"kind":"http","payload":"x","stampMs":1000,"revision":"7","secondary":true}"#,
        r#"{"kind":"http","payload":"x","stampMs":1000,"revision":7}"#,
    ] {
        assert!(serde_json::from_str::<Input>(input).is_err());
    }
    for venue in [
        "bitunix",
        "okx-unknown",
        "binance-unknown",
        "bybit-unknown",
        "",
    ] {
        assert!(matches!(
            hosted_signing::prepare(venue, &ids, http(QUERY)),
            Err(SignError::Unsupported)
        ));
    }
    let venue = families()[0];
    assert!(matches!(
        hosted_signing::prepare(
            venue,
            &ids,
            Input::WsAuth {
                to_sign: String::new(),
                output_format: OutputFormat::Hex
            }
        ),
        Err(SignError::InvalidRequest)
    ));
}

#[test]
fn trusted_capture_bounds_receipt_and_single_use_checks_apply_before_output_formatting() {
    let (signer, ids) = fixture();
    let venue = families()[0];
    let mut captures = Capture::default();
    for stamp in [f64::NAN, f64::INFINITY, -1., 1_000.5] {
        assert!(matches!(
            hosted_signing::prepare(
                venue,
                &ids,
                Input::Http {
                    payload: QUERY.into(),
                    stamp_ms: stamp,
                    revision: "7".into()
                }
            ),
            Err(SignError::InvalidRequest)
        ));
    }
    let (work, kind) = hosted_signing::prepare(venue, &ids, http(&"x".repeat(1_048_577))).unwrap();
    assert!(matches!(
        captures.prepare(work, kind, 100., STAMP, 7, 500),
        Err(SignError::InvalidRequest)
    ));
    for malformed in [
        SignResult::new(
            vec![b"hosted-primary".to_vec()],
            SignReceipt {
                nonce: None,
                stamp_ms: Some(STAMP),
                expiry_ms: None,
            },
        ),
        SignResult::new(
            vec![b"hosted-primary".to_vec(), vec![1; 31]],
            SignReceipt {
                nonce: None,
                stamp_ms: Some(STAMP),
                expiry_ms: None,
            },
        ),
        SignResult::new(
            vec![b"hosted-primary".to_vec(), vec![1; 32]],
            SignReceipt {
                nonce: None,
                stamp_ms: Some(STAMP + 1.),
                expiry_ms: None,
            },
        ),
    ] {
        let (work, kind) = hosted_signing::prepare(venue, &ids, http(QUERY)).unwrap();
        let prepared = captures.prepare(work, kind, 100., STAMP, 7, 500).unwrap();
        assert!(matches!(
            captures.finish(prepared.id, 101., STAMP + 1., 7, Ok(malformed)),
            Err(SignError::InvalidResult)
        ));
        assert!(captures.work(prepared.id).is_none());
    }
    let (work, kind) = hosted_signing::prepare(venue, &ids, http(QUERY)).unwrap();
    let prepared = captures.prepare(work, kind, 100., STAMP, 7, 500).unwrap();
    let signed = signer.sign_now(&prepared.work).unwrap();
    assert!(matches!(
        captures.finish(prepared.id, 101., STAMP + 1., 8, Ok(signed)),
        Err(SignError::InvalidRequest)
    ));
    assert!(captures.is_empty());
}

#[test]
fn malformed_presented_credentials_do_not_escape_the_trusted_formatter() {
    let (_, ids) = fixture();
    let venue = families()[0];
    for key in [vec![], vec![0xff], b"bad\r\nheader".to_vec()] {
        let (work, kind) = hosted_signing::prepare(venue, &ids, http(QUERY)).unwrap();
        let mut captures = Capture::default();
        let prepared = captures.prepare(work, kind, 100., STAMP, 7, 500).unwrap();
        let result = SignResult::new(vec![key, vec![1; 32]], prepared.work.receipt);
        // Structural validation cannot decide HTTP header semantics. The trusted
        // formatter rejects invalid UTF-8, empty and CR/LF before public output.
        let (kind, result) = captures
            .finish(prepared.id, 101., STAMP + 1., 7, Ok(result))
            .unwrap();
        assert!(matches!(
            hosted_signing::finish(kind, &result),
            Err(SignError::InvalidResult)
        ));
        assert!(captures.is_empty());
    }
}

#[test]
fn http_callback_headers_use_declared_policy_and_never_escape_ws_or_credential_capture() {
    let (signer, ids) = fixture();
    for venue in families() {
        let (work, kind) = hosted_signing::prepare(venue, &ids, http(QUERY)).unwrap();
        let mut captures = Capture::default();
        let prepared = captures
            .prepare_with_policy((work, kind.header_policy()), kind, 100., STAMP, 7, 5000)
            .unwrap();
        let mut signed = signer.sign_now(&prepared.work).unwrap();
        signed.headers = vec![
            ("X-Proof".into(), "attempt".into()),
            ("x-bapi-sign".into(), "replace-if-binance".into()),
            ("x-bapi-recv-window".into(), "1000".into()),
        ];
        let (kind, signed) = captures
            .finish(prepared.id, 101., STAMP + 1., 7, Ok(signed))
            .unwrap();
        let output = hosted_signing::finish(kind, &signed).unwrap();
        let headers = output.headers.unwrap();
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "X-Proof" && value == "attempt")
        );
        assert_eq!(
            headers.iter().any(|(name, _)| name == "x-bapi-sign"),
            venue.starts_with("binance")
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "x-bapi-recv-window" && value == "1000")
        );
        for input in [
            Input::Credential {},
            Input::WsAuth {
                to_sign: ORIGINAL_WS.into(),
                output_format: OutputFormat::Hex,
            },
        ] {
            let (work, kind) = hosted_signing::prepare(venue, &ids, input).unwrap();
            let prepared = captures
                .prepare_with_policy((work, kind.header_policy()), kind, 100., STAMP, 7, 5000)
                .unwrap();
            let mut signed = signer.sign_now(&prepared.work).unwrap();
            signed.headers = vec![("X-Proof".into(), "must-reject".into())];
            assert!(
                captures
                    .finish(prepared.id, 101., STAMP + 1., 7, Ok(signed))
                    .is_err()
            );
            assert!(captures.is_empty());
        }
    }
}
