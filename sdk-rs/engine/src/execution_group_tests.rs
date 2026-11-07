use super::*;
use crate::OrderIdentityKind;

fn member(command: CommandId, generation: u64, deadline: f64, key: &str) -> Member {
    Member {
        command,
        generation,
        operation: Operation::Read,
        deadline,
        key: key.into(),
    }
}

fn write(command: CommandId, generation: u64, deadline: f64) -> Member {
    Member {
        operation: Operation::Mutation,
        ..member(command, generation, deadline, "")
    }
}

fn read(groups: &mut Groups, founder: Member, timeout_ms: u32) -> (Execution, Observer) {
    let (execution, observer) = groups
        .begin(
            Root::Command(founder),
            Operation::Read,
            Window::UntilTerminal,
            timeout_ms,
            0.,
        )
        .expect("begin");
    (execution, observer.expect("founder observer"))
}

fn receipt(identity: &str) -> OrderIdentityReceipt {
    OrderIdentityReceipt {
        symbol: "BTCUSDT".into(),
        kind: OrderIdentityKind::ClientOrderId,
        identity: identity.into(),
    }
}

fn owners(sealed: &[Sealed]) -> Vec<(CommandId, u64)> {
    sealed.iter().map(|s| (s.command, s.generation)).collect()
}

#[test]
fn joined_read_seals_each_live_owner_once_with_its_projection() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 500., "BTC"), 1_000);
    let (_, joined) = groups
        .join(execution, member(2, 20, 500., "ETH"), 1.)
        .unwrap();
    assert_eq!(joined, Joined::Live(vec![]));
    let sealed = groups
        .complete(execution, |key| Ok(format!("{key}-rails")))
        .unwrap();
    assert_eq!(owners(&sealed), [(1, 10), (2, 20)]);
    assert_eq!(sealed[0].result, Ok("BTC-rails".into()));
    assert_eq!(sealed[1].result, Ok("ETH-rails".into()));
    assert!(sealed.iter().all(|s| !s.journal));
    assert_eq!(
        groups.complete(execution, |_| Ok(String::new())),
        Err(Error::Stale)
    );
    assert_eq!(
        groups.abort(execution, VenueError::new("late")),
        Err(Error::Stale)
    );
    assert_eq!(groups.member(1), None);
    assert_eq!(groups.member(2), None);
}

#[test]
fn execution_bound_is_capped_by_founder_and_never_raised_by_a_join() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 300., "a"), 1_000);
    assert_eq!(groups.deadline(execution), Some(300.));
    groups
        .join(execution, member(2, 20, 5_000., "b"), 1.)
        .unwrap();
    assert_eq!(groups.deadline(execution), Some(300.));
    let (other, _) = read(&mut groups, member(3, 30, 5_000., "c"), 200);
    assert_eq!(groups.deadline(other), Some(200.));
}

#[test]
fn expired_observer_detaches_alone_and_siblings_still_complete() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.join(execution, member(2, 20, 80., "b"), 1.).unwrap();
    groups.leg_issued(execution).unwrap();
    assert_eq!(groups.timer(execution), Some(80.));
    let expired = groups.expire(execution, 80.);
    assert_eq!(owners(&expired), [(2, 20)]);
    assert_eq!(
        expired[0].result.as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    assert!(groups.expire(execution, 81.).is_empty());
    let sealed = groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(owners(&sealed), [(1, 10)]);
}

#[test]
fn cancelled_founder_leaves_the_follower_attached() {
    let mut groups = Groups::default();
    let (execution, founder) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups
        .join(execution, member(2, 20, 900., "b"), 1.)
        .unwrap();
    let sealed = groups
        .detach(founder, VenueError::new("cancelled"))
        .unwrap();
    assert_eq!((sealed.command, sealed.generation), (1, 10));
    assert!(groups.may_publish(execution));
    assert_eq!(
        groups.detach(founder, VenueError::new("again")),
        Err(Error::Revoked)
    );
    let sealed = groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(owners(&sealed), [(2, 20)]);
}

#[test]
fn one_timer_owns_the_execution_deadline_only_while_no_leg_is_in_flight() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert_eq!(groups.timer(execution), Some(900.));
    groups.leg_issued(execution).unwrap();
    assert_eq!(groups.timer(execution), None);
    groups
        .join(execution, member(2, 20, 400., "b"), 1.)
        .unwrap();
    assert_eq!(groups.timer(execution), Some(400.));
    groups.leg_retired(execution).unwrap();
    assert_eq!(groups.timer(execution), Some(400.));
    // An observer at the execution bound is owned by the execution term only.
    groups
        .join(execution, member(3, 30, 900., "c"), 2.)
        .unwrap();
    groups.expire(execution, 400.);
    assert_eq!(groups.timer(execution), Some(900.));
}

#[test]
fn execution_deadline_without_a_leg_seals_every_remaining_owner() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups
        .join(execution, member(2, 20, 900., "b"), 1.)
        .unwrap();
    assert!(groups.expire(execution, 899.).is_empty());
    let sealed = groups.expire(execution, 900.);
    assert_eq!(owners(&sealed), [(1, 10), (2, 20)]);
    assert!(
        sealed
            .iter()
            .all(|s| s.result.as_ref().unwrap_err().kind == Some(ErrorKind::Timeout))
    );
    assert_eq!(
        groups.complete(execution, |_| Ok(String::new())),
        Err(Error::Stale)
    );
}

#[test]
fn the_execution_deadline_seals_owners_before_a_same_instant_leg_timeout() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.leg_issued(execution).unwrap();
    groups.leg_released(execution).unwrap();
    assert!(groups.expire(execution, 899.).is_empty());
    let sealed = groups.expire(execution, 900.);
    assert_eq!(owners(&sealed), [(1, 10)]);
    assert_eq!(
        sealed[0].result.as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    // The leg's own timeout then finds nothing left to seal.
    groups.leg_retired(execution).unwrap();
    assert!(groups.expire(execution, 900.).is_empty());
}

#[test]
fn mutation_expiry_projects_not_sent_only_before_any_release() {
    let mut groups = Groups::default();
    let (unsent, _) = groups
        .begin(
            Root::Command(write(1, 10, 900.)),
            Operation::Mutation,
            Window::UntilTerminal,
            1_000,
            0.,
        )
        .unwrap();
    let sealed = groups.expire(unsent, 900.);
    let error = sealed[0].result.as_ref().unwrap_err();
    assert!(sealed[0].journal);
    assert_eq!(error.outcome, Some(Outcome::NotSent));

    let (sent, _) = groups
        .begin(
            Root::Command(write(2, 20, 900.)),
            Operation::Mutation,
            Window::UntilTerminal,
            1_000,
            0.,
        )
        .unwrap();
    groups.leg_issued(sent).unwrap();
    groups.leg_released(sent).unwrap();
    groups.join(sent, write(3, 30, 500.), 1.).unwrap();
    let sealed = groups.expire(sent, 500.);
    assert_eq!(owners(&sealed), [(3, 30)]);
    assert!(sealed[0].journal);
    assert_eq!(sealed[0].result.as_ref().unwrap_err().outcome, None);
}

#[test]
fn mutation_abort_after_release_cannot_claim_a_definitive_outcome() {
    let mut groups = Groups::default();
    let (execution, _) = groups
        .begin(
            Root::Command(write(1, 10, 900.)),
            Operation::Mutation,
            Window::UntilTerminal,
            1_000,
            0.,
        )
        .unwrap();
    groups.leg_issued(execution).unwrap();
    groups.leg_released(execution).unwrap();
    let rejected = VenueError::new("refused")
        .kind(ErrorKind::Rejected)
        .outcome(Outcome::Rejected);
    let sealed = groups.abort(execution, rejected).unwrap();
    let error = sealed[0].result.as_ref().unwrap_err();
    assert!(sealed[0].journal);
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::Rejected), None)
    );
}

#[test]
fn late_join_replays_retained_progress_once_and_duplicate_join_is_idempotent() {
    let mut groups = Groups::default();
    let (execution, _) = groups
        .begin(
            Root::Command(write(1, 10, 900.)),
            Operation::Mutation,
            Window::UntilTerminal,
            1_000,
            0.,
        )
        .unwrap();
    let fanned = groups.progress(execution, vec![receipt("link-1")]).unwrap();
    assert_eq!(fanned, [(1, 10, vec![receipt("link-1")])]);
    let (observer, joined) = groups.join(execution, write(2, 20, 900.), 1.).unwrap();
    assert_eq!(joined, Joined::Live(vec![receipt("link-1")]));
    let (again, joined) = groups.join(execution, write(2, 20, 900.), 2.).unwrap();
    assert_eq!((again, joined), (observer, Joined::Already));
    assert_eq!(groups.deadline(execution), Some(900.));
    let fanned = groups.progress(execution, vec![receipt("link-2")]).unwrap();
    assert_eq!(owners_of(&fanned), [(1, 10), (2, 20)]);
}

fn owners_of(fanned: &[(CommandId, u64, Vec<OrderIdentityReceipt>)]) -> Vec<(CommandId, u64)> {
    fanned.iter().map(|(c, g, _)| (*c, *g)).collect()
}

#[test]
fn reused_command_id_cannot_borrow_an_old_observer_generation() {
    let mut groups = Groups::default();
    let (first, old) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert_eq!(
        groups.join(first, member(1, 11, 900., "a"), 1.),
        Err(Error::State)
    );
    groups.complete(first, |_| Ok("ok".into())).unwrap();
    let (second, _) = read(&mut groups, member(1, 11, 900., "a"), 1_000);
    assert_eq!(groups.member(1), Some((second, 11)));
    assert_eq!(
        groups.detach(old, VenueError::new("old")),
        Err(Error::Revoked)
    );
    assert_eq!(groups.member(1), Some((second, 11)));
}

#[test]
fn a_member_cannot_observe_a_second_execution() {
    let mut groups = Groups::default();
    let (first, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    let (second, _) = read(&mut groups, member(2, 20, 900., "b"), 1_000);
    assert_eq!(
        groups.join(second, member(1, 10, 900., "a"), 1.),
        Err(Error::State)
    );
    assert_eq!(groups.member(1), Some((first, 10)));
    let sealed = groups.complete(second, |_| Ok("ok".into())).unwrap();
    assert_eq!(owners(&sealed), [(2, 20)]);
}

#[test]
fn joins_reject_terminal_mismatched_expired_or_oversized_owners_atomically() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert_eq!(
        groups.join(execution, write(2, 20, 900.), 1.),
        Err(Error::Invalid)
    );
    assert_eq!(
        groups.join(execution, member(3, 30, 1., "c"), 1.),
        Err(Error::TimedOut)
    );
    assert_eq!(
        groups.join(execution, member(4, 40, 900., &"k".repeat(MAX_KEY + 1)), 1.),
        Err(Error::Invalid)
    );
    assert_eq!(
        groups.join(execution, member(5, 50, f64::NAN, "e"), 1.),
        Err(Error::Invalid)
    );
    groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(
        groups.join(execution, member(6, 60, 900., "f"), 2.),
        Err(Error::Revoked)
    );
    for command in 2..=6 {
        assert_eq!(groups.member(command), None);
    }
}

#[test]
fn begin_rejects_invalid_policies_without_registering_anything() {
    let mut groups = Groups::default();
    let cases = [
        (
            Root::Background { deadline: 900. },
            Operation::Mutation,
            1_000,
            0.,
        ),
        (
            Root::Command(member(1, 10, 900., "a")),
            Operation::Read,
            0,
            0.,
        ),
        (
            Root::Command(member(1, 10, 900., "a")),
            Operation::Read,
            1_000,
            f64::NAN,
        ),
        (
            Root::Command(member(1, 10, 900., "a")),
            Operation::Mutation,
            1_000,
            0.,
        ),
        (
            Root::Command(member(1, 10, 0., "a")),
            Operation::Read,
            1_000,
            0.,
        ),
    ];
    for (root, operation, timeout, now) in cases {
        assert!(
            groups
                .begin(root, operation, Window::UntilTerminal, timeout, now)
                .is_err()
        );
    }
    assert_eq!(groups.member(1), None);
}

#[test]
fn observer_capacity_refusal_leaves_the_execution_unchanged() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 1, 900., "a"), 1_000);
    for command in 2..=MAX_OBSERVERS as CommandId {
        groups
            .join(execution, member(command, 1, 900., "a"), 1.)
            .unwrap();
    }
    let over = MAX_OBSERVERS as CommandId + 1;
    assert_eq!(
        groups.join(execution, member(over, 1, 900., "a"), 1.),
        Err(Error::Capacity)
    );
    assert_eq!(groups.member(over), None);
    let sealed = groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(sealed.len(), MAX_OBSERVERS);
}

#[test]
fn last_command_observer_gone_revokes_unsent_work_and_publication() {
    let mut groups = Groups::default();
    let (execution, founder) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups
        .detach(founder, VenueError::new("cancelled"))
        .unwrap();
    assert!(!groups.may_publish(execution));
    assert_eq!(groups.leg_issued(execution), Err(Error::Revoked));
}

#[test]
fn background_root_publishes_without_observers_and_never_carries_a_write() {
    let mut groups = Groups::default();
    let (execution, observer) = groups
        .begin(
            Root::Background { deadline: 900. },
            Operation::Read,
            Window::BeforeRelease,
            1_000,
            0.,
        )
        .unwrap();
    assert_eq!(observer, None);
    assert_eq!(groups.deadline(execution), Some(900.));
    assert!(groups.may_publish(execution));
    groups.leg_issued(execution).unwrap();
    let (observer, _) = groups
        .join(execution, member(1, 10, 500., "BTC"), 1.)
        .unwrap();
    groups
        .detach(observer, VenueError::new("cancelled"))
        .unwrap();
    assert!(groups.may_publish(execution));
}

#[test]
fn before_release_window_queues_later_joiners_for_the_next_background_read() {
    let mut groups = Groups::default();
    let (first, _) = groups
        .begin(
            Root::Background { deadline: 900. },
            Operation::Read,
            Window::BeforeRelease,
            1_000,
            0.,
        )
        .unwrap();
    groups
        .join(first, member(1, 10, 2_000., "BTC"), 1.)
        .unwrap();
    groups.leg_issued(first).unwrap();
    groups.leg_released(first).unwrap();
    let (_, joined) = groups
        .join(first, member(2, 20, 2_000., "ETH"), 2.)
        .unwrap();
    assert_eq!(joined, Joined::Pending);
    groups.join(first, member(3, 30, 50., "SOL"), 3.).unwrap();
    // The pending owner keeps its own deadline across the old read's terminal.
    let sealed = groups.complete(first, |key| Ok(key.to_owned())).unwrap();
    assert_eq!(owners(&sealed), [(1, 10)]);
    groups.leg_retired(first).unwrap();
    assert_eq!(groups.timer(first), Some(50.));
    let expired = groups.expire(first, 50.);
    assert_eq!(owners(&expired), [(3, 30)]);

    let (next, sealed) = groups.begin_successor(first, 1_500., 1_000, 100.).unwrap();
    assert!(sealed.is_empty());
    assert_eq!(groups.deadline(next), Some(1_100.));
    assert_eq!(groups.member(2), Some((next, 20)));
    let sealed = groups.complete(next, |key| Ok(key.to_owned())).unwrap();
    assert_eq!(owners(&sealed), [(2, 20)]);
    assert_eq!(sealed[0].result, Ok("ETH".into()));
}

#[test]
fn command_rooted_before_release_refuses_late_joiners() {
    let mut groups = Groups::default();
    let (execution, _) = groups
        .begin(
            Root::Command(member(1, 10, 900., "a")),
            Operation::Read,
            Window::BeforeRelease,
            1_000,
            0.,
        )
        .unwrap();
    groups.leg_issued(execution).unwrap();
    groups.leg_released(execution).unwrap();
    assert_eq!(
        groups.join(execution, member(2, 20, 900., "b"), 1.),
        Err(Error::State)
    );
    assert_eq!(groups.member(2), None);
}

#[test]
fn leg_accounting_rejects_unissued_or_unknown_legs() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert_eq!(groups.leg_released(execution), Err(Error::State));
    assert_eq!(groups.leg_retired(execution), Err(Error::State));
    groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(groups.leg_issued(execution), Err(Error::Revoked));
}

fn write_execution(groups: &mut Groups, founder: Member) -> (Execution, Observer) {
    let (execution, observer) = groups
        .begin(
            Root::Command(founder),
            Operation::Mutation,
            Window::UntilTerminal,
            1_000,
            0.,
        )
        .unwrap();
    (execution, observer.unwrap())
}

fn background(groups: &mut Groups, deadline: f64, now: f64) -> Execution {
    groups
        .begin(
            Root::Background { deadline },
            Operation::Read,
            Window::BeforeRelease,
            1_000,
            now,
        )
        .unwrap()
        .0
}

#[test]
fn progress_beyond_retained_capacity_reaches_live_observers_and_closes_late_joins() {
    let mut groups = Groups::default();
    let (execution, _) = write_execution(&mut groups, write(1, 10, 900.));
    let many = (0..MAX_PROGRESS).map(|i| receipt(&i.to_string())).collect();
    groups.progress(execution, many).unwrap();
    let fanned = groups.progress(execution, vec![receipt("over")]).unwrap();
    assert_eq!(fanned, [(1, 10, vec![receipt("over")])]);
    assert_eq!(
        groups.join(execution, write(2, 20, 900.), 1.),
        Err(Error::Capacity)
    );
    assert_eq!(groups.member(2), None);
}

#[test]
fn expired_last_observer_with_a_leg_in_flight_cannot_be_revived() {
    let mut groups = Groups::default();
    let (execution, founder) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.join(execution, member(2, 20, 80., "b"), 1.).unwrap();
    groups.leg_issued(execution).unwrap();
    groups
        .detach(founder, VenueError::new("cancelled"))
        .unwrap();
    assert_eq!(owners(&groups.expire(execution, 80.)), [(2, 20)]);
    assert!(!groups.may_publish(execution));
    assert_eq!(
        groups.join(execution, member(3, 30, 900., "c"), 81.),
        Err(Error::Revoked)
    );
    assert!(!groups.may_publish(execution));
    groups.leg_retired(execution).unwrap();
    assert_eq!(groups.deadline(execution), None);
}

#[test]
fn a_stale_token_cannot_detach_a_newer_membership_of_the_same_generation() {
    let mut groups = Groups::default();
    let (first, old) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.complete(first, |_| Ok("ok".into())).unwrap();
    let (second, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert_eq!(
        groups.detach(old, VenueError::new("old")),
        Err(Error::Revoked)
    );
    assert_eq!(groups.member(1), Some((second, 10)));
    assert!(groups.may_publish(second));
}

#[test]
fn a_mutation_observer_never_projects_not_sent_while_shared_work_may_still_send() {
    let mut groups = Groups::default();
    let (issued, _) = write_execution(&mut groups, write(1, 10, 900.));
    groups.join(issued, write(2, 20, 50.), 1.).unwrap();
    groups.leg_issued(issued).unwrap();
    let sealed = groups.expire(issued, 50.);
    assert_eq!(owners(&sealed), [(2, 20)]);
    assert_eq!(sealed[0].result.as_ref().unwrap_err().outcome, None);

    let (idle, founder) = write_execution(&mut groups, write(3, 30, 900.));
    groups.join(idle, write(4, 40, 900.), 1.).unwrap();
    let not_sent = VenueError::new("cancelled").outcome(Outcome::NotSent);
    let sealed = groups.detach(founder, not_sent.clone()).unwrap();
    assert_eq!(sealed.result.unwrap_err().outcome, None);
    let sealed = groups.abort(idle, not_sent).unwrap();
    assert_eq!(owners(&sealed), [(4, 40)]);
    assert_eq!(
        sealed[0].result.as_ref().unwrap_err().outcome,
        Some(Outcome::NotSent)
    );
}

#[test]
fn joins_after_the_execution_deadline_time_out() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.leg_issued(execution).unwrap();
    assert_eq!(
        groups.join(execution, member(2, 20, 5_000., "b"), 950.),
        Err(Error::TimedOut)
    );
    assert_eq!(groups.member(2), None);
}

#[test]
fn non_finite_expiry_changes_nothing() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    assert!(groups.expire(execution, f64::INFINITY).is_empty());
    assert!(groups.expire(execution, f64::NAN).is_empty());
    assert_eq!(groups.member(1), Some((execution, 10)));
    assert_eq!(groups.timer(execution), Some(900.));
}

#[test]
fn a_successor_needs_a_released_background_previous() {
    let mut groups = Groups::default();
    let (command, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    command_leg(&mut groups, command);
    assert_eq!(
        groups.begin_successor(command, 900., 1_000, 1.),
        Err(Error::Invalid)
    );
    let unreleased = background(&mut groups, 900., 0.);
    assert_eq!(
        groups.begin_successor(unreleased, 900., 1_000, 1.),
        Err(Error::State)
    );
    groups.complete(unreleased, |_| Ok(String::new())).unwrap();
    assert_eq!(
        groups.begin_successor(unreleased, 900., 1_000, 1.),
        Err(Error::Revoked)
    );
}

fn command_leg(groups: &mut Groups, execution: Execution) {
    groups.leg_issued(execution).unwrap();
    groups.leg_released(execution).unwrap();
}

#[test]
fn pending_joiners_keep_their_token_survive_abort_and_skip_progress() {
    let mut groups = Groups::default();
    let first = background(&mut groups, 900., 0.);
    command_leg(&mut groups, first);
    let (queued, _) = groups.join(first, member(1, 10, 5_000., "a"), 1.).unwrap();
    let (dropped, _) = groups.join(first, member(2, 20, 5_000., "b"), 1.).unwrap();
    groups.join(first, member(3, 30, 150., "c"), 1.).unwrap();
    assert!(
        groups
            .abort(first, VenueError::new("read failed"))
            .unwrap()
            .is_empty()
    );
    let sealed = groups
        .detach(dropped, VenueError::new("cancelled"))
        .unwrap();
    assert!(!sealed.journal);
    assert_eq!(groups.member(2), None);
    groups.leg_retired(first).unwrap();

    let (next, sealed) = groups.begin_successor(first, 5_000., 1_000, 100.).unwrap();
    assert!(sealed.is_empty());
    assert_eq!(
        groups
            .join(next, member(1, 10, 5_000., "a"), 101.)
            .unwrap()
            .1,
        Joined::Already
    );
    // An earlier own deadline keeps its timer; later ones are clamped to the bound.
    assert_eq!(groups.timer(next), Some(150.));
    assert_eq!(owners(&groups.expire(next, 150.)), [(3, 30)]);
    assert_eq!(groups.timer(next), Some(1_100.));
    groups.detach(queued, VenueError::new("cancelled")).unwrap();
    assert_eq!(groups.member(1), None);
}

#[test]
fn adopted_pending_owner_past_its_deadline_is_sealed_at_adoption() {
    let mut groups = Groups::default();
    let first = background(&mut groups, 900., 0.);
    command_leg(&mut groups, first);
    groups.join(first, member(1, 10, 50., "a"), 1.).unwrap();
    groups.complete(first, |_| Ok(String::new())).unwrap();
    let (_, sealed) = groups.begin_successor(first, 900., 1_000, 60.).unwrap();
    assert_eq!(owners(&sealed), [(1, 10)]);
    assert_eq!(groups.member(1), None);
}

#[test]
fn completion_with_a_leg_in_flight_retires_only_after_the_leg() {
    let mut groups = Groups::default();
    let (execution, _) = read(&mut groups, member(1, 10, 900., "a"), 1_000);
    groups.leg_issued(execution).unwrap();
    groups.complete(execution, |_| Ok("ok".into())).unwrap();
    assert_eq!(groups.deadline(execution), Some(900.));
    groups.leg_retired(execution).unwrap();
    assert_eq!(groups.deadline(execution), None);
    assert_eq!(groups.leg_retired(execution), Err(Error::Revoked));
}

#[test]
fn member_capacity_refusal_is_atomic_for_begin_and_join() {
    let mut groups = Groups::default();
    let mut executions = Vec::new();
    for command in 0..MAX_MEMBERS as CommandId {
        executions.push(read(&mut groups, member(command, 1, 900., "a"), 1_000).0);
    }
    let over = MAX_MEMBERS as CommandId;
    assert!(
        groups
            .begin(
                Root::Command(member(over, 1, 900., "a")),
                Operation::Read,
                Window::UntilTerminal,
                1_000,
                0.,
            )
            .is_err()
    );
    assert_eq!(
        groups.join(executions[0], member(over, 1, 900., "a"), 1.),
        Err(Error::Capacity)
    );
    assert_eq!(groups.member(over), None);
}
