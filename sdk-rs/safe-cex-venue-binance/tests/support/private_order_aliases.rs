use super::*;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_venue_binance::exchange_classes::Method;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct Http {
    requests: Mutex<Vec<RestRequest>>,
    routes: Mutex<std::collections::BTreeMap<String, (u16, String)>>,
    held: AtomicBool,
    released: Mutex<std::collections::BTreeSet<String>>,
}
impl Http {
    fn response(&self, path: &str, status: u16, data: Value) {
        self.routes
            .lock()
            .unwrap()
            .insert(path.into(), (status, data.to_string()));
    }
    fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.url.split('?').next() == Some(path))
            .count()
    }
}
impl RestAdapter for Http {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let path = request.url.split('?').next().unwrap().to_owned();
        self.requests.lock().unwrap().push(request.clone());
        let (status, body) = self
            .routes
            .lock()
            .unwrap()
            .get(&path)
            .cloned()
            .unwrap_or_else(|| panic!("unscripted actual native request: {path}"));
        Box::pin(std::future::poll_fn(move |_| {
            if self.held.load(Ordering::SeqCst) && !self.released.lock().unwrap().contains(&path) {
                return Poll::Pending;
            }
            let response = RestResponse {
                status,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body.as_bytes().to_vec()),
            };
            Poll::Ready(if (200..300).contains(&status) {
                Ok(response)
            } else {
                Err(safe_cex_transport::RestFailure {
                    code: Some(
                        if status < 500 {
                            "ERR_BAD_REQUEST"
                        } else {
                            "ERR_BAD_RESPONSE"
                        }
                        .into(),
                    ),
                    transport_error: None,
                    message: format!("Request failed with status code {status}"),
                    response: Some(Box::new(response)),
                    request: Box::new(request.clone()),
                    context: None,
                })
            })
        }))
    }
}

#[derive(Default)]
struct Wire {
    sent: RefCell<Vec<String>>,
    opened: Cell<bool>,
}
impl WsSocket for Wire {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        if !self.opened.replace(true) {
            Box::pin(async { Ok(WsEvent::Open) })
        } else {
            Box::pin(std::future::pending())
        }
    }
    fn send(&self, text: &str) -> Result<(), ExchangeError> {
        self.sent.borrow_mut().push(text.into());
        Ok(())
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}

fn owner(http: Arc<Http>, runtime: Rc<Runtime>) -> Rc<ExchangeOwner> {
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime,
        http,
        Rc::new(|_, _| panic!("no external provider")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    safe_cex_store::Store::update(store.as_ref(), safe_cex_store::StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.1},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":100}}})).unwrap()]),
        tickers: Some(vec![serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":99,"ask":101,"last":100,"mark":100,"index":100,"percentage":0,"volume":0,"quoteVolume":0,"fundingRate":0,"fundingTime":0,"openInterest":0})).unwrap()]),
        ..Default::default()
    }, false).unwrap();
    ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"alias-key","secret":"alias-secret"})).unwrap(),
        store,
        deps,
    )
    .unwrap()
}
fn order() -> EventValue {
    EventValue::from_json(
        json!({"id":"9007199254740993","symbol":"BTCUSDT","type":"limit","side":"buy","price":91.2,"amount":3,"filled":0,"remaining":3,"status":"open","reduceOnly":false,"lastUpdate":0}),
    )
}
fn text(value: &EventValue) -> String {
    String::from_utf16(value.as_str().expect("native String").as_ref()).unwrap()
}
fn numbers(value: &EventValue) -> f64 {
    let EventValue::Number(number) = value else {
        panic!("expected native Number")
    };
    *number
}
fn poll<T>(runtime: &Runtime, future: &mut Pin<Box<impl Future<Output = T>>>) -> Poll<T> {
    for _ in 0..12 {
        runtime.pump();
    }
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}
fn events(owner: &ExchangeOwner, name: &str) -> Rc<RefCell<Vec<Vec<EventValue>>>> {
    let output = Rc::new(RefCell::new(vec![]));
    let sink = output.clone();
    owner.base.on_args(
        name,
        Rc::new(move |_, args| {
            sink.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    output
}

#[test]
fn native_cancel_capabilities_wait_for_http_before_store_status() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/order", 200, json!({}));
    http.held.store(true, Ordering::SeqCst);
    let owner = owner(http.clone(), runtime.clone());
    let row = order();
    owner
        .base
        .store
        .add_or_update_order(safe_cex_exchange::js_runtime::value::to_typed(&row).unwrap())
        .unwrap();
    let mut call = Box::pin(owner.invoke(Method::Cancel, vec![EventValue::array(vec![row])]));
    assert!(poll(&runtime, &mut call).is_pending());
    assert_eq!(http.count("/fapi/v1/order"), 1);
    assert_eq!(
        owner.base.store.orders()[0].status,
        safe_cex_types::OrderStatus::Open
    );
    let request = http.requests.lock().unwrap()[0].clone();
    assert!(request.method.eq_ignore_ascii_case("DELETE"));
    assert!(request.url.contains("symbol=BTCUSDT"));
    assert!(request.url.contains("orderId=9007199254740993"));
    http.held.store(false, Ordering::SeqCst);
    assert!(matches!(
        run(&runtime, call).unwrap(),
        EventValue::Undefined
    ));
    assert_eq!(
        owner.base.store.orders()[0].status,
        safe_cex_types::OrderStatus::Canceled
    );
    owner.dispose().unwrap();
}

#[test]
fn native_cancel_symbol_handles_wait_for_both_signed_deletes_and_keep_other_symbols() {
    use safe_cex_types::{OrderStatus, OrderType};

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cancel-order-handle-source.cjs"
        ))
        .output()
        .expect("constructed TypeScript cancel-symbol witness");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (case, algo_status) in [200, 500].into_iter().enumerate() {
        let runtime = Rc::new(Runtime::default());
        let http = Arc::new(Http::default());
        http.response("/fapi/v1/allOpenOrders", 200, json!({}));
        http.response(
            "/fapi/v1/algoOpenOrders",
            algo_status,
            json!({"msg":"algo refused"}),
        );
        http.held.store(true, Ordering::SeqCst);
        let owner = owner(http.clone(), runtime.clone());
        let errors = events(&owner, "error");
        let regular: safe_cex_types::Order =
            safe_cex_exchange::js_runtime::value::to_typed(&order()).unwrap();
        let mut algo = regular.clone();
        algo.id = "algo-order".into();
        algo.r#type = OrderType::StopLoss.into();
        let mut sibling = regular.clone();
        sibling.id = "other-symbol".into();
        sibling.symbol = "ETHUSDT".into();
        for row in [regular, algo, sibling] {
            owner.base.store.add_or_update_order(row).unwrap();
        }
        let mut call = Box::pin(owner.invoke(Method::CancelSymbol, vec![string("BTCUSDT")]));
        assert!(poll(&runtime, &mut call).is_pending());
        assert_eq!(http.count("/fapi/v1/allOpenOrders"), 1);
        assert_eq!(http.count("/fapi/v1/algoOpenOrders"), 1);
        assert!(
            owner
                .base
                .store
                .orders()
                .iter()
                .all(|row| row.status == OrderStatus::Open)
        );
        for request in http.requests.lock().unwrap().iter() {
            assert!(request.method.eq_ignore_ascii_case("DELETE"));
            assert!(request.url.contains("symbol=BTCUSDT"));
            assert!(request.url.contains("timestamp="));
            assert!(request.url.contains("signature="));
            assert!(!request.url.contains("ETHUSDT"));
        }
        http.released
            .lock()
            .unwrap()
            .insert("/fapi/v1/allOpenOrders".into());
        assert!(
            poll(&runtime, &mut call).is_pending(),
            "algo reply is still held"
        );
        assert!(
            owner
                .base
                .store
                .orders()
                .iter()
                .all(|row| row.status == OrderStatus::Open)
        );
        http.held.store(false, Ordering::SeqCst);
        let result = run(&runtime, call);
        assert_eq!(result.is_ok(), algo_status == 200);
        if let Err(error) = &result {
            let response = error
                .response
                .as_ref()
                .expect("retained HTTP rejection response");
            assert_eq!(response["status"], 500);
            assert_eq!(response["data"]["msg"], "algo refused");
        }
        let rows = owner.base.store.orders();
        assert_eq!(rows.len(), 3);
        for row in rows.iter() {
            let expected =
                if row.id == "other-symbol" || (row.id == "algo-order" && algo_status != 200) {
                    OrderStatus::Open
                } else {
                    OrderStatus::Canceled
                };
            assert_eq!(
                row.status, expected,
                "cancel-symbol handle status for {}",
                row.id
            );
        }
        assert_eq!(errors.borrow().len(), 2 * usize::from(algo_status != 200));
        let mut observed = json!({
            "ok":result.is_ok(),
            "orders":rows.iter().map(|row| json!([row.id,row.status])).collect::<Vec<_>>(),
            "errors":errors.borrow().len(),
        });
        if let Err(error) = &result {
            let response = error.response.as_ref().unwrap();
            observed["status"] = response["status"].clone();
            observed["msg"] = response["data"]["msg"].clone();
        }
        assert_eq!(
            observed, source[case],
            "constructed TypeScript cancel-symbol result"
        );
        drop(rows);
        owner.dispose().unwrap();
    }
}

#[test]
fn native_update_capabilities_cancel_then_format_and_place() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/order", 200, json!({"orderId":"new-order"}));
    let owner = owner(http.clone(), runtime.clone());
    let result = run(
        &runtime,
        owner.invoke(
            Method::UpdateOrder,
            vec![record([
                ("order", order()),
                ("update", record([("price", number(92.3))])),
            ])],
        ),
    )
    .unwrap();
    let ids = result.array_values().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(text(&ids[0]), "new-order");
    let requests = http.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests[0].method.eq_ignore_ascii_case("DELETE"));
    assert!(requests[1].method.eq_ignore_ascii_case("POST"));
    assert!(requests[0].url.contains("orderId=9007199254740993"));
    assert!(requests[1].url.contains("price=92.3"));
    assert!(requests[1].url.contains("quantity=3"));
    assert!(requests[1].url.contains("type=LIMIT"));
    drop(requests);
    owner.dispose().unwrap();
}

#[test]
fn native_batch_capabilities_classify_rejections_and_log_close_conflicts() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/batchOrders",200,json!([{"orderId":"accepted"},{"code":-5022,"msg":"Post Only order rejected"},{"code":-1100,"msg":"alias row failure"}]));
    let owner = owner(http.clone(), runtime.clone());
    let errors = events(&owner, "error");
    let result = run(
        &runtime,
        owner.invoke(
            Method::RegularBatch,
            vec![EventValue::array(vec![
                record([("symbol", string("BTCUSDT"))]),
                record([("symbol", string("BTCUSDT"))]),
            ])],
        ),
    )
    .unwrap();
    let ids = result.array_values().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(text(&ids[0]), "accepted");
    let errors = errors.borrow();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].len(), 2);
    assert!(text(&errors[0][0]).contains("alias row failure"));
    drop(errors);
    assert_eq!(http.count("/fapi/v1/batchOrders"), 1);
    let raw = http.requests.lock().unwrap()[0].url.clone();
    assert!(raw.contains("batchOrders="));
    owner.base.store.add_or_update_order(serde_json::from_value(json!({"id":"close-1","symbol":"BTCUSDT","type":"stop_market","side":"buy","price":80,"amount":0,"filled":0,"remaining":0,"status":"open","reduceOnly":true,"close":true,"lastUpdate":0})).unwrap()).unwrap();
    http.response("/fapi/v1/algoOrder", 200, json!({}));
    let logs = events(&owner, "log");
    run(
        &runtime,
        owner.invoke(
            Method::ConflictCancel,
            vec![record([
                ("symbol", string("BTCUSDT")),
                ("type", string("stop_market")),
                ("side", string("buy")),
                ("amount", number(0.0)),
                ("close", EventValue::Bool(true)),
            ])],
        ),
    )
    .unwrap();
    assert_eq!(http.count("/fapi/v1/algoOrder"), 1);
    assert_eq!(logs.borrow().len(), 1);
    assert_eq!(logs.borrow()[0].len(), 2);
    assert_eq!(
        text(&logs.borrow()[0][0]),
        "[binance] Canceling 1 existing closePosition order(s) before placing new one"
    );
    assert_eq!(text(&logs.borrow()[0][1]), "info");
    owner.dispose().unwrap();
}

#[test]
fn native_rate_hook_defers_fourth_private_read_until_window_opens() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/income", 200, json!([]));
    let owner = owner(http.clone(), runtime.clone());
    for _ in 0..3 {
        run(&runtime, owner.invoke(Method::IncomePage, vec![record([])])).unwrap();
    }
    let mut fourth = Box::pin(owner.invoke(Method::IncomePage, vec![record([])]));
    assert!(poll(&runtime, &mut fourth).is_pending());
    assert_eq!(http.count("/fapi/v1/income"), 3);
    runtime.monotonic_clock.set(Some(23001.0));
    runtime.sleep_clock.set(1001.0);
    assert!(matches!(
        run(&runtime, fourth).unwrap(),
        EventValue::Array(_)
    ));
    assert_eq!(http.count("/fapi/v1/income"), 4);
    owner.dispose().unwrap();
}

#[test]
fn native_key_and_retry_capabilities_reconnect_and_cancel_refresh() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/openOrders", 200, json!([]));
    http.response("/fapi/v1/openAlgoOrders", 200, json!([]));
    http.response(
        "/fapi/v1/listenKey",
        400,
        json!({"code":-2015,"msg":"key unavailable"}),
    );
    let owner = owner(http.clone(), runtime.clone());
    let errors = events(&owner, "error");
    run(&runtime, owner.private.connect_and_subscribe()).unwrap();
    for _ in 0..12 {
        runtime.pump();
    }
    assert_eq!(http.count("/fapi/v1/listenKey"), 1);
    assert!(!owner.private.connect.connecting.get());
    assert!(owner.private.connect.resync_orders.get());
    assert!(!errors.borrow().is_empty());
    assert!(runtime.sleeps.borrow().contains(&5000));
    http.response(
        "/fapi/v1/listenKey",
        200,
        json!({"listenKey":"retained-key"}),
    );
    let wire = Rc::new(Wire::default());
    *runtime.dial_socket.borrow_mut() = Some(wire);
    runtime.now.set(5001.0);
    runtime.sleep_clock.set(5001.0);
    for _ in 0..24 {
        runtime.pump();
    }
    assert_eq!(http.count("/fapi/v1/listenKey"), 2);
    assert!(!owner.private.connect.connecting.get());
    assert_eq!(
        owner.private.connect.retry_delay.get().to_bits(),
        5000.0f64.to_bits()
    );
    assert!(runtime.sleeps.borrow().contains(&1800000));
    owner.dispose().unwrap();
    runtime.now.set(1806002.0);
    runtime.sleep_clock.set(1806002.0);
    for _ in 0..12 {
        runtime.pump();
    }
    assert_eq!(
        http.count("/fapi/v1/listenKey"),
        2,
        "disposed refresh must not run"
    );
}

#[test]
fn native_key_future_stays_pending_until_terminal_response() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/listenKey", 200, json!({"listenKey":"held-key"}));
    http.held.store(true, Ordering::SeqCst);
    let owner = owner(http.clone(), runtime.clone());
    *runtime.dial_socket.borrow_mut() = Some(Rc::new(Wire::default()));
    let mut call = Box::pin(owner.private.connect_and_subscribe());
    assert!(poll(&runtime, &mut call).is_pending());
    assert!(owner.private.connect.connecting.get());
    assert_eq!(http.count("/fapi/v1/listenKey"), 1);
    http.held.store(false, Ordering::SeqCst);
    run(&runtime, call).unwrap();
    for _ in 0..12 {
        runtime.pump();
    }
    assert!(!owner.private.connect.connecting.get());
    assert!(runtime.sleeps.borrow().contains(&1800000));
    owner.dispose().unwrap();
}

#[test]
fn native_private_handler_and_event_array_keep_numeric_fields_before_transport() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    let owner = owner(http, runtime);
    owner
        .base
        .store
        .add_or_update_order(safe_cex_exchange::js_runtime::value::to_typed(&order()).unwrap())
        .unwrap();
    let handler: PrivateHandler = owner
        .private
        .parent
        .capture_store(PrivateOperation::UpdateOrder)
        .unwrap();
    handler(vec![
        record([("id", string("9007199254740993"))]),
        record([("filled", number(-0.0)), ("price", number(12.5))]),
    ])
    .unwrap();
    assert_eq!(
        owner.base.store.orders()[0].filled.to_bits(),
        (-0.0f64).to_bits()
    );
    let rows: EventArray = owner.private.parent.orders().unwrap();
    assert_eq!(rows.borrow().len(), 1);
    let row = rows.borrow()[0].clone().expect("own row");
    assert_eq!(text(&row.read("id").unwrap()), "9007199254740993");
    assert_eq!(
        numbers(&row.read("filled").unwrap()).to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        numbers(&row.read("price").unwrap()).to_bits(),
        12.5f64.to_bits()
    );
    let same = rows.clone();
    assert!(Rc::ptr_eq(&same, &rows));
    owner.dispose().unwrap();
}
#[test]
fn native_bulk_placement_retains_post_result_until_signed_http_settles() {
    let runtime = Rc::new(Runtime::default());
    let http = Arc::new(Http::default());
    http.response("/fapi/v1/order", 200, json!({"orderId":"9007199254740993"}));
    http.held.store(true, Ordering::SeqCst);
    let owner = owner(http.clone(), runtime.clone());
    let opts = EventValue::from_json(json!({
        "symbol":"BTCUSDT", "type":"limit", "side":"buy",
        "price":91.2, "amount":3, "clientOrderId":"batch-native"
    }));
    let mut call = Box::pin(owner.invoke(Method::PlaceOrders, vec![EventValue::array(vec![opts])]));
    assert!(poll(&runtime, &mut call).is_pending());
    assert_eq!(http.count("/fapi/v1/order"), 1);
    let request = http.requests.lock().unwrap()[0].clone();
    assert!(request.method.eq_ignore_ascii_case("POST"));
    assert!(request.url.contains("symbol=BTCUSDT"));
    assert!(request.url.contains("price=91.2"));
    assert!(request.url.contains("quantity=3"));
    assert!(request.url.contains("signature="));
    assert!(request.url.contains("newClientOrderId=batch-native"));
    http.held.store(false, Ordering::SeqCst);
    let result = run(&runtime, call).unwrap();
    let rows = result.array_values().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(text(&rows[0]), "9007199254740993");
    assert_eq!(http.count("/fapi/v1/order"), 1);
    owner.dispose().unwrap();
}
