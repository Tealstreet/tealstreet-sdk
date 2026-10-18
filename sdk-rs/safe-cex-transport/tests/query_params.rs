use safe_cex_transport::query::{QueryArrayFormat, QueryProfile, query_identity_eq, render_query};
use safe_cex_types::QueryValue as Q;
use std::{path::Path, process::Command};

fn object(values: Vec<(&str, Q)>) -> Q {
    Q::Object(
        values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
fn inputs() -> [Q; 3] {
    [
        object(vec![
            ("limit", Q::Number(f64::NEG_INFINITY)),
            ("startAt", Q::Number(f64::NAN)),
            ("endAt", Q::Number(f64::INFINITY)),
            ("nil", Q::Null),
            ("absent", Q::Undefined),
            (
                "list",
                Q::Array(vec![
                    Q::Number(f64::NAN),
                    Q::Number(f64::INFINITY),
                    Q::Number(f64::NEG_INFINITY),
                    Q::Null,
                    Q::Undefined,
                    Q::String("a b".into()),
                ]),
            ),
        ]),
        object(vec![
            ("list", Q::Array(vec![])),
            ("empty", object(vec![])),
            ("flag", Q::Bool(false)),
            ("zero", Q::Number(-0.0)),
            ("textZero", Q::String("-0".into())),
            ("textOverflow", Q::String("1e400".into())),
        ]),
        object(vec![
            (
                "list",
                Q::Array(vec![
                    Q::Array(vec![Q::Number(1.0), Q::Undefined]),
                    Q::Array(vec![Q::Null, Q::Number(2.0)]),
                ]),
            ),
            (
                "nested",
                object(vec![
                    ("a", Q::Number(f64::NAN)),
                    ("b", Q::Undefined),
                    ("c", Q::Null),
                ]),
            ),
        ]),
    ]
}

#[test]
fn source_query_profiles_preserve_native_values_and_exact_wire() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new("node")
        .arg(root.join("sdk-rs/safe-cex-types/tests/query_value.cjs"))
        .output()
        .expect("query source process");
    assert!(
        result.status.success(),
        "QUERY source execution: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let source: Vec<serde_json::Value> =
        serde_json::from_slice(&result.stdout).expect("source corpus");
    let retained: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../safe-cex-types/tests/query_value_profiles.json"
    ))
    .unwrap();
    assert_eq!(source.len(), 6, "QUERY source total");
    assert_eq!(retained.len(), 6, "QUERY retained total");
    let native = inputs();
    let Q::Object(first) = &native[0] else {
        panic!("QUERY native first object")
    };
    assert!(
        matches!(first.get("startAt"), Some(Q::Number(n)) if n.is_nan()),
        "QUERY native NaN identity"
    );
    assert!(
        matches!(first.get("limit"), Some(Q::Number(n)) if *n == f64::NEG_INFINITY),
        "QUERY native negative infinity identity"
    );
    let Q::Object(second) = &native[1] else {
        panic!("QUERY native second object")
    };
    assert!(
        matches!(second.get("zero"), Some(Q::Number(n)) if n.to_bits() == (-0.0f64).to_bits()),
        "QUERY native signed zero identity"
    );
    assert_eq!(
        second.get("textZero"),
        Some(&Q::String("-0".into())),
        "QUERY numeric-looking string identity"
    );
    for index in 0..6 {
        assert_eq!(
            source[index].get("input"),
            retained[index].get("input"),
            "QUERY source input content {index}"
        );
        assert!(
            source[index].get("input").is_some(),
            "QUERY required source input {index}"
        );
        assert_eq!(
            source[index].get("file"),
            retained[index].get("file"),
            "QUERY source profile identity {index}"
        );
        let profile = QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: if index < 3 {
                QueryArrayFormat::Indexed
            } else {
                QueryArrayFormat::Repeat
            },
            encode: true,
        };
        let actual = render_query(&native[index % 3], profile).unwrap();
        assert_eq!(
            Some(actual.as_str()),
            source[index]
                .get("output")
                .and_then(serde_json::Value::as_str),
            "QUERY exact source rendering {index}"
        );
    }
}

#[test]
fn retry_identity_retains_nan_and_distinguishes_zero_sign() {
    let native = inputs();
    assert!(
        query_identity_eq(&native[0], &native[0].clone()),
        "QUERY retry NaN survives clone"
    );
    assert!(
        !query_identity_eq(&Q::Number(-0.0), &Q::Number(0.0)),
        "QUERY retry signed zero identity"
    );
    assert!(
        !query_identity_eq(&Q::Number(12.25), &Q::Number(12.5)),
        "QUERY retry finite identity"
    );
}

#[test]
fn signing_wire_preserves_numeric_tokens_and_distinct_omissions() {
    use safe_cex_transport::query_signing::{SignedConfig, SigningConfig};
    let request: SigningConfig = serde_json::from_str(r#"{"method":"GET","url":"/fixture","baseURL":"","headers":{},"params":{"zero":-0,"text":"-0","huge":1e400,"word":"1e400","finite":12.25}}"#).unwrap();
    let response: SignedConfig = serde_json::from_str(r#"{"headers":{},"params":{"zero":-0,"text":"-0","huge":1e400,"word":"1e400","finite":12.25}}"#).unwrap();
    for params in [&request.params, &response.params] {
        let Some(Q::Object(values)) = params else {
            panic!("QUERY wire native object")
        };
        assert_eq!(values.len(), 5, "QUERY wire native field count");
        assert!(
            matches!(values.get("zero"), Some(Q::Number(n)) if n.to_bits() == (-0.0f64).to_bits()),
            "QUERY wire native signed zero"
        );
        assert!(
            matches!(values.get("huge"), Some(Q::Number(n)) if *n == f64::INFINITY),
            "QUERY wire native overflow"
        );
        assert!(
            matches!(values.get("finite"), Some(Q::Number(n)) if n.to_bits() == 12.25f64.to_bits()),
            "QUERY wire native finite bits"
        );
        assert_eq!(
            values.get("text"),
            Some(&Q::String("-0".into())),
            "QUERY wire native string zero"
        );
        assert_eq!(
            values.get("word"),
            Some(&Q::String("1e400".into())),
            "QUERY wire native string overflow"
        );
    }
    for (params, omitted) in [
        (None, true),
        (Some(Q::Undefined), true),
        (Some(Q::Null), false),
    ] {
        let request = SigningConfig {
            params: params.clone(),
            ..Default::default()
        };
        let response = SignedConfig {
            params,
            ..Default::default()
        };
        let request_wire = serde_json::to_value(&request).unwrap();
        let response_wire = serde_json::to_value(&response).unwrap();
        for wire in [&request_wire, &response_wire] {
            assert_eq!(
                wire.get("params").is_none(),
                omitted,
                "QUERY signing wire omission identity"
            );
            if !omitted {
                assert_eq!(
                    wire.get("params"),
                    Some(&serde_json::Value::Null),
                    "QUERY signing explicit null"
                );
            }
        }
        let request_back: SigningConfig = serde_json::from_str(&request_wire.to_string()).unwrap();
        let response_back: SignedConfig = serde_json::from_str(&response_wire.to_string()).unwrap();
        assert_eq!(
            request_back.params,
            if omitted { None } else { Some(Q::Null) },
            "QUERY request decoded omission"
        );
        assert_eq!(
            response_back.params,
            if omitted { None } else { Some(Q::Null) },
            "QUERY response decoded omission"
        );
    }
}

#[test]
fn query_object_parse_keeps_native_numbers_and_leaves_scalar_strings() {
    use safe_cex_transport::query::try_parse_query_json;
    let parsed = try_parse_query_json(Q::String(
        r#"{"zero":-0,"overflow":1e400,"string":"1e400"}"#.into(),
    ));
    let Q::Object(values) = parsed else {
        panic!("QUERY parsed native object")
    };
    assert_eq!(values.len(), 3, "QUERY parsed field count");
    assert!(
        matches!(values.get("zero"), Some(Q::Number(n)) if n.to_bits() == (-0.0f64).to_bits()),
        "QUERY parsed signed zero"
    );
    assert!(
        matches!(values.get("overflow"), Some(Q::Number(n)) if *n == f64::INFINITY),
        "QUERY parsed overflow"
    );
    assert_eq!(
        values.get("string"),
        Some(&Q::String("1e400".into())),
        "QUERY parsed string identity"
    );
    for text in ["-0", "1e400", "null", "NaN", "false", "\"word\"", "{"] {
        assert_eq!(
            try_parse_query_json(Q::String(text.into())),
            Q::String(text.into()),
            "QUERY scalar or invalid string remains unchanged"
        );
    }
}

#[test]
fn signing_parser_orders_native_integer_keys_before_url_or_wire() {
    use safe_cex_transport::query::try_parse_query_json;
    let input =
        r#"{"tail":"last","2":"b","1":"a","nested":{"2":"d","1":"c"},"array":[{"2":"f","1":"e"}]}"#;
    let parsed = try_parse_query_json(Q::String(input.into()));
    let Q::Object(values) = &parsed else {
        panic!("QUERY signing parsed object")
    };
    assert_eq!(
        values.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2", "tail", "nested", "array"],
        "QUERY signing native own-key order"
    );
    let Some(Q::Object(nested)) = values.get("nested") else {
        panic!("QUERY signing native nested object")
    };
    assert_eq!(
        nested.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2"],
        "QUERY signing native nested own-key order"
    );
    let Some(Q::Array(array)) = values.get("array") else {
        panic!("QUERY signing native array")
    };
    let Q::Object(nested) = &array[0] else {
        panic!("QUERY signing native array object")
    };
    assert_eq!(
        nested.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2"],
        "QUERY signing native array own-key order"
    );
    assert_eq!(
        values.get("1"),
        Some(&Q::String("a".into())),
        "QUERY signing native first value identity"
    );
    let legacy =
        safe_cex_transport::signing::try_parse_params_json(serde_json::Value::String(input.into()));
    assert_eq!(
        legacy
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["1", "2", "tail", "nested", "array"],
        "QUERY legacy signing native own-key order"
    );
    assert_eq!(
        render_query(
            &parsed,
            QueryProfile {
                key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                arrays: QueryArrayFormat::Indexed,
                encode: false
            }
        )
        .unwrap(),
        "1=a&2=b&tail=last&nested[1]=c&nested[2]=d&array[0][1]=e&array[0][2]=f",
        "QUERY signing ordered query bytes"
    );
}

#[test]
fn native_signer_wire_preserves_js_exponents_and_enforces_owned_depth_budget() {
    use safe_cex_transport::query_signing::{SignedConfig, SigningConfig};
    let native: SignedConfig = serde_json::from_str(
        r#"{"params":{"magnitude":1e+21,"zero":-0,"text":"1e+21"},"headers":{}}"#,
    )
    .unwrap();
    let Some(Q::Object(values)) = &native.params else {
        panic!("QUERY native wire object")
    };
    assert!(
        matches!(values.get("magnitude"), Some(Q::Number(n)) if *n==1e21),
        "QUERY native exponent value"
    );
    assert!(
        matches!(values.get("zero"), Some(Q::Number(n)) if n.to_bits()==(-0.0f64).to_bits()),
        "QUERY native wire zero sign"
    );
    assert_eq!(
        values.get("text"),
        Some(&Q::String("1e+21".into())),
        "QUERY native exponent string identity"
    );
    for (depth, accepted) in [(127, true), (128, false)] {
        let mut deep = serde_json::json!(0);
        for _ in 0..depth {
            deep = serde_json::json!({"child":deep});
        }
        let envelope =
            serde_json::json!({"method":"GET","url":"/","baseURL":"","headers":{},"params":deep});
        let result = serde_json::from_value::<SigningConfig>(envelope);
        assert_eq!(
            result.is_ok(),
            accepted,
            "QUERY owned wire depth boundary {depth}"
        );
        if let Ok(decoded) = result {
            let mut node = decoded.params.as_ref().unwrap();
            for _ in 0..depth {
                let Q::Object(map) = node else {
                    panic!("QUERY owned depth native object")
                };
                node = map.get("child").unwrap();
            }
            assert_eq!(node, &Q::Number(0.0), "QUERY owned depth native leaf");
        }
    }
    let wire = serde_json::to_string(&native).unwrap();
    assert!(
        wire.contains(r#""magnitude":1e+21"#),
        "QUERY JS exponent bytes"
    );
    assert!(
        !wire.contains(r#""data""#),
        "QUERY absent native data omitted"
    );
}
