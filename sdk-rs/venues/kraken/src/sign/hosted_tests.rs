use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Signer as _;

fn provider() -> (LocalSigner, safe_cex_engine::signing::Identities) {
    LocalSigner::from_setup_with_policy(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-primary".into()),
            secret: Secret::new("a3Jha2VuLXRlc3Qtc2VjcmV0".into()),
            stable_id: Some("hosted-fixture".into()),
            ..Default::default()
        }), secondary: None,
    }, crate::sign::setup::spot_policy()).unwrap()
}
fn descriptor(spot: bool, nonce: &str) -> Descriptor {
    Descriptor::from_json(&format!(r#"{{"kind":"{}","method":"{}","path":"{}","form":"","nonce":"{nonce}","stampMs":1700000000000,"testnet":false}}"#,
        if spot {"kraken.spot.http"} else {"kraken.futures.http"},
        if spot {"POST"} else {"GET"},
        if spot {"/0/private/Balance"} else {"/derivatives/api/v3/accounts"})).unwrap()
}
fn receipt(nonce: &str) -> SignReceipt {
    SignReceipt {stamp_ms: Some(1_700_000_000_000.), nonce: Some(nonce.parse().unwrap()), ..Default::default()}
}
#[test]
fn exact_nonce_spot_mac_and_roundtrip_use_original_receipt_without_reserving() {
    let (signer, ids) = provider();
    for (nonce, expected) in [
        ("9007199254740993", "g7jJ2FCBpk2xKM0mtOdachbGLzDZi6gv95byQMZTBu4+0p5aGI3JgkCizqSBQkRIC3F4TCy5ByZM9tVGRzuOPA=="),
        ("9007199254740995", "mzYX1YNrpDMLia26DjzoQCw19y8wYw1rI4dbLLqnmGme1AGZAJV0yzO4PmBY96MsBQpFYZDXEOv1Ya8ha92oIg=="),
        ("18446744073709551615", "SB7b+jwuJpe99ry770cXOR/HnyCUprD5wLBjV1idFp8OdXqt3GUq4ZLjJF9ys4AbfvDavV9hzKe8WnN4ZXPUFg=="),
    ] {
        let d = descriptor(true, nonce);
        let work = d.work(ids.primary.as_ref().unwrap(), receipt(nonce)).unwrap();
        assert_eq!(Descriptor::describe(&work).unwrap(), d);
        let result = signer.sign_ready(&work).unwrap().unwrap();
        assert_eq!(result.receipt, receipt(nonce));
        assert_eq!(result.values[0], b"fixture-primary");
        assert_eq!(STANDARD.encode(&result.values[2]), expected);
    }
}
#[test]
fn futures_roundtrip_and_wrong_graph_refuse_without_a_nonce_constructor() {
    let (_, ids) = provider();
    let d = descriptor(false, "9007199254740993");
    let mut work = d.work(ids.primary.as_ref().unwrap(), receipt(&d.nonce)).unwrap();
    assert_eq!(Descriptor::describe(&work).unwrap(), d);
    work.operations.swap(0, 1);
    assert!(Descriptor::describe(&work).is_err());
}
#[test]
fn forbidden_money_paths_and_noncanonical_nonce_never_build_work() {
    let (_, ids) = provider();
    for path in ["/0/private/Withdraw", "/0/private/WithdrawCancel", "/0/private/WalletTransfer", "/derivatives/api/v3/transfer", "/derivatives/api/v3/withdrawal"] {
        let mut d = descriptor(path.starts_with("/0/"), "123");
        d.path = path.into();
        assert!(d.work(ids.primary.as_ref().unwrap(), receipt("123")).is_err());
    }
    for nonce in ["0123", "+123", "1e3", "18446744073709551616"] {
        let d = descriptor(true, nonce);
        assert!(d.work(ids.primary.as_ref().unwrap(), receipt("123")).is_err());
    }
}
#[test]
fn receipt_presence_stamp_and_nonce_disagreement_refuse() {
    let (_, ids) = provider();
    let d = descriptor(true, "123");
    for r in [SignReceipt::default(), receipt("124"), SignReceipt {expiry_ms: Some(1_700_000_000_100.), ..receipt("123")}] {
        assert!(d.work(ids.primary.as_ref().unwrap(), r).is_err());
    }
}
