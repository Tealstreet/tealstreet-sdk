//! Independent native observations for each module helper's reviewed role.
use safe_cex_num::{JsCoercionError, JsRootCollator, numeric_ingress::js_value_as_f64};
use safe_cex_types::{OrderStopTriggerType, retained_json::RetainedJsonInput};
use safe_cex_venue_kraken::helpers;
use serde_json::{Value, json};
use std::{cell::Cell, cell::RefCell, cmp::Ordering};

#[test]
fn trigger_parser_observes_native_variants_absence_and_coercion_error() {
    use OrderStopTriggerType::{IndexPrice, LastPrice, MarkPrice};
    for (input, expected) in [
        (None, None),
        (Some(Value::Null), None),
        (Some(json!(false)), None),
        (Some(json!(0)), None),
        (Some(json!("")), None),
        (Some(json!("MARK")), Some(MarkPrice)),
        (Some(json!("Spot")), Some(IndexPrice)),
        (Some(json!("last")), Some(LastPrice)),
        (Some(json!(["spot"])), Some(IndexPrice)),
        (Some(json!(["last", "mark"])), None),
        (Some(json!("index")), None),
        (Some(json!(" mark")), None),
        (Some(json!({})), None),
    ] {
        assert_eq!(
            helpers::parse_trigger_signal(input.as_ref()).unwrap(),
            expected,
            "native trigger variant for {input:?}"
        );
    }
    let error = helpers::parse_trigger_signal(Some(&json!({"toString": null}))).unwrap_err();
    assert_eq!(error.name, "TypeError");
}

#[test]
fn sort_compares_literal_operands_without_decoding_them() {
    let collator = JsRootCollator::try_new().unwrap();
    for (left, right, expected) in [
        ("alpha", "beta", Ordering::Less),
        ("beta", "alpha", Ordering::Greater),
        ("same", "same", Ordering::Equal),
        ("", "x", Ordering::Less),
        ("x", "", Ordering::Greater),
    ] {
        assert_eq!(helpers::sort(&collator, left, right), expected);
    }
}

#[test]
fn error_constructor_preserves_literal_message_distinct_snapshots_and_presence() {
    let mut response = json!({"status":403,"nested":{"response":true},"explicitNull":null});
    response["fraction"] = RetainedJsonInput::Float(-0.0).into_value().unwrap();
    let mut cause = json!({"message": "cause", "code": 17, "nested": [false, "cause"]});
    let error =
        helpers::create_kraken_api_error("literal {broken json}", Some(&response), Some(&cause));
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, "literal {broken json}");
    assert_eq!(error.to_string(), "literal {broken json}");
    let native_response = error.response_json_view().unwrap();
    let native_cause = error.metadata.original_error.as_ref().unwrap();
    assert_eq!(js_value_as_f64(&native_response["status"]).unwrap(), 403.0);
    assert_eq!(
        js_value_as_f64(&native_response["fraction"])
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(native_response["nested"]["response"], true);
    let object = native_response.as_object().unwrap();
    assert!(object.contains_key("explicitNull"));
    assert!(object["explicitNull"].is_null());
    assert!(!object.contains_key("missing"));
    assert_eq!(js_value_as_f64(&native_cause["code"]).unwrap(), 17.0);
    assert_eq!(native_cause["message"], "cause");
    assert_eq!(native_cause["nested"], json!([false, "cause"]));

    response["status"] = json!(429);
    cause["message"] = json!("changed");
    assert_eq!(js_value_as_f64(&native_response["status"]).unwrap(), 403.0);
    assert_eq!(native_cause["message"], "cause");
    let absent = helpers::create_kraken_api_error("", None, None);
    assert!(absent.response_json_view().is_none());
    assert!(absent.metadata.original_error.is_none());
    let explicit_null =
        helpers::create_kraken_api_error("", Some(&Value::Null), Some(&Value::Null));
    assert!(matches!(
        explicit_null.response_json_view(),
        Some(Value::Null)
    ));
    assert!(matches!(
        explicit_null.metadata.original_error,
        Some(Value::Null)
    ));
}

#[test]
fn id_factory_executes_host_entropy_and_retains_pool_after_failure() {
    // Independent source expectation: the pinned nanoid hex alphabet maps
    // constant 0xaf entropy to 'f', with fixed venue length 32.
    let attempts = Cell::new(0);
    let draws = RefCell::new(Vec::new());
    let mut generate = helpers::create_kraken_spot_client_order_id(|buffer| {
        draws.borrow_mut().push(buffer.len());
        let attempt = attempts.get();
        attempts.set(attempt + 1);
        if attempt == 0 {
            return Err(JsCoercionError {
                name: "Error",
                message: "entropy refused",
            });
        }
        buffer.fill(0xaf);
        Ok(())
    });
    let error = generate().unwrap_err();
    assert_eq!(error.name, "Error");
    assert_eq!(error.message, "entropy refused");
    assert_eq!(generate().unwrap(), "ffffffffffffffffffffffffffffffff");
    assert_eq!(generate().unwrap(), "ffffffffffffffffffffffffffffffff");
    // The source grows its next-pool size before the failing entropy request.
    assert_eq!(draws.borrow().as_slice(), &[32, 512]);
    assert_eq!(attempts.get(), 2);
}
