//! After-failure parity at the real factory/AccountPort boundary.
//! The runtime adapter: ops in, engine commands and wire replies out.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algo_twap::ChaserProgress;
use safe_cex_algo_twap::ChaserRequest;
use safe_cex_algo_twap::TwapOwner;
use safe_cex_algo_twap::factory::{TwapChasers, TwapFactory};
use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::LocalSet;

mod support;
use support::{ClockGuard, ENGINE_UNKNOWN_TEXT};

// These parity fixtures exercise restore outcomes under an authenticated host.
// Keep valid ownership explicit so the owner gate does not mask the outcome assertions.
const RESTORE_OWNER: &str = "parity-owner";
const RESTORE_GENERATION: u64 = 7;

#[derive(Debug, PartialEq)]
enum RecordedWrite {
    Order(f64),
    Chase(String),
    CancelChase(String),
}

#[derive(Default)]
struct Port {
    writes: Rc<RefCell<Vec<RecordedWrite>>>,
    store: RefCell<AlgoStore>,
    ids: Cell<CommandId>,
    commands: RefCell<Vec<Command>>,
    emits: RefCell<Vec<(String, Value)>>,
    fail: Cell<bool>,
    fail_outcome: Cell<Option<safe_cex_engine::error::Outcome>>,
    answer_ms: Cell<u64>,
    next_failure: RefCell<Option<VenueError>>,
}

impl AccountPort for Port {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        mpsc::unbounded_channel().1
    }
    fn next_command_id(&self) -> CommandId {
        self.ids.set(self.ids.get() + 1);
        self.ids.get()
    }
    fn command(&self, command: Command) -> CommandFuture {
        if let Command::PlaceOrders { orders, .. } = &command {
            self.writes.borrow_mut().extend(
                orders
                    .iter()
                    .map(|order| RecordedWrite::Order(order.amount)),
            );
        }
        self.commands.borrow_mut().push(command);
        let fail = self.fail.get();
        let outcome = self.fail_outcome.get();
        let answer_ms = self.answer_ms.get();
        let next_failure = self.next_failure.borrow_mut().take();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(answer_ms)).await;
            if let Some(error) = next_failure {
                return Err(error);
            }
            if fail {
                Err(VenueError {
                    message: "rejected".into(),
                    outcome,
                    ..Default::default()
                })
            } else {
                Ok(r#"["order-1"]"#.to_owned())
            }
        })
    }
    fn emit(&self, op: &str, data: String) {
        self.emits
            .borrow_mut()
            .push((op.to_owned(), serde_json::from_str(&data).unwrap()));
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
}

#[derive(Default)]
struct Chasers {
    started: RefCell<Vec<ChaserRequest>>,
}

impl TwapChasers for Chasers {
    fn halt(&self, _cx: &AlgoCx, _id: &str) {}
    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
        self.started.borrow_mut().push(request.clone());
        Ok(())
    }
    fn progress(&self, _chaser_id: &str) -> Option<ChaserProgress> {
        None
    }
    fn remove(&self, _cx: &AlgoCx, _chaser_id: &str) {}
}

fn twap_options(extra: Value) -> String {
    let mut options = json!({
        "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1, "duration": 1,
        "ordersCount": 2, "side": "buy", "chaserDelay": 1000, "reduceOnly": false,
        "randomnessPercentage": 0, "useLimitOrders": false, "useChaserOrders": false,
    });
    if let (Value::Object(options), Value::Object(extra)) = (&mut options, extra) {
        options.extend(extra);
    }
    options.to_string()
}

fn placed_amounts(port: &Port) -> Vec<f64> {
    port.commands
        .borrow()
        .iter()
        .flat_map(|command| match command {
            Command::PlaceOrders { orders, .. } => {
                orders.iter().map(|order| order.amount).collect()
            }
            _ => Vec::new(),
        })
        .collect()
}

async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

fn market() -> Market {
    Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.5,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        pretty_symbol: None,
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn fixture(venue: &str) -> (Rc<Port>, AlgoCx, TwapFactory) {
    let port = Rc::new(Port::default());
    {
        let mut store = port.store.borrow_mut();
        store.venue = venue.to_owned();
        store.markets.insert("BTCUSDT".to_owned(), market());
    }
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = TwapFactory::with_owner(
        "a",
        TwapOwner {
            owner_id: RESTORE_OWNER.into(),
            generation: RESTORE_GENERATION,
        },
        cx.clone(),
        Rc::new(Chasers::default()),
    );
    (port, cx, factory)
}

async fn advance(seconds: u64) {
    tokio::time::advance(Duration::from_secs(seconds)).await;
    settle().await;
}

async fn check_continuation(
    venue: &str,
    restored_status: Option<&str>,
    outcome: safe_cex_engine::error::Outcome,
) {
    let _clock = ClockGuard::new();
    let (port, cx, factory) = fixture(venue);
    if venue == "bingx" {
        port.store
            .borrow_mut()
            .markets
            .get_mut("BTCUSDT")
            .unwrap()
            .limits
            .amount
            .max = f64::INFINITY;
    }
    let options: Value = serde_json::from_str(&twap_options(
        json!({ "size": 3, "ordersCount": 3, "reduceOnly": true }),
    ))
    .unwrap();
    if let Some(status) = restored_status {
        let restore = json!([{
            "id": "restored", "ownerId": RESTORE_OWNER, "ownerGeneration": RESTORE_GENERATION, "options": options,
            "state": { "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" },
                "size": 3, "ordersSizes": [1, 1, 1], "side": "buy", "status": status,
                "ordersCount": 3, "ordersExecuted": 0, "nextOrderAt": cx.now_ms() }
        }]);
        factory
            .handle("twap:state", &restore.to_string(), &cx)
            .unwrap();
        if status == "paused" {
            assert!(port.commands.borrow().is_empty());
            factory.handle("twap:resume", "\"restored\"", &cx).unwrap();
        }
        advance(1).await;
    } else {
        factory
            .start(serde_json::from_value(options).unwrap())
            .unwrap();
        settle().await;
    }
    assert_eq!(placed_amounts(&port), [1.0]);
    port.fail.set(true);
    port.fail_outcome.set(Some(outcome));
    advance(21).await;
    assert_eq!(placed_amounts(&port), [1.0, 1.0]);
    port.fail.set(false);
    advance(21).await;
    advance(100).await;
    assert_eq!(
        placed_amounts(&port),
        [1.0, 1.0, 1.0],
        "legacy must release the child after the failed child"
    );
    let commands = port.commands.borrow();
    assert_eq!(
        commands.len(),
        3,
        "no resend, cancellation or repair in this direct schedule"
    );
    let mut ids = std::collections::HashSet::new();
    for command in commands.iter() {
        let Command::PlaceOrders { orders, .. } = command else {
            panic!("unexpected compensation in direct schedule")
        };
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].symbol, "BTCUSDT");
        assert_eq!(orders[0].kind, safe_cex_engine::model::OrderType::Market);
        assert_eq!(orders[0].side, safe_cex_engine::model::OrderSide::Buy);
        assert_eq!(orders[0].price, None);
        assert_eq!(orders[0].reduce_only, Some(true));
        if let Some(id) = orders[0].client_order_id.as_deref() {
            assert!(ids.insert(id));
        } else {
            assert_eq!(venue, "bingx", "retain each venue's caller-id support");
        }
    }
    assert_eq!(
        *port.writes.borrow(),
        [
            RecordedWrite::Order(1.0),
            RecordedWrite::Order(1.0),
            RecordedWrite::Order(1.0)
        ]
    );
    assert!(factory.active_twaps("a").is_empty());
    assert!(port.emits.borrow().iter().any(|(op, data)| {
        op == "error"
            && data
                .as_str()
                .is_some_and(|text| text.contains("slice failed:"))
    }));
}

#[tokio::test(start_paused = true)]
async fn fresh_schedule_continues_after_failed_child() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:80-114,249-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | scheduled-TWAP-children (audit instance.rs:499/lib.rs:221; fresh schedules, every venue)
    LocalSet::new()
        .run_until(check_continuation(
            "binance",
            None,
            safe_cex_engine::error::Outcome::Rejected,
        ))
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_rule_delegation_with_unbounded_max_continues_after_refusal() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:117-139,167,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | quantity/min/max/grid/notional/price preflight (including BingX unbounded maximum)
    LocalSet::new()
        .run_until(check_continuation(
            "bingx",
            None,
            safe_cex_engine::error::Outcome::NotSent,
        ))
        .await;
}

#[tokio::test(start_paused = true)]
async fn restored_schedule_continues_after_failed_child() {
    // TS c78bed93d packages/safe-cex/src/webworker/handlers/TwapHandler.ts:60-100; twap/TwapInstance.ts:330-345,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | restore/resume child preflight
    LocalSet::new()
        .run_until(check_continuation(
            "binance",
            Some("running"),
            safe_cex_engine::error::Outcome::Rejected,
        ))
        .await;
}

#[tokio::test(start_paused = true)]
async fn resumed_schedule_continues_after_failed_child() {
    // TS c78bed93d packages/safe-cex/src/webworker/handlers/TwapHandler.ts:60-100; twap/TwapInstance.ts:330-345,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | restore/resume child preflight
    LocalSet::new()
        .run_until(check_continuation(
            "binance",
            Some("paused"),
            safe_cex_engine::error::Outcome::NotSent,
        ))
        .await;
}

#[derive(Default)]
struct PolicyChasers {
    writes: Rc<RefCell<Vec<RecordedWrite>>>,
    started: RefCell<Vec<ChaserRequest>>,
    removed: RefCell<Vec<String>>,
    refuse_first: Cell<bool>,
    halted: RefCell<Vec<String>>,
    uncertain: Cell<bool>,
}

impl TwapChasers for PolicyChasers {
    fn halt(&self, _cx: &AlgoCx, id: &str) {
        self.halted.borrow_mut().push(id.to_owned());
    }
    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
        self.writes
            .borrow_mut()
            .push(RecordedWrite::Chase(request.id.clone()));
        self.started.borrow_mut().push(request.clone());
        if self.refuse_first.replace(false) {
            Err("handoff refused".to_owned())
        } else {
            Ok(())
        }
    }
    fn progress(&self, id: &str) -> Option<ChaserProgress> {
        Some(ChaserProgress {
            remaining_size: 1.0,
            ended: self.uncertain.get()
                || self.removed.borrow().iter().any(|removed| removed == id),
            order_may_be_open: self.uncertain.get()
                && self
                    .started
                    .borrow()
                    .first()
                    .is_some_and(|request| request.id == id),
            ..Default::default()
        })
    }
    fn remove(&self, _cx: &AlgoCx, id: &str) {
        self.writes
            .borrow_mut()
            .push(RecordedWrite::CancelChase(id.to_owned()));
        self.removed.borrow_mut().push(id.to_owned());
    }
}

#[tokio::test(start_paused = true)]
async fn chaser_handoff_failure_keeps_later_slices_scheduled() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:142-165,181-194,213-220,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | chaser handoff and compensating market remainder
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, _) = fixture("binance");
            let chasers = Rc::new(PolicyChasers {
                writes: port.writes.clone(),
                ..Default::default()
            });
            chasers.refuse_first.set(true);
            let factory = TwapFactory::new("a", cx, chasers.clone());
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({
                        "size": 3, "ordersCount": 3, "useChaserOrders": true
                    })))
                    .unwrap(),
                )
                .unwrap();
            settle().await;
            assert_eq!(chasers.started.borrow().len(), 1);
            advance(21).await;
            assert_eq!(
                chasers.started.borrow().len(),
                2,
                "handoff refusal must not veto the later child"
            );
            advance(21).await;
            assert_eq!(chasers.started.borrow().len(), 3);
            assert_eq!(
                *port.writes.borrow(),
                chasers
                    .started
                    .borrow()
                    .iter()
                    .map(|request| RecordedWrite::Chase(request.id.clone()))
                    .collect::<Vec<_>>()
            );
            assert!(port.commands.borrow().is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn failed_compensating_market_remainder_keeps_next_chaser_scheduled() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:142-165,181-194,213-220,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | chaser handoff and compensating market remainder
    for outcome in [safe_cex_engine::error::Outcome::Rejected] {
        LocalSet::new()
            .run_until(async {
                let _clock = ClockGuard::new();
                let (port, cx, _) = fixture("binance");
                port.fail.set(true);
                port.fail_outcome.set(Some(outcome));
                let chasers = Rc::new(PolicyChasers {
                    writes: port.writes.clone(),
                    ..Default::default()
                });
                let factory = TwapFactory::new("a", cx, chasers.clone());
                factory
                    .start(
                        serde_json::from_str(&twap_options(json!({
                            "size": 3, "ordersCount": 3, "useChaserOrders": true,
                            "timeoutPct": 50, "timeoutAction": "market", "failAction": "market"
                        })))
                        .unwrap(),
                    )
                    .unwrap();
                settle().await;
                advance(11).await;
                assert_eq!(
                    chasers.removed.borrow().len(),
                    1,
                    "keep the compensating chaser cancellation"
                );
                assert!(
                    port.commands.borrow().is_empty(),
                    "wait for the existing chase cancellation to settle"
                );
                advance(1).await;
                assert_eq!(
                    placed_amounts(&port),
                    [1.0],
                    "keep the compensating market write"
                );
                advance(9).await;
                assert_eq!(
                    chasers.started.borrow().len(),
                    2,
                    "failed remainder must not stop the next scheduled chase"
                );
                assert_eq!(
                    port.commands.borrow().len(),
                    1,
                    "do not resend the failed remainder"
                );
                let started = chasers.started.borrow();
                assert_eq!(
                    *port.writes.borrow(),
                    [
                        RecordedWrite::Chase(started[0].id.clone()),
                        RecordedWrite::CancelChase(started[0].id.clone()),
                        RecordedWrite::Order(1.0),
                        RecordedWrite::Chase(started[1].id.clone()),
                    ]
                );
                assert!(port.emits.borrow().iter().any(|(op, data)| {
                    op == "error"
                        && data
                            .as_str()
                            .is_some_and(|text| text.contains("slice remainder failed:"))
                }));
            })
            .await;
    }
}

async fn check_unknown_halt(venue: &str, restored_status: Option<&str>) {
    let _clock = ClockGuard::new();
    let (port, cx, factory) = fixture(venue);
    if venue == "bingx" {
        port.store
            .borrow_mut()
            .markets
            .get_mut("BTCUSDT")
            .unwrap()
            .limits
            .amount
            .max = f64::INFINITY;
    }
    let options: Value =
        serde_json::from_str(&twap_options(json!({ "size": 3, "ordersCount": 3 }))).unwrap();
    if let Some(status) = restored_status {
        let rows = json!([{
            "id": "unknown-restore", "ownerId": RESTORE_OWNER, "ownerGeneration": RESTORE_GENERATION, "options": options,
            "state": { "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" },
                "size": 3, "ordersSizes": [1, 1, 1], "side": "buy", "status": status,
                "ordersCount": 3, "ordersExecuted": 0, "nextOrderAt": cx.now_ms() }
        }]);
        factory
            .handle("twap:state", &rows.to_string(), &cx)
            .unwrap();
        if status == "paused" {
            factory
                .handle("twap:resume", "\"unknown-restore\"", &cx)
                .unwrap();
        }
        advance(1).await;
    } else {
        factory
            .start(serde_json::from_value(options).unwrap())
            .unwrap();
        settle().await;
    }
    assert_eq!(placed_amounts(&port), [1.0]);
    port.fail.set(true);
    port.fail_outcome
        .set(Some(safe_cex_engine::error::Outcome::Unknown));
    advance(21).await;
    port.fail.set(false);
    advance(100).await;
    assert_eq!(
        placed_amounts(&port),
        [1.0, 1.0],
        "Unknown must halt this action, even if a later write would succeed"
    );
    assert_eq!(port.commands.borrow().len(), 2);
    assert!(factory.active_twaps("a").is_empty());
    let emits = port.emits.borrow();
    let (_, result) = emits
        .iter()
        .find(|(op, _)| op == "twap:outcome")
        .expect("indexed outcome event");
    let error = &result["error"];
    assert_eq!(error["outcome"], "unknown");
    assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
    assert_eq!(error["partial"]["orderIds"], json!(["order-1"]));
    let elements = error["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[0]["orderIds"], json!(["order-1"]));
    assert_eq!(elements[1]["status"], "unknown");
    assert_eq!(elements[1]["error"]["message"], error["message"]);
    assert_eq!(elements[2]["status"], "notSent");
    assert!(emits.iter().any(|(op, event)| op == "telemetry"
        && event["kind"] == "twap.unknown_outcome"
        && event["unknown"] == 1));
    assert!(
        emits
            .iter()
            .any(|(op, data)| op == "error" && data == &error["message"])
    );
}

#[tokio::test(start_paused = true)]
async fn unknown_exception_fresh_schedule_halts_with_outcomes_and_telemetry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:249-280.
    LocalSet::new()
        .run_until(check_unknown_halt("binance", None))
        .await;
}
#[tokio::test(start_paused = true)]
async fn unknown_exception_native_rule_delegation_halts_with_outcomes_and_telemetry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:117-139,255-280.
    LocalSet::new()
        .run_until(check_unknown_halt("bingx", None))
        .await;
}
#[tokio::test(start_paused = true)]
async fn unknown_exception_restored_schedule_halts_with_outcomes_and_telemetry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/handlers/TwapHandler.ts:95-99.
    LocalSet::new()
        .run_until(check_unknown_halt("binance", Some("running")))
        .await;
}
#[tokio::test(start_paused = true)]
async fn unknown_exception_resumed_schedule_halts_with_outcomes_and_telemetry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:330-345,255-280.
    LocalSet::new()
        .run_until(check_unknown_halt("binance", Some("paused")))
        .await;
}
#[tokio::test(start_paused = true)]
async fn unknown_exception_runtime_reply_halts_with_outcomes_and_telemetry() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    LocalSet::new()
        .run_until(check_unknown_halt("binance", None))
        .await;
}

#[tokio::test(start_paused = true)]
async fn unknown_exception_market_remainder_halts_without_next_chaser() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:181-194,213-220.
    for count in [1, 3] {
        LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, _) = fixture("binance");
            port.fail.set(true);
            port.answer_ms.set(1000);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Unknown));
            let chasers = Rc::new(PolicyChasers {
                writes: port.writes.clone(),
                ..Default::default()
            });
            let factory = TwapFactory::new("a", cx, chasers.clone());
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({
                        "size": count, "ordersCount": count, "useChaserOrders": true, "timeoutPct": 50,
                        "timeoutAction": "market", "failAction": "market"
                    })))
                    .unwrap(),
                )
                .unwrap();
            settle().await;
            advance(60 / count / 2 + 1).await;
            advance(1).await;
            advance(2).await;
            port.fail.set(false);
            advance(100).await;
            assert_eq!(
                chasers.started.borrow().len(),
                1,
                "uncertain remainder must halt new child chasers"
            );
            assert_eq!(
                port.commands.borrow().len(),
                1,
                "no replay/cancel/repair after Unknown"
            );
            let emits = port.emits.borrow();
            let (_, result) = emits
                .iter()
                .find(|(op, _)| op == "twap:outcome")
                .expect("journal event");
            let elements = result["error"]["elements"].as_array().unwrap();
            assert_eq!(elements[0]["status"], "applied");
            assert_eq!(elements[1]["status"], "unknown");
            assert!(elements[2..].iter().all(|row| row["status"] == "notSent"));
            assert_eq!(result["error"]["message"], ENGINE_UNKNOWN_TEXT);
            assert!(
                emits
                    .iter()
                    .any(|(op, data)| op == "telemetry" && data["kind"] == "twap.unknown_outcome")
            );
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn unknown_exception_restored_in_flight_halts_without_replay_or_suffix() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:291-310.
    LocalSet::new().run_until(async {
        let _clock = ClockGuard::new();
        let (port, cx, factory) = fixture("binance");
        let rows = json!([{ "id": "restored-flight", "ownerId": RESTORE_OWNER, "ownerGeneration": RESTORE_GENERATION, "options": serde_json::from_str::<Value>(&twap_options(json!({"size": 3, "ordersCount": 3}))).unwrap(),
            "state": { "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" }, "size": 3,
                "ordersSizes": [1, 1], "side": "buy", "status": "running", "ordersCount": 3,
                "ordersExecuted": 0, "nextOrderAt": cx.now_ms(), "inFlight": {"size": 1, "clientOrderId": "uncertain-client"} }
        }]);
        factory.handle("twap:state", &rows.to_string(), &cx).unwrap();
        advance(100).await;
        factory.handle("twap:resume", "\"restored-flight\"", &cx).unwrap();
        factory.handle("twap:state", &rows.to_string(), &cx).unwrap();
        advance(100).await;
        assert!(port.commands.borrow().is_empty());
        assert!(factory.active_twaps("a").is_empty());
        let emits = port.emits.borrow();
        let error = &emits.iter().find(|(op, _)| op == "twap:outcome").unwrap().1["error"];
        assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
        assert_eq!(error["elements"][0]["status"], "unknown");
        assert_eq!(error["elements"][0]["clientOrderId"], "uncertain-client");
        assert_eq!(error["elements"][1]["status"], "notSent");
        assert_eq!(error["elements"][2]["status"], "notSent");
        assert!(emits.iter().any(|(op, event)| op == "telemetry" && event["error"] == *error));
    }).await;
}

#[tokio::test(start_paused = true)]
async fn unknown_exception_chaser_halts_siblings_without_cancel_or_replacement() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:181-194,213-220,255-280.
    for orders_count in [1, 3] {
        LocalSet::new()
            .run_until(async {
                let _clock = ClockGuard::new();
                let (port, cx, _) = fixture("binance");
                let chasers = Rc::new(PolicyChasers {
                    writes: port.writes.clone(),
                    ..Default::default()
                });
                let factory = TwapFactory::new("a", cx, chasers.clone());
                factory.start(serde_json::from_str(&twap_options(json!({
                "size": orders_count, "ordersCount": orders_count, "useChaserOrders": true
            }))).unwrap()).unwrap();
                settle().await;
                if orders_count == 3 {
                    advance(21).await;
                }
                let started_before_unknown = chasers.started.borrow().len();
                chasers.uncertain.set(true);
                advance(1).await;
                advance(100).await;
                assert_eq!(chasers.started.borrow().len(), started_before_unknown);
                assert_eq!(
                    *chasers.halted.borrow(),
                    chasers
                        .started
                        .borrow()
                        .iter()
                        .map(|request| request.id.clone())
                        .collect::<Vec<_>>()
                );
                assert!(
                    chasers.removed.borrow().is_empty(),
                    "no automatic cancellation after Unknown"
                );
                assert!(port.commands.borrow().is_empty());
                assert_eq!(port.writes.borrow().len(), started_before_unknown);
                let emits = port.emits.borrow();
                let error = &emits.iter().find(|(op, _)| op == "twap:outcome").unwrap().1["error"];
                assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
                assert_eq!(error["elements"][0]["status"], "unknown");
                assert!(
                    error["elements"].as_array().unwrap()[started_before_unknown..]
                        .iter()
                        .all(|row| row["status"] == "notSent")
                );
                assert!(
                    emits
                        .iter()
                        .any(|(op, event)| op == "telemetry" && event["error"] == *error)
                );
            })
            .await;
    }
}

// The reviewer's known-partial-probe: accepted native prefix plus definite refusal,
// with no aggregate outcome and no Unknown elements.
fn known_partial_refusal() -> VenueError {
    use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
    let mut journal = WritePlan::new(vec![
        Element {
            index: 0,
            client_order_id: None,
        },
        Element {
            index: 1,
            client_order_id: None,
        },
    ]);
    journal
        .record(
            0,
            ElementResult::Placed {
                order_ids: vec!["native-accepted".to_owned()],
            },
        )
        .unwrap();
    journal
        .record(
            1,
            ElementResult::NotSent {
                error: Some(Box::new(
                    VenueError::new("insufficient margin")
                        .outcome(safe_cex_engine::error::Outcome::Rejected),
                )),
            },
        )
        .unwrap();
    journal.attach(
        VenueError::new("last split lot refused").partial_ids(&["native-accepted".to_owned()]),
    )
}

#[tokio::test(start_paused = true)]
async fn known_partial_probe_continues_then_genuine_unknown_halts() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, _, factory) = fixture("binance");
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({"size": 4, "ordersCount": 4})))
                        .unwrap(),
                )
                .unwrap();
            settle().await;
            let partial = known_partial_refusal();
            assert_eq!(
                safe_cex_algo_twap::factory::placement_outcome(Err(partial.clone())),
                Err(partial.clone()),
                "known-partial-probe must retain native facts and message"
            );
            *port.next_failure.borrow_mut() = Some(partial);
            advance(16).await;
            assert_eq!(placed_amounts(&port), [1.0, 1.0]);
            assert_eq!(factory.active_twaps("a").len(), 1);
            assert!(
                !port
                    .emits
                    .borrow()
                    .iter()
                    .any(|(op, _)| op == "twap:outcome" || op == "telemetry")
            );
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 2);
            let mut uncertain = known_partial_refusal();
            uncertain.elements.as_mut().unwrap().0[1].result =
                safe_cex_engine::write_plan::ElementResult::Unknown {
                    error: Box::new(VenueError::new("timeout")),
                };
            *port.next_failure.borrow_mut() = Some(uncertain);
            advance(16).await;
            advance(100).await;
            assert_eq!(
                placed_amounts(&port),
                [1.0, 1.0, 1.0],
                "continue after known partial; halt only after genuine Unknown"
            );
            assert!(factory.active_twaps("a").is_empty());
            let emits = port.emits.borrow();
            let error = &emits.iter().find(|(op, _)| op == "twap:outcome").unwrap().1["error"];
            assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
            assert_eq!(
                error["partial"]["orderIds"],
                json!(["order-1", "native-accepted"])
            );
            let refused = &error["elements"][1]["error"];
            assert_eq!(refused["message"], "last split lot refused");
            assert_eq!(refused["elements"][0]["status"], "placed");
            assert_eq!(
                refused["elements"][0]["orderIds"],
                json!(["native-accepted"])
            );
            assert_eq!(refused["elements"][1]["error"]["outcome"], "rejected");
            assert_eq!(error["elements"][2]["status"], "unknown");
            assert_eq!(error["elements"][3]["status"], "notSent");
            assert!(
                emits
                    .iter()
                    .any(|(op, event)| op == "telemetry" && event["error"] == *error)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn known_partial_probe_remainder_continues_then_genuine_unknown_halts() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:181-194,255-280.
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, _) = fixture("binance");
            let chasers = Rc::new(PolicyChasers {
                writes: port.writes.clone(),
                ..Default::default()
            });
            let factory = TwapFactory::new("a", cx, chasers.clone());
            *port.next_failure.borrow_mut() = Some(known_partial_refusal());
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({
                        "size": 3, "ordersCount": 3, "useChaserOrders": true,
                        "timeoutPct": 50, "timeoutAction": "market", "failAction": "market"
                    })))
                    .unwrap(),
                )
                .unwrap();
            settle().await;
            advance(11).await;
            advance(1).await;
            advance(9).await;
            assert_eq!(
                chasers.started.borrow().len(),
                2,
                "known partial remainder must keep next chase scheduled"
            );
            assert!(chasers.halted.borrow().is_empty());
            assert!(
                !port
                    .emits
                    .borrow()
                    .iter()
                    .any(|(op, _)| op == "twap:outcome" || op == "telemetry")
            );
            *port.next_failure.borrow_mut() =
                Some(VenueError::new("timeout").outcome(safe_cex_engine::error::Outcome::Unknown));
            advance(11).await;
            advance(1).await;
            advance(100).await;
            assert_eq!(chasers.started.borrow().len(), 2);
            assert_eq!(placed_amounts(&port), [1.0, 1.0]);
            let emits = port.emits.borrow();
            let error = &emits.iter().find(|(op, _)| op == "twap:outcome").unwrap().1["error"];
            assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
            assert_eq!(error["partial"]["orderIds"], json!(["native-accepted"]));
            assert_eq!(
                error["elements"][1]["error"]["message"],
                "last split lot refused"
            );
            assert_eq!(
                error["elements"][1]["error"]["elements"][0]["status"],
                "placed"
            );
            assert_eq!(error["elements"][3]["status"], "unknown");
            assert!(
                error["elements"].as_array().unwrap()[4..]
                    .iter()
                    .all(|row| row["status"] == "notSent")
            );
            assert!(
                emits
                    .iter()
                    .any(|(op, event)| op == "telemetry" && event["error"] == *error)
            );
        })
        .await;
}
