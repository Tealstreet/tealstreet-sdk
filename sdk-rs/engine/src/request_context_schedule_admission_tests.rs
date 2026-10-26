use super::*;
use crate::book::Book;
use crate::request_context::plan::{CallBound, RetryWindow, SerialCall, SerialPolicy};
use crate::request_context::{Credential, PreflightPolicy, PreflightPurpose, RetryPolicy};
use crate::{Command, Response, SocketId, Venue, VenueAction};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Live,
    Retired,
    ReceiptExpired,
    Planned,
}

struct Probe {
    origin: Origin,
    serial: bool,
    timeout_ms: u32,
    result: Option<Result<(), Error>>,
}

fn credential() -> Credential {
    Credential {
        slot: crate::CredentialSlot::Primary,
        reference: crate::signing::KeyRef("schedule-fixture".into()),
    }
}

impl Probe {
    fn run(&mut self, cx: &mut Cx<'_, ()>) {
        let read = !self.serial || self.origin == Origin::ReceiptExpired;
        let policy = PreflightPolicy {
            purpose: if read {
                PreflightPurpose::ReadOnly
            } else {
                PreflightPurpose::WriteAfterReads
            },
            read_credential: None,
            write_credential: (!read).then(credential),
        };
        let preflight = if read {
            cx.begin_preflight_with_command_wait(policy)
        } else {
            cx.begin_preflight(policy)
        }
        .unwrap();
        let serial_policy = || SerialPolicy {
            credential: Some(credential()),
            pacing: None,
            refusals_complete: false,
        };
        let first = SerialCall {
            auth: Default::default(),
            operation: Operation::Mutation,
            timeout_ms: 20_000,
            attempts: 1,
        };
        let admitted = if self.origin == Origin::Planned {
            Some(
                cx.admit_serial_plan(&preflight, first, serial_policy())
                    .unwrap()
                    .0,
            )
        } else {
            None
        };
        if self.origin == Origin::Retired {
            cx.fail_preflight(&preflight, "retired fixture").unwrap();
        }
        let now = cx.now;
        if self.origin == Origin::ReceiptExpired {
            cx.now = now + 5.;
        }
        let before = (
            cx.io.core.next_id,
            cx.io.core.actions.len(),
            cx.io.core.request_contexts.plan_context_storage(),
        );
        let result = if self.serial {
            cx.admit_serial_plan(
                &preflight,
                SerialCall {
                    timeout_ms: self.timeout_ms,
                    ..first
                },
                serial_policy(),
            )
            .map(|(plan, _)| plan)
        } else {
            let calls = [CallBound {
                timeout_ms: self.timeout_ms,
                attempts: 1,
                retry_window: RetryWindow::Shared,
                after_refusal: &[],
            }];
            cx.admit_plan(
                &preflight,
                &[StepBound {
                    wait_ms: 0,
                    calls: &calls,
                }],
                PlanPolicy {
                    independent_calls: false,
                    operation: Operation::Read,
                    credential: None,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
        };
        if result.is_err() {
            assert_eq!(
                (
                    cx.io.core.next_id,
                    cx.io.core.actions.len(),
                    cx.io.core.request_contexts.plan_context_storage()
                ),
                before
            );
        }
        cx.now = now;
        self.result = Some(result.as_ref().map(|_| ()).map_err(|error| *error));
        if let Ok(plan) = result {
            cx.fail_plan(&plan, "fixture cleanup").unwrap();
        } else if let Some(plan) = admitted {
            cx.fail_plan(&plan, "fixture cleanup").unwrap();
        } else if self.origin != Origin::Retired {
            cx.fail_preflight(&preflight, "fixture cleanup").unwrap();
        }
    }
}

impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, _: &mut Cx<'_, ()>) {}
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        self.run(cx);
    }
    fn on_venue_action(&mut self, cx: &mut Cx<'_, ()>, _: CommandId, _: VenueAction) {
        self.run(cx);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        panic!("no HTTP");
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {
        panic!("no HTTP");
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn schedule_admission_keeps_serial_and_declared_error_precedence() {
    for (origin, serial, timeout_ms, expected) in [
        (Origin::Live, false, 1, Ok(())),
        (Origin::Live, true, 1, Ok(())),
        (Origin::Live, false, 0, Err(Error::Invalid)),
        (Origin::Live, true, 0, Err(Error::Invalid)),
        (Origin::Retired, false, 0, Err(Error::Revoked)),
        (Origin::Retired, true, 0, Err(Error::Invalid)),
        (Origin::Retired, true, 1, Err(Error::Revoked)),
        (Origin::ReceiptExpired, false, 0, Err(Error::TimedOut)),
        (Origin::ReceiptExpired, true, 0, Err(Error::Invalid)),
        (Origin::Planned, true, 0, Err(Error::State)),
    ] {
        let mut engine = crate::test_support::seeded_engine(Probe {
            origin,
            serial,
            timeout_ms,
            result: None,
        });
        engine.start(0.);
        engine
            .handle_command(
                0.,
                Command::Venue {
                    command: 7,
                    action: VenueAction::FetchBalanceInfo,
                },
                Some(5),
            )
            .unwrap();
        assert_eq!(engine.venue.result, Some(expected));
        assert!(
            !engine
                .drain_actions()
                .any(|action| matches!(action, Action::ManagedHttp(_) | Action::Http(_)))
        );
    }
}
