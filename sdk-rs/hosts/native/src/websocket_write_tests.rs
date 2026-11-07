use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Context, Error, Operation, Policy, ReconciliationData, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, HttpRequest, Response, SocketChannel, SocketSpec};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    socket: Option<SocketId>,
    url: Option<String>,
    deny: bool,
    panic_release: bool,
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
            assert!(!this.panic_release, "venue release panic");
            Ok(if this.deny {
                Admission::Denied
            } else {
                Admission::Unscheduled
            })
        })
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
        claim: &mut dyn FnMut() -> Option<String>,
    ) -> Result<(), WebSocketWriteError> {
        if self.busy {
            return Err(WebSocketWriteError::NotReady);
        }
        if let Some(text) = claim() {
            self.sent.lock().unwrap().push(text);
            if self.send_error {
                return Err(WebSocketWriteError::Send("physical write failed".into()));
            }
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
