use safe_cex_crypto::{Encoding, WsOutputFormat};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

fn assert_string_union<T: DeserializeOwned + Serialize + PartialEq + Debug>(cases: &[(&str, T)]) {
    for (literal, expected) in cases {
        let value = Value::String((*literal).into());
        let raw = serde_json::to_string(&value).unwrap();
        for decoded in [
            serde_json::from_str::<T>(&raw).unwrap(),
            serde_json::from_value::<T>(value.clone()).unwrap(),
            T::deserialize(&value).unwrap(),
            <T as Deserialize>::deserialize(value).unwrap(),
        ] {
            assert_eq!(&decoded, expected, "actual variant for {literal}");
            assert_eq!(serde_json::to_value(decoded).unwrap(), json!(literal));
        }

        let tagged = json!({(*literal):null});
        assert!(
            serde_json::from_str::<T>(&tagged.to_string()).is_err(),
            "raw JSON tagged map must be refused for {literal}"
        );
        assert!(
            serde_json::from_value::<T>(tagged.clone()).is_err(),
            "owned tagged map must be refused for {literal}"
        );
        assert!(
            T::deserialize(&tagged).is_err(),
            "borrowed tagged map must be refused for {literal}"
        );
        assert!(
            <T as Deserialize>::deserialize(tagged).is_err(),
            "qualified tagged map must be refused for {literal}"
        );
        for invalid in [
            format!(" {literal}"),
            format!("{literal} "),
            format!("\u{feff}{literal}"),
            literal.to_uppercase(),
        ] {
            assert!(serde_json::from_value::<T>(json!(invalid)).is_err());
        }
    }
    for invalid in [
        json!([]),
        json!(["hex"]),
        json!({}),
        Value::Null,
        json!(true),
        json!(1),
        json!("unknown"),
    ] {
        assert!(serde_json::from_str::<T>(&invalid.to_string()).is_err());
        assert!(serde_json::from_value::<T>(invalid.clone()).is_err());
        assert!(T::deserialize(&invalid).is_err());
        assert!(<T as Deserialize>::deserialize(invalid).is_err());
    }
}

#[test]
fn encoding_accepts_only_literal_strings_and_preserves_native_variants() {
    assert_string_union(&[("hex", Encoding::Hex), ("base64", Encoding::Base64)]);
}

#[test]
fn ws_output_format_accepts_only_literal_strings_and_preserves_native_variants() {
    assert_string_union(&[
        ("hex", WsOutputFormat::Hex),
        ("base64", WsOutputFormat::Base64),
        ("hex-base64", WsOutputFormat::HexBase64),
        ("kraken-challenge", WsOutputFormat::KrakenChallenge),
        ("apexomni-base64", WsOutputFormat::ApexomniBase64),
        ("bitunix-double-sha256", WsOutputFormat::BitunixDoubleSha256),
        ("coinbase-jwt", WsOutputFormat::CoinbaseJwt),
    ]);
}
