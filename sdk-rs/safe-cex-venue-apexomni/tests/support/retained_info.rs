use safe_cex_types::retained_json::RetainedJsonInput as Input;

#[test]
fn maintained_private_frame_info_capture() {
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/retained-private-info.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows,
        json!([
            {"textUnits":[55296],"keyUnits":[56320],"overflow":"infinity","negativeZero":true},
            {"textUnits":[55296],"keyUnits":[56320],"overflow":"infinity","negativeZero":true}
        ])
    );
}

#[test]
fn private_frame_info_keeps_opaque_utf16_and_wire_numbers() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let h = fixture();
    // Start with an existing REST position so the source's partial
    // updatePositions operation has a matching row to update.
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/v3/account");
    h.http.ok("GET", "/v3/account", json!({"id":"16909060","l2key":"fixture-l2","positions":[{"symbol":"BTC-USDC","side":"LONG","size":"1","entryPrice":"100","customImr":"0.1"}],"contractAccount":{"makerFeeRate":"0.0012","takerFeeRate":"0.0013"}}));
    let private = started_private(&h);
    let mut order = raw_order();
    order["id"] = json!("private-retained-order");
    order["opaque"] = json!("OPAQUE_WIRE_VALUE");
    let raw = json!({"topic":"ws_zk_accounts_v3","contents":{
        "orders":[order],
        "positions":[{"symbol":"BTC-USDC","side":"LONG","size":"4","entryPrice":"123","customImr":"0.1","opaque":"OPAQUE_WIRE_VALUE"}]
    }}).to_string().replace("\"OPAQUE_WIRE_VALUE\"",r#"{"text":"\ud800","\udc00":"opaque-key-value","overflow":1e400,"negativeZero":-0}"#);
    private
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(raw)));
    h.runtime.pump();
    let check = |row: &Input| {
        let opaque = row.get("opaque").unwrap();
        assert_eq!(
            opaque.get("text").unwrap().as_text(),
            Some([0xd800].as_slice())
        );
        assert!(
            opaque
                .as_object()
                .unwrap()
                .iter()
                .any(|(key, value)| key == &[0xdc00] && value.text_equals("opaque-key-value"))
        );
        assert_eq!(
            opaque.get("overflow").unwrap().as_f64(),
            Some(f64::INFINITY)
        );
        assert_eq!(
            opaque
                .get("negativeZero")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
    };
    {
        let orders = h.store.orders();
        let order = orders
            .iter()
            .find(|o| o.id == "private-retained-order")
            .expect("private order with opaque metadata must reach actual Store");
        check(order.info.value().unwrap());
    }
    {
        let positions = h.store.positions();
        let position = positions
            .iter()
            .find(|p| p.contracts == 4.0)
            .unwrap_or_else(|| panic!("private position with opaque metadata must reach actual Store: reports={:?}, console={:?}", h.runtime.reports.borrow(),h.runtime.console.borrow()));
        check(position.info.value().unwrap());
    }
    done(&h);
}
fn opaque_info() -> safe_cex_exchange::js_runtime::value::Js {
    use safe_cex_exchange::js_runtime::value::Js;
    let opaque = Js::object(vec![
        ("text", Js::String(vec![0xd800].into())),
        ("nan", Js::Number(f64::NAN)),
        ("infinity", Js::Number(f64::INFINITY)),
        ("negativeZero", Js::Number(-0.0)),
        ("ownUndefined", Js::Undefined),
    ]);
    opaque
        .define_key(&[0xdc00], Js::str("opaque-key-value"))
        .unwrap();
    opaque
}
fn assert_opaque_info(opaque: &Input) {
    assert_eq!(
        opaque.get("text").unwrap().as_text(),
        Some([0xd800].as_slice())
    );
    assert!(opaque.get("nan").unwrap().as_f64().unwrap().is_nan());
    assert_eq!(
        opaque.get("infinity").unwrap().as_f64().unwrap(),
        f64::INFINITY
    );
    assert_eq!(
        opaque
            .get("negativeZero")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(matches!(opaque.get("ownUndefined"), Some(Input::Undefined)));
    assert!(
        opaque
            .as_object()
            .unwrap()
            .iter()
            .any(|(k, v)| k == &[0xdc00] && v.text_equals("opaque-key-value"))
    );
}
#[test]
fn rest_info_keeps_opaque_utf16_and_native_numbers() {
    use safe_cex_exchange::js_runtime::value::{Js, from_json};
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/v3/open-orders");
    let row = from_json(&raw_order());
    row.define("opaque", opaque_info()).unwrap();
    h.http.route_native(
        "GET",
        "/v3/open-orders",
        200.0,
        Js::object(vec![
            ("code", Js::Number(0.0)),
            ("data", Js::array(vec![row, from_json(&raw_order())])),
        ]),
    );
    let orders = h
        .drive(h.venue().fetch_orders(None))
        .expect("opaque info must not refuse the response");
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].id, "order-1");
    assert_eq!(orders[0].amount, 1.234);
    let opaque = orders[0].info.value().unwrap().get("opaque").unwrap();
    assert_opaque_info(opaque);
    assert!(orders[1].info.value().unwrap().get("opaque").is_none());
    done(&h);
}

#[test]
fn rest_market_info_and_stored_config_keep_opaque_utf16() {
    use safe_cex_exchange::js_runtime::value::{Js, from_json};
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/v3/symbols");
    let row = from_json(&contract());
    row.define("opaque", opaque_info()).unwrap();
    let control = from_json(&contract());
    control.define("symbol", Js::str("ETH-USDC")).unwrap();
    control
        .define("crossSymbolName", Js::str("ETHUSDC"))
        .unwrap();
    control.define("baseTokenId", Js::str("ETH")).unwrap();
    h.http.route_native(
        "GET",
        "/v3/symbols",
        200.0,
        Js::object(vec![
            ("code", Js::Number(0.)),
            (
                "data",
                Js::object(vec![(
                    "contractConfig",
                    Js::object(vec![
                        ("perpetualContract", Js::array(vec![row, control])),
                        ("opaque", opaque_info()),
                    ]),
                )]),
            ),
        ]),
    );
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("opaque config must not refuse market load");
    assert_eq!(markets.len(), 2);
    // Inspect the actual committed config before checking the returned model.
    {
        let config = h.store.zk_link_config();
        let config = config.as_deref().unwrap().get("contractConfig").unwrap();
        assert_opaque_info(config.get("opaque").unwrap());
        assert_opaque_info(
            config.get("perpetualContract").unwrap().as_array().unwrap()[0]
                .get("opaque")
                .unwrap(),
        );
    }
    assert_eq!(markets[0].id, "BTC-USDC");
    assert_eq!(markets[0].precision.amount, 0.001);
    assert_opaque_info(markets[0].info.as_ref().unwrap().get("opaque").unwrap());
    assert_eq!(markets[1].id, "ETH-USDC");
    assert!(markets[1].info.as_ref().unwrap().get("opaque").is_none());
    Store::update(
        h.store.as_ref(),
        safe_cex_store::contract::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_opaque_info(
        h.store.markets_by_id()["BTC-USDC"]
            .info
            .as_ref()
            .unwrap()
            .get("opaque")
            .unwrap(),
    );
    // The real signer consumes its named config fields and ignores opaque siblings.
    assert_eq!(
        h.drive(h.exchange.base().place_order(h.exchange.as_ref(), placed()))
            .unwrap(),
        ["9007199254740993"]
    );
    done(&h);
}
#[test]
fn rest_balance_info_keeps_opaque_utf16() {
    use safe_cex_exchange::js_runtime::value::{Js, from_json};
    let h = fixture();
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/v3/account-balance");
    let row =
        from_json(&json!({"totalEquityValue":"100","maintenanceMargin":"2","initialMargin":"10"}));
    row.define("opaque", opaque_info()).unwrap();
    h.http.route_native(
        "GET",
        "/v3/account-balance",
        200.,
        Js::object(vec![("code", Js::Number(0.)), ("data", row)]),
    );
    h.drive(h.venue().start()).unwrap();
    let balance = h.store.balance();
    assert_eq!(balance.total, 100.);
    assert_eq!(balance.free, 90.);
    assert_eq!(balance.used, 2.);
    assert_opaque_info(balance.info.as_ref().unwrap().get("opaque").unwrap());
    drop(balance);
    done(&h);
}
#[test]
fn actual_typescript_keeps_opaque_info_and_config() {
    let source = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/exchange.cjs"))
        .arg("--retained-info")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual TS retained info: {}",
        String::from_utf8_lossy(&source.stderr)
    );
}
