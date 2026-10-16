//! Private websocket session bodies. Native bindings retain source value/callee
//! identities; complete class construction and composition remain separate.
use crate::public_subscriptions::{ConnectHost, DialRequest, Runtime};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
/// Source value construction, optional chaining, ordinary JSON and object spread.
pub trait SessionRuntime: Runtime {
    fn text(&self, units: Vec<u16>) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn json_parse(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// CreateDataProperty for object-literal members; does not invoke inherited setters.
    fn define_field(
        &self,
        value: &Self::Value,
        key: &str,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
    fn set_field(
        &self,
        value: &Self::Value,
        key: &str,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
}
/// Capture createWebsocket before URL argument lookup, as source evaluation does.
pub trait PrivateConnectHost: ConnectHost {
    type Create;
    fn public_only(&self) -> Result<Self::Value, Self::Error>;
    fn key(&self) -> Result<Self::Value, Self::Error>;
    fn capture_create(&self) -> Result<Self::Create, Self::Error>;
    fn invoke_create(
        &self,
        callee: Self::Create,
        request: DialRequest<Self::Value>,
    ) -> Result<Self::Socket, Self::Error>;
}
pub trait OpenHost: Runtime {
    fn disposed(&self) -> bool;
    fn start_heartbeat(&self) -> Result<(), Self::Error>;
    /// Ordinary child call: synchronous throw propagates; returned Promise is ignored.
    fn send_login(&self) -> Result<(), Self::Error>;
    fn private_snapshot(&self, delay: u64) -> Result<(), Self::Error>;
    fn reconnected(&self) -> Result<(), Self::Error>;
}
/// Host-owned completion is started before await and survives observer removal.
pub trait Completion<V, E> {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<V, E>>;
}
pub trait TaskHost: SessionRuntime {
    type Operation: Completion<Self::Value, Self::Error>;
    type Resolution: Completion<Self::Value, Self::Error>;
    /// Source Promise resolution: primitive returns settle immediately; thenables
    /// read then now and schedule its invocation/adoption on the runtime queue.
    fn resolve_return(&self, value: Self::Value) -> Result<Rc<Self::Resolution>, Self::Error>;
    /// Eagerly poll the whole method through its synchronous prefix and retain
    /// the task through completion; resume awaits on the source microtask queue.
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>);
}
struct State<V, E> {
    value: Option<Result<V, E>>,
    waker: Option<Waker>,
}
/// Observation only: dropping this future does not cancel the method task.
pub struct MethodObserver<V, E> {
    state: Rc<RefCell<State<V, E>>>,
}
impl<V, E> Future for MethodObserver<V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut s = self.state.borrow_mut();
        if let Some(v) = s.value.take() {
            Poll::Ready(v)
        } else {
            s.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
pub(crate) fn start<H: TaskHost>(
    host: Rc<H>,
    body: impl Future<Output = Result<H::Value, H::Error>> + 'static,
) -> MethodObserver<H::Value, H::Error> {
    let state = Rc::new(RefCell::new(State {
        value: None,
        waker: None,
    }));
    let output = state.clone();
    let resolver = host.clone();
    host.start_task(Box::pin(async move {
        let value = match body.await {
            Err(error) => Err(error),
            Ok(value) => match resolver.resolve_return(value) {
                Err(error) => Err(error),
                Ok(op) => {
                    Observe {
                        op,
                        yielded: true,
                        marker: std::marker::PhantomData,
                    }
                    .await
                }
            },
        };
        let waker = {
            let mut s = output.borrow_mut();
            s.value = Some(value);
            s.waker.take()
        };
        if let Some(w) = waker {
            w.wake()
        }
    }));
    MethodObserver { state }
}
struct Observe<O, V, E> {
    op: Rc<O>,
    yielded: bool,
    marker: std::marker::PhantomData<fn() -> (V, E)>,
}
impl<O: Completion<V, E>, V, E> Future for Observe<O, V, E> {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        if !s.yielded {
            s.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        s.op.poll(cx)
    }
}
pub(crate) async fn observe<O: Completion<V, E>, V, E>(op: Result<Rc<O>, E>) -> Result<V, E> {
    Observe {
        op: op?,
        yielded: false,
        marker: std::marker::PhantomData,
    }
    .await
}
/// Native anonymous login object, before the source stringify operation.
pub struct LoginFrame<V> {
    pub method: &'static str,
    pub api_key: V,
    pub req_time: V,
    pub signature: V,
}
/// Source remote-signer request and nested wsAuth/extraData fields.
pub struct SigningRequest<V> {
    pub id: V,
    pub request_type: &'static str,
    pub exchange_name: &'static str,
    pub account_name: V,
    pub to_sign: Vec<u16>,
    pub output_format: &'static str,
    pub req_time: V,
}
pub trait LoginHost: TaskHost {
    type Sign;
    type Send;
    type Emit;
    fn key(&self) -> Result<Self::Value, Self::Error>;
    /// String.prototype.trim lookup/call; preserve original TypeError/coercion.
    fn trim(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn clock(&self) -> Result<Self::Value, Self::Error>;
    fn create_signature(
        &self,
        api_key: Self::Value,
        req_time: Self::Value,
    ) -> Result<Rc<Self::Operation>, Self::Error>;
    fn capture_send(&self) -> Result<Self::Send, Self::Error>;
    fn stringify_login(&self, frame: LoginFrame<Self::Value>) -> Result<Vec<u16>, Self::Error>;
    fn send_login_frame(
        &self,
        send: Self::Send,
        data: Vec<u16>,
        redact_auth: bool,
    ) -> Result<(), Self::Error>;
    fn remote_signer(&self) -> Result<Self::Value, Self::Error>;
    fn secret(&self) -> Result<Self::Value, Self::Error>;
    fn local_sign(
        &self,
        secret: Self::Value,
        api_key: Self::Value,
        req_time: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn capture_sign(&self, remote: &Self::Value) -> Result<Self::Sign, Self::Error>;
    fn nanoid(&self) -> Result<Self::Value, Self::Error>;
    fn sign(
        &self,
        callee: Self::Sign,
        request: SigningRequest<Self::Value>,
    ) -> Result<Rc<Self::Operation>, Self::Error>;
    fn capture_emit_error(&self) -> Result<Self::Emit, Self::Error>;
    fn emit_error(&self, callee: Self::Emit, text: Vec<u16>) -> Result<(), Self::Error>;
}
/// Deferred formatter must reread current options/redactAuth at each invocation.
pub struct DiagnosticOptions<V, E> {
    pub format: Rc<dyn Fn(V) -> Result<Vec<u16>, E>>,
}
pub trait DiagnosticHost: SessionRuntime {
    fn send(&self, data: &[u16]) -> Result<(), Self::Error>;
    fn emit_send(
        &self,
        data: &[u16],
        options: DiagnosticOptions<Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn emit_receive(
        &self,
        data: Self::Value,
        options: DiagnosticOptions<Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn redact(&self, data: Self::Value) -> Result<Self::Value, Self::Error>;
}
pub trait BalanceHost: TaskHost {
    type Handle: Clone;
    fn timer(&self) -> Option<Self::Handle>;
    fn clear_timeout(&self, handle: Option<Self::Handle>) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        period: u64,
        callback: Rc<dyn Fn() -> MethodObserver<Self::Value, Self::Error>>,
    ) -> Result<Self::Handle, Self::Error>;
    fn assign_timer(&self, handle: Self::Handle) -> Result<(), Self::Error>;
    fn fetch_balance(&self) -> Result<Rc<Self::Operation>, Self::Error>;
    fn disposed(&self) -> bool;
    fn update_balance(&self, balance: Self::Value) -> Result<(), Self::Error>;
}
/// MexcPrivateWebsocket.connectAndSubscribe.
pub fn connect<H: PrivateConnectHost>(host: &H) -> Result<(), H::Error> {
    if host.disposed() || host.truthy(&host.public_only()?) {
        return Ok(());
    }
    if !host.truthy(&host.key()?) {
        return Ok(());
    }
    let network = if host.truthy(&host.testnet()?) {
        "testnet"
    } else {
        "livenet"
    };
    let create = host.capture_create()?;
    let socket = host.invoke_create(
        create,
        DialRequest {
            url: host.url(network)?,
            protocols: host.undefined(),
            headers: vec![("User-Agent", units("Tealstreet"))],
        },
    )?;
    host.assign_socket(socket)?;
    for event in ["open", "message", "close"] {
        let socket = host.socket()?;
        let listener = host.listener(event)?;
        host.add_listener(socket, event, listener)?;
    }
    Ok(())
}
/// MexcPrivateWebsocket.onOpen. Both returned child Promises are intentionally unobserved.
pub fn on_open<H: OpenHost>(host: &H) -> Result<(), H::Error> {
    if host.disposed() {
        return Ok(());
    }
    host.start_heartbeat()?;
    host.send_login()?;
    host.private_snapshot(10)?;
    host.reconnected()
}
/// MexcPrivateWebsocket#sendLogin. Prefix starts now; continuation survives caller drop.
pub fn send_login<H: LoginHost>(host: Rc<H>) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let key = h.trim(&h.key()?)?;
        let req_time = h.text(h.abstract_string(&h.clock()?)?);
        let signature = observe(h.create_signature(key.clone(), req_time.clone())).await?;
        if !h.truthy(&signature) {
            return Ok(h.undefined());
        }
        let send = h.capture_send()?;
        let frame = LoginFrame {
            method: "login",
            api_key: key,
            req_time,
            signature,
        };
        let data = h.stringify_login(frame)?;
        h.send_login_frame(send, data, true)?;
        Ok(h.undefined())
    })
}
/// MexcPrivateWebsocket#createLoginSignature. Preserve local branch and remote read order.
pub fn create_login_signature<H: LoginHost>(
    host: Rc<H>,
    api_key: H::Value,
    req_time: H::Value,
) -> MethodObserver<H::Value, H::Error> {
    let h = host.clone();
    start(host, async move {
        let remote = h.remote_signer()?;
        if !h.truthy(&remote) {
            let secret = h.secret()?;
            let secret = if h.truthy(&secret) {
                secret
            } else {
                h.text(vec![])
            };
            return h.local_sign(secret, api_key, req_time);
        }
        let sign = h.capture_sign(&remote)?;
        let id = h.nanoid()?;
        let account_name = h.field(&remote, "accountName")?;
        let mut to_sign = h.abstract_string(&api_key)?;
        to_sign.extend(h.abstract_string(&req_time)?);
        let request = SigningRequest {
            id,
            request_type: "ws-auth",
            exchange_name: "mexc",
            account_name,
            to_sign,
            output_format: "hex",
            req_time,
        };
        let result = observe(h.sign(sign, request)).await?;
        let error = h.field(&result, "error")?;
        if h.truthy(&error) || !h.truthy(&h.field(&result, "wsAuthSignature")?) {
            let emit = h.capture_emit_error()?;
            let mut text = units("Remote WS signing unavailable: ");
            let error = h.field(&result, "error")?;
            let error = if h.nullish(&error) {
                h.text(units("no signature returned"))
            } else {
                error
            };
            text.extend(h.abstract_string(&error)?);
            h.emit_error(emit, text)?;
            return Ok(h.undefined());
        }
        h.field(&result, "wsAuthSignature")
    })
}
/// MexcPrivateWebsocket#sendWithDiagnostics: emit only after send succeeds.
pub fn send_diagnostics<H: DiagnosticHost>(
    host: Rc<H>,
    data: &[u16],
    options: H::Value,
) -> Result<(), H::Error> {
    host.send(data)?;
    let h = host.clone();
    host.emit_send(
        data,
        DiagnosticOptions {
            format: Rc::new(move |message| {
                let redact = if h.nullish(&options) {
                    h.undefined()
                } else {
                    h.field(&options, "redactAuth")?
                };
                let message = if h.truthy(&redact) {
                    h.redact(message)?
                } else {
                    message
                };
                let mut text = units("[mexc private] ");
                text.extend(h.abstract_string(&message)?);
                Ok(text)
            }),
        },
    )
}
/// MexcPrivateWebsocket#emitReceiveDiagnostics: global String, original data identity.
pub fn receive_diagnostics<H: DiagnosticHost>(host: Rc<H>, data: H::Value) -> Result<(), H::Error> {
    let h = host.clone();
    host.emit_receive(
        data,
        DiagnosticOptions {
            format: Rc::new(move |message| {
                let mut text = units("[mexc private] ");
                text.extend(h.global_string(&message)?);
                Ok(text)
            }),
        },
    )
}
/// MexcPrivateWebsocket#redactAuthFrame. Catch parse, property, spread and stringify failures.
pub fn redact_auth_frame<H: SessionRuntime>(host: &H, data: &H::Value) -> H::Value {
    let work = || -> Result<H::Value, H::Error> {
        let payload = host.json_parse(data)?;
        let method = if host.nullish(&payload) {
            host.undefined()
        } else {
            host.field(&payload, "method")?
        };
        if host.is_text(&method, "login") && host.truthy(&host.field(&payload, "param")?) {
            let param = host.spread(&host.field(&payload, "param")?)?;
            host.define_field(&param, "apiKey", host.text(units("[redacted]")))?;
            host.define_field(&param, "signature", host.text(units("[redacted]")))?;
            host.set_field(&payload, "param", param)?;
        }
        Ok(host.text(host.stringify(&payload)?))
    };
    work().unwrap_or_else(|_| host.text(units("[mexc private login redacted]")))
}
/// MexcPrivateWebsocket#throttledFetchBalance: clear/register/assign; each async tick catches all.
pub fn throttled_fetch_balance<H: BalanceHost>(host: Rc<H>) -> Result<(), H::Error> {
    host.clear_timeout(host.timer())?;
    let h = host.clone();
    let callback = Rc::new(move || {
        let h = h.clone();
        let run = h.clone();
        start(h, async move {
            let work = async {
                let balance = observe(run.fetch_balance()).await?;
                if !run.disposed() {
                    run.update_balance(balance)?;
                }
                Ok::<(), H::Error>(())
            };
            let _ = work.await;
            Ok(run.undefined())
        })
    });
    let handle = host.set_timeout(1000, callback)?;
    host.assign_timer(handle)
}
