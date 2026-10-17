use safe_cex_transport::RestResponseBody;
fn main() {
    let body = RestResponseBody::Encoded(b"null".to_vec());
    let value = body.json_value().expect("literal JSON");
    assert!(value.is_null());
    let value = safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap();
    assert_eq!(
        RestResponseBody::Untransformed(value.clone()).into_data(),
        value
    );
    assert_eq!(
        RestResponseBody::Transformed(value.clone()).into_data(),
        value
    );
}
