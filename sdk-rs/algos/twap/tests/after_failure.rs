//! After-failure parity at the real factory/AccountPort boundary.
//! The runtime adapter: ops in, engine commands and wire replies out.
#![cfg(not(target_arch = "wasm32"))]

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algo_twap::ChaserProgress;
use safe_cex_algo_twap::ChaserRequest;
use safe_cex_algo_twap::factory::{TwapChasers, TwapFactory};
use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[derive(Default)]
struct Port {
    store: RefCell<AlgoStore>,
    ids: Cell<CommandId>,
    commands: RefCell<Vec<Command>>,
    emits: RefCell<Vec<(String, Value)>>,
    fail: Cell<bool>,
    fail_outcome: Cell<Option<safe_cex_engine::error::Outcome>>,
    answer_ms: Cell<u64>,
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
        self.commands.borrow_mut().push(command);
        let fail = self.fail.get();
        let outcome = self.fail_outcome.get();
        let answer_ms = self.answer_ms.get();
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(answer_ms)).await;
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

struct Clock {
    start: tokio::time::Instant,
}

impl safe_cex_algos::executor::Executor for Clock {
    fn spawn(
        &self,
        task: safe_cex_algos::executor::LocalFuture<()>,
    ) -> safe_cex_algos::executor::TaskHandle {
        safe_cex_algos::executor::TaskHandle::new(tokio::task::spawn_local(task).abort_handle())
    }
    fn sleep(&self, ms: f64) -> safe_cex_algos::executor::LocalFuture<()> {
        Box::pin(tokio::time::sleep(Duration::from_secs_f64(
            ms.max(0.0) / 1000.0,
        )))
    }
    fn monotonic_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
    fn now_ms(&self) -> f64 {
        1_700_000_000_000.0 + self.start.elapsed().as_secs_f64() * 1000.0
    }
}

struct ClockGuard(Option<Rc<dyn safe_cex_algos::executor::Executor>>);

impl ClockGuard {
    fn new() -> Self {
        Self(safe_cex_algos::executor::set_current(Some(Rc::new(
            Clock {
                start: tokio::time::Instant::now(),
            },
        ))))
    }
}

impl Drop for ClockGuard {
    fn drop(&mut self) {
        safe_cex_algos::executor::set_current(self.0.take());
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
    let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
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
            "id": "restored", "options": options,
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
        assert_eq!(orders[0].reduce_only, Some(true));
        if let Some(id) = orders[0].client_order_id.as_deref() {
            assert!(ids.insert(id));
        } else {
            assert_eq!(venue, "bingx", "retain each venue's caller-id support");
        }
    }
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
    LocalSet::new()
        .run_until(check_continuation(
            "binance",
            Some("paused"),
            safe_cex_engine::error::Outcome::NotSent,
        ))
        .await;
}

#[tokio::test(start_paused = true)]
async fn unknown_runtime_failure_continues_without_resending_that_child() {
    LocalSet::new()
        .run_until(check_continuation(
            "binance",
            None,
            safe_cex_engine::error::Outcome::Unknown,
        ))
        .await;
}

#[derive(Default)]
struct PolicyChasers {
    started: RefCell<Vec<ChaserRequest>>,
    removed: RefCell<Vec<String>>,
    refuse_first: Cell<bool>,
}

impl TwapChasers for PolicyChasers {
    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
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
            ended: self.removed.borrow().iter().any(|removed| removed == id),
            ..Default::default()
        })
    }
    fn remove(&self, _cx: &AlgoCx, id: &str) {
        self.removed.borrow_mut().push(id.to_owned());
    }
}

#[tokio::test(start_paused = true)]
async fn chaser_handoff_failure_keeps_later_slices_scheduled() {
    LocalSet::new()
        .run_until(async {
            let _clock = ClockGuard::new();
            let (port, cx, _) = fixture("binance");
            let chasers = Rc::new(PolicyChasers::default());
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
            assert!(port.commands.borrow().is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn failed_compensating_market_remainder_keeps_next_chaser_scheduled() {
    for outcome in [
        safe_cex_engine::error::Outcome::Rejected,
        safe_cex_engine::error::Outcome::Unknown,
    ] {
        LocalSet::new()
            .run_until(async {
                let _clock = ClockGuard::new();
                let (port, cx, _) = fixture("binance");
                port.fail.set(true);
                port.fail_outcome.set(Some(outcome));
                let chasers = Rc::new(PolicyChasers::default());
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
