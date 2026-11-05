//! Real native account/signing setup and captured HTTP/timer bindings; default CI target.
mod support;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};
use serde_json::Value;
use support::*;

const NOW: f64 = 2000.;
const OWNER: u32 = 90;
const ACCOUNT: &str = "0x14791697260e4c9a71f18484c9f997b308e59325";
const CANCEL_ACK: &str =
    r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#;
const PLACE_ACK: &str =
    r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#;

fn boot(approved: bool) -> (Engine<Hyperliquid>, SocketId, Vec<HttpRequest>) {
    let mut e = private_engine(
        Options {
            application_id: Some(ACCOUNT.into()),
            ..Options::default()
        },
        TEST_SECRET.into(),
        false,
    );
    e.start(1000.);
    let mut socket = None;
    let mut held = Vec::new();
    for _ in 0..30 {
        let actions = prepared_now(&mut e);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    if !approved && kind(&r) == "maxBuilderFee" {
                        held.push(r);
                        continue;
                    }
                    let response = match kind(&r).as_str() {
                        "metaAndAssetCtxs" => META,
                        "perpDexs" => "[null]",
                        "userAbstraction" => "\"disabled\"",
                        "clearinghouseState" => STATE,
                        "frontendOpenOrders" => "[]",
                        "userFees" => r#"{"userCrossRate":"0.00045","userAddRate":"0.00015"}"#,
                        "maxBuilderFee" => "30",
                        other => panic!("unexpected startup read {other}"),
                    };
                    reply(&mut e, &r, 200, response);
                }
                Action::WsConnect { socket: id, .. } => {
                    socket = Some(id);
                    e.handle(1000., Event::WsOpened { socket: id });
                }
                _ => {}
            }
        }
    }
    assert!(e.store().loaded().markets && e.store().loaded().orders);
    let socket = socket.expect("real managed private socket");
    message(
        &mut e,
        socket,
        1500.,
        r#"{"channel":"orderUpdates","data":[{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"120","triggerPx":"120","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Take Profit Market","isPositionTpsl":true},"status":"open"}]}"#,
    );
    fresh_mark(&mut e, socket, NOW);
    e.drain_actions().for_each(drop);
    assert_eq!(e.store().order("9").unwrap().kind, OrderType::TakeProfit);
    (e, socket, held)
}

fn begin_amend(e: &mut Engine<Hyperliquid>) -> Vec<Action> {
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: OWNER,
            id: "9".into(),
            price: Some(75.),
            amount: None,
        }),
    );
    prepared_now(e)
}
fn cancel_protection(approved: bool) -> (Engine<Hyperliquid>, HttpRequest, Vec<HttpRequest>) {
    let (mut e, _, held) = boot(approved);
    let actions = begin_amend(&mut e);
    let reads: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(reads.len(), 1, "one fresh sizing read before any cancel");
    assert_eq!(kind(reads[0]), "orderStatus");
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
    answer_order_status(&mut e, reads[0]);
    let actions = prepared_now(&mut e);
    let cancel = actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("cancel after fresh sizing");
    assert_eq!(body(&cancel)["action"]["type"], "cancel");
    (e, cancel, held)
}
fn body(request: &HttpRequest) -> Value {
    serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
}
fn replacement(e: &mut Engine<Hyperliquid>, cancel: &HttpRequest) -> HttpRequest {
    reply(e, cancel, 200, CANCEL_ACK);
    let actions = prepared_now(e);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/info"))),
        "no optional approval read after removing protection"
    );
    assert!(!actions.iter().any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms >= NOW + 8000. && *at_ms <= NOW + 8001.)), "no optional eight-second fee wait after removing protection");
    let actions = advance_packet_timers(e, actions, NOW + 334.);
    let writes: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes.len(),
        1,
        "normal 334ms mutation pacing releases replacement"
    );
    writes.into_iter().next().unwrap()
}

#[test]
fn unknown_builder_uses_the_fallback_without_waiting_after_cancel() {
    let (mut e, cancel, held) = cancel_protection(false);
    assert_eq!(held.len(), 1, "account startup already warms approval");
    let order = replacement(&mut e, &cancel);
    assert!(body(&order)["action"].get("builder").is_none());
    assert_eq!(
        body(&order)["action"]["orders"][0]["t"]["trigger"]["triggerPx"],
        "75"
    );
    reply(&mut e, &order, 200, PLACE_ACK);
    let terminal: Vec<_> = prepared_now(&mut e)
        .into_iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandDone { command: OWNER, .. })
                    | Action::Notice(Notice::CommandFailed { command: OWNER, .. })
            )
        })
        .collect();
    assert_eq!(terminal.len(), 1);
    assert!(matches!(
        terminal[0],
        Action::Notice(Notice::CommandDone { .. })
    ));
}

#[test]
fn a_late_approval_cannot_change_this_amend_but_stays_cached_for_the_next_order() {
    let (mut e, cancel, held) = cancel_protection(false);
    reply(&mut e, &held[0], 200, "30");
    prepared_now(&mut e);
    let order = replacement(&mut e, &cancel);
    assert!(
        body(&order)["action"].get("builder").is_none(),
        "no fee may be added after the pre-cancel decision"
    );
    reply(&mut e, &order, 200, PLACE_ACK);
    prepared_now(&mut e);
    e.handle(
        NOW + 1000.,
        Event::Command(Command::PlaceOrders {
            command: OWNER + 1,
            orders: vec![
                serde_json::from_str(
                    r#"{"symbol":"BTC-PERP","type":"limit","side":"buy","price":90,"amount":1}"#,
                )
                .unwrap(),
            ],
        }),
    );
    let actions = prepared_now(&mut e);
    let next = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("future placement uses the cached approval without a lookup");
    assert_eq!(body(next)["action"]["builder"]["f"], 10);
}

#[test]
fn cached_approval_is_used_without_another_read() {
    let (mut e, cancel, held) = cancel_protection(true);
    assert!(held.is_empty());
    let order = replacement(&mut e, &cancel);
    assert_eq!(body(&order)["action"]["builder"]["f"], 10);
    assert_eq!(
        body(&order)["action"]["builder"]["b"],
        "0xea629e1e8534b82f704a818643f2daf38cb7fd4d"
    );
}

#[test]
fn an_unanswered_sizing_read_refuses_before_cancel_within_five_seconds() {
    let (mut e, _, _) = boot(false);
    let actions = begin_amend(&mut e);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5000. => Some(*timer),
            _ => None,
        })
        .expect("bounded sizing read");
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    e.handle(NOW + 5000., Event::TimerFired { timer });
    let actions = prepared_now(&mut e);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
    );
    let terminal: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: OWNER,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(terminal.len(), 1);
    assert_eq!(terminal[0].outcome, Some(Outcome::NotSent));
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
}

#[test]
fn review_3098_sizing_response_at_4999ms_still_cancels() {
    review_sizing_receipt_deadline(4999.0, true);
}
#[test]
fn review_3098_sizing_response_after_5s_cannot_beat_the_due_timer() {
    review_sizing_receipt_deadline(5001.0, false);
}
fn review_sizing_receipt_deadline(delay: f64, may_cancel: bool) {
    let (mut e, _, _) = boot(false);
    let issued = begin_amend(&mut e);
    let read = issued
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(
        issued
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == NOW + 5000.))
    );
    e.handle(NOW + delay, Event::HttpResponse {
        request: read.id,
        response: safe_cex_engine::Response {
            status: 200, headers: "",
            body: r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"120","triggerPx":"120","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Take Profit Market","isPositionTpsl":true},"status":"open"}}"#,
        },
    });
    let actions = prepared_now(&mut e);
    let cancel = actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")));
    assert_eq!(
        cancel, may_cancel,
        "five-second pre-cancel deadline must hold regardless of callback ordering"
    );
    if !may_cancel {
        assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.outcome == Some(Outcome::NotSent))));
    }
}

#[test]
fn review_3098_unknown_fee_does_not_wait_for_any_amend_kind_or_partial_fill() {
    for (kind, old_price, new_price, native) in [
        (OrderType::Limit, 110., 115., "Limit"),
        (OrderType::StopLoss, 80., 75., "Stop Market"),
        (OrderType::TakeProfit, 120., 125., "Take Profit Market"),
    ] {
        let (mut e, socket, held) = boot(false);
        assert_eq!(held.len(), 1);
        message(
            &mut e,
            socket,
            1501.,
            &format!(
                r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"BTC","oid":9,"side":"A","limitPx":"{old_price}","triggerPx":"{old_price}","origSz":"2","sz":"1","reduceOnly":true,"orderType":"{native}"}},"status":"open"}}]}}"#
            ),
        );
        e.drain_actions().for_each(drop);
        assert_eq!(e.store().order("9").unwrap().kind, kind);
        assert_eq!(e.store().order("9").unwrap().filled, 1.);
        e.handle(
            NOW,
            Event::Command(Command::AmendOrder {
                command: OWNER,
                id: "9".into(),
                price: Some(new_price),
                amount: None,
            }),
        );
        let actions = prepared_now(&mut e);
        let status = actions
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if kind_of_read(r) => Some(r),
                _ => None,
            })
            .expect("one fresh partial sizing read");
        answer_order_status(&mut e, status);
        let actions = prepared_now(&mut e);
        let cancel = actions
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
                _ => None,
            })
            .expect("cancel after sizing");
        if kind == OrderType::Limit {
            let order = replacement(&mut e, &cancel);
            assert!(body(&order)["action"].get("builder").is_none(), "{kind:?}");
            assert_eq!(body(&order)["action"]["orders"][0]["s"], "1", "{kind:?}");
        } else {
            assert_eq!(body(&cancel)["action"]["type"], "modify");
            assert!(body(&cancel)["action"].get("builder").is_none(), "{kind:?}");
            assert_eq!(body(&cancel)["action"]["order"]["s"], "1", "{kind:?}");
        }
    }
}
fn kind_of_read(r: &HttpRequest) -> bool {
    kind(r) == "orderStatus"
}

#[test]
fn sizing_response_at_the_exclusive_deadline_keeps_the_protection() {
    review_sizing_receipt_deadline(5000., false);
}

#[test]
fn a_cancel_prepared_before_the_deadline_cannot_claim_after_it() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    for claim_at in [NOW + 5000., NOW + 5001.] {
        let (mut e, _, _) = boot(false);
        let issued = begin_amend(&mut e);
        let read = issued
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if kind(r) == "orderStatus" => Some(r),
                _ => None,
            })
            .unwrap();
        answer_order_status(&mut e, read);
        let (preparation, request) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.ends_with("/exchange") => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("cancel owns a genuine prepared route before deadline");
        e.http_prepared(
            NOW,
            preparation,
            Prepared {
                id: request.id.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .expect("signed cancel bound before deadline");
        assert!(
            e.claim_http(claim_at, permit).is_err(),
            "expired cancel must never be released"
        );
        let actions = prepared_now(&mut e);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.outcome == Some(Outcome::NotSent))));
    }
}

#[test]
fn a_cancel_released_in_time_still_replaces_after_the_pre_cancel_deadline() {
    let (mut e, cancel, _) = cancel_protection(false);
    e.handle(
        NOW + 5001.,
        Event::HttpResponse {
            request: cancel.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: CANCEL_ACK,
            },
        },
    );
    let actions = prepared_now(&mut e);
    assert!(actions.iter().any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange") && body(r)["action"]["type"] == "order")));
}

#[test]
fn review_3098_retired_sizing_reply_cannot_release_a_new_amend_for_same_order() {
    review_sizing_owner(false);
}
#[test]
fn review_3098_successors_own_sizing_reply_still_releases_cancel() {
    review_sizing_owner(true);
}
#[test]
fn retired_sizing_failures_cannot_abort_or_rearm_the_successor() {
    for status in [429, 500] {
        review_sizing_owner_response(false, status);
    }
}
fn review_sizing_owner(use_current: bool) {
    review_sizing_owner_response(use_current, 200);
}
fn review_sizing_owner_response(use_current: bool, status: u16) {
    let (mut e, socket, _) = boot(false);
    let actions = begin_amend(&mut e);
    let old = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 5000., Event::TimerFired { timer });
    let failed = prepared_now(&mut e);
    assert!(failed.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.outcome == Some(Outcome::NotSent))));
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
    fresh_mark(&mut e, socket, NOW + 5100.);
    e.handle(
        NOW + 5100.,
        Event::Command(Command::AmendOrder {
            command: OWNER + 1,
            id: "9".into(),
            price: Some(70.),
            amount: None,
        }),
    );
    let actions = prepared_now(&mut e);
    let current = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r.clone()),
            _ => None,
        })
        .expect("new command owns a distinct fresh sizing read");
    assert_ne!(old.id, current.id);
    let before = serde_json::to_value(e.store().order("9").unwrap()).unwrap();
    e.handle(7150., Event::TimerFired { timer });
    let stale_timer = prepared_now(&mut e);
    assert!(!stale_timer.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
    let response = if use_current { current.id } else { old.id };
    e.handle(NOW + 5200., Event::HttpResponse {
        request: response,
        response: safe_cex_engine::Response { status, headers: "",
            body: r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"120","triggerPx":"120","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Take Profit Market","isPositionTpsl":true},"status":"open"}}"#,
        },
    });
    let actions = prepared_now(&mut e);
    let canceled = actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")));
    assert_eq!(
        canceled, use_current,
        "a retired read must not borrow the successor command's deadline or authority"
    );
    if !use_current {
        assert_eq!(
            serde_json::to_value(e.store().order("9").unwrap()).unwrap(),
            before
        );
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { .. })
                | Action::Notice(Notice::CommandDone { .. })
        )));
        e.handle(7300., Event::HttpResponse {request: current.id, response: safe_cex_engine::Response {status:200, headers:"", body:r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"120","triggerPx":"120","origSz":"0","sz":"0","reduceOnly":true,"orderType":"Take Profit Market","isPositionTpsl":true},"status":"open"}}"#}});
        assert!(
            prepared_now(&mut e)
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
    }
}

#[test]
fn a_retired_sizing_reply_after_timeout_alone_does_not_change_the_order() {
    let (mut e, _, _) = boot(false);
    let actions = begin_amend(&mut e);
    let old = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .unwrap();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 5000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(7000., Event::TimerFired { timer });
    prepared_now(&mut e);
    let before = serde_json::to_value(e.store().order("9").unwrap()).unwrap();
    e.handle(7200., Event::HttpResponse {request: old.id, response: safe_cex_engine::Response {status:200, headers:"", body:r#"{"status":"order","order":{"order":{"coin":"BTC","oid":9,"side":"A","limitPx":"80","origSz":"100","sz":"3"},"status":"open"}}"#}});
    let actions = prepared_now(&mut e);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Notice(Notice::CommandFailed { .. })
            | Action::Notice(Notice::CommandDone { .. })
    )));
    assert_eq!(
        serde_json::to_value(e.store().order("9").unwrap()).unwrap(),
        before
    );
}

fn fresh_mark(e: &mut Engine<Hyperliquid>, socket: SocketId, now: f64) {
    use base64::Engine as _;
    let raw = br#"{"BTC":{"markPx":"100"}}"#;
    let zipped = miniz_oxide::deflate::compress_to_vec(raw, 6);
    let text = serde_json::json!({"channel":"fastAssetCtxs","data":base64::engine::general_purpose::STANDARD.encode(zipped)}).to_string();
    message(e, socket, now, &text);
}
