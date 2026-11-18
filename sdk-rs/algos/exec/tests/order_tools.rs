use safe_cex_exec::{Exec, Host};
use serde_json::{Value, json};
#[derive(Default)]
struct Port {
    store: Value,
    calls: Vec<(u64, String, Value)>,
    events: Vec<(String, Value)>,
}
impl Host for Port {
    fn snapshot(&self) -> Value {
        self.store.clone()
    }
    fn call(&mut self, t: u64, m: &str, a: Value) -> Result<(), Value> {
        self.calls.push((t, m.into(), a));
        Ok(())
    }
    fn emit(&mut self, o: &str, d: Value) {
        self.events.push((o.into(), d));
    }
}
fn port() -> Port {
    Port {
        store: json!({"venue":"binance","markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.5}}],"tickers":[{"symbol":"BTCUSDT","last":50000}],"balance":{"free":1000},"positions":[],"orders":[{"id":"low","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":49000,"amount":1,"reduceOnly":false,"lastUpdate":3},{"id":"high","symbol":"BTCUSDT","status":"open","type":"limit","side":"sell","price":51000,"amount":2,"reduceOnly":true,"lastUpdate":1},{"id":"mid","symbol":"BTCUSDT","status":"open","type":"limit","side":"buy","price":50000,"amount":3,"lastUpdate":2},{"id":"stop","symbol":"BTCUSDT","status":"open","type":"stop_market","side":"sell","price":48000},{"id":"elsewhere","symbol":"ETHUSDT","status":"open","type":"limit","side":"buy","price":2000}]}),
        ..Default::default()
    }
}
fn req(command: &str) -> Value {
    json!({"symbol":"BTCUSDT","command":command,"reqId":"r"})
}
fn run(command: &str) -> (Exec, Port) {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&req(command), &mut p);
    (e, p)
}
fn result(p: &Port) -> &Value {
    &p.events
        .iter()
        .rev()
        .find(|(o, _)| o == "exec-result")
        .unwrap()
        .1
}
#[test]
fn cancel_selectors_sort_only_open_limits_in_the_requested_scope() {
    for (command, id) in [
        ("x top 1", "high"),
        ("x bottom 1", "low"),
        ("x first 1", "high"),
        ("x last 1", "low"),
    ] {
        let (_, p) = run(command);
        let rows = p.calls[0].2[0].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], id);
    }
}
#[test]
fn cancel_range_and_side_stops_do_not_sweep_other_order_types() {
    let (_, p) = run("cancel from 48999 to 50001");
    assert_eq!(p.calls[0].2[0].as_array().unwrap().len(), 2);
    let (_, p) = run("cancel sell stops");
    assert_eq!(p.calls[0].2[0][0]["id"], "stop");
    assert_eq!(p.calls[0].2[0].as_array().unwrap().len(), 1);
}
#[test]
fn empty_selector_does_not_issue_empty_cancel_and_bad_scope_refuses() {
    let (_, p) = run("cancel from 1 to 2");
    assert!(p.calls.is_empty());
    assert_eq!(result(&p)["success"], true);
    for text in [
        "cancel top 2 ETHFI",
        "cancel top 0",
        "cancel from 61k to 62k",
    ] {
        let (_, p) = run(text);
        assert!(p.calls.is_empty());
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn bump_filters_side_and_top_count_and_keeps_amend_price_only() {
    let (_, p) = run("bump buy top 1 +0.26");
    assert_eq!(p.calls.len(), 1);
    let a = &p.calls[0].2[0];
    assert_eq!(a["order"]["id"], "mid");
    assert_eq!(a["update"], json!({"price":50000.5}));
    assert_eq!(a["order"]["amount"], 3);
}
#[test]
fn bump_catches_each_order_error_and_does_not_retry_unknown() {
    let (mut e, mut p) = run("bump +10");
    let t = p.calls[0].0;
    e.complete(
        t,
        Err(json!({"message":"lost","outcome":"unknown"})),
        &mut p,
    );
    assert_eq!(p.calls.len(), 2);
    e.complete(p.calls[1].0, Ok(json!([])), &mut p);
    assert_eq!(p.calls.len(), 3);
    e.complete(p.calls[2].0, Ok(json!([])), &mut p);
    e.complete(t, Ok(json!([])), &mut p);
    assert_eq!(p.calls.len(), 3);
    assert_eq!(result(&p)["success"], true);
    assert!(
        p.events
            .iter()
            .any(|(_, d)| d["message"] == "Bumped 2 order(s), 1 failed")
    );
}
#[test]
fn bump_legacy_side_flag_and_strict_amount_do_not_change_unrelated_orders() {
    let (_, p) = run("bump +10 --sells");
    assert_eq!(p.calls[0].2[0]["order"]["id"], "high");
    for command in ["bump 61k", "bump +5 ETHFI", "bump 0"] {
        let (_, p) = run(command);
        assert!(p.calls.is_empty());
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn scale_prepares_all_rungs_then_sends_one_native_batch_and_captures_native_ids() {
    let (mut e, mut p) = run("scale buy $300 into 3 from 49000 to 51000");
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.calls[0].1, "placeOrders");
    let orders = p.calls[0].2[0].as_array().unwrap();
    assert_eq!(orders.len(), 3);
    for (i, o) in orders.iter().enumerate() {
        assert_eq!(o["amount"], 0.002);
        assert_eq!(o["price"], 49000. + i as f64 * 1000.);
        assert_eq!(o["timeInForce"], "PostOnly");
    }
    e.complete(p.calls[0].0, Ok(json!(["one", "two", "three"])), &mut p);
    assert_eq!(
        result(&p)["data"]["orderIds"],
        json!(["one", "two", "three"])
    );
    assert!(
        p.events
            .iter()
            .any(|(_, d)| d["message"] == "Created orders one,two,three")
    );
}
#[test]
fn scale_taker_reduce_and_percent_sizing_preserve_the_selected_side() {
    let mut p = port();
    p.store["positions"] = json!([{"symbol":"BTCUSDT","side":"long","contracts":0.03}]);
    let mut e = Exec::default();
    e.handle(
        &req("scale sell 50% into 3 from 51000 to 52000 reduce taker"),
        &mut p,
    );
    let orders = p.calls[0].2[0].as_array().unwrap();
    assert_eq!(orders.len(), 3);
    for o in orders {
        assert_eq!(o["amount"], 0.005);
        assert_eq!(o["reduceOnly"], true);
        assert!(o.get("timeInForce").is_none());
    }
}
#[test]
fn scale_bounds_unknown_scope_bad_modifier_and_too_small_rungs_refuse_before_any_send() {
    for command in [
        "scale buy $300 into 129 from 49000 to 51000",
        "scale buy $300 into 0 from 49000 to 51000",
        "scale buy $300 into 1 from 49000 to 51000",
        "scale buy $300 into 3 from 49000 to 51000 ETHFI",
        "scale buy $300 into 3 from 49000 to 51000 cubic",
        "scale buy $1 into 3 from 49000 to 51000",
    ] {
        let (_, p) = run(command);
        assert!(p.calls.is_empty(), "{command}");
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn partial_scale_keeps_structured_effects_and_never_resends_batch_or_tail() {
    for outcome in [Value::Null, json!("unknown")] {
        let (mut e, mut p) = run("scale buy $300 into 3 from 49000 to 51000");
        let t = p.calls[0].0;
        let error = json!({"message":"partial","partialOrderIds":["native-one"],"failedOrderIndex":1,"totalOrderCount":3,"outcome":outcome});
        e.complete(t, Err(error.clone()), &mut p);
        e.complete(t, Ok(json!(["late"])), &mut p);
        assert_eq!(p.calls.len(), 1);
        assert_eq!(result(&p)["errorDetails"], error);
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn a_listed_market_named_ro_cannot_capture_the_reduce_filter() {
    let mut p = port();
    p.store["markets"]
        .as_array_mut()
        .unwrap()
        .push(json!({"symbol":"ROUSDT","base":"RO","quote":"USDT"}));
    let mut e = Exec::default();
    e.handle(&req("cancel sells ro"), &mut p);
    assert_eq!(p.calls[0].2[0][0]["symbol"], "BTCUSDT");
    assert_eq!(p.calls[0].2[0][0]["id"], "high");
}
