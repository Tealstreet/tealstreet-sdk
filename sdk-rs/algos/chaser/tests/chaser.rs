//! Behaviour ported from the TS `ChaserOrderManager`/`ChaserOrderInstance`
//! tests, against a fake account port on a paused clock.
use std::cell::{Cell, Ref, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algos::{AccountPort, AlgoCx, AlgoFactory, AlgoStore, Change, CommandFuture, Scope};
use safe_cex_chaser::{ChaserFactory, MAX_CHASES_PER_ACCOUNT, UNCONFIRMED_ORDER_STOP_REASON};
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::{ErrorKind, Outcome, VenueError};
use safe_cex_engine::model::{
    Limits, Market, MinMax, Order, OrderSide, OrderStatus, OrderType, Position, PositionSide,
    Precision, Ticker,
};
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[path = "chaser/two_owner_deferral.rs"]
mod two_owner_deferral;

type Responder = Box<dyn FnMut(&Command) -> Result<String, VenueError>>;

/// Which commands go into a dead link.
type Unanswered = Box<dyn Fn(&Command) -> bool>;

struct FakePort {
    store: RefCell<AlgoStore>,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<Change>>>,
    commands: RefCell<Vec<Command>>,
    emitted: RefCell<Vec<(String, String)>>,
    responder: RefCell<Responder>,
    next_id: Cell<CommandId>,
    next_order: Cell<u32>,
    capabilities: Cell<safe_cex_engine::Capabilities>,
    /// Commands sent into a dead link: never answered.
    unanswered: RefCell<Option<Unanswered>>,
    /// Every cancel answers only after this long, like a sign waiting out its timeout.
    cancel_answer_ms: Cell<u64>,
    /// Every placement answers, and its row lands, only after this long.
    place_answer_ms: Cell<u64>,
    /// A placement's row is not in the store when it answers.
    rows_lag: Cell<bool>,
    /// Cancels answer done but leave the row as it was: Bybit 110001 for an
    /// order that filled unseen.
    cancels_find_nothing: Cell<bool>,
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
        if self
            .unanswered
            .borrow()
            .as_ref()
            .is_some_and(|dead| dead(&command))
        {
            self.commands.borrow_mut().push(command);
            return Box::pin(std::future::pending());
        }
        let delay = if is_cancel(&command) {
            self.cancel_answer_ms.get()
        } else if matches!(command, Command::PlaceOrders { .. }) {
            self.place_answer_ms.get()
        } else {
            0
        };
        let result = (self.responder.borrow_mut())(&command);
        self.commands.borrow_mut().push(command);
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
            result
        })
    }
    fn emit(&self, op: &str, data: String) {
        self.emitted.borrow_mut().push((op.into(), data));
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.capabilities.get()
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
            capabilities: Cell::new(safe_cex_engine::Capabilities {
                cancel_with_symbols: true,
                ..safe_cex_engine::Capabilities::BASE
            }),
            unanswered: RefCell::new(None),
            cancel_answer_ms: Cell::new(0),
            place_answer_ms: Cell::new(0),
            rows_lag: Cell::new(false),
            cancels_find_nothing: Cell::new(false),
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
                if !port.rows_lag.get() {
                    port.store.borrow_mut().orders.push(order(
                        &id,
                        opts.price.unwrap(),
                        opts.amount,
                        0.0,
                        OrderStatus::Open,
                    ));
                }
                return Ok(format!(r#"["{id}"]"#));
            }
            // A cancel acknowledgement closes the row, as the engine does.
            let cancelled = match command {
                Command::CancelOrders { ids, .. } => ids.clone(),
                Command::CancelOrdersWithSymbols { orders, .. } => {
                    orders.iter().map(|order| order.id.clone()).collect()
                }
                _ => Vec::new(),
            };
            if !port.cancels_find_nothing.get() {
                let mut store = port.store.borrow_mut();
                for order in store
                    .orders
                    .iter_mut()
                    .filter(|order| cancelled.contains(&order.id))
                {
                    order.status = OrderStatus::Canceled;
                }
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
                Command::CancelOrders { ids, .. } => Some(ids[0].clone()),
                _ => None,
            })
            .collect()
    }

    /// The venue's cancel forms: Bybit has no symbol-qualified cancel.
    fn without_symbol_cancel(&self) {
        self.capabilities.set(safe_cex_engine::Capabilities::BASE);
    }

    /// Every cancel is refused before it is sent; other commands answer as before.
    fn refuse_cancels(&self) {
        let mut inner = std::mem::replace(
            &mut *self.responder.borrow_mut(),
            Box::new(|_| Ok("[]".into())),
        );
        *self.responder.borrow_mut() = Box::new(move |command| match command {
            Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } => {
                Err(VenueError::new("cancel refused").outcome(Outcome::NotSent))
            }
            other => inner(other),
        });
    }

    fn errors(&self) -> Vec<String> {
        self.emitted
            .borrow()
            .iter()
            .filter(|(op, _)| op == "error")
            .map(|(_, data)| data.clone())
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
async fn a_venue_without_symbol_cancel_requotes_by_id() {
    local(async {
        let port = FakePort::new("bybit");
        port.without_symbol_cancel();
        port.accept_placements();
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        let commands = port.commands.borrow();
        assert!(
            commands.iter().any(
                |c| matches!(c, Command::CancelOrders { ids, .. } if ids == &["o1".to_string()])
            ),
            "{commands:?}"
        );
        assert!(
            !commands
                .iter()
                .any(|c| matches!(c, Command::CancelOrdersWithSymbols { .. }))
        );
        drop(commands);
        assert_eq!(port.placements(), vec![(100.0, 1.0), (101.0, 1.0)]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_chase_with_no_market_data_says_so_once_and_a_ticking_one_does_not() {
    local(async {
        let port = FakePort::new("blofin");
        port.accept_placements();
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        for _ in 0..3 {
            settle(20_000).await;
            port.set_ticker(100.0, 100.5);
        }
        assert!(
            warnings(&port)
                .iter()
                .all(|text| !text.contains("no market data"))
        );
        settle(100_000).await;
        let told = warnings(&port)
            .iter()
            .filter(|text| text.contains("no market data"))
            .count();
        assert_eq!(told, 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_chase_keeps_its_row_until_its_completion_is_out() {
    local(async {
        let port = FakePort::new("hyperliquid");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        *port.unanswered.borrow_mut() =
            Some(Box::new(|command| matches!(command, Command::Query { .. })));
        let id = factory.book().active_chasers()[0].id.clone();
        factory.book().remove(&id);
        // Past the page's 3 s for a row gone alone, with the fills read still out.
        settle(5_000).await;
        assert!(port.completes().is_empty());
        assert_eq!(factory.book().active_chasers().len(), 1);
        settle(20_000).await;
        assert_eq!(port.completes().len(), 1);
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_key_ends_the_chase_on_its_first_placement() {
    local(async {
        let port = FakePort::new("hyperliquid");
        let tries = Rc::new(std::cell::Cell::new(0));
        let seen = tries.clone();
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::PlaceOrders { .. } => {
                seen.set(seen.get() + 1);
                Err(VenueError::new("User or API Wallet does not exist")
                    .kind(ErrorKind::AuthInvalid)
                    .outcome(Outcome::NotSent))
            }
            _ => Ok("[]".into()),
        });
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(30_000).await;
        assert_eq!(tries.get(), 1);
        assert!(
            port.errors()
                .iter()
                .any(|error| error.contains("does not exist"))
        );
        assert_eq!(port.completes().len(), 1);
        assert!(
            factory
                .book()
                .active_chasers()
                .iter()
                .all(|chaser| chaser.is_stopped)
        );
    })
    .await;
}

/// Counts placements while answering each with `error`.
fn refuse_placements(port: &FakePort, error: VenueError) -> Rc<Cell<u32>> {
    let tries = Rc::new(Cell::new(0));
    let seen = tries.clone();
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::PlaceOrders { .. } => {
            seen.set(seen.get() + 1);
            Err(error.clone())
        }
        _ => Ok("[]".into()),
    });
    tries
}

#[tokio::test(start_paused = true)]
async fn a_reduce_only_refused_for_position_ends_the_chase_on_its_first_placement() {
    local(async {
        let port = FakePort::new("bitget");
        let tries = refuse_placements(
            &port,
            VenueError::new("Not enough position is available")
                .kind(ErrorKind::InvalidInput)
                .outcome(Outcome::Rejected),
        );
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(30_000).await;
        assert_eq!(tries.get(), 1);
        assert_eq!(
            port.errors(),
            [r#""Chaser for BTCUSDT stopped: the venue refused the order: Not enough position is available""#]
        );
        assert_eq!(port.completes().len(), 1);
        assert!(
            factory
                .book()
                .active_chasers()
                .iter()
                .all(|chaser| chaser.is_stopped)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_unaffordable_or_unsupported_order_is_placed_once() {
    for kind in [ErrorKind::InsufficientBalance, ErrorKind::Unsupported] {
        local(async {
            let port = FakePort::new("bitget");
            let tries = refuse_placements(
                &port,
                VenueError::new("refused")
                    .kind(kind)
                    .outcome(Outcome::Rejected),
            );
            let _chase = start(&port, BUY_ONE);
            settle(30_000).await;
            assert_eq!(tries.get(), 1, "{kind:?}");
            assert_eq!(port.errors().len(), 1, "{kind:?}: {:?}", port.errors());
        })
        .await;
    }
}

/// Binance GTX_ORDER_REJECT, as its code table classifies it: a post-only cross
/// is the routine re-price, so it retries to Binance's 20-try bound.
#[tokio::test(start_paused = true)]
async fn a_binance_post_only_cross_retries_to_its_bound() {
    local(async {
        let port = FakePort::new("binance");
        let tries = refuse_placements(
            &port,
            VenueError::new("Due to the order could not be executed as maker, the Post Only order will be rejected.")
                .kind(ErrorKind::Rejected)
                .outcome(Outcome::Rejected)
                .venue_code("-5022"),
        );
        let _chase = start(&port, BUY_ONE);
        settle(30_000).await;
        assert_eq!(tries.get(), 19);
        assert_eq!(port.errors().len(), 1, "{:?}", port.errors());
        assert!(port.errors()[0].contains("after 20 tries"), "{:?}", port.errors());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_passing_placement_failure_is_still_retried() {
    for kind in [
        ErrorKind::RateLimited,
        ErrorKind::Timeout,
        ErrorKind::Network,
        ErrorKind::Rejected,
    ] {
        local(async {
            let port = FakePort::new("bitget");
            let tries = refuse_placements(
                &port,
                VenueError::new("try again")
                    .kind(kind)
                    .outcome(Outcome::NotSent),
            );
            let _chase = start(&port, BUY_ONE);
            settle(30_000).await;
            assert_eq!(tries.get(), 9, "{kind:?}");
            assert_eq!(
                port.errors(),
                [r#""Chaser failed to place orders after 10 tries: try again""#],
                "{kind:?}"
            );
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_refused_requote_cancel_stops_the_chase_visibly_and_places_nothing_more() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.refuse_cancels();
        port.set_ticker(101.0, 101.5);
        settle(200).await;
        assert_eq!(
            port.placements().len(),
            1,
            "no replacement over a live order"
        );
        assert!(!port.errors().is_empty());
        let completes = port.completes();
        assert_eq!(completes.len(), 1);
        assert_eq!(completes[0]["stopReason"], "Failed to cancel orders");
        assert!(
            factory
                .book()
                .active_chasers()
                .iter()
                .all(|chaser| chaser.is_stopped)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_cancel_on_stop_says_the_order_may_still_be_open() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.refuse_cancels();
        let id = factory.book().active_chasers()[0].id.clone();
        factory
            .handle(
                "chaser:terminate",
                &serde_json::to_string(&id).unwrap(),
                &cx,
            )
            .unwrap();
        settle(10).await;
        assert!(
            port.errors().iter().any(|e| e.contains("o1")
                && e.contains("may still be open")
                && e.contains("Open Orders")
                && e.contains("Nuke")),
            "{:?}",
            port.errors()
        );
    })
    .await;
}

fn cancels_seen(port: &FakePort) -> usize {
    port.commands
        .borrow()
        .iter()
        .filter(|command| {
            matches!(
                command,
                Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. }
            )
        })
        .count()
}

fn warnings(port: &FakePort) -> Vec<String> {
    port.emitted
        .borrow()
        .iter()
        .filter(|(op, _)| op == "warning")
        .map(|(_, data)| data.clone())
        .collect()
}

fn completions(port: &FakePort) -> usize {
    port.emitted
        .borrow()
        .iter()
        .filter(|(op, _)| op == "chaser:complete")
        .count()
}

/// Cancels fail with `error` until `open` says the link is back.
fn cancels_fail_until(port: &FakePort, error: VenueError) -> Rc<Cell<bool>> {
    let open = Rc::new(Cell::new(false));
    let flag = open.clone();
    let mut inner = std::mem::replace(
        &mut *port.responder.borrow_mut(),
        Box::new(|_| Ok("[]".into())),
    );
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } if !flag.get() => {
            Err(error.clone())
        }
        other => inner(other),
    });
    open
}

fn stop(factory: &ChaserFactory, cx: &AlgoCx) -> String {
    let id = factory.book().active_chasers()[0].id.clone();
    factory
        .handle("chaser:terminate", &serde_json::to_string(&id).unwrap(), cx)
        .unwrap();
    id
}

fn transport_down() -> VenueError {
    VenueError::new("HTTP transport request failed").kind(ErrorKind::Network)
}

#[tokio::test(start_paused = true)]
async fn a_stop_keeps_cancelling_through_an_outage_past_60_s_and_the_row_stays_until_then() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(90_000).await;
        let rows = factory.book().active_chasers();
        assert_eq!(rows.len(), 1, "the row stays while its order may be live");
        assert!(rows[0].is_stopped);
        assert!(
            rows[0].stop_reason.contains("retrying")
                && rows[0].stop_reason.contains("Open Orders")
                && rows[0].stop_reason.contains("Nuke"),
            "{}",
            rows[0].stop_reason
        );
        assert!(cancels_seen(&port) > 5);
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        assert_eq!(completions(&port), 0);
        assert_eq!(
            warnings(&port).len(),
            1,
            "one retrying state: {:?}",
            warnings(&port)
        );
        assert!(
            warnings(&port)[0].contains("Open Orders") && warnings(&port)[0].contains("Nuke"),
            "{:?}",
            warnings(&port)
        );
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
        assert_eq!(completions(&port), 1);
        assert!(
            warnings(&port)
                .last()
                .unwrap()
                .contains("is cancelled; nothing filled")
        );
        let done = cancels_seen(&port);
        settle(60_000).await;
        assert_eq!(cancels_seen(&port), done);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_unavailable_signing_provider_is_retried_too() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(
            &port,
            VenueError::new("Signing provider is unavailable")
                .kind(ErrorKind::Internal)
                .outcome(Outcome::NotSent),
        );
        stop(&factory, &cx);
        settle(30_000).await;
        assert_eq!(factory.book().active_chasers().len(), 1);
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn traffic_returning_retries_within_about_a_second() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(60_000).await;
        online.set(true);
        port.set_ticker(100.0, 100.5);
        settle(1_100).await;
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_order_that_fills_during_the_retries_ends_them_and_says_filled() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(5_000).await;
        port.update_order("o1", |order| {
            order.filled = 1.0;
            order.status = OrderStatus::Closed;
        });
        settle(11_000).await;
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            warnings(&port).last().unwrap().contains("filled"),
            "{:?}",
            warnings(&port)
        );
        assert_eq!(completions(&port), 1);
        assert!(port.errors().is_empty(), "{:?}", port.errors());
    })
    .await;
}

/// The venue filled the order during the outage; the store has not caught up,
/// and the venue answers the cancel as done (Bybit 110001).
#[tokio::test(start_paused = true)]
async fn a_retried_stop_whose_order_filled_offline_says_filled_once() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(30_000).await;
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
        let warnings = warnings(&port);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[1].contains("o1") && warnings[1].contains("filled 1 before its cancel"),
            "{warnings:?}"
        );
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(complete["filled"], 1.0);
        assert_eq!(complete["outcomeTold"], true);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_retried_stop_that_cancelled_says_so_from_the_fills() {
    for (fills, end) in [
        (Ok("[]"), "is cancelled; nothing filled"),
        (Err("fills unavailable"), "its fills could not be read"),
    ] {
        local(async {
            let port = FakePort::new("bybit");
            accept_with_fills(&port, fills);
            let (factory, cx) = start(&port, BUY_ONE);
            settle(1100).await;
            let online = cancels_fail_until(&port, transport_down());
            stop(&factory, &cx);
            settle(30_000).await;
            online.set(true);
            settle(10_000).await;
            let warnings = warnings(&port);
            assert_eq!(warnings.len(), 2, "{warnings:?}");
            assert!(warnings[1].contains(end), "{warnings:?}");
            let [complete] = port.completes().try_into().unwrap();
            assert_eq!(complete["filled"], 0.0);
            assert_eq!(complete["outcomeTold"], true);
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_retried_stop_the_venue_refuses_because_it_filled_says_filled() {
    local(async {
        let port = FakePort::new("binance");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = Rc::new(Cell::new(false));
        let flag = online.clone();
        let mut inner = std::mem::replace(
            &mut *port.responder.borrow_mut(),
            Box::new(|_| Ok("[]".into())),
        );
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } => {
                Err(if flag.get() {
                    VenueError::new("Unknown order sent.")
                        .kind(ErrorKind::Rejected)
                        .outcome(Outcome::Rejected)
                } else {
                    transport_down()
                })
            }
            other => inner(other),
        });
        stop(&factory, &cx);
        settle(30_000).await;
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        let warnings = warnings(&port);
        assert!(
            warnings
                .last()
                .unwrap()
                .contains("filled 1 before its cancel"),
            "{warnings:?}"
        );
        assert_eq!(port.completes()[0]["outcomeTold"], true);
    })
    .await;
}

#[test]
fn a_retrying_warning_and_the_message_that_ends_it_name_the_same_order() {
    let retry = "Chaser stopped, but BTCUSDT order 71b464ec is still open: Signing provider is unavailable. Retrying the cancel";
    assert_eq!(safe_cex_chaser::retrying_order(retry), Some("71b464ec"));
    assert_eq!(
        safe_cex_chaser::retrying_order("Chaser stopped: BTCUSDT order o1 is now cancelled"),
        None
    );
    for (message, ids) in [
        (
            "Chaser stopped: BTCUSDT order 71b464ec is now cancelled",
            vec!["71b464ec"],
        ),
        (
            "Chaser stopped: BTCUSDT order o1 filled 1 before its cancel got through",
            vec!["o1"],
        ),
        (
            "Chaser could not cancel BTCUSDT order o1, o2: refused; it may still be open",
            vec!["o1", "o2"],
        ),
        (
            "Chaser could not cancel BTCUSDT order o1: refused",
            vec!["o1"],
        ),
        (retry, vec![]),
        ("Chase on BTCUSDT stopped with nothing filled.", vec![]),
    ] {
        assert_eq!(safe_cex_chaser::settled_orders(message), ids, "{message}");
    }
}

/// Hosted, a long drop: something else cancelled the order (the dispose's own
/// try, or the venue) while the cancel was retried, and nothing filled.
#[tokio::test(start_paused = true)]
async fn an_order_found_cancelled_while_its_cancel_retries_says_cancelled_nothing_filled() {
    for found in ["row", "refusal"] {
        local(async {
            let port = FakePort::new("bybit");
            accept_with_fills(&port, Ok("[]"));
            let (factory, cx) = start(&port, BUY_ONE);
            settle(1100).await;
            let online = cancels_fail_until(&port, transport_down());
            stop(&factory, &cx);
            settle(30_000).await;
            if found == "row" {
                port.update_order("o1", |order| order.status = OrderStatus::Canceled);
            } else {
                let mut inner = std::mem::replace(
                    &mut *port.responder.borrow_mut(),
                    Box::new(|_| Ok("[]".into())),
                );
                *port.responder.borrow_mut() = Box::new(move |command| {
                    if is_cancel(command) {
                        Err(VenueError::new("Order does not exist")
                            .kind(ErrorKind::Rejected)
                            .outcome(Outcome::Rejected))
                    } else {
                        inner(command)
                    }
                });
                online.set(true);
                // The reconnect's reconcile shows it gone, and the venue refuses its cancel.
                port.store
                    .borrow_mut()
                    .orders
                    .iter_mut()
                    .for_each(|order| order.status = OrderStatus::Canceled);
            }
            settle(15_000).await;
            let warnings = warnings(&port);
            assert_eq!(warnings.len(), 2, "{found}: {warnings:?}");
            assert_eq!(
                warnings[1], "\"Chaser stopped: BTCUSDT order o1 is cancelled; nothing filled\"",
                "{found}"
            );
            assert!(port.errors().is_empty(), "{found}: {:?}", port.errors());
            let [complete] = port.completes().try_into().unwrap();
            assert_eq!(
                complete["stopReason"], "User cancelled; order o1 is cancelled, nothing filled",
                "{found}"
            );
        })
        .await;
    }
}

#[tokio::test(start_paused = true)]
async fn the_retries_end_at_their_bound_with_a_final_message() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(590_000).await;
        assert!(port.errors().is_empty());
        settle(30_000).await;
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("may still be open"))
        );
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_definite_refusal_after_retries_stays_final() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(5_000).await;
        online.set(true);
        port.refuse_cancels();
        settle(11_000).await;
        let seen = cancels_seen(&port);
        settle(60_000).await;
        assert_eq!(cancels_seen(&port), seen);
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("may still be open"))
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_requote_cancel_lost_to_an_outage_stops_the_chase_and_keeps_cancelling() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        port.set_ticker(101.0, 101.5);
        settle(30_000).await;
        let rows = factory.book().active_chasers();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].is_stopped);
        let placements = port
            .commands
            .borrow()
            .iter()
            .filter(|command| matches!(command, Command::PlaceOrders { .. }))
            .count();
        assert_eq!(placements, 1, "nothing more is placed");
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

/// COSIGNED: the browser that signs went offline, so the requote's cancel could not
/// be sent. The chase says so, in its reason and its warning, and the cancel that
/// gets through once the browser is back ends it.
#[tokio::test(start_paused = true)]
async fn a_requote_with_its_signer_gone_says_the_signer_is_offline() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let back = cancels_fail_until(
            &port,
            VenueError::new("Signing provider is unavailable")
                .kind(ErrorKind::Internal)
                .outcome(Outcome::NotSent),
        );
        port.set_ticker(101.0, 101.5);
        settle(5_000).await;
        let rows = factory.book().active_chasers();
        assert!(
            rows[0]
                .stop_reason
                .starts_with(safe_cex_chaser::SIGNER_OFFLINE_STOP_REASON),
            "{}",
            rows[0].stop_reason
        );
        assert!(
            warnings(&port)[0].contains("the browser that signs for this account is offline"),
            "{:?}",
            warnings(&port)
        );
        back.set(true);
        settle(15_000).await;
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(
            complete["stopReason"],
            format!(
                "{}; order o1 is cancelled, nothing filled",
                safe_cex_chaser::SIGNER_OFFLINE_STOP_REASON
            )
        );
        assert_eq!(port.placements().len(), 1);
    })
    .await;
}

/// The hosted sign fails only after its 15 s: the chaser must wait to hear that.
#[tokio::test(start_paused = true)]
async fn a_requote_cancel_waiting_out_the_sign_timeout_names_the_offline_signer() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let back = cancels_fail_until(
            &port,
            VenueError::new("Signing provider is unavailable")
                .kind(ErrorKind::Internal)
                .outcome(Outcome::NotSent),
        );
        port.cancel_answer_ms.set(15_000);
        port.set_ticker(101.0, 101.5);
        // The requote's own cancel, then the stop's first attempt.
        settle(35_000).await;
        let rows = factory.book().active_chasers();
        assert!(
            rows[0]
                .stop_reason
                .starts_with(safe_cex_chaser::SIGNER_OFFLINE_STOP_REASON),
            "{}",
            rows[0].stop_reason
        );
        assert!(
            warnings(&port)[0].contains("the browser that signs for this account is offline"),
            "{:?}",
            warnings(&port)
        );
        // One holder: no cancel goes through and nothing ends until it returns.
        assert_eq!(rows.len(), 1);
        assert_eq!(completions(&port), 0);
        port.cancel_answer_ms.set(0);
        back.set(true);
        settle(30_000).await;
        assert_eq!(completions(&port), 1);
        assert!(
            warnings(&port)
                .last()
                .unwrap()
                .contains("is cancelled; nothing filled"),
            "{:?}",
            warnings(&port)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_during_the_first_placement_cancels_what_lands_before_completing() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        port.place_answer_ms.set(2_000);
        let (factory, cx) = start(&port, BUY_ONE);
        settle(500).await;
        stop(&factory, &cx);
        settle(100).await;
        assert_eq!(completions(&port), 0, "its order is still on the wire");
        settle(2_000).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        assert_eq!(completions(&port), 1);
        assert!(factory.book().active_chasers().is_empty());
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_during_a_requote_placement_cancels_what_lands() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.place_answer_ms.set(2_000);
        port.set_ticker(101.0, 101.5);
        settle(500).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        stop(&factory, &cx);
        settle(2_000).await;
        assert_eq!(port.cancels(), vec!["o1".to_string(), "o2".to_string()]);
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn nuke_and_dispose_wait_for_a_placement_on_the_wire_and_cancel_it() {
    local(async {
        for nuke in [true, false] {
            let port = FakePort::new("bybit");
            port.accept_placements();
            port.place_answer_ms.set(2_000);
            let (factory, _cx) = start(&port, BUY_ONE);
            settle(500).await;
            if nuke {
                factory.book().stop_for_nuke("nuked").await;
            } else {
                factory.book().shutdown("disposed").await;
            }
            assert_eq!(port.placements().len(), 1);
            assert!(
                port.cancels().contains(&"o1".to_string()),
                "landed and cancelled before the sweep went on"
            );
            assert_eq!(completions(&port), 1);
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_slow_replacement_row_does_not_read_as_the_order_leaving_the_book() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |_| {});
        settle(10).await;
        port.rows_lag.set(true);
        port.set_ticker(101.0, 101.5);
        settle(1200).await;
        assert_eq!(port.placements().len(), 2);
        for _ in 0..3 {
            port.publish(Change {
                orders: true,
                ..Change::default()
            });
            settle(10).await;
        }
        assert!(!factory.book().active_chasers()[0].is_stopped);
        assert_eq!(completions(&port), 0);
    })
    .await;
}

/// As these venues answer: a read with no symbol is empty, and a filled order's
/// cancel reads as done with its row Canceled (-2011).
fn binance_filled_unseen(port: &Rc<FakePort>, fills: Result<&'static str, &'static str>) {
    port.accept_placements();
    let accept = RefCell::new(std::mem::replace(
        &mut *port.responder.borrow_mut(),
        Box::new(|_| Ok("null".into())),
    ));
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::Query {
            query: safe_cex_engine::engine::Query::Fills { symbol, .. },
            ..
        } => match symbol {
            Some(_) => fills.map(Into::into).map_err(VenueError::new),
            None => Ok("[]".into()),
        },
        other => (accept.borrow_mut())(other),
    });
}

#[tokio::test(start_paused = true)]
async fn a_binance_order_filled_unseen_is_not_placed_again() {
    local(async {
        for venue in ["binance", "asterdex"] {
            let port = FakePort::new(venue);
            binance_filled_unseen(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
            let (factory, _cx) = start(&port, BUY_ONE);
            settle(1100).await;
            port.set_ticker(101.0, 101.5);
            settle(200).await;
            assert_eq!(port.placements().len(), 1, "{venue}");
            assert!(factory.book().active_chasers().is_empty(), "{venue}");
            assert_eq!(port.completes()[0]["filled"], 1.0, "{venue}");
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_binance_requote_whose_fills_cannot_be_read_places_nothing() {
    local(async {
        let port = FakePort::new("binance");
        binance_filled_unseen(&port, Err("read failed"));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(200).await;
        assert_eq!(port.placements().len(), 1);
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("could not be read"))
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_fills_read_names_the_symbol_and_starts_near_the_chase() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.cancels_find_nothing.set(true);
        port.set_ticker(101.0, 101.5);
        settle(1_500).await;
        let commands = port.commands.borrow();
        let (symbol, from_ms) = commands
            .iter()
            .find_map(|command| match command {
                Command::Query {
                    query:
                        safe_cex_engine::engine::Query::Fills {
                            symbol, from_ms, ..
                        },
                    ..
                } => Some((symbol.clone(), *from_ms)),
                _ => None,
            })
            .expect("a fills read");
        assert_eq!(symbol.as_deref(), Some("BTCUSDT"));
        assert!(from_ms.is_some_and(|from| from > 0.0), "{from_ms:?}");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_row_trailing_its_cancel_answer_does_not_end_the_chase_on_an_unreadable_venue() {
    local(async {
        let port = FakePort::new("polymarket");
        accept_with_fills(&port, Err("Polymarket historical fills are unsupported"));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.cancels_find_nothing.set(true);
        port.set_ticker(101.0, 101.5);
        settle(300).await;
        port.update_order("o1", |row| row.status = OrderStatus::Canceled);
        port.cancels_find_nothing.set(false);
        settle(500).await;
        assert_eq!(port.placements().len(), 2, "{:?}", port.errors());
        assert!(!factory.book().active_chasers()[0].is_stopped);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn lot_venues_compare_fills_in_lots() {
    local(async {
        for venue in ["kucoin", "mexc", "blofin", "okx", "bitrue"] {
            let port = FakePort::new(venue);
            port.store
                .borrow_mut()
                .markets
                .get_mut("BTCUSDT")
                .unwrap()
                .contract_size = Some(0.001);
            // 0.0004 BTC of a 1 lot order: not filled, and not the whole of it.
            accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":0.0004}]"#));
            let _chase = start(&port, BUY_ONE);
            settle(1100).await;
            port.cancels_find_nothing.set(true);
            port.set_ticker(101.0, 101.5);
            settle(1_500).await;
            let placements = port.placements();
            assert_eq!(placements.len(), 2, "{:?}", port.errors());
            assert!((placements[1].1 - 0.6).abs() < 1e-9, "{placements:?}");
        }
    })
    .await;
}

/// BloFin and Bitget close a filled row by status alone: its `filled` stays 0.
#[tokio::test(start_paused = true)]
async fn a_row_closed_by_status_alone_counts_as_filled() {
    local(async {
        let port = FakePort::new("blofin");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |row| row.status = OrderStatus::Closed);
        settle(100).await;
        assert!(factory.book().active_chasers().is_empty());
        assert_eq!(port.completes()[0]["filled"], 1.0);
        assert_eq!(port.completes()[0]["stopReason"], "Completed");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_racing_a_status_only_close_still_reports_the_fill() {
    local(async {
        let port = FakePort::new("bitget");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        stop(&factory, &cx);
        port.update_order("o1", |row| row.status = OrderStatus::Closed);
        settle(100).await;
        assert_eq!(port.completes()[0]["filled"], 1.0);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn failing_to_place_says_what_the_venue_answered() {
    local(async {
        let port = FakePort::new("blofin");
        *port.responder.borrow_mut() =
            Box::new(|_| Err(VenueError::new("post only would cross").outcome(Outcome::Rejected)));
        let _chase = start(&port, BUY_ONE);
        settle(5_000).await;
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("after 10 tries: post only would cross")),
            "{:?}",
            port.errors()
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_cancel_answered_not_exist_does_not_say_may_still_be_open() {
    local(async {
        let port = FakePort::new("bitunix");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let _closed = cancels_fail_until(
            &port,
            VenueError::new("Order not exist").outcome(Outcome::Rejected),
        );
        stop(&factory, &cx);
        settle(100).await;
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_fill_during_the_final_cancel_is_reported_from_the_venue() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(
            &port,
            Ok(r#"[{"orderId":"o1","qty":0.4},{"orderId":"other","qty":9}]"#),
        );
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        stop(&factory, &cx);
        settle(100).await;
        assert_eq!(port.completes()[0]["filled"], 0.4);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_vanished_row_reports_its_fill_without_a_client_id() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let _chase = start(
            &port,
            r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0}"#,
        );
        settle(1100).await;
        port.update_order("o1", |_| {});
        settle(10).await;
        port.remove_orders();
        settle(10).await;
        port.remove_orders();
        settle(100).await;
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(complete["filled"], 1.0);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_final_fill_falls_back_to_the_position() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Err("read failed"));
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.store.borrow_mut().positions.push(long(0.3));
        stop(&factory, &cx);
        settle(100).await;
        assert_eq!(port.completes()[0]["filled"], 0.3);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_late_fill_on_a_replaced_order_comes_off_the_remainder() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(1200).await;
        port.update_order("o1", |row| row.filled = 0.4);
        settle(60_000).await;
        port.set_ticker(102.0, 102.5);
        settle(1200).await;
        let placements = port.placements();
        assert_eq!(placements.len(), 3);
        assert!((placements[2].1 - 0.6).abs() < 1e-9, "{placements:?}");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_partial_refusal_of_a_split_order_cancels_the_parts_that_landed() {
    local(async {
        let port = FakePort::new("bybit");
        *port.responder.borrow_mut() = Box::new(|command| match command {
            Command::PlaceOrders { .. } => {
                let mut error = VenueError::new("part refused").outcome(Outcome::Rejected);
                error.partial = Some(Box::new(safe_cex_engine::error::Partial {
                    order_ids: vec!["part1".into()],
                    total: Some(2),
                    failed_index: Some(1),
                }));
                Err(error)
            }
            _ => Ok("null".into()),
        });
        let (factory, cx) = start(&port, BUY_ONE);
        settle(100).await;
        assert_eq!(port.placements().len(), 1, "not placed again");
        stop(&factory, &cx);
        settle(100).await;
        assert_eq!(port.cancels(), vec!["part1".to_string()]);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_order_cancelled_from_outside_stops_the_chase_and_says_so() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(100.0, 100.5);
        settle(100).await;
        port.update_order("o1", |row| row.status = OrderStatus::Canceled);
        settle(100).await;
        assert_eq!(port.placements().len(), 1, "{:?}", port.placements());
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            warnings(&port)
                .iter()
                .any(|w| w.contains("cancelled outside the chase")),
            "{:?}",
            warnings(&port)
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn hyperliquid_counts_a_row_fill_and_its_fill_event_once() {
    local(async {
        let port = FakePort::new("hyperliquid");
        accept_with_fills(&port, Ok("[]"));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.update_order("o1", |row| row.filled = 0.4);
        settle(10).await;
        port.publish(Change {
            fills: vec![safe_cex_engine::engine::Fill {
                id: None,
                order_id: "o1".into(),
                symbol: "BTCUSDT".into(),
                side: OrderSide::Buy,
                price: 100.0,
                amount: 0.4,
            }],
            ..Change::default()
        });
        settle(10).await;
        assert_eq!(
            factory
                .book()
                .progress(&factory.book().active_chasers()[0].id)
                .unwrap()
                .remaining_size,
            0.6
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_with_no_store_row_cancels_by_symbol_on_a_by_id_venue() {
    local(async {
        let port = FakePort::new("bybit");
        port.without_symbol_cancel();
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.store.borrow_mut().orders.clear();
        stop(&factory, &cx);
        settle(100).await;
        let commands = port.commands.borrow();
        assert!(
            commands.iter().any(|c| matches!(
                c,
                Command::CancelOrdersWithSymbols { orders, .. }
                    if orders[0].id == "o1" && orders[0].symbol == "BTCUSDT"
            )),
            "{commands:?}"
        );
    })
    .await;
}

/// The stop reason stays true when a later attempt's cancel goes through.
#[tokio::test(start_paused = true)]
async fn a_requote_cancel_that_later_succeeds_says_an_attempt_failed() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok("[]"));
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let back = cancels_fail_until(&port, transport_down());
        port.set_ticker(101.0, 101.5);
        settle(5_000).await;
        back.set(true);
        settle(15_000).await;
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(
            complete["stopReason"],
            "Stopped: a cancel attempt failed: HTTP transport request failed; order o1 is cancelled, nothing filled"
        );
    })
    .await;
}

/// No STOP: a requote's cancel lost to the outage stops the chase, and the
/// order fills offline before the venue answers the cancel as done.
#[tokio::test(start_paused = true)]
async fn a_requote_cancel_retried_through_an_outage_ends_with_one_final_from_the_fills() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        port.set_ticker(101.0, 101.5);
        settle(30_000).await;
        assert_eq!(warnings(&port).len(), 1, "{:?}", warnings(&port));
        online.set(true);
        settle(10_000).await;
        assert!(factory.book().active_chasers().is_empty());
        let warnings = warnings(&port);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[1].contains("o1") && warnings[1].contains("filled 1 before its cancel"),
            "{warnings:?}"
        );
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(complete["filled"], 1.0);
        assert_eq!(complete["outcomeTold"], true);
    })
    .await;
}

fn is_cancel(command: &Command) -> bool {
    matches!(
        command,
        Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. }
    )
}

/// The live LOCAL run: the cancel sent as the link came back went into a dead
/// connection and never answered; the order had filled during the drop.
#[tokio::test(start_paused = true)]
async fn a_cancel_lost_in_a_dead_link_is_tried_again_and_ends_with_the_fill() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        port.set_ticker(101.0, 101.5);
        settle(30_000).await;
        // The first cancel after the link returns goes into the dead connection.
        let first = Rc::new(Cell::new(true));
        let link = first.clone();
        *port.unanswered.borrow_mut() = Some(Box::new(move |command| {
            is_cancel(command) && link.replace(false)
        }));
        online.set(true);
        settle(40_000).await;
        assert!(!first.get(), "a cancel went into the dead link");
        assert!(factory.book().active_chasers().is_empty());
        let warnings = warnings(&port);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[1].contains("filled 1 before its cancel"),
            "{warnings:?}"
        );
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_fills_read_that_never_answers_still_ends_with_one_final() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let online = cancels_fail_until(&port, transport_down());
        *port.unanswered.borrow_mut() = Some(Box::new(|command| {
            matches!(
                command,
                Command::Query {
                    query: safe_cex_engine::engine::Query::Fills { .. },
                    ..
                }
            )
        }));
        stop(&factory, &cx);
        settle(30_000).await;
        online.set(true);
        settle(40_000).await;
        assert!(factory.book().active_chasers().is_empty());
        let warnings = warnings(&port);
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(
            warnings[1].contains("its fills could not be read"),
            "{warnings:?}"
        );
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn shutdown_during_the_retries_ends_them_with_one_try_and_a_final_message() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        cancels_fail_until(&port, transport_down());
        stop(&factory, &cx);
        settle(5_000).await;
        factory.book().shutdown("account disposed").await;
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("may still be open"))
        );
        assert_eq!(completions(&port), 1);
        let seen = cancels_seen(&port);
        settle(60_000).await;
        assert!(cancels_seen(&port) <= seen + 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_order_filled_before_the_drop_is_neither_cancelled_nor_reported_open() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.refuse_cancels();
        port.update_order("o1", |order| order.status = OrderStatus::Closed);
        settle(10).await;
        assert!(port.cancels().is_empty(), "{:?}", port.cancels());
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        assert_eq!(port.completes().len(), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_order_that_fills_while_its_cancel_waits_is_not_reported_open() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        // As in a drop: the sign times out, and the fill lands on the venue meanwhile.
        let weak = Rc::downgrade(&port);
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } => {
                weak.upgrade().unwrap().update_order("o1", |order| {
                    order.status = OrderStatus::Closed;
                });
                Err(VenueError::new("cancel refused").outcome(Outcome::NotSent))
            }
            _ => Ok("null".into()),
        });
        port.set_ticker(101.0, 101.5);
        settle(2000).await;
        assert_eq!(port.cancels(), ["o1"], "the one that raced, none after");
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        assert_eq!(port.placements().len(), 1, "no replacement");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_stop_whose_cancel_the_venue_refused_because_it_filled_says_filled() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (factory, cx) = start(&port, BUY_ONE);
        settle(1100).await;
        // The venue filled it first: its refusal arrives with the fill.
        let weak = Rc::downgrade(&port);
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::CancelOrders { .. } | Command::CancelOrdersWithSymbols { .. } => {
                weak.upgrade().unwrap().update_order("o1", |order| {
                    order.status = OrderStatus::Closed;
                    order.filled = order.amount;
                    order.remaining = 0.0;
                });
                Err(VenueError::new("order not exists or too late to cancel"))
            }
            _ => Ok("null".into()),
        });
        stop(&factory, &cx);
        settle(100).await;
        assert!(
            warnings(&port).iter().any(|w| w.contains("o1")
                && w.contains("filled")
                && w.contains("before its cancel")),
            "{:?}",
            warnings(&port)
        );
        assert!(port.errors().is_empty(), "{:?}", port.errors());
        assert_eq!(completions(&port), 1);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn an_order_still_open_when_its_cancel_is_refused_is_reported() {
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.refuse_cancels();
        port.set_ticker(101.0, 101.5);
        settle(2000).await;
        let errors = port.errors();
        assert!(
            errors.iter().any(|e| e.contains("failed to cancel")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("o1")
                && e.contains("may still be open")
                && e.contains("Open Orders")
                && e.contains("Nuke")),
            "{errors:?}"
        );
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
async fn keep_order_retains_legacy_cancellation_of_late_placement() {
    // TS c78bed93d ChaserOrderManager.ts:307-322 cancels late IDs even after removeKeepOrder.
    local(async {
        let port = FakePort::new("bybit");
        port.accept_placements();
        port.place_answer_ms.set(2_000);
        let (factory, _) = start(&port, BUY_ONE);
        settle(10).await;
        let book = factory.book();
        let id = book.active_chasers()[0].id.clone();
        book.remove_keep_order(&id);
        settle(2_500).await;
        assert_eq!(port.placements().len(), 1);
        assert_eq!(port.cancels(), ["o1"]);
        assert_eq!(port.completes().len(), 1);
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

/// Lighter: the placement answers with the client id; the row soon has the venue's.
#[tokio::test(start_paused = true)]
async fn a_chase_follows_its_order_to_the_venue_id_a_snapshot_shows() {
    local(async {
        let port = FakePort::new("lighter");
        port.accept_placements();
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        let mut native = order("n1", 100.0, 1.0, 0.0, OrderStatus::Open);
        native.client_order_id = Some("o1".into());
        for _ in 0..2 {
            {
                let mut store = port.store.borrow_mut();
                store.orders = vec![native.clone()];
                store.update_ids.orders += 1;
            }
            port.publish(Change {
                orders: true,
                ..Change::default()
            });
            settle(10).await;
        }
        settle(2_000).await;
        assert!(port.completes().is_empty(), "{:?}", port.completes());
        assert!(port.cancels().is_empty());
        let id = factory.book().active_chasers()[0].id.clone();
        factory.book().remove(&id);
        settle(100).await;
        assert_eq!(port.cancels(), vec!["n1".to_string()]);
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

fn long(contracts: f64) -> Position {
    Position {
        id: "p".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 100.0,
        notional: 100.0 * contracts,
        leverage: 1.0,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }
}

/// The LOCAL drop: o1 filled at the venue while the socket was down, so no fill
/// event came. The reconnect's REST reconcile shows the position but the order
/// row stays as it was, and Bybit answers a cancel of the filled order as done.
fn filled_during_the_drop(port: &Rc<FakePort>) {
    port.cancels_find_nothing.set(true);
    port.store.borrow_mut().positions.push(long(1.0));
    port.publish(Change {
        positions: true,
        ..Change::default()
    });
}

#[tokio::test(start_paused = true)]
async fn a_chase_whose_order_filled_during_a_drop_never_places_again() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        filled_during_the_drop(&port);
        settle(1000).await;
        // The market moves away: a requote would cancel o1 and place again.
        port.set_ticker(101.0, 101.5);
        settle(5_000).await;
        assert_eq!(
            port.placements().len(),
            1,
            "a second order would double the position"
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_requote_whose_cancelled_order_cannot_be_confirmed_places_nothing() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Err("fills unavailable"));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        // The venue answers the cancel as done, and the row does not say why.
        port.cancels_find_nothing.set(true);
        port.set_ticker(101.0, 101.5);
        settle(5_000).await;
        assert_eq!(
            port.placements().len(),
            1,
            "no order while the last is unconfirmed"
        );
        assert!(factory.book().active_chasers().is_empty());
        assert!(
            port.errors()
                .iter()
                .any(|e| e.contains("stopped before replacing order o1")
                    && e.contains("Check Positions")),
            "{:?}",
            port.errors()
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_chase_whose_order_filled_during_a_drop_ends_with_the_fill() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":1}]"#));
        let (factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        filled_during_the_drop(&port);
        settle(15_000).await;
        assert!(
            factory.book().active_chasers().is_empty(),
            "the chase is over"
        );
        assert_eq!(port.placements().len(), 1);
        let [complete] = port.completes().try_into().unwrap();
        assert_eq!(complete["filled"], 1.0);
        assert!(
            warnings(&port)
                .iter()
                .any(|w| w.contains("o1") && w.contains("filled 1")),
            "{:?}",
            warnings(&port)
        );
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
async fn unknown_notional_does_not_hide_a_confirmed_flat_reduce_position() {
    local(async {
        let port = FakePort::new("kraken");
        port.accept_placements();
        port.store.borrow_mut().positions.push(Position {
            id: "p".into(), symbol: "BTCUSDT".into(), side: PositionSide::Short,
            entry_price: 100.0, notional: f64::NAN, leverage: 1.0,
            margin_type: None, position_mode: None, unrealized_pnl: 0.0,
            realized_pnl: None, contracts: 1.0, liquidation_price: f64::NAN,
            margin_size: None, last_update: None,
        });
        let (factory, _cx) = start(&port, r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0,"reduceOnly":true}"#);
        settle(1100).await;
        assert!(!factory.book().active_chasers().is_empty());
        port.store.borrow_mut().positions[0].contracts = 0.0;
        port.update_order("o1", |_| {});
        settle(100).await;
        assert_eq!(port.completes().len(), 1);
        assert_eq!(port.completes()[0]["stopReason"], "Completed");
    }).await;
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
        let chase_id = factory.book().active_chasers()[0].id.clone();
        assert!(
            !factory
                .book()
                .progress(&chase_id)
                .unwrap()
                .order_may_be_open
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_lost_placement_never_found_stops_after_the_lookups() {
    local(async {
        let port = FakePort::new("hyperliquid");
        lose_placements(&port, r#"{"found":false,"definitive":false}"#);
        let (factory, _) = start(&port, BUY_ONE);
        settle(10).await;
        let id = factory.book().active_chasers()[0].id.clone();
        let progress = factory.book().progress(&id).unwrap();
        assert!(progress.order_may_be_open);
        assert!(!progress.ended);
        settle(20_000).await;
        assert_eq!(port.placements().len(), 1);
        assert_eq!(lookups(&port), 4);
        assert_eq!(
            port.completes()[0]["stopReason"],
            UNCONFIRMED_ORDER_STOP_REASON
        );
        let progress = factory.book().progress(&id).unwrap();
        assert!(progress.order_may_be_open && progress.ended);
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

#[tokio::test(start_paused = true)]
async fn a_hyperliquid_fill_event_lowers_the_remainder_before_replacement() {
    local(async {
        let port = FakePort::new("hyperliquid");
        port.accept_placements();
        let accept = std::mem::replace(
            &mut *port.responder.borrow_mut(),
            Box::new(|_| Ok("null".into())),
        );
        let accept = RefCell::new(accept);
        // REST knows nothing yet: it never lowers what the fill event counted.
        *port.responder.borrow_mut() = Box::new(move |command| match command {
            Command::Query { .. } => Ok("[]".into()),
            other => (accept.borrow_mut())(other),
        });
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        let fill = Fill {
            id: None,
            order_id: "o1".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Buy,
            price: 100.0,
            amount: 0.4,
        };
        port.publish(Change {
            fills: vec![fill],
            ..Change::default()
        });
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        assert_eq!(port.cancels(), vec!["o1".to_string()]);
        let placements = port.placements();
        assert_eq!(placements.len(), 2);
        assert_eq!(placements[1].0, 101.0);
        assert!((placements[1].1 - 0.6).abs() < 1e-9, "{placements:?}");
    })
    .await;
}

/// Placements land; fill lookups by order id answer with `fills`.
/// Fills reads answer `failures` times with `kind`, then with no fills.
fn fills_fail(port: &Rc<FakePort>, kind: ErrorKind, failures: u32) -> Rc<Cell<u32>> {
    port.accept_placements();
    let accept = RefCell::new(std::mem::replace(
        &mut *port.responder.borrow_mut(),
        Box::new(|_| Ok("null".into())),
    ));
    let reads = Rc::new(Cell::new(0));
    let seen = reads.clone();
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::Query {
            query: safe_cex_engine::engine::Query::Fills { .. },
            ..
        } => {
            seen.set(seen.get() + 1);
            if seen.get() <= failures {
                Err(VenueError::new("too many requests").kind(kind))
            } else {
                Ok("[]".into())
            }
        }
        other => (accept.borrow_mut())(other),
    });
    reads
}

#[tokio::test(start_paused = true)]
async fn a_rate_limited_fills_read_after_a_cancel_is_tried_again_and_the_chase_goes_on() {
    local(async {
        // Fills tracked, and a cancel answer that hides fills: the two paths that read.
        for venue in ["hyperliquid", "binance"] {
            let port = FakePort::new(venue);
            let reads = fills_fail(&port, ErrorKind::RateLimited, 2);
            let (_factory, _cx) = start(&port, BUY_ONE);
            settle(1100).await;
            port.set_ticker(101.0, 101.5);
            settle(10_000).await;
            assert!(reads.get() >= 3, "{venue}");
            assert_eq!(port.placements().len(), 2, "{venue} replaces its order");
            assert!(port.completes().is_empty(), "{venue}");
        }
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_fills_read_failing_past_its_bound_stops_and_says_why() {
    local(async {
        let port = FakePort::new("hyperliquid");
        let reads = fills_fail(&port, ErrorKind::Timeout, u32::MAX);
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(10_000).await;
        assert!(port.completes().is_empty(), "still inside the bound");
        // The stop's own final read has the same bound before the completion.
        settle(30_000).await;
        assert!(reads.get() > 1);
        assert_eq!(port.placements().len(), 1);
        assert_eq!(
            port.completes()[0]["stopReason"],
            "Failed to reconcile fills after cancellation"
        );
        assert!(
            port.errors()
                .iter()
                .any(|text| text.contains("too many requests"))
        );
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_fills_read_stops_at_once() {
    local(async {
        let port = FakePort::new("hyperliquid");
        let reads = fills_fail(&port, ErrorKind::Rejected, u32::MAX);
        // The cancel succeeded with the row still open, as the stream lags.
        port.cancels_find_nothing.set(true);
        let (_factory, _cx) = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        // The requote's read and the completion's final read, one try each.
        assert_eq!(reads.get(), 2);
        assert_eq!(port.placements().len(), 1);
        assert_eq!(port.completes().len(), 1);
        assert_eq!(cancels_seen(&port), 1, "its cancel already succeeded");
    })
    .await;
}

fn accept_with_fills(port: &Rc<FakePort>, fills: Result<&'static str, &'static str>) {
    port.accept_placements();
    let accept = std::mem::replace(
        &mut *port.responder.borrow_mut(),
        Box::new(|_| Ok("null".into())),
    );
    let accept = RefCell::new(accept);
    *port.responder.borrow_mut() = Box::new(move |command| match command {
        Command::Query {
            query: safe_cex_engine::engine::Query::Fills { .. },
            ..
        } => fills.map(Into::into).map_err(VenueError::new),
        other => (accept.borrow_mut())(other),
    });
}

#[tokio::test(start_paused = true)]
async fn a_rejoin_reconciles_lost_fills_before_replacing() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":0.4}]"#));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.publish(Change {
            resync: true,
            orders: true,
            positions: true,
            ..Change::default()
        });
        settle(10).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        let placements = port.placements();
        assert_eq!(placements.len(), 2);
        assert!((placements[1].1 - 0.6).abs() < 1e-9, "{placements:?}");
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn without_a_rejoin_a_row_venue_does_not_ask_rest() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Ok(r#"[{"orderId":"o1","qty":0.4}]"#));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        assert_eq!(port.placements()[1].1, 1.0);
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_rejoin_on_a_venue_without_fill_lookup_falls_back_to_rows() {
    local(async {
        let port = FakePort::new("bybit");
        accept_with_fills(&port, Err("fills by order id are unsupported"));
        let _chase = start(&port, BUY_ONE);
        settle(1100).await;
        port.publish(Change {
            resync: true,
            orders: true,
            ..Change::default()
        });
        settle(10).await;
        port.set_ticker(101.0, 101.5);
        settle(100).await;
        assert_eq!(port.placements().len(), 2, "the chase keeps running");
        assert!(port.completes().is_empty());
    })
    .await;
}
