//! AGT-45: a sendTx acknowledgement is the sequencer's queueing, not the order.
//! Placement results follow the venue's own order state or failed transaction.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, VenueError};
use safe_cex_venue_lighter::Lighter;
use serde_json::json;
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
fn terminal(actions: &[Action]) -> Option<Result<String, VenueError>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: 7, result }) => Some(Ok(result.clone())),
        Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(Err(error.clone())),
        _ => None,
    })
}
/// Place one order and acknowledge its transaction; returns the actions the
/// acknowledgement produced and the minted client index.
fn acknowledged(
    e: &mut Engine<Lighter>,
    timers: &mut Timers,
    order: Value,
) -> (Vec<Action>, String) {
    let order: PlaceOrderOpts = serde_json::from_value(order).unwrap();
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![order],
        }),
    );
    let out = drain(e, timers);
    let nonce = find(&out, "nextNonce").expect("nonce read");
    reply(e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    let out = drain(e, timers);
    let send = find(&out, "sendTx").expect("placement write");
    reply(e, &send, 200, r#"{"code":200,"tx_hash":"0xplaced"}"#);
    let out = drain(e, timers);
    let proof = find(&out, "/api/v1/accountOrders?").expect("immediate proof read");
    let ids = proof_ids(&proof);
    assert_eq!(ids.len(), 1, "{}", proof.url);
    (out, ids[0].clone())
}
use serde_json::Value;
fn limit() -> Value {
    json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90})
}
fn rows(id: &str, status: &str, filled: &str) -> String {
    json!({"code":200,"orders":[{"client_order_index": id.parse::<u64>().unwrap(),
        "order_index":"281474976710700","market_index":1,"status":status,
        "filled_base_amount":filled}]})
    .to_string()
}

#[test]
fn an_acknowledged_placement_is_not_success_until_its_order_appears() {
    let (mut e, mut timers) = ready_testnet();
    let (out, id) = acknowledged(&mut e, &mut timers, limit());
    assert_eq!(
        terminal(&out),
        None,
        "the ACK alone completes nothing: {out:?}"
    );
    let proof = find(&out, "/api/v1/accountOrders?").unwrap();
    reply(&mut e, &proof, 200, &rows(&id, "open", "0"));
    let out = drain(&mut e, &mut timers);
    assert_eq!(terminal(&out), Some(Ok(format!("[\"{id}\"]"))), "{out:?}");
}

#[test]
fn a_filled_market_order_is_proven_by_its_fill() {
    let (mut e, mut timers) = ready_testnet();
    let market = json!({"symbol":"ETH","type":"market","side":"buy","amount":0.2});
    let (out, id) = acknowledged(&mut e, &mut timers, market);
    let proof = find(&out, "/api/v1/accountOrders?").unwrap();
    reply(&mut e, &proof, 200, &rows(&id, "filled", "0.2"));
    let out = drain(&mut e, &mut timers);
    assert_eq!(terminal(&out), Some(Ok(format!("[\"{id}\"]"))), "{out:?}");
}

#[test]
fn an_order_the_venue_cancels_for_margin_fails_rejected_with_the_reason() {
    for (status, reason) in [
        ("canceled-margin-not-allowed", "not enough margin"),
        ("canceled-reduce-only", "would not reduce the position"),
        ("canceled-not-enough-liquidity", "not enough liquidity"),
    ] {
        let (mut e, mut timers) = ready_testnet();
        let (out, id) = acknowledged(&mut e, &mut timers, limit());
        assert!(
            e.store().order(&id).is_some(),
            "optimistic row while proving"
        );
        let proof = find(&out, "/api/v1/accountOrders?").unwrap();
        reply(&mut e, &proof, 200, &rows(&id, status, "0"));
        let out = drain(&mut e, &mut timers);
        let Some(Err(error)) = terminal(&out) else {
            panic!("{status}: a refused placement fails: {out:?}");
        };
        assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
        assert!(error.message.contains(reason), "{error:?}");
        assert_eq!(error.venue_message.as_deref(), Some(status));
        assert!(
            e.store().order(&id).is_none(),
            "the refused row is withdrawn"
        );
        assert!(find(&out, "sendTx").is_none(), "nothing is resent");
    }
}

#[test]
fn a_failed_transaction_with_no_order_fails_rejected_with_its_text() {
    let (mut e, mut timers) = ready_testnet();
    let (out, id) = acknowledged(&mut e, &mut timers, limit());
    let proof = find(&out, "/api/v1/accountOrders?").unwrap();
    reply(&mut e, &proof, 200, r#"{"code":200,"orders":[]}"#);
    let out = drain(&mut e, &mut timers);
    let tx = find(&out, "/api/v1/tx?by=hash&value=0xplaced").expect("tx status read");
    assert!(terminal(&out).is_none());
    reply(
        &mut e,
        &tx,
        200,
        r#"{"code":200,"hash":"0xplaced","status":0,"event_info":"{\"ae\":\"not enough margin to place order\"}"}"#,
    );
    let out = drain(&mut e, &mut timers);
    let Some(Err(error)) = terminal(&out) else {
        panic!("a failed transaction fails the placement: {out:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert!(
        error.message.contains("not enough margin to place order"),
        "{error:?}"
    );
    assert!(e.store().order(&id).is_none());
}

#[test]
fn an_unproven_placement_polls_then_reports_unknown_never_ok() {
    let (mut e, mut timers) = ready_testnet();
    let (mut out, id) = acknowledged(&mut e, &mut timers, limit());
    let (mut at, mut proofs) = (NOW, 0);
    // The row never appears and the transaction stays pending.
    loop {
        if let Some(result) = terminal(&out) {
            let Err(error) = result else {
                panic!("an unproven placement is never ok: {result:?}");
            };
            assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
            assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
            assert!(error.message.contains(&id), "{error:?}");
            assert!(
                (NOW + 9_999.0..NOW + 10_500.0).contains(&at),
                "bounded wait: {at}"
            );
            assert!((3..=8).contains(&proofs), "bounded polling: {proofs}");
            return;
        }
        let reads: Vec<HttpRequest> = out
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        for read in &reads {
            if read.url.contains("/api/v1/accountOrders?") {
                proofs += 1;
                assert_eq!(proof_ids(read), std::slice::from_ref(&id));
                reply_at(&mut e, at, read, 200, r#"{"code":200,"orders":[]}"#);
            } else if read.url.contains("/api/v1/tx?") {
                reply_at(&mut e, at, read, 200, r#"{"code":200,"status":1}"#);
            }
        }
        out = drain_at(&mut e, &mut timers, at);
        if terminal(&out).is_some() || out.iter().any(|a| matches!(a, Action::Http(_))) {
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
    }
}

#[test]
fn a_late_row_after_a_miss_still_proves_the_placement() {
    let (mut e, mut timers) = ready_testnet();
    let (out, id) = acknowledged(&mut e, &mut timers, limit());
    let proof = find(&out, "/api/v1/accountOrders?").unwrap();
    reply(&mut e, &proof, 200, r#"{"code":200,"orders":[]}"#);
    let out = drain(&mut e, &mut timers);
    let tx = find(&out, "/api/v1/tx?").unwrap();
    reply(
        &mut e,
        &tx,
        200,
        r#"{"code":21500,"message":"transaction not found"}"#,
    );
    let out = advance(&mut e, &mut timers, NOW + 300.0);
    let proof = find(&out, "/api/v1/accountOrders?").expect("a backed-off second read");
    reply_at(&mut e, NOW + 300.0, &proof, 200, &rows(&id, "open", "0"));
    let out = drain_at(&mut e, &mut timers, NOW + 300.0);
    assert_eq!(terminal(&out), Some(Ok(format!("[\"{id}\"]"))), "{out:?}");
}
