//! Original native controls: real Bingx, trusted provider, shared host driver.
use safe_cex_engine::model::{Market, MinMax, PlaceOrderOpts, Precision};
use safe_cex_engine::request_context::HttpAction;
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome,
    ReadCredentials, Response, VenueError,
};
use safe_cex_venue_bingx::{Bingx, paths};
use serde_json::{Value, json};
#[allow(dead_code)]
#[path = "support/oracle.rs"]
mod oracle;
mod support;

const COMMAND: u32 = 71;
const START: f64 = 1_000.;
const ORDER: &str = "/openApi/swap/v2/trade/order";

fn engine() -> Engine<Bingx> {
    let mut engine = support::engine(support::Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: Some(ReadCredentials {
            key: "secondary-key".into(),
            secret: "secondary-secret".into(),
            passphrase: None,
        }),
        ..Default::default()
    });
    engine.store_mut().set_markets(vec![Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: safe_cex_engine::model::Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    engine
}
fn orders(count: usize) -> Vec<PlaceOrderOpts> {
    (0..count)
        .map(|index| {
            serde_json::from_value(json!({
                "symbol": "BTCUSDT", "type": "limit", "side": "buy",
                "amount": 0.01, "price": 69_000., "clientOrderId": format!("plan-{index}")
            }))
            .unwrap()
        })
        .collect()
}
fn place(engine: &mut Engine<Bingx>, count: usize) {
    engine.handle(
        START,
        Event::Command(Command::PlaceOrders {
            command: COMMAND,
            orders: orders(count),
        }),
    );
}
fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request)
                if request.method == Method::Post && request.url.ends_with(ORDER) =>
            {
                Some(request.clone())
            }
            _ => None,
        })
        .collect()
}
fn packet(actions: &[Action]) -> HttpRequest {
    let mut writes = writes(actions);
    assert_eq!(writes.len(), 1, "one native order at a time");
    writes.pop().unwrap()
}
fn collect(engine: &mut Engine<Bingx>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
fn body(request: &HttpRequest) -> Value {
    serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
}
fn respond(
    engine: &mut Engine<Bingx>,
    request: &HttpRequest,
    now: f64,
    status: u16,
    body: &str,
) -> Vec<Action> {
    engine.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    collect(engine, now)
}
fn acknowledge(
    engine: &mut Engine<Bingx>,
    request: &HttpRequest,
    now: f64,
    native: u64,
) -> Vec<Action> {
    respond(
        engine,
        request,
        now,
        200,
        &json!({
            "code": 0, "msg": "", "data": { "orderId": native.to_string() }
        })
        .to_string(),
    )
}
fn signature_refusal(engine: &mut Engine<Bingx>, request: &HttpRequest, now: f64) -> Vec<Action> {
    respond(
        engine,
        request,
        now,
        200,
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
    )
}
fn failures(actions: &[Action]) -> Vec<&VenueError> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect()
}
fn ids(actions: &[Action]) -> Vec<String> {
    let result = actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(result),
            _ => None,
        })
        .expect("one native command result");
    serde_json::from_str(result).unwrap()
}
fn without_physical_signature(mut body: Value) -> Value {
    let object = body.as_object_mut().unwrap();
    object.remove("timestamp");
    object.remove("signature");
    body
}

#[test]
fn ordinary_write_requires_managed_preparation_before_physical_http() {
    let mut engine = engine();
    place(&mut engine, 1);
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
                Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Prepare { .. })
            ))
            .count(),
        1,
        "ordinary write bypassed managed Prepare/Bind/Claim"
    );
    assert!(
        writes(&actions).is_empty(),
        "nothing releases before preparation"
    );
}

#[test]
fn eight_serial_calls_keep_each_native_twenty_second_attempt() {
    let mut engine = engine();
    place(&mut engine, 8);
    let mut now = START;
    let mut actions = collect(&mut engine, now);
    for index in 0..8 {
        let request = packet(&actions);
        assert_eq!(
            request.timeout_ms,
            Some(20_000),
            "call {index} physical window"
        );
        now += 19_000.;
        actions = acknowledge(&mut engine, &request, now, 9_000 + index);
    }
    assert_eq!(now - START, 152_000.);
    assert_eq!(
        ids(&actions),
        (9_000..9_008).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn each_proven_signature_resend_has_a_fresh_window_within_the_complete_plan() {
    let mut engine = engine();
    place(&mut engine, 8);
    let mut now = START;
    let mut actions = collect(&mut engine, now);
    for index in 0..8 {
        let first = packet(&actions);
        now += 19_000.;
        actions = signature_refusal(&mut engine, &first, now);
        let second = packet(&actions);
        assert_eq!(second.timeout_ms, Some(20_000), "fresh attempt {index}");
        assert_eq!(
            without_physical_signature(body(&first)),
            without_physical_signature(body(&second))
        );
        assert_ne!(body(&first)["timestamp"], body(&second)["timestamp"]);
        now += 19_000.;
        actions = acknowledge(&mut engine, &second, now, 9_000 + index);
    }
    assert_eq!(now - START, 304_000.);
    assert!(now - START < 320_000.);
    assert_eq!(
        ids(&actions),
        (9_000..9_008).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn released_first_attempt_expires_unknown_without_a_generic_retry() {
    let mut engine = engine();
    place(&mut engine, 1);
    let actions = collect(&mut engine, START);
    let request = packet(&actions);
    let mut expired = Vec::new();
    for action in &actions {
        if let Action::SetTimer { timer, at_ms } = action
            && *at_ms <= START + 20_000.
        {
            engine.handle(START + 20_000., Event::TimerFired { timer: *timer });
            expired.extend(collect(&mut engine, START + 20_000.));
        }
    }
    let errors = failures(&expired);
    assert_eq!(
        errors.len(),
        1,
        "released write must have an engine deadline"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert!(writes(&expired).is_empty());
    let late = acknowledge(&mut engine, &request, START + 20_001., 9_000);
    assert!(failures(&late).is_empty());
    assert!(!late.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone {
            command: COMMAND,
            ..
        })
    )));
}

fn assert_uncertain_response_never_resends(status: u16, body: &str) {
    let mut engine = engine();
    place(&mut engine, 1);
    let first = packet(&collect(&mut engine, START));
    let actions = respond(&mut engine, &first, START + 1., status, body);
    assert!(
        writes(&actions).is_empty(),
        "unproven or text-only response resent the mutation"
    );
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
}
#[test]
fn gateway_100001_does_not_grant_a_write_resend() {
    assert_uncertain_response_never_resends(
        503,
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
    );
}
#[test]
fn timeout_status_100001_does_not_grant_a_write_resend() {
    assert_uncertain_response_never_resends(
        408,
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
    );
}
#[test]
fn signature_text_without_a_refusal_code_does_not_grant_a_write_resend() {
    assert_uncertain_response_never_resends(
        200,
        r#"{"code":80001,"msg":"Signature verification failed after request processing"}"#,
    );
}

#[test]
fn compatibility_primary_recipe_matches_an_independent_oracle_with_secondary_configured() {
    let mut engine = engine();
    place(&mut engine, 1);
    let request = packet(&collect(&mut engine, START));
    let actual = body(&request);
    let expected = oracle::Params::new()
        .str("symbol", "BTC-USDT")
        .str("side", "BUY")
        .str("positionSide", "BOTH")
        .str("type", "LIMIT")
        .num("price", 69_000.)
        .str("timeInForce", "GTC")
        .num("quantity", 0.01)
        .str("clientOrderId", "plan-0")
        .into_request(
            Method::Post,
            ORDER,
            Some(&oracle::Credentials {
                read: None,
                key: "primary-key".into(),
                secret: "primary-secret".into(),
            }),
            actual["timestamp"].as_f64().unwrap(),
        );
    assert_eq!((request.url, request.headers, request.body), expected);
}

#[test]
fn compatibility_one_signature_resend_preserves_identity_and_never_has_a_third_attempt() {
    let mut engine = engine();
    place(&mut engine, 1);
    let first = packet(&collect(&mut engine, START));
    let actions = signature_refusal(&mut engine, &first, START + 1.);
    let second = packet(&actions);
    assert_eq!(
        without_physical_signature(body(&first)),
        without_physical_signature(body(&second))
    );
    assert_eq!(body(&second)["clientOrderId"], "plan-0");
    assert_ne!(body(&first)["timestamp"], body(&second)["timestamp"]);
    let actions = signature_refusal(&mut engine, &second, START + 2.);
    assert!(writes(&actions).is_empty());
    assert_eq!(failures(&actions).len(), 1);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Rejected));
}

#[test]
fn compatibility_transport_loss_is_never_resent() {
    let mut engine = engine();
    place(&mut engine, 1);
    let first = packet(&collect(&mut engine, START));
    engine.handle(
        START + 1.,
        Event::HttpFailed {
            request: first.id,
            error: "transport lost",
        },
    );
    let actions = collect(&mut engine, START + 1.);
    assert!(writes(&actions).is_empty());
    assert_eq!(failures(&actions).len(), 1);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));
}

fn fail_second_after_prefix(uncertain: bool) -> Vec<Action> {
    let mut engine = engine();
    place(&mut engine, 2);
    let first = packet(&collect(&mut engine, START));
    let actions = acknowledge(&mut engine, &first, START + 1., 9_000);
    let second = packet(&actions);
    if uncertain {
        engine.handle(
            START + 2.,
            Event::HttpFailed {
                request: second.id,
                error: "transport lost",
            },
        );
        collect(&mut engine, START + 2.)
    } else {
        respond(
            &mut engine,
            &second,
            START + 2.,
            200,
            r#"{"code":80014,"msg":"Invalid parameter"}"#,
        )
    }
}
#[test]
fn compatibility_accepted_prefix_survives_a_later_uncertain_write() {
    let actions = fail_second_after_prefix(true);
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert_eq!(
        errors[0]
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.clone()),
        Some(vec!["9000".into()])
    );
    assert!(writes(&actions).is_empty());
}
#[test]
fn compatibility_accepted_prefix_is_known_partial_after_a_proven_refusal() {
    let actions = fail_second_after_prefix(false);
    let errors = failures(&actions);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, None);
    assert_eq!(
        errors[0]
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.clone()),
        Some(vec!["9000".into()])
    );
}
#[test]
fn compatibility_invalid_later_order_refuses_before_the_first_write() {
    let mut engine = engine();
    let mut orders = orders(2);
    orders[1].amount = f64::NAN;
    engine.handle(
        START,
        Event::Command(Command::PlaceOrders {
            command: COMMAND,
            orders,
        }),
    );
    let actions = collect(&mut engine, START);
    assert!(writes(&actions).is_empty());
    assert_eq!(failures(&actions).len(), 1);
}

#[test]
fn compatibility_eight_serial_native_successes_keep_all_ids_in_order() {
    let mut engine = engine();
    place(&mut engine, 8);
    let mut actions = collect(&mut engine, START);
    for index in 0..8 {
        let request = packet(&actions);
        actions = acknowledge(
            &mut engine,
            &request,
            START + (index + 1) as f64 * 19_000.,
            9_000 + index,
        );
    }
    assert_eq!(
        ids(&actions),
        (9_000..9_008).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

fn open_order(id: &str) -> safe_cex_engine::model::Order {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 69000.,
        amount: 0.01,
        filled: 0.,
        remaining: 0.01,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn cancel_packet(actions: &[Action]) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.method == Method::Delete && r.url.contains(ORDER) => {
                Some(r.clone())
            }
            _ => None,
        })
        .expect("one independent cancel")
}
fn order_reads(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method == Method::Get && r.url.contains("/trade/openOrders") => {
                Some(r.clone())
            }
            _ => None,
        })
        .collect()
}
#[test]
fn absent_cancel_keeps_the_row_until_a_fresh_managed_reconciliation_read() {
    let mut e = engine();
    e.store_mut().set_orders(vec![open_order("old")]);
    e.handle(
        START,
        Event::Command(Command::CancelOrders {
            command: COMMAND,
            ids: vec!["old".into()],
        }),
    );
    let r = cancel_packet(&collect(&mut e, START));
    let actions = respond(
        &mut e,
        &r,
        START + 1.,
        200,
        r#"{"code":80016,"msg":"order not exist"}"#,
    );
    assert_eq!(
        e.store().order("old").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open,
        "Satisfied(Cancel) must not publish a cancelled row"
    );
    let reads = order_reads(&actions);
    assert_eq!(
        reads.len(),
        1,
        "absent cancel requires one fresh-owner reconciliation"
    );
    assert_eq!(reads[0].timeout_ms, Some(20_000));
    assert_eq!(ids(&actions), Vec::<String>::new());
    let done = respond(
        &mut e,
        &reads[0],
        START + 2.,
        200,
        r#"{"code":0,"data":{"orders":[]}}"#,
    );
    assert!(e.store().orders().is_empty());
    assert!(failures(&done).is_empty());
}
#[test]
fn independent_cancel_sibling_survives_a_delivered_gateway_failure_without_retry() {
    let mut e = engine();
    e.store_mut()
        .set_orders(vec![open_order("old-a"), open_order("old-b")]);
    e.handle(
        START,
        Event::Command(Command::CancelOrders {
            command: COMMAND,
            ids: vec!["old-a".into(), "old-b".into()],
        }),
    );
    let first = cancel_packet(&collect(&mut e, START));
    let next = respond(
        &mut e,
        &first,
        START + 1.,
        503,
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
    );
    let second = cancel_packet(&next);
    assert_ne!(second.id, first.id);
    assert!(second.url.contains("orderId=old-b"));
    assert!(failures(&next).is_empty());
    let done = respond(
        &mut e,
        &second,
        START + 2.,
        200,
        r#"{"code":0,"data":{"orderId":"old-b","status":"CANCELED"}}"#,
    );
    assert_eq!(failures(&done).len(), 1);
    assert_eq!(failures(&done)[0].outcome, Some(Outcome::Unknown));
    assert_eq!(
        e.store().order("old-a").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
    assert_eq!(
        e.store().order("old-b").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}
#[test]
fn refused_envelope_with_an_id_is_unproven_evidence_and_never_a_clean_refusal() {
    let mut e = engine();
    place(&mut e, 1);
    let first = packet(&collect(&mut e, START));
    let done = respond(
        &mut e,
        &first,
        START + 1.,
        200,
        r#"{"code":80014,"msg":"Invalid parameter","data":{"orderId":"possible-effect"}}"#,
    );
    let errors = failures(&done);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(Outcome::Unknown));
    assert_eq!(
        errors[0].partial.as_ref().unwrap().order_ids,
        ["possible-effect"]
    );
}

#[test]
fn tuple_shaped_acknowledgements_never_prove_a_placement() {
    for body in [
        r#"[0,"",{"orderId":"tuple"}]"#,
        r#"{"code":0,"data":["tuple"]}"#,
        r#"{"code":0,"data":{"order":["tuple"]}}"#,
    ] {
        assert_uncertain_response_never_resends(200, body);
    }
}

#[test]
fn forced_reconciliation_never_reuses_or_is_cleared_by_an_older_snapshot() {
    for old_first in [true, false] {
        for old_failed in [true, false] {
            let mut e = engine();
            e.store_mut()
                .set_orders(vec![open_order("old-a"), open_order("old-b")]);
            e.handle(
                START,
                Event::Command(Command::CancelOrders {
                    command: COMMAND,
                    ids: vec!["old-a".into()],
                }),
            );
            let first = cancel_packet(&collect(&mut e, START));
            let actions = respond(
                &mut e,
                &first,
                START + 1.,
                200,
                r#"{"code":80016,"msg":"order not exist"}"#,
            );
            let old = order_reads(&actions).remove(0);
            e.handle(
                START + 2.,
                Event::Command(Command::CancelOrders {
                    command: COMMAND + 1,
                    ids: vec!["old-b".into()],
                }),
            );
            let second = cancel_packet(&collect(&mut e, START + 2.));
            let actions = respond(
                &mut e,
                &second,
                START + 3.,
                200,
                r#"{"code":80016,"msg":"order not exist"}"#,
            );
            let fresh = order_reads(&actions);
            assert_eq!(
                fresh.len(),
                1,
                "an in-flight old read must not suppress a fresh owner"
            );
            assert_ne!(fresh[0].id, old.id);
            let complete_old = |e: &mut Engine<Bingx>, now: f64| {
                if old_failed {
                    e.handle(
                        now,
                        Event::HttpFailed {
                            request: old.id,
                            error: "old read lost",
                        },
                    );
                    collect(e, now)
                } else {
                    respond(
                        e,
                        &old,
                        now,
                        200,
                        r#"{"code":0,"data":{"orders":[{"orderId":"old-a","symbol":"BTC-USDT","type":"LIMIT","side":"BUY","status":"NEW","origQty":"0.01","executedQty":"0","price":"69000","positionSide":"BOTH","timeInForce":"GTC"}]}}"#,
                    )
                }
            };
            if old_first {
                let stale = complete_old(&mut e, START + 4.);
                assert!(
                    order_reads(&stale).is_empty(),
                    "stale completion must keep the newer reservation"
                );
            }
            let done = respond(
                &mut e,
                &fresh[0],
                START + 5.,
                200,
                r#"{"code":0,"data":{"orders":[]}}"#,
            );
            assert!(order_reads(&done).is_empty());
            assert!(e.store().orders().is_empty());
            if !old_first {
                let stale = complete_old(&mut e, START + 6.);
                assert!(order_reads(&stale).is_empty());
                assert!(
                    e.store().orders().is_empty(),
                    "old snapshot resurrected a cancelled/absent row"
                );
            }
        }
    }
}

#[test]
fn an_absent_code_with_a_failed_cancel_list_is_not_satisfied() {
    let mut e = engine();
    e.store_mut().set_orders(vec![open_order("old")]);
    e.handle(
        START,
        Event::Command(Command::CancelOrders {
            command: COMMAND,
            ids: vec!["old".into()],
        }),
    );
    let r = cancel_packet(&collect(&mut e, START));
    let actions = respond(
        &mut e,
        &r,
        START + 1.,
        200,
        r#"{"code":80016,"msg":"order not exist","data":{"failed":["old"]}}"#,
    );
    assert_eq!(failures(&actions).len(), 1);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));
    assert_eq!(
        e.store().order("old").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn deeply_nested_object_acknowledgements_keep_the_parser_depth_limit() {
    let mut e = engine();
    place(&mut e, 1);
    let r = packet(&collect(&mut e, START));
    let nested = format!(
        "{}{}{}",
        "{\"order\":".repeat(140),
        r#"{"orderId":"too-deep"}"#,
        "}".repeat(140)
    );
    let body = format!(r#"{{"code":0,"data":{nested}}}"#);
    let actions = respond(&mut e, &r, START + 1., 200, &body);
    assert_eq!(failures(&actions).len(), 1);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));
    assert!(writes(&actions).is_empty());
}

#[test]
fn native_ack_state_cannot_be_used_as_a_signature_refusal_or_absence_proof() {
    let mut e = engine();
    place(&mut e, 1);
    let r = packet(&collect(&mut e, START));
    let actions = respond(
        &mut e,
        &r,
        START + 1.,
        200,
        r#"{"code":100001,"msg":"Signature verification failed","data":{"status":"NEW"}}"#,
    );
    assert!(
        writes(&actions).is_empty(),
        "acknowledged state forbids resend"
    );
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));

    let mut e = engine();
    e.store_mut().set_orders(vec![open_order("old")]);
    e.handle(
        START,
        Event::Command(Command::CancelOrders {
            command: COMMAND,
            ids: vec!["old".into()],
        }),
    );
    let r = cancel_packet(&collect(&mut e, START));
    let actions = respond(
        &mut e,
        &r,
        START + 1.,
        200,
        r#"{"code":80016,"msg":"order not exist","data":{"status":"NEW"}}"#,
    );
    assert_eq!(failures(&actions).len(), 1);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));
}

fn api_key(request: &HttpRequest) -> &str {
    request
        .headers
        .iter()
        .find(|(name, _)| *name == "X-BX-APIKEY")
        .map(|(_, key)| key.as_ref())
        .expect("signed read")
}
fn cancel_all(engine: &mut Engine<Bingx>, command: u32, now: f64) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(Command::CancelSymbolOrders {
            command,
            symbol: "BTCUSDT".into(),
        }),
    );
    collect(engine, now)
}
fn cancel_all_sent(actions: &[Action]) -> bool {
    actions.iter().any(|a| {
        matches!(a, Action::Http(r) if r.method == Method::Delete && r.url.contains("/trade/allOpenOrders"))
    })
}
fn fenced(actions: &[Action], command: u32) -> bool {
    actions.iter().any(|a| {
        matches!(a, Action::Notice(Notice::CommandFailed { command: id, error })
            if *id == command
                && error.message == "Placement outcome is uncertain; reconcile before cancel-all")
    })
}
/// The clearing snapshot is Primary-signed even with a read key configured, and
/// cancel-all is refused until it commits.
fn assert_fence_clears_only_on_the_primary_snapshot(
    engine: &mut Engine<Bingx>,
    actions: &[Action],
) {
    let reads = order_reads(actions);
    assert_eq!(reads.len(), 1, "one fresh reconciliation snapshot");
    assert_eq!(api_key(&reads[0]), "primary-key", "PostUnknownOrderRead");
    let refused = cancel_all(engine, COMMAND + 1, START + 3.);
    assert!(fenced(&refused, COMMAND + 1));
    assert!(!cancel_all_sent(&refused));
    respond(
        engine,
        &reads[0],
        START + 4.,
        200,
        r#"{"code":0,"data":{"orders":[]}}"#,
    );
    let admitted = cancel_all(engine, COMMAND + 2, START + 5.);
    assert!(!fenced(&admitted, COMMAND + 2));
    assert!(
        cancel_all_sent(&admitted),
        "a clean snapshot clears the fence"
    );
}

/// The beta ruling keeps cancel-all fenced: an Unknown create may land after
/// the native DELETE, and BingX has no automatic late-landing cancellation.
#[test]
fn lost_create_fences_cancel_all_until_the_fresh_primary_read() {
    let mut e = engine();
    place(&mut e, 1);
    let first = packet(&collect(&mut e, START));
    e.handle(
        START + 1.,
        Event::HttpFailed {
            request: first.id,
            error: "transport lost",
        },
    );
    let actions = collect(&mut e, START + 1.);
    assert_eq!(failures(&actions)[0].outcome, Some(Outcome::Unknown));
    let reads = order_reads(&actions);
    assert_eq!(reads.len(), 1, "one fresh reconciliation snapshot");
    assert_eq!(api_key(&reads[0]), "primary-key", "PostUnknownOrderRead");
    assert_fence_clears_only_on_the_primary_snapshot(&mut e, &actions);
}

#[test]
fn absent_cancel_fences_cancel_all_until_the_primary_signed_snapshot() {
    let mut e = engine();
    e.store_mut().set_orders(vec![open_order("old")]);
    e.handle(
        START,
        Event::Command(Command::CancelOrders {
            command: COMMAND,
            ids: vec!["old".into()],
        }),
    );
    let r = cancel_packet(&collect(&mut e, START));
    let actions = respond(
        &mut e,
        &r,
        START + 1.,
        200,
        r#"{"code":80016,"msg":"order not exist"}"#,
    );
    assert_fence_clears_only_on_the_primary_snapshot(&mut e, &actions);
}

#[test]
fn an_order_read_epoch_cannot_clear_a_later_concurrent_unknown_write() {
    let mut e = engine();
    place(&mut e, 1);
    let first = packet(&collect(&mut e, START));
    let mut second_orders = orders(1);
    second_orders[0].client_order_id = Some("epoch-second".into());
    e.handle(
        START + 0.5,
        Event::Command(Command::PlaceOrders {
            command: COMMAND + 1,
            orders: second_orders,
        }),
    );
    let second = packet(&collect(&mut e, START + 0.5));
    e.handle(
        START + 1.,
        Event::HttpFailed {
            request: first.id,
            error: "first unknown",
        },
    );
    let after_first = collect(&mut e, START + 1.);
    let old = order_reads(&after_first);
    assert_eq!(old.len(), 1);
    e.handle(
        START + 2.,
        Event::HttpFailed {
            request: second.id,
            error: "later unknown",
        },
    );
    let after_second = collect(&mut e, START + 2.);
    let fresh = order_reads(&after_second);
    assert_eq!(fresh.len(), 1);
    assert_ne!(old[0].id, fresh[0].id);
    assert_eq!(api_key(&old[0]), "primary-key");
    assert_eq!(api_key(&fresh[0]), "primary-key");
    respond(
        &mut e,
        &old[0],
        START + 3.,
        200,
        r#"{"code":0,"data":{"orders":[]}}"#,
    );
    let refused = cancel_all(&mut e, COMMAND + 2, START + 4.);
    assert!(fenced(&refused, COMMAND + 2));
    assert!(!cancel_all_sent(&refused));
    respond(
        &mut e,
        &fresh[0],
        START + 5.,
        200,
        r#"{"code":0,"data":{"orders":[]}}"#,
    );
    let admitted = cancel_all(&mut e, COMMAND + 3, START + 6.);
    assert!(!fenced(&admitted, COMMAND + 3));
    assert!(cancel_all_sent(&admitted));
}
