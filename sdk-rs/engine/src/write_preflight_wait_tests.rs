//! Caller wait for parked WriteAfterReads Preflights
//! (docs/plans/RUST_WRITE_PREFLIGHT_WAIT.md). Every control enters through a
//! genuine command callback with a stamped caller wait.
use super::*;
use crate::book::Book;
use crate::request_context::{CommandEntryError, HttpOwnerRetirement};
use crate::{Action, Command, Engine, Event, Notice, Response, SocketId, TimerId, Venue};

const WAIT_MS: u32 = 1_000;

fn write_policy() -> PreflightPolicy {
    PreflightPolicy {
        purpose: PreflightPurpose::WriteAfterReads,
        read_credential: None,
        write_credential: Some(crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("wait-primary".into()),
        }),
    }
}

#[derive(Default)]
struct Probe {
    /// The command ahead: an uncapped parked Preflight that holds the queue.
    ahead: Option<Preflight>,
    /// Capped Preflights parked behind it, in arrival order.
    queued: Vec<Preflight>,
    entries: Vec<Result<(), CommandEntryError>>,
    second_begin: Option<Result<(), CommandEntryError>>,
    ids_unchanged_after_second_begin: Option<bool>,
    retired: usize,
    /// P1: hand the expired Preflight itself back as the one to wake.
    wake_expired: bool,
    /// uufub4 P2: the queued command inherits an earlier maybe-sent write.
    inherit: bool,
    /// Exhaust resource IDs just before parking.
    exhaust_before_park: bool,
    /// Placements take an order-lock scope (engine guard_pending_placements).
    guard_placements: bool,
    park_results: Vec<Result<(), crate::request_context::Error>>,
    /// Admit a serial Plan from the ready callback.
    #[cfg(feature = "serial-plans")]
    admit_on_ready: bool,
    #[cfg(feature = "serial-plans")]
    plans: Vec<crate::request_context::Plan>,
    venue_timers: usize,
    ready: Vec<Preflight>,
}

impl Venue for Probe {
    type Pending = ();
    fn guard_pending_placements(&self) -> bool {
        self.guard_placements
    }
    fn guard_order_mutations(&self) -> bool {
        self.guard_placements
    }
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        if self.ahead.is_none() {
            let preflight = cx.begin_preflight(write_policy()).unwrap();
            cx.park_preflight(&preflight).unwrap();
            self.ahead = Some(preflight);
            return;
        }
        match cx.begin_preflight_with_command_wait(write_policy()) {
            Ok(preflight) => {
                self.entries.push(Ok(()));
                let ids = cx.io.core.next_id;
                self.second_begin = Some(
                    cx.begin_preflight_with_command_wait(write_policy())
                        .map(drop),
                );
                self.ids_unchanged_after_second_begin = Some(cx.io.core.next_id == ids);
                if self.inherit {
                    cx.inherit_preflight_uncertainty(&preflight).unwrap();
                }
                if self.exhaust_before_park {
                    cx.io.core.next_id = crate::resource_ids::ResourceIds::starting_at(
                        crate::resource_ids::MAX_RESOURCE_ID + 1,
                    );
                }
                let parked = cx.park_preflight(&preflight);
                self.park_results.push(parked);
                if parked.is_ok() {
                    self.queued.push(preflight);
                } else {
                    let _ = cx.fail_preflight(&preflight, "park refused");
                }
            }
            Err(error) => self.entries.push(Err(error)),
        }
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        self.retired += 1;
        let expired = self
            .queued
            .iter()
            .position(|preflight| owner.matches_preflight(preflight))?;
        let preflight = self.queued.remove(expired);
        self.wake_expired.then_some(preflight)
    }
    fn on_http_preflight_ready(&mut self, _cx: &mut Cx<'_, ()>, preflight: &Preflight) {
        self.ready.push(preflight.clone());
        self.queued
            .retain(|queued| queued.owner() != preflight.owner());
        #[cfg(feature = "serial-plans")]
        if self.admit_on_ready {
            let (plan, _) = _cx
                .admit_serial_plan(
                    preflight,
                    crate::request_context::plan::SerialCall {
                        operation: Operation::Mutation,
                        timeout_ms: 20_000,
                        attempts: 1,
                    },
                    crate::request_context::plan::SerialPolicy {
                        credential: Some(write_policy().write_credential.unwrap()),
                        pacing: None,
                        refusals_complete: false,
                    },
                )
                .expect("admitted before the cap");
            self.plans.push(plan);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {
        self.venue_timers += 1;
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn place(command: u32) -> Command {
    // The job ahead (command 1) trades another symbol, so lock controls on
    // BTCUSD see only the capped command's scope.
    let symbol = if command == 1 { "ETHUSD" } else { "BTCUSD" };
    Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_str(&format!(
                r#"{{"symbol":"{symbol}","type":"limit","side":"buy","amount":1,"price":1}}"#
            ))
            .unwrap(),
        ],
    }
}

/// An uncapped job ahead (command 1), then a capped command 2 parked behind it.
fn queued(probe: Probe) -> (Engine<Probe>, Vec<Action>) {
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle_command(0., place(1), None).unwrap();
    engine.drain_actions().for_each(drop);
    engine.handle_command(10., place(2), Some(WAIT_MS)).unwrap();
    let actions = engine.drain_actions().collect();
    (engine, actions)
}

fn cap_timer(actions: &[Action]) -> TimerId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 10. + f64::from(WAIT_MS) => Some(*timer),
            _ => None,
        })
        .expect("a parked capped Preflight arms one timer at its cap")
}

fn failures(actions: &[Action], command: u32) -> Vec<crate::VenueError> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(error.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_write_preflight_binds_the_caller_cap_and_spends_its_one_entry() {
    let (engine, _) = queued(Probe::default());
    assert_eq!(engine.venue.entries, vec![Ok(())]);
    let cap = engine
        .io
        .core
        .request_contexts
        .preflight_receipt_cap(&engine.venue.queued[0])
        .unwrap()
        .expect("bound receipt cap");
    assert_eq!(cap.1, 10. + f64::from(WAIT_MS));
    // P3: the one entry loan is spent; a second begin allocates nothing.
    assert_eq!(
        engine.venue.second_begin,
        Some(Err(CommandEntryError::Revoked))
    );
    assert_eq!(engine.venue.ids_unchanged_after_second_begin, Some(true));
}

#[test]
fn a_parked_capped_write_preflight_fails_at_its_cap_without_a_wake() {
    let (mut engine, actions) = queued(Probe::default());
    let timer = cap_timer(&actions);
    // Early fire re-arms for the exact cap and settles nothing.
    engine.handle(10. + f64::from(WAIT_MS) - 1., Event::TimerFired { timer });
    let early: Vec<_> = engine.drain_actions().collect();
    assert!(failures(&early, 2).is_empty());
    assert_eq!(engine.venue.retired, 0);
    engine.handle(10. + f64::from(WAIT_MS), Event::TimerFired { timer });
    let settled: Vec<_> = engine.drain_actions().collect();
    let errors = failures(&settled, 2);
    assert_eq!(errors.len(), 1, "one terminal at the cap");
    assert_eq!(errors[0].kind, Some(crate::ErrorKind::Timeout));
    assert_eq!(errors[0].outcome, Some(crate::Outcome::NotSent));
    assert_eq!(engine.venue.retired, 1, "the venue hook fires once");
    assert!(
        engine.venue.queued.is_empty(),
        "the venue dropped it from its FIFO"
    );
    assert!(
        engine.venue.ready.is_empty(),
        "no wake for the expired command"
    );
}

#[test]
fn a_hook_cannot_wake_the_preflight_that_just_expired() {
    let (mut engine, actions) = queued(Probe {
        wake_expired: true,
        ..Probe::default()
    });
    engine.handle(
        10. + f64::from(WAIT_MS),
        Event::TimerFired {
            timer: cap_timer(&actions),
        },
    );
    engine.prepare_dispatch(10. + f64::from(WAIT_MS));
    let settled: Vec<_> = engine.drain_actions().collect();
    assert_eq!(failures(&settled, 2).len(), 1);
    assert!(
        engine.venue.ready.is_empty(),
        "the expired Preflight is Revoked"
    );
}

#[test]
fn a_ready_wake_at_the_cap_loses_to_expiry_in_either_order() {
    for wake_first in [true, false] {
        let (mut engine, actions) = queued(Probe::default());
        let timer = cap_timer(&actions);
        let at = 10. + f64::from(WAIT_MS);
        let wake = |engine: &mut Engine<Probe>| {
            let parked = engine.venue.queued.first().cloned();
            if let Some(parked) = parked {
                let mut cx = Cx {
                    now: at,
                    store: &mut engine.store,
                    io: &mut engine.io,
                };
                let _ = cx.request_preflight_ready(&parked);
            }
            engine.prepare_dispatch(at);
        };
        if wake_first {
            wake(&mut engine);
        }
        engine.handle(at, Event::TimerFired { timer });
        if !wake_first {
            wake(&mut engine);
        }
        let settled: Vec<_> = engine.drain_actions().collect();
        assert_eq!(failures(&settled, 2).len(), 1, "wake_first={wake_first}");
        assert!(
            engine.venue.ready.is_empty(),
            "expiry wins at equality, wake_first={wake_first}"
        );
    }
}

#[test]
fn an_uncapped_parked_write_preflight_stays_timer_free() {
    let mut engine = crate::test_support::seeded_engine(Probe::default());
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    let timers = engine.io.core.timers.len();
    engine.handle_command(0., place(1), None).unwrap();
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(engine.io.core.timers.len(), timers);
    assert!(!actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}

#[test]
fn cap_expiry_keeps_the_owner_aware_mutation_seal() {
    for inherit in [false, true] {
        let (mut engine, actions) = queued(Probe {
            inherit,
            ..Probe::default()
        });
        engine.handle(
            10. + f64::from(WAIT_MS),
            Event::TimerFired {
                timer: cap_timer(&actions),
            },
        );
        let settled: Vec<_> = engine.drain_actions().collect();
        let errors = failures(&settled, 2);
        assert_eq!(errors.len(), 1, "inherit={inherit}");
        let expected = if inherit {
            crate::Outcome::Unknown
        } else {
            crate::Outcome::NotSent
        };
        assert_eq!(errors[0].outcome, Some(expected), "inherit={inherit}");
        assert_eq!(engine.venue.retired, 1, "inherit={inherit}");
    }
}

#[cfg(feature = "order-locks")]
#[test]
fn cap_expiry_releases_the_order_lock_for_the_next_command_on_the_symbol() {
    let (mut engine, actions) = queued(Probe {
        guard_placements: true,
        ..Probe::default()
    });
    assert!(
        engine.io.core.order_locks.contains(2),
        "the parked placement holds its BTCUSD scope"
    );
    let at = 10. + f64::from(WAIT_MS);
    engine.handle(
        at,
        Event::TimerFired {
            timer: cap_timer(&actions),
        },
    );
    engine.drain_actions().for_each(drop);
    assert!(
        !engine.io.core.order_locks.contains(2),
        "cap expiry releases it, with no fence for an unsent command"
    );
    engine
        .handle_command(
            at + 1.,
            Command::CancelSymbolOrders {
                command: 3,
                symbol: "BTCUSD".into(),
            },
            Some(WAIT_MS),
        )
        .unwrap();
    assert_eq!(
        engine.venue.entries,
        vec![Ok(()), Ok(())],
        "a cancel-all on the same symbol is not refused as a conflict"
    );
}

/// Wake the oldest parked capped Preflight at `now` and dispatch it.
fn wake(engine: &mut Engine<Probe>, now: f64) {
    let parked = engine
        .venue
        .queued
        .first()
        .cloned()
        .expect("a parked Preflight");
    let mut cx = Cx {
        now,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.request_preflight_ready(&parked).unwrap();
    engine.prepare_dispatch(now);
}

#[test]
fn a_late_duplicate_cap_fire_never_reaches_the_venue() {
    let (mut engine, actions) = queued(Probe::default());
    let timer = cap_timer(&actions);
    let at = 10. + f64::from(WAIT_MS);
    engine.handle(at, Event::TimerFired { timer });
    engine.handle(at + 5., Event::TimerFired { timer });
    let settled: Vec<_> = engine.drain_actions().collect();
    assert_eq!(failures(&settled, 2).len(), 1);
    assert_eq!(
        engine.venue.venue_timers, 0,
        "the engine owns its cap timer"
    );
}

#[test]
fn a_park_without_a_timer_id_is_refused_and_tracks_nothing() {
    let (engine, actions) = queued(Probe {
        exhaust_before_park: true,
        ..Probe::default()
    });
    assert_eq!(
        engine.venue.park_results,
        vec![Err(crate::request_context::Error::Capacity)]
    );
    assert!(engine.io.core.write_preflight_waits.is_empty());
    assert!(!actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}

#[test]
fn a_woken_sibling_leaves_its_cap_while_the_next_still_expires_on_time() {
    let (mut engine, _) = queued(Probe::default());
    engine.handle_command(20., place(3), Some(WAIT_MS)).unwrap();
    let actions: Vec<_> = engine.drain_actions().collect();
    let third = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 20. + f64::from(WAIT_MS) => Some(*timer),
            _ => None,
        })
        .expect("command 3's own cap timer");
    wake(&mut engine, 500.);
    assert_eq!(engine.venue.ready.len(), 1, "command 2 woke before its cap");
    let cleanup: Vec<_> = engine.drain_actions().collect();
    assert!(
        cleanup
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { .. })),
        "the woken Preflight's cap timer is cleared"
    );
    assert_eq!(engine.io.core.write_preflight_waits.len(), 1);
    engine.handle(20. + f64::from(WAIT_MS), Event::TimerFired { timer: third });
    let settled: Vec<_> = engine.drain_actions().collect();
    assert!(
        failures(&settled, 2).is_empty(),
        "the woken command keeps running"
    );
    assert_eq!(
        failures(&settled, 3).len(),
        1,
        "the sibling expires at its own cap"
    );
}

#[cfg(feature = "serial-plans")]
#[test]
fn a_plan_admitted_before_the_cap_runs_past_it() {
    let (mut engine, actions) = queued(Probe {
        admit_on_ready: true,
        ..Probe::default()
    });
    let timer = cap_timer(&actions);
    wake(&mut engine, 500.);
    assert_eq!(engine.venue.plans.len(), 1);
    engine.drain_actions().for_each(drop);
    engine.handle(10. + f64::from(WAIT_MS), Event::TimerFired { timer });
    engine.prepare_dispatch(60_000.);
    let settled: Vec<_> = engine.drain_actions().collect();
    assert!(
        failures(&settled, 2).is_empty(),
        "a write Plan never inherits the caller cap (RED18)"
    );
}

#[test]
fn a_non_finite_cap_fire_keeps_the_cap_armed_until_it_expires_once() {
    let (mut e, actions) = queued(Probe::default());
    let timer = cap_timer(&actions);
    let cap = 10. + f64::from(WAIT_MS);
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        e.handle(now, Event::TimerFired { timer });
        let rearmed: Vec<_> = e.drain_actions().collect();
        assert!(
            rearmed
                .iter()
                .any(|a| matches!(a, Action::SetTimer { timer: t, at_ms } if *t == timer && *at_ms == cap)),
            "a {now} fire re-arms the exact cap"
        );
        assert!(failures(&rearmed, 2).is_empty(), "no terminal at {now}");
        assert_eq!(e.venue.retired, 0);
    }
    e.handle(cap, Event::TimerFired { timer });
    let settled: Vec<_> = e.drain_actions().collect();
    assert_eq!(failures(&settled, 2).len(), 1, "one terminal at the cap");
    assert_eq!(e.venue.retired, 1, "the hook fires once");
    e.handle(cap + 1., Event::TimerFired { timer });
    assert!(failures(&e.drain_actions().collect::<Vec<_>>(), 2).is_empty());
    assert_eq!(e.venue.retired, 1, "a duplicate fire is inert");
}
