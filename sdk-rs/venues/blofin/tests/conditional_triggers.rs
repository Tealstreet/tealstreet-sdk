mod support;

use safe_cex_engine::model::{OrderSide, OrderType};
use safe_cex_engine::{Command, Event, Method};
use support::*;

fn ready(hedged: bool, algos: &str) -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.reply(
        "margin-mode",
        r#"{"code":"0","data":{"marginMode":"cross"}}"#,
    );
    let mode = if hedged {
        "long_short_mode"
    } else {
        "net_mode"
    };
    h.reply(
        "position-mode",
        &format!(r#"{{"code":"0","data":{{"positionMode":"{mode}"}}}}"#),
    );
    let positions = if hedged {
        format!(
            "{},{}",
            POSITION.replace("\"net\"", "\"long\""),
            POSITION
                .replace("\"net\"", "\"short\"")
                .replace("395624", "395625")
        )
    } else {
        POSITION.into()
    };
    h.reply(
        "account/positions",
        &format!(r#"{{"code":"0","data":[{positions}]}}"#),
    );
    h.reply("balances", r#"{"code":"0","data":[{"currency":"USDT","balance":"1010","available":"1000","frozen":"10"}]}"#);
    h.reply("orders-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-tpsl-pending", r#"{"code":"0","data":[]}"#);
    h.reply("orders-algo-pending", algos);
    if h.pending
        .iter()
        .any(|r| r.url.contains("orders-algo-pending"))
    {
        let terminal = h.take("orders-algo-pending");
        assert!(terminal.url.contains("after=501"));
        h.respond(terminal, r#"{"code":"0","data":[]}"#);
    }
    assert_eq!(h.e.store().hedged(), hedged);
    h
}

fn trigger(id: &str, size: &str, side: &str, position_side: &str) -> String {
    format!(
        r#"{{"instId":"BTC-USDT","algoId":"{id}","orderType":"trigger","side":"{side}","size":"{size}","triggerPrice":"55000","orderPrice":"-1","triggerPriceType":"last","marginMode":"cross","positionSide":"{position_side}","reduceOnly":"true","state":"live"}}"#
    )
}

fn assert_whole_position(h: &Harness) {
    let order = h.e.store().order("501").expect("venue trigger is listed");
    assert_eq!(order.kind, OrderType::Conditional);
    assert_eq!(order.side, OrderSide::Sell);
    assert!(order.reduce_only);
    assert_eq!(
        order.amount, 0.0,
        "whole-position sentinel is not a contract amount"
    );
    assert_eq!(order.remaining, 0.0);
    assert_eq!(order.filled, 0.0);
    assert_eq!(order.close, Some(true));
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(-1.0));
    assert!(h.requests.iter().all(|(method, _)| *method == Method::Get));
}

#[test]
fn agt46_whole_position_trigger_snapshot_has_unspecified_quantity() {
    let row = trigger("501", "-1", "sell", "net");
    let h = ready(false, &format!(r#"{{"code":"0","data":[{row}]}}"#));
    assert!(h.e.store().positions()[0].contracts > 0.0);
    assert_whole_position(&h);
}

#[test]
fn agt46_whole_position_trigger_updates_do_not_copy_the_current_position_size() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    let row = trigger("501", "-1", "sell", "net");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders-algo"}},"data":[{row}]}}"#
    ));
    assert_whole_position(&h);
    let resized = POSITION.replace("\"positions\":\"4\"", "\"positions\":\"9\"");
    h.ws(&format!(
        r#"{{"arg":{{"channel":"positions"}},"data":[{resized}]}}"#
    ));
    assert_eq!(h.e.store().positions()[0].contracts, 0.009);
    h.ws(&format!(
        r#"{{"arg":{{"channel":"orders-algo"}},"data":[{row}]}}"#
    ));
    assert_whole_position(&h);
}

#[test]
fn agt46_sized_trigger_keeps_its_positive_contract_quantity() {
    let row = trigger("501", "3", "sell", "net");
    let h = ready(false, &format!(r#"{{"code":"0","data":[{row}]}}"#));
    let order = h.e.store().order("501").unwrap();
    assert_eq!(order.kind, OrderType::Conditional);
    assert!(order.reduce_only);
    assert_eq!(order.amount, 0.003);
    assert_eq!(order.remaining, 0.003);
    assert_eq!(order.filled, 0.0);
    assert_eq!(order.close, None);
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(3.0));
}

#[test]
fn agt46_reduce_only_hedge_triggers_target_the_closing_position_side() {
    for (side, position_side, trigger_price) in [("sell", "long", 55000), ("buy", "short", 61000)] {
        for limit in [false, true] {
            let mut h = ready(true, r#"{"code":"0","data":[]}"#);
            assert_eq!(h.e.store().positions().len(), 2);
            let mut order = serde_json::json!({"symbol":"BTCUSDT","type":"conditional","side":side,"amount":1,"triggerPrice":trigger_price,"reduceOnly":true});
            if limit {
                order["price"] = serde_json::json!(trigger_price - 100);
            }
            h.handle(
                1.0,
                Event::Command(Command::PlaceOrders {
                    command: 7,
                    orders: serde_json::from_value(serde_json::json!([order])).unwrap(),
                }),
            );
            h.collect();
            assert!(h.failures.is_empty(), "{side}/{limit}: {:?}", h.failures);
            let request = h.take("order-algo");
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_ref().unwrap()).unwrap();
            assert_eq!(body["side"], side);
            assert_eq!(body["positionSide"], position_side);
            assert_eq!(body["orderType"], "trigger");
            assert_eq!(body["reduceOnly"], "true");
            assert_eq!(body["triggerPrice"], trigger_price.to_string());
            assert_eq!(
                body["orderPrice"],
                if limit {
                    (trigger_price - 100).to_string()
                } else {
                    "-1".into()
                }
            );
            assert_eq!(body["size"], "1");
            h.respond(
                request,
                r#"{"code":"0","data":{"algoId":"601","code":"0"}}"#,
            );
            let mut row = body;
            row["algoId"] = serde_json::json!("601");
            row["state"] = serde_json::json!("live");
            h.ws(&serde_json::json!({"arg":{"channel":"orders-algo"},"data":[row]}).to_string());
            let stored =
                h.e.store()
                    .order("601")
                    .expect("accepted closing trigger is listed");
            assert_eq!(stored.kind, OrderType::Conditional);
            assert_eq!(stored.price, f64::from(trigger_price));
            assert_eq!(stored.amount, 0.001);
            assert_eq!(stored.close, None);
            assert!(stored.reduce_only);
            assert_eq!(
                stored.side,
                if side == "sell" {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                }
            );
            assert!(h.failures.is_empty());
        }
    }
}
