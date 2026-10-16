use safe_cex_crypto::{
    poseidon_example::{self, ConsoleValue, ExampleConsole},
    poseidon_loaders::*,
    poseidon_types::{SchnorrKeyPair, WasmError},
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Host {
    events: RefCell<Vec<String>>,
    tasks: RefCell<Vec<LoaderFuture<()>>>,
    exec_ready: Rc<Cell<bool>>,
    sleep_ready: Rc<Cell<bool>>,
    fail_read: Cell<bool>,
    node: Cell<bool>,
    fetch_defined: Cell<bool>,
    fetch_ok: Cell<bool>,
    origin: RefCell<Option<String>>,
    globals: RefCell<HashMap<String, GlobalBinding>>,
    go: RefCell<Option<Rc<dyn Any>>>,
    instance: RefCell<Option<Rc<dyn Any>>>,
    calls: Rc<RefCell<Vec<(String, Receiver, Vec<String>)>>>,
    keys: Rc<Cell<usize>>,
    fail_at: Rc<Cell<usize>>,
}
fn pending<T: 'static>(ready: Rc<Cell<bool>>, value: T) -> LoaderFuture<T> {
    let mut value = Some(value);
    Box::pin(std::future::poll_fn(move |_| {
        if ready.get() {
            Poll::Ready(Ok(value.take().unwrap()))
        } else {
            Poll::Pending
        }
    }))
}
impl WasmHost for Host {
    fn import_wasm_exec(&self) -> LoaderFuture<()> {
        self.events.borrow_mut().push("exec".into());
        pending(self.exec_ready.clone(), ())
    }
    fn node_version_truthy(&self) -> bool {
        self.node.get()
    }
    fn fetch_defined(&self) -> bool {
        self.fetch_defined.get()
    }
    fn import_fs(&self) -> LoaderFuture<()> {
        self.events.borrow_mut().push("fs".into());
        Box::pin(async { Ok(()) })
    }
    fn import_path(&self) -> LoaderFuture<()> {
        self.events.borrow_mut().push("path".into());
        Box::pin(async { Ok(()) })
    }
    fn module_directory(&self) -> String {
        "/module".into()
    }
    fn path_join(&self, p: &[String]) -> Result<String, LoaderError> {
        assert_eq!(
            p,
            ["/module", "wasm", "poseidon_crypto.wasm"],
            "LOAD native node path parts"
        );
        Ok(p.join("/"))
    }
    fn read_file_sync(&self, path: &str) -> Result<Vec<u8>, LoaderError> {
        self.events.borrow_mut().push(format!("read:{path}"));
        if self.fail_read.get() {
            Err(LoaderError::new("read failed"))
        } else {
            Ok(vec![1, 2, 3])
        }
    }
    fn origin(&self) -> Option<String> {
        self.origin.borrow().clone()
    }
    fn fetch(&self, url: String) -> LoaderFuture<FetchResponse> {
        self.events.borrow_mut().push(format!("fetch:{url}"));
        let response = FetchResponse {
            ok: self.fetch_ok.get(),
            status: 502.,
            status_text: "Bad Gateway".into(),
            handle: Rc::new("response"),
        };
        Box::pin(async move { Ok(response) })
    }
    fn array_buffer(&self, response: FetchResponse) -> LoaderFuture<Vec<u8>> {
        assert_eq!(*response.handle.downcast_ref::<&str>().unwrap(), "response");
        self.events.borrow_mut().push("arrayBuffer".into());
        Box::pin(async { Ok(vec![1, 2, 3]) })
    }
    fn new_go(&self) -> Result<Rc<dyn Any>, LoaderError> {
        self.events.borrow_mut().push("go".into());
        let go: Rc<dyn Any> = Rc::new("GO");
        *self.go.borrow_mut() = Some(go.clone());
        Ok(go)
    }
    fn instantiate(&self, bytes: Vec<u8>, go: Rc<dyn Any>) -> LoaderFuture<Rc<dyn Any>> {
        assert_eq!(bytes, [1, 2, 3], "LOAD native input bytes");
        assert!(
            Rc::ptr_eq(&go, self.go.borrow().as_ref().unwrap()),
            "LOAD native Go identity"
        );
        self.events.borrow_mut().push("instantiate".into());
        let instance: Rc<dyn Any> = Rc::new("INSTANCE");
        *self.instance.borrow_mut() = Some(instance.clone());
        Box::pin(async move { Ok(instance) })
    }
    fn run(&self, go: Rc<dyn Any>, instance: Rc<dyn Any>) -> Result<(), LoaderError> {
        assert!(Rc::ptr_eq(&go, self.go.borrow().as_ref().unwrap()));
        assert!(Rc::ptr_eq(
            &instance,
            self.instance.borrow().as_ref().unwrap()
        ));
        self.events.borrow_mut().push("run".into());
        Ok(())
    }
    fn sleep(&self, ms: u64) -> LoaderFuture<()> {
        assert_eq!(ms, 100, "LOAD native initialization delay");
        self.events.borrow_mut().push("sleep:100".into());
        pending(self.sleep_ready.clone(), ())
    }
    fn global(&self, name: &str) -> Result<GlobalBinding, LoaderError> {
        Ok(self
            .globals
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or(GlobalBinding::Undefined))
    }
    fn spawn_init(&self, future: LoaderFuture<()>) {
        self.tasks.borrow_mut().push(future);
    }
}
struct Export {
    name: String,
    calls: Rc<RefCell<Vec<(String, Receiver, Vec<String>)>>>,
    keys: Rc<Cell<usize>>,
    fail_at: Rc<Cell<usize>>,
}
impl GlobalFunction for Export {
    fn call(&self, receiver: Receiver, args: &[String]) -> Result<CryptoValue, LoaderError> {
        self.calls
            .borrow_mut()
            .push((self.name.clone(), receiver, args.to_vec()));
        if self.calls.borrow().len() == self.fail_at.get() {
            return Ok(CryptoValue::Error(WasmError {
                error: "crypto failed".into(),
            }));
        }
        Ok(match self.name.as_str() {
            "schnorrGenerateKeys" => {
                let n = self.keys.get() + 1;
                self.keys.set(n);
                CryptoValue::Keys(SchnorrKeyPair {
                    private_key: format!("sk{n}"),
                    public_key: format!("pk{n}"),
                })
            }
            "schnorrPkFromSk" => CryptoValue::Text(args[0].replace("sk", "pk")),
            "schnorrSign" => CryptoValue::Text(format!("sig:{}:{}", args[0], args[1])),
            "schnorrVerify" => CryptoValue::Bool(args[1] == "pk1"),
            _ => CryptoValue::Text(format!("{}:{}", self.name, args.join("|"))),
        })
    }
}
fn host() -> Rc<Host> {
    let h = Rc::new(Host::default());
    h.exec_ready.set(true);
    h.sleep_ready.set(true);
    h.fetch_defined.set(true);
    h.fetch_ok.set(true);
    for name in [
        "poseidon2Hash",
        "poseidon2HashFp5",
        "poseidon2HashTwoToOne",
        "schnorrGenerateKeys",
        "schnorrPkFromSk",
        "schnorrSign",
        "schnorrVerify",
    ] {
        h.globals.borrow_mut().insert(
            name.into(),
            GlobalBinding::Function(Rc::new(Export {
                name: name.into(),
                calls: h.calls.clone(),
                keys: h.keys.clone(),
                fail_at: h.fail_at.clone(),
            })),
        );
    }
    h
}
fn poll<T>(f: &mut LoaderFuture<T>) -> Poll<Result<T, LoaderError>> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}
fn drain(h: &Host) {
    for _iteration in 0..32 {
        let tasks = std::mem::take(&mut *h.tasks.borrow_mut());
        for mut f in tasks {
            if poll(&mut f).is_pending() {
                h.tasks.borrow_mut().push(f);
            }
        }
    }
}
fn finish<T>(h: &Host, mut f: LoaderFuture<T>) -> Result<T, LoaderError> {
    for _iteration in 0..32 {
        match poll(&mut f) {
            Poll::Ready(v) => return v,
            Poll::Pending => drain(h),
        }
    }
    panic!("LOAD native pending at iteration32")
}
fn source() -> serde_json::Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/poseidon_loaders_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "LOAD source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn loaders_native_initialization_coalescing_retry_and_drop() {
    for (kind, node, expected) in [
        (
            LoaderKind::Browser,
            false,
            vec![
                "exec",
                "fetch:/wasm/poseidon_crypto.wasm",
                "arrayBuffer",
                "go",
                "instantiate",
                "run",
                "sleep:100",
            ],
        ),
        (
            LoaderKind::Node,
            false,
            vec![
                "exec",
                "fs",
                "path",
                "read:/module/wasm/poseidon_crypto.wasm",
                "go",
                "instantiate",
                "run",
                "sleep:100",
            ],
        ),
        (
            LoaderKind::Auto,
            true,
            vec![
                "exec",
                "fs",
                "path",
                "read:/module/wasm/poseidon_crypto.wasm",
                "go",
                "instantiate",
                "run",
                "sleep:100",
            ],
        ),
        (
            LoaderKind::Auto,
            false,
            vec![
                "exec",
                "fetch:/wasm/poseidon_crypto.wasm",
                "arrayBuffer",
                "go",
                "instantiate",
                "run",
                "sleep:100",
            ],
        ),
    ] {
        let h = host();
        h.node.set(node);
        h.exec_ready.set(false);
        h.sleep_ready.set(false);
        let loader = WasmLoader::new(kind, h.clone());
        assert!(!loader.is_wasm_ready());
        assert_eq!(
            loader.get_crypto_api().err().unwrap().message,
            "WASM not initialized. Call initWasm() first."
        );
        let mut first = loader.init_wasm();
        let marker = loader.init_marker().unwrap();
        let second = loader.init_wasm();
        assert!(
            Rc::ptr_eq(&marker, &loader.init_marker().unwrap()),
            "LOAD native shared init marker"
        );
        assert_eq!(
            &*h.events.borrow(),
            &["exec"],
            "LOAD native one eager import"
        );
        assert!(poll(&mut first).is_pending());
        drop(second);
        h.exec_ready.set(true);
        drain(&h);
        assert!(
            !loader.is_wasm_ready(),
            "LOAD native readiness waits for delay"
        );
        assert_eq!(&*h.events.borrow(), &expected);
        h.sleep_ready.set(true);
        drain(&h);
        finish(&h, first).unwrap();
        assert!(loader.is_wasm_ready(), "LOAD native ready after delay");
        assert!(Rc::ptr_eq(&marker, &loader.init_marker().unwrap()));
        let before = h.events.borrow().len();
        finish(&h, loader.init_wasm()).unwrap();
        assert_eq!(h.events.borrow().len(), before);
    }
    let h = host();
    h.fail_read.set(true);
    let loader = WasmLoader::new(LoaderKind::Node, h.clone());
    let error = finish(&h, loader.init_wasm()).unwrap_err();
    assert_eq!(
        error.message, "Failed to initialize WASM: Error: read failed",
        "LOAD native wrapped failure"
    );
    assert!(loader.init_marker().is_none());
    assert!(!loader.is_wasm_ready());
    h.fail_read.set(false);
    finish(&h, loader.init_wasm()).unwrap();
    assert!(loader.is_wasm_ready());
    let h = host();
    h.fetch_ok.set(false);
    let loader = WasmLoader::new(LoaderKind::Browser, h.clone());
    assert_eq!(
        finish(&h, loader.init_wasm()).unwrap_err().message,
        "Failed to initialize WASM: Error: Failed to fetch WASM: 502 Bad Gateway"
    );
    assert!(!h.events.borrow().contains(&"go".into()));
    let h = host();
    h.fetch_defined.set(false);
    let loader = WasmLoader::new(LoaderKind::Auto, h.clone());
    assert_eq!(
        finish(&h, loader.init_wasm()).unwrap_err().message,
        "Failed to initialize WASM: Error: Unsupported environment for WASM loading"
    );
    assert_eq!(&*h.events.borrow(), &["exec"]);
    let h = host();
    h.exec_ready.set(false);
    let loader = WasmLoader::new(LoaderKind::Browser, h.clone());
    drop(loader.init_wasm());
    h.exec_ready.set(true);
    drain(&h);
    assert!(
        loader.is_wasm_ready(),
        "LOAD native abandoned initialization continues"
    );
    assert_eq!(source()["delay"], 100);
}
#[test]
fn crypto_api_native_live_exports_receiver_fallback_and_arguments() {
    let h = host();
    let loader = WasmLoader::new(LoaderKind::Node, h.clone());
    finish(&h, loader.init_wasm()).unwrap();
    let api = loader.get_crypto_api().unwrap();
    let other = loader.get_crypto_api().unwrap();
    assert!(!Rc::ptr_eq(&api, &other), "API native fresh API object");
    assert!(api.is_ready());
    assert_eq!(
        api.poseidon2_hash("input".into()).unwrap(),
        CryptoValue::Text("poseidon2Hash:input".into())
    );
    assert_eq!(
        api.poseidon2_hash_fp5("fp5".into()).unwrap(),
        CryptoValue::Text("poseidon2HashFp5:fp5".into())
    );
    h.globals.borrow_mut().remove("poseidon2HashFp5");
    assert_eq!(
        api.poseidon2_hash_fp5("fallback".into()).unwrap(),
        CryptoValue::Text("poseidon2Hash:fallback".into()),
        "API native nullish fp5 fallback"
    );
    h.globals
        .borrow_mut()
        .insert("poseidon2HashFp5".into(), GlobalBinding::NonCallable);
    assert_eq!(
        api.poseidon2_hash_fp5("blocked".into()).unwrap(),
        CryptoValue::Error(WasmError {
            error: "poseidon2HashFp5 function not available".into()
        })
    );
    h.globals
        .borrow_mut()
        .insert("poseidon2HashFp5".into(), GlobalBinding::Null);
    api.poseidon2_hash_fp5("null".into()).unwrap();
    api.poseidon2_hash_two_to_one("LEFT".into(), "RIGHT".into())
        .unwrap();
    let keys = api.schnorr_generate_keys().unwrap();
    let CryptoValue::Keys(keys) = keys else {
        panic!("API native key pair")
    };
    assert_eq!(keys.private_key, "sk1");
    assert_eq!(keys.public_key, "pk1");
    api.schnorr_pk_from_sk("PRIVATE".into()).unwrap();
    api.schnorr_sign("MESSAGE".into(), "SECRET".into()).unwrap();
    let valid = api
        .schnorr_verify("M".into(), "pk1".into(), "SIG".into())
        .unwrap();
    assert_eq!(valid, CryptoValue::Bool(true));
    let calls = h.calls.borrow();
    assert_eq!(calls.len(), 9, "API native independent export-call count");
    assert_eq!(
        calls[1].1,
        Receiver::Unbound,
        "API native fp5 unbound receiver"
    );
    assert_eq!(calls[2].1, Receiver::Unbound);
    for i in [0, 4, 5, 6, 7, 8] {
        assert_eq!(
            calls[i].1,
            Receiver::Global,
            "API native exact global receiver at call{i}"
        );
    }
    assert_eq!(
        calls[3].1,
        Receiver::Unbound,
        "API native null fallback unbound receiver"
    );
    assert_eq!(
        calls[4].2,
        ["LEFT", "RIGHT"],
        "API native two-to-one argument order"
    );
    assert_eq!(
        calls[7].2,
        ["MESSAGE", "SECRET"],
        "API native sign argument order"
    );
    assert_eq!(
        calls[8].2,
        ["M", "pk1", "SIG"],
        "API native verify argument order"
    );
    drop(calls);
    h.globals.borrow_mut().remove("poseidon2Hash");
    assert_eq!(
        api.poseidon2_hash("x".into()).unwrap_err().name,
        "TypeError",
        "API native live global removal"
    );
    assert_eq!(source()["fp5Receiver"], "unbound");
}
struct IndexFixture {
    host: Rc<Host>,
    module: Rc<WasmLoader>,
    window: Cell<bool>,
    worker: Cell<bool>,
    self_defined: Cell<bool>,
    instance: Cell<bool>,
    imports: Cell<usize>,
    invalid_constructor: Cell<bool>,
    constructor: Rc<dyn Any>,
    selected: RefCell<Vec<LoaderKind>>,
}
impl IndexHost for IndexFixture {
    fn window_defined(&self) -> Result<bool, LoaderError> {
        Ok(self.window.get())
    }
    fn worker_constructor(&self) -> Result<Option<Rc<dyn Any>>, LoaderError> {
        Ok(self.worker.get().then(|| self.constructor.clone()))
    }
    fn self_defined(&self) -> Result<bool, LoaderError> {
        Ok(self.self_defined.get())
    }
    fn self_instanceof_worker(&self, constructor: Rc<dyn Any>) -> Result<bool, LoaderError> {
        assert!(
            Rc::ptr_eq(&constructor, &self.constructor),
            "INDEX native captured constructor identity"
        );
        if self.invalid_constructor.get() {
            return Err(LoaderError {
                name: "TypeError".into(),
                message: "Right-hand side of 'instanceof' is not callable".into(),
            });
        }
        self.host.events.borrow_mut().push("instanceof".into());
        Ok(self.instance.get())
    }
    fn import_loader(&self, kind: LoaderKind) -> LoaderFuture<Rc<WasmLoader>> {
        self.imports.set(self.imports.get() + 1);
        self.selected.borrow_mut().push(kind);
        let module = if kind == LoaderKind::Browser {
            WasmLoader::new(LoaderKind::Browser, self.host.clone())
        } else {
            self.module.clone()
        };
        Box::pin(async move { Ok(module) })
    }
    fn spawn_init(&self, f: LoaderFuture<()>) {
        self.host.spawn_init(f)
    }
}
fn index(h: Rc<Host>) -> (Rc<PoseidonIndex>, Rc<IndexFixture>) {
    let fixture = Rc::new(IndexFixture {
        host: h.clone(),
        module: WasmLoader::new(LoaderKind::Node, h),
        window: Cell::new(false),
        worker: Cell::new(false),
        self_defined: Cell::new(false),
        instance: Cell::new(false),
        imports: Cell::new(0),
        invalid_constructor: Cell::new(false),
        constructor: Rc::new("constructor"),
        selected: RefCell::new(vec![]),
    });
    (PoseidonIndex::new(fixture.clone()), fixture)
}
#[test]
fn index_native_environment_guards_and_preserved_failed_module() {
    let h = host();
    let (ix, f) = index(h.clone());
    assert!(!ix.is_wasm_ready());
    assert!(ix.get_crypto_api().is_err());
    assert!(
        !is_browser_like_runtime(f.as_ref()).unwrap(),
        "ENV native non-browser detection"
    );
    f.window.set(true);
    assert!(
        is_browser_like_runtime(f.as_ref()).unwrap(),
        "ENV native browser detection"
    );
    assert!(
        h.events.borrow().is_empty(),
        "INDEX native window short-circuits worker check"
    );
    f.window.set(false);
    f.worker.set(true);
    assert!(
        !is_browser_like_runtime(f.as_ref()).unwrap(),
        "ENV native non-browser detection"
    );
    f.self_defined.set(true);
    f.instance.set(true);
    assert!(
        is_browser_like_runtime(f.as_ref()).unwrap(),
        "ENV native browser detection"
    );
    f.instance.set(false);
    assert!(
        !is_browser_like_runtime(f.as_ref()).unwrap(),
        "ENV native non-browser detection"
    );
    h.events.borrow_mut().clear();
    h.fail_read.set(true);
    let a = ix.init_wasm();
    let marker = ix.init_marker().unwrap();
    let b = ix.init_wasm();
    assert!(Rc::ptr_eq(&marker, &ix.init_marker().unwrap()));
    assert_eq!(f.imports.get(), 1, "INDEX native one loader import");
    assert_eq!(
        finish(&h, a).unwrap_err().message,
        "Failed to initialize WASM: Error: Failed to initialize WASM: Error: read failed"
    );
    assert!(finish(&h, b).is_err());
    assert!(ix.init_marker().is_none());
    assert!(!ix.is_wasm_ready());
    h.fail_read.set(false);
    finish(&h, ix.init_wasm()).unwrap();
    assert!(
        !ix.is_wasm_ready(),
        "INDEX native retained failed module skips reinit"
    );
    assert_eq!(f.imports.get(), 1);
    assert!(ix.get_crypto_api().is_err());
    assert_eq!(
        &*f.selected.borrow(),
        &[LoaderKind::Node],
        "INDEX native actual node import selection"
    );
    let browser_host = host();
    let (browser, env) = index(browser_host.clone());
    env.window.set(true);
    finish(&browser_host, browser.init_wasm()).unwrap();
    assert_eq!(
        &*env.selected.borrow(),
        &[LoaderKind::Browser],
        "INDEX native actual browser import selection"
    );
    assert!(browser.is_wasm_ready());
    assert!(browser.get_crypto_api().unwrap().is_ready());
    let poisoned_host = host();
    let (poisoned, env) = index(poisoned_host.clone());
    env.worker.set(true);
    env.self_defined.set(true);
    env.invalid_constructor.set(true);
    let first = poisoned.init_wasm();
    let marker = poisoned.init_marker().unwrap();
    let error = finish(&poisoned_host, first).unwrap_err();
    assert_eq!(
        error.message,
        "Failed to initialize WASM: TypeError: Right-hand side of 'instanceof' is not callable"
    );
    let retained = poisoned.init_marker();
    assert!(
        retained.is_some(),
        "INDEX native pre-await failure retains rejected marker"
    );
    assert!(
        Rc::ptr_eq(&marker, &retained.unwrap()),
        "INDEX native same poisoned marker"
    );
    env.invalid_constructor.set(false);
    assert_eq!(
        finish(&poisoned_host, poisoned.init_wasm()).unwrap_err(),
        error
    );
    assert_eq!(
        env.imports.get(),
        0,
        "INDEX native poisoned marker prevents later imports"
    );
    assert!(!stub::is_wasm_ready(), "STUB native false readiness");
    assert_eq!(
        stub::init_wasm().err().unwrap().message,
        "WASM not loaded. This is a stub function.",
        "STUB native immediate refusal"
    );
    assert_eq!(
        stub::get_crypto_api().err().unwrap().message,
        "WASM not loaded. This is a stub function.",
        "STUB native immediate refusal"
    );
    assert_eq!(source()["failedModuleReady"], false);
}
#[derive(Default)]
struct Console(RefCell<Vec<(bool, Vec<ConsoleValue>)>>);
impl ExampleConsole for Console {
    fn log(&self, error: bool, args: &[ConsoleValue]) -> Result<(), LoaderError> {
        self.0.borrow_mut().push((error, args.to_vec()));
        Ok(())
    }
}
#[test]
fn example_native_key_roles_and_wrong_key_verification() {
    let h = host();
    let (ix, _) = index(h.clone());
    let console = Rc::new(Console::default());
    let pending = poseidon_example::main(ix, console.clone());
    assert_eq!(
        console.0.borrow().len(),
        1,
        "EXAMPLE native eager first log"
    );
    finish(&h, pending).unwrap();
    let calls = h.calls.borrow();
    assert_eq!(calls.len(), 8, "EXAMPLE native independent export count");
    assert_eq!(
        calls[0].2,
        ["0000000000000000000000000000000000000000000000000000000000000000"],
        "EXAMPLE native hash input"
    );
    assert_eq!(
        calls[1].2,
        [
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210"
        ],
        "EXAMPLE native distinct hash arguments"
    );
    assert_eq!(
        calls[4].2[0], "0000000000000000000000000000000000000000000000000000000000000001",
        "EXAMPLE native message"
    );
    assert_eq!(calls[4].0, "schnorrSign");
    assert_eq!(calls[4].2[1], "sk1", "EXAMPLE native private signing key");
    assert_eq!(calls[5].2[1], "pk1");
    assert_eq!(calls[7].2[1], "pk2", "EXAMPLE native wrong public key");
    drop(calls);
    assert_eq!(
        console.0.borrow().last().unwrap().1,
        vec![
            ConsoleValue::Text("Wrong key verification (should be false):".into()),
            ConsoleValue::Crypto(CryptoValue::Bool(false))
        ]
    );
    let logs = console.0.borrow();
    assert_eq!(
        logs.len(),
        24,
        "EXAMPLE native independent normal log count"
    );
    assert!(
        logs.iter().all(|(error, _)| !*error),
        "EXAMPLE native all normal logs use log channel"
    );
    let labels: Vec<&str> = logs
        .iter()
        .map(|(_, a)| match a.first() {
            None => "",
            Some(ConsoleValue::Text(s)) => s.as_str(),
            _ => panic!("EXAMPLE native text label"),
        })
        .collect();
    assert_eq!(
        labels,
        [
            "Initializing WASM...",
            "WASM initialized successfully!\n",
            "=== Example 1: Poseidon2 Hash ===",
            "Input:",
            "Hash:",
            "",
            "=== Example 2: Hash Two-to-One ===",
            "Hash1:",
            "Hash2:",
            "Combined:",
            "",
            "=== Example 3: Generate Schnorr Keys ===",
            "Private Key:",
            "Public Key:",
            "",
            "=== Example 4: Derive Public Key ===",
            "Derived PK:",
            "Matches generated PK:",
            "",
            "=== Example 5: Sign and Verify ===",
            "Message:",
            "Signature:",
            "Signature valid:",
            "Wrong key verification (should be false):"
        ],
        "EXAMPLE native complete normal label order"
    );
    drop(logs);
    for (at, label) in [
        (3, "Error generating keys:"),
        (5, "Error signing:"),
        (7, "Error generating wrong keys:"),
    ] {
        let error_host = host();
        error_host.fail_at.set(at);
        let (ix, _) = index(error_host.clone());
        let errors = Rc::new(Console::default());
        finish(&error_host, poseidon_example::main(ix, errors.clone())).unwrap();
        assert_eq!(
            error_host.calls.borrow().len(),
            at,
            "EXAMPLE native early exit call{at}"
        );
        assert_eq!(
            errors.0.borrow().last().unwrap(),
            &(
                true,
                vec![
                    ConsoleValue::Text(label.into()),
                    ConsoleValue::Text("crypto failed".into())
                ]
            ),
            "EXAMPLE native early error log{at}"
        );
    }
    assert_eq!(source()["exampleCalls"], 8);
}

#[test]
fn explicit_loader_modules_share_live_state_with_their_api() {
    macro_rules! check {
        ($module:ident) => {{
            let h = host();
            let module = $module::new(h.clone());
            assert!(!module.is_wasm_ready(), "MODULE native initial readiness");
            finish(&h, module.init_wasm()).unwrap();
            assert!(
                module.is_wasm_ready(),
                "MODULE native ready after initialization"
            );
            let api = module.get_crypto_api().unwrap();
            assert!(api.is_ready(), "MODULE native API reads same ready state");
            assert!(
                !Rc::ptr_eq(&api, &module.get_crypto_api().unwrap()),
                "MODULE native fresh API instance"
            );
        }};
    }
    check!(BrowserLoader);
    check!(NodeLoader);
    check!(AutoLoader);
}
