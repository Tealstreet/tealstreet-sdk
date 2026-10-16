use safe_cex_exchange::{
    cross_trade::{helpers::*, manager::*, place_order::*, *},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{
    ExchangeName, Market, OrderSide, OrderType, PlaceOrderOpts, object_map::JsObjectMap,
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("CROSS unexpectedly pending"),
    }
}
fn number(value: &str) -> f64 {
    match value {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        _ => value.parse().unwrap(),
    }
}
fn optional(value: &str) -> Option<f64> {
    if value == "undefined" {
        None
    } else {
        Some(number(value))
    }
}
fn native_number(actual: f64, expected: &str, label: &str) {
    let expected = number(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}")
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "{label}")
    }
}
fn native_optional(actual: Option<f64>, expected: &str, label: &str) {
    match (actual, optional(expected)) {
        (None, None) => {}
        (Some(actual), Some(_)) => native_number(actual, expected, label),
        _ => panic!("{label}: native optional number presence"),
    }
}
#[derive(serde::Deserialize)]
struct Modes {
    name: String,
    values: Vec<String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mapping {
    mappings: JsObjectMap<String>,
    fixed: Option<String>,
    symbol: String,
    #[serde(default)]
    forward: bool,
    #[serde(default)]
    no_exchange: bool,
    result: String,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Weight {
    weight: String,
    master_weight: String,
    mode: CrossTradWeightMode,
    master: CrossTradeStore,
    copy: CrossTradeStore,
    master_market: Market,
    copier_market: Market,
    result: String,
}
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum ExpectedEvent {
    Order {
        symbol: String,
        side: OrderSide,
        amount: String,
        price: String,
        #[serde(rename = "reduceOnly")]
        reduce_only: bool,
        r#type: OrderType,
    },
    Start {
        symbol: String,
        size: f64,
        side: OrderSide,
        #[serde(rename = "maxAttempts")]
        max_attempts: f64,
        #[serde(rename = "reduceOnly")]
        reduce_only: bool,
        delay: f64,
    },
    Terminate {
        id: String,
    },
    Warning {
        message: String,
    },
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceCase {
    label: String,
    copier: CrossTradeCopier,
    market: Market,
    amount: String,
    reference: String,
    active_chasers: Vec<safe_cex_types::ActiveChaser>,
    side: OrderSide,
    reduce_only: bool,
    fail: Option<String>,
    #[serde(default)]
    absent: bool,
    events: Vec<ExpectedEvent>,
    eager_events: usize,
    error: Option<String>,
    returned: bool,
    error_logs: usize,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedRequest {
    symbol: String,
    reference: String,
    account: String,
    side: OrderSide,
    amount: String,
    reduce_only: bool,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Audit {
    label: String,
    kind: String,
    config: CrossTradeConfig,
    copier: CrossTradeCopier,
    master: CrossTradeStore,
    copy: CrossTradeStore,
    slots: usize,
    calls: Vec<ExpectedRequest>,
    eager_calls: usize,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    failures: f64,
    muted_until: f64,
}
#[derive(serde::Deserialize)]
struct State {
    calls: usize,
    entry: Option<Entry>,
    muted: bool,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Lifecycle {
    lookups: Vec<String>,
    orders: Vec<ExpectedRequest>,
    scheduled: Vec<u64>,
    first_disposed: bool,
    second_disposed: bool,
}
#[derive(serde::Deserialize)]
struct Concurrency {
    master: CrossTradeStore,
    copy: CrossTradeStore,
    eager: Vec<ExpectedRequest>,
    calls: Vec<ExpectedRequest>,
}
#[derive(serde::Deserialize)]
struct Source {
    modes: Vec<Modes>,
    mappings: Vec<Mapping>,
    weights: Vec<Weight>,
    #[serde(rename = "placeCases")]
    place_cases: Vec<PlaceCase>,
    audits: Vec<Audit>,
    states: Vec<State>,
    lifecycle: Vec<Lifecycle>,
    concurrency: Concurrency,
}
fn source() -> Source {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cross-trade-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CROSS actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[derive(Default)]
struct Runtime {
    now: Cell<f64>,
    scheduled: RefCell<Vec<(CrossTradeLoopFn, u64)>>,
    errors: RefCell<Vec<String>>,
    debug_callback: RefCell<Option<Rc<dyn Fn(&str)>>>,
}
impl CrossTradeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn debug(&self, message: &str) {
        let callback = self.debug_callback.borrow().clone();
        if let Some(callback) = callback {
            callback(message);
        }
    }
    fn error(&self, message: &str, _: &ExchangeError) {
        self.errors.borrow_mut().push(message.into());
    }
    fn schedule_loop(&self, callback: CrossTradeLoopFn, interval: u64) {
        self.scheduled.borrow_mut().push((callback, interval));
    }
}
enum Event {
    Order(PlaceOrderOpts),
    Start(ChaserStart),
    Terminate(String),
    Warning(String),
}
struct Exchange {
    store: CrossTradeStore,
    events: Rc<RefCell<Vec<Event>>>,
    error: RefCell<Option<ExchangeError>>,
    warning_failure: bool,
    value: CrossTradeValue,
}
impl Exchange {
    fn new(store: CrossTradeStore) -> Rc<Self> {
        Rc::new(Self {
            store,
            events: Rc::new(RefCell::new(vec![])),
            error: RefCell::new(None),
            warning_failure: false,
            value: Rc::new("native-sentinel".to_string()),
        })
    }
}
impl CrossTradeExchange for Exchange {
    fn name(&self) -> ExchangeName {
        ExchangeName::Bybit
    }
    fn store(&self) -> CrossTradeStore {
        self.store.clone()
    }
    fn place_order(
        &self,
        order: PlaceOrderOpts,
    ) -> ExchangeFuture<'static, Option<CrossTradeValue>> {
        self.events.borrow_mut().push(Event::Order(order));
        let error = self.error.borrow_mut().take();
        let value = self.value.clone();
        Box::pin(async move {
            match error {
                Some(error) => Err(error),
                None => Ok(Some(value)),
            }
        })
    }
    fn chaser_terminate(&self, id: &str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Terminate(id.into()));
        Ok(())
    }
    fn chaser_start(&self, options: ChaserStart) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Start(options));
        Ok(())
    }
    fn warning(&self, message: &str) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::Warning(message.into()));
        if self.warning_failure {
            Err(ExchangeError::new("warning refused"))
        } else {
            Ok(())
        }
    }
}
fn market() -> Market {
    serde_json::from_str(r#"{"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.5},"limits":{"amount":{"min":0.01,"max":1000},"leverage":{"min":1,"max":100}}}"#).unwrap()
}
fn order(amount: f64, reduce_only: bool) -> CrossTradeOrder {
    CrossTradeOrder {
        copier: CrossTradeCopier::default(),
        market: market(),
        reference_price: Some(100.0),
        account: "c".into(),
        side: OrderSide::Buy,
        amount,
        reduce_only,
    }
}
fn assert_request(actual: &CrossTradeOrder, expected: &ExpectedRequest) {
    assert_eq!(
        actual.market.symbol, expected.symbol,
        "CROSS native audit symbol"
    );
    assert_eq!(actual.account, expected.account);
    assert_eq!(actual.side, expected.side, "CROSS native audit side");
    assert_eq!(
        actual.reduce_only, expected.reduce_only,
        "CROSS native audit reduce-only"
    );
    native_number(actual.amount, &expected.amount, "CROSS native audit amount");
    native_optional(
        actual.reference_price,
        &expected.reference,
        "CROSS native audit reference price",
    );
}
#[test]
fn native_defaults_enum_tags_mapping_and_weight_values() {
    let source = source();
    assert_eq!(source.modes.len(), 2);
    assert_eq!(source.modes[0].name, "CrossTradWeightMode");
    assert_eq!(source.modes[0].values, ["Balance", "Absolute"]);
    assert_eq!(source.modes[1].values, ["LimitSlippage", "Market", "Chase"]);
    // Decode actual source union strings and pin the native variants before wire operations.
    assert_eq!(
        serde_json::from_value::<Vec<CrossTradWeightMode>>(serde_json::json!(
            source.modes[0].values
        ))
        .expect("CROSS native weight-mode decode"),
        [CrossTradWeightMode::Balance, CrossTradWeightMode::Absolute],
        "CROSS native weight-mode variants"
    );
    assert_eq!(
        serde_json::from_value::<Vec<CrossTradeExecution>>(serde_json::json!(
            source.modes[1].values
        ))
        .expect("CROSS native execution-mode decode"),
        [
            CrossTradeExecution::LimitSlippage,
            CrossTradeExecution::Market,
            CrossTradeExecution::Chase
        ],
        "CROSS native execution-mode variants"
    );
    let copier = default_cross_trade_copier();
    assert!(!copier.enabled);
    assert_eq!(copier.account, None);
    assert_eq!(copier.weight, 1.0);
    assert_eq!(copier.execution, CrossTradeExecution::LimitSlippage);
    assert_eq!(copier.weight_mode, CrossTradWeightMode::Absolute);
    assert_eq!(copier.symbol_mappings.len(), 0);
    let config = default_cross_trade_config();
    assert!(!config.enabled);
    assert_eq!(config.master_account, None);
    assert_eq!(config.master_symbol, None);
    assert_eq!(config.master_weight, 1.0);
    assert_eq!(config.copiers.len(), 0);
    assert_eq!(source.mappings.len(), 12, "CROSS independent mapping floor");
    for case in source.mappings {
        let copier = CrossTradeCopier {
            symbol_mappings: case.mappings,
            ..Default::default()
        };
        let config = CrossTradeConfig {
            master_symbol: case.fixed,
            ..Default::default()
        };
        let actual = if case.forward {
            get_copier_symbol_from_master(
                &copier,
                (!case.no_exchange).then_some(&ExchangeName::Hyperliquid),
                &ExchangeName::Binance,
                &case.symbol,
            )
        } else {
            get_master_symbol_from_copier(
                &config,
                &copier,
                (!case.no_exchange).then_some(&ExchangeName::Binance),
                &ExchangeName::Bybit,
                &case.symbol,
            )
        };
        assert_eq!(
            actual,
            safe_cex_num::TableHit::Mapped(case.result),
            "CROSS native mapping precedence and collision"
        );
    }
    assert_eq!(source.weights.len(), 19, "CROSS independent weight floor");
    for case in source.weights {
        let copier = CrossTradeCopier {
            weight: number(&case.weight),
            weight_mode: case.mode,
            ..Default::default()
        };
        let config = CrossTradeConfig {
            master_weight: number(&case.master_weight),
            ..Default::default()
        };
        let actual = get_copier_weight_ratio(
            &config,
            &copier,
            &case.master,
            &case.copy,
            &case.master_market,
            &case.copier_market,
        );
        native_number(actual, &case.result, "CROSS native weight ratio");
    }
}
#[test]
fn native_execution_events_prices_errors_and_opaque_identity() {
    let source = source();
    assert_eq!(
        source.place_cases.len(),
        22,
        "CROSS independent execution floor"
    );
    for case in source.place_cases {
        let runtime = Rc::new(Runtime::default());
        let error = ExchangeError::new("order refused");
        let pointer = std::ptr::from_ref(error.metadata.as_ref());
        let ex = Rc::new(Exchange {
            store: CrossTradeStore {
                active_chasers: case.active_chasers,
                ..Default::default()
            },
            events: Rc::new(RefCell::new(vec![])),
            error: RefCell::new(case.fail.as_ref().map(|_| error)),
            warning_failure: case.fail.as_deref() == Some("warning"),
            value: Rc::new("native-sentinel".to_string()),
        });
        let ex_for_get = ex.clone();
        let absent = case.absent;
        let place = create_cross_trade_place_order(CrossTradePlaceOrderDeps {
            get_exchange: Rc::new(move |_| {
                Ok((!absent).then(|| ex_for_get.clone() as Rc<dyn CrossTradeExchange>))
            }),
            runtime: runtime.clone(),
        });
        let pending = place(CrossTradeOrder {
            copier: case.copier,
            market: case.market,
            reference_price: optional(&case.reference),
            account: "c".into(),
            side: case.side,
            amount: number(&case.amount),
            reduce_only: case.reduce_only,
        });
        assert_eq!(
            ex.events.borrow().len(),
            case.eager_events,
            "CROSS native eager execution events {}",
            case.label
        );
        let result = ready(pending);
        match (&result, &case.error) {
            (Ok(value), None) => {
                assert_eq!(value.is_some(), case.returned);
                if let Some(value) = value {
                    assert!(
                        Rc::ptr_eq(value, &ex.value),
                        "CROSS native opaque result identity"
                    );
                }
            }
            (Err(error), Some(expected)) => {
                assert_eq!(
                    &error.message, expected,
                    "CROSS native original/replacement error"
                );
                if case.fail.as_deref() != Some("warning") {
                    assert_eq!(
                        std::ptr::from_ref(error.metadata.as_ref()),
                        pointer,
                        "CROSS native original error allocation"
                    );
                }
            }
            _ => panic!("CROSS native error presence {}", case.label),
        }
        assert_eq!(runtime.errors.borrow().len(), case.error_logs);
        let events = ex.events.borrow();
        assert_eq!(
            events.len(),
            case.events.len(),
            "CROSS native execution event count {}",
            case.label
        );
        for (actual, expected) in events.iter().zip(case.events) {
            match (actual, expected) {
                (Event::Terminate(actual), ExpectedEvent::Terminate { id }) => {
                    assert_eq!(actual, &id)
                }
                (Event::Warning(actual), ExpectedEvent::Warning { message }) => {
                    assert_eq!(actual, &message)
                }
                (
                    Event::Start(actual),
                    ExpectedEvent::Start {
                        symbol,
                        size,
                        side,
                        max_attempts,
                        reduce_only,
                        delay,
                    },
                ) => {
                    assert_eq!(actual.symbol, symbol);
                    assert_eq!(actual.size.to_bits(), size.to_bits());
                    assert_eq!(actual.side, side);
                    assert_eq!(actual.max_attempts, max_attempts);
                    assert_eq!(actual.reduce_only, reduce_only);
                    assert_eq!(actual.delay, delay);
                }
                (
                    Event::Order(actual),
                    ExpectedEvent::Order {
                        symbol,
                        side,
                        amount,
                        price,
                        reduce_only,
                        r#type,
                    },
                ) => {
                    assert_eq!(actual.symbol, symbol);
                    assert_eq!(actual.side, side);
                    assert_eq!(actual.r#type, r#type);
                    assert_eq!(actual.reduce_only, Some(reduce_only));
                    native_number(actual.amount, &amount, "CROSS native order amount");
                    native_optional(actual.price, &price, "CROSS native adjusted slippage price");
                }
                _ => panic!("CROSS native execution event discriminant"),
            }
        }
    }
}
#[test]
fn native_position_audits_match_source_requests_before_polling() {
    let source = source();
    assert_eq!(
        source.audits.len(),
        26,
        "CROSS independent position-audit floor"
    );
    for case in source.audits {
        let calls = Rc::new(RefCell::new(vec![]));
        let capture = calls.clone();
        let manager = CrossTradeManager::new(CrossTradeManagerDeps {
            get_exchange: Rc::new(|_| Ok(None)),
            place_order: Rc::new(move |request| {
                capture.borrow_mut().push(request);
                Box::pin(async { Ok(None) })
            }),
            runtime: Rc::new(Runtime::default()),
        });
        let master = Exchange::new(case.master);
        let copy = Exchange::new(case.copy);
        let pending = if case.kind == "copier" {
            manager.audit_copier_positions(&case.config, &case.copier, master, copy)
        } else {
            manager.audit_master_positions(&case.config, &case.copier, master, copy)
        };
        assert_eq!(
            calls.borrow().len(),
            case.eager_calls,
            "CROSS native eager audit calls {}",
            case.label
        );
        assert_eq!(
            ready(pending).unwrap().len(),
            case.slots,
            "CROSS independent native audit slot floor {}",
            case.label
        );
        let calls = calls.borrow();
        assert_eq!(
            calls.len(),
            case.calls.len(),
            "CROSS native audit order count {}",
            case.label
        );
        for (actual, expected) in calls.iter().zip(case.calls) {
            assert_request(actual, &expected);
        }
    }
}
#[test]
fn native_failure_muting_expiry_minimum_and_late_completion() {
    let source = source();
    assert_eq!(source.states.len(), 8, "CROSS independent mute-state floor");
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(Cell::new(0));
    let fail = Rc::new(Cell::new(true));
    let count = calls.clone();
    let failing = fail.clone();
    let manager = CrossTradeManager::new(CrossTradeManagerDeps {
        get_exchange: Rc::new(|_| Ok(None)),
        place_order: Rc::new(move |_| {
            count.set(count.get() + 1);
            let error = failing.get();
            Box::pin(async move {
                if error {
                    Err(ExchangeError::new("failed"))
                } else {
                    Ok(None)
                }
            })
        }),
        runtime: runtime.clone(),
    });
    let steps = [
        (1000.0, 1.0, false),
        (1000.0, 1.0, false),
        (30999.0, 1.0, false),
        (31000.0, 1.0, false),
        (60999.0, 1.0, false),
        (61000.0, 1.0, false),
        (62000.0, 0.001, false),
        (62000.0, 0.001, true),
    ];
    for (index, ((now, amount, reduce_only), expected)) in
        steps.into_iter().zip(source.states).enumerate()
    {
        if index == 4 {
            fail.set(false);
        }
        runtime.now.set(now);
        ready(manager.safe_place_order(order(amount, reduce_only))).unwrap();
        assert_eq!(
            calls.get(),
            expected.calls,
            "CROSS native mute/minimum call count at step {index}"
        );
        assert_eq!(
            manager.is_muted("c", "BTCUSDT"),
            expected.muted,
            "CROSS native strict mute expiry at step {index}"
        );
        match (manager.failure_entry("c", "BTCUSDT"), expected.entry) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                assert_eq!(
                    actual.failures, expected.failures,
                    "CROSS native failure count"
                );
                assert_eq!(
                    actual.muted_until, expected.muted_until,
                    "CROSS native mute deadline"
                );
            }
            _ => panic!("CROSS native failure entry presence"),
        }
    }
    manager.record_failure("other", "ETHUSDT");
    manager.record_failure("c", "BTCUSDT");
    manager.record_success("c", "BTCUSDT");
    assert_eq!(
        manager.failure_entry("other", "ETHUSDT").unwrap().failures,
        1.0,
        "CROSS native success preserves unrelated entry"
    );
    assert!(
        manager.failure_entry("c", "BTCUSDT").is_none(),
        "CROSS native success removes target only"
    );
    manager.record_failure("c", "BTCUSDT");
    assert_eq!(
        manager.failure_entry("c", "BTCUSDT").unwrap().failures,
        1.0,
        "CROSS native failure restarts at one after success"
    );
    assert_eq!(
        CrossTradeManager::failure_key("a:b", "c"),
        "a:b:c",
        "CROSS native literal failure key"
    );
    assert_eq!(
        CrossTradeManager::failure_key("a:b", "c"),
        CrossTradeManager::failure_key("a", "b:c"),
        "CROSS preserves source colon collision"
    );
    let gate = Rc::new(Cell::new(false));
    let pending_gate = gate.clone();
    let late = CrossTradeManager::new(CrossTradeManagerDeps {
        get_exchange: Rc::new(|_| Ok(None)),
        place_order: Rc::new(move |_| {
            let gate = pending_gate.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if gate.get() {
                    Poll::Ready(Err(ExchangeError::new("late")))
                } else {
                    Poll::Pending
                }
            }))
        }),
        runtime,
    });
    let mut pending = late.safe_place_order(order(1.0, false));
    assert!(
        pending
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    late.stop();
    gate.set(true);
    ready(pending).unwrap();
    assert_eq!(
        late.failure_entry("c", "BTCUSDT").unwrap().failures,
        1.0,
        "CROSS native completion repopulates state after stop"
    );
    let mut market = market();
    for (min, amount, expected) in [
        (0.0, f64::NAN, true),
        (1.0, -1.0, true),
        (1.0, 0.99, false),
        (f64::NAN, 1.0, false),
    ] {
        market.limits.amount.min = min;
        assert_eq!(
            CrossTradeManager::meets_min_amount(&market, amount),
            expected,
            "CROSS native minimum amount predicate"
        );
    }
}
#[test]
fn native_loop_lifecycle_ignored_enabled_and_reentrant_stop() {
    let source = source();
    assert_eq!(source.lifecycle.len(), 1);
    let expected = source.lifecycle.into_iter().next().unwrap();
    let fixture = source
        .audits
        .into_iter()
        .find(|case| case.label == "open long")
        .unwrap();
    let runtime = Rc::new(Runtime::default());
    let master = Exchange::new(fixture.master);
    let copy = Exchange::new(fixture.copy);
    let lookups = Rc::new(RefCell::new(vec![]));
    let found = lookups.clone();
    let calls = Rc::new(RefCell::new(vec![]));
    let capture = calls.clone();
    // The lifecycle source fixture opens one contract rather than audit fixture's two.
    let mut master_store = master.store.clone();
    master_store.positions[0].contracts = 1.0;
    let master = Exchange::new(master_store);
    let manager = CrossTradeManager::new(CrossTradeManagerDeps {
        get_exchange: Rc::new(move |account| {
            found.borrow_mut().push(account.to_owned());
            Ok(Some(if account == "m" {
                master.clone()
            } else {
                copy.clone()
            }))
        }),
        place_order: Rc::new(move |request| {
            capture.borrow_mut().push(request);
            Box::pin(async { Ok(None) })
        }),
        runtime: runtime.clone(),
    });
    let mut config = fixture.config;
    config.enabled = false;
    config.copiers = vec![fixture.copier];
    manager.start(vec![config]);
    assert_eq!(
        calls.borrow().len(),
        0,
        "CROSS native first audit is deferred"
    );
    let first = manager.loop_state().unwrap();
    let (callback, interval) = runtime.scheduled.borrow_mut().remove(0);
    assert_eq!(interval, 3000);
    ready(callback()).unwrap();
    assert_eq!(*lookups.borrow(), expected.lookups);
    assert_eq!(
        calls.borrow().len(),
        1,
        "CROSS native ignores config enabled"
    );
    assert_request(&calls.borrow()[0], &expected.orders[0]);
    manager.start(vec![]);
    assert_eq!(first.is_disposed(), expected.first_disposed);
    let second = manager.loop_state().unwrap();
    manager.stop();
    assert_eq!(second.is_disposed(), expected.second_disposed);
    ready(first.run()).unwrap();
    ready(second.run()).unwrap();
    assert_eq!(
        runtime
            .scheduled
            .borrow()
            .iter()
            .map(|(_, delay)| *delay)
            .collect::<Vec<_>>(),
        expected.scheduled
    );
    let weak = Rc::downgrade(&manager);
    let once = Rc::new(Cell::new(false));
    let callback_once = once.clone();
    *runtime.debug_callback.borrow_mut() = Some(Rc::new(move |message| {
        if message.ends_with("Stopping cross trade manager") && !callback_once.replace(true) {
            weak.upgrade().unwrap().start(vec![]);
        }
    }));
    manager.stop();
    assert!(
        manager.loop_state().unwrap().is_disposed(),
        "CROSS native stop rereads a reentrant replacement loop"
    );
}

#[test]
fn native_copiers_continue_independently_and_stop_guards_master_audit() {
    let source = source().concurrency;
    let runtime = Rc::new(Runtime::default());
    let calls = Rc::new(RefCell::new(vec![]));
    let capture = calls.clone();
    let gate = Rc::new(Cell::new(false));
    let waiting = gate.clone();
    let master = Exchange::new(source.master);
    let copy = Exchange::new(source.copy);
    let manager = CrossTradeManager::new(CrossTradeManagerDeps {
        get_exchange: Rc::new(move |account| {
            Ok(Some(if account == "m" {
                master.clone()
            } else {
                copy.clone()
            }))
        }),
        place_order: Rc::new(move |request| {
            let blocked = request.account == "c1" && request.reduce_only;
            capture.borrow_mut().push(request);
            let gate = waiting.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if blocked && !gate.get() {
                    Poll::Pending
                } else {
                    Poll::Ready(Ok(None))
                }
            }))
        }),
        runtime: runtime.clone(),
    });
    let config = CrossTradeConfig {
        master_account: Some("m".into()),
        copiers: ["c1", "c2"]
            .map(|account| CrossTradeCopier {
                enabled: true,
                account: Some(account.into()),
                execution: CrossTradeExecution::Market,
                ..Default::default()
            })
            .into(),
        ..Default::default()
    };
    manager.start(vec![config]);
    let (callback, _) = runtime.scheduled.borrow_mut().remove(0);
    let mut future = callback();
    assert_eq!(
        calls.borrow().len(),
        2,
        "CROSS independent eager copier floor"
    );
    for (actual, expected) in calls.borrow().iter().zip(source.eager) {
        assert_request(actual, &expected);
    }
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(
        calls.borrow().len(),
        3,
        "CROSS second copier continues while first is pending"
    );
    manager.stop();
    gate.set(true);
    ready(future).unwrap();
    assert_eq!(
        calls.borrow().len(),
        3,
        "CROSS disposed guard blocks master audit"
    );
    for (actual, expected) in calls.borrow().iter().zip(source.calls) {
        assert_request(actual, &expected);
    }
    assert_eq!(
        runtime.scheduled.borrow().len(),
        0,
        "CROSS disposed guard blocks reschedule"
    );
}

#[test]
fn native_stock_prototype_symbol_results() {
    use safe_cex_num::{ProtoMember, TableHit};
    let mut copier = CrossTradeCopier::default();
    let config = CrossTradeConfig::default();
    assert_eq!(
        get_copier_symbol_from_master(&copier, None, &ExchangeName::Bybit, "constructor"),
        TableHit::Inherited(ProtoMember::Constructor),
        "CROSS native inherited constructor"
    );
    copier
        .symbol_mappings
        .insert("constructor".into(), "".into());
    assert_eq!(
        get_copier_symbol_from_master(&copier, None, &ExchangeName::Bybit, "constructor"),
        TableHit::Mapped("constructor".into()),
        "CROSS native own empty shadow"
    );
    copier
        .symbol_mappings
        .insert("constructor".into(), "COPY".into());
    assert_eq!(
        get_copier_symbol_from_master(&copier, None, &ExchangeName::Bybit, "constructor"),
        TableHit::Mapped("COPY".into()),
        "CROSS native own nonempty shadow"
    );
    copier
        .symbol_mappings
        .insert("BTCUSDT".into(), "__proto__".into());
    assert_eq!(
        get_master_symbol_from_copier(&config, &copier, None, &ExchangeName::Bybit, "__proto__"),
        TableHit::Inherited(ProtoMember::Prototype),
        "CROSS native inverted primitive setter"
    );
    let fixed = CrossTradeConfig {
        master_symbol: Some("FIXED".into()),
        ..Default::default()
    };
    assert_eq!(
        get_master_symbol_from_copier(&fixed, &copier, None, &ExchangeName::Bybit, "__proto__"),
        TableHit::Mapped("FIXED".into()),
        "CROSS native fixed precedence"
    );
    let _ = source();
}
