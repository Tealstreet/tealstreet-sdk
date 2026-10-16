use safe_cex_venue_coinbase::{starbase::*, starbase_auth::*, starbase_rpc::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
type E = Rc<String>;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/auth-session.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live auth oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn number(value: &Value) -> f64 {
    match value.get("special").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        _ => value.as_f64().unwrap(),
    }
}
fn kind(text: &str) -> CoinbaseStarbaseTransportKind {
    match text {
        "http" => CoinbaseStarbaseTransportKind::Http,
        "websocket" => CoinbaseStarbaseTransportKind::Websocket,
        _ => panic!("unknown kind"),
    }
}
fn error_fields(error: &CoinbaseStarbaseSessionFailure<E>, expected: &Value) {
    let (name, message) = match error.as_ref() {
        CoinbaseStarbaseSessionError::Request(CoinbaseStarbaseRequestError::Protocol(error)) => {
            (&*error.name, &*error.message)
        }
        CoinbaseStarbaseSessionError::Request(CoinbaseStarbaseRequestError::Rpc(error)) => {
            (&*error.name, &*error.message)
        }
        CoinbaseStarbaseSessionError::Coercion(error) => (error.name, error.message),
        other => panic!("unexpected native auth failure {other:?}"),
    };
    assert_eq!(
        name,
        expected["name"].as_str().unwrap(),
        "native auth error name"
    );
    assert_eq!(
        message,
        expected["message"].as_str().unwrap(),
        "native auth error message"
    );
}
fn auth_value(token: &str, expires: &Value) -> Value {
    json!({"access_token":token,"expires_in":expires,"scope":"scope identity","token_type":"bearer","extra":{"marker":"retained"}})
}
fn assert_calls(
    calls: &[(
        CoinbaseStarbaseRequestHandle,
        CoinbaseStarbaseTransportContext,
    )],
    expected: &Value,
) {
    let expected = expected.as_array().unwrap();
    assert_eq!(calls.len(), expected.len(), "native auth RPC call total");
    for ((request, context), expected) in calls.iter().zip(expected) {
        let request = request.borrow();
        assert_eq!(request.jsonrpc, CoinbaseStarbaseJsonRpcVersion::V2);
        assert_eq!(
            request.id.to_bits(),
            expected["request"]["id"].as_f64().unwrap().to_bits(),
            "native auth request ID"
        );
        assert_eq!(
            request.method,
            expected["request"]["method"].as_str().unwrap(),
            "native auth request method"
        );
        assert_eq!(
            &*request.params.borrow(),
            &expected["request"]["params"],
            "native auth request params"
        );
        assert_eq!(
            context.kind,
            kind(expected["context"]["kind"].as_str().unwrap()),
            "native auth context kind"
        );
        assert_eq!(
            context.endpoint,
            expected["context"]["endpoint"].as_str().unwrap(),
            "native auth context endpoint"
        );
        assert_eq!(
            context.authorization.as_deref(),
            expected["context"]
                .get("authorization")
                .and_then(Value::as_str),
            "native auth context authorization"
        );
    }
}
#[test]
fn auth_constructor_nullish_default_and_interval_validation_match_source() {
    let cases = oracle()["constructors"].as_array().unwrap();
    assert_eq!(cases.len(), 26, "independent auth constructor floor");
    for case in cases {
        let host = Microtasks::default();
        let transport: CoinbaseStarbaseTransport<E> =
            Rc::new(|_, _| Box::pin(async { Ok(Value::Null) }));
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(
                transport,
                kind(case["kind"].as_str().unwrap()),
                None,
            )
            .unwrap(),
        );
        let provider: CoinbaseStarbaseCdpJwtProvider<E> =
            Rc::new(|| CoinbaseStarbasePromise::resolved(Ok("jwt".into())));
        let refresh = &case["refresh"];
        let value = if refresh.get("missing").is_some() {
            None
        } else if refresh.get("special").is_some() {
            Some(CoinbaseStarbaseRefreshInterval::Number(number(refresh)))
        } else {
            Some(CoinbaseStarbaseRefreshInterval::Json(refresh.clone()))
        };
        match CoinbaseStarbaseAuthSession::new(
            client,
            provider,
            None,
            value.as_ref(),
            host.spawner(),
        ) {
            Ok(_) => assert_eq!(
                case["result"]["ok"], true,
                "native auth constructor acceptance"
            ),
            Err(error) => {
                assert_eq!(
                    error.name,
                    case["result"]["error"]["name"].as_str().unwrap()
                );
                assert_eq!(
                    error.message,
                    case["result"]["error"]["message"].as_str().unwrap()
                );
            }
        }
    }
}
#[test]
fn auth_expiry_force_alias_mutation_headers_and_failures_match_source() {
    let runs = oracle()["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 29, "independent auth scenario floor");
    let mut steps_seen = 0;
    for run in runs {
        let spec = &run["spec"];
        let host = Microtasks::default();
        let now = Rc::new(Cell::new(0.0));
        let clock_calls = Rc::new(Cell::new(0));
        let jwt_calls = Rc::new(Cell::new(0));
        let calls = Rc::new(RefCell::new(Vec::new()));
        let capture = calls.clone();
        let response_count = Rc::new(Cell::new(0));
        let count = response_count.clone();
        let responses = spec.get("responses").cloned();
        let expires = spec.get("expires").cloned().unwrap_or(json!(2));
        let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, context| {
            let index = count.get();
            count.set(index + 1);
            let id = request.borrow().id;
            capture.borrow_mut().push((request, context));
            let result = responses
                .as_ref()
                .map(|v| v[index].clone())
                .unwrap_or_else(|| auth_value(&format!("access-{index}"), &expires));
            Box::pin(async move { Ok(json!({"jsonrpc":"2.0","id":id,"result":result})) })
        });
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(
                transport,
                kind(spec.get("kind").and_then(Value::as_str).unwrap_or("http")),
                Some(&json!(7)),
            )
            .unwrap(),
        );
        let clock_now = now.clone();
        let clock_count = clock_calls.clone();
        let clock: CoinbaseStarbaseClock<E> = Rc::new(move || {
            clock_count.set(clock_count.get() + 1);
            Ok(clock_now.get())
        });
        let provider_count = jwt_calls.clone();
        let tokens = spec.get("tokens").cloned();
        let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || {
            let index = provider_count.get();
            provider_count.set(index + 1);
            CoinbaseStarbasePromise::resolved(Ok(tokens
                .as_ref()
                .and_then(|v| v[index].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("jwt-{index}"))))
        });
        let session = CoinbaseStarbaseAuthSession::new(
            client,
            provider,
            Some(clock),
            spec.get("refresh")
                .cloned()
                .map(CoinbaseStarbaseRefreshInterval::Json)
                .as_ref(),
            host.spawner(),
        )
        .unwrap();
        let mut last: Option<CoinbaseStarbaseAuthResultHandle> = None;
        let steps = spec["steps"].as_array().unwrap();
        let expected_steps = run["steps"].as_array().unwrap();
        assert_eq!(steps.len(), expected_steps.len());
        for (step, expected) in steps.iter().zip(expected_steps) {
            now.set(number(&step["now"]));
            if let Some(edit) = step.get("edit") {
                let mut result = last.as_ref().unwrap().borrow_mut();
                for (key, value) in edit.as_object().unwrap() {
                    result.insert(key.clone(), value.clone());
                }
            }
            if let Some(remove) = step.get("remove").and_then(Value::as_str) {
                last.as_ref().unwrap().borrow_mut().remove(remove);
            }
            let force = step["force"].as_bool().unwrap_or(false);
            let mut same = false;
            if step["action"] == "header" {
                let promise = session.get_http_authorization(force);
                host.drain();
                match completed(promise) {
                    Ok(value) => assert_eq!(
                        value,
                        expected["outcome"]["ok"].as_str().unwrap(),
                        "native bearer authorization"
                    ),
                    Err(error) => error_fields(&error, &expected["outcome"]["error"]),
                }
            } else {
                let promise = session.authenticate(force);
                host.drain();
                match completed(promise) {
                    Ok(value) => {
                        assert_eq!(
                            &*value.borrow(),
                            &expected["outcome"]["ok"],
                            "native auth result and retained extra fields"
                        );
                        same = last.as_ref().is_some_and(|last| last.shares_result(&value));
                        last = Some(value);
                    }
                    Err(error) => error_fields(&error, &expected["outcome"]["error"]),
                }
            }
            assert_eq!(
                same,
                expected["same"].as_bool().unwrap(),
                "native cached result identity"
            );
            assert_eq!(
                jwt_calls.get() as u64,
                expected["jwtCalls"].as_u64().unwrap(),
                "native JWT provider count"
            );
            assert_eq!(
                clock_calls.get(),
                expected["clockCalls"].as_u64().unwrap(),
                "native clock count"
            );
            assert_eq!(
                calls.borrow().len() as u64,
                expected["rpcCalls"].as_u64().unwrap(),
                "native authentication RPC count"
            );
            steps_seen += 1;
        }
        assert_calls(&calls.borrow(), &run["calls"]);
    }
    assert_eq!(steps_seen, 71, "independent auth operation floor");
}
#[test]
fn ignored_authentication_is_hot_and_consumers_cannot_drive_it() {
    let host = Microtasks::default();
    let count = Rc::new(Cell::new(0));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let capture = calls.clone();
    let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, _| {
        let id = request.borrow().id;
        capture.borrow_mut().push(id);
        Box::pin(async move {
            Ok(
                json!({"jsonrpc":"2.0","id":id,"result":auth_value(&format!("access-{}",id as u64),&json!(60))}),
            )
        })
    });
    let client = Rc::new(
        CoinbaseStarbaseJsonRpcClient::new(
            transport,
            CoinbaseStarbaseTransportKind::Http,
            Some(&json!(7)),
        )
        .unwrap(),
    );
    let provider_count = count.clone();
    let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || {
        provider_count.set(provider_count.get() + 1);
        CoinbaseStarbasePromise::resolved(Ok("jwt".into()))
    });
    let session = CoinbaseStarbaseAuthSession::new(
        client,
        provider,
        Some(Rc::new(|| Ok(1000.0))),
        None,
        host.spawner(),
    )
    .unwrap();
    let mut ignored = session.authenticate(false);
    assert_eq!(count.get(), 1, "provider is eager before await");
    assert!(is_pending(&mut ignored));
    assert!(
        calls.borrow().is_empty(),
        "consumer poll cannot advance an auth continuation"
    );
    drop(ignored);
    host.drain();
    let result = completed(session.authenticate(false));
    let result = result.unwrap();
    assert_eq!(
        count.get(),
        oracle()["ignored"]["jwtCalls"].as_u64().unwrap(),
        "ignored promise populates the cache"
    );
    assert_eq!(*calls.borrow(), vec![7.0]);
    assert_eq!(&*result.borrow(), &oracle()["ignored"]["result"]);
}
#[test]
fn concurrent_authentication_is_not_coalesced_and_uses_call_start_clock() {
    let host = Microtasks::default();
    let now = Rc::new(Cell::new(1000.0));
    let index = Rc::new(Cell::new(0));
    let calls = Rc::new(RefCell::new(Vec::new()));
    let capture = calls.clone();
    let (first_token, resolve_first) = CoinbaseStarbasePromise::pending();
    let (second_token, resolve_second) = CoinbaseStarbasePromise::pending();
    let tokens = [first_token, second_token];
    let provider_index = index.clone();
    let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || {
        let i = provider_index.get();
        provider_index.set(i + 1);
        tokens[i].clone()
    });
    let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, _| {
        let body = request.borrow();
        let id = body.id;
        let token = body.params.borrow()["token"].as_str().unwrap().to_owned();
        drop(body);
        capture.borrow_mut().push(request);
        Box::pin(async move {
            Ok(json!({"jsonrpc":"2.0","id":id,"result":auth_value(&token,&json!(2))}))
        })
    });
    let client = Rc::new(
        CoinbaseStarbaseJsonRpcClient::new(
            transport,
            CoinbaseStarbaseTransportKind::Http,
            Some(&json!(7)),
        )
        .unwrap(),
    );
    let clock_now = now.clone();
    let session = CoinbaseStarbaseAuthSession::new(
        client,
        provider,
        Some(Rc::new(move || Ok(clock_now.get()))),
        Some(&CoinbaseStarbaseRefreshInterval::Number(10000.0)),
        host.spawner(),
    )
    .unwrap();
    let first = session.authenticate(false);
    now.set(2000.0);
    let second = session.authenticate(false);
    assert_eq!(index.get(), 2, "concurrent cold auth calls are independent");
    resolve_second.resolve(Ok("second".into()));
    host.drain();
    let second = completed(second).unwrap();
    resolve_first.resolve(Ok("first".into()));
    host.drain();
    let first = completed(first).unwrap();
    now.set(2999.0);
    let latest = completed(session.authenticate(false)).unwrap();
    assert!(
        latest.shares_result(&first),
        "last completion replaces cache with original invocation timestamp"
    );
    let expected = &oracle()["concurrency"];
    assert_eq!(&*first.borrow(), &expected["first"]);
    assert_eq!(&*second.borrow(), &expected["second"]);
    assert_eq!(&*latest.borrow(), &expected["latest"]);
    assert_eq!(expected["sameFirst"], true);
    assert_eq!(
        index.get() as u64,
        expected["providerIndex"].as_u64().unwrap()
    );
    for (actual, expected) in calls
        .borrow()
        .iter()
        .zip(expected["calls"].as_array().unwrap())
    {
        let actual = actual.borrow();
        assert_eq!(actual.id, expected["id"].as_f64().unwrap());
        assert_eq!(&*actual.params.borrow(), &expected["params"]);
    }
    assert_eq!(calls.borrow().len(), 2, "independent concurrent RPC floor");
}
#[test]
fn provider_clock_and_transport_rejections_keep_original_carrier() {
    for site in ["clock", "provider", "transport"] {
        let host = Microtasks::default();
        let carrier = Rc::new("rejection identity".to_owned());
        let error = Rc::new(CoinbaseStarbaseSessionError::Request(
            CoinbaseStarbaseRequestError::Transport(carrier.clone()),
        ));
        let transport_error = carrier.clone();
        let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |_, _| {
            let error = transport_error.clone();
            Box::pin(async move { Err(CoinbaseStarbaseRequestError::Transport(error)) })
        });
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(
                transport,
                CoinbaseStarbaseTransportKind::Http,
                None,
            )
            .unwrap(),
        );
        let clock_error = error.clone();
        let clock: CoinbaseStarbaseClock<E> = if site == "clock" {
            Rc::new(move || Err(clock_error.clone()))
        } else {
            Rc::new(|| Ok(1000.0))
        };
        let provider_error = error.clone();
        let provider: CoinbaseStarbaseCdpJwtProvider<E> = if site == "provider" {
            Rc::new(move || CoinbaseStarbasePromise::resolved(Err(provider_error.clone())))
        } else {
            Rc::new(|| CoinbaseStarbasePromise::resolved(Ok("jwt".into())))
        };
        let session =
            CoinbaseStarbaseAuthSession::new(client, provider, Some(clock), None, host.spawner())
                .unwrap();
        let promise = session.authenticate(false);
        host.drain();
        let actual = completed(promise).unwrap_err();
        if site == "transport" {
            let CoinbaseStarbaseSessionError::Request(CoinbaseStarbaseRequestError::Transport(
                actual,
            )) = actual.as_ref()
            else {
                panic!("transport rejection variant")
            };
            assert!(Rc::ptr_eq(actual, &carrier));
        } else {
            assert!(
                Rc::ptr_eq(&actual, &error),
                "original callback rejection identity"
            );
        }
    }
}

#[test]
fn queued_authentication_observes_pending_cache_until_auth_await_completes() {
    let host = Microtasks::default();
    let jwt_calls = Rc::new(Cell::new(0));
    let rpc_calls = Rc::new(Cell::new(0));
    let count = rpc_calls.clone();
    let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, _| {
        count.set(count.get() + 1);
        let id = request.borrow().id;
        Box::pin(async move {
            Ok(
                json!({"jsonrpc":"2.0","id":id,"result":auth_value(&format!("racing-{}",id as u64),&json!(60))}),
            )
        })
    });
    let client = Rc::new(
        CoinbaseStarbaseJsonRpcClient::new(
            transport,
            CoinbaseStarbaseTransportKind::Http,
            Some(&json!(7)),
        )
        .unwrap(),
    );
    let count = jwt_calls.clone();
    let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || {
        count.set(count.get() + 1);
        CoinbaseStarbasePromise::resolved(Ok("jwt".into()))
    });
    let session = CoinbaseStarbaseAuthSession::new(
        client,
        provider,
        Some(Rc::new(|| Ok(1000.0))),
        None,
        host.spawner(),
    )
    .unwrap();
    drop(session.authenticate(false));
    let copy = session.clone();
    host.enqueue(move || {
        drop(copy.authenticate(false));
    });
    host.drain();
    let expected = &oracle()["queuedSecondAuth"];
    assert_eq!(jwt_calls.get(), 2, "queued second auth sees pending cache");
    assert_eq!(
        rpc_calls.get(),
        2,
        "queued second auth sends distinct request"
    );
    assert_eq!(
        jwt_calls.get() as u64,
        expected["jwtCalls"].as_u64().unwrap()
    );
    assert_eq!(
        rpc_calls.get() as u64,
        expected["rpcCalls"].as_u64().unwrap()
    );
    assert_eq!(
        &*completed(session.authenticate(false)).unwrap().borrow(),
        &expected["result"]
    );
}

#[test]
fn earlier_future_waiter_keeps_registration_order_with_auth_observer() {
    let host = Microtasks::default();
    let events = Rc::new(RefCell::new(Vec::new()));
    let (token, release) = CoinbaseStarbasePromise::pending();
    let first = token.clone();
    let captured = events.clone();
    (host.spawner())(Box::pin(async move {
        let _: Result<String, CoinbaseStarbaseSessionFailure<E>> = first.await;
        captured.borrow_mut().push("earlier-await".to_owned());
    }));
    let captured = events.clone();
    let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, _| {
        captured.borrow_mut().push(request.borrow().method.clone());
        let id = request.borrow().id;
        Box::pin(async move {
            Ok(json!({"jsonrpc":"2.0","id":id,"result":auth_value("access-0",&json!(2))}))
        })
    });
    let client = Rc::new(
        CoinbaseStarbaseJsonRpcClient::new(transport, CoinbaseStarbaseTransportKind::Http, None)
            .unwrap(),
    );
    let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || token.clone());
    let session = CoinbaseStarbaseAuthSession::new(
        client,
        provider,
        Some(Rc::new(|| Ok(1000.0))),
        None,
        host.spawner(),
    )
    .unwrap();
    let auth = session.authenticate(false);
    release.resolve(Ok("jwt".into()));
    host.drain();
    completed(auth).unwrap();
    assert_eq!(
        *events.borrow(),
        vec!["earlier-await", "public/auth"],
        "mixed Promise reactions retain registration order"
    );
    assert_eq!(
        events.borrow().iter().map(|e| json!(e)).collect::<Vec<_>>(),
        oracle()["orderedEvents"].as_array().unwrap().clone()
    );
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured = events.clone();
    (host.spawner())(Box::pin(async move {
        CoinbaseStarbasePromise::resolved("ready").await;
        captured.borrow_mut().push("after-await");
    }));
    events.borrow_mut().push("synchronous");
    host.drain();
    assert_eq!(
        *events.borrow(),
        vec!["synchronous", "after-await"],
        "fulfilled Promise await resumes in a later microtask"
    );
    assert_eq!(
        events.borrow().iter().map(|e| json!(e)).collect::<Vec<_>>(),
        oracle()["fulfilledEvents"].as_array().unwrap().clone()
    );
}
