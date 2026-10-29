//! Behaviour ported from the TS `ChaserOrderManager`/`ChaserOrderInstance`
//! tests, against a fake account port on a paused clock.
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_chaser::{ChaserFactory, MAX_CHASES_PER_ACCOUNT, UNCONFIRMED_ORDER_STOP_REASON};
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::{
    Limits, Market, MinMax, Order, OrderSide, OrderStatus, OrderType, Position, PositionSide,
    Precision, Ticker,
};
use tokio::sync::mpsc;
use tokio::task::LocalSet;

type Responder = Box<dyn FnMut(&Command) -> Result<String, VenueError>>;

struct FakePort {
    store: RefCell<AlgoStore>,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<Change>>>,
    commands: RefCell<Vec<Command>>,
    emitted: RefCell<Vec<(String, String)>>,
    responder: RefCell<Responder>,
    next_id: Cell<CommandId>,
    next_order: Cell<u32>,
}

impl AccountPort for FakePort {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        let (sender, receiver) = mpsc::unbounded_channel();
        self.subscribers.borrow_mut().push(sender);
        receiver
    }
    fn next_command_id(&self) -> CommandId {
        self.next_id.set(self.next_id.get() + 1);
        self.next_id.get()
    }
    fn command(&self, command: Command) -> CommandFuture {
        let result = (self.responder.borrow_mut())(&command);
        self.commands.borrow_mut().push(command);
        Box::pin(async move { result })
    }
    fn emit(&self, op: &str, data: String) {
        self.emitted.borrow_mut().push((op.into(), data));
    }
}

impl FakePort {
    fn new(venue: &str) -> Rc<Self> {
        let mut store = AlgoStore {
            venue: venue.into(),
            ..AlgoStore::default()
        };
        store.markets.insert("BTCUSDT".into(), market());
        store.tickers.insert("BTCUSDT".into(), ticker(100.0, 100.5));
        Rc::new(Self {
            store: RefCell::new(store),
            subscribers: RefCell::new(Vec::new()),
            commands: RefCell::new(Vec::new()),
            emitted: RefCell::new(Vec::new()),
            responder: RefCell::new(Box::new(|_| Ok("[]".into()))),
            next_id: Cell::new(0),
            next_order: Cell::new(0),
        })
    }

    /// Placements return fresh ids `o1`, `o2`, ... and land as open rows.
    fn accept_placements(self: &Rc<Self>) {
        let port = Rc::downgrade(self);
        *self.responder.borrow_mut() = Box::new(move |command| {
            let port = port.upgrade().unwrap();
            if let Command::PlaceOrders { orders, .. } = command {
                port.next_order.set(port.next_order.get() + 1);
                let id = format!("o{}", port.next_order.get());
                let opts = &orders[0];
                port.store.borrow_mut().orders.push(order(
                    &id,
                    opts.price.unwrap(),
                    opts.amount,
                    0.0,
                    OrderStatus::Open,
                ));
                return Ok(format!(r#"["{id}"]"#));
            }
            Ok("null".into())
        });
    }

    fn publish(&self, change: Change) {
        self.subscribers
            .borrow_mut()
            .retain(|sender| sender.send(change.clone()).is_ok());
    }

    fn set_ticker(&self, bid: f64, ask: f64) {
        self.store
            .borrow_mut()
            .tickers
            .insert("BTCUSDT".into(), ticker(bid, ask));
        self.publish(Change {
            tickers: true,
            ..Change::default()
        });
    }

    fn update_order(&self, id: &str, edit: impl FnOnce(&mut Order)) {
        {
            let mut store = self.store.borrow_mut();
            let row = store
                .orders
                .iter_mut()
                .find(|order| order.id == id)
                .unwrap();
            edit(row);
            store.update_ids.orders += 1;
        }
        self.publish(Change {
            orders: true,
            ..Change::default()
        });
    }

    fn remove_orders(&self) {
        {
            let mut store = self.store.borrow_mut();
            store.orders.clear();
            store.update_ids.orders += 1;
        }
        self.publish(Change {
            orders: true,
            ..Change::default()
        });
    }

    fn placements(&self) -> Vec<(f64, f64)> {
        self.commands
            .borrow()
            .iter()
            .filter_map(|command| match command {
                Command::PlaceOrders { orders, .. } => {
                    Some((orders[0].price.unwrap(), orders[0].amount))
                }
                _ => None,
            })
            .collect()
    }

    fn cancels(&self) -> Vec<String> {
        self.commands
            .borrow()
            .iter()
            .filter_map(|command| match command {
                Command::CancelOrdersWithSymbols { orders, .. } => Some(orders[0].id.clone()),
                _ => None,
            })
            .collect()
    }

    fn completes(&self) -> Vec<serde_json::Value> {
        self.emitted
            .borrow()
            .iter()
            .filter(|(op, _)| op == "chaser:complete")
            .map(|(_, data)| serde_json::from_str(data).unwrap())
            .collect()
    }

    fn messages(&self, op: &str) -> Vec<String> {
        self.emitted
            .borrow()
            .iter()
            .filter(|(emitted, _)| emitted == op)
            .map(|(_, data)| serde_json::from_str(data).unwrap())
            .collect()
    }
}

fn market() -> Market {
    Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
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

fn ticker(bid: f64, ask: f64) -> Ticker {
    Ticker {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        bid,
        ask,
        last: (bid + ask) / 2.0,
        ..Ticker::default()
    }
}

fn order(id: &str, price: f64, amount: f64, filled: f64, status: OrderStatus) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price,
        amount,
        filled,
        remaining: amount - filled,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn start(port: &Rc<FakePort>, payload: &str) -> (ChaserFactory, AlgoCx) {
    let cx = AlgoCx::new(port.clone(), Scope::new());
    let factory = ChaserFactory::new(cx.clone());
    factory.handle("chaser:start", payload, &cx).unwrap();
    (factory, cx)
}

const BUY_ONE: &str =
    r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0,"clientId":"c1"}"#;

async fn settle(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

async fn local(test: impl std::future::Future<Output = ()>) {
    LocalSet::new().run_until(test).await
}

#[tokio::test(start_paused = true)]
async fn a_buy_rests_post_only_at_the_bid_and_a_sell_at_the_ask() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _buy = start(&port, BUY_ONE);
        let _sell = start(
            &port,
            r#"{"symbol":"BTCUSDT","size":2,"side":"sell","maxAttempts":10,"delay":0}"#,
        );
        settle(10).await;
        assert_eq!(port.placements(), vec![(100.0, 1.0), (100.5, 2.0)]);
        let Command::PlaceOrders { orders, .. } = &port.commands.borrow()[0] else {
            panic!()
        };
        assert_eq!(orders[0].kind, OrderType::Limit);
        assert_eq!(
            orders[0].time_in_force,
            Some(safe_cex_engine::model::TimeInForce::PostOnly)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_locked_quote_keeps_the_buy_below_the_ask() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        port.store
            .borrow_mut()
            .tickers
            .insert("BTCUSDT".into(), ticker(100.5, 100.5));
        let _chase = start(&port, BUY_ONE);
        settle(10).await;
        assert_eq!(port.placements(), vec![(100.0, 1.0)]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_adverse_move_cancels_and_replaces_at_the_new_bid() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        assert_eq!(port.placements(), vec![(100.0, 1.0), (101.0, 1.0)]);
        assert_eq!(factory.book().active_chasers()[0].attempts, 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_favorable_move_leaves_the_order_resting() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(99.0, 99.5);
        settle(200).await;
        assert!(port.cancels().is_empty());
        assert_eq!(port.placements().len(), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_full_fill_completes_once_and_never_replaces() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |order| {
            order.filled = 1.0;
            order.status = OrderStatus::Closed;
        });
        settle(100).await;
        // As in TS, cleanup cancels the last current id; on a filled order the
        // venue refuses it harmlessly.
        assert_eq!(port.placements().len(), 1);
        let completes = port.completes();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0]["filled"], 1.0);
        assert_eq!(completes[0]["clientId"], "c1");
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_partial_fill_holds_steady_then_replaces_only_the_remainder() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |order| order.filled = 0.4);
        settle(10).await;
        port.set_ticker(101.0, 101.5);
        settle(200).await;
        assert!(
            port.cancels().is_empty(),
            "held steady inside the partial-fill window"
        );
        settle(1000).await;
        port.set_ticker(101.5, 102.0);
        settle(100).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        let placements = port.placements();
        assert_eq!(placements.last().unwrap().0, 101.5);
        assert!((placements.last().unwrap().1 - 0.6).abs() < 1e-9);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn user_stop_cancels_the_resting_order_and_reports_it() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let id = factory.book().active_chasers()[0].id.clone();
        factory
            .handle(
                "chaser:terminate",
                &serde_json::to_string(&id).unwrap(),
                &cx,
            )
            .unwrap();
        settle(10).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        let completes = port.completes();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0]["stopReason"], "User cancelled");
        assert_eq!(completes[0]["filled"], 0.0);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn keep_order_stops_without_cancelling() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let book = factory.book();
        let id = book.active_chasers()[0].id.clone();
        book.remove_keep_order(&id);
        settle(10).await;
        port.set_ticker(105.0, 105.5);
        settle(100).await;
        assert!(port.cancels().is_empty());
        assert_eq!(port.placements().len(), 1);
        assert_eq!(
            port.completes()[0]["stopReason"],
            safe_cex_chaser::KEPT_ORDER_STOP_REASON
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_vanished_order_needs_two_order_frames_to_stop_the_chase() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |_| {});
        settle(10).await;
        port.remove_orders();
        settle(10).await;
        assert!(
            port.completes().is_empty(),
            "one missing frame is not enough"
        );
        port.remove_orders();
        settle(10).await;
        assert_eq!(
            port.completes()[0]["stopReason"],
            "Tracked orders disappeared from store"
        );
        assert_eq!(
            port.cancels(),
            vec!["o1".to_string()],
            "the last known id is cancelled"
        );
        assert_eq!(port.messages("warning").len(), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_post_only_refusal_steps_one_tick_further_and_retries() {
    local(async {
        let port = FakePort::new("bybit");
        let calls = Rc::new(Cell::new(0));
        let counter = calls.clone();
        *port.responder.borrow_mut() = Box::new(move |command| {
            if matches!(command, Command::PlaceOrders { .. }) {
                counter.set(counter.get() + 1);
                if counter.get() == 1 {
                    return Ok("[]".into());
                }
                return Ok(r#"["o9"]"#.into());
            }
            Ok("null".into())
        });
        let _chase = start(&port, BUY_ONE);
        settle(300).await;
        assert_eq!(port.placements(), vec![(100.0, 1.0), (99.5, 1.0)]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_unknown_placement_outcome_stops_without_placing_again() {
    local(async {
        let port = FakePort::new("bybit");
        *port.responder.borrow_mut() = Box::new(|command| {
            if matches!(command, Command::PlaceOrders { .. }) {
                return Err(VenueError {
                    outcome: Some(Outcome::Unknown),
                    ..VenueError::new("timed out")
                });
            }
            Ok("null".into())
        });
        let _chase = start(&port, BUY_ONE);
        settle(2000).await;
        assert_eq!(port.placements().len(), 1);
        assert_eq!(
            port.completes()[0]["stopReason"],
            UNCONFIRMED_ORDER_STOP_REASON
        );
        assert!(port.messages("error")[0].contains("check open orders"));
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn the_length_condition_stops_and_cancels() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _chase = start(
            &port,
            r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0,"terminate":{"type":"percent","value":1,"anchorPrice":100}}"#,
        );
        settle(1100).await;
        port.set_ticker(101.5, 102.0);
        settle(100).await;
        assert_eq!(port.completes()[0]["stopReason"], "Terminated by length condition");
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        assert_eq!(port.placements().len(), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_reduce_only_chase_ends_when_its_position_is_gone() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        port.store.borrow_mut().positions.push(Position {
            id: "p".into(),
            symbol: "BTCUSDT".into(),
            side: PositionSide::Short,
            entry_price: 100.0,
            notional: 100.0,
            leverage: 1.0,
            margin_type: None,
            position_mode: None,
            unrealized_pnl: 0.0,
            realized_pnl: None,
            contracts: 1.0,
            liquidation_price: 0.0,
            margin_size: None,
            last_update: None,
        });
        let _chase = start(&port, r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0,"reduceOnly":true}"#);
        settle(1100).await;
        port.store.borrow_mut().positions.clear();
        port.update_order("o1", |_| {});
        settle(100).await;
        assert_eq!(port.completes()[0]["stopReason"], "Completed");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn fill_events_complete_a_hyperliquid_chase() {
    local(async {
        let port = FakePort::new("hyperliquid");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        let fill = Fill {
            id: None,
            order_id: "o1".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Buy,
            price: 100.0,
            amount: 1.0,
        };
        port.publish(Change {
            fills: vec![fill],
            ..Change::default()
        });
        settle(10).await;
        assert_eq!(
            port.completes()[0]["stopReason"],
            "Fully filled via fill events"
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn hyperliquid_stops_before_replacing_when_fill_reconciliation_fails() {
    local(async {
        let port = FakePort::new("hyperliquid");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        let accept = std::mem::replace(
            &mut *port.responder.borrow_mut(),
            Box::new(|_| Ok("null".into())),
        );
        let accept = RefCell::new(accept);
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::Query { .. } => Err(VenueError::new("history unavailable")),
            other => (accept.borrow_mut())(other),
        });
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        assert_eq!(
            port.placements().len(),
            1,
            "no replacement after a failed reconciliation"
        );
        assert_eq!(
            port.completes()[0]["stopReason"],
            "Failed to reconcile fills after cancellation"
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn terminate_all_stops_every_chase_with_a_warning() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        factory.handle("chaser:start", BUY_ONE, &cx).unwrap();
        settle(1100).await;
        factory.handle("chaser:terminateAll", "null", &cx).unwrap();
        settle(10).await;
        assert_eq!(port.cancels().len(), 2);
        assert_eq!(port.completes().len(), 2);
        assert_eq!(port.messages("warning").len(), 2);
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

fn reply_to_placements(port: &Rc<FakePort>, result: &'static str) {
    *port.responder.borrow_mut() = Box::new(move |command| {
        if matches!(command, Command::PlaceOrders { .. }) {
            return Ok(result.into());
        }
        Ok("null".into())
    });
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_accepted_placement_stops_without_placing_again() {
    for result in [r#"{"orderIds":["x"]}"#, "null"] {
        local(async {
            let port = FakePort::new("bybit");
            reply_to_placements(&port, result);
            let _chase = start(&port, BUY_ONE);
            settle(2000).await;
            assert_eq!(port.placements().len(), 1, "{result}");
            assert_eq!(
                port.completes()[0]["stopReason"],
                UNCONFIRMED_ORDER_STOP_REASON,
                "{result}"
            );
            assert!(port.messages("error")[0].contains("check open orders"));
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn shutdown_then_dispose_leaves_no_live_order() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        factory.handle("chaser:start", BUY_ONE, &cx).unwrap();
        settle(1100).await;
        factory.book().shutdown("Account disposed").await;
        cx.scope.dispose();
        let mut cancelled = port.cancels();
        cancelled.sort();
        assert_eq!(cancelled, vec!["o1".to_string(), "o2".to_string()]);
        assert_eq!(port.completes().len(), 2);
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn starts_beyond_the_account_cap_are_refused() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        for _ in 1..MAX_CHASES_PER_ACCOUNT {
            factory.handle("chaser:start", BUY_ONE, &cx).unwrap();
        }
        let refused = factory.handle("chaser:start", BUY_ONE, &cx).unwrap_err();
        assert!(refused.contains("already active"), "{refused}");
        assert_eq!(
            factory.book().active_chasers().len(),
            MAX_CHASES_PER_ACCOUNT
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_non_finite_price_stops_and_cleans_up_instead_of_panicking() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        port.store
            .borrow_mut()
            .tickers
            .insert("BTCUSDT".into(), ticker(f64::NAN, f64::NAN));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(100).await;
        assert!(port.placements().is_empty());
        assert_eq!(port.completes()[0]["stopReason"], "No usable quote");
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

/// Placements are lost (unknown outcome); lookups answer with `lookup`.
fn lose_placements(port: &Rc<FakePort>, lookup: &'static str) {
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::PlaceOrders { .. } => Err(VenueError {
            outcome: Some(Outcome::Unknown),
            ..VenueError::new("timed out")
        }),
        Command::Query {
            query: safe_cex_engine::engine::Query::OrderByClientId { .. },
            ..
        } => Ok(lookup.into()),
        _ => Ok("null".into()),
    });
}

fn client_ids(port: &FakePort) -> Vec<Option<String>> {
    port.commands
        .borrow()
        .iter()
        .filter_map(|command| match command {
            Command::PlaceOrders { orders, .. } => Some(orders[0].client_order_id.clone()),
            _ => None,
        })
        .collect()
}

fn lookups(port: &FakePort) -> usize {
    port.commands
        .borrow()
        .iter()
        .filter(|command| {
            matches!(
                command,
                Command::Query {
                    query: safe_cex_engine::engine::Query::OrderByClientId { .. },
                    ..
                }
            )
        })
        .count()
}

#[tokio::test(start_paused = true)]
async fn a_lost_placement_found_open_is_adopted() {
    local(async {
        let port = FakePort::new("hyperliquid");
        lose_placements(
            &port,
            r#"{"found":true,"order":{"id":"o7","status":"open","filled":0}}"#,
        );
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1500).await;
        assert_eq!(port.placements().len(), 1);
        let id = client_ids(&port)[0].clone().unwrap();
        assert!(id.starts_with("0x") && id.len() == 34, "{id}");
        assert_eq!(
            factory.book().active_chasers()[0].current_order_ids,
            vec!["o7".to_string()]
        );
        assert!(port.completes().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_lost_placement_never_found_stops_after_the_lookups() {
    local(async {
        let port = FakePort::new("hyperliquid");
        lose_placements(&port, r#"{"found":false,"definitive":false}"#);
        let _chase = start(&port, BUY_ONE);
        settle(20_000).await;
        assert_eq!(port.placements().len(), 1);
        assert_eq!(lookups(&port), 4);
        assert_eq!(
            port.completes()[0]["stopReason"],
            UNCONFIRMED_ORDER_STOP_REASON
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_lost_placement_found_filled_counts_and_places_only_the_rest() {
    local(async {
        let port = FakePort::new("hyperliquid");
        let placed = Rc::new(Cell::new(0));
        let count = placed.clone();
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::PlaceOrders { .. } => {
                count.set(count.get() + 1);
                if count.get() == 1 {
                    return Err(VenueError {
                        outcome: Some(Outcome::Unknown),
                        ..VenueError::new("timed out")
                    });
                }
                Ok(r#"["o2"]"#.into())
            }
            Command::Query {
                query: safe_cex_engine::engine::Query::OrderByClientId { .. },
                ..
            } => Ok(
                r#"{"found":true,"order":{"id":"o1","status":"canceled","filled":0.25}}"#.into(),
            ),
            _ => Ok("null".into()),
        });
        let _chase = start(&port, BUY_ONE);
        settle(2000).await;
        let placements = port.placements();
        assert_eq!(placements.len(), 2);
        assert!((placements[1].1 - 0.75).abs() < 1e-9, "{placements:?}");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_venue_without_client_id_lookup_places_without_one() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(10).await;
        assert_eq!(client_ids(&port), vec![None]);
    })
    .await;
}
