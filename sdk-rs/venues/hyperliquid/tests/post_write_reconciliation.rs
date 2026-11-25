mod support;

use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, model::PlaceOrderOpts};
use safe_cex_venue_hyperliquid::Hyperliquid;
use support::*;

fn place(kind: &str, result: &str) -> (Engine<Hyperliquid>, Vec<Action>) {
    let (engine, _) = ready("disabled");
    place_orders(engine, vec![order("ETH-PERP", kind)], result)
}

fn order(symbol: &str, kind: &str) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": symbol, "type": kind, "side": "buy", "price": 190, "amount": 0.2
    }))
    .unwrap()
}

fn place_orders(
    mut engine: Engine<Hyperliquid>,
    orders: Vec<PlaceOrderOpts>,
    result: &str,
) -> (Engine<Hyperliquid>, Vec<Action>) {
    let actions = acknowledge(&mut engine, orders, 93, 2_000., result);
    (engine, actions)
}

fn acknowledge(
    engine: &mut Engine<Hyperliquid>,
    orders: Vec<PlaceOrderOpts>,
    command: u32,
    now: f64,
    result: &str,
) -> Vec<Action> {
    engine.take_frame(now - 1.);
    engine.handle(
        now,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
    let actions = prepared_now(engine);
    let request = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("/exchange") => Some(request),
            _ => None,
        })
        .expect("the fixture must actually dispatch the order");
    reply(engine, request, 200, result);
    let mut actions = actions
        .into_iter()
        .filter(|action| !matches!(action, Action::Http(_)))
        .collect::<Vec<_>>();
    actions.extend(prepared_now(engine));
    assert!(
        actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandDone { command: completed, .. }) if *completed == command
        )),
        "the native reply must be a confirmed placement: {actions:?}"
    );
    actions
}

fn reads_within_second(
    engine: &mut Engine<Hyperliquid>,
    mut actions: Vec<Action>,
) -> Vec<HttpRequest> {
    let mut reads = Vec::new();
    for _ in 0..20 {
        let mut next = Vec::new();
        for action in actions {
            match action {
                Action::Http(request) => reads.push(request),
                Action::SetTimer { timer, at_ms } if at_ms <= 3_000. => {
                    engine.handle(at_ms, Event::TimerFired { timer });
                    next.extend(prepared_now(engine));
                }
                _ => {}
            }
        }
        if next.is_empty() {
            return reads;
        }
        actions = next;
    }
    panic!("post-write work did not settle");
}

#[test]
fn a_resting_ack_publishes_the_order_without_a_private_stream_update() {
    let (mut engine, actions) = place(
        "limit",
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":93}}]}}}"#,
    );
    assert_eq!(engine.store().order("93").unwrap().symbol, "ETH-PERP");
    let frame = engine
        .take_frame(2_001.)
        .expect("the order must be in the next native flush");
    assert!(
        safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::ORDERS)
    );
    assert!(
        reads_within_second(&mut engine, actions).is_empty(),
        "a resting ACK does not imply a fill or need a position read"
    );
}

#[test]
fn a_confirmed_fill_reconciles_the_position_when_private_updates_are_delayed() {
    let (mut engine, actions) = place(
        "market",
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":93,"totalSz":"0.2","avgPx":"200"}}]}}}"#,
    );
    assert!(
        !engine
            .store()
            .positions()
            .iter()
            .any(|position| position.symbol == "ETH-PERP")
    );
    let reads = reads_within_second(&mut engine, actions);
    assert!(
        reads
            .iter()
            .any(|request| kind(request) == "clearinghouseState"),
        "a confirmed fill must not wait indefinitely for its private stream to publish the position"
    );
    assert_eq!(
        reads.len(),
        1,
        "only the affected position scope is read, with no write resend"
    );
    reply(&mut engine, &reads[0], 200, &filled_state());
    assert_eq!(
        engine
            .store()
            .positions()
            .iter()
            .find(|position| position.symbol == "ETH-PERP")
            .unwrap()
            .contracts,
        0.2
    );
    let frame = engine
        .take_frame(3_001.)
        .expect("the reconciled position must reach the next frame");
    assert!(
        safe_cex_engine::wire::sections(frame)
            .any(|(kind, _)| kind == safe_cex_engine::wire::section::POSITIONS)
    );
}

fn filled_state() -> String {
    let mut state: serde_json::Value = serde_json::from_str(STATE).unwrap();
    let mut eth = state["assetPositions"][0].clone();
    eth["position"]["coin"] = "ETH".into();
    eth["position"]["szi"] = "0.2".into();
    state["assetPositions"].as_array_mut().unwrap().push(eth);
    state["time"] = 2350.into();
    state.to_string()
}

#[test]
fn an_ack_fill_shares_the_existing_hip3_fill_refresh_timer() {
    let (mut engine, socket) = ready_with_dex("disabled", true);
    message(
        &mut engine,
        socket,
        1900.,
        r#"{"channel":"userFills","data":{"fills":[{"coin":"xyz:FOO","side":"B","px":"50","sz":"1","time":1900,"tid":93,"oid":93,"dir":"Open Long","startPosition":"0"}]}}"#,
    );
    let mut actions = prepared_now(&mut engine);
    actions.extend(acknowledge(&mut engine, vec![order("ETH-PERP", "market")], 94, 2000., r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":94,"totalSz":"0.2","avgPx":"200"}}]}}}"#));
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms == 2250.))
            .count(),
        1
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::SetTimer { at_ms, .. } if *at_ms == 2350.))
    );
    let reads = reads_within_second(&mut engine, actions);
    let mut dexes = reads
        .iter()
        .filter(|request| kind(request) == "clearinghouseState")
        .map(|request| {
            let body: serde_json::Value =
                serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            body["dex"].as_str().unwrap_or("").to_owned()
        })
        .collect::<Vec<_>>();
    dexes.sort();
    assert_eq!(dexes, ["", "xyz"]);
    assert!(
        reads
            .iter()
            .all(|request| !request.url.ends_with("/exchange"))
    );
}

#[test]
fn a_failed_reconciliation_does_not_invent_the_acknowledged_position_or_resend() {
    let (mut engine, actions) = place(
        "market",
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":93,"totalSz":"0.2","avgPx":"200"}}]}}}"#,
    );
    let reads = reads_within_second(&mut engine, actions);
    assert_eq!(reads.len(), 1);
    let before = engine.store().positions().to_vec();
    reply(&mut engine, &reads[0], 503, "unavailable");
    assert_eq!(engine.store().positions(), before);
    assert!(!prepared_now(&mut engine).iter().any(
        |action| matches!(action, Action::Http(request) if request.url.ends_with("/exchange"))
    ));
}

#[test]
fn a_newer_private_snapshot_wins_over_the_post_fill_read() {
    let (engine, socket) = ready("disabled");
    let (mut engine, actions) = place_orders(
        engine,
        vec![order("ETH-PERP", "market")],
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"filled":{"oid":93,"totalSz":"0.2","avgPx":"200"}}]}}}"#,
    );
    let reads = reads_within_second(&mut engine, actions);
    assert_eq!(reads.len(), 1);
    let mut newer: serde_json::Value = serde_json::from_str(&filled_state()).unwrap();
    newer["assetPositions"][1]["position"]["szi"] = "0.5".into();
    newer["time"] = 2600.into();
    message(&mut engine, socket, 2600., &serde_json::json!({"channel":"clearinghouseState", "data":{"dex":"", "clearinghouseState":newer}}).to_string());
    reply(&mut engine, &reads[0], 200, &filled_state());
    assert_eq!(
        engine
            .store()
            .positions()
            .iter()
            .find(|position| position.symbol == "ETH-PERP")
            .unwrap()
            .contracts,
        0.5
    );
}
