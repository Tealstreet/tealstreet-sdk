//! Frame-fed Lighter: the page engine loads only the public market catalogue,
//! takes its account rows and account index from a data source's frames, and
//! sends each write as its signed transaction alone (plus the one nextNonce
//! seed its own credential needs before its first write).
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_lighter::Lighter;
use support::*;

const ACCOUNT_INDEX: u64 = 718_895;

fn page_engine() -> (Engine<Lighter>, Timers) {
    let mut e = engine(options());
    e.set_frame_fed().unwrap();
    e.start(NOW);
    let mut timers = Timers::new();
    let mut paths = Vec::new();
    for _ in 0..10 {
        let actions = drain(&mut e, &mut timers);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::Http(r) => {
                    assert!(
                        r.url.ends_with("/api/v1/orderBookDetails"),
                        "frame-fed start read {}",
                        r.url
                    );
                    paths.push(r.url.clone());
                    reply(&mut e, &r, 200, META);
                }
                Action::WsConnect { .. } => panic!("frame-fed start opened a socket"),
                _ => {}
            }
        }
    }
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert!(e.store().loaded().markets);
    (e, timers)
}

/// Every action until `until`, timers included; asserts none opens a socket.
fn run(e: &mut Engine<Lighter>, timers: &mut Timers, until: f64) -> Vec<Action> {
    let actions = advance(e, timers, until);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "a frame-fed engine opens no socket"
    );
    actions
}

fn http(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect()
}

fn terminal(actions: &[Action], command: u32) -> Option<Notice> {
    actions.iter().find_map(|a| match a {
        Action::Notice(
            n @ (Notice::CommandDone { command: c, .. } | Notice::CommandFailed { command: c, .. }),
        ) if *c == command => Some(n.clone()),
        _ => None,
    })
}

/// A data source's snapshot: resting order 42 on ETH and Lighter's VENUE_STATE
/// (version 1: account 718895, tier plus).
fn source_frame() -> Vec<u8> {
    let mut source = safe_cex_engine::Store::default();
    source.set_orders(vec![Order {
        id: "42".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "ETH".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 90.0,
        amount: 2.0,
        filled: 0.0,
        remaining: 2.0,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }]);
    let mut state = vec![1u8, 1];
    state.extend_from_slice(&ACCOUNT_INDEX.to_le_bytes());
    state.push(3);
    let mut writer = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.0);
    source.write(&source.account_snapshot(), &mut writer);
    writer.venue_state(&state);
    writer.finish()
}

fn fed() -> (Engine<Lighter>, Timers) {
    let (mut e, mut timers) = page_engine();
    e.apply_frame(NOW, &source_frame(), true).unwrap();
    assert!(e.store().order("42").is_some());
    assert!(http(&run(&mut e, &mut timers, NOW)).is_empty());
    (e, timers)
}

fn limit(price: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!(
        {"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":price}
    ))
    .unwrap()
}

fn place(e: &mut Engine<Lighter>, command: u32, orders: Vec<PlaceOrderOpts>) {
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders { command, orders }),
    );
}

fn seed_nonce(e: &mut Engine<Lighter>, timers: &mut Timers) -> Vec<Action> {
    let sent = http(&run(e, timers, NOW));
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        sent[0]
            .url
            .ends_with("/api/v1/nextNonce?account_index=718895&api_key_index=1"),
        "{}",
        sent[0].url
    );
    reply(e, &sent[0], 200, r#"{"code":200,"nonce":10}"#);
    run(e, timers, NOW)
}

fn ack(e: &mut Engine<Lighter>, timers: &mut Timers, r: &HttpRequest) -> Vec<Action> {
    reply(e, r, 200, r#"{"code":200,"tx_hash":"transaction-hash"}"#);
    run(e, timers, NOW)
}

#[test]
fn frame_fed_start_reads_only_the_market_catalogue_and_never_polls() {
    let (mut e, mut timers) = page_engine();
    let later = http(&run(&mut e, &mut timers, NOW + 300_000.0));
    assert!(
        later.is_empty(),
        "no ticker seed, poll or account read: {later:?}"
    );
    assert!(timers.is_empty(), "no poll or keepalive timer: {timers:?}");
}

#[test]
fn an_ingested_account_places_with_no_account_tier_or_approval_read() {
    let (mut e, mut timers) = fed();
    place(&mut e, 1, vec![limit(90.0)]);
    let after_seed = seed_nonce(&mut e, &mut timers);
    let sent = http(&after_seed);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.ends_with("/api/v1/sendTx"), "{}", sent[0].url);
    let done = ack(&mut e, &mut timers, &sent[0]);
    assert!(
        matches!(terminal(&done, 1), Some(Notice::CommandDone { .. })),
        "{done:?}"
    );
    assert!(http(&done).is_empty(), "no post-write read: {done:?}");

    // A seeded counter: the next placement is exactly its one write.
    place(&mut e, 2, vec![limit(89.0)]);
    let sent = http(&run(&mut e, &mut timers, NOW));
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.ends_with("/api/v1/sendTx"), "{}", sent[0].url);
    let done = ack(&mut e, &mut timers, &sent[0]);
    assert!(matches!(
        terminal(&done, 2),
        Some(Notice::CommandDone { .. })
    ));
    assert!(http(&run(&mut e, &mut timers, NOW + 300_000.0)).is_empty());
}

#[test]
fn a_cancel_of_an_ingested_order_is_its_one_write() {
    let (mut e, mut timers) = fed();
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 3,
            ids: vec!["42".into()],
        }),
    );
    let after_seed = seed_nonce(&mut e, &mut timers);
    let sent = http(&after_seed);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(sent[0].url.ends_with("/api/v1/sendTx"));
    let body = sent[0].body.as_deref().unwrap();
    assert!(body.contains("tx_type=15"), "a cancel transaction: {body}");
    let done = ack(&mut e, &mut timers, &sent[0]);
    assert!(
        matches!(terminal(&done, 3), Some(Notice::CommandDone { .. })),
        "{done:?}"
    );
    assert!(http(&done).is_empty(), "no reconcile read: {done:?}");
}

#[test]
fn an_unknown_placement_is_never_re_sent_or_read_back() {
    let (mut e, mut timers) = fed();
    place(&mut e, 4, vec![limit(90.0)]);
    let sent = http(&seed_nonce(&mut e, &mut timers));
    assert_eq!(sent.len(), 1, "{sent:?}");
    e.handle(
        NOW,
        Event::HttpFailed {
            request: sent[0].id,
            error: "connection reset",
        },
    );
    let out = run(&mut e, &mut timers, NOW + 300_000.0);
    assert!(
        matches!(terminal(&out, 4), Some(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::Unknown)),
        "{out:?}"
    );
    assert!(http(&out).is_empty(), "no re-send or read: {out:?}");
}

#[test]
fn a_grouped_placement_settles_unknown_at_once_without_a_stream() {
    let (mut e, mut timers) = fed();
    let grouped: PlaceOrderOpts = serde_json::from_value(serde_json::json!(
        {"symbol":"ETH","type":"limit","side":"buy","amount":0.2,"price":90,"stopLoss":80,"takeProfit":100}
    ))
    .unwrap();
    place(&mut e, 5, vec![grouped]);
    let sent = http(&seed_nonce(&mut e, &mut timers));
    assert_eq!(sent.len(), 1, "one grouped transaction: {sent:?}");
    assert!(sent[0].body.as_deref().unwrap().contains("tx_type=28"));
    let out = ack(&mut e, &mut timers, &sent[0]);
    assert!(
        matches!(terminal(&out, 5), Some(Notice::CommandFailed { error, .. })
            if error.outcome == Some(Outcome::Unknown) && error.message.contains("cannot confirm its legs")),
        "{out:?}"
    );
    let later = run(&mut e, &mut timers, NOW + 300_000.0);
    assert!(http(&later).is_empty(), "no re-send or read: {later:?}");
}

#[test]
fn a_reduce_only_limit_without_an_ingested_position_is_refused_without_a_read() {
    let (mut e, mut timers) = fed();
    let mut order = limit(110.0);
    order.side = OrderSide::Sell;
    order.reduce_only = Some(true);
    place(&mut e, 6, vec![order]);
    let out = run(&mut e, &mut timers, NOW);
    assert!(
        matches!(terminal(&out, 6), Some(Notice::CommandFailed { error, .. })
            if error.outcome == Some(Outcome::NotSent) && error.message.contains("No open position")),
        "{out:?}"
    );
    assert!(http(&out).is_empty(), "{out:?}");
}

#[test]
fn a_write_before_the_account_index_arrives_is_not_sent() {
    let (mut e, mut timers) = page_engine();
    place(&mut e, 7, vec![limit(90.0)]);
    let out = run(&mut e, &mut timers, NOW);
    assert!(
        matches!(terminal(&out, 7), Some(Notice::CommandFailed { error, .. })
            if error.outcome == Some(Outcome::NotSent) && error.message.contains("not loaded")),
        "{out:?}"
    );
    assert!(http(&out).is_empty(), "{out:?}");
}

#[test]
fn a_local_source_exports_its_resolved_account_index() {
    let (mut source, _) = ready();
    let frame = source.snapshot_frame(NOW).to_vec();
    let (mut e, mut timers) = page_engine();
    e.apply_frame(NOW, &frame, true).unwrap();
    assert!(e.store().order("42").is_some());
    e.handle(
        NOW,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["42".into()],
        }),
    );
    seed_nonce(&mut e, &mut timers);
}
