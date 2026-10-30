//! Hosted store reads wait for the account's first load, as `untilNativeData`
//! does in rust.exchange.ts, instead of refusing "not loaded yet".
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Command, Cx, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_exchange_server::methods::{StoreRead, read_when_loaded, until_account_loaded};
use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
use serde_json::json;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[derive(Clone, Default)]
struct Tape(Rc<RefCell<Option<Events>>>);
impl Tape {
    fn message(&self, text: &str) {
        let events = self.0.borrow();
        events
            .as_ref()
            .unwrap()
            .send(HostEvent::WsMessage(SocketId(1), text.into()))
            .unwrap();
    }
}
impl Transport for Tape {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        *self.0.borrow_mut() = Some(events.clone());
        let _ = events.send(HostEvent::WsOpened(socket));
        mpsc::unbounded_channel().0
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("the test venue never requests HTTP")
    }
}

/// Markets load at start; orders load on the first socket message.
struct LateOrders {
    book: Book,
}
impl Venue for LateOrders {
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
        cx.connect("test");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: &str) {
        cx.store.set_orders(Vec::new());
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        Some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn account() -> (safe_cex_native::AccountHandle, Tape) {
    let tape = Tape::default();
    let engine = safe_cex_engine::test_support::seeded_engine(LateOrders {
        book: Book::default(),
    });
    let config = ServerConfig {
        flush_every: Duration::from_millis(1),
        io_timeout: Duration::from_millis(100),
        ..ServerConfig::default()
    };
    let (handle, _task) =
        safe_cex_native::spawn_account(Host::new(engine, tape.clone()), config).unwrap();
    (handle, tape)
}

#[tokio::test]
async fn a_read_before_the_first_load_waits_and_then_answers() {
    LocalSet::new()
        .run_until(async {
            let (handle, tape) = account();
            let read = tokio::task::spawn_local({
                let handle = handle.clone();
                async move {
                    read_when_loaded(&handle, StoreRead::Orders(None), Duration::from_secs(5)).await
                }
            });
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(!read.is_finished(), "it waits for the orders section");
            tape.message("load");
            let answer = tokio::time::timeout(Duration::from_secs(3), read)
                .await
                .expect("answers once loaded")
                .unwrap();
            assert_eq!(answer.unwrap(), json!([]));
            handle.dispose();
        })
        .await;
}

#[tokio::test]
async fn a_read_that_never_loads_refuses_after_the_wait() {
    LocalSet::new()
        .run_until(async {
            let (handle, _tape) = account();
            let refused = read_when_loaded(&handle, StoreRead::Balance, Duration::from_millis(200))
                .await
                .unwrap_err();
            assert_eq!(refused.message, "balances did not load within 0s");
            handle.dispose();
        })
        .await;
}

#[tokio::test]
async fn get_account_on_an_after_load_venue_waits_for_the_start_then_refuses_visibly() {
    LocalSet::new()
        .run_until(async {
            let (handle, tape) = account();
            tokio::time::sleep(Duration::from_millis(100)).await;
            tape.message("load");
            // Orders load, but balance and positions never do: the start never completes.
            let refused = until_account_loaded(&handle, Duration::from_millis(200))
                .await
                .unwrap_err();
            assert_eq!(refused.message, "the account did not load within 0s");
            handle.dispose();
            assert_eq!(
                until_account_loaded(&handle, Duration::from_secs(1))
                    .await
                    .unwrap_err()
                    .message,
                "account is closed"
            );
        })
        .await;
}
