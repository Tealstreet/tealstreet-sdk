//! Algos on a real engine over a test venue: placements complete at once,
//! and a `fill <order id>` socket message fills the order.
use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{
    Limits, Market, MinMax, Order, OrderStatus, OrderType, Precision, Ticker,
};
use safe_cex_engine::{Cx, Engine, Response, SocketId, TimerId, Venue};
use std::collections::HashMap;

const WALL: f64 = 1_700_000_000_000.0;

struct TestVenue {
    commands: Rc<RefCell<Vec<Command>>>,
    next_order: u32,
    /// What a REST fills query reports per order id.
    rest_fills: RestFills,
}

impl Venue for TestVenue {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        let Some(id) = text.strip_prefix("fill ") else {
            return;
        };
        let Some(mut order) = cx.store.orders().iter().find(|o| o.id == id).cloned() else {
            return;
        };
        cx.fill(Fill {
            id: None,
            order_id: order.id.clone(),
            symbol: order.symbol.clone(),
            side: order.side,
            price: order.price,
            amount: order.remaining,
        });
        order.filled = order.amount;
        order.remaining = 0.0;
        order.status = OrderStatus::Closed;
        cx.store.upsert_order(order);
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        match &command {
            Command::PlaceOrders {
                command: id,
                orders,
            } => {
                let mut ids = Vec::new();
                for placed in orders {
                    self.next_order += 1;
                    let order_id = format!("o{}", self.next_order);
                    cx.store.upsert_order(Order {
                        id: order_id.clone(),
                        parent_id: None,
                        status: OrderStatus::Open,
                        symbol: placed.symbol.clone(),
                        kind: OrderType::Limit,
                        side: placed.side,
                        price: placed.price.unwrap_or(100.0),
                        amount: placed.amount,
                        filled: 0.0,
                        remaining: placed.amount,
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
                    ids.push(order_id);
                }
                cx.done(*id, serde_json::to_string(&ids).unwrap());
            }
            Command::CancelOrders { command: id, ids } => {
                for order_id in ids {
                    cx.store.set_order_status(order_id, OrderStatus::Canceled);
                }
                cx.done(*id, "[]".into());
            }
            Command::Query {
                command: id,
                query:
                    safe_cex_engine::engine::Query::Fills {
                        order_id: Some(order_id),
                        ..
                    },
            } => {
                let rows = self
                    .rest_fills
                    .borrow()
                    .get(order_id)
                    .map_or_else(Vec::new, |qty| {
                        vec![serde_json::json!({ "orderId": order_id, "qty": qty })]
                    });
                cx.done(*id, Value::from(rows).to_string());
            }
            other => {
                if let Some(id) = other.completion_id() {
                    cx.done(id, "null".into());
                }
            }
        }
        self.commands.borrow_mut().push(command);
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
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
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}

fn engine() -> (WasmEngine, Rc<RefCell<Vec<Command>>>) {
    let (engine, commands, _) = engine_on("testvenue");
    (engine, commands)
}

type RestFills = Rc<RefCell<HashMap<String, f64>>>;

/// A test engine whose algo store names `venue`, and the REST fills it reports.
fn engine_on(venue_name: &str) -> (WasmEngine, Rc<RefCell<Vec<Command>>>, RestFills) {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let rest_fills = Rc::new(RefCell::new(HashMap::new()));
    let venue = TestVenue {
        commands: commands.clone(),
        next_order: 0,
        rest_fills: rest_fills.clone(),
    };
    let Ok(mut engine) = Engine::new(venue, Some(crate::production_entropy().unwrap())) else {
        panic!("test engine must construct");
    };
    engine.store_mut().set_markets(vec![market()]);
    engine.store_mut().set_tickers(vec![Ticker {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        bid: 100.0,
        ask: 100.5,
        last: 100.25,
        ..Ticker::default()
    }]);
    (
        WasmEngine::for_algo_tests(venue_name, Box::new(engine)),
        commands,
        rest_fills,
    )
}

fn emits(engine: &mut WasmEngine) -> Vec<Value> {
    serde_json::from_str::<Vec<Value>>(&engine.algo_emits()).unwrap()
}

fn placements(commands: &RefCell<Vec<Command>>) -> Vec<CommandId> {
    commands
        .borrow()
        .iter()
        .filter_map(|command| match command {
            Command::PlaceOrders { command, .. } => Some(*command),
            _ => None,
        })
        .collect()
}

const BUY_ONE: &str = r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0}"#;

#[test]
fn a_chase_places_through_the_engine_and_completes_on_its_fill() {
    let (mut engine, commands) = engine();
    engine.algo_start("acct", 0.0, WALL).unwrap();
    let rows = emits(&mut engine);
    assert_eq!(rows[0]["op"], "store");
    assert_eq!(rows[0]["data"]["activeChasers"], serde_json::json!([]));

    let actions = engine
        .algo_op("chaser:start", BUY_ONE, 10.0, WALL + 10.0)
        .unwrap();
    assert!(
        !actions.contains("\"done\""),
        "algo completions never reach the JS host: {actions}"
    );
    let placed = placements(&commands);
    assert_eq!(placed.len(), 1);
    assert!(
        placed[0] >= FIRST_COMMAND_ID,
        "id {} in the algo range",
        placed[0]
    );
    assert!(
        engine
            .algos
            .running
            .as_ref()
            .unwrap()
            .port
            .pending
            .borrow()
            .is_empty(),
        "the done notice settled the placement"
    );
    let started = emits(&mut engine);
    assert!(
        started.iter().any(|emit| emit["op"] == "log"
            && emit["data"]
                .as_str()
                .is_some_and(|line| line.contains("processInstance starting"))),
        "{started:?}"
    );
    let row = started
        .iter()
        .rev()
        .find(|emit| emit["op"] == "store")
        .unwrap();
    assert_eq!(row["data"]["activeChasers"][0]["symbol"], "BTCUSDT");
    // The chase settles after placing: its timer is the next deadline.
    assert_eq!(engine.algo_deadline(), Some(1010.0));

    engine.algo_advance(1100.0, WALL + 1100.0);
    engine.ws_message(1.0, "fill o1", 1200.0).unwrap();
    let frame = engine
        .take_frame_with(1200.0, false, <[u8]>::to_vec)
        .unwrap();
    assert_eq!(safe_cex_engine::wire::fills(&frame).unwrap().len(), 1);
    assert_eq!(
        engine.algo_deadline(),
        Some(1100.0),
        "the frame woke the chase; work is due now"
    );
    engine.algo_advance(1200.0, WALL + 1200.0);
    let done = emits(&mut engine);
    let complete = done
        .iter()
        .find(|emit| emit["op"] == "chaser:complete")
        .unwrap_or_else(|| panic!("no chaser:complete in {done:?}"));
    assert_eq!(complete["data"]["symbol"], "BTCUSDT");
    let row = done
        .iter()
        .rev()
        .find(|emit| emit["op"] == "store")
        .unwrap();
    assert_eq!(row["data"]["activeChasers"], serde_json::json!([]));
}

#[test]
fn a_twap_slice_fires_as_the_host_advances() {
    let (mut engine, commands) = engine();
    engine.algo_start("acct", 0.0, WALL).unwrap();
    let options = serde_json::json!({
        "accountName": "acct", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1, "duration": 1,
        "ordersCount": 2, "side": "buy", "chaserDelay": 1000, "reduceOnly": false,
        "randomnessPercentage": 0, "useLimitOrders": false, "useChaserOrders": false,
    });
    engine
        .algo_op("twap:start", &options.to_string(), 0.0, WALL)
        .unwrap();
    assert_eq!(placements(&commands).len(), 1, "the first slice at once");
    engine.algo_advance(29_000.0, WALL + 29_000.0);
    assert_eq!(placements(&commands).len(), 1);
    engine.algo_advance(31_000.0, WALL + 31_000.0);
    let placed = placements(&commands);
    assert_eq!(placed.len(), 2, "the second slice 30 s later");
    assert!(placed.iter().all(|id| *id >= FIRST_COMMAND_ID));
    assert_ne!(placed[0], placed[1]);
}

#[test]
fn dispose_cancels_the_resting_chase_then_aborts_every_task() {
    let (mut engine, commands) = engine();
    engine.algo_start("acct", 0.0, WALL).unwrap();
    engine.algo_op("chaser:start", BUY_ONE, 0.0, WALL).unwrap();
    engine.algo_advance(1100.0, WALL + 1100.0);
    let executor = engine.algos.executor().unwrap();
    assert!(executor.live_tasks() > 0);
    emits(&mut engine);

    engine.algo_dispose(1200.0, WALL + 1200.0, None);
    let cancels: Vec<CommandId> = commands
        .borrow()
        .iter()
        .filter_map(|command| match command {
            Command::CancelOrders { command, ids } if ids == &["o1".to_owned()] => Some(*command),
            _ => None,
        })
        .collect();
    assert_eq!(
        cancels.len(),
        1,
        "the chase cancelled its order before stopping"
    );
    assert!(cancels[0] >= FIRST_COMMAND_ID);
    assert!(
        !engine.algo_running(),
        "shutdown finished and algos are gone"
    );
    assert_eq!(executor.live_tasks(), 0, "disposal aborted every task");
    assert_eq!(engine.algo_deadline(), None);
    let stopped = emits(&mut engine);
    assert!(stopped.contains(&serde_json::json!({
        "op": "error", "data": "Chases and TWAPs stopped: account disposed"
    })));
}

#[test]
fn only_algo_completions_are_held_back_from_the_js_host() {
    let host = AlgoHost::new("testvenue");
    let kept = host.route(vec![
        Action::Notice(Notice::CommandDone {
            command: 7,
            result: "[]".into(),
        }),
        Action::Notice(Notice::CommandDone {
            command: FIRST_COMMAND_ID,
            result: "[]".into(),
        }),
        Action::Notice(Notice::CommandError {
            command: FIRST_COMMAND_ID + 1,
            message: "x".into(),
        }),
    ]);
    assert_eq!(
        kept,
        vec![Action::Notice(Notice::CommandDone {
            command: 7,
            result: "[]".into(),
        })]
    );
}

fn cancels_of(commands: &RefCell<Vec<Command>>, order_id: &str) -> usize {
    commands
        .borrow()
        .iter()
        .filter(|command| {
            matches!(command, Command::CancelOrders { ids, .. } if ids == &[order_id.to_owned()])
        })
        .count()
}

#[test]
fn stop_all_stops_twaps_and_chases_and_reports_once_cancels_settle() {
    let (mut engine, commands) = engine();
    engine.algo_start("acct", 0.0, WALL).unwrap();
    engine.algo_op("chaser:start", BUY_ONE, 0.0, WALL).unwrap();
    let twap = serde_json::json!({
        "accountName": "acct", "activeMarket": { "symbol": "BTCUSDT" }, "size": 1, "duration": 1,
        "ordersCount": 2, "side": "buy", "randomnessPercentage": 0,
    });
    engine
        .algo_op("twap:start", &twap.to_string(), 0.0, WALL)
        .unwrap();
    engine.algo_advance(1100.0, WALL + 1100.0);
    let rows = emits(&mut engine);
    let store = rows
        .iter()
        .rev()
        .find(|emit| emit["op"] == "store")
        .unwrap();
    let chase_id = store["data"]["activeChasers"][0]["id"].clone();
    let twap_id = store["data"]["activeTWAPs"][0]["id"].clone();

    engine.algo_stop_all(1200.0, WALL + 1200.0, None).unwrap();
    assert_eq!(
        cancels_of(&commands, "o1"),
        1,
        "the chase cancelled its order"
    );
    let stopped = emits(&mut engine);
    let report = stopped
        .iter()
        .find(|emit| emit["op"] == "algos:stopped")
        .unwrap_or_else(|| panic!("no algos:stopped in {stopped:?}"));
    assert_eq!(
        report["data"],
        serde_json::json!({ "stoppedChasers": [chase_id], "stoppedTwaps": [twap_id], "settled": true })
    );
    assert!(stopped.contains(&serde_json::json!({ "op": "twap:stateUpdate", "data": {} })));
    let store = stopped
        .iter()
        .rev()
        .find(|emit| emit["op"] == "store")
        .unwrap();
    assert_eq!(store["data"]["activeTWAPs"], serde_json::json!([]));
    assert_eq!(store["data"]["activeChasers"], serde_json::json!([]));
    assert!(
        !stopped.iter().any(|emit| emit["op"] == "error"),
        "a nuke is not a disposal: {stopped:?}"
    );

    // The account stays up: new ops still run.
    assert!(engine.algo_running());
    let before = placements(&commands).len();
    engine
        .algo_op("chaser:start", BUY_ONE, 1300.0, WALL + 1300.0)
        .unwrap();
    assert_eq!(placements(&commands).len(), before + 1);
}

#[test]
fn a_fill_tracked_chase_reports_its_rest_reconciled_fill_on_completion() {
    // TS `ChaserOrderManager.emitComplete` on FILL_RECONCILED_VENUES: a fill that
    // beat the final cancel reaches `chaser:complete` only over REST.
    let (mut engine, commands, rest_fills) = engine_on("hyperliquid");
    engine.algo_start("acct", 0.0, WALL).unwrap();
    let chase =
        r#"{"symbol":"BTCUSDT","size":1,"side":"buy","maxAttempts":10,"delay":0,"clientId":"c1"}"#;
    engine.algo_op("chaser:start", chase, 0.0, WALL).unwrap();
    engine.algo_advance(1100.0, WALL + 1100.0);
    let rows = emits(&mut engine);
    let store = rows
        .iter()
        .rev()
        .find(|emit| emit["op"] == "store")
        .unwrap();
    let chase_id = store["data"]["activeChasers"][0]["id"].to_string();
    rest_fills.borrow_mut().insert("o1".into(), 0.4);

    engine
        .algo_op("chaser:terminate", &chase_id, 1200.0, WALL + 1200.0)
        .unwrap();
    assert_eq!(cancels_of(&commands, "o1"), 1);
    let done = emits(&mut engine);
    let complete = done
        .iter()
        .find(|emit| emit["op"] == "chaser:complete")
        .unwrap_or_else(|| panic!("no chaser:complete in {done:?}"));
    assert_eq!(complete["data"]["clientId"], "c1");
    assert_eq!(complete["data"]["filled"], 0.4);
}
