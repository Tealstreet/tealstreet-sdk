use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Context, Error, Operation, Policy, ReconciliationData, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, HttpRequest, Response, SocketChannel, SocketSpec};
use std::sync::{Arc, Mutex};

type WriterSlot = Arc<Mutex<Option<Arc<dyn WebSocketWriter>>>>;

#[derive(Default)]
struct Probe {
    socket: Option<SocketId>,
    url: Option<String>,
    deny: bool,
    panic_release: bool,
    retire_writer: Option<WriterSlot>,
    actual_releases: usize,
    retire_after_release: bool,
    releases: usize,
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        self.socket = Some(
            cx.connect(
                SocketSpec::new(self.url.as_deref().unwrap_or("wss://venue.test/orders"))
                    .channel(SocketChannel::Business),
            ),
        );
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, command: Command) {
        let Command::SetLeverage { command, .. } = command else {
            return;
        };
        let context = cx
            .http_context(
                Scope::Command(command),
                Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let write = cx
            .websocket_write(context.clone(), self.socket.unwrap(), "setting".into())
            .unwrap();
        cx.send_websocket_write(write, None, context).unwrap();
    }
    fn capture_http_reconciliation(&self, _: &Context) -> Result<ReconciliationData, Error> {
        ReconciliationData::new("setting".into())
    }
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::Mutable(|this, _, _| {
            this.releases += 1;
            if !this.retire_after_release
                && let Some(writer) = &this.retire_writer
            {
                writer.lock().unwrap().as_ref().unwrap().retire();
            }
            assert!(!this.panic_release, "venue release panic");
            Ok(if this.deny {
                Admission::Denied
            } else {
                Admission::Unscheduled
            })
        })
    }
    fn on_signing_release(
        &mut self,
        _: safe_cex_engine::SigningTarget<'_, Context>,
        _: safe_cex_engine::SigningRelease,
    ) {
        self.actual_releases += 1;
        if self.retire_after_release {
            self.retire_writer
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .retire();
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Context>,
        context: Context,
        _: Error,
        error: VenueError,
    ) {
        cx.fail_context(&context, error).unwrap();
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Context>, _: Context, _: Response<'_>) {}
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: &str) {
        cx.fail_context(&context, VenueError::new(error)).unwrap();
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Captured {
    sent: Arc<Mutex<Vec<String>>>,
    unsupported: bool,
    busy: bool,
    send_error: bool,
}
struct CapturedWriter {
    sent: Arc<Mutex<Vec<String>>>,
    busy: bool,
    send_error: bool,
}
impl WebSocketWriter for CapturedWriter {
    fn try_write(
        &self,
        _: RequestId,
        claim: &mut dyn FnMut(&mut dyn safe_cex_engine::WebSocketHandoff),
    ) -> Result<(), WebSocketWriteError> {
        if self.busy {
            return Err(WebSocketWriteError::NotReady);
        }
        let mut sent = false;
        claim(&mut |_: SocketId,
                    text: String,
                    commit: &mut safe_cex_engine::WebSocketCommit<'_>| {
            let mut text = Some(text);
            commit(&mut || {
                let Some(text) = text.take() else {
                    return false;
                };
                self.sent.lock().unwrap().push(text);
                sent = true;
                true
            })
        });
        if sent && self.send_error {
            return Err(WebSocketWriteError::Send("physical write failed".into()));
        }
        Ok(())
    }
    fn retire(&self) {}
}
impl Transport for Captured {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        mpsc::unbounded_channel().0
    }
    fn connect_socket(
        &self,
        _: bool,
        socket: SocketId,
        _: String,
        _: Vec<(&'static str, String)>,
        _: Option<safe_cex_engine::SocketChannel>,
        _: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> SocketConnection {
        SocketConnection {
            outgoing: self.connect(socket, String::new(), events),
            writer: (!self.unsupported).then(|| {
                Arc::new(CapturedWriter {
                    sent: self.sent.clone(),
                    busy: self.busy,
                    send_error: self.send_error,
                }) as Arc<dyn WebSocketWriter>
            }),
        }
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("socket write reached HTTP transport");
    }
}

#[test]
fn native_websocket_write_claims_at_physical_handoff_and_classifies_refusals() {
    for refusal in 0..7 {
        let captured = Arc::new(Mutex::new(vec![]));
        let engine = safe_cex_engine::test_support::seeded_engine(Probe {
            deny: refusal == 1,
            ..Probe::default()
        });
        let transport = Captured {
            sent: captured.clone(),
            unsupported: refusal == 4,
            busy: refusal == 5,
            send_error: refusal == 6,
        };
        let mut host = Host::new(engine, transport);
        host.clock = || 0.;
        host.engine.start(0.);
        host.execute(&mut |_| {});
        host.dispatch(HostEvent::WsOpened(host.engine.venue().socket.unwrap()));
        host.engine.handle(
            0.,
            Event::Command(Command::SetLeverage {
                command: 71,
                symbol: "BTC".into(),
                leverage: 2.,
                side: None,
            }),
        );
        let actions = host.engine.drain_actions().collect::<Vec<_>>();
        assert_eq!(host.engine.venue().releases, 0);
        assert!(captured.lock().unwrap().is_empty());
        if refusal == 2 {
            host.clock = || 2_000.;
        }
        if refusal == 3 {
            host.sockets.clear();
        }
        let binding = actions
            .into_iter()
            .find_map(safe_cex_engine::WebSocketWriteBinding::from_action)
            .expect("typed write binding");
        host.execute_websocket_write(binding);
        assert_eq!(
            captured.lock().unwrap().len(),
            usize::from(refusal == 0 || refusal == 6)
        );
        assert_eq!(
            host.engine.venue().releases,
            usize::from(refusal <= 1 || refusal == 6)
        );
        let mut notices = vec![];
        host.execute(&mut |notice| notices.push(notice));
        if refusal != 0 {
            let outcome = if refusal == 6 {
                safe_cex_engine::Outcome::Unknown
            } else {
                safe_cex_engine::Outcome::NotSent
            };
            assert!(
                notices.iter().any(|notice| matches!(notice,
                    Notice::CommandFailed { command: 71, error } if error.outcome == Some(outcome)
                )),
                "refusal {refusal}: {notices:?}"
            );
        }
    }
}

#[test]
fn foreign_websocket_binding_cannot_retire_the_current_hosts_repeated_counters() {
    fn queued(
        sent: Arc<Mutex<Vec<String>>>,
    ) -> (
        Host<Probe, Captured>,
        safe_cex_engine::WebSocketWriteBinding,
    ) {
        let engine = safe_cex_engine::test_support::seeded_engine(Probe::default());
        let mut host = Host::new(
            engine,
            Captured {
                sent,
                unsupported: false,
                busy: false,
                send_error: false,
            },
        );
        host.clock = || 0.;
        host.engine.start(0.);
        host.execute(&mut |_| {});
        host.dispatch(HostEvent::WsOpened(host.engine.venue().socket.unwrap()));
        host.engine.handle(
            0.,
            Event::Command(Command::SetLeverage {
                command: 71,
                symbol: "BTC".into(),
                leverage: 2.,
                side: None,
            }),
        );
        let binding = host
            .engine
            .drain_actions()
            .find_map(safe_cex_engine::WebSocketWriteBinding::from_action)
            .unwrap();
        (host, binding)
    }

    let old_frames = Arc::new(Mutex::new(Vec::new()));
    let (a, binding_a) = queued(old_frames.clone());
    drop(a);
    let frames = Arc::new(Mutex::new(Vec::new()));
    let (mut b, binding_b) = queued(frames.clone());
    assert_eq!(
        binding_a.preparation().attempt(),
        binding_b.preparation().attempt()
    );
    assert_ne!(binding_a.preparation(), binding_b.preparation());
    let socket_b = b.engine.venue().socket.unwrap();
    b.execute_websocket_write(binding_a);
    assert!(old_frames.lock().unwrap().is_empty());
    assert!(frames.lock().unwrap().is_empty());
    assert_eq!(b.engine.venue().releases, 0);
    assert_eq!(b.engine.websocket_write_socket(&binding_b), Some(socket_b));
    b.execute_websocket_write(binding_b);
    assert_eq!(*frames.lock().unwrap(), ["setting"]);
    assert_eq!(b.engine.venue().releases, 1);
    let mut notices = Vec::new();
    b.execute(&mut |notice| notices.push(notice));
    assert!(
        notices
            .iter()
            .all(|notice| !matches!(notice, Notice::CommandFailed { command: 71, .. })),
        "foreign work must not fail B's command: {notices:?}"
    );
}

async fn socket_handoff_probe(delayed_handshake: bool) -> (bool, usize, Vec<Notice>) {
    use futures_util::StreamExt;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (open, wait_open) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        wait_open.await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        tokio::time::timeout(Duration::from_millis(200), ws.next()).await
    });
    let engine = safe_cex_engine::test_support::seeded_engine(Probe {
        url: Some(format!("ws://{address}")),
        ..Probe::default()
    });
    let mut host = Host::new(engine, NetworkTransport::new());
    host.clock = || 0.;
    host.engine.start(0.);
    host.execute(&mut |_| {});
    let socket = host.engine.venue().socket.unwrap();
    let mut open = Some(open);
    if !delayed_handshake {
        open.take().unwrap().send(()).unwrap();
        loop {
            let event = tokio::time::timeout(Duration::from_secs(2), host.inbox.recv())
                .await
                .unwrap()
                .unwrap();
            let opened = matches!(event, HostEvent::WsOpened(id) if id == socket);
            host.dispatch(event);
            if opened {
                break;
            }
        }
    }
    host.engine.handle(
        0.,
        Event::Command(Command::SetLeverage {
            command: 71,
            symbol: "BTC".into(),
            leverage: 2.,
            side: None,
        }),
    );
    let actions = host.engine.drain_actions().collect::<Vec<_>>();
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 1_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    let binding = actions
        .into_iter()
        .find_map(safe_cex_engine::WebSocketWriteBinding::from_action)
        .unwrap();
    host.execute_websocket_write(binding);
    if delayed_handshake {
        host.clock = || 2_000.;
        host.engine.handle(2_000., Event::TimerFired { timer });
    }
    let mut notices = vec![];
    host.execute(&mut |notice| notices.push(notice));
    if let Some(open) = open {
        open.send(()).unwrap();
    }
    let received = server.await.unwrap();
    let sent = matches!(received, Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) if text.as_str() == "setting");
    (sent, host.engine.venue().releases, notices)
}

#[tokio::test(flavor = "current_thread")]
async fn expired_write_cannot_cross_delayed_socket_handshake() {
    let (sent, releases, notices) = socket_handoff_probe(true).await;
    assert!(
        !sent,
        "expired frame reached the physical socket after completion: {notices:?}"
    );
    assert_eq!(
        releases, 0,
        "a connecting socket cannot consume the write claim"
    );
    assert!(
        notices.iter().any(|notice| matches!(notice,
            Notice::CommandFailed { command: 71, error }
                if error.outcome == Some(safe_cex_engine::Outcome::NotSent)
        )),
        "a frame never handed to the writer is NotSent: {notices:?}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn open_socket_writer_handoff_control() {
    let (sent, releases, notices) = socket_handoff_probe(false).await;
    assert!(
        sent,
        "an open physical writer must accept the authorized frame: {notices:?}"
    );
    assert_eq!(releases, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn release_panic_does_not_break_host_stop() {
    use futures_util::StreamExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        tokio::time::timeout(Duration::from_millis(500), ws.next()).await
    });
    let engine = safe_cex_engine::test_support::seeded_engine(Probe {
        url: Some(format!("ws://{address}")),
        panic_release: true,
        ..Probe::default()
    });
    let mut host = Host::new(engine, NetworkTransport::new());
    host.clock = || 0.;
    host.engine.start(0.);
    host.execute(&mut |_| {});
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), host.inbox.recv())
            .await
            .unwrap()
            .unwrap();
        let opened = matches!(event, HostEvent::WsOpened(_));
        host.dispatch(event);
        if opened {
            break;
        }
    }
    host.engine.handle(
        0.,
        Event::Command(Command::SetLeverage {
            command: 71,
            symbol: "BTC".into(),
            leverage: 2.,
            side: None,
        }),
    );
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.execute(&mut |_| {})));
    assert!(
        result.is_err(),
        "venue bugs must retain their documented panic behavior"
    );
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.stop()));
    assert!(
        stopped.is_ok(),
        "writer retirement panicked on its poisoned mutex after the expected venue panic"
    );
    let _ = server.await;
}

struct ReentrantTransport {
    writer: WriterSlot,
    inner: NetworkTransport,
}
impl Transport for ReentrantTransport {
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner.connect(socket, url, events)
    }
    fn connect_socket(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> SocketConnection {
        let connection = self
            .inner
            .connect_socket(binary, socket, url, headers, channel, context, events);
        *self.writer.lock().unwrap() = connection.writer.clone();
        connection
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("unexpected HTTP");
    }
}
async fn retiring_claim(retire: &str) -> (bool, usize, Vec<Notice>) {
    use futures_util::StreamExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        tokio::time::timeout(Duration::from_millis(200), ws.next()).await
    });
    let slot = Arc::new(Mutex::new(None));
    let engine = safe_cex_engine::test_support::seeded_engine(Probe {
        url: Some(format!("ws://{address}")),
        retire_writer: (retire != "never").then(|| slot.clone()),
        retire_after_release: retire == "observer",
        ..Probe::default()
    });
    let mut host = Host::new(
        engine,
        ReentrantTransport {
            writer: slot,
            inner: NetworkTransport::new(),
        },
    );
    host.clock = || 0.;
    host.engine.start(0.);
    host.execute(&mut |_| {});
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), host.inbox.recv())
            .await
            .unwrap()
            .unwrap();
        let opened = matches!(event, HostEvent::WsOpened(_));
        host.dispatch(event);
        if opened {
            break;
        }
    }
    host.engine.handle(
        0.,
        Event::Command(Command::SetLeverage {
            command: 71,
            symbol: "BTC".into(),
            leverage: 2.,
            side: None,
        }),
    );
    let mut notices = Vec::new();
    host.execute(&mut |notice| notices.push(notice));
    let received = server.await.unwrap();
    let sent = matches!(received,Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) if text.as_str()=="setting");
    (sent, host.engine.venue().actual_releases, notices)
}
#[tokio::test(flavor = "current_thread")]
async fn open_writer_commits_one_release() {
    let (sent, releases, notices) = retiring_claim("never").await;
    assert!(sent, "{notices:?}");
    assert_eq!(releases, 1);
}
#[tokio::test(flavor = "current_thread")]
async fn retired_during_release_is_not_sent_without_commit() {
    let (sent, releases, notices) = retiring_claim("admission").await;
    assert!(!sent, "retired writer sent a frame: {notices:?}");
    assert_eq!(
        releases, 0,
        "no physical frame was handed off, but the engine claim was consumed: {notices:?}"
    );
    assert!(notices.iter().any(|notice| matches!(notice, Notice::CommandFailed {command:71,error} if error.outcome == Some(safe_cex_engine::Outcome::NotSent))), "{notices:?}");
}

#[tokio::test(flavor = "current_thread")]
async fn signing_release_observer_reenters_after_physical_handoff() {
    let (sent, releases, notices) = retiring_claim("observer").await;
    assert!(sent, "observer ran before physical handoff: {notices:?}");
    assert_eq!(releases, 1);
}
