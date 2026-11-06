use safe_cex_exec::{Exec, Host, positional};
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
    fn call(&mut self, token: u64, method: &str, args: Value) -> Result<(), Value> {
        self.calls.push((token, method.into(), args));
        Ok(())
    }
    fn emit(&mut self, op: &str, data: Value) {
        self.events.push((op.into(), data));
    }
}
fn port() -> Port {
    Port {
        store: json!({
            "markets":[{"symbol":"BTCUSDT"},{"symbol":"ETHUSDT"}],
            "tickers":[{"symbol":"BTCUSDT","last":50000,"mark":50001,"index":49999}],
            "positions":[{"symbol":"BTCUSDT","contracts":2,"leverage":5}],
            "orders":[
                {"id":"abc123","symbol":"BTCUSDT","side":"buy","type":"limit","status":"open"},
                {"id":"def456","symbol":"BTCUSDT","side":"sell","type":"stop_market","status":"open"},
                {"id":"ghi789","symbol":"ETHUSDT","side":"buy","type":"limit","status":"open"}
            ]
        }),
        ..Default::default()
    }
}
fn request(command: &str) -> Value {
    json!({"symbol":"BTCUSDT","command":command,"reqId":"cb:1"})
}
fn results(p: &Port) -> Vec<&Value> {
    p.events
        .iter()
        .filter(|(op, _)| op == "exec-result")
        .map(|(_, data)| data)
        .collect()
}
fn logs(p: &Port) -> Vec<&str> {
    p.events
        .iter()
        .filter(|(op, _)| op == "exec-log")
        .map(|(_, data)| data["message"].as_str().unwrap())
        .collect()
}

// Ported from CommandHandler.test.ts and worker-host/router.test.ts.
#[test]
fn empty_and_unknown_commands_keep_optional_reqid_contract() {
    let mut exec = Exec::default();
    let mut p = port();
    exec.handle(&request(" ; ; "), &mut p);
    assert_eq!(results(&p), [&json!({"reqId":"cb:1","handled":false})]);
    p.events.clear();
    exec.handle(
        &json!({"symbol":"BTCUSDT","command":"does-not-exist extra"}),
        &mut p,
    );
    assert!(results(&p).is_empty());
    assert_eq!(logs(&p), ["Unknown command: does-not-exist"]);
}
#[test]
fn malformed_payload_refuses_visibly_without_call() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&Value::Null, &mut p);
    assert_eq!(logs(&p), ["Invalid command payload."]);
    assert!(p.calls.is_empty());
    assert!(results(&p).is_empty());
}
#[test]
fn sequential_calls_wait_and_last_result_only_is_replied() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel; lev 3; echo done"), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert!(results(&p).is_empty());
    assert_eq!(p.calls[0].1, "cancelSymbolOrders");
    assert_eq!(p.calls[0].2, json!(["BTCUSDT"]));
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(p.calls[1].1, "setLeverage");
    e.complete(p.calls[1].0, Ok(json!([])), &mut p);
    assert_eq!(
        results(&p),
        [&json!({"reqId":"cb:1","handled":true,"success":true})]
    );
    assert_eq!(logs(&p).last(), Some(&"done"));
}
#[test]
fn parallel_slices_do_not_wait_or_replace_last_sequential_result() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("@cancel; @lev 3; echo final"), &mut p);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(results(&p).len(), 1);
    e.complete(
        p.calls[1].0,
        Err(json!({"name":"Rejected","message":"no","outcome":"rejected"})),
        &mut p,
    );
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(results(&p).len(), 1);
    assert_eq!(results(&p)[0]["success"], true);
    assert!(logs(&p).contains(&"Error: Command failed: no"));
}
#[test]
fn parallel_only_reports_handled_false_like_ts() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("@cancel"), &mut p);
    assert_eq!(results(&p), [&json!({"reqId":"cb:1","handled":false})]);
}
// Ported from router.command-outcome.test.ts: known partials stay structured.
#[test]
fn command_failure_preserves_native_outcome_and_partial_ids_once() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel"), &mut p);
    let token = p.calls[0].0;
    let error = json!({"name":"VenueError","message":"partially refused","outcome":"unknown","partialOrderIds":["accepted"]});
    e.complete(token, Err(error.clone()), &mut p);
    let expected = json!({"reqId":"cb:1","handled":true,"success":false,"error":"partially refused","errorReported":true,"errorDetails":error});
    assert_eq!(results(&p), [&expected]);
    e.complete(token, Ok(json!([])), &mut p);
    assert_eq!(results(&p).len(), 1);
    assert_eq!(logs(&p), ["Error: Command failed: partially refused"]);
}
#[test]
fn failure_does_not_stop_an_explicit_semicolon_chain() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel; echo continued"), &mut p);
    e.complete(p.calls[0].0, Err(json!({"message":"no"})), &mut p);
    assert_eq!(results(&p)[0]["success"], true);
    assert_eq!(logs(&p).last(), Some(&"continued"));
}
#[test]
fn socket_disposal_never_replays_or_releases_a_later_chain_write() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel; lev 9"), &mut p);
    let token = p.calls[0].0;
    e.dispose();
    e.complete(token, Ok(json!([])), &mut p);
    e.handle(&request("lev 9"), &mut p);
    assert_eq!(p.calls.len(), 1);
    assert!(results(&p).is_empty());
}
#[test]
fn request_ids_and_completions_are_isolated_across_concurrent_execs() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel"), &mut p);
    e.handle(
        &json!({"symbol":"ETHUSDT","command":"cancel","reqId":"other"}),
        &mut p,
    );
    e.complete(p.calls[1].0, Ok(json!([])), &mut p);
    assert_eq!(results(&p)[0]["reqId"], "other");
    e.complete(p.calls[0].0, Ok(json!([])), &mut p);
    assert_eq!(results(&p)[1]["reqId"], "cb:1");
}
// Dropping stays only for commands that change no order: order commands now
// refuse the same tokens before this runs (refused_tokens.rs), as dropping one
// silently changed their order.
#[test]
fn quote_flag_and_option_tokenization_matches_parse_args() {
    assert_eq!(
        positional("echo \"hello world\" --ignored=true -v -n 3 tail"),
        ["echo", "hello world", "tail"]
    );
    assert_eq!(positional("echo 'quoted text'"), ["echo", "quoted text"]);
    let mut p = port();
    Exec::default().handle(&request("buy $100 -v reduce"), &mut p);
    assert!(p.calls.is_empty());
}
#[test]
fn price_aliases_and_comma_symbols_use_only_the_store() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("lp; mp; ip ETHUSDT"), &mut p);
    assert_eq!(
        logs(&p),
        [
            "Last Price: BTCUSDT @ 50000",
            "Mark Price: BTCUSDT @ 50001",
            "Ticker ETHUSDT was not found or is not loaded yet"
        ]
    );
    assert!(p.calls.is_empty());
}
#[test]
fn filtered_cancels_pass_actual_order_rows_to_existing_method_adapter() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel stops"), &mut p);
    assert_eq!(p.calls[0].1, "cancelOrders");
    assert_eq!(p.calls[0].2[0][0]["id"], "def456");
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("cancel 123,789"), &mut p);
    assert_eq!(p.calls[0].2[0].as_array().unwrap().len(), 2);
}
#[test]
fn leverage_read_and_invalid_set_never_issue_a_write() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(&request("lev; lev 0"), &mut p);
    assert_eq!(logs(&p), ["BTCUSDT: 5x"]);
    assert!(p.calls.is_empty());
    assert_eq!(results(&p)[0]["success"], false);
}

#[test]
fn combined_side_and_reduce_only_filter_is_never_dropped() {
    let mut e = Exec::default();
    let mut p = port();
    p.store["orders"][0]["reduceOnly"] = json!(false);
    e.handle(&request("cancel buys ro"), &mut p);
    assert_eq!(p.calls[0].2, json!([[]]));
}

#[test]
fn one_suffix_intentionally_cancels_multiple_matches_across_symbols_like_ts() {
    let mut e = Exec::default();
    let mut p = port();
    p.store["orders"][2]["id"] = json!("different123");
    e.handle(&request("cancel 123"), &mut p);
    assert_eq!(p.calls[0].2[0].as_array().unwrap().len(), 2);
    assert_ne!(p.calls[0].2[0][0]["symbol"], p.calls[0].2[0][1]["symbol"]);
}

#[test]
fn payload_bounds_refuse_everything_before_any_effect_or_dispatch() {
    for text in [
        "echo x;".repeat(safe_cex_exec::MAX_STEPS + 1),
        "@cancel;".repeat(safe_cex_exec::MAX_PARALLEL + 1),
        "x".repeat(safe_cex_exec::MAX_COMMAND_BYTES + 1),
    ] {
        let mut e = Exec::default();
        let mut p = port();
        e.handle(&request(&text), &mut p);
        assert!(p.calls.is_empty());
        assert_eq!(logs(&p), ["EXEC command exceeds native resource limits"]);
        assert_eq!(results(&p)[0]["success"], false);
    }
}

#[test]
fn parallel_limit_counts_calls_still_in_flight_from_another_request() {
    let mut e = Exec::default();
    let mut p = port();
    e.handle(
        &request(&"@cancel;".repeat(safe_cex_exec::MAX_PARALLEL)),
        &mut p,
    );
    assert_eq!(p.calls.len(), safe_cex_exec::MAX_PARALLEL);
    e.handle(&request("@cancel; echo should-not-run"), &mut p);
    assert_eq!(p.calls.len(), safe_cex_exec::MAX_PARALLEL);
    assert_eq!(
        logs(&p).last(),
        Some(&"EXEC command exceeds native resource limits")
    );
}

#[test]
fn empty_filter_selections_still_call_cancel_orders_like_ts() {
    for head in ["buys", "sells", "limits", "ro", "xro"] {
        let mut e = Exec::default();
        let mut p = port();
        p.store["orders"] = json!([]);
        e.handle(&request(&format!("cancel {head}")), &mut p);
        assert_eq!(p.calls[0].1, "cancelOrders");
        assert_eq!(p.calls[0].2, json!([[]]));
    }
}
