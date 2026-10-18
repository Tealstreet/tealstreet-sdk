use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::StoreDataLoaded;
use safe_cex_venue_kraken::startup::{
    self, SocketKind, StartupHost, StartupSocket, StartupState, StartupUpdate,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/startup-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 2);
        value
    })
}
fn flag(value: &Value, name: &str) -> bool {
    value[name].as_bool().unwrap_or(false)
}
fn initial() -> StoreDataLoaded {
    StoreDataLoaded {
        balance: true,
        orders: false,
        markets: false,
        tickers: true,
        positions: false,
    }
}
fn changed() -> StoreDataLoaded {
    StoreDataLoaded {
        balance: false,
        orders: true,
        markets: false,
        tickers: false,
        positions: true,
    }
}
fn assert_loaded(value: &StoreDataLoaded, expected: &Value) {
    for (name, field) in [
        ("balance", value.balance),
        ("orders", value.orders),
        ("markets", value.markets),
        ("tickers", value.tickers),
        ("positions", value.positions),
    ] {
        assert_eq!(
            field,
            expected[name].as_bool().unwrap(),
            "START native loaded {name}"
        )
    }
}
fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
fn kind_name(kind: SocketKind) -> &'static str {
    match kind {
        SocketKind::FuturesPublic => "FuturesPublic",
        SocketKind::FuturesPrivate => "FuturesPrivate",
        SocketKind::SpotPublic => "SpotPublic",
        SocketKind::SpotPrivate => "SpotPrivate",
    }
}
fn public(kind: SocketKind) -> bool {
    matches!(kind, SocketKind::FuturesPublic | SocketKind::SpotPublic)
}
#[derive(Clone)]
struct Snapshot {
    disposed: bool,
    only: bool,
    loaded: StoreDataLoaded,
    public: Option<usize>,
    private: Option<usize>,
}
fn assert_snapshot(value: &Snapshot, expected: &Value) {
    assert_eq!(
        value.disposed,
        expected["disposed"].as_bool().unwrap(),
        "START native disposal"
    );
    assert_eq!(
        value.only,
        expected["only"].as_bool().unwrap(),
        "START native public-only"
    );
    assert_loaded(&value.loaded, &expected["loaded"]);
    assert_eq!(
        value.public.map(|n| n as u64),
        expected["public"].as_u64(),
        "START native public slot"
    );
    assert_eq!(
        value.private.map(|n| n as u64),
        expected["private"].as_u64(),
        "START native private slot"
    )
}
struct Batch {
    marker: Cell<f64>,
}
enum Event {
    Request(&'static str, Snapshot),
    Update {
        stage: &'static str,
        loaded: StoreDataLoaded,
        identity: Option<bool>,
        marker: Option<f64>,
        state: Snapshot,
    },
    Create(SocketKind, bool, Snapshot),
    Connect(SocketKind, usize, Snapshot),
    Tick(f64, Snapshot),
}
struct Host {
    owner: Weak<Host>,
    config: Value,
    state: StartupState<Socket>,
    loaded: RefCell<StoreDataLoaded>,
    trace: RefCell<Vec<Event>>,
    released: [Rc<Cell<bool>>; 3],
    pointer: Cell<usize>,
    markets: Rc<Batch>,
    tickers: Rc<Batch>,
}
impl Host {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            disposed: self.state.disposed.get(),
            only: self.state.public_data_only.get(),
            loaded: self.loaded.borrow().clone(),
            public: self.state.public_socket.borrow().as_ref().map(|s| s.id),
            private: self.state.private_socket.borrow().as_ref().map(|s| s.id),
        }
    }
    fn effect(&self, stage: &str) {
        if self.config["disposeAt"] == stage {
            self.state.disposed.set(true)
        }
        if self.config["onlyAt"] == stage {
            self.state.public_data_only.set(true)
        }
        if self.config["loadedAt"] == stage {
            *self.loaded.borrow_mut() = changed()
        }
    }
    fn fails(&self, stage: &str) -> bool {
        self.config["failAt"] == stage
    }
    fn failure(&self) -> ExchangeError {
        let mut error = ExchangeError::new("startup failure");
        error.name = "StartupError".into();
        self.pointer.set((&*error.metadata as *const _) as usize);
        error
    }
    fn begin<T: 'static>(
        &self,
        stage: &'static str,
        index: usize,
        value: T,
    ) -> Result<ExchangeFuture<'static, T>, ExchangeError> {
        self.trace
            .borrow_mut()
            .push(Event::Request(stage, self.snapshot()));
        self.effect(&format!("{stage}-call"));
        if self.fails(&format!("{stage}-call")) {
            return Err(self.failure());
        }
        let owner = self.owner.clone();
        let released = self.released[index].clone();
        let mut value = Some(value);
        Ok(Box::pin(poll_fn(move |_| {
            if !released.get() {
                return Poll::Pending;
            }
            let host = owner.upgrade().unwrap();
            host.effect(&format!("{stage}-resolve"));
            Poll::Ready(if host.fails(&format!("{stage}-resolve")) {
                Err(host.failure())
            } else {
                Ok(value.take().unwrap())
            })
        })))
    }
}
struct Socket {
    kind: SocketKind,
    id: usize,
    owner: Weak<Host>,
}
impl StartupSocket for Socket {
    fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        let host = self.owner.upgrade().unwrap();
        host.trace
            .borrow_mut()
            .push(Event::Connect(self.kind, self.id, host.snapshot()));
        let stage = if public(self.kind) {
            "public-connect"
        } else {
            "private-connect"
        };
        host.effect(stage);
        if public(self.kind) && flag(&host.config, "replaceConnect") {
            *host.state.public_socket.borrow_mut() = Some(Rc::new(Self {
                kind: self.kind,
                id: 41,
                owner: host.owner.clone(),
            }))
        }
        if host.fails(stage) {
            Err(host.failure())
        } else {
            Ok(())
        }
    }
}
impl StartupHost for Host {
    type Socket = Socket;
    type Markets = Batch;
    type Tickers = Batch;
    fn state(&self) -> &StartupState<Socket> {
        &self.state
    }
    fn init_external_public_data(&self) -> Result<ExchangeFuture<'static, bool>, ExchangeError> {
        self.begin("init", 0, flag(&self.config, "external"))
    }
    fn fetch_markets(&self) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.begin("markets", 1, self.markets.clone())
    }
    fn fetch_tickers(&self) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.begin("tickers", 2, self.tickers.clone())
    }
    fn loaded(&self) -> Result<StoreDataLoaded, ExchangeError> {
        Ok(self.loaded.borrow().clone())
    }
    fn update(&self, value: StartupUpdate<Batch, Batch>) -> Result<(), ExchangeError> {
        let (stage, loaded, identity, marker) = match value {
            StartupUpdate::Markets(batch, loaded) => (
                "markets-update",
                loaded,
                Some(Rc::ptr_eq(&batch, &self.markets)),
                Some(batch.marker.get()),
            ),
            StartupUpdate::Tickers(batch, loaded) => (
                "tickers-update",
                loaded,
                Some(Rc::ptr_eq(&batch, &self.tickers)),
                Some(batch.marker.get()),
            ),
            StartupUpdate::Loaded(loaded) => ("loaded-update", loaded, None, None),
        };
        self.trace.borrow_mut().push(Event::Update {
            stage,
            loaded: loaded.clone(),
            identity,
            marker,
            state: self.snapshot(),
        });
        *self.loaded.borrow_mut() = loaded;
        self.effect(stage);
        if self.fails(stage) {
            Err(self.failure())
        } else {
            Ok(())
        }
    }
    fn create_socket(&self, kind: SocketKind) -> Result<Rc<Socket>, ExchangeError> {
        let owner = self.owner.upgrade().unwrap();
        self.trace.borrow_mut().push(Event::Create(
            kind,
            std::ptr::eq(self, Rc::as_ptr(&owner)),
            self.snapshot(),
        ));
        let stage = if public(kind) {
            "public-create"
        } else {
            "private-create"
        };
        self.effect(stage);
        if self.fails(stage) {
            Err(self.failure())
        } else {
            Ok(Rc::new(Socket {
                kind,
                id: if public(kind) { 31 } else { 37 },
                owner: self.owner.clone(),
            }))
        }
    }
    fn start_tick(&self, count: f64) -> Result<(), ExchangeError> {
        self.trace
            .borrow_mut()
            .push(Event::Tick(count, self.snapshot()));
        self.effect("tick");
        if self.fails("tick") {
            Err(self.failure())
        } else {
            Ok(())
        }
    }
}
fn compare(family: &str) {
    let cases = oracle()["starts"][family].as_array().unwrap();
    assert_eq!(cases.len(), 41, "START independent cases");
    let mut events = 0;
    let mut failures = 0;
    for case in cases {
        let host = Rc::new_cyclic(|owner| Host {
            owner: owner.clone(),
            config: case["config"].clone(),
            state: StartupState {
                disposed: Cell::new(flag(&case["config"], "disposed")),
                public_data_only: Cell::new(flag(&case["config"], "only")),
                public_socket: RefCell::new(None),
                private_socket: RefCell::new(None),
            },
            loaded: RefCell::new(initial()),
            trace: RefCell::new(vec![]),
            released: std::array::from_fn(|_| Rc::new(Cell::new(false))),
            pointer: Cell::new(0),
            markets: Rc::new(Batch {
                marker: Cell::new(17.25),
            }),
            tickers: Rc::new(Batch {
                marker: Cell::new(19.5),
            }),
        });
        if flag(&host.config, "existing") {
            let (pub_kind, priv_kind) = if family == "Futures" {
                (SocketKind::FuturesPublic, SocketKind::FuturesPrivate)
            } else {
                (SocketKind::SpotPublic, SocketKind::SpotPrivate)
            };
            *host.state.public_socket.borrow_mut() = Some(Rc::new(Socket {
                kind: pub_kind,
                id: 7,
                owner: host.owner.clone(),
            }));
            *host.state.private_socket.borrow_mut() = Some(Rc::new(Socket {
                kind: priv_kind,
                id: 9,
                owner: host.owner.clone(),
            }))
        }
        let mut future: std::pin::Pin<Box<dyn Future<Output = Result<(), ExchangeError>>>> =
            if family == "Futures" {
                Box::pin(startup::start_futures(host.clone()))
            } else {
                Box::pin(startup::start_spot(host.clone()))
            };
        let mut context = Context::from_waker(Waker::noop());
        let mut result = None;
        for checkpoint in 0..4 {
            if checkpoint > 0 {
                host.released[checkpoint - 1].set(true)
            }
            if result.is_none() {
                if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
                    result = Some(value)
                }
            }
            let expected = &case["checkpoints"][checkpoint];
            assert_snapshot(&host.snapshot(), &expected["state"]);
            assert_eq!(
                host.trace.borrow().len(),
                expected["events"].as_u64().unwrap() as usize,
                "START native checkpoint events"
            );
            assert_eq!(
                result.is_some(),
                expected["done"].as_bool().unwrap(),
                "START native settlement without awaiting tick"
            )
        }
        match result.expect("startup settled") {
            Ok(()) => assert!(case["failure"].is_null(), "START native success"),
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    host.pointer.get(),
                    "START native original error identity"
                );
                assert_eq!(case["failure"]["identity"], true)
            }
        }
        assert_snapshot(&host.snapshot(), &case["state"]);
        let trace = host.trace.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(trace.len(), expected.len(), "START native trace length");
        events += trace.len();
        for (native, expected) in trace.iter().zip(expected) {
            match native {
                Event::Request(stage, state) => {
                    assert_eq!(*stage, expected["kind"]);
                    assert_snapshot(state, &expected["state"])
                }
                Event::Update {
                    stage,
                    loaded,
                    identity,
                    marker,
                    state,
                } => {
                    assert_eq!(*stage, expected["kind"]);
                    assert_loaded(loaded, &expected["loaded"]);
                    assert_eq!(
                        *identity,
                        expected["batchIdentity"].as_bool(),
                        "START native fetched batch identity"
                    );
                    if let Some(marker) = marker {
                        assert_eq!(
                            bits(*marker),
                            expected["marker"]["number"],
                            "START native opaque marker"
                        )
                    } else {
                        assert!(expected["marker"].is_null())
                    }
                    assert_snapshot(state, &expected["state"])
                }
                Event::Create(kind, parent, state) => {
                    assert_eq!(expected["kind"], "create");
                    assert_eq!(
                        kind_name(*kind),
                        expected["socket"],
                        "START native constructor kind"
                    );
                    assert!(*parent, "START native constructor parent");
                    assert_eq!(expected["parentIdentity"], true);
                    assert_snapshot(state, &expected["state"])
                }
                Event::Connect(kind, id, state) => {
                    assert_eq!(expected["kind"], "connect");
                    assert_eq!(kind_name(*kind), expected["socket"]);
                    assert_eq!(*id as u64, expected["id"].as_u64().unwrap());
                    assert_snapshot(state, &expected["state"])
                }
                Event::Tick(count, state) => {
                    assert_eq!(expected["kind"], "tick");
                    assert_eq!(
                        bits(*count),
                        expected["count"]["number"],
                        "START native tick count"
                    );
                    assert_snapshot(state, &expected["state"])
                }
            }
        }
    }
    assert_eq!(events, 270, "START independent events");
    assert_eq!(failures, 14, "START independent failures");
}
#[test]
fn futures_start_live_source() {
    compare("Futures")
}
#[test]
fn spot_start_live_source() {
    compare("Spot")
}
#[test]
fn fetch_and_set_orders_live_source() {
    let cases = oracle()["orders"].as_array().unwrap();
    assert_eq!(cases.len(), 9, "SNAPSHOT independent cases");
    let mut events = 0;
    let mut failures = 0;
    for case in cases {
        let config = &case["config"];
        let trace = RefCell::new(Vec::new());
        let loaded = RefCell::new(initial());
        let batch = Rc::new(Batch {
            marker: Cell::new(23.75),
        });
        let released = Cell::new(false);
        let pointer = Cell::new(0);
        let fail = || {
            let mut error = ExchangeError::new("orders snapshot failure");
            error.name = "SnapshotError".into();
            pointer.set((&*error.metadata as *const _) as usize);
            error
        };
        let fetch = || {
            trace
                .borrow_mut()
                .push(("fetch", Some(loaded.borrow().clone()), None, None));
            if flag(config, "syncFail") {
                return Err(fail());
            }
            let mut value = Some(batch.clone());
            let released = &released;
            let fail = &fail;
            Ok(poll_fn(move |_| {
                if !released.get() {
                    Poll::Pending
                } else if flag(config, "fetchFail") {
                    Poll::Ready(Err(fail()))
                } else {
                    Poll::Ready(Ok(value.take().unwrap()))
                }
            }))
        };
        let add = |value: &Rc<Batch>| {
            trace.borrow_mut().push((
                "add",
                Some(loaded.borrow().clone()),
                Some(Rc::ptr_eq(value, &batch)),
                Some(value.marker.get()),
            ));
            if flag(config, "mutateBatch") {
                value.marker.set(29.125)
            }
            if flag(config, "loadedAtAdd") {
                *loaded.borrow_mut() = changed()
            }
            if flag(config, "addFail") {
                Err(fail())
            } else {
                Ok(())
            }
        };
        let update = |flags: StoreDataLoaded| {
            trace
                .borrow_mut()
                .push(("update", Some(flags.clone()), None, None));
            *loaded.borrow_mut() = flags;
            if flag(config, "updateFail") {
                Err(fail())
            } else {
                Ok(())
            }
        };
        let mut future = Box::pin(startup::fetch_and_set_orders(
            fetch,
            add,
            || Ok(loaded.borrow().clone()),
            update,
        ));
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_eq!(
            trace.borrow().len(),
            case["prefix"].as_u64().unwrap() as usize
        );
        if flag(config, "loadedAwait") {
            *loaded.borrow_mut() = changed()
        }
        released.set(true);
        let result = match first {
            Poll::Ready(value) => value,
            Poll::Pending => match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("unsettled orders snapshot"),
            },
        };
        match result {
            Ok(value) => {
                assert!(case["failure"].is_null());
                assert!(
                    Rc::ptr_eq(&value, &batch),
                    "SNAPSHOT native original returned batch"
                );
                assert_eq!(case["result"]["identity"], true);
                assert_eq!(
                    bits(value.marker.get()),
                    case["result"]["marker"]["number"],
                    "SNAPSHOT native returned marker"
                )
            }
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    pointer.get(),
                    "SNAPSHOT native original error identity"
                );
                assert_eq!(case["failure"]["identity"], true)
            }
        }
        assert_eq!(
            loaded.borrow().orders,
            case["loaded"]["orders"].as_bool().unwrap(),
            "SNAPSHOT native orders-loaded flag"
        );
        assert_loaded(&loaded.borrow(), &case["loaded"]);
        let trace = trace.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(trace.len(), expected.len());
        events += trace.len();
        for ((kind, flags, identity, marker), expected) in trace.iter().zip(expected) {
            assert_eq!(*kind, expected["kind"]);
            assert_loaded(flags.as_ref().unwrap(), &expected["loaded"]);
            if *kind == "add" {
                assert_eq!(*identity, expected["identity"].as_bool());
                assert_eq!(bits(marker.unwrap()), expected["marker"]["number"])
            }
        }
    }
    assert_eq!(events, 21, "SNAPSHOT independent events");
    assert_eq!(failures, 5, "SNAPSHOT independent failures");
}
