//! R4: browser-hosted AsterDEX V1 signing uses the owned primary refs; V3
//! wallet accounts are never admitted, so a wallet key never becomes an HMAC key.
#![cfg(feature = "asterdex")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_engine::signing_capture::Capture;
use safe_cex_venues::EngineOptions;
use safe_cex_venues::hosted_signing::{self, Input, Output};

const STAMP: f64 = 1_709_209_849_123.;
const QUERY: &str = "symbol=BTCUSDT&recvWindow=20000&timestamp=1709209849123";
// Python hmac/hashlib over QUERY with the fixture secret, not the provider.
const QUERY_MAC: &str = "4c6d3d8130f58f899d84bf0f47f1277e0696ccf38256e0ab48ca3b3ba0232704";

fn fixture() -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    let primary = Identity {
        credential: KeyRef("owned/primary/presentation".into()),
        signing: KeyRef("owned/primary/mac".into()),
        passphrase: None,
        stable_id: None,
        extra: Default::default(),
    };
    signer.insert(primary.credential.clone(), b"hosted-aster-key".to_vec());
    signer.insert(primary.signing.clone(), b"fixture-only-v1-secret".to_vec());
    (
        signer,
        Identities {
            primary: Some(primary),
            secondary: None,
        },
    )
}

fn capture(signer: &LocalSigner, ids: &Identities, input: Input) -> Output {
    let (work, kind) = hosted_signing::prepare("asterdex", ids, input).unwrap();
    let mut captures = Capture::default();
    let prepared = captures.prepare(work, kind, 100., STAMP, 7, 5_000).unwrap();
    let result = signer.sign_now(&prepared.work).unwrap();
    let (kind, result) = captures
        .finish(prepared.id, 101., STAMP + 1., 7, Ok(result))
        .unwrap();
    hosted_signing::finish(kind, &result).unwrap()
}

#[test]
fn v1_hosted_http_signs_the_exact_query_with_the_owned_primary() {
    let (signer, ids) = fixture();
    let credential = capture(&signer, &ids, Input::Credential {});
    assert_eq!(credential.credential.as_deref(), Some("hosted-aster-key"));
    let output = capture(
        &signer,
        &ids,
        Input::Http {
            payload: QUERY.into(),
            stamp_ms: STAMP,
            revision: "7".into(),
        },
    );
    assert_eq!(output.credential.as_deref(), Some("hosted-aster-key"));
    assert_eq!(output.signature.as_deref(), Some(QUERY_MAC));
}

#[test]
fn only_resolved_v1_accounts_are_admitted_to_hosted_signing() {
    let admitted = |json: &str| {
        let options: EngineOptions = serde_json::from_str(json).unwrap();
        hosted_signing::admitted("asterdex", &options)
    };
    assert!(admitted(r#"{"asterdex":{"apiVersion":"v1"}}"#));
    assert!(!admitted(r#"{"asterdex":{"apiVersion":"v3"}}"#));
    // Only an explicit, setup-resolved V1 qualifies; inference cannot see
    // material held by the provider setup.
    assert!(!admitted(r#"{"key":"api-key","secret":"api-secret"}"#));
    assert!(!admitted(
        r#"{"applicationId":"0x2222222222222222222222222222222222222222","asterdex":{"signerAddress":"0x1111111111111111111111111111111111111111"}}"#
    ));
}

#[test]
fn ws_auth_is_refused_because_aster_uses_a_listen_key() {
    let (_, ids) = fixture();
    let input = Input::WsAuth {
        to_sign: "GET/realtime1709209800000".into(),
        output_format: hosted_signing::OutputFormat::Hex,
    };
    assert!(matches!(
        hosted_signing::prepare("asterdex", &ids, input),
        Err(safe_cex_engine::signing::SignError::InvalidRequest)
    ));
}
