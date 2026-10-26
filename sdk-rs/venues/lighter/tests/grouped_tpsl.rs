//! Grouped limit entry with TP/SL (TS d94e9d744f1): one create_grouped_orders
//! transaction, proven leg by leg from the private stream before Done.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, SocketId};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;

const BOUND: f64 = 10_000.0;

fn order(value: Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn grouped() -> PlaceOrderOpts {
    order(
        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90,"stopLoss":80,"takeProfit":100}),
    )
}
fn ready_grouped() -> (Engine<Lighter>, SocketId, Timers) {
    let mut opts = options();
    opts.testnet = true;
    ready_tracked(opts)
}
fn find(actions: &[Action], path: &str) -> Option<HttpRequest> {
    actions.iter().find_map(|a| match a {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    })
}
fn count(actions: &[Action], path: &str) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::Http(r) if r.url.contains(path)))
        .count()
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
fn decode(text: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = text.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            let pair = [bytes.next().unwrap(), bytes.next().unwrap()];
            out.push(u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap());
        } else {
            out.push(if b == b'+' { b' ' } else { b });
        }
    }
    String::from_utf8(out).unwrap()
}
fn param(r: &HttpRequest, key: &str) -> String {
    r.body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|p| {
            let (k, v) = p.split_once('=')?;
            (k == key).then(|| decode(v))
        })
        .unwrap()
}
fn wire(r: &HttpRequest) -> Value {
    serde_json::from_str(&param(r, "tx_info")).unwrap()
}
fn ids(r: &HttpRequest) -> Vec<String> {
    wire(r)["Orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["ClientOrderIndex"].to_string())
        .collect()
}

/// Places `orders` as command 7 and returns the signed submission.
fn submit(e: &mut Engine<Lighter>, t: &mut Timers, orders: Vec<PlaceOrderOpts>) -> HttpRequest {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    let nonce = find(&drain(e, t), "nextNonce").expect("nonce read");
    reply(e, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    find(&drain(e, t), "sendTx").expect("grouped submission")
}
fn ack(e: &mut Engine<Lighter>, t: &mut Timers, r: &HttpRequest) -> Vec<Action> {
    reply(e, r, 200, r#"{"code":200,"tx_hash":"transaction-hash"}"#);
    drain(e, t)
}
/// A distinct native order index for leg `i`; our minted index rides in
/// `client_order_index` only, as Lighter reports it.
fn native(i: usize) -> String {
    (900_001 + i).to_string()
}
/// One private-stream frame carrying `rows` (each `(order_index, client_index, status)`).
fn stream(e: &mut Engine<Lighter>, s: SocketId, now: f64, rows: &[(&str, Option<&str>, &str)]) {
    let rows: Vec<Value> = rows
        .iter()
        .map(|(id, client, status)| {
            let mut row = json!({"order_index":id,"market_id":1,"status":status,"initial_base_amount":"0.2","price":"90"});
            if let Some(client) = client {
                row["client_order_index"] = json!(client);
            }
            row
        })
        .collect();
    let frame = json!({"channel":"account_all_orders:718895","orders":{"1":rows}});
    message(e, s, now, &frame.to_string());
}
/// A complete active-orders answer; each row is `(order_index, client_order_index)`.
fn snapshot(rows: &[(String, &str)]) -> String {
    let rows: Vec<Value> = rows
        .iter()
        .map(|(id, client)| json!({"order_index":id,"client_order_index":client,"market_id":1,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"0.2","filled_base_amount":"0","remaining_base_amount":"0.2","price":"90"}))
        .collect();
    json!({"code":200,"orders":rows}).to_string()
}

#[test]
fn limit_with_sl_and_tp_is_one_otoco_transaction_parent_then_sl_then_tp() {
    let (mut e, _, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    assert_eq!(param(&send, "tx_type"), "28");
    let tx = wire(&send);
    assert_eq!(tx["GroupingType"], 3);
    let orders = tx["Orders"].as_array().unwrap();
    assert_eq!(orders.len(), 3);
    assert_eq!(
        (orders[0]["IsAsk"].clone(), orders[0]["Type"].clone()),
        (json!(0), json!(0))
    );
    assert!(orders[0]["BaseAmount"].as_u64().unwrap() > 0);
    for (leg, kind, trigger) in [(&orders[1], 2, 8_000), (&orders[2], 4, 10_000)] {
        assert_eq!(leg["IsAsk"], 1, "opposite side");
        assert_eq!(leg["Type"], kind);
        assert_eq!(leg["BaseAmount"], 0, "children close the filled parent");
        assert_eq!(leg["ReduceOnly"], 1);
        assert_eq!(leg["TimeInForce"], 0, "immediate or cancel");
        assert_eq!(leg["TriggerPrice"], trigger);
        assert!(leg["Price"].as_u64().unwrap() > 0, "execution guard price");
    }
}

#[test]
fn stop_loss_only_is_one_oto_transaction() {
    let (mut e, _, mut t) = ready_grouped();
    let mut entry = grouped();
    entry.take_profit = None;
    let send = submit(&mut e, &mut t, vec![entry]);
    let tx = wire(&send);
    assert_eq!(param(&send, "tx_type"), "28");
    assert_eq!(tx["GroupingType"], 1);
    assert_eq!(tx["Orders"].as_array().unwrap().len(), 2);
}

#[test]
fn a_multi_lot_parent_or_invalid_trigger_is_refused_before_any_request() {
    for bad in [
        json!({"symbol":"ETH","type":"limit","side":"buy","amount":1_500_000,"price":90,"stopLoss":80}),
        json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90,"stopLoss":-1}),
    ] {
        let (mut e, _, mut t) = ready_grouped();
        e.handle(
            NOW,
            Event::Command(Command::PlaceOrders {
                command: 7,
                orders: vec![order(bad.clone())],
            }),
        );
        let out = drain(&mut e, &mut t);
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{bad}");
        let failed = terminals(&out, 7);
        assert!(
            matches!(&failed[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::NotSent)),
            "{bad}: {failed:?}"
        );
    }
}

#[test]
fn market_entry_protections_keep_their_separate_transactions() {
    let (mut e, _, mut t) = ready_grouped();
    let mut entry = grouped();
    entry.kind = OrderType::Market;
    entry.price = None;
    let send = submit(&mut e, &mut t, vec![entry]);
    assert_eq!(param(&send, "tx_type"), "14");
}

#[test]
fn every_leg_proven_by_the_stream_completes_with_all_ids_parent_first() {
    let (mut e, s, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    assert!(
        terminals(&ack(&mut e, &mut t, &send), 7).is_empty(),
        "an ACK alone is not Done"
    );
    stream(
        &mut e,
        s,
        NOW + 1.0,
        &[
            (&native(0), Some(&legs[0]), "open"),
            (&native(1), Some(&legs[1]), "pending"),
            (&native(2), Some(&legs[2]), "pending"),
        ],
    );
    let out = advance(&mut e, &mut t, NOW + 2.0);
    let done = terminals(&out, 7);
    assert!(
        matches!(&done[..], [Notice::CommandDone { result, .. }] if serde_json::from_str::<Vec<String>>(result).unwrap() == legs),
        "{done:?}"
    );
    assert_eq!(count(&out, "sendTx"), 0);
}

#[test]
fn two_of_three_legs_never_release_and_the_bound_reports_unknown_with_every_id() {
    let (mut e, s, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let mut out = ack(&mut e, &mut t, &send);
    stream(
        &mut e,
        s,
        NOW + 1.0,
        &[
            (&native(0), Some(&legs[0]), "open"),
            (&native(1), Some(&legs[1]), "pending"),
        ],
    );
    out.extend(advance(&mut e, &mut t, NOW + BOUND - 100.0));
    assert!(
        terminals(&out, 7).is_empty(),
        "a partial sighting never releases"
    );
    out.extend(advance(&mut e, &mut t, NOW + BOUND + 1_000.0));
    let failed = terminals(&out, 7);
    let [Notice::CommandFailed { error, .. }] = &failed[..] else {
        panic!("one terminal expected: {failed:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    for leg in &legs {
        assert!(error.message.contains(leg.as_str()), "{}", error.message);
    }
    assert_eq!(count(&out, "sendTx"), 0, "nothing is replayed");
}

#[test]
fn ready_handler_seals_an_unproven_settled_group_as_unknown_with_every_id() {
    // 8x5tm1/dazzy3 pin: a released settled Plan has no bound left, so the ready
    // handler must end it even when the proof set is incomplete (stream lost).
    let (mut e, s, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let mut out = ack(&mut e, &mut t, &send);
    stream(
        &mut e,
        s,
        NOW + 1.0,
        &[(&native(0), Some(&legs[0]), "open")],
    );
    e.handle(
        NOW + 2.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    out.extend(advance(&mut e, &mut t, NOW + 3.0));
    let failed = terminals(&out, 7);
    let [Notice::CommandFailed { error, .. }] = &failed[..] else {
        panic!("the ready handler must seal exactly once: {failed:?}");
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(legs.iter().all(|leg| error.message.contains(leg.as_str())));
    // Never parked: nothing further happens at or past the original bound.
    let later = advance(&mut e, &mut t, NOW + BOUND * 3.0);
    assert!(terminals(&later, 7).is_empty());
    assert_eq!(count(&later, "sendTx"), 0);
}

#[test]
fn grouped_5xx_and_nonce_refusal_are_unknown_and_never_resent() {
    for (status, body) in [
        (503, r#"{"code":500,"message":"unavailable"}"#),
        (400, r#"{"code":21104,"message":"invalid nonce"}"#),
    ] {
        let (mut e, _, mut t) = ready_grouped();
        let send = submit(&mut e, &mut t, vec![grouped()]);
        reply(&mut e, &send, status, body);
        let out = advance(&mut e, &mut t, NOW + BOUND * 2.0);
        let failed = terminals(&out, 7);
        assert!(
            matches!(&failed[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown)),
            "{status}: {failed:?}"
        );
        assert_eq!(
            count(&out, "sendTx"),
            0,
            "{status}: no second grouped submission"
        );
    }
}

#[test]
fn cancel_or_amend_of_a_pending_group_leg_is_refused_before_any_request() {
    let (mut e, _, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    ack(&mut e, &mut t, &send);
    e.handle(
        NOW + 1.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec![legs[1].clone()],
        }),
    );
    e.handle(
        NOW + 1.0,
        Event::Command(Command::AmendOrder {
            command: 9,
            id: legs[0].clone(),
            price: Some(91.0),
            amount: None,
        }),
    );
    let out = drain_at(&mut e, &mut t, NOW + 1.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    for command in [8, 9] {
        let failed = terminals(&out, command);
        assert!(
            matches!(&failed[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::NotSent) && error.message.contains("awaiting confirmation of its grouped placement")),
            "{command}: {failed:?}"
        );
    }
}

#[test]
fn a_later_complete_snapshot_reconciles_an_unknown_group_into_the_store() {
    let (mut e, _, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let mut out = ack(&mut e, &mut t, &send);
    out.extend(advance(&mut e, &mut t, NOW + BOUND + 1_000.0));
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown))
    );
    let now = NOW + BOUND + 2_000.0;
    let read = find(&out, "accountActiveOrders")
        .or_else(|| find(&advance(&mut e, &mut t, now), "accountActiveOrders"))
        .expect("the Unknown group schedules a reconciling orders snapshot");
    let rows: Vec<(String, &str)> = (0..3).map(|i| (native(i), legs[i].as_str())).collect();
    reply_at(&mut e, now, &read, 200, &snapshot(&rows));
    let mut stored: Vec<String> = e.store().orders().iter().map(|o| o.id.clone()).collect();
    stored.sort();
    assert_eq!(
        stored,
        vec![native(0), native(1), native(2)],
        "the store is exactly the reconciling snapshot"
    );
    let next = advance(&mut e, &mut t, now + BOUND);
    assert!(
        terminals(&next, 7).is_empty(),
        "the Unknown group never turns into Done"
    );
}

#[test]
fn a_snapshot_missing_one_leg_reports_it_missing_and_never_done() {
    let (mut e, _, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let mut out = ack(&mut e, &mut t, &send);
    out.extend(advance(&mut e, &mut t, NOW + BOUND + 1_000.0));
    let now = NOW + BOUND + 2_000.0;
    let read = find(&out, "accountActiveOrders")
        .or_else(|| find(&advance(&mut e, &mut t, now), "accountActiveOrders"))
        .expect("reconciling orders snapshot");
    reply_at(
        &mut e,
        now,
        &read,
        200,
        &snapshot(&[(native(0), &legs[0]), (native(1), &legs[1])]),
    );
    assert!(e.store().order(&native(0)).is_some());
    assert!(
        e.store().order(&native(2)).is_none() && e.store().order(&legs[2]).is_none(),
        "the missing take profit is not invented"
    );
    let later = advance(&mut e, &mut t, now + BOUND);
    assert!(terminals(&later, 7).is_empty(), "no Done after the Unknown");
    assert!(
        terminals(&out, 7)
            .iter()
            .all(|n| matches!(n, Notice::CommandFailed { error, .. } if error.outcome == Some(Outcome::Unknown)))
    );
}

#[test]
fn known_engine_gap_place_after_unknown_placement_is_admitted() {
    // Documents the engine gap: order_locks acquire checks 'uncertain' only for
    // CancelSymbolOrders, so a placement after an Unknown one is admitted. fe7lgm's fix
    // flips this to a refusal (Lighter must also opt into guard_pending_placements).
    let (mut e, _, mut t) = ready_grouped();
    let plain =
        || order(json!({"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90}));
    let send = submit(&mut e, &mut t, vec![plain()]);
    reply(
        &mut e,
        &send,
        503,
        r#"{"code":500,"message":"unavailable"}"#,
    );
    let out = drain(&mut e, &mut t);
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown)),
        "precondition: the sealed placement terminal is Unknown"
    );
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: 8,
            orders: vec![plain()],
        }),
    );
    let next = drain(&mut e, &mut t);
    assert!(terminals(&next, 8).is_empty(), "today: not refused");
    let nonce = find(&next, "nextNonce").expect("today: the second placement proceeds");
    reply(&mut e, &nonce, 200, r#"{"code":200,"nonce":11}"#);
    let second = find(&drain(&mut e, &mut t), "sendTx").expect("today: it is signed and sent");
    assert_eq!(
        param(&second, "tx_type"),
        "14",
        "a native create-order transaction"
    );
}

#[test]
fn a_native_index_equal_to_a_leg_client_index_never_proves_it() {
    // dazzy3: native order indexes and minted client indexes share the u64 space.
    let (mut e, s, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let mut out = ack(&mut e, &mut t, &send);
    stream(
        &mut e,
        s,
        NOW + 1.0,
        &[
            (&native(0), Some(&legs[0]), "open"),
            (&native(1), Some(&legs[1]), "pending"),
            // An unrelated order whose NATIVE index coincides with the TP's client index.
            (&legs[2], Some("123"), "open"),
        ],
    );
    out.extend(advance(&mut e, &mut t, NOW + BOUND - 100.0));
    assert!(
        terminals(&out, 7).is_empty(),
        "a coincident native index never releases"
    );
    out.extend(advance(&mut e, &mut t, NOW + BOUND + 1_000.0));
    assert!(
        matches!(&terminals(&out, 7)[..], [Notice::CommandFailed { error, .. }] if error.outcome == Some(Outcome::Unknown)),
        "{:?}",
        terminals(&out, 7)
    );
}

#[test]
fn a_complete_snapshot_with_client_indexes_proves_the_group() {
    let (mut e, _, mut t) = ready_grouped();
    let send = submit(&mut e, &mut t, vec![grouped()]);
    let legs = ids(&send);
    let out = ack(&mut e, &mut t, &send);
    let read = find(&out, "accountActiveOrders").expect("the ACK refreshes active orders");
    let rows: Vec<(String, &str)> = (0..3).map(|i| (native(i), legs[i].as_str())).collect();
    reply_at(&mut e, NOW + 1.0, &read, 200, &snapshot(&rows));
    let done = terminals(&advance(&mut e, &mut t, NOW + 2.0), 7);
    assert!(
        matches!(&done[..], [Notice::CommandDone { result, .. }] if serde_json::from_str::<Vec<String>>(result).unwrap() == legs),
        "{done:?}"
    );
}
