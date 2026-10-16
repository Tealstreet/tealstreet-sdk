use safe_cex_crypto::{
    poseidon_loaders::*,
    poseidon_types::{SchnorrKeyPair, WasmError},
};
use safe_cex_lighter_sdk::{key_runtime::*, poseidon_bridge::*};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Scheduler {
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
}
impl KeyScheduler for Scheduler {
    fn spawn(&self, f: Pin<Box<dyn Future<Output = ()>>>) {
        self.tasks.borrow_mut().push(f)
    }
}
impl Scheduler {
    fn pump(&self) {
        for mut f in self.tasks.take() {
            if f.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(f)
            }
        }
    }
}
fn ready<T>(scheduler: &Scheduler, mut f: KeyFuture<T, Rc<NativeKeyError>>) -> T {
    for _ in 0..100 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r.unwrap_or_else(|e| {
                panic!("bridge error {}", String::from_utf16_lossy(&e.message))
            });
        }
        scheduler.pump()
    }
    panic!("bridge did not settle")
}
struct Export {
    name: String,
    calls: Rc<RefCell<Vec<(String, Receiver, Vec<String>)>>>,
    fail: Rc<Cell<bool>>,
}
impl GlobalFunction for Export {
    fn call(&self, receiver: Receiver, args: &[String]) -> Result<CryptoValue, LoaderError> {
        self.calls
            .borrow_mut()
            .push((self.name.clone(), receiver, args.to_vec()));
        if self.fail.get() {
            return Ok(CryptoValue::Error(WasmError {
                error: "bridge crypto cause".into(),
            }));
        }
        Ok(if self.name == "schnorrGenerateKeys" {
            CryptoValue::Keys(SchnorrKeyPair {
                private_key: "A".repeat(80),
                public_key: "B".repeat(80),
            })
        } else {
            CryptoValue::Text("derived-through-ported-api".into())
        })
    }
}
struct Wasm {
    scheduler: Rc<Scheduler>,
    calls: Rc<RefCell<Vec<(String, Receiver, Vec<String>)>>>,
    fail: Rc<Cell<bool>>,
}
impl WasmHost for Wasm {
    fn import_wasm_exec(&self) -> LoaderFuture<()> {
        Box::pin(async { Ok(()) })
    }
    fn node_version_truthy(&self) -> bool {
        true
    }
    fn fetch_defined(&self) -> bool {
        false
    }
    fn import_fs(&self) -> LoaderFuture<()> {
        Box::pin(async { Ok(()) })
    }
    fn import_path(&self) -> LoaderFuture<()> {
        Box::pin(async { Ok(()) })
    }
    fn module_directory(&self) -> String {
        "/fixture".into()
    }
    fn path_join(&self, p: &[String]) -> Result<String, LoaderError> {
        Ok(p.join("/"))
    }
    fn read_file_sync(&self, _: &str) -> Result<Vec<u8>, LoaderError> {
        Ok(vec![1, 2])
    }
    fn origin(&self) -> Option<String> {
        None
    }
    fn fetch(&self, _: String) -> LoaderFuture<FetchResponse> {
        panic!("unexpected browser fetch")
    }
    fn array_buffer(&self, _: FetchResponse) -> LoaderFuture<Vec<u8>> {
        panic!("unexpected browser bytes")
    }
    fn new_go(&self) -> Result<Rc<dyn Any>, LoaderError> {
        Ok(Rc::new(()))
    }
    fn instantiate(&self, bytes: Vec<u8>, _: Rc<dyn Any>) -> LoaderFuture<Rc<dyn Any>> {
        assert_eq!(bytes, [1, 2], "SDK-BRIDGE actual loader bytes");
        Box::pin(async { Ok(Rc::new(()) as Rc<dyn Any>) })
    }
    fn run(&self, _: Rc<dyn Any>, _: Rc<dyn Any>) -> Result<(), LoaderError> {
        Ok(())
    }
    fn sleep(&self, ms: u64) -> LoaderFuture<()> {
        assert_eq!(ms, 100, "SDK-BRIDGE ported loader delay");
        Box::pin(async { Ok(()) })
    }
    fn global(&self, name: &str) -> Result<GlobalBinding, LoaderError> {
        Ok(GlobalBinding::Function(Rc::new(Export {
            name: name.into(),
            calls: self.calls.clone(),
            fail: self.fail.clone(),
        })))
    }
    fn spawn_init(&self, f: LoaderFuture<()>) {
        self.scheduler
            .spawn(Box::pin(async move { f.await.unwrap() }))
    }
}
struct Index {
    module: Rc<WasmLoader>,
    scheduler: Rc<Scheduler>,
}
impl IndexHost for Index {
    fn window_defined(&self) -> Result<bool, LoaderError> {
        Ok(false)
    }
    fn worker_constructor(&self) -> Result<Option<Rc<dyn Any>>, LoaderError> {
        Ok(None)
    }
    fn self_defined(&self) -> Result<bool, LoaderError> {
        Ok(false)
    }
    fn self_instanceof_worker(&self, _: Rc<dyn Any>) -> Result<bool, LoaderError> {
        Ok(false)
    }
    fn import_loader(&self, kind: LoaderKind) -> LoaderFuture<Rc<WasmLoader>> {
        assert_eq!(kind, LoaderKind::Node, "SDK-BRIDGE ported index branch");
        let m = self.module.clone();
        Box::pin(async move { Ok(m) })
    }
    fn spawn_init(&self, f: LoaderFuture<()>) {
        self.scheduler
            .spawn(Box::pin(async move { f.await.unwrap() }))
    }
}
#[test]
fn production_bridge_calls_the_ported_index_loader_and_crypto_exports() {
    let scheduler = Rc::new(Scheduler::default());
    let calls = Rc::new(RefCell::new(vec![]));
    let fail = Rc::new(Cell::new(false));
    let wasm = Rc::new(Wasm {
        scheduler: scheduler.clone(),
        calls: calls.clone(),
        fail: fail.clone(),
    });
    let index = PoseidonIndex::new(Rc::new(Index {
        module: WasmLoader::new(LoaderKind::Node, wasm),
        scheduler: scheduler.clone(),
    }));
    let host = Rc::new(PoseidonKeyHost {
        index: index.clone(),
        scheduler: scheduler.clone(),
    });
    assert!(!index.is_wasm_ready(), "SDK-BRIDGE initial readiness");
    let keys = ready(&scheduler, generate_key_pair(host.clone()));
    assert!(index.is_wasm_ready(), "SDK-BRIDGE initialized actual index");
    assert_eq!(
        keys.private_key,
        NativeCryptoValue::Text("A".repeat(80).encode_utf16().collect()),
        "SDK-BRIDGE native private key"
    );
    assert_eq!(
        keys.public_key,
        NativeCryptoValue::Text("B".repeat(80).encode_utf16().collect()),
        "SDK-BRIDGE native public key"
    );
    let derived = ready(
        &scheduler,
        derive_public_key(host.clone(), "F".repeat(80).encode_utf16().collect()),
    );
    assert_eq!(
        derived,
        NativeCryptoValue::Text("derived-through-ported-api".encode_utf16().collect()),
        "SDK-BRIDGE native derivation"
    );
    assert_eq!(
        &*calls.borrow(),
        &[
            ("schnorrGenerateKeys".into(), Receiver::Global, vec![]),
            (
                "schnorrPkFromSk".into(),
                Receiver::Global,
                vec!["F".repeat(80)]
            )
        ],
        "SDK-BRIDGE live exported calls"
    );
    fail.set(true);
    let mut result = generate_key_pair(host);
    let error = loop {
        match result
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(r) => break r.err().expect("WASM error expected"),
            Poll::Pending => scheduler.pump(),
        }
    };
    assert_eq!(
        String::from_utf16_lossy(&error.message),
        "Key generation failed: bridge crypto cause",
        "SDK-BRIDGE native WASM error conversion"
    );
}
