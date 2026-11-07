//! Offline native-server fixture for the TypeScript session integration test.
//! No venue credentials or exchange network calls are accepted or performed.
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{Command, Cx, Engine, HttpRequest, Response, SocketId, TimerId, Venue};
use safe_cex_native::{Events, FrameServer, Host, ServerConfig, Transport};
use serde::Serialize;
use std::io::Read;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

struct Offline;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Counters<'a> {
    symbol: &'a str,
    writes: u32,
    timer_firings: u32,
}

#[derive(Serialize)]
struct Endpoint {
    url: String,
}

impl Transport for Offline {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("offline fixture must not open venue sockets")
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("offline fixture must not send venue requests")
    }
}

#[derive(Default)]
struct FixtureVenue {
    book: Book,
    writes: u32,
    timer_firings: u32,
    pending: Option<(TimerId, u32, f64)>,
}
impl Venue for FixtureVenue {
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
        self.book.apply(BookSide::Bids, 99., 1.);
        self.book.apply(BookSide::Bids, 98., 2.);
        self.book.apply(BookSide::Asks, 101., 1.);
        cx.book_changed("BTCUSDT");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
        let Some((pending_timer, command, amount)) = self.pending else {
            return;
        };
        if timer != pending_timer {
            return;
        }
        self.pending = None;
        self.timer_firings += 1;
        self.book.apply(BookSide::Bids, 99., amount);
        self.book.apply(BookSide::Bids, 98., 0.);
        self.book.apply(BookSide::Bids, 97., amount + 1.);
        self.book.apply(BookSide::Asks, 101., 0.);
        self.book.apply(BookSide::Asks, 102., amount + 2.);
        cx.book_changed("BTCUSDT");
        cx.done(command, self.counters("BTCUSDT"));
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        match command {
            Command::SubscribeOrderBook { .. } => cx.book_changed("BTCUSDT"),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => {
                if symbol != "BTCUSDT" || self.pending.is_some() {
                    cx.fail(command, "invalid or concurrent fixture update");
                    return;
                }
                self.writes += 1;
                // Exercise the native host's real timer path without wall-clock sleeps
                // in the consumer: its command acknowledgement is the synchronization.
                self.pending = Some((cx.set_timer(1.), command, leverage));
            }
            Command::FetchLeverage { command, symbol } => {
                cx.done(command, self.counters(&symbol));
            }
            _ => {}
        }
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        (symbol == "BTCUSDT").then_some(&self.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::once(("BTCUSDT", &self.book)))
    }
}
impl FixtureVenue {
    fn counters(&self, symbol: &str) -> String {
        serde_json::to_string(&Counters {
            symbol,
            writes: self.writes,
            timer_firings: self.timer_firings,
        })
        .expect("fixture counters are serializable")
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut server = FrameServer::new(ServerConfig {
                flush_every: Duration::from_millis(10),
                ..ServerConfig::default()
            })?;
            server.add_account(
                "fixture",
                "offline-test-token",
                Host::new(
                    Engine::new(
                        FixtureVenue::default(),
                        Some(safe_cex_native::production_entropy()),
                    )
                    .expect("production entropy configured"),
                    Offline,
                ),
            )?;
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            println!(
                "{}",
                serde_json::to_string(&Endpoint {
                    url: format!("ws://{}/accounts/fixture", listener.local_addr()?),
                })?
            );
            let (shutdown, stopping) = oneshot::channel();
            std::thread::spawn(move || {
                let _ = std::io::stdin().read_to_end(&mut Vec::new());
                let _ = shutdown.send(());
            });
            server
                .serve(listener, async {
                    let _ = stopping.await;
                })
                .await
        })
        .await?;
    Ok(())
}
