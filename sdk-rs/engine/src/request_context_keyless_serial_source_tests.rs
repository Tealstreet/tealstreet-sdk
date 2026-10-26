// Additional controls execute the source boundaries hidden behind entry REDs.
struct ForbiddenRecipe;
impl crate::SignStep for ForbiddenRecipe {
    fn freshness_ms(&self) -> f64 {
        panic!("a keyless root must not inspect signing freshness")
    }

    fn nonce_claim(&self) -> Option<&crate::nonce::NonceClaim> {
        panic!("a keyless root must not inspect or reserve a nonce")
    }
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        panic!("a keyless root must not prepare provider work")
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        panic!("a keyless root must not apply a provider result")
    }
}

#[test]
fn keyless_recipe_is_refused_before_even_reading_nonce_authority() {
    let mut e = admitted(Mode::Forbidden);
    assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    local_session_refusal(&mut e);
}

#[test]
fn legacy_sign_http_cannot_attach_to_a_keyless_managed_request() {
    let mut e = admitted(Mode::LegacyAttach);
    assert!(e.venue.legacy_unchanged);
    let actions = e.drain_actions().collect();
    let request = drive(&mut e, actions, 1.).pop().unwrap();
    assert_eq!(request.body.as_deref(), Some(CANCEL));
    assert_eq!(request.headers, vec![("Cookie", "cookie-1".into())]);
}

#[test]
fn keyless_read_resend_needs_no_write_fact_or_signing_work() {
    let mut e = admitted(Mode::ReadAuth);
    let actions = e.drain_actions().collect();
    let first = drive(&mut e, actions, 1.).pop().unwrap();
    reply(&mut e, first.id, "auth");
    assert!(e.venue.read_fact_absent);
    assert_eq!(e.venue.selection, Some(Ok(())));
    let actions = e.drain_actions().collect();
    assert!(drive(&mut e, actions, 1.).is_empty());
    e.handle(
        2.,
        Event::TimerFired {
            timer: e.venue.timer.unwrap(),
        },
    );
    let actions = e.drain_actions().collect();
    let second = drive(&mut e, actions, 2.).pop().unwrap();
    assert_eq!(first.body, second.body);
    assert_eq!(second.headers, vec![("Cookie", "cookie-2".into())]);
    reply_at(&mut e, second.id, "ok", 3.);
    let actions = e.drain_actions().collect();
    let leverage = drive(&mut e, actions, 3.).pop().unwrap();
    assert_eq!(leverage.body.as_deref(), Some(LEVERAGE));
}

#[test]
#[cfg(feature = "condition-waits")]
fn parked_session_preflight_retires_without_waking_or_latching() {
    let mut e = ready(Mode::ParkRetire);
    enter(&mut e);
    assert_eq!(e.venue.entry, Some(Ok(())));
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Http(_)))
    );
    e.handle(
        2.,
        Event::TimerFired {
            timer: e.venue.timer.unwrap(),
        },
    );
    assert_eq!(e.venue.selection, Some(Err(Error::Revoked)));
    local_session_refusal(&mut e);
}

#[test]
fn keyless_prepare_equality_expiry_is_not_sent_and_late_completion_is_inert() {
    let mut e = admitted(Mode::Chain);
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    let preparation = actions
        .into_iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        20_001.,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    assert_eq!(e.venue.issue, Some(Err(Error::TimedOut)));
    assert_eq!(e.venue.errors.len(), 1);
    assert_eq!(e.venue.errors[0].kind, Some(ErrorKind::Timeout));
    assert_eq!(e.venue.errors[0].outcome, Some(crate::Outcome::NotSent));
    local_session_refusal(&mut e);
    e.http_prepared(
        20_002.,
        preparation,
        Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_)
            | Action::Http(_)
            | Action::Notice(Notice::CommandFailed { command: 7, .. })
    )));
}

#[test]
fn changed_auth_resend_body_is_refused_before_any_preparation_or_id() {
    let mut e = admitted(Mode::AuthChangedBody);
    let actions = e.drain_actions().collect();
    let first = drive(&mut e, actions, 1.).pop().unwrap();
    reply(&mut e, first.id, "auth");
    assert_eq!(e.venue.selection, Some(Ok(())));
    assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
    assert_eq!(e.health().auth_failures, 0);
    assert!(!e.io.core.signing.refused());
    let failure = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(failure.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(
        failure.outcome,
        Some(crate::Outcome::Unknown),
        "released prior attempt remains journal-owned"
    );
}

#[test]
fn nonfinite_session_entry_refuses_before_ids_or_signing_work() {
    let mut e = ready(Mode::Chain);
    e.handle(
        f64::NAN,
        Event::Command(Command::CancelOrdersWithSymbols {
            command: 7,
            orders: vec![],
        }),
    );
    assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    local_session_refusal(&mut e);
}

#[test]
fn installed_secondary_does_not_sign_or_route_a_keyless_read() {
    let mut e = admitted(Mode::ReadWithSecondary);
    assert!(e.identities().secondary.is_some());
    assert!(e.identities().primary.is_none());
    let context = e.venue.preflight.as_ref().unwrap().owner();
    let session = e.venue.session.as_ref().unwrap();
    let generation =
        e.io.core
            .request_contexts
            .validate_session(session)
            .unwrap();
    assert!(
        matches!(e.io.core.request_contexts.keyless_session_egress(context).unwrap(), Some(crate::request_context::Egress::Session(egress)) if egress.generation() == generation)
    );
    let actions = e.drain_actions().collect();
    let request = drive(&mut e, actions, 1.).pop().unwrap();
    assert_eq!(request.headers, vec![("Cookie", "cookie-1".into())]);
    assert!(request.signer_headers.is_empty());
    assert_ne!(request.route, Some(crate::RequestRoute::Secondary));
}

#[test]
fn late_refused_attempt_delivery_cannot_select_or_spend_again() {
    let mut e = admitted(Mode::Auth);
    let actions = e.drain_actions().collect();
    let first = drive(&mut e, actions, 1.).pop().unwrap();
    reply(&mut e, first.id, "auth");
    assert_eq!(e.venue.selection, Some(Ok(())));
    let queued: Vec<_> = e.drain_actions().collect();
    no_sign(&queued);
    let before = e.io.core.next_id;
    reply(&mut e, first.id, "auth");
    assert_eq!(e.venue.selected, 1);
    assert_eq!(e.io.core.next_id, before);
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Http(_)))
    );
}

#[test]
fn retirement_after_accepted_call_seals_the_command_with_journal_projection() {
    let mut e = admitted(Mode::RetireBetween);
    let actions = e.drain_actions().collect();
    let first = drive(&mut e, actions, 1.).pop().unwrap();
    reply(&mut e, first.id, "ok");
    assert_eq!(e.venue.calls, 1);
    assert_eq!(e.venue.selection, Some(Err(Error::Revoked)));
    assert_eq!(e.venue.before, e.venue.after);
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(
        failures[0].outcome, None,
        "accepted first effect is not whole-command NotSent"
    );
    assert!(
        e.io.core.request_contexts.command_writes(7).is_none(),
        "sealed journal retired"
    );
    assert_eq!(e.health().auth_failures, 0);
    assert!(!e.io.core.signing.refused());
    later_signed_stages(&mut e);
}

#[test]
fn a_keyless_refusal_grant_cannot_authorize_a_signed_resend() {
    let mut e = admitted(Mode::SignedResend);
    let actions = e.drain_actions().collect();
    let first = drive(&mut e, actions, 1.).pop().unwrap();
    reply(&mut e, first.id, "auth");
    assert_eq!(e.venue.selection, Some(Ok(())));
    assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    let actions: Vec<_> = e.drain_actions().collect();
    no_sign(&actions);
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
    assert_eq!(e.health().auth_failures, 0);
    assert!(!e.io.core.signing.refused());
}

#[test]
#[cfg(feature = "sign-meta")]
fn a_secondary_selection_cannot_authorize_signing_a_keyless_root() {
    let mut e = admitted(Mode::Selected);
    assert!(e.identities().secondary.is_some());
    assert_eq!(e.venue.issue, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    local_session_refusal(&mut e);
}

#[test]
fn session_entry_cannot_mix_in_a_write_credential() {
    let mut e = ready(Mode::MixedAtEntry);
    enter(&mut e);
    assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
    assert_eq!(e.venue.before, e.venue.after);
    local_session_refusal(&mut e);
}

#[test]
fn ordinary_signed_staging_is_a_working_control_before_any_session_refusal() {
    let mut e = ready(Mode::Chain);
    later_signed_stages(&mut e);
}

#[test]
fn keyless_entry_requires_a_declared_credential_free_session_egress() {
    for mode in [Mode::NoSessionEgress, Mode::CredentialSession] {
        let mut e = ready(mode);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Err(Error::Invalid)));
        assert_eq!(e.venue.before, e.venue.after);
        local_session_refusal(&mut e);
    }
}

#[test]
fn no_keyless_roots_means_no_retirement_scan_visits() {
    let mut e = ready(Mode::Chain);
    assert_eq!(e.io.core.request_contexts.keyless_root_count(), 0);
    let before = e.io.core.request_contexts.keyless_scan_visits();
    later_signed_stages(&mut e);
    e.flush_keyless_session_roots(30_001.);
    assert_eq!(e.io.core.request_contexts.keyless_scan_visits(), before);
}

#[test]
fn empty_root_completion_releases_the_scan_fast_path() {
    for mode in [Mode::EmptyDone, Mode::EmptyFail] {
        let mut e = ready(mode);
        enter(&mut e);
        assert_eq!(e.venue.entry, Some(Ok(())));
        assert_eq!(e.io.core.request_contexts.keyless_root_count(), 0);
        let before = e.io.core.request_contexts.keyless_scan_visits();
        e.flush_keyless_session_roots(3.);
        assert_eq!(e.io.core.request_contexts.keyless_scan_visits(), before);
    }
}

#[test]
#[cfg(feature = "condition-waits")]
fn retired_preflight_waits_until_expiry_and_command_custody_end() {
    let mut e = ready(Mode::ParkRetire);
    enter(&mut e);
    e.handle_command(
        1.,
        Command::CancelOrdersWithSymbols {
            command: 9,
            orders: vec![],
        },
        None,
    )
    .unwrap();
    let session = e.venue.session.clone().unwrap();
    assert!(
        Cx {
            now: 2.,
            store: &mut e.store,
            io: &mut e.io
        }
        .retire_session(&session)
        .unwrap()
        .is_empty()
    );
    let before = e.io.core.request_contexts.keyless_scan_visits();
    e.io.core.http_plan_expiring = e.venue.expiry_plan.clone();
    e.flush_keyless_session_roots(2.);
    assert_eq!(e.io.core.request_contexts.keyless_scan_visits(), before);
    assert_eq!(e.io.core.request_contexts.keyless_root_count(), 1);
    e.io.core.http_plan_expiring = None;
    e.io.begin_command_entry(Some(10));
    e.flush_keyless_session_roots(2.);
    assert_eq!(e.io.core.request_contexts.keyless_scan_visits(), before);
    assert_eq!(e.io.core.request_contexts.keyless_root_count(), 1);
    e.io.core.command_entry = None;
    let retired = e.venue.retirements;
    e.flush_keyless_session_roots(2.);
    assert_eq!(e.io.core.request_contexts.keyless_root_count(), 0);
    assert_eq!(
        e.venue.retirements,
        retired + 1,
        "retirement callback delivered at the same flush"
    );
    local_session_refusal(&mut e);
}
