//! `Command::RepairSubscription`: a dead stream is re-subscribed natively
//! without changing what the account wants.
use safe_cex_engine::log::code;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{
    Action, Command, Engine, Event, Level, Notice, RequestId, Response, SubscriptionTopic,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::PreparedActions;
use support::setup::Options;

const NOW: f64 = 1_773_295_700_000.0;
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;

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

fn respond(e: &mut Engine<WeexFutures>, request: RequestId, body: &str) {
    e.handle(
        NOW,
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

fn request(actions: &[Action], path: &str) -> RequestId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.id),
            _ => None,
        })
        .unwrap()
}

/// A public-only engine with its public socket open and its tickers subscribed.
fn started() -> Engine<WeexFutures> {
    let mut e = support::setup::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(NOW);
    let id = request(&e.prepared_actions(NOW).collect::<Vec<_>>(), "exchangeInfo");
    respond(&mut e, id, INFO);
    let actions: Vec<_> = e.prepared_actions(NOW).collect();
    respond(
        &mut e,
        request(&actions, "24hr"),
        r#"[{"symbol":"BTCUSDT","lastPrice":"102623.90","priceChangePercent":"-1.96","volume":"176145.66489","quoteVolume":"18115688543.1"}]"#,
    );
    respond(
        &mut e,
        request(&actions, "bookTicker"),
        r#"[{"symbol":"BTCUSDT","bidPrice":"102623.8","askPrice":"102623.9"}]"#,
    );
    let socket = e
        .prepared_actions(NOW)
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(NOW, Event::WsOpened { socket });
    e.prepared_actions(NOW).for_each(drop);
    e
}

#[derive(serde::Deserialize)]
struct Rpc {
    method: String,
    params: Vec<String>,
    id: u32,
}

/// The subscription frames `command` sends, as `METHOD params`, and every action.
fn command(
    e: &mut Engine<WeexFutures>,
    ids: &mut Vec<u32>,
    command: Command,
) -> (Vec<String>, Vec<Action>) {
    e.handle(NOW, Event::Command(command));
    let out: Vec<_> = e.prepared_actions(NOW).collect();
    if let Some(depth) = out
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.url.contains("/depth")))
        .then(|| request(&out, "/depth"))
    {
        respond(
            e,
            depth,
            r#"{"lastUpdateId":160,"bids":[["100","2"]],"asks":[]}"#,
        );
        let mut out = out;
        out.extend(e.prepared_actions(NOW));
        return frames(ids, out);
    }
    frames(ids, out)
}

fn frames(ids: &mut Vec<u32>, out: Vec<Action>) -> (Vec<String>, Vec<Action>) {
    let sent = out
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => {
                let rpc: Rpc = serde_json::from_str(text).expect("rpc frame");
                ids.push(rpc.id);
                Some(format!("{} {}", rpc.method, rpc.params.join(",")))
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
    key: &str,
) {
    let mut e = started();
    let mut ids = Vec::new();
    let (subscribed, _) = command(&mut e, &mut ids, subscribe);
    assert_eq!(subscribed, [format!("SUBSCRIBE {key}")]);

    let (repaired, out) = command(&mut e, &mut ids, Command::RepairSubscription(topic));
    assert_eq!(
        repaired,
        [format!("UNSUBSCRIBE {key}"), format!("SUBSCRIBE {key}")]
    );
    assert!(
        repair_warnings(&out).is_empty(),
        "{:?}",
        repair_warnings(&out)
    );

    // Intent is unchanged: the account's one subscriber still unsubscribes once.
    let (released, _) = command(&mut e, &mut ids, unsubscribe);
    assert_eq!(released, [format!("UNSUBSCRIBE {key}")]);
    assert!(e.prepared_actions(NOW).next().is_none());
    // The repair spent its request ids; none is reused.
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
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
        "BTCUSDT@kline_5m_LAST_PRICE",
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
        "BTCUSDT@depth200",
    );
}

#[test]
fn repairing_an_unsubscribed_topic_sends_nothing_and_warns() {
    for topic in [candles(), book()] {
        let mut e = started();
        let (sent, out) = command(
            &mut e,
            &mut Vec::new(),
            Command::RepairSubscription(topic.clone()),
        );
        assert!(sent.is_empty(), "{sent:?}");
        let warnings = repair_warnings(&out);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(&format!(
                "WeexFutures cannot repair the {topic} subscription"
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
