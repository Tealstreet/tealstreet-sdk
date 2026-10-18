#[path = "support/repair_runtime.rs"]
mod host;
use host::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order, Position, StoreDataLoaded, StoreSubscriberError};
use safe_cex_venue_asterdex::private_repairs::{PrivateRepairs, RepairParent, RepairState};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

type Response<T> = Rc<RefCell<Option<Result<T, ExchangeError>>>>;
struct Waiting<T>(Response<T>);
impl<T> Future for Waiting<T> {
    type Output = Result<T, ExchangeError>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.0
            .borrow_mut()
            .take()
            .map_or(Poll::Pending, Poll::Ready)
    }
}
#[derive(Debug, Clone)]
enum Request {
    Position,
    Balance(f64),
    Order,
}
#[derive(Default)]
struct Parent {
    pv: Cell<f64>,
    bv: Cell<f64>,
    requests: RefCell<Vec<Request>>,
    positions: RefCell<VecDeque<Response<Vec<Position>>>>,
    balances: RefCell<VecDeque<Response<bool>>>,
    orders: RefCell<VecDeque<Response<Vec<Order>>>>,
}
fn enqueue<T: 'static>(queue: &RefCell<VecDeque<Response<T>>>) -> ExchangeFuture<'static, T> {
    let response = Rc::new(RefCell::new(None));
    queue.borrow_mut().push_back(response.clone());
    Box::pin(Waiting(response))
}
impl RepairParent for Parent {
    fn position_state_version(&self) -> f64 {
        self.pv.get()
    }
    fn balance_state_version(&self) -> f64 {
        self.bv.get()
    }
    fn fetch_positions(&self) -> ExchangeFuture<'static, Vec<Position>> {
        self.requests.borrow_mut().push(Request::Position);
        enqueue(&self.positions)
    }
    fn reconcile_balance_after_private_update(
        &self,
        version: f64,
    ) -> ExchangeFuture<'static, bool> {
        self.requests.borrow_mut().push(Request::Balance(version));
        enqueue(&self.balances)
    }
    fn fetch_orders(&self) -> ExchangeFuture<'static, Vec<Order>> {
        self.requests.borrow_mut().push(Request::Order);
        enqueue(&self.orders)
    }
}
struct Fixture {
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
    runtime: Rc<Runtime>,
    parent: Rc<Parent>,
    base: Rc<BaseExchange>,
    store: Rc<DefaultStore>,
    repairs: Rc<PrivateRepairs>,
    notifications: Rc<Cell<usize>>,
}
fn fixture(initial: &Value) -> Fixture {
    let runtime = Rc::new(Runtime::default());
    runtime.wall.set(1000.0);
    let parent = Rc::new(Parent::default());
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        serde_json::from_value::<StoreUpdate>(initial.clone()).unwrap(),
        false,
    )
    .unwrap();
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    ));
    let notifications = Rc::new(Cell::new(0));
    let seen = notifications.clone();
    let _ = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    let repairs = PrivateRepairs::new(base.clone(), parent.clone());
    Fixture {
        listeners: RefCell::new(Vec::new()),
        runtime,
        parent,
        base,
        store,
        repairs,
        notifications,
    }
}
struct Snapshot {
    state: RepairState,
    positions: Vec<Position>,
    orders: Vec<Order>,
    loaded: StoreDataLoaded,
    timers: usize,
    requests: usize,
    notifications: usize,
}
struct Trace {
    snapshots: Vec<Snapshot>,
    requests: Vec<Request>,
    logs: Vec<(String, String)>,
    errors: Vec<String>,
}
fn resolve<T>(queue: &RefCell<VecDeque<Response<T>>>, step: &Value, decode: impl FnOnce() -> T) {
    let request = queue.borrow_mut().pop_front().expect("pending repair RPC");
    *request.borrow_mut() = Some(if let Some(error) = step.get("error") {
        Err(ExchangeError::new(error.as_str().unwrap()))
    } else {
        Ok(decode())
    });
}
fn run(case: &Value) -> Trace {
    let f = fixture(&case["initial"]);
    let mut snapshots = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        match step["op"].as_str().unwrap() {
            "schedule-position" => f.repairs.schedule_position_repair().unwrap(),
            "run-position" => {
                f.repairs.run_position_repair();
            }
            "clear-position" => f.repairs.clear_position_repair_timer(),
            "schedule-balance" => f
                .repairs
                .schedule_balance_reconcile(step["version"].as_f64().unwrap())
                .unwrap(),
            "run-balance" => {
                f.repairs
                    .run_balance_reconcile(step["version"].as_f64().unwrap());
            }
            "clear-balance" => f.repairs.clear_balance_reconcile_timer(),
            "schedule-order" => f.repairs.schedule_order_reconcile().unwrap(),
            "run-order" => {
                f.repairs.run_order_reconcile();
            }
            "clear-order" => f.repairs.clear_order_reconcile_timer(),
            "position-version" => f.parent.pv.set(step["value"].as_f64().unwrap()),
            "balance-version" => f.parent.bv.set(step["value"].as_f64().unwrap()),
            "tick" => f.runtime.mono.set(step["value"].as_f64().unwrap()),
            "resolve-position" => resolve(&f.parent.positions, step, || {
                serde_json::from_value(step["data"].clone()).unwrap()
            }),
            "resolve-balance" => resolve(&f.parent.balances, step, || {
                step["applied"].as_bool().unwrap()
            }),
            "resolve-order" => resolve(&f.parent.orders, step, || {
                serde_json::from_value(step["data"].clone()).unwrap()
            }),
            "subscriber-error" => {
                let message = step["message"].as_str().unwrap().to_owned();
                let cleanup = Store::subscribe(
                    f.store.as_ref(),
                    Box::new(move |_, _| {
                        Err(StoreSubscriberError::new(message.clone())
                            .with_cause(Rc::new(ExchangeError::new(message.clone()))))
                    }),
                );
                f.listeners.borrow_mut().push(cleanup);
            }
            "snapshot" => snapshots.push(Snapshot {
                state: f.repairs.state(),
                positions: f.store.positions().unwrap().to_vec(),
                orders: f.store.orders().unwrap().to_vec(),
                loaded: f.store.loaded().unwrap().clone(),
                timers: f.runtime.timers(),
                requests: f.parent.requests.borrow().len(),
                notifications: f.notifications.get(),
            }),
            op => panic!("unknown repair step {op}"),
        }
        f.runtime.pump();
    }
    let trace = Trace {
        snapshots,
        requests: f.parent.requests.borrow().clone(),
        logs: f.runtime.logs.borrow().clone(),
        errors: f.runtime.errors.borrow().clone(),
    };
    f.repairs.clear_position_repair_timer();
    f.repairs.clear_balance_reconcile_timer();
    f.repairs.clear_order_reconcile_timer();
    for cleanup in f.listeners.take() {
        cleanup();
    }
    f.base.dispose().unwrap();
    trace
}
fn bool_at(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap()
}
fn check_native(trace: &Trace, source: &Value, name: &str, snapshots: usize, requests: usize) {
    assert_eq!(trace.snapshots.len(), snapshots);
    assert_eq!(source["snapshots"].as_array().unwrap().len(), snapshots);
    assert_eq!(trace.requests.len(), requests, "{name} request count");
    assert_eq!(source["requests"].as_array().unwrap().len(), requests);
    for (actual, expected) in trace
        .snapshots
        .iter()
        .zip(source["snapshots"].as_array().unwrap())
    {
        let s = &expected["state"];
        assert_eq!(
            actual.state,
            RepairState {
                position_timer: bool_at(s, "positionTimer"),
                position_in_flight: bool_at(s, "positionInFlight"),
                position_queued: bool_at(s, "positionQueued"),
                balance_timer: bool_at(s, "balanceTimer"),
                balance_in_flight: bool_at(s, "balanceInFlight"),
                balance_queued: bool_at(s, "balanceQueued"),
                order_timer: bool_at(s, "orderTimer")
            },
            "{name} native flags"
        );
        assert_eq!(
            actual.timers,
            expected["timers"].as_u64().unwrap() as usize,
            "{name} timers"
        );
        assert_eq!(
            actual.requests,
            expected["requests"].as_u64().unwrap() as usize,
            "{name} dispatch count"
        );
        assert_eq!(
            actual.notifications,
            expected["notifications"].as_u64().unwrap() as usize,
            "{name} real-store notification count"
        );
        let loaded = &expected["loaded"];
        for (key, value) in [
            ("balance", actual.loaded.balance),
            ("orders", actual.loaded.orders),
            ("markets", actual.loaded.markets),
            ("tickers", actual.loaded.tickers),
            ("positions", actual.loaded.positions),
        ] {
            assert_eq!(value, bool_at(loaded, key), "{name} loaded.{key}");
        }
        // Both fixtures independently seed exactly two positions; full repair
        // clears absent contracts rather than removing the second position.
        assert_eq!(actual.positions.len(), 2);
        assert_eq!(expected["positions"].as_array().unwrap().len(), 2);
        for (native, position) in actual
            .positions
            .iter()
            .zip(expected["positions"].as_array().unwrap())
        {
            assert_eq!(native.id, position["id"].as_str().unwrap());
            assert_eq!(native.symbol, position["symbol"].as_str().unwrap());
            assert_eq!(
                native.side,
                serde_json::from_value::<safe_cex_types::PositionSide>(position["side"].clone())
                    .unwrap()
            );
            for (key, value) in [
                ("contracts", native.contracts),
                ("entryPrice", native.entry_price),
                ("notional", native.notional),
                ("unrealizedPnl", native.unrealized_pnl),
                ("leverage", native.leverage),
                ("liquidationPrice", native.liquidation_price),
            ] {
                assert_eq!(
                    value,
                    position[key].as_f64().unwrap(),
                    "{name} native position {key}"
                );
            }
            assert_eq!(
                native.margin_size,
                position.get("marginSize").and_then(Value::as_f64)
            );
        }
        // Independently expected one seed/replacement order in all six cases.
        assert_eq!(actual.orders.len(), 1);
        assert_eq!(expected["orders"].as_array().unwrap().len(), 1);
        for (native, order) in actual
            .orders
            .iter()
            .zip(expected["orders"].as_array().unwrap())
        {
            assert_eq!(native.id, order["id"].as_str().unwrap());
            assert_eq!(native.symbol, order["symbol"].as_str().unwrap());
            assert_eq!(
                native.status,
                serde_json::from_value::<safe_cex_types::OrderStatus>(order["status"].clone())
                    .unwrap()
            );
            assert_eq!(
                native.r#type,
                serde_json::from_value::<safe_cex_types::OrderType>(order["type"].clone()).unwrap()
            );
            assert_eq!(
                native.side,
                serde_json::from_value::<safe_cex_types::OrderSide>(order["side"].clone()).unwrap()
            );
            assert_eq!(native.reduce_only, order["reduceOnly"].as_bool().unwrap());
            for (key, value) in [
                ("price", native.price.to_number()),
                ("amount", native.amount),
                ("filled", native.filled),
                ("remaining", native.remaining),
                ("lastUpdate", native.last_update),
            ] {
                assert_eq!(
                    value,
                    order[key].as_f64().unwrap(),
                    "{name} native order {key}"
                );
            }
        }
    }
    for (request, expected) in trace
        .requests
        .iter()
        .zip(source["requests"].as_array().unwrap())
    {
        match request {
            Request::Position => assert_eq!(expected["kind"], "position"),
            Request::Order => assert_eq!(expected["kind"], "order"),
            Request::Balance(version) => {
                assert_eq!(expected["kind"], "balance");
                assert_eq!(
                    *version,
                    expected["version"].as_f64().unwrap(),
                    "{name} native balance version"
                );
            }
        }
    }
    let errors: Vec<_> = source["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(trace.errors, errors, "{name} rejected operation");
}
fn check_family(family: &str) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-repairs-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("private-repairs-cases.json")).unwrap();
    const NAMES: [&str; 6] = [
        "position-stale-follow-up",
        "position-listener-failure",
        "balance-first-version-and-latest-retry",
        "balance-error-after-clear",
        "order-debounce",
        "order-overlap-error-and-clear",
    ];
    const SNAPSHOTS: [usize; 6] = [6, 3, 5, 3, 4, 4];
    const REQUESTS: [usize; 6] = [2, 1, 2, 1, 1, 2];
    assert_eq!(source.len(), 6);
    assert_eq!(cases.len(), 6);
    let indices: Vec<usize> = cases
        .iter()
        .enumerate()
        .filter(|(_, c)| c["family"] == family)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(indices.len(), 2);
    let traces: Vec<_> = indices.iter().map(|i| run(&cases[*i])).collect();
    // ALL native values before any log/boundary comparison.
    for (i, trace) in indices.iter().zip(&traces) {
        assert_eq!(cases[*i]["name"], NAMES[*i]);
        assert_eq!(source[*i]["name"], NAMES[*i]);
        check_native(trace, &source[*i], NAMES[*i], SNAPSHOTS[*i], REQUESTS[*i]);
    }
    for (i, trace) in indices.iter().zip(&traces) {
        let logs: Vec<_> = source[*i]["logs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["severity"].as_str().unwrap().to_owned(),
                    l["message"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(trace.logs, logs, "{} log order", NAMES[*i]);
    }
}
#[test]
#[ignore = "requires live TypeScript/production Store oracle"]
fn position_repairs_match_source() {
    check_family("position");
}
#[test]
#[ignore = "requires live TypeScript/production Store oracle"]
fn balance_reconcile_matches_source() {
    check_family("balance");
}
#[test]
#[ignore = "requires live TypeScript/production Store oracle"]
fn order_reconcile_matches_source() {
    check_family("order");
}

#[test]
fn native_owner_cancellation_clears_busy_flags_and_blocks_new_timers() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("private-repairs-cases.json")).unwrap();
    let f = fixture(&cases[0]["initial"]);
    f.repairs.run_position_repair();
    f.repairs.run_balance_reconcile(1.0);
    assert!(f.repairs.state().position_in_flight);
    assert!(f.repairs.state().balance_in_flight);
    f.base.dispose().unwrap();
    assert!(!f.repairs.state().position_in_flight);
    assert!(!f.repairs.state().balance_in_flight);
    f.repairs.schedule_position_repair().unwrap();
    f.repairs.schedule_balance_reconcile(2.0).unwrap();
    f.repairs.schedule_order_reconcile().unwrap();
    assert_eq!(f.runtime.timers(), 0);
    assert_eq!(f.parent.requests.borrow().len(), 2);
}
