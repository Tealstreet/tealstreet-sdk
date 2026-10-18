use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_types::StoreDataLoaded;
use safe_cex_venue_kraken::periodic_sync::{self, TickHost, TickUpdate};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::poll_fn,
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/periodic-sync-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(data["header"]["hashes"].as_object().unwrap().len(), 2);
        data
    })
}
fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}
fn count(tag: &str) -> Option<f64> {
    if tag == "missing" {
        return None;
    }
    Some(match tag {
        "zero" => 0.0,
        "negativeZero" => -0.0,
        "one" => 1.0,
        "nineteen" => 19.0,
        "twenty" => 20.0,
        "forty" => 40.0,
        "negativeTwenty" => -20.0,
        "fraction" => 20.5,
        "nan" => f64::NAN,
        "infinity" => f64::INFINITY,
        _ => panic!("independent count tag"),
    })
}
fn flags(value: &StoreDataLoaded, expected: &Value) {
    for (key, actual) in [
        ("markets", value.markets),
        ("tickers", value.tickers),
        ("orders", value.orders),
        ("positions", value.positions),
        ("balance", value.balance),
    ] {
        assert_eq!(
            actual,
            expected[key].as_bool().unwrap(),
            "TICK native loaded {key}"
        )
    }
}
struct Batch {
    marker: f64,
    truthy: bool,
}
enum Event {
    Fetch(&'static str),
    Full(&'static str, bool, f64),
    Loaded(StoreDataLoaded),
    Balance(bool, f64, StoreDataLoaded),
    Error(String, bool),
    Loop(f64),
    Override(f64, bool),
}
struct Task {
    future: RefCell<Option<ExchangeFuture<'static, ()>>>,
    settled: Cell<bool>,
    error: RefCell<Option<ExchangeError>>,
}
type LoopCallback = Rc<dyn Fn() -> Result<ExchangeFuture<'static, ()>, ExchangeError>>;
struct Host {
    owner: Weak<Host>,
    config: Value,
    family: String,
    disposed: Cell<bool>,
    loaded: RefCell<StoreDataLoaded>,
    released: RefCell<Vec<&'static str>>,
    tasks: RefCell<Vec<Rc<Task>>>,
    loops: RefCell<Vec<LoopCallback>>,
    events: RefCell<Vec<Event>>,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
    orders: Rc<Batch>,
    positions: Rc<Batch>,
    balance: Rc<Batch>,
}
impl Host {
    fn new(family: &str, config: Value) -> Rc<Self> {
        let mut error = ExchangeError::new(if flag(&config, "emptyMessage") {
            ""
        } else {
            "tick failure"
        });
        error.name = "TickError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        Rc::new_cyclic(|owner| Self {
            owner: owner.clone(),
            disposed: Cell::new(flag(&config, "disposed")),
            loaded: RefCell::new(StoreDataLoaded {
                markets: true,
                tickers: false,
                orders: false,
                positions: false,
                balance: false,
            }),
            released: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            loops: RefCell::new(vec![]),
            events: RefCell::new(vec![]),
            error: RefCell::new(Some(error)),
            pointer,
            orders: Rc::new(Batch {
                marker: 17.25,
                truthy: true,
            }),
            positions: Rc::new(Batch {
                marker: 19.5,
                truthy: true,
            }),
            balance: Rc::new(Batch {
                marker: 23.75,
                truthy: !flag(&config, "falseBalance"),
            }),
            family: family.into(),
            config,
        })
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            if let Some(error) = self.error.borrow_mut().take() {
                return Err(error);
            }
        }
        Ok(())
    }
    fn touch(&self, point: &str) {
        if self.config["disposeAt"] == point {
            self.disposed.set(true)
        }
    }
    fn fetch(
        &self,
        which: &'static str,
    ) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.events.borrow_mut().push(Event::Fetch(which));
        self.fail(&format!("sync-{which}"))?;
        let host = self.owner.upgrade().unwrap();
        Ok(Box::pin(poll_fn(move |_| {
            if !flag(&host.config, "immediate") && !host.released.borrow().contains(&which) {
                return Poll::Pending;
            }
            if host.config["fail"] == which || (host.config["fail"] == "emit" && which == "orders")
            {
                return Poll::Ready(Err(host.error.borrow_mut().take().unwrap()));
            }
            Poll::Ready(Ok(match which {
                "orders" => host.orders.clone(),
                "positions" => host.positions.clone(),
                "balance" => host.balance.clone(),
                _ => Rc::new(Batch {
                    marker: 0.0,
                    truthy: true,
                }),
            }))
        })))
    }
    fn drive(&self, cx: &mut Context<'_>) {
        let tasks = self.tasks.borrow().clone();
        for task in tasks {
            if task.settled.get() {
                continue;
            }
            let result = task.future.borrow_mut().as_mut().unwrap().as_mut().poll(cx);
            if let Poll::Ready(result) = result {
                task.future.borrow_mut().take();
                task.settled.set(true);
                if let Err(error) = result {
                    *task.error.borrow_mut() = Some(error)
                }
            }
        }
    }
}
impl TickHost for Host {
    type Orders = Batch;
    type Positions = Batch;
    type Balance = Batch;
    type Task = Rc<Task>;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn fetch_leverage_settings(&self) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let fetched = self.fetch("leverage")?;
        Ok(Box::pin(async move {
            fetched.await?;
            Ok(())
        }))
    }
    fn fetch_orders(&self) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.fetch("orders")
    }
    fn fetch_positions(&self) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.fetch("positions")
    }
    fn fetch_balance(&self) -> Result<ExchangeFuture<'static, Rc<Batch>>, ExchangeError> {
        self.fetch("balance")
    }
    fn balance_truthy(&self, balance: &Batch) -> bool {
        balance.truthy
    }
    fn loaded(&self) -> Result<StoreDataLoaded, ExchangeError> {
        Ok(self.loaded.borrow().clone())
    }
    fn update(&self, update: TickUpdate<Batch, Batch, Batch>) -> Result<(), ExchangeError> {
        match update {
            TickUpdate::Orders(batch) => {
                self.events.borrow_mut().push(Event::Full(
                    "orders",
                    Rc::ptr_eq(&batch, &self.orders),
                    batch.marker,
                ));
                self.fail("full-orders")?;
                self.touch("full-orders");
                if flag(&self.config, "changeLoaded") {
                    let mut loaded = self.loaded.borrow().clone();
                    loaded.markets = false;
                    loaded.tickers = true;
                    *self.loaded.borrow_mut() = loaded
                }
            }
            TickUpdate::Positions(batch) => {
                self.events.borrow_mut().push(Event::Full(
                    "positions",
                    Rc::ptr_eq(&batch, &self.positions),
                    batch.marker,
                ));
                self.fail("full-positions")?;
                self.touch("full-positions")
            }
            TickUpdate::Loaded(loaded) => {
                self.events.borrow_mut().push(Event::Loaded(loaded.clone()));
                self.fail("loaded-update")?;
                *self.loaded.borrow_mut() = loaded;
                self.touch("loaded")
            }
            TickUpdate::Balance(batch, loaded) => {
                self.events.borrow_mut().push(Event::Balance(
                    Rc::ptr_eq(&batch, &self.balance),
                    batch.marker,
                    loaded.clone(),
                ));
                self.fail("balance-update")?;
                *self.loaded.borrow_mut() = loaded;
                self.touch("balance")
            }
        }
        Ok(())
    }
    fn start_then(&self, future: ExchangeFuture<'static, ()>) -> Result<Rc<Task>, ExchangeError> {
        let task = Rc::new(Task {
            future: RefCell::new(Some(future)),
            settled: Cell::new(false),
            error: RefCell::new(None),
        });
        self.tasks.borrow_mut().push(task.clone());
        Ok(task)
    }
    fn promise_all(
        &self,
        tasks: Vec<Rc<Task>>,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let host = self.owner.upgrade().unwrap();
        Ok(Box::pin(poll_fn(move |cx| {
            host.drive(cx);
            for task in &tasks {
                if let Some(error) = task.error.borrow_mut().take() {
                    return Poll::Ready(Err(error));
                }
            }
            if tasks.iter().all(|task| task.settled.get()) {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        })))
    }
    fn emit_tick_error(
        &self,
        error: ExchangeError,
        fallback: &'static str,
    ) -> Result<(), ExchangeError> {
        let text = if error.message.is_empty() {
            fallback.into()
        } else {
            error.message.clone()
        };
        self.events.borrow_mut().push(Event::Error(
            text,
            (&*error.metadata as *const _) as usize == self.pointer,
        ));
        self.touch("error");
        if self.config["fail"] == "emit" {
            Err(error)
        } else {
            Ok(())
        }
    }
    fn current_tick(&self, count: f64) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        let host = self.owner.upgrade().unwrap();
        if flag(&self.config, "replaceTick") {
            self.events
                .borrow_mut()
                .push(Event::Override(count, std::ptr::eq(self, host.as_ref())));
            Ok(Box::pin(async { Ok(()) }))
        } else if self.family == "Futures" {
            Ok(Box::pin(periodic_sync::futures_tick(host, Some(count))))
        } else {
            Ok(Box::pin(periodic_sync::spot_tick(host, Some(count))))
        }
    }
    fn schedule_loop(&self, callback: LoopCallback, delay: f64) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Loop(delay));
        self.loops.borrow_mut().push(callback);
        self.fail("loop")
    }
}
fn run(family: &str, body: fn(Rc<Host>, Option<f64>) -> ExchangeFuture<'static, ()>) {
    let cases = oracle()[family].as_array().unwrap();
    assert_eq!(cases.len(), 38, "TICK independent cases");
    let mut total = 0;
    let mut checkpoints = 0;
    let mut failures = 0;
    for case in cases {
        let host = Host::new(family, case["config"].clone());
        let mut future = body(host.clone(), count(host.config["count"].as_str().unwrap()));
        let mut done = false;
        let mut result = None;
        let mut cx = Context::from_waker(Waker::noop());
        let mut checkpoint = 0;
        let mut poll = |future: &mut ExchangeFuture<'static, ()>,
                        done: &mut bool,
                        result: &mut Option<Result<(), ExchangeError>>| {
            host.drive(&mut cx);
            if !*done {
                if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                    *done = true;
                    *result = Some(value)
                }
            }
            host.drive(&mut cx)
        };
        let snapshot = |done: bool, index: usize| {
            let expected = &case["checkpoints"][index];
            assert_eq!(
                host.events.borrow().len(),
                expected["events"].as_u64().unwrap() as usize,
                "TICK native checkpoint events"
            );
            assert_eq!(
                done,
                expected["done"].as_bool().unwrap(),
                "TICK native checkpoint settlement"
            );
            assert_eq!(host.disposed.get(), expected["disposed"].as_bool().unwrap());
            flags(&host.loaded.borrow().clone(), &expected["flags"])
        };
        poll(&mut future, &mut done, &mut result);
        snapshot(done, checkpoint);
        checkpoint += 1;
        let order: Vec<&str> = host.config["order"]
            .as_array()
            .map(|rows| rows.iter().map(|v| v.as_str().unwrap()).collect())
            .unwrap_or(vec!["leverage", "orders", "positions", "balance"]);
        for pass in 0..if flag(&host.config, "recurse") { 2 } else { 1 } {
            if pass > 0 {
                let callback = host.loops.borrow_mut().remove(0);
                future = callback().expect("bound current tick factory");
                done = false;
                result = None;
                poll(&mut future, &mut done, &mut result);
                snapshot(done, checkpoint);
                checkpoint += 1
            }
            for which in &order {
                let key = match *which {
                    "leverage" => "leverage",
                    "orders" => "orders",
                    "positions" => "positions",
                    "balance" => "balance",
                    _ => panic!("release key"),
                };
                if host
                    .events
                    .borrow()
                    .iter()
                    .any(|e| matches!(e,Event::Fetch(x)if *x==key))
                {
                    host.touch(&format!("release-{key}"));
                    host.released.borrow_mut().push(key)
                }
                poll(&mut future, &mut done, &mut result);
                snapshot(done, checkpoint);
                checkpoint += 1;
            }
        }
        checkpoints += checkpoint;
        assert_eq!(checkpoint, case["checkpoints"].as_array().unwrap().len());
        match result.expect("tick settled") {
            Ok(()) => assert!(case["failure"].is_null()),
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    host.pointer,
                    "TICK native propagated original error identity"
                );
                assert_eq!(case["failure"]["identity"], true)
            }
        }
        let trace = host.events.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(trace.len(), expected.len());
        total += trace.len();
        for (event, value) in trace.iter().zip(expected) {
            match event {
                Event::Fetch(which) => {
                    assert_eq!(value["kind"], "fetch");
                    assert_eq!(*which, value["which"], "TICK native fetch order")
                }
                Event::Full(which, identity, marker) => {
                    assert_eq!(value["kind"], "full");
                    assert_eq!(*which, value["which"]);
                    assert!(*identity, "TICK native fetched batch identity");
                    assert_eq!(value["identity"], true);
                    assert_eq!(
                        format!("{:016x}", marker.to_bits()),
                        value["marker"]["number"]
                    )
                }
                Event::Loaded(loaded) => {
                    assert_eq!(value["kind"], "loaded");
                    flags(loaded, &value["flags"])
                }
                Event::Balance(identity, marker, loaded) => {
                    assert_eq!(value["kind"], "balance");
                    assert!(*identity);
                    assert_eq!(value["identity"], true);
                    assert_eq!(
                        format!("{:016x}", marker.to_bits()),
                        value["marker"]["number"]
                    );
                    flags(loaded, &value["flags"])
                }
                Event::Error(text, identity) => {
                    assert_eq!(value["kind"], "error");
                    assert_eq!(text, value["message"].as_str().unwrap());
                    assert!(*identity, "TICK native caught original error")
                }
                Event::Override(count, identity) => {
                    assert_eq!(value["kind"], "override");
                    assert_eq!(
                        format!("{:016x}", count.to_bits()),
                        value["count"]["number"],
                        "TICK native next count"
                    );
                    assert!(*identity, "TICK native current tick receiver");
                    assert_eq!(value["identity"], true);
                }
                Event::Loop(delay) => {
                    assert_eq!(value["kind"], "loop");
                    assert_eq!(
                        format!("{:016x}", delay.to_bits()),
                        value["delay"]["number"],
                        "TICK native loop delay"
                    )
                }
            }
        }
    }
    assert_eq!(
        (total, checkpoints, failures),
        if family == "Futures" {
            (291, 200, 2)
        } else {
            (268, 200, 2)
        },
        "TICK independent event/checkpoint/failure floors"
    );
}
#[test]
fn futures_tick_live_source() {
    run("Futures", |host, count| {
        Box::pin(periodic_sync::futures_tick(host, count))
    })
}
#[test]
fn spot_tick_live_source() {
    run("Spot", |host, count| {
        Box::pin(periodic_sync::spot_tick(host, count))
    })
}
