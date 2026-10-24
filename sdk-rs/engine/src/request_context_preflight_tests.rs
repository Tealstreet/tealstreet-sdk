use super::*;
use crate::book::Book;
use crate::request_context::{
    HttpOwnerRetirement, Preflight, PreflightPolicy, PreflightPurpose, RetryPolicy,
};
use crate::{
    Action, Capabilities, Command, Engine, Event, Method, Notice, Response, SocketId, TimerId,
    Venue,
};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Default)]
struct Probe {
    retired: Vec<HttpOwnerRetirement>,
    panic_hook: bool,
    retire_on_start: bool,
    retire_on_command: bool,
    retire_on_public: bool,
    corrupt_queue: bool,
    caught_bookkeeping: bool,
    callback_owner: Option<Preflight>,
    callbacks: usize,
    queries: Cell<usize>,
}
impl Probe {
    fn retire_cached(&mut self, cx: &mut Cx<'_, ()>) {
        let owner = cx
            .issue_preflight(7, policy(PreflightPurpose::ReadOnly))
            .unwrap();
        self.callback_owner = Some(owner.clone());
        if self.corrupt_queue {
            cx.io.request_contexts.retirements = VecDeque::new();
            self.caught_bookkeeping = catch_unwind(AssertUnwindSafe(|| {
                cx.done_preflight(&owner, "cached".into())
            }))
            .is_err();
        } else {
            cx.done_preflight(&owner, "cached".into()).unwrap();
            assert!(matches!(
                cx.issue_preflight(7, policy(PreflightPurpose::ReadOnly)),
                Err(Error::State)
            ));
        }
    }
}

impl Venue for Probe {
    type Pending = ();
    fn on_http_owner_retired(&mut self, token: &HttpOwnerRetirement) {
        self.retired.push(token.clone());
        assert!(!self.panic_hook, "native cleanup panic control");
    }
    fn capabilities(&self) -> Capabilities {
        self.queries.set(self.queries.get() + 1);
        Capabilities::BASE
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.callbacks += 1;
        if self.retire_on_start {
            self.retire_cached(cx);
        }
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, ()>) {
        self.callbacks += 1;
        if self.retire_on_public {
            self.retire_cached(cx);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {
        self.callbacks += 1;
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {
        self.callbacks += 1;
    }
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {
        self.callbacks += 1;
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {
        self.callbacks += 1;
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        self.callbacks += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        self.callbacks += 1;
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        self.callbacks += 1;
        if self.retire_on_command {
            self.retire_cached(cx);
        }
    }
    fn book(&self, _: &str) -> Option<&Book> {
        self.queries.set(self.queries.get() + 1);
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.queries.set(self.queries.get() + 1);
        Box::new(std::iter::empty())
    }
}
fn engine() -> Engine<Probe> {
    crate::test_support::seeded_engine(Probe::default())
}
fn cx<R>(engine: &mut Engine<Probe>, now: f64, f: impl FnOnce(&mut Cx<'_, ()>) -> R) -> R {
    f(&mut Cx {
        now,
        store: &mut engine.store,
        io: &mut engine.io,
    })
}
fn policy(purpose: PreflightPurpose) -> PreflightPolicy {
    PreflightPolicy {
        purpose,
        read_credential: None,
        write_credential: (purpose == PreflightPurpose::WriteAfterReads).then(|| {
            crate::request_context::Credential {
                slot: crate::CredentialSlot::Primary,
                reference: crate::signing::KeyRef("owned-primary".into()),
            }
        }),
    }
}
fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 100,
        pacing: None,
        retry: RetryPolicy::ReadFailure,
    }
}
fn begin(engine: &mut Engine<Probe>, command: CommandId, purpose: PreflightPurpose) -> Preflight {
    cx(engine, 0., |cx| {
        cx.issue_preflight(command, policy(purpose))
    })
    .unwrap()
}

#[test]
fn childless_generation_survives_unrelated_pruning_and_generic_completion() {
    let mut engine = engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    let unrelated = cx(&mut engine, 1_000_000., |cx| {
        cx.http_context(Scope::Background, read_policy())
    })
    .unwrap();
    assert!(engine.io.request_contexts.command_live(7, 1_000_000.));
    cx(&mut engine, 1_000_000., |cx| {
        assert!(matches!(
            cx.http_context(Scope::Command(7), read_policy()),
            Err(Error::State)
        ));
        cx.done(7, "numeric bypass".into());
        cx.fail(7, "numeric bypass");
        assert!(matches!(
            cx.issue_preflight(7, policy(PreflightPurpose::ReadOnly)),
            Err(Error::State)
        ));
        assert!(cx.validate_http_context(&unrelated).is_ok());
        cx.done_preflight(&owner, "cached".into()).unwrap();
        assert!(matches!(
            cx.issue_preflight(7, policy(PreflightPurpose::ReadOnly)),
            Err(Error::State)
        ));
    });
    assert!(engine.io.request_contexts.command_live(7, 2_000_000.));
    assert!(engine.venue.retired.is_empty());
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 7, result }) if result == "cached")).count(), 1);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, .. })))
    );
    assert_eq!(engine.venue.retired.len(), 1);
    assert!(engine.venue.retired[0].matches_preflight(&owner));
    assert!(!engine.io.request_contexts.command_live(7, 2_000_000.));
}

#[test]
fn wrong_issuer_old_generation_and_foreign_delivery_cannot_finish_owner() {
    let mut engine = engine();
    let mut other = self::engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    let foreign = begin(&mut other, 7, PreflightPurpose::ReadOnly);
    assert_eq!(owner.owner(), foreign.owner());
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            cx(&mut engine, now, |cx| cx
                .done_preflight(&owner, "invalid clock".into())),
            Err(Error::Invalid)
        );
        assert_eq!(
            cx(&mut engine, now, |cx| cx
                .fail_preflight(&owner, "invalid clock")),
            Err(Error::Invalid)
        );
    }
    assert_eq!(
        cx(&mut engine, 1., |cx| cx
            .done_preflight(&foreign, "wrong".into())),
        Err(Error::Revoked)
    );
    let background = cx(&mut engine, 1., |cx| {
        cx.http_context(Scope::Background, read_policy())
    })
    .unwrap();
    let delivery = Delivery {
        context: background,
        attempt: crate::request_context::Attempt {
            request: RequestId(90),
            generation: 91,
        },
    };
    engine.io.http_delivery = Some(delivery.clone());
    let ids = engine.io.next_id;
    cx(&mut engine, 2., |cx| {
        assert_eq!(
            cx.done_preflight(&owner, "borrowed".into()),
            Err(Error::Revoked)
        );
        assert_eq!(cx.fail_preflight(&owner, "borrowed"), Err(Error::Revoked));
        assert!(matches!(
            cx.issue_preflight(8, policy(PreflightPurpose::ReadOnly)),
            Err(Error::Revoked)
        ));
    });
    assert_eq!(engine.io.next_id, ids);
    assert_eq!(
        engine.io.http_delivery.as_ref().unwrap().context,
        delivery.context
    );
    engine.io.http_delivery = None;
    cx(&mut engine, 3., |cx| cx.done_preflight(&owner, "ok".into())).unwrap();
    engine.prepare_dispatch(3.);
    let next = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    assert_ne!(owner.owner(), next.owner());
    cx(&mut engine, 4., |cx| {
        assert_eq!(
            cx.done_preflight(&owner, "stale".into()),
            Err(Error::Revoked)
        );
        assert_eq!(cx.fail_preflight(&owner, "stale"), Err(Error::Revoked));
        cx.done_preflight(&next, "next".into()).unwrap();
    });
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.venue.retired.len(), 2);
    assert!(!engine.venue.retired[0].matches_preflight(&next));
}

#[test]
fn preflight_policy_clock_and_safe_capacity_refuse_before_identity_or_owner_mutation() {
    let mut engine = engine();
    for purpose in [
        PreflightPurpose::ReadOnly,
        PreflightPurpose::WriteAfterReads,
    ] {
        let mut invalid = policy(purpose);
        invalid.write_credential = match purpose {
            PreflightPurpose::ReadOnly => {
                policy(PreflightPurpose::WriteAfterReads).write_credential
            }
            _ => None,
        };
        let ids = engine.io.next_id;
        assert!(matches!(
            cx(&mut engine, 0., |cx| cx.issue_preflight(7, invalid)),
            Err(Error::Invalid)
        ));
        assert_eq!(engine.io.next_id, ids);
        assert!(engine.io.request_contexts.commands.is_empty());
    }
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let ids = engine.io.next_id;
        assert!(matches!(
            cx(&mut engine, now, |cx| cx
                .issue_preflight(7, policy(PreflightPurpose::ReadOnly))),
            Err(Error::Invalid)
        ));
        assert_eq!(engine.io.next_id, ids);
    }
    let mut invalid = policy(PreflightPurpose::ReadOnly);
    invalid.read_credential = Some(crate::request_context::Credential {
        slot: crate::CredentialSlot::Secondary,
        reference: crate::signing::KeyRef("".into()),
    });
    assert!(matches!(
        cx(&mut engine, 0., |cx| cx.issue_preflight(7, invalid)),
        Err(Error::Invalid)
    ));
    engine.io.next_id =
        crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID + 1);
    let ids = engine.io.next_id;
    assert!(matches!(
        cx(&mut engine, 0., |cx| cx
            .issue_preflight(7, policy(PreflightPurpose::ReadOnly))),
        Err(Error::Capacity)
    ));
    assert_eq!(engine.io.next_id, ids);
    assert!(engine.io.request_contexts.commands.is_empty());
    assert!(engine.io.actions.is_empty());
    assert!(!engine.failed());
}

#[test]
fn reserved_retirement_backing_covers_all_active_owners_without_post_seal_allocation() {
    let mut engine = engine();
    let owners = (1..=CAPACITY as u32)
        .map(|id| begin(&mut engine, id, PreflightPurpose::ReadOnly))
        .collect::<Vec<_>>();
    let capacity = engine.io.request_contexts.retirements.capacity();
    assert!(capacity >= CAPACITY);
    let ids = engine.io.next_id;
    assert!(matches!(
        cx(&mut engine, 0., |cx| cx
            .issue_preflight(999, policy(PreflightPurpose::ReadOnly))),
        Err(Error::Capacity)
    ));
    assert_eq!(engine.io.next_id, ids);
    for owner in &owners {
        cx(&mut engine, 1., |cx| {
            cx.done_preflight(owner, "cached".into())
        })
        .unwrap();
    }
    assert_eq!(engine.io.request_contexts.retirements.len(), CAPACITY);
    assert_eq!(engine.io.request_contexts.retirements.capacity(), capacity);
    engine.drain_actions().for_each(drop);
    assert_eq!(engine.venue.retired.len(), CAPACITY);
    assert!(engine.io.request_contexts.commands.is_empty());
    assert!(!engine.failed());
}

#[test]
fn write_preflight_cannot_claim_cached_success_and_failure_is_not_sent() {
    let mut engine = engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::WriteAfterReads);
    cx(&mut engine, 1., |cx| {
        assert_eq!(
            cx.done_preflight(&owner, "false success".into()),
            Err(Error::State)
        );
        cx.fail_preflight(
            &owner,
            VenueError::new("prerequisite refused").outcome(crate::Outcome::Unknown),
        )
        .unwrap();
    });
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::NotSent))));
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::Notice(Notice::CommandDone { command: 7, .. })
    )));
    assert!(engine.venue.retired[0].matches_preflight(&owner));
}

#[test]
fn hook_panic_fences_queued_actions_claims_callbacks_and_native_queries_forever() {
    let mut engine = engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    let request = cx(&mut engine, 0., |cx| {
        let context = cx.http_context(Scope::Background, read_policy()).unwrap();
        cx.request_managed(
            context,
            Method::Get,
            "https://venue.test/read".into(),
            None,
            (),
        )
        .unwrap()
    });
    let token = engine.io.managed_http[&request].delivery().attempt;
    let preparation = engine.io.managed_preparation(request).unwrap();
    engine.http_prepared(
        1.,
        preparation,
        crate::request_context::Prepared {
            id: 8,
            expires_at: None,
        },
    );
    let permit = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                crate::request_context::HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    cx(&mut engine, 2., |cx| {
        cx.request(
            Method::Get,
            "https://venue.test/plain".into(),
            vec![],
            None,
            (),
        );
        cx.send(SocketId(99), "queued");
        cx.notice(crate::log::Level::Info, "test.queued", "queued");
        cx.done_preflight(&owner, "cached".into()).unwrap();
    });
    engine.venue.panic_hook = true;
    let mut exposed = 0;
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            for _ in engine.drain_actions() {
                exposed += 1;
            }
        }))
        .is_err()
    );
    assert_eq!(exposed, 0);
    assert_eq!(engine.venue.retired.len(), 1);
    assert!(engine.failed());
    assert!(!engine.io.next_id.failed());
    assert!(!engine.io.signing.failed());
    let queries = engine.venue.queries.get();
    assert!(catch_unwind(AssertUnwindSafe(|| engine.venue())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| engine.capabilities())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| engine.book("BTC"))).is_err());
    #[cfg(feature = "book-deltas")]
    assert!(catch_unwind(AssertUnwindSafe(|| engine.set_book_deltas(true))).is_err());
    assert_eq!(engine.venue.queries.get(), queries);
    assert!(
        catch_unwind(AssertUnwindSafe(|| engine.handle(
            3.,
            Event::WsOpened {
                socket: SocketId(99)
            }
        )))
        .is_err()
    );
    assert!(catch_unwind(AssertUnwindSafe(|| engine.claim_http(3., permit))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| engine.prepare_dispatch(3.))).is_err());
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            engine.snapshot_frame(3.);
        }))
        .is_err()
    );
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            engine.drain_actions().for_each(drop);
        }))
        .is_err()
    );
    assert!(!engine.signing_pending(token.request));
    assert!(!engine.http_preparation_pending(preparation));
    assert!(engine.http_release(request).is_none());
    assert_eq!(engine.venue.callbacks, 0);
    assert_eq!(engine.venue.retired.len(), 1);
}

#[test]
fn invalid_retirement_token_poisoning_precedes_native_hook_and_allocator_failure_skips_hook() {
    let mut engine = engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    cx(&mut engine, 1., |cx| {
        cx.done_preflight(&owner, "cached".into())
    })
    .unwrap();
    let mut other = self::engine();
    let foreign = begin(&mut other, 7, PreflightPurpose::ReadOnly);
    engine.io.request_contexts.retirements[0] = HttpOwnerRetirement::new(foreign);
    assert!(catch_unwind(AssertUnwindSafe(|| engine.prepare_dispatch(2.))).is_err());
    assert!(engine.failed());
    assert!(engine.venue.retired.is_empty());
    assert!(!engine.io.next_id.failed());
    let mut engine = self::engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
    cx(&mut engine, 1., |cx| {
        cx.done_preflight(&owner, "cached".into())
    })
    .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| engine.io.next_id.invalidate())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| engine.prepare_dispatch(2.))).is_err());
    assert!(engine.venue.retired.is_empty());
}

#[cfg(feature = "order-locks")]
#[test]
fn lock_admission_never_relabels_prior_send_or_acquires_read_mutation_intent() {
    for purpose in [
        PreflightPurpose::ReadOnly,
        PreflightPurpose::WriteAfterReads,
    ] {
        let mut engine = engine();
        engine
            .io
            .order_locks
            .acquire(
                &Command::PlaceOrders {
                    command: 7,
                    orders: vec![],
                },
                &engine.store,
                true,
            )
            .unwrap();
        engine.io.order_locks.note_write();
        let ids = engine.io.next_id;
        assert!(matches!(
            cx(&mut engine, 0., |cx| cx.issue_preflight(7, policy(purpose))),
            Err(Error::State)
        ));
        assert_eq!(engine.io.next_id, ids);
        assert!(engine.io.request_contexts.commands.is_empty());
        engine.io.order_locks.finish(7, true);
        assert!(engine.io.order_locks.uncertain(""));
    }
    let mut engine = engine();
    engine
        .io
        .order_locks
        .acquire(
            &Command::PlaceOrders {
                command: 7,
                orders: vec![],
            },
            &engine.store,
            true,
        )
        .unwrap();
    assert!(matches!(
        cx(&mut engine, 0., |cx| cx
            .issue_preflight(7, policy(PreflightPurpose::ReadOnly))),
        Err(Error::State)
    ));
    let ids = engine.io.next_id;
    let mut invalid = policy(PreflightPurpose::WriteAfterReads);
    invalid.write_credential = None;
    assert!(matches!(
        cx(&mut engine, 0., |cx| cx.issue_preflight(7, invalid)),
        Err(Error::Invalid)
    ));
    assert_eq!(engine.io.next_id, ids);
    assert!(engine.io.request_contexts.commands.is_empty());
    // Refusal did not mark the lock managed: a subsequent legacy write is
    // still uncertain, rather than being silently relabelled unsent.
    engine.io.order_locks.note_write();
    assert!(!engine.io.order_locks.can_manage_unsent(7));
    engine.io.order_locks.finish(7, true);
    assert!(engine.io.order_locks.uncertain(""));
    let mut engine = self::engine();
    engine
        .io
        .order_locks
        .acquire(
            &Command::PlaceOrders {
                command: 7,
                orders: vec![],
            },
            &engine.store,
            true,
        )
        .unwrap();
    let owner = begin(&mut engine, 7, PreflightPurpose::WriteAfterReads);
    engine.io.order_locks.note_write();
    assert!(engine.io.order_locks.can_manage_unsent(7));
    cx(&mut engine, 1., |cx| {
        cx.fail_preflight(&owner, "prerequisite refused")
    })
    .unwrap();
    engine.drain_actions().for_each(drop);
    assert!(!engine.io.order_locks.uncertain(""));
    assert!(!engine.io.order_locks.contains(7));
}

#[test]
fn write_preflight_fallback_uses_the_shared_bounded_evidence_projection() {
    let mut engine = engine();
    let owner = begin(&mut engine, 7, PreflightPurpose::WriteAfterReads);
    let mut ids = (0..4_100)
        .map(|n| format!("native-{n}"))
        .collect::<Vec<_>>();
    ids.push("x".repeat(1_025));
    let fallback = VenueError::new("é".repeat(10_000))
        .venue_code("c".repeat(10_000))
        .venue_message("v".repeat(10_000))
        .partial_ids(&ids)
        .outcome(crate::Outcome::Unknown);
    cx(&mut engine, 1., |cx| cx.fail_preflight(&owner, fallback)).unwrap();
    assert!(engine.io.request_contexts.commands[&7].writes.is_none());
    assert!(engine.io.managed_http.is_empty());
    let error = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.outcome, Some(crate::Outcome::NotSent));
    // ID overflow uses the shared explicit ID marker in place of the text
    // suffix; it must not erase the native prefix or fabricate shortened IDs.
    assert!(error.message.starts_with('é'));
    assert!(error.message.contains("truncated IDs"));
    assert!(error.message.len() <= 4_096 + 64);
    assert!(error.venue_code.as_ref().unwrap().len() <= 4_096);
    assert!(error.venue_message.as_ref().unwrap().len() <= 4_096);
    let retained = error.partial.expect("bounded exact evidence IDs").order_ids;
    assert!(!retained.is_empty());
    assert!(retained.len() <= 4_096);
    assert!(
        retained
            .iter()
            .all(|id| id.len() <= 1_024 && ids.contains(id))
    );
    assert!(retained.contains(&"native-0".into()));
}

#[test]
fn real_native_callback_boundaries_flush_exact_owner_before_later_reuse() {
    for source in 0..3 {
        let mut engine = engine();
        match source {
            0 => {
                engine.venue.retire_on_start = true;
                engine.start(0.);
            }
            1 => {
                engine.venue.retire_on_command = true;
                engine.handle(
                    0.,
                    Event::Command(Command::FetchLeverage {
                        command: 7,
                        symbol: "BTCUSDT".into(),
                    }),
                );
            }
            _ => {
                engine.venue.retire_on_public = true;
                engine.apply_shared_tickers(0.);
            }
        }
        let owner = engine.venue.callback_owner.clone().unwrap();
        assert_eq!(engine.venue.retired.len(), 1);
        assert!(engine.venue.retired[0].matches_preflight(&owner));
        let next = begin(&mut engine, 7, PreflightPurpose::ReadOnly);
        assert_ne!(next.owner(), owner.owner());
        assert!(!engine.venue.retired[0].matches_preflight(&next));
        assert!(engine.io.request_contexts.command_live(7, 1.));
    }
}

#[test]
fn caught_post_seal_bookkeeping_failure_is_terminal_before_any_hook_or_drain() {
    let mut engine = engine();
    engine.venue.retire_on_command = true;
    engine.venue.corrupt_queue = true;
    assert!(
        catch_unwind(AssertUnwindSafe(|| engine.handle(
            0.,
            Event::Command(Command::FetchLeverage {
                command: 7,
                symbol: "BTCUSDT".into(),
            })
        )))
        .is_err()
    );
    assert!(engine.venue.caught_bookkeeping);
    assert!(engine.failed());
    assert!(engine.venue.retired.is_empty());
    assert!(!engine.io.next_id.failed());
    assert!(!engine.io.signing.failed());
    assert!(catch_unwind(AssertUnwindSafe(|| engine.drain_actions().for_each(drop))).is_err());
}
#[test]
fn delivery_scope_outlives_temporary_marker_removal_owner_retirement_and_reuse() {
    use crate::request_context::plan::{CallTerms, RetryWindow};
    let mut e = engine();
    let owner = begin(&mut e, 7, PreflightPurpose::ReadOnly);
    let context = cx(&mut e, 0., |cx| {
        cx.preflight_read_context(
            &owner,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            CallTerms {
                wait_ms: 0,
                timeout_ms: 20,
                attempts: 1,
                retry_window: RetryWindow::Shared,
            },
        )
    })
    .unwrap();
    let request = cx(&mut e, 0., |cx| {
        cx.request_managed(
            context,
            Method::Get,
            "https://venue.test/read".into(),
            None,
            (),
        )
    })
    .unwrap();
    let delivery = e.io.managed_http[&request].delivery();
    e.io.request_contexts.enter_delivery(&delivery);
    e.io.http_delivery = Some(delivery.clone());
    assert_eq!(e.io.request_contexts.delivery_depth, 1);
    let marker = e.io.http_delivery.take();
    assert_eq!(e.io.request_contexts.delivery_depth, 1);
    e.io.http_delivery = marker;
    cx(&mut e, 1., |cx| {
        cx.fail_preflight(&owner, "preparation refused")
    })
    .unwrap();
    e.flush_http_owner_retirements();
    assert!(!e.io.request_contexts.commands.contains_key(&7));
    assert_eq!(e.io.request_contexts.delivery_depth, 1);
    e.io.http_delivery = None;
    let next = begin(&mut e, 7, PreflightPurpose::ReadOnly);
    assert_ne!(owner.owner(), next.owner());
    e.io.request_contexts.exit_delivery(&delivery);
    assert_eq!(e.io.request_contexts.delivery_depth, 0);
    assert_eq!(e.io.request_contexts.commands[&7].deliveries, 0);

    let background = cx(&mut e, 2., |cx| {
        cx.http_context(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20,
                pacing: None,
                retry: RetryPolicy::Never,
            },
        )
    })
    .unwrap();
    let request = cx(&mut e, 2., |cx| {
        cx.request_managed(
            background,
            Method::Get,
            "https://venue.test/background".into(),
            None,
            (),
        )
    })
    .unwrap();
    let delivery = e.io.managed_http[&request].delivery();
    e.io.request_contexts.enter_delivery(&delivery);
    e.io.request_contexts.prune(30.);
    assert_eq!(e.io.request_contexts.delivery_depth, 1);
    e.io.request_contexts.exit_delivery(&delivery);
    assert_eq!(e.io.request_contexts.delivery_depth, 0);
}
