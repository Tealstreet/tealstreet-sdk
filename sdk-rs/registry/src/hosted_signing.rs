//! Compatibility inputs contain canonical bytes, never caller-chosen key refs.
//! The SDK retains its existing serializers; family recipes build atomic work.
use safe_cex_engine::signing::{
    Context, Identities, Identity, Operation, SignError, SignReceipt, SignRequest, SignResult,
};
use safe_cex_engine::signing_policy::{RemoteSigningPolicy, SignatureEncoding};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
pub enum OutputFormat {
    #[serde(rename = "hex")]
    Hex,
    #[serde(rename = "base64")]
    Base64,
    #[serde(rename = "hex-base64")]
    Base64Hex,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Input {
    Credential {},
    Http {
        payload: String,
        stamp_ms: f64,
        revision: String,
    },
    WsAuth {
        to_sign: String,
        output_format: OutputFormat,
    },
}

/// Formatting metadata remains bound to its one capture token.
#[derive(Clone, Copy)]
pub enum OutputKind {
    Credential,
    Http(RemoteSigningPolicy),
    WsAuth(OutputFormat),
}

impl OutputKind {
    pub fn header_policy(self) -> safe_cex_engine::signing_headers::ResultHeaderPolicy {
        match self {
            Self::Http(policy) => policy.header_policy(),
            _ => Default::default(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<Vec<(String, String)>>,
}

/// Both the layout and presentation policy are captured with the canonical work.
/// This helper does not admit a venue or resolve setup's absent-only fallback.
fn http_work(
    identity: &Identity,
    payload: String,
    context: Context,
    receipt: SignReceipt,
    policy: RemoteSigningPolicy,
) -> Result<(SignRequest, OutputKind), SignError> {
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    receipt.validate()?;
    let mut work = safe_cex_engine::signing::hmac_sha256_work(
        identity,
        vec![safe_cex_engine::signing::PayloadPart::Bytes {
            bytes: payload.into_bytes(),
        }],
        context,
        receipt,
    );
    if policy.passphrase_header.is_some() {
        work.operations.push(Operation::Present {
            key: identity.passphrase.clone().ok_or(SignError::Unavailable)?,
        });
    }
    // The existing capture admission owns canonical-work bounds before any
    // provider call; descriptor construction preserves that rejection boundary.
    Ok((work, OutputKind::Http(policy)))
}

/// Families with a hosted request codec. AsterDEX V3 signs with a wallet key,
/// which must never become an HMAC key. Only an explicit setup-resolved V1
/// qualifies: inference from options cannot see material held by the setup.
pub fn admitted(venue: &str, options: &crate::EngineOptions) -> bool {
    let _ = options;
    match venue {
        #[cfg(feature = "asterdex")]
        "asterdex" => {
            options.asterdex.api_version == Some(safe_cex_venue_asterdex::sign::Version::V1)
        }
        _ => family(venue),
    }
}

fn family(venue: &str) -> bool {
    let binance = cfg!(feature = "binance")
        && matches!(
            venue,
            "binance" | "binancepm" | "binanceinverse" | "binancespot"
        );
    let bybit = cfg!(feature = "bybit")
        && matches!(
            venue,
            "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader"
        );
    let okx = cfg!(feature = "okx") && matches!(venue, "okx" | "okxeu" | "okxdemo" | "okxeudemo");
    binance || bybit || okx || (cfg!(feature = "asterdex") && venue == "asterdex")
}

pub fn prepare(
    venue: &str,
    identities: &Identities,
    input: Input,
) -> Result<(SignRequest, OutputKind), SignError> {
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let context = Context {
        venue: venue.into(),
        method: "HOSTED".into(),
        ..Default::default()
    };
    if !family(venue) {
        return Err(SignError::Unsupported);
    }
    match input {
        Input::Credential {} => Ok((
            SignRequest {
                receipt: SignReceipt::default(),
                context,
                operations: vec![Operation::Present {
                    key: identity.credential.clone(),
                }],
            },
            OutputKind::Credential,
        )),
        Input::Http {
            payload,
            stamp_ms,
            revision: _,
        } => {
            let receipt = SignReceipt {
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
            };
            http_work(
                identity,
                payload,
                context,
                receipt,
                crate::remote_signing_policy(venue),
            )
        }
        Input::WsAuth {
            to_sign,
            output_format,
        } => {
            // AsterDEX authenticates its private socket with a listen key.
            if to_sign.is_empty() || venue == "asterdex" {
                return Err(SignError::InvalidRequest);
            }
            Ok((
                safe_cex_engine::signing::hmac_sha256_work(
                    identity,
                    vec![safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: to_sign.into_bytes(),
                    }],
                    context,
                    SignReceipt::default(),
                ),
                OutputKind::WsAuth(output_format),
            ))
        }
    }
}

pub fn finish(kind: OutputKind, result: &SignResult) -> Result<Output, SignError> {
    let expected = match kind {
        OutputKind::Credential => 1,
        OutputKind::Http(policy) if policy.passphrase_header.is_some() => 3,
        _ => 2,
    };
    if result.values.len() != expected {
        return Err(SignError::InvalidResult);
    }
    let mut headers = Vec::new();
    if let OutputKind::Http(policy) = kind
        && let Some(header) = policy.passphrase_header
    {
        let value = std::str::from_utf8(&result.values[2]).map_err(|_| SignError::InvalidResult)?;
        if value.is_empty() {
            return Err(SignError::InvalidResult);
        }
        headers.push((header.into(), value.into()));
        safe_cex_engine::signing_headers::validate(&headers)?;
    }
    // Trusted native presentation precedes the same declared callback precedence
    // used by dispatch. Case aliases cannot bypass a protected header name.
    kind.header_policy().apply(&mut headers, &result.headers)?;
    safe_cex_engine::signing_headers::validate(&headers)?;
    let key = result.values.first().ok_or(SignError::InvalidResult)?;
    let key = std::str::from_utf8(key).map_err(|_| SignError::InvalidResult)?;
    if key.is_empty() || key.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    if matches!(kind, OutputKind::Credential) {
        return Ok(Output {
            credential: Some(key.into()),
            signature: None,
            headers: None,
        });
    }
    let mac = result
        .values
        .get(1)
        .filter(|mac| mac.len() == 32)
        .ok_or(SignError::InvalidResult)?;
    let signature = match kind {
        OutputKind::Credential => unreachable!(),
        OutputKind::Http(policy) => match policy.signature_encoding {
            SignatureEncoding::Hex => safe_cex_crypto::hex(mac),
            SignatureEncoding::Base64 => {
                safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64)
            }
        },
        OutputKind::WsAuth(OutputFormat::Hex) => safe_cex_crypto::hex(mac),
        OutputKind::WsAuth(OutputFormat::Base64) => {
            safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64)
        }
        OutputKind::WsAuth(OutputFormat::Base64Hex) => safe_cex_crypto::encode(
            safe_cex_crypto::hex(mac).as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        ),
    };
    Ok(Output {
        credential: matches!(kind, OutputKind::Http(_)).then(|| key.into()),
        signature: Some(signature),
        headers: (!headers.is_empty()).then_some(headers),
    })
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{KeyRef, SetupFieldPolicy};
    use safe_cex_engine::signing_headers::MAX_VALUE_BYTES;

    const PAYLOAD: &str = "1709209849123GET/api/account?x=%2B&x=second";
    const MAC: &str = "SL/xd+lRj5sirx9sxjcCAiy6l78Q3WwR8844XzqxcNs=";

    fn fixture() -> (Identity, LocalSigner) {
        let identity = Identity {
            credential: KeyRef("ref:owned-primary-public".into()),
            signing: KeyRef("ref:owned-primary-signing".into()),
            passphrase: Some(KeyRef("ref:owned-primary-presentation".into())),
            stable_id: None,
            extra: Default::default(),
        };
        let mut signer = LocalSigner::new();
        signer.insert(identity.credential.clone(), b"public-identity".to_vec());
        signer.insert(identity.signing.clone(), b"fixture-only-provider".to_vec());
        signer.insert(
            identity.passphrase.clone().unwrap(),
            b"fixture-presentation".to_vec(),
        );
        (identity, signer)
    }

    fn prepared(identity: &Identity, policy: RemoteSigningPolicy) -> (SignRequest, OutputKind) {
        http_work(
            identity,
            PAYLOAD.into(),
            Context::default(),
            SignReceipt::default(),
            policy,
        )
        .unwrap()
    }

    fn families() -> [(&'static str, &'static [&'static str]); 2] {
        [
            ("okx", &["OK-ACCESS-PASSPHRASE"]),
            ("bitget", &["ACCESS-PASSPHRASE"]),
        ]
    }

    #[test]
    fn two_families_use_one_owned_atomic_work_and_formatter_with_explicit_admission() {
        let (identity, signer) = fixture();
        for (venue, names) in families() {
            let policy = crate::remote_signing_policy(venue);
            assert_eq!(policy.enabled, venue == "okx");
            assert_eq!(policy.signature_encoding, SignatureEncoding::Base64);
            assert_eq!(policy.passphrase_header, Some(names[0]));
            let (work, kind) = prepared(&identity, policy);
            assert_eq!(work.operations.len(), 3);
            assert!(
                matches!(&work.operations[2], Operation::Present { key } if Some(key) == identity.passphrase.as_ref())
            );
            let result = signer.sign_now(&work).unwrap();
            work.validate_result(&result).unwrap();
            let output = finish(kind, &result).unwrap();
            // Independent Python stdlib HMAC vector, not the formatter's oracle.
            assert_eq!(output.signature.as_deref(), Some(MAC));
            assert_eq!(output.credential.as_deref(), Some("public-identity"));
            assert_eq!(
                output.headers.unwrap(),
                vec![(names[0].into(), "fixture-presentation".into())]
            );
            let capture = prepare(
                venue,
                &Identities {
                    primary: Some(identity.clone()),
                    secondary: None,
                },
                Input::Credential {},
            );
            if venue == "okx" && cfg!(feature = "okx") {
                assert!(capture.is_ok());
            } else {
                assert!(matches!(capture, Err(SignError::Unsupported)));
            }
        }
    }

    #[test]
    fn missing_explicit_refs_do_not_fall_back_but_setup_resolved_aliases_work() {
        let (identity, mut signer) = fixture();
        let secondary = Identity {
            credential: KeyRef("ref:owned-secondary-public".into()),
            signing: KeyRef("ref:owned-secondary-signing".into()),
            passphrase: None,
            stable_id: None,
            extra: Default::default(),
        };
        signer.insert(secondary.credential.clone(), b"secondary-public".to_vec());
        signer.insert(
            secondary.signing.clone(),
            b"secondary-fixture-provider".to_vec(),
        );
        for (venue, _) in families() {
            let policy = crate::remote_signing_policy(venue);
            for missing in [None, Some(KeyRef(String::new()))] {
                let bad = Identity {
                    passphrase: missing,
                    ..identity.clone()
                };
                assert!(
                    http_work(
                        &bad,
                        PAYLOAD.into(),
                        Context::default(),
                        SignReceipt::default(),
                        policy
                    )
                    .is_err()
                );
            }
            let mut ids = Identities {
                primary: Some(identity.clone()),
                secondary: Some(secondary.clone()),
            };
            safe_cex_engine::signing::CredentialSetupPolicy {
                secondary: Some(safe_cex_engine::signing::IdentitySetupPolicy {
                    passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
                    ..Default::default()
                }),
                ..Default::default()
            }
            .apply_to_identities(&mut ids, false)
            .unwrap();
            let (work, kind) = prepared(ids.secondary.as_ref().unwrap(), policy);
            let result = signer.sign_now(&work).unwrap();
            let output = finish(kind, &result).unwrap();
            assert_eq!(output.credential.as_deref(), Some("secondary-public"));
            assert_eq!(output.headers.unwrap()[0].1, "fixture-presentation");
        }
    }

    #[test]
    fn malformed_presentations_or_batch_shapes_never_publish_partial_output() {
        let (identity, signer) = fixture();
        for (venue, _) in families() {
            let (work, kind) = prepared(&identity, crate::remote_signing_policy(venue));
            for invalid in [
                vec![],
                vec![0xff],
                b"injected\r\nheader".to_vec(),
                vec![0; 3],
                vec![b'x'; MAX_VALUE_BYTES + 1],
            ] {
                let mut result = signer.sign_now(&work).unwrap();
                result.values[2] = invalid;
                assert!(matches!(
                    finish(kind, &result),
                    Err(SignError::InvalidResult)
                ));
            }
            for count in [0, 1, 2, 4] {
                let result = SignResult::new(vec![vec![0x37; 32]; count], work.receipt);
                assert!(matches!(
                    finish(kind, &result),
                    Err(SignError::InvalidResult)
                ));
            }
            let mut bad_mac = signer.sign_now(&work).unwrap();
            bad_mac.values[1].pop();
            assert!(matches!(
                finish(kind, &bad_mac),
                Err(SignError::InvalidResult)
            ));
        }
    }

    #[test]
    fn declared_case_insensitive_precedence_and_combined_bounds_are_atomic() {
        let (identity, signer) = fixture();
        for (venue, protected) in families() {
            let base = crate::remote_signing_policy(venue);
            let policy = RemoteSigningPolicy {
                protected_headers: Some(protected),
                ..base
            };
            let (work, kind) = prepared(&identity, policy);
            let mut result = signer.sign_now(&work).unwrap();
            result.headers = vec![(protected[0].to_lowercase(), "callback".into())];
            assert_eq!(
                finish(kind, &result).unwrap().headers.unwrap(),
                vec![(protected[0].into(), "fixture-presentation".into())]
            );
            let (_, override_kind) = prepared(
                &identity,
                RemoteSigningPolicy {
                    protected_headers: Some(&[]),
                    ..base
                },
            );
            assert_eq!(
                finish(override_kind, &result).unwrap().headers.unwrap(),
                vec![(protected[0].into(), "callback".into())]
            );
            assert!(matches!(
                finish(
                    OutputKind::Http(RemoteSigningPolicy {
                        protected_headers: None,
                        ..base
                    }),
                    &result
                ),
                Err(SignError::InvalidResult)
            ));
            result.headers = (0..32)
                .map(|i| (format!("X-Callback-{i}"), "v".into()))
                .collect();
            assert!(matches!(
                finish(kind, &result),
                Err(SignError::InvalidResult)
            ));
        }
    }

    #[test]
    fn presentation_policy_stays_in_the_capture_and_stale_results_cannot_publish() {
        use safe_cex_engine::signing_capture::Capture;
        let (identity, signer) = fixture();
        for (venue, _) in families() {
            for (now, revision) in [(600., 7), (101., 8)] {
                let (work, kind) = prepared(&identity, crate::remote_signing_policy(venue));
                let mut captures = Capture::default();
                let pending = captures
                    .prepare_with_policy((work, kind.header_policy()), kind, 100., 1_000., 7, 500)
                    .unwrap();
                let result = signer.sign_now(&pending.work).unwrap();
                assert!(
                    captures
                        .finish(pending.id, now, 1_001., revision, Ok(result))
                        .is_err()
                );
                assert!(captures.is_empty());
            }
            let mut policy = crate::remote_signing_policy(venue);
            let (work, kind) = prepared(&identity, policy);
            let mut captures = Capture::default();
            let pending = captures
                .prepare_with_policy((work, kind.header_policy()), kind, 100., 1_000., 7, 500)
                .unwrap();
            policy.signature_encoding = SignatureEncoding::Hex;
            assert_eq!(policy.signature_encoding, SignatureEncoding::Hex);
            let result = signer.sign_now(&pending.work).unwrap();
            let (captured, result) = captures
                .finish(pending.id, 101., 1_001., 7, Ok(result))
                .unwrap();
            assert_eq!(
                finish(captured, &result).unwrap().signature.as_deref(),
                Some(MAC)
            );
            assert!(captures.is_empty());
        }
    }
}
