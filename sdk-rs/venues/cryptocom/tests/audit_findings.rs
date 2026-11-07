//! Crypto.com audit findings: transient login answers and documented refusals.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Action, Command, Event, Notice, Outcome, Response, SocketId};
use support::*;

fn startup_failed(h: &Harness) -> bool {
    h.actions
        .iter()
        .any(|a| matches!(a, Action::Notice(Notice::StartupFailed { .. })))
}

fn opened_private(h: &mut Harness) -> SocketId {
    let socket = h.socket(true);
    h.event(Event::WsOpened { socket });
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(*at_ms - h.now, 1000.0) => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.now += 1000.0;
    h.event(Event::TimerFired { timer });
    socket
}

// The docs warn TOO_MANY_REQUESTS is common right after a socket opens.
#[test]
fn a_transient_socket_login_answer_does_not_end_startup() {
    for reply in [
        r#"{"id":1,"method":"public/auth","code":42901,"message":"TOO_MANY_REQUESTS"}"#,
        r#"{"id":1,"method":"public/auth","code":50001,"message":"ERR_INTERNAL"}"#,
        r#"{"id":1,"method":"public/auth","code":40102,"message":"INVALID_NONCE"}"#,
    ] {
        let mut h = Harness::new();
        let socket = opened_private(&mut h);
        h.actions.clear();
        h.event(Event::WsMessage {
            socket,
            text: reply,
        });
        assert!(!startup_failed(&h), "{reply}");
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. })),
            "{reply}: a reconnect is scheduled"
        );
    }
}

#[test]
fn a_bad_key_login_still_ends_startup() {
    let mut h = Harness::new();
    let socket = opened_private(&mut h);
    h.actions.clear();
    h.event(Event::WsMessage {
        socket,
        text: r#"{"id":1,"method":"public/auth","code":40101,"message":"UNAUTHORIZED"}"#,
    });
    assert!(startup_failed(&h));
}

#[test]
fn a_rest_nonce_answer_does_not_end_startup() {
    let mut h = Harness::new();
    let r = h.http("get-positions");
    h.actions.clear();
    h.event(Event::HttpResponse {
        request: r.id,
        response: Response {
            status: 400,
            headers: "",
            body: r#"{"code":40102,"message":"INVALID_NONCE"}"#,
        },
    });
    assert!(!startup_failed(&h));
}

fn order() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":1,"price":90}"#,
    )
    .unwrap()
}

fn pace(h: &mut Harness) {
    if let Some(i) = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
    {
        let Action::SetTimer { timer, at_ms } = h.actions.remove(i) else {
            unreachable!()
        };
        h.now = at_ms;
        h.event(Event::TimerFired { timer });
    }
}

// Native refusal evidence remains authoritative at non-ambiguous HTTP statuses.
#[test]
fn nonambiguous_native_refusals_keep_their_reason_and_release_the_fence() {
    for (status, body, reason) in [
        (
            200,
            r#"{"code":306,"message":"INSUFFICIENT_AVAILABLE_BALANCE"}"#,
            "INSUFFICIENT_AVAILABLE_BALANCE",
        ),
        (
            200,
            r#"{"code":415,"message":"BELOW_MIN_ORDER_SIZE"}"#,
            "BELOW_MIN_ORDER_SIZE",
        ),
        (
            200,
            r#"{"code":43005,"message":"POST_ONLY_REJ"}"#,
            "POST_ONLY_REJ",
        ),
        (
            400,
            r#"{"code":1110,"message":"REDUCE_ONLY_REJECTED"}"#,
            "REDUCE_ONLY_REJECTED",
        ),
        (
            400,
            r#"{"code":213,"message":"INVALID_ORDERQTY"}"#,
            "INVALID_ORDERQTY",
        ),
    ] {
        let mut h = Harness::new();
        h.load();
        h.actions.clear();
        h.event(Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![order()],
        }));
        let r = h.http("private/create-order");
        h.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
        pace(&mut h);
        let error = h
            .failures
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("{body}: no failure"));
        assert_eq!(
            error.outcome,
            Some(Outcome::Rejected),
            "{body}: {}",
            error.legacy_text()
        );
        assert!(
            error.legacy_text().contains(reason),
            "{}",
            error.legacy_text()
        );
        h.failures.clear();
        h.actions.clear();
        h.event(Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![order()],
        }));
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("create-order"))),
            "{body}: the next order is fenced"
        );
    }
}
