//! A hosted `nuke()` runs exec's nuke on the account's algo host: it cancels each
//! open-order symbol, closes each position reduce-only, and answers the call once.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{
    InstrumentClassification, Limits, Market, MinMax, Order, OrderSide, OrderStatus, OrderType,
    Position, PositionSide, Precision,
};
use safe_cex_engine::{Command, Cx, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_exchange_server::algos::{self, AlgoInput};
use safe_cex_exchange_server::audit::Who;
use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[derive(Clone, Default)]
struct Quiet;
impl Transport for Quiet {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("the test venue never requests HTTP")
    }
}

/// One open order and one long position; records and accepts every command.
struct NukeVenue {
    book: Book,
    commands: Arc<Mutex<Vec<String>>>,
}
impl Venue for NukeVenue {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities::BASE
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        cx.store.set_markets(vec![Market {
            id: "BTCUSDT".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
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
        }]);
        // Bybit gates placements on its instrument labels; this market is a plain perp.
        cx.store
            .set_instrument_classifications(vec![InstrumentClassification {
                market: 0,
                symbol_type: None,
                market_region: None,
                underlying_ticker: None,
            }]);
        cx.store.set_orders(vec![Order {
            id: "o-1".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 50_000.,
            amount: 0.001,
            filled: 0.,
            remaining: 0.001,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        cx.store.set_positions(vec![Position {
            id: "BTCUSDT-long".into(),
            symbol: "BTCUSDT".into(),
            side: PositionSide::Long,
            entry_price: 60_000.,
            notional: 60.,
            leverage: 10.,
            margin_type: None,
            position_mode: None,
            unrealized_pnl: 0.,
            realized_pnl: None,
            contracts: 0.001,
            liquidation_price: 0.,
            margin_size: None,
            last_update: None,
        }]);
        cx.connect("test");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        let Some(id) = command.completion_id() else {
            return;
        };
        let (name, result) = match &command {
            Command::CancelSymbolOrders { symbol, .. } => (format!("cancel {symbol}"), "null"),
            Command::PlaceOrders { orders, .. } => (
                format!(
                    "place {}",
                    serde_json::to_string(orders).unwrap_or_default()
                ),
                r#"["close-1"]"#,
            ),
            other => (format!("{other:?}"), "null"),
        };
        self.commands.lock().unwrap().push(name);
        cx.done(id, result.into());
    }
    fn book(&self, _: &str) -> Option<&Book> {
        Some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("BTCUSDT", &self.book)))
    }
}

async fn nuke(mode: &'static str) -> (Value, Vec<String>) {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let engine = safe_cex_engine::test_support::seeded_engine(NukeVenue {
        book: Book::default(),
        commands: commands.clone(),
    });
    let config = ServerConfig {
        flush_every: Duration::from_millis(1),
        io_timeout: Duration::from_millis(100),
        ..ServerConfig::default()
    };
    let (handle, _task) = safe_cex_native::spawn_account(Host::new(engine, Quiet), config).unwrap();
    let sent = Rc::new(RefCell::new(Vec::<String>::new()));
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let who = Who {
        user: "u".into(),
        account: "acct".into(),
        venue: "bybit".into(),
    };
    let (inbox, inbox_rx) = mpsc::unbounded_channel();
    tokio::task::spawn_local(algos::run(
        handle,
        1,
        who,
        Box::new(move |text| {
            let _ = tx.send(text);
        }),
        None,
        inbox_rx,
    ));
    // Let the feed take its join snapshot so the store holds the order and position.
    tokio::time::sleep(Duration::from_millis(100)).await;
    inbox
        .send(AlgoInput::Nuke {
            mode,
            request_id: "r-nuke".into(),
        })
        .unwrap();
    let done = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let text = rx.recv().await.expect("algo host alive");
            sent.borrow_mut().push(text.clone());
            let message: Value = serde_json::from_str(&text).unwrap();
            if message["d"]["op"] == "callMethod:done" {
                return message["d"]["data"].clone();
            }
        }
    })
    .await
    .expect("nuke answers");
    let _ = inbox.send(AlgoInput::Dispose);
    let commands = commands.lock().unwrap().clone();
    (done, commands)
}

#[tokio::test]
async fn nuke_cancels_each_order_symbol_closes_each_position_and_answers_once() {
    LocalSet::new()
        .run_until(async {
            let (done, commands) = nuke("all").await;
            assert_eq!(
                done,
                json!({"requestId":"r-nuke","success":true,"result":null})
            );
            assert_eq!(commands.len(), 2, "{commands:?}");
            assert_eq!(commands[0], "cancel BTCUSDT");
            assert!(commands[1].starts_with("place "), "{commands:?}");
            assert!(commands[1].contains("\"side\":\"sell\""), "{commands:?}");
            assert!(commands[1].contains("\"reduceOnly\":true"), "{commands:?}");
        })
        .await;
}

#[tokio::test]
async fn nuke_positions_and_nuke_orders_run_one_half() {
    LocalSet::new()
        .run_until(async {
            let (done, commands) = nuke("positions").await;
            assert_eq!(done["success"], true);
            assert_eq!(commands.len(), 1, "{commands:?}");
            assert!(commands[0].starts_with("place "));
            let (done, commands) = nuke("orders").await;
            assert_eq!(done["success"], true);
            assert_eq!(commands, vec!["cancel BTCUSDT".to_owned()]);
        })
        .await;
}
