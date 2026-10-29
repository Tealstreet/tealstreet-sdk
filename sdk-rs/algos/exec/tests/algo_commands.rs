use safe_cex_exec::{AlgoReply, AlgoRequest, Exec, Host};
use serde_json::{Value, json};
#[derive(Default)]
struct Port {
    store: Value,
    requests: Vec<AlgoRequest>,
    events: Vec<(String, Value)>,
}
impl Host for Port {
    fn snapshot(&self) -> Value {
        self.store.clone()
    }
    fn call(&mut self, _: u64, _: &str, _: Value) -> Result<(), Value> {
        panic!("algo must use the injected factory")
    }
    fn algo(&mut self, request: AlgoRequest) -> Result<AlgoReply, Value> {
        self.requests.push(request);
        Ok(AlgoReply {
            message: "started".into(),
            ..Default::default()
        })
    }
    fn emit(&mut self, op: &str, data: Value) {
        self.events.push((op.into(), data));
    }
}
fn port() -> Port {
    Port {
        store: json!({"venue":"binance","markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":50000}],"positions":[],"orders":[],"balance":{"free":1000}}),
        ..Default::default()
    }
}
fn run(p: &mut Port, text: &str) {
    Exec::default().handle(&json!({"symbol":"BTCUSDT","command":text,"reqId":"r"}), p);
}
#[test]
fn chase_sizes_from_store_and_captures_termination_anchor() {
    let mut p = port();
    run(&mut p, "chase buy BTC $100 to 2%");
    let AlgoRequest::ChaseStart { options, errors } = &p.requests[0] else {
        panic!()
    };
    assert!(errors.is_empty());
    assert_eq!(options.len(), 1);
    let o = &options[0];
    assert_eq!(o["size"], 0.002);
    assert_eq!(o["symbol"], "BTCUSDT");
    assert_eq!(o["maxAttempts"], 1000);
    assert_eq!(
        o["terminate"],
        json!({"type":"percent","value":2.,"anchorPrice":50000.})
    );
    assert!(o.get("accountName").is_none());
    assert!(o.get("clientId").is_none());
}
#[test]
fn twap_units_and_chaser_policies_match_direct_options() {
    let mut p = port();
    run(
        &mut p,
        "twap chase sell BTC $100 into 5 over 2 timeout 75% next fail market reduce",
    );
    let AlgoRequest::TwapStart(o) = &p.requests[0] else {
        panic!()
    };
    assert_eq!(o["duration"], 2.);
    assert_eq!(o["ordersCount"], 5);
    assert_eq!(o["size"], 0.002);
    assert_eq!(o["side"], "sell");
    assert_eq!(o["reduceOnly"], true);
    assert_eq!(o["useChaserOrders"], true);
    assert_eq!(o["timeoutPct"], 75.);
    assert_eq!(o["timeoutAction"], "next");
    assert_eq!(o["failAction"], "market");
    run(&mut p, "twap buy $100 duration=1h30m slices=3");
    let AlgoRequest::TwapStart(o) = &p.requests[1] else {
        panic!()
    };
    assert_eq!(o["duration"], 90.);
}
#[test]
fn invalid_symbols_sizes_durations_and_incomplete_modifiers_do_not_start_algos() {
    for text in [
        "twap buy ETHFI $100 over 1",
        "twap buy 100 over 1",
        "twap buy $100 duration=1x",
        "twap buy $100 into 0 over 1",
        "chase buy SOLX $100",
        "chase buy $100 to",
        "chase buy $100 to 61k",
    ] {
        let mut p = port();
        run(&mut p, text);
        assert!(p.requests.is_empty(), "{text}");
        assert!(
            p.events
                .iter()
                .any(|(op, e)| op == "exec-result" && e["success"] == false),
            "{text}"
        );
    }
}
#[test]
fn algo_controls_delegate_to_the_same_typed_book() {
    let mut p = port();
    for text in [
        "chase cancel keep",
        "chasers",
        "chasers cancel all",
        "twap list",
        "twap stop abc",
    ] {
        run(&mut p, text);
    }
    assert_eq!(
        p.requests,
        vec![
            AlgoRequest::ChaseCancel { keep: true },
            AlgoRequest::ChasersList,
            AlgoRequest::ChasersCancel("all".into()),
            AlgoRequest::TwapList,
            AlgoRequest::TwapStop("abc".into())
        ]
    );
}
