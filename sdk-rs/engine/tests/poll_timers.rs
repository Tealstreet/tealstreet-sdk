mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Method, PollTimerBounds, Response, SocketId, TimerId, Venue,
};

struct Polls {
    specs: Vec<(f64, PollTimerBounds)>,
    logical: Vec<TimerId>,
    fired: Vec<TimerId>,
    updates: Vec<f64>,
    ordinary: Option<TimerId>,
    rearm_on_response: bool,
    register_in_hook: bool,
}

impl Polls {
    fn new(defaults: &[f64]) -> Self {
        Self {
            specs: defaults
                .iter()
                .map(|ms| (*ms, PollTimerBounds::default()))
                .collect(),
            logical: Vec::new(),
            fired: Vec::new(),
            updates: Vec::new(),
            ordinary: None,
            rearm_on_response: false,
            register_in_hook: false,
        }
    }
}

impl Venue for Polls {
    type Pending = usize;
    fn start(&mut self, cx: &mut Cx<'_, usize>) {
        for (ms, bounds) in &self.specs {
            self.logical
                .push(cx.set_poll_timer_with_bounds(*ms, *bounds));
        }
        self.ordinary = Some(cx.set_timer(75.));
    }
    fn on_open(&mut self, _: &mut Cx<'_, usize>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, usize>, _: SocketId, text: &str) {
        let index = text.parse::<usize>().unwrap();
        cx.clear_timer(self.logical[index]);
    }
    fn on_close(&mut self, _: &mut Cx<'_, usize>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, usize>, index: usize, _: Response<'_>) {
        let (ms, bounds) = self.specs[index];
        self.logical[index] = cx.set_poll_timer_with_bounds(ms, bounds);
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, usize>, _: usize, _: &str) {}
    fn on_poll_scale(&mut self, cx: &mut Cx<'_, usize>, scale: f64) {
        self.updates.push(scale);
        if self.register_in_hook {
            self.logical.push(cx.set_poll_timer(8_000.));
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, usize>, timer: TimerId) {
        self.fired.push(timer);
        if self.rearm_on_response
            && let Some(index) = self.logical.iter().position(|id| *id == timer)
        {
            cx.request(
                Method::Get,
                "https://fixture/account".into(),
                vec![],
                None,
                index,
            );
        }
    }
    fn on_command(&mut self, _: &mut Cx<'_, usize>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn timers(actions: &[Action]) -> Vec<(TimerId, f64)> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect()
}

fn take(engine: &mut Engine<Polls>) -> Vec<Action> {
    engine.drain_actions().collect()
}

fn policy(engine: &mut Engine<Polls>, now: f64, scale: f64) -> Vec<Action> {
    engine.handle(now, Event::PollScaleChanged { scale });
    take(engine)
}

fn clear(engine: &mut Engine<Polls>, now: f64, index: &str) -> Vec<Action> {
    engine.handle(
        now,
        Event::WsMessage {
            socket: SocketId(99),
            text: index,
        },
    );
    take(engine)
}

#[test]
fn first_matching_policy_keeps_existing_deadline_and_rearms_only_different_fallbacks() {
    let mut venue = Polls::new(&[2_000., 5_000.]);
    venue.specs[0].1.minimum_interval_ms = 2_000.;
    let mut engine = support::seeded_engine(venue);
    engine.start(5.);
    let original = timers(&take(&mut engine));
    assert!(policy(&mut engine, 50., 1.).is_empty());
    let actions = policy(&mut engine, 105., 0.5);
    let replacement = timers(&actions);
    assert_eq!(replacement.len(), 1);
    assert_eq!(replacement[0].1, 2_605.);
    assert!(!actions.iter().any(|action| matches!(action, Action::ClearTimer { timer } if *timer == original[0].0 || *timer == original[2].0)));
    assert!(policy(&mut engine, 205., 0.5).is_empty());
    engine.handle(
        2_005.,
        Event::TimerFired {
            timer: original[0].0,
        },
    );
    engine.handle(
        5_005.,
        Event::TimerFired {
            timer: original[1].0,
        },
    );
    assert_eq!(engine.venue().fired, [engine.venue().logical[0]]);
}

#[test]
fn an_unchanged_effective_minimum_never_postpones_a_poll() {
    let mut venue = Polls::new(&[100.]);
    venue.specs[0].1.minimum_interval_ms = 250.;
    let mut engine = support::seeded_engine(venue);
    engine.start(10.);
    let original = timers(&take(&mut engine));
    assert_eq!(original[0].1, 260.);
    assert!(policy(&mut engine, 20., 2.).is_empty());
    engine.handle(
        260.,
        Event::TimerFired {
            timer: original[0].0,
        },
    );
    assert_eq!(engine.venue().fired, [engine.venue().logical[0]]);
}

#[test]
fn fallback_cadences_are_independent_and_callbacks_use_logical_ids_once() {
    let mut engine = support::seeded_engine(Polls::new(&[100., 250.]));
    engine.start(10.);
    let physical = timers(&take(&mut engine));
    assert_eq!(
        physical.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [110., 260., 85.]
    );
    for (index, (timer, at)) in physical[..2].iter().enumerate() {
        assert_ne!(*timer, engine.venue().logical[index]);
        engine.handle(*at, Event::TimerFired { timer: *timer });
        engine.handle(*at, Event::TimerFired { timer: *timer });
    }
    assert_eq!(engine.venue().fired, engine.venue().logical);
    assert!(policy(&mut engine, 300., 0.5).is_empty());
}

#[test]
fn change_uses_fresh_physical_ids_and_fences_queued_callbacks_without_changing_ordinary_timers() {
    let mut engine = support::seeded_engine(Polls::new(&[4_000., 10_000.]));
    engine.start(0.);
    let old = timers(&take(&mut engine));
    let changes = policy(&mut engine, 20., 0.5);
    let fresh = timers(&changes);
    assert_eq!(
        fresh.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [2_020., 5_020.]
    );
    assert_eq!(
        changes
            .iter()
            .filter(|a| matches!(a, Action::ClearTimer { .. }))
            .count(),
        2
    );
    for (timer, _) in &old[..2] {
        assert!(!fresh.iter().any(|(new, _)| new == timer));
        engine.handle(10_000., Event::TimerFired { timer: *timer });
    }
    assert!(engine.venue().fired.is_empty());
    engine.handle(75., Event::TimerFired { timer: old[2].0 });
    assert_eq!(engine.venue().fired, [engine.venue().ordinary.unwrap()]);
    for (timer, at) in fresh {
        engine.handle(at, Event::TimerFired { timer });
    }
    assert_eq!(&engine.venue().fired[1..], engine.venue().logical);
}

#[test]
fn pause_retains_registrations_clear_removes_paused_demand_and_resume_is_fresh() {
    let mut engine = support::seeded_engine(Polls::new(&[4_000., 8_000.]));
    engine.start(0.);
    let old = timers(&take(&mut engine));
    let paused = policy(&mut engine, 10., f64::INFINITY);
    assert!(timers(&paused).is_empty());
    assert_eq!(paused.len(), 2);
    assert!(clear(&mut engine, 15., "0").is_empty());
    let resumed = timers(&policy(&mut engine, 20., 0.25));
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, 2_020.);
    for (timer, _) in old.into_iter().take(2) {
        engine.handle(2_020., Event::TimerFired { timer });
    }
    engine.handle(
        2_020.,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    assert_eq!(engine.venue().fired, [engine.venue().logical[1]]);
}

#[test]
fn initially_paused_polls_register_without_host_timers_and_clear_while_active_is_final() {
    let mut engine = support::seeded_engine(Polls::new(&[4_000., 8_000.]));
    assert!(policy(&mut engine, 0., f64::INFINITY).is_empty());
    engine.start(1.);
    assert_eq!(
        timers(&take(&mut engine)),
        [(engine.venue().ordinary.unwrap(), 76.)]
    );
    clear(&mut engine, 2., "1");
    let resumed = timers(&policy(&mut engine, 3., 0.5));
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, 2_003.);
    assert!(
        matches!(clear(&mut engine, 4., "0").as_slice(), [Action::ClearTimer { timer }] if *timer == resumed[0].0)
    );
    engine.handle(
        2_003.,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    assert!(engine.venue().fired.is_empty());
    assert!(policy(&mut engine, 60., 0.25).is_empty());
}

#[test]
fn recurring_minimum_and_absolute_cooldown_survive_policy_changes() {
    let mut venue = Polls::new(&[64_000.]);
    venue.specs[0].1 = PollTimerBounds {
        minimum_interval_ms: 5_000.,
        not_before_ms: 37_500.,
    };
    let mut engine = support::seeded_engine(venue);
    engine.start(0.);
    let first = timers(&take(&mut engine))[0];
    assert_eq!(first.1, 64_000.);
    // 64_000/128 = 500ms and 64_000/64 = 1_000ms both sit under the 5_000ms minimum.
    assert_eq!(timers(&policy(&mut engine, 10., 1. / 128.))[0].1, 37_500.);
    policy(&mut engine, 20., f64::INFINITY);
    let resumed = timers(&policy(&mut engine, 100., 1. / 128.))[0];
    assert_eq!(resumed.1, 37_500.);
    assert!(policy(&mut engine, 36_250., 1. / 64.).is_empty());
    assert_eq!(timers(&policy(&mut engine, 36_250., 0.125))[0].1, 44_250.);
    assert_eq!(timers(&policy(&mut engine, 37_500., 0.25))[0].1, 53_500.);
}

#[test]
fn read_completion_rearms_against_latest_policy_even_when_paused_during_io() {
    let mut venue = Polls::new(&[4_000.]);
    venue.rearm_on_response = true;
    let mut engine = support::seeded_engine(venue);
    engine.start(0.);
    let timer = timers(&take(&mut engine))[0].0;
    engine.handle(4_000., Event::TimerFired { timer });
    let request = take(&mut engine)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .unwrap();
    assert!(policy(&mut engine, 4_010., 0.5).is_empty());
    assert!(policy(&mut engine, 4_020., f64::INFINITY).is_empty());
    engine.handle(
        4_030.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    assert!(take(&mut engine).is_empty());
    assert_eq!(engine.venue().fired.len(), 1);
    let resumed = timers(&policy(&mut engine, 4_050., 0.25));
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, 5_050.);
    assert_ne!(timer, resumed[0].0);
}

#[test]
fn latest_policy_is_applied_before_legacy_hook_and_invalid_updates_do_nothing() {
    let mut venue = Polls::new(&[4_000.]);
    venue.register_in_hook = true;
    let mut engine = support::seeded_engine(venue);
    engine.start(0.);
    take(&mut engine);
    let changed = policy(&mut engine, 10., 0.25);
    assert_eq!(
        timers(&changed)
            .iter()
            .map(|(_, at)| *at)
            .collect::<Vec<_>>(),
        [1_010., 2_010.]
    );
    let ids = timers(&changed);
    for invalid in [f64::NAN, f64::NEG_INFINITY, -1., -0., 0.] {
        assert!(policy(&mut engine, 15., invalid).is_empty());
    }
    assert_eq!(engine.venue().updates, [0.25]);
    for (timer, at) in ids {
        engine.handle(at, Event::TimerFired { timer });
    }
    assert_eq!(engine.venue().fired.len(), 2);
}

#[test]
fn repeated_policy_leaves_existing_due_time_and_physical_id_unchanged() {
    let mut engine = support::seeded_engine(Polls::new(&[4_000.]));
    engine.start(0.);
    take(&mut engine);
    let fresh = timers(&policy(&mut engine, 10., 0.5))[0];
    assert_eq!(fresh.1, 2_010.);
    assert!(policy(&mut engine, 40., 0.5).is_empty());
    engine.handle(2_010., Event::TimerFired { timer: fresh.0 });
    assert_eq!(engine.venue().fired, engine.venue().logical);
}

#[test]
fn scale_multiplies_each_native_cadence_instead_of_replacing_it() {
    let mut engine = support::seeded_engine(Polls::new(&[500., 5_000.]));
    engine.start(0.);
    let native = timers(&take(&mut engine));
    assert_eq!(
        native[..2].iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [500., 5_000.]
    );
    assert!(policy(&mut engine, 10., 1.).is_empty());
    let slowed = timers(&policy(&mut engine, 20., 3.));
    assert_eq!(
        slowed.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [1_520., 15_020.]
    );
    let restored = timers(&policy(&mut engine, 30., 1.));
    assert_eq!(
        restored.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [530., 5_030.]
    );
}

#[test]
fn host_ceiling_bounds_slowed_polls_without_slowing_native_cadences() {
    let mut engine = support::seeded_engine(Polls::new(&[2_000., 5_000., 30_000.]));
    engine.start(0.);
    take(&mut engine);
    engine.handle(
        10.,
        Event::PollCeilingChanged {
            ceiling_ms: Some(8_000.),
        },
    );
    assert!(
        take(&mut engine).is_empty(),
        "ceiling alone keeps native cadences"
    );
    // A 4x server slowdown would turn the 5s poll into 20s; the host's
    // resolved 8s tick bounds it, and the slower native 30s poll stays native.
    let slowed = timers(&policy(&mut engine, 20., 4.));
    assert_eq!(
        slowed.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [8_020., 8_020.]
    );
    engine.handle(30., Event::PollCeilingChanged { ceiling_ms: None });
    // The 2s poll's 8s cadence is unchanged, so only the others re-arm.
    let unbounded = timers(&take(&mut engine));
    assert_eq!(
        unbounded.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [20_030., 120_030.]
    );
}

#[test]
fn host_policy_is_bounded_without_moving_native_cadences_past_the_bounds() {
    let mut engine = support::seeded_engine(Polls::new(&[10_000.]));
    engine.start(0.);
    take(&mut engine);
    assert_eq!(timers(&policy(&mut engine, 10., 0.01))[0].1, 1_010.);
    assert_eq!(timers(&policy(&mut engine, 20., 1e300))[0].1, 120_020.);
    let paused = policy(&mut engine, 30., f64::INFINITY);
    assert!(!paused.is_empty());
    assert!(timers(&paused).is_empty());

    let mut engine = support::seeded_engine(Polls::new(&[500., 200_000.]));
    engine.start(0.);
    let native = timers(&take(&mut engine));
    assert_eq!(
        native[..2].iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [500., 200_000.]
    );
    assert!(policy(&mut engine, 10., 1.).is_empty());
    // 2x doubles the 500ms poll, but the 200s native cadence is never lowered to the cap.
    let doubled = timers(&policy(&mut engine, 20., 2.));
    assert_eq!(
        doubled.iter().map(|(_, at)| *at).collect::<Vec<_>>(),
        [1_020.]
    );
    engine.handle(200_000., Event::TimerFired { timer: native[1].0 });
    assert_eq!(engine.venue().fired, [engine.venue().logical[1]]);
}
