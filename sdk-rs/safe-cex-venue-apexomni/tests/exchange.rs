use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::{JsJsonValue as J, numeric_ingress::js_from_value};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_apexomni::exchange::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{Arc, OnceLock},
};
#[path = "support/native_values.rs"]
pub mod native;
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/exchange.cjs"))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "exchange oracle {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn obj(fields: Vec<(&str, J)>) -> J {
    J::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn s(value: impl Into<String>) -> J {
    J::String(value.into())
}
fn n(value: f64) -> J {
    J::Number(value)
}
fn side(value: impl Into<safe_cex_types::ModelField<OrderSide>>) -> J {
    match value.into().into_value() {
        Some(OrderSide::Buy) => s("buy"),
        Some(OrderSide::Sell) => s("sell"),
        None => J::Undefined,
    }
}
thread_local! {
    static FIXTURE_STAMPS: RefCell<Vec<(f64, f64)>> = const { RefCell::new(Vec::new()) };
}
fn source_clock_stamp(value: f64) -> J {
    let now = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
    assert!(
        (now - 5_000.0..=now).contains(&value),
        "model timestamp must read the live virtual clock: {value} vs {now}"
    );
    // The source oracle freezes its virtual clock; compare its other fields exactly.
    let source = FIXTURE_STAMPS.with(|stamps| {
        stamps
            .borrow()
            .iter()
            .find(|(actual, _)| *actual == value)
            .map(|(_, source)| *source)
    });
    n(source.unwrap_or(1700001234567.0))
}
fn position(value: &Position) -> J {
    let mut fields = vec![
        ("id", s(&value.id)),
        ("symbol", s(&value.symbol)),
        (
            "side",
            s(if value.side == PositionSide::Long {
                "long"
            } else {
                "short"
            }),
        ),
        ("entryPrice", n(value.entry_price)),
        ("notional", n(value.notional)),
        ("leverage", n(value.leverage)),
        ("unrealizedPnl", n(value.unrealized_pnl)),
        ("contracts", n(value.contracts)),
        ("liquidationPrice", n(value.liquidation_price)),
    ];
    if let Some(v) = value.margin_size.into_value() {
        fields.push(("marginSize", n(v)));
    }
    if let Some(v) = value.realized_pnl.into_value() {
        fields.push(("realizedPnl", n(v)));
    }
    if let Some(v) = value.position_mode.into_value() {
        assert_eq!(v, PositionMode::Hedged);
        fields.push(("positionMode", s("hedged")));
    }
    if let Some(v) = value.margin_type.into_value() {
        assert_eq!(v, MarginType::Cross);
        fields.push(("marginType", s("cross")));
    }
    if let Some(v) = value.info.as_ref() {
        fields.push(("info", native::retained(v)));
    }
    if let Some(v) = value.last_update.into_value() {
        fields.push(("lastUpdate", source_clock_stamp(v)));
    }
    obj(fields)
}
fn order(value: &Order) -> J {
    let mut fields = vec![
        ("id", s(&value.id)),
        (
            "status",
            match value.status {
                ModelField::Value(OrderStatus::Open) => s("open"),
                ModelField::Value(OrderStatus::Closed) => s("closed"),
                ModelField::Value(OrderStatus::Canceled) => s("canceled"),
                ModelField::Undefined => J::Undefined,
                _ => panic!("source order status fixture"),
            },
        ),
        ("symbol", s(&value.symbol)),
        (
            "type",
            match value.r#type {
                ModelField::Value(OrderType::Limit) => s("limit"),
                ModelField::Value(OrderType::Market) => s("market"),
                ModelField::Value(OrderType::StopLoss) => s("stop_market"),
                ModelField::Value(OrderType::TakeProfit) => s("take_profit_market"),
                ModelField::Undefined => J::Undefined,
                _ => panic!("source order type fixture"),
            },
        ),
        ("side", side(value.side)),
        ("price", n((value.price).to_number())),
        ("amount", n(value.amount)),
        ("filled", n(value.filled)),
        ("remaining", n(value.remaining)),
        ("info", J::from(value.info.clone().unwrap())),
        ("lastUpdate", source_clock_stamp(value.last_update)),
    ];
    match value.reduce_only {
        ModelField::Absent => {}
        ModelField::Undefined => fields.push(("reduceOnly", J::Undefined)),
        ModelField::Value(value) => fields.push(("reduceOnly", J::Bool(value))),
    }
    if let Some(v) = value.time_in_force.into_value() {
        fields.push((
            "timeInForce",
            s(match v {
                OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
                OrderTimeInForce::PostOnly => "PostOnly",
                _ => panic!(),
            }),
        ));
    }
    for (key, v) in [
        ("tpTriggerType", value.tp_trigger_type),
        ("slTriggerType", value.sl_trigger_type),
    ] {
        if let Some(v) = v.into_value() {
            fields.push((
                key,
                s(match v {
                    OrderStopTriggerType::IndexPrice => "index_price",
                    OrderStopTriggerType::LastPrice => "last_price",
                    OrderStopTriggerType::MarkPrice => "mark_price",
                }),
            ));
        }
    }
    obj(fields)
}
fn balance(value: &Balance) -> J {
    let mut fields = vec![
        ("total", n(value.total)),
        ("upnl", n(value.upnl)),
        ("used", n(value.used)),
        ("free", n(value.free)),
    ];
    if let Some(v) = &value.info {
        fields.push(("info", native::retained(v)));
    }
    obj(fields)
}
fn market(value: &Market) -> J {
    assert_eq!(value.is_perpetual, None, "Apex market omits isPerpetual");
    let mut fields = vec![
        ("id", s(&value.id)),
        ("symbol", s(&value.symbol)),
        ("base", s(&value.base)),
        ("quote", s(&value.quote)),
        ("active", J::Bool(value.active)),
        (
            "precision",
            obj(vec![
                ("amount", n(value.precision.amount)),
                ("price", n(value.precision.price)),
            ]),
        ),
        (
            "limits",
            obj(vec![
                (
                    "amount",
                    obj(vec![
                        ("min", n(value.limits.amount.min)),
                        ("max", n(value.limits.amount.max)),
                    ]),
                ),
                (
                    "leverage",
                    obj(vec![
                        ("min", n(value.limits.leverage.min)),
                        ("max", n(value.limits.leverage.max)),
                    ]),
                ),
            ]),
        ),
        ("info", J::from(value.info.clone().unwrap())),
    ];
    if let Some(expiring) = value.is_expiring {
        fields.push(("isExpiring", J::Bool(expiring)));
    }
    obj(fields)
}
fn execution(value: &Execution) -> J {
    let mut fields = vec![
        ("timestamp", n(value.timestamp)),
        ("symbol", s(&value.symbol)),
        ("price", n(value.price)),
        ("qty", n(value.qty)),
        ("side", side(value.side)),
    ];
    if let Some(v) = &value.order_id {
        fields.push(("orderId", s(v)));
    }
    if let Some(v) = value.commission {
        fields.push(("commission", n(v)));
    }
    if let Some(v) = &value.commission_asset {
        fields.push(("commissionAsset", s(v)));
    }
    obj(fields)
}
#[derive(Default)]
struct Host {
    now_calls: std::cell::Cell<usize>,
    request_hook: RefCell<Option<Rc<dyn Fn()>>>,
    operations: RefCell<Vec<J>>,
    services: RefCell<Option<Rc<dyn safe_cex_venue_apexomni::lifecycle::ApexServices>>>,
    request_errors: RefCell<HashMap<&'static str, ExchangeError>>,
    record_timeouts: std::cell::Cell<bool>,
    record_random: std::cell::Cell<bool>,
    sequence_orders: std::cell::Cell<usize>,
    generate_order_ids: std::cell::Cell<bool>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    responses: RefCell<HashMap<&'static str, J>>,
}
impl Host {
    fn record(&self, name: &str, mut args: Vec<J>) {
        args.insert(0, s(name));
        self.operations.borrow_mut().push(J::Array(args));
    }
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        self.now_calls.set(self.now_calls.get() + 1);
        1700001234567.0
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.record("sleep", vec![n(delay as f64)]);
        Box::pin(async { Ok(()) })
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
        self.record("log", vec![s(message), s(severity)]);
    }
    fn unique_id(&self) -> String {
        "unique".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        if self.record_random.get() {
            self.record("random", vec![]);
        }
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.record("reported:error", vec![s(&error.message)]);
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker fixture")
    }
}
impl ApexHost for Host {
    fn services(
        &self,
    ) -> Result<Rc<dyn safe_cex_venue_apexomni::lifecycle::ApexServices>, ExchangeError> {
        self.services
            .borrow()
            .clone()
            .ok_or_else(|| ExchangeError::new("fixture runtime missing"))
    }
    fn create_api(
        &self,
        _: &ExchangeOptions,
        _: Rc<BaseExchange>,
        rate: Option<u8>,
    ) -> Result<(), ExchangeError> {
        self.record("api:create", vec![]);
        if let Some(rate) = rate {
            self.record("rateLimit", vec![n(rate.into())]);
        }
        Ok(())
    }
    fn create_websocket(
        &self,
        parent: std::rc::Weak<ApexOmniExchange>,
        private: bool,
    ) -> Result<(), ExchangeError> {
        assert!(parent.upgrade().is_some());
        self.record("socket:create", vec![J::Bool(private)]);
        Ok(())
    }
    fn dispose_websocket(&self, private: bool) -> Result<(), ExchangeError> {
        self.record("socket:dispose", vec![J::Bool(private)]);
        Ok(())
    }
    fn request(&self, r: ApexRequest) -> ExchangeFuture<'_, J> {
        let mut args = vec![
            s(r.method),
            s(r.path),
            r.params,
            r.data,
            r.content_type.map_or(J::Undefined, s),
        ];
        if self.record_timeouts.get() {
            args.push(r.timeout_ms.map_or(J::Undefined, n));
        }
        self.record("request", args);
        if let Some(hook) = self.request_hook.borrow().clone() {
            hook();
        }
        if r.path == "/v3/order" && self.generate_order_ids.get() {
            let id = self.sequence_orders.get() + 1;
            self.sequence_orders.set(id);
            return Box::pin(async move {
                Ok(obj(vec![(
                    "data",
                    obj(vec![("id", s(format!("placed-{id}")))]),
                )]))
            });
        }
        if let Some(error) = self.request_errors.borrow().get(r.path).cloned() {
            return Box::pin(async move { Err(error) });
        }
        let value = self
            .responses
            .borrow()
            .get(r.path)
            .cloned()
            .expect("fixture response");
        let yielding = self.services.borrow().is_some();
        Box::pin(async move {
            if yielding {
                let mut pending = true;
                futures::future::poll_fn(move |cx| {
                    if pending {
                        pending = false;
                        cx.waker().wake_by_ref();
                        std::task::Poll::Pending
                    } else {
                        std::task::Poll::Ready(())
                    }
                })
                .await;
            }
            Ok(value)
        })
    }
    fn date_parse(&self, text: &str) -> Result<f64, ExchangeError> {
        Ok(if text == "2026-10-13T00:00:00Z" {
            1791849600000.0
        } else {
            f64::NAN
        })
    }
    fn console(&self, level: &str, message: &str, detail: Option<&J>) -> Result<(), ExchangeError> {
        self.record(
            "console",
            vec![
                s(level),
                s(message),
                detail.cloned().unwrap_or(J::Undefined),
            ],
        );
        Ok(())
    }
}
fn fixture(host: &Rc<Host>) -> Rc<DefaultStore> {
    let mut store = DefaultStore::default();
    store.now = Arc::new(|| 1700001234567.0);
    let store = Rc::new(store);
    let c = &corpus()["fixtures"];
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![js_from_value(c["market"].clone()).unwrap()]),
            tickers: Some(vec![js_from_value(c["ticker"].clone()).unwrap()]),
            positions: Some(vec![js_from_value(c["position"].clone()).unwrap()]),
            balance: Some(js_from_value(c["balance"].clone()).unwrap()),
            zk_link_config: Some(json!({"id":"old","l2Key":"old-key"})),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let _ = host;
    store
}
fn responses() -> HashMap<&'static str, J> {
    let f = &corpus()["fixtures"];
    let mut disabled = f["contract"].clone();
    disabled["symbol"] = json!("disabled");
    disabled["enableTrade"] = json!(false);
    HashMap::from([
        ("/v3/account", J::from(json!({"data":f["account"]}))),
        ("/v3/open-orders", J::from(json!({"data":[f["order"]]}))),
        (
            "/v3/account-balance",
            J::from(
                json!({"data":{"totalEquityValue":"401.25","maintenanceMargin":"51.25","initialMargin":"61.25"}}),
            ),
        ),
        (
            "/v3/symbols",
            J::from(
                json!({"data":{"contractConfig":{"perpetualContract":[f["contract"],disabled]}}}),
            ),
        ),
    ])
}
fn run(name: &str) {
    let rows = corpus()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 54, "exchange source total floor");
    let mut count = 0;
    for row in rows.iter().filter(|r| r["name"] == name) {
        let variant = row["variant"].as_u64().unwrap();
        let host = Rc::new(Host::default());
        *host.responses.borrow_mut() = responses();
        host.responses
            .borrow_mut()
            .insert("/v3/delete-order", J::Object(vec![]));
        let store = fixture(&host);
        let base = BaseExchange::new(
            ExchangeName::Apexomni,
            serde_json::from_value(json!({"key":"key","secret":"secret","passphrase":"pass"}))
                .unwrap(),
            store.clone(),
            host.clone(),
            ExchangeSettings::default(),
        );
        let exchange = ApexOmniExchange::new(base, host.clone()).unwrap();
        let construction = host.operations.replace(Vec::new());
        let J::Array(args) = native::input(&row["args"]) else {
            panic!()
        };
        let actual = futures::executor::block_on(async {
            match name {
                "constructor" => {
                    assert_eq!(
                        exchange.unified_margin_status.get(),
                        1.0,
                        "ApexOmniExchange constructor native margin"
                    );
                    *host.operations.borrow_mut() = construction;
                    obj(vec![
                        ("margin", n(exchange.unified_margin_status.get())),
                        ("network", n(exchange.network_id.get())),
                        (
                            "cancelled",
                            n(exchange.recently_cancelled_orders.borrow().len() as f64),
                        ),
                        (
                            "executions",
                            n(exchange.recent_ws_executions.borrow().len() as f64),
                        ),
                        ("accountType", s(exchange.account_type())),
                    ])
                }
                "dispose" => {
                    exchange.dispose().unwrap();
                    obj(vec![("disposed", J::Bool(exchange.base.is_disposed()))])
                }
                "accountType" => {
                    exchange
                        .unified_margin_status
                        .set([1.0, 2.0, f64::NAN][variant as usize]);
                    s(exchange.account_type())
                }
                "getAccount" => exchange.get_account().await.unwrap(),
                "fetchRawBalance" | "fetchBalance" => {
                    if variant == 1 {
                        host.responses
                            .borrow_mut()
                            .insert("/v3/account-balance", J::Object(vec![]));
                    }
                    if variant == 2 {
                        exchange.base.dispose().unwrap();
                        Store::update(
                            store.as_ref(),
                            StoreUpdate {
                                markets: Some(vec![
                                    js_from_value(corpus()["fixtures"]["market"].clone()).unwrap(),
                                ]),
                                tickers: Some(vec![
                                    js_from_value(corpus()["fixtures"]["ticker"].clone()).unwrap(),
                                ]),
                                positions: Some(vec![
                                    js_from_value(corpus()["fixtures"]["position"].clone())
                                        .unwrap(),
                                ]),
                                zk_link_config: Some(json!({"id":"old","l2Key":"old-key"})),
                                balance: Some(
                                    js_from_value(corpus()["fixtures"]["balance"].clone()).unwrap(),
                                ),
                                ..Default::default()
                            },
                            false,
                        )
                        .unwrap();
                    }
                    balance(&if name == "fetchBalance" {
                        exchange.fetch_balance().await.unwrap()
                    } else {
                        exchange.fetch_raw_balance().await.unwrap()
                    })
                }
                "mapPosition" => exchange
                    .map_position(&args[0])
                    .unwrap()
                    .as_ref()
                    .map_or(J::Null, position),
                "mapOrder" => J::Array(
                    exchange
                        .map_order(&args[0])
                        .unwrap()
                        .iter()
                        .map(order)
                        .collect(),
                ),
                "fetchOrders" => {
                    if variant > 0 {
                        exchange.recently_cancelled_orders.borrow_mut().insert(
                            "order-1".into(),
                            1700001234567.0 - if variant == 1 { 9999.0 } else { 10001.0 },
                        );
                    }
                    J::Array(
                        exchange
                            .fetch_orders()
                            .await
                            .unwrap()
                            .iter()
                            .map(order)
                            .collect(),
                    )
                }
                "fetchPositions" => {
                    if variant == 1 {
                        store
                            .update_zk_link_config(Some(json!({"id":"old"})))
                            .unwrap();
                    }
                    if variant == 2 {
                        let mut response = host.responses.borrow_mut();
                        let J::Object(root) = response.get_mut("/v3/account").unwrap() else {
                            panic!()
                        };
                        let J::Object(fields) = &mut root[0].1 else {
                            panic!()
                        };
                        fields.iter_mut().find(|(k, _)| k == "positions").unwrap().1 =
                            J::Array(vec![]);
                    }
                    J::Array(
                        exchange
                            .fetch_positions()
                            .await
                            .unwrap()
                            .iter()
                            .map(position)
                            .collect(),
                    )
                }
                "fetchMarketsHelper" | "fetchMarkets" => J::Array(
                    if name == "fetchMarkets" {
                        exchange.fetch_markets().await.unwrap()
                    } else {
                        exchange.fetch_markets_helper().await.unwrap()
                    }
                    .iter()
                    .map(market)
                    .collect(),
                ),
                "normalizeApexSymbolKey" => {
                    s(exchange.normalize_apex_symbol_key(&args[0]).unwrap())
                }
                "normalizeExecutionTimestampToSeconds" => exchange
                    .normalize_execution_timestamp_to_seconds(&args[0])
                    .unwrap()
                    .map_or(J::Undefined, n),
                "recordWsExecution" => {
                    exchange.record_ws_execution(&args[0]).unwrap();
                    if variant == 0 {
                        assert_eq!(
                            exchange.recent_ws_executions.borrow()[0].qty.to_bits(),
                            2.25_f64.to_bits(),
                            "recordWsExecution independent native qty"
                        );
                    }
                    J::Array(
                        exchange
                            .recent_ws_executions
                            .borrow()
                            .iter()
                            .map(execution)
                            .collect(),
                    )
                }
                "getWsExecutionFallback" => {
                    exchange.record_ws_execution(&J::from(json!({"symbol":"BTC-USDC","side":"BUY","createdAt":1700000000123.0,"price":"83.25","size":"2.25","fee":"-0.75","orderId":"row-1"}))).unwrap();
                    J::Array(
                        exchange
                            .get_ws_execution_fallback(
                                "PERP-BTCUSDC-SWAP",
                                if variant == 1 {
                                    Some("different")
                                } else {
                                    None
                                },
                                if variant == 2 {
                                    Some(1700000001000.0)
                                } else {
                                    None
                                },
                                None,
                                5.0,
                            )
                            .unwrap()
                            .iter()
                            .map(execution)
                            .collect(),
                    )
                }
                "cancelOrdersHelper" | "cancelOrders" => {
                    let J::Array(orders) = &args[0] else { panic!() };
                    let orders: Vec<Order> = orders
                        .iter()
                        .map(|o| {
                            let raw = safe_cex_num::js_json_stringify(o).unwrap().unwrap();
                            safe_cex_num::numeric_ingress::js_from_str(&raw).unwrap()
                        })
                        .collect();
                    if name == "cancelOrders" {
                        exchange.cancel_orders(&orders).await.unwrap();
                    } else {
                        exchange.cancel_orders_helper(&orders, "0").await.unwrap();
                    }
                    J::Undefined
                }
                _ => panic!("unclassified exchange source row"),
            }
        });
        native::check_fields(&actual, &row["result"]["value"], &format!("{name} result"));
        let expected = native::input(&row["operations"]);
        let J::Array(mut expected) = expected else {
            panic!()
        };
        expected.retain(|v|!matches!(v,J::Array(values) if matches!(values.first(),Some(J::String(kind)) if kind.starts_with("store:")||kind=="base:dispose")));
        native::native_eq(
            &J::Array(host.operations.borrow().clone()),
            &J::Array(expected),
            &format!("{name} operations"),
        );
        if name.starts_with("cancelOrders") {
            let expected_ops = host.operations.borrow();
            assert!(
                expected_ops
                    .iter()
                    .filter(|v| matches!(v,J::Array(x) if x.first()==Some(&s("request"))))
                    .count()
                    == [2, 1, 12][variant as usize],
                "{name} native request count"
            );
        }
        let mut cancelled = exchange
            .recently_cancelled_orders
            .borrow()
            .iter()
            .map(|(id, time)| J::Array(vec![s(id), n(*time)]))
            .collect::<Vec<_>>();
        cancelled.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        let J::Array(mut expected_cancelled) = native::input(&row["cancelled"]) else {
            panic!()
        };
        expected_cancelled.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
        native::native_eq(
            &J::Array(cancelled),
            &J::Array(expected_cancelled),
            &format!("{name} cancelled state"),
        );
        if name != "dispose" {
            let zk = store
                .zk_link_config()
                .as_deref()
                .cloned()
                .map_or(J::Undefined, J::from);
            native::check_fields(&zk, &row["zk"], &format!("{name} config state"));
        }
        count += 1;
    }
    assert_eq!(count, 3, "{name} native case floor");
}
#[test]
fn constructor_native() {
    run("constructor");
}
#[test]
fn dispose_native() {
    run("dispose");
}
#[test]
fn account_type_native() {
    run("accountType");
}
#[test]
fn account_native() {
    run("getAccount");
}
#[test]
fn raw_balance_native() {
    run("fetchRawBalance");
}
#[test]
fn balance_native() {
    run("fetchBalance");
}
#[test]
fn position_native() {
    run("mapPosition");
}
#[test]
fn order_native() {
    run("mapOrder");
}
#[test]
fn orders_native() {
    run("fetchOrders");
}
#[test]
fn positions_native() {
    run("fetchPositions");
}
#[test]
fn markets_helper_native() {
    run("fetchMarketsHelper");
}
#[test]
fn markets_native() {
    run("fetchMarkets");
}
#[test]
fn symbol_native() {
    run("normalizeApexSymbolKey");
}
#[test]
fn timestamp_native() {
    run("normalizeExecutionTimestampToSeconds");
}
#[test]
fn record_execution_native() {
    run("recordWsExecution");
}
#[test]
fn execution_fallback_native() {
    run("getWsExecutionFallback");
}
#[test]
fn cancel_helper_native() {
    run("cancelOrdersHelper");
}
#[test]
fn cancel_native() {
    run("cancelOrders");
}

#[path = "support/lifecycle.rs"]
mod lifecycle_tests;

#[path = "support/api_runtime.rs"]
mod api_tests;

#[path = "support/historical_fills.rs"]
mod historical_tests;

#[path = "support/trading.rs"]
mod trading_tests;

#[path = "support/websockets.rs"]
mod websocket_tests;

#[test]
fn continuation_position_short_circuit_matches_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/continuation-observations.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-CONT actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    let names = [
        "mapPosition:ignored-custom",
        "mapPosition:ignored-default",
        "mapPosition:missing-market-unrealised",
        "mapOrder:inherited-type",
        "mapOrder:bad-type-clock",
        "mapOrder:bracket-order",
        "fetchMarketsHelper:filter-before-map",
    ];
    assert_eq!(rows.len(), 7, "APEX-CONT independent source count");
    for (row, name) in rows.iter().zip(names) {
        assert_eq!(row["name"], name, "APEX-CONT frozen source order");
    }
    for (index, row) in [&rows[0], &rows[1]].into_iter().enumerate() {
        assert_eq!(row["ok"], true, "APEX-CONT source short circuit outcome");
        assert_eq!(row["clocks"], 1, "APEX-CONT source position clock");
        let leverage = row["result"]["values"]
            .as_array()
            .unwrap()
            .iter()
            .find(|pair| pair[0] == "leverage")
            .unwrap();
        assert_eq!(
            leverage[1],
            json!({"kind":"number","bits":"4014000000000000"}),
            "APEX-CONT source leverage five"
        );
        assert_eq!(
            row["input"]["customImr"], "0.2",
            "APEX-CONT frozen custom leverage"
        );
        if index == 0 {
            assert_eq!(
                row["input"]["customInitialMarginRate"],
                json!({"toString":null}),
                "APEX-CONT frozen unused custom"
            );
        } else {
            assert_eq!(
                row["market"]["info"]["defaultInitialMarginRate"],
                json!({"toString":null}),
                "APEX-CONT frozen unused default"
            );
        }
        let host = Rc::new(Host::default());
        let store = fixture(&host);
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![js_from_value(row["market"].clone()).unwrap()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let base = BaseExchange::new(
            ExchangeName::Apexomni,
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            store,
            host.clone(),
            ExchangeSettings::default(),
        );
        let exchange = ApexOmniExchange::new(base, host.clone()).unwrap();
        host.now_calls.set(0);
        let actual = exchange
            .map_position(&J::from(row["input"].clone()))
            .unwrap_or_else(|error| {
                panic!(
                    "APEX-CONT {} native unexpected rejection: {error:?}",
                    row["name"].as_str().unwrap()
                )
            })
            .expect("APEX-CONT source position exists");
        assert_eq!(
            host.now_calls.get(),
            0,
            "APEX-CONT virtual clock must not read the raw runtime"
        );
        native::check_fields(
            &position(&actual),
            &row["result"],
            &format!(
                "APEX-CONT {} native position",
                row["name"].as_str().unwrap()
            ),
        );
    }
    let row = &rows[2];
    let host = Rc::new(Host::default());
    let store = fixture(&host);
    let exchange = ApexOmniExchange::new(
        BaseExchange::new(
            ExchangeName::Apexomni,
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            store,
            host.clone(),
            ExchangeSettings::default(),
        ),
        host.clone(),
    )
    .unwrap();
    host.operations.borrow_mut().clear();
    host.now_calls.set(0);
    let error = exchange
        .map_position(&J::from(row["input"].clone()))
        .expect_err("APEX-CONT missing-market unrealisedPnl must fail before lookup");
    assert_eq!(
        error.name,
        row["error"]["name"].as_str().unwrap(),
        "APEX-CONT missing-market native error kind"
    );
    assert_eq!(
        error.message,
        row["error"]["message"].as_str().unwrap(),
        "APEX-CONT missing-market native error message"
    );
    assert_eq!(
        host.now_calls.get(),
        0,
        "APEX-CONT native error before clock"
    );
    native::check(
        &J::Array(host.operations.borrow().clone()),
        &row["operations"],
        "APEX-CONT missing-market native error precedes warning",
    );
}

#[path = "support/response_envelopes.rs"]
mod response_envelope_tests;

#[path = "support/market_execution_boundaries.rs"]
mod market_execution_boundary_tests;

#[path = "support/order_continuation.rs"]
mod order_continuation_tests;
