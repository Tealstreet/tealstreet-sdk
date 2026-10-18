#[path = "support/data_trace.rs"]
mod data_trace;
#[path = "support/error_post.rs"]
mod error_post_support;
#[path = "support/error_trace.rs"]
mod error_trace;
#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
#[path = "../../safe-cex-exchange/tests/support/store.rs"]
mod test_store;
#[path = "support/ws_runtime.rs"]
mod ws_runtime;
use runtime::Runtime;
use safe_cex_exchange::{capabilities::ExchangeCapabilities, exchange::*, runtime::*};
use safe_cex_types::*;
use safe_cex_webworker::{command::*, payload::*, positions::UpdatePositionsHandler};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
use test_store::TestStore;

struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    result: RefCell<Vec<Position>>,
    delay: Cell<u64>,
    fails: Cell<bool>,
    failure: RefCell<Option<ExchangeError>>,
    fetches: RefCell<Vec<Option<Vec<String>>>>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        Box::pin(async move {
            self.fetches.borrow_mut().push(symbols);
            self.base.runtime.sleep(self.delay.get()).await?;
            if let Some(error) = self.failure.borrow().clone() {
                return Err(error);
            }
            if self.fails.get() {
                return Err(ExchangeError::new("fetch failed"));
            }
            Ok(self.result.borrow().clone())
        })
    }
}
fn venue() -> (Rc<Venue>, Rc<Runtime>, Rc<TestStore>) {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(100_000);
    let store = Rc::new(TestStore::new());
    let base = BaseExchange::new(
        ExchangeName::Bybit,
        serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        store.clone(),
        runtime.clone(),
        Default::default(),
    );
    (
        Rc::new(Venue {
            base,
            capabilities: Default::default(),
            result: RefCell::default(),
            delay: Cell::new(0),
            fails: Cell::new(false),
            failure: RefCell::new(None),
            fetches: RefCell::default(),
        }),
        runtime,
        store,
    )
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("expected ready"),
    }
}
fn position() -> Position {
    serde_json::from_value(json!({"id":"BTC-long","symbol":"BTC","side":"long","entryPrice":100,"notional":100,"leverage":2,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap()
}
#[test]
fn full_empty_refresh_clears_omitted_positions_but_scoped_empty_is_partial() {
    let (venue, _, store) = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler =
        UpdatePositionsHandler::new(venue, Rc::new(move |msg| sink.borrow_mut().push(msg)));
    store.data.borrow_mut().positions = vec![position()];
    ready(handler.handle_update_positions("scoped".into(), Some(vec!["BTC".into()])));
    assert_eq!(store.data.borrow().positions.len(), 1);
    ready(handler.handle_update_positions("all".into(), None));
    assert!(store.data.borrow().positions.is_empty());
    assert_eq!(messages.borrow().len(), 2);
    assert_eq!(messages.borrow()[1].data.as_ref().unwrap()["success"], true);
}
#[test]
fn position_cancellation_releases_owned_slot_and_settles_only_its_waiter() {
    let (venue, runtime, _) = venue();
    venue.delay.set(100);
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = UpdatePositionsHandler::new(
        venue.clone(),
        Rc::new(move |msg| sink.borrow_mut().push(msg)),
    );
    let mut request = Box::pin(handler.handle_update_positions("dropped".into(), None));
    assert!(
        request
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    ready(handler.handle_update_positions("joined".into(), None));
    drop(request);
    venue.delay.set(0);
    runtime.advance(100_010);
    ready(handler.handle_update_positions("retry".into(), None));
    assert_eq!(venue.fetches.borrow().len(), 2);
    assert_eq!(
        messages.borrow()[1].data.as_ref().unwrap()["reason"],
        "Worker request cancelled"
    );
    assert_eq!(messages.borrow()[2].data.as_ref().unwrap()["success"], true);
}
fn exec(req_id: Option<&str>) -> StoreWorkerPayload {
    let mut data = json!({"symbol":"BTC","command":"buy 1"});
    if let Some(id) = req_id {
        data["reqId"] = json!(id);
    }
    StoreWorkerPayload {
        op: StoreWorkerOp::Exec,
        instance_id: None,
        data: Some(data),
    }
}
#[test]
fn command_failures_echo_only_present_reqid_and_preserve_executor_results() {
    reset_command_executor();
    let (venue, runtime, _) = venue();
    let events = Rc::new(RefCell::new(Vec::<(String, Value)>::new()));
    for event in ["exec-log", "exec-result"] {
        let sink = events.clone();
        venue.base.on(
            event,
            Rc::new(move |name, data| {
                sink.borrow_mut().push((
                    name.into(),
                    safe_cex_exchange::js_runtime::value::to_json(data).unwrap(),
                ));
                Ok(())
            }),
        );
    }
    let handler = CommandHandler::new(venue);
    handler.handle(exec(None)).unwrap();
    assert_eq!(events.borrow().len(), 1);
    assert!(events.borrow()[0].1.get("reqId").is_none());
    handler.handle(exec(Some(""))).unwrap();
    assert_eq!(
        events.borrow()[2].1,
        json!({"reqId":"","handled":false,"success":false,"error":"Command execution is not available in this environment."})
    );
    events.borrow_mut().clear();
    register_command_executor(Rc::new(|_, _, _, _| {
        Box::pin(async { Err(ExchangeError::new("boom")) })
    }));
    handler.handle(exec(None)).unwrap();
    handler.handle(exec(Some("req"))).unwrap();
    runtime.advance(100_000);
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|(name, _)| name == "exec-result")
            .count(),
        1
    );
    assert_eq!(events.borrow().last().unwrap().1["success"], false);
    events.borrow_mut().clear();
    register_command_executor(Rc::new(|exchange, _, _, req| {
        Box::pin(async move {
            if let Some(req) = req {
                exchange.base().emit("exec-result",safe_cex_exchange::js_runtime::value::from_json(&json!({"reqId":req,"handled":true,"success":false,"error":"risk rejected","data":{"orderIds":["id"]}})))?;
            } else {
                exchange.base().emit(
                    "exec-log",
                    safe_cex_exchange::js_runtime::value::from_json(
                        &json!({"message":"risk rejected"}),
                    ),
                )?;
            }
            Ok(())
        })
    }));
    handler.handle(exec(Some("handled-failure"))).unwrap();
    handler.handle(exec(None)).unwrap();
    runtime.advance(100_000);
    assert_eq!(events.borrow().len(), 2);
    assert_eq!(events.borrow()[0].1["success"], false);
    assert_eq!(events.borrow()[0].1["error"], "risk rejected");
    assert_eq!(events.borrow()[0].1["data"]["orderIds"], json!(["id"]));
    reset_command_executor();
}

#[test]
fn market_buffer_timeout_cooldown_reset_and_disposal_are_independent() {
    use safe_cex_webworker::handler::BaseHandler;
    let (venue, runtime, store) = venue();
    let sent = Rc::new(RefCell::new(Vec::new()));
    let sink = sent.clone();
    let replayed = Rc::new(RefCell::new(Vec::new()));
    let replay = replayed.clone();
    let handler = BaseHandler::new(
        venue,
        "OHLCVSubscriptionsHandler".into(),
        error_trace::post(Rc::new(move |msg| sink.borrow_mut().push(msg))),
        Rc::new(move |msg| replay.borrow_mut().push(msg)),
    );
    let subscription = StoreWorkerPayload {
        op: StoreWorkerOp::SubscribeOHLCV,
        instance_id: None,
        data: Some(json!({"symbol":"SECRET_SYMBOL","account":"PRIVATE_ACCOUNT","subId":"a"})),
    };
    assert!(
        handler
            .buffer_until_markets_loaded(subscription.clone())
            .unwrap()
    );
    runtime.advance(129_999);
    assert!(sent.borrow().is_empty());
    runtime.advance(130_000);
    assert_eq!(sent.borrow().len(), 1);
    let data = sent.borrow()[0].data.clone().unwrap();
    assert_eq!(
        data[1]["sentryData"]["tags"]["worker_error_family"],
        "markets_not_loaded"
    );
    assert_eq!(
        data[1]["sentryData"]["extra"]["pendingOpCounts"]["subscribeOHLCV"],
        1
    );
    assert_eq!(data[1]["sentryData"]["extra"]["pendingAgeMs"], 30_000.0);
    assert!(!data.to_string().contains("SECRET_SYMBOL"));
    assert!(!data.to_string().contains("PRIVATE_ACCOUNT"));
    handler
        .buffer_until_markets_loaded(subscription.clone())
        .unwrap();
    runtime.advance(160_000);
    assert_eq!(sent.borrow().len(), 1);
    runtime.advance(430_000);
    handler
        .buffer_until_markets_loaded(subscription.clone())
        .unwrap();
    runtime.advance(460_000);
    assert_eq!(sent.borrow().len(), 2);
    assert_eq!(
        sent.borrow()[1].data.as_ref().unwrap()[1]["sentryData"]["extra"]["suppressedPendingTimeoutReports"],
        1
    );
    handler
        .buffer_until_markets_loaded(subscription.clone())
        .unwrap();
    store.data.borrow_mut().markets=vec![serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":5},"leverage":{"min":1,"max":100}}})).unwrap()];
    runtime.advance(460_500);
    assert_eq!(replayed.borrow().len(), 1);
    store.data.borrow_mut().markets.clear();
    handler
        .buffer_until_markets_loaded(subscription.clone())
        .unwrap();
    runtime.advance(490_500);
    assert_eq!(sent.borrow().len(), 3); // A successful flush resets the cooldown episode.
    handler.buffer_until_markets_loaded(subscription).unwrap();
    handler.dispose();
    runtime.advance(530_000);
    assert_eq!(sent.borrow().len(), 3);
    assert_eq!(replayed.borrow().len(), 1);
    let weak = Rc::downgrade(&handler);
    drop(handler);
    assert!(weak.upgrade().is_none());
}

#[test]
fn call_method_replies_preserve_null_undefined_unknown_and_cancellation() {
    use safe_cex_webworker::call_method::CallMethodHandler;
    let (venue, runtime, store) = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue.clone(),
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method("unknown".into(), "notAMethod".into(), vec![]));
    assert_eq!(
        messages.borrow()[0].data.as_ref().unwrap(),
        &json!({"requestId":"unknown","success":false})
    );
    ready(handler.handle_call_method("reconnect".into(), "reconnect".into(), vec![]));
    assert_eq!(
        messages.borrow()[1].data.as_ref().unwrap(),
        &json!({"requestId":"reconnect","success":true,"result":null})
    );
    ready(handler.handle_call_method("income".into(), "fetchIncomeHistory".into(), vec![]));
    assert_eq!(
        messages.borrow()[2].data.as_ref().unwrap()["result"],
        json!([])
    );
    ready(handler.handle_call_method(
        "void".into(),
        "updateSymbolMetadata".into(),
        vec![json!("BTC")],
    ));
    assert_eq!(
        messages.borrow()[3].data.as_ref().unwrap()["result"],
        Value::Null
    );
    runtime.advance(100_000);
    ready(handler.handle_call_method(
        "error".into(),
        "setLeverage".into(),
        vec![json!("BTC"), json!(3)],
    ));
    assert_eq!(
        messages.borrow()[4].data.as_ref().unwrap()["success"],
        false
    );
    assert_eq!(
        messages.borrow()[4].data.as_ref().unwrap()["result"]["message"],
        "Not implemented"
    );
    let unpolled = handler.handle_call_method("cancel".into(), "fetchOrders".into(), vec![]);
    drop(unpolled);
    assert_eq!(
        messages.borrow()[5].data.as_ref().unwrap()["result"]["message"],
        "Worker request cancelled"
    );
    let order: Order = serde_json::from_value(json!({"id":"o","symbol":"BTC","status":"open","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0})).unwrap();
    store.data.borrow_mut().orders.push(order);
    ready(handler.handle_call_method(
        "remove".into(),
        "removeOrders".into(),
        vec![json!([{"id":"o"}])],
    ));
    assert!(store.data.borrow().orders.is_empty());
    assert_eq!(
        messages.borrow()[6].data.as_ref().unwrap()["result"],
        Value::Null
    );
    handler.dispose();
}

#[test]
fn call_method_discarded_host_task_still_settles_the_waiter() {
    use safe_cex_webworker::call_method::CallMethodHandler;
    let (venue, runtime, _) = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue,
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    handler.handle(StoreWorkerPayload {
        op: StoreWorkerOp::CallMethod,
        instance_id: None,
        data: Some(json!({"requestId":"unpolled","method":"fetchOrders","args":[]})),
    });
    let tasks = std::mem::take(&mut *runtime.tasks.borrow_mut());
    drop(tasks);
    assert_eq!(messages.borrow().len(), 1);
    assert_eq!(
        messages.borrow()[0].data.as_ref().unwrap()["requestId"],
        "unpolled"
    );
    assert_eq!(
        messages.borrow()[0].data.as_ref().unwrap()["success"],
        false
    );
}

struct MethodVenue {
    inner: Rc<Venue>,
    orders_calls: Cell<u32>,
    fail_orders: Cell<bool>,
    calls: RefCell<Vec<Value>>,
}
impl Exchange for MethodVenue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        self.inner.base()
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        self.inner.capabilities()
    }
    fn recover_after_offline(&self) -> Result<bool, ExchangeError> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"recoverAfterOffline","args":[]}));
        if self.fail_orders.get() {
            Err(ExchangeError::new("recovery failed"))
        } else {
            self.inner.recover_after_offline()
        }
    }
    fn fetch_and_set_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.calls.borrow_mut().push(json!({"method":"fetchAndSetOrders","args":symbols.map(|s|vec![json!(s)]).unwrap_or_default()}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_and_set_private_snapshot(&self, tries: u32) -> ExchangeFuture<'_, ()> {
        assert_eq!(tries, 1, "worker snapshot uses the source default tries=1");
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchAndSetPrivateSnapshot","args":[]}));
        Box::pin(async move {
            if self.fail_orders.get() {
                Err(ExchangeError::new("snapshot override failed"))
            } else {
                Ok(())
            }
        })
    }
    fn fetch_orders(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        self.calls.borrow_mut().push(json!({"method":"fetchOrders","args":symbols.map(|s|vec![json!(s)]).unwrap_or_default()}));
        self.orders_calls.set(self.orders_calls.get() + 1);
        Box::pin(async move {
            if self.fail_orders.get() {
                Err(ExchangeError::new("orders failed"))
            } else {
                Ok(vec![])
            }
        })
    }
    fn set_leverage(
        &self,
        symbol: String,
        leverage: f64,
        side: Option<PositionSide>,
    ) -> ExchangeFuture<'_, ()> {
        let mut args = vec![json!(symbol), json!(leverage)];
        if let Some(side) = side {
            args.push(json!(side));
        }
        self.calls
            .borrow_mut()
            .push(json!({"method":"setLeverage","args":args}));
        Box::pin(async move {
            self.base().runtime.sleep(self.inner.delay.get()).await?;
            if symbol == "FAIL" {
                let mut error = ExchangeError::new("bad");
                error.stack = Some("trace".into());
                error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(42)));
                return Err(error);
            }
            Ok(())
        })
    }
    fn fetch_ohlcv(&self, options: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchOHLCV","args":[options]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_historical_fills(
        &self,
        options: ExecutionsOptions,
    ) -> ExchangeFuture<'_, Vec<Execution>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchHistoricalFills","args":[options]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_historical_orders(
        &self,
        options: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<HistoricalOrder>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchHistoricalOrders","args":[options]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_closed_pnl(
        &self,
        options: HistoricalTradeOptions,
    ) -> ExchangeFuture<'_, Vec<ClosedPnlEntry>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchClosedPnl","args":[options]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_income_history(
        &self,
        options: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchIncomeHistory","args":[options]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn fetch_income_history_range(
        &self,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'_, Vec<IncomeRecord>> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"fetchIncomeHistoryRange","args":[start,end,kind]}));
        Box::pin(async { Ok(vec![]) })
    }
    fn update_symbol_metadata(&self, symbol: String) -> ExchangeFuture<'_, ()> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"updateSymbolMetadata","args":[symbol]}));
        Box::pin(async { Ok(()) })
    }
    fn schedule_cancel(&self, time: Option<Option<f64>>) -> ExchangeFuture<'_, ()> {
        self.calls.borrow_mut().push(json!({"method":"scheduleCancel","args":time.map(|t|vec![json!(t)]).unwrap_or_default()}));
        Box::pin(async { Ok(()) })
    }
    fn change_margin_type(&self, flag: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"changeMarginType","args":[flag,symbol]}));
        Box::pin(async { Ok(()) })
    }
    fn change_position_mode(&self, flag: bool, symbol: Option<String>) -> ExchangeFuture<'_, ()> {
        self.calls
            .borrow_mut()
            .push(json!({"method":"changePositionMode","args":[flag,symbol]}));
        Box::pin(async { Ok(()) })
    }
}
#[test]
fn complete_call_method_dispatch_matches_live_typescript_replies() {
    use safe_cex_webworker::call_method::CallMethodHandler;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/call_method_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let records: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        records.len(),
        20,
        "18 source commands, unknown, and failure"
    );
    let (inner, runtime, _) = venue();
    let venue = Rc::new(MethodVenue {
        inner: inner.clone(),
        orders_calls: Cell::new(0),
        fail_orders: Cell::new(false),
        calls: RefCell::default(),
    });
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue.clone(),
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    for record in records {
        let method = record["method"].as_str().unwrap();
        let id = record
            .get("requestId")
            .and_then(Value::as_str)
            .unwrap_or(method);
        ready(handler.handle_call_method(
            id.into(),
            method.into(),
            record["args"].as_array().unwrap().clone(),
        ));
        let actual = serde_json::to_value(messages.borrow_mut().pop().unwrap()).unwrap();
        assert_eq!(actual, record["expected"], "{method}");
        runtime.advance(100_000);
        let calls = std::mem::take(&mut *venue.calls.borrow_mut());
        if !matches!(
            method,
            "fetchLastLogsFromWebsockets" | "removeOrders" | "reconnect"
        ) {
            fn js_numbers(value: Value) -> Value {
                match value {
                    Value::Number(n) => json!(n.as_f64().unwrap()),
                    Value::Array(a) => Value::Array(a.into_iter().map(js_numbers).collect()),
                    Value::Object(o) => {
                        Value::Object(o.into_iter().map(|(k, v)| (k, js_numbers(v))).collect())
                    }
                    other => other,
                }
            }
            assert_eq!(
                js_numbers(json!(calls)),
                js_numbers(record["calls"].clone()),
                "dispatch arguments for {method}"
            );
        }
    }
    venue.fail_orders.set(true);
    ready(handler.handle_call_method(
        "snapshot".into(),
        "fetchAndSetPrivateSnapshot".into(),
        vec![],
    ));
    assert_eq!(
        messages.borrow().last().unwrap().data.as_ref().unwrap()["result"]["message"],
        "snapshot override failed"
    );
    inner.delay.set(1000);
    let mut future = handler.handle_call_method(
        "pending".into(),
        "setLeverage".into(),
        vec![json!("BTC"), json!(3)],
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(
        messages.borrow().last().unwrap().data.as_ref().unwrap()["result"]["message"],
        "Worker request cancelled"
    );
}

#[test]
fn offline_recovery_inner_worker_route_is_admitted() {
    use safe_cex_webworker::call_method::CallMethodHandler;
    let (venue, _, _) = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue,
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method("recover-inner".into(), "recoverAfterOffline".into(), vec![]));
    assert_eq!(
        messages.borrow().len(),
        1,
        "RECOVERY inner independent reply floor"
    );
    let message = messages.borrow();
    let reply = message[0].data.as_ref().unwrap();
    assert_eq!(
        reply["success"], true,
        "RECOVERY native inner command admitted"
    );
    assert_eq!(
        reply["result"].as_bool(),
        Some(true),
        "RECOVERY native inner boolean result"
    );
}
#[test]
fn offline_recovery_outer_worker_route_is_admitted() {
    use safe_cex_webworker::call_method::CallMethodHandler;
    let (venue, runtime, _) = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue,
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    handler.handle(StoreWorkerPayload {
        op: StoreWorkerOp::CallMethod,
        instance_id: None,
        data: Some(json!({"requestId":"recover-outer","method":"recoverAfterOffline","args":[]})),
    });
    runtime.advance(100_000);
    assert_eq!(
        messages.borrow().len(),
        1,
        "RECOVERY outer independent reply floor"
    );
    let message = messages.borrow();
    let reply = message[0].data.as_ref().unwrap();
    assert_eq!(
        reply["success"], true,
        "RECOVERY native outer command admitted"
    );
    assert_eq!(
        reply["result"].as_bool(),
        Some(true),
        "RECOVERY native outer boolean result"
    );
}

#[test]
fn offline_recovery_visits_every_socket_and_preserves_false_policy() {
    use safe_cex_exchange::{
        socket_lifecycle::SocketSlot,
        websocket::{BaseWebSocket, WsProtocol},
        ws_parent::WsExchangeParent,
    };
    struct Policy {
        id: u8,
        recover: bool,
        calls: Rc<RefCell<Vec<u8>>>,
    }
    impl WsProtocol for Policy {
        fn recovers_after_offline(&self) -> bool {
            self.calls.borrow_mut().push(self.id);
            self.recover
        }
    }
    let (venue, runtime, _) = venue();
    let erased: Rc<dyn Exchange> = venue.clone();
    let parent = WsExchangeParent::new(&erased);
    let calls = Rc::new(RefCell::new(vec![]));
    for (id, slot, recover) in [
        (1, SocketSlot::Private, false),
        (2, SocketSlot::Public, true),
    ] {
        venue.base.set_socket(
            slot,
            Some(BaseWebSocket::new(
                id as u64,
                parent.clone(),
                runtime.clone(),
                Rc::new(Policy {
                    id,
                    recover,
                    calls: calls.clone(),
                }),
            )),
        );
    }
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = safe_cex_webworker::call_method::CallMethodHandler::new(
        venue,
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method("policy".into(), "recoverAfterOffline".into(), vec![]));
    let messages = messages.borrow();
    assert_eq!(messages.len(), 1, "RECOVERY native policy reply count");
    let reply = messages[0].data.as_ref().unwrap();
    assert_eq!(
        reply["success"], true,
        "RECOVERY native policy dispatch success"
    );
    assert_eq!(
        reply["result"].as_bool(),
        Some(false),
        "RECOVERY native false policy result"
    );
    assert_eq!(
        *calls.borrow(),
        vec![1, 2],
        "RECOVERY native visits every socket before reduction"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/call_method_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "RECOVERY actual source policy: {}",
        String::from_utf8_lossy(&source.stderr)
    );
}

#[test]
fn offline_recovery_reports_exchange_hook_failure() {
    let (inner, _, _) = venue();
    let venue = Rc::new(MethodVenue {
        inner,
        orders_calls: Cell::new(0),
        fail_orders: Cell::new(true),
        calls: RefCell::default(),
    });
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = safe_cex_webworker::call_method::CallMethodHandler::new(
        venue,
        data_trace::post(Rc::new(move |message| sink.borrow_mut().push(message))),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method("error".into(), "recoverAfterOffline".into(), vec![]));
    let messages = messages.borrow();
    assert_eq!(messages.len(), 1, "RECOVERY native failure reply count");
    let reply = messages[0].data.as_ref().unwrap();
    assert_eq!(reply["success"], false, "RECOVERY native hook failure");
    assert_eq!(
        reply["result"]["message"], "recovery failed",
        "RECOVERY native error message"
    );
}

#[test]
fn invalid_command_payloads_never_call_the_registered_executor() {
    let (venue, runtime, _) = venue();
    let events = Rc::new(RefCell::new(Vec::<(String, Value)>::new()));
    for event in ["exec-log", "exec-result"] {
        let sink = events.clone();
        venue.base.on(
            event,
            Rc::new(move |name, data| {
                sink.borrow_mut().push((
                    name.into(),
                    safe_cex_exchange::js_runtime::value::to_json(data).unwrap(),
                ));
                Ok(())
            }),
        );
    }
    let calls = Rc::new(Cell::new(0));
    let capture = calls.clone();
    register_command_executor(Rc::new(move |_, _, _, _| {
        capture.set(capture.get() + 1);
        Box::pin(async { Ok(()) })
    }));
    let handler = CommandHandler::new(venue);
    for data in [
        None,
        Some(Value::Null),
        Some(json!(false)),
        Some(json!(true)),
        Some(json!(0)),
        Some(json!(1)),
        Some(json!("")),
        Some(json!("buy 1")),
    ] {
        events.borrow_mut().clear();
        handler
            .handle(StoreWorkerPayload {
                op: StoreWorkerOp::Exec,
                instance_id: None,
                data,
            })
            .unwrap();
        runtime.advance(100_000);
        assert_eq!(
            calls.get(),
            0,
            "invalid data must never invoke the executor"
        );
        assert_eq!(
            *events.borrow(),
            [(
                "exec-log".into(),
                json!({"message":"Invalid command payload."})
            )]
        );
    }
    handler
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::Exec,
            instance_id: None,
            data: Some(json!([])),
        })
        .unwrap();
    assert_eq!(
        calls.get(),
        1,
        "arrays are truthy objects under the maintained typeof check"
    );
    reset_command_executor();
}

#[test]
fn position_failure_reason_uses_javascript_error_string_for_empty_fields() {
    for (name, message, expected) in [
        ("Error", "", "Error"),
        ("", "", ""),
        ("", "failed", "failed"),
        ("TypeError", "failed", "TypeError: failed"),
    ] {
        let (venue, _, _) = venue();
        let mut error = ExchangeError::new(message);
        error.name = name.into();
        *venue.failure.borrow_mut() = Some(error);
        let messages = Rc::new(RefCell::new(vec![]));
        let sink = messages.clone();
        let handler =
            UpdatePositionsHandler::new(venue, Rc::new(move |msg| sink.borrow_mut().push(msg)));
        ready(handler.handle_update_positions("string-error".into(), None));
        let messages = messages.borrow();
        let data = messages.last().unwrap().data.as_ref().unwrap();
        assert_eq!(data["success"], false);
        assert_eq!(data["reason"], expected);
    }
}

#[test]
fn actual_source_command_admission_and_position_error_strings() {
    for script in [
        "command-admission-source.cjs",
        "position-error-string-source.cjs",
        "markets-error-clone-source.cjs",
    ] {
        let output = std::process::Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests")
                    .join(script),
            )
            .output()
            .expect("execute maintained worker source");
        assert!(
            output.status.success(),
            "{script}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn fetch_ohlcv_worker_forwards_every_source_timeframe() {
    let (inner, _, _) = venue();
    let venue = Rc::new(MethodVenue {
        inner,
        orders_calls: Cell::new(0),
        fail_orders: Cell::new(false),
        calls: RefCell::default(),
    });
    let replies = Rc::new(RefCell::new(Vec::new()));
    let capture = replies.clone();
    let handler = safe_cex_webworker::call_method::CallMethodHandler::new(
        venue.clone(),
        data_trace::post(Rc::new(move |reply| capture.borrow_mut().push(reply))),
        error_post_support::unexpected(),
    );
    let intervals = [
        "1M", "1s", "5s", "15s", "30s", "1m", "3m", "5m", "15m", "30m", "1h", "2h", "4h", "6h",
        "12h", "1d", "1w", "3h", "5h", "7h", "8h", "9h", "10h", "11h", "24h", "2d", "3d",
    ];
    for (i, interval) in intervals.iter().enumerate() {
        let options =
            json!({"symbol":"BTC","interval":interval,"from":123.0,"to":456.0,"limit":7.0});
        ready(handler.handle_call_method(
            format!("interval-{i}"),
            "fetchOHLCV".into(),
            vec![options.clone()],
        ));
        let replies = replies.borrow();
        let data = replies.last().unwrap().data.as_ref().unwrap();
        assert_eq!(
            data["success"], true,
            "OHLCV-WORKER fetch admits {interval}: {data}"
        );
        assert_eq!(
            venue.calls.borrow().last().unwrap(),
            &json!({"method":"fetchOHLCV","args":[options]}),
            "OHLCV-WORKER venue receives {interval}"
        );
    }
    assert_eq!(replies.borrow().len(), 27);
    assert_eq!(venue.calls.borrow().len(), 27);
}

#[test]
fn actual_source_ohlcv_interval_inventory_and_forwarding() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ohlcv-interval-source.cjs"
        ))
        .output()
        .expect("execute actual worker OHLCV handlers");
    assert!(
        output.status.success(),
        "OHLCV-WORKER maintained source: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn market_timeout_never_projects_its_error_into_the_json_post_channel() {
    use safe_cex_webworker::error_post::{WorkerErrorHandle, WorkerErrorPost};
    let (venue, runtime, _) = venue();
    let posts = Rc::new(RefCell::new(Vec::new()));
    let capture = posts.clone();
    let native = WorkerErrorHandle::new("opaque host Error");
    let original = native.clone();
    let reports = Rc::new(RefCell::new(Vec::new()));
    let sink = reports.clone();
    let error_post = WorkerErrorPost::new(
        Rc::new(move |name, message| {
            assert_eq!(name, "MarketsNotLoadedError");
            assert_eq!(
                message,
                "Markets still not loaded after 30s, dropping 1 pending subscriptions"
            );
            Ok(native.clone())
        }),
        Rc::new(move |report| {
            sink.borrow_mut().push(report);
            Ok(())
        }),
    );
    let handler = safe_cex_webworker::subscriptions::OHLCVSubscriptionsHandler::new(
        venue,
        data_trace::post(Rc::new(move |payload| capture.borrow_mut().push(payload))),
        error_post,
    );
    handler
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOHLCV,
            instance_id: None,
            data: Some(json!({"symbol":"BTC","interval":"1m","subId":"s"})),
        })
        .unwrap();
    runtime.advance(130_000);
    assert!(
        posts.borrow().is_empty(),
        "WORKER-NATIVE-ERROR timeout Error must use the native object post capability, never JSON data"
    );
    let reports = reports.borrow();
    assert_eq!(
        reports.len(),
        1,
        "WORKER-NATIVE-ERROR exactly one native post"
    );
    assert!(
        reports[0].error.same_object(&original),
        "WORKER-NATIVE-ERROR original opaque handle reaches post"
    );
    assert_eq!(
        reports[0].tags["worker_handler"],
        "OHLCVSubscriptionsHandler"
    );
    assert_eq!(reports[0].extra["pendingCount"], 1);
    assert!(runtime.exceptions.borrow().is_empty());
}

#[test]
fn market_timeout_host_failures_retain_fired_deadline_until_flush() {
    use safe_cex_webworker::{
        error_post::{WorkerErrorHandle, WorkerErrorPost},
        handler::BaseHandler,
    };
    for fail_create in [true, false] {
        let (venue, runtime, store) = venue();
        let calls = Rc::new(Cell::new(0));
        let creates = calls.clone();
        let attempts = Rc::new(RefCell::new(Vec::new()));
        let sent = attempts.clone();
        let error_post = WorkerErrorPost::new(
            Rc::new(move |_, _| {
                let call = creates.get();
                creates.set(call + 1);
                if fail_create && call == 0 {
                    return Err(ExchangeError::new("native create failed"));
                }
                Ok(WorkerErrorHandle::new(call))
            }),
            Rc::new(move |report| {
                let first = sent.borrow().is_empty();
                sent.borrow_mut().push(report);
                if !fail_create && first {
                    return Err(ExchangeError::new("native post failed"));
                }
                Ok(())
            }),
        );
        let replayed = Rc::new(Cell::new(0));
        let replay_count = replayed.clone();
        let handler = BaseHandler::new(
            venue,
            "TimeoutTest".into(),
            error_post,
            Rc::new(move |_| replay_count.set(replay_count.get() + 1)),
        );
        let payload = StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOHLCV,
            instance_id: None,
            data: None,
        };
        handler
            .buffer_until_markets_loaded(payload.clone())
            .unwrap();
        runtime.advance(130_000);
        assert_eq!(
            runtime.exceptions.borrow().len(),
            1,
            "WORKER-NATIVE-ERROR host failure is visible"
        );
        assert_eq!(
            runtime.exceptions.borrow()[0].0.message,
            if fail_create {
                "native create failed"
            } else {
                "native post failed"
            }
        );
        handler.buffer_until_markets_loaded(payload).unwrap();
        runtime.advance(160_000);
        assert_eq!(
            calls.get(),
            1,
            "WORKER-NATIVE-ERROR thrown callback retains fired deadline; no replacement timeout"
        );
        store.data.borrow_mut().markets = vec![serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":5},"leverage":{"min":1,"max":100}}})).unwrap()];
        runtime.advance(160_500);
        assert_eq!(
            replayed.get(),
            2,
            "WORKER-NATIVE-ERROR failed report keeps both pending items until flush"
        );
        assert_eq!(calls.get(), 1);
        store.data.borrow_mut().markets.clear();
        handler
            .buffer_until_markets_loaded(StoreWorkerPayload {
                op: StoreWorkerOp::SubscribeOHLCV,
                instance_id: None,
                data: None,
            })
            .unwrap();
        runtime.advance(190_500);
        assert_eq!(
            calls.get(),
            2,
            "WORKER-NATIVE-ERROR loaded-market episode resets the deadline"
        );
        assert_eq!(attempts.borrow().last().unwrap().extra["pendingCount"], 1);
        assert_eq!(runtime.exceptions.borrow().len(), 1);
    }
}
