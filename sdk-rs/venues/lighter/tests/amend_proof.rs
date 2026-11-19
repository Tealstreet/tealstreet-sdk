//! AGT-77: a ModifyOrder acknowledgement is the sequencer's queueing, not the
//! edit. A size-up the venue refuses for margin fails its transaction; the amend
//! must report that refusal, and the next write must not reuse a skipped nonce.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, VenueError};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

fn ready_testnet() -> (Engine<Lighter>, Timers) {
    let mut opts = options();
    opts.testnet = true;
    let (e, _, timers) = ready_tracked(opts);
    (e, timers)
}
fn find(actions: &[Action], path: &str) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    })
}
fn terminal(actions: &[Action], command: u32) -> Option<Result<String, VenueError>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
            Some(Err(error.clone()))
        }
        _ => None,
    })
}
fn tx_nonce(r: &HttpRequest) -> u64 {
    let body = r.body.as_deref().unwrap();
    let info = body
        .split('&')
        .find_map(|p| p.strip_prefix("tx_info="))
        .unwrap();
    let mut decoded = Vec::new();
    let mut bytes = info.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
            decoded.push(u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap());
        } else {
            decoded.push(if b == b'+' { b' ' } else { b });
        }
    }
    let tx: Value = serde_json::from_slice(&decoded).unwrap();
    tx["Nonce"].as_u64().unwrap()
}

/// Size the resting order up and acknowledge its modify at nonce 10.
fn acknowledged_size_up(e: &mut Engine<Lighter>, timers: &mut Timers) -> Vec<Action> {
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: None,
            amount: Some(3.0),
        }),
    );
    let out = drain(e, timers);
    let nonce = find(&out, "nextNonce").expect("nonce read");
    reply(e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let out = drain(e, timers);
    let modify = find(&out, "sendTx").expect("modify write");
    assert_eq!(tx_nonce(&modify), 10);
    reply(e, &modify, 200, r#"{"code":200,"tx_hash":"0xmodify"}"#);
    drain(e, timers)
}

/// The next write: a plain limit placement, returning its nonce read or submit.
fn next_write(e: &mut Engine<Lighter>, timers: &mut Timers) -> Vec<Action> {
    let order: PlaceOrderOpts = serde_json::from_value(
        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}),
    )
    .unwrap();
    e.handle(
        NOW + 20_000.0,
        Event::Command(Command::PlaceOrders {
            command: 9,
            orders: vec![order],
        }),
    );
    drain_at(e, timers, NOW + 20_000.0)
}

#[test]
fn a_modify_ack_alone_is_not_success() {
    let (mut e, mut timers) = ready_testnet();
    let out = acknowledged_size_up(&mut e, &mut timers);
    assert_eq!(
        terminal(&out, 8),
        None,
        "the ACK alone completes nothing: {out:?}"
    );
    assert!(
        find(&out, "/api/v1/tx?by=hash&value=0xmodify").is_some(),
        "the modify's transaction is read: {out:?}"
    );
}

#[test]
fn a_failed_modify_transaction_is_rejected_with_the_venue_reason_and_resyncs_the_nonce() {
    let (mut e, mut timers) = ready_testnet();
    let out = acknowledged_size_up(&mut e, &mut timers);
    let tx = find(&out, "/api/v1/tx?by=hash&value=0xmodify").expect("tx status read");
    reply(
        &mut e,
        &tx,
        200,
        r#"{"code":200,"hash":"0xmodify","status":0,"event_info":"{\"ae\":\"not enough margin to modify order\"}"}"#,
    );
    let out = drain(&mut e, &mut timers);
    let Some(Err(error)) = terminal(&out, 8) else {
        panic!("a failed modify transaction fails the amend: {out:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert!(
        error.message.contains("Lighter order not changed"),
        "{error:?}"
    );
    assert!(error.message.contains("not enough margin"), "{error:?}");
    assert!(find(&out, "sendTx").is_none(), "nothing is resent");
    assert!(e.store().order("42").is_some(), "the original order stays");

    // The refused transaction did not take nonce 10's successor for granted.
    let out = next_write(&mut e, &mut timers);
    assert!(
        find(&out, "sendTx").is_none(),
        "no submit on a stale nonce: {out:?}"
    );
    let nonce = find(&out, "nextNonce").expect("the nonce is fetched again");
    reply_at(
        &mut e,
        NOW + 20_000.0,
        &nonce,
        200,
        r#"{"code":200,"nonce":10}"#,
    );
    let out = drain_at(&mut e, &mut timers, NOW + 20_000.0);
    let send = find(&out, "sendTx").expect("the next write is sent");
    assert_eq!(tx_nonce(&send), 10, "the venue's own nonce is used");
}

#[test]
fn an_executed_modify_succeeds_and_keeps_the_nonce_sequence() {
    let (mut e, mut timers) = ready_testnet();
    let out = acknowledged_size_up(&mut e, &mut timers);
    let tx = find(&out, "/api/v1/tx?by=hash&value=0xmodify").expect("tx status read");
    reply(
        &mut e,
        &tx,
        200,
        r#"{"code":200,"hash":"0xmodify","status":2}"#,
    );
    let out = drain(&mut e, &mut timers);
    assert_eq!(terminal(&out, 8), Some(Ok(r#"["42"]"#.into())), "{out:?}");

    let out = next_write(&mut e, &mut timers);
    assert!(find(&out, "nextNonce").is_none(), "{out:?}");
    let send = find(&out, "sendTx").expect("the next write uses the local sequence");
    assert_eq!(tx_nonce(&send), 11);
}

#[test]
fn an_unproven_modify_reports_unknown_and_resyncs_the_nonce() {
    let (mut e, mut timers) = ready_testnet();
    let mut out = acknowledged_size_up(&mut e, &mut timers);
    let mut at = NOW;
    let error = loop {
        if let Some(result) = terminal(&out, 8) {
            break result.expect_err("an unproven modify is never ok");
        }
        for read in out.iter().filter_map(|a| match a {
            Action::Http(r) if r.url.contains("/api/v1/tx?") => Some(r.clone()),
            _ => None,
        }) {
            reply_at(&mut e, at, &read, 200, r#"{"code":200,"status":1}"#);
        }
        out = drain_at(&mut e, &mut timers, at);
        if terminal(&out, 8).is_some() || out.iter().any(|a| matches!(a, Action::Http(_))) {
            continue;
        }
        let (timer, when) = timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a pending proof keeps a timer");
        assert!(when < NOW + 15_000.0, "the proof bound never expired");
        timers.retain(|(t, _)| *t != timer);
        at = when;
        e.handle(at, Event::TimerFired { timer });
        out = drain_at(&mut e, &mut timers, at);
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
    // The Unknown fences placements and amends; a cancel is the next write.
    e.handle(
        at,
        Event::Command(Command::CancelOrders {
            command: 9,
            ids: vec!["42".into()],
        }),
    );
    let out = drain_at(&mut e, &mut timers, at);
    assert!(find(&out, "sendTx").is_none(), "{out:?}");
    assert!(find(&out, "nextNonce").is_some(), "{out:?}");
}
