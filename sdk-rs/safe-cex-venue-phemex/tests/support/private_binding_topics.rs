fn private_frame(h: &Harness, value: Value) {
    h.runtime.sockets.borrow()[1]
        .1
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
    h.runtime.pump();
}
fn authenticated_private() -> Harness {
    let h = venue();
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    private_frame(&h, json!({"id":999,"result":{"status":"success"}}));
    assert!(
        h.runtime.sockets.borrow()[1]
            .1
            .sent
            .borrow()
            .iter()
            .any(|frame| frame.contains("aop_p.subscribe"))
    );
    h
}
fn private_order_row(status: &str, filled: &str) -> Value {
    json!({"orderID":"private-order","symbol":SYMBOL,"ordStatus":status,"ordType":"Limit","side":"Buy","priceRp":"99.5","orderQty":"0.009","leavesQty":"0.006","execQty":filled,"execPriceRp":"99.75","timeInForce":"GoodTillCancel","reduceOnly":false,"takeProfitRp":"0","stopLossRp":"0"})
}
#[test]
fn private_order_snapshot_replaces_store_orders_without_emitting_fill() {
    let h = authenticated_private();
    h.store.add_or_update_orders(vec![existing()]).unwrap();
    let fills = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, value| {
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    private_frame(
        &h,
        json!({"type":"snapshot","orders_p":[private_order_row("New","0.003")]}),
    );
    assert_eq!(h.store.orders().len(), 1);
    let order = h.store.orders()[0].clone();
    assert_eq!(order.id, "private-order");
    assert_eq!(order.symbol, SYMBOL);
    assert_eq!(order.status, OrderStatus::Open);
    assert_eq!(order.filled, 0.003);
    assert_eq!(order.remaining, 0.006);
    assert!(h.store.loaded().orders);
    assert!(
        fills.borrow().is_empty(),
        "source snapshots never notify fills"
    );
    h.venue().dispose().unwrap();
}
#[test]
fn private_filled_order_updates_store_and_emits_execution_price() {
    let h = authenticated_private();
    let fills = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, value| {
            sink.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    private_frame(
        &h,
        json!({"type":"incremental","orders_p":[private_order_row("Filled","0.009")]}),
    );
    let order = h
        .store
        .orders()
        .iter()
        .find(|o| o.id == "private-order")
        .unwrap()
        .clone();
    assert_eq!(order.status, OrderStatus::Closed);
    assert_eq!(order.filled, 0.009);
    assert_eq!(fills.borrow().len(), 1);
    let fill = fills.borrow()[0].clone();
    assert_eq!(fill.own_entries().unwrap().len(), 5);
    for (key, expected) in [("id", "private-order"), ("side", "buy"), ("symbol", SYMBOL)] {
        assert!(fill.get(key).unwrap().strict_equals(&Js::str(expected)));
    }
    assert_eq!(fill.get("price").unwrap().as_f64(), Some(99.75));
    assert_eq!(fill.get("amount").unwrap().as_f64(), Some(0.009));
    h.venue().dispose().unwrap();
}
#[test]
fn private_position_frame_updates_supported_market_only() {
    let h = authenticated_private();
    let reads = h.http.calls("/g-accounts/accountPositions").len();
    let mut row = account()["data"]["positions"][0].clone();
    row["size"] = json!("3");
    row["avgEntryPriceRp"] = json!("95");
    let mut unsupported = row.clone();
    unsupported["symbol"] = json!("UNSUPPORTEDUSDT");
    private_frame(&h, json!({"positions_p":[row,unsupported]}));
    let positions = h.store.positions();
    let position = positions
        .iter()
        .find(|p| p.symbol == SYMBOL && p.side == PositionSide::Long)
        .unwrap();
    assert_eq!(position.contracts, 3.0);
    assert_eq!(position.entry_price, 95.0);
    assert_eq!(position.unrealized_pnl, 15.0);
    assert!(positions.iter().all(|p| p.symbol == SYMBOL));
    assert_eq!(h.http.calls("/g-accounts/accountPositions").len(), reads);
    drop(positions);
    h.venue().dispose().unwrap();
}
#[test]
fn private_account_frame_merges_currency_balance_and_position_pnl() {
    let h = authenticated_private();
    let previous = h.store.balance().clone();
    let positions = h.store.positions().to_vec();
    private_frame(
        &h,
        json!({"accounts_p":[{"currency":"USDT","accountBalanceRv":"500","totalUsedBalanceRv":"50","marker":"private-account"}]}),
    );
    let balance = h.store.balance();
    let currencies = balance.currencies.as_ref().unwrap();
    let usdt = currencies.get("USDT").unwrap().as_balance().unwrap();
    assert_eq!(usdt.total, 480.0);
    assert_eq!(usdt.used, 50.0);
    assert_eq!(usdt.free, 450.0);
    assert_eq!(usdt.upnl, 20.0);
    assert!(
        usdt.info
            .as_ref()
            .unwrap()
            .get("marker")
            .unwrap()
            .text_equals("private-account")
    );
    assert_eq!(
        currencies.get("USDC"),
        previous.currencies.as_ref().unwrap().get("USDC")
    );
    assert_eq!(h.store.positions().as_ref(), positions.as_slice());
    drop(balance);
    h.venue().dispose().unwrap();
}
