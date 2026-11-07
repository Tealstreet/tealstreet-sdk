//! Real loopback protocol tests with deterministic, non-Send account engines.
#![cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::book::{Book, BookSide};
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::wire::{book_top, section, sections};
use safe_cex_engine::{
    Command, Cx, Event, HttpRequest, Method, Response, SocketId, TimerId, Venue,
};
use safe_cex_native::{Events, FrameServer, Host, HostEvent, ServerConfig, Transport};
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, LocalSet};
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const DEADLINE: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
struct Tape(Rc<RefCell<Option<Events>>>);
impl Tape {
    fn message(&self, text: &str) {
        self.0
            .borrow()
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
        panic!("test venue never requests HTTP")
    }
}

struct TestVenue {
    label: &'static str,
    book: Book,
    observed: Rc<RefCell<Vec<Command>>>,
    pending: Vec<u32>,
    received: Rc<RefCell<Vec<String>>>,
    started: Rc<Cell<usize>>,
    poll_scales: Rc<RefCell<Vec<f64>>>,
    poll_intervals: Rc<RefCell<Vec<f64>>>,
    poll_timer: Option<TimerId>,
}
impl Venue for TestVenue {
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            account: safe_cex_engine::AccountLookup::Query,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.started.set(self.started.get() + 1);
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
        cx.book_changed("BTCUSDT");
        // A 1 s native cadence: a scale is the host's interval in seconds.
        self.poll_timer = Some(cx.set_poll_timer(1_000.0));
    }
    fn on_poll_scale(&mut self, cx: &mut Cx<'_, ()>, scale: f64) {
        self.poll_scales.borrow_mut().push(scale);
        if let Some(interval) = self.poll_timer.and_then(|timer| cx.poll_interval(timer)) {
            self.poll_intervals.borrow_mut().push(interval);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.received.borrow_mut().push(text.into());
        if text == "complete" {
            for id in self.pending.drain(..) {
                cx.done(id, format!("\"{}\"", self.label));
            }
        } else {
            let amount: f64 = text.parse().unwrap();
            self.book.apply(BookSide::Bids, 99., amount);
            cx.book_changed("BTCUSDT");
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        self.observed.borrow_mut().push(command.clone());
        match command {
            Command::SubscribeOrderBook { .. } => cx.book_changed("BTCUSDT"),
            Command::FetchAccount { command } => cx.done(command, format!("\"{}\"", self.label)),
            Command::FetchLeverage { command, symbol } => {
                self.pending.push(command);
                if symbol == "queued" {
                    cx.request(
                        Method::Post,
                        "https://venue.test/held".into(),
                        vec![],
                        None,
                        (),
                    );
                }
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

struct Fixture {
    url: String,
    a: Tape,
    observed: Rc<RefCell<Vec<Command>>>,
    received: Rc<RefCell<Vec<String>>>,
    poll_scales: Rc<RefCell<Vec<f64>>>,
    poll_intervals: Rc<RefCell<Vec<f64>>>,
    shutdown: oneshot::Sender<()>,
    task: JoinHandle<std::io::Result<()>>,
}
impl Fixture {
    async fn new(config: ServerConfig) -> Self {
        Self::with_policy(config, safe_cex_engine::PublicFeedPolicy::Direct).await
    }
    async fn with_policy(config: ServerConfig, policy: safe_cex_engine::PublicFeedPolicy) -> Self {
        let mut server = FrameServer::new(config).unwrap();
        let a = Tape::default();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let received = Rc::new(RefCell::new(Vec::new()));
        let poll_scales = Rc::new(RefCell::new(Vec::new()));
        let poll_intervals = Rc::new(RefCell::new(Vec::new()));
        for (label, tape, commands) in [
            ("a", a.clone(), observed.clone()),
            ("b", Tape::default(), Rc::new(RefCell::new(Vec::new()))),
        ] {
            let mut book = Book::default();
            book.apply(BookSide::Bids, 99., 1.);
            book.apply(BookSide::Asks, 101., 1.);
            let mut engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
                label,
                book,
                observed: commands,
                pending: Vec::new(),
                received: received.clone(),
                started: Rc::default(),
                poll_scales: poll_scales.clone(),
                poll_intervals: poll_intervals.clone(),
                poll_timer: None,
            });
            engine.set_public_feed_policy(policy).unwrap();
            server
                .add_account(label, &format!("secret-{label}"), Host::new(engine, tape))
                .unwrap();
        }
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (shutdown, stopping) = oneshot::channel();
        let task = tokio::task::spawn_local(server.serve(listener, async {
            let _ = stopping.await;
        }));
        Self {
            url,
            a,
            observed,
            received,
            poll_scales,
            poll_intervals,
            shutdown,
            task,
        }
    }
    async fn connect(&self, account: &str) -> Socket {
        let mut request = format!("{}/accounts/{account}", self.url)
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "authorization",
            format!("Bearer secret-{account}").parse().unwrap(),
        );
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }
    async fn seen(&self, text: &str) {
        tokio::time::timeout(DEADLINE, async {
            while !self.received.borrow().iter().any(|message| message == text) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn stop(self) {
        let _ = self.shutdown.send(());
        tokio::time::timeout(DEADLINE, self.task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            self.a.0.borrow().as_ref().unwrap().is_closed(),
            "account transport receiver is dropped"
        );
    }
}
fn config() -> ServerConfig {
    ServerConfig {
        flush_every: Duration::from_millis(10),
        io_timeout: Duration::from_millis(100),
        ..ServerConfig::default()
    }
}
async fn message(socket: &mut Socket) -> Message {
    tokio::time::timeout(DEADLINE, socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
async fn frame(socket: &mut Socket) -> Vec<u8> {
    loop {
        if let Message::Binary(bytes) = message(socket).await {
            return bytes.to_vec();
        }
    }
}
async fn send(socket: &mut Socket, json: &str) {
    socket.send(Message::text(json)).await.unwrap();
}
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
enum Reply {
    Done {
        command: u32,
        result: Option<String>,
        error: Option<String>,
    },
    Error {
        error: String,
    },
}
async fn reply(socket: &mut Socket) -> Reply {
    loop {
        if let Message::Text(text) = message(socket).await {
            return serde_json::from_str(&text).unwrap();
        }
    }
}
async fn observed(fixture: &Fixture, predicate: impl Fn(&[Command]) -> bool) {
    tokio::time::timeout(DEADLINE, async {
        loop {
            if predicate(&fixture.observed.borrow()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn snapshot_delta_late_join_reversion_and_reconnect_are_ordered() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(ServerConfig {
                flush_every: Duration::from_secs(60),
                ..config()
            })
            .await;
            let mut first = fixture.connect("a").await;
            let initial = frame(&mut first).await;
            assert!(sections(&initial).any(|(kind, _)| kind == section::MARKETS));
            assert_eq!(
                sections(&initial).find_map(|(kind, data)| (kind == section::BOOK)
                    .then(|| book_top(data))
                    .flatten()),
                Some((0, 99., 101.))
            );
            fixture.a.message("2");
            fixture.seen("2").await;
            // With no periodic flush due, joining must publish amount=2 to the
            // existing peer before taking the new peer's matching snapshot.
            let mut second = fixture.connect("a").await;
            let snapshot = frame(&mut second).await;
            assert!(sections(&snapshot).any(|(kind, _)| kind == section::BOOK));
            let changed = frame(&mut first).await;
            let amount = |frame: &[u8]| {
                sections(frame).find_map(|(kind, payload)| {
                    (kind == section::BOOK_DELTA)
                        .then(|| f64::from_le_bytes(payload[24..32].try_into().unwrap()))
                })
            };
            assert_eq!(amount(&changed), Some(2.));
            fixture.a.message("1");
            fixture.seen("1").await;
            let mut reconnected = fixture.connect("a").await;
            assert_eq!(amount(&frame(&mut first).await), Some(1.));
            assert_eq!(amount(&frame(&mut second).await), Some(1.));
            assert!(
                sections(&frame(&mut reconnected).await).any(|(kind, _)| kind == section::BOOK)
            );
            first.close(None).await.unwrap();
            fixture.stop().await;
            assert!(matches!(message(&mut second).await, Message::Close(_)));
        })
        .await;
}

#[tokio::test]
async fn authenticated_accounts_and_command_results_are_isolated() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        let mut a = fixture.connect("a").await;
        let mut peer = fixture.connect("a").await;
        let mut b = fixture.connect("b").await;
        frame(&mut a).await; frame(&mut peer).await; frame(&mut b).await;
        for socket in [&mut a, &mut peer, &mut b] { send(socket, r#"{"op":"fetchAccount","command":7}"#).await; }
        for (socket, account) in [(&mut a, "a"), (&mut peer, "a"), (&mut b, "b")] {
            assert!(matches!(reply(socket).await, Reply::Done { command: 7, result: Some(result), error: None } if result == format!("\"{account}\"")));
        }
        send(&mut a, r#"{"op":"fetchLeverage","command":9,"symbol":"hold"}"#).await;
        send(&mut a, r#"{"op":"fetchLeverage","command":9,"symbol":"hold"}"#).await;
        assert!(matches!(reply(&mut a).await, Reply::Error { error } if error.contains("already pending")));
        a.close(None).await.unwrap();
        fixture.a.message("complete");
        send(&mut peer, r#"{"op":"fetchAccount","command":9}"#).await;
        assert!(matches!(reply(&mut peer).await, Reply::Done { command: 9, result: Some(_), .. }));
        assert!(tokio::time::timeout(Duration::from_millis(30), b.next()).await.is_err(), "no other account result leaks");
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn unsupported_symbol_amend_preserves_client_id_and_releases_pending_capacity() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(ServerConfig {
            max_pending_per_client: 1,
            max_pending_per_account: 1,
            ..config()
        }).await;
        let mut socket = fixture.connect("a").await;
        frame(&mut socket).await;
        send(&mut socket, r#"{"op":"amendOrderWithSymbol","command":7,"id":"unseen","symbol":"BTC-USDT","price":100}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Done {
            command: 7, result: None, error: Some(error),
        } if error.contains("unsupported")));
        assert!(fixture.observed.borrow().is_empty());
        send(&mut socket, r#"{"op":"fetchAccount","command":7}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Done {
            command: 7, result: Some(result), error: None,
        } if result == "\"a\""));
        assert_eq!(fixture.observed.borrow().len(), 1);
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn subscriptions_are_refcounted_and_disconnect_releases_them() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(config()).await;
            let mut a = fixture.connect("a").await;
            frame(&mut a).await;
            let mut b = fixture.connect("a").await;
            frame(&mut b).await;
            for socket in [&mut a, &mut b] {
                send(socket, r#"{"op":"subscribeOrderBook","symbol":"BTCUSDT"}"#).await;
                send(
                    socket,
                    r#"{"op":"subscribeCandles","symbol":"BTCUSDT","interval":"1m"}"#,
                )
                .await;
            }
            observed(&fixture, |commands| {
                commands
                    .iter()
                    .any(|command| matches!(command, Command::SubscribeCandles { .. }))
            })
            .await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert_eq!(
                fixture
                    .observed
                    .borrow()
                    .iter()
                    .filter(|command| matches!(command, Command::SubscribeOrderBook { .. }))
                    .count(),
                1
            );
            assert_eq!(
                fixture
                    .observed
                    .borrow()
                    .iter()
                    .filter(|command| matches!(command, Command::SubscribeCandles { .. }))
                    .count(),
                1
            );
            a.close(None).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert!(!fixture.observed.borrow().iter().any(|command| matches!(
                command,
                Command::UnsubscribeOrderBook { .. } | Command::UnsubscribeCandles { .. }
            )));
            b.close(None).await.unwrap();
            observed(&fixture, |commands| {
                commands
                    .iter()
                    .any(|command| matches!(command, Command::UnsubscribeCandles { .. }))
            })
            .await;
            assert_eq!(
                fixture
                    .observed
                    .borrow()
                    .iter()
                    .filter(|command| matches!(command, Command::UnsubscribeOrderBook { .. }))
                    .count(),
                1
            );
            fixture.stop().await;
        })
        .await;
}

#[tokio::test]
async fn bad_auth_is_rejected_before_upgrade_and_browser_tokens_are_not_echoed() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        for (path, auth) in [("/accounts/a", ""), ("/accounts/a", "Bearer secret-b"), ("/accounts/missing", "Bearer secret-a"), ("/accounts/a?token=secret-a", "Bearer secret-a")] {
            let mut request = format!("{}{path}", fixture.url).into_client_request().unwrap();
            if !auth.is_empty() { request.headers_mut().insert("authorization", auth.parse().unwrap()); }
            let error = tokio_tungstenite::connect_async(request).await.unwrap_err();
            assert!(matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 401));
        }
        let mut request = format!("{}/accounts/a", fixture.url).into_client_request().unwrap();
        request.headers_mut().insert("sec-websocket-protocol", "safe-cex-frame.v1, bearer.secret-a".parse().unwrap());
        let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
        assert_eq!(response.headers()["sec-websocket-protocol"], "safe-cex-frame.v1");
        frame(&mut socket).await;
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn malformed_commands_and_binary_input_do_not_mutate_the_engine_and_ping_is_flushed() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        let mut socket = fixture.connect("a").await; frame(&mut socket).await;
        for invalid in ["no json", r#"{"op":"createAccount","apiKey":"ignored"}"#, r#"{"op":"placeOrders","command":2,"orders":"invalid"}"#] {
            send(&mut socket, invalid).await;
            assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error == "invalid command"));
        }
        socket.send(Message::binary(vec![1, 2])).await.unwrap();
        assert!(matches!(reply(&mut socket).await, Reply::Error { .. }));
        socket.send(Message::Ping(vec![4, 5].into())).await.unwrap();
        assert_eq!(message(&mut socket).await, Message::Pong(vec![4, 5].into()));
        assert!(fixture.observed.borrow().is_empty());
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn bounded_commands_and_subscriptions_reject_before_venue_mutation() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(ServerConfig { max_pending_per_client: 1, max_pending_per_account: 1, max_subscriptions_per_client: 1, max_subscriptions_per_account: 1, ..config() }).await;
        let mut socket = fixture.connect("a").await; frame(&mut socket).await;
        send(&mut socket, r#"{"op":"subscribeOrderBook","symbol":"BTCUSDT"}"#).await;
        send(&mut socket, r#"{"op":"subscribeOrderBook","symbol":"UNKNOWN"}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error.contains("subscription limit")));
        send(&mut socket, r#"{"op":"setTickerSymbols","symbols":["A","B"]}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error.contains("subscription limit")));
        send(&mut socket, r#"{"op":"fetchLeverage","command":1,"symbol":"hold"}"#).await;
        send(&mut socket, r#"{"op":"fetchLeverage","command":2,"symbol":"hold"}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error.contains("pending command limit")));
        assert!(!fixture.observed.borrow().iter().any(|command| matches!(command, Command::SubscribeOrderBook { symbol } if symbol == "UNKNOWN")));
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn ticker_interest_is_a_union_and_empty_means_all() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        let mut a = fixture.connect("a").await; frame(&mut a).await;
        let mut b = fixture.connect("a").await; frame(&mut b).await;
        send(&mut a, r#"{"op":"setTickerSymbols","symbols":["A"]}"#).await;
        send(&mut b, r#"{"op":"setTickerSymbols","symbols":["B"]}"#).await;
        observed(&fixture, |commands| matches!(commands.last(), Some(Command::SetTickerSymbols { symbols }) if symbols == &["A", "B"])).await;
        send(&mut b, r#"{"op":"setTickerSymbols","symbols":[]}"#).await;
        observed(&fixture, |commands| matches!(commands.last(), Some(Command::SetTickerSymbols { symbols }) if symbols.is_empty())).await;
        b.close(None).await.unwrap();
        observed(&fixture, |commands| matches!(commands.last(), Some(Command::SetTickerSymbols { symbols }) if symbols == &["A"])).await;
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn disconnected_jobs_still_bound_account_capacity_until_completion() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(ServerConfig { max_pending_per_account: 1, ..config() }).await;
        let mut a = fixture.connect("a").await; frame(&mut a).await;
        send(&mut a, r#"{"op":"fetchLeverage","command":77,"symbol":"hold"}"#).await;
        observed(&fixture, |commands| commands.iter().any(|command| matches!(command, Command::FetchLeverage { .. }))).await;
        a.close(None).await.unwrap();
        let mut b = fixture.connect("a").await; frame(&mut b).await;
        send(&mut b, r#"{"op":"fetchAccount","command":77}"#).await;
        assert!(matches!(reply(&mut b).await, Reply::Error { error } if error.contains("pending command limit")));
        fixture.a.message("complete");
        fixture.seen("complete").await;
        send(&mut b, r#"{"op":"fetchAccount","command":78}"#).await;
        assert!(matches!(reply(&mut b).await, Reply::Done { command: 78, result: Some(_), .. }));
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn message_and_account_client_limits_close_connections_without_stopping_the_account() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(ServerConfig {
                max_message_bytes: 64,
                max_clients_per_account: 1,
                ..config()
            })
            .await;
            let mut a = fixture.connect("a").await;
            frame(&mut a).await;
            let mut rejected = fixture.connect("a").await;
            assert!(matches!(message(&mut rejected).await, Message::Close(_)));
            a.send(Message::text("x".repeat(65))).await.unwrap();
            assert!(
                tokio::time::timeout(DEADLINE, a.next())
                    .await
                    .unwrap()
                    .is_some()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
            let mut b = fixture.connect("a").await;
            frame(&mut b).await;
            send(&mut b, r#"{"op":"fetchAccount","command":1}"#).await;
            assert!(matches!(
                reply(&mut b).await,
                Reply::Done { command: 1, .. }
            ));
            fixture.stop().await;
        })
        .await;
}

#[tokio::test]
async fn native_socket_transport_cancels_when_the_host_event_receiver_closes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}", listener.local_addr().unwrap());
    let (events, mut inbox) = mpsc::unbounded_channel();
    let outgoing = safe_cex_native::NetworkTransport::new().connect(SocketId(1), url, events);
    let (stream, _) = tokio::time::timeout(DEADLINE, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut upstream = tokio_tungstenite::accept_async(stream).await.unwrap();
    assert!(matches!(
        tokio::time::timeout(DEADLINE, inbox.recv()).await.unwrap(),
        Some(HostEvent::WsOpened(_))
    ));
    inbox.close();
    // Keep the outgoing sender alive: cancellation must observe the closed
    // host receiver, rather than depend on a graceful upstream close write.
    let ended = tokio::time::timeout(DEADLINE, upstream.next())
        .await
        .unwrap();
    assert!(ended.is_none() || matches!(ended, Some(Err(_)) | Some(Ok(Message::Close(_)))));
    drop(outgoing);
}

#[tokio::test]
async fn oversized_snapshot_fails_closed_and_shutdown_cancels_unfinished_handshakes() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(ServerConfig {
                max_frame_bytes: 32,
                ..config()
            })
            .await;
            let mut socket = fixture.connect("a").await;
            assert!(matches!(message(&mut socket).await, Message::Close(_)));
            let address = fixture.url.strip_prefix("ws://").unwrap();
            let _unfinished = TcpStream::connect(address).await.unwrap();
            fixture.stop().await;
        })
        .await;
}

#[tokio::test]
async fn aborting_server_future_drops_account_transport_and_connections() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(config()).await;
            let mut socket = fixture.connect("a").await;
            frame(&mut socket).await;
            fixture.task.abort();
            assert!(fixture.task.await.unwrap_err().is_cancelled());
            let events = fixture.a.0.borrow().as_ref().unwrap().clone();
            tokio::time::timeout(DEADLINE, events.closed())
                .await
                .unwrap();
            let ended = tokio::time::timeout(DEADLINE, socket.next()).await.unwrap();
            assert!(ended.is_none() || matches!(ended, Some(Err(_)) | Some(Ok(Message::Close(_)))));
        })
        .await;
}

#[tokio::test]
async fn registration_rejects_preissued_jobs_before_startup_or_transport_execution() {
    struct CountingTransport(Rc<Cell<usize>>);
    impl Transport for CountingTransport {
        fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
            self.0.set(self.0.get() + 1);
            mpsc::unbounded_channel().0
        }
        fn request(&self, _: HttpRequest, _: Events) {
            self.0.set(self.0.get() + 1);
        }
    }
    LocalSet::new()
        .run_until(async {
            let started = Rc::new(Cell::new(0));
            let observed = Rc::new(RefCell::new(Vec::new()));
            let venue = || TestVenue {
                label: "preconfigured",
                book: Book::default(),
                observed: observed.clone(),
                pending: Vec::new(),
                received: Rc::default(),
                started: started.clone(),
                poll_scales: Rc::default(),
                poll_intervals: Rc::default(),
                poll_timer: None,
            };
            let transport_calls = Rc::new(Cell::new(0));
            let mut engine = safe_cex_engine::test_support::seeded_engine(venue());
            // A legal engine call creates an unfinished command1 and queued HTTP
            // effect before the server can allocate the same internal ID.
            engine.handle(
                0.,
                Event::Command(Command::FetchLeverage {
                    command: 1,
                    symbol: "queued".into(),
                }),
            );
            assert_eq!(observed.borrow().len(), 1);
            let mut server = FrameServer::new(config()).unwrap();
            let error = server
                .add_account(
                    "account",
                    "account-token",
                    Host::new(engine, CountingTransport(transport_calls.clone())),
                )
                .unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert!(error.to_string().contains("must not receive commands"));
            tokio::task::yield_now().await;
            assert_eq!(started.get(), 0, "rejected account never starts");
            assert_eq!(transport_calls.get(), 0, "queued HTTP is never executed");

            // Rejection reserves neither the account name nor token. Existing
            // command-free startup remains valid and runs its queued effects once.
            let mut fresh = safe_cex_engine::test_support::seeded_engine(venue());
            fresh.start(1.);
            server
                .add_account(
                    "account",
                    "account-token",
                    Host::new(fresh, CountingTransport(transport_calls.clone())),
                )
                .unwrap();
            tokio::task::yield_now().await;
            assert_eq!(started.get(), 1, "server must not repeat startup");
            assert_eq!(
                transport_calls.get(),
                1,
                "only fresh startup socket executes"
            );
        })
        .await;
}

#[tokio::test]
async fn websocket_polling_control_updates_and_pauses_without_using_command_ids() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(config()).await;
            let mut socket = fixture.connect("a").await;
            frame(&mut socket).await;
            send(&mut socket, r#"{"op":"setPollScale","scale":2.5}"#).await;
            send(&mut socket, r#"{"op":"setPollScale","scale":null}"#).await;
            // The account command is an ordered barrier after both configuration events.
            send(&mut socket, r#"{"op":"fetchAccount","command":1}"#).await;
            assert!(matches!(
                reply(&mut socket).await,
                Reply::Done { command: 1, .. }
            ));
            assert_eq!(*fixture.poll_scales.borrow(), [2.5, f64::INFINITY]);
            assert_eq!(
                fixture.observed.borrow().len(),
                1,
                "configuration is not a Command"
            );
            fixture.stop().await;
        })
        .await;
}

#[tokio::test]
async fn invalid_websocket_polling_controls_have_no_venue_effects() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        let mut socket = fixture.connect("a").await;
        frame(&mut socket).await;
        for json in [
            r#"{"op":"setPollScale","scale":0}"#,
            r#"{"op":"setPollScale","scale":-1}"#,
            r#"{"op":"setPollScale"}"#,
            r#"{"op":"setPollScale","scale":"4"}"#,
        ] {
            send(&mut socket, json).await;
            assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error == "invalid command"));
        }
        send(&mut socket, r#"{"op":"fetchAccount","command":1}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Done { command: 1, .. }));
        assert!(fixture.poll_scales.borrow().is_empty());
        assert_eq!(fixture.observed.borrow().len(), 1);
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn polling_deadlines_clamp_at_both_bounds_and_remain_cancelable() {
    LocalSet::new()
        .run_until(async {
            let fixture = Fixture::new(config()).await;
            let mut socket = fixture.connect("a").await;
            frame(&mut socket).await;
            // Scales of the venue's 1 s cadence: 1e308 ms, then 0.001 ms.
            send(&mut socket, r#"{"op":"setPollScale","scale":1e305}"#).await;
            send(&mut socket, r#"{"op":"fetchAccount","command":1}"#).await;
            assert!(
                matches!(reply(&mut socket).await, Reply::Done { command: 1, .. }),
                "host must continue serving commands around a huge deadline"
            );
            assert_eq!(*fixture.poll_intervals.borrow(), [120_000.]);
            send(&mut socket, r#"{"op":"setPollScale","scale":0.000001}"#).await;
            // Null emits ClearTimer for the bounded deadline, then a normal future
            // timer must still work. Account shutdown cancels that timer as usual.
            send(&mut socket, r#"{"op":"setPollScale","scale":null}"#).await;
            send(&mut socket, r#"{"op":"setPollScale","scale":2.5}"#).await;
            send(&mut socket, r#"{"op":"fetchAccount","command":2}"#).await;
            assert!(matches!(
                reply(&mut socket).await,
                Reply::Done { command: 2, .. }
            ));
            assert_eq!(
                *fixture.poll_intervals.borrow(),
                [120_000., 1_000., f64::INFINITY, 2500.]
            );
            fixture.stop().await;
        })
        .await;
}

#[tokio::test]
async fn socket_clients_cannot_inject_shared_public_data() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::new(config()).await;
        let mut socket = fixture.connect("a").await;
        frame(&mut socket).await;
        send(&mut socket, r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTCUSDT","bid":1}]}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Error { error } if error.contains("trusted host provider")));
        assert!(fixture.observed.borrow().is_empty());
        // Rejecting a provider control does not drop the authenticated account session.
        send(&mut socket, r#"{"op":"fetchAccount","command":7}"#).await;
        assert!(matches!(reply(&mut socket).await, Reply::Done { command: 7, error: None, .. }));
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn trusted_native_host_provider_updates_private_account_frames() {
    LocalSet::new().run_until(async {
        let fixture = Fixture::with_policy(config(), safe_cex_engine::PublicFeedPolicy::Shared).await;
        let mut socket = fixture.connect("a").await;
        frame(&mut socket).await;
        let safe_cex_engine::command::HostInput::PublicFeedUpdate(update) =
            safe_cex_engine::command::decode_host_input(r#"{"op":"publicFeedUpdate","tickers":[{"symbol":"BTCUSDT","bid":88,"ask":90}],"books":[{"symbol":"BTCUSDT","bids":[[88,2]],"asks":[[90,3]]}]}"#).unwrap() else { panic!("provider update") };
        fixture.a.0.borrow().as_ref().unwrap().send(HostEvent::PublicFeedUpdate(update)).unwrap();
        let next = frame(&mut socket).await;
        let book = sections(&next).find(|(kind, _)| *kind == section::BOOK).unwrap().1;
        let (_, bid, ask) = book_top(book).unwrap();
        assert_eq!((bid, ask), (88., 90.));
        assert!(fixture.observed.borrow().is_empty(), "provider ingress never dispatches a venue command");
        let mut late = fixture.connect("a").await;
        let snapshot = frame(&mut late).await;
        let book = sections(&snapshot).find(|(kind, _)| *kind == section::BOOK).unwrap().1;
        let (_, bid, ask) = book_top(book).unwrap();
        assert_eq!((bid, ask), (88., 90.));
        fixture.stop().await;
    }).await;
}

#[tokio::test]
async fn in_process_account_handle_snapshots_commands_and_disposes() {
    LocalSet::new().run_until(async {
        let tape = Tape::default();
        let mut book = Book::default();
        book.apply(BookSide::Bids, 99., 1.);
        let engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
            label: "a",
            book,
            observed: Rc::default(),
            pending: Vec::new(),
            received: Rc::default(),
            started: Rc::default(),
            poll_scales: Rc::default(),
            poll_intervals: Rc::default(),
            poll_timer: None,
        });
        let (handle, task) = safe_cex_native::spawn_account(Host::new(engine, tape.clone()), config()).unwrap();
        async fn next(link: &mut safe_cex_native::ClientLink) -> safe_cex_native::Outgoing {
            tokio::time::timeout(DEADLINE, link.outgoing.recv()).await.unwrap().unwrap()
        }
        let mut first = handle.join(1, false).await.unwrap();
        let safe_cex_native::Outgoing::Frame(snapshot) = next(&mut first).await else { panic!("snapshot first") };
        assert!(sections(&snapshot).any(|(kind, _)| kind == section::MARKETS));
        assert!(handle.input(1, "not json").is_err(), "malformed input returns a notice");
        assert!(handle.input(1, r#"{"op":"fetchAccount","command":7}"#).unwrap());
        let done = loop {
            if let safe_cex_native::Outgoing::Text(text) = next(&mut first).await { break text; }
        };
        assert!(matches!(serde_json::from_str::<Reply>(&done).unwrap(), Reply::Done { command: 7, result: Some(result), error: None } if result == "\"a\""));
        let markets = handle.read(|store| store.markets().len()).await;
        assert_eq!(markets, Some(1), "reads run against the engine store");
        // A second in-process client on the same account gets its own snapshot.
        let mut second = handle.join(2, false).await.unwrap();
        assert!(matches!(next(&mut second).await, safe_cex_native::Outgoing::Frame(_)));
        assert!(handle.join(2, false).await.is_err(), "duplicate client IDs are refused");
        handle.dispose();
        tokio::time::timeout(DEADLINE, task).await.unwrap().unwrap().unwrap();
        assert!(*first.canceled.borrow(), "dispose cancels clients");
        assert!(tape.0.borrow().as_ref().unwrap().is_closed(), "dispose drops the account transport");
        assert!(handle.join(3, false).await.is_err(), "a disposed account refuses joins");
    }).await;
}

#[tokio::test]
async fn a_deep_in_process_queue_survives_a_burst_the_default_peer_cannot() {
    LocalSet::new()
        .run_until(async {
            let tape = Tape::default();
            let mut book = Book::default();
            book.apply(BookSide::Bids, 99., 1.);
            let engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
                label: "a",
                book,
                observed: Rc::default(),
                pending: Vec::new(),
                received: Rc::default(),
                started: Rc::default(),
                poll_scales: Rc::default(),
                poll_intervals: Rc::default(),
                poll_timer: None,
            });
            let config = ServerConfig {
                flush_every: Duration::from_millis(1),
                ..config()
            };
            let (handle, _task) =
                safe_cex_native::spawn_account(Host::new(engine, tape.clone()), config).unwrap();
            let shallow = handle.join(1, false).await.unwrap();
            let deep = handle.join_with_capacity(2, false, 4096).await.unwrap();
            // Neither client reads while 40 book changes each flush a frame.
            for amount in 2..42 {
                tape.message(&amount.to_string());
                tokio::time::sleep(Duration::from_millis(3)).await;
            }
            assert!(
                *shallow.canceled.borrow(),
                "a 32-deep peer is dropped as slow"
            );
            assert!(
                !*deep.canceled.borrow(),
                "the deep in-process client is kept"
            );
            assert!(
                deep.outgoing.len() > 32,
                "every frame is still queued: {}",
                deep.outgoing.len()
            );
        })
        .await;
}

#[tokio::test]
async fn an_account_handle_reads_books_and_takes_a_trusted_public_feed() {
    LocalSet::new().run_until(async {
        let mut book = Book::default();
        book.apply(BookSide::Bids, 99., 1.);
        book.apply(BookSide::Asks, 101., 2.);
        let mut engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
            label: "a",
            book,
            observed: Rc::default(),
            pending: Vec::new(),
            received: Rc::default(),
            started: Rc::default(),
            poll_scales: Rc::default(),
            poll_intervals: Rc::default(),
            poll_timer: None,
        });
        engine.set_public_feed_policy(safe_cex_engine::PublicFeedPolicy::Shared).unwrap();
        let (handle, _task) = safe_cex_native::spawn_account(Host::new(engine, Tape::default()), config()).unwrap();
        let books = handle.books(vec!["BTCUSDT".into(), "UNKNOWN".into()]).await.unwrap();
        assert!(books.is_empty(), "a shared-feed account has no book until the provider sends one");
        let safe_cex_engine::command::HostInput::PublicFeedUpdate(update) =
            safe_cex_engine::command::decode_host_input(r#"{"op":"publicFeedUpdate","books":[{"symbol":"BTCUSDT","bids":[[88,2]],"asks":[[90,3]]}]}"#).unwrap() else { panic!("update") };
        assert!(handle.public_feed(update));
        let mut fed = Vec::new();
        for _ in 0..50 {
            fed = handle.books(vec!["BTCUSDT".into()]).await.unwrap();
            if fed.first().is_some_and(|book| book.1 == vec![(88., 2.)]) { break; }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        assert_eq!(fed, vec![("BTCUSDT".to_owned(), vec![(88., 2.)], vec![(90., 3.)])], "the shared feed set the book");
    }).await;
}

#[tokio::test]
async fn a_direct_account_handle_reads_its_venue_books() {
    LocalSet::new()
        .run_until(async {
            let mut book = Book::default();
            book.apply(BookSide::Bids, 99., 1.);
            book.apply(BookSide::Asks, 101., 2.);
            let engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
                label: "a",
                book,
                observed: Rc::default(),
                pending: Vec::new(),
                received: Rc::default(),
                started: Rc::default(),
                poll_scales: Rc::default(),
                poll_intervals: Rc::default(),
                poll_timer: None,
            });
            let (handle, _task) =
                safe_cex_native::spawn_account(Host::new(engine, Tape::default()), config())
                    .unwrap();
            let books = handle
                .books(vec!["BTCUSDT".into(), "UNKNOWN".into()])
                .await
                .unwrap();
            assert_eq!(
                books,
                vec![("BTCUSDT".to_owned(), vec![(99., 1.)], vec![(101., 2.)])]
            );
        })
        .await;
}
