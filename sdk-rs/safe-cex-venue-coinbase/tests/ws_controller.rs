use safe_cex_venue_coinbase::{
    starbase::*, starbase_auth::*, starbase_rpc::*, starbase_session::*, starbase_ws_controller::*,
};
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
#[derive(Debug)]
struct HostError {
    name: String,
    message: String,
}
type E = Rc<HostError>;
type Controller = CoinbaseStarbaseWebsocketSessionController<E>;
fn failure(message: &str) -> CoinbaseStarbaseSessionFailure<E> {
    Rc::new(CoinbaseStarbaseSessionError::Request(
        CoinbaseStarbaseRequestError::Transport(Rc::new(HostError {
            name: "Error".into(),
            message: message.into(),
        })),
    ))
}
fn auth_result() -> Value {
    json!({"access_token":"native-token","expires_in":60,"scope":"native-scope","token_type":"bearer"})
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/ws-controller.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "live controller oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn state(text: &str) -> CoinbaseStarbaseWebsocketSessionState {
    match text {
        "idle" => CoinbaseStarbaseWebsocketSessionState::Idle,
        "activating" => CoinbaseStarbaseWebsocketSessionState::Activating,
        "ready" => CoinbaseStarbaseWebsocketSessionState::Ready,
        "disconnected" => CoinbaseStarbaseWebsocketSessionState::Disconnected,
        "rate-limit-backoff" => CoinbaseStarbaseWebsocketSessionState::RateLimitBackoff,
        "failed" => CoinbaseStarbaseWebsocketSessionState::Failed,
        _ => panic!("state"),
    }
}
fn scope(text: &str) -> CoinbaseStarbaseCancelOnDisconnectScope {
    match text {
        "connection" => CoinbaseStarbaseCancelOnDisconnectScope::Connection,
        "account" => CoinbaseStarbaseCancelOnDisconnectScope::Account,
        _ => panic!("scope"),
    }
}
fn reason(text: &str) -> CoinbaseStarbaseWebsocketDisconnectReason {
    match text {
        "transport" => CoinbaseStarbaseWebsocketDisconnectReason::Transport,
        "rate-limit" => CoinbaseStarbaseWebsocketDisconnectReason::RateLimit,
        "heartbeat-timeout" => CoinbaseStarbaseWebsocketDisconnectReason::HeartbeatTimeout,
        _ => panic!("reason"),
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
        CoinbaseStarbaseSessionError::Request(CoinbaseStarbaseRequestError::Transport(error)) => {
            (&*error.name, &*error.message)
        }
        CoinbaseStarbaseSessionError::Coercion(error) => (error.name, error.message),
    };
    assert_eq!(
        name,
        expected["name"].as_str().unwrap(),
        "native controller error name"
    );
    assert_eq!(
        message,
        expected["message"].as_str().unwrap(),
        "native controller error message"
    );
}
fn unit(actual: Result<(), CoinbaseStarbaseSessionFailure<E>>, expected: &Value) {
    match actual {
        Ok(()) => assert_eq!(
            expected["undefined"], true,
            "native activation completion branch"
        ),
        Err(error) => {
            assert!(
                expected.get("error").is_some(),
                "native activation error branch"
            );
            error_fields(&error, &expected["error"]);
        }
    }
}
struct Fixture {
    host: Microtasks,
    controller: Controller,
    jwt_calls: Rc<Cell<u64>>,
    replays: Rc<Cell<u64>>,
    calls: Rc<
        RefCell<
            Vec<(
                CoinbaseStarbaseRequestHandle,
                CoinbaseStarbaseTransportContext,
            )>,
        >,
    >,
    events: Rc<RefCell<Vec<String>>>,
    release: CoinbaseStarbasePromiseResolver<()>,
    policy: Option<Rc<RefCell<CoinbaseStarbaseCancelOnDisconnectPolicy>>>,
}
impl Fixture {
    fn new(spec: &Value) -> Result<Self, CoinbaseStarbaseProtocolError> {
        let host = Microtasks::default();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let events = Rc::new(RefCell::new(Vec::new()));
        let holder: Rc<RefCell<Option<Controller>>> = Rc::new(RefCell::new(None));
        let (gate, release) = CoinbaseStarbasePromise::pending();
        let policy = spec.get("policy").map(|policy| {
            Rc::new(RefCell::new(CoinbaseStarbaseCancelOnDisconnectPolicy {
                enabled: policy["enabled"].as_bool().unwrap(),
                scope: policy.get("scope").map(|v| scope(v.as_str().unwrap())),
            }))
        });
        let capture = calls.clone();
        let captured_events = events.clone();
        let controller = holder.clone();
        let transport_spec = spec.clone();
        let transport_policy = policy.clone();
        let transport: CoinbaseStarbaseTransport<E> = Rc::new(move |request, context| {
            let id = request.borrow().id;
            let method = request.borrow().method.clone();
            capture.borrow_mut().push((request, context));
            captured_events.borrow_mut().push(method.clone());
            if transport_spec["disconnectAt"].as_str() == Some(&method) {
                controller
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .record_disconnect(reason(
                        transport_spec
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("transport"),
                    ));
            }
            let wait = (transport_spec["gate"].as_str() == Some(&method)).then(|| gate.clone());
            let spec = transport_spec.clone();
            let policy = transport_policy.clone();
            Box::pin(async move {
                if let Some(wait) = wait {
                    wait.await;
                }
                if spec["rejectAt"].as_str() == Some(&method) {
                    return Err(CoinbaseStarbaseRequestError::Transport(Rc::new(
                        HostError {
                            name: "Error".into(),
                            message: "wire identity".into(),
                        },
                    )));
                }
                let result = match method.as_str() {
                    "public/auth" => spec.get("authResult").cloned().unwrap_or_else(auth_result),
                    "public/set_heartbeat" => {
                        spec.get("heartbeatResult").cloned().unwrap_or(json!("ok"))
                    }
                    "private/enable_cancel_on_disconnect"
                    | "private/disable_cancel_on_disconnect" => {
                        spec.get("mutationResult").cloned().unwrap_or(json!("ok"))
                    }
                    "private/get_cancel_on_disconnect" => {
                        spec.get("readback").cloned().unwrap_or_else(|| {
                            let policy = policy.as_ref().unwrap().borrow();
                            let scope = match policy
                                .scope
                                .unwrap_or(CoinbaseStarbaseCancelOnDisconnectScope::Connection)
                            {
                                CoinbaseStarbaseCancelOnDisconnectScope::Connection => "connection",
                                CoinbaseStarbaseCancelOnDisconnectScope::Account => "account",
                            };
                            json!({"enabled":policy.enabled,"scope":scope})
                        })
                    }
                    "public/test" => spec
                        .get("testResult")
                        .cloned()
                        .unwrap_or(json!({"version":"native-version"})),
                    _ => panic!("unexpected method {method}"),
                };
                Ok(json!({"jsonrpc":"2.0","id":id,"result":result}))
            })
        });
        let client_kind = if spec["kind"] == "http" {
            CoinbaseStarbaseTransportKind::Http
        } else {
            CoinbaseStarbaseTransportKind::Websocket
        };
        let client = Rc::new(
            CoinbaseStarbaseJsonRpcClient::new(transport, client_kind, Some(&json!(7))).unwrap(),
        );
        let jwt_calls = Rc::new(Cell::new(0));
        let count = jwt_calls.clone();
        let captured_events = events.clone();
        let jwt_error = spec["jwtError"].as_bool().unwrap_or(false);
        let provider: CoinbaseStarbaseCdpJwtProvider<E> = Rc::new(move || {
            count.set(count.get() + 1);
            captured_events.borrow_mut().push("jwt".into());
            CoinbaseStarbasePromise::resolved(if jwt_error {
                Err(failure("jwt identity"))
            } else {
                Ok(format!("jwt-{}", count.get()))
            })
        });
        let auth = CoinbaseStarbaseAuthSession::new(
            client.clone(),
            provider,
            Some(Rc::new(|| Ok(1000.0))),
            None,
            host.spawner(),
        )
        .unwrap();
        let heartbeat_client = if client_kind == CoinbaseStarbaseTransportKind::Websocket {
            client.clone()
        } else {
            Rc::new(
                CoinbaseStarbaseJsonRpcClient::new(
                    Rc::new(|_, _| Box::pin(async { Ok(Value::Null) })),
                    CoinbaseStarbaseTransportKind::Websocket,
                    None,
                )
                .unwrap(),
            )
        };
        let heartbeat =
            Rc::new(CoinbaseStarbaseHeartbeatController::new(heartbeat_client, 10.5).unwrap());
        let replays = Rc::new(Cell::new(0));
        let count = replays.clone();
        let captured_events = events.clone();
        let controller = holder.clone();
        let replay_spec = spec.clone();
        let replay: CoinbaseStarbaseReplaySubscriptions<E> = Rc::new(move || {
            count.set(count.get() + 1);
            captured_events.borrow_mut().push("replay".into());
            if replay_spec["disconnectAt"] == "replay" {
                controller
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .record_disconnect(reason(
                        replay_spec
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("transport"),
                    ));
            }
            CoinbaseStarbasePromise::resolved(
                if replay_spec["replayError"].as_bool().unwrap_or(false) {
                    Err(failure("replay identity"))
                } else {
                    Ok(())
                },
            )
        });
        let controller = Controller::new(
            auth,
            policy.clone(),
            client,
            heartbeat,
            replay,
            host.spawner(),
        )?;
        *holder.borrow_mut() = Some(controller.clone());
        Ok(Self {
            host,
            controller,
            jwt_calls,
            replays,
            calls,
            events,
            release,
            policy,
        })
    }
    fn assert_snapshot(&self, expected: &Value) {
        assert_eq!(
            self.controller.state(),
            state(expected["state"].as_str().unwrap()),
            "native controller state"
        );
        assert_eq!(
            self.jwt_calls.get(),
            expected["jwtCalls"].as_u64().unwrap(),
            "native controller JWT count"
        );
        assert_eq!(
            self.replays.get(),
            expected["replays"].as_u64().unwrap(),
            "native replay count"
        );
        let expected_events: Vec<_> = expected["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            *self.events.borrow(),
            expected_events,
            "native lifecycle event order"
        );
        let calls = self.calls.borrow();
        let expected_calls = expected["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            expected_calls.len(),
            "native controller call total"
        );
        for ((request, context), expected) in calls.iter().zip(expected_calls) {
            let request = request.borrow();
            assert_eq!(request.jsonrpc, CoinbaseStarbaseJsonRpcVersion::V2);
            assert_eq!(
                request.id.to_bits(),
                expected["request"]["id"].as_f64().unwrap().to_bits(),
                "native lifecycle request ID"
            );
            assert_eq!(
                request.method,
                expected["request"]["method"].as_str().unwrap(),
                "native lifecycle method"
            );
            assert_eq!(
                &*request.params.borrow(),
                &expected["request"]["params"],
                "native lifecycle params"
            );
            assert_eq!(context.kind, CoinbaseStarbaseTransportKind::Websocket);
            assert_eq!(
                context.endpoint,
                expected["context"]["endpoint"].as_str().unwrap()
            );
            assert_eq!(context.authorization, None);
        }
    }
}
fn auth_outcome(
    actual: Result<CoinbaseStarbaseAuthResultHandle, CoinbaseStarbaseSessionFailure<E>>,
    expected: &Value,
) {
    match actual {
        Ok(value) => assert_eq!(
            &*value.borrow(),
            &expected["ok"],
            "native maintained authentication result"
        ),
        Err(error) => error_fields(&error, &expected["error"]),
    }
}
fn heartbeat_outcome(
    actual: Result<Option<Value>, CoinbaseStarbaseSessionFailure<E>>,
    expected: &Value,
) {
    match actual {
        Ok(None) => assert_eq!(
            expected["undefined"], true,
            "native classified heartbeat absence"
        ),
        Ok(Some(value)) => assert_eq!(value, expected["ok"], "native classified heartbeat result"),
        Err(error) => error_fields(&error, &expected["error"]),
    }
}
#[test]
fn controller_constructor_activation_policy_failure_and_ready_operations_match_source() {
    let constructors = oracle()["constructors"].as_array().unwrap();
    assert_eq!(
        constructors.len(),
        2,
        "independent controller constructor floor"
    );
    for case in constructors {
        match Fixture::new(&json!({"kind":case["kind"]})) {
            Ok(f) => assert_eq!(
                f.controller.state(),
                state(case["result"]["ok"].as_str().unwrap())
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
    let runs = oracle()["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 22, "independent activation scenario floor");
    for run in runs {
        let f = Fixture::new(&run["spec"]).unwrap();
        let first = f.controller.activate();
        let second = f.controller.activate();
        assert!(
            first.same_promise(&second),
            "in-flight activation returns identical Promise"
        );
        assert_eq!(run["initial"]["same"], true);
        f.assert_snapshot(&run["initial"]);
        f.host.drain();
        unit(completed(first.clone()), &run["result"]);
        f.assert_snapshot(&run["final"]);
        if let Some(ready) = run.get("ready") {
            let next = f.controller.activate();
            assert!(
                !next.same_promise(&first),
                "ready activation is a distinct fulfilled Promise"
            );
            unit(completed(next), &ready["result"]);
            let maintained = f.controller.maintain_authentication();
            f.host.drain();
            auth_outcome(completed(maintained), &ready["maintenance"]);
            let heartbeat = f
                .controller
                .handle_classified_heartbeat(CoinbaseStarbaseHeartbeatSignal::Heartbeat);
            f.host.drain();
            heartbeat_outcome(completed(heartbeat), &ready["heartbeat"]);
            let test = f
                .controller
                .handle_classified_heartbeat(CoinbaseStarbaseHeartbeatSignal::TestRequest);
            f.host.drain();
            heartbeat_outcome(completed(test), &ready["test"]);
            f.assert_snapshot(&ready["final"]);
        }
    }
}
#[test]
fn controller_readiness_backoff_and_disconnect_guards_match_source() {
    let guards = oracle()["guards"].as_array().unwrap();
    assert_eq!(guards.len(), 6, "independent readiness state floor");
    for case in guards {
        let f = Fixture::new(&json!({})).unwrap();
        f.controller
            .set_state(state(case["state"].as_str().unwrap()));
        let maintained = f.controller.maintain_authentication();
        f.host.drain();
        auth_outcome(completed(maintained), &case["maintenance"]);
        let heartbeat = f
            .controller
            .handle_classified_heartbeat(CoinbaseStarbaseHeartbeatSignal::Heartbeat);
        f.host.drain();
        heartbeat_outcome(completed(heartbeat), &case["heartbeat"]);
        let test = f
            .controller
            .handle_classified_heartbeat(CoinbaseStarbaseHeartbeatSignal::TestRequest);
        f.host.drain();
        heartbeat_outcome(completed(test), &case["test"]);
        unit(f.controller.acknowledge_rate_limit_backoff(), &case["ack"]);
        f.assert_snapshot(&case["final"]);
    }
    let disconnects = oracle()["disconnects"].as_array().unwrap();
    assert_eq!(disconnects.len(), 3, "independent disconnect reason floor");
    for case in disconnects {
        let f = Fixture::new(&json!({})).unwrap();
        f.controller
            .record_disconnect(reason(case["reason"].as_str().unwrap()));
        assert_eq!(
            f.controller.state(),
            state(case["state"].as_str().unwrap()),
            "native disconnect state"
        );
        let activation = f.controller.activate();
        f.host.drain();
        unit(completed(activation), &case["activation"]);
        if let Some(ack) = case.get("acknowledgement") {
            unit(f.controller.acknowledge_rate_limit_backoff(), ack);
            let activation = f.controller.activate();
            f.host.drain();
            completed(activation).unwrap();
        }
        f.assert_snapshot(&case["final"]);
    }
}
#[test]
fn queued_disconnect_interleaves_with_ready_awaits_and_consumer_poll_is_passive() {
    let f = Fixture::new(&json!({})).unwrap();
    let mut pending = f.controller.activate();
    assert!(is_pending(&mut pending));
    assert_eq!(
        *f.events.borrow(),
        vec!["jwt"],
        "consumer polling cannot execute activation producer"
    );
    let controller = f.controller.clone();
    f.host.enqueue(move || {
        controller.record_disconnect(CoinbaseStarbaseWebsocketDisconnectReason::Transport)
    });
    f.host.drain();
    unit(completed(pending), &oracle()["queuedRace"]["result"]);
    f.assert_snapshot(&oracle()["queuedRace"]["final"]);
    let ignored = Fixture::new(&json!({})).unwrap();
    drop(ignored.controller.activate());
    ignored.host.drain();
    ignored.assert_snapshot(&oracle()["ignored"]);
}
#[test]
fn replacement_generation_policy_alias_and_failed_retry_match_source() {
    let stale = Fixture::new(&json!({"gate":"public/auth"})).unwrap();
    let old = stale.controller.activate();
    stale.host.drain();
    stale
        .controller
        .record_disconnect(CoinbaseStarbaseWebsocketDisconnectReason::Transport);
    let fresh = stale.controller.activate();
    let shared = stale.controller.activate();
    assert!(shared.same_promise(&fresh));
    assert!(!fresh.same_promise(&old));
    stale.release.resolve(());
    stale.host.drain();
    unit(completed(old), &oracle()["replacement"]["old"]);
    unit(completed(fresh), &oracle()["replacement"]["fresh"]);
    stale.assert_snapshot(&oracle()["replacement"]["final"]);
    let changed=Fixture::new(&json!({"gate":"private/enable_cancel_on_disconnect","policy":{"enabled":true,"scope":"connection"}})).unwrap();
    let activation = changed.controller.activate();
    changed.host.drain();
    {
        let mut policy = changed.policy.as_ref().unwrap().borrow_mut();
        policy.enabled = false;
        policy.scope = Some(CoinbaseStarbaseCancelOnDisconnectScope::Account);
    }
    changed.release.resolve(());
    changed.host.drain();
    unit(completed(activation), &oracle()["policyMutation"]["result"]);
    changed.assert_snapshot(&oracle()["policyMutation"]["final"]);
    let retry = Fixture::new(&json!({"replayError":true})).unwrap();
    for expected in oracle()["retry"]["results"].as_array().unwrap() {
        let activation = retry.controller.activate();
        retry.host.drain();
        unit(completed(activation), expected);
    }
    retry.assert_snapshot(&oracle()["retry"]["final"]);
    assert_eq!(
        retry.jwt_calls.get(),
        2,
        "failed activation clears promise so a fresh force-auth retry starts"
    );
}
