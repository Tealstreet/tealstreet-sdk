use safe_cex_venue_backpack::{
    account_reads::{listen_ohlcv, listen_order_book},
    private_auth::{self, AuthHost, AuthMessage, AuthSignature},
    public_control::{JsString, Subscriptions},
    socket_lifecycle::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
type Error = Rc<&'static str>;
fn oracle() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private_auth_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn strings(v: &Value) -> Vec<JsString> {
    serde_json::from_value(v.clone()).unwrap()
}
fn units(v: &Value) -> JsString {
    serde_json::from_value(v.clone()).unwrap()
}
struct Hold<T> {
    open: Rc<Cell<bool>>,
    result: Result<T, Error>,
}
impl<T: Clone> Future for Hold<T> {
    type Output = Result<T, Error>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if self.open.get() {
            Poll::Ready(self.result.clone())
        } else {
            Poll::Pending
        }
    }
}
fn poll<F: Future + ?Sized>(f: Pin<&mut F>) -> Poll<F::Output> {
    f.poll(&mut Context::from_waker(Waker::noop()))
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match poll(f.as_mut()) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture must be ready"),
    }
}
fn original_error(
    result: Result<(), Error>,
    error: &Error,
    expected: bool,
    row: &Value,
    label: &str,
) {
    assert_eq!(result.is_err(), expected, "{label} native error presence");
    assert_eq!(row["sameError"], expected);
    if let Err(e) = result {
        assert!(Rc::ptr_eq(&e, error), "{label} native original error");
    }
}
fn set(values: &[JsString]) -> Rc<RefCell<Subscriptions>> {
    let s = Rc::new(RefCell::new(Subscriptions::default()));
    for v in values {
        s.borrow_mut().insert(v.clone());
    }
    s
}
fn symbols() -> Vec<Option<JsString>> {
    vec![
        None,
        Some(vec![]),
        Some(u("BTC_USDC")),
        Some(u("λ🚀")),
        Some(vec![88, 0xd800]),
    ]
}
fn auth_streams(method: usize, symbol: Option<&[u16]>) -> Vec<JsString> {
    match method {
        0 => vec![u("account.orderUpdate"), u("account.positionUpdate")],
        1 => {
            let mut s = u("account.orderUpdate");
            if let Some(symbol) = symbol.filter(|s| !s.is_empty()) {
                s.push(46);
                s.extend_from_slice(symbol);
            }
            vec![s]
        }
        2 => vec![u("account.positionUpdate")],
        _ => unreachable!(),
    }
}
const AUTH_MODES: [&str; 14] = [
    "success",
    "pending",
    "signer",
    "key",
    "secret",
    "factory",
    "await",
    "lookup",
    "send",
    "format",
    "emitLookup",
    "emit",
    "has",
    "add",
];
fn auth_trace(
    method: usize,
    existing: bool,
    mode: &str,
    streams: &[JsString],
) -> (Vec<String>, Vec<String>) {
    let mut trace = vec![];
    if method != 0 {
        trace.push("has".into());
        if existing || mode == "has" {
            return (trace.clone(), trace);
        }
    }
    for phase in ["signer", "key", "secret", "factory"] {
        trace.push(phase.into());
        if mode == phase {
            trace.extend(["emitLookup", "format", "emit"].map(str::to_string));
            return (trace.clone(), trace);
        }
    }
    let eager = trace.clone();
    if mode != "await" {
        trace.push("lookup".into());
        if mode != "lookup" {
            trace.push("send".into());
            if !["send", "format", "emitLookup", "emit"].contains(&mode) {
                for (index, _stream) in streams.iter().enumerate() {
                    trace.push(format!("add:{index}"));
                    if mode == "add" {
                        break;
                    }
                }
            }
        }
    }
    if [
        "await",
        "lookup",
        "send",
        "format",
        "emitLookup",
        "emit",
        "add",
    ]
    .contains(&mode)
    {
        trace.push("emitLookup".into());
        if mode != "emitLookup" {
            trace.push("format".into());
            if mode != "format" {
                trace.push("emit".into());
            }
        }
    }
    (eager, trace)
}
struct AuthCtx {
    method: usize,
    existing: bool,
    mode: &'static str,
    row: Value,
    streams: Vec<JsString>,
    state: Rc<RefCell<Subscriptions>>,
    signature: AuthSignature,
    open: Rc<Cell<bool>>,
    calls: Vec<String>,
    messages: Vec<JsString>,
    wires: Vec<(JsString, JsString)>,
    requests: usize,
    signer: Rc<()>,
    emitter: Rc<()>,
    primary: Error,
    coerce: Error,
    emit_lookup: Error,
    emit: Error,
}
#[derive(Clone)]
struct AuthOps(Rc<RefCell<AuthCtx>>);
impl AuthHost for AuthOps {
    type Error = Error;
    type Signer = Rc<()>;
    type Emitter = Rc<()>;
    type SignatureFuture = Hold<AuthSignature>;
    fn has_subscription(&mut self, stream: &[u16]) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("has".into());
        assert_eq!(stream, c.streams[0], "AUTH native guard stream");
        if c.mode == "has" {
            return Err(c.primary.clone());
        }
        let present = c.state.borrow().contains(stream);
        Ok(present)
    }
    fn add_subscription(&mut self, stream: JsString) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        let index = c
            .streams
            .iter()
            .position(|value| value == &stream)
            .expect("native add stream must match requested stream");
        c.calls.push(format!("add:{index}"));
        c.state.borrow_mut().insert(stream);
        if c.mode == "add" {
            return Err(c.primary.clone());
        }
        Ok(())
    }
    fn signature_function(&mut self) -> Result<Rc<()>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("signer".into());
        if c.mode == "signer" {
            Err(c.primary.clone())
        } else {
            Ok(c.signer.clone())
        }
    }
    fn key(&mut self) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("key".into());
        if c.mode == "key" {
            Err(c.primary.clone())
        } else {
            let mut v = u("KEYλ");
            v.push(0xd800);
            Ok(v)
        }
    }
    fn secret(&mut self) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("secret".into());
        if c.mode == "secret" {
            Err(c.primary.clone())
        } else {
            let mut v = u("SECRET");
            v.push(0xdc00);
            Ok(v)
        }
    }
    fn generate_signature(
        &mut self,
        signer: Rc<()>,
        key: JsString,
        secret: JsString,
        window: f64,
    ) -> Result<Self::SignatureFuture, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("factory".into());
        assert!(
            Rc::ptr_eq(&signer, &c.signer),
            "AUTH native signer identity"
        );
        let mut expected_key = u("KEYλ");
        expected_key.push(0xd800);
        let mut expected_secret = u("SECRET");
        expected_secret.push(0xdc00);
        assert_eq!(key, expected_key, "AUTH native key");
        assert_eq!(secret, expected_secret, "AUTH native secret");
        assert_eq!(window.to_bits(), 5000.0_f64.to_bits(), "AUTH native window");
        assert_eq!(units(&c.row["requests"][0]["key"]), key);
        assert_eq!(units(&c.row["requests"][0]["secret"]), secret);
        assert_eq!(c.row["requests"][0]["window"], 5000);
        assert_eq!(c.row["requests"][0]["sameSigner"], true);
        c.requests += 1;
        c.state.borrow_mut().insert(u("during"));
        if c.mode == "factory" {
            return Err(c.primary.clone());
        }
        Ok(Hold {
            open: c.open.clone(),
            result: if c.mode == "await" {
                Err(c.primary.clone())
            } else {
                Ok(c.signature.clone())
            },
        })
    }
    fn send_subscription(
        &mut self,
        streams: &[JsString],
        signature: &AuthSignature,
    ) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("lookup".into());
        assert!(
            Rc::ptr_eq(signature, &c.signature),
            "AUTH native signature array identity"
        );
        if c.mode == "lookup" {
            return Err(c.primary.clone());
        }
        let mut getter = u("getter");
        getter.push(0xd800);
        signature.borrow_mut()[2] = getter.clone();
        if c.mode == "pending" {
            c.state = set(&[u("replaced")]);
        }
        c.calls.push("send".into());
        let m = AuthMessage::snapshot(streams, signature);
        assert_eq!(m.method, "SUBSCRIBE", "AUTH native method");
        assert_eq!(m.params, c.streams, "AUTH native params");
        let mut changed = u("changed");
        changed.push(0xdc00);
        let expected = [
            u("SIGKEY"),
            if c.mode == "pending" {
                changed
            } else {
                vec![0xd800]
            },
            getter,
            u("W\0"),
        ];
        assert_eq!(m.signature, expected, "AUTH native signature values");
        assert_eq!(c.row["packets"][0]["method"], "SUBSCRIBE");
        assert_eq!(strings(&c.row["packets"][0]["params"]), m.params);
        assert_eq!(strings(&c.row["packets"][0]["signature"]), m.signature);
        let mut before = vec![u("keep")];
        if c.existing {
            before.extend(c.streams.clone());
        }
        before.push(u("during"));
        if c.mode == "pending" {
            before = vec![u("replaced")];
        }
        assert_eq!(
            c.state.borrow().values(),
            before,
            "AUTH native membership before send"
        );
        assert_eq!(strings(&c.row["before"][0]), before);
        let expected_wire = units(&c.row["wires"][0]);
        c.wires.push((m.to_json(), expected_wire));
        c.state.borrow_mut().remove(&u("during"));
        c.state.borrow_mut().insert(u("sent"));
        if ["send", "format", "emitLookup", "emit"].contains(&c.mode) {
            Err(c.primary.clone())
        } else {
            Ok(())
        }
    }
    fn error_text(&mut self, error: &Error) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("format".into());
        assert!(
            Rc::ptr_eq(error, &c.primary),
            "AUTH native formatter error identity"
        );
        if c.mode == "format" {
            Err(c.coerce.clone())
        } else {
            let mut text = u("boom");
            text.push(0xd800);
            Ok(text)
        }
    }
    fn error_emitter(&mut self) -> Result<Rc<()>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("emitLookup".into());
        if c.mode == "emitLookup" {
            Err(c.emit_lookup.clone())
        } else {
            Ok(c.emitter.clone())
        }
    }
    fn emit_error(&mut self, emitter: Rc<()>, message: JsString) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("emit".into());
        assert!(
            Rc::ptr_eq(&emitter, &c.emitter),
            "AUTH native emitter identity"
        );
        let prefix = [
            "Failed to authenticate WebSocket: ",
            "Failed to subscribe to order updates: ",
            "Failed to subscribe to position updates: ",
        ][c.method];
        let mut expected = u(prefix);
        expected.extend(u("boom"));
        expected.push(0xd800);
        assert_eq!(message, expected, "AUTH native emitted message");
        c.messages.push(message);
        if c.mode == "emit" {
            Err(c.emit.clone())
        } else {
            Ok(())
        }
    }
}
fn auth_cases(method: usize) {
    let rows = oracle()["auth"].as_array().unwrap();
    assert_eq!(rows.len(), 196, "AUTH independent corpus size");
    let (start, count) = match method {
        0 => (0, 28),
        1 => (28, 140),
        2 => (168, 28),
        _ => unreachable!(),
    };
    let mut wires = vec![];
    for j in 0..count {
        let row = &rows[start + j];
        let index = if method == 1 { j / 28 } else { 0 };
        let existing = (j % 28) / 14 == 1;
        let mode = AUTH_MODES[j % 14];
        let symbol = symbols()[index].clone();
        assert_eq!(row["method"], method);
        assert_eq!(row["symbolIndex"], index);
        assert_eq!(row["symbol"], json!(symbol));
        assert_eq!(row["existing"], existing);
        assert_eq!(row["mode"], mode);
        let streams = auth_streams(method, symbol.as_deref());
        let mut initial = vec![u("keep")];
        if existing {
            initial.extend(streams.clone());
        }
        let c = Rc::new(RefCell::new(AuthCtx {
            method,
            existing,
            mode,
            row: row.clone(),
            streams: streams.clone(),
            state: set(&initial),
            signature: Rc::new(RefCell::new([
                u("SIGKEY"),
                vec![0xd800],
                vec![0xdc00, 34, 92],
                u("W\0"),
            ])),
            open: Rc::new(Cell::new(mode != "pending")),
            calls: vec![],
            messages: vec![],
            wires: vec![],
            requests: 0,
            signer: Rc::new(()),
            emitter: Rc::new(()),
            primary: Rc::new("primary"),
            coerce: Rc::new("coerce"),
            emit_lookup: Rc::new("emitLookup"),
            emit: Rc::new("emit"),
        }));
        let future: Pin<Box<dyn Future<Output = Result<(), Error>> + '_>> = match method {
            0 => Box::pin(private_auth::authenticate(AuthOps(c.clone()))),
            1 => Box::pin(private_auth::subscribe_to_order_updates(
                symbol.as_deref(),
                AuthOps(c.clone()),
            )),
            2 => Box::pin(private_auth::subscribe_to_position_updates(AuthOps(
                c.clone(),
            ))),
            _ => unreachable!(),
        };
        let (eager, trace) = auth_trace(method, existing, mode, &streams);
        assert_eq!(
            c.borrow().calls,
            eager,
            "AUTH {method}/{j} native eager operations"
        );
        assert_eq!(row["eager"], json!(eager));
        let mut future = future;
        let first = poll(future.as_mut());
        let active = method == 0 || !existing;
        let guarded_error = method != 0 && mode == "has";
        let pending = active && !guarded_error && mode == "pending";
        assert_eq!(
            first.is_pending(),
            pending,
            "AUTH native pending {method}/{j}"
        );
        assert_eq!(row["pending"], pending);
        let result = if pending {
            let ctx = c.borrow();
            let mut changed = u("changed");
            changed.push(0xdc00);
            ctx.signature.borrow_mut()[1] = changed;
            ctx.open.set(true);
            drop(ctx);
            match poll(future.as_mut()) {
                Poll::Ready(r) => r,
                Poll::Pending => panic!("released signature must finish"),
            }
        } else {
            match first {
                Poll::Ready(r) => r,
                Poll::Pending => unreachable!(),
            }
        };
        let ctx = c.borrow();
        assert_eq!(ctx.calls, trace, "AUTH native operation order {method}/{j}");
        assert_eq!(row["calls"], json!(trace));
        let outcome = if guarded_error {
            "primary"
        } else if active && ["format", "emitLookup", "emit"].contains(&mode) {
            mode_to_outcome(mode)
        } else {
            "ok"
        };
        assert_eq!(row["outcome"], outcome);
        match result {
            Ok(()) => assert_eq!(outcome, "ok", "AUTH native result {method}/{j}"),
            Err(e) => {
                let expected = match outcome {
                    "primary" => &ctx.primary,
                    "coerce" => &ctx.coerce,
                    "emitLookup" => &ctx.emit_lookup,
                    "emit" => &ctx.emit,
                    _ => panic!("unexpected auth error"),
                };
                assert!(
                    Rc::ptr_eq(&e, expected),
                    "AUTH native outcome identity {method}/{j}"
                );
            }
        }
        let factory = active && !guarded_error && !["signer", "key", "secret"].contains(&mode);
        let send = factory && !["factory", "await", "lookup"].contains(&mode);
        let recovery = active && !guarded_error && !["success", "pending", "has"].contains(&mode);
        let emissions = usize::from(recovery && !["format", "emitLookup"].contains(&mode));
        assert_eq!(ctx.requests, usize::from(factory));
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            usize::from(factory)
        );
        for field in ["packets", "wires", "before"] {
            assert_eq!(
                row[field].as_array().unwrap().len(),
                usize::from(send),
                "AUTH independent {field} count {method}/{j}"
            );
        }
        assert_eq!(ctx.messages.len(), emissions);
        assert_eq!(row["messages"].as_array().unwrap().len(), emissions);
        assert_eq!(strings(&row["messages"]), ctx.messages);
        let mut after = initial;
        if factory {
            after.push(u("during"));
        }
        if send {
            after.retain(|v| v != &u("during"));
            if mode == "pending" {
                after = vec![u("replaced")];
            }
            after.push(u("sent"));
            if !["send", "format", "emitLookup", "emit"].contains(&mode) {
                for s in streams {
                    if !after.contains(&s) {
                        after.push(s);
                    }
                    if mode == "add" {
                        break;
                    }
                }
            }
        }
        assert_eq!(
            ctx.state.borrow().values(),
            after,
            "AUTH native final membership {method}/{j}"
        );
        assert_eq!(strings(&row["after"]), after);
        wires.extend(ctx.wires.clone());
    }
    for (i, (actual, expected)) in wires.into_iter().enumerate() {
        assert_eq!(
            actual, expected,
            "AUTH wire {method}/{i} after native assertions"
        );
    }
}
fn mode_to_outcome(mode: &str) -> &str {
    if mode == "format" { "coerce" } else { mode }
}
#[test]
fn private_authenticate() {
    auth_cases(0);
}
#[test]
fn private_order_subscriptions() {
    auth_cases(1);
}
#[test]
fn private_position_subscriptions() {
    auth_cases(2);
}

const CONNECT_FAILURES: [&str; 11] = [
    "none", "disposed", "options", "url", "create", "assign", "method", "handler", "open",
    "message", "close",
];
struct ConnectCtx {
    disposed: bool,
    testnet: bool,
    fail: &'static str,
    calls: Vec<String>,
    current: Rc<&'static str>,
    old: Rc<&'static str>,
    created: Rc<&'static str>,
    second: Rc<&'static str>,
    handlers: [Rc<()>; 3],
    registrations: Vec<(&'static str, &'static str)>,
    urls: Vec<JsString>,
    error: Error,
}
struct ConnectOps(Rc<RefCell<ConnectCtx>>);
impl Connection for ConnectOps {
    type Socket = Rc<&'static str>;
    type Error = Error;
    fn is_disposed(&mut self) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("disposed".into());
        if c.fail == "disposed" {
            Err(c.error.clone())
        } else {
            Ok(c.disposed)
        }
    }
    fn is_testnet(&mut self) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("options".into());
        if c.fail == "options" {
            Err(c.error.clone())
        } else {
            Ok(c.testnet)
        }
    }
    fn websocket_url(&mut self, network: Network) -> Result<JsString, Error> {
        let mut c = self.0.borrow_mut();
        assert_eq!(
            network,
            if c.testnet {
                Network::Testnet
            } else {
                Network::Production
            },
            "CONNECT native selected network"
        );
        c.calls.push(
            if network == Network::Testnet {
                "url:TESTNET"
            } else {
                "url:PRODUCTION"
            }
            .into(),
        );
        if c.fail == "url" {
            return Err(c.error.clone());
        }
        Ok(if c.testnet {
            let mut v = u("wss://testnet.fixture/");
            v.push(0xd800);
            v
        } else {
            u("wss://production.fixture/λ")
        })
    }
    fn create_websocket(&mut self, url: JsString) -> Result<Rc<&'static str>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("create".into());
        let expected = if c.testnet {
            let mut v = u("wss://testnet.fixture/");
            v.push(0xd800);
            v
        } else {
            u("wss://production.fixture/λ")
        };
        assert_eq!(url, expected, "CONNECT native URL");
        c.urls.push(url);
        if c.fail == "create" {
            Err(c.error.clone())
        } else {
            Ok(c.created.clone())
        }
    }
    fn assign_websocket(&mut self, socket: Rc<&'static str>) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push(format!("assign:{socket}"));
        assert!(
            Rc::ptr_eq(&socket, &c.created),
            "CONNECT native created socket identity"
        );
        if c.fail == "assign" {
            return Err(c.error.clone());
        }
        c.current = socket;
        Ok(())
    }
    fn add_listener(&mut self, event: Event) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("socket".into());
        let receiver = c.current.clone();
        c.calls.push(format!("method:{receiver}"));
        if c.fail == "method" {
            return Err(c.error.clone());
        }
        let name = event.name();
        c.calls.push(format!("handler:{name}"));
        if c.fail == "handler" {
            return Err(c.error.clone());
        }
        let index = match event {
            Event::Open => 0,
            Event::Message => 1,
            Event::Close => 2,
        };
        let handler = c.handlers[index].clone();
        if event == Event::Open {
            c.calls.push("assign:second".into());
            c.current = c.second.clone();
        }
        let expected_receiver = if event == Event::Open {
            &c.created
        } else {
            &c.second
        };
        assert!(
            Rc::ptr_eq(&receiver, expected_receiver),
            "CONNECT native captured listener receiver"
        );
        assert!(
            Rc::ptr_eq(&handler, &c.handlers[index]),
            "CONNECT native handler identity"
        );
        c.calls.push(format!("listen:{name}:{receiver}"));
        c.registrations.push((name, *receiver));
        if c.fail == name {
            Err(c.error.clone())
        } else {
            Ok(())
        }
    }
}
fn connect_trace(disposed: bool, testnet: bool, fail: &str) -> Vec<String> {
    let mut v = vec!["disposed".into()];
    if disposed || fail == "disposed" {
        return v;
    }
    for (name, phase) in [
        ("options", "options"),
        (
            if testnet {
                "url:TESTNET"
            } else {
                "url:PRODUCTION"
            },
            "url",
        ),
        ("create", "create"),
        ("assign:created", "assign"),
    ] {
        v.push(name.into());
        if fail == phase {
            return v;
        }
    }
    for event in ["open", "message", "close"] {
        let socket = if event == "open" { "created" } else { "second" };
        v.push("socket".into());
        v.push(format!("method:{socket}"));
        if fail == "method" {
            return v;
        }
        v.push(format!("handler:{event}"));
        if fail == "handler" {
            return v;
        }
        if event == "open" {
            v.push("assign:second".into());
        }
        v.push(format!("listen:{event}:{socket}"));
        if fail == event {
            return v;
        }
    }
    v
}
fn connection(private: bool) {
    let rows = oracle()["connection"].as_array().unwrap();
    assert_eq!(rows.len(), 88, "CONNECT independent size");
    let start = if private { 44 } else { 0 };
    for i in 0..44 {
        let row = &rows[start + i];
        let disposed = i / 22 == 1;
        let testnet = (i % 22) / 11 == 1;
        let fail = CONNECT_FAILURES[i % 11];
        assert_eq!(row["kind"], if private { "private" } else { "public" });
        assert_eq!(row["disposed"], disposed);
        assert_eq!(row["testnet"], testnet);
        assert_eq!(row["fail"], fail);
        let old = Rc::new("old");
        let c = Rc::new(RefCell::new(ConnectCtx {
            disposed,
            testnet,
            fail,
            calls: vec![],
            current: old.clone(),
            old,
            created: Rc::new("created"),
            second: Rc::new("second"),
            handlers: [Rc::new(()), Rc::new(()), Rc::new(())],
            registrations: vec![],
            urls: vec![],
            error: Rc::new("connect error"),
        }));
        let mut host = ConnectOps(c.clone());
        let expected = connect_trace(disposed, testnet, fail);
        let result = if private {
            let f = connect_private(&mut host);
            assert_eq!(
                c.borrow().calls,
                expected,
                "CONNECT_PRIVATE native eager operations {i}"
            );
            assert_eq!(row["eager"], json!(expected));
            ready(f)
        } else {
            connect_public(&mut host)
        };
        let ctx = c.borrow();
        assert_eq!(ctx.calls, expected, "CONNECT native operation order {i}");
        assert_eq!(row["calls"], json!(expected));
        let rejected = fail == "disposed" || (!disposed && fail != "none");
        original_error(result, &ctx.error, rejected, row, &format!("CONNECT {i}"));
        let created = expected.contains(&"create".to_string());
        assert_eq!(
            ctx.urls.len(),
            usize::from(created),
            "CONNECT native create count {i}"
        );
        assert_eq!(strings(&row["created"]), ctx.urls);
        let expected_registrations: Vec<_> = expected
            .iter()
            .filter(|s| s.starts_with("listen:"))
            .map(|s| {
                let mut v = s.split(':');
                v.next();
                (v.next().unwrap(), v.next().unwrap())
            })
            .collect();
        assert_eq!(
            ctx.registrations, expected_registrations,
            "CONNECT native registrations {i}"
        );
        assert_eq!(
            row["registrations"].as_array().unwrap().len(),
            expected_registrations.len()
        );
        for (j, (event, socket)) in ctx.registrations.iter().enumerate() {
            assert_eq!(row["registrations"][j]["event"], *event);
            assert_eq!(row["registrations"][j]["socket"], *socket);
            assert_eq!(row["registrations"][j]["sameHandler"], true);
            assert_eq!(row["registrations"][j]["sameReceiver"], true);
        }
        let switched = expected.contains(&"assign:second".to_string());
        let assigned = expected.contains(&"assign:created".to_string()) && fail != "assign";
        let current = if switched {
            &ctx.second
        } else if assigned {
            &ctx.created
        } else {
            &ctx.old
        };
        assert!(
            Rc::ptr_eq(&ctx.current, current),
            "CONNECT native final socket {i}"
        );
        assert_eq!(row["current"], **current);
        assert_eq!(row["sameInitial"], !assigned);
    }
}
#[test]
fn public_connection() {
    connection(false);
}
#[test]
fn private_connection() {
    connection(true);
}

struct OpenCtx {
    disposed: bool,
    fail: &'static str,
    calls: Vec<&'static str>,
    open: Rc<Cell<bool>>,
    outstanding: bool,
    error: Error,
}
struct OpenOps(Rc<RefCell<OpenCtx>>);
impl PrivateOpen for OpenOps {
    type Error = Error;
    type Authentication = Hold<()>;
    fn is_disposed(&mut self) -> Result<bool, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("disposed");
        if c.fail == "disposed" {
            Err(c.error.clone())
        } else {
            Ok(c.disposed)
        }
    }
    fn track_last_received(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("track");
        if c.fail == "track" {
            Err(c.error.clone())
        } else {
            Ok(())
        }
    }
    fn authenticate(&mut self) -> Result<Hold<()>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("auth");
        if c.fail == "factory" {
            return Err(c.error.clone());
        }
        Ok(Hold {
            open: c.open.clone(),
            result: if c.fail == "await" {
                Err(c.error.clone())
            } else {
                Ok(())
            },
        })
    }
    fn resubscribe(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("resubscribe");
        if c.fail == "resubscribe" {
            Err(c.error.clone())
        } else {
            c.outstanding = true;
            Ok(())
        }
    }
    fn ping(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("ping");
        if c.fail == "ping" {
            Err(c.error.clone())
        } else {
            Ok(())
        }
    }
}
#[test]
fn private_open() {
    let rows = oracle()["open"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "OPEN independent size");
    for (i, row) in rows.iter().enumerate() {
        let disposed = i / 8 == 1;
        let fail = [
            "none",
            "disposed",
            "track",
            "factory",
            "await",
            "resubscribe",
            "ping",
            "pending",
        ][i % 8];
        assert_eq!(row["disposed"], disposed);
        assert_eq!(row["fail"], fail);
        let c = Rc::new(RefCell::new(OpenCtx {
            disposed,
            fail,
            calls: vec![],
            open: Rc::new(Cell::new(fail != "pending")),
            outstanding: false,
            error: Rc::new("open error"),
        }));
        let f = on_open_private(OpenOps(c.clone()));
        let mut eager = vec!["disposed"];
        if !disposed && fail != "disposed" {
            eager.push("track");
            if fail != "track" {
                eager.push("auth");
            }
        }
        assert_eq!(
            c.borrow().calls,
            eager,
            "OPEN_PRIVATE native eager operations {i}"
        );
        assert_eq!(row["eager"], json!(eager));
        let mut f = std::pin::pin!(f);
        let first = poll(f.as_mut());
        let pending = !disposed && fail == "pending";
        assert_eq!(
            first.is_pending(),
            pending,
            "OPEN_PRIVATE native pending {i}"
        );
        assert_eq!(row["pending"], pending);
        let result = if pending {
            c.borrow().open.set(true);
            match poll(f.as_mut()) {
                Poll::Ready(r) => r,
                Poll::Pending => panic!("released auth must finish"),
            }
        } else {
            match first {
                Poll::Ready(r) => r,
                Poll::Pending => unreachable!(),
            }
        };
        let mut expected = eager;
        if !disposed && !["disposed", "track", "factory", "await"].contains(&fail) {
            expected.push("resubscribe");
            if fail != "resubscribe" {
                expected.push("ping");
            }
        }
        let ctx = c.borrow();
        assert_eq!(
            ctx.calls, expected,
            "OPEN_PRIVATE native callback order {i}"
        );
        assert_eq!(row["calls"], json!(expected));
        let rejected = fail == "disposed" || (!disposed && !["none", "pending"].contains(&fail));
        original_error(
            result,
            &ctx.error,
            rejected,
            row,
            &format!("OPEN_PRIVATE {i}"),
        );
        assert_eq!(
            ctx.outstanding,
            expected.contains(&"ping"),
            "OPEN_PRIVATE native ignores pending resubscribe {i}"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Timer {
    Undefined,
    Zero,
    A,
    B,
}
struct DisposalCtx {
    mode: &'static str,
    calls: Vec<&'static str>,
    timer: Timer,
    timer_reads: usize,
    socket_reads: usize,
    subscriptions: Vec<&'static str>,
    first: Rc<()>,
    second: Rc<()>,
    error: Error,
    missing: Error,
}
struct DisposalOps(Rc<RefCell<DisposalCtx>>);
impl PrivateDisposal for DisposalOps {
    type Error = Error;
    type Timer = Timer;
    type Socket = Rc<()>;
    fn clear_subscriptions(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("clearSub");
        if c.mode == "clearSub" {
            return Err(c.error.clone());
        }
        c.subscriptions.clear();
        Ok(())
    }
    fn timeout(&mut self) -> Result<Timer, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("timer");
        c.timer_reads += 1;
        if c.mode == "timerSecond" && c.timer_reads == 2 {
            return Err(c.error.clone());
        }
        Ok(if c.timer_reads >= 2 && c.timer == Timer::A {
            Timer::B
        } else {
            c.timer.clone()
        })
    }
    fn timeout_truthy(&self, t: &Timer) -> bool {
        matches!(t, Timer::A | Timer::B)
    }
    fn clear_timeout(&mut self, t: Timer) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("clearTimer");
        assert_eq!(t, Timer::B, "DISPOSE_PRIVATE native second timer value");
        if c.mode == "clearTimer" {
            Err(c.error.clone())
        } else {
            Ok(())
        }
    }
    fn unset_timeout(&mut self) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("unset");
        if c.mode == "unsetTimer" {
            return Err(c.error.clone());
        }
        c.timer = Timer::Undefined;
        Ok(())
    }
    fn socket(&mut self) -> Result<Option<Rc<()>>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("socket");
        c.socket_reads += 1;
        if c.mode == "socketSecond" && c.socket_reads == 2 {
            return Err(c.error.clone());
        }
        if c.mode == "missingSocket" && c.socket_reads >= 2 {
            return Ok(None);
        }
        Ok(Some(
            if c.mode == "differentSocket" && c.socket_reads >= 2 {
                c.second.clone()
            } else {
                c.first.clone()
            },
        ))
    }
    fn close_socket(&mut self, s: Option<Rc<()>>) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        let Some(s) = s else {
            return Err(c.missing.clone());
        };
        let second = c.mode == "differentSocket";
        assert!(
            Rc::ptr_eq(&s, if second { &c.second } else { &c.first }),
            "DISPOSE_PRIVATE native current socket identity"
        );
        c.calls.push(if second { "close:B" } else { "close:A" });
        if c.mode == "close" {
            Err(c.error.clone())
        } else {
            Ok(())
        }
    }
}
#[test]
fn private_disposal() {
    let rows = oracle()["disposal"].as_array().unwrap();
    assert_eq!(rows.len(), 11, "DISPOSE_PRIVATE independent size");
    for (i, row) in rows.iter().enumerate() {
        let mode = [
            "none",
            "absentTimer",
            "zeroTimer",
            "clearSub",
            "timerSecond",
            "clearTimer",
            "unsetTimer",
            "socketSecond",
            "close",
            "differentSocket",
            "missingSocket",
        ][i];
        assert_eq!(row["mode"], mode);
        let c = Rc::new(RefCell::new(DisposalCtx {
            mode,
            calls: vec![],
            timer: match mode {
                "absentTimer" => Timer::Undefined,
                "zeroTimer" => Timer::Zero,
                _ => Timer::A,
            },
            timer_reads: 0,
            socket_reads: 0,
            subscriptions: vec!["a"],
            first: Rc::new(()),
            second: Rc::new(()),
            error: Rc::new("dispose error"),
            missing: Rc::new("missing-property error"),
        }));
        let result = dispose_private(&mut DisposalOps(c.clone()));
        let mut expected = vec!["clearSub"];
        if mode != "clearSub" {
            expected.push("timer");
            if !["absentTimer", "zeroTimer"].contains(&mode) {
                expected.push("timer");
                if mode != "timerSecond" {
                    expected.push("clearTimer");
                    if mode != "clearTimer" {
                        expected.push("unset");
                    }
                }
            }
            if !["timerSecond", "clearTimer", "unsetTimer"].contains(&mode) {
                expected.extend(["socket", "socket"]);
                if !["socketSecond", "missingSocket"].contains(&mode) {
                    expected.push(if mode == "differentSocket" {
                        "close:B"
                    } else {
                        "close:A"
                    });
                }
            }
        }
        let ctx = c.borrow();
        assert_eq!(
            ctx.calls, expected,
            "DISPOSE_PRIVATE native operation order {i}"
        );
        assert_eq!(row["calls"], json!(expected));
        let outcome = !["none", "absentTimer", "zeroTimer", "differentSocket"].contains(&mode);
        if mode == "missingSocket" {
            assert!(Rc::ptr_eq(
                &result.expect_err("missing access must fail"),
                &ctx.missing
            ));
            assert_eq!(row["missingError"], true);
            assert_eq!(row["sameError"], false);
        } else {
            assert_eq!(row["missingError"], false);
            original_error(
                result,
                &ctx.error,
                outcome,
                row,
                &format!("DISPOSE_PRIVATE {i}"),
            );
        }
        let cleared = ![
            "clearSub",
            "timerSecond",
            "clearTimer",
            "unsetTimer",
            "zeroTimer",
        ]
        .contains(&mode);
        assert_eq!(
            ctx.timer == Timer::Undefined,
            cleared,
            "DISPOSE_PRIVATE native cleared timer {i}"
        );
        assert_eq!(
            row["timer"],
            match ctx.timer {
                Timer::Undefined => "undefined",
                Timer::Zero => "zero",
                _ => "A",
            }
        );
        assert_eq!(ctx.subscriptions.is_empty(), mode != "clearSub");
        assert_eq!(row["subscriptions"], json!(ctx.subscriptions));
    }
}

#[derive(Clone)]
enum CacheValue {
    Undefined,
    Number(f64),
    Handle(Rc<&'static str>),
}
fn label(v: &CacheValue) -> String {
    match v {
        CacheValue::Undefined => "undefined".into(),
        CacheValue::Number(n) => format!("number:{:016x}", n.to_bits()),
        CacheValue::Handle(h) => h.to_string(),
    }
}
struct CacheCtx {
    input: &'static str,
    fail: &'static str,
    symbol: JsString,
    calls: Vec<&'static str>,
    original: Rc<RefCell<CacheValue>>,
    current: Rc<RefCell<CacheValue>>,
    replacement_map: Rc<RefCell<CacheValue>>,
    new: Rc<&'static str>,
    alternate: Rc<&'static str>,
    updater: Rc<()>,
    factory_calls: usize,
    error: Error,
}
struct CacheOps(Rc<RefCell<CacheCtx>>);
impl DepthThrottle for CacheOps {
    type Error = Error;
    type Value = CacheValue;
    type Updater = Rc<()>;
    type Target = Rc<RefCell<CacheValue>>;
    fn throttle_target(&mut self) -> Result<Self::Target, Error> {
        Ok(self.0.borrow().current.clone())
    }
    fn cached_throttle(&mut self, symbol: &[u16]) -> Result<CacheValue, Error> {
        let mut c = self.0.borrow_mut();
        assert_eq!(symbol, c.symbol, "THROTTLE native symbol");
        if ["constructor", "__proto__"].contains(&c.input) || !Rc::ptr_eq(&c.current, &c.original) {
            return Ok(c.current.borrow().clone());
        }
        c.calls.push("get");
        if c.fail == "get" {
            Err(c.error.clone())
        } else {
            Ok(c.current.borrow().clone())
        }
    }
    fn is_truthy(&self, v: &CacheValue) -> bool {
        match v {
            CacheValue::Undefined => false,
            CacheValue::Number(n) => *n != 0.0 && !n.is_nan(),
            CacheValue::Handle(_) => true,
        }
    }
    fn depth_updater(&mut self) -> Result<Rc<()>, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("updater");
        if c.input == "updaterMap" {
            c.current = c.replacement_map.clone();
        }
        if c.fail == "updater" {
            Err(c.error.clone())
        } else {
            Ok(c.updater.clone())
        }
    }
    fn create_throttle(
        &mut self,
        updater: Rc<()>,
        wait: f64,
        leading: bool,
        trailing: bool,
    ) -> Result<CacheValue, Error> {
        let mut c = self.0.borrow_mut();
        c.calls.push("factory");
        assert!(
            Rc::ptr_eq(&updater, &c.updater),
            "THROTTLE native updater identity"
        );
        assert_eq!(wait.to_bits(), 100.0_f64.to_bits(), "THROTTLE native wait");
        assert!(leading, "THROTTLE native leading");
        assert!(trailing, "THROTTLE native trailing");
        c.factory_calls += 1;
        if c.input == "factoryMap" {
            c.current = c.replacement_map.clone();
        }
        if c.fail == "factory" {
            Err(c.error.clone())
        } else {
            Ok(CacheValue::Handle(c.new.clone()))
        }
    }
    fn set_throttle(
        &mut self,
        target: Self::Target,
        symbol: &[u16],
        v: CacheValue,
    ) -> Result<(), Error> {
        let mut c = self.0.borrow_mut();
        assert!(
            Rc::ptr_eq(&target, &c.original),
            "THROTTLE native captured assignment target"
        );
        c.calls.push("set");
        assert_eq!(symbol, c.symbol);
        let CacheValue::Handle(h) = v else {
            panic!("factory must return new handle")
        };
        assert!(
            Rc::ptr_eq(&h, &c.new),
            "THROTTLE native assigned factory handle"
        );
        if c.fail == "set" {
            return Err(c.error.clone());
        }
        *target.borrow_mut() = CacheValue::Handle(if c.input == "replacement" {
            c.alternate.clone()
        } else {
            h
        });
        Ok(())
    }
}
#[test]
fn depth_throttle_cache() {
    let rows = oracle()["cache"].as_array().unwrap();
    assert_eq!(rows.len(), 50, "THROTTLE independent size");
    for (i, row) in rows.iter().enumerate() {
        let input = [
            "missing",
            "zero",
            "nan",
            "old",
            "constructor",
            "__proto__",
            "lone",
            "replacement",
            "updaterMap",
            "factoryMap",
        ][i / 5];
        let fail = ["none", "get", "updater", "factory", "set"][i % 5];
        let symbol = match input {
            "constructor" | "__proto__" => u(input),
            "lone" => vec![88, 0xd800],
            _ => u("BTC"),
        };
        assert_eq!(row["input"], input);
        assert_eq!(row["fail"], fail);
        assert_eq!(units(&row["symbol"]), symbol);
        let initial = match input {
            "zero" => CacheValue::Number(0.0),
            "nan" => CacheValue::Number(f64::NAN),
            "old" => CacheValue::Handle(Rc::new("old")),
            "constructor" => CacheValue::Handle(Rc::new("constructor")),
            "__proto__" => CacheValue::Handle(Rc::new("prototype")),
            _ => CacheValue::Undefined,
        };
        let original = Rc::new(RefCell::new(initial));
        let alternate = Rc::new("alternate");
        let c = Rc::new(RefCell::new(CacheCtx {
            input,
            fail,
            symbol: symbol.clone(),
            calls: vec![],
            current: original.clone(),
            original,
            replacement_map: Rc::new(RefCell::new(CacheValue::Handle(alternate.clone()))),
            new: Rc::new("new"),
            alternate,
            updater: Rc::new(()),
            factory_calls: 0,
            error: Rc::new("throttle error"),
        }));
        let result = get_depth_ticker_update_throttle(&symbol, &mut CacheOps(c.clone()));
        let inherited = ["constructor", "__proto__"].contains(&input);
        let mut expected = vec![];
        let mut rejected = false;
        if !inherited {
            expected.push("get");
            if fail == "get" {
                rejected = true;
            } else if input == "old" {
                expected.push("get");
            } else {
                for name in ["updater", "factory", "set", "get"] {
                    if name == "get" && ["updaterMap", "factoryMap"].contains(&input) {
                        continue;
                    }
                    expected.push(name);
                    if fail == name {
                        rejected = true;
                        break;
                    }
                }
            }
        }
        let ctx = c.borrow();
        assert_eq!(ctx.calls, expected, "THROTTLE native operation order {i}");
        assert_eq!(row["calls"], json!(expected));
        let factory = expected.contains(&"factory");
        assert_eq!(ctx.factory_calls, usize::from(factory));
        assert_eq!(
            row["factoryArgs"].as_array().unwrap().len(),
            usize::from(factory)
        );
        if factory {
            assert_eq!(row["factoryArgs"][0]["wait"], 100);
            assert_eq!(row["factoryArgs"][0]["leading"], true);
            assert_eq!(row["factoryArgs"][0]["trailing"], true);
            assert_eq!(row["factoryArgs"][0]["sameUpdater"], true);
        }
        assert_eq!(
            result.is_err(),
            rejected,
            "THROTTLE native error presence {i}"
        );
        assert_eq!(row["sameError"], rejected);
        let current_value = ctx.current.borrow();
        match result {
            Err(e) => {
                assert!(Rc::ptr_eq(&e, &ctx.error));
                assert_eq!(row["result"], Value::Null);
            }
            Ok(v) => {
                let CacheValue::Handle(h) = &v else {
                    panic!("success must return cached callable/prototype value")
                };
                let CacheValue::Handle(current) = &*current_value else {
                    panic!("successful cache must contain handle")
                };
                assert!(
                    Rc::ptr_eq(h, current),
                    "THROTTLE native returned current property identity {i}"
                );
                assert_eq!(row["result"], label(&v));
                assert_eq!(row["sameValue"], true);
            }
        }
        assert_eq!(row["current"], label(&current_value));
        assert_eq!(row["original"], label(&ctx.original.borrow()));
        if !rejected && ["updaterMap", "factoryMap"].contains(&input) {
            let original = ctx.original.borrow();
            let CacheValue::Handle(handle) = &*original else {
                panic!("original map must contain factory handle");
            };
            assert!(
                Rc::ptr_eq(handle, &ctx.new),
                "THROTTLE native original map factory identity {i}"
            );
            let CacheValue::Handle(handle) = &*current_value else {
                panic!("replacement map must retain alternate");
            };
            assert!(
                Rc::ptr_eq(handle, &ctx.alternate),
                "THROTTLE native replacement map retained identity {i}"
            );
        }
    }
}

struct Options {
    symbol: JsString,
    interval: JsString,
}
fn delegation(book: bool) {
    let rows = oracle()["delegation"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "LISTEN independent size");
    let start = if book { 6 } else { 0 };
    for j in 0..6 {
        let row = &rows[start + j];
        let i = j / 2;
        let fail = j % 2 == 1;
        let symbol = [u("BTC"), u("λ🚀"), vec![88, 0xd800]][i].clone();
        let interval = [u("1m"), u("5m"), vec![0xdc00]][i].clone();
        assert_eq!(row["method"], if book { "book" } else { "ohlcv" });
        assert_eq!(row["index"], i);
        assert_eq!(row["fail"], fail);
        assert_eq!(units(&row["symbol"]), symbol);
        assert_eq!(units(&row["interval"]), interval);
        let opts = Rc::new(Options {
            symbol: symbol.clone(),
            interval,
        });
        let callback = Rc::new(());
        let unsubscribe = Rc::new(Cell::new(77));
        let error = Rc::new("delegate error");
        let calls = Cell::new(0);
        let result = if book {
            listen_order_book(&symbol, &callback, |s, c| {
                calls.set(calls.get() + 1);
                assert_eq!(s, &symbol, "LISTEN_BOOK native symbol");
                assert!(
                    Rc::ptr_eq(c, &callback),
                    "LISTEN_BOOK native callback identity"
                );
                assert_eq!(units(&row["calls"][0]["symbol"]), *s);
                if fail {
                    Err(error.clone())
                } else {
                    Ok(unsubscribe.clone())
                }
            })
        } else {
            listen_ohlcv(&opts, &callback, |o, c| {
                calls.set(calls.get() + 1);
                assert!(Rc::ptr_eq(o, &opts), "LISTEN_OHLCV native opts identity");
                assert!(
                    Rc::ptr_eq(c, &callback),
                    "LISTEN_OHLCV native callback identity"
                );
                assert_eq!(units(&row["calls"][0]["symbol"]), o.symbol);
                assert_eq!(units(&row["calls"][0]["interval"]), o.interval);
                assert_eq!(row["calls"][0]["sameOpts"], true);
                if fail {
                    Err(error.clone())
                } else {
                    Ok(unsubscribe.clone())
                }
            })
        };
        assert_eq!(calls.get(), 1, "LISTEN native count");
        assert_eq!(row["calls"].as_array().unwrap().len(), 1);
        assert_eq!(row["calls"][0]["sameCallback"], true);
        assert_eq!(result.is_err(), fail);
        assert_eq!(row["sameError"], fail);
        match result {
            Ok(r) => {
                assert!(
                    Rc::ptr_eq(&r, &unsubscribe),
                    "{} native return identity",
                    if book { "LISTEN_BOOK" } else { "LISTEN_OHLCV" }
                );
                assert_eq!(r.get(), 77);
                assert_eq!(row["sameReturn"], true);
            }
            Err(e) => {
                assert!(Rc::ptr_eq(&e, &error));
                assert_eq!(row["sameReturn"], false);
            }
        }
    }
}
#[test]
fn exchange_listen_ohlcv() {
    delegation(false);
}
#[test]
fn exchange_listen_order_book() {
    delegation(true);
}
