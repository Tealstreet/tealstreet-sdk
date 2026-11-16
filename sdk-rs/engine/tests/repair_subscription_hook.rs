//! `Venue::repair_subscription`: a venue that owns its sockets answers
//! `Command::RepairSubscription` itself; only refusals are visible.
use safe_cex_engine::ControlFrame;
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, Level, LogEvent, Notice, Response, SocketId,
    SubscriptionTopic, TimerId, Value, Venue,
};

/// Answers each repair with `answer`, sending one frame when it succeeds.
struct Owner {
    answer: Option<Result<(), String>>,
    repairs: Vec<SubscriptionTopic>,
}

impl Venue for Owner {
    type Pending = ();
    fn repair_subscription(
        &mut self,
        cx: &mut Cx<'_, ()>,
        topic: &SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        self.repairs.push(topic.clone());
        if let Some(Ok(())) = self.answer {
            cx.send(
                SocketId(1),
                ControlFrame::subscription(format!("repaired {topic}")),
            );
        }
        self.answer.clone()
    }
    fn start(&mut self, _cx: &mut Cx<'_, ()>) {}
    fn on_open(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId) {}
    fn on_message(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _text: &str) {}
    fn on_close(&mut self, _cx: &mut Cx<'_, ()>, _socket: SocketId, _code: u16) {}
    fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
    fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
    fn on_timer(&mut self, _cx: &mut Cx<'_, ()>, _timer: TimerId) {}
    fn on_command(&mut self, _cx: &mut Cx<'_, ()>, command: Command) {
        panic!("repair must not reach on_command: {command:?}");
    }
    fn book(&self, _symbol: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn topic() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M1,
    }
}

fn repair(answer: Option<Result<(), String>>) -> (Engine<Owner>, Vec<Action>) {
    let mut engine = support::seeded_engine(Owner {
        answer,
        repairs: vec![],
    });
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    engine.handle(1., Event::Command(Command::RepairSubscription(topic())));
    let actions = engine.drain_actions().collect();
    (engine, actions)
}

fn warnings(actions: &[Action]) -> Vec<&LogEvent> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

fn frames(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn assert_one_warning(actions: &[Action], reason: &str) {
    let warnings = warnings(actions);
    assert_eq!(warnings.len(), 1, "{actions:?}");
    assert_eq!(
        warnings[0].message,
        format!("Owner cannot repair the candles BTCUSDT 1m subscription: {reason}")
    );
    assert!(
        warnings[0]
            .fields
            .contains(&("venue", Value::from("Owner")))
    );
    assert!(
        warnings[0]
            .fields
            .contains(&("topic", Value::from("candles BTCUSDT 1m")))
    );
}

#[test]
fn an_unsupported_hook_keeps_the_no_managed_sockets_warning() {
    let (engine, actions) = repair(None);
    assert_eq!(engine.venue().repairs, [topic()]);
    assert!(frames(&actions).is_empty());
    assert_one_warning(&actions, "the venue has no managed sockets");
}

#[test]
fn a_refused_repair_warns_with_the_venue_reason() {
    let (engine, actions) = repair(Some(Err("candle1m BTCUSDT is not subscribed".into())));
    assert_eq!(engine.venue().repairs, [topic()]);
    assert!(frames(&actions).is_empty());
    assert_one_warning(&actions, "candle1m BTCUSDT is not subscribed");
}

#[test]
fn a_successful_repair_is_silent() {
    let (engine, actions) = repair(Some(Ok(())));
    assert_eq!(engine.venue().repairs, [topic()]);
    assert_eq!(frames(&actions), ["repaired candles BTCUSDT 1m"]);
    assert!(warnings(&actions).is_empty(), "{actions:?}");
}
