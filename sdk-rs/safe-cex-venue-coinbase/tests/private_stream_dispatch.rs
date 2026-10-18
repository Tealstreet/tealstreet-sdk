use safe_cex_venue_coinbase::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    private_stream_dispatch::*,
    public_stream_dispatch::{PublicSocket, PublicSocketListener},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    process::Command,
    rc::Rc,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
const FIXTURE: &[u8] = include_bytes!("fixtures/private-stream-dispatch.json");
const HASH: &str = "aa8dd526cfb4aae53fdc8d455ecbb1f635a856f77e7c4d936cddf15a992a2382";
#[derive(Clone)]
enum Trace {
    Route(String, PrivateOptions),
    Create(Option<Value>),
    Listener(PublicSocketListener, PublicSocket),
    Reset,
    Close(PublicSocket),
    Send(PrivateSubscription),
    Local(Option<Value>, Option<Value>),
    Remote(PrivateWsAuthRequest),
    Track,
    Diagnostic(String),
    Auth(Option<Frame>),
    Health,
    Emit(Option<Frame>),
    Orders(Frame),
    Pong,
}
struct Host {
    trace: Rc<RefCell<Vec<Trace>>>,
    disposed: Cell<bool>,
    options: PrivateOptions,
    testnet: bool,
    route: PrivateStreamRoute,
    fail: Option<String>,
    failure: PortfolioFailure,
    errors: Rc<RefCell<Vec<PortfolioFailure>>>,
    local: RefCell<VecDeque<Promise<Result<Value, PortfolioFailure>>>>,
    local_override: RefCell<Option<Promise<Result<Value, PortfolioFailure>>>>,
    parse_error: Option<PortfolioFailure>,
    order_hook: RefCell<Option<Rc<dyn Fn(Frame)>>>,
}
impl Host {
    fn record(&self, kind: &str, t: Trace) -> Result<(), PortfolioFailure> {
        self.trace.borrow_mut().push(t);
        if self.fail.as_deref() == Some(kind) {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
}
impl PrivateStreamHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> PrivateOptions {
        self.options.clone()
    }
    fn exchange_name(&self) -> String {
        "Coinbase".into()
    }
    fn testnet(&self) -> bool {
        self.testnet
    }
    fn resolve_private_route(
        &self,
        name: String,
        options: PrivateOptions,
    ) -> Result<PrivateStreamRoute, PortfolioFailure> {
        assert!(
            Rc::ptr_eq(&options, &self.options),
            "routing receives original native options root"
        );
        self.record("route", Trace::Route(name, options))?;
        Ok(self.route.clone())
    }
    fn create_websocket(&self, url: Option<Value>) -> Result<PublicSocket, PortfolioFailure> {
        self.record("create", Trace::Create(url))?;
        Ok(Rc::new(RefCell::new(Frame::Null)))
    }
    fn add_listener(
        &self,
        s: PublicSocket,
        e: PublicSocketListener,
    ) -> Result<(), PortfolioFailure> {
        self.record("listener", Trace::Listener(e, s))
    }
    fn reset_order_snapshot_assembly(&self) -> Result<(), PortfolioFailure> {
        self.record("reset", Trace::Reset)
    }
    fn close_socket(&self, s: PublicSocket) -> Result<(), PortfolioFailure> {
        self.record("close", Trace::Close(s))
    }
    fn send_with_log(&self, p: PrivateSubscription) -> Result<(), PortfolioFailure> {
        self.record("send", Trace::Send(p))
    }
    fn sign_local(
        &self,
        key: Option<Value>,
        secret: Option<Value>,
    ) -> Promise<Result<Value, PortfolioFailure>> {
        if let Err(e) = self.record("local", Trace::Local(key, secret)) {
            return Promise::resolved(Err(e));
        }
        if let Some(p) = self.local_override.borrow().clone() {
            return p;
        }
        self.local
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| Promise::resolved(Ok(json!("local-jwt"))))
    }
    fn track_last_received(&self) -> Result<(), PortfolioFailure> {
        self.record("track", Trace::Track)
    }
    fn emit_ws_diagnostic_receive(&self, raw: &str) -> Result<(), PortfolioFailure> {
        self.record("diagnostic", Trace::Diagnostic(raw.into()))
    }
    fn json_parse(&self, raw: &str) -> Result<Frame, PortfolioFailure> {
        if let Some(e) = &self.parse_error {
            return Err(e.clone());
        }
        Ok(Frame::from_json(
            &serde_json::from_str::<Value>(raw)
                .expect("valid raw JSON or declared host parser rejection"),
        ))
    }
    fn mark_private_user_stream_auth_failed(
        &self,
        v: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.record("auth", Trace::Auth(v))
    }
    fn mark_private_user_stream_message_received(&self) -> Result<(), PortfolioFailure> {
        self.record("health", Trace::Health)
    }
    fn emit_and_log_error(&self, v: Option<Frame>) -> Result<(), PortfolioFailure> {
        self.record("emit", Trace::Emit(v))
    }
    fn handle_user_event_orders(&self, v: Frame) -> Result<(), PortfolioFailure> {
        self.record("orders", Trace::Orders(v.clone()))?;
        let hook = self.order_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(v);
        }
        Ok(())
    }
    fn handle_pong_event(&self) -> Result<(), PortfolioFailure> {
        self.record("pong", Trace::Pong)
    }
}
fn thrown(v: Value) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(v)))
}
fn setup(input: &Value, tasks: &Microtasks) -> Rc<PrivateStreamReceiver<Host>> {
    let trace = Rc::new(RefCell::new(vec![]));
    let errors = Rc::new(RefCell::new(vec![]));
    let failure = thrown(json!({"name":"Error","message":"controlled failure"}));
    let remote = if input["remote"].as_bool().unwrap_or(false) {
        let plans = Rc::new(RefCell::new(
            input["plans"]
                .as_array()
                .map(|v| v.iter().cloned().collect::<VecDeque<_>>())
                .unwrap_or_default(),
        ));
        let log = trace.clone();
        let errs = errors.clone();
        let fail = input["fail"].as_str().map(str::to_owned);
        let failure = failure.clone();
        Some(Rc::new(PrivateRemoteSigner {
            account_name: RefCell::new(if input["omitAccount"].as_bool().unwrap_or(false) {
                None
            } else {
                Some(
                    input
                        .get("account")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or(json!("account-A")),
                )
            }),
            sign: Rc::new(move |request| {
                log.borrow_mut().push(Trace::Remote(request));
                if fail.as_deref() == Some("remote") {
                    return Err(failure.clone());
                }
                let p = plans
                    .borrow_mut()
                    .pop_front()
                    .unwrap_or(json!({"data":{"wsAuthSignature":"remote-jwt"}}));
                let result = if let Some(e) = p.get("error") {
                    let e = thrown(e.clone());
                    errs.borrow_mut().push(e.clone());
                    Err(e)
                } else {
                    Ok(p.get("data").cloned())
                };
                Ok(Promise::resolved(result))
            }),
        }))
    } else {
        None
    };
    let local = input["local"]
        .as_array()
        .map(|v| {
            v.iter()
                .map(|p| {
                    let value = if let Some(e) = p.get("error") {
                        let e = thrown(e.clone());
                        errors.borrow_mut().push(e.clone());
                        Err(e)
                    } else {
                        Ok(p.get("jwt").cloned().unwrap_or(Value::Null))
                    };
                    Promise::resolved(value)
                })
                .collect()
        })
        .unwrap_or_default();
    let parse_error = input.get("parseError").map(|v| {
        let e = thrown(v.clone());
        errors.borrow_mut().push(e.clone());
        e
    });
    Rc::new(PrivateStreamReceiver {
        host: Host {
            trace,
            disposed: Cell::new(input["disposed"].as_bool().unwrap_or(false)),
            options: Rc::new(RefCell::new(PrivateStreamOptions {
                key: Some(
                    input
                        .get("key")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or(json!("key")),
                ),
                secret: Some(
                    input
                        .get("secret")
                        .filter(|v| !v.is_null())
                        .cloned()
                        .unwrap_or(json!("secret")),
                ),
                remote_signer: remote,
            })),
            testnet: input["testnet"].as_bool().unwrap_or(false),
            route: PrivateStreamRoute {
                tunnelled: input["route"]["tunnelled"].as_bool().unwrap_or(false),
                host: input["route"].get("host").cloned(),
            },
            fail: input["fail"].as_str().map(str::to_owned),
            failure,
            errors,
            local: RefCell::new(local),
            local_override: RefCell::new(None),
            parse_error,
            order_hook: RefCell::new(None),
        },
        spawn: tasks.spawner(),
        ws: RefCell::new(if input["socket"] == false {
            None
        } else {
            Some(Rc::new(RefCell::new(Frame::Null)))
        }),
        subscription_attempt: Cell::new(input["attempt"].as_f64().unwrap_or(0.0)),
    })
}
fn bits(v: f64, e: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", v.to_bits()),
        e["$bits"].as_str().unwrap(),
        "{label}: decoded native number bits"
    );
}
fn frame(v: &Frame, e: &Value, label: &str) {
    match v {
        Frame::Undefined => assert_eq!(e["$undefined"], true, "native undefined"),
        Frame::Null => assert!(e.is_null(), "{label}: native null"),
        Frame::Bool(v) => assert_eq!(Some(*v), e.as_bool(), "{label}: native bool"),
        Frame::Number(v) => bits(*v, e, label),
        Frame::String(v) => assert_eq!(
            *v,
            e.as_str().unwrap().encode_utf16().collect::<Vec<_>>(),
            "{label}: native UTF16 string"
        ),
        Frame::Array(v) => {
            let e = e.as_array().unwrap();
            let v = v.borrow();
            assert_eq!(v.len(), e.len(), "{label}: independent source array size");
            for (v, e) in v.iter().zip(e) {
                frame(v, e, label);
            }
        }
        Frame::Object(v) => {
            let e = e.as_object().unwrap();
            assert_eq!(
                v.borrow().len(),
                e.len(),
                "{label}: independent source record size"
            );
            for (k, e) in e {
                frame(&v.borrow()[&k.encode_utf16().collect::<Vec<_>>()], e, label);
            }
        }
    }
}
fn optional(v: Option<&Value>, e: &Value, label: &str) {
    match v {
        None => assert_eq!(e, &json!({"$undefined":true}), "{label}: native undefined"),
        Some(v) => frame(&Frame::from_json(v), e, label),
    }
}
fn frame_optional(v: Option<&Frame>, e: &Value, label: &str) {
    match v {
        None => assert_eq!(e, &json!({"$undefined":true}), "{label}: native undefined"),
        Some(v) => frame(v, e, label),
    }
}
fn trace(t: &Trace, e: &Value, label: &str) {
    let kind = e["kind"].as_str().unwrap();
    match t {
        Trace::Route(name, opts) => {
            assert_eq!(kind, "route");
            assert_eq!(
                name,
                e["name"].as_str().unwrap(),
                "{label}: routing exchange name"
            );
            let o = opts.borrow();
            optional(o.key.as_ref(), &e["key"], label);
            optional(o.secret.as_ref(), &e["secret"], label);
            assert_eq!(
                o.remote_signer.is_some(),
                e["remote"].as_bool().unwrap(),
                "{label}: routing signer present"
            );
        }
        Trace::Create(url) => {
            assert_eq!(kind, "create");
            optional(
                url.as_ref(),
                &e["url"],
                &format!("{label}: private endpoint"),
            );
        }
        Trace::Listener(event, _) => {
            assert_eq!(kind, "listener");
            let name = match event {
                PublicSocketListener::Open => "open",
                PublicSocketListener::Message => "message",
                PublicSocketListener::Close => "close",
            };
            assert_eq!(
                name,
                e["event"].as_str().unwrap(),
                "{label}: listener event"
            );
            assert_eq!(
                name,
                e["callback"].as_str().unwrap(),
                "{label}: listener callback"
            );
        }
        Trace::Reset => assert_eq!(kind, "reset"),
        Trace::Close(_) => assert_eq!(kind, "close"),
        Trace::Track => assert_eq!(kind, "track"),
        Trace::Health => assert_eq!(kind, "health", "{label}: health before events"),
        Trace::Pong => assert_eq!(kind, "pong"),
        Trace::Diagnostic(raw) => {
            assert_eq!(kind, "diagnostic");
            assert_eq!(raw, e["raw"].as_str().unwrap(), "{label}: raw diagnostic");
        }
        Trace::Auth(v) => {
            assert_eq!(kind, "auth", "{label}: auth classification");
            frame_optional(v.as_ref(), &e["value"], label);
        }
        Trace::Emit(v) => {
            assert_eq!(kind, "emit", "{label}: normal error classification");
            frame_optional(v.as_ref(), &e["value"], label);
        }
        Trace::Orders(v) => {
            assert_eq!(kind, "orders");
            frame(v, &e["data"], label);
        }
        Trace::Local(key, secret) => {
            assert_eq!(kind, "local");
            optional(key.as_ref(), &e["key"], label);
            optional(secret.as_ref(), &e["secret"], label);
        }
        Trace::Remote(request) => {
            assert_eq!(kind, "remote");
            let e = &e["request"];
            assert_eq!(
                request.id,
                e["id"].as_str().unwrap(),
                "{label}: native signing request ID"
            );
            assert_eq!(
                request.request_type,
                e["type"].as_str().unwrap(),
                "{label}: native signing request type"
            );
            assert_eq!(
                request.exchange_name,
                e["exchangeName"].as_str().unwrap(),
                "{label}: native signing exchange name"
            );
            optional(request.account_name.as_ref(), &e["accountName"], label);
            assert_eq!(
                request.to_sign,
                e["wsAuth"]["toSign"].as_str().unwrap(),
                "{label}: native to-sign text"
            );
            assert_eq!(
                request.output_format,
                e["wsAuth"]["outputFormat"].as_str().unwrap(),
                "{label}: native output format"
            );
            assert_eq!(
                request.channel,
                e["wsAuth"]["extraData"]["channel"].as_str().unwrap(),
                "{label}: native signer channel"
            );
        }
        Trace::Send(p) => {
            assert_eq!(kind, "send");
            let e = &e["payload"];
            assert_eq!(
                p.request_type,
                e["type"].as_str().unwrap(),
                "{label}: native subscription type"
            );
            assert_eq!(
                p.channel,
                e["channel"].as_str().unwrap(),
                "{label}: native subscription channel"
            );
            optional(
                Some(&p.jwt),
                &e["jwt"],
                &format!("{label}: native subscription JWT"),
            );
        }
    }
}
#[test]
fn private_stream_live_native_corpus() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE)),
        HASH,
        "content-pinned private stream corpus"
    );
    let inputs: Vec<Value> = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(inputs.len(), 55, "independent private stream case floor");
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-stream-dispatch.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "private source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outputs: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(outputs.len(), 55, "independent private source output floor");
    for (input, e) in inputs.iter().zip(outputs) {
        let label = input["label"].as_str().unwrap();
        let tasks = Microtasks::default();
        let r = setup(input, &tasks);
        let data = input.get("frame").map(Frame::from_json);
        let mut result: Result<Option<Value>, PortfolioFailure> = Ok(None);
        match input["op"].as_str().unwrap() {
            "connect" => result = connect_and_subscribe(&r).map(|()| None),
            "open" => {
                let p = on_open(r.clone());
                tasks.drain();
                result = completed(p).map(|()| None);
            }
            "build" => {
                let p = build_subscription_jwt(
                    r.clone(),
                    input["channel"].as_str().unwrap_or("user").into(),
                );
                tasks.drain();
                result = completed(p).map(Some);
            }
            "subscribe" => {
                let p = subscribe(
                    r.clone(),
                    input["argumentAttempt"]
                        .as_f64()
                        .unwrap_or(r.subscription_attempt.get()),
                );
                tasks.drain();
                result = completed(p).map(|()| None);
            }
            "message" => result = on_message(&r, input["raw"].as_str().unwrap()).map(|()| None),
            "user" => result = handle_user_event(&r, data.as_ref()).map(|()| None),
            "error" => {
                result = handle_private_stream_error(&r, input.get("message").map(Frame::from_json))
                    .map(|()| None)
            }
            "ping" => ping(&r),
            "position" => handle_position_topic(&r, &[]),
            _ => panic!("unknown independent operation"),
        };
        match &result {
            Ok(v) => {
                assert!(e["error"].is_null(), "{label}: native success");
                optional(v.as_ref(), &e["result"], label);
            }
            Err(err) => {
                let v = err.value.as_ref();
                assert_eq!(
                    v.and_then(|v| v.get("name")).and_then(Value::as_str),
                    e["error"].get("name").and_then(Value::as_str),
                    "{label}: native error name"
                );
                assert_eq!(
                    v.and_then(|v| v.get("message")).and_then(Value::as_str),
                    e["error"].get("message").and_then(Value::as_str),
                    "{label}: native error message"
                );
            }
        }
        assert_eq!(
            result
                .as_ref()
                .err()
                .is_some_and(|err| Rc::ptr_eq(err, &r.host.failure)),
            e["sameFailure"].as_bool().unwrap(),
            "{label}: thrown host identity"
        );
        let index = result
            .as_ref()
            .err()
            .and_then(|err| {
                r.host
                    .errors
                    .borrow()
                    .iter()
                    .position(|e| Rc::ptr_eq(e, err))
            })
            .map(|v| v as i64)
            .unwrap_or(-1);
        assert_eq!(
            index,
            e["errorIndex"].as_i64().unwrap(),
            "{label}: thrown signer/parser identity"
        );
        bits(r.subscription_attempt.get(), &e["attempt"], label);
        let traces = r.host.trace.borrow();
        let expected = e["trace"].as_array().unwrap();
        assert_eq!(
            traces.len(),
            expected.len(),
            "{label}: independent source invocation count"
        );
        for (t, e) in traces.iter().zip(expected) {
            trace(t, e, label);
        }
        if input["op"] == "connect" {
            for t in traces.iter() {
                if let Trace::Listener(_, s) = t {
                    assert!(
                        Rc::ptr_eq(s, r.ws.borrow().as_ref().unwrap()),
                        "connect listener retains current socket root"
                    );
                }
            }
        }
    }
}
#[test]
fn local_signature_promise_adoption_matches_source_checkpoints() {
    let tasks = Microtasks::default();
    let r = setup(&json!({}), &tasks);
    let p = subscribe(r.clone(), 0.0);
    assert!(
        is_pending(&mut p.clone()),
        "local signature is adopted asynchronously"
    );
    let seen = Rc::new(RefCell::new(vec![]));
    for i in 1..=3 {
        let marker_seen = seen.clone();
        let log = r.host.trace.clone();
        tasks.enqueue(move || {
            marker_seen.borrow_mut().push((
                i,
                log.borrow()
                    .iter()
                    .filter(|t| matches!(t, Trace::Send(_)))
                    .count(),
            ));
        });
        for _ in 0..1000 {
            if seen.borrow().len() == i {
                break;
            }
            assert!(tasks.tick(), "checkpoint job pending");
        }
        assert_eq!(
            seen.borrow().len(),
            i,
            "checkpoint enumeration stopped at 1000"
        );
    }
    assert_eq!(
        *seen.borrow(),
        [(1, 0), (2, 0), (3, 1)],
        "local signing adoption source checkpoint write counts"
    );
    tasks.drain();
    completed(p).unwrap();
    assert_eq!(
        r.host
            .trace
            .borrow()
            .iter()
            .filter(|t| matches!(t, Trace::Send(_)))
            .count(),
        2,
        "both channels sent after local adoption"
    );
}
#[test]
fn pending_local_signatures_cancel_and_rejections_close_current_socket() {
    for disposed in [false, true] {
        let tasks = Microtasks::default();
        let r = setup(&json!({"attempt":1}), &tasks);
        let (sign, resolve) = Promise::pending();
        *r.host.local_override.borrow_mut() = Some(sign);
        let p = subscribe(r.clone(), 1.0);
        assert_eq!(
            r.host
                .trace
                .borrow()
                .iter()
                .filter(|t| matches!(t, Trace::Local(_, _)))
                .count(),
            1,
            "hot local signer invoked before subscribe returns"
        );
        assert!(
            is_pending(&mut p.clone()),
            "subscribe waits pending local signing"
        );
        if disposed {
            r.host.disposed.set(true)
        } else {
            r.subscription_attempt.set(2.0)
        }
        resolve.resolve(Ok(json!("late-jwt")));
        tasks.drain();
        completed(p).unwrap();
        assert_eq!(
            r.host
                .trace
                .borrow()
                .iter()
                .filter(|t| matches!(t, Trace::Send(_)))
                .count(),
            0,
            "stale/disposed local signature suppresses writes"
        );
    }
    let tasks = Microtasks::default();
    let r = setup(&json!({}), &tasks);
    let (sign, reject) = Promise::pending();
    *r.host.local_override.borrow_mut() = Some(sign);
    let p = on_open(r.clone());
    assert_eq!(
        r.subscription_attempt.get(),
        1.0,
        "open increments attempt before await"
    );
    let current = Rc::new(RefCell::new(Frame::Bool(true)));
    *r.ws.borrow_mut() = Some(current.clone());
    r.host.disposed.set(true);
    r.subscription_attempt.set(7.0);
    let error = thrown(json!({"message":"late reject"}));
    reject.resolve(Err(error));
    tasks.drain();
    completed(p).unwrap();
    let traces = r.host.trace.borrow();
    let closes: Vec<_> = traces
        .iter()
        .filter_map(|t| {
            if let Trace::Close(s) = t {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        closes.len(),
        1,
        "stale signing rejection closes exactly one socket"
    );
    assert!(
        Rc::ptr_eq(closes[0], &current),
        "stale signing rejection closes current socket root"
    );
    assert!(
        matches!(traces[traces.len() - 2], Trace::Emit(_)),
        "signing rejection emits before current socket close"
    );
}
#[test]
fn user_event_dispatch_retains_roots_and_captures_length() {
    let tasks = Microtasks::default();
    let r = setup(&json!({}), &tasks);
    let data = Frame::from_json(&json!({"events":[{"orders":[],"id":"A"}]}));
    let Frame::Array(events) = data.get("events").unwrap() else {
        panic!()
    };
    let original = events.clone();
    let root = events.borrow()[0].clone();
    let Frame::Object(root) = root else { panic!() };
    let observed = Rc::new(Cell::new(0));
    let seen = observed.clone();
    let expected = root.clone();
    *r.host.order_hook.borrow_mut() = Some(Rc::new(move |event| {
        seen.set(seen.get() + 1);
        let Frame::Object(event) = event else {
            panic!()
        };
        assert!(
            Rc::ptr_eq(&event, &expected),
            "user dispatcher retains decoded event root"
        );
        event
            .borrow_mut()
            .insert("marker".encode_utf16().collect(), Frame::Number(91.0));
        original
            .borrow_mut()
            .push(Frame::from_json(&json!({"orders":[],"id":"B"})));
    }));
    handle_user_event(&r, Some(&data)).unwrap();
    assert_eq!(
        observed.get(),
        1,
        "user forEach captures original event count"
    );
    assert!(
        matches!(root.borrow().get(&"marker".encode_utf16().collect::<Vec<_>>()),Some(Frame::Number(v)) if *v==91.0),
        "user callback mutation reaches original input root"
    );
    assert!(
        matches!(r.host.trace.borrow().first(), Some(Trace::Health)),
        "health mark precedes order dispatch"
    );
}
#[test]
fn synchronous_remote_signer_throw_rejects_before_microtasks() {
    let tasks = Microtasks::default();
    let r = setup(&json!({"remote":true,"fail":"remote"}), &tasks);
    let p = build_subscription_jwt(r.clone(), "user".into());
    let result =
        completed(p).expect_err("synchronous signer throw is already rejected before microtasks");
    assert!(
        Rc::ptr_eq(&result, &r.host.failure),
        "synchronous signer throw retains original failure root"
    );
    assert_eq!(
        r.host.trace.borrow().len(),
        1,
        "synchronous signer invokes only the remote callback"
    );
}
