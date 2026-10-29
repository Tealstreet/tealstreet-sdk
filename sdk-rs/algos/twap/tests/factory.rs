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
use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
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

#[tokio::test(start_paused = true)]
async fn runs_slices_on_the_interval_and_reports_state() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new(cx.clone(), Rc::new(Chasers::default()));
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
            let factory = TwapFactory::new(cx.clone(), Rc::new(Chasers::default()));
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
            let factory = TwapFactory::new(cx.clone(), chasers.clone());
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
            let factory = TwapFactory::new(account.clone(), Rc::new(Chasers::default()));
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
async fn terminate_all_stops_every_twap_and_reports_it() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new(cx.clone(), Rc::new(Chasers::default()));
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
        let factory = TwapFactory::new(cx.clone(), Rc::new(Chasers::default()));
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
        assert!(
            error.starts_with("order ids unknown, the order may be live"),
            "{unreadable}"
        );
    }
    let unknown = VenueError {
        message: "timeout".into(),
        outcome: Some(Outcome::Unknown),
        ..Default::default()
    };
    assert_eq!(
        placement_outcome(Err(unknown)),
        Err("outcome unknown, the order may be live: timeout".to_owned())
    );
    let rejected = VenueError {
        message: "insufficient margin".into(),
        outcome: Some(Outcome::Rejected),
        ..Default::default()
    };
    assert_eq!(
        placement_outcome(Err(rejected)),
        Err("insufficient margin".to_owned())
    );
}

#[tokio::test(start_paused = true)]
async fn chase_slices_run_on_the_account_chaser_book() {
    LocalSet::new()
        .run_until(async {
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
            let book = safe_cex_chaser::ChaserFactory::new().book(&cx);
            let factory = TwapFactory::new(cx.clone(), Rc::new(book.clone()));
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
