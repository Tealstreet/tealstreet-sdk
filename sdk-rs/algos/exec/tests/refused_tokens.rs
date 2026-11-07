//! The tokenizer drops a dash token and the token after it. A command that can
//! change an order refuses them instead of acting on what is left.
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
    fn call(&mut self, t: u64, m: &str, a: Value) -> Result<(), Value> {
        self.calls.push((t, m.into(), a));
        Ok(())
    }
    fn emit(&mut self, op: &str, data: Value) {
        self.events.push((op.into(), data));
    }
}
fn store() -> Value {
    json!({
        "venue":"binance",
        "markets":[
            {"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.5}},
            {"symbol":"ETHUSDT","base":"ETH","quote":"USDT","precision":{"amount":0.01,"price":0.1}}
        ],
        "tickers":[
            {"symbol":"BTCUSDT","last":50000,"bid":49999,"ask":50001},
            {"symbol":"ETHUSDT","last":3000}
        ],
        "balance":{"free":100000},
        "positions":[
            {"symbol":"BTCUSDT","side":"long","contracts":1,"notional":50000,"entryPrice":40000,"leverage":1},
            {"symbol":"ETHUSDT","side":"long","contracts":2,"notional":6000,"entryPrice":2000,"leverage":1}
        ],
        "orders":[
            {"id":"abc","symbol":"BTCUSDT","side":"buy","type":"limit","status":"open","price":49000,"amount":1,"remaining":1},
            {"id":"sel","symbol":"BTCUSDT","side":"sell","type":"limit","status":"open","price":51000,"amount":1,"remaining":1}
        ]
    })
}
fn run(command: &str) -> Port {
    let mut p = Port {
        store: store(),
        ..Default::default()
    };
    Exec::default().handle(
        &json!({"symbol":"BTCUSDT","command":command,"reqId":"r"}),
        &mut p,
    );
    p
}
fn failure(p: &Port) -> String {
    let result = &p.events.iter().rev().find(|(op, _)| op == "exec-result");
    result
        .filter(|(_, r)| r["success"] == false)
        .and_then(|(_, r)| r["error"].as_str())
        .unwrap_or("")
        .to_string()
}

#[test]
fn a_flag_or_negative_on_a_command_that_can_change_an_order_is_refused_by_name() {
    for (command, token) in [
        // Live: a limit buy at 68080 with no stop.
        ("buy $90 at -5% stop 68080", "-5%"),
        ("buy $250 ETHUSDT at -0.5%", "-0.5%"),
        ("buy $100 stop -1%", "-1%"),
        ("buy $100 stop -1% trigger -0.9%", "-1%"),
        ("sell BTCUSDT $500 at -1% reduce", "-1%"),
        ("stop at -1% trigger 18500", "-1%"),
        // `stop @hi -1%` and `stop $entry -3*$ticksize` after preprocess.
        ("stop 50000 -1%", "-1%"),
        ("stop 40000 -1.5", "-1.5"),
        ("stop at -100", "-100"),
        ("stop sell 0.01 at -100", "-100"),
        ("stop sell 0.01 at -100 ETHUSDT", "-100"),
        ("bump buy bottom 5 -2 ETHUSDT", "-2"),
        ("bump -5 --buys", "-5"),
        ("scale buy $1000 into 10 from -0.25% to -1%", "-0.25%"),
        (
            "scale buy $1000 into 10 from -0.25% to -1% ETHUSDT",
            "-0.25%",
        ),
        (
            "scale buy $1000 into 10 from -0.25% to -1% reduce",
            "-0.25%",
        ),
        ("close -50%", "-50%"),
        ("c -1", "-1"),
        ("close -1 ETHUSDT", "-1"),
        ("close -$100", "-$100"),
        ("close --all ETHUSDT", "--all"),
        ("trail -1%", "-1%"),
        ("buy -1", "-1"),
        ("b -$100", "-$100"),
        ("buy $100 -5", "-5"),
        ("buy $100 -1%", "-1%"),
        ("buy $100 at -1%", "-1%"),
        ("buy $100 at 40000 -5 reduce", "-5"),
        ("buy $100 at 40000 -x reduce", "-x"),
        ("buy $100 --reduce", "--reduce"),
        ("sell $100 -r reduce", "-r"),
        ("s $100 reduce -r", "-r"),
        ("stop -1%", "-1%"),
        ("stop at -10%", "-10%"),
        ("stop at -100 ETHUSDT", "-100"),
        ("stop -$500 at 1%", "-$500"),
        ("stop at 1% -5 ETHUSDT", "-5"),
        ("be -0.5%", "-0.5%"),
        ("breakeven -0.5% ETHUSDT", "-0.5%"),
        ("be --x ETHUSDT", "--x"),
        ("move abc -5", "-5"),
        ("mv abc 49000 -5", "-5"),
        ("scale buy $180 into 2 from -10% to -12%", "-10%"),
        ("scale buy $180 into 2 from 1000 to 2000 -1 reduce", "-1"),
        ("scale buy $180 into 2 from 1000 to 2000 --x reduce", "--x"),
        ("bump -5", "-5"),
        ("bump sell -5 ETHUSDT", "-5"),
        ("bump 100 --buys ETHUSDT", "--buys"),
        ("bump 100 -s ETHUSDT", "-s"),
        ("bump --buys 100", "--buys"),
        ("bump 100 --x", "--x"),
        ("chase buy $100 -1%", "-1%"),
        ("chase buy $100 --x reduce", "--x"),
        ("twap buy $100 -5m", "-5m"),
        ("swarm buy $100 into 2 --x reduce", "--x"),
        ("nuke --orders", "--orders"),
        ("nuke -5", "-5"),
        ("nuke positions --x", "--x"),
        ("cancel --buys", "--buys"),
        ("x -5", "-5"),
        ("lev -5", "-5"),
        ("leverage --x 5", "--x"),
        ("kill --all", "--all"),
    ] {
        let p = run(command);
        assert!(p.calls.is_empty(), "{command} issued {:?}", p.calls);
        let error = failure(&p);
        assert!(
            error.contains(&format!("\"{token}\"")),
            "{command}: {error}"
        );
    }
}

/// A stop size needs a `$`; anything else was ignored and the stop covered the
/// whole position.
#[test]
fn a_stop_refuses_a_token_it_would_ignore_as_a_size() {
    for (command, token) in [
        ("stop sell 0.01 at 70000", "0.01"),
        ("stop 0.01 at 70000", "0.01"),
        ("stop sell 50% at 70000", "50%"),
        ("stop at 70000 $500", "$500"),
        ("stop at 70000 500$", "500$"),
        ("stop 70000 69000", "69000"),
        ("stop at 70000 trigger 69000 5", "5"),
    ] {
        let p = run(command);
        assert!(p.calls.is_empty(), "{command} issued {:?}", p.calls);
        let error = failure(&p);
        assert!(
            error.contains(&format!("\"{token}\"")),
            "{command}: {error}"
        );
    }
    for (command, amount) in [
        ("stop 70000", 0.),
        ("stop at 70000 ETHUSDT", 0.),
        ("stop at 70000 trigger 69000", 0.),
        ("stop sell at 70000", 0.),
        ("stop $500 at 70000", 0.01),
        ("stop sell $500 at 70000", 0.01),
        ("stop 500$ at 70000 trigger 69000", 0.01),
    ] {
        let p = run(command);
        assert_eq!(p.calls.len(), 1, "{command}: {:?}", p.events);
        assert_eq!(p.calls[0].2[0]["amount"], amount, "{command}");
    }
}

#[test]
fn a_price_at_or_below_zero_places_nothing() {
    for command in [
        "stop 0",
        "stop at 0",
        "stop at 0 trigger 100",
        "stop at 100 trigger 0",
        "be 0",
        "move abc 0",
        "buy $100 at 0",
        "buy $100 stop 0",
        "sell $100 at 40000 stop 0",
        "scale buy $180 into 2 from 0 to 100",
        "scale buy $180 into 2 from 100 to 0",
    ] {
        let p = run(command);
        assert!(p.calls.is_empty(), "{command} issued {:?}", p.calls);
        assert!(!failure(&p).is_empty(), "{command}: {:?}", p.events);
    }
}

#[test]
fn working_positive_forms_are_unchanged() {
    for (command, method) in [
        // Preprocess folds `$entry -1%`, `$bid -0.1%`, `$liq -5%` into these.
        ("buy $100 at 39600", "placeOrder"),
        ("buy 50% stop 39200", "placeOrder"),
        ("sell $100 at 28500", "placeOrder"),
        ("stop 39600", "placeOrder"),
        ("stop at 39200", "placeOrder"),
        (
            "scale buy $500 into 5 from 49949.001 to 49749.005",
            "placeOrders",
        ),
        ("buy $100", "placeOrder"),
        ("buy $100 at 40000 reduce", "placeOrder"),
        ("buy $100 at +1%", "placeOrder"),
        ("sell $100 at 60000 po", "placeOrder"),
        ("buy $100 stop 40000", "placeOrder"),
        ("close", "placeOrder"),
        ("close 50%", "placeOrder"),
        ("close ETHUSDT", "placeOrder"),
        ("stop 18500", "placeOrder"),
        ("stop 1%", "placeOrder"),
        ("stop at 0.1% ETHUSDT", "placeOrder"),
        ("stop at +100", "placeOrder"),
        ("stop sell at 1%", "placeOrder"),
        ("stop $500 at 1%", "placeOrder"),
        ("stop buy $500 at 1%", "placeOrder"),
        ("stop 500$ at 1% trigger 19000", "placeOrder"),
        ("be", "placeOrder"),
        ("be +3%", "placeOrder"),
        ("be +100 ETHUSDT", "placeOrder"),
        ("move abc 49500", "updateOrder"),
        ("move abc +10", "updateOrder"),
        ("bump top 1 +5", "updateOrder"),
        ("bump 100 --buys", "updateOrder"),
        ("bump 100 -b", "updateOrder"),
        ("bump 100 --sells", "updateOrder"),
        ("bump 100 -s", "updateOrder"),
        ("scale buy $180 into 2 from 1000 to 2000", "placeOrders"),
        (
            "scale sell $180 into 2 from +1% to +2% reduce",
            "placeOrders",
        ),
        ("nuke orders", "nuke"),
        ("cancel abc", "cancelOrders"),
        ("cancel", "cancelSymbolOrders"),
        ("lev 5", "setLeverage"),
    ] {
        let p = run(command);
        assert_eq!(
            p.calls.first().map(|c| c.1.as_str()),
            Some(method),
            "{command}: {:?}",
            p.events
        );
    }
}

#[test]
fn commands_that_change_no_order_keep_their_flags() {
    assert_eq!(positional("x --name value -v tail"), ["x"]);
    assert_eq!(positional("x -n 3 --on=1 tail"), ["x", "tail"]);
    assert!(failure(&run("orders --all")).is_empty());
    assert!(failure(&run("positions --all")).is_empty());
    for command in [
        "bal --all",
        "simulation --accounts a",
        "script:operator --accounts a --reduce",
        "track max @x $last -1%",
        "echo -5",
    ] {
        assert!(
            !failure(&run(command)).contains("refusing"),
            "{command} was refused"
        );
    }
}

#[test]
fn no_help_text_suggests_a_refused_form() {
    assert!(
        run("stop")
            .events
            .iter()
            .all(|(_, e)| !e.to_string().contains("at -1%"))
    );
    let catalog: Vec<Value> = serde_json::from_str(include_str!("../src/catalog.json")).unwrap();
    for row in catalog {
        for example in row["examples"].as_array().unwrap() {
            let example = example.as_str().unwrap();
            let p = run(example);
            assert!(
                !failure(&p).contains("refusing"),
                "catalog example refused: {example}"
            );
        }
    }
}
