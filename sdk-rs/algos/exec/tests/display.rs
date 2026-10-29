use safe_cex_exec::{Exec, Host};
use serde_json::{Value, json};
#[derive(Default)]
struct Port {
    store: Value,
    events: Vec<(String, Value)>,
    calls: Vec<(u64, String, Value)>,
}
impl Host for Port {
    fn snapshot(&self) -> Value {
        self.store.clone()
    }
    fn call(&mut self, t: u64, m: &str, a: Value) -> Result<(), Value> {
        self.calls.push((t, m.into(), a));
        Ok(())
    }
    fn emit(&mut self, op: &str, v: Value) {
        self.events.push((op.into(), v));
    }
}
fn port() -> Port {
    Port {
        store: json!({"venue":"binance","accountName":"trusted","balance":{"total":1000,"free":800,"used":200},"markets":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","precision":{"amount":0.001,"price":0.1}},{"symbol":"ETHUSDT","base":"ETH","quote":"USDT","precision":{"amount":0.01,"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":50000,"mark":51000}],"positions":[{"symbol":"BTCUSDT","side":"long","contracts":0.02,"notional":1000,"entryPrice":50000,"unrealizedPnl":12.25,"leverage":5},{"symbol":"ETHUSDT","side":"short","contracts":1,"notional":3000,"entryPrice":3000,"unrealizedPnl":-10,"leverage":3}],"orders":[{"symbol":"BTCUSDT","side":"buy","type":"limit","status":"open","amount":0.01,"price":48000},{"symbol":"ETHUSDT","side":"sell","type":"limit","status":"creating","amount":1,"price":3100},{"symbol":"BTCUSDT","status":"closed","price":999}]}),
        ..Default::default()
    }
}
fn run(p: &mut Port, text: &str) -> Value {
    Exec::default().handle(&json!({"symbol":"BTCUSDT","command":text,"reqId":"r"}), p);
    p.events
        .iter()
        .rev()
        .find(|(op, _)| op == "exec-result")
        .unwrap()
        .1
        .clone()
}
fn logs(p: &Port) -> String {
    p.events
        .iter()
        .filter(|(op, _)| op == "exec-log")
        .map(|(_, e)| e["message"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn orders_and_positions_keep_scope_status_and_side_filters() {
    let mut p = port();
    assert_eq!(run(&mut p, "orders")["success"], true);
    let out = logs(&p);
    assert!(out.contains("BTCUSDT"));
    assert!(!out.contains("ETHUSDT"));
    assert!(!out.contains("999"));
    p.events.clear();
    run(&mut p, "o --all");
    assert!(logs(&p).contains("ETHUSDT"));
    p.events.clear();
    run(&mut p, "ps short --all");
    let out = logs(&p);
    assert!(out.contains("ETHUSDT"));
    assert!(!out.contains("BTCUSDT"));
    assert!(out.contains("SHORT"));
    assert!(!out.contains("LONG"));
    assert!(p.calls.is_empty());
}
#[test]
fn margin_ports_the_no_double_currency_oracles() {
    let mut p = port();
    run(&mut p, "bal");
    let out = logs(&p);
    assert!(!out.contains("$$"));
    assert!(out.contains("$1,000.00"));
    assert_eq!(out.matches('$').count(), 5);
    for key in ["Account Lev", "Margin Ratio"] {
        assert!(!out.lines().find(|l| l.contains(key)).unwrap().contains('$'));
    }
    assert!(out.contains("+$2.25"));
}
#[test]
fn coin_and_loaded_absence_stays_unknown_and_a_delivered_empty_section_is_distinct() {
    let mut p = port();
    p.store["coins"] = Value::Null;
    p.store["loaded"] = Value::Null;
    run(&mut p, "spot");
    assert!(logs(&p).contains("unknown / not yet available"));
    assert!(!logs(&p).contains("No spot balances"));
    p.events.clear();
    run(&mut p, "connection");
    assert!(logs(&p).contains("Data Loaded:\n  unknown / not yet available"));
    p.events.clear();
    p.store["coins"] = json!([]);
    run(&mut p, "spot");
    assert!(logs(&p).contains("No spot balances"));
    p.events.clear();
    p.store["loaded"] = json!({"markets":true,"orders":false});
    run(&mut p, "conn");
    assert!(logs(&p).contains("markets: Yes"));
    assert!(logs(&p).contains("orders: No"));
}
#[test]
fn spot_sorts_positive_holdings_and_never_hides_total() {
    let mut p = port();
    p.store["coins"] = json!([{"base":"ETH","amount":2,"notional":6000},{"base":"BTC","amount":1,"notional":50000},{"base":"ZERO","amount":0,"notional":0}]);
    run(&mut p, "spot");
    let out = logs(&p);
    assert!(out.find("BTC").unwrap() < out.find("ETH").unwrap());
    assert!(!out.contains("ZERO"));
    assert!(out.contains("$56,000.00"));
}
#[test]
fn check_never_guesses_crypto_from_missing_or_unlisted_native_labels() {
    let mut p = port();
    run(&mut p, "check BTCUSDT");
    assert!(logs(&p).contains("no instrument classifier implemented for binance"));
    p.store["venue"] = json!("bybit");
    p.store["instrumentClassificationsReady"] = json!(false);
    p.events.clear();
    run(&mut p, "check");
    assert!(logs(&p).contains("unknown / not yet available"));
    p.store["instrumentClassificationsReady"] = json!(true);
    for (label, expected) in [
        (Value::Null, "no symbolType field"),
        (json!("NEW"), "unrecognised bybit symbolType"),
        (json!("innovation"), "crypto perpetual"),
        (
            json!("stock"),
            "tokenized equity (ACME) — may require signing a trading agreement",
        ),
    ] {
        p.events.clear();
        p.store["markets"][0]["instrumentClassification"] =
            json!({"symbolType":label,"underlyingTicker":"ACME"});
        run(&mut p, "check");
        assert!(logs(&p).contains(expected), "{expected}: {}", logs(&p));
    }
    assert!(p.calls.is_empty());
}
#[test]
fn max_uses_position_leverage_and_contract_units_without_writing() {
    let mut p = port();
    run(&mut p, "max");
    let out = logs(&p);
    assert!(out.contains("5x (from open position)"));
    assert!(out.contains("Max notional: $4000.00"));
    assert!(out.contains("Max contracts: 0.08"));
    assert!(p.calls.is_empty());
}
#[test]
fn generated_aliases_are_case_insensitive_and_do_not_enable_wait_bypass() {
    let mut p = port();
    run(&mut p, "ST");
    assert!(logs(&p).contains("Account: trusted"));
    p.events.clear();
    run(&mut p, "HELP lev");
    assert!(logs(&p).contains("Command: leverage"));
    p.events.clear();
    let result = run(&mut p, "SLEEP 100m; lev 8");
    assert_eq!(result["success"], false);
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("chain stopped: wait did not complete")
    );
    assert!(p.calls.is_empty());
}
#[test]
fn hosted_preprocess_refusals_match_the_existing_ts_worker_boundary() {
    for (command, head) in [
        ("set @a 1", "set"),
        ("sim on", "simulation"),
        ("vars", "print"),
        ("alias foo buy", "alias"),
        ("budget $100", "budget"),
    ] {
        let mut p = port();
        let r = run(&mut p, command);
        assert_eq!(r["error"], format!("{head}: preprocess store unavailable"));
        assert_eq!(r["success"], false);
        assert!(p.calls.is_empty());
    }
    let mut p = port();
    assert!(
        run(&mut p, "when (long) (buy $100)")["error"]
            .as_str()
            .unwrap()
            .contains("needs a session that stays open")
    );
}

#[test]
fn focused_position_card_help_examples_and_bare_symbol_preserve_read_only_session_contract() {
    let mut p = port();
    run(&mut p, "position show BTC");
    let out = logs(&p);
    for text in [
        "BTCUSDT  LONG",
        "Size:",
        "Entry:",
        "uPnL:",
        "Liq:    N/A",
        "Lev:    1.00x / 5x",
    ] {
        assert!(out.contains(text), "{text}: {out}");
    }
    p.events.clear();
    run(&mut p, "help buy");
    assert!(logs(&p).contains("Examples:"));
    p.events.clear();
    run(&mut p, "ethusdt");
    assert!(logs(&p).contains("Symbol set to ETHUSDT"));
    p.events.clear();
    run(&mut p, "status");
    assert!(
        logs(&p).contains("Symbol: BTCUSDT"),
        "worker session updates stay client-owned"
    );
    assert!(p.calls.is_empty());
}

#[test]
fn hosted_mutations_refuse_without_processing_or_logging_payload_credentials() {
    for command in [
        "account other",
        "account enable other",
        "accounts add --api-key secret-sentinel --api-secret secret-sentinel",
        "accounts import '{\"apiKey\":\"secret-sentinel\"}'",
        "accounts remove other",
        "prompt preset minimal",
        "prompt prefix secret-sentinel",
        "prompt set secret-sentinel",
    ] {
        let mut p = port();
        let result = run(&mut p, command);
        assert_eq!(result["handled"], true, "{command}");
        assert_eq!(result["success"], false, "{command}");
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("not available in hosted mode")
        );
        assert!(
            !p.events
                .iter()
                .any(|(_, v)| v.to_string().contains("secret-sentinel"))
        );
        assert!(p.calls.is_empty());
    }
}
#[test]
fn account_reads_stay_trusted_and_the_worker_list_remains_empty() {
    let mut p = port();
    run(&mut p, "acc");
    assert!(logs(&p).contains("Current account: trusted"));
    p.events.clear();
    run(&mut p, "accounts");
    assert!(logs(&p).contains("No accounts configured"));
    assert!(!logs(&p).contains("trusted"));
    p.events.clear();
    run(&mut p, "prompt list");
    let out = logs(&p);
    for name in ["minimal", "default", "detailed", "trader", "compact"] {
        assert!(out.contains(name));
    }
    p.events.clear();
    run(&mut p, "prompt preview");
    assert!(logs(&p).contains("[UNKNOWN] [trusted BTCUSDT"));
    assert!(p.calls.is_empty());
}
#[test]
fn exchange_help_uses_inherited_ts_schema_flags_and_excludes_disabled_picker_rows() {
    let mut p = port();
    run(&mut p, "ex");
    let out = logs(&p);
    assert!(out.contains("Bybit"));
    assert!(out.contains("Hyperliquid"));
    assert!(out.contains("grvtSignerAddress, grvtTradingAccountId"));
    assert!(
        !out.lines()
            .any(|l| l.trim_start().starts_with("polymarket "))
    );
    assert!(p.calls.is_empty());
}
#[test]
fn generated_metadata_is_current_including_ts_inheritance_and_prompt_presets() {
    let status = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/generate-cli-catalog.py"
        ))
        .arg("--check")
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn every_registered_ts_command_has_a_native_route_or_an_explicit_hosted_refusal() {
    let catalog: Vec<Value> = serde_json::from_str(include_str!("../src/catalog.json")).unwrap();
    assert_eq!(catalog.len(), 58);
    for row in catalog {
        let mut p = port();
        let name = row["name"].as_str().unwrap();
        Exec::default().handle(
            &json!({"symbol":"BTCUSDT","command":name,"reqId":"r"}),
            &mut p,
        );
        assert!(
            !logs(&p).starts_with("Unknown command:"),
            "missing native route: {name}"
        );
        if p.calls.is_empty() {
            let r = p.events.iter().find(|(op, _)| op == "exec-result").unwrap();
            assert_eq!(r.1["handled"], true, "{name}: {:?}", p.events);
        }
    }
}
#[test]
fn legacy_sequential_refusal_without_reqid_is_visible_as_in_typescript_worker_router() {
    let mut p = port();
    Exec::default().handle(
        &json!({"symbol":"BTCUSDT","command":"prompt set ignored"}),
        &mut p,
    );
    assert!(logs(&p).contains("not available in hosted mode"));
    assert_eq!(
        p.events.iter().filter(|(op, _)| op == "exec-log").count(),
        1
    );
    assert!(p.events.iter().all(|(op, _)| op != "exec-result"));
    assert!(p.calls.is_empty());
}
