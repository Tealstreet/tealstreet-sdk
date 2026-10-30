//! The algo feed against a real account actor: bursts beyond a socket peer's
//! queue lose nothing, and a dropped feed rejoins instead of disposing.
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::engine::Fill;
use safe_cex_engine::model::{Limits, Market, MinMax, OrderSide, Precision};
use safe_cex_engine::{Command, Cx, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_exchange_server::algos::Feed;
use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
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

/// Every socket message is one fill of `amount`, flushed in its own frame.
struct FillVenue {
    book: Book,
    fills: u32,
}
impl Venue for FillVenue {
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
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.fills += 1;
        cx.fill(Fill {
            id: Some(format!("e{}", self.fills)),
            order_id: "o1".into(),
            symbol: "BTCUSDT".into(),
            side: OrderSide::Buy,
            price: 100.,
            amount: text.parse().unwrap(),
        });
        self.book.apply(BookSide::Bids, 99., f64::from(self.fills));
        cx.book_changed("BTCUSDT");
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, symbol: &str) -> Option<&Book> {
        (symbol == "BTCUSDT").then_some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("BTCUSDT", &self.book)))
    }
}

fn account() -> (safe_cex_native::AccountHandle, Tape) {
    let tape = Tape::default();
    let engine = safe_cex_engine::test_support::seeded_engine(FillVenue {
        book: Book::default(),
        fills: 0,
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

async fn fills_until(feed: &mut Feed, total: usize) -> Vec<Fill> {
    let mut fills = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), async {
        while fills.len() < total {
            fills.extend(feed.next().await.expect("account alive").fills);
        }
    })
    .await
    .expect("every fill arrives");
    fills
}

#[tokio::test]
async fn a_burst_past_a_socket_peers_queue_keeps_the_account_and_every_fill() {
    LocalSet::new()
        .run_until(async {
            let (handle, tape) = account();
            let mut feed = Feed::join(handle.clone(), 1).await.unwrap();
            // The feed is not polled while 40 fills each flush a frame (a socket peer's queue is 32).
            for _ in 0..40 {
                tape.message("0.1");
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
            let fills = fills_until(&mut feed, 40).await;
            assert_eq!(fills.len(), 40);
            let ids: Vec<_> = fills.iter().map(|fill| fill.id.clone().unwrap()).collect();
            assert_eq!(ids.first().map(String::as_str), Some("e1"));
            assert_eq!(
                ids.last().map(String::as_str),
                Some("e40"),
                "in order, none lost"
            );
            assert!(handle.join(99, false).await.is_ok(), "the account is alive");
        })
        .await;
}

#[tokio::test]
async fn a_dropped_feed_rejoins_with_a_snapshot_instead_of_ending() {
    LocalSet::new()
        .run_until(async {
            let (handle, tape) = account();
            let mut feed = Feed::join(handle.clone(), 1).await.unwrap();
            assert!(feed.next().await.unwrap().frames >= 1, "the join snapshot");
            handle.leave(feed.client());
            let rejoined = tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if feed.next().await.expect("account alive").rejoined {
                        break;
                    }
                }
            })
            .await;
            assert!(rejoined.is_ok(), "the feed rejoined");
            assert_ne!(feed.client(), 1);
            tape.message("0.5");
            assert_eq!(
                fills_until(&mut feed, 1).await[0].amount,
                0.5,
                "it keeps receiving"
            );
            handle.dispose();
            assert!(
                tokio::time::timeout(Duration::from_secs(3), async {
                    while feed.next().await.is_some() {}
                })
                .await
                .is_ok(),
                "only the account's end ends the feed"
            );
        })
        .await;
}

#[tokio::test]
async fn a_feed_dropped_over_and_over_gives_up_instead_of_spinning() {
    LocalSet::new()
        .run_until(async {
            let (handle, _tape) = account();
            let mut feed = Feed::join(handle.clone(), 1).await.unwrap();
            let mut rejoins = 0;
            let ended = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    handle.leave(feed.client());
                    match feed.next().await {
                        Some(batch) => rejoins += usize::from(batch.rejoined),
                        None => break,
                    }
                }
            })
            .await;
            assert!(ended.is_ok(), "the feed ends");
            assert_eq!(rejoins, safe_cex_exchange_server::algos::MAX_REJOINS);
        })
        .await;
}

#[tokio::test]
async fn an_algo_host_whose_feed_gives_up_leaves_the_account_running() {
    LocalSet::new()
        .run_until(async {
            let (handle, _tape) = account();
            let (_algos, inbox) = mpsc::unbounded_channel();
            let host = tokio::task::spawn_local(safe_cex_exchange_server::algos::run(
                handle.clone(),
                1,
                Default::default(),
                Box::new(|_| {}),
                Box::new(|_| Default::default()),
                None,
                Default::default(),
                Default::default(),
                inbox,
            ));
            // Drop the host's feed over and over, its rejoined clients included.
            let ended = tokio::time::timeout(Duration::from_secs(10), async {
                while !host.is_finished() {
                    handle.leave(1);
                    for id in (1u64 << 62)..(1u64 << 62) + 64 {
                        handle.leave(id);
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(ended.is_ok(), "the algo host gave up");
            assert!(
                handle.join(99, false).await.is_ok(),
                "an algo failure never disposes the account"
            );
        })
        .await;
}
