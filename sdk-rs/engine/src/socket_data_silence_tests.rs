//! A socket that answers pings but carries no data on a feed that streams
//! continuously is reconnected; quiet topics alone never count.
use super::*;
use crate::ControlFrame;
use crate::socket::{Config, Timeout};
use crate::socket_lifecycle::{
    AuthMode, AuthRecovery, DataSilence, Invalidation, RetireReason, TrafficPolicy,
};

const SILENCE: DataSilence = DataSilence {
    after_ms: 60_000.,
    streams: &["tick."],
};

#[derive(Default)]
struct Feed {
    feed: Option<FeedId>,
    retired: Vec<RetireReason>,
    sent: Vec<String>,
    delivered: usize,
}

fn policy() -> FeedPolicy {
    FeedPolicy {
        role: SocketChannel::Public,
        // Pings every 10 s keep the idle timeout from ever firing while pongs come back.
        supervision: Config {
            idle_timeout_ms: 20_000.,
            ping_interval_ms: Some(10_000.),
            ..Config::default()
        },
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::Native,
        invalidation: Invalidation::Required,
        subscription_interval_ms: 0.,
        close_floors: &[],
        data_silence: Some(SILENCE),
    }
}

impl Venue for Feed {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let feed = cx.register_socket_feed(policy()).unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://silence.test").unwrap();
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, text: &str) -> Control {
        if text == "pong" {
            Control::Pong
        } else if text.starts_with("account") {
            Control::Unwatched
        } else {
            Control::Other
        }
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Reconnect(feed) => {
                cx.connect_socket(feed, "wss://silence.test")?;
            }
            LifecycleEvent::Retired { reason, .. } => self.retired.push(reason),
            LifecycleEvent::Ping(epoch) => cx.send(
                epoch.socket(),
                ControlFrame::heartbeat(String::from("ping")),
            ),
            _ => {}
        }
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &Store,
        _: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for item in items {
            emit(EncodedBatch {
                text: format!("{subscribe}:{}", item.key),
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
    fn subscription_interest(
        &self,
        _: &Store,
        _: &crate::SubscriptionTopic,
    ) -> Option<(FeedId, String)> {
        Some((self.feed?, "tick.BTC".into()))
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {
        self.delivered += 1;
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Harness {
    engine: Engine<Feed>,
    now: f64,
    socket: Option<SocketId>,
    connects: usize,
    timers: Vec<(TimerId, f64)>,
}

impl Harness {
    /// An open feed subscribed to `keys`.
    fn subscribed(keys: &[&str]) -> Self {
        let mut h = Self {
            engine: crate::test_support::seeded_engine(Feed::default()),
            now: 0.,
            socket: None,
            connects: 0,
            timers: Vec::new(),
        };
        h.engine.start(0.);
        h.collect();
        h.event(Event::WsOpened {
            socket: h.socket.unwrap(),
        });
        let feed = h.engine.venue.feed.unwrap();
        let interests = keys
            .iter()
            .map(|key| Interest::from_wire_args(*key))
            .collect();
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut h.engine.store,
                io: &mut h.engine.io,
            };
            cx.set_socket_interests(feed, interests).unwrap();
        }
        h.engine.process_socket_work(0.);
        h.collect();
        h
    }
    fn collect(&mut self) {
        let actions: Vec<_> = self.engine.drain_actions().collect();
        for action in actions {
            match action {
                Action::WsConnect { socket, .. } => {
                    self.socket = Some(socket);
                    self.connects += 1;
                }
                Action::WsSend { text, .. } if text != "ping" => self.engine.venue.sent.push(text),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn message(&mut self, text: &str) {
        let socket = self.socket.unwrap();
        self.event(Event::WsMessage { socket, text });
    }
    /// Fire due timers through `until`, in order, answering every ping with a
    /// pong and opening any reconnect at once.
    fn advance(&mut self, until: f64) {
        while let Some(&(timer, at)) = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            let connects = self.connects;
            self.event(Event::TimerFired { timer });
            if self.connects > connects {
                self.event(Event::WsOpened {
                    socket: self.socket.unwrap(),
                });
            }
            self.message("pong");
        }
        self.now = until;
    }
    fn silent_retirements(&self) -> usize {
        self.engine
            .venue
            .retired
            .iter()
            .filter(|reason| **reason == RetireReason::Timeout(Timeout::Silent))
            .count()
    }
}

#[test]
fn a_socket_answering_pings_but_carrying_no_data_is_reconnected() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.message("tick.BTC 100");
    h.advance(59_000.);
    assert_eq!(h.silent_retirements(), 0, "within the window");
    assert_eq!(h.connects, 1);
    h.advance(62_000.);
    assert_eq!(h.silent_retirements(), 1, "{:?}", h.engine.venue.retired);
    h.advance(70_000.);
    assert_eq!(h.connects, 2, "the silent socket was redialled");
}

#[test]
fn unwatched_traffic_is_delivered_but_never_proves_the_streams_alive() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.message("tick.BTC 100");
    for second in (10..=50).step_by(10) {
        h.advance(f64::from(second) * 1_000.);
        h.message("account BTC position");
    }
    h.advance(62_000.);
    assert_eq!(
        h.engine.venue.delivered, 6,
        "account frames still reach the venue"
    );
    assert_eq!(h.silent_retirements(), 1, "{:?}", h.engine.venue.retired);
}

#[test]
fn a_feed_with_only_quiet_topics_is_left_alone() {
    let mut h = Harness::subscribed(&["candle.1h.BTC"]);
    h.advance(15. * 60_000.);
    assert_eq!(h.silent_retirements(), 0, "{:?}", h.engine.venue.retired);
    assert_eq!(h.connects, 1);
}

#[test]
fn data_inside_the_window_keeps_the_socket() {
    let mut h = Harness::subscribed(&["tick.BTC", "candle.1h.BTC"]);
    for second in (30..600).step_by(30) {
        h.advance(f64::from(second) * 1_000.);
        h.message("tick.BTC 100");
    }
    assert_eq!(h.silent_retirements(), 0);
    assert_eq!(h.connects, 1);
}

#[test]
fn a_feed_that_stays_silent_waits_twice_as_long_each_time() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.advance(61_000.);
    assert_eq!(h.silent_retirements(), 1);
    // The reconnect replays the subscription; still nothing comes. The second window is 120 s.
    h.advance(61_000. + 100_000.);
    assert_eq!(h.silent_retirements(), 1, "the window doubled");
    h.advance(61_000. + 130_000.);
    assert_eq!(h.silent_retirements(), 2);
}

#[test]
fn a_repair_on_a_silent_feed_reconnects_instead_of_resubscribing() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.engine.venue.sent.clear();
    h.advance(31_000.);
    h.event(Event::Command(Command::RepairSubscription(
        crate::SubscriptionTopic::OrderBook {
            symbol: "BTC".into(),
        },
    )));
    assert_eq!(h.silent_retirements(), 1, "{:?}", h.engine.venue.retired);
    assert!(
        !h.engine
            .venue
            .sent
            .iter()
            .any(|text| text == "false:tick.BTC"),
        "no in-place resubscribe: {:?}",
        h.engine.venue.sent
    );
}

#[test]
fn a_repair_on_a_live_feed_resubscribes_in_place() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.advance(25_000.);
    h.message("tick.BTC 100");
    h.engine.venue.sent.clear();
    h.advance(31_000.);
    h.event(Event::Command(Command::RepairSubscription(
        crate::SubscriptionTopic::OrderBook {
            symbol: "BTC".into(),
        },
    )));
    assert_eq!(h.silent_retirements(), 0);
    assert_eq!(h.engine.venue.sent, ["false:tick.BTC", "true:tick.BTC"]);
}

fn repair(h: &mut Harness) {
    h.event(Event::Command(Command::RepairSubscription(
        crate::SubscriptionTopic::OrderBook {
            symbol: "BTC".into(),
        },
    )));
}

// IRYSUSDT on Hosted: the venue stopped one stream while the socket's others flowed, so the
// first in-place resubscribe changed nothing. Repeating the repair reconnects the socket.
#[test]
fn a_topic_repaired_again_within_ten_minutes_reconnects_its_socket() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    for second in (10..=60).step_by(10) {
        h.advance(f64::from(second) * 1_000.);
        h.message("tick.BTC 100");
    }
    h.engine.venue.sent.clear();
    repair(&mut h);
    assert_eq!(h.engine.venue.sent, ["false:tick.BTC", "true:tick.BTC"]);
    assert_eq!(h.silent_retirements(), 0);
    h.advance(90_000.);
    h.message("tick.BTC 100");
    repair(&mut h);
    assert_eq!(h.silent_retirements(), 1, "the repeat repair reconnects");
}

#[test]
fn a_topic_repaired_again_after_ten_minutes_resubscribes_in_place() {
    let mut h = Harness::subscribed(&["tick.BTC"]);
    h.message("tick.BTC 100");
    repair(&mut h);
    for half_minute in 1..=22 {
        h.advance(f64::from(half_minute) * 30_000.);
        h.message("tick.BTC 100");
    }
    h.engine.venue.sent.clear();
    repair(&mut h);
    assert_eq!(h.silent_retirements(), 0);
    assert_eq!(h.engine.venue.sent, ["false:tick.BTC", "true:tick.BTC"]);
}

#[test]
fn an_unnamed_managed_topic_keeps_the_default_refusal_without_a_hook() {
    let mut engine = crate::test_support::seeded_engine(Feed::default());
    engine.handle(
        0.,
        Event::Command(Command::RepairSubscription(
            crate::SubscriptionTopic::OrderBook {
                symbol: "BTC".into(),
            },
        )),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert_eq!(actions.len(), 1, "{actions:?}");
    let Action::Notice(Notice::Event(event)) = &actions[0] else {
        panic!("only the original refusal is emitted: {actions:?}");
    };
    assert_eq!(event.level, Level::Warn);
    assert_eq!(event.code, log_code::SUBSCRIPTION_REJECTED);
    assert!(
        event
            .message
            .ends_with("the venue cannot name its stream for this topic")
    );
}
