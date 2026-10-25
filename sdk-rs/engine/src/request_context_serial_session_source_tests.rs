// Source-stage controls extend the reviewed originals without weakening them.
mod source_controls {
    use super::*;

    #[test]
    fn floor_is_the_released_revision_not_the_current_cache() {
        let mut e = start(false, 2);
        let first = prep(&drain(&mut e));
        let packet = release(&mut e, first, 0.);
        e.venue.revision = 2;
        reply(&mut e, first, 1., 200, r#"{"code":1000}"#);
        assert_eq!(e.venue.selected, vec![Ok(())]);
        let second = prep(&drain(&mut e));
        let retried = release(&mut e, second, 2.);
        assert_eq!(retried.body, packet.body);
        assert!(
            retried
                .headers
                .iter()
                .any(|(k, v)| *k == "Authorization" && v == "token-2")
        );
    }

    #[test]
    fn native_codec_statuses_never_consult_the_exemption_hook() {
        for status in [200, 201, 400, 401, 403, 404, 429] {
            let mut e = start(false, 1);
            let first = prep(&drain(&mut e));
            release(&mut e, first, 0.);
            reply(&mut e, first, 1., status, r#"{"code":0}"#);
            assert_eq!(e.venue.exempt_calls.get(), 0, "status {status}");
        }
    }

    #[test]
    fn every_unproven_status_consults_the_full_body_before_evidence() {
        for status in [408, 500, 502, 503, 504, 599] {
            let mut e = start(false, 2);
            auth(&mut e, status);
            assert_eq!(e.venue.exempt_calls.get(), 1, "status {status}");
            assert_eq!(e.venue.selected, vec![Ok(())]);
            assert_eq!(e.venue.unproven.get(), 0);
        }
    }

    #[test]
    fn read_two_attempts_remain_generic_retry_never() {
        let mut e = start(true, 2);
        assert_eq!(e.venue.admitted, Some(Ok(())));
        let plan = e.venue.plan.clone().unwrap();
        let slot = e.venue.slot.clone().unwrap();
        let context = e.venue.context.clone().unwrap();
        let before = e.io.core.next_id;
        let mut cx = Cx {
            now: 0.,
            store: &mut e.store,
            io: &mut e.io,
        };
        assert_eq!(
            cx.plan_retry_context(
                &plan,
                &slot,
                &context,
                crate::request_context::RetryEvidence::ReadFailure
            ),
            Err(Error::RetryForbidden)
        );
        assert_eq!(cx.io.core.next_id, before);
    }

    #[test]
    fn released_read_integrity_failure_is_not_an_immediate_network_retry() {
        let mut e = start(true, 1);
        e.venue.immediate = true;
        let first = prep(&drain(&mut e));
        release(&mut e, first, 0.);
        e.handle(
            1.,
            Event::HttpStart {
                request: first.attempt().request,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            1.,
            Event::HttpChunk {
                request: first.attempt().request,
                data: &[0xff],
            },
        );
        e.handle(
            1.,
            Event::HttpEnd {
                request: first.attempt().request,
            },
        );
        let actions = drain(&mut e);
        assert!(e.venue.selected.iter().all(Result::is_err));
        no_http(&actions);
        failed(&actions);
    }
    #[test]
    fn prior_serial_ids_survive_a_parked_resend_expiry() {
        let mut e = start(false, 1);
        e.venue.continue_after_success = true;
        let first = prep(&drain(&mut e));
        release(&mut e, first, 0.);
        reply(&mut e, first, 1., 200, r#"{"code":0}"#);
        let next = prep(&drain(&mut e));
        release(&mut e, next, 2.);
        reply(&mut e, next, 3., 200, r#"{"code":1000}"#);
        let resend = prep(&drain(&mut e));
        assert!(bind(&mut e, resend, 4.).is_none());
        let timer = e.venue.park_timer.unwrap();
        e.handle(20_004., Event::TimerFired { timer });
        let actions = drain(&mut e);
        assert_eq!(failed(&actions).outcome, Some(crate::Outcome::Unknown));
        assert_eq!(
            failed(&actions).partial.as_ref().unwrap().order_ids,
            ["id-1"]
        );
        assert_eq!(
            e.venue.preparation_errors.last().unwrap().outcome,
            Some(crate::Outcome::NotSent)
        );
    }
}
