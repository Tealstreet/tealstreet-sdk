//! Original controls for the trading write funnel
//! (`docs/plans/RUST_BLOFIN_WRITE_FUNNEL.md`, W1): every trading command owns a
//! Preflight root and one streamed serial Plan; writes are Primary managed
//! Mutations, the Verify GET is a managed Read, and the journal projects outcomes.
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Notice, Outcome, RequestId, TimerId, VenueError,
};
use std::collections::HashMap;
use support::*;

const ACCEPTED_CANCEL: &str = r#"{"code":"0","data":[{"orderId":"12","code":"0"}]}"#;
const GONE_CANCEL: &str =
    r#"{"code":"0","data":[{"orderId":"12","code":"1000","msg":"The order has been canceled"}]}"#;
const CANCELED_DETAIL: &str = r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#;
const LEVERAGE_OK: &str = r#"{"code":"0","data":{"leverage":"30"}}"#;
const LEVERAGE_REFUSED: &str = r#"{"code":"110006","msg":"Leverage cannot be adjusted"}"#;

/// Releases pending managed work at `h.now`, recording each Prepare's routing
/// intent; legacy requests pass through with none.
fn release(h: &mut Harness) -> HashMap<RequestId, RoutingIntent> {
    let now = h.now;
    h.e.prepare_dispatch(now);
    let mut queue: std::collections::VecDeque<_> = h.e.drain_actions().collect();
    let mut intents = HashMap::new();
    let mut actions = Vec::new();
    while let Some(action) = queue.pop_front() {
        let Action::ManagedHttp(managed) = action else {
            actions.push(action);
            continue;
        };
        match *managed {
            HttpAction::Prepare {
                preparation,
                intent,
                request,
            } => {
                if h.e.http_preparation_pending(preparation) {
                    intents.insert(request.id, intent);
                    h.e.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
            }
            HttpAction::Bind {
                permit, request, ..
            } => {
                if let Ok(Claim::Released(_)) = h.e.claim_http(now, permit) {
                    actions.push(Action::Http(request));
                }
            }
            HttpAction::Cancel { .. } => {}
            HttpAction::BindGroup { .. } => panic!("W1 declares no Mutation barriers"),
        }
        for action in h.e.drain_actions().collect::<Vec<_>>().into_iter().rev() {
            queue.push_front(action);
        }
    }
    h.collect_actions(actions);
    intents
}

fn event(h: &mut Harness, now: f64, event: Event<'_>) -> HashMap<RequestId, RoutingIntent> {
    h.now = h.now.max(now);
    let now = h.now;
    h.e.handle(now, event);
    release(h)
}

fn command(h: &mut Harness, command: Command) -> HashMap<RequestId, RoutingIntent> {
    let now = h.now.max(1.0);
    event(h, now, Event::Command(command))
}

fn reply(
    h: &mut Harness,
    path: &str,
    status: u16,
    body: &str,
) -> HashMap<RequestId, RoutingIntent> {
    let request = h.take(path);
    answer(h, &request, status, body)
}

fn answer(
    h: &mut Harness,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> HashMap<RequestId, RoutingIntent> {
    let now = h.now;
    event(
        h,
        now,
        Event::HttpResponse {
            request: request.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body,
            },
        },
    )
}

/// The one pending request for `path`, which must be a managed preparation.
fn managed(
    h: &Harness,
    intents: &HashMap<RequestId, RoutingIntent>,
    path: &str,
) -> (HttpRequest, RoutingIntent) {
    let found: Vec<_> = h.pending.iter().filter(|r| r.url.contains(path)).collect();
    assert_eq!(found.len(), 1, "one pending {path}: {:?}", h.pending);
    let request = found[0].clone();
    let intent = *intents
        .get(&request.id)
        .unwrap_or_else(|| panic!("{path} must be a managed preparation, not legacy HTTP"));
    (request, intent)
}

fn count(h: &Harness, path: &str) -> usize {
    h.pending.iter().filter(|r| r.url.contains(path)).count()
}

/// Fires every armed timer due at `at`, in time order; returns the intents of
/// whatever they released.
fn fire_until(h: &mut Harness, at: f64) -> HashMap<RequestId, RoutingIntent> {
    let mut intents = HashMap::new();
    let mut fired = std::collections::HashSet::new();
    loop {
        let mut armed: HashMap<TimerId, f64> = HashMap::new();
        for action in &h.actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    armed.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    armed.remove(timer);
                }
                _ => {}
            }
        }
        let next = armed
            .into_iter()
            .filter(|(timer, due)| *due <= at && *timer != h.tick && !fired.contains(timer))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((timer, due)) = next else {
            h.now = h.now.max(at);
            return intents;
        };
        fired.insert(timer);
        intents.extend(event(h, due, Event::TimerFired { timer }));
    }
}

fn terminals(h: &Harness, id: u32) -> Vec<Result<String, VenueError>> {
    let mut failures = h.failures.iter();
    h.actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone { command, result }) => {
                Some((*command, Ok(result.clone())))
            }
            Action::Notice(Notice::CommandFailed { command, .. }) => {
                Some((*command, Err(failures.next().expect("failure").clone())))
            }
            _ => None,
        })
        .filter(|(command, _)| *command == id)
        .map(|(_, terminal)| terminal)
        .collect()
}

fn failure(h: &Harness, id: u32) -> VenueError {
    let terminals = terminals(h, id);
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    terminals[0].clone().expect_err("a failure terminal")
}

fn succeeded(h: &Harness, id: u32) -> String {
    let terminals = terminals(h, id);
    assert_eq!(terminals.len(), 1, "exactly one terminal: {terminals:?}");
    terminals[0].clone().expect("a success terminal")
}

fn limit(amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "buy", "amount": amount, "price": 60000
    }))
    .unwrap()
}

fn place(id: u32, amount: f64) -> Command {
    Command::PlaceOrders {
        command: id,
        orders: vec![limit(amount)],
    }
}

fn cancel(id: u32) -> Command {
    Command::CancelOrders {
        command: id,
        ids: vec!["12".into()],
    }
}

fn amend(id: u32) -> Command {
    Command::AmendOrder {
        command: id,
        id: "12".into(),
        price: Some(61000.0),
        amount: None,
    }
}

fn margin(id: u32, isolated: bool) -> Command {
    Command::SetMarginType {
        command: id,
        symbol: "BTCUSDT".into(),
        isolated,
    }
}

fn mode(id: u32, hedged: bool) -> Command {
    Command::SetPositionMode {
        command: id,
        hedged,
        symbol: None,
    }
}

fn leverage(id: u32) -> Command {
    Command::SetLeverage {
        command: id,
        symbol: "BTCUSDT".into(),
        leverage: 30.0,
        side: None,
    }
}

fn seeded() -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h
}

fn seed_order(h: &mut Harness, id: &str, extra: &str) {
    h.ws(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"{extra}}}]}}"#));
}

/// A flat account switched to isolated margin and hedge mode.
fn hedged_isolated() -> Harness {
    let mut h = Harness::new();
    h.open_private();
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
    );
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    command(&mut h, margin(80, true));
    reply(
        &mut h,
        "set-margin-mode",
        200,
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    // Settle the margin commit's account refresh, so a join refresh can issue.
    reply(
        &mut h,
        "account/positions",
        200,
        r#"{"code":"0","data":[]}"#,
    );
    if count(&h, "balances") == 1 {
        reply(
            &mut h,
            "balances",
            200,
            r#"{"code":"0","data":[{"available":"1000","frozen":"0"}]}"#,
        );
    }
    command(&mut h, mode(81, true));
    reply(
        &mut h,
        "set-position-mode",
        200,
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    h
}

fn leg(h: &Harness, side: &str) -> usize {
    h.pending
        .iter()
        .filter(|r| r.url.ends_with("set-leverage") && r.body.as_deref().unwrap().contains(side))
        .count()
}

// ---- Managed requests ----

#[test]
fn a_placement_batch_is_a_primary_managed_mutation() {
    let mut h = seeded();
    let intents = command(&mut h, place(7, 1.0));
    let (request, intent) = managed(&h, &intents, "/api/v1/trade/batch-orders");
    assert_eq!(intent, RoutingIntent::Primary);
    assert_eq!(request.method, safe_cex_engine::Method::Post);
    let rows: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(rows[0]["instId"], "BTC-USDT");
    assert_eq!(rows[0]["size"], "1");
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    assert_eq!(succeeded(&h, 7), r#"["1"]"#);
}

#[test]
fn cancels_and_settings_are_primary_managed_mutations() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    let intents = command(&mut h, cancel(7));
    assert_eq!(
        managed(&h, &intents, "cancel-batch-orders").1,
        RoutingIntent::Primary
    );
    reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    succeeded(&h, 7);
    for (id, setting, path, body) in [
        (
            8,
            margin(8, true),
            "set-margin-mode",
            r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
        ),
        (
            9,
            mode(9, true),
            "set-position-mode",
            r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
        ),
    ] {
        let intents = command(&mut h, setting);
        assert_eq!(managed(&h, &intents, path).1, RoutingIntent::Primary);
        reply(&mut h, path, 200, body);
        succeeded(&h, id);
    }
}

#[test]
fn verify_is_a_secondary_managed_read_after_one_native_pause() {
    let mut h = Harness::with_secondary();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    let intents = command(&mut h, amend(7));
    assert_eq!(
        managed(&h, &intents, "cancel-batch-orders").1,
        RoutingIntent::Primary
    );
    reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    // The fact is recorded and the pause armed; nothing is sent before it ends.
    assert!(h.pending.is_empty(), "no send before the pause");
    assert!(terminals(&h, 7).is_empty());
    let at = h.now + 350.0;
    let intents = fire_until(&mut h, at - 1.0);
    assert!(intents.is_empty() && h.pending.is_empty());
    let intents = fire_until(&mut h, at);
    assert_eq!(h.pending.len(), 1, "exactly one send after the pause");
    let (request, intent) = managed(&h, &intents, "order-detail");
    assert_eq!(intent, RoutingIntent::Secondary);
    assert_eq!(request.method, safe_cex_engine::Method::Get);
    let intents = reply(&mut h, "order-detail", 200, CANCELED_DETAIL);
    assert_eq!(
        managed(&h, &intents, "batch-orders").1,
        RoutingIntent::Primary
    );
}

// ---- Entry and FIFO ----

#[test]
fn an_entry_refusal_is_not_sent_without_http() {
    let mut h = seeded();
    let mut opts = limit(1.0);
    opts.bybit_slippage_tolerance_ticks = Some(2.0);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 7,
            orders: vec![opts],
        },
    );
    assert!(h.pending.is_empty());
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::NotSent));
}

#[test]
fn queued_commands_wait_without_io_and_run_in_fifo_order() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    command(&mut h, place(7, 1.0));
    command(&mut h, cancel(8));
    command(&mut h, margin(9, true));
    assert_eq!(h.pending.len(), 1, "only the active job sends");
    let intents = reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    managed(&h, &intents, "cancel-batch-orders");
    assert_eq!(count(&h, "set-margin-mode"), 0);
    let intents = reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    managed(&h, &intents, "set-margin-mode");
}

#[test]
fn a_dequeue_replan_refusal_is_not_sent_and_wakes_the_next() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    command(&mut h, place(7, 1.0));
    command(&mut h, amend(8));
    command(&mut h, margin(9, true));
    // The amend target is gone by the time the amend is planned.
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"canceled"}]}"#);
    let intents = reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    assert_eq!(failure(&h, 8).outcome, Some(Outcome::NotSent));
    assert_eq!(count(&h, "cancel-batch-orders"), 0);
    managed(&h, &intents, "set-margin-mode");
}

#[test]
fn an_expired_parked_command_fails_once_and_never_sends() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    command(&mut h, place(7, 1.0));
    h.now = 1.0;
    h.e.handle_command(1.0, cancel(8), Some(100)).unwrap();
    release(&mut h);
    command(&mut h, margin(9, true));
    // The caller's wait lapses while the command is parked behind job 7.
    fire_until(&mut h, 200.0);
    let intents = reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    assert_eq!(
        count(&h, "cancel-batch-orders"),
        0,
        "an expired command never sends"
    );
    assert_eq!(terminals(&h, 8).len(), 1, "the expired command fails once");
    assert_eq!(failure(&h, 8).outcome, Some(Outcome::NotSent));
    managed(&h, &intents, "set-margin-mode");
    // No engine wait binds a WriteAfterReads Preflight: nothing later adds a second.
    fire_until(&mut h, 100_000.0);
    assert_eq!(terminals(&h, 8).len(), 1);
}

// ---- D-a: documented codes ----

#[test]
fn a_documented_row_refusal_is_rejected_with_its_code() {
    let mut h = seeded();
    command(&mut h, place(7, 1.0));
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"","code":"102015","msg":"Order price is not within the price limit"}]}"#,
    );
    let error = failure(&h, 7);
    assert_eq!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(error.venue_code.as_deref(), Some("102015"));
    assert!(h.e.store().orders().is_empty());
}

#[test]
fn a_documented_setting_refusal_is_rejected() {
    let mut h = seeded();
    command(&mut h, margin(7, true));
    reply(
        &mut h,
        "set-margin-mode",
        200,
        r#"{"code":"110019","msg":"Margin mode cannot be adjusted"}"#,
    );
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Rejected));
}

#[test]
fn an_unlisted_row_code_is_unknown() {
    let mut h = seeded();
    command(&mut h, place(7, 1.0));
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"","code":"999999","msg":"new"}]}"#,
    );
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
}

#[test]
fn a_cancel_scoped_gone_code_on_a_placement_is_unknown() {
    let mut h = seeded();
    command(&mut h, place(7, 1.0));
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"","code":"1000","msg":"The order has been canceled"}]}"#,
    );
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
}

#[test]
fn a_row_count_mismatch_is_unknown() {
    let mut h = seeded();
    let intents = command(&mut h, place(7, 1.0));
    managed(&h, &intents, "batch-orders");
    reply(&mut h, "batch-orders", 200, r#"{"code":"0","data":[]}"#);
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
}

// ---- D-b, D-c, D-d ----

#[test]
fn a_failed_verify_after_an_accepted_cancel_is_never_not_sent() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    let intents = command(&mut h, amend(7));
    managed(&h, &intents, "cancel-batch-orders");
    reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    let at = h.now + 350.0;
    fire_until(&mut h, at);
    let request = h.take("order-detail");
    let now = h.now;
    event(
        &mut h,
        now,
        Event::HttpFailed {
            request: request.id,
            error: "connection reset",
        },
    );
    let error = failure(&h, 7);
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert_ne!(error.outcome, Some(Outcome::Rejected));
    assert_eq!(count(&h, "batch-orders"), 0);
}

#[test]
fn a_plain_cancel_gone_is_satisfied_fences_and_reconciles() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    seed_order(&mut h, "13", "");
    let intents = command(&mut h, cancel(7));
    managed(&h, &intents, "cancel-batch-orders");
    reply(&mut h, "cancel-batch-orders", 200, GONE_CANCEL);
    succeeded(&h, 7);
    assert!(h.e.store().order("12").is_none());
    // The absent-order fence queues one fresh order snapshot and holds
    // cancel-all until it lands.
    assert_eq!(count(&h, "orders-pending?"), 1, "{:?}", h.pending);
    command(
        &mut h,
        Command::CancelSymbolOrders {
            command: 9,
            symbol: "BTCUSDT".into(),
        },
    );
    assert_eq!(
        failure(&h, 9).message,
        "Placement outcome is uncertain; reconcile before cancel-all"
    );
    assert_eq!(
        count(&h, "cancel-batch-orders"),
        0,
        "no cancel write while fenced"
    );
    // Answer the snapshot's pages until it completes; 13 is still open.
    while let Some(request) = h
        .pending
        .iter()
        .find(|r| r.url.contains("/api/v1/trade/orders-"))
        .cloned()
    {
        h.take(&request.url);
        let body = if request.url.ends_with("/orders-pending?limit=100") {
            r#"{"code":"0","data":[{"instId":"BTC-USDT","orderCategory":"normal","orderId":"13","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"}]}"#
        } else {
            r#"{"code":"0","data":[]}"#
        };
        answer(&mut h, &request, 200, body);
    }
    // A clean snapshot cleared the fence: cancel-all for the symbol proceeds.
    let intents = command(
        &mut h,
        Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        },
    );
    managed(&h, &intents, "cancel-batch-orders");
}

#[test]
fn a_replacement_cancel_gone_stops_without_a_replacement_prepare() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    let intents = command(&mut h, amend(7));
    managed(&h, &intents, "cancel-batch-orders");
    reply(&mut h, "cancel-batch-orders", 200, GONE_CANCEL);
    fire_until(&mut h, 10_000.0);
    assert_eq!(
        count(&h, "order-detail") + count(&h, "/batch-orders"),
        0,
        "no replacement work is prepared"
    );
    assert_ne!(failure(&h, 7).outcome, Some(Outcome::NotSent));
}

#[test]
fn progress_between_steps_aborts_before_the_next_call() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    let intents = command(&mut h, amend(7));
    managed(&h, &intents, "cancel-batch-orders");
    // The order fills while its cancel is in flight.
    seed_order(&mut h, "12", r#","filledSize":"4""#);
    reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    fire_until(&mut h, 10_000.0);
    assert_eq!(
        count(&h, "order-detail") + count(&h, "/batch-orders"),
        0,
        "no call follows execution progress"
    );
    let error = failure(&h, 7);
    assert!(
        error.message.contains("execution progress"),
        "{}",
        error.message
    );
    assert!(!matches!(
        error.outcome,
        Some(Outcome::NotSent | Outcome::Rejected)
    ));
}

// ---- D-e and capacity ----

#[test]
fn a_refusal_mid_split_keeps_the_earlier_accepted_ids() {
    let mut h = seeded();
    // 21 lots at the fixture's 500 maximum: one batch of 20, then one of 1.
    command(&mut h, place(7, 10_500.0));
    let ids: Vec<String> = (0..20).map(|i| format!("a{i}")).collect();
    let rows: Vec<_> = ids
        .iter()
        .map(|id| serde_json::json!({"orderId": id, "code": "0", "msg": "success"}))
        .collect();
    let body = serde_json::json!({"code": "0", "data": rows}).to_string();
    reply(&mut h, "batch-orders", 200, &body);
    let at = h.now + 350.0;
    let intents = fire_until(&mut h, at);
    let (request, _) = managed(&h, &intents, "batch-orders");
    let sent: Vec<serde_json::Value> =
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(sent.len(), 1);
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"","code":"103003","msg":"Insufficient balance"}]}"#,
    );
    let error = failure(&h, 7);
    // Earlier calls took effect, so the refused tail is no whole-command outcome.
    assert_eq!(error.outcome, None, "{error:?}");
    let partial = error.partial.as_ref().expect("accepted IDs survive");
    assert_eq!(partial.order_ids, ids);
}

#[test]
fn a_maximum_split_streams_fifty_calls_through_one_plan() {
    let mut h = seeded();
    // 999 lots at the fixture's 500 maximum (the native ceiling is 1000
    // lots): 49 batches of 20, then 19.
    let mut intents = command(&mut h, place(7, 499_500.0));
    let mut all = Vec::new();
    for batch in 0..50 {
        let (request, intent) = managed(&h, &intents, "batch-orders");
        assert_eq!(intent, RoutingIntent::Primary);
        let sent: Vec<serde_json::Value> =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        let size = if batch == 49 { 19 } else { 20 };
        assert_eq!(sent.len(), size);
        let ids: Vec<String> = (0..size).map(|i| format!("b{batch}-{i}")).collect();
        let rows: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({"orderId": id, "code": "0", "msg": "success"}))
            .collect();
        all.extend(ids);
        let body = serde_json::json!({"code": "0", "data": rows}).to_string();
        reply(&mut h, "batch-orders", 200, &body);
        let at = h.now + 350.0;
        intents = fire_until(&mut h, at);
    }
    assert!(h.pending.is_empty());
    assert_eq!(succeeded(&h, 7), serde_json::to_string(&all).unwrap());
}

// ---- Released failures and terminals ----

#[test]
fn a_released_503_seals_unknown_restores_rows_and_wakes_the_next() {
    let mut h = seeded();
    command(&mut h, place(7, 1.0));
    command(&mut h, margin(8, true));
    assert_eq!(h.e.store().orders().len(), 1, "the placement is Creating");
    let intents = reply(&mut h, "batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert!(h.e.store().orders().is_empty());
    managed(&h, &intents, "set-margin-mode");
}

#[test]
fn an_unknown_placement_stays_fenced_until_a_fresh_complete_order_read() {
    let mut h = seeded();
    let tick = h.tick;
    event(&mut h, 10_000., Event::TimerFired { timer: tick });
    let stale = h.take("orders-pending?");
    command(&mut h, place(700, 1.0));
    reply(&mut h, "batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 700).outcome, Some(Outcome::Unknown));

    let refused = |h: &mut Harness, id| {
        command(h, place(id, 1.0));
        assert_eq!(count(h, "batch-orders"), 0, "no retry POST while uncertain");
        let error = failure(h, id);
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(error.message.contains("uncertain"));
    };
    refused(&mut h, 701);

    answer(&mut h, &stale, 200, r#"{"code":"0","data":[]}"#);
    for path in ["orders-tpsl-pending", "orders-algo-pending"] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    refused(&mut h, 702);
    let retry_at = h.now + 1_000.;
    fire_until(&mut h, retry_at);
    assert_eq!(count(&h, "orders-pending?"), 1, "fresh reconcile is owed");
    for path in [
        "orders-pending?",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    command(&mut h, place(703, 1.0));
    assert_eq!(
        count(&h, "batch-orders"),
        1,
        "fresh read releases the fence"
    );
}

#[test]
fn engine_reasks_join_a_slow_primary_reconciliation_read() {
    let mut h = seeded();
    command(&mut h, place(750, 1.0));
    reply(&mut h, "batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 750).outcome, Some(Outcome::Unknown));
    let first = h.take("orders-pending?");
    let sealed_at = h.now;
    for at in [sealed_at + 1_000., sealed_at + 3_000.] {
        let timer = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .expect("actual engine reconciliation re-ask timer");
        event(&mut h, at, Event::TimerFired { timer });
        assert_eq!(
            count(&h, "orders-pending?"),
            0,
            "join the held Primary read"
        );
    }
    h.now = sealed_at + 3_100.;
    answer(&mut h, &first, 200, r#"{"code":"0","data":[]}"#);
    for path in ["orders-tpsl-pending", "orders-algo-pending"] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    command(&mut h, place(751, 1.0));
    assert_eq!(
        count(&h, "batch-orders"),
        1,
        "the original slow census releases"
    );
}

#[test]
fn an_engine_reask_restarts_reconciliation_after_native_retries_are_exhausted() {
    let mut h = seeded();
    command(&mut h, place(770, 1.0));
    reply(&mut h, "batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 770).outcome, Some(Outcome::Unknown));
    let sealed_at = h.now;

    for delay in [0., 1_000., 3_000.] {
        if delay > 0. {
            fire_until(&mut h, sealed_at + delay);
        }
        assert_eq!(count(&h, "orders-pending?"), 1);
        let request = h.take("orders-pending?");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("ACCESS-KEY") && value == "key")
        );
        answer(&mut h, &request, 200, r#"{"code":"0","data":{}}"#);
        assert_eq!(count(&h, "orders-pending?"), 0);
    }

    command(&mut h, place(771, 1.0));
    assert_eq!(failure(&h, 771).outcome, Some(Outcome::NotSent));
    assert_eq!(count(&h, "batch-orders"), 0);
    let before_reask = h.actions.len();
    fire_until(&mut h, sealed_at + 7_000.);
    assert!(
        h.actions[before_reask..]
            .iter()
            .all(|a| !matches!(a, Action::Http(r) if r.url.contains("account/positions")))
    );
    assert_eq!(
        count(&h, "orders-pending?"),
        1,
        "the engine re-ask restarts the idle owed census without the periodic poll"
    );
    for path in [
        "orders-pending?",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    command(&mut h, place(772, 1.0));
    assert_eq!(count(&h, "batch-orders"), 1, "the clean census releases");
}

#[test]
fn a_new_uncertain_amend_requires_a_census_started_after_its_mark() {
    let mut instruments: serde_json::Value = serde_json::from_str(INSTRUMENT).unwrap();
    let mut eth = instruments["data"][0].clone();
    eth["instId"] = "ETH-USDT".into();
    eth["baseCurrency"] = "ETH".into();
    instruments["data"].as_array_mut().unwrap().push(eth);
    let mut h = Harness::with_instruments(&instruments.to_string());
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders"},"data":[{"instId":"ETH-USDT","orderCategory":"normal","orderId":"22","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"}]}"#);

    command(&mut h, place(780, 1.0));
    reply(&mut h, "batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 780).outcome, Some(Outcome::Unknown));
    command(
        &mut h,
        Command::AmendOrder {
            command: 781,
            id: "22".into(),
            price: Some(61000.),
            amount: None,
        },
    );
    assert_eq!(count(&h, "cancel-batch-orders"), 1);
    reply(&mut h, "orders-pending?", 200, r#"{"code":"0","data":[]}"#);
    reply(&mut h, "cancel-batch-orders", 503, "Service Unavailable");
    assert_eq!(failure(&h, 781).outcome, Some(Outcome::Unknown));
    for path in ["orders-tpsl-pending", "orders-algo-pending"] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    let eth_place = |id| {
        let mut order = limit(1.0);
        order.symbol = "ETHUSDT".into();
        Command::PlaceOrders {
            command: id,
            orders: vec![order],
        }
    };
    command(&mut h, eth_place(782));
    assert_eq!(
        count(&h, "batch-orders"),
        0,
        "a post-mark last family cannot make a pre-mark first page fresh"
    );
    assert_eq!(failure(&h, 782).outcome, Some(Outcome::NotSent));
    assert!(failure(&h, 782).message.contains("uncertain"));
    let reask_at = h.now + 1_000.;
    fire_until(&mut h, reask_at);
    for path in [
        "orders-pending?",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        reply(&mut h, path, 200, r#"{"code":"0","data":[]}"#);
    }
    command(&mut h, eth_place(783));
    assert_eq!(
        count(&h, "batch-orders"),
        1,
        "a wholly fresh census releases"
    );
}

#[test]
fn a_late_or_duplicate_response_after_a_terminal_changes_nothing() {
    let mut h = seeded();
    let intents = command(&mut h, place(7, 1.0));
    let (request, _) = managed(&h, &intents, "batch-orders");
    h.take("batch-orders");
    answer(
        &mut h,
        &request,
        200,
        r#"{"code":"0","data":[{"orderId":"","code":"102015","msg":"price"}]}"#,
    );
    let before = h.actions.len();
    answer(
        &mut h,
        &request,
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    assert_eq!(terminals(&h, 7).len(), 1);
    assert!(
        !h.actions[before..]
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. }))),
    );
    assert!(h.e.store().orders().is_empty());
}

// ---- W1 serial hedged join (interim until W2's barrier) ----

#[test]
fn the_hedged_join_sends_short_only_after_long_is_accepted() {
    let mut h = hedged_isolated();
    let intents = command(&mut h, leverage(7));
    assert_eq!((leg(&h, "\"long\""), leg(&h, "\"short\"")), (1, 0));
    assert_eq!(
        managed(&h, &intents, "set-leverage").1,
        RoutingIntent::Primary
    );
    let intents = reply(&mut h, "set-leverage", 200, LEVERAGE_OK);
    assert_eq!((leg(&h, "\"long\""), leg(&h, "\"short\"")), (0, 1));
    managed(&h, &intents, "set-leverage");
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    reply(&mut h, "set-leverage", 200, LEVERAGE_OK);
    succeeded(&h, 7);
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
fn a_long_503_never_prepares_short_and_seals_unknown() {
    let mut h = hedged_isolated();
    command(&mut h, leverage(7));
    reply(&mut h, "set-leverage", 503, "Service Unavailable");
    assert_eq!(count(&h, "set-leverage"), 0, "short is never prepared");
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
fn a_long_refusal_never_prepares_short_and_is_rejected() {
    let mut h = hedged_isolated();
    command(&mut h, leverage(7));
    reply(&mut h, "set-leverage", 200, LEVERAGE_REFUSED);
    assert_eq!(count(&h, "set-leverage"), 0, "short is never prepared");
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Rejected));
}

#[test]
fn a_short_refusal_after_long_keeps_its_effect_without_commit() {
    let mut h = hedged_isolated();
    command(&mut h, leverage(7));
    reply(&mut h, "set-leverage", 200, LEVERAGE_OK);
    let positions = count(&h, "account/positions");
    reply(&mut h, "set-leverage", 200, LEVERAGE_REFUSED);
    let error = failure(&h, 7);
    assert_eq!(error.outcome, None, "long took effect: {error:?}");
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "native join refresh"
    );
}

#[test]
fn a_short_503_after_long_is_unknown_without_commit() {
    let mut h = hedged_isolated();
    command(&mut h, leverage(7));
    reply(&mut h, "set-leverage", 200, LEVERAGE_OK);
    let positions = count(&h, "account/positions");
    reply(&mut h, "set-leverage", 503, "Service Unavailable");
    assert_eq!(failure(&h, 7).outcome, Some(Outcome::Unknown));
    assert_ne!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
    assert_eq!(
        count(&h, "account/positions"),
        positions + 1,
        "native join refresh"
    );
}

#[test]
fn a_queued_leverage_is_planned_against_the_state_at_its_wake() {
    let mut h = seeded();
    command(&mut h, place(7, 1.0));
    command(&mut h, margin(8, true));
    command(&mut h, mode(9, true));
    // Planned at entry this would be one net leg; at its wake it is the join.
    command(&mut h, leverage(10));
    reply(
        &mut h,
        "batch-orders",
        200,
        r#"{"code":"0","data":[{"orderId":"1","code":"0","msg":"success"}]}"#,
    );
    reply(
        &mut h,
        "set-margin-mode",
        200,
        r#"{"code":"0","data":{"marginMode":"isolated"}}"#,
    );
    let intents = reply(
        &mut h,
        "set-position-mode",
        200,
        r#"{"code":"0","data":{"positionMode":"long_short_mode"}}"#,
    );
    assert_eq!((leg(&h, "\"long\""), leg(&h, "\"short\"")), (1, 0));
    managed(&h, &intents, "set-leverage");
}

// ---- Review pins (dazzy3, 4yjlw0, fe7lgm) ----

#[test]
fn a_text_only_gone_row_is_classified_by_its_code_and_stops() {
    let mut h = seeded();
    let ids: Vec<String> = (0..21).map(|i| format!("{}", 100 + i)).collect();
    for id in &ids {
        seed_order(&mut h, id, "");
    }
    command(
        &mut h,
        Command::CancelOrders {
            command: 7,
            ids: ids.clone(),
        },
    );
    // Batch 1 (20): one row carries the "gone" text under an unlisted code.
    let rows: Vec<_> = ids[..20]
        .iter()
        .enumerate()
        .map(|(i, id)| {
            if i == 3 {
                serde_json::json!({"orderId": id, "code": "1", "msg": "Cancel failed as the order has been filled, triggered, canceled or does not exist."})
            } else {
                serde_json::json!({"orderId": id, "code": "0"})
            }
        })
        .collect();
    let body = serde_json::json!({"code": "0", "data": rows}).to_string();
    reply(&mut h, "cancel-batch-orders", 200, &body);
    fire_until(&mut h, 10_000.0);
    assert_eq!(
        count(&h, "cancel-batch-orders"),
        0,
        "the job stops at the unproven row"
    );
    let error = failure(&h, 7);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.message.contains("cancellation rejected"),
        "{}",
        error.message
    );
    assert!(
        h.e.store().order(&ids[3]).is_some(),
        "an unproven row is not removed"
    );
}

#[test]
fn the_reconcile_snapshot_is_primary_signed_with_a_secondary_configured() {
    let mut h = Harness::with_secondary();
    h.open_private();
    h.seed();
    seed_order(&mut h, "12", "");
    command(&mut h, cancel(7));
    let intents = reply(&mut h, "cancel-batch-orders", 200, GONE_CANCEL);
    let (_, intent) = managed(&h, &intents, "orders-pending?");
    assert_eq!(
        intent,
        RoutingIntent::Primary,
        "the fence-clearing snapshot"
    );
    // Every later page of that snapshot stays Primary.
    let mut pages = 0;
    while let Some(request) = h
        .pending
        .iter()
        .find(|r| r.url.contains("/api/v1/trade/orders-"))
        .cloned()
    {
        h.take(&request.url);
        let intents = answer(&mut h, &request, 200, r#"{"code":"0","data":[]}"#);
        pages += 1;
        for next in h
            .pending
            .iter()
            .filter(|r| r.url.contains("/api/v1/trade/orders-"))
        {
            assert_eq!(intents.get(&next.id), Some(&RoutingIntent::Primary));
        }
    }
    assert_eq!(pages, 3);
}

#[test]
fn the_journal_owns_a_non_2xx_outcome() {
    for (status, body, outcome) in [
        // An undocumented status with no native code proves nothing.
        (404, "not found", Some(Outcome::Unknown)),
        (400, "Bad Request", Some(Outcome::Unknown)),
        // A documented native code in the body wins over the status.
        (
            400,
            r#"{"code":"102015","msg":"Order price is not within the price limit"}"#,
            Some(Outcome::Rejected),
        ),
        (429, "{}", Some(Outcome::Rejected)),
    ] {
        let mut h = seeded();
        command(&mut h, place(7, 1.0));
        reply(&mut h, "batch-orders", status, body);
        assert_eq!(failure(&h, 7).outcome, outcome, "{status} {body}");
    }
}

#[test]
fn a_verify_detail_in_the_wrong_state_is_never_not_sent() {
    let mut h = seeded();
    seed_order(&mut h, "12", "");
    command(&mut h, amend(7));
    reply(&mut h, "cancel-batch-orders", 200, ACCEPTED_CANCEL);
    let at = h.now + 350.0;
    fire_until(&mut h, at);
    reply(
        &mut h,
        "order-detail",
        200,
        r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"live","filledSize":"0"}}"#,
    );
    let error = failure(&h, 7);
    assert!(!matches!(
        error.outcome,
        Some(Outcome::NotSent | Outcome::Rejected)
    ));
    assert_eq!(count(&h, "/batch-orders"), 0);
}
