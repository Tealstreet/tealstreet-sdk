use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Signer as _;

#[test]
fn pure_spot_u64_vectors_preserve_decimal_body_receipt_and_mac_without_reservation() {
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("fixture-primary".into()),
                secret: Secret::new("a3Jha2VuLXRlc3Qtc2VjcmV0".into()),
                stable_id: Some("pure-kraken-reference".into()),
                ..Default::default()
            }),
            secondary: None,
        },
        crate::sign::setup::spot_policy(),
    )
    .unwrap();
    let recipe = Selected::spot(&identities, CredentialRole::Read { scope: None })
        .unwrap()
        .spot_http("/0/private/Balance", "", 5000.)
        .unwrap();
    for (nonce, signature) in [
        (
            9_007_199_254_740_993_u64,
            "g7jJ2FCBpk2xKM0mtOdachbGLzDZi6gv95byQMZTBu4+0p5aGI3JgkCizqSBQkRIC3F4TCy5ByZM9tVGRzuOPA==",
        ),
        (
            9_007_199_254_740_995,
            "mzYX1YNrpDMLia26DjzoQCw19y8wYw1rI4dbLLqnmGme1AGZAJV0yzO4PmBY96MsBQpFYZDXEOv1Ya8ha92oIg==",
        ),
        (
            u64::MAX,
            "SB7b+jwuJpe99ry770cXOR/HnyCUprD5wLBjV1idFp8OdXqt3GUq4ZLjJF9ys4AbfvDavV9hzKe8WnN4ZXPUFg==",
        ),
    ] {
        // Pure work only: no Cx, dispatcher reserve, transport, or WASM claim.
        let (body, work) = recipe.work(nonce, 1_700_000_000_000.).unwrap();
        assert_eq!(body, Some(format!("nonce={nonce}")));
        assert_eq!(work.receipt.nonce, Some(nonce));
        let json = serde_json::to_string(&work.receipt).unwrap();
        assert!(json.contains(&format!(r#""nonce":"{nonce}""#)));
        let decoded: SignReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, work.receipt);
        let result = signer.sign_ready(&work).unwrap().unwrap();
        assert_eq!(result.values[0], b"fixture-primary");
        assert_eq!(STANDARD.encode(&result.values[2]), signature);
        assert_eq!(result.receipt, work.receipt);
    }
}
