use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::Error;
use safe_cex_engine::request_context::{
    Admission, Context, HttpRelease, Operation, Plan, PlanPolicy, PreflightPolicy,
    PreflightPurpose, RetryPolicy,
    plan::{CallBound, MutationBarrierBound, RetryWindow, Slot, StepBound},
};
use safe_cex_engine::{Action, Command, Notice, RequestId, SocketId, TimerId, Venue, VenueError};
use safe_cex_engine::{Cx, HttpRequest, Method, Response};
use safe_cex_native::{
    BoundHttp, Events, Host, HostEvent, HttpPrepareError, HttpPrepareFuture, PreparedHttp,
    Transport,
};
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, time::Duration};
use tokio::{sync::mpsc, task::AbortHandle};

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
    fn http_mutation_barriers() -> bool {
        true
    }
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
                None,
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

#[tokio::test]
async fn engine_dependency_cannot_enable_group_handoff_in_an_off_host() {
    let state = TransportState::default();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe::default());
    engine.set_signer(Some(Arc::new(Provider))).unwrap();
    let host = Host::new(engine, TestTransport(state.clone()));
    let (sender, commands) = mpsc::unbounded_channel();
    sender
        .send(Command::SetLeverage {
            command: 7,
            symbol: "BTC-USDT".into(),
            leverage: 5.,
            side: None,
        })
        .unwrap();
    let mut sender = Some(sender);
    let mut notices = Vec::new();
    let engine = tokio::time::timeout(
        Duration::from_secs(5),
        host.run(
            commands,
            Duration::from_millis(10),
            |_| {},
            |notice| {
                if matches!(
                    notice,
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                ) {
                    sender.take();
                }
                notices.push(notice);
            },
        ),
    )
    .await
    .expect("bounded public native Host terminal");
    assert!(
        engine.venue().plan.is_some(),
        "genuine engine barrier admission ran"
    );
    assert_eq!(engine.venue().slots.len(), 2);
    let terminal: Vec<_> = notices
        .iter()
        .filter(|notice| {
            matches!(
                notice,
                Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }
            )
        })
        .collect();
    assert_eq!(terminal.len(), 1);
    if cfg!(feature = "host-barriers") {
        assert_eq!(
            *state.events.lock().unwrap(),
            ["bind", "bind", "ready", "ready", "send", "send"]
        );
        assert_eq!(engine.venue().replies, 2);
        assert!(matches!(
            terminal[0],
            Notice::CommandDone { command: 7, .. }
        ));
    } else {
        assert!(
            state.events.lock().unwrap().is_empty(),
            "off host never binds, checks or sends"
        );
        assert_eq!(engine.venue().replies, 0);
        let Notice::CommandFailed { error, .. } = terminal[0] else {
            panic!("off host must refuse");
        };
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    }
}
