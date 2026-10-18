use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_registry::{loaders::EXCHANGE_LOADERS, module_host::*};
use safe_cex_store::{Store, store::DefaultStore};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn poll<T>(future: &mut (impl Future<Output = T> + Unpin)) -> Poll<T> {
    std::pin::Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
}
fn ready<T>(mut future: impl Future<Output = T> + Unpin) -> T {
    match poll(&mut future) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("REGISTRY unexpected pending"),
    }
}
#[derive(serde::Deserialize)]
struct Observation {
    native: bool,
    name: ExchangeName,
    path: String,
    export: String,
}
struct Namespace {
    expected: String,
    value: RefCell<Option<ExchangeConstructor>>,
    error: RefCell<Option<ModuleError>>,
    events: Rc<RefCell<Vec<String>>>,
}
impl ExchangeModule for Namespace {
    fn constructor_export(
        &self,
        export: &'static str,
    ) -> Result<Option<ExchangeConstructor>, ModuleError> {
        self.events.borrow_mut().push(export.into());
        if let Some(error) = &*self.error.borrow() {
            return Err(error.clone());
        }
        if export != self.expected {
            return Ok(None);
        }
        Ok(self.value.borrow().clone())
    }
}
struct Host {
    namespace: Rc<Namespace>,
    error: RefCell<Option<ModuleError>>,
    settled: Cell<bool>,
}
impl NativeModuleHost for Host {
    fn require_module(
        &self,
        specifier: &'static str,
    ) -> Result<Rc<dyn ExchangeModule>, ModuleError> {
        self.namespace.events.borrow_mut().push(specifier.into());
        if let Some(error) = &*self.error.borrow() {
            return Err(error.clone());
        }
        Ok(self.namespace.clone())
    }
}
impl WebModuleHost for Host {
    fn import_module(&self, specifier: &'static str) -> ModuleFuture<'_> {
        self.namespace.events.borrow_mut().push(specifier.into());
        Box::pin(std::future::poll_fn(move |_| {
            if !self.settled.get() {
                return Poll::Pending;
            }
            Poll::Ready(if let Some(error) = &*self.error.borrow() {
                Err(error.clone())
            } else {
                Ok(self.namespace.clone() as Rc<dyn ExchangeModule>)
            })
        }))
    }
}
fn constructor(calls: Rc<Cell<usize>>) -> ExchangeConstructor {
    ExchangeConstructor::new(move |_, _, _| {
        calls.set(calls.get() + 1);
        Err(ExchangeError::new("test constructor refusal"))
    })
}

#[test]
fn every_loader_matches_source_and_retains_native_identity_and_timing() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/loaders.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "REGISTRY source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observations: Vec<Observation> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        observations.len(),
        98,
        "REGISTRY independent source entry floor"
    );
    assert_eq!(
        EXCHANGE_LOADERS.len(),
        49,
        "REGISTRY independent callable registration floor"
    );
    let descriptors = safe_cex_registry::registry::EXCHANGE_REGISTRY;
    assert_eq!(
        descriptors.len(),
        49,
        "REGISTRY independent descriptor floor"
    );
    let mut descriptors_compared = 0;
    for (index, expected) in observations.iter().filter(|row| row.native).enumerate() {
        assert_eq!(
            descriptors[index].name, expected.name,
            "REGISTRY native descriptor name"
        );
        assert_eq!(
            descriptors[index].module, expected.path,
            "REGISTRY native descriptor module"
        );
        assert_eq!(
            descriptors[index].constructor, expected.export,
            "REGISTRY native descriptor constructor"
        );
        descriptors_compared += 1;
    }
    assert_eq!(
        descriptors_compared, 49,
        "REGISTRY independent descriptor comparisons"
    );
    let mut compared = 0;
    for native in [true, false] {
        assert_eq!(
            observations
                .iter()
                .filter(|row| row.native == native)
                .count(),
            49,
            "REGISTRY independent per-platform source entry floor"
        );
        for (entry, source) in EXCHANGE_LOADERS
            .iter()
            .zip(observations.iter().filter(|row| row.native == native))
        {
            compared += 1;
            // Source-derived strings are decoded observations, never Rust re-encoding.
            assert_eq!(entry.name, source.name, "REGISTRY native entry order");
            let calls = Rc::new(Cell::new(0));
            let selected = constructor(calls.clone());
            let events = Rc::new(RefCell::new(Vec::new()));
            let host = Host {
                namespace: Rc::new(Namespace {
                    expected: source.export.clone(),
                    value: RefCell::new(Some(selected.clone())),
                    error: RefCell::new(None),
                    events: events.clone(),
                }),
                error: RefCell::new(None),
                settled: Cell::new(false),
            };
            if native {
                let future = (entry.native)(&host).unwrap();
                assert_eq!(
                    *events.borrow(),
                    [source.path.clone(), source.export.clone()],
                    "REGISTRY native require and export before future return"
                );
                let value = ready(future).expect("REGISTRY native selected export");
                assert!(
                    value.same_constructor(&selected),
                    "REGISTRY native constructor identity"
                );
            } else {
                let mut future = (entry.web)(&host);
                assert_eq!(
                    *events.borrow(),
                    [source.path.clone()],
                    "REGISTRY web import before poll"
                );
                assert!(
                    poll(&mut future).is_pending(),
                    "REGISTRY web waits for host settlement"
                );
                assert_eq!(
                    events.borrow().len(),
                    1,
                    "REGISTRY web no premature export access"
                );
                let replacement = constructor(calls.clone());
                *host.namespace.value.borrow_mut() = Some(replacement.clone());
                host.settled.set(true);
                let value = ready(future)
                    .unwrap()
                    .expect("REGISTRY web selected export");
                assert!(
                    value.same_constructor(&replacement),
                    "REGISTRY web live constructor identity"
                );
                assert_eq!(
                    *events.borrow(),
                    [source.path.clone(), source.export.clone()],
                    "REGISTRY web export after settlement"
                );
            }
            assert_eq!(calls.get(), 0, "REGISTRY loader never constructs");
            *host.namespace.value.borrow_mut() = None;
            if native {
                assert!(
                    ready((entry.native)(&host).unwrap()).is_none(),
                    "REGISTRY native missing export"
                );
            } else {
                assert!(
                    ready((entry.web)(&host)).unwrap().is_none(),
                    "REGISTRY web missing export"
                );
            }
            let sentinel: ModuleError = Rc::new(ExchangeError::new("original host failure"));
            for export_error in [false, true] {
                *host.error.borrow_mut() = if export_error {
                    None
                } else {
                    Some(sentinel.clone())
                };
                *host.namespace.error.borrow_mut() = if export_error {
                    Some(sentinel.clone())
                } else {
                    None
                };
                let error = if native {
                    match (entry.native)(&host) {
                        Err(error) => error,
                        Ok(_) => panic!("REGISTRY native must fail before Ready exists"),
                    }
                } else {
                    match ready((entry.web)(&host)) {
                        Err(error) => error,
                        Ok(_) => panic!("REGISTRY web must reject"),
                    }
                };
                assert!(
                    Rc::ptr_eq(&error, &sentinel),
                    "REGISTRY original native failure identity"
                );
            }
        }
    }
    assert_eq!(
        compared, 98,
        "REGISTRY independently expected executed entries"
    );
}

#[test]
fn selected_constructor_is_callable_and_forwards_native_arguments() {
    let store: Rc<dyn Store> = Rc::new(DefaultStore::new());
    let expected_store = store.clone();
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let selected = ExchangeConstructor::new(move |name, options, store| {
        assert_eq!(
            name,
            ExchangeName::Kucoin,
            "REGISTRY constructor native name"
        );
        assert_eq!(options.key, "native-key", "REGISTRY constructor native key");
        assert_eq!(
            options.secret, "native-secret",
            "REGISTRY constructor native secret"
        );
        assert_eq!(
            options.testnet,
            Some(true),
            "REGISTRY constructor native testnet"
        );
        assert!(
            Rc::ptr_eq(&store, &expected_store),
            "REGISTRY constructor native Store identity"
        );
        captured.set(captured.get() + 1);
        Err(ExchangeError::new("explicit constructor refusal"))
    });
    let options: ExchangeOptions =
        serde_json::from_str(r#"{"key":"native-key","secret":"native-secret","testnet":true}"#)
            .unwrap();
    let result = selected.construct(ExchangeName::Kucoin, options, store);
    let Err(error) = result else {
        panic!("REGISTRY constructor refusal must propagate")
    };
    assert_eq!(
        error.message, "explicit constructor refusal",
        "REGISTRY constructor result propagation"
    );
    assert_eq!(
        calls.get(),
        1,
        "REGISTRY exactly one actual constructor invocation"
    );
}

#[test]
fn chosen_caller_owned_future_drops_export_continuation() {
    let mut cases = 0;
    for settled_before_drop in [false, true] {
        let calls = Rc::new(Cell::new(0));
        let events = Rc::new(RefCell::new(Vec::new()));
        let host = Host {
            namespace: Rc::new(Namespace {
                expected: "ApexOmniExchange".into(),
                value: RefCell::new(Some(constructor(calls.clone()))),
                error: RefCell::new(None),
                events: events.clone(),
            }),
            error: RefCell::new(None),
            settled: Cell::new(false),
        };
        let future = (EXCHANGE_LOADERS[0].web)(&host);
        assert_eq!(
            *events.borrow(),
            ["./apexomni/apexomni.exchange"],
            "REGISTRY CHOICE native import begins before polling"
        );
        if settled_before_drop {
            host.settled.set(true);
        }
        drop(future);
        host.settled.set(true);
        assert_eq!(
            *events.borrow(),
            ["./apexomni/apexomni.exchange"],
            "REGISTRY CHOICE native dropped continuation omits export"
        );
        assert_eq!(calls.get(), 0, "REGISTRY CHOICE native no construction");
        cases += 1;
    }
    assert_eq!(
        cases, 2,
        "REGISTRY CHOICE independent native schedule floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/registry-choice-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "REGISTRY CHOICE actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "REGISTRY CHOICE 17 source cases",
        "REGISTRY CHOICE completed source marker"
    );
}
