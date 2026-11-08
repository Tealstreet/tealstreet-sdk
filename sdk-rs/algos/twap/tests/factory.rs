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

struct Port {
    store: RefCell<AlgoStore>,
    ids: Cell<CommandId>,
    commands: RefCell<Vec<Command>>,
    emits: RefCell<Vec<(String, Value)>>,
    fail: Cell<bool>,
    fail_outcome: Cell<Option<safe_cex_engine::error::Outcome>>,
    answer_ms: Cell<u64>,
}

impl Default for Port {
    fn default() -> Self {
        let mut store = AlgoStore::default();
        let market = market();
        store.markets.insert(market.symbol.clone(), market);
        Self {
            store: RefCell::new(store),
            ids: Cell::new(0),
            commands: RefCell::new(Vec::new()),
            emits: RefCell::new(Vec::new()),
            fail: Cell::new(false),
            fail_outcome: Cell::new(None),
            answer_ms: Cell::new(0),
        }
    }
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
async fn a_failed_slice_logs_an_error_and_continues() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.fail.set(true);
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
            assert_eq!(factory.active_twaps("a")[0]["ordersExecuted"], 1);
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
    for unreadable in [r#"{"orderIds":["a"]}"#, "null", "", "[]", r#"[""]"#] {
        let error = placement_outcome(Ok(unreadable.into())).unwrap_err();
        assert!(
            error
                .message
                .starts_with("order ids unknown, the order may be live"),
            "{unreadable}"
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown));
    }
    for outcome in [Outcome::Rejected, Outcome::NotSent, Outcome::Unknown] {
        let native = VenueError::new("native failure")
            .outcome(outcome)
            .kind(safe_cex_engine::error::ErrorKind::InsufficientBalance);
        let failure = placement_outcome(Err(native.clone())).unwrap_err();
        assert_eq!(failure.outcome, Some(outcome));
        assert_eq!(failure.kind, native.kind);
    }
    assert_eq!(
        placement_outcome(Err(VenueError::new("?")))
            .unwrap_err()
            .outcome,
        Some(Outcome::Unknown)
    );
    let partial = VenueError::new("partly refused")
        .outcome(Outcome::Rejected)
        .partial_ids(&["a".to_owned()]);
    let failure = placement_outcome(Err(partial)).unwrap_err();
    assert_eq!(failure.outcome, Some(Outcome::Unknown));
    assert_eq!(failure.partial.unwrap().order_ids, ["a"]);
}

#[derive(Default)]
struct RefusingChasers;

impl TwapChasers for RefusingChasers {
    fn start_with_id(&self, _cx: &AlgoCx, _request: &ChaserRequest) -> Result<(), String> {
        Err("chaser refused".to_owned())
    }
    fn progress(&self, _chaser_id: &str) -> Option<ChaserProgress> {
        None
    }
    fn remove(&self, _cx: &AlgoCx, _chaser_id: &str) {}
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
                factory.start(unbounded).map_err(|error| error.message),
                Err("TWAP BTCUSDT rejected: ordersCount must be between 1 and 1000".to_owned())
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
async fn last_wire_slice_above_max_refuses_with_zero_transport_requests() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let mut rules = market();
            rules.precision.amount = 1.0;
            rules.limits.amount = MinMax { min: 1.0, max: 1.5 };
            port.store
                .borrow_mut()
                .markets
                .insert(rules.symbol.clone(), rules);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            let options = serde_json::from_str(&twap_options(json!({ "size": 3 }))).unwrap();
            // Exact wire quantities are 1 then 2 after carrying the first slice's dust.
            let error: VenueError = factory.start(options).expect_err("invalid last slice");
            assert_eq!(
                error.kind,
                Some(safe_cex_engine::error::ErrorKind::InvalidInput)
            );
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::error::Outcome::NotSent)
            );
            settle().await;
            assert!(
                port.commands.borrow().is_empty(),
                "every transport command is recorded"
            );
            assert!(factory.active_twaps("a").is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn restored_valid_first_subminimum_last_refuses_before_resume() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            let mut rules = market();
            rules.precision.amount = 1.0;
            rules.limits.amount = MinMax {
                min: 2.0,
                max: 10.0,
            };
            port.store
                .borrow_mut()
                .markets
                .insert(rules.symbol.clone(), rules);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            let options: Value = serde_json::from_str(&twap_options(
                json!({ "size": 4, "randomnessPercentage": 50 }),
            ))
            .unwrap();
            let state = json!({ "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" },
            "size": 4, "ordersSizes": [3, 1], "side": "buy", "status": "paused",
            "ordersCount": 2, "ordersExecuted": 0, "nextOrderAt": 0 });
            factory
                .handle(
                    "twap:state",
                    &json!([{ "id": "restored", "options": options, "state": state }]).to_string(),
                    &cx,
                )
                .unwrap();
            factory.handle("twap:resume", r#""restored""#, &cx).unwrap();
            settle().await;
            tokio::time::advance(Duration::from_secs(120)).await;
            settle().await;
            assert!(
                port.commands.borrow().is_empty(),
                "must not dispatch the valid prefix"
            );
            assert!(factory.active_twaps("a").is_empty());
            assert!(port.emits.borrow().iter().any(|(op, data)| op == "error"
                && data.as_str().is_some_and(|text| text.contains("minimum"))));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn all_valid_wire_slices_dispatch_once_with_reduce_only_and_ids() {
    LocalSet::new().run_until(async {
        let port = Rc::new(Port::default());
        let mut rules = market();
        rules.precision.amount = 1.0;
        rules.limits.amount = MinMax { min: 2.0, max: 3.0 };
        port.store.borrow_mut().markets.insert(rules.symbol.clone(), rules);
        port.store.borrow_mut().venue = "binance".to_owned();
        let cx = AlgoCx::new(port.clone(), Scope::new());
        let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
        let options = serde_json::from_str(&twap_options(json!({ "size": 4, "reduceOnly": true }))).unwrap();
        factory.start(options).unwrap();
        settle().await;
        tokio::time::advance(Duration::from_secs(31)).await;
        settle().await;
        assert_eq!(placed_amounts(&port), [2.0, 2.0]);
        let commands = port.commands.borrow();
        assert_eq!(commands.len(), 2);
        let orders: Vec<_> = commands.iter().flat_map(|command| match command {
            Command::PlaceOrders { orders, .. } => orders.iter().collect(),
            _ => Vec::new(),
        }).collect();
        assert!(orders.iter().all(|order| order.reduce_only == Some(true) && order.client_order_id.is_some()));
        assert_ne!(orders[0].client_order_id, orders[1].client_order_id);
    }).await;
}

#[tokio::test(start_paused = true)]
async fn caller_market_cannot_replace_missing_native_rules() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            port.store.borrow_mut().markets.clear();
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            let options = serde_json::from_str(&twap_options(json!({
                "activeMarket": { "symbol": "BTCUSDT", "precision": { "amount": 0.001 },
                    "limits": { "amount": { "min": 0.001, "max": 100 } } }
            })))
            .unwrap();
            let error = factory
                .start(options)
                .expect_err("only the native store supplies rules");
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::error::Outcome::NotSent)
            );
            settle().await;
            assert!(port.commands.borrow().is_empty());
        })
        .await;
}

fn native_notional_port(minimum: f64, exempt: bool) -> Rc<Port> {
    let port = Rc::new(Port::default());
    {
        let mut store = port.store.borrow_mut();
        store.market_order_facts.insert(
            "BTCUSDT".to_owned(),
            safe_cex_engine::market_facts::MarketOrderFacts {
                min_notional: Some(minimum),
                reduce_only_exempt: exempt,
                ..Default::default()
            },
        );
        store.tickers.insert(
            "BTCUSDT".to_owned(),
            Ticker {
                symbol: "BTCUSDT".to_owned(),
                last: 10.0,
                ..Default::default()
            },
        );
    }
    port
}

#[tokio::test(start_paused = true)]
async fn native_notional_last_child_refuses_before_a_valid_prefix() {
    LocalSet::new()
        .run_until(async {
            let port = native_notional_port(20.0, false);
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx.clone(), Rc::new(Chasers::default()));
            let options: Value = serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap();
            let state = json!({ "accountName": "a", "activeMarket": { "symbol": "BTCUSDT" },
            "size": 4, "ordersSizes": [3, 1], "side": "buy", "status": "paused",
            "ordersCount": 2, "ordersExecuted": 0, "nextOrderAt": 0 });
            factory
                .handle(
                    "twap:state",
                    &json!([{ "id": "value-tail", "options": options, "state": state }])
                        .to_string(),
                    &cx,
                )
                .unwrap();
            factory
                .handle("twap:resume", r#""value-tail""#, &cx)
                .unwrap();
            settle().await;
            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert!(
                port.commands.borrow().is_empty(),
                "last child is worth10, below native20; first is worth30"
            );
            assert!(factory.active_twaps("a").is_empty());
            assert!(port.emits.borrow().iter().any(|(op, data)| op == "error"
                && data.as_str().is_some_and(|text| text.contains("notional"))));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_notional_missing_reference_is_typed_not_sent() {
    LocalSet::new()
        .run_until(async {
            let port = native_notional_port(20.0, false);
            port.store.borrow_mut().tickers.clear();
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            let options = serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap();
            let error = factory
                .start(options)
                .expect_err("known minimum requires a native reference");
            assert_eq!(
                error.kind,
                Some(safe_cex_engine::error::ErrorKind::InvalidInput)
            );
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::error::Outcome::NotSent)
            );
            settle().await;
            assert!(port.commands.borrow().is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_notional_all_valid_and_explicit_close_exemption_controls() {
    LocalSet::new()
        .run_until(async {
            for (minimum, exempt, reduce_only) in [(20.0, false, false), (30.0, true, true)] {
                let port = native_notional_port(minimum, exempt);
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options = serde_json::from_str(&twap_options(
                    json!({ "size": 4, "reduceOnly": reduce_only }),
                ))
                .unwrap();
                factory.start(options).unwrap();
                settle().await;
                tokio::time::advance(Duration::from_secs(31)).await;
                settle().await;
                assert_eq!(placed_amounts(&port), [2.0, 2.0]);
                assert_eq!(port.commands.borrow().len(), 2);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_tick_is_checked_even_when_notional_is_unknown_or_exempt() {
    LocalSet::new()
        .run_until(async {
            for minimum in [None, Some(20.0)] {
                let port = native_notional_port(20.0, true);
                port.store
                    .borrow_mut()
                    .market_order_facts
                    .get_mut("BTCUSDT")
                    .unwrap()
                    .min_notional = minimum;
                port.store
                    .borrow_mut()
                    .tickers
                    .get_mut("BTCUSDT")
                    .unwrap()
                    .bid = 0.2;
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options = serde_json::from_str(&twap_options(
                    json!({ "size": 4, "useLimitOrders": true, "reduceOnly": true }),
                ))
                .unwrap();
                let error = factory
                    .start(options)
                    .expect_err("notional status cannot bypass the price tick");
                assert_eq!(
                    error.outcome,
                    Some(safe_cex_engine::error::Outcome::NotSent)
                );
                assert!(error.message.contains("price"));
                settle().await;
                assert!(port.commands.borrow().is_empty());
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_quote_units_do_not_use_price_and_exemption_is_not_blanket_reduce_only() {
    LocalSet::new()
        .run_until(async {
            for (per_amount, succeeds) in [(10.0, true), (1.0, false)] {
                let port = native_notional_port(20.0, false);
                port.store.borrow_mut().tickers.clear();
                port.store
                    .borrow_mut()
                    .market_order_facts
                    .get_mut("BTCUSDT")
                    .unwrap()
                    .notional_basis =
                    safe_cex_engine::market_facts::NotionalBasis::Quote { per_amount };
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options =
                    serde_json::from_str(&twap_options(json!({ "size": 4, "reduceOnly": true })))
                        .unwrap();
                let result = factory.start(options);
                assert_eq!(result.is_ok(), succeeds);
                settle().await;
                assert_eq!(port.commands.borrow().len(), usize::from(succeeds));
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_marketable_limit_price_is_checked_even_when_notional_is_unknown() {
    LocalSet::new()
        .run_until(async {
            let port = native_notional_port(20.0, false);
            {
                let mut store = port.store.borrow_mut();
                let facts = store.market_order_facts.get_mut("BTCUSDT").unwrap();
                facts.min_notional = None;
                facts.market_slippage = Some(0.02);
                store.tickers.clear();
            }
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            let options = serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap();
            let error = factory
                .start(options)
                .expect_err("native marketable limit still requires an execution price");
            assert_eq!(
                error.outcome,
                Some(safe_cex_engine::error::Outcome::NotSent)
            );
            settle().await;
            assert!(port.commands.borrow().is_empty());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_market_notional_uses_the_rounded_execution_price_and_side() {
    LocalSet::new()
        .run_until(async {
            for (side, succeeds) in [("buy", true), ("sell", false)] {
                let port = native_notional_port(200.0, false);
                {
                    let mut store = port.store.borrow_mut();
                    store
                        .market_order_facts
                        .get_mut("BTCUSDT")
                        .unwrap()
                        .market_slippage = Some(0.02);
                    let ticker = store.tickers.get_mut("BTCUSDT").unwrap();
                    ticker.last = 0.0;
                    ticker.mark = 100.0;
                }
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options =
                    serde_json::from_str(&twap_options(json!({ "size": 4, "side": side })))
                        .unwrap();
                let result = factory.start(options);
                assert_eq!(result.is_ok(), succeeds, "2 units at102 or98, minimum200");
                settle().await;
                assert_eq!(port.commands.borrow().len(), usize::from(succeeds));
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_limit_notional_is_checked_at_the_exact_normalized_wire_price() {
    LocalSet::new()
        .run_until(async {
            for (minimum, succeeds) in [(200.0, true), (200.2, false)] {
                let port = native_notional_port(minimum, false);
                port.store
                    .borrow_mut()
                    .tickers
                    .get_mut("BTCUSDT")
                    .unwrap()
                    .bid = 100.2;
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options = serde_json::from_str(&twap_options(
                    json!({ "size": 4, "useLimitOrders": true }),
                ))
                .unwrap();
                let result = factory.start(options);
                assert_eq!(result.is_ok(), succeeds, "2 at rounded100, not raw100.2");
                settle().await;
                let commands = port.commands.borrow();
                assert_eq!(commands.len(), usize::from(succeeds));
                if succeeds {
                    let Command::PlaceOrders { orders, .. } = &commands[0] else {
                        panic!("placement command")
                    };
                    assert_eq!(orders[0].price, Some(100.0));
                    assert_eq!(orders[0].amount, 2.0);
                }
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_market_exemption_does_not_exempt_limit_children() {
    LocalSet::new()
        .run_until(async {
            for limit in [false, true] {
                let port = native_notional_port(1000.0, false);
                {
                    let mut store = port.store.borrow_mut();
                    store
                        .market_order_facts
                        .get_mut("BTCUSDT")
                        .unwrap()
                        .market_notional_exempt = true;
                    store.tickers.get_mut("BTCUSDT").unwrap().bid = 10.0;
                }
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options = serde_json::from_str(&twap_options(
                    json!({ "size": 4, "useLimitOrders": limit }),
                ))
                .unwrap();
                assert_eq!(factory.start(options).is_ok(), !limit);
                settle().await;
                assert_eq!(port.commands.borrow().len(), usize::from(!limit));
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn native_market_lot_grid_is_used_before_any_child_and_does_not_change_limits() {
    LocalSet::new()
        .run_until(async {
            for limit in [false, true] {
                let port = native_notional_port(0.0, false);
                {
                    let mut store = port.store.borrow_mut();
                    store
                        .market_order_facts
                        .get_mut("BTCUSDT")
                        .unwrap()
                        .market_amount = Some(safe_cex_engine::market_facts::MarketAmountRules {
                        minimum: Some(1.0),
                        maximum: Some(10.0),
                        step: Some(1.0),
                    });
                    store.tickers.get_mut("BTCUSDT").unwrap().bid = 10.0;
                }
                let cx = AlgoCx::new(port.clone(), Scope::new());
                let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
                let options = serde_json::from_str(&twap_options(
                    json!({ "size": 3, "useLimitOrders": limit }),
                ))
                .unwrap();
                factory.start(options).unwrap();
                settle().await;
                tokio::time::advance(Duration::from_secs(31)).await;
                settle().await;
                assert_eq!(
                    placed_amounts(&port),
                    if limit {
                        vec![1.5, 1.5]
                    } else {
                        vec![1.0, 2.0]
                    }
                );
                assert_eq!(port.commands.borrow().len(), 2);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn bingx_positive_infinite_maximum_keeps_valid_twaps_enabled() {
    LocalSet::new()
        .run_until(async {
            let port = Rc::new(Port::default());
            {
                let mut store = port.store.borrow_mut();
                store.venue = "bingx".to_owned();
                store.markets.get_mut("BTCUSDT").unwrap().limits.amount.max = f64::INFINITY;
            }
            let cx = AlgoCx::new(port.clone(), Scope::new());
            let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
            factory
                .start(serde_json::from_str(&twap_options(json!({ "size": 4 }))).unwrap())
                .expect("BingX publishes positive infinity to mean no quantity cap");
            settle().await;
            tokio::time::advance(Duration::from_secs(31)).await;
            settle().await;
            assert_eq!(placed_amounts(&port), [2.0, 2.0]);
            assert_eq!(port.commands.borrow().len(), 2);
        })
        .await;
}

async fn scripted_child_failure(outcome: Option<safe_cex_engine::error::Outcome>) {
    let port = Rc::new(Port::default());
    port.store.borrow_mut().venue = "binance".to_owned();
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = TwapFactory::new("a", cx, Rc::new(Chasers::default()));
    let id = factory
        .start(
            serde_json::from_str(&twap_options(json!({
                "size": 3, "ordersCount": 3, "reduceOnly": true
            })))
            .unwrap(),
        )
        .unwrap();
    settle().await;
    assert_eq!(placed_amounts(&port), [1.0]);
    port.fail.set(outcome.is_some());
    port.fail_outcome.set(outcome);
    tokio::time::advance(Duration::from_secs(21)).await;
    settle().await;
    tokio::time::advance(Duration::from_secs(21)).await;
    settle().await;
    tokio::time::advance(Duration::from_secs(100)).await;
    settle().await;
    let expected_writes = if outcome.is_some() { 2 } else { 3 };
    assert_eq!(
        port.commands.borrow().len(),
        expected_writes,
        "no suffix, resend, cancel or rollback after failure"
    );
    let commands = port.commands.borrow();
    let orders: Vec<_> = commands
        .iter()
        .flat_map(|command| match command {
            Command::PlaceOrders { orders, .. } => orders.iter().collect::<Vec<_>>(),
            _ => panic!("no repair/cancel command belongs to this plan"),
        })
        .collect();
    assert!(orders.iter().all(|order| order.reduce_only == Some(true)));
    assert_ne!(orders[0].client_order_id, orders[1].client_order_id);
    let emits = port.emits.borrow();
    let result = emits
        .iter()
        .rev()
        .find(|(op, data)| op == "twap:outcome" && data["id"] == id)
        .expect("terminal result must retain every indexed child outcome");
    let elements = result.1["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 3);
    for (index, element) in elements.iter().enumerate() {
        assert_eq!(element["index"], index);
        assert!(
            element["clientOrderId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        if let Some(order) = orders.get(index) {
            assert_eq!(
                element["clientOrderId"].as_str(),
                order.client_order_id.as_deref()
            );
        }
    }
    assert_eq!(elements[0]["status"], "placed");
    assert_eq!(elements[0]["orderIds"], json!(["order-1"]));
    match outcome {
        None => assert!(elements.iter().all(|element| element["status"] == "placed")),
        Some(safe_cex_engine::error::Outcome::Rejected) => {
            assert_eq!(elements[1]["status"], "notSent");
            assert_eq!(elements[1]["error"]["outcome"], "rejected");
            assert_eq!(elements[2]["status"], "notSent");
            assert_eq!(result.1["error"]["partial"]["orderIds"], json!(["order-1"]));
        }
        Some(safe_cex_engine::error::Outcome::Unknown) => {
            assert_eq!(elements[1]["status"], "unknown");
            assert_eq!(elements[1]["error"]["outcome"], "unknown");
            assert_eq!(elements[2]["status"], "notSent");
        }
        other => panic!("unexpected script {other:?}"),
    }
    assert!(factory.active_twaps("a").is_empty());
}

#[tokio::test(start_paused = true)]
async fn runtime_rejection_stops_suffix_and_keeps_indexed_outcomes() {
    LocalSet::new()
        .run_until(scripted_child_failure(Some(
            safe_cex_engine::error::Outcome::Rejected,
        )))
        .await;
}

#[tokio::test(start_paused = true)]
async fn runtime_unknown_stops_suffix_and_keeps_indexed_outcomes() {
    LocalSet::new()
        .run_until(scripted_child_failure(Some(
            safe_cex_engine::error::Outcome::Unknown,
        )))
        .await;
}

#[tokio::test(start_paused = true)]
async fn all_valid_schedule_reports_each_child_once() {
    LocalSet::new()
        .run_until(scripted_child_failure(None))
        .await;
}
