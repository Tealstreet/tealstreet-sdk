use super::*;
use safe_cex_engine::credential_selection::{CredentialRole, SlotPolicy, Slots};
use safe_cex_engine::nonce::{KRAKEN, NonceClaim};
use safe_cex_engine::signing::{Identities, Identity, KeyRef, SignError, SignRequest, SignResult};

struct NonceRecipe(NonceClaim);
impl safe_cex_engine::SignStep for NonceRecipe {
    fn nonce_claim(&self) -> Option<&NonceClaim> {
        Some(&self.0)
    }
    fn prepare(
        &mut self,
        action: &mut Action,
        cx: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let nonce = cx
            .reserve_nonce(&self.0)
            .map_err(|_| SignError::InvalidRequest)?;
        let mut work = Recipe.prepare(action, cx)?;
        work.receipt.nonce = Some(nonce);
        Ok(work)
    }
    fn apply(&mut self, action: &mut Action, result: &SignResult) -> Result<(), SignError> {
        Recipe.apply(action, result)
    }
    fn freshness_ms(&self) -> f64 {
        10_000.
    }
}
#[derive(Default)]
struct NonceVenue {
    responses: usize,
    failures: Vec<VenueError>,
}
fn identity() -> Identity {
    Identity {
        credential: KeyRef("fixture".into()),
        signing: KeyRef("fixture".into()),
        passphrase: None,
        stable_id: Some("native-nonce-final-claim".into()),
        extra: Default::default(),
    }
}
impl Venue for NonceVenue {
    #[inline]
    fn dispatch_nonce() -> bool {
        true
    }

    type Pending = Context;
    fn http_release() -> safe_cex_engine::HttpRelease<Self> {
        safe_cex_engine::HttpRelease::NonceUnscheduled(|_, _, _| Ok(()))
    }
    fn start(&mut self, cx: &mut Cx<'_, Context>) {
        let slots = Slots::new(Some(identity()), None, SlotPolicy::default());
        for _ in 0..2 {
            let (selection, selected, claim) = slots
                .select_with_nonce(CredentialRole::Read { scope: None }, &KRAKEN)
                .unwrap();
            let context = cx
                .http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: Some(selection.credential(selected.credential.clone())),
                        timeout_ms: 5_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            cx.request_signed_managed(
                context.clone(),
                None,
                Method::Post,
                "https://venue.test/nonce".into(),
                None,
                Box::new(NonceRecipe(claim)),
                context,
            )
            .unwrap();
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Response<'_>) {
        self.responses += 1;
        cx.finish_http_context(&context).unwrap();
    }
    fn on_http_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, error: VenueError) {
        self.failures.push(error);
    }
    fn on_sign_error(&mut self, _: &mut Cx<'_, Context>, _: Context, error: VenueError) {
        self.failures.push(error);
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, Context>, _: Context, _: &str) {
        panic!("typed failure only");
    }
    fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) {
        panic!("no venue timer");
    }
    fn on_command(&mut self, _: &mut Cx<'_, Context>, _: Command) {
        panic!("background fixture");
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
async fn next(host: &mut Host<NonceVenue, ControlledTransport>) -> HostEvent {
    tokio::time::timeout(Duration::from_secs(1), host.inbox.recv())
        .await
        .unwrap()
        .unwrap()
}
#[tokio::test(start_paused = true)]
async fn final_nonce_claim_stale_drops_old_packet_and_only_engine_repair_sends() {
    let (route_a, route_rx_a) = oneshot::channel();
    let (route_b, route_rx_b) = oneshot::channel();
    let (route_repair, route_rx_repair) = oneshot::channel();
    let (provider_a, provider_rx_a) = oneshot::channel();
    let (provider_b, provider_rx_b) = oneshot::channel();
    let (provider_repair, provider_rx_repair) = oneshot::channel();
    let signer = Arc::new(ParkedSigner {
        gates: Mutex::new(vec![provider_rx_a, provider_rx_b, provider_rx_repair]),
        started: AtomicUsize::new(0),
        dropped: Arc::default(),
    });
    let sent = Handoffs::default();
    let mut engine = safe_cex_engine::test_support::seeded_engine(NonceVenue::default());
    engine
        .set_identities(Identities {
            primary: Some(identity()),
            secondary: None,
        })
        .unwrap();
    let mut host = Host::new(
        engine,
        ControlledTransport {
            gates: Mutex::new(vec![route_rx_a, route_rx_b, route_rx_repair]),
            sent: sent.clone(),
            discarded: Arc::default(),
            clock: || 1_000,
        },
    )
    .with_signer(signer.clone())
    .unwrap();
    host.clock = || 1_000.;
    host.engine.start(1_000.);
    host.execute(&mut |_| {});
    route_a.send(()).unwrap();
    let first = next(&mut host).await;
    assert!(matches!(first, HostEvent::HttpPrepared(..)));
    host.dispatch(first);
    host.execute(&mut |_| {});
    route_b.send(()).unwrap();
    let second = next(&mut host).await;
    assert!(matches!(second, HostEvent::HttpPrepared(..)));
    host.dispatch(second);
    host.execute(&mut |_| {});
    provider_a.send(()).unwrap();
    let event = next(&mut host).await;
    let HostEvent::Signed(_, Ok(result)) = &event else {
        panic!("first provider result");
    };
    let nonce_a = result.receipt.nonce.unwrap();
    host.dispatch(event);
    let mut deferred = host
        .engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(action)
                if matches!(
                    *action,
                    safe_cex_engine::request_context::HttpAction::Bind { .. }
                ) =>
            {
                Some(*action)
            }
            Action::Notice(_) => panic!("host must not invent a terminal"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(deferred.len(), 1);
    provider_b.send(()).unwrap();
    let event = next(&mut host).await;
    let HostEvent::Signed(_, Ok(result)) = &event else {
        panic!("second provider result");
    };
    let nonce_b = result.receipt.nonce.unwrap();
    assert!(nonce_b > nonce_a);
    host.dispatch(event);
    host.execute(&mut |_| {});
    assert_eq!(sent.lock().unwrap().len(), 1);
    let stale_preparation = match &deferred[0] {
        safe_cex_engine::request_context::HttpAction::Bind { preparation, .. } => *preparation,
        _ => unreachable!("held Bind"),
    };
    host.execute_managed_http(deferred.remove(0));
    assert_eq!(
        sent.lock().unwrap().len(),
        1,
        "stale final Claim must never send"
    );
    assert!(
        host.engine.venue().failures.is_empty(),
        "host emits no terminal on Stale"
    );
    let repairs = host.engine.drain_actions().collect::<Vec<_>>();
    let phases = repairs
        .iter()
        .filter_map(|action| match action {
            Action::ManagedHttp(action) => Some(match **action {
                safe_cex_engine::request_context::HttpAction::Cancel { preparation, .. } => {
                    assert_eq!(preparation, stale_preparation);
                    "cancel"
                }
                safe_cex_engine::request_context::HttpAction::Prepare { preparation, .. } => {
                    assert_eq!(preparation.attempt(), stale_preparation.attempt());
                    assert_ne!(preparation, stale_preparation);
                    "prepare"
                }
                _ => panic!("only cancel then prepare"),
            }),
            Action::Notice(_) => panic!("repair is not a terminal"),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(phases, ["cancel", "prepare"]);
    for action in repairs {
        if let Action::ManagedHttp(action) = action {
            host.execute_managed_http(*action);
        }
    }
    route_repair.send(()).unwrap();
    for _ in 0..8 {
        let event = next(&mut host).await;
        let prepared = matches!(event, HostEvent::HttpPrepared(..));
        host.dispatch(event);
        host.execute(&mut |_| {});
        if prepared {
            break;
        }
    }
    provider_repair.send(()).unwrap();
    for _ in 0..8 {
        let event = next(&mut host).await;
        if let HostEvent::Signed(_, Ok(result)) = &event {
            assert!(result.receipt.nonce.unwrap() > nonce_b);
        }
        host.dispatch(event);
        host.execute(&mut |_| {});
        if host.engine.venue().responses == 2 {
            break;
        }
    }
    assert_eq!(sent.lock().unwrap().len(), 2);
    assert_eq!(host.engine.venue().responses, 2);
    assert!(host.engine.venue().failures.is_empty());
    assert_eq!(signer.started.load(Ordering::SeqCst), 3);
    assert!(host.managed_http.resources.is_empty());
    assert!(host.signing_tasks.is_empty());
    host.stop();
}
