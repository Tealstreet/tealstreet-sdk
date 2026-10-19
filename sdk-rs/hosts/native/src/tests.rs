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
    }
    fn on_close(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, _: u16) {
        self.0.push((cx.now, socket, "closed".into()));
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
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
