//! Private socket frames through the constructed protocol and native Store bridge.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_exchange::socket_lifecycle::SocketSlot;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn names() -> [ExchangeName; 4] {
    [
        ExchangeName::Bybit,
        ExchangeName::Bybitdemo,
        ExchangeName::Bybitinverse,
        ExchangeName::Bybitspot,
    ]
}
fn setup(name: ExchangeName) -> (support::Harness, Rc<RefCell<Vec<Value>>>, String) {
    let inverse = name == ExchangeName::Bybitinverse;
    let symbol = if inverse { "BTCUSD" } else { "BTCUSDT" }.to_owned();
    let h = support::harness_as(name, support::options(), |_| {});
    Store::update(h.store.as_ref(), StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":if inverse {"USD"}else{"USDT"},"active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}}})).unwrap()]),
        tickers: Some(vec![serde_json::from_value(json!({"id":symbol,"symbol":symbol,"last":100,"mark":100,"bid":99,"ask":101,"index":100,"percentage":0,"volume":0,"quoteVolume":0,"fundingRate":0,"fundingTime":0,"openInterest":0})).unwrap()]),
        positions: inverse.then(|| vec![serde_json::from_value(json!({"id":symbol,"symbol":symbol,"side":"long","entryPrice":1,"notional":0,"leverage":1,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0})).unwrap()]),
        balance: Some(serde_json::from_value(json!({"total":100,"free":80,"used":20,"upnl":21})).unwrap()),
        ..Default::default()
    }, false).unwrap();
    let fills = Rc::new(RefCell::new(vec![]));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    (h, fills, symbol)
}
fn push(h: &support::Harness, topic: &str, rows: Value) {
    h.venue()
        .base()
        .get_socket(SocketSlot::Private)
        .unwrap()
        .on_message(&SocketFrame::Text(
            json!({"topic":topic,"data":rows}).to_string(),
        ))
        .unwrap();
}
fn order(symbol: &str) -> Value {
    json!({"orderId":"stream-o","orderLinkId":"","symbol":symbol,"side":"Buy","orderType":"Limit","price":"100","qty":"10","leavesQty":"10","cumExecQty":"0","orderStatus":"New","timeInForce":"GTC","reduceOnly":false,"closeOnTrigger":false,"stopOrderType":"","triggerPrice":"","stopLoss":"","takeProfit":"","tpslMode":"","createType":"CreateByUser","positionIdx":0,"triggerBy":""})
}
fn position(symbol: &str) -> Value {
    json!({"symbol":symbol,"side":"Buy","size":"2","avgPrice":"80","positionValue":"200","leverage":"5","unrealisedPnl":"3","curRealisedPnl":"-1","liqPrice":"50","positionIdx":0,"positionIM":"40","updatedTime":"1700000000000","seq":7,"positionStatus":"Normal"})
}
#[test]
fn private_orders_reach_the_store_for_each_bound_family() {
    for name in names() {
        let (h, fills, symbol) = setup(name);
        push(&h, "order", json!([order(&symbol)]));
        let rows = h.store.orders().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "stream-o");
        assert_eq!(rows[0].symbol, symbol);
        assert_eq!(rows[0].side, safe_cex_types::OrderSide::Buy);
        assert_eq!(rows[0].r#type, safe_cex_types::OrderType::Limit);
        assert_eq!(rows[0].status, safe_cex_types::OrderStatus::Open);
        assert!(fills.borrow().is_empty());
        drop(rows);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn private_partial_fills_emit_once_and_update_each_bound_family() {
    for name in names() {
        let inverse = name == ExchangeName::Bybitinverse;
        let (h, fills, symbol) = setup(name);
        push(&h, "order", json!([order(&symbol)]));
        let mut row = order(&symbol);
        row["orderStatus"] = json!("PartiallyFilled");
        row["cumExecQty"] = json!("2");
        row["leavesQty"] = json!("8");
        row["avgPrice"] = json!("100");
        row["cumExecValue"] = json!(if inverse { "0.02" } else { "200" });
        push(&h, "order", json!([row.clone()]));
        push(&h, "order", json!([row]));
        assert_eq!(
            *fills.borrow(),
            vec![json!({"id":"stream-o","symbol":symbol,"side":"buy","price":100.0,"amount":2.0})]
        );
        assert_eq!(h.store.orders().unwrap().len(), 1);
        assert!(h.store.orders().unwrap()[0].filled > 0.0);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn private_positions_reach_derivative_stores_and_are_ignored_by_spot() {
    for name in names() {
        let spot = name == ExchangeName::Bybitspot;
        let (h, fills, symbol) = setup(name);
        push(&h, "position", json!([position(&symbol)]));
        let rows = h.store.positions().unwrap();
        if spot {
            assert!(rows.is_empty());
        } else {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].symbol, symbol);
            assert_eq!(rows[0].entry_price, 80.0);
            assert_eq!(rows[0].leverage, 5.0);
            assert!(rows[0].contracts > 0.0);
        }
        assert!(fills.borrow().is_empty());
        drop(rows);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn private_balance_follows_source_position_sync_and_ignores_wallet_frames() {
    for name in names() {
        let primary = matches!(name, ExchangeName::Bybit | ExchangeName::Bybitdemo);
        let (h, fills, symbol) = setup(name);
        push(&h, "position", json!([position(&symbol)]));
        let upnl = h.store.balance().unwrap().upnl;
        assert_eq!(upnl, if primary { 3.0 } else { 21.0 });
        push(
            &h,
            "wallet",
            json!([{"totalEquity":"999","totalPerpUPL":"999"}]),
        );
        assert_eq!(h.store.balance().unwrap().upnl, upnl);
        assert_eq!(h.store.balance().unwrap().total, 100.0);
        assert!(fills.borrow().is_empty());
        h.venue().dispose().unwrap();
    }
}

#[test]
fn spot_unknown_stop_types_survive_rest_and_private_store_projection() {
    for stop_type in ["tpslOrder", "OcoOrder", "BidirectionalTpslOrder"] {
        let (h, fills, symbol) = setup(ExchangeName::Bybitspot);
        let known = order(&symbol);
        let mut unknown = order(&symbol);
        unknown["orderId"] = json!("unknown-stop");
        unknown["stopOrderType"] = json!(stop_type);
        unknown["triggerPrice"] = json!("90");
        h.wire.ok(
            "GET",
            "/v5/order/realtime",
            json!({
                "list":[known.clone(),unknown.clone()],"nextPageCursor":""
            }),
        );
        let fetched = h.drive(h.venue().fetch_orders(None)).unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].r#type, safe_cex_types::OrderType::Limit);
        assert_eq!(fetched[1].id, "unknown-stop");
        assert!(matches!(
            fetched[1].r#type,
            safe_cex_types::ModelField::Undefined
        ));
        assert_eq!(fetched[1].price, 90.);
        assert!(
            serde_json::to_value(&fetched[1])
                .unwrap()
                .get("type")
                .is_none()
        );
        push(&h, "order", json!([known, unknown]));
        let orders = h.store.orders().unwrap();
        assert_eq!(orders.len(), 2);
        let retained = orders.iter().find(|o| o.id == "unknown-stop").unwrap();
        assert!(matches!(
            retained.r#type,
            safe_cex_types::ModelField::Undefined
        ));
        assert_eq!(retained.price, 90.);
        assert!(
            serde_json::to_value(retained)
                .unwrap()
                .get("type")
                .is_none()
        );
        assert!(fills.borrow().is_empty());
        assert!(h.runtime.reports.borrow().is_empty());
        drop(orders);
        h.venue().dispose().unwrap();
    }
}
