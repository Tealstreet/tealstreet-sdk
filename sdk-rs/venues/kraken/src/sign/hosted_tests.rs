use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Signer as _;

fn provider() -> (LocalSigner, safe_cex_engine::signing::Identities) {
    LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("fixture-primary".into()),
                secret: Secret::new("a3Jha2VuLXRlc3Qtc2VjcmV0".into()),
                stable_id: Some("hosted-fixture".into()),
                ..Default::default()
            }),
            secondary: None,
        },
        crate::sign::setup::spot_policy(),
    )
    .unwrap()
}
fn descriptor(spot: bool, nonce: &str) -> Descriptor {
    Descriptor::from_json(&format!(r#"{{"kind":"{}","method":"{}","path":"{}","form":"","nonce":"{nonce}","stampMs":1700000000000,"testnet":false}}"#,
        if spot {"kraken.spot.http"} else {"kraken.futures.http"},
        if spot {"POST"} else {"GET"},
        if spot {"/0/private/Balance"} else {"/derivatives/api/v3/accounts"})).unwrap()
}
fn receipt(nonce: &str) -> SignReceipt {
    SignReceipt {
        stamp_ms: Some(1_700_000_000_000.),
        nonce: Some(nonce.parse().unwrap()),
        ..Default::default()
    }
}
#[test]
fn exact_nonce_spot_mac_and_roundtrip_use_original_receipt_without_reserving() {
    let (signer, ids) = provider();
    for (nonce, expected) in [
        (
            "9007199254740993",
            "g7jJ2FCBpk2xKM0mtOdachbGLzDZi6gv95byQMZTBu4+0p5aGI3JgkCizqSBQkRIC3F4TCy5ByZM9tVGRzuOPA==",
        ),
        (
            "9007199254740995",
            "mzYX1YNrpDMLia26DjzoQCw19y8wYw1rI4dbLLqnmGme1AGZAJV0yzO4PmBY96MsBQpFYZDXEOv1Ya8ha92oIg==",
        ),
        (
            "18446744073709551615",
            "SB7b+jwuJpe99ry770cXOR/HnyCUprD5wLBjV1idFp8OdXqt3GUq4ZLjJF9ys4AbfvDavV9hzKe8WnN4ZXPUFg==",
        ),
    ] {
        let d = descriptor(true, nonce);
        let work = d
            .work(ids.primary.as_ref().unwrap(), receipt(nonce))
            .unwrap();
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
    let mut work = d
        .work(ids.primary.as_ref().unwrap(), receipt(&d.nonce))
        .unwrap();
    assert_eq!(Descriptor::describe(&work).unwrap(), d);
    work.operations.swap(0, 1);
    assert!(Descriptor::describe(&work).is_err());
}
#[test]
fn forbidden_money_paths_and_noncanonical_nonce_never_build_work() {
    let (_, ids) = provider();
    for path in [
        "/0/private/Withdraw",
        "/0/private/WithdrawCancel",
        "/0/private/WalletTransfer",
        "/derivatives/api/v3/transfer",
        "/derivatives/api/v3/withdrawal",
    ] {
        let mut d = descriptor(path.starts_with("/0/"), "123");
        d.path = path.into();
        assert!(
            d.work(ids.primary.as_ref().unwrap(), receipt("123"))
                .is_err()
        );
    }
    for nonce in ["0123", "+123", "1e3", "18446744073709551616"] {
        let d = descriptor(true, nonce);
        assert!(
            d.work(ids.primary.as_ref().unwrap(), receipt("123"))
                .is_err()
        );
    }
}
#[test]
fn receipt_presence_stamp_and_nonce_disagreement_refuse() {
    let (_, ids) = provider();
    let d = descriptor(true, "123");
    for r in [
        SignReceipt::default(),
        receipt("124"),
        SignReceipt {
            expiry_ms: Some(1_700_000_000_100.),
            ..receipt("123")
        },
    ] {
        assert!(d.work(ids.primary.as_ref().unwrap(), r).is_err());
    }
}

#[test]
fn futures_literal_macs_match_pinned_corpus() {
    let (signer, ids) = provider();
    for (nonce, expected) in [
        (
            "9007199254740993",
            "QdI7sspHk2q/rWv+pKGis1FtpNCAaTfcsuIWOnx/BKAsjAx9Jw2DmdZBMuIIfj+tggTsI4iwN+KOm84h8HZJ4Q==",
        ),
        (
            "9007199254740995",
            "rnSSfaw7tny3lN7lJrWGxLzcFE/2bKRgWUqFG3YqAzyxOFpzV68kQ+lG1JRoy9LkOHw0gn08IAx4YntPZXVhHw==",
        ),
        (
            "18446744073709551615",
            "aN/XqAOUkvgMvV0TNvKwcKfu7YcKg97A/gJGXc+pGMJb+wJxG7ReuCe/+PSiSGFzzN4mRwityY4e5bk8ZleZ1A==",
        ),
    ] {
        let d = descriptor(false, nonce);
        let work = d
            .work(ids.primary.as_ref().unwrap(), receipt(nonce))
            .unwrap();
        let signed = signer.sign_ready(&work).unwrap().unwrap();
        assert_eq!(STANDARD.encode(&signed.values[2]), expected);
        d.verify(&signed, "fixture-primary").unwrap();
    }
}
#[test]
fn closed_path_role_slot_and_raw_deadline_checks_refuse_before_signing() {
    use safe_cex_engine::{CredentialSlot, credential_selection::CredentialRole};
    let d = descriptor(false, "123");
    let r = receipt("123");
    let at = 1_700_000_000_000.;
    d.validate(
        CredentialSlot::Secondary,
        CredentialRole::Read { scope: None },
        r,
        at,
        at + 1000.,
    )
    .unwrap();
    for role in [CredentialRole::Mutation, CredentialRole::SessionAuth] {
        assert!(
            d.validate(CredentialSlot::Primary, role, r, at, at + 1000.)
                .is_err()
        );
    }
    for (now, deadline) in [
        (f64::NAN, at + 1000.),
        (f64::INFINITY, at + 1000.),
        (-1., at + 1000.),
        (at, f64::NAN),
        (at, at),
        (at, at + 5000.01),
        (at + 5000., at + 6000.),
    ] {
        assert!(
            d.validate(
                CredentialSlot::Primary,
                CredentialRole::Read { scope: None },
                r,
                now,
                deadline
            )
            .is_err()
        );
    }
    d.validate(
        CredentialSlot::Primary,
        CredentialRole::Read { scope: None },
        r,
        at + 0.25,
        at + 1000.5,
    )
    .unwrap();
}
#[test]
fn nonce_alias_fields_and_altered_operation_graphs_refuse() {
    let (_, ids) = provider();
    let mut d = descriptor(true, "123");
    for form in ["nonce=123", "%6Eonce=123", "n%6fnce=123"] {
        d.form = form.into();
        assert!(
            d.work(ids.primary.as_ref().unwrap(), receipt("123"))
                .is_err()
        );
    }
    let d = descriptor(false, "123");
    let work = d
        .work(ids.primary.as_ref().unwrap(), receipt("123"))
        .unwrap();
    for bad in [
        {
            let mut w = work.clone();
            w.context.host = "other.invalid".into();
            w
        },
        {
            let mut w = work.clone();
            w.context.path = "/api/v3/accounts".into();
            w
        },
        {
            let mut w = work.clone();
            w.operations.push(w.operations[0].clone());
            w
        },
        {
            let mut w = work.clone();
            w.receipt.nonce = Some(124);
            w
        },
    ] {
        assert!(Descriptor::describe(&bad).is_err());
    }
}
#[test]
fn installed_executor_owns_refs_and_exposes_only_public_binding() {
    use safe_cex_engine::{CredentialSlot, credential_selection::CredentialRole};
    let executor=Executor::from_json(r#"{"primary":{"key":"fixture-primary","secret":"a3Jha2VuLXRlc3Qtc2VjcmV0","stableId":"executor-subject"}}"#.into(),true,false).unwrap();
    assert_eq!(
        executor.binding(CredentialSlot::Primary).unwrap(),
        ("executor-subject".into(), "fixture-primary".into())
    );
    assert!(executor.binding(CredentialSlot::Secondary).is_err());
    let d = descriptor(true, "9007199254740993");
    let r = receipt(&d.nonce);
    let result = executor
        .execute(
            &d,
            CredentialSlot::Primary,
            CredentialRole::Read { scope: None },
            r,
            1_700_000_000_000.,
            1_700_000_001_000.,
        )
        .unwrap();
    d.verify(&result, "fixture-primary").unwrap();
    assert!(d.verify(&result, "other-credential").is_err());
    let mut bad = result.clone();
    bad.values[1][0] ^= 1;
    assert!(d.verify(&bad, "fixture-primary").is_err());
}
