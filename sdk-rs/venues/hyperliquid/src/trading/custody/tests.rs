//! Real engine retirement tokens and timers, without HTTP or signing work.
use super::*;
use safe_cex_engine::book::Book;
use safe_cex_engine::{Action, Command, Event, Response, SocketId, Venue};

struct RetirementProbe(Hyperliquid, [Option<TimerId>; 2], bool);
impl Venue for RetirementProbe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.1 = [Some(cx.set_timer(5.)), Some(cx.set_timer(10.))];
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let command = command.completion_id().unwrap();
        let preflight = cx
            .begin_preflight(PreflightPolicy {
                purpose: PreflightPurpose::ReadOnly,
                read_credential: None,
                write_credential: None,
            })
            .unwrap();
        cx.park_preflight_for(&preflight, None).unwrap();
        let job = Job {
            custody: Some(Custody {
                preflight,
                intent_deadline: cx.now + 1000.,
                intent_timer: Some(cx.set_timer(1000.)),
                intent_expired: false,
                plan: None,
                slot: None,
                context: None,
                ready: false,
                wait: None,
            }),
            command,
            steps: VecDeque::new(),
            ids: Vec::new(),
            journal: Default::default(),
            replacement: None,
            amend_source: None,
            amend_filled: false,
            amend_total: None,
            amend_read: false,
            amend_deadline: None,
            builder_checked: false,
            amend_builder: None,
            effective: false,
            replacing_stop: None,
            replacing_order: None,
            stop_note: None,
            protection: None,
        };
        if self.0.trading.active.is_none() {
            self.0.trading.active = Some(job);
        } else {
            self.0.trading.queue.push_back(job);
        }
    }
    fn on_http_owner_retired_preflight(
        &mut self,
        owner: &HttpOwnerRetirement,
    ) -> Option<Preflight> {
        // Keep the surviving owner parked so both retirement paths can be observed.
        let _ = self.0.trade_owner_retired(owner);
        None
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if Some(timer) == self.1[0] {
            let job = if self.2 {
                self.0.trading.queue.front()
            } else {
                self.0.trading.active.as_ref()
            }
            .unwrap();
            cx.request_preflight_ready(&job.custody.as_ref().unwrap().preflight)
                .unwrap();
        } else {
            self.0.trade_timer(cx, timer);
        }
    }
    fn on_http_preflight_ready(&mut self, cx: &mut Cx<'_, Pending>, owner: &Preflight) {
        cx.fail_preflight(owner, "test retirement").unwrap();
    }
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

#[test]
fn retired_active_and_queued_intent_timers_clear_without_clearing_the_peer() {
    for retire_queued in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(RetirementProbe(
            Hyperliquid::new(crate::Options::default()),
            [None, None],
            retire_queued,
        ));
        e.start(0.);
        e.drain_actions().for_each(drop);
        let mut timers = Vec::new();
        for command in [1, 2] {
            e.handle_command(
                0.,
                Command::CancelOrders {
                    command,
                    ids: vec!["1".into()],
                },
                Some(2000),
            )
            .unwrap();
            timers.push(
                e.drain_actions()
                    .filter_map(|action| match action {
                        Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let index = usize::from(retire_queued);
        let intent = timers[index].iter().find(|(_, at)| *at == 1000.).unwrap().0;
        let peer = timers[1 - index]
            .iter()
            .find(|(_, at)| *at == 1000.)
            .unwrap()
            .0;
        e.handle(
            5.,
            Event::TimerFired {
                timer: e.venue().1[0].unwrap(),
            },
        );
        e.drain_actions().for_each(drop);
        let remaining = e.venue().0.trading.active.as_ref();
        assert_eq!(
            remaining.unwrap().command,
            if retire_queued { 1 } else { 2 }
        );
        // A genuine venue timer callback supplies cleanup authority after retirement.
        e.handle(
            10.,
            Event::TimerFired {
                timer: e.venue().1[1].unwrap(),
            },
        );
        let cleared: Vec<_> = e
            .drain_actions()
            .filter_map(|action| match action {
                Action::ClearTimer { timer } => Some(timer),
                _ => None,
            })
            .collect();
        assert!(cleared.contains(&intent), "retired timer must be cancelled");
        assert!(!cleared.contains(&peer), "peer timer must not be cancelled");
    }
}
