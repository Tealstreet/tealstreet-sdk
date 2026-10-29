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
        Box::pin(async move {
            if fail {
                Err(VenueError {
                    message: "rejected".into(),
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
}

#[derive(Default)]
struct Chasers {
    started: RefCell<Vec<ChaserRequest>>,
}

impl TwapChasers for Chasers {
    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) {
        self.started.borrow_mut().push(request.clone());
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

#[tokio::test(start_paused = true)]
async fn runs_slices_on_the_interval_and_reports_state() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new(Rc::new(Chasers::default()));
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
async fn a_failed_slice_logs_an_error_and_continues() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.fail.set(true);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new(Rc::new(Chasers::default()));
            factory
                .handle("twap:start", &twap_options(json!({})), &cx)
                .unwrap();
            settle().await;
            assert!(port.emits.borrow().contains(&(
                "log".to_owned(),
                json!(["TWAP BTCUSDT slice failed: rejected", "error"])
            )));
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 1);
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
            let factory = TwapFactory::new(chasers.clone());
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
async fn disposing_the_scope_stops_the_schedule_and_a_new_scope_restarts_it() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let scope = Scope::new();
            let cx = AlgoCx::new(port.clone(), scope.clone());
            let factory = TwapFactory::new(Rc::new(Chasers::default()));
            factory
                .handle(
                    "twap:start",
                    &twap_options(json!({ "ordersCount": 4 })),
                    &cx,
                )
                .unwrap();
            settle().await;
            scope.dispose();
            tokio::time::sleep(Duration::from_millis(60_000)).await;
            settle().await;
            assert_eq!(placed_amounts(&port).len(), 1);

            let id = factory.active_twaps("a")[0]["id"]
                .as_str()
                .unwrap()
                .to_owned();
            let cx = AlgoCx::new(port.clone(), Scope::new());
            factory
                .handle("twap:pause", &json!(id).to_string(), &cx)
                .unwrap();
            factory
                .handle("twap:resume", &json!(id).to_string(), &cx)
                .unwrap();
            tokio::time::sleep(Duration::from_millis(16_000)).await;
            settle().await;
            assert_eq!(placed_amounts(&port).len(), 2);
        })
        .await;
}

#[test]
fn rejects_malformed_payloads() {
    let factory = TwapFactory::new(Rc::new(Chasers::default()));
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    LocalSet::new().block_on(&rt, async {
        let cx = AlgoCx::new(Rc::new(Port::default()), Scope::new());
        assert!(factory.handle("twap:pause", "{", &cx).is_err());
    });
}
