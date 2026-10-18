fn started_private(h: &Harness) -> std::rc::Rc<support::Socket> {
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    h.runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("private"))
        .unwrap()
        .1
        .clone()
}

#[test]
fn private_reopen_refreshes_positions_orders_and_balance() {
    let h = fixture_with_tick_interval(Some(60_000.0));
    let private = started_private(&h);
    h.runtime.advance(1_000.0);
    h.http.routes.borrow_mut().retain(|(method, path, _)| {
        method != "GET" || !["/v3/account", "/v3/account-balance"].contains(&path.as_str())
    });
    h.http.ok("GET", "/v3/account", json!({"id":"16909060","l2key":"fixture-l2","positions":[{"symbol":"BTC-USDC","side":"LONG","size":"4","entryPrice":"123","customImr":"0.1"}],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
    h.http.ok(
        "GET",
        "/v3/account-balance",
        json!({"totalEquityValue":"777","maintenanceMargin":"11","initialMargin":"111"}),
    );
    h.http.requests.borrow_mut().clear();
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Open);
    for _ in 0..8 {
        h.runtime.pump();
    }
    assert!(
        !h.http.calls("/v3/account").is_empty(),
        "reopen must dispatch the source position snapshot override; calls={:?}, reports={:?}, console={:?}",
        h.http.requests.borrow(),
        h.runtime.reports.borrow(),
        h.runtime.console.borrow()
    );
    assert!(!h.http.calls("/v3/open-orders").is_empty());
    assert_eq!(h.store.balance().unwrap().total, 1269.0);
    assert_eq!(h.store.balance().unwrap().upnl, -492.0);
    assert_eq!(h.store.balance().unwrap().free, 666.0);
    assert_eq!(
        h.store
            .positions()
            .unwrap()
            .iter()
            .find(|p| p.id == "BTC-USDC:long")
            .unwrap()
            .contracts,
        4.0
    );
    assert!(
        h.store.loaded().unwrap().positions
            && h.store.loaded().unwrap().orders
            && h.store.loaded().unwrap().balance
    );
    done(&h);
}

#[test]
fn pong_replaces_the_private_self_heal_ping_timer() {
    let h = fixture();
    let private = started_private(&h);
    let ping_count = || {
        private
            .sent
            .borrow()
            .iter()
            .filter(|text| text.as_str() == "{\"op\":\"ping\"}")
            .count()
    };
    let before = ping_count();
    h.runtime.advance(1_000.0);
    frame(&private, json!({"op":"pong"}));
    h.runtime.pump();
    h.runtime.advance(4_000.0);
    assert_eq!(
        ping_count(),
        before,
        "pong must cancel the previous self-heal deadline"
    );
    h.runtime.advance(1_000.0);
    assert_eq!(
        ping_count(),
        before + 1,
        "pong must replace self-heal, not start a second ping chain"
    );
    done(&h);
}

#[test]
fn private_close_cancels_self_heal_while_replacement_dial_is_pending() {
    let h = fixture();
    let private = started_private(&h);
    h.runtime.hold_dials.set(true);
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Close {
            code: Some(1000),
            reason: "fixture close".into(),
        });
    h.runtime.pump();
    h.runtime.console.borrow_mut().clear();
    h.runtime.advance(5_001.0);
    assert!(
        !h.runtime
            .console
            .borrow()
            .iter()
            .any(|message| message.contains("Skipping ping/pong")),
        "closed socket must not run the self-heal callback: {:?}",
        h.runtime.console.borrow()
    );
    done(&h);
}

#[test]
fn private_fill_topic_retains_execution_for_history_reads() {
    let h = fixture();
    let private = started_private(&h);
    let orders = h.store.orders().unwrap().to_vec();
    let balance = h.store.balance().unwrap().clone();
    h.http.ok("GET", "/v3/fills", json!([]));
    h.http.ok("GET", "/v3/history-trades", json!([]));
    frame(
        &private,
        json!({"topic":"ws_zk_accounts_v3","contents":{"fills":[{"symbol":"BTC-USDC","side":"BUY","createdAt":h.runtime.now.get(),"price":"123.25","size":"2.5","fee":"-0.125","orderId":"ws-fill-1","token":"USDC"}]}}),
    );
    h.runtime.pump();
    assert_eq!(h.store.orders().unwrap().as_slice(), orders.as_slice());
    assert_eq!(h.store.balance().unwrap(), balance);
    let executions = h
        .drive(h.venue().fetch_historical_fills(
            serde_json::from_value(json!({"symbol":"BTCUSDC","limit":10})).unwrap(),
        ))
        .unwrap();
    let execution = executions
        .iter()
        .find(|e| e.order_id.as_deref() == Some("ws-fill-1"))
        .unwrap();
    assert_eq!(execution.symbol, "BTCUSDC");
    assert_eq!(execution.price, 123.25);
    assert_eq!(execution.qty, 2.5);
    assert_eq!(execution.commission, Some(0.125));
    assert_eq!(execution.commission_asset.as_deref(), Some("USDC"));
    done(&h);
}

#[test]
fn private_wallet_only_frame_preserves_rest_owned_balance() {
    let h = fixture();
    let private = started_private(&h);
    let balance = h.store.balance().unwrap().clone();
    let orders = h.store.orders().unwrap().to_vec();
    let positions = h.store.positions().unwrap().to_vec();
    frame(
        &private,
        json!({"topic":"ws_zk_accounts_v3","contents":{"wallets":[{"token":"USDC","balance":"9999"}]},"op":"ping","args":["wallet-frame"]}),
    );
    h.runtime.pump();
    assert_eq!(h.store.balance().unwrap(), balance);
    assert_eq!(h.store.orders().unwrap().as_slice(), orders.as_slice());
    assert_eq!(
        h.store.positions().unwrap().as_slice(),
        positions.as_slice()
    );
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|text| text.contains("wallet-frame"))
    );
    assert!(h.runtime.reports.borrow().is_empty());
    done(&h);
}
