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
        let projected = error.response.as_ref().unwrap();
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
