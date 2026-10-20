mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Response};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use std::collections::HashMap;
use support::*;

const NOW: f64 = 1_700_000_000_123.0;
fn ready() -> TestEngine {
    let mut e = Engine::new(ApexOmni::new(Options {
        key: Some("api-key".into()),
        secret: Some("mysecret".into()),
        passphrase: Some("pass".into()),
        omni_key_seed: Some(format!("{}1b", "01".repeat(64))),
        ..Options::default()
    }));
    e.start(NOW);
    let symbols = requests(&mut e).remove(0);
    response(&mut e, &symbols, NOW + 1.0, MARKETS);
    let reads = requests(&mut e);
    for request in reads {
        response(
            &mut e,
            &request,
            NOW + 2.0,
            if request.url.ends_with("/v3/account") {
                r#"{"code":0,"data":{"id":"611779989544632629","positions":[],"contractAccount":{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}"#
            } else {
                r#"{"code":0,"data":[]}"#
            },
        );
    }
    for request in requests(&mut e) {
        response(
            &mut e,
            &request,
            NOW + 3.0,
            r#"{"code":0,"data":{"totalEquityValue":"1000","initialMargin":"0","maintenanceMargin":"0"}}"#,
        );
    }
    e.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 60000.0;
        t.mark = 60000.0;
    });
    e.drain_actions().for_each(drop);
    e
}
fn order(extra: serde_json::Value) -> PlaceOrderOpts {
    let mut base = serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":60000});
    for (key, value) in extra.as_object().unwrap() {
        base[key] = value.clone();
    }
    serde_json::from_value(base).unwrap()
}
fn place(e: &mut TestEngine, orders: Vec<PlaceOrderOpts>) -> Vec<Action> {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::PlaceOrders {
            command: 42,
            orders,
        }),
    );
    e.drain_actions().collect()
}
fn http(actions: Vec<Action>) -> Vec<HttpRequest> {
    actions
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(u8::from_str_radix(&text[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}
fn form(request: &HttpRequest) -> HashMap<String, String> {
    request
        .body
        .as_ref()
        .unwrap()
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            (decode(k), decode(v))
        })
        .collect()
}

#[test]
fn caller_id_is_verbatim_in_both_wire_fields_and_changes_signature() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"caller-01/+é"}))],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["clientId"], "caller-01/+é");
    assert_eq!(p["clientOrderId"], p["clientId"]);
    assert_eq!(p["symbol"], "BTC-USDT");
    assert_eq!(p["brokerId"], "6587");
    assert_eq!(p["size"], "0.001");
    assert_eq!(p["limitFee"], "0.030000");
    assert_eq!(p["signature"].len(), 128);
    assert!(!p.contains_key("reduceOnly"));
}

#[test]
fn batch_caller_id_preflight_rejects_before_any_first_write() {
    for bad in [
        serde_json::json!({"clientOrderId":""}),
        serde_json::json!({"clientOrderId":"has space"}),
        serde_json::json!({"clientOrderId":"same","stopLoss":58000}),
        serde_json::json!({"clientOrderId":"same","amount":1001}),
        serde_json::json!({"clientOrderId":"same","type":"stop_market","triggerPrice":58000}),
    ] {
        let mut e = ready();
        let actions = place(&mut e, vec![order(serde_json::json!({})), order(bad)]);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 42,
                result: Err(_)
            })
        )));
    }
    let mut e = ready();
    let actions = place(
        &mut e,
        vec![
            order(serde_json::json!({"clientOrderId":"same"})),
            order(serde_json::json!({"clientOrderId":"same"})),
        ],
    );
    assert!(http(actions).is_empty());
}

#[test]
fn split_bracket_and_unsupported_shapes_fail_before_writes() {
    for extra in [
        serde_json::json!({"amount":1001,"takeProfit":65000}),
        serde_json::json!({"type":"conditional","triggerPrice":59000}),
        serde_json::json!({"type":"trailing_stop_market","price":100}),
        serde_json::json!({"amount":0.0001}),
        serde_json::json!({"price":0}),
    ] {
        let mut e = ready();
        let actions = place(&mut e, vec![order(extra)]);
        assert!(http(actions).is_empty());
    }
}

#[test]
fn standalone_trigger_rounds_before_slippage() {
    let mut e = ready();
    let r=http(place(&mut e,vec![order(serde_json::json!({"type":"stop_market","triggerPrice":100.0498,"slTriggerType":"last_price"}))])).remove(0);
    let p = form(&r);
    assert_eq!(p["triggerPrice"], "100");
    assert_eq!(p["price"], "100.5");
    assert_eq!(p["type"], "STOP_LIMIT");
    assert_eq!(p["triggerPriceType"], "MARKET");
}

#[test]
fn bracket_legs_reverse_side_preserve_exact_size_and_sign_independently() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(
            serde_json::json!({"amount":0.1,"price":53000,"stopLoss":50000,"takeProfit":65000}),
        )],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["limitFee"], "2.650000");
    assert_eq!(p["isOpenTpslOrder"], "true");
    for prefix in ["tp", "sl"] {
        assert_eq!(p[&format!("{prefix}Size")], p["size"]);
        assert_eq!(p[&format!("{prefix}Side")], "SELL");
        assert_ne!(p[&format!("{prefix}ClientOrderId")], p["clientOrderId"]);
        assert_ne!(p[&format!("{prefix}Signature")], p["signature"]);
        assert_eq!(p[&format!("{prefix}Expiration")], p["expiration"]);
    }
    assert_eq!(p["tpPrice"], "64675");
    assert_eq!(p["slPrice"], "49750");
    assert_eq!(p["tpTriggerPriceType"], "MARKET");
    assert_eq!(p["slTriggerPriceType"], "INDEX");
}

#[test]
fn market_execution_price_uses_current_mark_and_default_tif() {
    let mut e = ready();
    let r = http(place(
        &mut e,
        vec![order(serde_json::json!({"type":"market","side":"sell"}))],
    ))
    .remove(0);
    let p = form(&r);
    assert_eq!(p["price"], "58800");
    assert_eq!(p["timeInForce"], "GOOD_TIL_CANCEL");
}

#[test]
fn network_failure_does_not_retry_an_order_and_releases_fifo() {
    let mut e = ready();
    let first = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"first"}))],
    ))
    .remove(0);
    assert!(
        http(place(
            &mut e,
            vec![order(serde_json::json!({"clientOrderId":"second"}))]
        ))
        .is_empty()
    );
    e.handle(
        NOW + 11.0,
        Event::HttpFailed {
            request: first.id,
            error: "connection reset after write",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    let second = http(actions).remove(0);
    assert_eq!(form(&second)["clientOrderId"], "second");
}

#[test]
fn signature_rejection_retries_once_with_same_l2_body_and_fresh_legacy_auth() {
    let mut e = ready();
    let first = http(place(
        &mut e,
        vec![order(serde_json::json!({"clientOrderId":"legacy"}))],
    ))
    .remove(0);
    response(
        &mut e,
        &first,
        NOW + 20.0,
        r#"{"code":20016,"msg":"signature rejected"}"#,
    );
    let retry = requests(&mut e).remove(0);
    assert_eq!(first.body, retry.body);
    assert_ne!(first.headers, retry.headers);
    response(
        &mut e,
        &retry,
        NOW + 30.0,
        r#"{"code":20016,"msg":"signature rejected"}"#,
    );
    assert!(requests(&mut e).is_empty());
}

#[test]
fn http_and_business_rejections_never_cancel_replace_or_retry_writes() {
    for (status, body) in [
        (500, r#"{"msg":"server error"}"#),
        (200, r#"{"code":10003,"msg":"insufficient funds"}"#),
        (200, r#"{"code":0,"data":{}}"#),
        (200, r#"{"code":"broken","data":{"id":"must-not-accept"}}"#),
        (200, r#"{"code":{},"data":{"id":"must-not-accept"}}"#),
        (200, r#"{"code":0.5,"data":{"id":"must-not-accept"}}"#),
    ] {
        let mut e = ready();
        let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
        e.handle(
            NOW + 11.0,
            Event::HttpResponse {
                request: first.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        assert!(requests(&mut e).is_empty());
    }
}

#[test]
fn virtual_cancellation_deduplicates_parent_and_never_retries_ambiguous_write() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec![
                "abc__stop_loss".into(),
                "abc__take_profit".into(),
                "abc".into(),
            ],
        }),
    );
    let first = requests(&mut e).remove(0);
    assert_eq!(form(&first)["id"], "abc");
    e.handle(
        NOW + 20.0,
        Event::HttpFailed {
            request: first.id,
            error: "reset",
        },
    );
    assert!(requests(&mut e).is_empty());
}

#[test]
fn leverage_uses_real_multipart_and_clamps_to_market_rails() {
    let mut e = ready();
    e.handle(
        NOW + 10.0,
        Event::Command(Command::SetLeverage {
            command: 9,
            symbol: "BTCUSDT".into(),
            leverage: 500.0,
        }),
    );
    let request = requests(&mut e).remove(0);
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "Content-Type" && v.starts_with("multipart/form-data; boundary="))
    );
    let body = request.body.unwrap();
    assert!(body.contains("\r\n\r\n0.01000\r\n"));
    assert!(body.contains("\r\n\r\nBTC-USDT\r\n"));
}
