//! Lighter opts into `guard_pending_placements` and `fence_uncertain_placements`:
//! an unproven (Unknown) placement fences its symbol until a complete orders
//! snapshot requested after that write reconciles it.
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
fn place(e: &mut Engine<Lighter>, at: f64, command: u32) {
    e.handle(
        at,
        Event::Command(Command::PlaceOrders {
            command,
            orders: vec![plain()],
        }),
    );
}
/// Places one ETH limit as `command` at `at`, answering the nonce; returns the submission.
fn submit_at(
    e: &mut Engine<Lighter>,
    t: &mut Timers,
    at: f64,
    command: u32,
    nonce: u64,
) -> HttpRequest {
    place(e, at, command);
    let out = drain_at(e, t, at);
    if let Some(read) = find(&out, "nextNonce") {
        reply_at(
            e,
            at,
            &read,
            200,
            &format!(r#"{{"code":200,"nonce":{nonce}}}"#),
        );
        return find(&drain_at(e, t, at), "sendTx").expect("placement submission");
    }
    find(&out, "sendTx").expect("placement submission")
}
fn submit(e: &mut Engine<Lighter>, t: &mut Timers, command: u32, nonce: u64) -> HttpRequest {
    submit_at(e, t, NOW, command, nonce)
}
fn orders_reads(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.url.contains("accountActiveOrders") => Some(r.clone()),
            _ => None,
        })
        .collect()
}
/// One ETH placement at `at` whose submission answers 503: sealed Unknown.
/// Returns what followed the seal (the reconcile's reads among it).
fn unknown_placement_at(e: &mut Engine<Lighter>, t: &mut Timers, at: f64) -> Vec<Action> {
    let send = submit_at(e, t, at, 7, 10);
    reply_at(e, at, &send, 503, r#"{"code":500,"message":"unavailable"}"#);
    let out = drain_at(e, t, at);
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown)),
        "precondition: the sealed placement terminal is Unknown: {:?}",
        terminals(&out, 7)
    );
    out
}
fn unknown_placement(e: &mut Engine<Lighter>, t: &mut Timers) -> Vec<Action> {
    unknown_placement_at(e, t, NOW)
}
/// A later placement on ETH is refused before any request.
fn assert_place_refused(e: &mut Engine<Lighter>, t: &mut Timers, at: f64, command: u32) {
    place(e, at, command);
    let out = drain_at(e, t, at);
    assert!(http(&out).is_empty(), "nothing is sent: {:?}", http(&out));
    assert!(
        matches!(&terminals(&out, command)[..], [Notice::CommandFailed { error, .. }]
            if error.outcome == Some(Outcome::NotSent)
                && error.message == "Placement outcome is uncertain; reconcile before placing or amending"),
        "{:?}",
        terminals(&out, command)
    );
}
/// A later placement on ETH proceeds to signing.
fn assert_place_admitted(e: &mut Engine<Lighter>, t: &mut Timers, at: f64, command: u32) {
    place(e, at, command);
    let out = drain_at(e, t, at);
    assert!(
        terminals(&out, command).is_empty(),
        "{:?}",
        terminals(&out, command)
    );
    assert!(
        find(&out, "nextNonce").is_some() || find(&out, "sendTx").is_some(),
        "the placement proceeds: {:?}",
        http(&out)
    );
}

// Lighter's cancel-all is composed from the local store (one cancel per known id), so
// it would skip the uncertain order yet report all cancelled; it stays refused
// (fence_uncertain_cancel_all default) until reconcile. Cancel by id stays admitted.
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
fn place_after_unknown_placement_is_refused_until_reconciled() {
    let (mut e, mut t) = ready_eth();
    let out = unknown_placement(&mut e, &mut t);
    let reads = orders_reads(&out);
    assert!(
        !reads.is_empty(),
        "the Unknown starts an orders read at once"
    );
    assert_place_refused(&mut e, &mut t, NOW, 8);
    for read in &reads {
        reply(&mut e, read, 200, OPEN);
    }
    drain(&mut e, &mut t);
    assert_place_admitted(&mut e, &mut t, NOW, 9);
}

#[test]
fn a_fresh_complete_snapshot_releases_cancel_all_and_its_cancels_send() {
    // dazzy3: a stuck cancel-all fence is the worst failure on this path.
    let (mut e, mut t) = ready_eth();
    let out = unknown_placement(&mut e, &mut t);
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "ETH".into(),
        }),
    );
    assert!(
        matches!(
            &terminals(&drain(&mut e, &mut t), 8)[..],
            [Notice::CommandFailed { .. }]
        ),
        "refused while uncertain"
    );
    for read in &orders_reads(&out) {
        reply(&mut e, read, 200, OPEN);
    }
    drain(&mut e, &mut t);
    e.handle(
        NOW,
        Event::Command(Command::CancelSymbolOrders {
            command: 9,
            symbol: "ETH".into(),
        }),
    );
    let out = drain(&mut e, &mut t);
    assert!(terminals(&out, 9).is_empty(), "{:?}", terminals(&out, 9));
    let send = if let Some(read) = find(&out, "nextNonce") {
        reply(&mut e, &read, 200, r#"{"code":200,"nonce":20}"#);
        find(&drain(&mut e, &mut t), "sendTx")
    } else {
        find(&out, "sendTx")
    };
    assert!(send.is_some(), "the released cancel-all sends its cancel");
}

#[test]
fn an_orders_read_issued_before_the_unknown_write_does_not_release_it() {
    let (mut e, mut t) = ready_eth();
    // The 2 s sync poll puts an orders read in flight before the placement.
    let at = NOW + 2_000.;
    let stale = orders_reads(&advance(&mut e, &mut t, at));
    assert_eq!(stale.len(), 1, "one pre-write orders read is in flight");
    let out = unknown_placement_at(&mut e, &mut t, at);
    // Its answer, though complete, predates the write: still fenced.
    reply_at(&mut e, at, &stale[0], 200, OPEN);
    let after = drain_at(&mut e, &mut t, at);
    assert_place_refused(&mut e, &mut t, at, 8);
    // A fresh read follows and releases the fence.
    let fresh: Vec<_> = orders_reads(&out)
        .into_iter()
        .chain(orders_reads(&after))
        .filter(|r| r.id != stale[0].id)
        .collect();
    assert!(
        !fresh.is_empty(),
        "a fresh orders read follows the stale one"
    );
    for read in &fresh {
        reply_at(&mut e, at, read, 200, OPEN);
    }
    drain_at(&mut e, &mut t, at);
    assert_place_admitted(&mut e, &mut t, at, 9);
}

// The engine re-asks reconcile_uncertain at 1, 2 and 4 s while fenced. A re-ask must
// join the outstanding post-write read (owing at most one more), never restart it or
// raise a release floor, so a read slower than the backoff still releases on arrival.
#[test]
fn reconcile_re_asks_join_a_slow_post_write_read_that_still_releases() {
    let (mut e, mut t) = ready_eth();
    let out = unknown_placement(&mut e, &mut t);
    let reads = orders_reads(&out);
    assert_eq!(
        reads.len(),
        1,
        "one post-write orders read: {:?}",
        http(&out)
    );
    // Re-asks at +1, +2 and +4 s while that read stays unanswered.
    let mut during = Vec::new();
    for at in [NOW + 1_000., NOW + 2_000., NOW + 4_000., NOW + 4_500.] {
        during.extend(orders_reads(&advance(&mut e, &mut t, at)));
    }
    assert!(
        during.is_empty(),
        "no re-ask restarts or duplicates the in-flight read: {:?}",
        during.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    let at = NOW + 4_500.;
    assert_place_refused(&mut e, &mut t, at, 8);
    // The slow read lands clean: the fence releases on its completion.
    reply_at(&mut e, at, &reads[0], 200, OPEN);
    let after = orders_reads(&drain_at(&mut e, &mut t, at));
    assert!(
        after.len() <= 1,
        "at most the one owed read follows: {:?}",
        after.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_place_admitted(&mut e, &mut t, at, 9);
}

#[test]
fn a_failed_orders_read_keeps_the_fence_until_a_later_clean_read() {
    let (mut e, mut t) = ready_eth();
    let out = unknown_placement(&mut e, &mut t);
    for read in &orders_reads(&out) {
        reply(&mut e, read, 503, r#"{"code":500,"message":"unavailable"}"#);
    }
    // The failure settles the read and a fresh post-write read follows at once;
    // the failed one released nothing.
    let fresh = orders_reads(&drain(&mut e, &mut t));
    assert_eq!(fresh.len(), 1, "one fresh orders read follows the failure");
    assert_place_refused(&mut e, &mut t, NOW, 8);
    reply(&mut e, &fresh[0], 200, OPEN);
    drain(&mut e, &mut t);
    assert_place_admitted(&mut e, &mut t, NOW, 9);
}

#[test]
fn a_snapshot_with_an_unmappable_row_does_not_release_the_fence() {
    let (mut e, mut t) = ready_eth();
    let out = unknown_placement(&mut e, &mut t);
    // A row on an unknown market leaves the snapshot's coverage incomplete.
    let partial = r#"{"code":200,"orders":[{"order_index":"42","market_id":1,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2","price":"90"},{"order_index":"77","market_id":99,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"1","filled_base_amount":"0","remaining_base_amount":"1","price":"5"}]}"#;
    for read in &orders_reads(&out) {
        reply(&mut e, read, 200, partial);
    }
    drain(&mut e, &mut t);
    assert_place_refused(&mut e, &mut t, NOW, 8);
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
