//! Ports of the TS TWAP suites: TwapInstance, TwapInstance.timeout,
//! TwapInstance.termination-race, TwapManager and TwapHandler.

use std::collections::HashMap;

use safe_cex_algo_twap::{
    ChaserProgress, OrderKind, OrderSide, TickerQuote, TwapBook, TwapEffect, TwapEnv,
    TwapMarketView, TwapStatus,
};
use serde_json::{Value, json};

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
}

impl TwapMarketView for View {
    fn ticker(&self, _symbol: &str) -> Option<TickerQuote> {
        self.ticker
    }
    fn chaser(&self, chaser_id: &str) -> Option<ChaserProgress> {
        self.chasers.get(chaser_id).copied()
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

fn chaser_ids(effects: &[TwapEffect]) -> Vec<String> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            TwapEffect::StartChaser { request, .. } => Some(request.id.clone()),
            _ => None,
        })
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
fn settle_all(
    book: &mut TwapBook,
    env: &mut Env,
    id: &str,
    effects: &[TwapEffect],
    outcome: Result<Vec<String>, String>,
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
    let state = json!({
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": [0.2, 0.2, 0.2], "side": "buy", "status": "paused", "ordersCount": 5,
        "ordersExecuted": 2, "nextOrderAt": env.now + 10_000.0, "origin": "cli",
    });
    book.handle(
        "twap:state",
        &json!([{ "id": "existing-id", "options": options(json!({})), "state": state }]),
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
    let mut book = TwapBook::new();
    let state = json!({
        "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
        "ordersSizes": [0.2], "side": "buy", "status": "running", "ordersCount": 5,
        "ordersExecuted": 2, "nextOrderAt": env.now + 10_000.0,
    });
    book.handle(
        "twap:state",
        &json!([{ "id": "old", "options": options(json!({})), "state": state }]),
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
fn failed_slice_logs_and_advances() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    let settled = settle_all(
        &mut book,
        &mut env,
        &id,
        &effects,
        Err("insufficient margin".into()),
    );
    assert!(settled.contains(&TwapEffect::Log {
        message: "TWAP BTCUSDT slice failed: insufficient margin".into()
    }));
    assert_eq!(book.get(&id).unwrap().state.orders_executed, 1);
    assert!(book.wake_at().is_some());
}

#[test]
fn pause_stops_the_schedule_and_resume_keeps_next_order_at() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    book.handle("twap:stop", &json!(id), &mut env, &view);
    let settled = settle_all(&mut book, &mut env, &id, &effects, Err("boom".into()));
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
    let mut book = TwapBook::new();
    let (id, effects) = start(&mut book, &mut env, &view, chase(overrides));
    let chasers = chaser_ids(&effects);
    assert_eq!(chasers.len(), 1);
    assert!(market_amounts(&effects).is_empty());
    view.chasers.insert(
        chasers[0].clone(),
        ChaserProgress {
            remaining_size: 0.2,
            stopped: false,
        },
    );
    (book, env, view, id, chasers[0].clone())
}

#[test]
fn chaser_slice_request_matches_ts() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
    let (_, effects) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "reduceOnly": true, "chaserDelay": 750 })),
    );
    let request = effects.iter().find_map(|e| match e {
        TwapEffect::StartChaser { request, .. } => Some(request.clone()),
        _ => None,
    });
    let request = request.unwrap();
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
fn no_timeout_without_timeout_pct() {
    let (mut book, mut env, view, _, _) = start_chase(json!({}));
    env.advance(60_000.0);
    assert_eq!(book.wake_at(), Some(env.now + 60_000.0));
    env.advance(60_000.0);
    let effects = book.wake(&mut env, &view);
    assert!(removed_chasers(&effects).is_empty());
    assert!(market_amounts(&effects).is_empty());
    assert_eq!(chaser_ids(&effects).len(), 1);
}

#[test]
fn timeout_market_cancels_chaser_and_markets_the_remainder() {
    let (mut book, mut env, view, _, chaser) =
        start_chase(json!({ "timeoutPct": 50, "timeoutAction": "market" }));
    env.advance(59_999.0);
    assert!(book.wake(&mut env, &view).is_empty());
    env.advance(1.0);
    let effects = book.wake(&mut env, &view);
    assert_eq!(removed_chasers(&effects), vec![chaser]);
    let markets = placements(&effects);
    assert_eq!(markets.len(), 1);
    let (token, request) = markets[0];
    assert_eq!(token, None);
    assert_eq!(
        (request.side, request.amount, request.reduce_only),
        (OrderSide::Buy, 0.2, false)
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
        chaser,
        ChaserProgress {
            remaining_size: 0.2,
            stopped: true,
        },
    );
    env.advance(60_000.0);
    assert_eq!(market_amounts(&book.wake(&mut env, &view)), vec![0.2]);
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
    let (mut book, mut env, view, _, _) =
        start_chase(json!({ "reduceOnly": true, "timeoutPct": 50 }));
    env.advance(60_000.0);
    let effects = book.wake(&mut env, &view);
    assert!(placements(&effects)[0].1.reduce_only);
}

#[test]
fn last_slice_timeout_still_fires_after_completion() {
    let (mut env, mut view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
    let (id, effects) = start(
        &mut book,
        &mut env,
        &view,
        chase(json!({ "ordersCount": 1, "timeoutPct": 50 })),
    );
    assert!(
        book.get(&id).is_none(),
        "a one-slice TWAP completes on its first slice"
    );
    view.chasers.insert(
        chaser_ids(&effects)[0].clone(),
        ChaserProgress {
            remaining_size: 1.0,
            stopped: false,
        },
    );
    env.advance(300_000.0);
    assert_eq!(market_amounts(&book.wake(&mut env, &view)), vec![1.0]);
    assert_eq!(book.wake_at(), None);
}

// --- TwapManager.test.ts ---

#[test]
fn serialize_state_skips_completed_twaps() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
        "options": options(json!({})),
        "state": {
            "accountName": "test-account", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1.0,
            "ordersSizes": [0.5, 0.5], "side": "buy", "status": status, "ordersCount": 5,
            "ordersExecuted": 3, "nextOrderAt": next_order_at,
        },
    })
}

#[test]
fn restore_resumes_running_twaps_and_skips_known_ids() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
    let mut book = TwapBook::new();
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
fn an_unknown_placement_outcome_still_advances_the_slice() {
    // TS parity: the slice counts as executed and the next one is scheduled;
    // the failure log says the order may be live.
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
    let (id, effects) = start(&mut book, &mut env, &view, json!({ "ordersCount": 2 }));
    let settled = settle_all(
        &mut book,
        &mut env,
        &id,
        &effects,
        Err("outcome unknown, the order may be live: timeout".into()),
    );
    assert_eq!(
        logs(&settled),
        ["TWAP BTCUSDT slice failed: outcome unknown, the order may be live: timeout"]
    );
    assert_eq!(book.get(&id).unwrap().state.orders_executed, 1);
    assert!(book.wake_at().is_some());
}

#[test]
fn an_account_runs_at_most_twenty_twaps() {
    let (mut env, view) = (Env::new(), View::default());
    let mut book = TwapBook::new();
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
