#[path = "support/complete_host.rs"]
mod host;
#[path = "support/retained_assert.rs"]
mod retained_assert;
use host::*;
use safe_cex_num::numeric_ingress::js_value_as_f64;
use safe_cex_types::*;
use safe_cex_venue_blofin::{exchange::*, private_stream_data::*, public_stream_data::*};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/complete_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "maintained full-class oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            66,
            "independent66floor"
        );
        assert_eq!(v["stoppedAfter"], 66);
        assert_eq!(
            v["sourceMembers"].as_array().unwrap().len(),
            71,
            "independent71memberfloor"
        );
        assert_eq!(
            v["sourcePrivate"].as_array().unwrap().len(),
            16,
            "independent16memberfloor"
        );
        v
    })
}
fn number(n: f64, e: &Value) {
    if e["number"] == "nan" {
        assert!(n.is_nan(), "native NaN")
    } else {
        assert_eq!(
            n.to_bits(),
            u64::from_str_radix(e["number"].as_str().unwrap(), 16).unwrap(),
            "native numeric bits"
        )
    }
}
fn effects(n: &Value, e: &Value) {
    match (n, e) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64(), b.as_f64(), "effect numeric value")
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "effect count");
            for (a, b) in a.iter().zip(b) {
                effects(a, b);
            }
        }
        _ => assert_eq!(n, e, "effect after native assertions"),
    }
}
fn native(r: &Runtime, e: &Value) {
    let orders = r.store.orders();
    let expected = e["orders"].as_array().unwrap();
    assert_eq!(orders.len(), expected.len(), "native orders count");
    for (o, e) in orders.iter().zip(expected) {
        assert_eq!(o.id, e["id"]);
        assert_eq!(o.symbol, e["symbol"]);
        assert_eq!(o.side, OrderSide::Buy);
        assert_eq!(o.r#type, OrderType::Limit);
        if let Some(status) = e["status"].as_str() {
            assert_eq!(
                o.status,
                match status {
                    "open" => OrderStatus::Open,
                    "closed" => OrderStatus::Closed,
                    "canceled" => OrderStatus::Canceled,
                    _ => panic!("status floor"),
                }
            )
        }
        for (n, k) in [
            (o.price.to_number(), "price"),
            (o.amount, "amount"),
            (o.filled, "filled"),
            (o.remaining, "remaining"),
            (o.last_update, "lastUpdate"),
        ] {
            number(n, &e[k]);
        }
        assert_eq!(o.reduce_only, false);
        assert_eq!(o.time_in_force, Some(OrderTimeInForce::GoodTillCancel));
        retained_assert::assert_value(o.info.as_ref().unwrap(), &e["info"]);
        let extra = o.extra.as_ref().unwrap();
        number(
            extra.remaining_native.unwrap(),
            &e["extra"]["remainingNative"],
        );
        number(
            extra.filled_in_placement_units.unwrap(),
            &e["extra"]["filledInPlacementUnits"],
        );
    }
    let positions = r.store.positions();
    let expected = e["positions"].as_array().unwrap();
    assert_eq!(positions.len(), expected.len(), "native positions count");
    for (p, e) in positions.iter().zip(expected) {
        assert_eq!(p.id, e["id"]);
        assert_eq!(p.symbol, e["symbol"]);
        assert_eq!(p.side, PositionSide::Long);
        assert_eq!(p.margin_type, Some(MarginType::Cross));
        assert_eq!(p.position_mode, Some(PositionMode::OneWay));
        for (n, k) in [
            (p.entry_price, "entryPrice"),
            (p.notional, "notional"),
            (p.leverage, "leverage"),
            (p.unrealized_pnl, "unrealizedPnl"),
            (p.contracts, "contracts"),
            (p.liquidation_price, "liquidationPrice"),
        ] {
            number(n, &e[k]);
        }
        retained_assert::assert_value(p.info.as_ref().unwrap(), &e["info"]);
        number(
            p.extra.as_ref().unwrap().contracts_native.unwrap(),
            &e["extra"]["contractsNative"],
        );
    }
    let balance = r.store.balance();
    for (n, k) in [
        (balance.free, "free"),
        (balance.total, "total"),
        (balance.used, "used"),
        (balance.upnl, "upnl"),
    ] {
        number(n, &e["balance"][k]);
    }
    match &balance.info {
        Some(i) => retained_assert::assert_value(i, &e["balance"]["info"]),
        None => assert!(e["balance"].get("info").is_none()),
    }
    let fills = r.fills.borrow();
    let expected = e["fills"].as_array().unwrap();
    assert_eq!(fills.len(), expected.len(), "native fills count");
    for (f, e) in fills.iter().zip(expected) {
        assert_eq!(f.id, e["id"]);
        assert_eq!(f.order_id, e["orderId"]);
        assert_eq!(f.symbol, e["symbol"]);
        assert_eq!(f.side, OrderSide::Buy);
        for (n, k) in [
            (f.price, "price"),
            (f.amount, "amount"),
            (f.timestamp, "timestamp"),
        ] {
            number(n, &e[k]);
        }
    }
    let statuses = r.fill_statuses.borrow();
    let expected = e["fillStatuses"].as_array().unwrap();
    assert_eq!(
        statuses.len(),
        expected.len(),
        "independent fill status observation count"
    );
    for (actual, expected) in statuses.iter().zip(expected) {
        assert_eq!(
            *actual,
            match expected.as_str() {
                Some("open") => Some(OrderStatus::Open),
                Some("closed") => Some(OrderStatus::Closed),
                Some("canceled") => Some(OrderStatus::Canceled),
                None => None,
                _ => panic!("fill status floor"),
            },
            "native Store status before fill callback"
        );
    }
    assert_eq!(
        r.context.disposed.get(),
        e["parentDisposed"].as_bool().unwrap(),
        "independent parent disposal state"
    );
}
fn run(e: &Value) -> (Rc<Runtime>, Rc<BlofinPrivateWebsocket<Socket>>, Vec<String>) {
    let r = Runtime::new(corpus(), &e["initial"]);
    let socket = BlofinPrivateWebsocket::new(Rc::new(Socket {
        context: r.context.clone(),
        runtime: r.clone(),
        public: false,
        disposed: std::cell::Cell::new(false),
    }))
    .unwrap();
    r.trace.borrow_mut().clear();
    let mut errors = vec![];
    for action in e["actions"].as_array().unwrap() {
        let arg = &action[1];
        let result = match action[0].as_str().unwrap() {
            "root-orders" => socket.handle_order_topic(arg).map_err(|e| e.to_string()),
            "root-positions" => socket.handle_position_topic(arg).map_err(|e| e.to_string()),
            "root-account" => socket.handle_account_topic(arg).map_err(|e| e.to_string()),
            "orders" => socket
                .handle_order_topic(&json!({"data":arg}))
                .map_err(|e| e.to_string()),
            "positions" => socket
                .handle_position_topic(&json!({"data":arg}))
                .map_err(|e| e.to_string()),
            "account" => socket
                .handle_account_topic(&json!({"data":arg}))
                .map_err(|e| e.to_string()),
            "message" => socket.on_message(arg.as_str().unwrap()),
            "dispose" => socket.dispose(),
            "mutate-account" | "replace-account" => Ok(()),
            _ => panic!("action floor"),
        };
        if let Err(e) = result {
            errors.push(e)
        }
    }
    (r, socket, errors)
}
fn group(name: &str, floor: usize) {
    let mut count = 0;
    for e in corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["group"] == name)
    {
        count += 1;
        let (r, _, errors) = run(e);
        native(&r, e);
        assert_eq!(
            errors,
            e["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "native failure text"
        );
        effects(&Value::Array(r.trace.borrow().clone()), &e["trace"]);
    }
    assert_eq!(count, floor, "independent group floor")
}
#[test]
fn orders_native() {
    group("orders", 30);
    let e = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["group"] == "orders-mapper-alias")
        .unwrap();
    let (r, _, _) = run(e);
    assert_eq!(
        r.fills.borrow()[0].amount,
        0.25,
        "native immutable mapper input retains2 contracts"
    );
    number(0.5, &e["fills"][0]["amount"]);
}
#[test]
fn terminal_reconciliation_native() {
    group("terminal", 4);
}
#[test]
fn positions_native() {
    group("positions", 8);
}
#[test]
fn account_native() {
    group("account", 7);
    let e = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["group"] == "account-alias")
        .unwrap();
    let (r, _, _) = run(e);
    assert_eq!(
        String::from_utf16(
            r.store
                .balance()
                .info
                .as_ref()
                .unwrap()
                .get("totalEquity")
                .unwrap()
                .as_text()
                .unwrap()
        )
        .unwrap(),
        "100",
        "native account info owned copy"
    );
    assert_eq!(
        e["balance"]["info"]["totalEquity"], "999",
        "source account info live alias"
    );
}
#[test]
fn private_facade_native() {
    group("private", 1);
    group("malformed", 12);
    group("private-dispose", 1);
    let e = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["group"] == "private-replace")
        .unwrap();
    let (r, s, _) = run(e);
    assert_eq!(
        r.store.balance().total,
        97.25,
        "native concrete dispatcher body"
    );
    number(20.0, &e["balance"]["total"]);
    assert!(r.any_pong.get(), "private constructor keepalive field");
    assert!(s.core.orders_throttle_timeout.borrow().is_none());
    // Emitter re-entry observes the already committed cumulative fill; no RefCell guard spans emission.
    let r = Runtime::new(corpus(), &json!({}));
    let s = BlofinPrivateWebsocket::new(Rc::new(Socket {
        runtime: r.clone(),
        context: r.context.clone(),
        public: false,
        disposed: std::cell::Cell::new(false),
    }))
    .unwrap();
    let weak = Rc::downgrade(&s);
    let data = json!({"data":[corpus()["order"]]});
    let input = data.clone();
    *r.reentry.borrow_mut() = Some(Rc::new(move || {
        let s = weak.upgrade().unwrap();
        s.handle_order_topic(&input).unwrap();
    }));
    s.handle_order_topic(&data).unwrap();
    assert_eq!(
        r.fills.borrow().len(),
        1,
        "reentry sees the cumulative fill committed before emitter"
    );
    s.handle_order_topic(&data).unwrap();
    assert_eq!(r.fills.borrow().len(), 1, "replayed frame emits nothing");
}
fn first_source_signature() -> String {
    corpus()["signedRequest"]["headers"]["ACCESS-SIGN"]
        .as_str()
        .unwrap()
        .into()
}
fn first_native_signature(r: &Runtime) -> String {
    r.adapter.requests.borrow()[0].headers["ACCESS-SIGN"].clone()
}
fn scheduled(r: &Runtime) -> f64 {
    r.trace
        .borrow()
        .iter()
        .find(|v| v[0] == "schedule")
        .unwrap()[1]
        .as_f64()
        .unwrap()
}
#[test]
fn exchange_class_native() {
    let r = Runtime::new(corpus(), &json!({}));
    let exchange = exchange(&r);
    assert_eq!(
        exchange.context.name,
        ExchangeName::Blofin,
        "native exchange class name"
    );
    assert!(r.any_pong.get(), "both concrete sockets keepalive fields");
    assert_eq!(
        *r.trace.borrow(),
        vec![
            json!(["base", "blofin"]),
            json!(["xhr"]),
            json!(["public"]),
            json!(["socket-construct"])
        ],
        "complete constructor effect ordering"
    );
    assert!(Rc::ptr_eq(&exchange.context.store, &exchange.base.store));
    assert!(
        Rc::ptr_eq(&exchange.xhr, r.xhr.borrow().as_ref().unwrap()),
        "assigned signed XHR identity"
    );
    assert!(exchange.fields.account_modes.position_mode.get().is_none());
    assert!(exchange.fields.account_modes.margin_type.get().is_none());
    assert_eq!(
        (exchange.fields.client_order_id_rule.mint.as_ref().unwrap())(),
        "b3cdedc0a20880ba0123456789abcdef"
    );
    assert_eq!(corpus()["sourceMint"], "b3cdedc0a20880ba0123456789abcdef");
    effects(
        &Value::Array(r.trace.borrow().clone()),
        &corpus()["constructorTrace"],
    );
    assert!(
        ready(exchange.fetch_normal_orders()).unwrap().is_empty(),
        "native static member binding"
    );
    assert_eq!(
        corpus()["staticMemberResult"][0]["id"],
        "runtime-replacement",
        "source reads writable mapper field"
    );
    let mut caller_options = r.context.options.clone();
    caller_options.testnet = Some(true);
    assert_eq!(caller_options.testnet, Some(true));
    assert_eq!(
        exchange.private_websocket.endpoint(),
        "wss://openapi.blofin.com/ws/private",
        "native owned options"
    );
    assert_eq!(
        corpus()["liveEndpoint"],
        "wss://demo-trading-openapi.blofin.com/ws/private",
        "source live options mutation"
    );
    r.trace.borrow_mut().clear();
    exchange
        .private_websocket
        .handle_order_topic(&json!({"data":[corpus()["order"]]}))
        .unwrap();
    assert_eq!(
        r.store.orders()[0].amount,
        1.0,
        "private uses same parent Store"
    );
    assert_eq!(r.fills.borrow()[0].amount, 0.25);
    // Timer fetch invokes the parent's actual REST reader, not the panic host placeholder.
    let cb = r.timers.borrow_mut().remove(&1).unwrap();
    ready(cb());
    assert!(r.store.loaded().orders);
    assert!(r.store.orders().is_empty());
    assert_eq!(
        r.trace
            .borrow()
            .iter()
            .filter(|v| v[0] == "request")
            .count(),
        3,
        "independent native3RESTroutes"
    );
    exchange
        .apply_confirmed_position_mode(PositionMode::Hedged)
        .unwrap();
    assert_eq!(
        ready(exchange.get_or_fetch_position_mode(None, false)).unwrap(),
        PositionMode::Hedged,
        "read body shares confirmed cache"
    );
    assert_eq!(
        exchange.symbol_to_market_id("BTCUSDT").as_deref(),
        Some("BTC-USDT")
    );
    let candle: CandleCallback<String> = Rc::new(|_| Ok(()));
    let clean = exchange
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m"})).unwrap(),
            candle,
        )
        .unwrap();
    assert!(
        exchange
            .public_websocket
            .message_handlers
            .borrow()
            .contains_key("candle1m.BTC-USDT"),
        "concrete public listener installed"
    );
    clean().unwrap();
    // Native factory signs actual facade reads before the common HTTP adapter.
    let requests = r.adapter.requests.borrow();
    let first = &requests[0];
    assert_eq!(first.headers["ACCESS-KEY"], "k");
    assert_eq!(first.headers["ACCESS-PASSPHRASE"], "p");
    assert_eq!(first.headers["ACCESS-TIMESTAMP"], "1710000000000.125");
    assert_eq!(
        first.headers["ACCESS-NONCE"],
        "0123456789abcdef0123456789abcdef"
    );
    assert_eq!(
        first.headers["ACCESS-SIGN"].len(),
        88,
        "base64 of64hex bytes"
    );
    assert_eq!(first.timeout_ms, Some(20000));
    drop(requests);
    assert_eq!(
        first_source_signature(),
        first_native_signature(&r),
        "actual source/native factory HMAC header"
    );
    for (index, streak) in [0.0, 2.0, 7.0].into_iter().enumerate() {
        exchange.base.auth_failure_streak.set(streak);
        let host = ExchangeHost { runtime: r.clone() };
        assert_eq!(
            host.get_tick_interval(&exchange.base).unwrap(),
            corpus()["backoff"][index].as_f64().unwrap(),
            "actual inherited source policy"
        );
        r.trace.borrow_mut().clear();
        ready(exchange.tick(None)).unwrap();
        assert_eq!(
            exchange.context.auth_failure_streak.get(),
            0.0,
            "signed success resets shared inherited streak"
        );
        assert_eq!(
            scheduled(&r),
            2000.0,
            "tick observes reset after successful signed reads"
        );
    }
    for endpoint in safe_cex_venue_blofin::types::ENDPOINTS
        .as_object()
        .unwrap()
        .values()
    {
        r.adapter.responses.borrow_mut().insert(
            endpoint.as_str().unwrap().into(),
            json!({"code":"152401","msg":"rejected","data":[]}),
        );
    }
    exchange.base.auth_failure_streak.set(6.0);
    r.trace.borrow_mut().clear();
    ready(exchange.tick(None)).unwrap();
    assert_eq!(
        exchange.context.auth_failure_streak.get(),
        7.0,
        "actual API failure increments same base cell with cap"
    );
    assert_eq!(
        scheduled(&r),
        60000.0,
        "inherited backoff drives concrete tick after auth rejection"
    );
    r.adapter.responses.borrow_mut().clear();
    assert!(
        Rc::ptr_eq(&exchange.context, &exchange.base),
        "shared base state cells"
    );
    r.trace.borrow_mut().clear();
    exchange.dispose().unwrap();
    assert_eq!(
        *r.trace.borrow(),
        vec![
            json!(["base-dispose"]),
            json!(["public-dispose"]),
            json!(["socket-dispose"])
        ],
        "complete dispose effect order"
    );
    assert_eq!(
        corpus()["retainedParentOrders"],
        1,
        "source child owns parent"
    );
    let r2 = Runtime::new(corpus(), &json!({}));
    let parent2 = host::exchange(&r2);
    parent2.private_websocket.dispose().unwrap();
    parent2
        .private_websocket
        .on_message("{\"arg\":{\"channel\":\"account\"},\"data\":{\"totalEquity\":\"100\"}}")
        .unwrap();
    assert_eq!(
        r2.store.balance().total,
        20.0,
        "private disposal prevents subsequent data dispatch"
    );
    assert!(
        !parent2.context.disposed.get(),
        "private disposal leaves parent live"
    );
    assert_eq!(
        parent2
            .map_orders(
                &safe_cex_num::numeric_ingress::js_from_value::<
                    safe_cex_venue_blofin::orders::OrdersInput,
                >(json!([corpus()["order"]]))
                .unwrap()
            )
            .unwrap()
            .len(),
        1,
        "parent mapper remains usable"
    );
    let cleanup = parent2
        .listen_order_book("BTCUSDT".into(), Rc::new(|_| Ok(())))
        .unwrap();
    assert!(
        parent2
            .public_websocket
            .message_handlers
            .borrow()
            .contains_key("books.BTC-USDT"),
        "public socket survives private disposal"
    );
    cleanup().unwrap();
    let private = exchange.private_websocket.clone();
    drop(exchange);
    let e = private
        .handle_order_topic(&json!({"data":[corpus()["order"]]}))
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "Parent exchange dropped",
        "chosen Weak parent lifetime"
    );
}
struct Spy {
    opts: RefCell<Option<OHLCVOptions>>,
    candle: RefCell<Option<CandleCallback<String>>>,
    symbol: RefCell<Option<String>>,
    book: RefCell<Option<BookCallback<String>>>,
    cleanup: PublicCleanup<String>,
}
impl BlofinPublicListeners for Spy {
    type Error = String;
    fn listen_candles(
        &self,
        opts: OHLCVOptions,
        cb: CandleCallback<String>,
    ) -> Result<PublicCleanup<String>, PublicDataError<String>> {
        *self.opts.borrow_mut() = Some(opts);
        *self.candle.borrow_mut() = Some(cb);
        Ok(self.cleanup.clone())
    }
    fn listen_book(
        &self,
        symbol: String,
        cb: BookCallback<String>,
    ) -> Result<PublicCleanup<String>, PublicDataError<String>> {
        *self.symbol.borrow_mut() = Some(symbol);
        *self.book.borrow_mut() = Some(cb);
        Ok(self.cleanup.clone())
    }
}
fn spy() -> Spy {
    Spy {
        opts: RefCell::default(),
        candle: RefCell::default(),
        symbol: RefCell::default(),
        book: RefCell::default(),
        cleanup: Rc::new(|| Ok(())),
    }
}
#[test]
fn forwarding_ohlcv_native() {
    let spy = spy();
    let callback: CandleCallback<String> = Rc::new(|_| Ok(()));
    let opts: OHLCVOptions = serde_json::from_value(
        json!({"symbol":"BTCUSDT","interval":"1m","limit":7,"from":12,"to":45}),
    )
    .unwrap();
    let cleanup = listen_ohlcv(&spy, opts.clone(), callback.clone()).unwrap();
    assert!(
        Rc::ptr_eq(&cleanup, &spy.cleanup),
        "exact OHLCV cleanup identity"
    );
    assert!(
        Rc::ptr_eq(&callback, spy.candle.borrow().as_ref().unwrap()),
        "exact OHLCV callback identity"
    );
    let forwarded = spy.opts.borrow();
    let forwarded = forwarded.as_ref().unwrap();
    assert_eq!(forwarded.symbol, opts.symbol);
    assert_eq!(forwarded.interval, opts.interval);
    assert_eq!(forwarded.limit, opts.limit);
    assert_eq!(forwarded.from, opts.from);
    assert_eq!(forwarded.to, opts.to);
    assert_eq!(corpus()["forwarding"]["cleanupIdentity"], true);
}
#[test]
fn forwarding_book_native() {
    let spy = spy();
    let callback: BookCallback<String> = Rc::new(|_| Ok(()));
    let cleanup = listen_order_book(&spy, "BTCUSDT".into(), callback.clone()).unwrap();
    assert!(
        Rc::ptr_eq(&cleanup, &spy.cleanup),
        "exact book cleanup identity"
    );
    assert!(
        Rc::ptr_eq(&callback, spy.book.borrow().as_ref().unwrap()),
        "exact book callback identity"
    );
    assert_eq!(spy.symbol.borrow().as_deref(), Some("BTCUSDT"));
    assert_eq!(corpus()["forwarding"]["callbackIdentity"], true);
}
