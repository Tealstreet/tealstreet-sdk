#![cfg(feature = "okx")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

#[test]
fn all_okx_aliases_capture_primary_http_and_ws_with_provider_owned_passphrase() {
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
    for alias in ["okx", "okxeu", "okxdemo", "okxeudemo"] {
        let (work, kind) = hosted_signing::prepare(
            alias,
            &ids,
            Input::Http {
                payload: "1970-01-01T00:00:00.000ZGET/api/v5/account/config".into(),
                stamp_ms: 0.,
                revision: "0".into(),
            },
        )
        .unwrap();
        work.validate().unwrap();
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(
            output.signature.as_deref(),
            Some("29klY97actzxrGqk1y/O9sExvXx4Rs9/13y6nq9tfi0=")
        );
        assert_eq!(output.credential.as_deref(), Some("test-key"));
        assert_eq!(
            output.headers,
            Some(vec![(
                "OK-ACCESS-PASSPHRASE".into(),
                "test-passphrase".into()
            )])
        );
        let (work, kind) = hosted_signing::prepare(
            alias,
            &ids,
            Input::WsAuth {
                to_sign: "1234567890GET/users/self/verify".into(),
                output_format: OutputFormat::Base64,
            },
        )
        .unwrap();
        let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
        assert_eq!(
            output.signature.as_deref(),
            Some("qf8dggkdwH7IVTM6AIiNv9i1NS6Boqw3+pNUhrOWjWQ=")
        );
        assert!(output.credential.is_none());
        assert!(output.headers.is_none());
    }
}
