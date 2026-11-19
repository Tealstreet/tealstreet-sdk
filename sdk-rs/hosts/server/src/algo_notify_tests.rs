use super::*;
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Order, OrderStatus, OrderType};
use serde_json::json;

fn tap(delegated: bool, held: bool) -> (Rc<Tap>, HostNotifier) {
    let notifier = HostNotifier::unstarted();
    let notify = AlgoNotify::new(
        notifier.clone(),
        Scope::new("auth0|u1", "bybit", "synthetic-main"),
        Arc::new(move || delegated),
        Arc::new(AtomicBool::new(held)),
    );
    (Rc::new(Tap::new(notify)), notifier)
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
        "disposal stops belong to delegated.shutdown"
    );
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
            &Scope::new("auth0|u1", "bybit", "synthetic-main").account_digest,
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
