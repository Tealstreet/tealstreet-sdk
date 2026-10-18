use safe_cex_exchange::{capabilities::*, exchange::*, runtime::*};
#[path = "support/store.rs"]
mod test_store;
use safe_cex_types::*;
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use test_store::TestStore;

fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut cx = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("test expected a ready future"),
    }
}
struct Runtime(Cell<u64>);
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        100_000.0
    }
    fn now_ms(&self) -> f64 {
        100_000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no task expected")
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        self.0.set(self.0.get() + 1);
        self.0.get().to_string()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(
        &self,
        _: serde_json::Value,
    ) -> ExchangeFuture<'static, serde_json::Value> {
        Box::pin(async { Err(ExchangeError::new("Worker unavailable")) })
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    writes: Rc<RefCell<Vec<String>>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(BaseExchange::place_orders_sequential(self, orders))
    }
}
fn venue() -> Venue {
    venue_with_maximum(NumericValue::Number(5.0))
}
fn venue_with_maximum(maximum: NumericValue) -> Venue {
    let opts = serde_json::from_value(json!({"key":"k", "secret":"s"})).unwrap();
    let mut market: Market = serde_json::from_value(json!({"id":"BTC", "symbol":"BTC", "base":"BTC", "quote":"USDT", "active":true,"precision":{"amount":1,"price":0.1},"limits":{"amount":{"min":1,"max":5},"leverage":{"min":1,"max":100}}})).unwrap();
    market.limits.amount.max = maximum;
    let store = Rc::new(TestStore::new());
    store.data.borrow_mut().markets = vec![market];
    let settings = ExchangeSettings {
        tick_interval: Some(500.0),
        client_order_id_rule: Some(ClientOrderIdRule {
            label: "Venue".into(),
            requirement: "start with good".into(),
            is_valid: Rc::new(|id| id.starts_with("good")),
            is_honorable_shape: None,
            mint: None,
        }),
        ..Default::default()
    };
    let base = BaseExchange::new(
        ExchangeName::Bybit,
        opts,
        store,
        Rc::new(Runtime(Cell::new(0))),
        settings,
    );
    let writes = Rc::new(RefCell::new(vec![]));
    let record = writes.clone();
    let capabilities = ExchangeCapabilities {
        place_order: PlaceOrderBinding::implemented::<Venue>(
            move |_, order: ValidatedOrder<Venue>| {
                let record = record.clone();
                Box::pin(async move {
                    let id = order.options().client_order_id.clone().unwrap_or_default();
                    record.borrow_mut().push(id.clone());
                    Ok(vec![id])
                })
            },
        ),
        ..Default::default()
    };
    Venue {
        base,
        capabilities,
        writes,
    }
}
fn order(id: &str) -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTC", "type":"limit", "side":"buy", "amount":1, "clientOrderId":id}),
    )
    .unwrap()
}
#[test]
fn whole_batch_validates_before_any_venue_write_and_emits_before_rejection() {
    let venue = venue();
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    venue.base.on(
        "error",
        Rc::new(move |_, message| {
            seen.borrow_mut().push(message.clone());
            Ok(())
        }),
    );
    assert!(
        ready(
            venue
                .base
                .place_orders(&venue, vec![order("good-first"), order("bad-second")])
        )
        .is_err()
    );
    assert!(venue.writes.borrow().is_empty());
    assert_eq!(errors.borrow().len(), 1);
    assert!(
        ready(
            venue
                .base
                .place_orders(&venue, vec![order("good-first"), order("good-first")])
        )
        .is_err()
    );
    assert!(venue.writes.borrow().is_empty());
    assert_eq!(
        ready(
            venue
                .base
                .place_orders(&venue, vec![order("good-first"), order("good-second")])
        )
        .unwrap(),
        vec!["good-first", "good-second"]
    );
}
#[test]
fn zero_trigger_fields_are_present_and_split_checks_use_rounded_amount() {
    for maximum in [NumericValue::Number(5.0), NumericValue::Text("0005".into())] {
        let venue = venue_with_maximum(maximum);
        let mut opts = order("good");
        opts.stop_loss = Some(0.0);
        assert!(venue.base.resolve_caller_client_order_id(&opts).is_err());
        opts.stop_loss = None;
        opts.amount = 5.49;
        assert!(
            venue.base.resolve_caller_client_order_id(&opts).is_ok(),
            "caller split uses amount precision, not price precision"
        );
        opts.amount = 5.51;
        assert!(venue.base.resolve_caller_client_order_id(&opts).is_err());
    }
}
#[test]
fn minted_id_split_uses_amount_precision_identity() {
    for maximum in [NumericValue::Number(5.0), NumericValue::Text("0005".into())] {
        let mut venue = venue_with_maximum(maximum);
        venue
            .base
            .settings
            .client_order_id_rule
            .as_mut()
            .unwrap()
            .mint = Some(Rc::new(|| "good-minted".into()));
        let mut opts = order("good");
        opts.client_order_id = None;
        opts.amount = 5.49;
        assert_eq!(
            venue.base.mint_client_order_id(&opts).unwrap().as_deref(),
            Some("good-minted"),
            "mint split uses amount precision, not price precision"
        );
        opts.amount = 5.51;
        assert_eq!(venue.base.mint_client_order_id(&opts).unwrap(), None);
    }
}
#[test]
fn empty_response_and_fallback_brand_remain_distinguishable() {
    let venue = venue();
    let method = BaseCapabilityMethod::FetchIncomeHistory;
    assert!(is_base_capability_fallback(
        &venue.capabilities.fetch_income_history,
        method
    ));
    assert!(
        ready(venue.capabilities.fetch_income_history.call(&venue, None))
            .unwrap()
            .is_empty()
    );
    let implemented =
        CapabilityBinding::<Option<IncomeHistoryOptions>, Vec<IncomeRecord>>::implemented(
            |_, _| Box::pin(async { Ok(vec![]) }),
        );
    assert!(!is_base_capability_fallback(&implemented, method));
    assert!(!is_inert_capability(&implemented, method));
    let stub = CapabilityBinding::<Option<IncomeHistoryOptions>, Vec<IncomeRecord>>::empty_override(
        |_, _| Box::pin(async { Ok(vec![]) }),
    );
    assert!(!is_base_capability_fallback(&stub, method));
    assert!(is_inert_capability(&stub, method));
}

const SOURCE_BRAND_METHODS: [(&str, BaseCapabilityMethod); 10] = [
    ("fetchClosedPnl", BaseCapabilityMethod::FetchClosedPnl),
    (
        "fetchHistoricalFills",
        BaseCapabilityMethod::FetchHistoricalFills,
    ),
    (
        "fetchIncomeHistory",
        BaseCapabilityMethod::FetchIncomeHistory,
    ),
    (
        "fetchIncomeHistoryRange",
        BaseCapabilityMethod::FetchIncomeHistoryRange,
    ),
    (
        "fetchHistoricalOrders",
        BaseCapabilityMethod::FetchHistoricalOrders,
    ),
    ("placeOrder", BaseCapabilityMethod::PlaceOrder),
    ("start", BaseCapabilityMethod::Start),
    ("fetchOptionChain", BaseCapabilityMethod::FetchOptionChain),
    (
        "fetchOptionExpiries",
        BaseCapabilityMethod::FetchOptionExpiries,
    ),
    (
        "fetchAccountGreeks",
        BaseCapabilityMethod::FetchAccountGreeks,
    ),
];
fn source_brand() -> &'static serde_json::Value {
    static SOURCE: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/capability_brand_source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn brand_row(binding: &impl CapabilityBrand) -> Vec<bool> {
    SOURCE_BRAND_METHODS
        .iter()
        .map(|(_, method)| is_base_capability_fallback(binding, *method))
        .collect()
}
#[test]
fn native_capability_brand_matrix_matches_source() {
    let source = source_brand();
    assert_eq!(
        json!(SOURCE_BRAND_METHODS.map(|(name, _)| name)),
        source["methods"]
    );
    let base = ExchangeCapabilities::default();
    let matrix = vec![
        brand_row(&base.fetch_closed_pnl),
        brand_row(&base.fetch_historical_fills),
        brand_row(&base.fetch_income_history),
        brand_row(&base.fetch_income_history_range),
        brand_row(&base.fetch_historical_orders),
        brand_row(&base.place_order),
        brand_row(&base.start),
        brand_row(&base.fetch_option_chain),
        brand_row(&base.fetch_option_expiries),
        brand_row(&base.fetch_account_greeks),
    ];
    assert_eq!(
        json!(matrix),
        source["matrix"],
        "CAPABILITY_BRAND exact method identity"
    );
}
#[test]
fn native_capability_brand_replacement_and_callback_match_source() {
    let source = source_brand();
    let mut venue = venue();
    let original = std::mem::replace(
        &mut venue.capabilities.fetch_income_history,
        CapabilityBinding::implemented(|_, _| Box::pin(async { Ok(vec![]) })),
    );
    let method = BaseCapabilityMethod::FetchIncomeHistory;
    assert_eq!(
        json!({
            "retainedAlias": is_base_capability_fallback(&original, method),
            "replacementBranded": is_base_capability_fallback(&venue.capabilities.fetch_income_history, method),
            "originalEmpty": ready(original.call(&venue, None)).unwrap().is_empty(),
            "replacementEmpty": ready(venue.capabilities.fetch_income_history.call(&venue, None)).unwrap().is_empty(),
        }),
        source["replacement"],
        "CAPABILITY_BRAND replacement is unbranded"
    );
    let calls = Rc::new(Cell::new(0));
    let marker = Rc::new(Cell::new(17));
    let (observed, result) = (calls.clone(), marker.clone());
    let implemented = CapabilityBinding::implemented(move |_, ()| {
        observed.set(observed.get() + 1);
        let result = result.clone();
        Box::pin(async move { Ok(result) })
    });
    let brand = brand_row(&implemented);
    let calls_before = calls.get();
    let returned = ready(implemented.call(&venue, ())).unwrap();
    assert_eq!(
        json!({"brand":brand,"callsBefore":calls_before,
        "originalResult":Rc::ptr_eq(&returned,&marker),"callsAfter":calls.get()}),
        source["implemented"],
        "CAPABILITY_BRAND original callback and referent"
    );
}
#[test]
fn option_fallbacks_reject_naming_the_venue_and_carry_their_brand() {
    let venue = venue();
    let query: OptionChainQuery = serde_json::from_value(json!({"underlying": "BTC"})).unwrap();
    let message = |result: Result<_, ExchangeError>| match result {
        Ok::<(), _>(()) => panic!("option fallback must reject, never return an empty result"),
        Err(error) => error.message,
    };
    let chain = message(ready(venue.fetch_option_chain(query.clone())).map(|_| ()));
    let expiries = message(ready(venue.fetch_option_expiries("BTC".into())).map(|_| ()));
    let greeks = message(ready(venue.fetch_account_greeks()).map(|_| ()));
    let expected = [
        "Option chains are not supported on bybit",
        "Option expiries are not supported on bybit",
        "Account greeks are not supported on bybit",
    ];
    assert_eq!([chain, expiries, greeks], expected, "trait defaults");
    let capabilities = &venue.capabilities;
    let through_bindings = [
        message(ready(capabilities.fetch_option_chain(&venue, query)).map(|_| ())),
        message(ready(capabilities.fetch_option_expiries(&venue, "BTC".into())).map(|_| ())),
        message(ready(capabilities.fetch_account_greeks(&venue)).map(|_| ())),
    ];
    assert_eq!(through_bindings, expected, "branded capability defaults");
    assert!(is_base_capability_fallback(
        &capabilities.fetch_option_chain,
        BaseCapabilityMethod::FetchOptionChain
    ));
    assert!(is_base_capability_fallback(
        &capabilities.fetch_option_expiries,
        BaseCapabilityMethod::FetchOptionExpiries
    ));
    assert!(is_base_capability_fallback(
        &capabilities.fetch_account_greeks,
        BaseCapabilityMethod::FetchAccountGreeks
    ));
    assert!(!is_base_capability_fallback(
        &capabilities.fetch_option_chain,
        BaseCapabilityMethod::FetchOptionExpiries
    ));
    let implemented = CapabilityBinding::<(), Vec<AccountGreeks>>::implemented(|_, ()| {
        Box::pin(async { Ok(vec![]) })
    });
    assert!(!is_base_capability_fallback(
        &implemented,
        BaseCapabilityMethod::FetchAccountGreeks
    ));
}
#[test]
fn active_symbols_keep_original_strings_and_auth_backoff_caps_without_gating_history() {
    let venue = venue();
    venue
        .base
        .set_active_ticker_symbols(&[" BTC ".into(), " ".into(), "BTC".into(), "BTC".into()])
        .unwrap();
    assert_eq!(venue.base.get_active_ticker_symbols(), vec![" BTC ", "BTC"]);
    for _ in 0..20 {
        venue.base.record_auth_failure();
    }
    assert_eq!(venue.base.auth_failure_streak(), 7);
    assert_eq!(venue.base.get_tick_interval(), Some(60_000.0));
    assert!(
        ready(venue.capabilities.fetch_income_history.call(&venue, None))
            .unwrap()
            .is_empty()
    );
    venue.base.record_auth_success();
    assert_eq!(venue.base.get_tick_interval(), Some(500.0));
}

#[test]
fn symbol_filter_uses_javascript_trim_but_preserves_supplied_symbol_bytes() {
    let venue = venue();
    venue
        .base
        .set_active_ticker_symbols(&["\u{feff}".into(), "\u{0085}".into(), " BTC ".into()])
        .unwrap();
    assert_eq!(
        venue.base.get_active_ticker_symbols(),
        vec!["\u{0085}", " BTC "]
    );
}

struct OtherVenue(Venue);
impl Exchange for OtherVenue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.0.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.0.capabilities
    }
}

#[test]
fn dynamic_erasure_checks_identity_before_minting_typed_payloads() {
    let a = venue();
    let mut b = OtherVenue(venue());
    // B deliberately receives A's registered binding. Both concrete types are
    // in scope, but dynamic raw dispatch must never mint an A token for B.
    assert!(ready(b.base().place_order(&b, order("good-b"))).is_err());
    assert!(b.0.writes.borrow().is_empty());
    let record = b.0.writes.clone();
    b.0.capabilities.place_order = PlaceOrderBinding::implemented::<OtherVenue>(move |_, token| {
        let record = record.clone();
        Box::pin(async move {
            let id = token.options().client_order_id.clone().unwrap();
            record.borrow_mut().push(id.clone());
            Ok(vec![id])
        })
    });
    assert_eq!(
        ready(b.base().place_order(&b, order("good-b"))).unwrap(),
        vec!["good-b"]
    );
    assert_eq!(
        ready(a.base.place_order(&a, order("good-a"))).unwrap(),
        vec!["good-a"]
    );
    assert!(ready(a.base.place_order(&b, order("bad-b"))).is_err());
    assert_eq!(*b.0.writes.borrow(), vec!["good-b"]);
    b.0.capabilities.place_orders = PlaceOrdersBinding::implemented::<Venue>(|_, _| {
        panic!("wrong concrete bulk handler must never receive a token")
    });
    assert!(ready(b.base().place_orders(&b, vec![order("good-bulk")])).is_err());
}

struct PendingCancelVenue {
    inner: Venue,
    calls: Cell<usize>,
    ready: Cell<bool>,
}
impl Exchange for PendingCancelVenue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.inner.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.inner.capabilities
    }
    fn place_orders(&self, batch: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        Box::pin(BaseExchange::place_orders_sequential(self, batch))
    }
    fn cancel_all_orders(&self) -> ExchangeFuture<'_, ()> {
        self.calls.set(self.calls.get() + 1);
        Box::pin(std::future::poll_fn(move |_| {
            if self.ready.get() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
}
#[test]
fn adversarial_dropped_nuke_orders_does_not_permanently_latch_nuking() {
    let venue = PendingCancelVenue {
        inner: venue(),
        calls: Cell::new(0),
        ready: Cell::new(false),
    };
    let mut request = Box::pin(venue.base().nuke_orders(&venue));
    assert!(
        request
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(request);
    venue.ready.set(true);
    assert!(ready(venue.base().nuke_orders(&venue)).is_ok());
    assert_eq!(
        venue.calls.get(),
        2,
        "A dropped in-flight future cannot leave every future nuke silently skipped"
    );
}

#[test]
fn native_bulk_registration_receives_target_typed_batch_after_whole_preflight() {
    let mut venue = venue();
    assert!(
        is_base_capability_fallback(
            &venue.capabilities.place_orders,
            BaseCapabilityMethod::PlaceOrders
        ),
        "unregistered native batch override must remain visibly base fallback"
    );
    let batches = Rc::new(Cell::new(0));
    let count = batches.clone();
    venue.capabilities.place_orders =
        PlaceOrdersBinding::implemented::<Venue>(move |venue, batch| {
            count.set(count.get() + 1);
            let ids: Vec<_> = batch
                .orders()
                .iter()
                .map(|order| order.options().client_order_id.clone().unwrap())
                .collect();
            venue.writes.borrow_mut().extend(ids.clone());
            Box::pin(async move { Ok(ids) })
        });
    assert!(
        ready(
            venue
                .base
                .place_orders(&venue, vec![order("good-first"), order("bad-second")])
        )
        .is_err()
    );
    assert_eq!(batches.get(), 0);
    assert!(venue.writes.borrow().is_empty());
    assert_eq!(
        ready(
            venue
                .base
                .place_orders(&venue, vec![order("good-first"), order("good-second")])
        )
        .unwrap(),
        vec!["good-first", "good-second"]
    );
    assert_eq!(batches.get(), 1);
    assert!(!is_base_capability_fallback(
        &venue.capabilities.place_orders,
        BaseCapabilityMethod::PlaceOrders
    ));
    assert!(!is_inert_capability(
        &venue.capabilities.place_orders,
        BaseCapabilityMethod::PlaceOrders
    ));
    let stub = PlaceOrderBinding::empty_override::<Venue>(|_, _| Box::pin(async { Ok(vec![]) }));
    assert!(is_empty_stub_capability(&stub));
    assert!(is_inert_capability(&stub, BaseCapabilityMethod::PlaceOrder));
    assert!(is_base_capability_fallback(
        &PlaceOrderBinding::default(),
        BaseCapabilityMethod::PlaceOrder
    ));
}

#[derive(Clone, Debug, PartialEq)]
enum ExecutionMode {
    Plain,
    Bbo { level: u8 },
    TickSize { ticks: u32 },
}
#[derive(Clone)]
struct ExtendedOptions {
    shared: PlaceOrderOpts,
    execution: ExecutionMode,
}
impl std::borrow::Borrow<PlaceOrderOpts> for ExtendedOptions {
    fn borrow(&self) -> &PlaceOrderOpts {
        &self.shared
    }
}
impl From<PlaceOrderOpts> for ExtendedOptions {
    fn from(shared: PlaceOrderOpts) -> Self {
        Self {
            shared,
            execution: ExecutionMode::Plain,
        }
    }
}
impl<'de> serde::Deserialize<'de> for ExtendedOptions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let shared = serde_json::from_value(value.clone()).map_err(serde::de::Error::custom)?;
        let execution = match (
            value.get("bybitBbo"),
            value.get("bybitSlippageToleranceTicks"),
        ) {
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "BBO and TickSize are mutually exclusive",
                ));
            }
            (Some(bbo), None) => ExecutionMode::Bbo {
                level: serde_json::from_value(bbo["level"].clone())
                    .map_err(serde::de::Error::custom)?,
            },
            (None, Some(ticks)) => ExecutionMode::TickSize {
                ticks: serde_json::from_value(ticks.clone()).map_err(serde::de::Error::custom)?,
            },
            (None, None) => ExecutionMode::Plain,
        };
        Ok(Self { shared, execution })
    }
}
struct ExtendedVenue {
    inner: Venue,
    validated: RefCell<Vec<ExecutionMode>>,
    sent: RefCell<Vec<ExecutionMode>>,
}
impl Exchange for ExtendedVenue {
    type PlaceOpts = ExtendedOptions;
    fn base(&self) -> &BaseExchange {
        &self.inner.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.inner.capabilities
    }
    fn validate_order_capabilities(&self, options: &ExtendedOptions) -> Result<(), ExchangeError> {
        self.validated.borrow_mut().push(options.execution.clone());
        if options.execution == (ExecutionMode::Bbo { level: 0 }) {
            return Err(ExchangeError::new("invalid BBO level"));
        }
        Ok(())
    }
    fn place_order(&self, order: ValidatedOrder<Self>) -> ExchangeFuture<'_, Vec<String>> {
        self.sent
            .borrow_mut()
            .push(order.options().execution.clone());
        // Closing the fixture position lets the actual generic nuke finish once.
        self.base()
            .store
            .add_or_update_positions_full(vec![])
            .unwrap();
        Box::pin(async { Ok(vec!["native".into()]) })
    }
    fn place_orders(&self, orders: ValidatedBatch<Self>) -> ExchangeFuture<'_, Vec<String>> {
        for order in orders.orders() {
            self.sent
                .borrow_mut()
                .push(order.options().execution.clone());
        }
        Box::pin(async { Ok(vec!["bulk".into()]) })
    }
}
fn extended_venue(bulk: bool) -> ExtendedVenue {
    let mut inner = venue();
    inner.capabilities.place_order =
        PlaceOrderBinding::implemented::<ExtendedVenue>(|v, o| v.place_order(o));
    if bulk {
        inner.capabilities.place_orders =
            PlaceOrdersBinding::implemented::<ExtendedVenue>(|v, o| v.place_orders(o));
    }
    ExtendedVenue {
        inner,
        validated: RefCell::new(vec![]),
        sent: RefCell::new(vec![]),
    }
}
fn extended_order(id: &str, execution: ExecutionMode) -> ExtendedOptions {
    ExtendedOptions {
        shared: order(id),
        execution,
    }
}
#[test]
fn full_venue_options_survive_typed_and_json_dyn_placement() {
    let venue = extended_venue(false);
    let erased: &dyn Exchange = &venue;
    let bbo = ExecutionMode::Bbo { level: 3 };
    ready(
        venue
            .base()
            .place_order(erased, extended_order("good-bbo", bbo.clone())),
    )
    .unwrap();
    let mut json_order = serde_json::to_value(order("good-ticks")).unwrap();
    json_order["bybitSlippageToleranceTicks"] = json!(9);
    ready(venue.base().place_order_json(erased, json_order)).unwrap();
    ready(
        venue
            .base()
            .place_order_with_client_order_id(erased, extended_order("good-ref", bbo.clone())),
    )
    .unwrap();
    assert_eq!(
        *venue.validated.borrow(),
        vec![
            bbo.clone(),
            ExecutionMode::TickSize { ticks: 9 },
            bbo.clone()
        ]
    );
    assert_eq!(
        *venue.sent.borrow(),
        vec![bbo.clone(), ExecutionMode::TickSize { ticks: 9 }, bbo]
    );
    let other = self::venue();
    assert!(
        ready(other.base.place_order(
            &other,
            extended_order("good-wrong", ExecutionMode::Bbo { level: 2 })
        ))
        .is_err()
    );
    assert!(
        other.writes.borrow().is_empty(),
        "never project an unrelated typed extension into shared opts"
    );
}
#[test]
fn every_batch_extension_is_validated_before_sequential_or_native_write() {
    for bulk in [false, true] {
        let venue = extended_venue(bulk);
        let inputs = vec![
            extended_order("good-first", ExecutionMode::Bbo { level: 2 }),
            extended_order("good-last", ExecutionMode::Bbo { level: 0 }),
        ];
        assert!(ready(venue.base().place_orders(&venue, inputs)).is_err());
        assert!(venue.sent.borrow().is_empty());
        assert_eq!(venue.validated.borrow().len(), 2);
        venue.validated.borrow_mut().clear();
        let mut a = serde_json::to_value(order("good-first")).unwrap();
        a["bybitBbo"] = json!({"level":2});
        let mut b = serde_json::to_value(order("good-last")).unwrap();
        b["bybitBbo"] = json!({"level":0});
        assert!(ready(venue.base().place_orders_json(&venue, vec![a.clone(), b])).is_err());
        assert!(venue.sent.borrow().is_empty());
        ready(venue.base().place_orders_json(&venue, vec![a])).unwrap();
        assert_eq!(*venue.sent.borrow(), vec![ExecutionMode::Bbo { level: 2 }]);
    }
}
#[test]
fn malformed_or_conflicting_json_extensions_cannot_fall_back_to_shared_options() {
    let venue = extended_venue(false);
    for extension in [
        json!({"bybitBbo":null}),
        json!({"bybitSlippageToleranceTicks":null}),
        json!({"bybitBbo":{"level":1},"bybitSlippageToleranceTicks":2}),
    ] {
        let mut input = serde_json::to_value(order("good-input")).unwrap();
        input
            .as_object_mut()
            .unwrap()
            .extend(extension.as_object().unwrap().clone());
        assert!(ready(venue.base().place_order_json(&venue, input.clone())).is_err());
        assert!(
            ready(venue.base().place_orders_json(
                &venue,
                vec![serde_json::to_value(order("good-first")).unwrap(), input]
            ))
            .is_err()
        );
    }
    assert!(venue.sent.borrow().is_empty());
}
#[test]
fn generic_nuke_converts_shared_order_into_plain_venue_options() {
    let venue = extended_venue(false);
    venue.base().store.add_or_update_position(serde_json::from_value(json!({"id":"BTC","symbol":"BTC","side":"long","entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap()).unwrap();
    let dynamic: &dyn Exchange = &venue;
    ready(dynamic.nuke_positions(0)).unwrap();
    assert_eq!(*venue.validated.borrow(), vec![ExecutionMode::Plain]);
    assert_eq!(*venue.sent.borrow(), vec![ExecutionMode::Plain]);
}
struct RestrictedVenue(ExtendedVenue);
impl Exchange for RestrictedVenue {
    type PlaceOpts = ExtendedOptions;
    fn base(&self) -> &BaseExchange {
        self.0.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        self.0.capabilities()
    }
    fn validate_order_capabilities(&self, options: &ExtendedOptions) -> Result<(), ExchangeError> {
        if options.execution != ExecutionMode::Plain {
            return Err(ExchangeError::new(
                "family variant forbids execution extensions",
            ));
        }
        Ok(())
    }
}
#[test]
fn family_variant_has_its_own_typed_capability_hook() {
    let mut restricted = RestrictedVenue(extended_venue(false));
    restricted.0.inner.capabilities.place_order =
        PlaceOrderBinding::implemented::<RestrictedVenue>(|venue, _| {
            venue.0.sent.borrow_mut().push(ExecutionMode::Plain);
            Box::pin(async { Ok(vec![]) })
        });
    let normal = extended_venue(false);
    let input = extended_order("good-bbo", ExecutionMode::Bbo { level: 1 });
    ready(normal.base().place_order(&normal, input.clone())).unwrap();
    assert!(
        ready(restricted.base().place_orders(
            &restricted,
            vec![ExtendedOptions::from(order("good-first")), input]
        ))
        .is_err()
    );
    assert!(restricted.0.sent.borrow().is_empty());
}

#[test]
fn dynamic_leverage_hook_preserves_venue_override_and_default_store_commit() {
    struct DefaultLeverage {
        base: BaseExchange,
        caps: ExchangeCapabilities,
    }
    impl Exchange for DefaultLeverage {
        type PlaceOpts = PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            &self.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.caps
        }
        fn fetch_symbol_max_leverage(&self, symbol: String) -> ExchangeFuture<'_, f64> {
            Box::pin(async move {
                assert_eq!(symbol, "BTC");
                Ok(25.0)
            })
        }
    }
    struct OverrideLeverage(DefaultLeverage);
    impl Exchange for OverrideLeverage {
        type PlaceOpts = PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            &self.0.base
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            &self.0.caps
        }
        fn fetch_symbol_max_leverage(&self, _: String) -> ExchangeFuture<'_, f64> {
            panic!("dynamic leverage lookup must not bypass venue override")
        }
        fn fetch_leverage_for_symbol(
            &self,
            symbol: String,
        ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
            Box::pin(async move {
                if symbol == "reject" {
                    return Err(ExchangeError::new("venue refusal"));
                }
                Ok(Some(FetchedLeverage {
                    buy_leverage: 7.0,
                    sell_leverage: 9.0,
                    margin_type: None,
                    position_mode: None,
                    is_fetching: None,
                }))
            })
        }
    }
    fn make() -> DefaultLeverage {
        DefaultLeverage {
            base: BaseExchange::new(
                ExchangeName::Binance,
                serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
                Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
                Rc::new(Runtime(Cell::new(0))),
                ExchangeSettings::default(),
            ),
            caps: ExchangeCapabilities::default(),
        }
    }
    let default = make();
    let dynamic: &dyn Exchange = &default;
    let value = ready(dynamic.fetch_leverage_for_symbol("BTC".into()))
        .unwrap()
        .expect("default leverage hook must return base result");
    assert_eq!(
        value.buy_leverage, 25.0,
        "default leverage must dispatch max-leverage hook"
    );
    assert_eq!(
        default.base.store.fetched_leverage().unwrap()["BTC"].sell_leverage,
        25.0,
        "default leverage must commit to production Store"
    );
    let venue = OverrideLeverage(make());
    let dynamic: &dyn Exchange = &venue;
    let value = ready(dynamic.fetch_leverage_for_symbol("BTC".into()))
        .unwrap()
        .unwrap();
    assert_eq!((value.buy_leverage, value.sell_leverage), (7.0, 9.0));
    assert!(venue.base().store.fetched_leverage().unwrap().is_empty());
    assert_eq!(
        ready(dynamic.fetch_leverage_for_symbol("reject".into()))
            .unwrap_err()
            .message,
        "venue refusal"
    );
}

#[test]
fn destructive_and_read_orchestration_dispatches_through_dyn_venue() {
    struct Override(Venue, RefCell<Vec<&'static str>>);
    impl Exchange for Override {
        type PlaceOpts = PlaceOrderOpts;
        fn base(&self) -> &BaseExchange {
            self.0.base()
        }
        fn capabilities(&self) -> &ExchangeCapabilities {
            self.0.capabilities()
        }
        fn nuke(&self, tries: u32) -> ExchangeFuture<'_, ()> {
            assert_eq!(tries, 2);
            self.1.borrow_mut().push("nuke");
            Box::pin(async { Ok(()) })
        }
        fn nuke_positions(&self, tries: u32) -> ExchangeFuture<'_, ()> {
            assert_eq!(tries, 1);
            self.1.borrow_mut().push("positions");
            Box::pin(async { Ok(()) })
        }
        fn dispose(&self) -> Result<(), ExchangeError> {
            self.1.borrow_mut().push("dispose");
            Ok(())
        }
        fn set_all_leverage(&self, leverage: f64) -> ExchangeFuture<'_, ()> {
            assert_eq!(leverage, 4.0);
            self.1.borrow_mut().push("leverage");
            Box::pin(async { Ok(()) })
        }
        fn find_order_by_client_order_id(
            &self,
            symbol: String,
            id: String,
        ) -> ExchangeFuture<'_, ClientOrderLookup> {
            assert_eq!((symbol.as_str(), id.as_str()), ("BTC", "caller"));
            self.1.borrow_mut().push("lookup");
            Box::pin(async { Ok(ClientOrderLookup::NotFound { definitive: true }) })
        }
        fn refresh_markets(
            &self,
        ) -> ExchangeFuture<'_, safe_cex_exchange::snapshots::MarketChanges> {
            self.1.borrow_mut().push("markets");
            Box::pin(async { Err(ExchangeError::new("venue refresh refusal")) })
        }
    }
    let venue = Override(venue(), RefCell::new(vec![]));
    let dynamic: &dyn Exchange = &venue;
    ready(dynamic.nuke(2)).unwrap();
    ready(dynamic.nuke_positions(1)).unwrap();
    dynamic.dispose().unwrap();
    assert!(
        !dynamic.base().is_disposed(),
        "venue teardown override must not be bypassed"
    );
    ready(dynamic.set_all_leverage(4.0)).unwrap();
    assert!(matches!(
        ready(dynamic.find_order_by_client_order_id("BTC".into(), "caller".into())).unwrap(),
        ClientOrderLookup::NotFound { definitive: true }
    ));
    assert_eq!(
        ready(dynamic.refresh_markets())
            .err()
            .expect("venue refresh must refuse")
            .message,
        "venue refresh refusal"
    );
    assert_eq!(
        *venue.1.borrow(),
        vec![
            "nuke",
            "positions",
            "dispose",
            "leverage",
            "lookup",
            "markets"
        ],
        "all source orchestration overrides must dispatch dynamically"
    );
}
