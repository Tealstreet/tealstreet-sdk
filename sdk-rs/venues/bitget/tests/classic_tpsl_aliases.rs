#[path = "../src/venue_state.rs"]
mod source_state;
mod support;

use safe_cex_engine::{Command, Method, Outcome, model::*, wire};
use serde_json::{Value, json};
use support::*;

fn row(plan: &str, execution: Option<Value>, order_type: Option<&str>) -> Value {
    let mut row = json!({
        "orderId":"alias-stop", "symbol":"BTCUSDT", "size":"0.5",
        "triggerPrice":"80", "triggerType":"mark_price", "holdSide":"long",
        "planType":plan, "status":"live", "side":"sell"
    });
    if let Some(value) = execution {
        row["executePrice"] = value;
    }
    if let Some(value) = order_type {
        row["orderType"] = json!(value);
    }
    row
}

fn ingest(row: &Value, rest: bool) -> Harness {
    if !rest {
        let mut h = Harness::classic();
        h.private_message(&json!({"arg":{"channel":"orders-algo"},"data":[row]}).to_string());
        return h;
    }
    let mut h = Harness::private_started();
    h.respond("/account/settings", 200, r#"{"code":"40084","data":null}"#);
    h.respond("/all-position", 200, POSITION);
    h.respond("/account/accounts", 200, ACCOUNT);
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.respond(
        "planType=profit_loss",
        200,
        &json!({"code":"00000","data":{"entrustedList":[row],"endId":""}}).to_string(),
    );
    h.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    h
}

fn state(h: &mut Harness) -> source_state::VenueState {
    let frame = h.engine.snapshot_frame(h.now);
    let (_, section) = wire::sections(frame)
        .find(|(kind, _)| *kind == wire::section::VENUE_STATE)
        .unwrap();
    let length = u32::from_le_bytes(section[..4].try_into().unwrap()) as usize;
    source_state::decode(&section[4..4 + length]).unwrap()
}

fn reload(h: &mut Harness) -> Harness {
    let frame = h.engine.snapshot_frame(h.now).to_vec();
    let mut next = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    next.respond("/contracts", 200, CONTRACTS);
    next.engine.apply_frame(next.now, &frame, true).unwrap();
    next.collect();
    next
}

fn amend(h: &mut Harness) {
    h.command(Command::AmendOrder {
        command: 701,
        id: "alias-stop".into(),
        price: Some(85.0),
        amount: None,
    });
}

fn assert_alias(alias: &str, canonical: &str, kind: OrderType, full: bool) {
    for rest in [false, true] {
        for execution in [None, Some(json!("")), Some(json!("  "))] {
            let mut row = row(alias, execution, Some("market"));
            if full {
                row["size"] = json!("0");
            }
            let mut h = ingest(&row, rest);
            assert_eq!(state(&mut h).plans["alias-stop"], canonical);
            assert_eq!(
                state(&mut h).execution_prices.get("alias-stop"),
                Some(&0.0),
                "{alias}/rest={rest}: documented market execution must be retained"
            );
            let mut mirror = reload(&mut h);
            for engine in [&mut h, &mut mirror] {
                let original = engine.engine.store().order("alias-stop").unwrap().clone();
                assert_eq!(original.kind, kind);
                assert_eq!(original.side, OrderSide::Sell);
                assert_eq!(original.filled, 0.0);
                assert_eq!(original.amount, if full { 0.0 } else { 0.5 });
                assert!(original.reduce_only);
                if kind == OrderType::TakeProfit {
                    engine
                        .engine
                        .store_mut()
                        .update_ticker("BTCUSDT", |t| t.mark = 70.0);
                }
                amend(engine);
                assert!(engine.failures.is_empty(), "{:?}", engine.failures);
                let request = engine.take("/modify-tpsl-order");
                assert_eq!(
                    request.body.as_deref().unwrap(),
                    if full {
                        r#"{"symbol":"BTCUSDT","productType":"usdt-futures","marginCoin":"USDT","orderId":"alias-stop","size":"","triggerPrice":"85","executePrice":"0","triggerType":"mark_price"}"#
                    } else {
                        r#"{"symbol":"BTCUSDT","productType":"usdt-futures","marginCoin":"USDT","orderId":"alias-stop","size":"0.5","triggerPrice":"85","executePrice":"0","triggerType":"mark_price"}"#
                    }
                );
                assert_eq!(
                    engine.engine.store().order("alias-stop").unwrap(),
                    &original
                );
                assert!(!engine.requests.iter().any(|r| r.url.contains("cancel")));
            }
        }
    }
}

#[test]
fn classic_psl_blank_market_execution_matches_position_stop_loss() {
    assert_alias("psl", "pos_loss", OrderType::StopLoss, true);
}

#[test]
fn classic_ptp_blank_market_execution_matches_position_take_profit() {
    assert_alias("ptp", "pos_profit", OrderType::TakeProfit, true);
}

#[test]
fn classic_sl_blank_market_execution_matches_sized_stop_loss() {
    assert_alias("sl", "loss_plan", OrderType::StopLoss, false);
}

#[test]
fn classic_tp_blank_market_execution_matches_sized_take_profit() {
    assert_alias("tp", "profit_plan", OrderType::TakeProfit, false);
}

#[test]
fn short_aliases_still_require_explicit_market_and_unambiguous_execution() {
    for alias in ["psl", "ptp", "sl", "tp"] {
        for rest in [false, true] {
            for (execution, order_type) in [
                (None, None),
                (Some(json!("")), Some("limit")),
                (Some(json!("")), Some("MARKET")),
                (Some(Value::Null), Some("market")),
                (Some(json!("garbled")), Some("market")),
                (Some(json!(-1)), Some("market")),
                (Some(json!(true)), Some("market")),
            ] {
                let mut h = ingest(&row(alias, execution, order_type), rest);
                assert!(!state(&mut h).execution_prices.contains_key("alias-stop"));
                if matches!(alias, "ptp" | "tp") {
                    h.engine
                        .store_mut()
                        .update_ticker("BTCUSDT", |t| t.mark = 70.0);
                }
                amend(&mut h);
                let error = h.failures.last().unwrap();
                assert_eq!(error.outcome, Some(Outcome::NotSent));
                assert!(error.message.contains("execution price is unavailable"));
                assert!(!h.requests.iter().any(|r| r.method == Method::Post));
            }
        }
    }
}

#[test]
fn non_tpsl_and_unknown_plan_spellings_never_prove_blank_market_execution() {
    for plan in [
        "pl",
        "track",
        "track_plan",
        "moving_plan",
        "normal_plan",
        "SL",
        " sl",
        "unknown",
    ] {
        let mut h = ingest(&row(plan, Some(json!("")), Some("market")), false);
        assert!(
            !state(&mut h).execution_prices.contains_key("alias-stop"),
            "{plan}"
        );
    }
}
