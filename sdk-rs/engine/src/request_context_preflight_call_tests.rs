// Probe pendings are whole Contexts, returned by refused managed requests.
#[allow(clippy::result_large_err)]
mod preflight_calls {
    use super::*;
    use crate::request_context::{Preflight, PreflightPolicy, PreflightPurpose, RetryEvidence,
        HttpOwnerRetirement, plan::{CallTerms, RetryWindow}};

    #[derive(Default)]
    struct Calls {
        owner: Option<Preflight>,
        context: Option<Context>,
        signed: bool,
        shared: bool,
        second: bool,
        retry_failure: bool,
        explicit_failure: bool,
        leave_ready: bool,
        publications: usize,
        responses: usize,
        failures: usize,
        retired: usize,
        stale: Option<Context>,
    }
    fn policy(signed: bool) -> Policy {
        Policy { operation: Operation::Read, credential: signed.then(|| crate::request_context::Credential {
            slot: CredentialSlot::Primary, reference: crate::signing::KeyRef("fixture".into()),
        }), timeout_ms: 20_000, pacing: None, retry: RetryPolicy::ReadFailure }
    }
    fn terms(shared: bool) -> CallTerms {
        CallTerms { wait_ms: 0, timeout_ms: 20_000, attempts: 2,
            retry_window: if shared { RetryWindow::Shared } else { RetryWindow::FreshAttempt } }
    }
    struct LongRecipe;
    impl crate::SignStep for LongRecipe {
        fn prepare(&mut self, outbound: &mut Action, cx: &crate::DispatchContext<'_>)
            -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            let mut work = Recipe.prepare(outbound, cx)?;
            work.receipt.expiry_ms = Some(cx.stamp_ms() + 40_000.);
            Ok(work)
        }
        fn apply(&mut self, outbound: &mut Action, result: &crate::signing::SignResult)
            -> Result<(), crate::signing::SignError> { Recipe.apply(outbound, result) }
        fn freshness_ms(&self) -> f64 { 40_000. }
    }
    struct BadStaticHeaders;
    impl crate::SignStep for BadStaticHeaders {
        fn freshness_ms(&self) -> f64 { 40_000. }
        fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
            &[("bad\nname", "value")]
        }
        fn prepare(&mut self, _: &mut Action, _: &crate::DispatchContext<'_>)
            -> Result<crate::signing::SignRequest, crate::signing::SignError> { panic!("invalid headers reached provider preparation") }
        fn apply(&mut self, _: &mut Action, _: &crate::signing::SignResult)
            -> Result<(), crate::signing::SignError> { panic!("invalid headers reached provider result") }
    }
    fn queue(cx: &mut Cx<'_, Context>, context: Context, signed: bool) -> Result<RequestId, (Context, Error)> {
        if signed {
            cx.request_signed_managed(context.clone(), None, Method::Get, "https://venue.test/read".into(), None, Box::new(LongRecipe), context)
        } else {
            cx.request_managed(context.clone(), None, Method::Get, "https://venue.test/read".into(), None, context)
        }
    }
    impl Calls {
        fn failure(&mut self, cx: &mut Cx<'_, Context>, context: Context) {
            self.failures += 1;
            let owner = self.owner.as_ref().unwrap();
            assert!(cx.validate_preflight_delivery(owner, &context).is_err());
            assert!(cx.done_preflight(owner, "wrong".into()).is_err());
            let before = cx.io.core.next_id;
            assert!(cx.preflight_read_context(owner, policy(self.signed), terms(self.shared)).is_err());
            assert_eq!(cx.io.core.next_id, before);
            if self.retry_failure {
                self.retry_failure = false;
                let retry = cx.preflight_retry_context(owner, &context, RetryEvidence::ReadFailure).unwrap();
                if !self.leave_ready { queue(cx, retry, self.signed).unwrap(); }
            } else if self.explicit_failure {
                cx.fail_preflight(owner, "native explicit failure").unwrap();
            }
        }
    }
    impl Venue for Calls {
        type Pending = Context;
        fn start(&mut self, _: &mut Cx<'_, Context>) {}
        fn on_command(&mut self, cx: &mut Cx<'_, Context>, _: Command) {
            let p = policy(self.signed);
            let owner = cx.begin_preflight(PreflightPolicy { purpose: PreflightPurpose::ReadOnly,
                read_credential: p.credential.clone(), write_credential: None }).unwrap();
            let context = cx.preflight_read_context(&owner, p, terms(self.shared)).unwrap();
            queue(cx, context.clone(), self.signed).unwrap();
            self.owner = Some(owner);
            self.context = Some(context);
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Context>, context: Context, response: Response<'_>) {
            self.responses += 1;
            let owner = self.owner.as_ref().unwrap();
            cx.validate_preflight_delivery(owner, &context).unwrap();
            if response.body == "retry" {
                let retry = cx.preflight_retry_context(owner, &context, RetryEvidence::ReadFailure).unwrap();
                assert!(cx.preflight_retry_context(owner, &context, RetryEvidence::ReadFailure).is_err());
                queue(cx, retry, self.signed).unwrap();
                assert!(cx.validate_preflight_delivery(owner, &context).is_err());
                return;
            }
            self.publications += 1;
            cx.store.set_hedged(true);
            cx.finish_preflight_call(owner, &context).unwrap();
            assert!(cx.validate_preflight_delivery(owner, &context).is_err());
            if self.second {
                self.second = false;
                self.stale = Some(context);
                let next = cx.preflight_read_context(owner, policy(self.signed), terms(self.shared)).unwrap();
                assert_eq!(next.owner(), self.context.as_ref().unwrap().owner());
                assert_ne!(next.leg(), self.context.as_ref().unwrap().leg());
                self.context = Some(next.clone());
                queue(cx, next, self.signed).unwrap();
            } else { cx.done_preflight(owner, "validated".into()).unwrap(); }
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: &str) { self.failure(cx, context); }
        fn on_http_preparation_failed(&mut self, cx: &mut Cx<'_, Context>, context: Context, _: Error, _: VenueError) { self.failure(cx, context); }
        fn on_http_owner_retired(&mut self, _: &HttpOwnerRetirement) { self.retired += 1; }
        fn on_open(&mut self, _: &mut Cx<'_, Context>, _: SocketId) {}
        fn on_close(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: u16) {}
        fn on_message(&mut self, _: &mut Cx<'_, Context>, _: SocketId, _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, Context>, _: TimerId) { panic!("physical timer leaked") }
        fn book(&self, _: &str) -> Option<&Book> { None }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> { Box::new(std::iter::empty()) }
    }
    fn cx<T>(engine: &mut Engine<Calls>, now: f64, f: impl FnOnce(&mut Cx<'_, Context>) -> T) -> T {
        f(&mut Cx { now, store: &mut engine.store, io: &mut engine.io })
    }
    fn prepared(actions: &[Action]) -> (Preparation, HttpRequest) {
        actions.iter().find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, request, .. } => Some((*preparation, request.clone())), _ => None,
            }, _ => None,
        }).expect("owned preparation")
    }
    fn start(probe: Calls) -> (Engine<Calls>, Vec<Action>) {
        let mut engine = crate::test_support::seeded_engine(probe);
        engine.start(0.);
        engine.handle(0., Event::Command(Command::FetchCandles { command: 7,
            symbol: "BTCUSD".into(), interval: Interval::M1, limit: Some(2),
            from_ms: None, to_ms: None }));
        let actions = engine.drain_actions().collect();
        (engine, actions)
    }
    fn release(engine: &mut Engine<Calls>, token: Preparation, now: f64) {
        engine.http_prepared(now, token, Prepared { id: token.attempt().request.0, expires_at: None });
        let mut actions = engine.drain_actions().collect::<Vec<_>>();
        if engine.venue.signed {
            let (attempt, work) = work(&actions);
            engine.handle(now, Event::Signed { attempt, result: Ok(crate::signing::SignResult::new(vec![b"value".to_vec()], work.receipt)) });
            actions = engine.drain_actions().collect();
        }
        let permit = actions.iter().find_map(|a| match a { Action::ManagedHttp(a) => match a.as_ref() {
            HttpAction::Bind { permit, .. } => Some(*permit), _ => None,
        }, _ => None }).expect("owned binding");
        assert!(matches!(engine.claim_http(now, permit), Ok(Claim::Released(_))));
    }
    fn response(engine: &mut Engine<Calls>, token: Preparation, now: f64, body: &str) {
        engine.handle(now, Event::HttpResponse { request: token.attempt().request,
            response: Response { status: 200, headers: "", body } });
    }
    fn terminal(actions: &[Action]) -> usize {
        actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, .. } | Notice::CommandFailed { command: 7, .. }))).count()
    }

    #[test]
    fn initial_packet_timer_lifecycle_and_signer_use_selected_physical_cap() {
        for signed in [false, true] {
            let (mut e, actions) = start(Calls { signed, ..Default::default() });
            let (token, packet) = prepared(&actions);
            assert_eq!(packet.timeout_ms, Some(20_000));
            assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { at_ms: 20_000., .. })));
            let managed = &e.io.core.managed_http[&token.attempt().request];
            assert_eq!(managed.lifecycle.deadline(), 20_000.);
            assert_eq!(managed.lifecycle.context().deadline(), 40_000.);
            if signed {
                e.http_prepared(0., token, Prepared { id: 9, expires_at: None });
                let work = e.drain_actions().collect::<Vec<_>>();
                assert!(work.iter().any(|a| matches!(a, Action::Sign { timeout_ms: 20_000, .. })));
            }
        }
    }
    #[test]
    fn fresh_and_shared_retry_windows_survive_real_native_delivery_without_new_generation() {
        for shared in [false, true] {
            let (mut e, actions) = start(Calls { shared, ..Default::default() });
            let (first, _) = prepared(&actions);
            let owner = e.venue.owner.clone().unwrap();
            release(&mut e, first, 0.);
            response(&mut e, first, 19_000., "retry");
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(terminal(&actions), 0);
            let (second, packet) = prepared(&actions);
            assert_eq!(packet.timeout_ms, Some(if shared { 1_000 } else { 20_000 }));
            assert_ne!(first.attempt(), second.attempt());
            assert_eq!(e.venue.owner.as_ref().unwrap().owner(), owner.owner());
            release(&mut e, second, 19_000.);
            response(&mut e, second, if shared { 19_500. } else { 38_000. }, "ok");
            let final_actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(terminal(&final_actions), 1);
            assert_eq!(e.venue.publications, 1);
            assert_eq!(e.venue.retired, 1);
        }
    }
    #[test]
    fn generic_clone_and_retirement_apis_cannot_bypass_owned_call() {
        let (mut e, _) = start(Calls::default());
        let context = e.venue.context.clone().unwrap();
        let owner = e.venue.owner.clone().unwrap();
        let before = e.io.core.next_id;
        cx(&mut e, 1., |cx| {
            assert_eq!(queue(cx, context.clone(), false).unwrap_err().1, Error::Revoked);
            assert!(cx.http_leg_context(&context, policy(false)).is_err());
            assert!(cx.http_retry_context(&context, RetryEvidence::ReadFailure).is_err());
            assert!(cx.finish_http_context(&context).is_err());
            assert!(cx.done_context(&context, "wrong".into()).is_err());
            assert!(cx.validate_http_context(&context).is_err());
            assert!(cx.done_preflight(&owner, "wrong".into()).is_err());
        });
        assert_eq!(e.io.core.next_id, before);
        assert_eq!(e.io.core.managed_http.len(), 1);
        assert_eq!(e.venue.retired, 0);
    }
    #[test]
    fn two_successful_calls_keep_generation_and_reject_the_old_delivery() {
        let (mut e, actions) = start(Calls { second: true, ..Default::default() });
        let (first, _) = prepared(&actions);
        release(&mut e, first, 0.);
        response(&mut e, first, 19_000., "ok");
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminal(&actions), 0);
        let (second, packet) = prepared(&actions);
        assert_eq!(packet.timeout_ms, Some(20_000));
        response(&mut e, first, 19_001., "ok");
        assert_eq!(e.venue.responses, 1);
        release(&mut e, second, 19_001.);
        response(&mut e, second, 38_000., "ok");
        assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 1);
        assert_eq!(e.venue.responses, 2);
        assert_eq!(e.venue.publications, 2);
        assert_eq!(e.venue.retired, 1);
    }
    #[test]
    fn ignored_failure_seals_once_but_a_callback_retry_survives_the_old_fallback() {
        for (retry_failure, explicit_failure) in [(false, false), (false, true), (true, false)] {
            let (mut e, actions) = start(Calls { retry_failure, explicit_failure, ..Default::default() });
            let (first, _) = prepared(&actions);
            release(&mut e, first, 0.);
            e.handle(19_000., Event::HttpFailed { request: first.attempt().request, error: "network lost" });
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(e.venue.failures, 1);
            assert_eq!(terminal(&actions), usize::from(!retry_failure));
            if retry_failure {
                let (second, packet) = prepared(&actions);
                assert_eq!(packet.timeout_ms, Some(20_000));
                release(&mut e, second, 19_000.);
                response(&mut e, second, 38_000., "ok");
                assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 1);
                assert_eq!(e.venue.publications, 1);
            } else {
                assert_eq!(e.venue.publications, 0);
                if !explicit_failure {
                    assert!(actions.iter().any(|action| matches!(action,
                        Action::Notice(Notice::CommandFailed { error, .. }) if error.kind == Some(ErrorKind::Network))));
                }
            }
            assert_eq!(e.venue.retired, 1);
        }
    }

    #[test]
    fn whole_call_reservation_refuses_atomically_in_either_actual_id_namespace() {
        for io_short in [true, false] {
            let mut e = crate::test_support::seeded_engine(Calls::default());
            let p = policy(true);
            let owner = cx(&mut e, 0., |cx| cx.issue_preflight(7, PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly, read_credential: p.credential.clone(), write_credential: None,
            })).unwrap();
            if !io_short {
                e.io.core.signing.set_test_ids(crate::resource_ids::ResourceIds::with_ceiling(1));
            }
            if io_short {
                // Advance to a real near-exhaustion range, retaining the
                // existing generation rather than rewinding an issued ID.
                e.io.core.next_id = crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID - 5);
            }
            let before = e.io.core.next_id;
            assert_eq!(cx(&mut e, 0., |cx| cx.preflight_read_context(&owner, p.clone(), terms(false))), Err(Error::Capacity));
            assert_eq!(e.io.core.next_id, before);
            assert!(e.io.core.managed_http.is_empty());
            assert!(e.io.core.actions.is_empty());
            // A one-attempt declaration fits the unchanged limiting namespace;
            // no allocator is reset after refusal.
            let one = CallTerms { attempts: 1, ..terms(false) };
            let context = cx(&mut e, 0., |cx| cx.preflight_read_context(&owner, p, one)).unwrap();
            cx(&mut e, 0., |cx| queue(cx, context, true)).unwrap();
            let (token, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            let mut expected = before;
            assert_eq!(token.attempt().request.0, expected.reserve(1).unwrap().start + 1);
            assert_eq!(e.io.core.managed_http[&token.attempt().request].signing_attempt, Some(RequestId(1)));
        }
    }

    #[test]
    fn ready_call_can_fail_before_queue_and_after_its_window_without_success_or_reset() {
        for now in [1., 50_000.] {
            let mut e = crate::test_support::seeded_engine(Calls::default());
            let owner = cx(&mut e, 0., |cx| cx.issue_preflight(7, PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly, read_credential: None, write_credential: None,
            })).unwrap();
            cx(&mut e, 0., |cx| cx.preflight_read_context(&owner, policy(false), terms(false))).unwrap();
            let before = e.io.core.next_id;
            cx(&mut e, now, |cx| {
                assert!(cx.done_preflight(&owner, "wrong".into()).is_err());
                assert!(cx.preflight_read_context(&owner, policy(false), terms(false)).is_err());
                cx.fail_preflight(&owner, "cancelled before queue").unwrap();
            });
            assert_eq!(e.io.core.next_id, before);
            assert!(e.io.core.managed_http.is_empty());
            assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 1);
            assert_eq!(e.venue.retired, 1);
        }
    }

    #[test]
    fn exact_physical_expiry_and_pruned_chain_keep_failure_only_custody() {
        for at in [20_000., 50_000.] {
            let (mut e, actions) = start(Calls::default());
            let (token, _) = prepared(&actions);
            release(&mut e, token, 0.);
            let timer = e.io.core.managed_http[&token.attempt().request].timer;
            if at > 40_000. {
                cx(&mut e, at, |cx| cx.http_context(Scope::Background, policy(false))).unwrap();
            }
            e.handle(at, Event::TimerFired { timer });
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(terminal(&actions), 1);
            assert!(actions.iter().any(|action| matches!(action,
                Action::Notice(Notice::CommandFailed { error, .. }) if error.kind == Some(ErrorKind::Timeout))));
            assert_eq!(e.venue.publications, 0);
            assert_eq!(e.venue.retired, 1);
            response(&mut e, token, at + 1., "ok");
            assert_eq!(e.venue.responses, 0);
            assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 0);
        }
    }

    #[test]
    fn selected_retry_can_abort_after_expiry_without_queueing_or_resetting_credits() {
        let (mut e, actions) = start(Calls { retry_failure: true, leave_ready: true, ..Default::default() });
        let (first, _) = prepared(&actions);
        release(&mut e, first, 0.);
        e.handle(19_000., Event::HttpFailed { request: first.attempt().request, error: "network lost" });
        assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 0);
        assert!(e.io.core.managed_http.is_empty());
        let owner = e.venue.owner.clone().unwrap();
        let before = e.io.core.next_id;
        cx(&mut e, 50_000., |cx| {
            assert!(cx.done_preflight(&owner, "wrong".into()).is_err());
            assert!(cx.preflight_read_context(&owner, policy(false), terms(false)).is_err());
            cx.fail_preflight(&owner, "retry preparation cancelled").unwrap();
        });
        assert_eq!(e.io.core.next_id, before);
        assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 1);
        assert_eq!(e.venue.retired, 1);
    }

    #[test]
    fn selected_queue_refusal_seals_centrally_without_a_native_finalizer() {
      for bad_headers in [false, true] {
        let mut e = crate::test_support::seeded_engine(Calls::default());
        let p = policy(true);
        let owner = cx(&mut e, 0., |cx| cx.issue_preflight(7, PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: p.credential.clone(), write_credential: p.credential.clone(),
        })).unwrap();
        let context = cx(&mut e, 0., |cx| cx.preflight_read_context(&owner, p, terms(false))).unwrap();
        let before = e.io.core.next_id;
        let refusal = cx(&mut e, 0., |cx| {
            if bad_headers {
                cx.request_signed_managed(context.clone(), None, Method::Get, "https://venue.test/read".into(), None, Box::new(BadStaticHeaders), context)
            } else { queue(cx, context, false) }
        });
        assert_eq!(refusal.unwrap_err().1, Error::Invalid);
        assert_eq!(e.io.core.next_id, before);
        assert!(e.io.core.managed_http.is_empty());
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminal(&actions), 1);
        assert!(actions.iter().all(|a| !matches!(a, Action::ManagedHttp(_) | Action::Sign { .. })));
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. })
            if error.outcome == Some(crate::Outcome::NotSent))));
        assert_eq!(e.venue.failures, 0);
        assert_eq!(e.venue.retired, 1);
        assert!(cx(&mut e, 1., |cx| cx.fail_preflight(&owner, "duplicate")).is_err());
      }
    }

    #[test]
    fn identical_foreign_delivery_cannot_borrow_a_ready_retry_or_its_failure_authority() {
        let (mut e, actions) = start(Calls { retry_failure: true, leave_ready: true, ..Default::default() });
        let (first, _) = prepared(&actions);
        release(&mut e, first, 0.);
        let (mut foreign, foreign_actions) = start(Calls::default());
        let (foreign_first, _) = prepared(&foreign_actions);
        release(&mut foreign, foreign_first, 0.);
        let wrong = foreign.io.core.managed_http[&foreign_first.attempt().request].delivery();
        let actual = e.io.core.managed_http[&first.attempt().request].delivery();
        assert_eq!(wrong.context, actual.context); // Equal public metadata.
        assert_eq!(wrong.attempt, actual.attempt);
        assert_ne!(wrong, actual); // Private issuers remain distinct.
        e.handle(19_000., Event::HttpFailed { request: first.attempt().request, error: "network lost" });
        let context = e.venue.context.clone().unwrap();
        let owner = e.venue.owner.clone().unwrap();
        let before = e.io.core.next_id;
        e.io.core.http_delivery = Some(wrong.clone());
        cx(&mut e, 19_001., |cx| {
            assert!(cx.validate_preflight_delivery(&owner, &context).is_err());
            assert!(cx.preflight_retry_context(&owner, &context, RetryEvidence::ReadFailure).is_err());
            assert_eq!(queue(cx, context, false).unwrap_err().1, Error::Revoked);
            assert!(cx.fail_preflight(&owner, "borrowed").is_err());
            assert_eq!(cx.io.core.http_delivery.as_ref(), Some(&wrong));
        });
        e.io.core.http_delivery = None;
        assert_eq!(e.io.core.next_id, before);
        assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 0);
        cx(&mut e, 50_000., |cx| cx.fail_preflight(&owner, "cancelled")).unwrap();
        assert_eq!(terminal(&e.drain_actions().collect::<Vec<_>>()), 1);
        assert_eq!(e.venue.retired, 1);
    }

    fn failure_outcome(actions: &[Action]) -> Option<crate::Outcome> {
        actions.iter().find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error.outcome),
            _ => None,
        }).expect("command failure")
    }
    fn write_preflight(e: &mut Engine<Calls>, inherited: bool) -> (Preflight, Context) {
        let owner = cx(e, 0., |cx| cx.issue_preflight(7, PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None, write_credential: policy(true).credential,
        })).unwrap();
        if inherited {
            cx(e, 0., |cx| cx.inherit_preflight_uncertainty(&owner)).unwrap();
        }
        let context = cx(e, 0., |cx| cx.preflight_read_context(&owner, policy(false), terms(false))).unwrap();
        (owner, context)
    }

    #[test]
    fn inherited_uncertainty_survives_the_central_read_admission_refusal() {
        for inherited in [false, true] {
            let mut e = crate::test_support::seeded_engine(Calls::default());
            let (_, context) = write_preflight(&mut e, inherited);
            let refusal = cx(&mut e, 0., |cx| cx.request_signed_managed(context.clone(), None, Method::Get,
                "https://venue.test/read".into(), None, Box::new(BadStaticHeaders), context));
            assert_eq!(refusal.unwrap_err().1, Error::Invalid);
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert!(actions.iter().all(|a| !matches!(a, Action::ManagedHttp(_) | Action::Sign { .. })));
            assert_eq!(failure_outcome(&actions), Some(if inherited {
                crate::Outcome::Unknown } else { crate::Outcome::NotSent }));
        }
    }

    #[test]
    fn inherited_uncertainty_survives_an_engine_sealed_read_failure() {
        for inherited in [false, true] {
            let mut e = crate::test_support::seeded_engine(Calls::default());
            let (owner, context) = write_preflight(&mut e, inherited);
            cx(&mut e, 0., |cx| queue(cx, context, false)).unwrap();
            e.venue.owner = Some(owner);
            let (first, _) = prepared(&e.drain_actions().collect::<Vec<_>>());
            release(&mut e, first, 0.);
            e.handle(25_000., Event::HttpFailed { request: first.attempt().request, error: "network lost" });
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(terminal(&actions), 1);
            assert_eq!(failure_outcome(&actions), Some(if inherited {
                crate::Outcome::Unknown } else { crate::Outcome::NotSent }));
        }
    }
}
