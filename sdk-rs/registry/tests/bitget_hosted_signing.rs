#![cfg(feature = "bitget")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef, Operation, SignError};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, Identities) {
    let mut provider = LocalSigner::new();
    let identity = Identity::new(
        KeyRef("ref:owned-public".into()),
        Some(KeyRef("ref:owned-primary".into())),
    )
    .with_passphrase(Some(KeyRef("ref:owned-presentation".into())));
    provider.insert(identity.credential.clone(), b"test-key".to_vec());
    provider.insert(
        identity.signing_ref().unwrap().clone(),
        b"test-secret".to_vec(),
    );
    provider.insert(
        identity.passphrase.clone().unwrap(),
        b"test-passphrase".to_vec(),
    );
    let ids = Identities {
        primary: Some(identity),
        secondary: Some(
            Identity::new(
                KeyRef("ref:unavailable-secondary-public".into()),
                Some(KeyRef("ref:unavailable-secondary".into())),
            )
            .with_passphrase(Some(KeyRef(
                "ref:unavailable-secondary-presentation".into(),
            ))),
        ),
    };
    (provider, ids)
}

#[test]
fn all_bitget_aliases_capture_primary_http_and_ws_with_provider_owned_passphrase() {
    let (provider, ids) = fixture();
    for alias in ["bitget", "bitgetusdc", "bitgetinverse"] {
        let (work, kind) = hosted_signing::prepare(
            alias,
            &ids,
            Input::Http {
                payload: "0GET/api/v2/mix/account/accounts?productType=USDT-FUTURES".into(),
                stamp_ms: 0.,
                revision: "0".into(),
            },
        )
        .unwrap();
        work.validate().unwrap();
        assert!(
            matches!(&work.operations[1], Operation::Sign { key, .. } if key.0 == "ref:owned-primary"),
            "hosted capture signs with primary even when secondary is configured"
        );
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        // Independent Python stdlib HMAC vectors.
        assert_eq!(
            output.signature.as_deref(),
            Some("W5FY5C8bU0lHFeZjSbQiJzWuWkcW7w75zsOvygqSzRI=")
        );
        assert_eq!(output.credential.as_deref(), Some("test-key"));
        assert_eq!(
            output.headers,
            Some(vec![("ACCESS-PASSPHRASE".into(), "test-passphrase".into())])
        );
        let (work, kind) = hosted_signing::prepare(
            alias,
            &ids,
            Input::WsAuth {
                to_sign: "1234567890GET/user/verify".into(),
                output_format: OutputFormat::Base64,
            },
        )
        .unwrap();
        let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
        assert_eq!(
            output.signature.as_deref(),
            Some("F16UnQLNNM3oDznYa+MKnpJuiu/XrG/FE5j6UV9UB/8=")
        );
        assert!(output.credential.is_none());
        assert!(output.headers.is_none());
    }
}

#[test]
fn bitget_capture_without_a_provider_passphrase_refuses_before_provider_work() {
    let (_, mut ids) = fixture();
    ids.primary.as_mut().unwrap().passphrase = None;
    for alias in ["bitget", "bitgetusdc", "bitgetinverse"] {
        assert!(matches!(
            hosted_signing::prepare(
                alias,
                &ids,
                Input::Http {
                    payload: "0GET/api/v2/mix/account/accounts".into(),
                    stamp_ms: 0.,
                    revision: "0".into(),
                },
            ),
            Err(SignError::Unavailable)
        ));
    }
}
