//! Structured write failures preserve transport/native provenance and batch effects.
mod support;
use safe_cex_engine::model::{OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn order(extra: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021{extra}}}"#)).unwrap()
}

fn classified_failure(e: &mut TestEngine, command: u32) -> safe_cex_engine::VenueError {
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect();
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command))
            .count(),
        1,
        "one failure completion: {actions:?}"
    );
    assert!(!actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)));
    actions
        .into_iter()
        .find_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
                Some(error)
            }
            _ => None,
        })
        .unwrap()
}
fn respond_status(
    e: &mut TestEngine,
    request: safe_cex_engine::RequestId,
    status: u16,
    body: &str,
) {
    e.handle(
        10.0,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

#[test]
fn write_failures_classify_http_before_body_and_keep_native_diagnostics() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, kind, outcome) in [
        (401, ErrorKind::AuthInvalid, Outcome::Rejected),
        (403, ErrorKind::AuthPermission, Outcome::Rejected),
        (429, ErrorKind::RateLimited, Outcome::Rejected),
        (400, ErrorKind::Rejected, Outcome::Rejected),
        (408, ErrorKind::Timeout, Outcome::Unknown),
        (503, ErrorKind::VenueUnavailable, Outcome::Unknown),
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 1001,
                orders: vec![order("")],
            }),
        );
        let request = requests(&mut e).remove(0);
        respond_status(
            &mut e,
            request.id,
            status,
            r#"{"code":"10007","msg":"Sign signature error"}"#,
        );
        let error = classified_failure(&mut e, 1001);
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.outcome, Some(outcome));
        // The status classifies a non-2xx answer; its body code is not evidence,
        // and the engine seals 408/5xx before the venue reads the body.
        let unproven = status == 408 || status >= 500;
        assert_eq!(
            error.message,
            if unproven {
                "HTTP response does not prove the write outcome"
            } else {
                "Sign signature error"
            }
        );
        assert_eq!(error.venue_code, None);
    }
}

#[test]
fn write_transport_and_malformed_success_are_unknown_but_preflight_is_not_sent() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let (mut e, _) = startup();
    let mut invalid = order("");
    invalid.amount = 0.0;
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1002,
            orders: vec![invalid],
        }),
    );
    let error = classified_failure(&mut e, 1002);
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    for body in [
        None,
        Some(r#"{"code":0,"data":{"orderId":[]}}"#),
        Some(r#"{"code":null,"data":{}}"#),
        Some(r#"{"code":0}"#),
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 1003,
                orders: vec![order("")],
            }),
        );
        let request = requests(&mut e).remove(0);
        if let Some(body) = body {
            respond_status(&mut e, request.id, 200, body);
        } else {
            e.handle(
                10.0,
                Event::HttpFailed {
                    request: request.id,
                    error: "response lost",
                },
            );
        }
        let error = classified_failure(&mut e, 1003);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(
            error.kind,
            Some(if body.is_none() {
                ErrorKind::Network
            } else {
                ErrorKind::Internal
            })
        );
        if body.is_none() {
            assert_eq!(error.message, "response lost");
        }
    }
}

#[test]
fn batch_item_failures_complete_once_with_partials_and_do_not_block_independent_steps() {
    use safe_cex_engine::ErrorKind;
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1004,
            orders: (0..6).map(|_| order("")).collect(),
        }),
    );
    let first = requests(&mut e).remove(0);
    reply(
        &mut e,
        first.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"accepted-first"},{"orderId":"accepted-2"},{"orderId":"accepted-3"},{"orderId":"accepted-4"}],"failureList":[{"errorCode":20003,"errorMsg":"Insufficient balance"}]}}"#,
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
    let timer = actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1000.0, Event::TimerFired { timer });
    let second = requests(&mut e).remove(0);
    assert!(second.url.ends_with("place_order"));
    reply(
        &mut e,
        second.id,
        r#"{"code":0,"data":{"orderId":"accepted-later"}}"#,
    );
    let error = classified_failure(&mut e, 1004);
    assert_eq!(error.kind, Some(ErrorKind::Rejected));
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, "Insufficient balance (20003)");
    assert_eq!(error.venue_code.as_deref(), Some("20003"));
    assert_eq!(error.venue_message.as_deref(), Some("Insufficient balance"));
    assert_eq!(
        error.partial.unwrap().order_ids,
        vec![
            "accepted-first",
            "accepted-2",
            "accepted-3",
            "accepted-4",
            "accepted-later"
        ]
    );
}

#[test]
fn an_unknown_batch_leg_ends_the_job_before_later_chunks() {
    use safe_cex_engine::Outcome;
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1005,
            orders: (0..6).map(|_| order("")).collect(),
        }),
    );
    let first = requests(&mut e).remove(0);
    reply(
        &mut e,
        first.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"accepted"},{"orderId":"accepted-2"},{"orderId":"accepted-3"},{"orderId":"accepted-4"}],"failureList":[{"errorCode":10001,"errorMsg":"Network Error"}]}}"#,
    );
    // Native sent the next chunk; an uncertain leg now stops the job's writes.
    let error = classified_failure(&mut e, 1005);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(
        error.partial.unwrap().order_ids,
        vec!["accepted", "accepted-2", "accepted-3", "accepted-4"]
    );
    assert!(
        !requests(&mut e)
            .iter()
            .any(|r| r.url.ends_with("place_order"))
    );
}

#[test]
fn partial_cancel_is_effective_before_its_rejection_is_reported() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 1006,
            ids: vec!["one".into(), "protect__position-1__tp".into()],
        }),
    );
    let first = requests(&mut e).remove(0);
    reply(
        &mut e,
        first.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"protect"}],"failureList":[]}}"#,
    );
    let second = requests(&mut e).remove(0);
    reply(
        &mut e,
        second.id,
        r#"{"code":20003,"msg":"Insufficient balance"}"#,
    );
    let error = classified_failure(&mut e, 1006);
    assert_eq!(error.outcome, None);
    assert_eq!(
        e.store().order("protect__position-1__tp").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn uncertain_amend_answer_never_enters_cancel_recreate_fallback() {
    use safe_cex_engine::Outcome;
    for status in [408, 503] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::AmendOrder {
                command: 1007,
                id: "one__tp".into(),
                price: Some(79.0),
                amount: None,
            }),
        );
        let request = requests(&mut e).remove(0);
        respond_status(
            &mut e,
            request.id,
            status,
            r#"{"code":20003,"msg":"The order does not support this operation"}"#,
        );
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        // Only the reconcile's order census follows; no fallback write.
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
        );
        assert!(actions.iter().any(|a| matches!(a,Action::Notice(Notice::CommandFailed {command:1007,error}) if error.outcome==Some(Outcome::Unknown))));
    }
}

#[test]
fn replacement_read_transport_failure_keeps_effective_cancel_without_claiming_unknown_write() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 1008,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let request = requests(&mut e).remove(0);
    reply(
        &mut e,
        request.id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = requests(&mut e).remove(0);
    reply(
        &mut e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let detail = requests(&mut e).remove(0);
    assert_eq!(detail.method, safe_cex_engine::Method::Get);
    e.handle(
        10.0,
        Event::HttpFailed {
            request: detail.id,
            error: "read lost",
        },
    );
    let error = classified_failure(&mut e, 1008);
    // The journal selects the refused amend; the lost read's text follows.
    assert_eq!(error.kind, Some(ErrorKind::Rejected));
    assert!(error.message.ends_with("; read lost"), "{}", error.message);
    assert_eq!(error.outcome, None);
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn incomplete_or_malformed_batch_ack_keeps_observed_ids_and_unknown_outcome() {
    use safe_cex_engine::Outcome;
    for data in [
        r#"{"successList":[{"orderId":"observed"}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"observed"}],"failureList":null}"#,
        r#"{"successList":[{"orderId":"observed"}],"failureList":{}}"#,
        r#"{"successList":[{"orderId":"observed"},{}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"observed","clientId":[]},{"orderId":"peer"}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"observed"},{"orderId":"  "}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"observed"},{"clientId":"\t"}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"observed"}],"failureList":[{"errorCode":"bad","errorMsg":"refused"}]}"#,
        r#"{"successList":[{"orderId":"observed"}],"failureList":[{"errorCode":null,"errorMsg":"refused"}]}"#,
        r#"{"successList":[{"orderId":"observed"},{"orderId":"observed"}],"failureList":[]}"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 1009,
                orders: vec![order(""), order("")],
            }),
        );
        let request = requests(&mut e).remove(0);
        reply(
            &mut e,
            request.id,
            &format!(r#"{{"code":0,"data":{data}}}"#),
        );
        let error = classified_failure(&mut e, 1009);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(
            error.partial.unwrap().order_ids,
            if data.contains("peer") {
                vec!["observed", "peer"]
            } else {
                vec!["observed"]
            }
        );
    }
}

#[test]
fn successful_single_ack_without_native_id_preserves_existing_client_identity_contract() {
    let (mut e, _) = startup();
    let mut options = order("");
    options.client_order_id = Some("ack-client".into());
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1010,
            orders: vec![options],
        }),
    );
    let request = requests(&mut e).remove(0);
    reply(&mut e, request.id, r#"{"code":0,"data":{}}"#);
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandDone {command:1010,result}) if result==r#"["ack-client"]"#)));
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandFailed { command: 1010, .. })
    )));
}

#[test]
fn malformed_cancel_peer_preserves_confirmed_cancellation_and_blocks_replacement() {
    use safe_cex_engine::Outcome;
    for data in [
        r#"{"successList":[{"orderId":"one"},{"orderId":[]}],"failureList":[]}"#,
        r#"{"successList":[{"orderId":"one"}],"failureList":[{"orderId":"other","errorCode":"bad"}]}"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 1011,
                ids: vec!["one__tp".into()],
            }),
        );
        let request = requests(&mut e).remove(0);
        reply(
            &mut e,
            request.id,
            &format!(r#"{{"code":0,"data":{data}}}"#),
        );
        let error = classified_failure(&mut e, 1011);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert_eq!(
            e.store().order("one").unwrap().status,
            OrderStatus::Canceled
        );
        assert!(
            requests(&mut e).is_empty(),
            "uncertain ACK must block reconciliation/replacement"
        );
    }
}

#[test]
fn undocumented_native_code_is_uncertain_even_with_an_empty_message() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1012,
            orders: vec![order("")],
        }),
    );
    let request = requests(&mut e).remove(0);
    reply(&mut e, request.id, r#"{"code":99999,"msg":""}"#);
    let error = classified_failure(&mut e, 1012);
    assert_eq!(error.kind, Some(ErrorKind::VenueUnavailable));
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.message, "");
    assert_eq!(error.venue_code.as_deref(), Some("99999"));
}
