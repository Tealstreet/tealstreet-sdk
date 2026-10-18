use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_transport::{RestFailure, RestRequest, RestResponse, RestResponseBody};
use serde_json::{Value, json};
use std::error::Error;

#[test]
fn encoded_rest_rejections_keep_original_bytes_when_the_view_is_not_json() {
    // A malformed response must not replace the original rejection with a parser error.
    let cases: [(&[u8], Value); 2] = [
        (b"not-json-response", json!("not-json-response")),
        (
            br#"{"message":"wire detail","marker":42}"#,
            json!({"message":"wire detail","marker":42}),
        ),
    ];
    for (bytes, view) in cases {
        let mut failure =
            RestFailure::pre_transmit("original rejection".into(), RestRequest::default());
        failure.response = Some(Box::new(RestResponse {
            status: 502,
            reason: None,
            headers: Default::default(),
            body: RestResponseBody::Encoded(bytes.to_vec()),
        }));
        let error = ExchangeError::from(failure);
        assert_eq!(
            error.message, "original rejection",
            "encoded REST keeps original rejection"
        );
        let native = error
            .source()
            .unwrap()
            .downcast_ref::<RestFailure>()
            .unwrap();
        let response = native.response.as_ref().unwrap();
        assert_eq!(response.status, 502, "encoded native original status");
        assert_eq!(response.reason, None, "encoded native missing reason");
        let RestResponseBody::Encoded(original) = &response.body else {
            panic!("encoded native original phase")
        };
        assert_eq!(original, bytes, "encoded native original bytes");
        let projected = error.response_json_view().unwrap();
        assert_eq!(
            projected["data"], view,
            "secondary encoded REST response view"
        );
        assert!(
            !projected.as_object().unwrap().contains_key("statusText"),
            "encoded view keeps absent reason absent"
        );
    }
}

#[test]
fn response_authority_is_nearest_origin_and_keeps_original_cause() {
    use safe_cex_exchange::runtime::ExchangeErrorOrigin;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let mut failure = RestFailure::pre_transmit("original".into(), RestRequest::default());
    failure.response = Some(Box::new(RestResponse {
        status: 400,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Transformed(Input::Object(vec![(
            vec![0xd800],
            Input::Float(-0.0),
        )])),
    }));
    let original = ExchangeError::from(failure);
    let mut chained = ExchangeError::new("outer");
    chained.origin = Some(ExchangeErrorOrigin::new(original));
    let underlying = chained.retained_response_input().unwrap();
    let native = safe_cex_exchange::js_runtime::value::from_retained_input(underlying.clone());
    assert_eq!(
        native
            .get("data")
            .unwrap()
            .get_key(&[0xd800])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0f64).to_bits()
    );
    let nearest = Input::Object(vec![
        ("data".encode_utf16().collect(), Input::Text(vec![0xdfff])),
        ("missing".encode_utf16().collect(), Input::Undefined),
    ]);
    chained = chained.with_retained_response(nearest.clone());
    assert_eq!(chained.retained_response_input(), Some(nearest));
    assert_eq!(chained.response_json_view(), Some(json!({"data":"�"})));
    assert!(
        chained
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<ExchangeError>()
    );
    let nearer = Input::Object(vec![("data".encode_utf16().collect(), Input::Float(-0.0))]);
    chained = chained.with_retained_response(nearer.clone());
    assert_eq!(chained.retained_response_input(), Some(nearer));
    let cause = chained.source().unwrap().source().unwrap();
    assert!(
        cause
            .source()
            .unwrap()
            .source()
            .unwrap()
            .is::<RestFailure>()
    );
}

#[test]
fn api_response_wrapper_keeps_a_store_subscriber_cause_on_the_source_chain() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    use std::rc::Rc;
    let original = Rc::new(std::io::Error::other("original subscriber failure"));
    let mut error = ExchangeError::new("outer");
    error.store_subscriber_cause =
        Some(safe_cex_types::StoreSubscriberError::new("subscriber").with_cause(original.clone()));
    let response = Input::Object(vec![("data".encode_utf16().collect(), Input::Float(-0.0))]);
    let error = error.with_retained_response(response.clone());
    assert_eq!(error.retained_response_input(), Some(response));
    let mut source = error.source();
    let mut retained = None;
    while let Some(cause) = source {
        if let Some(cause) = cause.downcast_ref::<std::io::Error>() {
            retained = Some(cause);
            break;
        }
        source = cause.source();
    }
    assert!(
        retained.is_some_and(|cause| std::ptr::eq(cause, original.as_ref())),
        "response wrapper must preserve the exact subscriber cause"
    );
}

#[test]
fn response_authority_skips_an_outer_transport_without_a_response() {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let mut original =
        RestFailure::pre_transmit("inner HTTP rejection".into(), RestRequest::default());
    original.response = Some(Box::new(RestResponse {
        status: 409,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Transformed(Input::Text(vec![0xd800, 0xdfff])),
    }));
    let outer = RestFailure::from_transport(
        safe_cex_transport::TransportError::Rejected(
            safe_cex_transport::provider::TransportErrorOrigin::new(original),
        ),
        RestRequest::default(),
    );
    let error = ExchangeError::from(outer);
    let response = error
        .retained_response_input()
        .expect("nearest response after empty transport origin");
    assert_eq!(
        response.get("data"),
        Some(&Input::Text(vec![0xd800, 0xdfff]))
    );
    assert_eq!(response.get("status"), Some(&Input::Float(409.0)));
}
