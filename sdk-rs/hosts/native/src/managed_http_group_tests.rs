use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Admission, Context, HttpRelease, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, RetryPolicy,
    plan::{CallBound, MutationBarrierBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::{Cx, HttpRequest, Method, Response};
use std::sync::{Arc, Mutex};

fn credential() -> safe_cex_engine::request_context::Credential {
    safe_cex_engine::request_context::Credential {
        slot: safe_cex_engine::CredentialSlot::Primary,
        reference: safe_cex_engine::signing::KeyRef("group-native".into()),
    }
}
struct Recipe;
impl safe_cex_engine::SignStep for Recipe {
    fn freshness_ms(&self) -> f64 {
        5_000.
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        cx: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        Ok(safe_cex_engine::signing::SignRequest {
            receipt: safe_cex_engine::signing::SignReceipt {
                nonce: None,
                stamp_ms: Some(cx.stamp_ms()),
                expiry_ms: Some(cx.stamp_ms() + 5_000.),
            },
            context: Default::default(),
            operations: vec![safe_cex_engine::signing::Operation::Sign {
                algorithm: safe_cex_engine::signing::Algorithm::HmacSha256,
                key: credential().reference,
                payload: vec![],
            }],
        })
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        Ok(())
    }
}
struct Provider;
impl safe_cex_engine::signing::Signer for Provider {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(
        &self,
        work: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        Some(Ok(safe_cex_engine::signing::SignResult::new(
            vec![vec![7; 32]],
            work.receipt,
        )))
    }
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        Box::pin(async move {
            Ok(safe_cex_engine::signing::SignResult::new(
                vec![vec![7; 32]],
                work.receipt,
            ))
        })
    }
}
#[derive(Default)]
struct Probe {
    plan: Option<Plan>,
    slots: HashMap<u64, Slot>,
    replies: usize,
    failures: usize,
    errors: Vec<VenueError>,
}
impl Venue for Probe {
    type Pending = Context;
    fn start(&mut self, _: &mut Cx<'_, Context>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::WriteAfterReads,
                read_credential: None,
                write_credential: Some(credential()),
            })
            .unwrap();
        let calls: [CallBound; 2] = std::array::from_fn(|_| CallBound {
            timeout_ms: 20_000,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        });
        let plan = cx
            .admit_plan_with_barriers(
                &preflight,
                &[StepBound {
                    wait_ms: 0,
                    calls: &calls,
                }],
                PlanPolicy {
                    independent_calls: true,
                    operation: Operation::Mutation,
                    credential: Some(credential()),
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                &[MutationBarrierBound { step: 0 }],
            )
            .unwrap();
        for slot in cx.plan_slots(&plan).unwrap() {
            let context = cx.plan_leg_context(&plan, &slot).unwrap();
            self.slots.insert(context.leg(), slot);
            cx.request_signed_managed(
                context.clone(),
                Method::Post,
                "https://venue.test/group".into(),
                None,
                Box::new(Recipe),
                context,
            )
            .unwrap();
        }
        self.plan = Some(plan);
    }
    fn prepare_group_http_release(&self, _: &Context, _: &HttpRelease) -> Result<Admission, Error> {
        Ok(Admission::Unscheduled)
    }
    fn capture_http_reconciliation(
        &self,
        _: &Context,
    ) -> Result<safe_cex_engine::request_context::ReconciliationData, Error> {
        safe_cex_engine::request_context::ReconciliationData::new("native-group".into())
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
        cx.http_write_fact(
            &context,
            safe_cex_engine::classify::WriteFact::Accepted {
                order_ids: vec![format!("native-{}", context.leg())],
            },
            None,
        )
        .unwrap();
        let plan = self.plan.as_ref().unwrap();
        cx.finish_plan_call(plan, &self.slots[&context.leg()], &context)
            .unwrap();
        self.replies += 1;
        if self.replies + self.failures == 2 {
            cx.done_plan(plan, "[]".into()).unwrap();
        }
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, error: VenueError) {
        self.errors.push(error);
        self.failures += 1;
        let plan = self.plan.as_ref().unwrap();
        cx.finish_plan_failed_call(plan, &self.slots[&context.leg()], &context)
            .unwrap();
        if self.replies + self.failures == 2 {
            cx.done_plan(plan, "[]".into()).unwrap();
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[derive(Clone, Default)]
struct TransportState {
    events: Arc<Mutex<Vec<&'static str>>>,
    bind_failure: bool,
    ready_failure: bool,
    send_panic: bool,
}
struct TestTransport(TransportState);
impl Transport for TestTransport {
    fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
        panic!("no sockets");
    }
    fn request(&self, _: HttpRequest, _: Events) {
        panic!("no legacy request");
    }
    fn prepare_http(
        &self,
        request: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let state = self.0.clone();
        Box::pin(async move {
            Ok(Box::new(Packet {
                state,
                request: request.id,
                ordinal: 0,
            }) as Box<dyn PreparedHttp>)
        })
    }
}
struct Packet {
    state: TransportState,
    request: RequestId,
    ordinal: usize,
}
impl std::fmt::Debug for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GroupPacketFixture")
    }
}
impl PreparedHttp for Packet {
    fn bind(
        mut self: Box<Self>,
        packet: HttpRequest,
    ) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        assert_eq!(self.request, packet.id);
        let mut events = self.state.events.lock().unwrap();
        self.ordinal = events.iter().filter(|event| **event == "bind").count();
        events.push("bind");
        if self.ordinal == 1 && self.state.bind_failure {
            return Err(HttpPrepareError::Invalid("fixture bind refusal"));
        }
        drop(events);
        Ok(self)
    }
}
impl BoundHttp for Packet {
    fn ready(&self) -> bool {
        self.state.events.lock().unwrap().push("ready");
        !(self.ordinal == 1 && self.state.ready_failure)
    }
    fn dispatch(self: Box<Self>, events: Events) -> AbortHandle {
        self.state.events.lock().unwrap().push("send");
        if self.ordinal == 0 && self.state.send_panic {
            panic!("fixture transport panic");
        }
        let request = self.request;
        tokio::spawn(async move {
            let _ = events.send(HostEvent::HttpStart {
                request,
                status: 200,
                headers: String::new(),
            });
            let _ = events.send(HostEvent::HttpChunk(request, b"[]".to_vec()));
            let _ = events.send(HostEvent::HttpEnd(request));
        })
        .abort_handle()
    }
}
async fn run(state: TransportState) -> (Host<Probe, TestTransport>, Vec<Notice>) {
    let mut host = Host::new(
        safe_cex_engine::test_support::seeded_engine(Probe::default()),
        TestTransport(state),
    );
    host.engine.set_signer(Some(Arc::new(Provider))).unwrap();
    host.clock = || 1.;
    host.engine.start(0.);
    host.engine.handle(
        1.,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTC".into(),
            leverage: 5.,
            side: None,
        }),
    );
    let mut notices = Vec::new();
    host.execute(&mut |notice| notices.push(notice));
    let mut prepared = 0;
    for _ in 0..32 {
        if prepared == 2 {
            break;
        }
        let event = tokio::time::timeout(Duration::from_secs(5), host.inbox.recv())
            .await
            .expect("native group preparation deadline")
            .unwrap();
        match &event {
            HostEvent::HttpPrepared(..) => prepared += 1,
            HostEvent::Timer(_) => {}
            _ => panic!("unexpected event before both preparations: {event:?}"),
        }
        host.dispatch(event);
        host.execute(&mut |notice| notices.push(notice));
    }
    assert_eq!(prepared, 2, "bounded native route preparation");
    (host, notices)
}
async fn drain_terminal(host: &mut Host<Probe, TestTransport>, notices: &mut Vec<Notice>) -> usize {
    let mut timers = 0;
    for _ in 0..32 {
        if notices.iter().any(|notice| {
            matches!(
                notice,
                Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
            )
        }) {
            return timers;
        }
        let event = tokio::time::timeout(Duration::from_secs(5), host.inbox.recv())
            .await
            .expect("native group terminal deadline")
            .unwrap();
        match &event {
            HostEvent::Timer(_) => timers += 1,
            HostEvent::HttpStart { .. }
            | HostEvent::HttpChunk(..)
            | HostEvent::HttpEnd(_)
            | HostEvent::HttpFailed(..) => {}
            _ => panic!("unexpected event after group release: {event:?}"),
        }
        host.dispatch(event);
        host.execute(&mut |notice| notices.push(notice));
    }
    panic!("native group exceeded bounded terminal drain");
}
#[tokio::test]
async fn both_native_resources_bind_and_check_before_back_to_back_sends() {
    let state = TransportState::default();
    let (mut host, mut notices) = run(state.clone()).await;
    assert_eq!(
        *state.events.lock().unwrap(),
        ["bind", "bind", "ready", "ready", "send", "send"]
    );
    // Re-deliver an actually armed timer before transport callbacks complete.
    let timer = *host
        .timers
        .keys()
        .next()
        .expect("genuine armed group timer");
    host.events.send(HostEvent::Timer(timer)).unwrap();
    assert!(drain_terminal(&mut host, &mut notices).await >= 1);
    assert_eq!(host.engine.venue().failures, 0);
    assert!(host.managed_http.resources.is_empty());
    assert!(
        !notices
            .iter()
            .any(|notice| matches!(notice, Notice::CommandFailed { command: 7, .. }))
    );
    assert_eq!(
        host.engine.venue().replies,
        2,
        "failures={:?}; notices={notices:?}",
        host.engine.venue().errors
    );
    assert_eq!(
        notices
            .iter()
            .filter(|notice| matches!(notice, Notice::CommandDone { command: 7, .. }))
            .count(),
        1
    );
}
#[tokio::test]
async fn second_native_bind_failure_or_unready_member_releases_nothing() {
    for state in [
        TransportState {
            bind_failure: true,
            ..Default::default()
        },
        TransportState {
            ready_failure: true,
            ..Default::default()
        },
    ] {
        let (host, notices) = run(state.clone()).await;
        assert_eq!(
            state
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|event| **event == "bind")
                .count(),
            2
        );
        assert!(!state.events.lock().unwrap().contains(&"send"));
        let errors: Vec<_> = notices
            .iter()
            .filter_map(|notice| match notice {
                Notice::CommandFailed { command: 7, error } => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(host.managed_http.resources.is_empty());
    }
}
#[tokio::test]
async fn first_native_send_panic_still_hands_off_sibling_and_projects_unknown() {
    let state = TransportState {
        send_panic: true,
        ..Default::default()
    };
    let (mut host, mut notices) = run(state.clone()).await;
    assert_eq!(
        state
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| **event == "send")
            .count(),
        2
    );
    drain_terminal(&mut host, &mut notices).await;
    let errors: Vec<_> = notices
        .iter()
        .filter_map(|notice| match notice {
            Notice::CommandFailed { command: 7, error } => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(errors[0].partial.as_ref().unwrap().order_ids.len(), 1);
    assert_eq!(host.engine.venue().replies, 1);
    assert_eq!(host.engine.venue().failures, 1);
    assert!(host.managed_http.resources.is_empty());
}
