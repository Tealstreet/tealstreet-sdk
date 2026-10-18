#[path = "support/retained_observe.rs"]
mod retained_observe;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_bingx::data_reads::{BingxDataReads, PositionMetadata, ReadTransport};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/positions_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "BINGX-POS source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..1000 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("fixture did not resolve within 1000 local polls")
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(v) => {
            let mut keys = v.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&v[k])))
                    .collect(),
            )
        }
        Value::Array(v) => Value::Array(v.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}
fn side(s: PositionSide) -> &'static str {
    match s {
        PositionSide::Long => "long",
        PositionSide::Short => "short",
    }
}
fn mode(s: PositionMode) -> &'static str {
    match s {
        PositionMode::Hedged => "hedged",
        PositionMode::OneWay => "oneway",
    }
}
fn margin(s: MarginType) -> &'static str {
    match s {
        MarginType::Isolated => "isolated",
        MarginType::Cross => "cross",
    }
}
fn position(v: &Position) -> Value {
    let mut out = json!({"id":v.id,"symbol":v.symbol,"side":side(v.side.into_value().expect("known source side")),"entryPrice":number(v.entry_price),"notional":number(v.notional),"leverage":number(v.leverage),"unrealizedPnl":number(v.unrealized_pnl),"contracts":number(v.contracts),"liquidationPrice":number(v.liquidation_price)});
    if let Some(s) = v.margin_type.into_value() {
        out["marginType"] = json!(margin(s));
    }
    if let Some(s) = v.position_mode.into_value() {
        out["positionMode"] = json!(mode(s));
    }
    if let Some(s) = v.margin_size.into_value() {
        out["marginSize"] = number(s);
    }
    if let Some(s) = v.info.as_ref() {
        out["info"] = retained_observe::observe(s);
    }
    assert!(
        v.extra.is_none() && v.realized_pnl.is_none() && v.last_update.is_none(),
        "BINGX-POS native position shape"
    );
    out
}
fn metadata(v: &PositionMetadata) -> Value {
    json!({"symbol":v.symbol,"side":side(v.side),"leverage":number(v.leverage),"positionMode":mode(v.position_mode),"marginType":margin(v.margin_type)})
}
fn order(v: &Order) -> Value {
    assert_eq!(v.r#type, OrderType::Limit, "BINGX-POS native order type");
    assert_eq!(v.side, OrderSide::Buy, "BINGX-POS native order side");
    let status = match v.status.value().expect("present source enum in fixture") {
        OrderStatus::Open => "open",
        OrderStatus::Canceled => "canceled",
        _ => panic!("unexpected native order status"),
    };
    json!({"id":v.id,"symbol":v.symbol,"status":status,"type":"limit","side":"buy","price":number((v.price).to_number()),"amount":number(v.amount),"filled":number(v.filled),"remaining":number(v.remaining),"reduceOnly":v.reduce_only,"lastUpdate":number(v.last_update)})
}
#[derive(Default)]
struct Host {
    logs: RefCell<Vec<String>>,
    events: Rc<RefCell<Vec<String>>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    exceptions: RefCell<Vec<String>>,
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("no timers in this slice")
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        struct Task;
        impl ExchangeTask for Task {
            fn cancel(&self) {}
        }
        self.tasks.borrow_mut().push(future);
        Box::new(Task)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if severity == "error" {
            self.logs
                .borrow_mut()
                .push(message.strip_prefix("exchange error ").unwrap().into());
        }
    }
    fn unique_id(&self) -> String {
        "id".into()
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
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.exceptions.borrow_mut().push(error.message.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker in this slice")
    }
}
struct Transport {
    input: Value,
    mode: RefCell<String>,
    calls: RefCell<Vec<Value>>,
    hook: RefCell<Option<Rc<dyn Fn(&safe_cex_transport::RestRequest)>>>,
}
impl ReadTransport for Transport {
    fn send(&self, request: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        if let Some(hook) = self.hook.borrow().as_ref() {
            hook(&request);
        }
        let body_value = request
            .body
            .as_ref()
            .map(|b| b.untransformed().unwrap().clone().into_value().unwrap());
        let body = body_value.as_ref();
        let args = if request.method == "POST" {
            body.cloned().unwrap()
        } else {
            request
                .params
                .as_ref()
                .map_or(json!({"undefined":true}), |v| json!({"params":v}))
        };
        self.calls
            .borrow_mut()
            .push(json!({"method":request.method,"url":request.url,"config":observe(&args)}));
        let params = if request.method == "POST" {
            body
        } else {
            request.params.as_ref()
        };
        let error = &self.input["error"];
        let rejected = !error.is_null()
            && (error["scope"] == "all"
                || (error["scope"] == "margin"
                    && request.method == "GET"
                    && request.url == "/openApi/swap/v2/trade/marginType")
                || (error["scope"] == "order-b" && params.unwrap()["orderId"] == "b"));
        let result = if rejected {
            let mut e = ExchangeError::new(error["message"].as_str().unwrap());
            e = e.with_retained_response((json!({"data":{"msg":error["msg"]}})).into());
            e.store_subscriber_ids = vec![71, 83];
            Err(e)
        } else {
            let value = match request.method.as_str() {
                "POST" => {
                    if request.url == "/openApi/swap/v1/positionSide/dual"
                        && self.input["postCode"] == 0
                    {
                        *self.mode.borrow_mut() =
                            params.unwrap()["dualSidePosition"].as_str().unwrap().into();
                    }
                    json!({"code":self.input["postCode"],"msg":self.input["postMsg"]})
                }
                "DELETE" => {
                    json!({"code":if request.url=="/openApi/swap/v2/trade/order"{self.input["cancelCodes"][params.unwrap()["orderId"].as_str().unwrap()].clone()}else{self.input.get("cancelAllCode").cloned().unwrap_or(json!(0))},"msg":"cancel rejected"})
                }
                "GET" => match request.url.as_str() {
                    "/openApi/swap/v2/user/positions" => {
                        json!({"code":self.input.get("positionCode").cloned().unwrap_or(json!(0)),"msg":"position rejected","data":self.input["rows"]})
                    }
                    "/openApi/swap/v1/positionSide/dual" => {
                        json!({"code":0,"data":{"dualSidePosition":*self.mode.borrow()}})
                    }
                    "/openApi/swap/v2/trade/leverage" => {
                        let symbol = params.unwrap()["symbol"].as_str().unwrap();
                        if self.input["leverageFailure"] == symbol {
                            json!({"code":9,"msg":"no leverage"})
                        } else {
                            json!({"code":0,"data":{"longLeverage":if symbol=="ETH-USDC"{11}else{17},"shortLeverage":if symbol=="ETH-USDC"{31}else{23}}})
                        }
                    }
                    "/openApi/swap/v2/trade/marginType" => {
                        json!({"code":0,"data":{"marginType":if params.unwrap()["symbol"]=="ETH-USDC"{"CROSSED"}else{"ISOLATED"}}})
                    }
                    _ => panic!("unexpected URL {}", request.url),
                },
                _ => panic!("unexpected method"),
            };
            Ok(serde_json::from_value(value).unwrap())
        };
        Box::pin(std::future::ready(result))
    }
}
fn fixture(
    input: Value,
) -> (
    Rc<BingxDataReads>,
    Rc<DefaultStore>,
    Rc<Transport>,
    Rc<Host>,
) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(serde_json::from_value(input["markets"].clone()).unwrap()),
            positions: Some(serde_json::from_value(input["cached"].clone()).unwrap()),
            orders: Some(serde_json::from_value(input["orders"].clone()).unwrap()),
            fetched_leverage: Some(
                serde_json::from_value(json!({"OLD":{"buyLeverage":2,"sellLeverage":3}})).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host::default());
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store.clone(),
        host.clone(),
        ExchangeSettings::default(),
    ));
    let events = host.events.clone();
    base.on(
        "error",
        Rc::new(move |_, v| {
            let v = safe_cex_exchange::js_runtime::value::to_json(v).unwrap();
            events.borrow_mut().push(v.as_str().unwrap().into());
            Ok(())
        }),
    );
    if input["disposed"] == true {
        base.dispose().unwrap();
    }
    let transport = Rc::new(Transport {
        mode: RefCell::new(input["mode"].as_str().unwrap().into()),
        input,
        calls: RefCell::new(Vec::new()),
        hook: RefCell::new(None),
    });
    (
        Rc::new(BingxDataReads::new(base, transport.clone())),
        store,
        transport,
        host,
    )
}
fn state(store: &DefaultStore) -> Value {
    let leverage = Store::fetched_leverage(store);
    let leverage=Value::Object(leverage.iter().map(|(key,v)|(key.clone(),json!({"buyLeverage":number(v.buy_leverage),"sellLeverage":number(v.sell_leverage)}))).collect());
    let ids = Store::update_ids(store);
    json!({"positions":Store::positions(store).iter().map(position).collect::<Vec<_>>(),"orders":Store::orders(store).iter().map(order).collect::<Vec<_>>(),"fetchedLeverage":leverage,"updateIds":{"orders":number(ids.orders),"positions":number(ids.positions)}})
}
#[test]
fn positions_match_actual_typescript() {
    let names = [
        "fetchOpenPositions",
        "fetchPositions",
        "fetchPositionMetadata",
        "changePositionMode",
        "changeMarginType",
        "setLeverage",
        "cancelOrders",
        "cancelSymbolOrders",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-POS source method census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "BINGX-POS fixed input case count");
    let input = json!(
        cases
            .iter()
            .map(|c| json!([c["method"], c["variant"], c["input"]]))
            .collect::<Vec<_>>()
    );
    let encoded = serde_json::to_string(&sorted(&input)).unwrap();
    let fingerprint = encoded.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(
        fingerprint, 0x40d8d59c58f15663,
        "BINGX-POS pinned input content"
    );
    println!("BINGX-POS runtime {}", corpus()["runtime"]);
    let mut counts = [0; 8];
    for (index, case) in cases.iter().enumerate() {
        let method = case["method"].as_str().unwrap();
        let family = names.iter().position(|n| *n == method).unwrap();
        assert_eq!(family, index / 4, "BINGX-POS source method order");
        assert_eq!(
            case["variant"],
            json!(index % 4),
            "BINGX-POS source variant order"
        );
        counts[family] += 1;
        let (venue, store, transport, host) = fixture(case["input"].clone());
        let symbols = case["input"]["symbols"].as_array().map(|v| {
            v.iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        });
        let symbol = case["input"]["symbol"].as_str().unwrap();
        let flag = case["input"]["flag"].as_bool().unwrap();
        let void = |()| json!({"undefined":true});
        let result: Result<Value, ExchangeError> = match method {
            "fetchOpenPositions" => ready(venue.fetch_open_positions())
                .map(|v| json!(v.iter().map(position).collect::<Vec<_>>())),
            "fetchPositions" => ready(venue.fetch_positions(symbols.as_deref()))
                .map(|v| json!(v.iter().map(position).collect::<Vec<_>>())),
            "fetchPositionMetadata" => {
                ready(venue.fetch_position_metadata(symbols.as_ref().unwrap()))
                    .map(|v| json!(v.iter().map(metadata).collect::<Vec<_>>()))
            }
            "changePositionMode" => ready(venue.change_position_mode(flag)).map(void),
            "changeMarginType" => ready(venue.change_margin_type(flag, Some(symbol))).map(void),
            "setLeverage" => ready(venue.set_leverage(
                symbol,
                js_value_as_f64(&case["input"]["leverage"]).unwrap(),
                case["input"]["side"].as_str().map(|s| match s {
                    "long" => PositionSide::Long,
                    "short" => PositionSide::Short,
                    _ => panic!("invalid fixture side"),
                }),
            ))
            .map(void),
            "cancelOrders" => ready(venue.cancel_orders(
                &serde_json::from_value::<Vec<Order>>(case["input"]["orders"].clone()).unwrap(),
            ))
            .map(void),
            "cancelSymbolOrders" => ready(venue.cancel_symbol_orders(symbol)).map(void),
            _ => unreachable!(),
        };
        let output = match result {
            Ok(v) => json!({"ok":v}),
            Err(e) => {
                if method == "cancelSymbolOrders" && index % 4 == 2 {
                    assert_eq!(
                        e.store_subscriber_ids,
                        vec![71, 83],
                        "BINGX-POS original transport boundaries"
                    );
                }
                json!({"error":{"name":e.name,"message":e.message}})
            }
        };
        assert_eq!(
            output,
            case["output"],
            "BINGX-POS native {method} variant {}",
            index % 4
        );
        assert_eq!(
            json!(*transport.calls.borrow()),
            case["calls"],
            "BINGX-POS native request order/config {method} variant {}",
            index % 4
        );
        assert_eq!(
            state(store.as_ref()),
            case["state"],
            "BINGX-POS native Store effects {method} variant {}",
            index % 4
        );
        let reports = case["reports"].as_array().unwrap();
        assert_eq!(
            *host.logs.borrow(),
            reports
                .iter()
                .map(|r| r[0].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "BINGX-POS native report messages {method}"
        );
        let formatted = reports
            .iter()
            .map(|r| {
                format!(
                    "{}: {}",
                    r.get(1).and_then(Value::as_str).unwrap_or("Exchange Error"),
                    r[0].as_str().unwrap()
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            *host.events.borrow(),
            formatted,
            "BINGX-POS native report labels {method}"
        );
        assert!(
            host.tasks.borrow().is_empty(),
            "BINGX-POS no abandoned ordinary fixture work"
        );
    }
    assert_eq!(counts, [4; 8], "BINGX-POS native executed rows");
}
#[test]
fn typed_position_and_nan_request_boundaries_are_explicit() {
    assert_eq!(
        corpus()["boundary"]["inherited"]["side"],
        "function",
        "BINGX-POS actual source callable side"
    );
    assert_eq!(
        corpus()["boundary"]["inherited"]["id"],
        "BTCUSDT:function Object() { [native code] }",
        "BINGX-POS source callable interpolation"
    );
    let (venue, _, _, host) = fixture(corpus()["boundary"]["inherited"]["input"].clone());
    assert_eq!(
        ready(venue.fetch_open_positions()).unwrap()[0].id,
        "ETHUSDC:long",
        "BINGX-POS typed side refusal returns cached positions"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["typed PositionSide refuses inherited callable"],
        "BINGX-POS typed side refusal report"
    );
    assert_eq!(
        corpus()["boundary"]["nanBody"]["leverage"],
        json!({"number":"NaN"}),
        "BINGX-POS actual source forwards NaN"
    );
    let (venue, _, transport, host) = fixture(corpus()["cases"][21]["input"].clone());
    let error = ready(venue.set_leverage("BTCUSDT", f64::NAN, None)).unwrap_err();
    assert_eq!(
        error.message, "typed RestBody refuses NaN leverage",
        "BINGX-POS explicit NaN refusal"
    );
    assert_eq!(
        transport.calls.borrow().len(),
        1,
        "BINGX-POS no NaN POST and no position refresh"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["typed RestBody refuses NaN leverage"],
        "BINGX-POS visible NaN refusal"
    );
}
#[test]
fn metadata_disposal_guard_after_both_requests() {
    let (venue, store, transport, _) = fixture(corpus()["cases"][8]["input"].clone());
    let base = venue.base.clone();
    *transport.hook.borrow_mut() = Some(Rc::new(move |request| {
        if request.url == "/openApi/swap/v2/trade/marginType" {
            base.dispose().unwrap();
        }
    }));
    let output = ready(venue.fetch_position_metadata(&["BTCUSDT".into()]));
    assert!(
        output.unwrap().is_empty(),
        "BINGX-POS in-flight metadata disposal"
    );
    assert_eq!(
        transport.calls.borrow().len(),
        3,
        "BINGX-POS disposal occurs after both requests start"
    );
    assert!(
        Store::fetched_leverage(store.as_ref()).contains_key("BTCUSDT"),
        "BINGX-POS leverage completion remains observable"
    );
}
#[test]
fn later_source_error_wins_over_typed_side_refusal() {
    assert_eq!(
        corpus()["boundary"]["prefix"]["reports"],
        json!([["Cannot convert object to primitive value"]]),
        "BINGX-POS actual source later field error"
    );
    let (venue, _, _, host) = fixture(corpus()["boundary"]["prefix"]["input"].clone());
    assert_eq!(
        ready(venue.fetch_open_positions()).unwrap()[0].id,
        "ETHUSDC:long",
        "BINGX-POS later error cached fallback"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["Cannot convert object to primitive value"],
        "BINGX-POS source error precedes typed side refusal"
    );
}
#[test]
fn write_failures_preserve_subscriber_cause_and_partial_effects() {
    for (case_index, method) in [(12, "changePositionMode"), (24, "cancelOrders")] {
        let (venue, store, transport, host) =
            fixture(corpus()["cases"][case_index]["input"].clone());
        let cleanup = Store::subscribe(
            store.as_ref(),
            Box::new(|_, _| {
                let mut cause = ExchangeError::new("subscriber cause");
                cause.name = "ListenerError".into();
                cause.store_subscriber_ids = vec![71, 83];
                cause = cause
                    .with_retained_response((json!({"data":{"msg":"subscriber denied"}})).into());
                Err(StoreSubscriberError::new("subscriber cause").with_cause(Rc::new(cause)))
            }),
        );
        let error = if method == "changePositionMode" {
            ready(venue.change_position_mode(true)).unwrap_err()
        } else {
            ready(
                venue.cancel_orders(
                    &serde_json::from_value::<Vec<Order>>(
                        corpus()["cases"][case_index]["input"]["orders"].clone(),
                    )
                    .unwrap(),
                ),
            )
            .unwrap_err()
        };
        assert_eq!(
            error.name, "ListenerError",
            "BINGX-POS subscriber error name {method}"
        );
        assert_eq!(
            error.message, "subscriber cause",
            "BINGX-POS subscriber original cause {method}"
        );
        assert_eq!(
            &error.store_subscriber_ids[..2],
            &[71, 83],
            "BINGX-POS subscriber inner boundaries {method}"
        );
        assert_eq!(
            error.store_subscriber_ids.len(),
            3,
            "BINGX-POS subscriber outer boundary {method}"
        );
        assert_eq!(
            *host.logs.borrow(),
            vec!["subscriber denied"],
            "BINGX-POS subscriber response-message preference {method}"
        );
        if method == "cancelOrders" {
            assert_eq!(
                transport.calls.borrow().len(),
                1,
                "BINGX-POS failed group notification stops later groups"
            );
            assert_eq!(
                Store::orders(store.as_ref())
                    .iter()
                    .find(|o| o.id == "k")
                    .unwrap()
                    .status,
                OrderStatus::Canceled,
                "BINGX-POS completed cancellation survives notification failure"
            );
        } else {
            assert_eq!(
                Store::positions(store.as_ref())[0].symbol,
                "BTCUSDT",
                "BINGX-POS refreshed positions survive notification failure"
            );
        }
        cleanup();
    }
}
#[test]
fn rejected_metadata_pair_keeps_pending_leverage_sibling_running() {
    assert_eq!(
        corpus()["boundary"]["lateSibling"],
        json!({"early":[],"before":false,"afterBuyLeverage":17}),
        "BINGX-POS actual source pending sibling continues"
    );
    let (venue, store, transport, host) = fixture(corpus()["cases"][10]["input"].clone());
    struct Deferred {
        inner: Rc<Transport>,
        release: Rc<std::cell::Cell<bool>>,
    }
    impl ReadTransport for Deferred {
        fn send(
            &self,
            request: safe_cex_transport::RestRequest,
        ) -> ExchangeFuture<'_, JsNumberInput> {
            let delayed = request.url == "/openApi/swap/v2/trade/leverage";
            let future = self.inner.send(request);
            let release = self.release.clone();
            Box::pin(async move {
                if delayed {
                    std::future::poll_fn(|cx| {
                        if release.get() {
                            Poll::Ready(())
                        } else {
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        }
                    })
                    .await;
                }
                future.await
            })
        }
    }
    let release = Rc::new(std::cell::Cell::new(false));
    let venue = Rc::new(BingxDataReads::new(
        venue.base.clone(),
        Rc::new(Deferred {
            inner: transport,
            release: release.clone(),
        }),
    ));
    assert!(
        ready(venue.fetch_position_metadata(&["BTCUSDT".into()]))
            .unwrap()
            .is_empty(),
        "BINGX-POS rejection does not wait for pending sibling"
    );
    assert!(
        !Store::fetched_leverage(store.as_ref()).contains_key("BTCUSDT"),
        "BINGX-POS pending sibling has not written yet"
    );
    let mut tasks = host.tasks.borrow_mut().drain(..).collect::<Vec<_>>();
    assert_eq!(
        tasks.len(),
        1,
        "BINGX-POS exactly one owned sibling continuation"
    );
    release.set(true);
    ready(tasks.pop().unwrap());
    assert_eq!(
        Store::fetched_leverage(store.as_ref())["BTCUSDT"].buy_leverage,
        17.0,
        "BINGX-POS sibling writes after pair rejection"
    );
}
