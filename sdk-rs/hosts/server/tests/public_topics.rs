use safe_cex_engine::ControlFrame;
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthMode, AuthRecovery, ControlPolicy, EncodedBatch, FeedId, FeedPolicy, Interest,
    Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
use safe_cex_engine::{
    Command, Cx, Engine, Event, Response, SocketChannel, SocketId, TimerId, Venue,
};
use safe_cex_exchange_server::auth::Verifier;
use safe_cex_exchange_server::public_topics::PublicTopics;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

struct TopicsVenue<const MANAGED: bool = true> {
    feed: Option<FeedId>,
    topics: Vec<String>,
}
impl<const MANAGED: bool> Venue for TopicsVenue<MANAGED> {
    type Pending = ();
    fn managed_sockets() -> bool {
        MANAGED
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if !MANAGED {
            cx.connect("wss://fixture.invalid");
            return;
        }
        let feed = cx
            .register_socket_feed(FeedPolicy {
                role: SocketChannel::Public,
                supervision: Config {
                    ping_interval_ms: None,
                    ..Config::default()
                },
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::None,
                subscription_interval_ms: 0.,
                close_floors: &[],
                data_silence: None,
            })
            .unwrap();
        self.feed = Some(feed);
        cx.set_socket_interests(
            feed,
            self.topics
                .iter()
                .cloned()
                .map(Interest::from_wire_args)
                .collect(),
        )
        .unwrap();
        cx.connect_socket(feed, "wss://fixture.invalid").unwrap();
    }
    fn on_socket_event(
        &mut self,
        _: &mut Cx<'_, ()>,
        _: LifecycleEvent,
    ) -> Result<(), SocketError> {
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &safe_cex_engine::Store,
        _: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for item in items {
            emit(EncodedBatch {
                text: format!("{subscribe}:{}", item.wire_args),
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
    fn on_open(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId) {
        if !MANAGED {
            for topic in &self.topics {
                cx.send(socket, ControlFrame::subscription(topic));
            }
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, text: &str) {
        if !MANAGED {
            cx.send(socket, ControlFrame::subscription(text));
            return;
        }
        if text == "stop" {
            cx.stop_socket_feed(self.feed.unwrap());
        } else {
            cx.set_socket_interests(self.feed.unwrap(), vec![]).unwrap();
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn topics() -> Vec<String> {
    vec![
        json!({"type":"allMids"}),
        json!({"type":"fastAssetCtxs"}),
        json!({"type":"l2Book","coin":"private-symbol-BTC"}),
        json!({"type":"bbo","coin":"private-symbol-BTC"}),
        json!({"type":"l2Book","coin":"ETH"}),
        json!({"type":"bbo","coin":"ETH"}),
        json!({"type":"candle","coin":"private-symbol-BTC","interval":"1m"}),
        json!({"type":"candle","coin":"private-symbol-BTC","interval":"5m"}),
    ]
    .into_iter()
    .map(|subscription| json!({"method":"subscribe","subscription":subscription}).to_string())
    .collect()
}
fn engine(tracker: &Arc<PublicTopics>, venue: &str, topics: Vec<String>) -> Engine<TopicsVenue> {
    let mut engine =
        safe_cex_engine::test_support::seeded_engine(TopicsVenue { feed: None, topics });
    engine.set_interest_observer(tracker.observer(venue));
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    assert_eq!(
        tracker.snapshot().venues[venue].total,
        0,
        "desired and queued topics are not upstream yet"
    );
    engine
}
fn open(engine: &mut Engine<TopicsVenue>) {
    engine.handle(
        1.,
        Event::WsOpened {
            socket: SocketId(1),
        },
    );
    safe_cex_engine::test_support::prepared_now(engine);
}

#[test]
fn counts_follow_actual_handoff_and_peak_survives_unsubscribe_close_stop_and_rebuild() {
    let tracker = Arc::new(PublicTopics::default());
    let mut e = engine(&tracker, "hyperliquid", topics());
    open(&mut e);
    let counts = tracker.snapshot();
    let c = &counts.venues["hyperliquid"];
    assert_eq!(
        (c.book, c.candles, c.other, c.total, c.peak_total),
        (2, 2, 2, 8, 8)
    );
    assert!(c.peak_at > 0 && c.peak_at <= counts.checked_at);
    let peak_at = c.peak_at;
    e.handle(
        2.,
        Event::WsMessage {
            socket: SocketId(1),
            text: "unsubscribe",
        },
    );
    safe_cex_engine::test_support::prepared_now(&mut e);
    let c = &tracker.snapshot().venues["hyperliquid"];
    assert_eq!(
        (c.book, c.candles, c.other, c.total, c.peak_total, c.peak_at),
        (0, 0, 0, 0, 8, peak_at)
    );
    drop(e);
    assert!(tracker.snapshot().venues.is_empty());
    for stop in [false, true] {
        let mut e = engine(&tracker, "hyperliquid", topics()[..3].to_vec());
        open(&mut e);
        assert_eq!(tracker.snapshot().venues["hyperliquid"].total, 3);
        if stop {
            e.handle(
                2.,
                Event::WsMessage {
                    socket: SocketId(1),
                    text: "stop",
                },
            );
        } else {
            e.handle(
                2.,
                Event::WsClosed {
                    socket: SocketId(1),
                    code: 1006,
                },
            );
        }
        safe_cex_engine::test_support::prepared_now(&mut e);
        let c = &tracker.snapshot().venues["hyperliquid"];
        assert_eq!((c.total, c.peak_total, c.peak_at), (0, 8, peak_at));
    }
    // An engine dropped while subscribed also releases its counts.
    let mut e = engine(&tracker, "hyperliquid", topics());
    open(&mut e);
    drop(e);
    assert!(tracker.snapshot().venues.is_empty());
}

#[test]
fn spot_precision_tiers_are_one_book_and_unknown_venues_are_other() {
    let tracker = Arc::new(PublicTopics::default());
    let topics: Vec<_> = [None, Some(2), Some(3), Some(4), Some(5)].into_iter().map(|figures| {
        json!({"method":"subscribe","subscription":{"type":"l2Book","coin":"@1","nSigFigs":figures}}).to_string()
    }).collect();
    let mut spot = engine(&tracker, "hyperliquidspot", topics.clone());
    open(&mut spot);
    let c = &tracker.snapshot().venues["hyperliquidspot"];
    assert_eq!((c.book, c.candles, c.other, c.total), (1, 0, 0, 5));
    let mut unknown = engine(&tracker, "bybit", topics);
    open(&mut unknown);
    let c = &tracker.snapshot().venues["bybit"];
    assert_eq!((c.book, c.candles, c.other, c.total), (0, 0, 5, 5));
}

#[test]
fn failed_construction_releases_registration() {
    let tracker = Arc::new(PublicTopics::default());
    let observer = tracker.observer("hyperliquid");
    let clone = observer.clone();
    drop(observer);
    assert!(
        tracker.snapshot().venues.is_empty(),
        "a pending constructor is not a live engine"
    );
    drop(clone);
    assert!(tracker.snapshot().venues.is_empty());
}

#[tokio::test]
async fn route_is_local_count_only_small_and_unforwarded() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::unsigned(),
    );
    let mut e = engine(&app.registry.public_topics, "hyperliquid", topics());
    open(&mut e);
    let socket = app.registry.user_drains.socket("private-user-sentinel");
    let address = support::serve(app.clone()).await;
    let url = format!("http://{address}/admin/public-topics");
    let client = reqwest::Client::new();
    for body in ["", "{}"] {
        let response = client.post(&url).body(body).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(
            !response
                .headers()
                .contains_key("access-control-allow-origin")
        );
        let text = response.text().await.unwrap();
        assert!(!text.contains("private-user-sentinel") && !text.contains("private-symbol-BTC"));
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert_eq!(value["venues"]["hyperliquid"].as_object().unwrap().len(), 6);
        assert_eq!(value["venues"]["hyperliquid"]["total"], 8);
        assert_eq!(value["venues"]["hyperliquid"]["peakTotal"], 8);
        assert!(value["checkedAt"].as_u64().unwrap() > 0);
    }
    for header in [
        "cf-connecting-ip",
        "cf-ray",
        "cdn-loop",
        "x-forwarded-for",
        "x-real-ip",
        "forwarded",
    ] {
        assert_eq!(
            client
                .post(&url)
                .header(header, "127.0.0.1")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    for body in [
        "[]",
        "null",
        "{broken",
        r#"{"userId":"private-user-sentinel"}"#,
    ] {
        assert_eq!(
            client.post(&url).body(body).send().await.unwrap().status(),
            400
        );
    }
    assert_eq!(client.get(&url).send().await.unwrap().status(), 405);
    assert_eq!(
        client
            .post(&url)
            .body(" ".repeat(1025))
            .send()
            .await
            .unwrap()
            .status(),
        413
    );
    // Supply a real non-local transport peer through ConnectInfo, as the on-box guard sees it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let remote = "100.64.1.2:5000".parse::<std::net::SocketAddr>().unwrap();
    let router = safe_cex_exchange_server::http::router(app)
        .layer(axum::Extension(axum::extract::ConnectInfo(remote)));
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    assert_eq!(
        client
            .post(format!("http://{address}/admin/public-topics"))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    server.abort();
    drop(socket);
}

#[derive(Clone, Default)]
struct Tape;
impl safe_cex_native::Transport for Tape {
    fn connect(
        &self,
        socket: SocketId,
        _: String,
        events: safe_cex_native::Events,
    ) -> tokio::sync::mpsc::UnboundedSender<String> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        events
            .send(safe_cex_native::HostEvent::WsOpened(socket))
            .unwrap();
        tokio::task::spawn_local(async move { while receiver.recv().await.is_some() {} });
        sender
    }
    fn request(&self, _: safe_cex_engine::HttpRequest, _: safe_cex_native::Events) {
        panic!("fixture has no HTTP");
    }
}

async fn native_account<const MANAGED: bool>(
    venue: &str,
    wire_topics: Vec<String>,
    expected: usize,
) {
    let tracker = Arc::new(PublicTopics::default());
    let config = safe_cex_native::ServerConfig {
        interest_observer: Some(tracker.observer(venue)),
        ..Default::default()
    };
    let engine = safe_cex_engine::test_support::seeded_engine(TopicsVenue::<MANAGED> {
        feed: None,
        topics: wire_topics,
    });
    let host = safe_cex_native::Host::new(engine, Tape);
    let events = host.event_sender();
    let (handle, task) = safe_cex_native::spawn_account(host, config).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while tracker.snapshot().venues[venue].total != expected {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    if !MANAGED {
        events
            .send(safe_cex_native::HostEvent::WsMessage(
                SocketId(1),
                r#"{"type":"unsubscribe","channel":"order_book/1"}"#.into(),
            ))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while tracker.snapshot().venues[venue].total != 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        events
            .send(safe_cex_native::HostEvent::WsClosed(SocketId(1), 1006))
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while tracker.snapshot().venues[venue].total != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(tracker.snapshot().venues[venue].peak_total, expected);
    }
    handle.dispose();
    task.await.unwrap().unwrap();
    assert!(
        tracker.snapshot().venues.is_empty(),
        "a retained handle cannot retain stale topic counts"
    );
}

#[tokio::test]
async fn native_account_config_observes_managed_and_legacy_handoff_and_teardown() {
    tokio::task::LocalSet::new()
        .run_until(async {
            native_account::<true>("hyperliquid", topics(), 8).await;
            native_account::<false>(
                "lighter",
                vec![
                    r#"{"type":"subscribe","channel":"market_stats/all"}"#.into(),
                    r#"{"type":"subscribe","channel":"order_book/1"}"#.into(),
                ],
                2,
            )
            .await;
        })
        .await;
}
