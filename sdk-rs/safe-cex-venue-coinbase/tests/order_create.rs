use indexmap::IndexMap;
use safe_cex_types::{Market, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_venue_coinbase::{order_create::*, starbase_auth::CoinbaseStarbasePromise, types::*};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};
const FIXTURE: &str = include_str!("fixtures/order-create.json");
const HASH: &str = "9be7b6c0ed22d306f089ca644551fabd526716d0ade50893a70821589018358e";
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        assert_eq!(
            format!("{:x}", Sha256::digest(FIXTURE)),
            HASH,
            "independent order corpus content"
        );
        let result = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/order-create.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "live order oracle: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    })
}
fn number(v: &Value) -> f64 {
    if let Some(s) = v.get("$n").and_then(Value::as_str) {
        match s {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            "-0" => -0.0,
            _ => panic!("unknown number"),
        }
    } else {
        v.as_f64().unwrap()
    }
}
fn assert_number(actual: f64, expected: &Value, label: &str) {
    let bits = u64::from_str_radix(expected["$bits"].as_str().unwrap(), 16).unwrap();
    if f64::from_bits(bits).is_nan() {
        assert!(actual.is_nan(), "{label}: decoded native number");
    } else {
        assert_eq!(
            actual.to_bits(),
            bits,
            "{label}: decoded native number bits"
        );
    }
}
fn assert_json(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected.get("$undefined") == Some(&Value::Bool(true)) {
        assert!(actual.is_none(), "{label}: native undefined");
        return;
    }
    let actual = actual.expect("expected native field");
    if expected.get("$bits").is_some() {
        assert_number(actual.as_f64().unwrap(), expected, label);
        return;
    }
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "{label}: native own field count");
            for (k, v) in e {
                assert_json(a.get(k), v, label);
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "{label}: native array count");
            for (a, e) in a.iter().zip(e) {
                assert_json(Some(a), e, label);
            }
        }
        _ => assert_eq!(actual, expected, "{label}: decoded native JSON value"),
    }
}
fn options(v: &Value) -> PlaceOrderOpts {
    let mut clean = v.clone();
    for k in ["amount", "price"] {
        if clean.get(k).is_some_and(Value::is_object) {
            clean[k] = json!(0);
        }
    }
    let mut opts: PlaceOrderOpts = serde_json::from_value(clean).unwrap();
    opts.amount = number(&v["amount"]);
    opts.price = v.get("price").map(number);
    opts
}
fn order(v: &Value) -> Rc<Order> {
    let mut raw = json!({"id":v["id"],"status":v["status"],"symbol":"BTC-USDC","type":"limit","side":"buy","price":17.25,"amount":3.5,"filled":0,"remaining":3.5,"reduceOnly":false,"lastUpdate":0});
    let mut result: Order = serde_json::from_value(raw.take()).unwrap();
    result.last_update = number(&v["lastUpdate"]);
    Rc::new(result)
}
fn cancel_input(n: usize) -> Vec<Rc<Order>> {
    (0..n)
        .map(|i| order(&json!({"id":format!("order-{i}"),"status":"open","lastUpdate":0})))
        .collect()
}
#[derive(Clone, Debug)]
enum Event {
    Marker(&'static str),
    Clock,
    Loaded,
    Orders,
    Market(String),
    Markets,
    Resolve(usize),
    Uuid,
    Create(&'static str, PlaceOrderRequest),
    Cancel(&'static str, Vec<String>),
    Add(Rc<CoinbaseOptimisticOrder>),
    Update(Vec<String>, OrderStatus),
    Reconcile(&'static str),
    Write,
    Warning(&'static str),
    Emit(Vec<Option<Value>>),
}
struct Host {
    input: Value,
    events: Rc<RefCell<Vec<Event>>>,
    markets: IndexMap<String, Rc<Market>>,
    stored: Vec<Rc<Order>>,
    resolves: usize,
    cancels: usize,
    updates: usize,
}
impl Host {
    fn new(input: &Value) -> Self {
        let keys = input
            .get("marketKeys")
            .cloned()
            .unwrap_or(json!(["BTC-USD", "BTC-USDC"]));
        let markets=keys.as_array().unwrap().iter().map(|id| {
            let id=id.as_str().unwrap();let mut value=json!({"id":id,"symbol":id,"base":"BTC","quote":if id.ends_with("USDC"){"USDC"}else{"USD"},"active":true,"precision":{"amount":0.001,"price":0.05},"limits":{"amount":{"min":0.001,"max":100000},"leverage":{"min":1,"max":10}},"isSpot":true,"isExpiring":false,"contractSize":1});
            if let Some(patch)=input.get("marketPatch").and_then(Value::as_object){value.as_object_mut().unwrap().extend(patch.clone());}
            if let Some(key)=input.get("marketRemove").and_then(Value::as_str){value.as_object_mut().unwrap().shift_remove(key);}
            (id.into(),Rc::new(serde_json::from_value(value).unwrap()))
        }).collect();
        Self {
            input: input.clone(),
            events: Rc::new(RefCell::new(Vec::new())),
            markets,
            stored: input
                .get("stored")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(order).collect())
                .unwrap_or_default(),
            resolves: 0,
            cancels: 0,
            updates: 0,
        }
    }
    fn record(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
    fn fail(&self, at: &str) -> Result<(), CoinbaseOrderFailure> {
        if self.input.get("throwAt").and_then(Value::as_str) == Some(at) {
            Err(CoinbaseOrderFailure {
                value: self
                    .input
                    .get("hostError")
                    .or_else(|| self.input.get("error"))
                    .cloned(),
                native: None,
            })
        } else {
            Ok(())
        }
    }
}
impl CoinbaseOrderHost for Host {
    fn now_ms(&mut self) -> f64 {
        self.record(Event::Clock);
        self.input.get("now").map(number).unwrap_or(10000.0)
    }
    fn orders_loaded(
        &mut self,
    ) -> std::result::Result<bool, safe_cex_venue_coinbase::order_create::CoinbaseOrderFailure>
    {
        self.record(Event::Loaded);
        Ok(self
            .input
            .get("loaded")
            .and_then(Value::as_bool)
            .unwrap_or(false))
    }
    fn orders(
        &mut self,
    ) -> std::result::Result<
        Vec<Rc<Order>>,
        safe_cex_venue_coinbase::order_create::CoinbaseOrderFailure,
    > {
        self.record(Event::Orders);
        Ok(self.stored.clone())
    }
    fn market_by_symbol(
        &mut self,
        symbol: &str,
    ) -> std::result::Result<
        Option<Rc<safe_cex_types::Market>>,
        safe_cex_venue_coinbase::order_create::CoinbaseOrderFailure,
    > {
        self.record(Event::Market(symbol.into()));
        Ok(self.markets.get(symbol).cloned())
    }
    fn markets_by_id(
        &mut self,
    ) -> std::result::Result<
        &IndexMap<std::string::String, Rc<safe_cex_types::Market>>,
        safe_cex_venue_coinbase::order_create::CoinbaseOrderFailure,
    > {
        self.record(Event::Markets);
        Ok(&self.markets)
    }
    fn resolve_caller_client_order_id(
        &mut self,
        opts: &safe_cex_types::PreparedOrderOpts,
    ) -> Result<Option<String>, CoinbaseOrderFailure> {
        self.resolves += 1;
        self.record(Event::Resolve(self.resolves));
        self.fail(&format!("resolve{}", self.resolves))?;
        Ok(opts.client_order_id.clone())
    }
    fn uuid(&mut self) -> String {
        self.record(Event::Uuid);
        "uuid-17".into()
    }
    fn post_create_order(
        &mut self,
        endpoint: &'static str,
        payload: &PlaceOrderRequest,
    ) -> CoinbaseStarbasePromise<Result<CoinbaseOrderResponse, CoinbaseOrderFailure>> {
        self.record(Event::Create(endpoint, payload.clone()));
        let result = self.fail("postCreate").map(|()| {
            CoinbaseOrderResponse::from_json(
                self.input
                    .get("response")
                    .cloned()
                    .unwrap_or(json!({"success":true,"order_id":"created-23"}))
                    .as_object()
                    .unwrap()
                    .clone(),
            )
        });
        CoinbaseStarbasePromise::resolved(result)
    }
    fn post_cancel_orders(
        &mut self,
        endpoint: &'static str,
        ids: &[String],
    ) -> CoinbaseStarbasePromise<Result<(), CoinbaseOrderFailure>> {
        self.cancels += 1;
        self.record(Event::Cancel(endpoint, ids.to_vec()));
        CoinbaseStarbasePromise::resolved(self.fail(&format!("postCancel{}", self.cancels)))
    }
    fn add_or_update_order(
        &mut self,
        order: Rc<CoinbaseOptimisticOrder>,
    ) -> Result<(), CoinbaseOrderFailure> {
        self.record(Event::Add(order));
        self.fail("add")
    }
    fn update_orders_status(
        &mut self,
        ids: &[String],
        status: OrderStatus,
    ) -> Result<(), CoinbaseOrderFailure> {
        self.updates += 1;
        self.record(Event::Update(ids.to_vec(), status));
        self.fail(&format!("update{}", self.updates))
    }
    fn schedule_open_orders_rest_reconciliation(
        &mut self,
        reason: &'static str,
    ) -> Result<(), CoinbaseOrderFailure> {
        self.record(Event::Reconcile(reason));
        self.fail("reconcile")
    }
    fn schedule_private_user_stream_write_check(&mut self) -> Result<(), CoinbaseOrderFailure> {
        self.record(Event::Write);
        self.fail("write")
    }
    fn log_warning(&mut self, message: &'static str) -> Result<(), CoinbaseOrderFailure> {
        self.record(Event::Warning(message));
        self.fail("warning")
    }
    fn emit_and_log_error(&mut self, args: &[Option<&Value>]) -> Result<(), CoinbaseOrderFailure> {
        self.record(Event::Emit(args.iter().map(|v| v.cloned()).collect()));
        self.fail("emit")
    }
}
fn assert_payload(a: &PlaceOrderRequest, e: &Value, label: &str) {
    assert_eq!(
        a.client_order_id,
        e["client_order_id"].as_str().unwrap(),
        "{label}: native caller ID"
    );
    assert_eq!(a.product_id, e["product_id"].as_str().unwrap());
    assert_eq!(
        a.side,
        if e["side"] == "BUY" {
            PlaceOrderRequestSide::Buy
        } else {
            PlaceOrderRequestSide::Sell
        },
        "{label}: native request side"
    );
    assert_eq!(a.leverage.as_deref(), Some("10"));
    assert!(
        a.margin_type.is_none()
            && a.retail_portfolio_id.is_none()
            && a.self_trade_prevention_id.is_none()
    );
    let c = &a.order_configuration;
    let e = &e["order_configuration"];
    assert_eq!(
        c.market_market_ioc.is_some(),
        e.get("market_market_ioc").is_some(),
        "{label}: decoded native create configuration"
    );
    assert_eq!(
        c.limit_limit_gtc.is_some(),
        e.get("limit_limit_gtc").is_some(),
        "{label}: decoded native create configuration"
    );
    assert_eq!(
        c.stop_limit_stop_limit_gtc.is_some(),
        e.get("stop_limit_stop_limit_gtc").is_some(),
        "{label}: decoded native create configuration"
    );
    assert!(
        c.sor_limit_ioc.is_none()
            && c.limit_limit_gtd.is_none()
            && c.stop_limit_stop_limit_gtd.is_none()
            && c.trigger_bracket_gtc.is_none()
            && c.trigger_bracket_gtd.is_none()
    );
    if let Some(v) = &c.market_market_ioc {
        assert_eq!(
            v.base_size,
            e["market_market_ioc"]["base_size"].as_str().unwrap(),
            "{label}: native amount precision"
        );
        assert!(v.quote_size.is_none());
    }
    if let Some(v) = &c.limit_limit_gtc {
        let e = &e["limit_limit_gtc"];
        assert_eq!(
            v.base_size,
            e["base_size"].as_str().unwrap(),
            "{label}: native amount precision"
        );
        assert_eq!(
            v.limit_price,
            e["limit_price"].as_str().unwrap(),
            "{label}: native price precision"
        );
        assert_eq!(v.post_only, e["post_only"].as_bool());
    }
    if let Some(v) = &c.stop_limit_stop_limit_gtc {
        let e = &e["stop_limit_stop_limit_gtc"];
        assert_eq!(v.base_size, e["base_size"].as_str().unwrap());
        assert_eq!(v.limit_price, e["limit_price"].as_str().unwrap());
        assert_eq!(v.stop_price, e["stop_price"].as_str().unwrap());
        assert_eq!(
            v.stop_direction,
            if e["stop_direction"] == "STOP_DIRECTION_STOP_UP" {
                StopDirection::StopDirectionStopUp
            } else {
                StopDirection::StopDirectionStopDown
            },
            "{label}: native stop direction"
        );
    }
}
fn assert_optimistic(a: &CoinbaseOptimisticOrder, e: &Value, label: &str) {
    assert_eq!(a.id, e["id"].as_str().unwrap());
    assert_eq!(a.parent_id, e["parentId"].as_str().unwrap());
    assert_eq!(a.status, OrderStatus::Open);
    assert_eq!(a.r#type, OrderType::Limit);
    assert_eq!(
        a.symbol,
        e["symbol"].as_str().unwrap(),
        "{label}: native optimistic market"
    );
    assert_eq!(
        a.side,
        if e["side"] == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        }
    );
    for (a, key) in [
        (a.price, "price"),
        (a.amount, "amount"),
        (a.filled, "filled"),
        (a.remaining, "remaining"),
        (a.last_update, "lastUpdate"),
    ] {
        assert_number(a, &e[key], label);
    }
    assert_number(
        a.extra.remaining_native.unwrap(),
        &e["extra"]["remainingNative"],
        label,
    );
    assert!(!a.reduce_only);
    assert_eq!(
        a.info.fields.borrow().len(),
        e["info"].as_object().unwrap().len()
    );
    for (k, v) in e["info"].as_object().unwrap() {
        assert_json(
            Some(
                &a.info.fields.borrow()[&k.encode_utf16().collect::<Vec<_>>()]
                    .borrow()
                    .clone()
                    .into_value()
                    .unwrap(),
            ),
            v,
            label,
        );
    }
}
fn trace(actual: &[Event], expected: &Value, label: &str) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label}: native side effect count"
    );
    for (a, e) in actual.iter().zip(expected) {
        let kind = match a {
            Event::Marker(_) => "marker",
            Event::Clock => "clock",
            Event::Loaded => "loaded",
            Event::Orders => "orders",
            Event::Market(_) => "market",
            Event::Markets => "markets",
            Event::Resolve(_) => "resolve",
            Event::Uuid => "uuid",
            Event::Create(..) => "create",
            Event::Cancel(..) => "cancel",
            Event::Add(_) => "add",
            Event::Update(..) => "update",
            Event::Reconcile(_) => "reconcile",
            Event::Write => "write",
            Event::Warning(_) => "warning",
            Event::Emit(_) => "emit",
        };
        assert_eq!(
            kind,
            e["kind"].as_str().unwrap(),
            "{label}: native side effect order"
        );
        match a {
            Event::Marker(name) => assert_eq!(*name, e["name"].as_str().unwrap()),
            Event::Market(s) => assert_eq!(s, e["symbol"].as_str().unwrap()),
            Event::Resolve(n) => assert_number(*n as f64, &e["ordinal"], label),
            Event::Create(endpoint, payload) => {
                assert_eq!(*endpoint, e["endpoint"].as_str().unwrap());
                assert_payload(payload, &e["payload"], label);
            }
            Event::Cancel(endpoint, ids) => {
                assert_eq!(*endpoint, e["endpoint"].as_str().unwrap());
                assert_eq!(
                    ids,
                    &e["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>(),
                    "{label}: native cancellation batch"
                );
            }
            Event::Update(ids, status) => {
                assert_eq!(
                    *status,
                    OrderStatus::Canceled,
                    "{label}: native cancellation status"
                );
                assert_eq!(
                    ids,
                    &e["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                );
            }
            Event::Add(order) => assert_optimistic(order, &e["order"], label),
            Event::Reconcile(reason) => assert_eq!(
                *reason,
                e["reason"].as_str().unwrap(),
                "{label}: native reconciliation reason"
            ),
            Event::Warning(message) => {
                assert_eq!(*message, e["message"].as_str().unwrap());
                assert_eq!(e["severity"], "warning");
            }
            Event::Emit(args) => {
                let expected = e["args"].as_array().unwrap();
                assert_eq!(
                    args.len(),
                    expected.len(),
                    "{label}: native error argument count"
                );
                for (a, e) in args.iter().zip(expected) {
                    assert_json(a.as_ref(), e, label);
                }
            }
            _ => {}
        }
    }
}
fn outcome<T>(actual: &Result<T, CoinbaseOrderFailure>, expected: &Value, label: &str) {
    match actual {
        Ok(_) => assert!(
            expected["error"].is_null(),
            "{label}: source unexpected failure: {}",
            expected["error"]
        ),
        Err(err) => {
            assert!(
                !expected["error"].is_null(),
                "{label}: native unexpected failure {err:?}"
            );
            assert_json(err.value.as_ref(), &expected["error"], label);
        }
    }
}
fn cases(family: &str, expected: usize) -> Vec<(Value, Value)> {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    let inputs = fixture[family].as_array().unwrap();
    let outputs = oracle()[family].as_array().unwrap();
    assert_eq!(inputs.len(), expected, "independent input family floor");
    assert_eq!(outputs.len(), expected, "independent source family floor");
    inputs
        .iter()
        .cloned()
        .zip(outputs.iter().cloned())
        .collect()
}
#[test]
fn native_create_payload_matches_live_source() {
    for (input, e) in cases("format", 42) {
        let mut h = Host::new(&input);
        let a = format_create_order(
            &mut h,
            &safe_cex_types::PreparedOrderOpts::from(&options(&input["opts"])),
        );
        let label = input["label"].as_str().unwrap();
        outcome(&a, &e, label);
        if let Ok(a) = a {
            assert_payload(&a, &e["result"], label);
        }
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_optimistic_order_matches_live_source() {
    for (input, e) in cases("optimistic", 31) {
        let mut h = Host::new(&input);
        let payload: PlaceOrderRequest = serde_json::from_value(input["payload"].clone()).unwrap();
        let response =
            CoinbaseOrderResponse::from_json(input["response"].as_object().unwrap().clone());
        let a = build_optimistic_order_from_create(&mut h, &payload, "optimistic-19", &response);
        let label = input["label"].as_str().unwrap();
        outcome(&a, &e, label);
        if let Ok(a) = a {
            assert_eq!(
                a.is_none(),
                e["result"].get("$undefined").is_some(),
                "{label}: decoded native optimistic presence"
            );
            if let Some(a) = a {
                assert_optimistic(&a, &e["result"], label);
            }
        }
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_place_order_matches_live_source() {
    for (input, e) in cases("place", 18) {
        let h = Rc::new(RefCell::new(Host::new(&input)));
        let tasks = Microtasks::default();
        let events = h.borrow().events.clone();
        let opts = options(&input["opts"]);
        let pending = place_order(h.clone(), opts, &tasks.spawner());
        let label = input["label"].as_str().unwrap();
        trace(&events.borrow(), &e["initialTrace"], label);
        tasks.drain();
        let a = completed(pending);
        outcome(&a, &e, label);
        if let Ok(a) = a {
            assert_eq!(
                a,
                e["result"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "{label}: decoded native placement IDs"
            );
        }
        trace(&h.borrow().events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_cancel_order_matches_live_source() {
    for (input, e) in cases("cancel", 8) {
        let h = Rc::new(RefCell::new(Host::new(&input)));
        let tasks = Microtasks::default();
        let events = h.borrow().events.clone();
        let orders = cancel_input(input["count"].as_u64().unwrap() as usize);
        let pending = cancel_orders(h.clone(), orders, &tasks.spawner());
        let label = input["label"].as_str().unwrap();
        trace(&events.borrow(), &e["initialTrace"], label);
        tasks.drain();
        let a = completed(pending);
        outcome(&a, &e, label);
        trace(&h.borrow().events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_poll_order_matches_live_source() {
    for (input, e) in cases("poll", 8) {
        let mut h = Host::new(&input);
        let mut state = CoinbaseOrderPolling {
            next_orders_poll_at: number(&input["next"]),
        };
        let a = should_poll_orders(&mut state, &mut h).unwrap();
        let label = input["label"].as_str().unwrap();
        assert_eq!(
            a,
            e["result"].as_bool().unwrap(),
            "{label}: decoded native poll decision"
        );
        assert_number(state.next_orders_poll_at, &e["next"], label);
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_rest_merge_matches_live_source() {
    for (input, e) in cases("merge", 7) {
        let mut h = Host::new(&input);
        let rest = Rc::new(
            input["rest"]
                .as_array()
                .unwrap()
                .iter()
                .map(order)
                .collect::<Vec<_>>(),
        );
        let a = merge_rest_orders_with_fresh_ws_state(&mut h, rest.clone()).unwrap();
        let label = input["label"].as_str().unwrap();
        let expected = e["result"].as_array().unwrap();
        assert_eq!(
            a.len(),
            expected.len(),
            "{label}: decoded native merged order count"
        );
        assert_eq!(
            Rc::ptr_eq(&a, &rest),
            e["identity"].as_bool().unwrap(),
            "{label}: native REST array identity"
        );
        for (a, e) in a.iter().zip(expected) {
            assert_eq!(
                a.id,
                e["id"].as_str().unwrap(),
                "{label}: native merged order ID"
            );
            assert_number(a.last_update, &e["lastUpdate"], label);
            assert!(
                rest.iter()
                    .chain(&h.stored)
                    .any(|input| Rc::ptr_eq(a, input)),
                "native merged order handle alias"
            );
        }
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_fetch_error_matches_live_source() {
    for (input, e) in cases("fetchError", 8) {
        let mut h = Host::new(&input);
        let mut state = CoinbaseOrderPolling {
            next_orders_poll_at: number(&input["next"]),
        };
        let a = emit_orders_fetch_error(&mut state, &mut h, input.get("error").cloned());
        let label = input["label"].as_str().unwrap();
        outcome(&a, &e, label);
        assert_number(state.next_orders_poll_at, &e["next"], label);
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_market_resolver_matches_live_source() {
    for (input, e) in cases("resolve", 7) {
        let mut h = Host::new(&input);
        let a = resolve_market_by_coinbase_product_id(&mut h, input["product"].as_str().unwrap())
            .unwrap();
        let label = input["label"].as_str().unwrap();
        assert_eq!(
            a.is_none(),
            e["result"].get("$undefined").is_some(),
            "{label}: native market presence"
        );
        if let Some(a) = a {
            assert_eq!(
                a.id,
                e["result"]["id"].as_str().unwrap(),
                "{label}: decoded native alias market"
            );
            assert!(
                Rc::ptr_eq(&a, &h.markets[&a.id]),
                "native market handle alias"
            );
        }
        trace(&h.events.borrow(), &e["trace"], label);
    }
}
#[test]
fn native_optimistic_info_retains_shallow_aliases_and_primitive_copies() {
    let input = json!({});
    let mut h = Host::new(&input);
    let payload:PlaceOrderRequest=serde_json::from_value(json!({"client_order_id":"caller","product_id":"BTC-USDC","side":"BUY","order_configuration":{"limit_limit_gtc":{"base_size":"3","limit_price":"17"}}})).unwrap();
    let response = CoinbaseOrderResponse::from_json(
        json!({"object":{"x":1},"array":[2],"primitive":3,"optimisticFromCreateOrder":false})
            .as_object()
            .unwrap()
            .clone(),
    );
    let a = build_optimistic_order_from_create(&mut h, &payload, "id", &response)
        .unwrap()
        .unwrap();
    for key in ["object", "array"] {
        assert!(
            Rc::ptr_eq(
                &a.info.fields.borrow()[&key.encode_utf16().collect::<Vec<_>>()],
                &response.fields.borrow()[&key.encode_utf16().collect::<Vec<_>>()]
            ),
            "native optimistic info object alias"
        );
    }
    assert!(
        !Rc::ptr_eq(
            &a.info.fields.borrow()[&"primitive".encode_utf16().collect::<Vec<_>>()],
            &response.fields.borrow()[&"primitive".encode_utf16().collect::<Vec<_>>()]
        ),
        "native optimistic info primitive copy"
    );
    *response.fields.borrow()[&"object".encode_utf16().collect::<Vec<_>>()]
        .borrow_mut()
        .get_mut("x")
        .unwrap() = json!(19).into();
    if let Input::Array(values) =
        &mut *response.fields.borrow()[&"array".encode_utf16().collect::<Vec<_>>()].borrow_mut()
    {
        values.push(json!(23).into());
    } else {
        panic!("native retained array");
    }
    assert_eq!(
        a.info.fields.borrow()[&"object".encode_utf16().collect::<Vec<_>>()]
            .borrow()
            .get("x")
            .and_then(Input::as_f64)
            .unwrap(),
        19.0
    );
    assert_eq!(
        a.info.fields.borrow()[&"array".encode_utf16().collect::<Vec<_>>()]
            .borrow()
            .as_array()
            .unwrap()[1]
            .as_f64()
            .unwrap(),
        23.0
    );
    response.fields.borrow_mut().insert(
        "object".encode_utf16().collect(),
        Rc::new(RefCell::new(json!({"x":29}).into())),
    );
    response.fields.borrow_mut().insert(
        "primitive".encode_utf16().collect(),
        Rc::new(RefCell::new(json!(31).into())),
    );
    assert_eq!(
        a.info.fields.borrow()[&"object".encode_utf16().collect::<Vec<_>>()]
            .borrow()
            .get("x")
            .and_then(Input::as_f64)
            .unwrap(),
        19.0,
        "native shallow alias survives parent property replacement"
    );
    let source = &oracle()["alias"];
    assert!(
        !Rc::ptr_eq(&a.info.fields, &response.fields),
        "native shallow info root is distinct"
    );
    assert!(
        Rc::ptr_eq(&response.clone().fields, &response.fields),
        "native response clone retains root identity"
    );
    assert_eq!(
        source["identities"],
        json!({"object":true,"array":true,"root":false})
    );
    for (k, v) in source["info"].as_object().unwrap() {
        assert_json(
            Some(
                &a.info.fields.borrow()[&k.encode_utf16().collect::<Vec<_>>()]
                    .borrow()
                    .clone()
                    .into_value()
                    .unwrap(),
            ),
            v,
            "live shallow info alias probe",
        );
    }
    assert_eq!(
        *a.info.fields.borrow()[&"primitive".encode_utf16().collect::<Vec<_>>()].borrow(),
        Input::from(json!(3))
    );
    assert_eq!(
        *a.info.fields.borrow()[&"optimisticFromCreateOrder"
            .encode_utf16()
            .collect::<Vec<_>>()]
            .borrow(),
        Input::Bool(true)
    );
    assert_eq!(
        *response.fields.borrow()[&"optimisticFromCreateOrder"
            .encode_utf16()
            .collect::<Vec<_>>()]
            .borrow(),
        Input::Bool(false)
    );
}

#[test]
fn native_unawaited_order_promises_continue_like_source() {
    for (family, index) in [("place", 0), ("cancel", 3)] {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let input = &fixture[family][index];
        let expected = oracle()["hot"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["family"] == family)
            .unwrap();
        let h = Rc::new(RefCell::new(Host::new(input)));
        let tasks = Microtasks::default();
        if family == "place" {
            let pending = place_order(h.clone(), options(&input["opts"]), &tasks.spawner());
            assert!(
                microtasks::is_pending(&mut pending.clone()),
                "native hot create initial await"
            );
            drop(pending);
        } else {
            let pending = cancel_orders(h.clone(), cancel_input(101), &tasks.spawner());
            assert!(
                microtasks::is_pending(&mut pending.clone()),
                "native hot cancel initial await"
            );
            drop(pending);
        }
        trace(
            &h.borrow().events.borrow(),
            &expected["initialTrace"],
            family,
        );
        tasks.drain();
        assert!(
            matches!(h.borrow().events.borrow().last(), Some(Event::Write)),
            "native unawaited producer continues through write check"
        );
        trace(&h.borrow().events.borrow(), &expected["trace"], family);
    }
}
#[test]
fn native_cancel_microtask_order_matches_actual_p_iteration() {
    assert_eq!(
        oracle()["timings"].as_array().unwrap().len(),
        2,
        "independent cancellation timing floor"
    );
    for reject in [false, true] {
        let input = if reject {
            json!({"throwAt":"postCancel1","error":{"message":"failed"}})
        } else {
            json!({})
        };
        let h = Rc::new(RefCell::new(Host::new(&input)));
        let tasks = Microtasks::default();
        let events = h.borrow().events.clone();
        let pending = cancel_orders(h.clone(), cancel_input(1), &tasks.spawner());
        events.borrow_mut().push(Event::Marker("after-call"));
        let queued = events.clone();
        tasks.enqueue(move || queued.borrow_mut().push(Event::Marker("queued")));
        tasks.drain();
        completed(pending).unwrap();
        trace(
            &events.borrow(),
            &oracle()["timings"][usize::from(reject)]["trace"],
            "actual p-iteration timing",
        );
    }
}
