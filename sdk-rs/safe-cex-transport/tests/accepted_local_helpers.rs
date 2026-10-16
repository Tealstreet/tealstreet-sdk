//! Native output and retry-role witnesses for source-pinned local helpers.
use safe_cex_transport::RestBody;
use serde_json::{Value, json};
const FORM: &str = "application/x-www-form-urlencoded";
fn encode(value: Value) -> Result<String, String> {
    RestBody::Untransformed(value)
        .wire_bytes_for_content_type(Some(FORM))
        .map(|bytes| String::from_utf8(bytes).unwrap())
        .map_err(|error| error.to_string())
}
fn nested(containers: usize) -> Value {
    (0..containers).fold(Value::Bool(true), |child, _| json!({"next": child}))
}
#[test]
fn form_output_helpers_retain_actual_keys_coercion_order_and_omissions() {
    assert_eq!(
        encode(json!({" a ":true,"list":[1,null,"x y"],"inner":{" z[] ":"v+"}})).unwrap(),
        "a=true&list%5B%5D=1&list%5B%5D=x+y&inner%5Bz%5D=v%2B",
        "FORM HELPERS native wire pairs"
    );
    assert_eq!(
        encode(json!({"items[]":{"length":3,"2":false,"1":null,"0":"x y"},"skip":null})).unwrap(),
        "items%5B%5D=x+y&items%5B%5D=false",
        "FORM HELPERS native array-like order"
    );
    assert_eq!(
        encode(json!({"items[]":{"length":1.5,"0":"never"}})).unwrap_err(),
        "Invalid array length",
        "FORM HELPERS native length refusal"
    );
}
#[test]
fn form_metadata_depth_validator_keeps_exact_boundary_and_native_error() {
    let admitted = encode(json!({"payload{}": nested(100)})).unwrap();
    let expected = format!(
        "payload%7B%7D={}true{}",
        "%7B%22next%22%3A".repeat(100),
        "%7D".repeat(100)
    );
    assert_eq!(
        admitted, expected,
        "FORM HELPERS metadata boundary native bytes"
    );
    assert_eq!(
        encode(json!({"payload{}": nested(101)})).unwrap_err(),
        "Object is too deeply nested (101 levels). Max depth: 100",
        "FORM HELPERS metadata depth refusal"
    );
}
#[test]
fn form_path_depth_validator_keeps_its_distinct_boundary_and_native_error() {
    let expected = format!("next{}=true", "%5Bnext%5D".repeat(100));
    assert_eq!(
        encode(nested(101)).unwrap(),
        expected,
        "FORM HELPERS path boundary native bytes"
    );
    assert_eq!(
        encode(nested(102)).unwrap_err(),
        "Object is too deeply nested (101 levels). Max depth: 100",
        "FORM HELPERS path depth refusal"
    );
}

#[test]
fn signature_marker_helper_retains_method_selection_and_error_order() {
    use safe_cex_transport::{RestResponse, RestResponseBody, retry::is_error_signature};
    for (body, expected, error_field) in [
        (json!({}), false, None),
        (json!({"ret_msg":null,"retMsg":"error sign!"}), true, None),
        (json!({"ret_msg":"prefix error sign! suffix"}), true, None),
        (json!({"ret_msg":["error sign!"]}), true, None),
        (json!({"ret_msg":["prefix error sign!"]}), false, None),
        (
            json!({"ret_msg":42,"retMsg":"error sign!"}),
            false,
            Some("ret_msg"),
        ),
        (
            json!({"ret_msg":false,"retMsg":"error sign!"}),
            false,
            Some("ret_msg"),
        ),
        (json!({"ret_msg":"none","retMsg":[]}), false, None),
        (
            json!({"ret_msg":"none","retMsg":{"includes":true}}),
            false,
            Some("retMsg"),
        ),
        (json!({"ret_msg":"error sign!","retMsg":42}), true, None),
    ] {
        let response = RestResponse {
            status: 400,
            reason: None,
            headers: Default::default(),
            body: RestResponseBody::Transformed(body),
        };
        match error_field {
            None => assert_eq!(
                is_error_signature(&response).unwrap(),
                expected,
                "LOCAL RETRY native method selection"
            ),
            Some(field) => {
                let error = is_error_signature(&response).unwrap_err();
                assert_eq!(error.name, "TypeError", "LOCAL RETRY native error name");
                assert_eq!(
                    error.message,
                    format!("error?.response?.data?.{field}?.includes is not a function"),
                    "LOCAL RETRY native error order"
                );
            }
        }
    }
}

#[test]
fn query_output_helpers_sort_only_views_and_keep_encoding_a_profile_choice() {
    use safe_cex_transport::query::{
        QueryArrayFormat, QueryKeyOrder, QueryProfile, query_identity_eq, render_query,
    };
    use safe_cex_types::QueryValue as Q;
    let input = Q::Object(
        [
            ("z".into(), Q::String("a b/+".into())),
            ("n".into(), Q::Number(-0.0)),
            (
                "a".into(),
                Q::Array(vec![Q::Bool(false), Q::Null, Q::Undefined]),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let before = input.clone();
    for (encode, expected) in [
        (false, "a=false&a=&n=0&z=a b/+"),
        (true, "a=false&a=&n=0&z=a%20b%2F%2B"),
    ] {
        assert_eq!(
            render_query(
                &input,
                QueryProfile {
                    arrays: QueryArrayFormat::Repeat,
                    encode,
                    key_order: QueryKeyOrder::JsRoot
                }
            )
            .unwrap(),
            expected,
            "LOCAL QUERY native sorted view bytes"
        );
        assert!(
            query_identity_eq(&input, &before),
            "LOCAL QUERY source values/order/bits unchanged"
        );
    }
}
