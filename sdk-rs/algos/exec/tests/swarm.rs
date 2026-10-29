use safe_cex_exec::{Exec, Host};
use serde_json::{Value, json};
use std::collections::VecDeque;
#[derive(Default)]
struct Port {
    store: Value,
    calls: Vec<(u64, String, Value)>,
    events: Vec<(String, Value)>,
    random: VecDeque<f64>,
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
    fn random(&mut self) -> Result<f64, String> {
        self.random.pop_front().ok_or("missing random".into())
    }
}
fn port() -> Port {
    Port {
        store: json!({"venue":"binance","markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":50000}],"balance":{"free":1000},"positions":[],"orders":[]}),
        ..Default::default()
    }
}
fn req(command: &str, id: &str) -> Value {
    json!({"symbol":"BTCUSDT","command":command,"reqId":id})
}
fn result<'a>(p: &'a Port, id: &str) -> &'a Value {
    &p.events
        .iter()
        .rev()
        .find(|(op, d)| op == "exec-result" && d["reqId"] == id)
        .unwrap()
        .1
}
#[test]
fn equal_swarm_fires_every_slice_concurrently_and_absorbs_lot_remainder() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $500 into 3", "r"), &mut p);
    assert_eq!(p.calls.len(), 3);
    let sizes: Vec<_> = p
        .calls
        .iter()
        .map(|c| c.2[0]["amount"].as_f64().unwrap())
        .collect();
    assert_eq!(sizes, vec![0.003, 0.003, 0.004]);
    assert!(p.events.iter().all(|(op, _)| op != "exec-result"));
    for (i, id) in [(2, "third"), (0, "first"), (1, "second")] {
        e.complete(p.calls[i].0, Ok(json!([id])), &mut p);
    }
    assert_eq!(
        result(&p, "r")["data"]["orderIds"],
        json!(["first", "second", "third"])
    );
}
#[test]
fn irregular_weights_match_ts_distribution_and_keep_exact_total_after_rounding() {
    let mut p = port();
    p.random = vec![0., 0.9999999, 0.5].into();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $600 into 3 irregular", "r"), &mut p);
    let sizes: Vec<_> = p
        .calls
        .iter()
        .map(|c| c.2[0]["amount"].as_f64().unwrap())
        .collect();
    assert_eq!(sizes, vec![0.002, 0.006, 0.004]);
}
#[test]
fn mixed_unknown_and_success_keep_only_fulfilled_native_ids_and_never_retry_failed_slice() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm sell $500 into 2", "r"), &mut p);
    let first = p.calls[0].0;
    e.complete(
        first,
        Err(json!({"message":"lost","outcome":"unknown"})),
        &mut p,
    );
    assert!(p.events.iter().all(|(op, _)| op != "exec-result"));
    e.complete(p.calls[1].0, Ok(json!(["real"])), &mut p);
    e.complete(first, Ok(json!(["late"])), &mut p);
    assert_eq!(p.calls.len(), 2);
    assert_eq!(result(&p, "r")["data"]["orderIds"], json!(["real"]));
    assert_eq!(result(&p, "r")["success"], true);
    assert!(
        p.events
            .iter()
            .any(|(_, d)| d["message"] == "Swarm partial: 1 placed, 1 failed")
    );
}
#[test]
fn all_failed_swarm_returns_failure_with_native_error_details() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $500 into 2", "r"), &mut p);
    for i in 0..2 {
        e.complete(
            p.calls[i].0,
            Err(json!({"message":"lost","outcome":"unknown","partialOrderIds":["known"]})),
            &mut p,
        );
    }
    assert_eq!(result(&p, "r")["success"], false);
    assert_eq!(
        result(&p, "r")["errorDetails"]["partialOrderIds"],
        json!(["known"])
    );
    assert_eq!(p.calls.len(), 2);
}
#[test]
fn a_second_swarm_is_refused_whole_against_calls_from_first_payload() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $5000 into 20", "one"), &mut p);
    assert_eq!(p.calls.len(), 20);
    e.handle(&req("swarm buy $5000 into 20", "two"), &mut p);
    assert_eq!(p.calls.len(), 20);
    assert_eq!(result(&p, "two")["success"], false);
}
#[test]
fn oversized_tiny_bad_symbol_and_bad_randomness_swarm_refuse_without_dispatch() {
    for text in [
        "swarm buy $1000 into 33",
        "swarm buy $1000 into 0",
        "swarm buy $1 into 3",
        "swarm buy $1000 into 3 ETHFI",
        "swarm buy $1000 into 3 irregular",
    ] {
        let mut p = port();
        let mut e = Exec::default();
        e.handle(&req(text, "r"), &mut p);
        assert!(p.calls.is_empty(), "{text}");
        assert_eq!(result(&p, "r")["success"], false);
    }
}
#[test]
fn swarm_waits_for_all_slices_before_driving_the_next_sequential_step() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $500 into 2; lev 3", "r"), &mut p);
    e.complete(p.calls[0].0, Ok(json!(["one"])), &mut p);
    assert_eq!(p.calls.len(), 2);
    e.complete(p.calls[1].0, Ok(json!(["two"])), &mut p);
    assert_eq!(p.calls.len(), 3);
    assert_eq!(p.calls[2].1, "setLeverage");
}
#[test]
fn dispose_drops_swarm_completions_and_never_drives_a_later_write() {
    let mut p = port();
    let mut e = Exec::default();
    e.handle(&req("swarm buy $500 into 2; lev 3", "r"), &mut p);
    e.dispose();
    for i in 0..2 {
        e.complete(p.calls[i].0, Ok(json!(["late"])), &mut p);
    }
    assert_eq!(p.calls.len(), 2);
    assert!(p.events.iter().all(|(op, _)| op != "exec-result"));
}
