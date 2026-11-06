mod support;

use safe_cex_engine::{Command, Method, Outcome, model::*, wire};
use serde_json::{Value, json};
use support::*;

fn row(plan: &str, execution: Option<Value>, order_type: Option<&str>) -> Value {
    let mut row = json!({
        "orderId":"stop", "symbol":"BTCUSDT", "size":"0.5", "triggerPrice":"80",
        "triggerType":"mark_price", "holdSide":"long", "planType":plan,
        "status":"live", "side":"sell"
    });
    if let Some(value) = execution {
        row["executePrice"] = value;
    }
    if let Some(value) = order_type {
        row["orderType"] = json!(value);
    }
    row
}

fn source(row: &Value, rest: bool) -> Harness {
    if !rest {
        let mut h = Harness::classic();
        h.private_message(&json!({"arg":{"channel":"orders-algo"},"data":[row]}).to_string());
        return h;
    }
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
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

fn reloaded(frame: &[u8]) -> Harness {
    let mut h = Harness::started_configure(
        FixtureOptions {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..FixtureOptions::default()
        },
        false,
        |engine| engine.set_frame_fed().unwrap(),
    );
    h.respond("/contracts", 200, CONTRACTS);
    h.engine.apply_frame(h.now, frame, true).unwrap();
    h.collect();
    h
}

fn amend(h: &mut Harness) {
    h.command(Command::AmendOrder {
        command: 701,
        id: "stop".into(),
        price: Some(85.0),
        amount: None,
    });
}

fn assert_market_payload(h: &mut Harness, full: bool) {
    let original = h.engine.store().order("stop").unwrap().clone();
    if original.kind == OrderType::TakeProfit {
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.mark = 70.0);
    }
    amend(h);
    assert!(h.failures.is_empty(), "{:?}", h.failures);
    let r = h.take("/modify-tpsl-order");
    assert_eq!(
        r.body.as_deref().unwrap(),
        if full {
            r#"{"symbol":"BTCUSDT","productType":"usdt-futures","marginCoin":"USDT","orderId":"stop","size":"","triggerPrice":"85","executePrice":"0","triggerType":"mark_price"}"#
        } else {
            r#"{"symbol":"BTCUSDT","productType":"usdt-futures","marginCoin":"USDT","orderId":"stop","size":"0.5","triggerPrice":"85","executePrice":"0","triggerType":"mark_price"}"#
        }
    );
    assert_eq!(h.engine.store().order("stop").unwrap(), &original);
    assert_eq!(original.side, OrderSide::Sell);
    assert!(original.reduce_only);
    assert!(!h.requests.iter().any(|r| r.url.contains("cancel")));
}

#[test]
fn explicit_classic_market_tpsl_blank_or_absent_execution_keeps_market_wire() {
    for rest in [true, false] {
        for plan in ["loss_plan", "profit_plan", "pos_loss", "pos_profit"] {
            for execution in [None, Some(json!("")), Some(json!("  "))] {
                let mut row = row(plan, execution, Some("market"));
                let full = plan.starts_with("pos_");
                if full {
                    row["size"] = json!("0");
                }
                assert_market_payload(&mut source(&row, rest), full);
            }
        }
    }
}

#[test]
fn market_execution_survives_v2_frames_and_a_second_engine_reload() {
    for rest in [true, false] {
        let mut source = source(&row("loss_plan", Some(json!("")), Some("market")), rest);
        let frame = source.engine.snapshot_frame(source.now).to_vec();
        let state = |frame: &[u8]| {
            wire::sections(frame)
                .find(|(kind, _)| *kind == wire::section::VENUE_STATE)
                .unwrap()
                .1
                .to_vec()
        };
        assert_eq!(state(&frame)[4], 2);
        let mut first = reloaded(&frame);
        let next = first.engine.snapshot_frame(first.now).to_vec();
        assert_eq!(state(&frame), state(&next));
        assert_market_payload(&mut first, false);
        assert_market_payload(&mut reloaded(&next), false);
    }
}

#[test]
fn ambiguous_execution_never_borrows_market_mode_or_a_live_price() {
    for rest in [true, false] {
        for (execution, order_type) in [
            (None, None),
            (Some(json!("")), Some("limit")),
            (Some(json!("garbled")), Some("market")),
            (Some(Value::Null), Some("market")),
            (Some(json!(-1)), Some("market")),
            (Some(json!(true)), Some("market")),
            (Some(json!({})), Some("market")),
            (Some(json!("NaN")), Some("market")),
        ] {
            let mut h = source(&row("loss_plan", execution, order_type), rest);
            amend(&mut h);
            let error = h.failures.last().unwrap();
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert!(
                error.message.contains("execution price is unavailable"),
                "{error:?}"
            );
            assert!(error.message.contains("re-place"));
            assert!(!h.requests.iter().any(|r| r.method == Method::Post));
            assert_eq!(
                h.engine.store().order("stop").unwrap().status,
                OrderStatus::Open
            );
        }
    }
}

#[test]
fn ordinary_limit_execution_payload_is_unchanged() {
    for rest in [true, false] {
        let mut h = source(&row("loss_plan", Some(json!("79")), Some("limit")), rest);
        amend(&mut h);
        assert_eq!(
            h.take("/modify-tpsl-order").body.as_deref().unwrap(),
            r#"{"symbol":"BTCUSDT","productType":"usdt-futures","marginCoin":"USDT","orderId":"stop","size":"0.5","triggerPrice":"85","executePrice":"79","triggerType":"mark_price"}"#
        );
    }
}
