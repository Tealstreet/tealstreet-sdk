use safe_cex_num::JsJsonValue as J;
use safe_cex_transport::query::axios_default_query;
use serde_json::json;

#[test]
fn default_query_matches_axios_traversal_and_encoding() {
    let cases = [
        (json!({"a":[1,2]}), "a%5B%5D=1&a%5B%5D=2"),
        (json!({"a":"f[g]h"}), "a=f%5Bg%5Dh"),
        (json!({"a":"x~y!z(1)'"}), "a=x~y!z(1)'"),
        (json!({"a":{"b":1}}), "a%5Bb%5D=1"),
        (json!({"a":[{"b":1}]}), "a%5B0%5D%5Bb%5D=1"),
        (
            json!({"tags[]":["a b",null,"x+y"]}),
            "tags%5B%5D=a+b&tags%5B%5D=x%2By",
        ),
        (json!({" x ":1," nest ":{" y ":2}}), "x=1&+nest+%5By%5D=2"),
        (json!({"10":1,"2":2,"a":3}), "2=2&10=1&a=3"),
        (json!({"a":[[],[null,null]],"b":[]}), ""),
        (json!({"a":":$, + /?&=ñ"}), "a=:$,+%2B+%2F%3F%26%3D%C3%B1"),
    ];
    for (params, expected) in cases {
        assert_eq!(
            axios_default_query(&params.clone().into()).unwrap(),
            expected,
            "Axios params {params}"
        );
    }
}

fn object(fields: &[(&str, J)]) -> J {
    J::Object(
        fields
            .iter()
            .map(|(key, value)| ((*key).into(), value.clone()))
            .collect(),
    )
}
#[test]
fn default_query_keeps_native_numbers_until_the_selected_output_phase() {
    let params = object(&[
        ("p", J::Number(f64::NAN)),
        ("i", J::Number(f64::INFINITY)),
        ("n", J::Number(f64::NEG_INFINITY)),
        ("z", J::Number(-0.0)),
        ("skip", J::Undefined),
        ("nil", J::Null),
    ]);
    assert_eq!(
        axios_default_query(&params).unwrap(),
        "p=NaN&i=Infinity&n=-Infinity&z=0"
    );
    let nested = object(&[(
        "a",
        object(&[
            ("b", J::Number(f64::NAN)),
            (
                "c",
                J::Array(vec![J::Number(f64::INFINITY), J::Number(-0.0)]),
            ),
        ]),
    )]);
    assert_eq!(
        axios_default_query(&nested).unwrap(),
        "a%5Bb%5D=NaN&a%5Bc%5D%5B0%5D=Infinity&a%5Bc%5D%5B1%5D=0"
    );
    let metadata = object(&[(
        "meta{}",
        object(&[
            ("n", J::Number(f64::NAN)),
            ("i", J::Number(f64::INFINITY)),
            ("z", J::Number(-0.0)),
        ]),
    )]);
    assert_eq!(
        axios_default_query(&metadata).unwrap(),
        "meta%7B%7D=%7B%22n%22:null,%22i%22:null,%22z%22:0%7D"
    );
    let arraylike = object(&[(
        "list[]",
        object(&[
            ("0", object(&[("a", J::Number(1.0))])),
            ("2", J::Number(f64::NAN)),
            ("length", J::Number(3.0)),
        ]),
    )]);
    assert_eq!(
        axios_default_query(&arraylike).unwrap(),
        "list%5B%5D=%5Bobject+Object%5D&list%5B%5D=NaN"
    );
}

#[test]
fn actual_installed_axios_agrees_on_uri_and_physical_http_bytes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/axios-default-query-source.cjs"
        ))
        .output()
        .expect("installed Axios source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("15 getUri + 15 physical request targets PASS")
    );
}

#[test]
fn default_query_keeps_metadata_and_path_depth_boundaries() {
    fn nested(count: usize) -> J {
        (0..count).fold(J::Bool(true), |child, _| object(&[("next", child)]))
    }
    let metadata = object(&[("payload{}", nested(100))]);
    assert_eq!(
        axios_default_query(&metadata).unwrap(),
        format!(
            "payload%7B%7D={}true{}",
            "%7B%22next%22:".repeat(100),
            "%7D".repeat(100)
        )
    );
    assert_eq!(
        axios_default_query(&object(&[("payload{}", nested(101))]))
            .unwrap_err()
            .to_string(),
        "Object is too deeply nested (101 levels). Max depth: 100"
    );
    assert_eq!(
        axios_default_query(&nested(101)).unwrap(),
        format!("next{}=true", "%5Bnext%5D".repeat(100))
    );
    assert_eq!(
        axios_default_query(&nested(102)).unwrap_err().to_string(),
        "Object is too deeply nested (101 levels). Max depth: 100"
    );
    assert_eq!(
        axios_default_query(&object(&[(
            "a",
            J::Array(vec![J::Bool(true), J::Bool(false), J::BigInt(3.into())])
        )]))
        .unwrap(),
        "a%5B%5D=true&a%5B%5D=false&a%5B%5D=3"
    );
    assert_eq!(
        axios_default_query(&object(&[("a", J::Number(1.0)), ("a", J::Number(2.0))])).unwrap(),
        "a=2"
    );
}
