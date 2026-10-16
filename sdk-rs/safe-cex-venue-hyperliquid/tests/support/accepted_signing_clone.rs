use super::{SigningInput, order_request_to_order_wire};
use safe_cex_num::JsJsonValue;
use serde_json::json;

fn field<'a>(value: &'a JsJsonValue, key: &str) -> &'a JsJsonValue {
    let JsJsonValue::Object(fields) = value else {
        panic!("expected a native prepared object");
    };
    &fields.iter().find(|(name, _)| name == key).unwrap().1
}

#[test]
fn signing_payload_clone_retains_distinct_wire_and_encoding_snapshots() {
    let input = json!({"limit_px":1.25,"sz":2.5,"order_type":{"limit":{"tif":"Gtc"}}});
    let original = order_request_to_order_wire(&input, f64::INFINITY).unwrap();
    let mut cloned = original.clone();
    for payload in [&original, &cloned] {
        assert!(payload.wire["a"].is_null());
        assert!(!payload.wire.as_object().unwrap().contains_key("b"));
        assert!(!payload.wire.as_object().unwrap().contains_key("r"));
        assert_eq!(payload.wire["p"].as_str(), Some("1.25"));
        assert_eq!(payload.wire["s"].as_str(), Some("2.5"));
        let view = payload.encoding_view();
        assert!(matches!(view, std::borrow::Cow::Borrowed(_)));
        let JsJsonValue::Number(asset) = field(&view, "a") else {
            panic!("Clone lost the native nonfinite signing asset");
        };
        assert_eq!(asset.to_bits(), 0x7ff0000000000000);
        assert!(matches!(field(&view, "b"), JsJsonValue::Null));
        assert!(matches!(field(&view, "r"), JsJsonValue::Null));
        assert_eq!(field(&view, "p"), &JsJsonValue::String("1.25".into()));
        assert_eq!(field(&view, "s"), &JsJsonValue::String("2.5".into()));
        assert_eq!(
            field(field(field(&view, "t"), "limit"), "tif"),
            &JsJsonValue::String("Gtc".into())
        );
    }
    // Both clone fields are owned snapshots. Mutating the clone cannot change
    // either independent native view of the original.
    cloned.wire["p"] = "9".into();
    let JsJsonValue::Object(fields) = &mut cloned.encoding else {
        panic!("expected the cloned native object");
    };
    fields.iter_mut().find(|(name, _)| name == "p").unwrap().1 = JsJsonValue::String("9".into());
    assert_eq!(original.wire["p"].as_str(), Some("1.25"));
    assert_eq!(
        field(&original.encoding_view(), "p"),
        &JsJsonValue::String("1.25".into())
    );
    assert_eq!(cloned.wire["p"].as_str(), Some("9"));
    assert_eq!(
        field(&cloned.encoding_view(), "p"),
        &JsJsonValue::String("9".into())
    );
}
