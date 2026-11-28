// Rule 5: pin the calmer lifecycle messages; owner, no-cancel and no-replay assertions stay intact.
//! The runtime adapter: ops in, engine commands and wire replies out.
#![cfg(not(target_arch = "wasm32"))]

mod support;
use support::ENGINE_UNKNOWN_TEXT;

use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algo_twap::ChaserProgress;
use safe_cex_algo_twap::ChaserRequest;
use safe_cex_algo_twap::factory::{TwapChasers, TwapFactory};
use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_engine::engine::{Command, CommandId};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
use serde_json::{Value, json};
use tokio::sync::{mpsc, oneshot};
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
    held_answer: RefCell<Option<oneshot::Receiver<()>>>,
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
        let held_answer = self.held_answer.borrow_mut().take();
        Box::pin(async move {
            if let Some(answer) = held_answer {
                answer.await.unwrap();
            }
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
    progress: Cell<Option<ChaserProgress>>,
    removed: RefCell<Vec<String>>,
}

impl TwapChasers for Chasers {
    fn halt(&self, _cx: &AlgoCx, _id: &str) {}
    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
        self.started.borrow_mut().push(request.clone());
        Ok(())
    }
    fn progress(&self, _chaser_id: &str) -> Option<ChaserProgress> {
        self.progress.get()
    }
    fn remove(&self, _cx: &AlgoCx, chaser_id: &str) {
        self.removed.borrow_mut().push(chaser_id.to_owned());
    }
}

struct Owner {
    port: Rc<Port>,
    cx: AlgoCx,
    factory: TwapFactory,
}

impl Owner {
    fn new(chasers: Rc<Chasers>) -> Self {
        let port = Rc::new(Port::default());
        let cx = AlgoCx::new(port.clone(), Scope::new());
        // Different logins can reuse the same account name.
        let factory = TwapFactory::new("a", cx.clone(), chasers);
        Self { port, cx, factory }
    }

    fn start(&self, extra: Value) {
        self.factory
            .handle("twap:start", &twap_options(extra), &self.cx)
            .unwrap();
    }

    fn hold_next_answer(&self) -> oneshot::Sender<()> {
        let (answer, pending) = oneshot::channel();
        *self.port.held_answer.borrow_mut() = Some(pending);
        answer
    }
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

#[tokio::test(start_paused = true)]
async fn amend_repartitions_only_unplaced_twap_quantity() {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({"ordersCount":4})), &cx)
                .unwrap();
            tokio::time::sleep(Duration::from_millis(1)).await;
            settle().await;
            let id = factory.active_twaps("a")[0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert_eq!(placed_amounts(&port), vec![0.25]);
            factory
                .handle(
                    "twap:amend",
                    &json!({"id":id,"update":{"intervalMs":1000,"remainingSlices":2}}).to_string(),
                    &cx,
                )
                .unwrap();
            assert_eq!(
                placed_amounts(&port),
                vec![0.25],
                "amend has no immediate venue effect"
            );
            tokio::time::sleep(Duration::from_millis(999)).await;
            assert_eq!(placed_amounts(&port), vec![0.25]);
            tokio::time::sleep(Duration::from_millis(3)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.25, 0.375]);
            tokio::time::sleep(Duration::from_millis(1002)).await;
            settle().await;
            tokio::time::sleep(Duration::from_millis(1002)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.25, 0.375, 0.375]);
            assert!(factory.active_twaps("a").is_empty());
            let error = factory
                .handle(
                    "twap:amend",
                    &json!({"id":id,"update":{"intervalMs":1000}}).to_string(),
                    &cx,
                )
                .unwrap_err();
            assert!(error.contains("not amendable"), "{error}");
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn amend_refuses_unknown_twap_and_immutable_quantity() {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            for field in ["size", "side", "symbol"] {
                let error = factory
                    .handle(
                        "twap:amend",
                        &json!({"id":"missing","update":{field:2}}).to_string(),
                        &cx,
                    )
                    .unwrap_err();
                assert!(error.contains("unsupported amend field"), "{error}");
            }
            let error = factory
                .handle(
                    "twap:amend",
                    r#"{"id":"missing","update":{"remainingSlices":3}}"#,
                    &cx,
                )
                .unwrap_err();
            assert!(error.contains("not amendable"), "{error}");
            assert!(port.commands.borrow().is_empty());
            cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn amend_during_a_pending_slice_keeps_its_quantity_and_waits_for_its_answer() {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            port.answer_ms.set(5000);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({"ordersCount":4})), &cx)
                .unwrap();
            settle().await;
            let id = factory.active_twaps("a")[0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            let before = placed_amounts(&port);
            assert_eq!(before, vec![0.25]);
            factory
                .handle(
                    "twap:amend",
                    &json!({"id":id,"update":{"intervalMs":1000,"remainingSlices":2}}).to_string(),
                    &cx,
                )
                .unwrap();
            assert_eq!(placed_amounts(&port), before);
            tokio::time::advance(Duration::from_millis(5001)).await;
            settle().await;
            assert_eq!(
                placed_amounts(&port),
                before,
                "pending answer never resets the amended interval to the old 15 seconds"
            );
            tokio::time::advance(Duration::from_millis(1001)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.25, 0.375]);
            factory
                .handle("twap:stop", &json!(id).to_string(), &cx)
                .unwrap();
            assert!(
                factory
                    .handle(
                        "twap:amend",
                        &json!({"id":id,"update":{"intervalMs":500}}).to_string(),
                        &cx
                    )
                    .unwrap_err()
                    .contains("not amendable")
            );
            cx.scope.dispose();
        })
        .await;
}

async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn captured_start_cannot_adopt_the_binding_refreshed_after_its_check() {
    LocalSet::new()
        .run_until(async {
            for (owner_id, generation) in [("A", 7), ("B", 8), ("A", 8), ("A", 9)] {
                let initial = safe_cex_algo_twap::TwapOwner {
                    owner_id: "A".into(),
                    generation: 7,
                };
                let replacement = safe_cex_algo_twap::TwapOwner {
                    owner_id: owner_id.into(),
                    generation,
                };
                let changed = initial != replacement;
                let owner = Rc::new(RefCell::new(initial.clone()));
                let armed = Rc::new(Cell::new(false));
                let port = Rc::new(Port::default());
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory =
                    TwapFactory::with_owner("a", initial, cx.clone(), Rc::new(Chasers::default()))
                        .guard_owner(Rc::new({
                            let owner = owner.clone();
                            let armed = armed.clone();
                            move || {
                                let snapshot = owner.borrow().clone();
                                if armed.replace(false) {
                                    *owner.borrow_mut() = replacement.clone();
                                }
                                snapshot
                            }
                        }));
                let captured = factory.capture_owner();
                armed.set(true);
                let result =
                    captured.start(serde_json::from_str(&twap_options(json!({}))).unwrap());
                settle().await;
                assert_eq!(
                    result.is_err(),
                    changed,
                    "captured start adopted {owner_id}/{generation}"
                );
                assert_eq!(
                    placed_amounts(&port),
                    if changed { vec![] } else { vec![0.5] }
                );
                if changed {
                    assert!(factory.active_twaps("a").is_empty());
                    factory
                        .capture_owner()
                        .start(serde_json::from_str(&twap_options(json!({}))).unwrap())
                        .unwrap();
                    settle().await;
                    assert_eq!(placed_amounts(&port), vec![0.5], "fresh owner control");
                }
                cx.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn typed_start_reports_owner_change_during_first_effects() {
    LocalSet::new()
        .run_until(async {
            for (change, after_dispatch, chaser) in [
                (true, false, false),
                (true, true, false),
                (false, false, false),
                (true, true, true),
                (false, true, true),
            ] {
                let initial = safe_cex_algo_twap::TwapOwner {
                    owner_id: "A".into(),
                    generation: 7,
                };
                let replacement = safe_cex_algo_twap::TwapOwner {
                    owner_id: "B".into(),
                    generation: 8,
                };
                let reads = Rc::new(Cell::new(0));
                let port = Rc::new(Port::default());
                let chasers = Rc::new(Chasers::default());
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory =
                    TwapFactory::with_owner("a", initial.clone(), cx.clone(), chasers.clone())
                        .guard_owner(Rc::new({
                            let reads = reads.clone();
                            let port = port.clone();
                            let chasers = chasers.clone();
                            move || {
                                reads.set(reads.get() + 1);
                                let dispatched = !port.commands.borrow().is_empty()
                                    || !chasers.started.borrow().is_empty();
                                if change
                                    && if after_dispatch {
                                        dispatched
                                    } else {
                                        reads.get() >= 4
                                    }
                                {
                                    replacement.clone()
                                } else {
                                    initial.clone()
                                }
                            }
                        }));
                let result = factory.capture_owner().start(
                    serde_json::from_str(&twap_options(json!({"useChaserOrders":chaser}))).unwrap(),
                );
                if change {
                    let message =
                        result.expect_err("discarded work must never report Starting TWAP");
                    assert!(
                        message.message.contains(if after_dispatch {
                            "The account connection changed while the TWAP was starting. Check your open orders before restarting it."
                        } else {
                            "The TWAP couldn’t start because the account connection changed"
                        }),
                        "{message:?}"
                    );
                    assert_eq!(
                        message.outcome,
                        Some(if after_dispatch {
                            safe_cex_engine::error::Outcome::Unknown
                        } else {
                            safe_cex_engine::error::Outcome::NotSent
                        })
                    );
                    assert!(factory.active_twaps("a").is_empty());
                } else {
                    assert_eq!(factory.active_twaps("a")[0]["id"], result.unwrap());
                }
                assert_eq!(
                    placed_amounts(&port),
                    if chaser || (change && !after_dispatch) {
                        vec![]
                    } else {
                        vec![0.5]
                    }
                );
                assert_eq!(chasers.started.borrow().len(), usize::from(chaser));
                cx.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn binding_changes_during_slice_deferral_drop_the_retired_schedule() {
    LocalSet::new()
        .run_until(async {
            for settle_before_rebind in [false, true] {
                for (owner_id, generation, expected) in [("A", 7, 2), ("B", 7, 1), ("A", 8, 1)] {
                    let owner = Rc::new(RefCell::new(safe_cex_algo_twap::TwapOwner {
                        owner_id: "A".into(),
                        generation: 7,
                    }));
                    let port = Rc::new(Port::default());
                    // Exercise a deferred completion and, separately, a scheduled next slice.
                    port.answer_ms.set(100);
                    let cx = AlgoCx::new(port.clone(), Scope::new());
                    let factory = TwapFactory::with_owner(
                        "a",
                        owner.borrow().clone(),
                        cx.clone(),
                        Rc::new(Chasers::default()),
                    )
                    .guard_owner(Rc::new({
                        let owner = owner.clone();
                        move || owner.borrow().clone()
                    }));
                    factory
                        .handle("twap:start", &twap_options(json!({"duration": 0.5})), &cx)
                        .unwrap();
                    settle().await;
                    assert_eq!(placed_amounts(&port), vec![0.5]);
                    if settle_before_rebind {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        settle().await;
                    }
                    *owner.borrow_mut() = safe_cex_algo_twap::TwapOwner {
                        owner_id: owner_id.into(),
                        generation,
                    };
                    let before = port.emits.borrow().len();
                    if !settle_before_rebind {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        settle().await;
                    }
                    tokio::time::sleep(Duration::from_millis(15_001)).await;
                    settle().await;
                    assert_eq!(placed_amounts(&port), vec![0.5; expected]);
                    if expected == 1 {
                        assert!(factory.active_twaps("a").is_empty());
                        assert!(
                            port.emits.borrow()[before..].iter().all(|(op, data)| {
                                op != "twap:stateUpdate" || data == &json!({})
                            }),
                            "retired completion must not republish the saved row"
                        );
                        assert!(port.emits.borrow().iter().any(|(op, data)| {
                            op == "error"
                                && data == "TWAPs stopped because the account connection changed"
                        }));
                    }
                    cx.scope.dispose();
                }
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn same_name_different_owner_cannot_restore_a_pending_slice() {
    LocalSet::new()
        .run_until(async {
            for tagged in [false, true] {
                let port_a = Rc::new(Port::default());
                let scope_a = Scope::new();
                let cx_a = AlgoCx::new(port_a.clone(), scope_a.clone());
                let factory_a = TwapFactory::with_owner(
                    "a",
                    safe_cex_algo_twap::TwapOwner {
                        owner_id: "A".into(),
                        generation: 7,
                    },
                    cx_a.clone(),
                    Rc::new(Chasers::default()),
                );
                factory_a
                    .handle("twap:start", &twap_options(json!({"duration": 0.5})), &cx_a)
                    .unwrap();
                settle().await;
                assert_eq!(placed_amounts(&port_a), vec![0.5]);
                let mut rows: Vec<Value> = port_a
                    .emits
                    .borrow()
                    .iter()
                    .rev()
                    .find(|(op, data)| {
                        op == "twap:stateUpdate" && !data.as_object().unwrap().is_empty()
                    })
                    .unwrap()
                    .1
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(id, row)| {
                        let mut row = row.clone();
                        row["id"] = json!(id);
                        row
                    })
                    .collect();
                if tagged {
                    rows[0]["ownerId"] = json!("A");
                    rows[0]["ownerGeneration"] = json!(7);
                } else {
                    rows[0].as_object_mut().unwrap().remove("ownerId");
                    rows[0].as_object_mut().unwrap().remove("ownerGeneration");
                }
                scope_a.dispose();
                let port_b = Rc::new(Port::default());
                let cx_b = AlgoCx::new(port_b.clone(), Scope::new());
                let factory_b = TwapFactory::with_owner(
                    "a",
                    safe_cex_algo_twap::TwapOwner {
                        owner_id: "B".into(),
                        generation: 7,
                    },
                    cx_b.clone(),
                    Rc::new(Chasers::default()),
                );
                factory_b
                    .handle("twap:state", &json!(rows).to_string(), &cx_b)
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(15_001)).await;
                settle().await;
                assert!(
                    placed_amounts(&port_b).is_empty(),
                    "tagged={tagged}: stale A slice reached same-name B"
                );
                assert!(
                    factory_b.active_twaps("a").is_empty(),
                    "mismatched rows must be dropped"
                );
                assert_eq!(
                    placed_amounts(&port_a),
                    vec![0.5],
                    "disposed A must stay stopped"
                );
                cx_b.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn restore_checks_generation_and_requires_a_trusted_current_owner() {
    LocalSet::new()
        .run_until(async {
            for generation in [Some(7), Some(8), None] {
                let source = Rc::new(Port::default());
                let source_scope = Scope::new();
                let source_cx = AlgoCx::new(source.clone(), source_scope.clone());
                let a = TwapFactory::with_owner(
                    "a",
                    safe_cex_algo_twap::TwapOwner {
                        owner_id: "A".into(),
                        generation: 7,
                    },
                    source_cx.clone(),
                    Rc::new(Chasers::default()),
                );
                a.handle(
                    "twap:start",
                    &twap_options(
                        json!({"duration": 0.5, "ownerId": "forged", "ownerGeneration": 999}),
                    ),
                    &source_cx,
                )
                .unwrap();
                settle().await;
                assert_eq!(placed_amounts(&source), vec![0.5]);
                let rows: Vec<Value> = source
                    .emits
                    .borrow()
                    .iter()
                    .rev()
                    .find(|(op, data)| {
                        op == "twap:stateUpdate" && !data.as_object().unwrap().is_empty()
                    })
                    .unwrap()
                    .1
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(id, row)| {
                        assert_eq!(row["ownerId"], "A");
                        assert_eq!(row["ownerGeneration"], 7);
                        let mut row = row.clone();
                        row["id"] = json!(id);
                        row
                    })
                    .collect();
                source_scope.dispose();
                let target = Rc::new(Port::default());
                let target_cx = AlgoCx::new(target.clone(), Scope::new());
                let b = match generation {
                    Some(generation) => TwapFactory::with_owner(
                        "a",
                        safe_cex_algo_twap::TwapOwner {
                            owner_id: "A".into(),
                            generation,
                        },
                        target_cx.clone(),
                        Rc::new(Chasers::default()),
                    ),
                    None => TwapFactory::new("a", target_cx.clone(), Rc::new(Chasers::default())),
                };
                b.handle("twap:state", &json!(rows).to_string(), &target_cx)
                    .unwrap();
                assert_eq!(
                    b.active_twaps("a").len(),
                    usize::from(generation == Some(7))
                );
                tokio::time::sleep(Duration::from_millis(15_001)).await;
                settle().await;
                assert_eq!(
                    placed_amounts(&target),
                    if generation == Some(7) {
                        vec![0.5]
                    } else {
                        vec![]
                    }
                );
                assert_eq!(placed_amounts(&source), vec![0.5]);
                target_cx.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn runs_slices_on_the_interval_and_reports_state() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.5]);
            assert_eq!(factory.active_twaps("a").len(), 1);

            // One minute over two slices: the next one is 30 s out.
            tokio::time::sleep(Duration::from_millis(29_000)).await;
            assert_eq!(placed_amounts(&port), vec![0.5]);
            tokio::time::sleep(Duration::from_millis(2_000)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.5, 0.5]);
            assert!(factory.active_twaps("a").is_empty());

            let emits = port.emits.borrow();
            let (op, last) = emits.last().unwrap();
            assert_eq!((op.as_str(), last), ("twap:stateUpdate", &json!({})));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_slice_timer_drops_the_old_schedule() {
    LocalSet::new()
        .run_until(async {
            let old = Owner::new(Rc::new(Chasers::default()));
            old.start(json!({}));
            settle().await;
            assert_eq!(placed_amounts(&old.port), [0.5]);
            assert_eq!(old.factory.active_twaps("a")[0]["ordersExecuted"], 1);

            tokio::time::advance(Duration::from_secs(10)).await;
            settle().await;
            let current = Owner::new(Rc::new(Chasers::default()));
            old.cx.scope.dispose();
            current.start(json!({ "size": 4, "duration": 2 }));
            settle().await;
            let old_state = old.factory.active_twaps("a");
            let old_emits = old.port.emits.borrow().clone();
            let current_emits = current.port.emits.borrow().clone();
            assert_eq!(placed_amounts(&current.port), [2.0]);

            tokio::time::advance(Duration::from_secs(21)).await;
            settle().await;
            assert_eq!(placed_amounts(&old.port), [0.5]);
            assert_eq!(old.factory.active_twaps("a"), old_state);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(placed_amounts(&current.port), [2.0]);
            assert_eq!(*current.port.emits.borrow(), current_emits);

            tokio::time::advance(Duration::from_secs(40)).await;
            settle().await;
            assert_eq!(placed_amounts(&old.port), [0.5]);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(placed_amounts(&current.port), [2.0, 2.0]);
            assert!(current.factory.active_twaps("a").is_empty());
            current.cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_placement_drops_the_old_completion() {
    LocalSet::new()
        .run_until(async {
            let old = Owner::new(Rc::new(Chasers::default()));
            let old_answer = old.hold_next_answer();
            old.start(json!({}));
            settle().await;
            assert_eq!(placed_amounts(&old.port), [0.5]);
            assert_eq!(old.factory.pending_placements(), 1);
            assert_eq!(old.factory.active_twaps("a")[0]["ordersExecuted"], 0);

            let current = Owner::new(Rc::new(Chasers::default()));
            let current_answer = current.hold_next_answer();
            current.start(json!({ "size": 4 }));
            settle().await;
            assert_eq!(current.factory.pending_placements(), 1);
            let old_state = old.factory.active_twaps("a");
            let old_emits = old.port.emits.borrow().clone();
            let current_state = current.factory.active_twaps("a");
            let current_emits = current.port.emits.borrow().clone();

            // Queue a late reply before the executor polls the aborted task.
            old.cx.scope.dispose();
            old_answer.send(()).unwrap();
            settle().await;
            assert_eq!(old.factory.active_twaps("a"), old_state);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(current.factory.active_twaps("a"), current_state);
            assert_eq!(*current.port.emits.borrow(), current_emits);
            assert_eq!(current.factory.pending_placements(), 1);
            assert_eq!(placed_amounts(&current.port), [2.0]);

            current_answer.send(()).unwrap();
            settle().await;
            assert_eq!(current.factory.pending_placements(), 0);
            assert_eq!(current.factory.active_twaps("a")[0]["ordersExecuted"], 1);
            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert_eq!(placed_amounts(&old.port), [0.5]);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(placed_amounts(&current.port), [2.0, 2.0]);
            assert!(current.factory.active_twaps("a").is_empty());
            current.cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn owner_switch_during_timeout_cancel_drops_the_old_market_remainder() {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let chasers = Rc::new(Chasers::default());
            chasers.progress.set(Some(ChaserProgress {
                remaining_size: 0.5,
                ..ChaserProgress::default()
            }));
            let old = Owner::new(chasers.clone());
            old.start(json!({
                "useChaserOrders": true, "timeoutPct": 50, "timeoutAction": "market"
            }));
            settle().await;
            let chaser_id = chasers.started.borrow()[0].id.clone();
            tokio::time::advance(Duration::from_millis(15_001)).await;
            settle().await;
            assert_eq!(*chasers.removed.borrow(), [chaser_id]);
            assert!(placed_amounts(&old.port).is_empty());

            let current_chasers = Rc::new(Chasers::default());
            let current = Owner::new(current_chasers.clone());
            current.start(json!({ "size": 4 }));
            settle().await;
            old.cx.scope.dispose();
            let old_state = old.factory.active_twaps("a");
            let old_emits = old.port.emits.borrow().clone();
            let current_state = current.factory.active_twaps("a");
            let current_emits = current.port.emits.borrow().clone();
            chasers.progress.set(Some(ChaserProgress {
                remaining_size: 0.25,
                stopped: true,
                ended: true,
                order_may_be_open: false,
            }));
            tokio::time::advance(Duration::from_millis(251)).await;
            settle().await;
            assert!(placed_amounts(&old.port).is_empty());
            assert_eq!(old.factory.active_twaps("a"), old_state);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(chasers.started.borrow().len(), 1);
            assert_eq!(chasers.removed.borrow().len(), 1);
            assert_eq!(placed_amounts(&current.port), [2.0]);
            assert_eq!(current.factory.active_twaps("a"), current_state);
            assert_eq!(*current.port.emits.borrow(), current_emits);
            assert!(current_chasers.removed.borrow().is_empty());

            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert!(placed_amounts(&old.port).is_empty());
            assert_eq!(chasers.started.borrow().len(), 1);
            assert_eq!(*old.port.emits.borrow(), old_emits);
            assert_eq!(placed_amounts(&current.port), [2.0, 2.0]);
            assert!(current.factory.active_twaps("a").is_empty());
            current.cx.scope.dispose();
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_failed_slice_logs_an_error_and_continues() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Rejected));
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            settle().await;
            assert!(port.emits.borrow().contains(&(
                "error".to_owned(),
                json!("TWAP BTCUSDT slice failed: rejected")
            )));
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_venue_refusal_is_not_counted_and_the_end_says_so() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Rejected));
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            settle().await;
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 0);
            tokio::time::advance(std::time::Duration::from_secs(31)).await;
            settle().await;
            assert!(factory.active_twaps("a").is_empty());
            let errors: Vec<Value> = port
                .emits
                .borrow()
                .iter()
                .filter(|(op, _)| op == "error")
                .map(|(_, data)| data.clone())
                .collect();
            assert_eq!(
                errors,
                [
                    json!("TWAP BTCUSDT slice failed: rejected"),
                    json!("TWAP BTCUSDT slice failed: rejected"),
                    json!("TWAP BTCUSDT ended with 0 of 2 slices placed"),
                ]
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn chaser_slices_start_through_the_chaser_runtime() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let chasers = Rc::new(Chasers::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), chasers.clone());
            factory
                .handle(
                    "twap:start",
                    &twap_options(json!({ "useChaserOrders": true })),
                    &cx,
                )
                .unwrap();
            settle().await;
            assert!(port.commands.borrow().is_empty());
            let started = chasers.started.borrow();
            assert_eq!(
                (started.len(), started[0].size, started[0].id.len()),
                (1, 0.5, 21)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn closing_the_sending_socket_leaves_the_schedule_running() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let account = AlgoCx::new(port.clone(), Scope::new());
            let socket = account.child();
            let factory = TwapFactory::new("a", account.clone(), Rc::new(Chasers::default()));
            let options = twap_options(json!({ "ordersCount": 4 }));
            factory.handle("twap:start", &options, &socket).unwrap();
            settle().await;
            socket.scope.dispose();
            tokio::time::sleep(Duration::from_millis(15_001)).await;
            settle().await;
            assert_eq!(placed_amounts(&port).len(), 2);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_nuke_waits_for_a_market_slice_on_the_wire() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.answer_ms.set(3_000);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.5]);
            let started = tokio::time::Instant::now();
            assert_eq!(factory.stop_for_nuke().len(), 1);
            factory.placements_settled().await;
            assert!(started.elapsed() >= Duration::from_millis(3_000));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn terminate_all_stops_every_twap_and_reports_it() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            for _ in 0..2 {
                factory
                    .handle(
                        "twap:start",
                        &twap_options(json!({ "ordersCount": 4 })),
                        &cx,
                    )
                    .unwrap();
            }
            settle().await;
            factory.terminate_all();
            assert!(factory.active_twaps("a").is_empty());
            assert_eq!(
                port.emits.borrow().last().unwrap(),
                &("twap:stateUpdate".to_owned(), json!({}))
            );
            tokio::time::sleep(Duration::from_millis(60_000)).await;
            settle().await;
            assert_eq!(placed_amounts(&port).len(), 2);
        })
        .await;
}

#[test]
fn rejects_malformed_payloads() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    LocalSet::new().block_on(&rt, async {
        let cx = AlgoCx::new(Rc::new(Port::default()), Scope::new());
        let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
        assert!(factory.handle("twap:pause", "{", &cx).is_err());
    });
}

#[test]
fn placement_outcomes_read_ids_strictly() {
    use safe_cex_algo_twap::factory::placement_outcome;
    use safe_cex_engine::error::Outcome;
    assert_eq!(
        placement_outcome(Ok(r#"["a","b"]"#.into())),
        Ok(vec!["a".to_owned(), "b".to_owned()])
    );
    assert_eq!(placement_outcome(Ok("[]".into())), Ok(vec![]));
    for unreadable in [r#"{"orderIds":["a"]}"#, "null", ""] {
        let error = placement_outcome(Ok(unreadable.into())).unwrap_err();
        assert_eq!(error.message, ENGINE_UNKNOWN_TEXT);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
    let failed = |message: &str, outcome| {
        placement_outcome(Err(VenueError {
            message: message.into(),
            outcome,
            ..Default::default()
        }))
    };
    for outcome in [Some(Outcome::Unknown), None] {
        let error = failed("timeout", outcome).unwrap_err();
        assert_eq!(error.message, ENGINE_UNKNOWN_TEXT);
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
    for outcome in [Outcome::Rejected, Outcome::NotSent] {
        let error = failed("insufficient margin", Some(outcome)).unwrap_err();
        assert_eq!(error.message, "insufficient margin");
        assert_eq!(error.outcome, Some(outcome));
    }
    let partial = VenueError::new("partly refused")
        .outcome(Outcome::Rejected)
        .partial_ids(&["a".to_owned()]);
    assert_eq!(placement_outcome(Err(partial.clone())), Err(partial));
}

#[derive(Default)]
struct RefusingChasers;

impl TwapChasers for RefusingChasers {
    fn halt(&self, _cx: &AlgoCx, _id: &str) {}
    fn start_with_id(&self, _cx: &AlgoCx, _request: &ChaserRequest) -> Result<(), String> {
        Err("chaser refused".to_owned())
    }
    fn progress(&self, _chaser_id: &str) -> Option<ChaserProgress> {
        None
    }
    fn remove(&self, _cx: &AlgoCx, _chaser_id: &str) {}
}

#[tokio::test(start_paused = true)]
async fn typed_start_counts_only_accepted_chaser_admission_as_dispatch() {
    LocalSet::new()
        .run_until(async {
            for (refused, change) in [(false, false), (true, false), (false, true), (true, true)] {
                let initial = safe_cex_algo_twap::TwapOwner {
                    owner_id: "A".into(),
                    generation: 7,
                };
                let reads = Rc::new(Cell::new(0));
                let port = Rc::new(Port::default());
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let accepted = Rc::new(Chasers::default());
                let chasers: Rc<dyn TwapChasers> = if refused {
                    Rc::new(RefusingChasers)
                } else {
                    accepted.clone()
                };
                let factory = TwapFactory::with_owner("a", initial.clone(), cx.clone(), chasers)
                    .guard_owner(Rc::new({
                        let reads = reads.clone();
                        move || {
                            reads.set(reads.get() + 1);
                            // capture + initial check/refresh precede child admission.
                            if change && reads.get() >= 4 {
                                safe_cex_algo_twap::TwapOwner {
                                    owner_id: "B".into(),
                                    generation: 8,
                                }
                            } else {
                                initial.clone()
                            }
                        }
                    }));
                let result = factory.capture_owner().start(
                    serde_json::from_str(&twap_options(json!({"useChaserOrders":true}))).unwrap(),
                );
                assert!(placed_amounts(&port).is_empty());
                assert_eq!(accepted.started.borrow().len(), usize::from(!refused));
                if !change {
                    assert_eq!(
                        port.emits
                            .borrow()
                            .iter()
                            .filter(|(op, data)| op == "error" && data == &json!("chaser refused"))
                            .count(),
                        usize::from(refused)
                    );
                }
                if change {
                    let error = result.expect_err("retired starts must refuse");
                    assert_eq!(
                        error.outcome,
                        Some(if refused {
                            safe_cex_engine::error::Outcome::NotSent
                        } else {
                            safe_cex_engine::error::Outcome::Unknown
                        })
                    );
                    assert!(factory.active_twaps("a").is_empty());
                } else {
                    assert_eq!(factory.active_twaps("a")[0]["id"], result.unwrap());
                }
                cx.scope.dispose();
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_chase_slice_is_not_counted() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(RefusingChasers));
            factory
                .handle(
                    "twap:start",
                    &twap_options(json!({ "useChaserOrders": true })),
                    &cx,
                )
                .unwrap();
            settle().await;
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 0);
            let emits = port.emits.borrow();
            assert!(emits.contains(&("error".to_owned(), json!("chaser refused"))));
            let (_, last_state) = emits
                .iter()
                .rev()
                .find(|(op, _)| op == "twap:stateUpdate")
                .unwrap();
            let row = last_state.as_object().unwrap().values().next().unwrap();
            assert_eq!(row["state"]["ordersExecuted"], 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn chase_slices_run_on_the_account_chaser_book() {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            {
                let mut store = port.store.borrow_mut();
                store.markets.insert("BTCUSDT".into(), market());
                store.tickers.insert(
                    "BTCUSDT".into(),
                    Ticker {
                        id: "BTCUSDT".into(),
                        symbol: "BTCUSDT".into(),
                        bid: 100.0,
                        ask: 100.5,
                        last: 100.25,
                        ..Ticker::default()
                    },
                );
            }
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let book = safe_cex_chaser::ChaserFactory::new(cx.clone()).book();
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(book.clone()));
            let options = twap_options(
                json!({ "useChaserOrders": true, "timeoutPct": 50, "timeoutAction": "next" }),
            );
            factory.handle("twap:start", &options, &cx).unwrap();
            settle().await;
            let active = book.active_chasers();
            assert_eq!(active.len(), 1);
            let chaser_id = active[0].id.clone();
            assert_eq!(
                (
                    active[0].size,
                    active[0].max_attempts,
                    active[0].reduce_only
                ),
                (0.5, 1000.0, false)
            );
            assert_eq!(
                book.progress(&chaser_id).map(|p| p.remaining_size),
                Some(0.5)
            );

            // At 50% of the 30 s slice the unfilled chase is removed (`next`).
            tokio::time::sleep(Duration::from_millis(15_001)).await;
            settle().await;
            assert!(book.progress(&chaser_id).is_none_or(|p| p.stopped));
        })
        .await;
}

async fn halt_real_chaser_keeps_order(answer_ms: u64) {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            {
                let mut store = port.store.borrow_mut();
                store.venue = "binance".into();
                store.markets.insert("BTCUSDT".into(), market());
                store.tickers.insert(
                    "BTCUSDT".into(),
                    Ticker {
                        symbol: "BTCUSDT".into(),
                        bid: 100.0,
                        ask: 101.0,
                        last: 100.0,
                        ..Ticker::default()
                    },
                );
            }
            port.answer_ms.set(answer_ms);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let chasers = safe_cex_chaser::ChaserBook::new(cx.clone());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(chasers.clone()));
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({
                        "size": 1, "ordersCount": 1, "useChaserOrders": true
                    })))
                    .unwrap(),
                )
                .unwrap();
            settle().await;
            assert_eq!(placed_amounts(&port), [1.0]);
            let id = chasers.active_chasers()[0].id.clone();
            TwapChasers::halt(&chasers, &cx, &id);
            for _ in 0..8 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                settle().await;
            }
            assert_eq!(placed_amounts(&port), [1.0]);
            assert!(
                !port.commands.borrow().iter().any(|command| matches!(
                    command,
                    Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. }
                )),
                "Unknown halt must retain even a late accepted order: {:?}",
                port.commands.borrow()
            );
            assert!(chasers.active_chasers().is_empty());
            assert_eq!(
                port.emits
                    .borrow()
                    .iter()
                    .filter(|(op, _)| op == "chaser:complete")
                    .count(),
                1
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn unknown_halt_keeps_late_accepted_chaser_order() {
    // TS c78bed93d TwapInstance.ts:213-220 cancels on timeout; Unknown uses the exception.
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at TwapInstance.ts:255-280.
    halt_real_chaser_keeps_order(5000).await;
}

#[tokio::test(start_paused = true)]
async fn unknown_halt_keeps_already_accepted_chaser_order() {
    halt_real_chaser_keeps_order(0).await;
}

async fn real_chaser_placement_outcome(duration: f64, unknown: bool) {
    LocalSet::new()
        .run_until(async {
            let _clock = support::ClockGuard::new();
            let port = Rc::new(Port::default());
            {
                let mut store = port.store.borrow_mut();
                store.venue = "binance".into();
                store.markets.insert("BTCUSDT".into(), market());
                store.tickers.insert(
                    "BTCUSDT".into(),
                    Ticker {
                        symbol: "BTCUSDT".into(),
                        bid: 100.0,
                        ask: 101.0,
                        last: 100.0,
                        ..Ticker::default()
                    },
                );
            }
            port.fail.set(unknown);
            port.fail_outcome
                .set(Some(safe_cex_engine::Outcome::Unknown));
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let chasers = safe_cex_chaser::ChaserBook::new(cx.clone());
            let factory = TwapFactory::new("a", cx, Rc::new(chasers));
            factory
                .start(
                    serde_json::from_str(&twap_options(json!({
                        "size": 3, "ordersCount": 3, "duration": duration,
                        "useChaserOrders": true
                    })))
                    .unwrap(),
                )
                .unwrap();
            settle().await;
            for _ in 0..80 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                settle().await;
            }
            assert_eq!(
                placed_amounts(&port),
                if unknown { vec![1.0] } else { vec![1.0; 3] },
                "real chaser placement outcome must control later TWAP dispatch: {:?}",
                port.commands.borrow()
            );
            if unknown {
                assert!(!port.commands.borrow().iter().any(|command| matches!(
                    command,
                    Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. }
                )));
                let emits = port.emits.borrow();
                let outcomes: Vec<_> = emits
                    .iter()
                    .filter(|(op, _)| op == "twap:outcome")
                    .collect();
                assert_eq!(outcomes.len(), 1);
                let error = &outcomes[0].1["error"];
                assert_eq!(error["message"], ENGINE_UNKNOWN_TEXT);
                assert_eq!(error["outcome"], "unknown");
                assert_eq!(error["elements"].as_array().unwrap().len(), 3);
                assert_eq!(error["elements"][0]["status"], "unknown");
                assert_eq!(
                    error["elements"][0]["error"]["message"],
                    ENGINE_UNKNOWN_TEXT
                );
                for row in &error["elements"].as_array().unwrap()[1..] {
                    assert_eq!(row["status"], "notSent");
                }
                let telemetry: Vec<_> = emits.iter().filter(|(op, _)| op == "telemetry").collect();
                assert_eq!(telemetry.len(), 1);
                assert_eq!(telemetry[0].1["kind"], "twap.unknown_outcome");
                assert_eq!(telemetry[0].1["error"], *error);
                assert_eq!(telemetry[0].1["twapId"], outcomes[0].1["id"]);
                assert!(
                    emits
                        .iter()
                        .any(|(op, value)| op == "error" && value == ENGINE_UNKNOWN_TEXT)
                );
                assert!(factory.active_twaps("a").is_empty());
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_chaser_unknown_placement_halts_twap() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at TwapInstance.ts:249-280.
    real_chaser_placement_outcome(1.0, true).await;
}

#[tokio::test(start_paused = true)]
async fn real_chaser_unknown_placement_halts_before_lookup_retries_finish() {
    // TS TwapInstance.ts:142-165,249-280 delegates children; the Unknown exception halts new writes.
    real_chaser_placement_outcome(0.1, true).await;
}

#[tokio::test(start_paused = true)]
async fn real_chaser_success_keeps_later_children_scheduled() {
    real_chaser_placement_outcome(1.0, false).await;
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

#[tokio::test(start_paused = true)]
async fn typed_start_and_stop_for_the_exec_twap_command() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            let options: safe_cex_algo_twap::TwapOptions =
                serde_json::from_str(&twap_options(json!({ "origin": "cli" }))).unwrap();

            let id = factory.start(options.clone()).unwrap();
            settle().await;
            assert_eq!(placed_amounts(&port), vec![0.5]);
            assert_eq!(factory.active_twaps("a")[0]["id"], json!(id));
            assert_eq!(factory.active_twaps("a")[0]["origin"], "cli");

            assert!(factory.stop(&id));
            assert!(!factory.stop(&id));
            assert!(factory.active_twaps("a").is_empty());

            let mut unbounded = options;
            unbounded.orders_count = u32::MAX;
            let emitted = port.emits.borrow().len();
            assert_eq!(
                factory.start(unbounded),
                Err(VenueError::new(
                    "TWAP BTCUSDT rejected: ordersCount must be between 1 and 1000"
                )
                .outcome(safe_cex_engine::error::Outcome::NotSent))
            );
            assert_eq!(
                port.emits.borrow().len(),
                emitted,
                "the typed caller reports the refusal"
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn clones_share_one_book() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let registered = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            let exec_handle = registered.clone();
            registered
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            let id = exec_handle.active_twaps("a")[0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(exec_handle.stop(&id));
            assert!(registered.active_twaps("a").is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn refusals_reach_the_user_as_errors() {
    // The web toasts `error`; `log` reaches only its exchange log.
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            factory
                .handle(
                    "twap:start",
                    &twap_options(json!({ "accountName": "b" })),
                    &cx,
                )
                .unwrap();
            settle().await;
            assert_eq!(
                *port.emits.borrow(),
                [(
                    "error".to_owned(),
                    json!(r#"TWAP BTCUSDT rejected: it names account "b", not "a""#)
                )]
            );
            assert!(placed_amounts(&port).is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn legacy_parity_bingx_unbounded_max_does_not_gate_the_schedule() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:117-139,167,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | quantity/min/max/grid/notional/price preflight (including BingX unbounded maximum)
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.store
                .borrow_mut()
                .markets
                .insert("BTCUSDT".to_owned(), market());
            {
                let mut store = port.store.borrow_mut();
                store.venue = "bingx".to_owned();
                store.markets.get_mut("BTCUSDT").unwrap().limits.amount.max = f64::INFINITY;
            }
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            factory
                .start(serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap())
                .unwrap();
            settle().await;
            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), [2.0, 2.0]);
            assert_eq!(port.commands.borrow().len(), 2);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn legacy_parity_restored_invalid_tail_is_left_to_each_native_write() {
    // TS c78bed93d packages/safe-cex/src/webworker/handlers/TwapHandler.ts:60-100; twap/TwapInstance.ts:330-345,255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | restore/resume child preflight
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.store
                .borrow_mut()
                .markets
                .insert("BTCUSDT".to_owned(), market());
            {
                let mut store = port.store.borrow_mut();
                let market = store.markets.get_mut("BTCUSDT").unwrap();
                market.precision.amount = 1.0;
                market.limits.amount.min = 2.0;
                market.limits.amount.max = 10.0;
            }
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::with_owner(
                "a",
                safe_cex_algo_twap::TwapOwner {
                    owner_id: "parity-owner".into(),
                    generation: 7,
                },
                cx.clone(),
                Rc::new(Chasers::default()),
            );
            let options: Value = serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap();
            let restore = json!([{
                "id": "legacy-tail", "ownerId": "parity-owner", "ownerGeneration": 7, "options": options,
                "state": { "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" },
                    "size": 4, "ordersSizes": [3, 1], "side": "buy", "status": "running",
                    "ordersCount": 2, "ordersExecuted": 0, "nextOrderAt": cx.now_ms() }
            }]);
            factory
                .handle("twap:state", &restore.to_string(), &cx)
                .unwrap();
            tokio::time::advance(Duration::from_millis(1)).await;
            settle().await;
            assert_eq!(
                placed_amounts(&port),
                [3.0],
                "legacy starts the valid prefix without a tail gate"
            );
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Rejected));
            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), [3.0, 1.0]);
            assert_eq!(port.commands.borrow().len(), 2);
            assert!(
                port.emits.borrow().iter().any(
                    |(op, data)| op == "error" && data == "TWAP BTCUSDT slice failed: rejected"
                )
            );
            assert!(factory.active_twaps("a").is_empty());
        })
        .await;
}

async fn legacy_continues_after_child_failure(outcome: safe_cex_engine::error::Outcome) {
    let port = Rc::new(Port::default());
    port.store
        .borrow_mut()
        .markets
        .insert("BTCUSDT".to_owned(), market());
    port.store.borrow_mut().venue = "binance".to_owned();
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
    factory
        .start(
            serde_json::from_str(&twap_options(json!({
                "size": 3, "ordersCount": 3, "reduceOnly": true
            })))
            .unwrap(),
        )
        .unwrap();
    settle().await;
    assert_eq!(placed_amounts(&port), [1.0]);
    port.fail.set(true);
    port.fail_outcome.set(Some(outcome));
    tokio::time::advance(Duration::from_secs(21)).await;
    settle().await;
    assert_eq!(placed_amounts(&port), [1.0, 1.0]);
    port.fail.set(false);
    tokio::time::advance(Duration::from_secs(21)).await;
    settle().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    settle().await;
    let commands = port.commands.borrow();
    assert_eq!(
        commands.len(),
        if outcome == safe_cex_engine::error::Outcome::Unknown {
            2
        } else {
            3
        },
        "Unknown halts; definite refusal retains legacy continuation"
    );
    let mut client_ids = std::collections::HashSet::new();
    for command in commands.iter() {
        let Command::PlaceOrders { orders, .. } = command else {
            panic!("no repair/cancel belongs to this schedule")
        };
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].reduce_only, Some(true));
        assert!(client_ids.insert(orders[0].client_order_id.as_ref().unwrap()));
    }
    assert!(factory.active_twaps("a").is_empty());
    let expected = if outcome == safe_cex_engine::error::Outcome::Unknown {
        ENGINE_UNKNOWN_TEXT
    } else {
        "TWAP BTCUSDT slice failed: rejected"
    };
    assert!(
        port.emits
            .borrow()
            .iter()
            .any(|(op, data)| op == "error" && data == expected)
    );
}

#[tokio::test(start_paused = true)]
async fn legacy_parity_rejected_child_does_not_stop_later_children() {
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | runtime rejected/unknown child continuation
    LocalSet::new()
        .run_until(legacy_continues_after_child_failure(
            safe_cex_engine::error::Outcome::Rejected,
        ))
        .await;
}

#[tokio::test(start_paused = true)]
async fn unknown_exception_child_stops_later_children() {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    // TS c78bed93d packages/safe-cex/src/webworker/twap/TwapInstance.ts:255-280.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: shared-twap | runtime rejected/unknown child continuation
    LocalSet::new()
        .run_until(legacy_continues_after_child_failure(
            safe_cex_engine::error::Outcome::Unknown,
        ))
        .await;
}

#[test]
fn native_partial_facts_distinguish_refusal_from_uncertainty() {
    use safe_cex_algo_twap::factory::placement_outcome;
    use safe_cex_engine::error::Outcome;
    use safe_cex_engine::write_plan::{Element, ElementResult, WritePlan};
    let mut journal = WritePlan::new(vec![
        Element {
            index: 0,
            client_order_id: None,
            role: None,
        },
        Element {
            index: 1,
            client_order_id: None,
            role: None,
        },
    ]);
    journal
        .record(
            0,
            ElementResult::Placed {
                order_ids: vec!["accepted".to_owned()],
            },
        )
        .unwrap();
    journal
        .record(
            1,
            ElementResult::NotSent {
                error: Some(Box::new(
                    VenueError::new("refused").outcome(Outcome::Rejected),
                )),
            },
        )
        .unwrap();
    for aggregate in [None, Some(Outcome::Rejected), Some(Outcome::NotSent)] {
        let known = journal.attach(VenueError {
            message: "partly refused".to_owned(),
            outcome: aggregate,
            ..Default::default()
        });
        assert_eq!(
            placement_outcome(Err(known.clone())),
            Err(known),
            "native journal proves a fully known partial even without aggregate partial IDs"
        );
    }
    let aggregate_unknown =
        journal.attach(VenueError::new("uncertain aggregate").outcome(Outcome::Unknown));
    let error = placement_outcome(Err(aggregate_unknown)).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.message, ENGINE_UNKNOWN_TEXT);
    journal
        .record(
            1,
            ElementResult::Unknown {
                error: Box::new(VenueError::new("uncertain leg")),
            },
        )
        .unwrap();
    for aggregate in [None, Some(Outcome::Rejected), Some(Outcome::NotSent)] {
        let error = placement_outcome(Err(journal.attach(VenueError {
            message: "native unknown".to_owned(),
            outcome: aggregate,
            ..Default::default()
        })))
        .unwrap_err();
        assert_eq!(
            error.outcome,
            Some(Outcome::Unknown),
            "Unknown leg takes precedence over an outer refusal hint"
        );
        assert_eq!(error.message, ENGINE_UNKNOWN_TEXT);
    }
}

/// A limit TWAP whose first slice rests as `order-1`, listed open when `listed`.
async fn resting_limit_slice(listed: bool) -> (Rc<Port>, AlgoCx, TwapFactory) {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    let port = Rc::new(Port::default());
    port.store.borrow_mut().tickers.insert(
        "BTCUSDT".into(),
        Ticker {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            bid: 100.0,
            ask: 100.5,
            last: 100.25,
            ..Ticker::default()
        },
    );
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
    factory
        .handle(
            "twap:start",
            &twap_options(json!({ "useLimitOrders": true })),
            &cx,
        )
        .unwrap();
    settle().await;
    if listed {
        port.store.borrow_mut().orders.push(Order {
            id: "order-1".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 100.0,
            amount: 0.5,
            filled: 0.0,
            remaining: 0.5,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        });
    }
    assert_eq!(
        factory.active_twaps("a")[0]["currentOrderIds"],
        json!(["order-1"])
    );
    (port, cx, factory)
}

fn cancelled_ids(port: &Port) -> Vec<(bool, Vec<String>)> {
    port.commands
        .borrow()
        .iter()
        .filter_map(|command| match command {
            Command::CancelOrders { ids, .. } => Some((true, ids.clone())),
            Command::CancelOrdersWithSymbols { orders, .. } => {
                Some((false, orders.iter().map(|order| order.id.clone()).collect()))
            }
            _ => None,
        })
        .collect()
}

fn errors(port: &Port) -> Vec<Value> {
    port.emits
        .borrow()
        .iter()
        .filter(|(op, _)| op == "error")
        .map(|(_, data)| data.clone())
        .collect()
}

#[tokio::test(start_paused = true)]
async fn stopping_a_limit_twap_cancels_its_resting_slice_and_disposal_waits_for_it() {
    LocalSet::new()
        .run_until(async {
            let (port, cx, factory) = resting_limit_slice(true).await;
            port.answer_ms.set(3_000);
            let id = factory.active_twaps("a")[0]["id"].clone();
            factory.handle("twap:stop", &id.to_string(), &cx).unwrap();
            assert_eq!(cancelled_ids(&port), [(true, vec!["order-1".to_owned()])]);
            let started = tokio::time::Instant::now();
            factory.placements_settled().await;
            assert!(started.elapsed() >= Duration::from_millis(3_000));
            assert!(errors(&port).is_empty(), "{:?}", errors(&port));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unconfirmed_slice_cancel_is_told_and_never_resent() {
    LocalSet::new()
        .run_until(async {
            let (port, _, factory) = resting_limit_slice(true).await;
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Unknown));
            factory.terminate_all();
            settle().await;
            tokio::time::sleep(Duration::from_millis(120_000)).await;
            settle().await;
            assert_eq!(cancelled_ids(&port).len(), 1);
            assert_eq!(
                errors(&port),
                [json!(
                    "We couldn’t confirm TWAP BTCUSDT’s limit orders were cancelled: buy 0.5 at 100 (order order-1). Check your open orders before trying again."
                )]
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_slice_gone_from_the_store_is_cancelled_by_symbol_and_its_refusal_is_quiet() {
    LocalSet::new()
        .run_until(async {
            let (port, cx, factory) = resting_limit_slice(false).await;
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Rejected));
            let id = factory.active_twaps("a")[0]["id"].clone();
            factory.handle("twap:stop", &id.to_string(), &cx).unwrap();
            settle().await;
            assert_eq!(cancelled_ids(&port), [(false, vec!["order-1".to_owned()])]);
            assert!(errors(&port).is_empty(), "{:?}", errors(&port));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_listed_slice_whose_cancel_is_refused_is_named_to_the_user() {
    LocalSet::new()
        .run_until(async {
            let (port, cx, factory) = resting_limit_slice(true).await;
            port.fail.set(true);
            port.fail_outcome
                .set(Some(safe_cex_engine::error::Outcome::Rejected));
            let id = factory.active_twaps("a")[0]["id"].clone();
            factory.handle("twap:stop", &id.to_string(), &cx).unwrap();
            settle().await;
            assert_eq!(
                errors(&port),
                [json!(
                    "TWAP BTCUSDT couldn’t cancel its limit orders: buy 0.5 at 100 (order order-1). rejected"
                )]
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_limit_slice_landing_after_an_owner_change_is_named_and_not_cancelled() {
    LocalSet::new()
        .run_until(async {
            let owner = Rc::new(RefCell::new(safe_cex_algo_twap::TwapOwner {
                owner_id: "A".into(),
                generation: 7,
            }));
            let port = Rc::new(Port::default());
            port.answer_ms.set(100);
            port.store.borrow_mut().tickers.insert(
                "BTCUSDT".into(),
                Ticker {
                    id: "BTCUSDT".into(),
                    symbol: "BTCUSDT".into(),
                    bid: 100.0,
                    ask: 100.5,
                    last: 100.25,
                    ..Ticker::default()
                },
            );
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::with_owner(
                "a",
                owner.borrow().clone(),
                cx.clone(),
                Rc::new(Chasers::default()),
            )
            .guard_owner(Rc::new({
                let owner = owner.clone();
                move || owner.borrow().clone()
            }));
            factory
                .handle(
                    "twap:start",
                    &twap_options(json!({ "useLimitOrders": true })),
                    &cx,
                )
                .unwrap();
            settle().await;
            owner.borrow_mut().owner_id = "B".into();
            tokio::time::sleep(Duration::from_millis(100)).await;
            settle().await;
            assert!(cancelled_ids(&port).is_empty());
            assert!(errors(&port).contains(&json!(
                "TWAP BTCUSDT’s limit order placed as the account connection changed is still open: buy 0.5 at 100 (order order-1)."
            )), "{:?}", errors(&port));
            cx.scope.dispose();
        })
        .await;
}
