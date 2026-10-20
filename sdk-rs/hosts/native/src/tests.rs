use super::*;
use safe_cex_engine::{Cx, HttpRequest, LogLevel, Response, book::Book};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
struct RecordingVenue(Vec<(f64, SocketId, String)>);

impl Venue for RecordingVenue {
    type Pending = ();

    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, text: &str) {
        self.0.push((cx.now, socket, text.into()));
        cx.send(socket, text);
        cx.log(LogLevel::Info, text);
        if text == "deactivate" {
            cx.deactivate();
        }
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, data: &[u8]) {
        let text = format!("binary:{data:?}");
        self.0.push((cx.now, socket, text.clone()));
        cx.send(socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, _: u16) {
        self.0.push((cx.now, socket, "closed".into()));
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_tick_interval(&mut self, cx: &mut Cx<'_, ()>, interval_ms: f64) {
        self.0
            .push((cx.now, SocketId(0), format!("tick:{interval_ms}")));
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        self.0.push((cx.now, SocketId(0), "command".into()));
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct NoTransport;

impl Transport for NoTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("unexpected socket")
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("unexpected request")
    }
}

fn host() -> Host<RecordingVenue, NoTransport> {
    Host::new(Engine::new(RecordingVenue::default()), NoTransport)
}

fn dispatch_next(host: &mut Host<RecordingVenue, NoTransport>) {
    let event = host
        .pending
        .take()
        .unwrap_or_else(|| host.inbox.try_recv().unwrap());
    host.dispatch_ready(event, MAX_BATCH_MESSAGES);
}

#[test]
fn decoded_messages_keep_their_raw_arrival_time() {
    let mut host = host();
    host.events
        .send_at(HostEvent::WsMessage(SocketId(1), "decoded".into()), 17.0)
        .unwrap();
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0,
        [(17.0, SocketId(1), "decoded".into())]
    );
}

#[test]
fn mixed_websocket_bursts_preserve_timestamps_actions_and_close_barriers() {
    let mut host = host();
    for (event, at) in [
        (HostEvent::WsMessage(SocketId(1), "first".into()), 11.0),
        (HostEvent::WsBinary(SocketId(2), vec![0, 255]), 12.5),
        (HostEvent::WsMessage(SocketId(1), "last".into()), 13.0),
        (HostEvent::WsClosed(SocketId(2), 1000), 14.0),
        (HostEvent::WsBinary(SocketId(1), vec![]), 15.0),
    ] {
        host.events.send_at(event, at).unwrap();
    }
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0,
        [
            (11.0, SocketId(1), "first".into()),
            (12.5, SocketId(2), "binary:[0, 255]".into()),
            (13.0, SocketId(1), "last".into()),
        ]
    );
    let sent: Vec<_> = host
        .engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsSend { socket, text } => Some((socket, text)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        [
            (SocketId(1), "first".into()),
            (SocketId(2), "binary:[0, 255]".into()),
            (SocketId(1), "last".into())
        ]
    );
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last(),
        Some(&(14.0, SocketId(2), "closed".into()))
    );
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last(),
        Some(&(15.0, SocketId(1), "binary:[]".into()))
    );
}

#[test]
fn tick_configuration_is_a_timestamped_barrier_between_mixed_bursts() {
    let mut host = host();
    let events = host.event_sender();
    for (event, at) in [
        (HostEvent::WsMessage(SocketId(1), "first".into()), 11.0),
        (HostEvent::WsBinary(SocketId(2), vec![255]), 12.0),
        (HostEvent::TickIntervalChanged(f64::INFINITY), 13.0),
        (HostEvent::WsMessage(SocketId(1), "after".into()), 14.0),
    ] {
        events.send_at(event, at).unwrap();
    }
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 2);
    assert!(matches!(
        host.pending.as_ref().unwrap().event,
        QueuedInput::Ready(HostEvent::TickIntervalChanged(interval)) if interval.is_infinite()
    ));
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last(),
        Some(&(13.0, SocketId(0), "tick:inf".into()))
    );
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last(),
        Some(&(14.0, SocketId(1), "after".into()))
    );
}

#[test]
fn mixed_websocket_bursts_share_count_and_byte_limits() {
    let mut host = host();
    host.events
        .send(HostEvent::WsMessage(
            SocketId(1),
            "x".repeat(MAX_BATCH_BYTES - 1),
        ))
        .unwrap();
    host.events
        .send(HostEvent::WsBinary(SocketId(2), vec![0, 1]))
        .unwrap();
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 1);
    assert!(matches!(
        host.pending.as_ref().unwrap().event,
        QueuedInput::Ready(HostEvent::WsBinary(..))
    ));
    for index in 0..MAX_BATCH_MESSAGES {
        let event = if index % 2 == 0 {
            HostEvent::WsMessage(SocketId(1), "text".into())
        } else {
            HostEvent::WsBinary(SocketId(2), vec![255])
        };
        host.events.send(event).unwrap();
    }
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 1 + MAX_BATCH_MESSAGES);
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 2 + MAX_BATCH_MESSAGES);
}

#[test]
fn messages_retain_arrival_time_socket_order_and_close_barriers() {
    static TIME: AtomicU64 = AtomicU64::new(0);
    fn clock() -> f64 {
        TIME.load(Ordering::Relaxed) as f64
    }
    let mut host = host();
    host.events.clock = clock;
    host.clock = clock;
    for (now, socket, text) in [(11, 1, "first"), (12, 2, "second")] {
        TIME.store(now, Ordering::Relaxed);
        host.events
            .send(HostEvent::WsMessage(SocketId(socket), text.into()))
            .unwrap();
    }
    host.events
        .send(HostEvent::WsClosed(SocketId(1), 1000))
        .unwrap();
    TIME.store(13, Ordering::Relaxed);
    host.events
        .send(HostEvent::WsMessage(SocketId(2), "last".into()))
        .unwrap();
    TIME.store(999, Ordering::Relaxed);

    let (outgoing, mut sent) = mpsc::unbounded_channel();
    host.sockets.insert(SocketId(1), outgoing.clone());
    host.sockets.insert(SocketId(2), outgoing);
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0,
        [
            (11.0, SocketId(1), "first".into()),
            (12.0, SocketId(2), "second".into())
        ]
    );
    host.execute(&mut |_| {});
    assert_eq!(sent.try_recv().unwrap(), "first");
    assert_eq!(sent.try_recv().unwrap(), "second");
    assert!(sent.try_recv().is_err());

    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last().unwrap(),
        &(12.0, SocketId(1), "closed".into())
    );
    assert!(!host.sockets.contains_key(&SocketId(1)));
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0.last().unwrap(),
        &(13.0, SocketId(2), "last".into())
    );
}

#[test]
fn ready_bursts_stop_at_count_and_byte_limits_without_losing_messages() {
    let mut host = host();
    for n in 0..MAX_BATCH_MESSAGES + 1 {
        host.events
            .send(HostEvent::WsMessage(SocketId(1), n.to_string()))
            .unwrap();
    }
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), MAX_BATCH_MESSAGES);
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), MAX_BATCH_MESSAGES + 1);
    for _ in 0..2 {
        host.events
            .send(HostEvent::WsMessage(
                SocketId(1),
                "x".repeat(MAX_BATCH_BYTES / 2 + 1),
            ))
            .unwrap();
    }
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), MAX_BATCH_MESSAGES + 2);
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), MAX_BATCH_MESSAGES + 3);
    assert!(host.pending.is_none());
    assert!(host.inbox.try_recv().is_err());
}

#[tokio::test]
async fn external_barriers_drain_a_fixed_prefix_without_starvation() {
    let mut host = host();
    for n in 0..MAX_BATCH_MESSAGES + 1 {
        host.events
            .send(HostEvent::WsMessage(SocketId(1), n.to_string()))
            .unwrap();
    }
    host.events
        .send(HostEvent::WsClosed(SocketId(1), 1000))
        .unwrap();
    host.events
        .send(HostEvent::WsMessage(SocketId(2), "before-command".into()))
        .unwrap();
    let events = host.events.clone();
    let mut added = false;
    host.drain_ready_prefix(&mut |_| {
        if !added {
            added = true;
            events
                .send(HostEvent::WsMessage(SocketId(3), "after-command".into()))
                .unwrap();
        }
    })
    .await;
    let records = &host.engine.venue().0;
    assert_eq!(records.len(), MAX_BATCH_MESSAGES + 3);
    assert_eq!(records[MAX_BATCH_MESSAGES + 1].2, "closed");
    assert_eq!(records[MAX_BATCH_MESSAGES + 2].2, "before-command");
    assert!(host.pending.is_none());
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.last().unwrap().2, "after-command");
    assert!(host.inbox.try_recv().is_err());
}

#[test]
fn an_oversized_message_does_not_capture_the_next_message() {
    let mut host = host();
    host.events
        .send(HostEvent::WsMessage(
            SocketId(1),
            "x".repeat(MAX_BATCH_BYTES + 1),
        ))
        .unwrap();
    host.events
        .send(HostEvent::WsMessage(SocketId(2), "small".into()))
        .unwrap();
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 1);
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.last().unwrap().2, "small");
}

#[tokio::test]
async fn command_time_precedes_arrivals_while_its_ready_prefix_drains() {
    static TIME: AtomicU64 = AtomicU64::new(70);
    fn clock() -> f64 {
        TIME.load(Ordering::Relaxed) as f64
    }
    let mut host = host();
    host.clock = clock;
    host.events.clock = clock;
    host.events
        .send(HostEvent::WsMessage(SocketId(1), "before".into()))
        .unwrap();
    TIME.store(80, Ordering::Relaxed);
    let events = host.events.clone();
    host.dispatch_command(
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        &mut |_| {
            TIME.store(90, Ordering::Relaxed);
            events
                .send(HostEvent::WsMessage(SocketId(1), "later".into()))
                .unwrap();
            TIME.store(100, Ordering::Relaxed);
        },
    )
    .await;
    dispatch_next(&mut host);
    assert_eq!(
        host.engine
            .venue()
            .0
            .iter()
            .map(|(time, _, text)| (*time, text.as_str()))
            .collect::<Vec<_>>(),
        [(70.0, "before"), (80.0, "command"), (90.0, "later")]
    );
}

#[tokio::test]
async fn standalone_event_channel_preserves_delivery_and_receiver_drop_cancellation() {
    let (events, mut received) = Events::channel();
    assert!(matches!(
        received.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    events
        .send_at(HostEvent::WsOpened(SocketId(7)), 12.0)
        .unwrap();
    events
        .send(HostEvent::WsMessage(SocketId(7), "arrival".into()))
        .unwrap();
    assert!(matches!(
        received.recv().await,
        Some(HostEvent::WsOpened(SocketId(7)))
    ));
    assert!(
        matches!(received.try_recv(), Ok(HostEvent::WsMessage(SocketId(7), text)) if text == "arrival")
    );
    assert!(!events.sender.is_closed());
    drop(received);
    events.closed().await;
    assert!(events.send(HostEvent::WsClosed(SocketId(7), 1000)).is_err());
}

#[test]
fn deactivation_notices_preserve_action_order_after_a_timestamped_batch() {
    let mut host = host();
    for (now, text) in [(1.0, "before"), (2.0, "deactivate"), (3.0, "after")] {
        host.events
            .send_at(HostEvent::WsMessage(SocketId(1), text.into()), now)
            .unwrap();
    }
    dispatch_next(&mut host);
    let mut notices = Vec::new();
    host.execute(&mut |notice| notices.push(notice));
    assert_eq!(
        notices,
        [
            Notice::Log {
                level: LogLevel::Info,
                message: "before".into()
            },
            Notice::Log {
                level: LogLevel::Info,
                message: "deactivate".into()
            },
            Notice::Deactivate,
            Notice::Log {
                level: LogLevel::Info,
                message: "after".into()
            },
        ]
    );
    assert_eq!(
        host.engine
            .venue()
            .0
            .iter()
            .map(|(now, _, _)| *now)
            .collect::<Vec<_>>(),
        [1.0, 2.0, 3.0]
    );
}

// The decoder gate recreates a synchronous inflate on one runtime worker while
// a second producer reports a later control event. No scheduling sleeps are used.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn socket_decode_reserves_its_inbox_position_before_later_control_events() {
    for actor in [false, true] {
        let mut host = host();
        let (observer, mut receiver) = Events::channel();
        let events = if actor { host.event_sender() } else { observer };
        let (entered, decoding) = tokio::sync::oneshot::channel();
        let (release, released) = std::sync::mpsc::channel();
        let control = events.clone();
        let second_worker = tokio::spawn(async move {
            decoding.await.unwrap();
            control
                .send_at(HostEvent::TickIntervalChanged(18.0), 18.0)
                .unwrap();
            release.send(()).unwrap();
        });
        let producer = tokio::spawn(async move {
            assert!(events.send_deferred_at(17.0, None, move || {
                entered.send(()).unwrap();
                released.recv().unwrap();
                Some(HostEvent::WsMessage(SocketId(1), "decoded".into()))
            }));
        });
        producer.await.unwrap();
        if actor {
            dispatch_next(&mut host);
            dispatch_next(&mut host);
            assert_eq!(
                host.engine.venue().0,
                [
                    (17.0, SocketId(1), "decoded".into()),
                    (18.0, SocketId(0), "tick:18".into()),
                ]
            );
        } else {
            assert!(
                matches!(receiver.recv().await, Some(HostEvent::WsMessage(SocketId(1), text)) if text == "decoded")
            );
            assert!(matches!(
                receiver.recv().await,
                Some(HostEvent::TickIntervalChanged(18.0))
            ));
        }
        second_worker.await.unwrap();
    }
}

#[test]
fn deferred_decoded_byte_limits_cache_once_and_keep_later_message_separate() {
    let mut host = host();
    let decoded = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    host.events
        .send_at(HostEvent::WsMessage(SocketId(1), "prefix".into()), 11.0)
        .unwrap();
    let calls = decoded.clone();
    assert!(host.events.send_deferred_at(12.0, None, move || {
        calls.fetch_add(1, Ordering::SeqCst);
        Some(HostEvent::WsMessage(
            SocketId(1),
            "x".repeat(MAX_BATCH_BYTES + 1),
        ))
    }));
    host.events
        .send_at(HostEvent::WsMessage(SocketId(1), "after".into()), 13.0)
        .unwrap();
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 1);
    assert_eq!(decoded.load(Ordering::SeqCst), 1);
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0.len(), 2);
    assert_eq!(host.engine.venue().0[1].0, 12.0);
    assert_eq!(host.engine.venue().0[1].2.len(), MAX_BATCH_BYTES + 1);
    assert_eq!(
        decoded.load(Ordering::SeqCst),
        1,
        "pending must not decode twice"
    );
    dispatch_next(&mut host);
    assert_eq!(
        host.engine.venue().0[2],
        (13.0, SocketId(1), "after".into())
    );
}

#[tokio::test]
async fn suppressed_inputs_count_toward_fairness_and_the_captured_prefix() {
    let mut host = host();
    for _ in 0..MAX_BATCH_MESSAGES + 3 {
        assert!(host.events.send_deferred_at(17.0, None, || None));
    }
    let first = host.inbox.try_recv().unwrap();
    assert_eq!(
        host.dispatch_ready(first, MAX_BATCH_MESSAGES),
        MAX_BATCH_MESSAGES
    );
    assert_eq!(host.inbox.len(), 3);
    assert!(host.engine.venue().0.is_empty());
    let late = host.events.clone();
    assert!(host.events.send_deferred_at(18.0, None, move || {
        late.send_at(HostEvent::WsMessage(SocketId(1), "late".into()), 19.0)
            .unwrap();
        Some(HostEvent::WsMessage(SocketId(1), "captured".into()))
    }));
    host.drain_ready_prefix(&mut |_| {}).await;
    assert_eq!(
        host.engine.venue().0,
        [(18.0, SocketId(1), "captured".into())]
    );
    assert_eq!(
        host.inbox.len(),
        1,
        "new arrivals cannot extend a control barrier"
    );
    dispatch_next(&mut host);
    assert_eq!(host.engine.venue().0[1], (19.0, SocketId(1), "late".into()));
}

#[test]
fn locally_canceled_decoded_pending_input_is_skipped_without_decoding_twice() {
    let mut host = host();
    let (outgoing, _pending) = mpsc::unbounded_channel();
    let decoded = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    host.events
        .send_at(HostEvent::WsMessage(SocketId(1), "prefix".into()), 11.0)
        .unwrap();
    let calls = decoded.clone();
    assert!(
        host.events
            .send_deferred_at(12.0, Some(outgoing.downgrade()), move || {
                calls.fetch_add(1, Ordering::SeqCst);
                Some(HostEvent::WsMessage(
                    SocketId(1),
                    "x".repeat(MAX_BATCH_BYTES + 1),
                ))
            })
    );
    host.events
        .send_at(HostEvent::WsMessage(SocketId(2), "live".into()), 13.0)
        .unwrap();
    dispatch_next(&mut host);
    assert_eq!(decoded.load(Ordering::SeqCst), 1);
    assert_eq!(host.engine.venue().0.len(), 1);
    drop(outgoing); // An action drain can locally close this socket between bursts.
    dispatch_next(&mut host);
    assert_eq!(decoded.load(Ordering::SeqCst), 1);
    assert_eq!(
        host.engine.venue().0,
        [
            (11.0, SocketId(1), "prefix".into()),
            (13.0, SocketId(2), "live".into()),
        ]
    );
}

#[derive(Default)]
struct SigningDeadlineVenue {
    message_times: Vec<f64>,
    failures: Vec<(f64, String)>,
}

impl Venue for SigningDeadlineVenue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let id = cx.request(
            safe_cex_engine::Method::Post,
            "https://venue.invalid/orders".into(),
            vec![("SIGNATURE", "placeholder".into())],
            None,
            (),
        );
        cx.sign_request(
            id,
            safe_cex_engine::PayloadSignRequest {
                exchange_name: "paradex",
                key: "public-key".into(),
                payload: "0x123".into(),
                scheme: "stark",
                encoding: "felt-pair",
            },
            safe_cex_engine::SignatureTarget::Header("SIGNATURE"),
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: &str) {
        self.message_times.push(cx.now);
        cx.log(LogLevel::Info, "prior WS action");
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        panic!("signing failure must use on_sign_failed");
    }
    fn on_sign_failed(&mut self, cx: &mut Cx<'_, ()>, _: (), error: &str) {
        self.failures.push((cx.now, error.into()));
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn signing_dispatch_charges_queue_delay_after_prior_ws_actions() {
    static TIME: AtomicU64 = AtomicU64::new(0);
    fn clock() -> f64 {
        TIME.load(Ordering::Relaxed) as f64
    }
    for dispatched_at in [19_500, 20_000, 25_000] {
        let mut host = Host::new(Engine::new(SigningDeadlineVenue::default()), NoTransport);
        host.clock = clock;
        host.events.clock = clock;
        host.engine.start(0.0);
        let actions = host.engine.drain_actions().collect::<Vec<_>>();
        let request = match actions.as_slice() {
            [
                Action::SignPayload {
                    request,
                    timeout_ms: 20_000,
                    ..
                },
            ] => *request,
            _ => panic!("startup must hold the HTTP request for signing"),
        };
        host.events
            .send_at(HostEvent::WsMessage(SocketId(1), "prior".into()), 17_000.0)
            .unwrap();
        let signed = || {
            HostEvent::PayloadSigned(
                request,
                PayloadSignerResult {
                    signature: "signed".into(),
                    headers: vec![],
                },
            )
        };
        host.events.send_at(signed(), 18_000.0).unwrap();
        TIME.store(18_000, Ordering::Relaxed);
        let first = host.inbox.try_recv().unwrap();
        host.dispatch_ready(first, MAX_BATCH_MESSAGES);
        assert_eq!(host.engine.venue().message_times, [17_000.0]);
        assert!(
            host.pending.is_some(),
            "signing is a barrier after the WS burst"
        );
        let mut notices = 0;
        host.execute(&mut |_| {
            notices += 1;
            TIME.store(dispatched_at, Ordering::Relaxed);
        });
        assert_eq!(notices, 1);
        let completion = host.pending.take().unwrap();
        host.dispatch_ready(completion, MAX_BATCH_MESSAGES);
        let actions = host.engine.drain_actions().collect::<Vec<_>>();
        if dispatched_at < 20_000 {
            assert!(host.engine.venue().failures.is_empty());
            assert!(matches!(actions.as_slice(), [Action::Http(http)]
                if http.id == request && http.timeout_ms == Some(500)
                && http.headers.as_slice() == [("SIGNATURE", String::from("signed"))]));
        } else {
            assert!(actions.is_empty(), "expired signing must never emit HTTP");
            assert_eq!(
                host.engine.venue().failures,
                [(dispatched_at as f64, "Payload signing timed out".into())]
            );
        }
        host.events.send_at(signed(), 18_000.0).unwrap();
        let duplicate = host.inbox.try_recv().unwrap();
        host.dispatch_ready(duplicate, MAX_BATCH_MESSAGES);
        assert!(
            !host.engine.has_actions(),
            "duplicate completion cannot resume HTTP"
        );
        assert_eq!(
            host.engine.venue().failures.len(),
            usize::from(dispatched_at >= 20_000)
        );
    }
}
