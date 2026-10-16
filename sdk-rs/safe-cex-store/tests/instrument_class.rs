use safe_cex_store::instrument_class::*;
use safe_cex_types::Market;
use serde_json::{Value, json};

fn market(info: Value) -> Market {
    serde_json::from_value(json!({"id":"SHOPUSDT","symbol":"SHOPUSDT","base":"SHOP","quote":"USDT","active":true,"precision":{"amount":1,"price":0.01},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":10}},"info":info})).unwrap()
}
#[test]
fn classification_retains_unknown_and_bybit_wire_values() {
    for venue in venues_with_instrument_class_fn() {
        for (label, class) in [
            ("", AssetClass::Crypto),
            ("innovation", AssetClass::Crypto),
            ("stock", AssetClass::Equity),
            ("ETF", AssetClass::Etf),
            ("commodity", AssetClass::Commodity),
            ("forex", AssetClass::Fx),
        ] {
            let result =
                resolve_venue_instrument_class(venue, &market(json!({"symbolType":label})));
            assert_eq!(
                is_non_crypto_instrument(&result),
                Some(class != AssetClass::Crypto)
            );
            assert!(
                matches!(result, InstrumentClassResult::Classified { asset_class, venue_label, .. } if asset_class == class && venue_label == label)
            );
        }
    }
    for info in [
        Value::Null,
        json!({}),
        json!({"symbolType":null}),
        json!({"symbolType":42}),
        json!({"symbolType":false}),
        json!({"symbolType":[]}),
        json!({"symbolType":"NewClass"}),
    ] {
        let result = classify_market("bybit", &market(info));
        assert_eq!(is_non_crypto_instrument(&result), None);
        assert!(tradfi_preflight_message("SHOPUSDT", "bybit", &result).is_none());
    }
    assert_eq!(
        classify_market("hyperliquid", &market(json!({"symbolType":"stock"}))),
        InstrumentClassResult::Unknown {
            reason: "no instrument classifier implemented for hyperliquid".into()
        }
    );
    assert_eq!(
        describe_instrument(
            "X",
            "bybit",
            &classify_market("bybit", &market(json!({"symbolType":"new-wire-value"})))
        ),
        "X on bybit: unrecognised bybit symbolType \"new-wire-value\""
    );
}
#[test]
fn copy_is_shared_and_never_asserts_eligibility() {
    let result = classify_market(
        "bybit",
        &market(
            json!({"symbolType":" stock ","underlyingTicker":" Shopify ","marketRegion":" US "}),
        ),
    );
    assert_eq!(
        describe_instrument("SHOPUSDT", "bybit", &result),
        "SHOPUSDT on bybit: tokenized equity (Shopify) — may require signing a trading agreement"
    );
    assert_eq!(
        tradfi_preflight_message("SHOPUSDT", "bybit", &result).unwrap(),
        "SHOPUSDT on bybit is a tokenized equity (Shopify). Opening a position may require an agreement you have not signed with the venue — place the order through the venue's own UI first to accept it."
    );
    let crypto = classify_market("bybit", &market(json!({"symbolType":"innovation"})));
    assert_eq!(
        describe_instrument("X", "bybit", &crypto),
        "X on bybit: crypto perpetual"
    );
    assert!(tradfi_preflight_message("X", "bybit", &crypto).is_none());
}

#[test]
fn javascript_trim_and_prototype_lookup_bugs_are_preserved() {
    let bom = classify_market(
        "bybit",
        &market(
            json!({"symbolType":"\u{feff}stock\u{feff}","underlyingTicker":"\u{feff}X\u{feff}"}),
        ),
    );
    assert_eq!(
        describe_instrument("X", "bybit", &bom),
        "X on bybit: tokenized equity (X) — may require signing a trading agreement"
    );
    let nel = classify_market(
        "bybit",
        &market(json!({"symbolType":"\u{0085}stock\u{0085}"})),
    );
    assert!(matches!(nel, InstrumentClassResult::Unknown { .. }));
    for label in ["constructor", "toString", "__proto__"] {
        let result = classify_market("bybit", &market(json!({"symbolType":label})));
        assert_eq!(is_non_crypto_instrument(&result), Some(true));
        assert_eq!(
            describe_instrument("X", "bybit", &result),
            "X on bybit: undefined — may require signing a trading agreement"
        );
        assert!(
            matches!(result, InstrumentClassResult::Classified { asset_class: AssetClass::PrototypeProperty(_), venue_label, .. } if venue_label == label)
        );
    }
}

#[test]
fn checked_in_typescript_oracle_cases() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("instrument_oracle.json")).unwrap();
    for case in cases {
        let exchange = case["exchange"].as_str().unwrap();
        let result = classify_market(exchange, &market(case["info"].clone()));
        assert_eq!(
            serde_json::to_value(is_non_crypto_instrument(&result)).unwrap(),
            case["nonCrypto"],
            "{case}"
        );
        assert_eq!(
            describe_instrument("X", exchange, &result),
            case["description"].as_str().unwrap(),
            "{case}"
        );
        assert_eq!(
            serde_json::to_value(tradfi_preflight_message("X", exchange, &result)).unwrap(),
            case["preflight"],
            "{case}"
        );
    }
}
