//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Engine, Event, Level, Notice, SubscriptionTopic};
use safe_cex_venue_txflow::TxFlow;
use std::collections::HashSet;

const SYMBOL: &str = "BTC-USDC";
const BOOK: &str =
    r#"{"method":"subscribe","type":"l2Book","subscription":{"type":"l2Book","coin":"1"}}"#;
const CANDLE: &str =
    r#"{"method":"subscribe","subscription":{"type":"candle","coin":"1","interval":"5m"}}"#;

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: SYMBOL.into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: SYMBOL.into(),
    }
}

struct Rig {
    engine: Engine<TxFlow>,
    now: f64,
}

impl Rig {
    fn new() -> Self {
        let (engine, _, _) = support::ready();
        let mut rig = Self {
            engine,
            now: 20_000.0,
        };
        // Releases the opening subscriptions' pacing.
        rig.command(None);
        rig
    }

    /// Every action `command` produces once the feeds' pacing has flushed.
    fn command(&mut self, command: Option<Command>) -> Vec<Action> {
        self.now += 1_000.0;
        if let Some(command) = command {
            self.engine.handle(self.now, Event::Command(command));
        }
        let mut out = safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now);
        let mut fired = HashSet::new();
        loop {
            let due: Vec<_> = out
                .iter()
                .filter_map(|a| match a {
                    Action::SetTimer { timer, at_ms }
                        if *at_ms <= self.now + 900.0 && !fired.contains(timer) =>
                    {
                        Some((*timer, *at_ms))
                    }
                    _ => None,
                })
                .collect();
            if due.is_empty() {
                return out;
            }
            for (timer, at) in due {
                fired.insert(timer);
                self.engine.handle(at, Event::TimerFired { timer });
                out.extend(safe_cex_engine::test_support::prepared_actions(
                    &mut self.engine,
                    at,
                ));
            }
        }
    }
}

fn sent(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn repair_warnings(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::Event(event))
                if event.code == code::SUBSCRIPTION_REJECTED && event.level == Level::Warn =>
            {
                Some(event.message.clone())
            }
            _ => None,
        })
        .collect()
}

fn assert_repair_resubscribes_once(
    subscribe: Command,
    unsubscribe: Command,
    topic: SubscriptionTopic,
    frame: &str,
) {
    let mut rig = Rig::new();
    assert_eq!(sent(&rig.command(Some(subscribe))), [frame]);
    let unsubscribe_frame =
        frame.replacen(r#""method":"subscribe""#, r#""method":"unsubscribe""#, 1);

    let repaired = rig.command(Some(Command::RepairSubscription(topic)));
    assert_eq!(sent(&repaired), [unsubscribe_frame.as_str(), frame]);
    assert!(
        repair_warnings(&repaired).is_empty(),
        "{:?}",
        repair_warnings(&repaired)
    );

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    assert_eq!(sent(&rig.command(Some(unsubscribe))), [unsubscribe_frame]);
    assert!(sent(&rig.command(None)).is_empty());
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: SYMBOL.into(),
            interval: Interval::M5,
        },
        candles(),
        CANDLE,
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        },
        book(),
        BOOK,
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut rig = Rig::new();
        let out = rig.command(Some(Command::RepairSubscription(topic.clone())));
        assert!(sent(&out).is_empty(), "{out:?}");
        let warnings = repair_warnings(&out);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!("TxFlow cannot repair the {topic} subscription")),
            "{}",
            warnings[0]
        );
        assert!(
            warnings[0].ends_with("is not subscribed"),
            "{}",
            warnings[0]
        );
    }
}
