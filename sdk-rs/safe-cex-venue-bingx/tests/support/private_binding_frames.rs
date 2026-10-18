fn private_frame(h: &Harness, value: Value) {
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("listenKey="))
        .unwrap()
        .1
        .clone();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
    for _ in 0..8 {
        h.runtime.pump();
    }
}

fn started_private_fixture() -> Harness {
    let h = fixture();
    h.drive(h.venue().start()).unwrap();
    for _ in 0..8 {
        h.runtime.pump();
    }
    h
}

fn private_order(status: &str, filled: &str) -> Value {
    json!({"e":"ORDER_TRADE_UPDATE","o":{"i":"frame-order","s":"BTC-USDT","o":"LIMIT","S":"BUY","X":status,"q":"3","p":"120","z":filled,"sp":"0","f":"GTC","ro":false,"ap":"121","l":filled}})
}

#[test]
fn private_order_frame_commits_the_real_mapped_order() {
    let h = started_private_fixture();
    private_frame(&h, private_order("NEW", "0"));
    let orders = h.store.orders();
    let order = orders
        .iter()
        .find(|order| order.id == "frame-order")
        .unwrap();
    assert_eq!(order.symbol, "BTCUSDT");
    assert_eq!(order.price, 120.0);
    assert_eq!(order.amount, 3.0);
    assert_eq!(order.status, OrderStatus::Open);
    drop(orders);
    done(&h);
}

#[test]
fn private_fill_frame_closes_the_order_and_emits_the_fill() {
    use std::{cell::RefCell, rc::Rc};
    let h = started_private_fixture();
    private_frame(&h, private_order("NEW", "0"));
    let fills = Rc::new(RefCell::new(Vec::new()));
    let capture = fills.clone();
    h.exchange.base().on(
        "fill",
        Rc::new(move |_, fill| {
            let fill = safe_cex_exchange::js_runtime::value::to_json(fill).unwrap();
            capture.borrow_mut().push(fill.clone());
            Ok(())
        }),
    );
    private_frame(&h, private_order("FILLED", "3"));
    assert_eq!(
        *fills.borrow(),
        vec![
            json!({"id":"frame-order","symbol":"BTCUSDT","side":"buy","price":121.0,"amount":3.0})
        ]
    );
    assert_eq!(
        h.store
            .orders()
            .iter()
            .find(|order| order.id == "frame-order")
            .unwrap()
            .status,
        OrderStatus::Closed
    );
    done(&h);
}

#[test]
fn private_position_frame_commits_before_the_delayed_rest_heal() {
    use std::rc::Rc;
    let h = started_private_fixture();
    let gate = Rc::new(support::Release::default());
    *h.http.held_request.borrow_mut() = Some(("GET".into(), POSITIONS.into(), gate));
    private_frame(
        &h,
        json!({"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTC-USDT","ps":"BOTH","pa":"4","ep":"123","up":"7","liqp":"60","mt":"isolated"}]}}),
    );
    let positions = h.store.positions();
    let position = positions
        .iter()
        .find(|p| p.symbol == "BTCUSDT" && p.side == PositionSide::Long)
        .unwrap();
    assert_eq!(position.contracts, 4.0);
    assert_eq!(position.entry_price, 123.0);
    assert_eq!(position.unrealized_pnl, 7.0);
    assert_eq!(position.margin_type, Some(MarginType::Isolated));
    drop(positions);
    done(&h);
}

#[test]
fn balance_only_account_frame_preserves_the_source_rest_owned_balance() {
    let h = started_private_fixture();
    let before = h.store.balance().clone();
    assert_eq!(before.total, 10.0);
    // TS handles position-shaped account rows only; balance remains REST-owned.
    private_frame(
        &h,
        json!({"e":"ACCOUNT_UPDATE","a":{"B":[{"a":"USDT","wb":"999","cw":"777"}]}}),
    );
    assert_eq!(*h.store.balance(), before);
    done(&h);
}

#[test]
fn refused_market_row_emits_its_value_and_returns_empty_fresh_markets() {
    use std::{cell::RefCell, rc::Rc};
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(method, path, _)| method != "GET" || path != MARKET);
    h.http.ok("GET", MARKET, json!([{"symbol":"BTC-USDT","asset":{},"currency":"USDT","quantityPrecision":3,"pricePrecision":2}]));
    let errors = Rc::new(RefCell::new(Vec::new()));
    let capture = errors.clone();
    h.exchange.base().on(
        "error",
        Rc::new(move |_, error| {
            let error = safe_cex_exchange::js_runtime::value::to_json(error).unwrap();
            capture.borrow_mut().push(error.clone());
            Ok(())
        }),
    );
    let before = h.store.markets().to_vec();
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    assert!(markets.is_empty());
    assert_eq!(h.store.markets().as_ref(), before.as_slice());
    assert!(
        errors
            .borrow()
            .iter()
            .any(|error| error.to_string().contains("base={}")),
        "visible refusal cause: {:?}",
        errors.borrow()
    );
    assert_eq!(
        h.runtime.reports.borrow().as_slice(),
        &["Exchange Error: Refused BingX market row 0 symbol=BTC-USDT: base={}: expected string"]
    );
    h.runtime.reports.borrow_mut().clear();
    done(&h);
}
