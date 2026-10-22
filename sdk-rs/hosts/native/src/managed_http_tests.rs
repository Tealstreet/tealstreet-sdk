use super::*;
use safe_cex_engine::request_context::{
    Context, HttpRelease, Operation, Pacing, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, HttpRequest, Method, Response, book::Book};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::oneshot;

#[derive(Debug)]
struct Pending(Context);
#[derive(Default)]
struct Probe {
    responses: usize,
    failures: usize,
    releases: Vec<HttpRelease>,
    errors: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn http_pacing(&self) -> &[Pacing] {
        &[Pacing {
            group: 1,
            minimum_gap_ms: 450.,
        }]
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for _ in 0..2 {
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 5_000,
                        pacing: Some(Pacing {
                            group: 1,
                            minimum_gap_ms: 450.,
                        }),
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_managed(
                context.clone(),
                Method::Post,
                "https://venue.test/read".into(),
                Some("{}".into()),
                Pending(context),
            )
            .unwrap();
        }
    }
    fn on_http_release(
        &mut self,
        _: &mut Pending,
        release: &HttpRelease,
    ) -> Result<safe_cex_engine::request_context::Admission, Error> {
        self.releases.push(*release);
        Ok(safe_cex_engine::request_context::Admission::Unscheduled)
    }
    fn on_http_preparation_failed(
        &mut self,
        _: &mut Cx<'_, Pending>,
        _: Pending,
        _: Error,
        error: VenueError,
    ) {
        self.failures += 1;
        self.errors.push(error);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
        self.responses += 1;
        cx.finish_http_context(&pending.0).unwrap();
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
        self.failures += 1;
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
        panic!("engine-owned timer leaked");
    }
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

type Handoffs = Arc<Mutex<Vec<(RequestId, u64)>>>;
struct ControlledTransport {
    gates: Mutex<Vec<oneshot::Receiver<()>>>,
    sent: Handoffs,
    discarded: Arc<AtomicUsize>,
    clock: fn() -> u64,
}
impl Transport for ControlledTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("no socket");
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("managed HTTP bypassed preparation");
    }
    fn prepare_http(
        &self,
        request: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let gate = self.gates.lock().unwrap().remove(0);
        let prepared = ControlledPrepared {
            request: request.id,
            sent: self.sent.clone(),
            discarded: self.discarded.clone(),
            bound: false,
            clock: self.clock,
        };
        Box::pin(async move {
            gate.await
                .map_err(|_| HttpPrepareError::Transport("cancelled test gate".into()))?;
            Ok(Box::new(prepared) as Box<dyn PreparedHttp>)
        })
    }
}
struct ControlledPrepared {
    request: RequestId,
    sent: Handoffs,
    discarded: Arc<AtomicUsize>,
    bound: bool,
    clock: fn() -> u64,
}
impl std::fmt::Debug for ControlledPrepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ControlledPrepared")
    }
}
impl Drop for ControlledPrepared {
    fn drop(&mut self) {
        if !self.bound {
            self.discarded.fetch_add(1, Ordering::SeqCst);
        }
    }
}
impl PreparedHttp for ControlledPrepared {
    fn bind(
        mut self: Box<Self>,
        packet: HttpRequest,
    ) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        assert_eq!(packet.id, self.request);
        self.bound = true;
        Ok(Box::new(ControlledBound {
            request: self.request,
            sent: self.sent.clone(),
            clock: self.clock,
        }))
    }
}
struct ControlledBound {
    request: RequestId,
    sent: Handoffs,
    clock: fn() -> u64,
}
impl std::fmt::Debug for ControlledBound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ControlledBound")
    }
}
impl BoundHttp for ControlledBound {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> AbortHandle {
        self.sent
            .lock()
            .unwrap()
            .push((self.request, (self.clock)()));
        tokio::spawn(async move {
            let _ = events.send(HostEvent::HttpStart {
                request: self.request,
                status: 200,
                headers: String::new(),
            });
            let _ = events.send(HostEvent::HttpChunk(self.request, b"[]".to_vec()));
            let _ = events.send(HostEvent::HttpEnd(self.request));
        })
        .abort_handle()
    }
}

async fn ready(host: &mut Host<Probe, ControlledTransport>) -> Preparation {
    let event = host.inbox.recv().await.unwrap();
    let HostEvent::HttpPrepared(token, _) = &event else {
        panic!("expected prepared HTTP, got {event:?}");
    };
    let token = *token;
    host.dispatch(event);
    host.execute(&mut |_| {});
    token
}
async fn response(host: &mut Host<Probe, ControlledTransport>) {
    for _ in 0..3 {
        let event = host.inbox.recv().await.unwrap();
        assert!(matches!(
            event,
            HostEvent::HttpStart { .. } | HostEvent::HttpChunk(..) | HostEvent::HttpEnd(_)
        ));
        host.dispatch(event);
        host.execute(&mut |_| {});
    }
}

#[tokio::test(start_paused = true)]
async fn delayed_first_route_fast_second_and_fast_ack_keep_gap_at_native_handoff() {
    static START: OnceLock<tokio::time::Instant> = OnceLock::new();
    START.set(tokio::time::Instant::now()).unwrap();
    fn clock() -> u64 {
        START.get().unwrap().elapsed().as_millis() as u64
    }
    let (first_tx, first_rx) = oneshot::channel();
    let (second_tx, second_rx) = oneshot::channel();
    let sent = Handoffs::default();
    let mut host = Host::new(
        safe_cex_engine::test_support::seeded_engine(Probe::default()),
        ControlledTransport {
            gates: Mutex::new(vec![first_rx, second_rx]),
            sent: sent.clone(),
            discarded: Arc::default(),
            clock,
        },
    );
    host.clock = || clock() as f64;
    host.engine.start(0.);
    host.execute(&mut |_| {});
    assert!(sent.lock().unwrap().is_empty());
    tokio::time::advance(Duration::from_millis(700)).await;
    second_tx.send(()).unwrap();
    let second = ready(&mut host).await;
    assert_eq!(
        sent.lock().unwrap().as_slice(),
        [(second.attempt().request, 700)]
    );
    response(&mut host).await;
    assert_eq!(host.engine.venue().responses, 1);
    tokio::time::advance(Duration::from_millis(1)).await;
    first_tx.send(()).unwrap();
    let first = ready(&mut host).await;
    assert_eq!(sent.lock().unwrap().len(), 1);
    assert_eq!(host.engine.venue().releases.len(), 1);
    tokio::time::advance(Duration::from_millis(448)).await;
    assert!(host.inbox.try_recv().is_err());
    tokio::time::advance(Duration::from_millis(1)).await;
    let wake = host.inbox.recv().await.unwrap();
    assert_eq!(
        clock(),
        1_150,
        "a delayed task must not restart the wake duration"
    );
    assert!(matches!(wake, HostEvent::HttpWake(..)));
    host.dispatch(wake);
    host.execute(&mut |_| {});
    assert_eq!(
        sent.lock().unwrap().as_slice(),
        [
            (second.attempt().request, 700),
            (first.attempt().request, 1_150),
        ]
    );
    response(&mut host).await;
    assert_eq!(host.engine.venue().responses, 2);
    assert_eq!(host.engine.venue().failures, 0);
    assert!(host.managed_http.resources.is_empty());
    host.stop();
}

#[tokio::test(start_paused = true)]
async fn unsupported_custom_transport_is_typed_not_sent_without_legacy_fallback() {
    struct Unsupported;
    impl Transport for Unsupported {
        fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
            panic!("no socket");
        }
        fn request(&self, _: HttpRequest, _: Events) {
            panic!("unsupported managed request used legacy fallback");
        }
    }
    let mut host = Host::new(
        safe_cex_engine::test_support::seeded_engine(Probe::default()),
        Unsupported,
    );
    host.clock = || 0.;
    host.engine.start(0.);
    host.execute(&mut |_| {});
    for _ in 0..2 {
        let event = host.inbox.recv().await.unwrap();
        assert!(matches!(
            event,
            HostEvent::HttpPrepared(_, Err(HttpPrepareError::Unsupported))
        ));
        host.dispatch(event);
        host.execute(&mut |_| {});
    }
    assert_eq!(host.engine.venue().errors.len(), 2);
    for error in &host.engine.venue().errors {
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    }
    assert!(host.engine.venue().releases.is_empty());
    assert!(host.managed_http.resources.is_empty());
    host.stop();
}
