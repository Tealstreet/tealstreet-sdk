//! Lighter opts into `guard_pending_placements`: an unproven (Unknown)
//! placement fences its symbol until a complete orders snapshot reconciles it.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn order(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn plain() -> PlaceOrderOpts {
    order(json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}))
}
fn find(actions: &[Action], path: &str) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    })
}
fn terminals(actions: &[Action], command: u32) -> Vec<Notice> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(n @ Notice::CommandDone { command: c, .. })
            | Action::Notice(n @ Notice::CommandFailed { command: c, .. })
                if *c == command =>
            {
                Some(n.clone())
            }
            _ => None,
        })
        .collect()
}
fn http(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.url.clone()),
            _ => None,
        })
        .collect()
}
fn ready_eth() -> (Engine<Lighter>, Timers) {
    let mut opts = options();
    opts.testnet = true;
    let (e, _, t) = ready_tracked(opts);
    (e, t)
}
/// Places one ETH limit as `command`, answering the nonce; returns the submission.
fn submit(e: &mut Engine<Lighter>, t: &mut Timers, command: u32, nonce: u64) -> HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![plain()],
        }),
    );
    let out = drain(e, t);
    if let Some(read) = find(&out, "nextNonce") {
        reply(e, &read, 200, &format!(r#"{{"code":200,"nonce":{nonce}}}"#));
        return find(&drain(e, t), "sendTx").expect("placement submission");
    }
    find(&out, "sendTx").expect("placement submission")
}
/// One ETH placement whose submission answers 503: sealed Unknown.
fn unknown_placement(e: &mut Engine<Lighter>, t: &mut Timers) {
    let send = submit(e, t, 7, 10);
    reply(e, &send, 503, r#"{"code":500,"message":"unavailable"}"#);
    let out = drain(e, t);
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown)),
        "precondition: the sealed placement terminal is Unknown: {:?}",
        terminals(&out, 7)
    );
}

#[test]
fn cancel_all_after_an_unknown_placement_is_refused_by_the_fence() {
    let (mut e, mut t) = ready_eth();
    assert!(
        e.store().order("42").is_some(),
        "a store ETH order would be cancelled"
    );
    unknown_placement(&mut e, &mut t);
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "ETH".into(),
        }),
    );
    let out = drain(&mut e, &mut t);
    assert!(
        http(&out).is_empty(),
        "no cancel is signed or sent: {:?}",
        http(&out)
    );
    assert!(
        matches!(&terminals(&out, 8)[..], [Notice::CommandFailed { error, .. }]
            if error.message == "Placement outcome is uncertain; reconcile before cancel-all"),
        "{:?}",
        terminals(&out, 8)
    );
}

#[test]
fn known_engine_gap_place_after_unknown_placement_is_admitted_until_engine_fix() {
    // The engine's order-lock acquire consults 'uncertain' only for cancel-all, so a
    // placement after an Unknown one still proceeds. fe7lgm's engine fix flips this
    // to a refusal; Lighter's guard_pending_placements opt-in is already in place.
    let (mut e, mut t) = ready_eth();
    unknown_placement(&mut e, &mut t);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![plain()],
        }),
    );
    let out = drain(&mut e, &mut t);
    assert!(terminals(&out, 8).is_empty(), "today: not refused");
    assert!(
        find(&out, "nextNonce").is_some(),
        "today: the placement proceeds"
    );
}

#[test]
fn a_proven_placement_and_a_later_cancel_all_still_proceed() {
    let (mut e, mut t) = ready_eth();
    let send = submit(&mut e, &mut t, 7, 10);
    reply(
        &mut e,
        &send,
        200,
        r#"{"code":200,"tx_hash":"transaction-hash"}"#,
    );
    let out = drain(&mut e, &mut t);
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandDone { .. }]),
        "a proven placement completes: {:?}",
        terminals(&out, 7)
    );
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "ETH".into(),
        }),
    );
    let out = drain(&mut e, &mut t);
    assert!(terminals(&out, 8).is_empty(), "{:?}", terminals(&out, 8));
    assert!(
        find(&out, "nextNonce").is_some() || find(&out, "sendTx").is_some(),
        "cancel-all proceeds to signing: {:?}",
        http(&out)
    );
}
