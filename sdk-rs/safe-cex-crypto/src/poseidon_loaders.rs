//! Source loader state machines. Host bindings perform Go/WASM and global-export
//! operations; no cryptographic algorithm or readiness shortcut is substituted.
use crate::poseidon_types::{SchnorrKeyPair, WasmError};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
pub type LoaderFuture<T> = Pin<Box<dyn Future<Output = Result<T, LoaderError>>>>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoaderError {
    pub name: String,
    pub message: String,
}
impl LoaderError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            name: "Error".into(),
            message: message.into(),
        }
    }
    pub fn js_string(&self) -> String {
        match (self.name.is_empty(), self.message.is_empty()) {
            (true, _) => self.message.clone(),
            (_, true) => self.name.clone(),
            _ => format!("{}: {}", self.name, self.message),
        }
    }
    fn wrapped(self) -> Self {
        Self::new(format!("Failed to initialize WASM: {}", self.js_string()))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoaderKind {
    Browser,
    Node,
    Auto,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Receiver {
    Global,
    Unbound,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CryptoValue {
    Text(String),
    Bool(bool),
    Keys(SchnorrKeyPair),
    Error(WasmError),
}
pub trait GlobalFunction {
    fn call(&self, receiver: Receiver, args: &[String]) -> Result<CryptoValue, LoaderError>;
}
#[derive(Clone)]
pub enum GlobalBinding {
    Undefined,
    Null,
    NonCallable,
    Function(Rc<dyn GlobalFunction>),
}
pub struct FetchResponse {
    pub ok: bool,
    pub status: f64,
    pub status_text: String,
    pub handle: Rc<dyn std::any::Any>,
}
pub trait WasmHost {
    fn import_wasm_exec(&self) -> LoaderFuture<()>;
    fn node_version_truthy(&self) -> bool;
    fn fetch_defined(&self) -> bool;
    fn import_fs(&self) -> LoaderFuture<()>;
    fn import_path(&self) -> LoaderFuture<()>;
    fn module_directory(&self) -> String;
    fn path_join(&self, parts: &[String]) -> Result<String, LoaderError>;
    fn read_file_sync(&self, path: &str) -> Result<Vec<u8>, LoaderError>;
    fn origin(&self) -> Option<String>;
    fn fetch(&self, url: String) -> LoaderFuture<FetchResponse>;
    fn array_buffer(&self, response: FetchResponse) -> LoaderFuture<Vec<u8>>;
    fn new_go(&self) -> Result<Rc<dyn std::any::Any>, LoaderError>;
    fn instantiate(
        &self,
        bytes: Vec<u8>,
        go: Rc<dyn std::any::Any>,
    ) -> LoaderFuture<Rc<dyn std::any::Any>>;
    /// Launch Go.run without awaiting its returned Promise.
    fn run(
        &self,
        go: Rc<dyn std::any::Any>,
        instance: Rc<dyn std::any::Any>,
    ) -> Result<(), LoaderError>;
    fn sleep(&self, milliseconds: u64) -> LoaderFuture<()>;
    fn global(&self, name: &str) -> Result<GlobalBinding, LoaderError>;
    /// Retain and poll an initialization task even when every caller is dropped.
    fn spawn_init(&self, future: LoaderFuture<()>);
}
pub struct WasmLoader {
    kind: LoaderKind,
    host: Rc<dyn WasmHost>,
    ready: Cell<bool>,
    init: RefCell<Option<Rc<InitTask>>>,
}
impl WasmLoader {
    pub fn new(kind: LoaderKind, host: Rc<dyn WasmHost>) -> Rc<Self> {
        Rc::new(Self {
            kind,
            host,
            ready: Cell::new(false),
            init: RefCell::new(None),
        })
    }
    pub fn is_wasm_ready(&self) -> bool {
        self.ready.get()
    }
    pub fn init_marker(&self) -> Option<Rc<InitTask>> {
        self.init.borrow().clone()
    }
    // ts browser/node/auto initWasm: shared body with explicit acquisition branch.
    pub fn init_wasm(self: &Rc<Self>) -> LoaderFuture<()> {
        if self.ready.get() {
            return Box::pin(async { Ok(()) });
        }
        let existing = self.init.borrow().clone();
        if let Some(task) = existing {
            return task.awaiter();
        }
        let imported = self.host.import_wasm_exec();
        let owner = self.clone();
        let future = Box::pin(async move {
            let result = async {
                imported.await?;
                let node = match owner.kind {
                    LoaderKind::Node => true,
                    LoaderKind::Browser => false,
                    LoaderKind::Auto => owner.host.node_version_truthy(),
                };
                let bytes = if node {
                    owner.host.import_fs().await?;
                    owner.host.import_path().await?;
                    let path = owner.host.path_join(&[
                        owner.host.module_directory(),
                        "wasm".into(),
                        "poseidon_crypto.wasm".into(),
                    ])?;
                    owner.host.read_file_sync(&path)?
                } else {
                    if owner.kind == LoaderKind::Auto && !owner.host.fetch_defined() {
                        return Err(LoaderError::new("Unsupported environment for WASM loading"));
                    }
                    let origin = owner
                        .host
                        .origin()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_default();
                    let response = owner
                        .host
                        .fetch(format!("{origin}/wasm/poseidon_crypto.wasm"))
                        .await?;
                    if !response.ok {
                        return Err(LoaderError::new(format!(
                            "Failed to fetch WASM: {} {}",
                            safe_cex_types::js_coercion::js_number_string(response.status),
                            response.status_text
                        )));
                    }
                    owner.host.array_buffer(response).await?
                };
                let go = owner.host.new_go()?;
                let instance = owner.host.instantiate(bytes, go.clone()).await?;
                owner.host.run(go, instance)?;
                owner.host.sleep(100).await?;
                owner.ready.set(true);
                Ok(())
            }
            .await;
            if let Err(error) = result {
                *owner.init.borrow_mut() = None;
                return Err(error.wrapped());
            }
            Ok(())
        });
        let task = InitTask::new(future);
        *self.init.borrow_mut() = Some(task.clone());
        let driven = task.clone();
        self.host
            .spawn_init(Box::pin(poll_fn(move |cx| match driven.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => Poll::Ready(Ok(())),
            })));
        task.awaiter()
    }
    pub fn get_crypto_api(self: &Rc<Self>) -> Result<Rc<PoseidonCrypto>, LoaderError> {
        if !self.ready.get() {
            return Err(not_initialized());
        }
        Ok(Rc::new(PoseidonCrypto {
            loader: self.clone(),
        }))
    }
}
pub struct PoseidonCrypto {
    loader: Rc<WasmLoader>,
}
impl PoseidonCrypto {
    pub fn is_ready(&self) -> bool {
        self.loader.ready.get()
    }
    fn invoke(&self, name: &str, args: &[String]) -> Result<CryptoValue, LoaderError> {
        match self.loader.host.global(name)? {
            GlobalBinding::Function(f) => f.call(Receiver::Global, args),
            _ => Err(LoaderError {
                name: "TypeError".into(),
                message: format!("globalThis.{name} is not a function"),
            }),
        }
    }
    pub fn poseidon2_hash(&self, input: String) -> Result<CryptoValue, LoaderError> {
        self.invoke("poseidon2Hash", &[input])
    }
    pub fn poseidon2_hash_fp5(&self, input: String) -> Result<CryptoValue, LoaderError> {
        let primary = self.loader.host.global("poseidon2HashFp5")?;
        let selected = match primary {
            GlobalBinding::Undefined | GlobalBinding::Null => {
                self.loader.host.global("poseidon2Hash")?
            }
            other => other,
        };
        match selected {
            GlobalBinding::Function(f) => f.call(Receiver::Unbound, &[input]),
            _ => Ok(CryptoValue::Error(WasmError {
                error: "poseidon2HashFp5 function not available".into(),
            })),
        }
    }
    pub fn poseidon2_hash_two_to_one(
        &self,
        a: String,
        b: String,
    ) -> Result<CryptoValue, LoaderError> {
        self.invoke("poseidon2HashTwoToOne", &[a, b])
    }
    pub fn schnorr_generate_keys(&self) -> Result<CryptoValue, LoaderError> {
        self.invoke("schnorrGenerateKeys", &[])
    }
    pub fn schnorr_pk_from_sk(&self, key: String) -> Result<CryptoValue, LoaderError> {
        self.invoke("schnorrPkFromSk", &[key])
    }
    pub fn schnorr_sign(&self, message: String, key: String) -> Result<CryptoValue, LoaderError> {
        self.invoke("schnorrSign", &[message, key])
    }
    pub fn schnorr_verify(
        &self,
        message: String,
        key: String,
        signature: String,
    ) -> Result<CryptoValue, LoaderError> {
        self.invoke("schnorrVerify", &[message, key, signature])
    }
}
fn not_initialized() -> LoaderError {
    LoaderError::new("WASM not initialized. Call initWasm() first.")
}
pub struct InitTask {
    future: RefCell<Option<LoaderFuture<()>>>,
    result: RefCell<Option<Result<(), LoaderError>>>,
    waiters: RefCell<Vec<Waker>>,
}
impl InitTask {
    fn new(future: LoaderFuture<()>) -> Rc<Self> {
        Rc::new(Self {
            future: RefCell::new(Some(future)),
            result: RefCell::new(None),
            waiters: RefCell::new(vec![]),
        })
    }
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<(), LoaderError>> {
        if let Some(r) = self.result.borrow().clone() {
            return Poll::Ready(r);
        }
        {
            let mut waiters = self.waiters.borrow_mut();
            if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
                waiters.push(cx.waker().clone());
            }
        }
        let pending = self.future.borrow_mut().take();
        if let Some(mut future) = pending {
            match future.as_mut().poll(cx) {
                Poll::Pending => *self.future.borrow_mut() = Some(future),
                Poll::Ready(r) => {
                    *self.result.borrow_mut() = Some(r.clone());
                    for w in std::mem::take(&mut *self.waiters.borrow_mut()) {
                        w.wake();
                    }
                    return Poll::Ready(r);
                }
            }
        }
        Poll::Pending
    }
    fn awaiter(self: &Rc<Self>) -> LoaderFuture<()> {
        let this = self.clone();
        Box::pin(poll_fn(move |cx| this.poll(cx)))
    }
}
pub trait IndexHost {
    fn window_defined(&self) -> Result<bool, LoaderError>;
    /// None means only JavaScript undefined. Null/noncallable values remain
    /// Some opaque handles and the instanceof primitive may throw.
    fn worker_constructor(&self) -> Result<Option<Rc<dyn std::any::Any>>, LoaderError>;
    fn self_defined(&self) -> Result<bool, LoaderError>;
    fn self_instanceof_worker(
        &self,
        constructor: Rc<dyn std::any::Any>,
    ) -> Result<bool, LoaderError>;
    fn import_loader(&self, kind: LoaderKind) -> LoaderFuture<Rc<WasmLoader>>;
    fn spawn_init(&self, future: LoaderFuture<()>);
}
// ts: isBrowserLikeRuntime, preserve definedness and short-circuit order.
pub fn is_browser_like_runtime(host: &dyn IndexHost) -> Result<bool, LoaderError> {
    if host.window_defined()? {
        return Ok(true);
    }
    let Some(constructor) = host.worker_constructor()? else {
        return Ok(false);
    };
    if !host.self_defined()? {
        return Ok(false);
    }
    host.self_instanceof_worker(constructor)
}
pub struct PoseidonIndex {
    host: Rc<dyn IndexHost>,
    runtime: RefCell<Option<Rc<WasmLoader>>>,
    init: RefCell<Option<Rc<InitTask>>>,
}
impl PoseidonIndex {
    pub fn new(host: Rc<dyn IndexHost>) -> Rc<Self> {
        Rc::new(Self {
            host,
            runtime: RefCell::new(None),
            init: RefCell::new(None),
        })
    }
    pub fn init_marker(&self) -> Option<Rc<InitTask>> {
        self.init.borrow().clone()
    }
    pub fn init_wasm(self: &Rc<Self>) -> LoaderFuture<()> {
        if self.runtime.borrow().is_some() {
            return Box::pin(async { Ok(()) });
        }
        let existing = self.init.borrow().clone();
        if let Some(task) = existing {
            return task.awaiter();
        }
        let imported = match is_browser_like_runtime(self.host.as_ref()) {
            Ok(browser) => Ok(self.host.import_loader(if browser {
                LoaderKind::Browser
            } else {
                LoaderKind::Node
            })),
            Err(e) => Err(e),
        };
        let pre_await_failure = imported.is_err();
        let owner = self.clone();
        let task = InitTask::new(Box::pin(async move {
            let result = async {
                let module = imported?.await?;
                *owner.runtime.borrow_mut() = Some(module.clone());
                module.init_wasm().await
            }
            .await;
            if let Err(e) = result {
                // Source catch runs before outer assignment when detection throws;
                // that outer assignment restores the rejected marker afterward.
                if !pre_await_failure {
                    *owner.init.borrow_mut() = None;
                }
                return Err(e.wrapped());
            }
            Ok(())
        }));
        *self.init.borrow_mut() = Some(task.clone());
        let driven = task.clone();
        self.host
            .spawn_init(Box::pin(poll_fn(move |cx| match driven.poll(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => Poll::Ready(Ok(())),
            })));
        task.awaiter()
    }
    pub fn is_wasm_ready(&self) -> bool {
        self.runtime
            .borrow()
            .as_ref()
            .is_some_and(|m| m.is_wasm_ready())
    }
    pub fn get_crypto_api(&self) -> Result<Rc<PoseidonCrypto>, LoaderError> {
        self.runtime
            .borrow()
            .clone()
            .ok_or_else(not_initialized)?
            .get_crypto_api()
    }
}
pub mod stub {
    use super::{LoaderError, LoaderFuture, PoseidonCrypto};
    use std::rc::Rc;
    pub fn init_wasm() -> Result<LoaderFuture<()>, LoaderError> {
        Err(LoaderError::new(
            "WASM not loaded. This is a stub function.",
        ))
    }
    pub fn is_wasm_ready() -> bool {
        false
    }
    pub fn get_crypto_api() -> Result<Rc<PoseidonCrypto>, LoaderError> {
        Err(LoaderError::new(
            "WASM not loaded. This is a stub function.",
        ))
    }
}

/// Explicit module context for the corresponding TypeScript loader file.
pub struct BrowserLoader {
    core: Rc<WasmLoader>,
}
impl BrowserLoader {
    pub fn new(host: Rc<dyn WasmHost>) -> Rc<Self> {
        Rc::new(Self {
            core: WasmLoader::new(LoaderKind::Browser, host),
        })
    }
    pub fn init_wasm(&self) -> LoaderFuture<()> {
        self.core.init_wasm()
    }
    pub fn is_wasm_ready(&self) -> bool {
        self.core.ready.get()
    }
    pub fn get_crypto_api(&self) -> Result<Rc<PoseidonCrypto>, LoaderError> {
        self.core.get_crypto_api()
    }
}

/// Explicit module context for the corresponding TypeScript loader file.
pub struct NodeLoader {
    core: Rc<WasmLoader>,
}
impl NodeLoader {
    pub fn new(host: Rc<dyn WasmHost>) -> Rc<Self> {
        Rc::new(Self {
            core: WasmLoader::new(LoaderKind::Node, host),
        })
    }
    pub fn init_wasm(&self) -> LoaderFuture<()> {
        self.core.init_wasm()
    }
    pub fn is_wasm_ready(&self) -> bool {
        self.core.ready.get()
    }
    pub fn get_crypto_api(&self) -> Result<Rc<PoseidonCrypto>, LoaderError> {
        self.core.get_crypto_api()
    }
}

/// Explicit module context for the corresponding TypeScript loader file.
pub struct AutoLoader {
    core: Rc<WasmLoader>,
}
impl AutoLoader {
    pub fn new(host: Rc<dyn WasmHost>) -> Rc<Self> {
        Rc::new(Self {
            core: WasmLoader::new(LoaderKind::Auto, host),
        })
    }
    pub fn init_wasm(&self) -> LoaderFuture<()> {
        self.core.init_wasm()
    }
    pub fn is_wasm_ready(&self) -> bool {
        self.core.ready.get()
    }
    pub fn get_crypto_api(&self) -> Result<Rc<PoseidonCrypto>, LoaderError> {
        self.core.get_crypto_api()
    }
}
