use super::*;
use safe_cex_venue_apexomni::{
    lifecycle::{ApexServices, Cleanup},
    zklink_host::{SdkInitialization, TransactionKind, ZkLinkHandle, ZkLinkHost},
};
use std::{cell::Cell, rc::Weak};
fn trading_corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/trading.cjs"))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .env(
                "SAFE_CEX_VIRTUAL_TIME",
                safe_cex_store::virtual_clock::VIRTUAL_CLOCK
                    .get_current_time_ms()
                    .to_string(),
            )
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "trading source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
struct Services {
    host: Weak<Host>,
    init: SdkInitialization,
}
impl Services {
    fn host(&self) -> Rc<Host> {
        self.host.upgrade().unwrap()
    }
}
impl ZkLinkHost for Services {
    fn init(&self) -> ExchangeFuture<'_, ()> {
        self.host().record("sdk:init", vec![]);
        Box::pin(async { Ok(()) })
    }
    fn builder(&self, kind: TransactionKind, args: Vec<J>) -> Result<ZkLinkHandle, ExchangeError> {
        assert_eq!(kind, TransactionKind::Contract);
        self.host().record("contract:builder", args);
        Ok(Rc::new(Cell::new(false)))
    }
    fn new_transaction(
        &self,
        kind: TransactionKind,
        tx: ZkLinkHandle,
    ) -> Result<ZkLinkHandle, ExchangeError> {
        assert_eq!(kind, TransactionKind::Contract);
        assert!(!tx.downcast_ref::<Cell<bool>>().unwrap().replace(true));
        self.host().record("contract:new", vec![]);
        Ok(tx)
    }
    fn create_signed_contract(&self, tx: ZkLinkHandle) -> Result<J, ExchangeError> {
        assert!(tx.downcast_ref::<Cell<bool>>().unwrap().get());
        self.host().record("contract:sign", vec![]);
        Ok(obj(vec![(
            "signature",
            obj(vec![("signature", s("contract-signature"))]),
        )]))
    }
}
fn place(p: &safe_cex_types::PreparedOrderOpts) -> J {
    let mut fields = vec![
        ("symbol", s(&p.symbol)),
        (
            "type",
            s(match p.r#type.into_value() {
                Some(OrderType::Limit) => "limit",
                Some(OrderType::Market) => "market",
                Some(OrderType::StopLoss) => "stop_market",
                Some(OrderType::TakeProfit) => "take_profit_market",
                _ => panic!(),
            }),
        ),
        ("side", side(p.side)),
        ("amount", n(p.amount)),
    ];
    for (key, value) in [
        ("price", p.price),
        ("triggerPrice", p.trigger_price),
        ("stopLoss", p.stop_loss),
        ("takeProfit", p.take_profit),
    ] {
        if let Some(value) = value {
            fields.push((key, n(value)));
        }
    }
    match p.reduce_only {
        ModelField::Absent => {}
        ModelField::Undefined => fields.push(("reduceOnly", J::Undefined)),
        ModelField::Value(value) => fields.push(("reduceOnly", J::Bool(value))),
    }
    if let Some(value) = p.time_in_force {
        fields.push((
            "timeInForce",
            s(match value {
                OrderTimeInForce::GoodTillCancel => "GoodTillCancel",
                OrderTimeInForce::PostOnly => "PostOnly",
                _ => panic!(),
            }),
        ));
    }
    for (key, value) in [
        ("tpTriggerType", p.tp_trigger_type),
        ("slTriggerType", p.sl_trigger_type),
    ] {
        if let Some(value) = value {
            fields.push((
                key,
                s(match value {
                    OrderStopTriggerType::IndexPrice => "index_price",
                    OrderStopTriggerType::LastPrice => "last_price",
                    OrderStopTriggerType::MarkPrice => "mark_price",
                }),
            ));
        }
    }
    obj(fields)
}
impl ApexServices for Services {
    fn init_external_public_data(&self) -> ExchangeFuture<'_, bool> {
        panic!("not used by trading")
    }
    fn public_data_only(&self) -> bool {
        false
    }
    fn connect_websocket(&self, _: bool) -> Result<(), ExchangeError> {
        panic!("not used by trading")
    }
    fn sdk_initialization(&self) -> &SdkInitialization {
        &self.init
    }
    fn sdk(&self) -> &dyn ZkLinkHost {
        self
    }
    fn new_signer(&self, seed: Option<String>) -> ExchangeFuture<'_, Rc<dyn ZkLinkHost>> {
        self.host().record("signer:create", vec![J::Object(vec![])]);
        self.host()
            .record("signer:init", vec![seed.map_or(J::Undefined, s)]);
        Box::pin(async {
            Ok(Rc::new(Services {
                host: self.host.clone(),
                init: Default::default(),
            }) as Rc<dyn ZkLinkHost>)
        })
    }
    fn schedule_tick(
        &self,
        _: ExchangeFuture<'static, ()>,
        _: Option<f64>,
    ) -> Result<(), ExchangeError> {
        panic!("not used by trading")
    }
    fn date_now_ms(&self) -> f64 {
        1700001234567.
    }
    fn dayjs_value_ms(&self, _: Option<f64>) -> Result<f64, ExchangeError> {
        panic!("not used by trading")
    }
    fn dayjs_subtract(&self, _: f64, _: f64, _: &str) -> Result<f64, ExchangeError> {
        panic!("not used by trading")
    }
    fn listen_ohlcv(
        &self,
        _: OHLCVOptions,
        _: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        panic!("not used by trading")
    }
    fn listen_order_book(
        &self,
        _: &str,
        _: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        panic!("not used by trading")
    }
    fn throttle_fetch_orders(&self) -> Result<(), ExchangeError> {
        self.host().record("socket:throttle-orders", vec![]);
        Ok(())
    }
    fn place_orders(
        &self,
        orders: Vec<safe_cex_types::PreparedOrderOpts>,
    ) -> ExchangeFuture<'_, Vec<String>> {
        self.host().record(
            "place:orders",
            vec![J::Array(orders.iter().map(place).collect())],
        );
        Box::pin(async { Ok(Vec::new()) })
    }
}
fn run_trading(name: &str) {
    let rows = trading_corpus()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 13, "trading source floor");
    let mut count = 0;
    for row in rows.iter().filter(|r| r["name"] == name) {
        let host = Rc::new(Host::default());
        host.generate_order_ids.set(true);
        let services = Rc::new(Services {
            host: Rc::downgrade(&host),
            init: Default::default(),
        });
        *host.services.borrow_mut() = Some(services);
        let store = fixture(&host);
        let mut markets = Store::markets(store.as_ref()).to_vec();
        markets[0].limits.amount.max = (0.009).into();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                zk_link_config: Some(trading_corpus()["zk"].clone().into()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let opts = serde_json::from_value(
            json!({"key":"key","secret":"secret","passphrase":"pass","omniKeySeed":"seed"}),
        )
        .unwrap();
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                opts,
                store.clone(),
                host.clone(),
                Default::default(),
            ),
            host.clone(),
        )
        .unwrap();
        host.record_random.set(true);
        host.responses
            .borrow_mut()
            .insert("/v3/delete-order", J::from(json!({"data":{}})));
        host.responses
            .borrow_mut()
            .insert("/v3/delete-open-orders", J::from(json!({"data":{}})));
        if name != "placeOrder" {
            let mut raw = corpus()["fixtures"]["order"].clone();
            raw["size"] = json!("0.006");
            raw["cumSuccessFillSize"] = json!("0");
            let mapped = venue.map_order(&native::capture(&J::from(raw))).unwrap();
            store.add_or_update_orders_full(mapped).unwrap();
        }
        host.operations.borrow_mut().clear();
        let raw: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
        let result = futures::executor::block_on(async {
            match name {
                "placeOrder" => venue
                    .place_order(&serde_json::from_value::<PlaceOrderOpts>(raw[0].clone()).unwrap())
                    .await
                    .map(|ids| J::Array(ids.into_iter().map(s).collect())),
                "updateOrder" => venue
                    .update_order(
                        &serde_json::from_value::<UpdateOrderOpts>(raw[0].clone()).unwrap(),
                    )
                    .await
                    .map(|ids| J::Array(ids.into_iter().map(s).collect())),
                "cancelSymbolOrders" => venue
                    .cancel_symbol_orders(raw[0].as_str().unwrap(), raw[1].as_bool().unwrap())
                    .await
                    .map(|_| J::Undefined),
                _ => panic!(),
            }
        });
        assert_eq!(
            result.is_ok(),
            row["result"]["ok"].as_bool().unwrap(),
            "{name} native result kind"
        );
        if row["result"]["ok"] == true {
            native::check(
                &result.unwrap(),
                &row["result"]["value"],
                &format!("{name} native result"),
            );
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.message,
                row["result"]["message"].as_str().unwrap(),
                "{name} native error"
            );
        }
        let J::Array(expected) = native::input(&row["operations"]) else {
            panic!()
        };
        let expected:Vec<J>=expected.into_iter().filter(|v|!matches!(v,J::Array(a)if matches!(a.first(),Some(J::String(n))if n=="caller:id"||n=="error"))).collect();
        let actual:Vec<J>=host.operations.borrow().iter().filter(|v|!matches!(v,J::Array(a)if a.first()==Some(&s("reported:error"))||(a.first()==Some(&s("log"))&&matches!(a.get(2),Some(J::String(level))if level=="error"||level=="trace")))).cloned().collect();
        fn order_fields(value: J) -> J {
            match value {
                J::Object(mut fields) => {
                    fields.sort_by(|a, b| a.0.cmp(&b.0));
                    J::Object(
                        fields
                            .into_iter()
                            .map(|(k, v)| (k, order_fields(v)))
                            .collect(),
                    )
                }
                J::Array(values) => J::Array(values.into_iter().map(order_fields).collect()),
                v => v,
            }
        }
        native::native_eq(
            &order_fields(J::Array(actual)),
            &order_fields(J::Array(expected)),
            &format!("{name} native operations"),
        );
        count += 1;
    }
    assert_eq!(
        count,
        if name == "placeOrder" { 7 } else { 3 },
        "{name} comparison floor"
    );
}
#[test]
fn trading_place_native() {
    run_trading("placeOrder");
}
#[test]
fn trading_update_native() {
    run_trading("updateOrder");
}
#[test]
fn trading_cancel_symbol_native() {
    run_trading("cancelSymbolOrders");
}

#[test]
fn update_order_store_selection_begins_at_first_poll() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/update-calltime.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-UPDATE source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "APEX-UPDATE independent source floor");
    for (row, label) in rows.iter().zip(["clear-after-call", "fill-after-call"]) {
        assert_eq!(row["label"], label, "APEX-UPDATE authored source order");
        let clear = label == "clear-after-call";
        let host = Rc::new(Host::default());
        let services = Rc::new(Services {
            host: Rc::downgrade(&host),
            init: Default::default(),
        });
        *host.services.borrow_mut() = Some(services);
        host.responses
            .borrow_mut()
            .insert("/v3/delete-order", J::from(json!({"data":{}})));
        let store = fixture(&host);
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(
                    json!({"key":"key","secret":"secret","passphrase":"pass","omniKeySeed":"seed"}),
                )
                .unwrap(),
                store.clone(),
                host.clone(),
                Default::default(),
            ),
            host.clone(),
        )
        .unwrap();
        let mut raw = corpus()["fixtures"]["order"].clone();
        raw["size"] = json!("0.006");
        raw["cumSuccessFillSize"] = json!("0");
        let mapped = venue.map_order(&native::capture(&J::from(raw))).unwrap();
        assert_eq!(
            mapped.len(),
            3,
            "APEX-UPDATE independently expected base and brackets"
        );
        store
            .add_or_update_orders_full(if clear { mapped.clone() } else { vec![] })
            .unwrap();
        host.operations.borrow_mut().clear();
        let options: UpdateOrderOpts = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
        let mut future = Box::pin(venue.update_order(&options));
        assert!(
            host.operations.borrow().is_empty(),
            "APEX-UPDATE {label} native unpolled has no operations"
        );
        assert!(
            venue.recently_cancelled_orders.borrow().is_empty(),
            "APEX-UPDATE {label} native unpolled cache empty"
        );
        store
            .add_or_update_orders_full(if clear { vec![] } else { mapped })
            .unwrap();
        let mut context = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let first = future.as_mut().poll(&mut context);
        if clear {
            let std::task::Poll::Ready(Err(error)) = first else {
                panic!("APEX-UPDATE native first poll sees cleared Store")
            };
            assert_eq!(
                error.name, "Error",
                "APEX-UPDATE native missing base error kind"
            );
            assert_eq!(
                error.message, "Base order order-1 not found",
                "APEX-UPDATE native missing base error"
            );
            assert!(
                host.operations.borrow().is_empty(),
                "APEX-UPDATE missing native base makes no requests"
            );
            assert!(
                venue.recently_cancelled_orders.borrow().is_empty(),
                "APEX-UPDATE missing native base has no cancellation"
            );
        } else {
            assert!(
                first.is_pending(),
                "APEX-UPDATE native first poll sees filled Store"
            );
            assert_eq!(
                host.operations.borrow().len(),
                1,
                "APEX-UPDATE native first poll starts one cancellation request"
            );
            let operations = host.operations.borrow();
            let J::Array(request) = &operations[0] else {
                panic!("native request")
            };
            assert_eq!(
                &request[..3],
                &[s("request"), s("post"), s("/v3/delete-order")],
                "APEX-UPDATE native cancellation path"
            );
            let cancelled = venue.recently_cancelled_orders.borrow();
            assert_eq!(
                cancelled.len(),
                1,
                "APEX-UPDATE native first poll cancellation floor"
            );
            assert_eq!(
                cancelled.get("order-1"),
                Some(&1700001234567.0),
                "APEX-UPDATE native selected cancellation"
            );
            assert!(
                store.orders().is_empty(),
                "APEX-UPDATE native selected base and brackets removed"
            );
        }
        drop(future);
        // Source values were captured before its wire argument snapshot; native observations precede comparisons.
        native::check(
            &J::Array(vec![]),
            &row["prefix"]["operations"],
            "APEX-UPDATE source call-time operations",
        );
        native::check(
            &J::Array(vec![]),
            &row["prefix"]["cancelled"],
            "APEX-UPDATE source call-time cancellations",
        );
        assert_eq!(
            row["prefix"]["orders"]["values"].as_array().unwrap().len(),
            if clear { 3 } else { 0 },
            "APEX-UPDATE source call-time Store count"
        );
        if clear {
            assert_eq!(
                row["result"]["ok"], true,
                "APEX-UPDATE clear-after-call source eager selection succeeds"
            );
            native::check(
                &J::Array(vec![s("placed-1")]),
                &row["result"]["value"],
                "APEX-UPDATE source captured replacement",
            );
            native::check(
                &J::Array(vec![J::Array(vec![s("order-1"), n(1700001234567.)])]),
                &row["cancelled"],
                "APEX-UPDATE source captured cancellation",
            );
        } else {
            assert_eq!(
                row["result"]["ok"], false,
                "APEX-UPDATE fill-after-call source eager selection rejects"
            );
            assert_eq!(
                row["result"]["name"], "Error",
                "APEX-UPDATE source missing base kind"
            );
            assert_eq!(
                row["result"]["message"], "Base order order-1 not found",
                "APEX-UPDATE source missing base error"
            );
            native::check(
                &J::Array(vec![]),
                &row["operations"],
                "APEX-UPDATE source rejected before cancellation",
            );
        }
    }
}
