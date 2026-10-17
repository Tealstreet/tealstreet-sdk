use safe_cex_transport::signature_rejection::{
    IsSignatureRejection, SignatureRetry, resend_on_rejected_data, signature_rejected_response,
};
use safe_cex_transport::{RestBody, RestFailure, RestRequest, RestResponse, RestResponseBody};
use serde_json::{Value, json};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};

#[test]
fn reviewed_transport_error_origin_clones_preserve_the_same_native_cause() {
    use safe_cex_transport::provider::{TransportError, TransportErrorOrigin};
    use std::error::Error;

    let original = TransportErrorOrigin::new(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "signer refused",
    ));
    let copied = original.clone();
    assert_eq!(original, copied);
    assert!(std::ptr::eq(original.error(), copied.error()));
    let unrelated = TransportErrorOrigin::new(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "signer refused",
    ));
    assert_ne!(original, unrelated);
    drop(original);
    let failure = TransportError::Rejected(copied);
    let cloned_failure = failure.clone();
    assert!(std::ptr::eq(
        failure.source().unwrap(),
        cloned_failure.source().unwrap()
    ));
    drop(failure);
    let cause = cloned_failure
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .unwrap();
    assert_eq!(cause.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(cause.to_string(), "signer refused");
    assert_eq!(cloned_failure.to_string(), "signer refused");
}

#[test]
fn reviewed_transformed_data_clone_preserves_payload_and_completed_phase() {
    for (input, header, expected, bytes) in [
        (
            json!(" {\"x\":1} "),
            None,
            json!(" {\"x\":1} "),
            b" {\"x\":1} ".to_vec(),
        ),
        (
            json!({"z":false,"a":[true]}),
            Some("application/json"),
            json!("{\"z\":false,\"a\":[true]}"),
            b"{\"z\":false,\"a\":[true]}".to_vec(),
        ),
        (Value::Null, None, Value::Null, b"null".to_vec()),
        (
            json!(true),
            Some("application/json"),
            json!("true"),
            b"true".to_vec(),
        ),
        (
            json!("left"),
            Some("application/json"),
            json!("\"left\""),
            b"\"left\"".to_vec(),
        ),
    ] {
        let original = RestBody::Untransformed(input).transform(header).unwrap();
        let RestBody::Transformed(data) = &original else {
            panic!("TRANSFORM CLONE requires actual completed phase")
        };
        assert_eq!(
            data.value(),
            &expected,
            "TRANSFORM CLONE original native payload"
        );
        let copied = data.clone();
        assert_eq!(copied.value(), &expected);
        if let (Some(left), Some(right)) = (data.value().as_str(), copied.value().as_str()) {
            if !left.is_empty() {
                assert_ne!(
                    left.as_ptr(),
                    right.as_ptr(),
                    "TRANSFORM CLONE owns string payload"
                );
            }
        }
        drop(original);
        let copied = RestBody::Transformed(copied);
        assert_eq!(
            copied.wire_bytes_for_content_type(header).unwrap(),
            bytes,
            "TRANSFORM CLONE wire identity"
        );
        let again = copied
            .transform(Some("application/x-www-form-urlencoded"))
            .unwrap();
        let RestBody::Transformed(data) = &again else {
            panic!("TRANSFORM CLONE must retain completed phase")
        };
        assert_eq!(
            data.value(),
            &expected,
            "TRANSFORM CLONE identity on repeated transform"
        );
        assert_eq!(again.wire_bytes().unwrap(), bytes);
    }
}
#[test]
fn reviewed_signature_retry_clones_share_one_stamp_across_both_paths() {
    for via_error in [false, true] {
        for sibling_first in [false, true] {
            let data = json!({"code":-1022});
            let mut error =
                RestFailure::pre_transmit("signature rejected".into(), RestRequest::default());
            error.response = Some(Box::new(RestResponse {
                status: 401,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&data).unwrap()),
            }));
            let count = Rc::new(Cell::new(0));
            let observed = count.clone();
            let predicate: IsSignatureRejection = Rc::new(move |value| {
                observed.set(observed.get() + 1);
                Ok(value["code"] == json!(-1022))
            });
            let original = SignatureRetry::default();
            let sibling = original.clone();
            assert!(!original.has_retried());
            assert!(!sibling.has_retried());
            let first = if sibling_first { &sibling } else { &original };
            assert!(
                if via_error {
                    signature_rejected_response(&error, first, &predicate).unwrap()
                } else {
                    resend_on_rejected_data(&data, first, &predicate).unwrap()
                },
                "SIGNATURE CLONE first approval"
            );
            let late = original.clone();
            for copy in [&original, &sibling, &late] {
                assert!(copy.has_retried(), "SIGNATURE CLONE shared stamp");
                assert!(
                    !resend_on_rejected_data(&data, copy, &predicate).unwrap(),
                    "SIGNATURE CLONE resolved retry stays spent"
                );
                assert!(
                    !signature_rejected_response(&error, copy, &predicate).unwrap(),
                    "SIGNATURE CLONE error retry stays spent"
                );
            }
            assert_eq!(
                count.get(),
                1,
                "SIGNATURE CLONE refuses before repeated predicate"
            );
            assert!(
                !SignatureRetry::default().has_retried(),
                "separate operation stamp independent"
            );
        }
    }
}

#[test]
fn reviewed_network_failure_clone_preserves_diagnostics_and_retry_disposition() {
    use safe_cex_transport::{NetworkFailure, TransportError};
    for (code, retry) in [
        (None, false),
        (Some(""), false),
        (Some("ECONNRESET"), true),
        (Some("ECONNABORTED"), false),
        (Some("ERR_CANCELED"), false),
        (Some("ENOTFOUND"), false),
    ] {
        let message = format!("retained network diagnostic {code:?} \u{1f680}");
        let original = NetworkFailure::from_code(message.clone(), code.map(str::to_owned));
        let duplicate = original.clone();
        assert_eq!(
            duplicate, original,
            "NETWORK CLONE native diagnostic equality"
        );
        assert_eq!(original.code(), code);
        assert_eq!(duplicate.code(), code);
        if let Some(code) = code.filter(|code| !code.is_empty()) {
            assert_ne!(
                original.code().unwrap().as_ptr(),
                duplicate.code().unwrap().as_ptr(),
                "NETWORK CLONE owns code storage"
            );
            assert_eq!(duplicate.code().unwrap(), code);
        }
        let error = TransportError::Network(original);
        let copied = TransportError::Network(duplicate);
        assert_eq!(error.to_string(), message);
        assert_eq!(copied.to_string(), message);
        let failure = RestFailure::from_transport(error, RestRequest::default());
        let clone = RestFailure::from_transport(copied, RestRequest::default());
        assert_eq!(
            failure.is_retryable_network(),
            retry,
            "NETWORK CLONE original retry disposition"
        );
        assert_eq!(
            clone.is_retryable_network(),
            retry,
            "NETWORK CLONE duplicated retry disposition"
        );
        assert_eq!(failure.code.as_deref(), code);
        assert_eq!(clone.code.as_deref(), code);
        assert_eq!(
            failure, clone,
            "NETWORK CLONE unchanged native error carrier"
        );
        drop(failure);
        assert_eq!(clone.message, message);
        assert_eq!(clone.code.as_deref(), code);
    }
}
