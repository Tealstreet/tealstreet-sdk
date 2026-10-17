//! napi binding for native-host exchange accounts. Every account lives on one
//! dedicated thread running a current-thread tokio `LocalSet`, as JS runs one
//! thread. Commands arrive over a channel and settle napi Promises; events, Store
//! slices and signer/token requests reach JS through ThreadsafeFunctions.
// napi registers exports at load time; the unit-test build has no registration.
#![cfg_attr(test, allow(dead_code))]
use napi::{
    Env, Status,
    bindgen_prelude::{JsObjectValue, Promise, PromiseRaw, ToNapiValue, Unknown},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;
use safe_cex_registry::native_account::{
    JsonFuture, NativeAccount, NativeAccountSpec, native_account_call, native_account_watch,
    open_native_account,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    future::{Future, poll_fn},
    pin::pin,
    rc::{Rc, Weak},
    sync::OnceLock,
    task::Poll,
};
use tokio::sync::{mpsc, oneshot, watch};

type Request = ThreadsafeFunction<String, Promise<String>, String, Status, false>;
type Notify = ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type Reply<T> = oneshot::Sender<Result<T, Value>>;

/// A JSON options object plus the account's name for remote signing.
#[napi(object)]
pub struct NativeAccountInit {
    pub exchange: String,
    pub options_json: String,
    pub account_name: Option<String>,
}

enum Command {
    Open {
        init: NativeAccountInit,
        sign: Option<Request>,
        proxy_token: Option<Request>,
        on_event: Notify,
        on_store: Notify,
        reply: Reply<u32>,
    },
    Call {
        id: u32,
        method: String,
        args_json: String,
        reply: Reply<String>,
    },
    SignerAvailable {
        id: u32,
        available: bool,
    },
    Close {
        id: u32,
        reply: Reply<()>,
    },
}

struct Open {
    account: Rc<NativeAccount>,
    unwatch: Box<dyn Fn()>,
    // Venue runtimes can outlive disposal; only an open account owns JS callbacks.
    _sign: Option<Rc<Request>>,
    _proxy_token: Option<Rc<Request>>,
    closed: watch::Sender<bool>,
}

fn commands() -> &'static mpsc::UnboundedSender<Command> {
    static HOST: OnceLock<mpsc::UnboundedSender<Command>> = OnceLock::new();
    HOST.get_or_init(|| {
        let (sender, receiver) = mpsc::unbounded_channel();
        std::thread::Builder::new()
            .name("safe-cex-native-host".into())
            .spawn(move || run_host(receiver))
            .expect("native host thread starts");
        sender
    })
}

fn run_host(mut receiver: mpsc::UnboundedReceiver<Command>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("native host runtime builds");
    let local = tokio::task::LocalSet::new();
    local.block_on(&runtime, async move {
        let hyperliquid_limiter = safe_cex_registry::native_host::hyperliquid_rate_limiter();
        let mut accounts: HashMap<u32, Open> = HashMap::new();
        let mut next = 1u32;
        while let Some(command) = receiver.recv().await {
            match command {
                Command::Open {
                    init,
                    sign,
                    proxy_token,
                    on_event,
                    on_store,
                    reply,
                } => {
                    let opened = open(
                        init,
                        sign,
                        proxy_token,
                        on_event,
                        on_store,
                        hyperliquid_limiter.clone(),
                    );
                    let _ = reply.send(opened.map(|open| {
                        let id = next;
                        next += 1;
                        accounts.insert(id, open);
                        id
                    }));
                }
                Command::Call {
                    id,
                    method,
                    args_json,
                    reply,
                } => {
                    let Some(open) = accounts.get(&id) else {
                        let _ = reply.send(Err(host_error(&format!("no native account {id}"))));
                        continue;
                    };
                    let account = open.account.clone();
                    tokio::task::spawn_local(async move {
                        let result = match serde_json::from_str::<Vec<Value>>(&args_json) {
                            Ok(args) => native_account_call(account, method, args).await,
                            Err(error) => Err(json!({"name":"Error","message":error.to_string()})),
                        };
                        let _ = reply.send(result.map(|value| value.to_string()));
                    });
                }
                Command::SignerAvailable { id, available } => {
                    if let Some(open) = accounts.get(&id) {
                        safe_cex_registry::native_account::native_account_signer_available(
                            &open.account,
                            available,
                        );
                    }
                }
                Command::Close { id, reply } => {
                    let Some(open) = accounts.remove(&id) else {
                        let _ = reply.send(Err(host_error(&format!("no native account {id}"))));
                        continue;
                    };
                    (open.unwatch)();
                    open.closed.send_replace(true);
                    // Acknowledgement means disposal finished; keep its errors visible
                    // to the JS host rather than leaving live work behind a resolved close.
                    tokio::task::spawn_local(async move {
                        let result = native_account_call(open.account, "dispose".into(), vec![])
                            .await
                            .map(|_| ());
                        let _ = reply.send(result);
                    });
                }
            }
        }
    });
}

fn host_error(message: &str) -> Value {
    json!({"name":"Error","message":message})
}

/// A JS callback returning Promise<string>, as a JSON future on the host thread.
fn request(
    callback: Weak<Request>,
    mut closed: watch::Receiver<bool>,
    payload: String,
) -> JsonFuture<String> {
    Box::pin(async move {
        let closed_error = || json!({"name":"Error","message":"native account callback is closed"});
        if *closed.borrow() {
            return Err(closed_error());
        }
        let mut operation = pin!(async move {
            let failed = |message: String| json!({"name":"Error","message":message});
            let callback = callback.upgrade().ok_or_else(closed_error)?;
            let pending = callback
                .call_async_catch(payload)
                .await
                .map_err(|e| failed(e.to_string()))?;
            drop(callback);
            pending.await.map_err(|e| failed(e.to_string()))
        });
        let mut closing = pin!(closed.changed());
        poll_fn(|context| {
            if closing.as_mut().poll(context).is_ready() {
                Poll::Ready(Err(closed_error()))
            } else {
                operation.as_mut().poll(context)
            }
        })
        .await
    })
}

fn open(
    init: NativeAccountInit,
    sign: Option<Request>,
    proxy_token: Option<Request>,
    on_event: Notify,
    on_store: Notify,
    hyperliquid_limiter: Rc<safe_cex_registry::native_host::HyperliquidRateLimiter>,
) -> Result<Open, Value> {
    let options: Value =
        serde_json::from_str(&init.options_json).map_err(|e| host_error(&e.to_string()))?;
    let sign = sign.map(Rc::new);
    let proxy_token = proxy_token.map(Rc::new);
    let (closed, cancellation) = watch::channel(false);
    let sign_source = sign.as_ref().map(Rc::downgrade).map(|callback| {
        let cancellation = cancellation.clone();
        Rc::new(move |payload: Value| {
            let pending = request(callback.clone(), cancellation.clone(), payload.to_string());
            Box::pin(async move {
                let text = pending.await?;
                serde_json::from_str(&text)
                    .map_err(|e| json!({"name":"Error","message":format!("signer returned invalid JSON: {e}")}))
            }) as JsonFuture<Value>
        }) as Rc<dyn Fn(Value) -> JsonFuture<Value>>
    });
    let proxy_source = proxy_token.as_ref().map(Rc::downgrade).map(|callback| {
        let cancellation = cancellation.clone();
        Rc::new(move || request(callback.clone(), cancellation.clone(), String::new()))
            as Rc<dyn Fn() -> JsonFuture<String>>
    });
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter,
        exchange: init.exchange,
        options,
        account_name: init.account_name,
        sign: sign_source,
        proxy_token: proxy_source,
    })?;
    let unwatch = native_account_watch(
        &account,
        Rc::new(move |event: &str, data: &Value| {
            on_event.call(
                json!({"event":event,"data":data}).to_string(),
                ThreadsafeFunctionCallMode::NonBlocking,
            );
        }),
        Rc::new(move |slices: Value| {
            on_store.call(slices.to_string(), ThreadsafeFunctionCallMode::NonBlocking);
        }),
    );
    Ok(Open {
        account,
        unwatch,
        _sign: sign,
        _proxy_token: proxy_token,
        closed,
    })
}

fn rejected(env: &Env, error: Value) -> napi::Result<napi::Error> {
    let field = |name| {
        error
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(|| napi::Error::from_reason(format!("native error has no string {name}")))
    };
    let mut object = env.create_error(napi::Error::from_reason(field("message")?))?;
    object.set_named_property("name", field("name")?)?;
    if let Some(code) = error.get("code") {
        object.set_named_property("code", code.clone())?;
    } else if !object.delete_named_property("code")? {
        return Err(napi::Error::from_reason(
            "cannot remove synthetic native error code",
        ));
    }
    Ok(napi::Error::from_unknown_without_coercion(
        object.into_unknown(env)?,
    ))
}
async fn send<T>(command: impl FnOnce(Reply<T>) -> Command) -> Result<T, Value> {
    let (reply, settled) = oneshot::channel();
    commands()
        .send(command(reply))
        .map_err(|_| host_error("native host thread stopped"))?;
    settled
        .await
        .map_err(|_| host_error("native host dropped the request"))?
}
fn settle<'env, T: Send + ToNapiValue + 'static>(
    env: &'env Env,
    command: impl FnOnce(Reply<T>) -> Command + Send + 'static,
) -> napi::Result<PromiseRaw<'env, T>> {
    env.spawn_future_with_callback(async move { Ok(send(command).await) }, |env, result| {
        result.or_else(|error| Err(rejected(env, error)?))
    })
}

/// Opens an account. `sign(requestJson)` and `proxyToken()` resolve strings; `onEvent`
/// receives `{event, data}` JSON and `onStore` the private Store slices as JSON.
/// Rejections preserve the source error’s name, message and optional code.
#[napi]
pub fn native_account_open<'env>(
    env: &'env Env,
    init: NativeAccountInit,
    on_event: Notify,
    on_store: Notify,
    sign: Option<Request>,
    proxy_token: Option<Request>,
) -> napi::Result<PromiseRaw<'env, u32>> {
    settle(env, move |reply| Command::Open {
        init,
        sign,
        proxy_token,
        on_event,
        on_store,
        reply,
    })
}

/// Calls an Exchange method by its TypeScript name with JSON-array arguments.
#[napi]
pub fn native_account_call_method<'env>(
    env: &'env Env,
    id: u32,
    method: String,
    args_json: String,
) -> napi::Result<PromiseRaw<'env, String>> {
    settle(env, move |reply| Command::Call {
        id,
        method,
        args_json,
        reply,
    })
}

/// ts: RemoteSigner.onAvailabilityChange, pushed from the host's signer session.
#[napi(js_name = "nativeAccountSignerAvailable")]
pub fn signer_available(env: &Env, id: u32, available: bool) -> napi::Result<()> {
    commands()
        .send(Command::SignerAvailable { id, available })
        .or_else(|_| Err(rejected(env, host_error("native host thread stopped"))?))
}

/// Stops forwarding and resolves after disposal; disposal errors reject.
#[napi]
pub fn native_account_close<'env>(env: &'env Env, id: u32) -> napi::Result<PromiseRaw<'env, ()>> {
    settle(env, move |reply| Command::Close { id, reply })
}
