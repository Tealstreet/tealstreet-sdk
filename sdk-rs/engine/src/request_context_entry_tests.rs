use super::*;
use crate::book::Book;
use crate::request_context::{Operation, Policy, RetryPolicy, Scope};

#[derive(Default)]
struct Probe {
    mutation: bool,
    refusals: usize,
    fill: Option<usize>,
    callback: Option<fn(&mut Cx<'_, ()>)>,
    existing_read: bool,
    calls: usize,
    timer: Option<TimerId>,
    outside: Option<fn(&mut Cx<'_, ()>)>,
    guard_writes: bool,
    activity: Option<fn(&mut Cx<'_, ()>)>,
}
fn read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if let Some(callback) = self.outside {
            self.timer = Some(cx.set_timer(10.));
            callback(cx);
        }
        if self.existing_read {
            cx.http_context(Scope::Command(7), read_policy()).unwrap();
        }
        for _ in 0..self.fill.unwrap_or(256) {
            cx.http_context(Scope::Background, read_policy()).unwrap();
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        self.calls += 1;
        if let Some(activity) = self.activity {
            activity(cx);
            let ids = cx.io.core.next_id;
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(crate::request_context::CommandEntryError::Revoked)
            );
            assert_eq!(cx.io.core.next_id, ids);
            cx.done(7, "legacy result".into());
            return;
        }
        if let Some(callback) = self.callback {
            callback(cx);
            return;
        }
        let command = command.completion_id().unwrap();
        let mut policy = read_policy();
        if self.mutation {
            policy.operation = Operation::Mutation;
            policy.credential = Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("entry-fixture".into()),
            });
        }
        // The engine owns an issuance refusal without a numeric fallback.
        let _ = command;
        match cx.admit_http_command(policy) {
            Ok(context) => {
                cx.done_context(&context, "unexpected".into()).unwrap();
            }
            Err(_) => {
                self.refusals += 1;
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: TimerId) {
        if let Some(callback) = self.outside {
            callback(cx);
        }
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, ()>) {
        if let Some(callback) = self.outside {
            callback(cx);
        }
    }
    fn guard_order_mutations(&self) -> bool {
        self.guard_writes
    }
    fn on_venue_action(&mut self, cx: &mut Cx<'_, ()>, _: CommandId, _: VenueAction) {
        self.calls += 1;
        if let Some(callback) = self.callback {
            callback(cx);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        if let Some(callback) = self.outside {
            callback(cx);
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn command() -> Command {
    Command::FetchCandles {
        command: 7,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }
}
#[test]
fn unissued_read_command_is_rejected_once_without_a_numeric_fallback() {
    let mut e = crate::test_support::seeded_engine(Probe::default());
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(command()));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue.refusals, 1);
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        errors.len(),
        1,
        "entry refusal must not leave the command hanging"
    );
    assert_eq!(errors[0].kind, Some(ErrorKind::Internal));
    assert_eq!(errors[0].outcome, None);
    assert!(e.io.core.managed_http.is_empty());
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_) | Action::SetTimer { .. } | Action::Sign { .. }
    )));
}
#[test]
fn unissued_write_command_is_not_sent_without_a_context_or_write_journal() {
    let mut e = crate::test_support::seeded_engine(Probe {
        mutation: true,
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(command()));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let errors = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 7, error }) => Some(error),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1, "write entry refusal must be engine-owned");
    assert_eq!(errors[0].outcome, Some(crate::Outcome::NotSent));
    assert!(e.io.core.managed_http.is_empty());
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_) | Action::SetTimer { .. } | Action::Sign { .. }
    )));
}

fn terminal_count(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| {
            matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 7, .. }
                        | Notice::CommandFailed { command: 7, .. }
                )
            )
        })
        .count()
}
fn assert_reissue_refused(cx: &mut Cx<'_, ()>) {
    let ids = cx.io.core.next_id;
    assert_eq!(
        cx.http_context(Scope::Command(7), read_policy()),
        Err(crate::request_context::Error::Revoked)
    );
    assert_eq!(cx.io.core.next_id, ids, "refusal must precede allocation");
}
#[test]
fn automatic_rejection_cannot_reissue_a_generic_context_or_complete_twice() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            let mut invalid = read_policy();
            invalid.timeout_ms = 0;
            assert_eq!(
                cx.admit_http_command(invalid),
                Err(crate::request_context::CommandEntryError::Rejected(
                    crate::request_context::Error::Invalid
                ))
            );
            cx.fail(7, "accidental fallback");
            cx.done(7, "accidental success".into());
            assert_reissue_refused(cx);
        }),
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(1., Event::Command(command()));
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn context_completion_cannot_reissue_in_the_same_callback() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            let root = cx.admit_http_command(read_policy()).unwrap();
            cx.done_context(&root, "[]".into()).unwrap();
            assert_reissue_refused(cx);
        }),
        ..Default::default()
    });
    e.start(0.);
    e.handle(1., Event::Command(command()));
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn cached_legacy_completion_cannot_issue_a_context_in_the_same_callback() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            cx.done(7, "cached".into());
            assert_reissue_refused(cx);
        }),
        ..Default::default()
    });
    e.start(0.);
    e.handle(1., Event::Command(command()));
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn nonfinite_time_cannot_deliver_a_duplicate_command_over_an_existing_read_owner() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = crate::test_support::seeded_engine(Probe {
            fill: Some(0),
            existing_read: true,
            callback: Some(|cx| {
                let _ = cx.admit_http_command(read_policy());
                cx.fail(7, "must not steal old read");
                cx.done(7, "must not steal old read".into());
            }),
            ..Default::default()
        });
        e.start(0.);
        e.drain_actions().for_each(drop);
        e.handle(now, Event::Command(command()));
        assert_eq!(
            e.venue.calls, 0,
            "existing ownership blocks entry even at invalid time"
        );
        assert!(e.io.core.request_contexts.command_live(7, 1.));
        assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 0);
    }
}

#[test]
fn safe_id_refusal_is_typed_nonfatal_and_leaves_no_owner_or_actions() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            use crate::request_context::{CommandEntryError, Error};
            let before = cx.io.core.next_id;
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(CommandEntryError::Rejected(Error::Capacity))
            );
            assert_eq!(cx.io.core.next_id, before);
            assert!(cx.io.core.request_contexts.command_entry_unowned(7));
            assert!(!cx.io.core.next_id.failed());
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(CommandEntryError::Revoked)
            );
        }),
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.io.core.next_id =
        crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID + 1);
    e.handle(1., Event::Command(command()));
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.len(), 1);
    assert_eq!(terminal_count(&actions), 1);
    assert!(!e.failed());
}
#[test]
fn invalid_new_command_clock_is_rejected_once_without_an_owner() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = crate::test_support::seeded_engine(Probe {
            fill: Some(0),
            callback: Some(|cx| {
                use crate::request_context::{CommandEntryError, Error};
                assert_eq!(
                    cx.admit_http_command(read_policy()),
                    Err(CommandEntryError::Rejected(Error::Invalid))
                );
                assert!(cx.io.core.request_contexts.command_entry_unowned(7));
            }),
            ..Default::default()
        });
        e.start(0.);
        e.drain_actions().for_each(drop);
        e.handle(now, Event::Command(command()));
        assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    }
}
#[test]
fn successful_entry_keeps_sibling_contexts_and_refuses_numeric_completion() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            use crate::request_context::CommandEntryError;
            let root = cx.admit_http_command(read_policy()).unwrap();
            let ids = cx.io.core.next_id;
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(CommandEntryError::Revoked)
            );
            assert_eq!(cx.io.core.next_id, ids);
            let child = cx.http_leg_context(&root, read_policy()).unwrap();
            assert_eq!(child.owner(), root.owner());
            cx.fail(7, "must not steal Context");
            cx.done(7, "must not steal Context".into());
            assert_eq!(terminal_count(&cx.io.core.actions), 0);
            cx.done_context(&root, "[]".into()).unwrap();
        }),
        ..Default::default()
    });
    e.start(0.);
    for now in [1., 2.] {
        e.handle(now, Event::Command(command()));
        assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    }
    assert_eq!(
        e.venue.calls, 2,
        "numeric ID reuse gets a new genuine callback"
    );
}
#[test]
fn legacy_http_and_no_id_ws_activity_revoke_only_new_entry_rights() {
    for callback in [
        (|cx: &mut Cx<'_, ()>| {
            cx.request(
                Method::Get,
                "https://venue.test/read".into(),
                vec![],
                None,
                (),
            );
        }) as fn(&mut Cx<'_, ()>),
        |cx| {
            cx.send(SocketId(31), "frame");
        },
        |cx| {
            cx.close(SocketId(31));
        },
        |cx| {
            cx.set_timer(100.);
        },
    ] {
        let mut e = crate::test_support::seeded_engine(Probe {
            fill: Some(0),
            activity: Some(callback),
            ..Default::default()
        });
        e.start(0.);
        e.drain_actions().for_each(drop);
        e.handle(1., Event::Command(command()));
        assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
        assert!(e.io.core.command_entry.is_none());
    }
}
#[test]
fn start_timer_and_public_callbacks_cannot_borrow_command_entry() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        outside: Some(|cx| {
            let ids = cx.io.core.next_id;
            assert_eq!(
                cx.admit_http_command(read_policy()),
                Err(crate::request_context::CommandEntryError::Revoked)
            );
            assert_eq!(cx.io.core.next_id, ids);
        }),
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(
        11.,
        Event::TimerFired {
            timer: e.venue.timer.unwrap(),
        },
    );
    e.apply_shared_tickers(12.);
    let mut cx = Cx {
        now: 12.,
        store: &mut e.store,
        io: &mut e.io,
    };
    let root = cx.http_context(Scope::Background, read_policy()).unwrap();
    let request = cx
        .request_managed(
            root,
            None,
            Method::Get,
            "https://venue.test/background".into(),
            None,
            (),
        )
        .unwrap();
    let physical = crate::test_support::prepared_actions(&mut e, 12.);
    assert!(
        physical
            .iter()
            .any(|a| matches!(a, Action::Http(h) if h.id == request))
    );
    e.handle(
        13.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    assert!(e.io.core.command_entry.is_none());
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 0);
}
#[test]
fn venue_action_gets_entry_but_unsupported_account_command_does_not() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            let root = cx.admit_http_command(read_policy()).unwrap();
            cx.done_context(&root, "[]".into()).unwrap();
        }),
        ..Default::default()
    });
    e.start(0.);
    e.handle(
        1.,
        Event::Command(Command::Venue {
            command: 7,
            action: VenueAction::FetchBalanceInfo,
        }),
    );
    assert_eq!(e.venue.calls, 1);
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    e.handle(2., Event::Command(Command::FetchAccount { command: 7 }));
    assert_eq!(e.venue.calls, 1);
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    assert!(e.io.core.command_entry.is_none());
}
#[cfg(feature = "order-locks")]
#[test]
fn deferred_cancel_gets_its_own_entry_after_the_placement_releases() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        guard_writes: true,
        callback: Some(|cx| {
            let root = cx.admit_http_command(read_policy()).unwrap();
            assert!(matches!(
                root.owner(),
                crate::request_context::Owner::Command { id: 7, .. }
            ));
            cx.done_context(&root, "[]".into()).unwrap();
        }),
        ..Default::default()
    });
    e.start(0.);
    e.io.core
        .order_locks
        .acquire(
            &Command::PlaceOrders {
                command: 9,
                orders: vec![],
            },
            &e.store,
            true,
        )
        .unwrap();
    e.handle(
        1.,
        Event::Command(Command::CancelSymbolOrders {
            command: 7,
            symbol: String::new(),
        }),
    );
    assert_eq!(e.venue.calls, 0);
    assert!(e.io.core.command_entry.is_none());
    e.io.core.order_locks.finish(9, false);
    e.handle(
        2.,
        Event::TimerFired {
            timer: TimerId(999),
        },
    );
    assert_eq!(e.venue.calls, 1);
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}

struct ParkedStep;
impl crate::SignStep for ParkedStep {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &crate::DispatchContext<'_>,
    ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
        Ok(crate::signing::SignRequest {
            receipt: crate::signing::SignReceipt {
                stamp_ms: Some(0.),
                expiry_ms: None,
            },
            operations: vec![crate::signing::Operation::Present {
                key: crate::signing::KeyRef("owned-entry-test".into()),
            }],
            context: crate::signing::Context::default(),
        })
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &crate::signing::SignResult,
    ) -> Result<(), crate::signing::SignError> {
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        20_000.
    }
}
struct ParkedProvider;
impl crate::signing::Signer for ParkedProvider {
    fn sign(&self, _: crate::signing::SignRequest) -> crate::signing::SignFuture {
        Box::pin(std::future::pending())
    }
}
#[test]
fn caught_command_panic_fences_real_held_permit_and_pending_signer() {
    use crate::request_context::{HttpAction, Prepared};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|_| panic!("native command failed")),
        ..Default::default()
    });
    e.set_signer(Some(std::sync::Arc::new(ParkedProvider)))
        .unwrap();
    e.start(0.);
    e.drain_actions().for_each(drop);
    let mut cx = Cx {
        now: 0.,
        store: &mut e.store,
        io: &mut e.io,
    };
    let root = cx.http_context(Scope::Background, read_policy()).unwrap();
    cx.request_managed(
        root,
        None,
        Method::Get,
        "https://venue.test/held".into(),
        None,
        (),
    )
    .unwrap();
    let legacy = cx.request(
        Method::Get,
        "https://venue.test/signed".into(),
        vec![],
        None,
        (),
    );
    cx.sign_http(legacy, Box::new(ParkedStep));
    let mut preparation = None;
    let mut signing = None;
    for a in e.drain_actions() {
        match a {
            Action::ManagedHttp(a) => {
                if let HttpAction::Prepare { preparation: p, .. } = *a {
                    preparation = Some(p);
                }
            }
            Action::Sign { attempt, work, .. } => signing = Some((attempt, work)),
            _ => {}
        }
    }
    let preparation = preparation.expect("real queued managed request");
    let (attempt, work) = signing.expect("real asynchronous provider work emitted");
    assert!(e.signing_pending(attempt));
    e.http_prepared(
        0.,
        preparation,
        Prepared {
            id: 45,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("real prepared resource held before claim");
    assert!(catch_unwind(AssertUnwindSafe(|| e.handle(1., Event::Command(command())))).is_err());
    assert!(e.failed());
    assert!(!e.http_preparation_pending(preparation));
    assert!(!e.signing_pending(attempt));
    assert!(catch_unwind(AssertUnwindSafe(|| e.claim_http(2., permit))).is_err());
    assert!(
        catch_unwind(AssertUnwindSafe(|| e.handle(
            2.,
            Event::Signed {
                attempt,
                result: Ok(crate::signing::SignResult::new(
                    vec![b"presented".to_vec()],
                    work.receipt
                ))
            }
        )))
        .is_err()
    );
    assert!(catch_unwind(AssertUnwindSafe(|| e.drain_actions().for_each(drop))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| e.capabilities())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| e.books().count())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| e.book("BTCUSD").is_some())).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| e.venue())).is_err());
}

#[test]
fn read_root_binding_accepts_original_only_without_granting_io_or_renewal() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            use crate::request_context::Error;
            let root = cx.admit_http_command(read_policy()).unwrap();
            let child = cx.http_leg_context(&root, read_policy()).unwrap();
            assert_eq!(
                cx.validate_read_completion_context(&child),
                Ok(()),
                "generic companion check alone is insufficient for root binding"
            );
            assert_eq!(cx.validate_read_root_context(&child), Err(Error::Revoked));
            assert_eq!(cx.validate_read_root_context(&root), Ok(()));
            let mut foreign = crate::test_support::seeded_engine(Probe {
                fill: Some(0),
                ..Default::default()
            });
            let foreign_cx = Cx {
                now: cx.now,
                store: &mut foreign.store,
                io: &mut foreign.io,
            };
            assert_eq!(
                foreign_cx.validate_read_root_context(&root),
                Err(Error::Revoked)
            );
            // Completion may seal an expired Read; expiry never renews send rights.
            cx.now = root.deadline();
            assert_eq!(cx.validate_read_root_context(&root), Ok(()));
            assert_eq!(cx.validate_http_context(&root), Err(Error::TimedOut));
            cx.done_context(&root, "[]".into()).unwrap();
            assert_eq!(cx.validate_read_root_context(&root), Err(Error::Revoked));
        }),
        ..Default::default()
    });
    e.start(0.);
    e.handle(1., Event::Command(command()));
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn read_root_binding_refuses_a_command_with_a_mutation_sibling() {
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        callback: Some(|cx| {
            let root = cx.admit_http_command(read_policy()).unwrap();
            let mut write = read_policy();
            write.operation = Operation::Mutation;
            write.credential = Some(crate::request_context::Credential {
                slot: CredentialSlot::Primary,
                reference: crate::signing::KeyRef("owned-write-entry-test".into()),
            });
            // Existing same-owner sibling API stays explicit; it does not make
            // the original read a read-only completion owner.
            cx.http_context(Scope::Command(7), write).unwrap();
            assert_eq!(
                cx.validate_read_root_context(&root),
                Err(crate::request_context::Error::Invalid)
            );
        }),
        ..Default::default()
    });
    e.start(0.);
    e.handle(1., Event::Command(command()));
}

#[cfg(feature = "order-locks")]
#[test]
fn unissued_locked_place_releases_only_its_own_scope_without_uncertainty() {
    let mut e = crate::test_support::seeded_engine(Probe {
        mutation: true,
        guard_writes: true,
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.io.core
        .order_locks
        .acquire(
            &Command::AmendOrderWithSymbol {
                command: 42,
                id: "unrelated".into(),
                symbol: "ETHUSD".into(),
                price: Some(1.),
                amount: None,
            },
            &e.store,
            true,
        )
        .unwrap();
    e.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }),
    );
    assert_eq!(e.venue.calls, 1);
    assert!(!e.io.core.order_locks.contains(7));
    assert!(!e.io.core.active_commands.contains(&7));
    assert!(e.io.core.order_locks.contains(42));
    assert!(!e.io.core.order_locks.uncertain(""));
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
    e.handle(
        2.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSD".into(),
        }),
    );
    assert_eq!(
        e.venue.calls, 2,
        "no phantom placement lock defers the following cancel"
    );
    assert!(!e.io.core.order_locks.contains(8));
    assert!(e.io.core.order_locks.contains(42));
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, error }) if error.outcome == Some(crate::Outcome::NotSent))));
}

#[path = "request_context_entry_retention_tests.rs"]
mod retention_tests;

#[cfg(feature = "managed-sockets")]
mod managed_socket_activity {
    use super::*;
    use crate::socket_lifecycle::{
        AuthMode, AuthRecovery, ControlPolicy, FeedId, FeedPolicy, Invalidation, RetireReason,
        TrafficPolicy,
    };
    fn policy() -> FeedPolicy {
        FeedPolicy {
            role: SocketChannel::Public,
            supervision: crate::socket::Config::default(),
            auth: AuthMode::None,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::AnyText,
            controls: ControlPolicy::None,
            invalidation: Invalidation::None,
            subscription_interval_ms: 0.,
        }
    }
    fn feed(cx: &Cx<'_, ()>) -> FeedId {
        cx.io.core.socket_driver.state.feeds()[0]
    }
    fn run(activity: fn(&mut Cx<'_, ()>)) {
        let mut e = crate::test_support::seeded_engine(Probe {
            fill: Some(0),
            outside: Some(|cx| {
                let feed = cx.register_socket_feed(policy()).unwrap();
                cx.connect_socket(feed, "wss://entry.test").unwrap();
            }),
            activity: Some(activity),
            ..Default::default()
        });
        e.start(0.);
        e.drain_actions().for_each(drop);
        e.handle(1., Event::Command(command()));
        assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 1);
        assert!(e.io.core.command_entry.is_none());
    }
    #[test]
    fn managed_close_revokes_fresh_command_entry() {
        run(|cx| cx.close(cx.socket_epoch(feed(cx)).unwrap().socket()));
    }
    #[test]
    fn managed_retire_revokes_fresh_command_entry() {
        run(|cx| cx.retire_socket(feed(cx), RetireReason::Stopped));
    }
    #[test]
    fn managed_stop_revokes_fresh_command_entry() {
        run(|cx| cx.stop_socket_feed(feed(cx)));
    }
    #[test]
    fn managed_one_shot_revokes_fresh_command_entry() {
        run(|cx| {
            cx.send_after_socket_interests(feed(cx), "frame".into())
                .unwrap()
        });
    }
    #[test]
    fn managed_interest_proposal_revokes_fresh_command_entry() {
        run(|cx| {
            cx.set_socket_interests(feed(cx), Vec::new()).unwrap();
        });
    }
    #[test]
    fn managed_repair_proposal_revokes_fresh_command_entry() {
        run(|cx| {
            cx.repair_socket_interests(feed(cx), Vec::new()).unwrap();
        });
    }
    #[test]
    fn managed_feed_registration_revokes_fresh_command_entry() {
        run(|cx| {
            cx.register_socket_feed(policy()).unwrap();
        });
    }
}

#[cfg(feature = "order-locks")]
#[test]
fn duplicate_active_command_id_is_rejected_before_the_venue_sees_it() {
    // The original command stays active: the venue answers nothing yet.
    let mut e = crate::test_support::seeded_engine(Probe {
        fill: Some(0),
        guard_writes: true,
        callback: Some(|_| {}),
        ..Default::default()
    });
    e.start(0.);
    e.drain_actions().for_each(drop);
    e.handle(
        1.,
        Event::Command(Command::PlaceOrders {
            command: 7,
            orders: vec![],
        }),
    );
    assert_eq!(e.venue.calls, 1);
    assert_eq!(terminal_count(&e.drain_actions().collect::<Vec<_>>()), 0);
    assert!(e.io.core.active_commands.contains(&7));
    for (now, duplicate) in [
        (
            2.,
            Command::PlaceOrders {
                command: 7,
                orders: vec![],
            },
        ),
        (
            3.,
            Command::SetMarginType {
                command: 7,
                symbol: "BTCUSD".into(),
                isolated: true,
            },
        ),
        (
            4.,
            Command::FetchLeverage {
                command: 7,
                symbol: "BTCUSD".into(),
            },
        ),
    ] {
        e.handle(now, Event::Command(duplicate));
        assert_eq!(
            e.venue.calls, 1,
            "the venue never sees a duplicate active ID"
        );
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(terminal_count(&actions), 0, "the original stays unanswered");
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::Event(event)) if event.code == log_code::COMMAND_DUPLICATE
        )));
        assert!(e.io.core.active_commands.contains(&7));
    }
}
