use super::*;
use safe_cex_venue_apexomni::{
    lifecycle::{ApexServices, Cleanup},
    zklink_host::{SdkInitialization, TransactionKind, ZkLinkHandle, ZkLinkHost},
};
use std::{cell::Cell, rc::Weak};
fn lifecycle_corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/lifecycle.cjs"
            ))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "lifecycle source {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
// The source-started positions promise uses the real runtime job queue. Drive
// that queue while awaiting the parent, rather than only after start returns.
fn drive_lifecycle<T>(host: &Host, future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..1024 {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        let tasks = host.tasks.replace(Vec::new());
        for mut task in tasks {
            if task.as_mut().poll(&mut cx).is_pending() {
                host.tasks.borrow_mut().push(task);
            }
        }
    }
    panic!("prepared lifecycle future did not settle while driving runtime jobs");
}
struct Services {
    host: Weak<Host>,
    variant: u64,
    seed_failure: bool,
    init: SdkInitialization,
    scheduled: Cell<usize>,
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
    fn builder(&self, _: TransactionKind, _: Vec<J>) -> Result<ZkLinkHandle, ExchangeError> {
        panic!("not used by lifecycle")
    }
    fn new_transaction(
        &self,
        _: TransactionKind,
        _: ZkLinkHandle,
    ) -> Result<ZkLinkHandle, ExchangeError> {
        panic!("not used by lifecycle")
    }
    fn create_signed_contract(&self, _: ZkLinkHandle) -> Result<J, ExchangeError> {
        panic!("not used by lifecycle")
    }
}
impl ApexServices for Services {
    fn init_external_public_data(&self) -> ExchangeFuture<'_, bool> {
        self.host().record("external:init", vec![]);
        Box::pin(async { Ok(self.variant == 1) })
    }
    fn public_data_only(&self) -> bool {
        self.variant == 2
    }
    fn connect_websocket(&self, private: bool) -> Result<(), ExchangeError> {
        self.host().record("socket:connect", vec![J::Bool(private)]);
        Ok(())
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
            if self.seed_failure {
                Err(ExchangeError::new("bad seed"))
            } else {
                Ok(Rc::new(Services {
                    host: self.host.clone(),
                    variant: 0,
                    seed_failure: false,
                    init: Default::default(),
                    scheduled: Cell::new(0),
                }) as Rc<dyn ZkLinkHost>)
            }
        })
    }
    fn schedule_tick(
        &self,
        _: ExchangeFuture<'static, ()>,
        interval: Option<f64>,
    ) -> Result<(), ExchangeError> {
        self.host()
            .record("loop", vec![interval.map_or(J::Undefined, n)]);
        self.scheduled.set(self.scheduled.get() + 1);
        Ok(())
    }
    fn date_now_ms(&self) -> f64 {
        1700001234567.
    }
    fn throttle_fetch_orders(&self) -> Result<(), ExchangeError> {
        panic!("not used by lifecycle")
    }
    fn place_orders(
        &self,
        _: Vec<safe_cex_types::PreparedOrderOpts>,
    ) -> ExchangeFuture<'_, Vec<String>> {
        panic!("not used by lifecycle")
    }
    fn dayjs_value_ms(&self, input: Option<f64>) -> Result<f64, ExchangeError> {
        Ok(input
            .map(safe_cex_num::js_time_clip)
            .unwrap_or(1700001234567.))
    }
    fn dayjs_subtract(&self, end: f64, amount: f64, unit: &str) -> Result<f64, ExchangeError> {
        Ok(end
            - amount
                * match unit {
                    "m" => 60000.,
                    "h" => 3600000.,
                    "d" => 86400000.,
                    _ => panic!("fixture interval"),
                })
    }
    fn listen_ohlcv(
        &self,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        assert_eq!(
            opts.interval,
            safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1)
        );
        self.host().record(
            "listen:candle",
            vec![obj(vec![("symbol", s(opts.symbol)), ("interval", s("1m"))])],
        );
        callback(Candle {
            timestamp: 1.,
            open: 2.,
            high: 3.,
            low: 4.,
            close: 5.,
            volume: 6.,
            last_side: None,
        })?;
        let host = self.host.clone();
        Ok(Rc::new(move || {
            host.upgrade().unwrap().record("cleanup:candle", vec![]);
            Ok(())
        }))
    }
    fn listen_order_book(
        &self,
        symbol: &str,
        callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        self.host().record("listen:book", vec![s(symbol)]);
        callback(OrderBook {
            bids: vec![],
            asks: vec![],
        })?;
        let host = self.host.clone();
        Ok(Rc::new(move || {
            host.upgrade().unwrap().record("cleanup:book", vec![]);
            Ok(())
        }))
    }
}
fn candle(c: &Candle) -> J {
    assert_eq!(c.last_side, None);
    obj(vec![
        ("timestamp", n(c.timestamp)),
        ("open", n(c.open)),
        ("high", n(c.high)),
        ("low", n(c.low)),
        ("close", n(c.close)),
        ("volume", n(c.volume)),
    ])
}
fn ticker(t: &Ticker) -> J {
    assert_eq!(t.timestamp, None);
    assert_eq!(t.mid_price, None);
    assert_eq!(t.info, None);
    obj(vec![
        ("id", s(&t.id)),
        ("symbol", s(&t.symbol)),
        ("bid", n(t.bid)),
        ("ask", n(t.ask)),
        ("last", n(t.last)),
        ("mark", n(t.mark)),
        ("index", n(t.index)),
        ("percentage", n(t.percentage)),
        ("openInterest", n(t.open_interest)),
        ("fundingRate", n(t.funding_rate)),
        ("fundingTime", n(t.funding_time)),
        ("volume", n(t.volume)),
        ("quoteVolume", n(t.quote_volume)),
    ])
}
fn state(store: &DefaultStore) -> J {
    let loaded = Store::loaded(store).unwrap();
    obj(vec![
        (
            "markets",
            J::Array(Store::markets(store).unwrap().iter().map(market).collect()),
        ),
        (
            "tickers",
            J::Array(Store::tickers(store).unwrap().iter().map(ticker).collect()),
        ),
        (
            "positions",
            J::Array(
                Store::positions(store)
                    .unwrap()
                    .iter()
                    .map(position)
                    .collect(),
            ),
        ),
        (
            "orders",
            J::Array(Store::orders(store).unwrap().iter().map(order).collect()),
        ),
        ("balance", balance(&Store::balance(store).unwrap())),
        (
            "loaded",
            obj(vec![
                ("balance", J::Bool(loaded.balance)),
                ("orders", J::Bool(loaded.orders)),
                ("positions", J::Bool(loaded.positions)),
                ("markets", J::Bool(loaded.markets)),
                ("tickers", J::Bool(loaded.tickers)),
            ]),
        ),
    ])
}
fn run_lifecycle(name: &str) {
    let rows = lifecycle_corpus()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 28, "lifecycle source floor");
    let mut compared = 0;
    for row in rows.iter().filter(|r| r["name"] == name) {
        let variant = row["variant"].as_u64().unwrap();
        let host = Rc::new(Host::default());
        *host.responses.borrow_mut() = responses();
        host.responses
            .borrow_mut()
            .insert("/v3/user", J::Object(vec![]));
        host.responses
            .borrow_mut()
            .insert("/v3/set-initial-margin-rate", J::Object(vec![]));
        host.responses.borrow_mut().insert("/v3/klines",J::from(json!({"data":{"BTCUSDC":[{"t":2000,"o":"10.25","h":"12.25","l":"9.25","c":"11.25","v":"7.25"},{"t":1000,"o":"20.25","h":"22.25","l":"19.25","c":"21.25","v":"8.25"}]}})));
        host.responses.borrow_mut().insert("/v3/symbols",J::from(json!({"data":{"contractConfig":{"perpetualContract":[corpus()["fixtures"]["contract"]]}}})));
        if name == "validateAccount" && variant == 1 {
            let mut error = ExchangeError::new("network cause");
            error.response = Some(json!({"data":{"msg":"bad key"}}));
            host.request_errors.borrow_mut().insert("/v3/user", error);
        }
        let services = Rc::new(Services {
            host: Rc::downgrade(&host),
            variant,
            seed_failure: name == "validateAccount" && variant == 2,
            init: Default::default(),
            scheduled: Cell::new(0),
        });
        *host.services.borrow_mut() = Some(services.clone());
        let store = fixture(&host);
        let opts = serde_json::from_value(
            json!({"key":"key","secret":"secret","passphrase":"pass","extra":{"tickInterval":321}}),
        )
        .unwrap();
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                opts,
                store.clone(),
                host.clone(),
                ExchangeSettings::default(),
            ),
            host.clone(),
        )
        .unwrap();
        host.operations.borrow_mut().clear();
        if variant == 2
            && [
                "fetchAndSetPrivateSnapshot",
                "fetchAndSetPrivateSnapshotHelper",
                "tick",
            ]
            .contains(&name)
        {
            venue.base.dispose().unwrap();
            let old = fixture(&host);
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    markets: Some(Store::markets(old.as_ref()).unwrap().to_vec()),
                    tickers: Some(Store::tickers(old.as_ref()).unwrap().to_vec()),
                    positions: Some(Store::positions(old.as_ref()).unwrap().to_vec()),
                    balance: Some(Store::balance(old.as_ref()).unwrap().clone()),
                    zk_link_config: Some(json!({"id":"old","l2Key":"old-key"}).into()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        if variant == 1 && name.contains("Snapshot") {
            let stamp = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms() - 500.;
            FIXTURE_STAMPS.with(|stamps| stamps.borrow_mut().push((stamp, 1700001234067.)));
            venue.last_websocket_positions_update.set(Some(stamp));
            let mut p = Store::positions(store.as_ref()).unwrap().to_vec();
            p[0].last_update = Some(stamp).into();
            p[0].id = "BTC-USDC:short".into();
            p[0].side = safe_cex_types::ModelField::Value(PositionSide::Short);
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    positions: Some(p),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let notifications = Rc::new(Cell::new(0usize));
        let counter = notifications.clone();
        let unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                counter.set(counter.get() + 1);
                Ok(())
            }),
        );
        let J::Array(args) = native::input(&row["args"]) else {
            panic!()
        };
        let actual = drive_lifecycle(host.as_ref(), async {
            match name {
                "validateAccount" => obj(vec![(
                    "error",
                    J::from(venue.validate_account().await.unwrap().into_value()),
                )]),
                "start" => {
                    venue.start().await.unwrap();
                    J::Undefined
                }
                "fetchAndSetPrivateSnapshot" => {
                    venue.fetch_and_set_private_snapshot().await.unwrap();
                    J::Undefined
                }
                "fetchAndSetPrivateSnapshotHelper" => {
                    venue.fetch_and_set_private_snapshot_helper().await.unwrap();
                    J::Undefined
                }
                "tick" => {
                    venue.tick().await.unwrap();
                    J::Undefined
                }
                "fetchOHLCV" => {
                    let opts = OHLCVOptions {
                        symbol: "BTCUSDC".into(),
                        interval: if variant == 1 {
                            safe_cex_types::AnyTimeframe::Variant1(Timeframe::D1)
                        } else {
                            safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1)
                        },
                        limit: Some(if variant == 2 {
                            0.
                        } else if variant == 3 {
                            0.000013333333333333333
                        } else {
                            3.
                        }),
                        to: if variant == 2 {
                            None
                        } else {
                            Some(if variant == 3 { 1000.9 } else { 1700001234567. })
                        },
                        from: Some(1700000000000.),
                    };
                    J::Array(
                        venue
                            .fetch_ohlcv(&OHLCVOptions {
                                from: if variant == 2 { opts.from } else { None },
                                ..opts
                            })
                            .await
                            .unwrap()
                            .iter()
                            .map(candle)
                            .collect(),
                    )
                }
                "listenOHLCV" => {
                    let opts = OHLCVOptions {
                        symbol: "BTCUSDC".into(),
                        interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1),
                        from: None,
                        to: None,
                        limit: None,
                    };
                    let h = host.clone();
                    let clean = venue
                        .listen_ohlcv(
                            opts,
                            Rc::new(move |c| {
                                h.record("callback:candle", vec![candle(&c)]);
                                Ok(())
                            }),
                        )
                        .unwrap();
                    clean().unwrap();
                    J::Undefined
                }
                "listenOrderBook" => {
                    let h = host.clone();
                    let clean = venue
                        .listen_order_book(
                            "BTCUSDC",
                            Rc::new(move |book| {
                                assert!(book.bids.is_empty() && book.asks.is_empty());
                                h.record(
                                    "callback:book",
                                    vec![obj(vec![
                                        ("bids", J::Array(vec![])),
                                        ("asks", J::Array(vec![])),
                                    ])],
                                );
                                Ok(())
                            }),
                        )
                        .unwrap();
                    clean().unwrap();
                    J::Undefined
                }
                "setLeverage" => {
                    let J::Number(input) = args[1] else { panic!() };
                    venue.set_leverage("BTCUSDC", input).await.unwrap();
                    J::Undefined
                }
                _ => panic!(),
            }
        });
        loop {
            let tasks = host.tasks.replace(Vec::new());
            if tasks.is_empty() {
                break;
            }
            for task in tasks {
                futures::executor::block_on(task);
            }
        }
        native::check_fields(
            &actual,
            &row["result"],
            &format!("{name} lifecycle native result"),
        );
        native::check_fields(
            &state(store.as_ref()),
            &row["state"],
            &format!("{name} lifecycle native Store"),
        );
        assert_eq!(
            notifications.get(),
            row["notifications"].as_u64().unwrap() as usize,
            "{name} lifecycle native notification count"
        );
        unsubscribe();
        assert_eq!(
            services.scheduled.get(),
            row["tasks"].as_u64().unwrap() as usize,
            "{name} lifecycle native timer count"
        );
        let J::Array(expected) = native::input(&row["operations"]) else {
            panic!()
        };
        let expected:Vec<J>=expected.into_iter().filter(|v|!matches!(v,J::Array(a) if matches!(a.first(),Some(J::String(n)) if n.starts_with("store:")||n=="error"))).collect();
        let actual:Vec<J>=host.operations.borrow().iter().filter(|v|!matches!(v,J::Array(a) if a.first()==Some(&s("reported:error"))||(a.first()==Some(&s("log"))&&matches!(a.get(2),Some(J::String(level)) if level=="error"||level=="trace")))).cloned().collect();
        fields_eq(
            &J::Array(actual),
            &J::Array(expected),
            &format!("{name} lifecycle native operations"),
        );
        if name == "validateAccount" && variant == 1 {
            assert!(
                host.operations.borrow().iter().any(
                    |v| *v == J::Array(vec![s("reported:error"), s("Exchange Error: bad key")])
                ),
                "validateAccount native error cause"
            );
        }
        compared += 1;
    }
    assert_eq!(
        compared,
        if name == "fetchOHLCV" { 4 } else { 3 },
        "{name} lifecycle comparison floor"
    );
}
fn fields_eq(actual: &J, expected: &J, label: &str) {
    fn order(value: J) -> J {
        match value {
            J::Object(mut fields) => {
                fields.sort_by(|a, b| a.0.cmp(&b.0));
                J::Object(fields.into_iter().map(|(k, v)| (k, order(v))).collect())
            }
            J::Array(values) => J::Array(values.into_iter().map(order).collect()),
            v => v,
        }
    }
    native::native_eq(&order(actual.clone()), &order(expected.clone()), label);
}

#[test]
fn lifecycle_validate() {
    run_lifecycle("validateAccount");
}
#[test]
fn lifecycle_start() {
    run_lifecycle("start");
}
#[test]
fn lifecycle_snapshot() {
    run_lifecycle("fetchAndSetPrivateSnapshot");
}
#[test]
fn lifecycle_snapshot_helper() {
    run_lifecycle("fetchAndSetPrivateSnapshotHelper");
}
#[test]
fn lifecycle_tick() {
    run_lifecycle("tick");
}
#[test]
fn lifecycle_candles() {
    run_lifecycle("fetchOHLCV");
}
#[test]
fn lifecycle_listen_candle() {
    run_lifecycle("listenOHLCV");
}
#[test]
fn lifecycle_listen_book() {
    run_lifecycle("listenOrderBook");
}
#[test]
fn lifecycle_leverage() {
    run_lifecycle("setLeverage");
}

#[test]
fn candle_response_shapes_match_actual_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/candle-shapes.cjs"
        ))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-CANDLE source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 24, "APEX-CANDLE independent source floor");
    let mut count = 0;
    let mut errors = 0;
    for row in rows {
        let label = format!("APEX-CANDLE {}", row["label"].as_str().unwrap());
        let host = Rc::new(Host::default());
        host.responses
            .borrow_mut()
            .insert("/v3/klines", native::input(&row["payload"]));
        let services = Rc::new(Services {
            host: Rc::downgrade(&host),
            variant: 0,
            seed_failure: false,
            init: Default::default(),
            scheduled: Cell::new(0),
        });
        *host.services.borrow_mut() = Some(services);
        let venue = ApexOmniExchange::new(
            BaseExchange::new(
                ExchangeName::Apexomni,
                serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
                fixture(&host),
                host.clone(),
                Default::default(),
            ),
            host.clone(),
        )
        .unwrap();
        host.operations.borrow_mut().clear();
        let opts = OHLCVOptions {
            symbol: "BTCUSDC".into(),
            interval: safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1),
            from: None,
            to: Some(1700001234567.),
            limit: Some(3.),
        };
        match futures::executor::block_on(venue.fetch_ohlcv(&opts)) {
            Ok(candles) => {
                assert_eq!(
                    row["result"]["ok"], true,
                    "{label} native expected rejection"
                );
                native::check_fields(
                    &J::Array(candles.iter().map(candle).collect()),
                    &row["result"]["value"],
                    &format!("{label} native candles"),
                );
            }
            Err(error) => {
                assert_eq!(
                    row["result"]["ok"], false,
                    "{label} native unexpected rejection: {error:?}"
                );
                assert_eq!(
                    error.name,
                    row["result"]["name"].as_str().unwrap(),
                    "{label} native error kind"
                );
                assert_eq!(
                    error.message,
                    row["result"]["message"].as_str().unwrap(),
                    "{label} native error message"
                );
                errors += 1;
            }
        }
        native::check_fields(
            &J::Array(host.operations.borrow().clone()),
            &row["operations"],
            &format!("{label} native operations"),
        );
        count += 1;
    }
    assert_eq!(count, 24, "APEX-CANDLE independent executed floor");
    assert_eq!(errors, 6, "APEX-CANDLE independently selected error floor");
}
