//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, Response, SocketId, SubscriptionTopic,
};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"500","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"20690.1","askRp":"20690.2","lastRp":"20690.2","markRp":"20690.3","indexRp":"20690","openRp":"20000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;

fn candles() -> SubscriptionTopic {
    SubscriptionTopic::Candles {
        symbol: "BTCUSDT".into(),
        interval: Interval::M5,
    }
}

fn book() -> SubscriptionTopic {
    SubscriptionTopic::OrderBook {
        symbol: "BTCUSDT".into(),
    }
}

fn actions(engine: &mut Engine<PhemexPerpetuals>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

/// A public-only engine with its public socket open and ready.
fn opened() -> Engine<PhemexPerpetuals> {
    let mut engine = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(0.);
    for (at, path, body) in [(1., "products", PRODUCTS), (2., "24hr/all", DAILY)] {
        let request = actions(&mut engine, at - 1.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(request) if request.url.contains(path) => Some(request.id),
                _ => None,
            })
            .expect("public discovery request");
        engine.handle(
            at,
            Event::HttpResponse {
                request,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
    }
    let socket: SocketId = actions(&mut engine, 2.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("public connect");
    engine.handle(3., Event::WsOpened { socket });
    actions(&mut engine, 3.);
    engine
}

/// Every frame `command` sends, as `method params` without its rpc id.
fn frames(engine: &mut Engine<PhemexPerpetuals>, command: Command) -> (Vec<String>, Vec<Action>) {
    #[derive(serde::Deserialize)]
    struct Rpc {
        method: String,
        params: serde_json::Value,
    }
    engine.handle(4., Event::Command(command));
    let out = actions(engine, 4.);
    let sent = out
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => {
                let rpc: Rpc = serde_json::from_str(text).expect("rpc frame");
                Some(format!("{} {}", rpc.method, rpc.params))
            }
            _ => None,
        })
        .collect();
    (sent, out)
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
    expected: [&str; 2],
) {
    let mut engine = opened();
    frames(&mut engine, subscribe);

    let (repaired, out) = frames(&mut engine, Command::RepairSubscription(topic));
    assert_eq!(repaired, expected);
    assert!(
        repair_warnings(&out).is_empty(),
        "{:?}",
        repair_warnings(&out)
    );

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let (released, _) = frames(&mut engine, unsubscribe);
    assert_eq!(released, [expected[0]]);
    assert!(actions(&mut engine, 5.).is_empty());
}

#[test]
fn repairing_a_live_candle_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M5,
        },
        Command::UnsubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M5,
        },
        candles(),
        [
            r#"kline_p.unsubscribe ["BTCUSDT",300]"#,
            r#"kline_p.subscribe ["BTCUSDT",300]"#,
        ],
    );
}

#[test]
fn repairing_a_live_book_resubscribes_exactly_that_topic() {
    assert_repair_resubscribes_once(
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        book(),
        [
            r#"orderbook_p.unsubscribe ["BTCUSDT"]"#,
            r#"orderbook_p.subscribe ["BTCUSDT",false,0]"#,
        ],
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut engine = opened();
        let (sent, out) = frames(&mut engine, Command::RepairSubscription(topic.clone()));
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&out);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!(
                "PhemexPerpetuals cannot repair the {topic} subscription"
            )),
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
