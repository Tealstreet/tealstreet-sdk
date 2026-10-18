//! The actual Exchange cancellation entrypoint, constructor, signing and HTTP host.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::*;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

const CANCEL_ALL: &str = "/api/v2/mix/order/cancel-all-orders";
const NORMAL: &str = "/api/v3/trade/unfilled-orders";
const STRATEGY: &str = "/api/v3/trade/unfilled-strategy-orders";
const CANCEL_NORMAL: &str = "/api/v3/trade/cancel-order";
const CANCEL_STRATEGY: &str = "/api/v3/trade/cancel-strategy-order";
fn classic(http: &Http) {
    http.route(
        "GET",
        "/api/v3/account/settings",
        400.0,
        json!({"code":"40084","msg":"Classic Account mode","requestTime":1,"data":null}),
    );
    http.ok("POST", CANCEL_ALL, json!({}));
}
fn row(id: &str) -> Value {
    json!({"orderId":id,"clientOid":format!("client-{id}"),"symbol":"BTCUSDT","side":"buy","orderType":"limit","price":"100","qty":"2","cumExecQty":"0","status":"live","tradeSide":"open","posSide":"long","reduceOnly":"NO","force":"gtc","cTime":"1700000000000","posMode":"one_way_mode"})
}
#[test]
fn real_cancel_all_classic_calls_venue_without_store_orders() {
    for (name, product, coin) in [
        (ExchangeName::Bitget, "usdt-futures", "USDT"),
        (ExchangeName::Bitgetusdc, "usdc-futures", "USDC"),
    ] {
        let h = harness(name, options(), classic);
        assert!(h.store.orders().unwrap().is_empty());
        h.drive(h.venue().cancel_all_orders()).unwrap();
        let calls = h.http.calls(CANCEL_ALL);
        assert_eq!(
            calls.len(),
            1,
            "real classic cancel-all must call venue with empty Store"
        );
        assert_eq!(
            calls[0].data,
            json!({"productType":product,"marginCoin":coin})
        );
        assert_eq!(calls[0].headers["ACCESS-KEY"], "fixture-key");
        assert!(calls[0].headers["ACCESS-SIGN"].is_string());
    }
}
#[test]
fn real_cancel_all_uta_fetches_unmapped_normal_and_plan_orders() {
    for (name, category) in [
        (ExchangeName::Bitget, "USDT-FUTURES"),
        (ExchangeName::Bitgetusdc, "USDC-FUTURES"),
    ] {
        let h = harness(name, options(), |http| {
            http.ok(
                "GET",
                "/api/v3/account/settings",
                json!({"holdMode":"one_way_mode"}),
            );
            http.ok("GET", NORMAL, json!({"list":[row("unseen-normal")]}));
            let mut plan = row("unseen-plan");
            plan["planType"] = json!("loss_plan");
            plan["triggerPrice"] = json!("95");
            http.ok("GET", STRATEGY, json!({"list":[plan]}));
            http.ok("POST", CANCEL_NORMAL, json!({}));
            http.ok("POST", CANCEL_STRATEGY, json!({}));
        });
        assert!(h.store.orders().unwrap().is_empty());
        h.drive(h.venue().cancel_all_orders()).unwrap();
        let normal = h.http.calls(NORMAL);
        let strategy = h.http.calls(STRATEGY);
        assert_eq!(
            normal.len(),
            1,
            "real UTA cancel-all must fetch venue normal orders"
        );
        assert_eq!(
            strategy.len(),
            1,
            "real UTA cancel-all must fetch venue plan orders"
        );
        assert_eq!(normal[0].params["category"], category);
        assert_eq!(
            strategy[0].params,
            json!({"category":category,"type":"tpsl"})
        );
        let normal = h.http.calls(CANCEL_NORMAL);
        let strategy = h.http.calls(CANCEL_STRATEGY);
        assert_eq!(normal.len(), 1, "unmapped normal order must be cancelled");
        assert_eq!(strategy.len(), 1, "unmapped TP/SL plan must be cancelled");
        assert_eq!(
            normal[0].data,
            json!({"category":category,"orderId":"unseen-normal","clientOid":"client-unseen-normal"})
        );
        assert_eq!(
            strategy[0].data,
            json!({"orderId":"unseen-plan","clientOid":"client-unseen-plan"})
        );
        assert!(
            h.store.orders().unwrap().is_empty(),
            "source pure venue sweep does not publish fetched orders"
        );
        assert!(
            h.http.calls(CANCEL_ALL).is_empty(),
            "UTA must not call Classic cancellation"
        );
    }
}
fn market(symbol: &str, base: &str) -> Value {
    json!({"symbol":symbol,"baseCoin":base,"quoteCoin":"USD","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"1","minTradeNum":"1","maxOrderQty":"100","minLever":"1","maxLever":"100"})
}
#[test]
fn real_inverse_cancel_all_uses_each_unique_market_margin_coin() {
    let h = harness(ExchangeName::Bitgetinverse, options(), |http| {
        http.ok(
            "GET",
            "/api/v2/mix/market/contracts",
            json!([
                market("BTCUSD", "BTC"),
                market("ETHUSD", "ETH"),
                market("BTCUSD-OTHER", "BTC")
            ]),
        );
        http.ok("POST", CANCEL_ALL, json!({}));
    });
    let markets = h.drive(h.venue().fetch_markets()).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert!(h.store.orders().unwrap().is_empty());
    h.drive(h.venue().cancel_all_orders()).unwrap();
    let payloads: Vec<_> = h
        .http
        .calls(CANCEL_ALL)
        .into_iter()
        .map(|r| r.data)
        .collect();
    assert_eq!(
        payloads,
        vec![
            json!({"productType":"coin-futures","marginCoin":"BTC"}),
            json!({"productType":"coin-futures","marginCoin":"ETH"})
        ],
        "real inverse cancel-all must preserve unique market coin order"
    );
}
#[test]
fn real_cancel_all_emits_and_swallows_venue_failure() {
    let h = harness(ExchangeName::Bitget, options(), |http| {
        http.route(
            "GET",
            "/api/v3/account/settings",
            400.0,
            json!({"code":"40084","msg":"Classic Account mode","requestTime":1,"data":null}),
        );
        http.route(
            "POST",
            CANCEL_ALL,
            503.0,
            json!({"code":"50000","msg":"synthetic cancel refusal","requestTime":1,"data":null}),
        );
    });
    let errors = Rc::new(RefCell::new(Vec::new()));
    let observed = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    h.drive(h.venue().cancel_all_orders())
        .expect("source emits and swallows refusal");
    assert_eq!(
        h.http.calls(CANCEL_ALL).len(),
        1,
        "real cancel-all failure must attempt venue cancellation"
    );
    assert!(
        errors
            .borrow()
            .iter()
            .any(|v| v.to_string().contains("synthetic cancel refusal")),
        "real cancel-all refusal must emit source error"
    );
}

fn seed_symbol_orders(h: &Harness) {
    let orders = ["BTCUSDT", "ETHUSDT"].map(|symbol| serde_json::from_value(json!({"id":format!("cancel-{symbol}"),"symbol":symbol,"side":"buy","type":"limit","status":"open","price":100,"amount":2,"filled":0,"remaining":2,"lastUpdate":1,"reduceOnly":false,"info":{}})).unwrap());
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            orders: Some(orders.into()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
}

#[test]
fn real_symbol_cancel_emits_its_outer_error_and_rethrows_for_classic_families() {
    const BATCH: &str = "/api/v2/mix/order/batch-cancel-orders";
    for name in [ExchangeName::Bitget, ExchangeName::Bitgetusdc] {
        let h = harness(name, options(), |http| {
            classic(http);
            http.route("POST", BATCH, 503.0, json!({"code":"50000","msg":"synthetic symbol cancel refusal","requestTime":1,"data":null}));
        });
        seed_symbol_orders(&h);
        let errors = Rc::new(RefCell::new(Vec::new()));
        let observed = errors.clone();
        h.venue().base().on(
            "error",
            Rc::new(move |_, value| {
                let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
                observed.borrow_mut().push(value.clone());
                Ok(())
            }),
        );
        let error = h
            .drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
            .unwrap_err();
        assert!(!error.message.is_empty(), "{error:?}");
        let calls = h.http.calls(BATCH);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].data["symbol"], "BTCUSDT");
        assert_eq!(
            calls[0].data["orderIdList"],
            json!([{"orderId":"cancel-BTCUSDT"}])
        );
        assert_eq!(
            errors.borrow().len(),
            2,
            "source normal-cancel and symbol-cancel catches each emit before rethrow"
        );
        for error in errors.borrow().iter() {
            assert!(
                error
                    .to_string()
                    .contains("synthetic symbol cancel refusal")
            );
        }
    }
}

#[test]
fn real_inverse_symbol_cancel_preserves_the_inner_swallowed_refusal() {
    const BATCH: &str = "/api/v2/mix/order/batch-cancel-orders";
    let h = harness(ExchangeName::Bitgetinverse, options(), |http| {
        http.route("POST", BATCH, 503.0, json!({"code":"50000","msg":"synthetic inverse cancel refusal","requestTime":1,"data":null}));
    });
    seed_symbol_orders(&h);
    let errors = Rc::new(RefCell::new(Vec::new()));
    let observed = errors.clone();
    h.venue().base().on(
        "error",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    h.drive(h.venue().cancel_symbol_orders("BTCUSDT".into()))
        .unwrap();
    assert_eq!(h.http.calls(BATCH).len(), 1);
    assert_eq!(
        errors.borrow().len(),
        1,
        "inverse normal-cancel swallows the refusal before the outer catch"
    );
}
