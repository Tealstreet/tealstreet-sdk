//! Private MEXC updates over real native-host HTTP and WebSocket transports.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_types::{ExchangeName, OrderStatus, PositionSide};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

async fn started() -> (support::Harness, Rc<Cell<bool>>) {
    let revised = Rc::new(Cell::new(false));
    let assets = revised.clone();
    let h = support::venue_harness(ExchangeName::Mexc, Rc::new(move |r| {
        let data = match r.path.as_str() {
            "/api/v1/contract/detail" => json!([{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","contractSize":0.5,"priceUnit":0.1,"volUnit":1,"minVol":1,"maxVol":1000,"minLeverage":1,"maxLeverage":20,"state":0,"apiAllowed":true}]),
            "/api/v1/private/position/position_mode" => json!(1),
            "/api/v1/private/account/assets" => if assets.get() {
                json!([{"currency":"USDT","availableBalance":140,"equity":200,"unrealized":7,"frozenBalance":0}])
            } else {
                json!([{"currency":"USDT","availableBalance":81,"equity":123,"unrealized":5,"frozenBalance":0}])
            },
            _ => json!([]),
        };
        Some(json!({"success":true,"code":0,"data":data}))
    }), json!({"key":"native-key","secret":support::SECRET,"extra":{"tickInterval":60_000}}),None,safe_cex_exchange::native::native_runtime()).await;
    h.exchange.start().await.expect("MEXC startup");
    assert_eq!(
        h.store.markets().len(),
        1,
        "production market mapping loaded"
    );
    support::eventually("MEXC login", || {
        h.received
            .borrow()
            .iter()
            .any(|text| serde_json::from_str::<Value>(text).is_ok_and(|f| f["method"] == "login"))
    })
    .await;
    push(&h, json!({"channel":"rs.login","data":"success"}));
    support::eventually("MEXC private filters", || {
        h.received.borrow().iter().any(|text| {
            serde_json::from_str::<Value>(text).is_ok_and(|f| f["method"] == "personal.filter")
        })
    })
    .await;
    (h, revised)
}
fn push(h: &support::Harness, frame: Value) {
    // Public and private MEXC sockets share the source URL. Only the private
    // production protocol handles these topics; both receive actual wire text.
    assert!(!h.peers.borrow().is_empty());
    for peer in h.peers.borrow().iter() {
        peer.push.send(frame.to_string()).unwrap();
    }
}
fn order(id: &str, state: u32, filled: f64) -> Value {
    json!({"orderId":id,"symbol":"BTC_USDT","side":1,"orderType":1,"vol":4,"price":29.5,"state":state,"dealVol":filled,"createTime":1000,"updateTime":2000})
}
#[test]
fn native_mexc_order_frames_update_partial_fills_and_terminal_statuses() {
    support::run_local(async {
        let (h, _) = started().await;
        push(
            &h,
            json!({"channel":"push.personal.order","data":[order("ws-1",2,1.5),order("ws-2",1,0.0)]}),
        );
        support::eventually("MEXC private orders", || {
            h.store.orders().iter().any(|o| o.id == "ws-1")
        })
        .await;
        {
            let orders = h.store.orders();
            let first = orders.iter().find(|o| o.id == "ws-1").unwrap();
            assert_eq!(
                (first.amount, first.filled, first.remaining),
                (2.0, 0.75, 1.25)
            );
            assert_eq!(first.status, OrderStatus::Open);
            assert_eq!(first.price, 29.5);
            assert!(
                orders.iter().any(|o| o.id == "ws-2"),
                "entire batch delivered"
            );
        }
        push(
            &h,
            json!({"channel":"push.personal.order","data":[order("ws-1",3,4.0),order("ws-2",4,0.0)]}),
        );
        support::eventually("MEXC terminal orders", || {
            h.store
                .orders()
                .iter()
                .any(|o| o.id == "ws-1" && o.status == OrderStatus::Closed)
        })
        .await;
        let orders = h.store.orders();
        assert_eq!(
            orders.iter().find(|o| o.id == "ws-2").unwrap().status,
            OrderStatus::Canceled
        );
        assert_eq!(
            orders.iter().find(|o| o.id == "ws-1").unwrap().remaining,
            0.0
        );
        drop(orders);
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn native_mexc_fill_frame_emits_the_production_fill_event() {
    support::run_local(async {
        let (h, _) = started().await;
        let fills = Rc::new(RefCell::new(Vec::new()));
        let capture = fills.clone();
        h.exchange.base().on(
            "fill",
            Rc::new(move |_, payload| {
                capture
                    .borrow_mut()
                    .push(safe_cex_exchange::js_runtime::value::to_json(payload).unwrap());
                Ok(())
            }),
        );
        push(
            &h,
            json!({"channel":"push.personal.order.deal","data":{"id":"fill-7","orderId":"order-3","symbol":"BTC_USDT","side":1,"vol":6,"price":21,"fee":2,"profit":5,"role":1,"createTime":1000}}),
        );
        support::eventually("MEXC fill emitter", || !fills.borrow().is_empty()).await;
        assert_eq!(
            &*fills.borrow(),
            &[
                json!({"id":"fill-7","orderId":"order-3","symbol":"BTCUSDT","side":"buy","amount":3.0,"price":21.0,"fee":2.0,"realizedPnl":5.0})
            ]
        );
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn native_mexc_position_frame_updates_the_production_store() {
    support::run_local(async {
        let (h, _) = started().await;
        push(
            &h,
            json!({"channel":"push.personal.position","data":[{"symbol":"BTC_USDT","positionType":2,"holdVol":3,"holdAvgPrice":12.5,"unrealized":1.25,"realised":0.5,"liquidatePrice":8,"im":2,"leverage":10,"openType":1,"updateTime":1700000000000_i64}]}),
        );
        support::eventually("MEXC private position", || {
            h.store
                .positions()
                .iter()
                .any(|p| p.side == PositionSide::Short && p.contracts == 1.5)
        })
        .await;
        let positions = h.store.positions();
        let short = positions
            .iter()
            .find(|p| p.side == PositionSide::Short)
            .unwrap();
        assert_eq!(
            (short.entry_price, short.notional, short.leverage),
            (12.5, 18.75, 10.0)
        );
        assert_eq!(
            (
                short.unrealized_pnl,
                short.realized_pnl.into_value(),
                short.margin_size.into_value(),
            ),
            (1.25, Some(0.5), Some(2.0))
        );
        assert_eq!(short.liquidation_price, 8.0);
        drop(positions);
        h.exchange.dispose().unwrap();
    });
}
#[test]
fn native_mexc_asset_frame_refreshes_balance_from_the_production_endpoint() {
    support::run_local(async {
        let (h, revised) = started().await;
        assert_eq!(h.store.balance().total, 123.0);
        let before = h
            .seen
            .borrow()
            .iter()
            .filter(|r| r.path == "/api/v1/private/account/assets")
            .count();
        revised.set(true);
        push(&h, json!({"channel":"push.personal.asset","data":{}}));
        support::eventually("MEXC debounced balance", || {
            h.store.balance().total == 200.0
        })
        .await;
        let balance = h.store.balance();
        assert_eq!(
            (balance.free, balance.used, balance.upnl),
            (140.0, 60.0, 7.0)
        );
        assert_eq!(
            serde_json::to_value(&balance.currencies).unwrap()["USDT"]["total"],
            200.0
        );
        drop(balance);
        assert_eq!(
            h.seen
                .borrow()
                .iter()
                .filter(|r| r.path == "/api/v1/private/account/assets")
                .count(),
            before + 1,
            "asset frame causes exactly one read before the 60s polling loop"
        );
        h.exchange.dispose().unwrap();
    });
}
