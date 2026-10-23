//! Original-source Spot controls without touching the reserved Spot test target.
mod spot_support;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Response, model::PlaceOrderOpts,
};
use safe_cex_venue_kraken::KrakenSpot;
use spot_support::*;

fn order() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"XBT/USD","type":"limit","side":"buy","amount":1,"price":99}"#,
    )
    .unwrap()
}
fn place(e: &mut Engine<KrakenSpot>, count: usize) -> HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 761,
            orders: (0..count).map(|_| order()).collect(),
        }),
    );
    requests(e)
        .into_iter()
        .find(|request| request.url.ends_with("AddOrder"))
        .expect("native Spot placement")
}
fn respond(e: &mut Engine<KrakenSpot>, request: &HttpRequest, status: u16, body: &str) {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

#[test]
#[ignore = "original RED: Spot write adoption waits for signer and Plan seams"]
fn p0_write_spot_accepted_txid_survives_later_refusal() {
    let (mut e, _, _) = ready(false);
    let first = place(&mut e, 2);
    respond(
        &mut e,
        &first,
        200,
        r#"{"error":[],"result":{"txid":["spot-accepted"]}}"#,
    );
    let second = requests(&mut e)
        .into_iter()
        .find(|request| request.url.ends_with("AddOrder"))
        .unwrap();
    respond(
        &mut e,
        &second,
        200,
        r#"{"error":["EOrder:Insufficient funds"],"result":{}}"#,
    );
    let out: Vec<_> = e.drain_actions().collect();
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 761,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert!(!matches!(
        failures[0].outcome,
        Some(Outcome::Rejected | Outcome::NotSent)
    ));
    assert_eq!(
        failures[0]
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.as_slice()),
        Some(["spot-accepted".to_owned()].as_slice())
    );
    assert!(!out.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("AddOrder"))
    ));
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 761, .. })
    )));
}

#[test]
#[ignore = "original RED: Spot role-aware classification waits for shared adoption"]
fn p0_write_spot_non_auth_native_error_on_401_does_not_deactivate() {
    let (mut e, _, _) = ready(false);
    let request = place(&mut e, 1);
    respond(
        &mut e,
        &request,
        401,
        r#"{"error":["EOrder:Insufficient funds"],"result":{}}"#,
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 761, .. })
    )));
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Notice(Notice::Deactivate))),
        "status alone must not revoke a valid credential"
    );
    assert_eq!(
        out.iter()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 761, .. })
            ))
            .count(),
        1
    );
    assert!(!out.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("AddOrder"))
    ));
}

#[test]
#[ignore = "original RED: Spot write resend must use the shared classifier grant"]
fn p0_write_spot_5xx_signature_error_does_not_authorize_resend() {
    let (mut e, _, _) = ready(false);
    let request = place(&mut e, 1);
    respond(
        &mut e,
        &request,
        503,
        r#"{"error":["EAPI:Invalid signature"],"result":{}}"#,
    );
    let out: Vec<_> = e.drain_actions().collect();
    assert!(!out.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 761, .. })
    )));
    assert!(
        !out.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.ends_with("AddOrder"))
        ),
        "uncertain writes cannot be replayed"
    );
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 761,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Unknown));
}
