//! Negative prices reach the commands that document them, and a token an order
//! command would have to ignore refuses the command instead of changing the order.
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
            {"id":"sel","symbol":"BTCUSDT","side":"sell","type":"limit","status":"open","price":51000,"amount":1,"remaining":1},
            {"id":"eth","symbol":"ETHUSDT","side":"sell","type":"limit","status":"open","price":3100,"amount":1,"remaining":1}
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
/// Every call a sequential command issues, each acknowledged in turn.
fn sequence(command: &str) -> Port {
    let mut e = Exec::default();
    let mut p = Port {
        store: store(),
        ..Default::default()
    };
    e.handle(
        &json!({"symbol":"BTCUSDT","command":command,"reqId":"r"}),
        &mut p,
    );
    let mut acknowledged = 0;
    while acknowledged < p.calls.len() {
        let token = p.calls[acknowledged].0;
        e.complete(token, Ok(json!([])), &mut p);
        acknowledged += 1;
    }
    p
}
/// The one call a command issues.
fn call(command: &str) -> (String, Value) {
    let p = run(command);
    assert_eq!(p.calls.len(), 1, "{command}: {:?}", p.events);
    (p.calls[0].1.clone(), p.calls[0].2.clone())
}

#[test]
fn signed_values_are_positional_and_real_flags_are_not() {
    assert_eq!(positional("bump -5"), ["bump", "-5"]);
    assert_eq!(positional("be -0.5% ETHUSDT"), ["be", "-0.5%", "ETHUSDT"]);
    assert_eq!(positional("move abc -.5"), ["move", "abc", "-.5"]);
    assert_eq!(positional("bump 100 --buys"), ["bump", "100"]);
    assert_eq!(positional("bump 100 -b"), ["bump", "100"]);
    assert_eq!(positional("x --name value -v tail"), ["x"]);
    assert_eq!(positional("x -n 3 --on=1 tail"), ["x", "tail"]);
    assert_eq!(positional("x -v -5"), ["x", "-5"]);
}

#[test]
fn documented_bump_and_move_forms_shift_down() {
    for (command, expected) in [
        ("bump -5", vec![("abc", 48995.), ("sel", 50995.)]),
        ("bump sell -5", vec![("sel", 50995.)]),
        ("bump sell -5 ETHUSDT", vec![("eth", 3095.)]),
        ("bump buy bottom 5 -2 BTCUSDT", vec![("abc", 48998.)]),
    ] {
        let p = sequence(command);
        assert_eq!(p.calls.len(), expected.len(), "{command}: {:?}", p.events);
        for (call, (id, price)) in p.calls.iter().zip(expected) {
            assert_eq!(call.2[0]["order"]["id"], id, "{command}: {call:?}");
            assert_eq!(call.2[0]["update"]["price"], price, "{command}: {call:?}");
        }
    }
    let (_, args) = call("move abc -5");
    assert_eq!(args[0]["update"]["price"], 48995.);
}

#[test]
fn documented_stop_forms_trigger_below_the_last_price() {
    for (command, symbol, trigger) in [
        ("stop -1%", "BTCUSDT", 49500.),
        ("stop at -10%", "BTCUSDT", 45000.),
        ("stop at -100", "BTCUSDT", 49900.),
        ("stop at -100 ETHUSDT", "ETHUSDT", 2900.),
        ("stop sell 0.01 at -100", "BTCUSDT", 49900.),
    ] {
        let (_, args) = call(command);
        let order = &args[0];
        assert_eq!(order["symbol"], symbol, "{command}");
        assert_eq!(order["side"], "sell", "{command}");
        assert_eq!(order["type"], "stop_market", "{command}");
        assert_eq!(order["triggerPrice"], trigger, "{command}: {order}");
    }
}

/// A bare number is not a stop size (looksLikeSize needs `$`), so
/// this catalog form covers the whole position, as legacy StopCommand does.
#[test]
fn a_bare_stop_size_is_ignored_like_legacy() {
    let (_, args) = call("stop sell 0.01 at -100");
    assert_eq!(args[0]["amount"], 0.);
}

#[test]
fn a_negative_breakeven_offset_is_not_dropped_to_entry() {
    let (_, args) = call("be -0.5%");
    assert_eq!(args[0]["triggerPrice"], 39800.);
    let (_, args) = call("be -0.5% ETHUSDT");
    assert_eq!(args[0]["symbol"], "ETHUSDT");
    assert_eq!(args[0]["triggerPrice"], 1990.);
}

#[test]
fn documented_negative_scale_prices_the_rungs_below_the_last_price() {
    let (_, args) = call("scale buy $180 into 2 from -10% to -12%");
    let prices: Vec<_> = args[0]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["price"].as_f64().unwrap())
        .collect();
    assert_eq!(prices, [44000., 45000.]);
    let (_, args) = call("scale buy $1000 into 10 from -0.25% to -1% ETHUSDT");
    assert_eq!(args[0][0]["symbol"], "ETHUSDT");
    assert_eq!(args[0][0]["price"], 2970.);
}

#[test]
fn negative_limit_and_stop_operands_on_buy_and_sell_are_relative() {
    let (_, args) = call("buy $100 at -1%");
    assert_eq!(args[0]["price"], 49500.);
}

#[test]
fn real_flags_still_parse_on_the_commands_that_document_them() {
    let p = run("bump 100 --buys");
    assert_eq!(p.calls.len(), 1, "{:?}", p.events);
    assert_eq!(p.calls[0].2[0]["order"]["id"], "abc");
    let p = run("bump 100 -s");
    assert_eq!(p.calls.len(), 1, "{:?}", p.events);
    assert_eq!(p.calls[0].2[0]["order"]["id"], "sel");
    let p = run("orders --all");
    assert!(p.calls.is_empty());
    assert!(format!("{:?}", p.events).contains("ETHUSDT"));
}

/// Each of these used to lose a token and could still have placed an order.
#[test]
fn a_token_an_order_command_would_ignore_refuses_the_command() {
    for command in [
        "close -50%",
        "close -1",
        "close -1 ETHUSDT",
        "close -$100",
        "close --all ETHUSDT",
        "trail -1%",
        "buy -1",
        "buy -$100",
        "buy -100$",
        "buy $100 -5",
        "buy $100 -1%",
        "buy $100 at 40000 -5",
        "buy $100 at 40000 -5 reduce",
        "buy $100 at 40000 -x reduce",
        "buy $100 --reduce",
        "sell $100 -r reduce",
        "stop -$500 at 1%",
        "stop -5 at 1%",
        "stop 1% -5",
        "stop at 1% -5 ETHUSDT",
        "be -1 -2",
        "be --x ETHUSDT",
        "move abc 49000 -5",
        "scale buy $180 into 2 from 1000 to 2000 -1 reduce",
        "scale buy $180 into 2 from 1000 to 2000 --x reduce",
        "scale buy -$180 into 2 from 1000 to 2000",
        "scale buy $180 into -2 from 1000 to 2000",
        "bump 100 --buys ETHUSDT",
        "bump --buys 100",
        "bump 100 --x",
        "chase buy $100 -1%",
        "chase buy $100 --x reduce",
        "twap buy $100 -5m",
        "swarm buy -$100 into 2",
        "swarm buy $100 into 2 --x reduce",
        "nuke --orders",
        "nuke -5",
        "cancel --buys",
        "cancel -5",
        "lev -5",
        "lev --x 5",
        "kill --all",
    ] {
        let p = run(command);
        assert!(p.calls.is_empty(), "{command} issued {:?}", p.calls);
        let result = &p.events.iter().rev().find(|(op, _)| op == "exec-result");
        assert_eq!(
            result.map(|(_, r)| r["success"].clone()),
            Some(json!(false)),
            "{command}: {:?}",
            p.events
        );
    }
}

#[test]
fn a_relative_price_at_or_below_zero_never_reaches_the_venue() {
    for command in [
        "stop at -100000",
        "stop -200%",
        "be -200%",
        "bump -100000",
        "move abc -100000",
        "buy $100 at -200%",
        "scale buy $180 into 2 from -200% to -300%",
    ] {
        let p = run(command);
        assert!(p.calls.is_empty(), "{command} issued {:?}", p.calls);
    }
}
