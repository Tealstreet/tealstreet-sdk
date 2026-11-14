#![cfg(feature = "coinbase")]
use safe_cex_engine::signing::Operation;
use safe_cex_engine::{Action, Engine, Venue};
use safe_cex_venues::{
    EngineOptions, Visit, build,
    hosted_signing::{self, Input, OutputFormat},
    local_signer_from_capsule,
};

const KEY: &str = "organizations/fixture/apiKeys/fixture";
fn b64(bytes: &[u8]) -> String {
    safe_cex_crypto::encode(bytes, safe_cex_crypto::Encoding::Base64)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}
fn input(uri: Option<&str>) -> String {
    let header = format!(r#"{{"alg":"EdDSA","typ":"JWT","kid":"{KEY}","nonce":"fixture-nonce"}}"#);
    let claims = format!(
        r#"{{"iss":"cdp","aud":["cdp_service"],"nbf":1700000000,"exp":1700000120,"sub":"{KEY}"{}}}"#,
        uri.map_or(String::new(), |uri| format!(",\"uri\":\"{uri}\""))
    );
    format!("{}.{}", b64(header.as_bytes()), b64(claims.as_bytes()))
}
fn setup() -> (
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
    EngineOptions,
) {
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    let json = serde_json::json!({"primary":{"key":KEY,"secret":safe_cex_crypto::encode(&[7;32], safe_cex_crypto::Encoding::Base64)}}).to_string();
    let (provider, ids) = local_signer_from_capsule("coinbase", json, &mut options).unwrap();
    (provider, ids, options)
}
fn auth(payload: String) -> Input {
    Input::WsAuth {
        to_sign: payload,
        output_format: serde_json::from_str::<OutputFormat>("\"coinbase-jws\"").unwrap(),
    }
}
#[test]
fn exact_jws_capture_matches_local_signer_for_rest_and_ws_and_clock_fences() {
    let (provider, ids, _) = setup();
    for venue in ["coinbase", "coinbasespot"] {
        for uri in [
            None,
            Some("GET api.coinbase.com/api/v3/brokerage/accounts"),
            Some("POST api.coinbase.com/api/v3/brokerage/orders"),
            Some("POST drb.coinbase.com/api/v2/public/auth"),
        ] {
            let payload = input(uri);
            let (work, kind) = hosted_signing::prepare(venue, &ids, auth(payload.clone())).unwrap();
            assert!(
                matches!(&work.operations[1], Operation::Sign { payload: parts, .. } if matches!(&parts[0], safe_cex_engine::signing::PayloadPart::Bytes { bytes } if bytes == payload.as_bytes()))
            );
            let local = provider.sign_ready(&work).unwrap().unwrap();
            assert_eq!(
                local.values[1],
                safe_cex_crypto::ed25519::sign(payload.as_bytes(), &[7; 32]).unwrap()
            );
            let mut captures = safe_cex_engine::signing_capture::Capture::default();
            let capture = captures
                .prepare(work.clone(), kind, 0., 1700000000000., 0, 5000)
                .unwrap();
            let result = provider.sign_ready(&capture.work).unwrap().unwrap();
            let (kind, result) = captures
                .finish(capture.id, 1., 1700000000001., 0, Ok(result))
                .unwrap();
            assert_eq!(
                hosted_signing::finish(kind, &result).unwrap().signature,
                Some(b64(&local.values[1]))
            );
            assert!(
                captures
                    .finish(
                        capture.id,
                        1.,
                        1700000000001.,
                        0,
                        provider.sign_ready(&work).unwrap()
                    )
                    .is_err()
            );
            let capture = captures
                .prepare(work.clone(), kind, 0., 1700000000000., 0, 5000)
                .unwrap();
            assert!(
                captures
                    .finish(
                        capture.id,
                        1.,
                        1700000000001.,
                        1,
                        provider.sign_ready(&work).unwrap()
                    )
                    .is_err()
            );
            assert!(
                captures
                    .prepare(work, kind, 0., 1700000120000., 0, 5000)
                    .is_err()
            );
        }
    }
}
#[test]
fn foreign_key_algorithms_and_claim_smuggling_refuse_without_signature() {
    let (provider, ids, _) = setup();
    let foreign = input(None).split_once('.').map(|(_, body)| format!("{}.{}", b64(r#"{"alg":"EdDSA","typ":"JWT","kid":"organizations/other/apiKeys/other","nonce":"fixture-nonce"}"#.as_bytes()), body)).unwrap();
    assert!(hosted_signing::prepare("coinbase", &ids, auth(foreign)).is_err());
    let (mut work, _) = hosted_signing::prepare("coinbase", &ids, auth(input(None))).unwrap();
    if let Operation::Sign { algorithm, .. } = &mut work.operations[1] {
        *algorithm = safe_cex_engine::signing::Algorithm::Es256;
    }
    assert!(provider.sign_ready(&work).unwrap().is_err());
    for bad in [format!("{}.extra", input(None)), "not-a-jws".into()] {
        assert!(hosted_signing::prepare("coinbase", &ids, auth(bad)).is_err());
    }
}
struct Start;
impl Visit for Start {
    type Output = Vec<Action>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Vec<Action> {
        engine.start(1700000000000.);
        safe_cex_engine::test_support::prepared_actions(&mut engine, 1700000000000.)
    }
}
fn assert_public_start(venue: &str, actions: &[Action]) {
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect();
    assert!(!requests.is_empty(), "{venue} must start public discovery");
    for http in requests {
        assert!(!http.url.contains("/portfolios"));
        assert!(
            !http
                .headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        );
        assert!(
            if venue == "coinbasespot" {
                http.url.contains("/market/products")
            } else {
                http.url.contains("drb.coinbase.com")
            },
            "unexpected public URL: {}",
            http.url
        );
    }
}
#[test]
fn keyless_aliases_ignore_stale_signing_material_and_start_publicly() {
    for venue in ["coinbase", "coinbasespot"] {
        for json in [
            "{}",
            r#"{"secret":"not a key","readCredentials":{"key":"stale","secret":"malformed"}}"#,
            r#"{"key":"","secret":"malformed","readCredentials":{"key":"stale","secret":"malformed"}}"#,
        ] {
            let actions = build(
                venue,
                serde_json::from_str(json).unwrap(),
                Some(safe_cex_engine::test_support::seeded_entropy(4)),
                Start,
            )
            .unwrap();
            assert_public_start(venue, &actions);
        }
    }
}
#[test]
fn explicit_public_aliases_ignore_both_raw_credential_slots() {
    for venue in ["coinbase", "coinbasespot"] {
        let options = serde_json::from_str(r#"{"publicDataOnly":true,"key":"stale","secret":"malformed","readCredentials":{"key":"stale","secret":"malformed"}}"#).unwrap();
        assert_public_start(
            venue,
            &build(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(4)),
                Start,
            )
            .unwrap(),
        );
    }
}
#[test]
fn public_and_keyless_capsules_ignore_unused_credential_slots() {
    for venue in ["coinbase", "coinbasespot"] {
        for (options, capsule) in [
            (
                r#"{"publicDataOnly":true}"#,
                r#"{"primary":{"key":"stale","secret":"malformed"},"secondary":{"key":"stale","secret":"malformed"}}"#,
            ),
            (
                "{}",
                r#"{"secondary":{"key":"stale","secret":"malformed"}}"#,
            ),
            (
                "{}",
                r#"{"primary":{"key":"","secret":"malformed"},"secondary":{"key":"stale","secret":"malformed"}}"#,
            ),
        ] {
            let mut options: EngineOptions = serde_json::from_str(options).unwrap();
            let (provider, ids) =
                local_signer_from_capsule(venue, capsule.into(), &mut options).unwrap();
            assert!(ids.primary.is_none() && ids.secondary.is_none());
            assert!(options.coinbase_signing.is_none() && options.coinbase_read_signing.is_none());
            options.signing = Some(ids);
            let actions = safe_cex_venues::build_with_signer(
                venue,
                options,
                Some(safe_cex_engine::test_support::seeded_entropy(4)),
                Some(provider),
                Start,
            )
            .unwrap();
            assert_public_start(venue, &actions);
        }
    }
}
#[test]
fn private_authority_still_requires_valid_material_and_a_bound_provider() {
    for venue in ["coinbase", "coinbasespot"] {
        let malformed = serde_json::from_str(r#"{"key":"stale","secret":"malformed"}"#).unwrap();
        assert!(
            build(
                venue,
                malformed,
                Some(safe_cex_engine::test_support::seeded_entropy(4)),
                Start
            )
            .is_err()
        );
        let (_, ids, mut options) = setup();
        options.signing = Some(ids);
        let error = safe_cex_venues::build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(4)),
            None,
            Start,
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "Coinbase requires a bound JWT signing provider"
        );
        let (provider, ids, mut options) = setup();
        options.signing = Some(ids);
        options.coinbase_signing = None;
        let error = safe_cex_venues::build_with_signer(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(4)),
            Some(provider),
            Start,
        )
        .unwrap_err();
        assert_eq!(
            error.message,
            "Coinbase requires a bound JWT signing provider"
        );
    }
}
#[test]
fn aliases_construct_with_owned_local_keys_and_payload_mode_refuses_before_io() {
    for venue in ["coinbase", "coinbasespot"] {
        let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
        options.key = Some(KEY.into());
        options.secret = Some(safe_cex_crypto::encode(
            &[7; 32],
            safe_cex_crypto::Encoding::Base64,
        ));
        let actions = build(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(4)),
            Start,
        )
        .unwrap();
        assert!(actions.iter().any(|action| matches!(action, Action::Http(http) if http.url.contains("/portfolios") && http.headers.iter().any(|(name, value)| *name == "Authorization" && value.starts_with("Bearer ")))));
        let options: EngineOptions =
            serde_json::from_str(&format!(r#"{{"key":"{KEY}","payloadSigner":true}}"#)).unwrap();
        let error = build(
            venue,
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(4)),
            Start,
        )
        .unwrap_err();
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert_eq!(
            error.message,
            "Delegated signing requires a configured callback provider"
        );
    }
}

#[test]
fn provider_refusal_never_releases_a_private_startup_packet() {
    use safe_cex_engine::signing::{
        Algorithm, Capability, KeyRef, SignError, SignFuture, SignRequest, SignResult, Signer,
    };
    struct Refuse;
    impl Signer for Refuse {
        fn immediate(&self) -> bool {
            true
        }
        fn sign(&self, _: SignRequest) -> SignFuture {
            Box::pin(std::future::ready(Err(SignError::Unavailable)))
        }
        fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
            Some(Err(SignError::Unavailable))
        }
        fn can_sign(&self, _: &KeyRef, _: Algorithm) -> Capability {
            Capability::Supported
        }
    }
    let (_, ids, mut options) = setup();
    options.signing = Some(ids);
    let actions = safe_cex_venues::build_with_signer(
        "coinbase",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(4)),
        Some(std::sync::Arc::new(Refuse)),
        Start,
    )
    .unwrap();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(_))));
}
