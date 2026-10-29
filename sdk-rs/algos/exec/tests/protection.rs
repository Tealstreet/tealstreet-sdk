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
        store: json!({"venue":"hyperliquid","markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.5},"contractSize":0.01},{"symbol":"ETHUSDT","base":"ETH","quote":"USDT","precision":{"amount":0.01,"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":50000,"mark":50000}],"positions":[{"symbol":"BTCUSDT","side":"long","contracts":0.1,"entryPrice":49000}],"orders":[]}),
        ..Default::default()
    }
}
fn req(text: &str) -> Value {
    json!({"symbol":"BTCUSDT","command":text,"reqId":"r"})
}
fn result(p: &Port) -> &Value {
    &p.events
        .iter()
        .rev()
        .find(|(op, _)| op == "exec-result")
        .unwrap()
        .1
}
fn run(text: &str) -> (Exec, Port) {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req(text), &mut p);
    (e, p)
}
#[test]
fn close_converts_full_and_partial_base_position_into_native_contracts() {
    let (_, p) = run("close");
    assert_eq!(p.calls[0].2[0]["amount"], 10.);
    assert_eq!(p.calls[0].2[0]["side"], "sell");
    assert_eq!(p.calls[0].2[0]["reduceOnly"], true);
    let (_, p) = run("close 50%");
    assert_eq!(p.calls[0].2[0]["amount"], 5.);
    let (_, p) = run("close 200%");
    assert_eq!(p.calls[0].2[0]["amount"], 10.);
}
#[test]
fn hedge_partial_close_requires_side_and_picks_opposite_order_side() {
    let mut p = port();
    p.store["positions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"symbol":"BTCUSDT","side":"short","contracts":0.2}));
    let mut e = Exec::default();
    e.handle(&req("close 50%"), &mut p);
    assert!(p.calls.is_empty());
    assert_eq!(result(&p)["success"], false);
    e.handle(&req("close short 50%"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 10.);
    assert_eq!(p.calls[0].2[0]["side"], "buy");
}
#[test]
fn global_close_continues_after_a_failed_symbol_without_retrying_it() {
    let mut p = port();
    p.store["positions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"symbol":"ETHUSDT","side":"short","contracts":3}));
    let mut e = Exec::default();
    e.handle(&req("close all"), &mut p);
    let first = p.calls[0].0;
    e.complete(
        first,
        Err(json!({"message":"lost","outcome":"unknown"})),
        &mut p,
    );
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.calls[1].2[0]["symbol"], "ETHUSDT");
    e.complete(p.calls[1].0, Ok(json!(["real"])), &mut p);
    e.complete(first, Ok(json!(["late"])), &mut p);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(result(&p)["success"], true);
    assert!(result(&p).get("data").is_none());
    assert!(
        p.events
            .iter()
            .any(|(_, d)| d["message"] == "Closing 1 open position(s)")
    );
}
#[test]
fn global_close_missing_market_does_not_abort_other_positions() {
    let mut p = port();
    p.store["positions"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"symbol":"MISSING","side":"long","contracts":1}));
    let mut e = Exec::default();
    e.handle(&req("close all"), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.calls[0].2[0]["symbol"], "BTCUSDT");
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(result(&p)["success"], true);
}
#[test]
fn stop_bare_price_infers_reducing_side_and_native_full_position_shape() {
    let (_, p) = run("stop 48500.26");
    let o = &p.calls[0].2[0];
    assert_eq!(o["type"], "stop_market");
    assert_eq!(o["side"], "sell");
    assert_eq!(o["amount"], 0.);
    assert_eq!(o["price"], 48500.5);
    assert_eq!(o["triggerPrice"], 48500.5);
    assert_eq!(o["reduceOnly"], true);
    let (_, p) = run("stop at 1%");
    assert_eq!(p.calls[0].2[0]["type"], "take_profit_market");
}
#[test]
fn sized_limit_stop_keeps_trigger_and_limit_distinct_and_native_amount() {
    let (_, p) = run("stop sell $500 at 49000 trigger 49500");
    let o = &p.calls[0].2[0];
    assert_eq!(o["amount"], 1.);
    assert_eq!(o["price"], 49000.);
    assert_eq!(o["triggerPrice"], 49500.);
}
#[test]
fn unsupported_infinity_and_malformed_trigger_refuse_before_call() {
    let mut p = port();
    p.store["venue"] = json!("lighter");
    let mut e = Exec::default();
    e.handle(&req("stop 48000"), &mut p);
    assert!(p.calls.is_empty());
    assert!(
        result(&p)["error"]
            .as_str()
            .unwrap()
            .contains("does not support Full")
    );
    for command in ["stop", "stop at", "stop 48000 trigger 0"] {
        let (_, p) = run(command);
        assert!(p.calls.is_empty(), "{command}");
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn breakeven_cancels_only_protective_stop_before_replacement() {
    let mut p = port();
    p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"sl","type":"stop_market","side":"sell"},{"symbol":"BTCUSDT","id":"tp","type":"take_profit_market","side":"sell"}]);
    let mut e = Exec::default();
    e.handle(&req("be +1%"), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.calls[0].1, "cancelOrders");
    assert_eq!(p.calls[0].2[0].as_array().unwrap().len(), 1);
    assert_eq!(p.calls[0].2[0][0]["id"], "sl");
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(p.calls[1].2[0]["price"], 49490.);
    assert_eq!(p.calls[1].2[0]["amount"], 0.);
}
#[test]
fn failed_breakeven_cancel_never_sends_replacement_and_late_completion_is_ignored() {
    let mut p = port();
    p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"sl","type":"stop_market","side":"sell"}]);
    let mut e = Exec::default();
    e.handle(&req("be"), &mut p);
    let t = p.calls[0].0;
    e.complete(
        t,
        Err(json!({"message":"lost","outcome":"unknown"})),
        &mut p,
    );
    e.complete(t, Ok(json!([])), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(result(&p)["success"], false);
}
#[test]
fn underwater_breakeven_warns_that_it_is_not_a_stop_loss() {
    let (mut e, mut p) = run("be 51000");
    assert_eq!(p.calls[0].2[0]["type"], "take_profit_market");
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert!(p.events.iter().any(|(_, d)| {
        d["message"]
            .as_str()
            .is_some_and(|s| s.contains("position has no stop loss now"))
    }));
}
#[test]
fn trailing_distance_uses_current_price_and_capability_gate() {
    let mut p = port();
    p.store["venue"] = json!("binance");
    let mut e = Exec::default();
    e.handle(&req("trail 20%"), &mut p);
    assert_eq!(p.calls[0].2[0]["price"], 40000.);
    assert_eq!(p.calls[0].2[0]["type"], "trailing_stop_market");
    let (_, p) = run("trail 20%");
    assert!(p.calls.is_empty());
    assert!(
        result(&p)["error"]
            .as_str()
            .unwrap()
            .contains("does not support trailing")
    );
}
#[test]
fn move_uses_unique_prefix_and_tick_rounding_without_changing_size() {
    let mut p = port();
    p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"abcd","type":"limit","side":"buy","status":"open","price":49000,"amount":2}]);
    let mut e = Exec::default();
    e.handle(&req("mv abc +0.26"), &mut p);
    assert_eq!(p.calls[0].1, "updateOrder");
    assert_eq!(p.calls[0].2[0]["update"], json!({"price":49000.5}));
    assert_eq!(p.calls[0].2[0]["order"]["id"], "abcd");
    p.store["orders"]
        .as_array_mut()
        .unwrap()
        .push(json!({"symbol":"BTCUSDT","id":"abce","type":"limit","status":"open","price":50000}));
    e.handle(&req("move abc 49500"), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(result(&p)["error"], "Ambiguous order ID");
}

#[test]
fn null_close_reply_is_unknown_and_does_not_become_success() {
    let (mut e, mut p) = run("close");
    e.complete(p.calls[0].0, Ok(Value::Null), &mut p);
    assert_eq!(result(&p)["success"], false);
    assert_eq!(result(&p)["errorDetails"]["outcome"], "unknown");
}

#[test]
fn failed_replacement_after_a_confirmed_cancel_is_explicitly_unprotected_and_never_retried() {
    for outcome in ["rejected", "unknown"] {
        let mut p = port();
        p.store["orders"] =
            json!([{"symbol":"BTCUSDT","id":"sl","type":"stop_market","side":"sell"}]);
        let mut e = Exec::default();
        e.handle(&req("be"), &mut p);
        e.complete(p.calls[0].0, Ok(json!([])), &mut p);
        let replacement = p.calls[1].0;
        e.complete(
            replacement,
            Err(json!({"message":"lost replacement","outcome":outcome})),
            &mut p,
        );
        e.complete(replacement, Ok(json!(["late"])), &mut p);
        assert_eq!(p.calls.len(), 2);
        assert_eq!(result(&p)["success"], false);
        assert!(
            result(&p)["error"]
                .as_str()
                .unwrap()
                .contains("stop-loss removed, replacement not confirmed — position unprotected")
        );
        assert_eq!(result(&p)["errorDetails"]["outcome"], outcome);
    }
}
#[test]
fn replacement_preparation_failure_after_cancel_is_also_explicitly_unprotected() {
    let mut p = port();
    p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"sl","type":"stop_market","side":"sell"}]);
    let mut e = Exec::default();
    e.handle(&req("be"), &mut p);
    p.store["markets"] = json!([]);
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert!(
        result(&p)["error"]
            .as_str()
            .unwrap()
            .contains("position unprotected")
    );
}

#[test]
fn unknown_explicit_protective_or_close_symbol_cannot_fall_back_to_focused_position() {
    for text in [
        "close ETHFI",
        "close long SOLX",
        "stop SOLX 150",
        "stop 150 SOLX",
        "be ETHFI",
        "trail 20% SOLX",
    ] {
        let (_, p) = run(text);
        assert!(p.calls.is_empty(), "{text}");
        assert!(
            result(&p)["error"]
                .as_str()
                .unwrap()
                .contains("Unknown symbol"),
            "{text}"
        );
    }
    for text in ["close", "stop 150"] {
        let (_, p) = run(text);
        assert_eq!(p.calls.len(), 1);
        assert_eq!(p.calls[0].2[0]["symbol"], "BTCUSDT");
    }
}
#[test]
fn malformed_move_and_trigger_prices_refuse_the_whole_token() {
    for malformed in ["61k", "61,000", "6100O"] {
        let mut p = port();
        p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"abc","type":"limit","side":"sell","status":"open","price":62000}]);
        let mut e = Exec::default();
        e.handle(&req(&format!("mv abc {malformed}")), &mut p);
        assert!(p.calls.is_empty());
        assert_eq!(result(&p)["error"], "Invalid price");
        e.handle(&req(&format!("stop 48000 trigger {malformed}")), &mut p);
        assert!(p.calls.is_empty());
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn unprotected_replacement_surfaces_symbol_in_error_result_and_one_error_notice() {
    let mut p = port();
    p.store["orders"] = json!([{"symbol":"BTCUSDT","id":"sl","type":"stop_market","side":"sell"}]);
    let mut e = Exec::default();
    e.handle(&req("be"), &mut p);
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    let token = p.calls[1].0;
    e.complete(
        token,
        Err(json!({"message":"lost","outcome":"unknown"})),
        &mut p,
    );
    e.complete(token, Ok(json!(["late"])), &mut p);
    let message = result(&p)["error"].as_str().unwrap();
    assert!(message.contains("BTCUSDT"));
    assert!(message.contains("no stop is in place"));
    let notices: Vec<_> = p.events.iter().filter(|(op, _)| op == "error").collect();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].1, json!(message));
    assert_eq!(p.calls.len(), 2);
}
