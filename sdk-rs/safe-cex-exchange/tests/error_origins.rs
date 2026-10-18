use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_types::StoreSubscriberError;
use std::{cell::Cell, error::Error, fmt, rc::Rc};

#[derive(Debug)]
struct ListenerOrigin {
    code: u32,
    live: Rc<Cell<u32>>,
}
impl fmt::Display for ListenerOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("opaque listener failure")
    }
}
impl Error for ListenerOrigin {}

#[test]
fn shared_store_conversion_retains_opaque_listener_origin() {
    let store = DefaultStore::new();
    let live = Rc::new(Cell::new(7));
    let original: Rc<dyn Error> = Rc::new(ListenerOrigin {
        code: 413,
        live: live.clone(),
    });
    let first_calls = Rc::new(Cell::new(0));
    let later_calls = Rc::new(Cell::new(0));
    let first = first_calls.clone();
    let cause = original.clone();
    let _first_cleanup = Store::subscribe(
        &store,
        Box::new(move |data, _| {
            assert!(
                data.options.is_hedged,
                "origin callback observes committed setting"
            );
            first.set(first.get() + 1);
            Err(StoreSubscriberError::new("opaque listener failure").with_cause(cause.clone()))
        }),
    );
    let later = later_calls.clone();
    let _later_cleanup = Store::subscribe(
        &store,
        Box::new(move |_, _| {
            later.set(later.get() + 1);
            Ok(())
        }),
    );
    let error = ExchangeError::from(store.set_setting("isHedged", true).unwrap_err());
    assert!(store.options().is_hedged, "origin setting stays committed");
    assert_eq!(first_calls.get(), 1, "origin first subscriber calls");
    assert_eq!(later_calls.get(), 0, "origin aborts later subscriber");
    assert_eq!(
        error.store_subscriber_ids,
        [0],
        "origin subscriber identity"
    );
    let direct = error
        .source()
        .expect("shared conversion retains opaque subscriber cause");
    assert!(
        std::ptr::eq(direct, original.as_ref()),
        "shared conversion direct original opaque identity"
    );
    let retained = error
        .origin
        .as_ref()
        .expect("shared conversion retains typed origin")
        .error();
    let mutation = retained
        .downcast_ref::<safe_cex_store::StoreMutationError>()
        .expect("shared conversion retains whole Store mutation");
    let opaque = mutation
        .source()
        .unwrap()
        .source()
        .unwrap()
        .source()
        .unwrap();
    assert!(
        std::ptr::eq(opaque, original.as_ref()),
        "shared conversion original opaque identity"
    );
    assert_eq!(
        opaque.downcast_ref::<ListenerOrigin>().unwrap().code,
        413,
        "shared conversion opaque code"
    );
    live.set(42);
    assert_eq!(
        opaque.downcast_ref::<ListenerOrigin>().unwrap().live.get(),
        42,
        "shared conversion live opaque state"
    );
    let cloned = error.clone();
    assert!(
        std::ptr::eq(cloned.origin.as_ref().unwrap().error(), retained),
        "shared conversion clone retains origin identity"
    );
    assert!(
        std::ptr::eq(cloned.source().unwrap(), direct),
        "shared conversion clone retains direct opaque identity"
    );
}

#[test]
fn shared_rest_conversion_retains_typed_request_response_and_transport() {
    use safe_cex_transport::{
        NetworkFailure, RestFailure, RestRequest, RestResponse, RestResponseBody, TransportError,
    };
    use safe_cex_types::QueryValue;
    use serde_json::json;
    let request = RestRequest {
        method: "post".into(),
        url: "/origin-fixture".into(),
        params: Some(json!({"symbol":"BTC_USDT","orderId":"42"})),
        query_params: Some(QueryValue::Number(f64::NAN)),
        headers: indexmap::IndexMap::from([(
            "X-Origin-Fixture".into(),
            "request-marker-97".into(),
        )]),
        ..Default::default()
    };
    let mut failure = RestFailure::from_transport(
        TransportError::Network(NetworkFailure::from_code(
            "native transport".into(),
            Some("ORIGIN_CODE".into()),
        )),
        request,
    );
    failure.context = Some(Box::new(json!({"phase":"cancel","marker":42})));
    failure.response = Some(Box::new(RestResponse {
        status: 503,
        reason: Some("Fixture Unavailable".into()),
        headers: std::collections::BTreeMap::from([("x-fixture".into(), "response-13".into())]),
        body: RestResponseBody::Transformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"message":"response detail","marker":84}),
            )
            .unwrap(),
        ),
    }));
    let error = ExchangeError::from(failure);
    // Native originating values precede any JSON metadata projection.
    assert_eq!(
        error.message, "native transport",
        "REST retains original transport message"
    );
    let retained = error
        .source()
        .unwrap()
        .downcast_ref::<RestFailure>()
        .expect("REST typed original failure");
    assert_eq!(
        retained.request.method, "post",
        "REST native request method"
    );
    assert_eq!(
        retained.request.url, "/origin-fixture",
        "REST native request URL"
    );
    assert_eq!(
        retained.request.headers["X-Origin-Fixture"], "request-marker-97",
        "REST native request headers"
    );
    assert_eq!(
        retained.request.params.as_ref().unwrap()["orderId"],
        "42",
        "REST native request parameter"
    );
    let QueryValue::Number(number) = retained.request.query_params.as_ref().unwrap() else {
        panic!("REST native nonfinite query remains Number")
    };
    assert_eq!(
        number.to_bits(),
        f64::NAN.to_bits(),
        "REST native query Number bits"
    );
    let response = retained.response.as_ref().unwrap();
    assert_eq!(response.status, 503, "REST native response status");
    assert_eq!(
        response.reason.as_deref(),
        Some("Fixture Unavailable"),
        "REST native reason"
    );
    assert_eq!(
        response.headers["x-fixture"], "response-13",
        "REST native response headers"
    );
    let RestResponseBody::Transformed(body) = &response.body else {
        panic!("REST native response phase")
    };
    assert_eq!(
        body.clone().into_value().unwrap()["marker"],
        84,
        "REST native response body"
    );
    assert_eq!(
        retained.code.as_deref(),
        Some("ORIGIN_CODE"),
        "REST native code"
    );
    assert_eq!(
        retained.context.as_deref().unwrap()["marker"],
        42,
        "REST native context"
    );
    let transport = retained
        .source()
        .unwrap()
        .downcast_ref::<TransportError>()
        .unwrap();
    assert!(
        std::ptr::eq(transport, retained.transport_error.as_deref().unwrap()),
        "REST source chain original transport identity"
    );
    assert_eq!(
        transport.to_string(),
        "native transport",
        "REST native transport message"
    );
    let cloned = error.clone();
    assert_eq!(cloned, error, "REST cloned error retains pointer equality");
    assert!(
        std::ptr::eq(
            cloned
                .source()
                .unwrap()
                .downcast_ref::<RestFailure>()
                .unwrap(),
            retained
        ),
        "REST cloned error same native origin"
    );
    let independent = ExchangeError::from(retained.clone());
    assert_ne!(
        error, independent,
        "REST independent allocations stay distinct origins"
    );
    assert!(
        !format!("{error:?}").contains("request-marker-97"),
        "REST origin Debug omits retained request payload"
    );
    assert_eq!(
        error
            .code
            .as_ref()
            .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap()),
        Some(json!("ORIGIN_CODE")),
        "REST metadata code view"
    );
    assert_eq!(
        error.response.as_ref().unwrap()["data"]["marker"],
        84,
        "REST metadata response view"
    );
    assert_eq!(
        error.context.as_ref().unwrap()["marker"],
        42,
        "REST metadata context view"
    );
}

#[test]
fn shared_store_conversion_keeps_non_notification_origin_and_recovered_core_origin() {
    use safe_cex_exchange::runtime::ExchangeErrorOrigin;
    use safe_cex_store::{StoreMutationError, StoreNotificationError};
    let error = ExchangeError::from(StoreMutationError::InvalidOperation(
        "invalid origin fixture",
    ));
    assert!(
        matches!(
            error.source().unwrap().downcast_ref::<StoreMutationError>(),
            Some(StoreMutationError::InvalidOperation(
                "invalid origin fixture"
            ))
        ),
        "whole non-notification Store failure retained"
    );
    let mut original = ExchangeError::new("earlier core failure");
    original.name = "RangeError".into();
    original.code = Some(safe_cex_exchange::js_runtime::value::from_json(
        &serde_json::json!(42),
    ));
    original.origin = Some(ExchangeErrorOrigin::new(ListenerOrigin {
        code: 197,
        live: Rc::new(Cell::new(13)),
    }));
    let subscriber = StoreSubscriberError::from(original.clone());
    let original_subscriber_cause = subscriber.clone();
    let notification = StoreNotificationError::from_subscriber(7, subscriber);
    let recovered = ExchangeError::from(StoreMutationError::Notification(notification));
    let mut expected = original.clone();
    expected.store_subscriber_ids.push(7);
    expected.store_subscriber_cause = Some(original_subscriber_cause.clone());
    assert_eq!(
        recovered, expected,
        "recovered core metadata and subscriber identity"
    );
    assert!(
        std::ptr::eq(
            recovered.source().unwrap(),
            original_subscriber_cause.source().unwrap()
        ),
        "recovered core keeps original ExchangeError identity"
    );
    let retained_core = recovered
        .source()
        .unwrap()
        .downcast_ref::<ExchangeError>()
        .expect("recovered original core remains downcastable");
    assert_eq!(
        retained_core, &original,
        "recovered core original native fields"
    );
    assert!(
        std::ptr::eq(retained_core.source().unwrap(), original.source().unwrap()),
        "recovered core keeps earlier origin through source chain"
    );
}

#[test]
fn rest_error_json_view_keeps_exact_native_body_in_original_failure() {
    use safe_cex_transport::{
        NetworkFailure, RestFailure, RestRequest, RestResponse, RestResponseBody, TransportError,
    };
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let nan_bits = 0x7ff8_0000_0000_0042;
    let mut failure = RestFailure::from_transport(
        TransportError::Network(NetworkFailure::from_code("retained body".into(), None)),
        RestRequest::default(),
    );
    failure.response = Some(Box::new(RestResponse {
        status: 502,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Transformed(Input::Object(vec![
            (
                "nan".encode_utf16().collect(),
                Input::Float(f64::from_bits(nan_bits)),
            ),
            ("zero".encode_utf16().collect(), Input::Float(-0.0)),
            ("own".encode_utf16().collect(), Input::Undefined),
            (vec![0xd800], Input::Text(vec![0xd801])),
        ])),
    }));
    let error = ExchangeError::from(failure);
    let cloned = error.clone();
    assert!(
        std::ptr::eq(
            error.origin.as_ref().unwrap().error(),
            cloned.origin.as_ref().unwrap().error()
        ),
        "retained body origin identity survives cloning"
    );
    let retained = error
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<RestFailure>()
        .unwrap();
    let RestResponseBody::Transformed(Input::Object(fields)) =
        &retained.response.as_ref().unwrap().body
    else {
        panic!("retained body native transform phase")
    };
    assert_eq!(fields.len(), 4, "retained body keeps every own key");
    let Input::Float(nan) = fields[0].1 else {
        panic!("native NaN")
    };
    assert_eq!(nan.to_bits(), nan_bits, "retained body exact NaN payload");
    let Input::Float(zero) = fields[1].1 else {
        panic!("native signed zero")
    };
    assert_eq!(
        zero.to_bits(),
        (-0.0_f64).to_bits(),
        "retained body exact zero sign"
    );
    assert_eq!(
        fields[2].1,
        Input::Undefined,
        "retained body owns undefined"
    );
    assert_eq!(
        fields[3],
        (vec![0xd800], Input::Text(vec![0xd801])),
        "retained body exact key and value units"
    );
    let view = &error.response.as_ref().unwrap()["data"];
    assert!(
        view["nan"].is_null(),
        "legacy JSON view projects nonfinite to null"
    );
    assert_eq!(view["zero"], 0, "legacy JSON view projects zero sign");
    assert!(
        view.get("own").is_none(),
        "legacy JSON view omits undefined"
    );
    assert_eq!(
        view["\u{fffd}"], "\u{fffd}",
        "legacy JSON view replaces lone surrogates"
    );
}
