use safe_cex_venue_coinbase::{starbase::*, starbase_rpc::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

type Sentinel = Rc<String>;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/rpc.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live RPC oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
fn kind(value: &Value) -> CoinbaseStarbaseTransportKind {
    match value.as_str().unwrap() {
        "http" => CoinbaseStarbaseTransportKind::Http,
        "websocket" => CoinbaseStarbaseTransportKind::Websocket,
        other => panic!("kind {other}"),
    }
}
fn params(value: Value) -> CoinbaseStarbaseJsonRpcParams {
    CoinbaseStarbaseJsonRpcParams::new(value)
}
fn inert() -> CoinbaseStarbaseTransport<Sentinel> {
    Rc::new(|_, _| Box::pin(async { Ok(Value::Null) }))
}
fn assert_protocol(actual: &CoinbaseStarbaseProtocolError, expected: &Value) {
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "native protocol error name"
    );
    assert_eq!(
        actual.message,
        expected["message"].as_str().unwrap(),
        "native protocol error message"
    );
    assert_eq!(
        actual.to_string(),
        expected["display"].as_str().unwrap(),
        "native protocol error display"
    );
}
fn assert_rpc_error(actual: &CoinbaseStarbaseRpcError, expected: &Value) {
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "native RPC error name"
    );
    assert_eq!(
        actual.message,
        expected["message"].as_str().unwrap(),
        "native RPC error message"
    );
    assert_eq!(
        actual.code.to_bits(),
        expected["code"].as_f64().unwrap().to_bits(),
        "native RPC error code"
    );
    assert_eq!(
        actual.data.as_ref(),
        expected.get("data"),
        "native RPC error data absence/null identity"
    );
    assert_eq!(
        actual.method,
        expected["method"].as_str().unwrap(),
        "native RPC error method"
    );
    assert_eq!(
        actual.request_id.map(f64::to_bits),
        expected
            .get("requestId")
            .and_then(Value::as_f64)
            .map(f64::to_bits),
        "native RPC error request ID"
    );
    assert_eq!(
        actual.to_string(),
        expected["display"].as_str().unwrap(),
        "native RPC error display"
    );
}
fn assert_result(actual: Result<Value, CoinbaseStarbaseRequestError<Sentinel>>, expected: &Value) {
    match (actual, expected.get("error")) {
        (Ok(actual), None) => assert_eq!(actual, expected["ok"], "decoded native RPC result"),
        (Err(CoinbaseStarbaseRequestError::Protocol(error)), Some(expected)) => {
            assert_protocol(&error, expected)
        }
        (Err(CoinbaseStarbaseRequestError::Rpc(error)), Some(expected)) => {
            assert_rpc_error(&error, expected)
        }
        (actual, expected) => panic!("native RPC result branch: {actual:?} vs {expected:?}"),
    }
}
fn assert_request(actual: &CoinbaseStarbaseRequestHandle, expected: &Value) {
    let actual = actual.borrow();
    assert_eq!(
        actual.jsonrpc,
        CoinbaseStarbaseJsonRpcVersion::V2,
        "native RPC version variant"
    );
    assert_eq!(expected["jsonrpc"], "2.0");
    assert_eq!(
        actual.id.to_bits(),
        expected["id"].as_f64().unwrap().to_bits(),
        "native request ID"
    );
    assert_eq!(
        actual.method,
        expected["method"].as_str().unwrap(),
        "native request method"
    );
    assert_eq!(
        &*actual.params.borrow(),
        &expected["params"],
        "native request params"
    );
}
fn assert_calls(
    actual: &[(
        CoinbaseStarbaseRequestHandle,
        CoinbaseStarbaseTransportContext,
    )],
    expected: &Value,
) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "native transport call count");
    for ((request, context), expected) in actual.iter().zip(expected) {
        assert_request(request, &expected["request"]);
        assert_eq!(
            context.kind,
            kind(&expected["context"]["kind"]),
            "native context kind"
        );
        assert_eq!(
            context.endpoint,
            expected["context"]["endpoint"].as_str().unwrap(),
            "native context endpoint"
        );
        assert_eq!(
            context.authorization.as_deref(),
            expected["context"]
                .get("authorization")
                .and_then(Value::as_str),
            "native context authorization"
        );
    }
}

#[test]
fn client_constructor_and_build_validation_match_live_source() {
    let cases = oracle()["constructors"].as_array().unwrap();
    assert_eq!(cases.len(), 13, "constructor corpus floor");
    for case in cases {
        let actual = CoinbaseStarbaseJsonRpcClient::new(
            inert(),
            CoinbaseStarbaseTransportKind::Http,
            case.get("initial"),
        );
        match actual {
            Ok(client) => assert_request(
                &client
                    .build_request(Some(&json!("public/test")), params(json!({})))
                    .unwrap(),
                &case["result"]["ok"],
            ),
            Err(error) => assert_protocol(&error, &case["result"]["error"]),
        }
    }
    let cases = oracle()["builds"].as_array().unwrap();
    assert_eq!(cases.len(), 20, "build corpus floor");
    for case in cases {
        let client = CoinbaseStarbaseJsonRpcClient::new(
            inert(),
            CoinbaseStarbaseTransportKind::Http,
            Some(&json!(7)),
        )
        .unwrap();
        match client.build_request(case.get("method"), params(case["params"].clone())) {
            Ok(request) => assert_request(&request, &case["result"]["ok"]),
            Err(error) => assert_protocol(&error, &case["result"]["error"]),
        }
        assert_request(
            &client
                .build_request(Some(&json!("public/test")), params(json!({})))
                .unwrap(),
            &case["next"],
        );
    }
    let client = CoinbaseStarbaseJsonRpcClient::new(
        inert(),
        CoinbaseStarbaseTransportKind::Http,
        Some(&json!(9_007_199_254_740_991_u64)),
    )
    .unwrap();
    let cases = oracle()["exhaustion"].as_array().unwrap();
    assert_eq!(cases.len(), 3, "exhaustion corpus floor");
    for (case, (method, input)) in cases.iter().zip([
        ("public/test", json!({})),
        ("public/test", json!({})),
        ("invalid", json!([])),
    ]) {
        match client.build_request(Some(&json!(method)), params(input)) {
            Ok(request) => assert_request(&request, &case["ok"]),
            Err(error) => assert_protocol(&error, &case["error"]),
        }
    }
}

#[test]
fn client_response_precedence_and_native_fields_match_live_source() {
    let cases = oracle()["responses"].as_array().unwrap();
    assert_eq!(cases.len(), 26, "response corpus floor");
    for case in cases {
        let response = case["response"].clone();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let capture = calls.clone();
        let transport: CoinbaseStarbaseTransport<Sentinel> = Rc::new(move |request, context| {
            capture.borrow_mut().push((request, context));
            let response = response.clone();
            Box::pin(async move { Ok(response) })
        });
        let client = CoinbaseStarbaseJsonRpcClient::new(
            transport,
            CoinbaseStarbaseTransportKind::Http,
            Some(&json!(7)),
        )
        .unwrap();
        assert_result(
            ready(client.request(
                Some(&json!("private/read")),
                params(json!({"marker":"native-param"})),
                Some("Bearer native-auth".into()),
            )),
            &case["result"],
        );
        assert_calls(&calls.borrow(), &case["calls"]);
    }
}

#[test]
fn http_transport_builds_native_envelope_and_preserves_sender_error() {
    let mut wire_pairs = Vec::new();
    let cases = oracle()["httpCases"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "HTTP corpus floor");
    for case in cases {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let capture = calls.clone();
        let send: CoinbaseStarbaseHttpSender<Sentinel> = Rc::new(move |request| {
            capture.borrow_mut().push(request);
            Box::pin(async { Ok(json!({"transportMarker":"native-response"})) })
        });
        let transport = create_coinbase_starbase_http_transport(send);
        let request = CoinbaseStarbaseRequestHandle::new(CoinbaseStarbaseJsonRpcRequest {
            jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
            id: 17.0,
            method: case["request"]["method"].as_str().unwrap().into(),
            params: params(case["request"]["params"].clone()),
        });
        let context = &case["context"];
        assert_result(
            ready(transport(
                request.clone(),
                CoinbaseStarbaseTransportContext {
                    endpoint: context["endpoint"].as_str().unwrap().into(),
                    kind: kind(&context["kind"]),
                    authorization: context
                        .get("authorization")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                },
            )),
            &case["result"],
        );
        let actual = calls.borrow();
        let expected = case["calls"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "native sender count");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_request(&actual.body, &expected["body"]);
            assert!(
                actual.body.shares_request(&request),
                "HTTP body retains original request object"
            );
            assert_eq!(
                actual.method,
                CoinbaseStarbaseHttpMethod::Post,
                "native HTTP POST variant"
            );
            assert_eq!(
                actual.url,
                expected["url"].as_str().unwrap(),
                "native HTTP URL"
            );
            assert_eq!(
                actual.headers,
                expected["headers"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
                    .collect::<indexmap::IndexMap<String, String>>(),
                "native HTTP headers"
            );
            assert_eq!(
                actual.headers.keys().collect::<Vec<_>>(),
                expected["headers"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>(),
                "native header insertion order"
            );
            wire_pairs.push((actual.clone(), expected.clone()));
        }
    }
    let sentinel = Rc::new("native sender error".to_owned());
    let sent = sentinel.clone();
    let send: CoinbaseStarbaseHttpSender<Sentinel> = Rc::new(move |_| {
        let sent = sent.clone();
        Box::pin(async move { Err(sent) })
    });
    let client = CoinbaseStarbaseJsonRpcClient::new(
        create_coinbase_starbase_http_transport(send),
        CoinbaseStarbaseTransportKind::Http,
        None,
    )
    .unwrap();
    match ready(client.request(Some(&json!("public/test")), params(json!({})), None)) {
        Err(CoinbaseStarbaseRequestError::Transport(error)) => assert!(
            Rc::ptr_eq(&error, &sentinel),
            "sender failure identity propagates unchanged"
        ),
        other => panic!("sender failure {other:?}"),
    }
    // Complete the native assertions for every case before the first wire assertion.
    for (actual, expected) in wire_pairs {
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            expected,
            "HTTP wire envelope"
        );
    }
}

#[test]
fn request_aliases_eager_ids_and_transport_failures_match_live_source() {
    let client =
        CoinbaseStarbaseJsonRpcClient::new(inert(), CoinbaseStarbaseTransportKind::Http, None)
            .unwrap();
    let input = params(json!({"marker":"before"}));
    let request = client
        .build_request(Some(&json!("public/test")), input.clone())
        .unwrap();
    input
        .borrow_object_mut()
        .unwrap()
        .insert("marker".into(), json!("after"));
    assert!(request.borrow().params.shares_object(&input));
    assert_eq!(oracle()["parameterAlias"]["same"], true);
    assert_eq!(
        request.borrow().params.borrow()["marker"],
        oracle()["parameterAlias"]["marker"],
        "native live params alias"
    );
    let transport: CoinbaseStarbaseTransport<Sentinel> = Rc::new(|request, _| {
        request.borrow_mut().id = 99.0;
        request.borrow_mut().method = "private/changed".into();
        Box::pin(async { Ok(json!({"jsonrpc":"2.0","id":7,"result":"unreachable"})) })
    });
    let client = CoinbaseStarbaseJsonRpcClient::new(
        transport,
        CoinbaseStarbaseTransportKind::Http,
        Some(&json!(7)),
    )
    .unwrap();
    assert_result(
        ready(client.request(Some(&json!("public/test")), params(json!({})), None)),
        &oracle()["requestAlias"],
    );
    let calls = Rc::new(RefCell::new(Vec::new()));
    let capture = calls.clone();
    let transport: CoinbaseStarbaseTransport<Sentinel> = Rc::new(move |request, _| {
        let id = request.borrow().id;
        capture.borrow_mut().push(id);
        Box::pin(async move { Ok(json!({"jsonrpc":"2.0","id":id,"result":id})) })
    });
    let client = CoinbaseStarbaseJsonRpcClient::new(
        transport,
        CoinbaseStarbaseTransportKind::Http,
        Some(&json!(7)),
    )
    .unwrap();
    let first = client.request(Some(&json!("public/test")), params(json!({})), None);
    let second = client.request(Some(&json!("public/test")), params(json!({})), None);
    assert_eq!(
        *calls.borrow(),
        vec![7.0, 8.0],
        "request IDs and transport calls advance before await"
    );
    assert_eq!(oracle()["eager"]["callsBeforeAwait"], json!([7, 8]));
    assert_eq!(ready(first).unwrap().as_f64(), Some(7.0));
    assert_eq!(ready(second).unwrap().as_f64(), Some(8.0));
    let sentinel = Rc::new("native transport failure".to_owned());
    let sent = sentinel.clone();
    let transport: CoinbaseStarbaseTransport<Sentinel> = Rc::new(move |_, _| {
        let sent = sent.clone();
        Box::pin(async move { Err(CoinbaseStarbaseRequestError::Transport(sent)) })
    });
    let client = CoinbaseStarbaseJsonRpcClient::new(
        transport,
        CoinbaseStarbaseTransportKind::Http,
        Some(&json!(7)),
    )
    .unwrap();
    match ready(client.request(Some(&json!("public/test")), params(json!({})), None)) {
        Err(CoinbaseStarbaseRequestError::Transport(error)) => assert!(
            Rc::ptr_eq(&error, &sentinel),
            "transport failure identity propagates unchanged"
        ),
        other => panic!("transport error {other:?}"),
    }
    assert_eq!(oracle()["transportFailure"]["same"], true);
    assert_eq!(
        client
            .build_request(Some(&json!("public/test")), params(json!({})))
            .unwrap()
            .borrow()
            .id,
        oracle()["transportFailure"]["nextId"].as_f64().unwrap(),
        "failed request consumes ID"
    );
}

#[test]
fn rpc_error_and_declared_response_variants_keep_native_fields() {
    let cases = oracle()["rpcErrors"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "RPC error constructor corpus floor");
    for case in cases {
        let error = CoinbaseStarbaseRpcError::new(
            CoinbaseStarbaseJsonRpcErrorObject {
                code: case["raw"]["code"].as_f64().unwrap(),
                message: case["raw"]["message"].as_str().unwrap().into(),
                data: case["raw"].get("data").cloned(),
            },
            "private/method-identity".into(),
            case.get("requestId").and_then(Value::as_f64),
        );
        assert_rpc_error(&error, &case["result"]);
    }
    let success = CoinbaseStarbaseJsonRpcResponse::Success(CoinbaseStarbaseJsonRpcSuccess {
        jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
        id: 7.0,
        result: json!({"marker":"native-result"}),
    });
    let CoinbaseStarbaseJsonRpcResponse::Success(value) = &success else {
        panic!("native success variant")
    };
    assert_eq!(value.jsonrpc, CoinbaseStarbaseJsonRpcVersion::V2);
    assert_eq!(value.id.to_bits(), 7.0_f64.to_bits());
    assert_eq!(value.result["marker"], "native-result");
    let error =
        CoinbaseStarbaseJsonRpcResponse::<Value>::Error(CoinbaseStarbaseJsonRpcErrorResponse {
            jsonrpc: CoinbaseStarbaseJsonRpcVersion::V2,
            id: None,
            error: CoinbaseStarbaseJsonRpcErrorObject {
                code: -32001.0,
                message: "native-message".into(),
                data: Some(Value::Null),
            },
        });
    let CoinbaseStarbaseJsonRpcResponse::Error(value) = &error else {
        panic!("native error variant")
    };
    assert_eq!(value.jsonrpc, CoinbaseStarbaseJsonRpcVersion::V2);
    assert_eq!(value.id, None);
    assert_eq!(value.error.code.to_bits(), (-32001.0_f64).to_bits());
    assert_eq!(value.error.message, "native-message");
    assert_eq!(value.error.data, Some(Value::Null));
    assert_eq!(
        serde_json::to_value(success).unwrap(),
        json!({"jsonrpc":"2.0","id":7,"result":{"marker":"native-result"}})
    );
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({"jsonrpc":"2.0","error":{"code":-32001.0,"message":"native-message","data":null}})
    );
}

#[test]
fn heartbeat_constructor_and_operations_match_live_source() {
    let cases = oracle()["heartbeatConstructors"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "heartbeat constructor corpus floor");
    for case in cases {
        let interval = match case["interval"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            text => text.parse().unwrap(),
        };
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(inert(), kind(&case["kind"]), None).unwrap(),
        );
        match CoinbaseStarbaseHeartbeatController::new(client, interval) {
            Ok(_) => assert_eq!(case["result"]["ok"], true),
            Err(error) => assert_protocol(&error, &case["result"]["error"]),
        }
    }
    let cases = oracle()["heartbeats"].as_array().unwrap();
    assert_eq!(cases.len(), 16, "heartbeat operation corpus floor");
    for case in cases {
        let response = case["response"].clone();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let capture = calls.clone();
        let transport: CoinbaseStarbaseTransport<Sentinel> = Rc::new(move |request, context| {
            let id = request.borrow().id;
            capture.borrow_mut().push((request, context));
            let response = response.clone();
            Box::pin(async move { Ok(json!({"jsonrpc":"2.0","id":id,"result":response})) })
        });
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(
                transport,
                CoinbaseStarbaseTransportKind::Websocket,
                Some(&json!(7)),
            )
            .unwrap(),
        );
        let heartbeat = CoinbaseStarbaseHeartbeatController::new(client.clone(), 10.5).unwrap();
        let actual = match case["action"].as_str().unwrap() {
            "enable" => ready(heartbeat.enable()).map(|()| None),
            "disable" => ready(heartbeat.disable()).map(|()| None),
            "heartbeat" => {
                ready(heartbeat.handle_signal(CoinbaseStarbaseHeartbeatSignal::Heartbeat))
            }
            "test_request" => {
                ready(heartbeat.handle_signal(CoinbaseStarbaseHeartbeatSignal::TestRequest))
            }
            other => panic!("heartbeat action {other}"),
        };
        match actual {
            Ok(value) => {
                assert!(
                    case["result"].get("error").is_none(),
                    "native heartbeat success branch"
                );
                assert_eq!(
                    value.is_none(),
                    case["result"]["returnedUndefined"].as_bool().unwrap(),
                    "native heartbeat absence"
                );
                if let Some(value) = value {
                    assert_eq!(value, case["result"]["ok"], "native heartbeat test result");
                }
            }
            Err(CoinbaseStarbaseRequestError::Protocol(error)) => {
                assert_protocol(&error, &case["result"]["error"])
            }
            other => panic!("heartbeat error {other:?}"),
        }
        assert_calls(&calls.borrow(), &case["calls"]);
        assert_eq!(
            client
                .build_request(Some(&json!("public/test")), params(json!({})))
                .unwrap()
                .borrow()
                .id
                .to_bits(),
            case["nextId"].as_f64().unwrap().to_bits(),
            "native heartbeat ID consumption"
        );
    }
}

#[test]
fn request_declaration_preserves_record_boundary_and_required_fields() {
    let expected = &oracle()["requestDeclaration"];
    assert_eq!(
        expected["constraint"], "CoinbaseStarbaseJsonRpcParams",
        "source request record constraint"
    );
    assert_eq!(
        expected["paramsAlias"], "Record<string, unknown>",
        "source params record alias"
    );
    let parsed = syn::parse_file(include_str!("../src/starbase_rpc.rs")).unwrap();
    let request = parsed
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Struct(item) if item.ident == "CoinbaseStarbaseJsonRpcRequest" => Some(item),
            _ => None,
        })
        .unwrap();
    assert!(
        request.generics.params.is_empty(),
        "request uses the shared params handle, never an unconstrained scalar generic"
    );
    let fields: Vec<Value> = request.fields.iter().map(|field| {
        let syn::Type::Path(ty) = &field.ty else { panic!("native request field type") };
        let native_type = ty.path.segments.last().unwrap().ident.to_string();
        let source_type = match native_type.as_str() {
            "CoinbaseStarbaseJsonRpcVersion" => "'2.0'",
            "f64" => "number",
            "String" => "string",
            "CoinbaseStarbaseJsonRpcParams" => "Params",
            _ => panic!("unclassified native request field: {native_type}"),
        };
        json!({"name":field.ident.as_ref().unwrap().to_string(), "optional":false, "type":source_type})
    }).collect();
    assert_eq!(fields.len(), 4, "independent request field floor");
    assert_eq!(
        json!(fields),
        expected["fields"],
        "source-versus-native request record declaration"
    );
}
