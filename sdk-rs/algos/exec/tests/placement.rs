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
    fn emit(&mut self, op: &str, data: Value) {
        self.events.push((op.into(), data));
    }
}
fn port() -> Port {
    Port {
        store: json!({"venue":"binance","markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.5}},{"symbol":"ETHUSDT","base":"ETH","quote":"USDT","precision":{"amount":0.01,"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":50000,"bid":49999,"ask":50001},{"symbol":"ETHUSDT","last":3000}],"balance":{"free":1000},"positions":[],"orders":[]}),
        ..Default::default()
    }
}
fn request(command: &str) -> Value {
    json!({"symbol":"BTCUSDT","command":command,"reqId":"r"})
}
fn result(p: &Port) -> &Value {
    &p.events
        .iter()
        .rev()
        .find(|(op, _)| op == "exec-result")
        .unwrap()
        .1
}
fn run(command: &str) -> (Exec, Port) {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request(command), &mut p);
    (e, p)
}
// Ported sizing/grammar oracles from BuyCommand.test.ts, SellCommand.test.ts and sizeSyntax.test.ts.
#[test]
fn usd_forms_leading_trailing_and_base_symbols_preserve_amount_and_side() {
    for command in ["buy $100", "b 100$", "buy BTC $100", "buy 100$ BTCUSDT"] {
        let (_, p) = run(command);
        assert_eq!(p.calls.len(), 1, "{command}");
        let o = &p.calls[0].2[0];
        assert_eq!(o["amount"], 0.002);
        assert_eq!(o["side"], "buy");
        assert_eq!(o["symbol"], "BTCUSDT");
        assert_eq!(o["type"], "market");
    }
    let (_, p) = run("s 100$ ETH");
    assert_eq!(p.calls[0].2[0]["amount"], 0.03);
    assert_eq!(p.calls[0].2[0]["side"], "sell");
}
#[test]
fn limit_best_and_attached_stop_round_prices_and_keep_post_only() {
    let (_, p) = run("buy $100 at +0.26 stop 49000.26 po");
    let o = &p.calls[0].2[0];
    assert_eq!(o["price"], 50000.5);
    assert_eq!(o["stopLoss"], 49000.5);
    assert_eq!(o["timeInForce"], "PostOnly");
    let (_, p) = run("sell $100 best po");
    assert_eq!(p.calls[0].2[0]["price"], 50001.);
    assert_eq!(p.calls[0].2[0]["type"], "limit");
    let (_, p) = run("buy $100 po");
    assert_eq!(p.calls[0].2[0]["timeInForce"], "GoodTillCancel");
}
#[test]
fn typo_bare_size_stray_price_and_missing_market_refuse_before_dispatch() {
    for command in [
        "buy 100",
        "buy $100 XBTUSD",
        "buy BTC,XBTUSD $100",
        "buy $100 0.05",
        "buy $100 at",
        "buy $0",
        "buy -1%",
        "buy $100 at 0",
    ] {
        let (_, p) = run(command);
        assert!(p.calls.is_empty(), "{command}");
        assert_eq!(result(&p)["success"], false, "{command}");
    }
}
#[test]
fn balance_percent_and_hedged_reduce_percent_use_correct_units() {
    let mut p = port();
    p.store["positions"] = json!([{"symbol":"BTCUSDT","side":"long","contracts":2,"leverage":5},{"symbol":"BTCUSDT","side":"short","contracts":3,"leverage":5}]);
    let mut e = Exec::default();
    e.handle(&request("buy 10%"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 0.01);
    e.handle(&request("buy 50% reduce"), &mut p);
    assert_eq!(p.calls[1].2[0]["amount"], 1.5);
    assert_eq!(p.calls[1].2[0]["reduceOnly"], true);
    e.handle(&request("sell 50%pos reduce"), &mut p);
    assert_eq!(p.calls[2].2[0]["amount"], 1.);
    e.handle(&request("buy 10%bal reduce"), &mut p);
    assert_eq!(p.calls[3].2[0]["amount"], 0.01);
}
#[test]
fn inverse_notional_and_percent_do_not_need_price_and_contract_size_is_respected() {
    let mut p = port();
    p.store["markets"][0]["isInverse"] = json!(true);
    p.store["markets"][0]["contractSize"] = json!(100);
    p.store["markets"][0]["precision"]["amount"] = json!(1);
    p.store["tickers"] = json!([]);
    let mut e = Exec::default();
    e.handle(&request("buy $1000"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 10.);
    e.handle(&request("sell 50%bal"), &mut p);
    assert_eq!(p.calls[1].2[0]["amount"], 5.);
}
#[test]
fn linear_contract_size_and_native_position_extra_match_ts() {
    for contract in [0.01, 1000.] {
        let mut p = port();
        p.store["markets"][0]["contractSize"] = json!(contract);
        p.store["markets"][0]["precision"]["amount"] = json!(0.00000001);
        let mut e = Exec::default();
        e.handle(&request("buy $1000"), &mut p);
        assert_eq!(p.calls.len(), 1, "{:?}", p.events);
        assert_eq!(
            p.calls[0].2[0]["amount"],
            json!((0.02 / contract * 1e8_f64).round() / 1e8)
        );
    }
    let mut p = port();
    p.store["markets"][0]["contractSize"] = json!(0.01);
    p.store["positions"] =
        json!([{"symbol":"BTCUSDT","side":"long","contracts":2,"extra":{"contractsNative":1}}]);
    let mut e = Exec::default();
    e.handle(&request("sell 50%pos reduce"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 100.);
}
#[test]
fn multi_symbol_calls_are_sequential_and_ids_are_native_including_empty_results() {
    let (mut e, mut p) = run("buy BTC,ETH $100");
    assert_eq!(p.calls.len(), 1);
    e.complete(p.calls[0].0, Ok(json!(["real-1", "real-sl"])), &mut p);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.calls[1].2[0]["symbol"], "ETHUSDT");
    e.complete(p.calls[1].0, Ok(json!([])), &mut p);
    assert_eq!(result(&p)["data"]["orderIds"], json!(["real-1", "real-sl"]));
}
#[test]
fn unknown_placement_stops_symbol_fanout_and_never_retries_but_explicit_chain_keeps_ts_parity() {
    let (mut e, mut p) = run("buy BTC,ETH $100");
    let token = p.calls[0].0;
    e.complete(
        token,
        Err(json!({"message":"lost","outcome":"unknown","partialOrderIds":["real"]})),
        &mut p,
    );
    e.complete(token, Ok(json!(["late"])), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(
        result(&p)["errorDetails"]["partialOrderIds"],
        json!(["real"])
    );
    for command in ["cancel; buy $100", "lev 5; buy $100"] {
        let (mut e, mut p) = run(command);
        e.complete(p.calls[0].0, Err(json!({"message":"refused"})), &mut p);
        assert_eq!(p.calls.len(), 2);
        assert_eq!(p.calls[1].1, "placeOrder");
    }
}
#[test]
fn fresh_snapshot_is_used_for_each_successor_and_scope_dispose_never_replays() {
    let (mut e, mut p) = run("buy BTC,ETH $100");
    p.store["tickers"][1]["last"] = json!(1000);
    e.complete(p.calls[0].0, Ok(json!(["first"])), &mut p);
    assert_eq!(p.calls[1].2[0]["amount"], 0.1);
    e.dispose();
    e.complete(p.calls[1].0, Ok(json!(["late"])), &mut p);
    assert!(p.events.iter().all(|(op, _)| op != "exec-result"));
}
#[test]
fn private_health_blocks_entry_but_not_reduce_or_absent_telemetry() {
    let mut p = port();
    p.store["socketHealth"] = json!({"privateInitialized":true,"privateHealthy":false});
    let mut e = Exec::default();
    e.handle(&request("buy $100"), &mut p);
    assert!(p.calls.is_empty());
    e.handle(&request("buy $100 reduce"), &mut p);
    assert_eq!(p.calls.len(), 1);
    p.store["socketHealth"]["privateInitialized"] = json!(false);
    e.handle(&request("buy $100"), &mut p);
    assert_eq!(p.calls.len(), 2);
}
#[test]
fn reduce_only_clamp_subtracts_resting_orders_and_preserves_other_venues() {
    let mut p = port();
    p.store["venue"] = json!("hyperliquid");
    p.store["positions"] = json!([{"symbol":"BTCUSDT","side":"long","contracts":0.01}]);
    p.store["orders"] = json!([{"symbol":"BTCUSDT","side":"sell","type":"limit","status":"open","amount":0.004,"price":49000}]);
    let mut e = Exec::default();
    e.handle(&request("sell $1000 at 49500 reduce"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 0.006);
    p.store["venue"] = json!("binance");
    e.handle(&request("sell $1000 at 49500 reduce"), &mut p);
    assert_eq!(p.calls[1].2[0]["amount"], 0.02);
}
#[test]
fn classified_noncrypto_refuses_and_innovation_remains_crypto() {
    let mut p = port();
    p.store["venue"] = json!("bybit");
    p.store["markets"][0]["info"] = json!({"symbolType":"stock","underlyingTicker":"COIN"});
    let mut e = Exec::default();
    e.handle(&request("buy $100"), &mut p);
    assert!(p.calls.is_empty());
    assert!(
        result(&p)["error"]
            .as_str()
            .unwrap()
            .contains("tokenized equity")
    );
    p.store["markets"][0]["info"]["symbolType"] = json!("innovation");
    e.handle(&request("buy $100"), &mut p);
    assert_eq!(p.calls.len(), 1);
}

#[test]
fn missing_classification_section_refuses_before_bybit_placement() {
    let mut p = port();
    p.store["venue"] = json!("bybit");
    p.store["instrumentClassificationsReady"] = json!(false);
    let mut e = Exec::default();
    e.handle(&request("buy $100"), &mut p);
    assert!(p.calls.is_empty());
    assert!(
        result(&p)["error"]
            .as_str()
            .unwrap()
            .contains("classification is not loaded")
    );
    p.store["instrumentClassificationsReady"] = json!(true);
    e.handle(&request("buy $100"), &mut p);
    assert_eq!(p.calls.len(), 1);
}
#[test]
fn hundredth_contracts_and_tiny_lots_do_not_collapse_size_to_zero() {
    let mut p = port();
    p.store["markets"][0]["precision"]["amount"] = json!(1e-8);
    p.store["markets"][0]["contractSize"] = json!(1000);
    let mut e = Exec::default();
    e.handle(&request("buy $1000"), &mut p);
    assert_eq!(p.calls[0].2[0]["amount"], 0.00002);
}

#[test]
fn a_base_prefix_is_not_an_explicit_market_symbol() {
    for text in ["buy ETHFI $5000", "buy BTCDOM $5000", "buy BTCX $5000"] {
        let (_, p) = run(text);
        assert!(p.calls.is_empty(), "{text}");
        assert_eq!(result(&p)["success"], false);
    }
    for text in ["buy ETH $5000", "buy ETHUSDT $5000", "buy ethusdt $5000"] {
        let (_, p) = run(text);
        assert_eq!(p.calls.len(), 1, "{text}");
        assert_eq!(p.calls[0].2[0]["symbol"], "ETHUSDT");
    }
    for text in ["cancel buys ETHFI", "lev 5 ETHFI", "cancel ETHFI"] {
        let (_, p) = run(text);
        assert!(p.calls.is_empty(), "{text}");
        assert_eq!(result(&p)["success"], false);
    }
}
#[test]
fn null_placement_is_unknown_and_stops_multi_symbol_fanout() {
    let (mut e, mut p) = run("buy BTC,ETH $100");
    e.complete(p.calls[0].0, Ok(Value::Null), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert_eq!(result(&p)["success"], false);
    assert_eq!(result(&p)["errorDetails"]["outcome"], "unknown");
}
