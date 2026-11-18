mod support;

use safe_cex_engine::{Command, ErrorKind, Event, Outcome, model::*};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::*;

fn ready(product: Product, uta: bool) -> Harness {
    let mut h = Harness::started(FixtureOptions {
        product,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Default::default()
    });
    let quote = match product {
        Product::Usdt => "USDT",
        Product::Usdc => "USDC",
        Product::Inverse => "USD",
    };
    h.respond("/contracts", 200, &CONTRACTS.replace("USDT", quote));
    h.respond("/tickers", 200, &TICKERS.replace("USDT", quote));
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    h.respond(
        "/account/settings",
        200,
        if uta {
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#
        } else {
            r#"{"code":"40084","data":null}"#
        },
    );
    if uta {
        h.respond(
            "/current-position",
            200,
            &POSITION
                .replace("USDT", quote)
                .replace("\"data\":[", "\"data\":{\"list\":[")
                .replace("]}", "]}}"),
        );
        h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#);
        h.respond(
            "/unfilled-orders",
            200,
            r#"{"code":"00000","data":{"list":[]}}"#,
        );
        while h
            .requests
            .iter()
            .any(|r| r.url.contains("/unfilled-strategy-orders"))
        {
            h.respond(
                "/unfilled-strategy-orders",
                200,
                r#"{"code":"00000","data":[]}"#,
            );
        }
    } else {
        h.respond("/all-position", 200, &POSITION.replace("USDT", quote));
        let coin = match product {
            Product::Usdt => "USDT",
            Product::Usdc => "USDC",
            Product::Inverse => "BTC",
        };
        h.respond("/account/accounts", 200, &ACCOUNT.replace("USDT", coin));
        h.orders(&OPEN_ORDER.replace("USDT", quote));
    }
    h.login();
    h.requests.clear();
    h
}

fn entry() -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":90,"takeProfit":110,"stopLoss":80})
}

fn place(h: &mut Harness, mut value: Value) {
    value["symbol"] = json!(h.engine.store().markets()[0].symbol);
    h.command(Command::PlaceOrders {
        command: 501,
        orders: vec![serde_json::from_value(value).unwrap()],
    });
}

fn refused(h: &Harness) {
    assert!(
        h.requests.is_empty(),
        "unsupported selector reached I/O: {:?}",
        h.requests
    );
    let error = h.failures.last().expect("typed refusal before send");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.message.contains("trigger"), "{}", error.message);
}

#[test]
fn classic_presets_refuse_every_explicit_selector_while_default_is_unproven() {
    for product in [Product::Usdt, Product::Usdc, Product::Inverse] {
        for field in ["tpTriggerType", "slTriggerType", "triggerType"] {
            for trigger in ["last_price", "mark_price", "index_price"] {
                let mut h = ready(product, false);
                let mut value = entry();
                value[field] = json!(trigger);
                place(&mut h, value);
                refused(&h);
            }
        }
    }
}

#[test]
fn uta_entries_serialize_each_leg_and_refuse_index_before_io() {
    for product in [Product::Usdt, Product::Usdc] {
        for (tp, sl, expected_tp, expected_sl) in [
            ("last_price", "mark_price", "market", "mark"),
            ("mark_price", "last_price", "mark", "market"),
        ] {
            let mut h = ready(product, true);
            let mut value = entry();
            value["tpTriggerType"] = json!(tp);
            value["slTriggerType"] = json!(sl);
            place(&mut h, value);
            let r = h.take("/api/v3/trade/place-order");
            let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["tpTriggerBy"], expected_tp);
            assert_eq!(body["slTriggerBy"], expected_sl);
        }
        for field in ["tpTriggerType", "slTriggerType", "triggerType"] {
            let mut h = ready(product, true);
            let mut value = entry();
            value[field] = json!("index_price");
            place(&mut h, value);
            refused(&h);
        }
    }
}

#[test]
fn uta_position_tpsl_serializes_last_and_mark_but_refuses_index() {
    for (kind, field, wire_field) in [
        ("stop_market", "slTriggerType", "slTriggerBy"),
        ("take_profit_market", "tpTriggerType", "tpTriggerBy"),
    ] {
        for (trigger, expected) in [("last_price", "market"), ("mark_price", "mark")] {
            let mut h = ready(Product::Usdt, true);
            place(
                &mut h,
                json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":80,field:trigger}),
            );
            let r = h.take("/place-strategy-order");
            let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
            assert_eq!(body[wire_field], expected);
        }
        let mut h = ready(Product::Usdt, true);
        place(
            &mut h,
            json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":80,field:"index_price"}),
        );
        refused(&h);
    }
}

#[test]
fn classic_position_tpsl_serializes_last_and_mark_but_refuses_index() {
    for product in [Product::Usdt, Product::Usdc, Product::Inverse] {
        for (kind, field) in [
            ("stop_market", "slTriggerType"),
            ("take_profit_market", "tpTriggerType"),
        ] {
            for (trigger, expected) in [("last_price", "fill_price"), ("mark_price", "mark_price")]
            {
                let mut h = ready(product, false);
                place(
                    &mut h,
                    json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":80,field:trigger}),
                );
                let r = h.take("/place-tpsl-order");
                let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
                assert_eq!(body["triggerType"], expected);
                assert_eq!(
                    body["planType"],
                    if kind == "stop_market" {
                        "pos_loss"
                    } else {
                        "pos_profit"
                    }
                );
            }
            let mut h = ready(product, false);
            place(
                &mut h,
                json!({"symbol":"BTCUSDT","type":kind,"side":"sell","amount":0,"price":80,field:"index_price"}),
            );
            refused(&h);
        }
    }
}

#[test]
fn classic_preset_reads_use_returned_trigger_fields_and_never_invent_a_default() {
    for (field_sl, field_tp) in [
        ("presetStopLossTriggerType", "presetStopSurplusTriggerType"),
        ("presetStopLossType", "presetStopSurplusType"),
    ] {
        for (wire, expected) in [
            ("fill_price", Some(StopTriggerType::LastPrice)),
            ("mark_price", Some(StopTriggerType::MarkPrice)),
            ("", None),
            ("unknown", None),
        ] {
            let mut h = ready(Product::Usdt, false);
            h.private_message(&json!({"arg":{"channel":"orders"},"data":[{
                "orderId":"preset","symbol":"BTCUSDT","size":"1","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live",
                "presetStopLossPrice":"80","presetStopSurplusPrice":"110",field_sl:wire,field_tp:wire
            }]}).to_string());
            for (suffix, sl) in [("stop_loss", true), ("take_profit", false)] {
                let row = h
                    .engine
                    .store()
                    .order(&format!("preset__{suffix}"))
                    .unwrap();
                let extra = row.extra.as_ref().unwrap();
                assert_eq!(
                    if sl {
                        extra.sl_trigger_type
                    } else {
                        extra.tp_trigger_type
                    },
                    expected,
                    "{wire}/{suffix}"
                );
            }
        }
    }
}
