//! Ports of the TS TWAP suites: TwapInstance, TwapInstance.timeout,
//! TwapInstance.termination-race, TwapManager and TwapHandler.

use std::collections::HashMap;

use safe_cex_algo_twap::SliceFailure;
use safe_cex_algo_twap::{
    ChaserProgress, OrderKind, OrderSide, TickerQuote, TwapBook, TwapEffect, TwapEnv,
    TwapMarketView, TwapStatus,
};
use serde_json::{Value, json};

fn owned_book() -> TwapBook {
    TwapBook::with_owner(
        "test-account",
        safe_cex_algo_twap::TwapOwner {
            owner_id: "A".into(),
            generation: 7,
        },
    )
}

#[path = "twap/identity.rs"]
#[cfg(not(target_arch = "wasm32"))]
mod identity;

struct Env {
    now: f64,
    units: Vec<f64>,
    ids: u32,
}

impl Env {
    fn new() -> Self {
        Self {
            now: 1_700_000_000_000.0,
            units: Vec::new(),
            ids: 0,
        }
    }
    fn advance(&mut self, ms: f64) {
        self.now += ms;
    }
}

impl TwapEnv for Env {
    fn now_ms(&self) -> f64 {
        self.now
    }
    fn random_unit(&mut self) -> f64 {
        if self.units.is_empty() {
            0.5
        } else {
            self.units.remove(0)
        }
    }
    fn new_id(&mut self) -> String {
        self.ids += 1;
        format!("id-{}", self.ids)
    }
}

#[derive(Default)]
struct View {
    ticker: Option<TickerQuote>,
    chasers: HashMap<String, ChaserProgress>,
    client_id: Option<String>,
    started: std::cell::RefCell<Vec<safe_cex_algo_twap::ChaserRequest>>,
    refuse_chasers: std::cell::Cell<bool>,
    step: Option<f64>,
}

impl TwapMarketView for View {
    fn amount_step(&self, _symbol: &str) -> Option<f64> {
        self.step
    }
    fn start_chaser(&self, request: &safe_cex_algo_twap::ChaserRequest) -> Result<(), String> {
        if self.refuse_chasers.get() {
            return Err("chaser refused".to_owned());
        }
        self.started.borrow_mut().push(request.clone());
        Ok(())
    }
    fn ticker(&self, _symbol: &str) -> Option<TickerQuote> {
        self.ticker
    }
    fn chaser(&self, chaser_id: &str) -> Option<ChaserProgress> {
        self.chasers.get(chaser_id).copied()
    }
    fn client_order_id(&self, _request: &safe_cex_algo_twap::TwapOrderRequest) -> Option<String> {
        self.client_id.clone()
    }
}

fn options(overrides: Value) -> Value {
    let mut base = json!({
        "accountName": "test-account",
        "activeMarket": { "id": "BTCUSDT", "symbol": "BTCUSDT", "base": "BTC", "quote": "USDT" },
        "size": 1.0,
        "duration": 10,
        "ordersCount": 5,
        "side": "buy",
        "chaserDelay": 1000,
        "reduceOnly": false,
        "randomnessPercentage": 0,
        "useLimitOrders": false,
        "useChaserOrders": false,
    });
    if let (Value::Object(base), Value::Object(overrides)) = (&mut base, overrides) {
        base.extend(overrides);
    }
    base
}

fn start(
    book: &mut TwapBook,
    env: &mut Env,
    view: &View,
    overrides: Value,
) -> (String, Vec<TwapEffect>) {
    let effects = book.handle("twap:start", &options(overrides), env, view);
    let id = book
        .ids()
        .last()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("id-{}", env.ids));
    (id, effects)
}

fn placements(effects: &[TwapEffect]) -> Vec<(Option<u64>, &safe_cex_algo_twap::TwapOrderRequest)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::PlaceOrder {
                settle_token,
                request,
                ..
            } => Some((*settle_token, request)),
            _ => None,
        })
        .collect()
}

fn market_amounts(effects: &[TwapEffect]) -> Vec<f64> {
    placements(effects)
        .into_iter()
        .filter(|(_, r)| r.kind == OrderKind::Market)
        .map(|(_, r)| r.amount)
        .collect()
}

/// The chasers started since the last call.
fn chaser_ids(view: &View) -> Vec<String> {
    view.started
        .take()
        .into_iter()
        .map(|request| request.id)
        .collect()
}

fn removed_chasers(effects: &[TwapEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::RemoveChaser { chaser_id, .. } => Some(chaser_id.clone()),
            _ => None,
        })
        .collect()
}

fn state_updates(effects: &[TwapEffect]) -> Vec<&Value> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::StateUpdate { data } => Some(data),
            _ => None,
        })
        .collect()
}

/// Settle every slice placement in `effects` with `outcome`.
fn refused(message: &str) -> Result<Vec<String>, SliceFailure> {
    Err(SliceFailure {
        message: message.to_owned(),
        outcome: Some(safe_cex_engine::error::Outcome::NotSent),
        ..Default::default()
    })
}

fn settle_all(
    book: &mut TwapBook,
    env: &mut Env,
    id: &str,
    effects: &[TwapEffect],
    outcome: Result<Vec<String>, SliceFailure>,
) -> Vec<TwapEffect> {
    let mut out = Vec::new();
    for (token, _) in placements(effects) {
        if let Some(token) = token {
            out.extend(book.placement_settled(id, token, outcome.clone(), env));
        }
    }
    out
}

// --- TwapInstance.test.ts ---

#[test]
fn origin_defaults_to_web_and_keeps_provided_values() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, _) = start(&mut book, &mut env, &view, json!({}));
    assert_eq!(book.serialize_state()[&id]["state"]["origin"], "web");
    for origin in ["web", "cli", "webhook", "automation"] {
        let (id, _) = start(&mut book, &mut env, &view, json!({ "origin": origin }));
        assert_eq!(book.serialize_state()[&id]["state"]["origin"], origin);
    }
}

#[test]
fn new_state_has_created_at_and_exact_slice_sizes() {
    let mut env = Env::new();
    let options: safe_cex_algo_twap::TwapOptions =
        serde_json::from_value(options(json!({ "randomnessPercentage": 10 }))).unwrap();
    let instance = safe_cex_algo_twap::TwapInstance::new("x".into(), options, None, &mut env);
    assert_eq!(instance.state.status, TwapStatus::Idle);
    assert_eq!(instance.state.created_at, Some(env.now));
    assert_eq!(instance.state.started_at, None);
    assert_eq!(instance.state.orders_sizes.len(), 5);
    let sum: f64 = instance.state.orders_sizes.iter().sum();
    assert!((sum - 1.0).abs() < 1e-10, "{sum}");
}

#[test]
fn randomness_skews_pairs_in_opposite_directions() {
    let mut env = Env::new();
    // random(0, 10) with unit 0.5 is 5 → each pair skews by 5%.
    let options: safe_cex_algo_twap::TwapOptions = serde_json::from_value(options(
        json!({ "randomnessPercentage": 10, "ordersCount": 4 }),
    ))
    .unwrap();
    let instance = safe_cex_algo_twap::TwapInstance::new("x".into(), options, None, &mut env);
    assert_eq!(
        instance.state.orders_sizes,
        vec![0.2625, 0.2375, 0.2625, 0.2375]
    );
}

#[test]
fn restored_state_keeps_status_counts_and_id() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let state = json!({
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": [0.2, 0.2, 0.2], "side": "buy", "status": "paused", "ordersCount": 5,
        "ordersExecuted": 2, "nextOrderAt": env.now + 10_000.0, "origin": "cli",
    });
    book.handle(
        "twap:state",
        &json!([{ "id": "existing-id", "ownerId": "A", "ownerGeneration": 7, "options": options(json!({})), "state": state }]),
        &mut env,
        &view,
    );
    let instance = book.get("existing-id").unwrap();
    assert_eq!(instance.state.status, TwapStatus::Paused);
    assert_eq!(instance.state.orders_executed, 2);
    assert_eq!(
        book.serialize_state()["existing-id"]["state"]["origin"],
        "cli"
    );
}

#[test]
fn restored_state_without_origin_stays_without_origin() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let state = json!({
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": [0.2], "side": "buy", "status": "running", "ordersCount": 5,
        "ordersExecuted": 2, "nextOrderAt": env.now + 10_000.0,
    });
    book.handle(
        "twap:state",
        &json!([{ "id": "old", "ownerId": "A", "ownerGeneration": 7, "options": options(json!({})), "state": state }]),
        &mut env,
        &view,
    );
    assert_eq!(book.get("old").unwrap().state.status, TwapStatus::Running);
    assert!(
        book.serialize_state()["old"]["state"]
            .get("origin")
            .is_none()
    );
}

#[test]
fn start_from_idle_sets_started_at_and_running() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 0.001, "ordersCount": 2 }),
    );
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec![]));
    let instance = book.get(&id).unwrap();
    assert_eq!(instance.state.started_at, Some(env.now));
    assert_eq!(instance.state.status, TwapStatus::Running);
}

#[test]
fn direct_limit_slices_track_returned_order_ids() {
    let (mut env, mut view) = (Env::new(), View::default());
    view.ticker = Some(TickerQuote {
        bid: Some(49_900.0),
        ask: Some(50_100.0),
        last: Some(50_000.0),
    });
    let mut book = owned_book();
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 0.002, "ordersCount": 2, "useLimitOrders": true }),
    );
    let (_, request) = placements(&effects)[0];
    assert_eq!(
        (request.kind, request.price),
        (OrderKind::Limit, Some(49_900.0))
    );
    settle_all(
        &mut book,
        &mut env,
        &id,
        &effects,
        Ok(vec!["twap-order-1".into()]),
    );
    assert_eq!(book.get(&id).unwrap().current_order_ids(), ["twap-order-1"]);
    assert_eq!(
        book.order_owner("test-account", "twap-order-1"),
        Some(id.as_str())
    );
    let active = book.active_twaps("test-account");
    assert_eq!(active[0]["currentOrderIds"], json!(["twap-order-1"]));
}

#[test]
fn limit_slices_without_a_price_fall_back_to_market_and_are_untracked() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "ordersCount": 2, "useLimitOrders": true, "side": "sell" }),
    );
    let (_, request) = placements(&effects)[0];
    assert_eq!(
        (request.kind, request.price, request.side),
        (OrderKind::Market, None, OrderSide::Sell)
    );
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["m".into()]));
    assert!(book.get(&id).unwrap().current_order_ids().is_empty());
}

// --- full schedule ---

#[test]
fn slices_run_on_the_interval_and_complete() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    // 10 minutes / 5 slices → 120 s.
    let (id, mut effects) = start(&mut book, &mut env, &view, json!({}));
    let mut placed = 0;
    loop {
        let amounts = market_amounts(&effects);
        placed += amounts.len();
        let settled = settle_all(&mut book, &mut env, &id, &effects, Ok(vec![]));
        if book.get(&id).is_none() {
            let last = state_updates(&settled);
            assert_eq!(last.last().unwrap(), &&json!({}));
            break;
        }
        assert_eq!(book.wake_at(), Some(env.now + 120_000.0));
        env.advance(119_999.0);
        assert!(book.wake(&mut env, &view).is_empty());
        env.advance(1.0);
        effects = book.wake(&mut env, &view);
    }
    assert_eq!(placed, 5);
    assert_eq!(book.wake_at(), None);
}

#[test]
fn a_refused_slice_logs_advances_and_is_not_counted() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    let settled = settle_all(
        &mut book,
        &mut env,
        &id,
        &effects,
        refused("insufficient margin"),
    );
    assert!(settled.contains(&TwapEffect::Log {
        message: "TWAP BTCUSDT slice failed: insufficient margin".into()
    }));
    assert_eq!(book.get(&id).unwrap().state.orders_executed, 0);
    assert!(book.wake_at().is_some());
}

#[test]
fn a_twap_whose_every_slice_is_refused_ends_saying_none_were_placed() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    let first = settle_all(&mut book, &mut env, &id, &effects, refused("reduce only"));
    assert_eq!(logs(&first), ["TWAP BTCUSDT slice failed: reduce only"]);

    env.now = book.wake_at().unwrap();
    let effects = book.wake(&mut env, &view);
    let last = settle_all(&mut book, &mut env, &id, &effects, refused("reduce only"));
    assert_eq!(
        logs(&last),
        [
            "TWAP BTCUSDT slice failed: reduce only",
            "TWAP BTCUSDT ended with 0 of 2 slices placed",
        ]
    );
    assert_eq!(book.wake_at(), None);
    assert!(book.get(&id).is_none());
}

#[test]
fn a_twap_that_placed_every_slice_ends_without_a_shortfall_error() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 1 }));
    let last = settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["a".into()]));
    assert!(logs(&last).is_empty());
}

#[test]
fn a_user_stop_during_a_refused_placement_reports_no_shortfall() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    book.handle("twap:stop", &json!(id), &mut env, &view);
    let settled = settle_all(&mut book, &mut env, &id, &effects, refused("no"));
    assert_eq!(logs(&settled), ["TWAP BTCUSDT slice failed: no"]);
}

#[test]
fn pause_stops_the_schedule_and_resume_keeps_next_order_at() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({}));
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec![]));
    let next = book.get(&id).unwrap().state.next_order_at;
    book.handle("twap:pause", &json!(id), &mut env, &view);
    assert_eq!(book.get(&id).unwrap().state.status, TwapStatus::Paused);
    assert_eq!(book.wake_at(), None);
    env.advance(30_000.0);
    book.handle("twap:resume", &json!(id), &mut env, &view);
    assert_eq!(book.wake_at(), Some(next));
    // Resuming after the due time fires immediately.
    book.handle("twap:pause", &json!(id), &mut env, &view);
    env.advance(500_000.0);
    book.handle("twap:resume", &json!(id), &mut env, &view);
    assert_eq!(book.wake_at(), Some(env.now));
}

// --- TwapInstance.termination-race.test.ts ---

#[test]
fn in_flight_placement_settling_after_terminate_does_not_resurrect() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 2, "duration": 1, "ordersCount": 2 }),
    );
    assert_eq!(placements(&effects).len(), 1);
    book.handle("twap:stop", &json!(id), &mut env, &view);
    assert!(book.get(&id).is_none());
    let settled = settle_all(&mut book, &mut env, &id, &effects, Ok(vec![]));
    assert!(placements(&settled).is_empty());
    env.advance(120_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
    assert_eq!(book.wake_at(), None);
}

#[test]
fn in_flight_failure_after_terminate_still_logs() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    book.handle("twap:stop", &json!(id), &mut env, &view);
    let settled = settle_all(&mut book, &mut env, &id, &effects, refused("boom"));
    assert_eq!(
        settled,
        vec![TwapEffect::Log {
            message: "TWAP BTCUSDT slice failed: boom".into()
        }]
    );
}

// --- TwapInstance.timeout.test.ts (slice = 120 s) ---

fn chase(overrides: Value) -> Value {
    let mut base = json!({ "useChaserOrders": true });
    if let (Value::Object(base), Value::Object(overrides)) = (&mut base, overrides) {
        base.extend(overrides);
    }
    base
}

fn start_chase(overrides: Value) -> (TwapBook, Env, View, String, String) {
    let (mut env, mut view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, chase(overrides));
    let chasers = chaser_ids(&view);
    assert_eq!(chasers.len(), 1);
    assert!(market_amounts(&effects).is_empty());
    view.chasers.insert(
        chasers[0].clone(),
        ChaserProgress {
            remaining_size: 0.2,
            stopped: false,
            ..Default::default()
        },
    );
    (book, env, view, id, chasers[0].clone())
}

#[test]
fn chaser_slice_request_matches_ts() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (twap_id, _) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "reduceOnly": true, "chaserDelay": 750 })),
    );
    let request = view.started.borrow()[0].clone();
    assert_eq!(request.twap_id, twap_id, "a slice names its TWAP");
    assert_eq!(
        (
            request.size,
            request.side,
            request.max_attempts,
            request.reduce_only,
            request.delay
        ),
        (0.2, OrderSide::Buy, 1000, true, 750.0)
    );
}

#[test]
fn a_refused_last_chase_slice_ends_with_the_true_count() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    view.refuse_chasers.set(true);
    let (id, first) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "ordersCount": 3 })),
    );
    assert_eq!(logs(&first), ["chaser refused"]);
    assert_eq!(book.get(&id).unwrap().state.orders_executed, 0);
    let update = state_updates(&first).pop().unwrap().clone();
    assert_eq!(update[&id]["state"]["ordersExecuted"], 0);

    view.refuse_chasers.set(false);
    env.advance(book.get(&id).unwrap().state.next_order_at - env.now);
    book.wake(&mut env, &view);
    assert_eq!(book.get(&id).unwrap().state.orders_executed, 1);

    view.refuse_chasers.set(true);
    env.advance(book.get(&id).unwrap().state.next_order_at - env.now);
    let last = book.wake(&mut env, &view);
    assert_eq!(
        logs(&last),
        [
            "chaser refused",
            "TWAP BTCUSDT ended with 1 of 3 slices placed"
        ]
    );
    assert!(book.get(&id).is_none());
}

#[test]
fn a_chase_twap_that_started_every_slice_ends_without_a_shortfall_error() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (_, effects) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "ordersCount": 1 })),
    );
    assert!(logs(&effects).is_empty());
}

#[test]
fn no_timeout_without_timeout_pct() {
    let (mut book, mut env, view, _, _) = start_chase(json!({}));
    env.advance(60_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
    assert_eq!(
        book.wake_at(),
        Some(env.now + 250.0),
        "observe child uncertainty without a timeout action"
    );
    env.advance(60_000.0);
    let effects = book.wake(&mut env, &view);
    assert!(removed_chasers(&effects).is_empty());
    assert!(market_amounts(&effects).is_empty());
    assert_eq!(chaser_ids(&view).len(), 1);
}

#[test]
fn timeout_market_cancels_chaser_and_markets_the_remainder() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(59_999.0);
    assert!(book.wake(&mut env, &view).is_empty());
    env.advance(1.0);
    let effects = book.wake(&mut env, &view);
    assert_eq!(removed_chasers(&effects), vec![chaser.clone()]);
    assert!(market_amounts(&effects).is_empty(), "not beside its cancel");
    let effects = settle_chaser(&mut book, &mut env, &mut view, &chaser, 0.2);
    let markets = placements(&effects);
    assert_eq!(markets.len(), 1);
    let (token, request) = markets[0];
    assert_eq!(token, None);
    assert_eq!(
        (request.side, request.amount, request.reduce_only),
        (OrderSide::Buy, 0.2, false)
    );
}

/// The chase's cancel has settled with `remaining` left; the next poll sees it.
fn settle_chaser(
    book: &mut TwapBook,
    env: &mut Env,
    view: &mut View,
    chaser: &str,
    remaining: f64,
) -> Vec<TwapEffect> {
    view.chasers.insert(
        chaser.to_owned(),
        ChaserProgress {
            remaining_size: remaining,
            stopped: true,
            ended: true,
            order_may_be_open: false,
        },
    );
    env.advance(250.0);
    book.wake(env, view)
}

#[test]
fn a_timed_out_slice_sends_no_market_while_its_cancel_still_retries() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(60_000.0);
    assert!(market_amounts(&book.wake(&mut env, &view)).is_empty());
    view.chasers.insert(
        chaser.clone(),
        ChaserProgress {
            remaining_size: 0.2,
            stopped: true,
            ..Default::default()
        },
    );
    for _ in 0..40 {
        env.advance(250.0);
        assert!(market_amounts(&book.wake(&mut env, &view)).is_empty());
    }
    // A fill during the wait shrinks the market to what is really left.
    let effects = settle_chaser(&mut book, &mut env, &mut view, &chaser, 0.05);
    assert_eq!(market_amounts(&effects), vec![0.05]);
}

#[test]
fn a_timed_out_slice_sends_no_market_if_its_order_may_still_be_open() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(60_000.0);
    book.wake(&mut env, &view);
    view.chasers.insert(
        chaser,
        ChaserProgress {
            remaining_size: 0.2,
            stopped: true,
            ended: true,
            order_may_be_open: true,
        },
    );
    env.advance(250.0);
    let effects = book.wake(&mut env, &view);
    assert!(market_amounts(&effects).is_empty());
    assert!(effects.iter().any(|effect| matches!(
        effect,
        TwapEffect::Log { message } if message == "The outcome is unknown. Check Open Orders"
    )));
}

#[test]
fn stopping_a_twap_drops_a_market_waiting_on_its_slice() {
    let (mut book, mut env, mut view, id, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(60_000.0);
    book.wake(&mut env, &view);
    book.handle("twap:stop", &json!(id), &mut env, &view);
    assert!(settle_chaser(&mut book, &mut env, &mut view, &chaser, 0.2).is_empty());
}

#[test]
fn slices_stay_on_the_market_step_and_sum_to_the_size() {
    let (mut env, mut view) = (Env::new(), View::default());
    view.step = Some(0.001);
    let mut book = owned_book();
    let (id, mut effects) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 0.01, "ordersCount": 4, "randomnessPercentage": 0 }),
    );
    let mut sent = Vec::new();
    for _ in 0..4 {
        sent.extend(market_amounts(&effects));
        let token = placements(&effects)[0].0.unwrap();
        book.placement_settled(&id, token, Ok(vec!["o".into()]), &mut env);
        env.advance(150_000.0);
        effects = book.wake(&mut env, &view);
    }
    assert_eq!(sent.len(), 4, "{sent:?}");
    assert!((sent.iter().sum::<f64>() - 0.01).abs() < 1e-9, "{sent:?}");
    assert!(
        sent.iter()
            .all(|size| ((size / 0.001).round() - size / 0.001).abs() < 1e-9),
        "{sent:?}"
    );
}

#[test]
fn timeout_next_cancels_without_market() {
    let (mut book, mut env, view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "next" }));
    env.advance(60_000.0);
    let effects = book.wake(&mut env, &view);
    assert_eq!(removed_chasers(&effects), vec![chaser]);
    assert!(market_amounts(&effects).is_empty());
}

#[test]
fn filled_chaser_needs_no_timeout_action() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    view.chasers.insert(
        chaser,
        ChaserProgress {
            remaining_size: 0.0,
            stopped: true,
            ..Default::default()
        },
    );
    env.advance(60_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
}

#[test]
fn dropped_chaser_needs_no_timeout_action() {
    let (mut book, mut env, mut view, _, _) = start_chase(json!({ "timeoutPct": 50 }));
    view.chasers.clear();
    env.advance(60_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
}

#[test]
fn fail_action_market_applies_to_a_stopped_unfilled_chaser() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "next", "failAction": "market" }));
    view.chasers.insert(
        chaser.clone(),
        ChaserProgress {
            remaining_size: 0.2,
            stopped: true,
            ..Default::default()
        },
    );
    env.advance(60_000.0);
    assert!(market_amounts(&book.wake(&mut env, &view)).is_empty());
    let effects = settle_chaser(&mut book, &mut env, &mut view, &chaser, 0.2);
    assert_eq!(market_amounts(&effects), vec![0.2]);
}

#[test]
fn fail_action_next_skips_a_stopped_unfilled_chaser() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market", "failAction": "next" }));
    view.chasers.insert(
        chaser.clone(),
        ChaserProgress {
            remaining_size: 0.2,
            stopped: true,
            ..Default::default()
        },
    );
    env.advance(60_000.0);
    let effects = book.wake(&mut env, &view);
    assert_eq!(removed_chasers(&effects), vec![chaser]);
    assert!(market_amounts(&effects).is_empty());
}

#[test]
fn terminate_cancels_pending_slice_timeouts() {
    let (mut book, mut env, view, id, _) = start_chase(json!({ "timeoutPct": 50 }));
    book.handle("twap:stop", &json!(id), &mut env, &view);
    assert_eq!(book.wake_at(), None);
    env.advance(60_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
}

#[test]
fn reduce_only_reaches_the_fallback_market() {
    let (mut book, mut env, mut view, _, chaser) =
        start_chase(json!({ "reduceOnly": true, "timeoutPct": 50 }));
    env.advance(60_000.0);
    book.wake(&mut env, &view);
    let effects = settle_chaser(&mut book, &mut env, &mut view, &chaser, 0.2);
    assert!(placements(&effects)[0].1.reduce_only);
}

#[test]
fn last_slice_timeout_still_fires_after_completion() {
    let (mut env, mut view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, _) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "ordersCount": 1, "timeoutPct": 50 })),
    );
    assert!(
        book.get(&id).is_none(),
        "a one-slice TWAP completes on its first slice"
    );
    let chaser = chaser_ids(&view)[0].clone();
    view.chasers.insert(
        chaser.clone(),
        ChaserProgress {
            remaining_size: 1.0,
            stopped: false,
            ..Default::default()
        },
    );
    env.advance(300_000.0);
    assert!(market_amounts(&book.wake(&mut env, &view)).is_empty());
    let effects = settle_chaser(&mut book, &mut env, &mut view, &chaser, 1.0);
    assert_eq!(market_amounts(&effects), vec![1.0]);
    assert_eq!(book.wake_at(), None);
}

// --- TwapManager.test.ts ---

#[test]
fn serialize_state_skips_completed_twaps() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    assert_eq!(book.serialize_state(), json!({}));
    let (running, effects) = start(&mut book, &mut env, &view, json!({}));
    settle_all(&mut book, &mut env, &running, &effects, Ok(vec![]));
    let (paused, effects) = start(&mut book, &mut env, &view, json!({}));
    settle_all(&mut book, &mut env, &paused, &effects, Ok(vec![]));
    book.handle("twap:pause", &json!(paused), &mut env, &view);
    let (stopped, _) = start(&mut book, &mut env, &view, json!({}));
    book.handle("twap:stop", &json!(stopped), &mut env, &view);
    let state = book.serialize_state();
    let keys: Vec<&String> = state.as_object().unwrap().keys().collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(state[&running]["state"]["status"], "running");
    assert_eq!(state[&paused]["state"]["status"], "paused");
    assert_eq!(state[&running]["options"]["ordersCount"], 5);
}

// --- TwapHandler.test.ts ---

#[test]
fn start_emits_state_update_with_the_new_twap() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({}));
    let updates = state_updates(&effects);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0][&id]["state"]["status"], "running");
    assert_eq!(
        updates[0][&id]["state"]["ordersSizes"],
        json!([0.2, 0.2, 0.2, 0.2])
    );
}

#[test]
fn ops_on_unknown_ids_are_ignored() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    for op in [
        "twap:resume",
        "twap:pause",
        "twap:stop",
        "twap:cleanup",
        "twap:unknown",
    ] {
        assert!(
            book.handle(op, &json!("missing"), &mut env, &view)
                .is_empty()
        );
    }
}

#[test]
fn cleanup_marks_the_twap() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, _) = start(&mut book, &mut env, &view, json!({}));
    let effects = book.handle("twap:cleanup", &json!(id), &mut env, &view);
    assert_eq!(
        state_updates(&effects)[0][&id]["state"]["markForCleanup"],
        true
    );
}

fn persisted(id: &str, status: &str, next_order_at: f64) -> Value {
    json!({
        "id": id,
        "ownerId": "A",
        "ownerGeneration": 7,
        "options": options(json!({})),
        "state": {
            "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
            "ordersSizes": [0.5, 0.5], "side": "buy", "status": status, "ordersCount": 5,
            "ordersExecuted": 3, "nextOrderAt": next_order_at,
        },
    })
}

#[test]
fn restore_drops_foreign_and_stale_rows_and_keeps_the_matching_generation() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut foreign = persisted("foreign", "running", env.now + 15_000.);
    foreign["ownerId"] = json!("B");
    let mut stale = persisted("stale", "running", env.now + 15_000.);
    stale["ownerGeneration"] = json!(6);
    let mut missing = persisted("missing", "running", env.now + 15_000.);
    missing.as_object_mut().unwrap().remove("ownerGeneration");
    book.handle(
        "twap:state",
        &json!([
            foreign,
            stale,
            missing,
            persisted("current", "running", env.now + 15_000.)
        ]),
        &mut env,
        &view,
    );
    assert_eq!(book.ids().collect::<Vec<_>>(), vec!["current"]);
    let snapshot = book.serialize_state();
    assert_eq!(snapshot.as_object().unwrap().len(), 1);
    assert_eq!(snapshot["current"]["ownerId"], "A");
    assert_eq!(snapshot["current"]["ownerGeneration"], 7);
    env.advance(15_001.);
    let effects = book.wake(&mut env, &view);
    let placed: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::PlaceOrder { twap_id, .. } => Some(twap_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(placed, ["current"]);
}

#[test]
#[ignore = "requires Node and workspace Yarn dependencies; run with --include-ignored"]
fn actual_worker_round_trip_preserves_native_restore_ownership() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    for generation in [Some(7), Some(6), Some(0), None] {
        let (mut env, view) = (Env::new(), View::default());
        let mut row = persisted("round-trip", "running", env.now + 15_000.);
        if let Some(generation) = generation {
            row["ownerGeneration"] = json!(generation);
        } else {
            row.as_object_mut().unwrap().remove("ownerGeneration");
        }
        let rows = json!([row]);
        let mut control = owned_book();
        control.handle("twap:state", &rows, &mut env, &view);
        assert_eq!(control.ids().count(), usize::from(generation == Some(7)));

        let mut worker = Command::new("node")
            .args([
                "--import",
                "tsx",
                "packages/safe-cex/src/__tests__/fixtures/twapWorkerRestore.ts",
            ])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("worker contract test needs Node and the workspace's Yarn dependencies");
        worker
            .stdin
            .take()
            .unwrap()
            .write_all(
                json!({ "rows": rows, "now": env.now })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = worker.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "TypeScript wrapper failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let forwarded: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(forwarded.as_array().unwrap().len(), 1);
        let mut actual = owned_book();
        actual.handle("twap:state", &forwarded, &mut env, &view);
        assert_eq!(
            actual.serialize_state(),
            control.serialize_state(),
            "generation {generation:?}: the actual TS boundary must preserve native restore eligibility"
        );
        assert_eq!(forwarded[0]["ownerId"], rows[0]["ownerId"]);
        assert_eq!(
            forwarded[0].get("ownerGeneration"),
            rows[0].get("ownerGeneration"),
            "never replace missing/stale persisted generation evidence"
        );
        env.advance(15_001.);
        assert_eq!(
            market_amounts(&actual.wake(&mut env, &view)),
            if generation == Some(7) {
                vec![0.5]
            } else {
                vec![]
            }
        );
    }
}

#[test]
fn restored_owner_round_trip_adds_generation_to_the_worker_fixture() {
    let mut expected: Value = serde_json::from_str(include_str!(
        "../../../../packages/safe-cex/src/__tests__/fixtures/twap-owner-state.json"
    ))
    .unwrap();
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut legacy = expected["retained-A"].clone();
    legacy["id"] = json!("retained-A");
    book.handle("twap:state", &json!([legacy]), &mut env, &view);
    assert_eq!(
        book.serialize_state(),
        json!({}),
        "an ownerId alone cannot prove generation"
    );
    expected["retained-A"]["ownerGeneration"] = json!(7);
    let mut row = expected["retained-A"].clone();
    row["id"] = json!("retained-A");
    let effects = book.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(book.serialize_state(), expected);
    assert_eq!(state_updates(&effects), vec![&expected]);
    book.handle("twap:pause", &json!("retained-A"), &mut env, &view);
    let mut row = book.serialize_state()["retained-A"].clone();
    row["id"] = json!("retained-A");
    let mut reloaded = owned_book();
    reloaded.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(reloaded.serialize_state()["retained-A"]["ownerId"], "A");
}

#[test]
fn native_untagged_rows_are_dropped_without_adopting_the_current_owner() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut row = persisted("legacy", "paused", 0.);
    row.as_object_mut().unwrap().remove("ownerId");
    row.as_object_mut().unwrap().remove("ownerGeneration");
    book.handle("twap:state", &json!([row]), &mut env, &view);
    let snapshot = book.serialize_state();
    assert_eq!(snapshot, json!({}));
    assert!(book.ids().next().is_none());
}

#[test]
fn restore_resumes_running_twaps_and_skips_known_ids() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (existing, _) = start(&mut book, &mut env, &view, json!({}));
    let due = env.now + 5_000.0;
    let restore = json!([
        persisted(&existing, "paused", 0.0),
        persisted("r1", "running", due),
        persisted("p1", "paused", due)
    ]);
    book.handle("twap:state", &restore, &mut env, &view);
    assert_eq!(
        book.get(&existing).unwrap().state.status,
        TwapStatus::Running
    );
    assert_eq!(book.get("r1").unwrap().state.status, TwapStatus::Running);
    assert_eq!(book.get("p1").unwrap().state.status, TwapStatus::Paused);
    env.advance(5_000.0);
    let effects = book.wake(&mut env, &view);
    assert_eq!(
        placements(&effects).len(),
        1,
        "only the restored running TWAP is due"
    );
}

#[test]
fn restore_stops_at_the_first_malformed_entry() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let restore =
        json!([persisted("a", "paused", 0.0), { "id": "bad" }, persisted("b", "paused", 0.0)]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert!(book.get("a").is_some());
    assert!(book.get("b").is_none());
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, TwapEffect::Diagnostic { .. }))
    );
}

#[test]
fn active_twaps_matches_the_postprocessor_projection() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, _) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "side": "sell", "size": 2 }),
    );
    start(
        &mut book,
        &mut env,
        &view,
        json!({ "accountName": "other" }),
    );
    let active = book.active_twaps("test-account");
    assert_eq!(active.len(), 1);
    assert_eq!(active[0]["id"], json!(id));
    assert_eq!(active[0]["size"], json!(-2.0));
    assert_eq!(active[0]["symbol"], "BTCUSDT");
    assert_eq!(active[0]["status"], "running");
    assert_eq!(active[0]["chaserDelay"], 1000);
    assert_eq!(active[0]["currentOrderIds"], json!([]));
}

#[test]
fn options_round_trip_unknown_fields() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, _) = start(&mut book, &mut env, &view, json!({ "clientTag": "x" }));
    assert_eq!(book.serialize_state()[&id]["options"]["clientTag"], "x");
}

// --- client input bounds (server-only; TS trusted its own worker) ---

fn logs(effects: &[TwapEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::Log { message } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn start_refuses_out_of_bounds_options_before_building_slices() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    for bad in [
        json!({ "ordersCount": 0 }),
        json!({ "ordersCount": 1001 }),
        json!({ "ordersCount": u32::MAX }),
        json!({ "size": 0 }),
        json!({ "size": -1 }),
        json!({ "duration": 0 }),
        json!({ "duration": -5 }),
        json!({ "randomnessPercentage": 100.5 }),
        json!({ "randomnessPercentage": -1 }),
        json!({ "timeoutPct": 101 }),
        json!({ "timeoutPct": -1 }),
        json!({ "chaserDelay": -1 }),
    ] {
        let effects = book.handle("twap:start", &options(bad.clone()), &mut env, &view);
        assert_eq!(logs(&effects).len(), 1, "{bad}");
        assert!(
            logs(&effects)[0].starts_with("TWAP BTCUSDT rejected: "),
            "{bad}"
        );
        assert!(placements(&effects).is_empty(), "{bad}");
        assert_eq!(book.ids().count(), 0, "{bad}");
    }
}

#[test]
fn start_accepts_the_bounds_themselves() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    for good in [
        json!({ "ordersCount": 1 }),
        json!({ "ordersCount": safe_cex_algo_twap::MAX_ORDERS_COUNT }),
        json!({ "randomnessPercentage": 100 }),
        json!({ "useChaserOrders": true, "timeoutPct": 0 }),
        json!({ "useChaserOrders": true, "timeoutPct": 100 }),
    ] {
        let effects = book.handle("twap:start", &options(good.clone()), &mut env, &view);
        assert!(logs(&effects).is_empty(), "{good}");
    }
}

#[test]
fn restore_skips_entries_outside_the_bounds_and_keeps_going() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut oversized = persisted("big", "paused", 0.0);
    oversized["state"]["ordersSizes"] = json!(vec![0.1; 6]);
    let mut miscounted = persisted("count", "paused", 0.0);
    miscounted["state"]["ordersCount"] = json!(u32::MAX);
    let mut unbounded = persisted("opts", "paused", 0.0);
    unbounded["options"]["ordersCount"] = json!(u32::MAX);
    let restore = json!([
        oversized,
        miscounted,
        unbounded,
        persisted("ok", "paused", 0.0)
    ]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert_eq!(logs(&effects).len(), 3);
    assert_eq!(book.ids().collect::<Vec<_>>(), ["ok"]);
}

#[test]
fn an_unknown_placement_outcome_halts_the_action() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    let settled = settle_all(
        &mut book,
        &mut env,
        &id,
        &effects,
        Err(SliceFailure::new("timeout").outcome(safe_cex_engine::error::Outcome::Unknown)),
    );
    assert_eq!(
        logs(&settled),
        ["The outcome is unknown. Check Open Orders"]
    );
    assert!(book.get(&id).is_none());
    assert!(book.wake_at().is_none());
}

#[test]
fn an_account_runs_at_most_twenty_twaps() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    for _ in 0..safe_cex_algo_twap::MAX_TWAPS_PER_ACCOUNT {
        let effects = book.handle("twap:start", &options(json!({})), &mut env, &view);
        assert!(logs(&effects).is_empty());
    }
    let effects = book.handle("twap:start", &options(json!({})), &mut env, &view);
    assert_eq!(
        logs(&effects),
        ["TWAP BTCUSDT rejected: at most 20 TWAPs can run per account"]
    );
    assert!(placements(&effects).is_empty());
    let restored = book.handle(
        "twap:state",
        &json!([persisted("extra", "paused", 0.0)]),
        &mut env,
        &view,
    );
    assert_eq!(
        logs(&restored),
        ["TWAP BTCUSDT not restored: at most 20 TWAPs can run per account"]
    );

    // A stopped TWAP frees its place.
    let first = book.ids().next().unwrap().to_owned();
    book.handle("twap:stop", &json!(first), &mut env, &view);
    let effects = book.handle("twap:start", &options(json!({})), &mut env, &view);
    assert!(logs(&effects).is_empty());
    assert_eq!(book.ids().count(), 20);
}

// Restore safety: twap:state replays one tab's sessionStorage, which can be
// stale, duplicated into another tab, or re-sent on every account load.

const SLICE_MS: f64 = 120_000.0; // duration 10 min / 5 slices

fn diagnostics(effects: &[TwapEffect]) -> Vec<&str> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::Diagnostic { message } => Some(message.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn restore_refuses_finished_unstarted_and_empty_twaps() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let due = env.now + 5_000.0;
    let mut empty = persisted("empty", "running", due);
    empty["state"]["ordersSizes"] = json!([]);
    let restore = json!([
        persisted("done", "completed", due),
        persisted("idle", "idle", due),
        empty
    ]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert_eq!(
        logs(&effects),
        [
            "TWAP BTCUSDT not restored: it already completed",
            "TWAP BTCUSDT not restored: it never started",
            "TWAP BTCUSDT not restored: no slices remain",
        ]
    );
    assert_eq!(book.ids().count(), 0);
    env.advance(SLICE_MS * 10.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn restore_refuses_slices_beyond_the_remaining_size_or_count() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let due = env.now + 5_000.0;
    let mut inflated = persisted("inflated", "running", due);
    inflated["state"]["ordersSizes"] = json!([0.6, 0.6]);
    let mut overcounted = persisted("overcounted", "running", due);
    overcounted["state"]["ordersExecuted"] = json!(4);
    let mut negative = persisted("negative", "running", due);
    negative["state"]["ordersSizes"] = json!([0.5, -0.5]);
    let restore = json!([
        inflated,
        overcounted,
        negative,
        persisted("ok", "running", due)
    ]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert_eq!(
        logs(&effects),
        [
            "TWAP BTCUSDT not restored: remaining slices exceed the TWAP size",
            "TWAP BTCUSDT not restored: remaining and executed slices exceed ordersCount",
            "TWAP BTCUSDT not restored: restored slice sizes must be finite and non-negative",
        ]
    );
    assert_eq!(book.ids().collect::<Vec<_>>(), ["ok"]);
    // Only what remained is placed: 0.5 + 0.5, never more than the size.
    let mut placed = Vec::new();
    for _ in 0..4 {
        env.advance(SLICE_MS);
        let effects = book.wake(&mut env, &view);
        placed.extend(market_amounts(&effects));
        settle_all(&mut book, &mut env, "ok", &effects, Ok(vec!["o".into()]));
    }
    assert_eq!(placed, [0.5, 0.5]);
    assert!(book.get("ok").is_none(), "completed after its last slice");
}

#[test]
fn restore_refuses_a_running_twap_whose_schedule_lapsed() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let lapsed_at = env.now - SLICE_MS - safe_cex_algo_twap::SCHEDULE_GRACE_MS - 1.0;
    let within = env.now - SLICE_MS;
    let restore = json!([
        persisted("lapsed", "running", lapsed_at),
        persisted("fresh", "running", within),
        // Paused TWAPs keep their place; they place nothing until resumed.
        persisted("paused", "paused", lapsed_at)
    ]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert_eq!(
        logs(&effects),
        ["TWAP BTCUSDT not restored: its schedule lapsed, so no client was running it"]
    );
    assert_eq!(book.ids().collect::<Vec<_>>(), ["fresh", "paused"]);
}

#[test]
fn a_late_restore_resumes_pacing_without_a_catch_up_burst() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let restore = json!([persisted("late", "running", env.now - SLICE_MS)]);
    book.handle("twap:state", &restore, &mut env, &view);
    // One overdue slice now, then the normal interval, never a backlog.
    let first = book.wake(&mut env, &view);
    assert_eq!(market_amounts(&first), [0.5]);
    settle_all(&mut book, &mut env, "late", &first, Ok(vec!["o1".into()]));
    env.advance(SLICE_MS - 1.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
    env.advance(1.0);
    assert_eq!(market_amounts(&book.wake(&mut env, &view)), [0.5]);
}

#[test]
fn a_resumed_paused_twap_paces_from_now() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let restore = json!([persisted("p", "paused", env.now - 3_600_000.0)]);
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    assert!(placements(&effects).is_empty());
    env.advance(SLICE_MS * 5.0);
    assert!(
        placements(&book.wake(&mut env, &view)).is_empty(),
        "paused places nothing"
    );
    book.handle("twap:resume", &json!("p"), &mut env, &view);
    let first = book.wake(&mut env, &view);
    assert_eq!(market_amounts(&first), [0.5]);
    settle_all(&mut book, &mut env, "p", &first, Ok(vec!["o1".into()]));
    env.advance(SLICE_MS - 1.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn two_holders_restoring_the_same_twap_run_it_once() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let due = env.now + 5_000.0;
    // Two tabs (or a re-sent load) carry the same rows; one carries it twice.
    let rows = json!([
        persisted("t", "running", due),
        persisted("t", "running", due)
    ]);
    book.handle("twap:state", &rows, &mut env, &view);
    book.handle("twap:state", &rows, &mut env, &view);
    assert_eq!(book.ids().collect::<Vec<_>>(), ["t"]);
    env.advance(5_000.0);
    assert_eq!(placements(&book.wake(&mut env, &view)).len(), 1);
}

#[test]
fn restore_never_revives_a_stopped_or_finished_twap() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    // A TWAP the server started and the user stopped from another tab.
    let (stopped, first) = start(&mut book, &mut env, &view, json!({}));
    let stale = book.serialize_state()[&stopped].clone();
    book.handle("twap:stop", &json!(stopped), &mut env, &view);
    settle_all(&mut book, &mut env, &stopped, &first, Ok(vec!["o".into()]));
    // A TWAP that ran to its end.
    let restore = json!([persisted("ran", "running", env.now + 1_000.0)]);
    book.handle("twap:state", &restore, &mut env, &view);
    for _ in 0..3 {
        env.advance(SLICE_MS);
        let effects = book.wake(&mut env, &view);
        settle_all(&mut book, &mut env, "ran", &effects, Ok(vec!["o".into()]));
    }
    assert!(book.get("ran").is_none());

    let resurrect = json!([
        { "id": stopped, "options": stale["options"], "state": stale["state"] },
        persisted("ran", "running", env.now + 1_000.0)
    ]);
    let effects = book.handle("twap:state", &resurrect, &mut env, &view);
    assert_eq!(
        diagnostics(&effects),
        [
            format!("TWAP {stopped} not restored: it already finished"),
            "TWAP ran not restored: it already finished".to_owned(),
        ]
    );
    assert_eq!(book.ids().count(), 0);
    env.advance(SLICE_MS * 10.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn restore_never_replaces_a_twap_finishing_its_last_placement() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 1 }));
    let stale = json!({ "id": id, "options": options(json!({ "ordersCount": 1 })), "state": {
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": [1.0], "side": "buy", "status": "running", "ordersCount": 1,
        "ordersExecuted": 0, "nextOrderAt": env.now,
    }});
    // Stopped while its only slice is in flight; the entry lingers until it settles.
    book.handle("twap:stop", &json!(id), &mut env, &view);
    let restored = book.handle("twap:state", &json!([stale]), &mut env, &view);
    assert!(placements(&restored).is_empty());
    settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["o".into()]));
    env.advance(SLICE_MS * 2.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
    assert!(book.get(&id).is_none());
}

#[test]
fn a_twap_naming_another_account_is_refused_visibly() {
    // It would trade on this account yet never appear in its activeTWAPs.
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let started = book.handle(
        "twap:start",
        &options(json!({ "accountName": "other-account" })),
        &mut env,
        &view,
    );
    assert_eq!(
        logs(&started),
        [r#"TWAP BTCUSDT rejected: it names account "other-account", not "test-account""#]
    );
    assert!(placements(&started).is_empty());

    let mut foreign = persisted("foreign", "running", env.now + 1_000.0);
    foreign["options"]["accountName"] = json!("other-account");
    foreign["state"]["accountName"] = json!("other-account");
    let restored = book.handle("twap:state", &json!([foreign]), &mut env, &view);
    assert_eq!(
        logs(&restored),
        [r#"TWAP BTCUSDT not restored: it names account "other-account", not "test-account""#]
    );
    assert_eq!(book.ids().count(), 0);
    env.advance(SLICE_MS * 10.0);
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn restore_refuses_state_filed_under_another_account() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut row = persisted("moved", "running", env.now + 1_000.0);
    row["state"]["accountName"] = json!("other-account");
    let effects = book.handle("twap:state", &json!([row]), &mut env, &view);
    assert_eq!(
        logs(&effects),
        ["TWAP BTCUSDT not restored: restored state names another account than its options"]
    );
    assert_eq!(book.ids().count(), 0);
}

// --- TwapState.inFlight (TwapInstance.reconcileInFlight) ---

#[test]
fn a_placing_slice_is_persisted_in_flight_with_its_client_id_until_it_settles() {
    let mut env = Env::new();
    let view = View {
        client_id: Some("x-caller-1".into()),
        ..View::default()
    };
    let mut book = owned_book();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    assert_eq!(
        placements(&effects)[0].1.client_order_id.as_deref(),
        Some("x-caller-1")
    );
    let persisted = state_updates(&effects).last().copied().unwrap().clone();
    assert_eq!(
        persisted[&id]["state"]["inFlight"],
        json!({ "size": 0.5, "clientOrderId": "x-caller-1" })
    );
    assert_eq!(persisted[&id]["state"]["ordersSizes"], json!([0.5]));

    let settled = settle_all(&mut book, &mut env, &id, &effects, Ok(vec!["o1".into()]));
    let persisted = state_updates(&settled).last().copied().unwrap().clone();
    assert!(persisted[&id]["state"].get("inFlight").is_none());
    assert_eq!(persisted[&id]["state"]["ordersExecuted"], 1);
}

fn in_flight_row(env: &Env, sizes: Value, executed: u32, in_flight: Value) -> Value {
    json!([{ "id": "restored", "ownerId": "A", "ownerGeneration": 7, "options": options(json!({ "ordersCount": 2 })), "state": {
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": sizes, "side": "buy", "status": "running", "ordersCount": 2,
        "ordersExecuted": executed, "nextOrderAt": env.now, "inFlight": in_flight,
    } }])
}

#[test]
fn an_unreconciled_in_flight_slice_is_counted_and_never_placed_again() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let restore = in_flight_row(
        &env,
        json!([0.5]),
        0,
        json!({ "size": 0.5, "clientOrderId": "c1" }),
    );
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:291-310.
    assert_eq!(
        logs(&effects),
        ["The outcome is unknown. Check Open Orders"]
    );
    assert!(book.get("restored").is_none());
    assert!(placements(&book.wake(&mut env, &view)).is_empty());
}

#[test]
fn a_last_slice_in_flight_is_restored_and_completes_without_placing() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let restore = in_flight_row(&env, json!([]), 1, json!({ "size": 0.5 }));
    book.handle("twap:state", &restore, &mut env, &view);
    assert!(book.get("restored").is_none());
    let woken = book.wake(&mut env, &view);
    assert!(placements(&woken).is_empty());
    assert!(
        book.get("restored").is_none(),
        "nothing remains, so it completed"
    );
}

#[test]
fn an_in_flight_slice_counts_toward_the_restore_bounds() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    for (sizes, executed, in_flight, reason) in [
        (
            json!([0.5]),
            1,
            json!({ "size": 0.5 }),
            "remaining and executed slices exceed ordersCount",
        ),
        (
            json!([0.6]),
            0,
            json!({ "size": 0.6 }),
            "remaining slices exceed the TWAP size",
        ),
        (
            json!([0.5]),
            0,
            json!({ "size": 0 }),
            "its in-flight slice is invalid",
        ),
    ] {
        let effects = book.handle(
            "twap:state",
            &in_flight_row(&env, sizes, executed, in_flight),
            &mut env,
            &view,
        );
        assert!(
            effects.contains(&TwapEffect::Log {
                message: format!("TWAP BTCUSDT not restored: {reason}")
            }),
            "{reason}: {effects:?}"
        );
    }
    let mut lapsed = in_flight_row(&env, json!([0.5]), 0, json!({ "size": 0.5 }));
    lapsed[0]["state"]["nextOrderAt"] = json!(env.now - 600_000.0);
    let effects = book.handle("twap:state", &lapsed, &mut env, &view);
    assert!(effects.contains(&TwapEffect::Log {
        message: "TWAP BTCUSDT not restored: its schedule lapsed, so no client was running it; a slice may have been in flight, check open orders and fills".into()
    }));
    assert!(book.get("restored").is_none());
}

#[test]
fn a_restored_paused_twap_is_persisted_so_the_host_sees_it_adopted() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = owned_book();
    let mut restore = in_flight_row(&env, json!([0.5, 0.5]), 0, Value::Null);
    restore[0]["state"]["status"] = json!("paused");
    let effects = book.handle("twap:state", &restore, &mut env, &view);
    let persisted = state_updates(&effects).last().copied().unwrap().clone();
    assert_eq!(persisted["restored"]["state"]["status"], "paused");
}

#[test]
fn a_late_remainder_success_updates_unknown_journal_without_restarting() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:181-194,255-280.
    let (mut book, mut env, mut view, id, first_chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(60_000.0);
    book.wake(&mut env, &view);
    let first = settle_chaser(&mut book, &mut env, &mut view, &first_chaser, 0.2);
    assert_eq!(market_amounts(&first), [0.2]);
    env.advance(59_750.0);
    book.wake(&mut env, &view);
    let second_chaser = chaser_ids(&view).pop().unwrap();
    view.chasers.insert(
        second_chaser.clone(),
        ChaserProgress {
            remaining_size: 0.2,
            ..Default::default()
        },
    );
    env.advance(60_000.0);
    book.wake(&mut env, &view);
    let second = settle_chaser(&mut book, &mut env, &mut view, &second_chaser, 0.2);
    assert_eq!(market_amounts(&second), [0.2]);
    let unknown = book.remainder_settled(
        &id,
        3,
        Err(SliceFailure::new("timeout").outcome(safe_cex_engine::error::Outcome::Unknown)),
    );
    assert_eq!(
        logs(&unknown),
        ["The outcome is unknown. Check Open Orders"]
    );
    let late = book.remainder_settled(&id, 1, Ok(vec!["late-native".to_owned()]));
    let error = late
        .iter()
        .find_map(|effect| match effect {
            TwapEffect::Outcome { error, .. } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["late-native"]);
    let rows = error.elements.as_ref().unwrap();
    assert!(
        matches!(&rows[1].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["late-native"])
    );
    assert!(matches!(
        &rows[3].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
    assert!(placements(&late).is_empty());
    env.advance(600_000.0);
    assert!(book.wake(&mut env, &view).is_empty());
    assert!(book.get(&id).is_none());
}

#[test]
fn a_skipped_zero_slice_keeps_unknown_element_indices_aligned() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:249-280.
    let (mut env, mut view) = (Env::new(), View::default());
    view.step = Some(1.0);
    let mut book = TwapBook::new("test-account");
    let (id, first) = start(
        &mut book,
        &mut env,
        &view,
        json!({ "size": 1.5, "ordersCount": 3 }),
    );
    assert!(placements(&first).is_empty());
    env.advance(book.wake_at().unwrap() - env.now);
    let second = book.wake(&mut env, &view);
    assert_eq!(market_amounts(&second), [1.0]);
    let settled = settle_all(
        &mut book,
        &mut env,
        &id,
        &second,
        Err(SliceFailure::new("timeout").outcome(safe_cex_engine::error::Outcome::Unknown)),
    );
    let error = settled
        .iter()
        .find_map(|effect| match effect {
            TwapEffect::Outcome { error, .. } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.message, "The outcome is unknown. Check Open Orders");
    let rows = error.elements.as_ref().unwrap();
    assert!(matches!(
        &rows[0].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { .. }
    ));
    assert!(matches!(
        &rows[1].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
    assert!(matches!(
        &rows[2].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { .. }
    ));
    assert!(book.wake_at().is_none());
}
