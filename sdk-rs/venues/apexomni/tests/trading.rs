mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketChannel, SocketId,
};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use std::collections::HashMap;
use support::*;

const NOW: f64 = 1_700_000_000_123.0;
fn ready() -> TestEngine {
    ready_with_socket().0
}
fn ready_with_socket() -> (TestEngine, SocketId) {
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
    let mut socket = None;
    let reads = e
        .drain_actions()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request),
            Action::WsConnect {
                socket: id,
                channel: Some(SocketChannel::Private),
                ..
            } => {
                socket = Some(id);
                None
            }
            _ => None,
        })
        .collect::<Vec<_>>();
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
    (e, socket.unwrap())
}
fn order(extra: serde_json::Value) -> PlaceOrderOpts {
    let mut base = serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":60000});
    for (key, value) in extra.as_object().unwrap() {
        base[key] = value.clone();
    }
    serde_json::from_value(base).unwrap()
}
fn place(e: &mut TestEngine, orders: Vec<PlaceOrderOpts>) -> Vec<Action> {
    place_with_id(e, 42, orders)
}
fn place_with_id(
    e: &mut TestEngine,
    command: safe_cex_engine::CommandId,
    orders: Vec<PlaceOrderOpts>,
) -> Vec<Action> {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::PlaceOrders { command, orders }),
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
fn bybit_only_controls_reject_single_and_whole_batch_without_io_or_state_changes() {
    for extra in [
        serde_json::json!({"bybitBbo":{"sideType":"Counterparty","level":1}}),
        serde_json::json!({"bybitSlippageToleranceTicks":0}),
    ] {
        for batch in [false, true] {
            let mut e = ready();
            let mut orders = vec![];
            if batch {
                orders.push(order(
                    serde_json::json!({"clientOrderId":"first-must-not-place"}),
                ));
            }
            orders.push(order(extra.clone()));
            let actions = place(&mut e, orders);
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::WsSend { .. }))
            );
            assert!(actions.iter().any(|action| matches!(action,
                Action::Notice(Notice::CommandDone { command: 42, result: Err(message) })
                    if message.contains("Bybit BBO and TickSize controls are not supported by ApeX Omni")
            )));
            assert!(e.store().orders().is_empty());
            // A rejected preflight must leave the sequenced trading queue usable.
            assert_eq!(
                http(place(&mut e, vec![order(serde_json::json!({}))])).len(),
                1
            );
        }
    }
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
        http(place_with_id(
            &mut e,
            43,
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

fn parent_update(
    e: &mut TestEngine,
    socket: SocketId,
    at: f64,
    filled: f64,
    status: &str,
    snapshot: bool,
) {
    ws(
        e,
        socket,
        at,
        &format!(
            r#"{{"topic":"ws_zk_accounts_v3","type":"{}","contents":{{"orders":[{{"id":"parent","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","status":"{status}","size":"10","price":"60000","cumSuccessFillSize":"{filled}","updatedAt":{at},"openSlParam":{{"triggerPrice":"50000","triggerPriceType":"INDEX"}},"openTpParams":{{"triggerPrice":"65000","triggerPriceType":"MARKET"}}}}]}}}}"#,
            if snapshot { "snapshot" } else { "delta" },
        ),
    );
}

fn amend(e: &mut TestEngine, id: &str, amount: Option<f64>) -> Vec<HttpRequest> {
    e.handle(
        NOW + 10.0,
        Event::Command(Command::AmendOrder {
            command: 99,
            id: id.into(),
            price: Some(59000.0),
            amount,
        }),
    );
    requests(e)
}

fn amend_fill(e: &mut TestEngine, socket: SocketId, created_at: f64, size: f64) {
    ws(
        e,
        socket,
        NOW + 12.0,
        &format!(
            r#"{{"topic":"ws_zk_accounts_v3","type":"delta","contents":{{"fills":[{{"id":"new-fill","orderId":"parent","symbol":"BTC-USDT","side":"BUY","size":"{size}","price":"60000","fee":"0","createdAt":{created_at}}}]}}}}"#,
        ),
    );
}

#[test]
fn partial_parent_and_virtual_amendments_replace_only_remaining_and_keep_siblings() {
    for id in ["parent", "parent__take_profit"] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, id, None).remove(0);
        response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
        let replacement = requests(&mut e).remove(0);
        let fields = form(&replacement);
        assert_eq!(fields["size"], "4");
        assert_eq!(fields["slSize"], "4");
        assert_eq!(fields["tpSize"], "4");
        assert_eq!(fields["slTriggerPrice"], "50000");
        assert_eq!(
            fields["tpTriggerPrice"],
            if id == "parent" { "65000" } else { "59000" }
        );
    }
}

#[test]
fn amend_rechecks_remaining_when_its_fifo_turn_starts() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    assert!(amend(&mut e, "parent", None).is_empty());
    parent_update(&mut e, socket, NOW + 11.0, 8.0, "OPEN", false);
    e.handle(
        NOW + 12.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost placement response",
        },
    );
    let cancel = requests(&mut e).remove(0);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2");
}

#[test]
fn in_flight_amend_counts_same_execution_once_in_both_topic_orders() {
    for fill_first in [true, false] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, "parent", None).remove(0);
        if fill_first {
            amend_fill(&mut e, socket, NOW + 11.0, 1.0);
        }
        parent_update(&mut e, socket, NOW + 12.0, 7.0, "OPEN", false);
        if !fill_first {
            amend_fill(&mut e, socket, NOW + 11.0, 1.0);
        }
        response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
        assert_eq!(form(&requests(&mut e).remove(0))["size"], "3");
    }
}

#[test]
fn delayed_pre_amend_fill_does_not_reduce_the_captured_remaining_again() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    amend_fill(&mut e, socket, NOW + 4.0, 6.0);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "4");
}

#[test]
fn new_genuine_amend_fill_counts_despite_outstanding_old_synthetic_coverage() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    parent_update(&mut e, socket, NOW + 6.0, 7.0, "OPEN", false);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    amend_fill(&mut e, socket, NOW + 11.0, 1.0);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2");
}

#[test]
fn distinct_execution_after_a_cumulative_report_reduces_remaining_again() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", None).remove(0);
    parent_update(&mut e, socket, NOW + 11.0, 7.0, "OPEN", false);
    amend_fill(&mut e, socket, NOW + 12.0, 1.0);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2");
}

#[test]
fn genuine_full_fill_during_cancel_suppresses_explicit_replacement_quantity() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", Some(2.5)).remove(0);
    amend_fill(&mut e, socket, NOW + 11.0, 4.0);
    e.drain_actions().for_each(drop);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 99, result: Ok(ids) }) if ids == "[]")));
    assert!(http(actions).is_empty());
}

#[test]
fn fully_filled_parent_while_amend_is_queued_never_sends_cancellation() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let first = http(place(&mut e, vec![order(serde_json::json!({}))])).remove(0);
    assert!(amend(&mut e, "parent", None).is_empty());
    parent_update(&mut e, socket, NOW + 11.0, 10.0, "FILLED", false);
    e.handle(
        NOW + 12.0,
        Event::HttpFailed {
            request: first.id,
            error: "lost placement response",
        },
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone {
            command: 99,
            result: Err(_)
        })
    )));
    assert!(http(actions).is_empty());
}

#[test]
fn unchanged_or_invalid_amendment_rejects_before_cancellation() {
    for price in [None, Some(f64::NAN), Some(f64::INFINITY), Some(0.0)] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        e.handle(
            NOW + 10.0,
            Event::Command(Command::AmendOrder {
                command: 99,
                id: "parent".into(),
                price,
                amount: None,
            }),
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone {
                command: 99,
                result: Err(_)
            })
        )));
        assert!(http(actions).is_empty());
        assert_eq!(e.store().order("parent").unwrap().remaining, 4.0);
    }
}

#[test]
fn own_cancel_event_keeps_replacement_but_full_fill_suppresses_it() {
    for (status, filled) in [("CANCELED", 8.0), ("FILLED", 10.0)] {
        let (mut e, socket) = ready_with_socket();
        parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
        e.drain_actions().for_each(drop);
        let cancel = amend(&mut e, "parent", None).remove(0);
        parent_update(&mut e, socket, NOW + 12.0, filled, status, false);
        e.drain_actions().for_each(drop);
        response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
        let actions = e.drain_actions().collect::<Vec<_>>();
        if status == "FILLED" {
            assert!(actions.iter().any(|action| matches!(action, Action::Notice(Notice::CommandDone { command: 99, result: Ok(ids) }) if ids == "[]")));
            assert!(http(actions).is_empty());
        } else {
            assert_eq!(form(&http(actions).remove(0))["size"], "2");
        }
    }
}

#[test]
fn explicit_amend_quantity_remains_verbatim_after_an_additional_fill() {
    let (mut e, socket) = ready_with_socket();
    parent_update(&mut e, socket, NOW + 5.0, 6.0, "OPEN", true);
    e.drain_actions().for_each(drop);
    let cancel = amend(&mut e, "parent", Some(2.5)).remove(0);
    parent_update(&mut e, socket, NOW + 12.0, 7.0, "OPEN", false);
    response(&mut e, &cancel, NOW + 15.0, r#"{"code":0,"data":null}"#);
    assert_eq!(form(&requests(&mut e).remove(0))["size"], "2.5");
}

#[test]
fn unsupported_stp_rejects_single_and_mixed_batches_before_io_and_keeps_queue_usable() {
    for mode in ["EXPIRE_TAKER", "EXPIRE_MAKER", "EXPIRE_BOTH"] {
        for mixed in [false, true] {
            let mut engine = ready();
            let before_balance = engine.store().balance().clone();
            let mut orders = Vec::new();
            if mixed {
                orders.push(order(serde_json::json!({})));
            }
            orders.push(order(serde_json::json!({"stpMode": mode})));
            let actions = place(&mut engine, orders);
            assert!(
                actions
                    .iter()
                    .all(|action| matches!(action, Action::Notice(_)))
            );
            assert!(actions.iter().any(|action| matches!(action,
                Action::Notice(Notice::CommandDone { command: 42, result: Err(error) })
                if error.contains("self-trade prevention"))));
            assert!(engine.store().orders().is_empty());
            assert!(engine.store().positions().is_empty());
            assert_eq!(engine.store().balance(), &before_balance);
            assert_eq!(
                http(place(&mut engine, vec![order(serde_json::json!({}))])).len(),
                1
            );
        }
    }
}
