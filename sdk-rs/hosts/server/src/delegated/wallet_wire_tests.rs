use super::*;
use crate::envelope::InitPayload;
use serde_json::{Value, json};

#[test]
fn encrypted_wallet_full_init_keeps_material_outside_public_settings() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/delegated-wallet-handover.json"
    ))
    .unwrap();
    let input = &fixture["input"];
    let frame = &fixture["frame"];
    let material = input["signingKey"].as_str().unwrap();
    assert!(!frame.to_string().contains(material));
    let init: InitPayload = serde_json::from_value(frame["d"].clone()).unwrap();
    assert!(!format!("{init:?}").contains(material));
    assert!(!init.fingerprint().contains(material));
    let sealed = init.sealed.as_ref().unwrap();
    let mut handovers = Handovers::default();
    handovers.attempts.insert(
        init.account_name.clone(),
        Attempt {
            id: sealed.attempt_id.clone(),
            venue: init.exchange_name.clone(),
            private: Zeroizing::new([fixture["recipientSeedByte"].as_u64().unwrap() as u8; 32]),
            expires: 1000,
        },
    );
    let mut opening = handovers.take(&init.account_name, sealed, 100).unwrap();
    opening.boot_id = "synthetic-wallet-boot";
    let credentials = opening
        .open(&init.exchange_name, &init.key, sealed, 101)
        .expect("sealed key-only credentials reach trusted construction");
    assert!(handovers.take(&init.account_name, sealed, 102).is_err());
    let prepared = prepare_delegated(&init, credentials.pair()).unwrap();
    assert!(!prepared.options.contains(material));
    assert!(!format!("{:?}", prepared.plan).contains(material));
}

#[test]
fn unsafe_unsealed_wallet_scalar_shape_is_refused() {
    let material = "0x0000000000000000000000000000000000000000000000000000000000000001";
    let mut handovers = Handovers::default();
    let offer = handovers.issue("wallet", "paradex", 100).unwrap();
    let payload = json!({"key":material,"secret":"unused-nonempty"});
    let sealed = tests::seal(&offer, "paradex", material, payload.to_string().as_bytes());
    let init: InitPayload = serde_json::from_value(json!({
        "accountName":"wallet","exchangeName":"paradex","key":material,
        "paradexAddress":"0x123","subkeyAddress":"0x456","sealed":sealed
    }))
    .unwrap();
    let sealed = init.sealed.as_ref().unwrap();
    let result = handovers
        .take("wallet", sealed, 101)
        .unwrap()
        .open("paradex", &init.key, sealed, 102)
        .and_then(|credentials| prepare_delegated(&init, credentials.pair()).map_err(|_| Refused));
    assert!(
        result.is_err(),
        "unsealed scalar layout must not be admitted"
    );
}

#[test]
fn encrypted_hmac_control_and_empty_material_refusal_are_unchanged() {
    for (secret, accepted) in [("synthetic-secret", true), ("", false)] {
        let mut handovers = Handovers::default();
        let offer = handovers.issue("hmac", "binance", 100).unwrap();
        let payload = json!({"key":"synthetic-key","secret":secret});
        let sealed = tests::seal(
            &offer,
            "binance",
            "synthetic-key",
            payload.to_string().as_bytes(),
        );
        let frame = json!({"t":"init","d":{
            "accountName":"hmac","exchangeName":"binance","key":"synthetic-key","sealed":sealed
        }});
        assert!(!frame.to_string().contains("synthetic-secret"));
        let init: InitPayload = serde_json::from_value(frame["d"].clone()).unwrap();
        let sealed = init.sealed.as_ref().unwrap();
        let result = handovers
            .take("hmac", sealed, 101)
            .unwrap()
            .open("binance", &init.key, sealed, 102);
        assert_eq!(result.is_ok(), accepted);
        if let Ok(credentials) = result {
            assert!(prepare_delegated(&init, credentials.pair()).is_ok());
        }
    }
}

#[test]
fn wallet_wire_rejects_missing_material_foreign_layout_and_wrong_public_binding() {
    let material = "0x0000000000000000000000000000000000000000000000000000000000000001";
    for (venue, identity, signing_key) in [
        ("paradex", "0x456", None),
        ("paradex", "0x456", Some("")),
        ("paradex", "other-public-identity", Some(material)),
        ("paradex", material, Some(material)),
        ("paradex", "0x456", Some("invalid-synthetic-material")),
        ("binance", "0x456", Some(material)),
    ] {
        let mut handovers = Handovers::default();
        let offer = handovers.issue("fixture", venue, 100).unwrap();
        let mut payload = json!({"key":identity,"secret":""});
        if let Some(signing_key) = signing_key {
            payload["signingKey"] = signing_key.into();
        }
        let sealed = tests::seal(&offer, venue, identity, payload.to_string().as_bytes());
        let init: InitPayload = serde_json::from_value(json!({
            "accountName":"fixture","exchangeName":venue,"key":identity,
            "paradexAddress":"0x123","subkeyAddress":"0x456","sealed":sealed
        }))
        .unwrap();
        let sealed = init.sealed.as_ref().unwrap();
        let result = handovers
            .take("fixture", sealed, 101)
            .unwrap()
            .open(venue, &init.key, sealed, 102)
            .and_then(|credentials| {
                prepare_delegated(&init, credentials.pair()).map_err(|_| Refused)
            });
        assert!(result.is_err());
        assert!(handovers.take("fixture", sealed, 103).is_err());
    }
}
