use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_venue_cryptocom::{
    api::{is_non_retryable_private_write_units, is_public_endpoint, is_public_endpoint_units},
    signing::{generate_authenticated_request, generate_signature},
    signing_value::SigningValue,
    types::PUBLIC_ENDPOINTS,
};
use serde_json::{Value, json};
use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

fn value(value: Value) -> SigningValue {
    SigningValue::from_value(value)
}
fn units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

#[test]
fn retained_module_array_changes_real_classifier() {
    let table = PUBLIC_ENDPOINTS.clone();
    let original = table.slots();
    assert_eq!(original.len(), 9, "PUBLIC-INDEPENDENT-INITIAL-LENGTH");
    assert_eq!(
        original,
        [
            Some(units("/public/get-instruments")),
            Some(units("/public/get-book")),
            Some(units("/public/get-tickers")),
            Some(units("/public/get-ticker")),
            Some(units("/public/get-trades")),
            Some(units("/public/get-candlestick")),
            Some(units("/public/get-valuations")),
            Some(units("/public/get-expired-settlement-price")),
            Some(units("/public/get-insurance")),
        ],
        "PUBLIC-NATIVE-INITIAL-VALUES"
    );
    assert!(
        table.same_reference(&PUBLIC_ENDPOINTS),
        "PUBLIC-RETAINED-IDENTITY"
    );
    assert!(!is_public_endpoint("/audit/custom"));
    assert!(is_public_endpoint("/public/get-book"));
    assert_eq!(table.push(units("/audit/custom")), 10);
    assert!(is_public_endpoint("/audit/custom"), "PUBLIC-LIVE-PUSH");
    assert_eq!(table.remove(1), Some(units("/public/get-book")));
    assert!(
        !is_public_endpoint("/public/get-book"),
        "PUBLIC-LIVE-REMOVE"
    );
    table.replace_contents(vec![None, Some(vec![0xd83d])]);
    assert!(is_public_endpoint("😀"), "PUBLIC-HALF-SURROGATE-NEEDLE");
    assert!(!is_public_endpoint(""), "PUBLIC-HOLE-SKIPPED");
    table.set(3, vec![0xd800]);
    assert!(
        is_public_endpoint_units(&[0x61, 0xd800, 0x62]),
        "PUBLIC-UTF16-URL"
    );
    table.delete(1);
    assert!(!is_public_endpoint("😀"), "PUBLIC-DELETE-SKIPS-HOLE");
    table.set(0, vec![]);
    assert!(is_public_endpoint(""), "PUBLIC-EMPTY-NEEDLE");
    table.replace_contents(original);
    assert!(!is_public_endpoint("/audit/custom"));
    assert!(is_public_endpoint("/public/get-book"));
    // A module array in a separate JS worker is a separate native thread's array.
    table.push(units("/audit/parent"));
    assert!(
        !std::thread::spawn(|| is_public_endpoint("/audit/parent"))
            .join()
            .unwrap(),
        "PUBLIC-THREAD-ISOLATION"
    );
    table.remove(9);
    let source = source();
    assert_eq!(
        source["table"],
        json!([false, true, true, false, true, false, true, false, true]),
        "PUBLIC-SOURCE-SEQUENCE"
    );
}

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/behaviour_repairs_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "BEHAVIOUR-SOURCE:{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn request_retains_root_nested_aliases_and_primitive_values() {
    let clock = VirtualClock::default();
    let mut params =
        value(json!({"counter":1,"child":{"value":2},"array":[{"value":3},4],"negzero":-0.0}));
    let old = params.clone();
    let child = params.property("child").unwrap();
    let array = params.property("array").unwrap();
    let element = array.element(0).unwrap();
    let request = generate_authenticated_request(
        "method",
        params.clone(),
        "key",
        "secret",
        Some(97.0),
        &clock,
    )
    .unwrap();
    assert!(
        request.params.same_reference(&params),
        "PARAMS-ROOT-IDENTITY"
    );
    let signed = request.sig.clone();
    assert!(params.set_property("counter".into(), value(json!(7))));
    assert_eq!(
        request.params.property("counter").unwrap().to_value(),
        json!(7),
        "PARAMS-LIVE-ROOT"
    );
    assert!(child.set_property("value".into(), value(json!(8))));
    assert_eq!(
        request
            .params
            .property("child")
            .unwrap()
            .property("value")
            .unwrap()
            .to_value(),
        json!(8),
        "PARAMS-LIVE-CHILD"
    );
    assert!(element.set_property("value".into(), value(json!(9))));
    assert_eq!(
        request
            .params
            .property("array")
            .unwrap()
            .element(0)
            .unwrap()
            .property("value")
            .unwrap()
            .to_value(),
        json!(9),
        "PARAMS-LIVE-ARRAY-CHILD"
    );
    assert!(
        request
            .params
            .set_property("reverse".into(), value(json!(11)))
    );
    assert_eq!(
        params.property("reverse").unwrap().to_value(),
        json!(11),
        "PARAMS-REVERSE-WRITE"
    );
    let primitive = array.element(1).unwrap();
    assert!(array.set_element(1, value(json!(5))));
    assert_eq!(primitive.to_value(), json!(4), "PARAMS-PRIMITIVE-CAPTURE");
    assert!(params.set_property("child".into(), value(json!({"value":10}))));
    assert_eq!(
        child.property("value").unwrap().to_value(),
        json!(8),
        "PARAMS-OLD-CHILD-RETAINED"
    );
    params = value(json!({"replacement":true}));
    assert!(
        request.params.same_reference(&old),
        "PARAMS-OLD-ROOT-RETAINED"
    );
    assert!(!request.params.same_reference(&params));
    assert_eq!(request.sig, signed, "PARAMS-SIGNATURE-STAYS-FROZEN");
    assert_eq!(
        request
            .params
            .property("negzero")
            .unwrap()
            .to_value()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits(),
        "PARAMS-NATIVE-NEGATIVE-ZERO"
    );
    let order = value(json!({"z":1,"a":2})).to_value();
    assert_eq!(
        order
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["z", "a"],
        "PARAMS-KEY-ORDER"
    );
    let mut scalar = value(json!(3));
    let scalar_request = generate_authenticated_request(
        "method",
        scalar.clone(),
        "key",
        "secret",
        Some(97.0),
        &clock,
    )
    .unwrap();
    scalar = value(json!({"replacement":true}));
    assert_eq!(
        scalar_request.params.to_value(),
        json!(3),
        "PARAMS-SCALAR-ROOT-CAPTURE"
    );
    assert!(scalar.property("replacement").is_some());
    assert!(
        !old.set_property("cycle".into(), old.clone()),
        "PARAMS-CYCLE-REJECTED"
    );
    assert!(
        old.set_property("otherAlias".into(), element.clone()),
        "PARAMS-DAG-ACCEPTED"
    );
    let shared = old.property("otherAlias").unwrap();
    assert!(shared.same_reference(&element));
    assert!(array.set_element(2, value(json!(6))));
    assert_eq!(array.remove_element(2).unwrap().to_value(), json!(6));
    assert_eq!(
        old.remove_property("reverse").unwrap().to_value(),
        json!(11)
    );
    let source = source();
    assert_eq!(
        source["params"],
        json!({"counter":7,"child":8,"arrayChild":9,"reverse":11,"primitive":4,"oldChild":8,"oldRoot":true,"scalar":3,"signedFrozen":true,"negativeZero":true,"keys":["z","a"]}),
        "PARAMS-SOURCE-OBSERVATIONS"
    );
}

#[test]
fn clock_mutation_precedes_signing_and_leaves_no_guard() {
    let params = value(json!({"counter":1}));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut clock = VirtualClock::default();
    clock.now = {
        let params = params.clone();
        let calls = calls.clone();
        Arc::new(move || {
            let n = calls.fetch_add(1, Ordering::SeqCst);
            assert!(
                params.set_property("counter".into(), value(json!(7 + n))),
                "PARAMS-CLOCK-WRITE"
            );
            1000.0 + n as f64
        })
    };
    let request =
        generate_authenticated_request("method", params.clone(), "key", "secret", None, &clock)
            .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        request.params.property("counter").unwrap().to_value(),
        json!(8)
    );
    let expected = generate_signature(
        "method",
        1000.0,
        "key",
        "secret",
        &json!({"counter":8}),
        1001.0,
    )
    .unwrap();
    assert_eq!(request.sig, expected, "PARAMS-CLOCK-BEFORE-SNAPSHOT");
    assert_eq!(
        request.sig, "9aabf2ce41e94e5574dee9a1674bf7b108356a0f7862cb8f8fda4b85cdc9535a",
        "PARAMS-CLOCK-PINNED-DIGEST"
    );
    assert!(
        params.set_property("counter".into(), value(json!(9))),
        "PARAMS-WRITE-WHILE-RESULT-ALIVE"
    );
    let invalid = value(json!({"a":{"b":{"c":{"toString":0}}}}));
    assert!(
        generate_authenticated_request(
            "method",
            invalid.clone(),
            "key",
            "secret",
            Some(97.0),
            &VirtualClock::default()
        )
        .is_err()
    );
    assert!(
        invalid.set_property("afterError".into(), value(json!(1))),
        "PARAMS-ERROR-LEAVES-NO-GUARD"
    );
    let source = source();
    assert_eq!(
        request.sig,
        source["clockDigest"].as_str().unwrap(),
        "PARAMS-SOURCE-CLOCK-DIGEST"
    );
}

#[test]
fn signing_stops_at_source_object_cutoff_on_shared_dag() {
    let mut diamond = value(json!({"leaf":0}));
    for _ in 0..100 {
        let parent = value(json!({}));
        assert!(parent.set_property("a".into(), diamond.clone()));
        assert!(parent.set_property("b".into(), diamond.clone()));
        assert!(
            !diamond.set_property("back".into(), parent.clone()),
            "PARAMS-ANCESTOR-CYCLE-REJECTED"
        );
        diamond = parent;
    }
    let mut clock = VirtualClock::default();
    clock.now = Arc::new(|| 1000.0);
    let request = generate_authenticated_request(
        "method",
        diamond.clone(),
        "key",
        "secret",
        Some(97.0),
        &clock,
    )
    .unwrap();
    assert!(request.params.same_reference(&diamond));
    assert_eq!(
        request.sig, "62ce60778be0dc112776f17a6bb21eafd2361dc2c3f1044409d5b4ee23a5f4e9",
        "PARAMS-DAG-CUTOFF"
    );
    let source = source();
    assert_eq!(
        request.sig,
        source["diamondDigest"].as_str().unwrap(),
        "PARAMS-SOURCE-DAG-CUTOFF"
    );
}

#[test]
fn request_error_stops_before_unvisited_array_dag() {
    let mut diamond = value(json!([]));
    for _ in 0..100 {
        let parent = value(json!([]));
        assert!(parent.set_element(0, diamond.clone()));
        assert!(parent.set_element(1, diamond.clone()));
        diamond = parent;
    }
    let root = value(json!({"z":null,"a":{"b":{"c":{"toString":0}}}}));
    assert!(root.set_property("z".into(), diamond));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut clock = VirtualClock::default();
    clock.now = {
        let calls = calls.clone();
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            1000.0
        })
    };
    let error =
        generate_authenticated_request("method", root, "key", "secret", None, &clock).unwrap_err();
    assert_eq!(calls.load(Ordering::SeqCst), 2, "PARAMS-ERROR-CLOCK-ORDER");
    assert_eq!(error.name, "TypeError", "PARAMS-EARLY-ERROR-NAME");
    assert_eq!(
        error.message, "Cannot convert object to primitive value",
        "PARAMS-EARLY-ERROR-MESSAGE"
    );
    assert_eq!(
        source()["errorBeforeLargeSibling"],
        true,
        "PARAMS-SOURCE-EARLY-ERROR"
    );
}

#[test]
fn private_classifier_preserves_full_code_unit_inputs() {
    let mut prefix = vec![0xd800];
    prefix.extend(units("/private/create-order"));
    let mut interior = units("/private/");
    interior.push(0xd800);
    interior.extend(units("create-order"));
    let mut suffix = units("/private/cancel-order");
    suffix.push(0xdfff);
    let advanced = units("prefix/private/advanced/cancel-order?x=1");
    let public = units("/public/get-book");
    let observed = [
        None,
        Some(&[][..]),
        Some(prefix.as_slice()),
        Some(interior.as_slice()),
        Some(suffix.as_slice()),
        Some(advanced.as_slice()),
        Some(public.as_slice()),
    ]
    .map(is_non_retryable_private_write_units);
    assert_eq!(
        observed,
        [false, false, true, false, true, true, false],
        "PRIVATE-UTF16-NATIVE"
    );
    assert_eq!(
        source()["private"],
        json!([false, false, true, false, true, true, false]),
        "PRIVATE-SOURCE-UTF16"
    );
}

#[test]
fn interval_returns_retained_referents_and_utf16_errors() {
    use safe_cex_num::prototype_realm::{PrototypeRealm, PrototypeScalar, PrototypeValue};
    use safe_cex_venue_cryptocom::types::{
        map_candlestick_interval, map_candlestick_interval_units,
    };
    let realm = std::rc::Rc::new(PrototypeRealm::default());
    let _binding = realm.bind();
    let PrototypeValue::Reference(first) = map_candlestick_interval("toString").unwrap() else {
        panic!("INTERVAL-REFERENCE-KIND")
    };
    let PrototypeValue::Reference(second) = map_candlestick_interval("toString").unwrap() else {
        panic!("INTERVAL-REFERENCE-KIND")
    };
    assert_eq!(first, second, "INTERVAL-REFERENT-IDENTITY");
    first.set("auditMarker", PrototypeScalar::Number(7.0));
    let PrototypeValue::Number(number) = second.get("auditMarker") else {
        panic!("INTERVAL-LIVE-REFERENT")
    };
    assert_eq!(number.to_bits(), 7.0f64.to_bits(), "INTERVAL-LIVE-REFERENT");
    let mut expected = units("Unsupported Crypto.com candlestick interval: ");
    expected.push(0xd800);
    assert_eq!(
        map_candlestick_interval_units(&[0xd800]).unwrap_err(),
        expected,
        "INTERVAL-UTF16-ERROR"
    );
    let source = source();
    assert_eq!(
        source["interval"],
        json!({"same":true,"marker":7,"errorUnits":expected}),
        "INTERVAL-SOURCE-REFERENT"
    );
}
