//! The typed account carrier retains the fields the Polymarket constructor reads.
use safe_cex_exchange::js_runtime::value::{Js, from_typed, to_typed};
use safe_cex_types::{ExchangeOptions, ExchangeOptionsPolymarket};
use serde_json::json;

#[test]
fn polymarket_account_options_reach_the_receiver_without_dropping_fields_or_number_bits() {
    let root = Js::object(vec![
        ("key", Js::str("")),
        ("secret", Js::str("")),
        (
            "polymarket",
            Js::object(vec![
                ("apiKey", Js::str("derived-key")),
                ("apiSecret", Js::str("derived-secret")),
                ("apiPassphrase", Js::str("derived-passphrase")),
                ("funderAddress", Js::str("funder-🟦")),
                ("signerType", Js::Number(3.0)),
            ]),
        ),
    ]);
    for signer_type in [
        0.0,
        1.0,
        2.0,
        3.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        root.get("polymarket")
            .unwrap()
            .set("signerType", Js::Number(signer_type))
            .unwrap();
        let typed: ExchangeOptions = to_typed(&root).unwrap();
        let account = typed
            .polymarket
            .as_ref()
            .expect("typed registry preserves options.polymarket");
        assert_eq!(account.api_key.as_deref(), Some("derived-key"));
        assert_eq!(account.api_secret.as_deref(), Some("derived-secret"));
        assert_eq!(
            account.api_passphrase.as_deref(),
            Some("derived-passphrase")
        );
        assert_eq!(account.funder_address.as_deref(), Some("funder-🟦"));
        assert_eq!(
            account.signer_type.unwrap().to_bits(),
            signer_type.to_bits()
        );
        let projected = from_typed(&typed).unwrap().get("polymarket").unwrap();
        assert!(
            projected.is_object(),
            "native account carrier reaches source constructor"
        );
        for (key, expected) in [
            ("apiKey", "derived-key"),
            ("apiSecret", "derived-secret"),
            ("apiPassphrase", "derived-passphrase"),
            ("funderAddress", "funder-🟦"),
        ] {
            assert_eq!(projected.get(key).unwrap().to_display(), expected);
        }
        assert_eq!(
            projected
                .get("signerType")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            signer_type.to_bits()
        );
    }
}

#[test]
fn polymarket_account_carrier_distinguishes_omission_and_rejects_wrong_record_shapes() {
    let omitted: ExchangeOptions = serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap();
    assert!(omitted.polymarket.is_none());
    assert!(
        serde_json::to_value(&omitted)
            .unwrap()
            .get("polymarket")
            .is_none()
    );
    let empty: ExchangeOptions =
        serde_json::from_value(json!({"key":"k","secret":"s","polymarket":{}})).unwrap();
    assert_eq!(empty.polymarket, Some(ExchangeOptionsPolymarket::default()));
    assert_eq!(
        serde_json::to_value(&empty).unwrap()["polymarket"],
        json!({})
    );
    for invalid in [
        json!(null),
        json!([]),
        json!(["key", "secret", "passphrase", 3, "funder"]),
        json!("account"),
        json!({"apiKey":null}),
        json!({"apiSecret":3}),
        json!({"apiPassphrase":false}),
        json!({"signerType":"3"}),
        json!({"signerType":null}),
        json!({"funderAddress":[]}),
    ] {
        assert!(
            serde_json::from_value::<ExchangeOptions>(
                json!({"key":"k","secret":"s","polymarket":invalid})
            )
            .is_err()
        );
    }
}
