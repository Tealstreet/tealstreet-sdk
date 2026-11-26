use super::*;
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Order, OrderStatus, OrderType};
use serde_json::json;

/// The orders the fill tests below placed through this account (AGT-196).
const PLACED_HERE: [&str; 8] = [
    "order-secret-1",
    "order-2",
    "o",
    "ioc-removed",
    "ioc-kept",
    "kept-closed",
    "seen-later",
    "vanished",
];

fn tap(delegated: bool, held: bool) -> (Rc<Tap>, HostNotifier) {
    let placements = Placements::default();
    for id in PLACED_HERE {
        placements.lock().unwrap().record("BTCUSDT", id);
    }
    tap_for("auth0|u1", delegated, held, placements)
}

fn tap_for(
    user: &str,
    delegated: bool,
    held: bool,
    placements: Placements,
) -> (Rc<Tap>, HostNotifier) {
    let notifier = HostNotifier::unstarted();
    let notify = AlgoNotify::new(
        notifier.clone(),
        Scope::new(user, "bybit", "synthetic-main", "venue-account"),
        Arc::new(move || delegated),
        Arc::new(AtomicBool::new(held)),
    );
    (Rc::new(Tap::new(notify, placements)), notifier)
}

fn sent(notifier: &HostNotifier) -> Vec<Value> {
    notifier
        .queued()
        .iter()
        .map(|notification| serde_json::to_value(notification).unwrap())
        .collect()
}

fn fill(order_id: &str, amount: f64) -> Fill {
    Fill {
        id: Some(format!("exec-{order_id}")),
        order_id: order_id.into(),
        symbol: "BTCUSDT".into(),
        side: OrderSide::Buy,
        price: 65_432.1,
        amount,
    }
}

fn order(id: &str, amount: f64, filled: f64) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 65_432.1,
        amount,
        filled,
        remaining: amount - filled,
        reduce_only: false,
        close: None,
        client_order_id: Some("client-secret".into()),
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn settled(id: &str, amount: f64, filled: f64, status: OrderStatus) -> Order {
    Order {
        status,
        remaining: 0.0,
        ..order(id, amount, filled)
    }
}

fn store(orders: Vec<Order>) -> AlgoStore {
    AlgoStore {
        venue: "bybit".into(),
        orders,
        ..AlgoStore::default()
    }
}

#[tokio::test(start_paused = true)]
async fn fills_aggregate_per_order_after_a_quiet_window_and_report_final_state() {
    let (tap, notifier) = tap(true, false);
    // Recorded after the frame applied; the venue kept the closed row.
    tap.record_fills(
        &[fill("order-secret-1", 0.4)],
        &store(vec![order("order-secret-1", 1.0, 0.4)]),
    );
    tokio::time::advance(Duration::from_millis(1500)).await;
    tap.record_fills(
        &[fill("order-secret-1", 0.6)],
        &store(vec![settled(
            "order-secret-1",
            1.0,
            1.0,
            OrderStatus::Closed,
        )]),
    );
    tap.record_fills(
        &[fill("order-2", 0.1)],
        &store(vec![order("order-2", 1.0, 0.1)]),
    );
    tokio::time::advance(Duration::from_millis(1999)).await;
    tap.flush_fills(&store(vec![order("order-2", 1.0, 0.1)]), false);
    assert!(
        sent(&notifier).is_empty(),
        "each new fill restarts the order's window"
    );
    assert_eq!(
        tap.next_fill_due(),
        Some(Instant::now() + Duration::from_millis(1))
    );
    tokio::time::advance(Duration::from_millis(1)).await;
    tap.flush_fills(&store(vec![order("order-2", 1.0, 0.1)]), false);
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 2);
    let filled = queued
        .iter()
        .find(|n| n["fields"]["verb"] == "filled")
        .unwrap();
    assert_eq!(
        filled["fields"],
        json!({"symbol": "BTCUSDT", "side": "buy", "verb": "filled"})
    );
    assert_eq!(filled["kind"], "fill");
    assert_eq!(filled["severity"], "info");
    assert!(filled["collapseKey"].as_str().unwrap().starts_with("fill:"));
    let partial = queued
        .iter()
        .find(|n| n["fields"]["verb"] == "partiallyFilled")
        .unwrap();
    assert_ne!(
        partial["collapseKey"], filled["collapseKey"],
        "collapse is per order"
    );
    let text = serde_json::to_string(&queued).unwrap();
    for secret in [
        "order-secret-1",
        "order-2",
        "exec-",
        "client-secret",
        "65432",
        "0.6",
    ] {
        assert!(!text.contains(secret), "{secret} leaked into {text}");
    }
    assert_eq!(tap.next_fill_due(), None);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_partial_order_is_partially_filled_and_ids_are_stable_per_state() {
    let (tap, notifier) = tap(true, true);
    tap.record_fills(&[fill("o", 0.3)], &store(vec![order("o", 1.0, 0.3)]));
    // The order left the store unfilled (cancelled) before the window closed.
    tap.flush_fills(&store(Vec::new()), true);
    let queued = sent(&notifier);
    assert_eq!(queued[0]["fields"]["verb"], "partiallyFilled");
    assert_eq!(queued[0]["clientPresent"], true);
    let (again, again_notifier) = self::tap(true, true);
    again.record_fills(&[fill("o", 0.3)], &store(vec![order("o", 1.0, 0.3)]));
    again.flush_fills(&store(Vec::new()), true);
    assert_eq!(sent(&again_notifier)[0]["id"], queued[0]["id"]);
}

#[tokio::test(start_paused = true)]
async fn non_delegated_accounts_emit_nothing() {
    let (tap, notifier) = tap(false, false);
    tap.record_fills(&[fill("o", 1.0)], &store(Vec::new()));
    tap.flush_fills(&store(Vec::new()), true);
    tap.observe(
        "chaser:complete",
        &json!({"id":"c","symbol":"BTCUSDT","side":"buy","stopReason":"Failed to place orders after 3 tries"}).to_string(),
    );
    tap.rejected(Algo::Chase, "BTCUSDT", OrderSide::Buy);
    assert!(sent(&notifier).is_empty());
}

fn slice(id: &str, symbol: &str, side: OrderSide) -> safe_cex_chaser::ActiveChaser {
    safe_cex_chaser::ActiveChaser {
        id: id.into(),
        symbol: symbol.into(),
        size: 1.0,
        side,
        max_attempts: 10.0,
        reduce_only: false,
        attempts: 1,
        is_stopped: false,
        stop_reason: String::new(),
        current_order_ids: Vec::new(),
        origin: None,
        twap_id: Some("t1".into()),
    }
}

fn chase_end(id: &str, reason: &str) -> String {
    json!({"id": id, "clientId": "client-secret", "symbol": "ETHUSDT", "side": "sell",
        "reduceOnly": false, "size": 2.5, "filled": 1.25, "stopReason": reason})
    .to_string()
}

#[test]
fn chase_ends_classify_client_stops_finishes_and_errors() {
    let (tap, notifier) = tap(true, false);
    for reason in CLIENT_CHASE_STOPS {
        tap.observe("chaser:complete", &chase_end("c0", reason));
    }
    assert!(
        sent(&notifier).is_empty(),
        "explicit client stops never notify"
    );
    tap.observe("chaser:complete", &chase_end("c1", "Completed"));
    tap.observe(
        "chaser:complete",
        &chase_end("c2", "Failed to place orders after 3 tries"),
    );
    tap.set_chases(Box::new(|| {
        vec![slice("slice", "ETHUSDT", OrderSide::Sell)]
    }));
    tap.observe(
        "chaser:complete",
        &chase_end("slice", "Failed to cancel orders"),
    );
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 2, "a TWAP slice reports through its TWAP");
    assert_eq!(queued[0]["kind"], "algoStopped");
    assert_eq!(queued[0]["severity"], "info");
    assert_eq!(
        queued[0]["fields"],
        json!({"symbol":"ETHUSDT","side":"sell","verb":"finished","algo":"chase","cause":"algoFinished"})
    );
    assert_eq!(queued[1]["severity"], "warning");
    assert_eq!(
        queued[1]["fields"],
        json!({"symbol":"ETHUSDT","side":"sell","verb":"stopped","algo":"chase","cause":"algoError"})
    );
    let text = serde_json::to_string(&queued).unwrap();
    assert!(!text.contains("client-secret") && !text.contains("2.5") && !text.contains("Failed"));
    tap.end();
    tap.observe(
        "chaser:complete",
        &chase_end("c3", "Failed to cancel orders"),
    );
    assert_eq!(
        sent(&notifier).len(),
        2,
        "disposal stops are not ordinary ends"
    );
}

fn chase(id: &str, is_stopped: bool) -> safe_cex_chaser::ActiveChaser {
    safe_cex_chaser::ActiveChaser {
        is_stopped,
        twap_id: None,
        ..slice(id, "ETHUSDT", OrderSide::Sell)
    }
}

fn twap_row(id: &str, symbol: &str, status: &str) -> Value {
    json!({"id": id, "accountName": "synthetic-main", "activeMarket": {"symbol": symbol},
        "symbol": symbol, "side": "buy", "size": 3.0, "status": status, "ordersExecuted": 1})
}

#[test]
fn a_box_stop_reports_each_running_chase_and_twap_as_a_server_restart() {
    let (tap, notifier) = tap(true, false);
    tap.observe("twap:stateUpdate", &twap_state(&[("t-client", "XRPUSDT")]));
    tap.client_stop("t-client");
    tap.end();
    let chases = [
        chase("c-running", false),
        chase("c-cancelling", true),
        slice("slice", "BTCUSDT", OrderSide::Buy),
    ];
    let twaps = [
        twap_row("t1", "BTCUSDT", "running"),
        twap_row("t-paused", "SOLUSDT", "paused"),
        twap_row("t-client", "XRPUSDT", "running"),
    ];
    tap.server_stopped(&chases, &twaps);
    let queued = sent(&notifier);
    assert_eq!(
        queued.len(),
        2,
        "stopping chases, slices, paused TWAPs and client stops are not reported: {queued:?}"
    );
    for notification in &queued {
        assert_eq!(notification["kind"], "algoStopped");
        assert_eq!(notification["severity"], "warning");
    }
    assert_eq!(
        queued[0]["fields"],
        json!({"symbol":"ETHUSDT","side":"sell","verb":"stopped","algo":"chase","cause":"serverRestart"})
    );
    assert_eq!(
        queued[1]["fields"],
        json!({"symbol":"BTCUSDT","side":"buy","verb":"stopped","algo":"twap","cause":"serverRestart"})
    );

    // A chase is never restored, so its box stop and ordinary end share one id and
    // the gateway keeps one. A TWAP is restored under its id: its later real end
    // must not be dropped as a duplicate of the stop, so only the collapse key matches.
    let (ordinary, ordinary_notifier) = self::tap(true, false);
    ordinary.observe(
        "chaser:complete",
        &chase_end("c-running", "Failed to place orders after 3 tries"),
    );
    ordinary.observe("twap:stateUpdate", &twap_state(&[("t1", "BTCUSDT")]));
    ordinary.observe("twap:stateUpdate", &twap_state(&[]));
    let ends = sent(&ordinary_notifier);
    assert_eq!(ends.len(), 2);
    assert_eq!(ends[0]["id"], queued[0]["id"], "chase");
    assert_ne!(ends[1]["id"], queued[1]["id"], "TWAP");
    for (end, stop) in ends.iter().zip(&queued) {
        assert_eq!(end["collapseKey"], stop["collapseKey"]);
    }

    let (cosigned, cosigned_notifier) = self::tap(false, false);
    cosigned.server_stopped(&chases, &twaps);
    assert!(sent(&cosigned_notifier).is_empty());
}

fn twap_state(rows: &[(&str, &str)]) -> String {
    let mut state = serde_json::Map::new();
    for (id, symbol) in rows {
        state.insert(
            (*id).into(),
            json!({"options": {"accountName":"synthetic-main","activeMarket":{"symbol":symbol},
                "size": 3.0, "side":"buy"}, "state": {"status":"running","ordersExecuted":1}}),
        );
    }
    Value::Object(state).to_string()
}

#[test]
fn twap_ends_are_seen_when_they_leave_the_state_map() {
    let (tap, notifier) = tap(true, false);
    tap.observe(
        "twap:stateUpdate",
        &twap_state(&[("t1", "BTCUSDT"), ("t2", "SOLUSDT"), ("t3", "ETHUSDT")]),
    );
    tap.observe(
        "error",
        &json!("TWAP SOLUSDT slice failed: insufficient balance").to_string(),
    );
    tap.client_stop("t3");
    tap.observe("twap:stateUpdate", &twap_state(&[]));
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 2, "a client twap:stop never notifies");
    let by_symbol = |symbol: &str| {
        queued
            .iter()
            .find(|n| n["fields"]["symbol"] == symbol)
            .unwrap()
            .clone()
    };
    assert_eq!(
        by_symbol("BTCUSDT")["fields"],
        json!({"symbol":"BTCUSDT","side":"buy","verb":"finished","algo":"twap","cause":"algoFinished"})
    );
    assert_eq!(by_symbol("SOLUSDT")["fields"]["cause"], "algoError");
    assert!(
        !serde_json::to_string(&queued)
            .unwrap()
            .contains("insufficient")
    );
    tap.observe("twap:stateUpdate", &twap_state(&[("t4", "XRPUSDT")]));
    tap.observe(
        "error",
        &json!("TWAPs stopped: authenticated connection binding changed").to_string(),
    );
    tap.observe("twap:stateUpdate", "{}");
    let rebind = sent(&notifier);
    assert_eq!(rebind.len(), 3);
    assert_eq!(
        rebind[2]["fields"]["cause"], "algoError",
        "an owner rebind is not a finish"
    );
}

struct Venue {
    store: RefCell<AlgoStore>,
    outcome: Cell<Option<Outcome>>,
}

impl AccountPort for Venue {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<Change> {
        tokio::sync::mpsc::unbounded_channel().1
    }
    fn next_command_id(&self) -> CommandId {
        1
    }
    fn command(&self, _: Command) -> CommandFuture {
        let outcome = self.outcome.get();
        Box::pin(async move {
            let mut error = VenueError::new("venue refusal text: balance 123.45");
            error.outcome = outcome;
            Err(error)
        })
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
    fn emit(&self, _: &str, _: String) {}
}

fn place() -> Command {
    Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"sell","amount":0.5,"price":70000.0}))
                .unwrap(),
        ],
    }
}

#[tokio::test(start_paused = true)]
async fn algo_child_rejections_notify_once_per_window_and_other_failures_never() {
    let (tap, notifier) = tap(true, false);
    let venue = Rc::new(Venue {
        store: RefCell::new(store(Vec::new())),
        outcome: Cell::new(Some(Outcome::Rejected)),
    });
    let port = RejectTap {
        port: venue.clone(),
        tap: tap.clone(),
        algo: Algo::Chase,
    };
    let _ = port.command(place()).await;
    let _ = port.command(place()).await;
    venue.outcome.set(Some(Outcome::Unknown));
    tokio::time::advance(REJECT_REPEAT).await;
    let _ = port.command(place()).await;
    venue.outcome.set(Some(Outcome::NotSent));
    let _ = port.command(place()).await;
    let cancel = Command::CancelOrders {
        command: 2,
        ids: vec!["o".into()],
    };
    venue.outcome.set(Some(Outcome::Rejected));
    let _ = port.command(cancel).await;
    let queued = sent(&notifier);
    assert_eq!(
        queued.len(),
        1,
        "one per window; unknown, notSent and cancels never"
    );
    assert_eq!(queued[0]["kind"], "orderRejected");
    assert_eq!(queued[0]["severity"], "warning");
    assert_eq!(
        queued[0]["fields"],
        json!({"symbol":"BTCUSDT","side":"sell","verb":"rejected","algo":"chase","cause":"venueRejected"})
    );
    let _ = port.command(place()).await;
    assert_eq!(sent(&notifier).len(), 2, "a new window notifies again");
    let text = serde_json::to_string(&sent(&notifier)).unwrap();
    assert!(!text.contains("123.45") && !text.contains("70000") && !text.contains("refusal"));
}

#[test]
fn retried_cancel_suffixes_keep_client_stops_silent() {
    let (tap, notifier) = tap(true, false);
    for reason in [
        "User cancelled; retrying the cancel of order o1. Cancel it on the venue to stop now.",
        "User cancelled; order o1 is cancelled, nothing filled",
        "Stopped by nuke; order o1 filled 0.5",
        "Client disconnected while chase was active; order o1 is cancelled after filling 0.2",
        "User cancelled, kept order in book; retrying",
    ] {
        tap.observe("chaser:complete", &chase_end("c", reason));
    }
    assert!(
        sent(&notifier).is_empty(),
        "a retried client cancel is still a client stop"
    );
    tap.observe(
        "chaser:complete",
        &chase_end("done", "Completed; order o2 filled 1"),
    );
    tap.observe(
        "chaser:complete",
        &chase_end("bad", "Failed to cancel orders; order o3 filled 1"),
    );
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 2);
    assert_eq!(queued[0]["fields"]["cause"], "algoFinished");
    assert_eq!(queued[1]["fields"]["cause"], "algoError");
}

#[test]
fn a_refused_or_unknown_client_stop_never_hides_a_later_twap_end() {
    let (tap, notifier) = tap(true, false);
    tap.client_stop("t1");
    tap.observe(
        "twap:stateUpdate",
        &twap_state(&[("t1", "BTCUSDT"), ("t2", "ETHUSDT")]),
    );
    tap.client_stop("t2");
    tap.client_stop_refused("t2");
    tap.observe("twap:stateUpdate", &twap_state(&[]));
    let queued = sent(&notifier);
    assert_eq!(
        queued.len(),
        2,
        "neither an unseen nor a refused stop suppresses the end"
    );
}

#[tokio::test(start_paused = true)]
async fn twap_slice_chase_rejections_report_the_twap() {
    let (tap, notifier) = tap(true, false);
    tap.set_chases(Box::new(|| vec![slice("s1", "BTCUSDT", OrderSide::Sell)]));
    let venue = Rc::new(Venue {
        store: RefCell::new(store(Vec::new())),
        outcome: Cell::new(Some(Outcome::Rejected)),
    });
    let port = RejectTap {
        port: venue,
        tap: tap.clone(),
        algo: Algo::Chase,
    };
    let _ = port.command(place()).await;
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0]["fields"]["algo"], "twap");
}

#[tokio::test(start_paused = true)]
async fn an_ioc_partial_cancelled_in_the_fill_batch_is_never_reported_filled() {
    let (tap, notifier) = tap(true, false);
    // Remainder cancelled in the same batch: removed, or kept as a canceled row.
    tap.record_fills(&[fill("ioc-removed", 0.4)], &store(Vec::new()));
    tap.record_fills(
        &[fill("ioc-kept", 0.4)],
        &store(vec![settled("ioc-kept", 1.0, 0.4, OrderStatus::Canceled)]),
    );
    tap.flush_fills(&store(Vec::new()), true);
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 2);
    assert!(
        queued
            .iter()
            .all(|n| n["fields"]["verb"] == "partiallyFilled"),
        "{queued:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn filled_needs_the_order_record_to_show_its_size_met() {
    let (tap, notifier) = tap(true, false);
    tap.record_fills(
        &[fill("kept-closed", 1.0)],
        &store(vec![settled("kept-closed", 1.0, 1.0, OrderStatus::Closed)]),
    );
    // Proof read at flush time from the current store also counts.
    tap.record_fills(&[fill("seen-later", 1.0)], &store(Vec::new()));
    // A full fill whose row vanished in its own batch is unproven: partial wording.
    tap.record_fills(&[fill("vanished", 1.0)], &store(Vec::new()));
    tap.flush_fills(
        &store(vec![settled("seen-later", 2.0, 2.0, OrderStatus::Closed)]),
        true,
    );
    let verbs: HashMap<String, String> = sent(&notifier)
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap().to_owned(),
                n["fields"]["verb"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let id = |order: &str, key: &str| {
        crate::host_notify::notification_id(
            "auth0|u1",
            &Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account").account_digest,
            Kind::Fill,
            &format!("{order}\0{key}"),
        )
    };
    assert_eq!(verbs[&id("kept-closed", "filled")], "filled");
    assert_eq!(verbs[&id("seen-later", "filled")], "filled");
    assert_eq!(
        verbs[&id("vanished", "partiallyFilled\x001")],
        "partiallyFilled"
    );
}

/// AGT-196: two Tealstreet users connected to ONE venue account each run their
/// own resident, and both private streams carry every fill on that account.
/// Both still report it (the box never withholds); each carries the same
/// user-independent key, and only the placer claims it, so the gateway can
/// withhold the other user's push.
#[tokio::test(start_paused = true)]
async fn a_shared_venue_accounts_fill_carries_one_key_and_only_the_placer_claims_it() {
    let placer_orders = Placements::default();
    placer_orders
        .lock()
        .unwrap()
        .record("BTCUSDT", "shared-order");
    let (placer, placer_sent) = tap_for("auth0|placer", true, false, placer_orders);
    let (other, other_sent) = tap_for("auth0|other", true, false, Placements::default());
    let venue_account = store(vec![order("shared-order", 1.0, 1.0)]);
    for tap in [&placer, &other] {
        tap.record_fills(&[fill("shared-order", 1.0)], &venue_account);
        tokio::time::advance(FILL_DEBOUNCE).await;
        tap.flush_fills(&venue_account, false);
    }
    let (placer_sent, other_sent) = (sent(&placer_sent), sent(&other_sent));
    assert_eq!(
        (placer_sent.len(), other_sent.len()),
        (1, 1),
        "neither is withheld here"
    );
    assert_eq!(placer_sent[0]["placedHere"], true);
    assert_eq!(other_sent[0]["placedHere"], false);
    let key = placer_sent[0]["fillKey"].as_str().unwrap();
    assert_eq!(key.len(), 64);
    assert_eq!(
        other_sent[0]["fillKey"], key,
        "one key per venue order, whoever saw it"
    );
    assert!(!key.contains("shared-order"));
    assert_ne!(
        placer_sent[0]["collapseKey"], other_sent[0]["collapseKey"],
        "collapse stays per account"
    );
}

/// A fill nobody placed through Tealstreet (venue UI, bot, liquidation, ADL) is
/// still reported, unclaimed, so it reaches the user exactly as before.
#[tokio::test(start_paused = true)]
async fn an_unplaced_fill_such_as_a_liquidation_is_still_reported_unclaimed() {
    let (tap, notifier) = tap_for("auth0|u1", true, false, Placements::default());
    tap.record_fills(&[fill("liquidation-1", 1.0)], &store(Vec::new()));
    tap.flush_fills(&store(Vec::new()), true);
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0]["placedHere"], false);
    assert_eq!(queued[0]["kind"], "fill");
}

/// An order placed here is claimed by its client order id (the dispatch
/// receipt), its parent, or its own id once the native host linked it to an
/// algo placed here (an OKX triggered stop files under a new `ordId`).
#[tokio::test(start_paused = true)]
async fn placed_here_covers_client_ids_parents_and_linked_triggered_children() {
    let placements = Placements::default();
    for id in ["client-secret", "entry", "algo9"] {
        placements.lock().unwrap().record("BTCUSDT", id);
    }
    placements
        .lock()
        .unwrap()
        .link("BTCUSDT", "900", &["algo9".into()]);
    let (tap, notifier) = tap_for("auth0|u1", true, false, placements);
    let mut by_client = order("venue-assigned", 1.0, 1.0);
    by_client.client_order_id = Some("client-secret".into());
    let mut leg = order("venue-leg", 1.0, 1.0);
    leg.client_order_id = None;
    leg.parent_id = Some("entry".into());
    let mut foreign = order("foreign", 1.0, 1.0);
    foreign.client_order_id = Some("someone-else".into());
    // The triggered child "900" has no row: it filled in its first frame.
    let rows = store(vec![by_client, leg, foreign]);
    let ids = ["venue-assigned", "venue-leg", "900", "foreign"];
    tap.record_fills(&ids.map(|id| fill(id, 1.0)), &rows);
    tap.flush_fills(&rows, true);
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 4, "every fill is reported");
    let claimed: Vec<bool> = ids
        .iter()
        .map(|id| {
            let key = crate::host_notify::fill_key(
                &Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account"),
                "BTCUSDT",
                id,
            );
            queued
                .iter()
                .find(|n| n["fillKey"] == key.as_str())
                .unwrap()["placedHere"]
                .as_bool()
                .unwrap()
        })
        .collect();
    assert_eq!(claimed, [true, true, true, false]);
}

/// Review B finding 2: Binance USDⓈ-M order ids repeat across symbols. This
/// resident's BTCUSDT order 7 does not claim another login's ETHUSDT order 7,
/// nor lend that order its BTCUSDT row's client id.
#[tokio::test(start_paused = true)]
async fn a_claim_never_crosses_symbols() {
    let placements = Placements::default();
    placements.lock().unwrap().record("BTCUSDT", "7");
    placements
        .lock()
        .unwrap()
        .record("BTCUSDT", "client-secret");
    let (tap, notifier) = tap_for("auth0|u1", true, false, placements);
    let mine = order("7", 1.0, 1.0);
    let mut theirs = order("7", 1.0, 1.0);
    theirs.symbol = "ETHUSDT".into();
    theirs.client_order_id = Some("theirs".into());
    let mut theirs_by_client = order("8", 1.0, 1.0);
    theirs_by_client.symbol = "ETHUSDT".into();
    let rows = store(vec![mine, theirs, theirs_by_client]);
    let on = |symbol: &str, id: &str| Fill {
        symbol: symbol.into(),
        ..fill(id, 1.0)
    };
    let fills = [on("BTCUSDT", "7"), on("ETHUSDT", "7"), on("ETHUSDT", "8")];
    tap.record_fills(&fills, &rows);
    tap.flush_fills(&rows, true);
    let queued = sent(&notifier);
    assert_eq!(queued.len(), 3, "one report per (symbol, order)");
    let scope = Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account");
    let claimed: Vec<bool> = fills
        .iter()
        .map(|fill| {
            let key = crate::host_notify::fill_key(&scope, &fill.symbol, &fill.order_id);
            queued
                .iter()
                .find(|n| n["fillKey"] == key.as_str())
                .unwrap()["placedHere"]
                .as_bool()
                .unwrap()
        })
        .collect();
    assert_eq!(claimed, [true, false, false]);
}

/// Review B (1acdf351b7): Binance spot and portfolio margin key orders by a
/// client id a later order may reuse. Two orders reusing `desk` on one symbol
/// must not share a key, or a claim on the first withholds the second; the key
/// is the venue's own order id from the fill's frame.
#[tokio::test(start_paused = true)]
async fn orders_reusing_one_client_id_get_different_fill_keys() {
    let placements = Placements::default();
    let (tap, notifier) = tap_for("auth0|u1", true, false, placements.clone());
    let scope = Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account");
    let mut keys = Vec::new();
    for native in ["111", "222"] {
        placements
            .lock()
            .unwrap()
            .note_native_id("BTCUSDT", "desk", native, Some("exec-desk"));
        tap.record_fills(&[fill("desk", 1.0)], &store(Vec::new()));
        tap.flush_fills(&store(Vec::new()), true);
        let sent = sent(&notifier);
        let key = sent.last().unwrap()["fillKey"].as_str().unwrap().to_owned();
        assert_eq!(key, crate::host_notify::fill_key(&scope, "BTCUSDT", native));
        keys.push(key);
    }
    assert_ne!(keys[0], keys[1]);
    assert_ne!(
        keys[0],
        crate::host_notify::fill_key(&scope, "BTCUSDT", "desk")
    );
}

/// Review B (10e8ae20c6) item 5: a closed caller id claims by its native id,
/// so this login's late fills of its own `desk` claim and another order
/// reusing `desk` (another native id) does not.
#[tokio::test(start_paused = true)]
async fn a_client_keyed_fill_claims_by_its_native_id() {
    let placements = Placements::default();
    placements.lock().unwrap().record("BTCUSDT", "n-1");
    let (tap, notifier) = tap_for("auth0|u1", true, false, placements.clone());
    let mut claimed = Vec::new();
    for native in ["n-1", "n-2"] {
        placements
            .lock()
            .unwrap()
            .note_native_id("BTCUSDT", "desk", native, Some("exec-desk"));
        tap.record_fills(&[fill("desk", 1.0)], &store(Vec::new()));
        tap.flush_fills(&store(Vec::new()), true);
        claimed.push(
            sent(&notifier).last().unwrap()["placedHere"]
                .as_bool()
                .unwrap(),
        );
    }
    assert_eq!(claimed, [true, false]);
}

/// One `desk` fill with its own trade id.
fn desk_fill(trade: &str) -> Fill {
    Fill {
        id: Some(trade.into()),
        ..fill("desk", 1.0)
    }
}

/// Review A and B (2750605102): A's `desk` (native n-a) fills and B reuses
/// `desk` (native n-b) inside one 2 s debounce, both before any flush. Each
/// fill's native id is resolved from its own trade id when recorded, so they
/// stay two notifications with two keys, and each login claims only its own:
/// neither is withheld from its placer.
#[tokio::test(start_paused = true)]
async fn two_orders_sharing_a_client_id_in_one_debounce_keep_their_own_keys_and_claims() {
    let scope = Scope::new("auth0|u1", "bybit", "synthetic-main", "venue-account");
    let mut claims = Vec::new();
    for (user, own) in [("auth0|a", "n-a"), ("auth0|b", "n-b")] {
        let placements = Placements::default();
        placements.lock().unwrap().record("BTCUSDT", own);
        {
            let mut ledger = placements.lock().unwrap();
            ledger.note_native_id("BTCUSDT", "desk", "n-a", Some("t-a"));
            ledger.note_native_id("BTCUSDT", "desk", "n-b", Some("t-b"));
        }
        let (tap, notifier) = tap_for(user, true, false, placements);
        tap.record_fills(&[desk_fill("t-a"), desk_fill("t-b")], &store(Vec::new()));
        tap.flush_fills(&store(Vec::new()), true);
        let sent = sent(&notifier);
        assert_eq!(sent.len(), 2, "{user}: two orders, two notifications");
        let scope = Scope::new(user, "bybit", "synthetic-main", "venue-account");
        let by_native: Vec<(String, bool)> = ["n-a", "n-b"]
            .iter()
            .map(|native| {
                let key = crate::host_notify::fill_key(&scope, "BTCUSDT", native);
                let n = sent
                    .iter()
                    .find(|n| n["fillKey"] == key.as_str())
                    .unwrap_or_else(|| panic!("{user}: {native}'s own key"));
                ((*native).to_owned(), n["placedHere"].as_bool().unwrap())
            })
            .collect();
        claims.push(by_native);
    }
    assert_eq!(
        claims,
        [
            vec![("n-a".to_owned(), true), ("n-b".to_owned(), false)],
            vec![("n-a".to_owned(), false), ("n-b".to_owned(), true)],
        ]
    );
    assert_eq!(
        crate::host_notify::fill_key(&scope, "BTCUSDT", "n-a"),
        crate::host_notify::fill_key(
            &Scope::new("auth0|b", "bybit", "synthetic-main", "venue-account"),
            "BTCUSDT",
            "n-a"
        ),
        "one key per order across logins"
    );
}

/// A client-keyed fill without its own trade id can only be matched to the
/// latest order under its client id. It is claimed when any native id seen
/// under that client id is this login's: a duplicate push, never a withheld
/// one.
#[tokio::test(start_paused = true)]
async fn a_client_keyed_fill_without_a_trade_id_is_claimed_if_any_order_could_be_ours() {
    let placements = Placements::default();
    {
        let mut ledger = placements.lock().unwrap();
        ledger.record("BTCUSDT", "n-a");
        ledger.note_native_id("BTCUSDT", "desk", "n-a", Some("t-a"));
        ledger.note_native_id("BTCUSDT", "desk", "n-b", Some("t-b"));
    }
    let (tap, notifier) = tap_for("auth0|a", true, false, placements);
    let anonymous = Fill {
        id: None,
        ..fill("desk", 1.0)
    };
    tap.record_fills(&[anonymous], &store(Vec::new()));
    tap.flush_fills(&store(Vec::new()), true);
    assert_eq!(sent(&notifier)[0]["placedHere"], true);
}

/// One fill's key on a tap for `user` connected to `venue_account`.
async fn fill_key_on(user: &str, venue_account: &str, fill: Fill) -> Value {
    let notifier = HostNotifier::unstarted();
    let notify = AlgoNotify::new(
        notifier.clone(),
        Scope::new(user, "bybit", "synthetic-main", venue_account),
        Arc::new(|| true),
        Arc::new(AtomicBool::new(false)),
    );
    let tap = Tap::new(notify, Placements::default());
    tap.record_fills(std::slice::from_ref(&fill), &store(Vec::new()));
    tap.flush_fills(&store(Vec::new()), true);
    sent(&notifier)[0]["fillKey"].clone()
}

/// AGT-196 review: the key names the VENUE account, so every Tealstreet user
/// connected to it computes the same one, and unrelated accounts never collide
/// on per-symbol order ids (Binance USDⓈ-M) or caller-chosen ids (PM).
#[tokio::test(start_paused = true)]
async fn a_fill_key_is_shared_on_one_venue_account_and_distinct_across_accounts() {
    let on = |user: &'static str, account: &'static str, symbol: &'static str| {
        let mut fill = fill("7", 1.0);
        fill.symbol = symbol.into();
        fill_key_on(user, account, fill)
    };
    let shared = on("auth0|a", "venue-account-1", "BTCUSDT").await;
    assert_eq!(
        on("auth0|b", "venue-account-1", "BTCUSDT").await,
        shared,
        "two users, one venue account"
    );
    assert_ne!(
        on("auth0|c", "venue-account-2", "BTCUSDT").await,
        shared,
        "another venue account, same order id and symbol"
    );
    assert_ne!(
        on("auth0|a", "venue-account-1", "ETHUSDT").await,
        shared,
        "the same id on another symbol is another order"
    );
}
