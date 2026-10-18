//! Independent native witnesses for the two source string tags.
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_kraken::types::KrakenBaseUrl;
use serde::Deserialize;
use serde_json::{Value, json};

#[test]
fn base_url_tags_preserve_native_variants_before_serialization() {
    for (tag, expected) in [
        ("derivatives", KrakenBaseUrl::Derivatives),
        ("history", KrakenBaseUrl::History),
    ] {
        let input = json!(tag);
        let wire = serde_json::to_string(tag).unwrap();
        let variants = [
            serde_json::from_value::<KrakenBaseUrl>(input.clone()).unwrap(),
            KrakenBaseUrl::deserialize(&input).unwrap(),
            <KrakenBaseUrl as Deserialize>::deserialize(input).unwrap(),
            serde_json::from_str::<KrakenBaseUrl>(&wire).unwrap(),
            js_from_str::<KrakenBaseUrl>(&wire).unwrap(),
        ];
        for actual in variants {
            assert_eq!(actual, expected, "native variant for {tag}");
            assert_eq!(serde_json::to_value(actual).unwrap(), json!(tag));
        }
    }
}

fn refuse(input: Value) {
    let text = input.to_string();
    assert!(
        serde_json::from_value::<KrakenBaseUrl>(input.clone()).is_err(),
        "owned route admitted non-string {text}"
    );
    assert!(
        KrakenBaseUrl::deserialize(&input).is_err(),
        "borrowed route admitted non-string {text}"
    );
    assert!(
        <KrakenBaseUrl as Deserialize>::deserialize(input).is_err(),
        "qualified route admitted non-string {text}"
    );
    assert!(
        serde_json::from_str::<KrakenBaseUrl>(&text).is_err(),
        "text route admitted non-string {text}"
    );
    assert!(
        js_from_str::<KrakenBaseUrl>(&text).is_err(),
        "shared numeric-ingress route admitted non-string {text}"
    );
}

#[test]
fn tagged_base_url_shapes_are_rejected_on_every_ingress_path() {
    for input in [json!({"derivatives": null}), json!({"history": null})] {
        refuse(input);
    }
    for input in [Value::Null, json!(false), json!(17), json!([]), json!({})] {
        refuse(input);
    }
    for input in ["", "History", "Derivatives", "other"] {
        refuse(json!(input));
    }
}
