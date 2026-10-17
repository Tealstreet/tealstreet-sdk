//! napi binding for native-host exchange accounts. Every account lives on one
//! dedicated thread running a current-thread tokio `LocalSet`, as JS runs one
//! thread. Commands arrive over a channel and settle napi Promises; events, Store
//! slices and signer/token requests reach JS through ThreadsafeFunctions.
// napi registers exports at load time; the unit-test build has no registration.
#![cfg_attr(test, allow(dead_code))]
use napi::{
    Status,
    bindgen_prelude::{Promise, Unknown},
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
    rc::{Rc, Weak},
    sync::OnceLock,
};
use tokio::sync::{mpsc, oneshot};

type Request = ThreadsafeFunction<String, Promise<String>, String, Status, false>;
type Notify = ThreadsafeFunction<String, Unknown<'static>, String, Status, false>;
type Reply<T> = oneshot::Sender<Result<T, String>>;

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
                        let _ = reply.send(Err(error_json(&format!("no native account {id}"))));
                        continue;
                    };
                    let account = open.account.clone();
                    tokio::task::spawn_local(async move {
                        let result = match serde_json::from_str::<Vec<Value>>(&args_json) {
                            Ok(args) => native_account_call(account, method, args).await,
                            Err(error) => Err(json!({"name":"Error","message":error.to_string()})),
                        };
                        let _ = reply.send(
                            result
                                .map(|value| value.to_string())
                                .map_err(|error| error.to_string()),
                        );
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
                        let _ = reply.send(Err(error_json(&format!("no native account {id}"))));
                        continue;
                    };
                    (open.unwatch)();
                    // Acknowledgement means disposal finished; keep its errors visible
                    // to the JS host rather than leaving live work behind a resolved close.
                    tokio::task::spawn_local(async move {
                        let result = native_account_call(open.account, "dispose".into(), vec![])
                            .await
                            .map(|_| ())
                            .map_err(|error| error.to_string());
                        let _ = reply.send(result);
                    });
                }
            }
        }
    });
}

fn error_json(message: &str) -> String {
    json!({"name":"Error","message":message}).to_string()
}

/// A JS callback returning Promise<string>, as a JSON future on the host thread.
fn request(callback: Weak<Request>, payload: String) -> JsonFuture<String> {
    Box::pin(async move {
        let failed = |message: String| json!({"name":"Error","message":message});
        let callback = callback
            .upgrade()
            .ok_or_else(|| failed("native account callback is closed".into()))?;
        let pending = callback
            .call_async_catch(payload)
            .await
            .map_err(|e| failed(e.to_string()))?;
        pending.await.map_err(|e| failed(e.to_string()))
    })
}

fn open(
    init: NativeAccountInit,
    sign: Option<Request>,
    proxy_token: Option<Request>,
    on_event: Notify,
    on_store: Notify,
    hyperliquid_limiter: Rc<safe_cex_registry::native_host::HyperliquidRateLimiter>,
) -> Result<Open, String> {
    let options: Value =
        serde_json::from_str(&init.options_json).map_err(|e| error_json(&e.to_string()))?;
    let sign = sign.map(Rc::new);
    let proxy_token = proxy_token.map(Rc::new);
    let sign_source = sign.as_ref().map(Rc::downgrade).map(|callback| {
        Rc::new(move |payload: Value| {
            let pending = request(callback.clone(), payload.to_string());
            Box::pin(async move {
                let text = pending.await?;
                serde_json::from_str(&text)
                    .map_err(|e| json!({"name":"Error","message":format!("signer returned invalid JSON: {e}")}))
            }) as JsonFuture<Value>
        }) as Rc<dyn Fn(Value) -> JsonFuture<Value>>
    });
    let proxy_source = proxy_token.as_ref().map(Rc::downgrade).map(|callback| {
        Rc::new(move || request(callback.clone(), String::new()))
            as Rc<dyn Fn() -> JsonFuture<String>>
    });
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter,
        exchange: init.exchange,
        options,
        account_name: init.account_name,
        sign: sign_source,
        proxy_token: proxy_source,
    })
    .map_err(|error| error.to_string())?;
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
    })
}

fn rejected(error: String) -> napi::Error {
    napi::Error::new(Status::GenericFailure, error)
}
async fn send<T>(command: impl FnOnce(Reply<T>) -> Command) -> napi::Result<T> {
    let (reply, settled) = oneshot::channel();
    commands()
        .send(command(reply))
        .map_err(|_| rejected(error_json("native host thread stopped")))?;
    settled
        .await
        .map_err(|_| rejected(error_json("native host dropped the request")))?
        .map_err(rejected)
}

/// Opens an account. `sign(requestJson)` and `proxyToken()` resolve strings; `onEvent`
/// receives `{event, data}` JSON and `onStore` the private Store slices as JSON.
/// Rejections carry `{name, message, code?}` JSON as the error message.
#[napi]
pub async fn native_account_open(
    init: NativeAccountInit,
    on_event: Notify,
    on_store: Notify,
    sign: Option<Request>,
    proxy_token: Option<Request>,
) -> napi::Result<u32> {
    send(|reply| Command::Open {
        init,
        sign,
        proxy_token,
        on_event,
        on_store,
        reply,
    })
    .await
}

/// Calls an Exchange method by its TypeScript name with JSON-array arguments.
#[napi]
pub async fn native_account_call_method(
    id: u32,
    method: String,
    args_json: String,
) -> napi::Result<String> {
    send(|reply| Command::Call {
        id,
        method,
        args_json,
        reply,
    })
    .await
}

/// ts: RemoteSigner.onAvailabilityChange, pushed from the host's signer session.
#[napi(js_name = "nativeAccountSignerAvailable")]
pub fn signer_available(id: u32, available: bool) -> napi::Result<()> {
    commands()
        .send(Command::SignerAvailable { id, available })
        .map_err(|_| rejected(error_json("native host thread stopped")))
}

/// Stops forwarding and resolves after disposal; disposal errors reject.
#[napi]
pub async fn native_account_close(id: u32) -> napi::Result<()> {
    send(|reply| Command::Close { id, reply }).await
}
