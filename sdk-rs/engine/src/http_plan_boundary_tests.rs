use super::*;
use crate::request_context::{
    Error, Operation, Plan, PlanPolicy, PreflightPolicy, PreflightPurpose, RetryPolicy,
    plan::{CallBound, RetryWindow, StepBound},
};

#[derive(Default)]
struct Probe {
    order_read: bool,
    reconcile_in_read: bool,
    refuse_ready: bool,
    plan: Option<Plan>,
    request: Option<RequestId>,
    ready: usize,
    responses: usize,
    deferred: usize,
    read_token: Option<u64>,
    ready_token: Option<u64>,
}
impl Venue for Probe {
    type Pending = ();
    fn guard_pending_placements(&self) -> bool {
        true
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        true
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.order_read {
            self.request = Some(cx.request(
                Method::Get,
                "https://venue.test/orders".into(),
                vec![],
                None,
                (),
            ));
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
        if let Command::CancelSymbolOrders { command, .. } = command {
            assert!(cx.io.http_delivery.is_none());
            assert!(cx.io.order_response_token.is_none());
            self.deferred += 1;
            cx.done(command, "deferred cancel".into());
            return;
        }
        let write = matches!(command, Command::PlaceOrders { .. });
        let credential = write.then(|| crate::request_context::Credential {
            slot: crate::CredentialSlot::Primary,
            reference: crate::signing::KeyRef("fixture".into()),
        });
        let owner = cx
            .begin_preflight(PreflightPolicy {
                purpose: if write {
                    PreflightPurpose::WriteAfterReads
                } else {
                    PreflightPurpose::ReadOnly
                },
                read_credential: None,
                write_credential: credential.clone(),
            })
            .unwrap();
        let calls = [CallBound {
            timeout_ms: 100,
            attempts: 1,
            retry_window: RetryWindow::Shared,
            after_refusal: &[],
        }];
        self.plan = Some(
            cx.admit_plan(
                &owner,
                &[StepBound {
                    wait_ms: 100,
                    calls: &calls,
                }],
                PlanPolicy {
                    operation: if write {
                        Operation::Mutation
                    } else {
                        Operation::Read
                    },
                    credential,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
            )
            .unwrap(),
        );
    }
    fn on_http_plan_ready(&mut self, cx: &mut Cx<'_, ()>, plan: &Plan) {
        self.ready += 1;
        assert!(cx.io.command_entry.is_none());
        assert!(cx.io.http_delivery.is_none());
        self.ready_token = cx.io.order_response_token;
        if self.order_read {
            cx.orders_reconciled(Some("BTCUSD"));
        }
        if self.refuse_ready {
            let slot = cx.plan_slots(plan).unwrap().remove(0);
            let context = cx.plan_leg_context(plan, &slot).unwrap();
            // Wrong transport preparation is a real typed pre-send refusal,
            // not a manual terminal: selected write journal enrollment survives.
            assert_eq!(
                cx.request_managed(
                    context,
                    None,
                    Method::Post,
                    "https://venue.test/write".into(),
                    None,
                    ()
                )
                .unwrap_err()
                .1,
                Error::Invalid
            );
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), response: Response<'_>) {
        assert_eq!(response.status, 200);
        assert_eq!(response.body, "[]");
        self.responses += 1;
        self.read_token = cx.io.order_response_token;
        assert!(self.read_token.is_some());
        if self.reconcile_in_read {
            cx.orders_reconciled(Some("BTCUSD"));
        }
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {
        panic!("Plan timer escaped");
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn place(command: CommandId) -> Command {
    Command::PlaceOrders {
        command,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSD","type":"limit","side":"buy","amount":1,"price":1}"#,
            )
            .unwrap(),
        ],
    }
}
fn read() -> Command {
    Command::FetchCandles {
        command: 7,
        symbol: "BTCUSD".into(),
        interval: Interval::M1,
        limit: Some(2),
        from_ms: None,
        to_ms: None,
    }
}
fn read_run(reconcile: bool) -> Engine<Probe> {
    let mut e = crate::test_support::seeded_engine(Probe {
        order_read: true,
        reconcile_in_read: reconcile,
        ..Default::default()
    });
    // Seed an already-existing uncertain placement fence using the existing
    // lock state machine; the tested order read is real Engine request/response.
    e.io.order_locks
        .acquire(&place(101), &e.store, true)
        .unwrap();
    e.io.order_locks.note_write();
    e.io.order_locks.finish(101, true);
    assert!(e.io.order_locks.uncertain("BTCUSD"));
    e.start(0.);
    e.handle(0., Event::Command(read()));
    let request = e.venue.request.unwrap();
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::Http(h) if h.id == request))
    );
    e.handle(
        100.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "[]",
            },
        },
    );
    assert_eq!(e.venue.responses, 1);
    assert_eq!(e.venue.ready, 1);
    assert_eq!(
        e.io.order_response_token, e.venue.read_token,
        "outer token must be restored exactly"
    );
    e
}
#[test]
fn plan_readiness_cannot_borrow_a_real_order_response_token() {
    let e = read_run(false);
    assert!(
        e.io.order_locks.uncertain("BTCUSD"),
        "readiness borrowed unrelated order-read authority"
    );
    assert_eq!(e.venue.ready_token, None);
}
#[test]
fn legitimate_order_read_keeps_its_reconciliation_authority() {
    let e = read_run(true);
    assert!(!e.io.order_locks.uncertain("BTCUSD"));
}
fn deferred(refuse_ready: bool) {
    let mut e = crate::test_support::seeded_engine(Probe {
        refuse_ready,
        ..Default::default()
    });
    e.start(0.);
    e.handle(0., Event::Command(place(7)));
    e.handle(
        1.,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSD".into(),
        }),
    );
    assert_eq!(e.venue.deferred, 0);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms: 100. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        if refuse_ready { 100. } else { 200. },
        Event::TimerFired { timer },
    );
    assert_eq!(
        e.venue.deferred, 1,
        "released Plan lock left deferred cancel parked without another event"
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, result }) if result == "deferred cancel")).count(), 1);
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 7, error }) if error.outcome == Some(crate::Outcome::NotSent))).count(), 1);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::Sign { .. }
    )));
}
#[test]
fn root_expiry_runs_deferred_cancel_without_another_external_event() {
    deferred(false);
}
#[test]
fn readiness_queue_refusal_runs_deferred_cancel_without_another_external_event() {
    deferred(true);
}
