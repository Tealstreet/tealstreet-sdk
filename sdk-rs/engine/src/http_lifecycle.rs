//! Managed HTTP preparation and the engine's synchronous final claim.
use super::*;
use crate::request_context::{
    Attempt, Claim, Context, Error, HttpAction, HttpRelease, Lifecycle, Operation, Owner, Permit,
    Preparation, Prepared, ReleaseTime,
};

pub(super) struct Managed {
    lifecycle: Lifecycle,
    original: HttpRequest,
    timer: TimerId,
    response_started: bool,
}

impl<P> Cx<'_, P> {
    /// Queue a semantic, unsigned public read. Private recipes use the signed
    /// entry point; a missing signing recipe must never send an unsigned account
    /// request. Failure preserves the caller's continuation.
    pub fn request_managed(
        &mut self,
        context: Context,
        method: Method,
        url: String,
        body: Option<String>,
        pending: P,
    ) -> Result<RequestId, (P, Error)> {
        if let Err(error) = self.io.request_contexts.validate(&context, self.now) {
            return Err((pending, error));
        }
        if context.credential().is_some() || context.operation() != Operation::Read {
            return Err((pending, Error::Invalid));
        }
        if self.io.managed_http.len() >= 256 {
            return Err((pending, Error::Capacity));
        }
        let request = RequestId(self.io.next());
        let generation = u64::from(self.io.next());
        let timer = TimerId(self.io.next());
        let http = HttpRequest {
            id: request,
            method,
            route: None,
            timeout_ms: Some((context.deadline() - self.now).ceil().max(1.) as u32),
            url,
            headers: vec![],
            signer_headers: vec![],
            body,
        };
        let intent = context.routing_intent();
        let deadline = context.deadline();
        let lifecycle = Lifecycle::new(
            context,
            Attempt {
                request,
                generation,
            },
        );
        let preparation = lifecycle.preparation();
        self.io.managed_http.insert(
            request,
            Managed {
                lifecycle,
                original: http.clone(),
                timer,
                response_started: false,
            },
        );
        self.io.pending.insert(
            request,
            PendingHttp {
                value: pending,
                auth: None,
            },
        );
        self.io.timers.insert(timer, ());
        self.io.actions.push(Action::SetTimer {
            timer,
            at_ms: deadline,
        });
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Prepare {
                preparation,
                intent,
                request: http,
            })));
        Ok(request)
    }
}

impl<P> Io<P> {
    pub(super) fn managed_response_live(
        &self,
        request: RequestId,
        token: Preparation,
        now: f64,
    ) -> bool {
        self.managed_http.get(&request).is_some_and(|managed| {
            managed.lifecycle.preparation() == token
                && managed.lifecycle.accepts_response()
                && self
                    .request_contexts
                    .validate(managed.lifecycle.context(), now)
                    .is_ok()
        })
    }

    pub(super) fn retire_http_leg(&mut self, context: &Context) {
        let requests = self
            .managed_http
            .iter()
            .filter_map(|(request, managed)| {
                (managed.lifecycle.context().leg() == context.leg()
                    && managed.lifecycle.context().owner() == context.owner()
                    && (managed.lifecycle.release().is_none()
                        || managed.lifecycle.context().operation() == Operation::Read))
                    .then_some(*request)
            })
            .collect::<Vec<_>>();
        for request in requests {
            self.discard_managed(request);
        }
    }

    pub(super) fn managed_preparation(&self, request: RequestId) -> Option<Preparation> {
        Some(self.managed_http.get(&request)?.lifecycle.preparation())
    }

    pub(super) fn retire_http_command(&mut self, command: CommandId) {
        let requests = self.managed_http.iter().filter_map(|(request, managed)| {
            (matches!(managed.lifecycle.context().owner(), Owner::Command {id, ..} if id == command)
                && (managed.lifecycle.release().is_none()
                    || managed.lifecycle.context().operation() == Operation::Read))
                .then_some(*request)
        }).collect::<Vec<_>>();
        for request in requests {
            self.discard_managed(request);
        }
    }

    fn take_managed(&mut self, request: RequestId) -> Option<Managed> {
        let managed = self.managed_http.remove(&request)?;
        self.timers.remove(&managed.timer);
        self.actions.push(Action::ClearTimer {
            timer: managed.timer,
        });
        self.actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Cancel {
                preparation: managed.lifecycle.preparation(),
            })));
        Some(managed)
    }

    fn discard_managed(&mut self, request: RequestId) -> Option<PendingHttp<P>> {
        self.take_managed(request)?;
        self.streams.remove(&request);
        self.incoming.remove(&request);
        #[cfg(feature = "order-locks")]
        self.order_read_tokens.remove(&request);
        self.pending.remove(&request)
    }
}

impl<V: Venue> Engine<V> {
    /// Hosts check this before inserting an asynchronous prepared resource.
    /// A duplicate/late completion must not overwrite its replacement.
    pub fn http_preparation_pending(&self, token: Preparation) -> bool {
        self.io
            .managed_http
            .get(&token.attempt().request)
            .is_some_and(|managed| managed.lifecycle.expects_preparation(token))
    }

    /// Preparation is a host resource, never evidence of transport dispatch.
    pub fn http_prepared(&mut self, now: f64, token: Preparation, prepared: Prepared) {
        self.io.dispatch_now = now;
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token {
            return;
        }
        let prepared_result = self
            .io
            .request_contexts
            .validate(managed.lifecycle.context(), now)
            .and_then(|()| managed.lifecycle.prepared(token, prepared))
            .and_then(|()| managed.lifecycle.unsigned())
            .and_then(|()| managed.lifecycle.permit());
        match prepared_result {
            Ok(permit) => self
                .io
                .actions
                .push(Action::ManagedHttp(Box::new(HttpAction::Bind {
                    preparation: token,
                    permit,
                    request: managed.original.clone(),
                }))),
            Err(Error::State) => {} // Duplicate readiness cannot replace an issued permit.
            Err(error) => self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP preparation is no longer valid"),
            ),
        }
    }

    /// A typed pre-claim failure is distinct from a response transport failure.
    /// Old preparation failures cannot remove or settle a replacement resource.
    pub fn http_preparation_failed(
        &mut self,
        now: f64,
        token: Preparation,
        reason: Error,
        mut error: VenueError,
    ) {
        self.io.dispatch_now = now;
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token || managed.lifecycle.release().is_some() {
            return;
        }
        if reason == Error::Stale {
            self.reprepare_http(now, token);
            return;
        }
        error.outcome = Some(crate::Outcome::NotSent);
        if error.kind.is_none() {
            error.kind = Some(match reason {
                Error::TimedOut => ErrorKind::Timeout,
                Error::Invalid | Error::Revoked | Error::Duplicate | Error::RetryForbidden => {
                    ErrorKind::InvalidInput
                }
                _ => ErrorKind::Internal,
            });
        }
        if let Some(pending) = self.io.discard_managed(request) {
            self.venue.on_http_preparation_failed(
                &mut Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                },
                pending.value,
                reason,
                error,
            );
        }
    }

    /// The host checks its bound resource, calls this, and immediately invokes
    /// its transport API on Released, with no intervening await or actor hop.
    pub fn claim_http(&mut self, now: f64, permit: Permit) -> Result<Claim, Error> {
        self.io.dispatch_now = now;
        let request = permit.attempt().request;
        let managed = self
            .io
            .managed_http
            .get_mut(&request)
            .ok_or(Error::Revoked)?;
        if !managed.lifecycle.is_current_permit(permit) {
            return Err(Error::Revoked);
        }
        let preparation = managed.lifecycle.preparation();
        if let Err(error) = self
            .io
            .request_contexts
            .validate(managed.lifecycle.context(), now)
        {
            self.http_preparation_failed(
                now,
                preparation,
                error,
                VenueError::new("HTTP request context is no longer active"),
            );
            return Err(error);
        }
        let pending = self.io.pending.get_mut(&request).ok_or(Error::Revoked)?;
        let time = ReleaseTime {
            raw_ms: now,
            protocol_ms: self.io.protocol_clock.at(now),
            revision: self.io.protocol_clock.revision(),
        };
        let result =
            managed
                .lifecycle
                .claim(permit, time, &mut self.io.http_pacing, &mut |release| {
                    self.venue.on_http_release(&mut pending.value, release)
                });
        if let Ok(Claim::Released(release)) = result {
            #[cfg(feature = "order-locks")]
            {
                if managed.lifecycle.context().operation() == Operation::Mutation {
                    self.io.order_locks.note_write();
                }
                self.io
                    .order_read_tokens
                    .insert(request, self.io.order_locks.token());
            }
            self.venue.on_signing_receipt(
                SigningTarget::Http {
                    request,
                    pending: &mut pending.value,
                },
                release.signature,
            );
        }
        if let Err(error) = result {
            self.http_preparation_failed(
                now,
                preparation,
                error,
                VenueError::new("HTTP request was not released"),
            );
        }
        result
    }

    fn reprepare_http(&mut self, now: f64, token: Preparation) {
        let request = token.attempt().request;
        let Some(managed) = self.io.managed_http.get_mut(&request) else {
            return;
        };
        if managed.lifecycle.preparation() != token || managed.lifecycle.release().is_some() {
            return;
        }
        let next = self
            .io
            .request_contexts
            .validate(managed.lifecycle.context(), now)
            .and_then(|()| managed.lifecycle.reprepare());
        if let Err(error) = next {
            self.http_preparation_failed(
                now,
                token,
                error,
                VenueError::new("HTTP request cannot be prepared again"),
            );
            return;
        }
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Cancel {
                preparation: token,
            })));
        let mut http = managed.original.clone();
        http.timeout_ms = Some(
            (managed.lifecycle.context().deadline() - now)
                .ceil()
                .max(1.) as u32,
        );
        self.io
            .actions
            .push(Action::ManagedHttp(Box::new(HttpAction::Prepare {
                preparation: managed.lifecycle.preparation(),
                intent: managed.lifecycle.context().routing_intent(),
                request: http,
            })));
    }

    /// Admission metadata for a host resource or reducer. Reading it grants no
    /// permission to send and does not renew any response lease.
    pub fn http_release(&self, request: RequestId) -> Option<HttpRelease> {
        self.io.managed_http.get(&request)?.lifecycle.release()
    }

    pub(super) fn expire_http_timer(&mut self, now: f64, event: &Event<'_>) -> bool {
        let Event::TimerFired { timer } = event else {
            return false;
        };
        let Some((request, managed)) = self
            .io
            .managed_http
            .iter()
            .find(|(_, managed)| managed.timer == *timer)
        else {
            return false;
        };
        let request = *request;
        if now.is_finite() && now < managed.lifecycle.context().deadline() {
            self.io.actions.push(Action::SetTimer {
                timer: *timer,
                at_ms: managed.lifecycle.context().deadline(),
            });
            return true;
        }
        if managed.lifecycle.release().is_some() {
            self.handle_event(
                now,
                Event::HttpFailed {
                    request,
                    error: "HTTP request timed out",
                },
            );
            self.io.discard_managed(request);
        } else {
            self.http_preparation_failed(
                now,
                managed.lifecycle.preparation(),
                Error::TimedOut,
                VenueError::new("HTTP request timed out"),
            );
        }
        true
    }

    pub(super) fn admit_http_event(&mut self, now: f64, event: &Event<'_>) -> bool {
        let request = match event {
            Event::HttpResponse { request, .. }
            | Event::HttpStart { request, .. }
            | Event::HttpChunk { request, .. }
            | Event::HttpEnd { request }
            | Event::HttpFailed { request, .. } => *request,
            _ => return true,
        };
        let Some(managed) = self.io.managed_http.get(&request) else {
            return true;
        };
        if !now.is_finite() || now >= managed.lifecycle.context().deadline() {
            let timer = managed.timer;
            self.expire_http_timer(now, &Event::TimerFired { timer });
            return false;
        }
        if !managed.lifecycle.accepts_response() {
            return false;
        }
        let started = managed.response_started;
        match event {
            Event::HttpResponse { .. } | Event::HttpStart { .. } if started => false,
            Event::HttpChunk { .. } | Event::HttpEnd { .. } if !started => false,
            Event::HttpStart { .. } => {
                self.io
                    .managed_http
                    .get_mut(&request)
                    .expect("managed request exists")
                    .response_started = true;
                true
            }
            _ => true,
        }
    }

    pub(super) fn settle_http_event(&mut self, request: RequestId) {
        self.io.discard_managed(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request_context::{Admission, Pacing, Policy, RetryPolicy, Scope};

    #[derive(Debug)]
    struct Pending(Context);
    #[derive(Default)]
    struct Probe {
        count: usize,
        command: Option<CommandId>,
        deny: bool,
        responses: usize,
        items: usize,
        failures: Vec<(Error, Option<crate::Outcome>)>,
        releases: Vec<HttpRelease>,
        receipts: usize,
        finish_in_item: Option<bool>,
    }
    const GROUP: Pacing = Pacing {
        group: 1,
        minimum_gap_ms: 450.,
    };
    impl Venue for Probe {
        type Pending = Pending;
        fn http_pacing(&self) -> &[Pacing] {
            &[GROUP]
        }
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            for _ in 0..self.count {
                let scope = self.command.map_or(Scope::Background, Scope::Command);
                let context = cx
                    .http_context(
                        scope,
                        Policy {
                            operation: Operation::Read,
                            credential: None,
                            timeout_ms: 1_000,
                            pacing: Some(GROUP),
                            retry: RetryPolicy::Never,
                        },
                    )
                    .unwrap();
                let request = cx
                    .request_managed(
                        context.clone(),
                        Method::Post,
                        "https://venue.test/read".into(),
                        Some("{}".into()),
                        Pending(context),
                    )
                    .unwrap();
                cx.stream_response(request, ArrayAt::Root);
            }
        }
        fn on_http_release(
            &mut self,
            _: &mut Pending,
            release: &HttpRelease,
        ) -> Result<Admission, Error> {
            self.releases.push(*release);
            Ok(if self.deny {
                Admission::Denied
            } else {
                Admission::Unscheduled
            })
        }
        fn on_http_preparation_failed(
            &mut self,
            _: &mut Cx<'_, Pending>,
            _: Pending,
            reason: Error,
            error: VenueError,
        ) {
            self.failures.push((reason, error.outcome));
        }
        fn on_signing_receipt(
            &mut self,
            _: SigningTarget<'_, Pending>,
            _: crate::signing::SignReceipt,
        ) {
            self.receipts += 1;
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, _: Response<'_>) {
            self.responses += 1;
            cx.finish_http_context(&pending.0).unwrap();
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, _: &mut Pending, _: &str) {
            self.items += 1;
            if let Some(fail) = self.finish_in_item {
                let command = self.command.unwrap();
                if fail {
                    cx.fail(command, "stopped during streaming");
                } else {
                    cx.done(command, "null".into());
                }
            }
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {
            panic!("unexpected transport failure");
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            if let Command::FetchLeverage { command, .. } = command {
                cx.done(command, "null".into());
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {
            panic!("engine timer leaked to venue");
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    fn start(probe: Probe) -> (Engine<Probe>, Vec<Preparation>) {
        let mut engine = crate::test_support::seeded_engine(probe);
        engine.start(0.);
        let tokens = engine
            .drain_actions()
            .filter_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare { preparation, .. } => Some(preparation),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        (engine, tokens)
    }
    fn bind(engine: &mut Engine<Probe>, token: Preparation, at: f64) -> Permit {
        engine.http_prepared(
            at,
            token,
            Prepared {
                id: u64::from(token.attempt().request.0),
                expires_at: None,
            },
        );
        engine
            .drain_actions()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap()
    }
    fn response(engine: &mut Engine<Probe>, token: Preparation, at: f64) {
        engine.handle(
            at,
            Event::HttpResponse {
                request: token.attempt().request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: "[1]",
                },
            },
        );
    }

    #[test]
    fn preclaim_responses_cannot_reach_streams_auth_or_reducers_and_post_reads_stay_reads() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let token = tokens[0];
        response(&mut engine, token, 1.);
        assert_eq!(engine.venue().responses, 0);
        assert_eq!(engine.venue().items, 0);
        let permit = bind(&mut engine, token, 2.);
        assert_eq!(engine.venue().receipts, 0);
        response(&mut engine, token, 3.);
        assert_eq!(engine.venue().responses, 0);
        #[cfg(feature = "order-locks")]
        let before = engine.io.order_locks.token();
        assert!(matches!(
            engine.claim_http(4., permit),
            Ok(Claim::Released(_))
        ));
        #[cfg(feature = "order-locks")]
        assert_eq!(engine.io.order_locks.token(), before);
        assert_eq!(engine.venue().receipts, 1);
        response(&mut engine, token, 5.);
        assert_eq!(engine.venue().responses, 1);
        assert_eq!(engine.venue().items, 1);
        response(&mut engine, token, 6.);
        assert_eq!(engine.venue().responses, 1);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
    }

    #[test]
    fn final_claim_starts_gap_and_repeated_waits_consume_no_admission() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            ..Probe::default()
        });
        let first = bind(&mut engine, tokens[0], 20.);
        let second = bind(&mut engine, tokens[1], 30.);
        assert!(engine.venue().releases.is_empty());
        assert!(matches!(
            engine.claim_http(500., first),
            Ok(Claim::Released(_))
        ));
        for now in [500., 600., 949.] {
            assert_eq!(engine.claim_http(now, second), Ok(Claim::WaitUntil(950.)));
        }
        assert_eq!(engine.venue().releases.len(), 1);
        response(&mut engine, tokens[0], 501.);
        assert!(matches!(
            engine.claim_http(950., second),
            Ok(Claim::Released(_))
        ));
        assert_eq!(engine.venue().releases.len(), 2);
        assert_eq!(engine.claim_http(951., second), Err(Error::Revoked));
    }

    #[test]
    fn denial_and_typed_setup_failure_settle_once_without_response_failure_or_gap() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            deny: true,
            ..Probe::default()
        });
        let first = bind(&mut engine, tokens[0], 1.);
        assert_eq!(engine.claim_http(2., first), Err(Error::Revoked));
        assert_eq!(
            engine.venue().failures,
            [(Error::Revoked, Some(crate::Outcome::NotSent))]
        );
        engine.http_preparation_failed(3., tokens[0], Error::Invalid, VenueError::new("late"));
        assert_eq!(engine.venue().failures.len(), 1);
        engine.venue.deny = false;
        let second = bind(&mut engine, tokens[1], 3.);
        assert!(matches!(
            engine.claim_http(3., second),
            Ok(Claim::Released(_))
        ));
    }

    #[test]
    fn stale_route_reprepares_without_extending_deadline_and_old_cleanup_cannot_win() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let old = tokens[0];
        let old_permit = bind(&mut engine, old, 1.);
        engine.http_preparation_failed(100., old, Error::Stale, VenueError::new("route expired"));
        let actions = engine.drain_actions().collect::<Vec<_>>();
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::ManagedHttp(action)
            if matches!(**action, HttpAction::Cancel {preparation} if preparation == old)))
        );
        let replacement = actions
            .into_iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert_eq!(request.timeout_ms, Some(900));
                        Some(preparation)
                    }
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        assert_ne!(replacement, old);
        assert!(!engine.http_preparation_pending(old));
        assert!(engine.http_preparation_pending(replacement));
        engine.http_preparation_failed(101., old, Error::Invalid, VenueError::new("old failure"));
        let permit = bind(&mut engine, replacement, 102.);
        assert_eq!(engine.claim_http(103., old_permit), Err(Error::Revoked));
        assert!(matches!(
            engine.claim_http(104., permit),
            Ok(Claim::Released(_))
        ));
        assert!(engine.venue().failures.is_empty());
    }

    #[test]
    fn original_timer_bounds_lost_preparation_and_permit_callbacks() {
        for ready in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                ..Probe::default()
            });
            let token = tokens[0];
            if ready {
                bind(&mut engine, token, 1.);
            }
            let timer = engine
                .io
                .managed_http
                .get(&token.attempt().request)
                .unwrap()
                .timer;
            engine.handle(1_000., Event::TimerFired { timer });
            assert_eq!(
                engine.venue().failures,
                [(Error::TimedOut, Some(crate::Outcome::NotSent))]
            );
            engine.handle(1_001., Event::TimerFired { timer });
            engine.http_prepared(
                1_001.,
                token,
                Prepared {
                    id: 9,
                    expires_at: None,
                },
            );
            response(&mut engine, token, 1_002.);
            assert_eq!(engine.venue().failures.len(), 1);
            assert_eq!(engine.venue().responses, 0);
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.managed_http.is_empty());
        }
    }

    #[test]
    fn command_completion_cancels_unreleased_children_without_a_second_result() {
        let (mut engine, tokens) = start(Probe {
            count: 2,
            command: Some(7),
            ..Probe::default()
        });
        let permit = bind(&mut engine, tokens[0], 1.);
        engine.handle(
            2.,
            Event::Command(Command::FetchLeverage {
                command: 7,
                symbol: "BTC".into(),
            }),
        );
        assert_eq!(engine.claim_http(3., permit), Err(Error::Revoked));
        assert!(!engine.http_preparation_pending(tokens[1]));
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
        assert!(engine.venue().failures.is_empty());
        let results = engine
            .drain_actions()
            .filter(|action| {
                matches!(
                    action,
                    Action::Notice(Notice::CommandDone { command: 7, .. })
                )
            })
            .count();
        assert_eq!(results, 1);
    }

    #[test]
    fn retirement_in_first_streamed_row_stops_same_chunk_and_cannot_resurrect_pending() {
        for fail in [false, true] {
            let (mut engine, tokens) = start(Probe {
                count: 1,
                command: Some(7),
                finish_in_item: Some(fail),
                ..Probe::default()
            });
            let permit = bind(&mut engine, tokens[0], 1.);
            assert!(matches!(
                engine.claim_http(2., permit),
                Ok(Claim::Released(_))
            ));
            engine.handle(
                3.,
                Event::HttpResponse {
                    request: tokens[0].attempt().request,
                    response: Response {
                        status: 200,
                        headers: "",
                        body: "[1,2]",
                    },
                },
            );
            assert_eq!(engine.venue().items, 1);
            assert_eq!(engine.venue().responses, 0);
            assert!(engine.io.pending.is_empty());
            assert!(engine.io.incoming.is_empty());
            assert!(engine.io.managed_http.is_empty());
            response(&mut engine, tokens[0], 4.);
            assert_eq!(engine.venue().items, 1);
            assert_eq!(engine.venue().responses, 0);
            let terminal = engine
                .drain_actions()
                .filter(|action| {
                    matches!(
                        action,
                        Action::Notice(
                            Notice::CommandDone { command: 7, .. }
                                | Notice::CommandFailed { command: 7, .. }
                        )
                    )
                })
                .count();
            assert_eq!(terminal, 1);
        }
    }

    #[test]
    fn managed_response_order_rejects_early_end_and_duplicate_start_without_losing_pending() {
        let (mut engine, tokens) = start(Probe {
            count: 1,
            ..Probe::default()
        });
        let token = tokens[0];
        let request = token.attempt().request;
        let permit = bind(&mut engine, token, 1.);
        assert!(matches!(
            engine.claim_http(2., permit),
            Ok(Claim::Released(_))
        ));
        engine.handle(3., Event::HttpEnd { request });
        assert!(
            engine.io.pending.contains_key(&request),
            "early end must not consume continuation"
        );
        assert!(engine.io.managed_http.contains_key(&request));
        engine.handle(
            4.,
            Event::HttpChunk {
                request,
                data: b"[99]",
            },
        );
        assert_eq!(engine.venue().items, 0);
        engine.handle(
            5.,
            Event::HttpStart {
                request,
                status: 200,
                headers: "first: yes",
            },
        );
        engine.handle(
            6.,
            Event::HttpChunk {
                request,
                data: b"[1,",
            },
        );
        assert_eq!(engine.venue().items, 1);
        engine.handle(
            7.,
            Event::HttpStart {
                request,
                status: 500,
                headers: "duplicate: no",
            },
        );
        engine.handle(
            8.,
            Event::HttpChunk {
                request,
                data: b"2]",
            },
        );
        assert_eq!(engine.venue().items, 2);
        engine.handle(9., Event::HttpEnd { request });
        assert_eq!(engine.venue().responses, 1);
        engine.handle(10., Event::HttpEnd { request });
        assert_eq!(engine.venue().responses, 1);
        assert!(engine.io.pending.is_empty());
        assert!(engine.io.managed_http.is_empty());
    }

    #[test]
    fn unsigned_entry_refuses_mutations_until_command_ledger_wiring_preserving_continuation() {
        let mut io = Io::<Pending>::default();
        let mut store = Store::default();
        let mut cx = Cx {
            now: 0.,
            io: &mut io,
            store: &mut store,
        };
        let context = cx
            .http_context(
                Scope::Command(7),
                Policy {
                    operation: Operation::Mutation,
                    credential: None,
                    timeout_ms: 1_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap();
        let result = cx.request_managed(
            context.clone(),
            Method::Post,
            "https://venue.test/write".into(),
            None,
            Pending(context.clone()),
        );
        assert!(matches!(result, Err((Pending(returned), Error::Invalid)) if returned == context));
        assert!(cx.io.pending.is_empty());
        assert!(cx.io.managed_http.is_empty());
        assert!(cx.io.actions.is_empty());
        assert!(cx.io.timers.is_empty());
        cx.finish_http_context(&context).unwrap();
    }
}
