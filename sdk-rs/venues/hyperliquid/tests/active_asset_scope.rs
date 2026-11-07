//! activeAssetData streams only for the selected symbol and symbols with open orders:
//! one per market put ~200 subscriptions on every keyed account, and Hyperliquid caps an
//! IP at 1,000, so about five accounts behind one IP silenced each other.
mod support;

use safe_cex_engine::model::MarginType;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Response, SocketId};
use safe_cex_venue_hyperliquid::Hyperliquid;

fn ready() -> (Engine<Hyperliquid>, SocketId, Vec<String>) {
    support::ready_with_policy("disabled", safe_cex_engine::PublicFeedPolicy::Direct)
}

fn asset_topics(sent: &[String], method: &str) -> Vec<String> {
    sent.iter()
        .filter(|text| text.contains("\"type\":\"activeAssetData\""))
        .filter(|text| text.contains(&format!("\"method\":\"{method}\"")))
        .filter_map(|text| {
            let value: serde_json::Value = serde_json::from_str(text).ok()?;
            Some(value["subscription"]["coin"].as_str()?.to_owned())
        })
        .collect()
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

fn select(e: &mut Engine<Hyperliquid>, at: f64, command: u32, symbol: &str) -> Vec<Action> {
    e.handle(
        at,
        Event::Command(Command::FetchLeverage {
            command,
            symbol: symbol.into(),
        }),
    );
    prepared_now(e)
}

fn leverage_read(actions: &[Action]) -> &safe_cex_engine::HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(request) if support::kind(request) == "activeAssetData" => Some(request),
            _ => None,
        })
        .expect("a fresh activeAssetData read")
}

fn answer(
    e: &mut Engine<Hyperliquid>,
    at: f64,
    request: &safe_cex_engine::HttpRequest,
    body: &str,
) -> Vec<Action> {
    e.handle(
        at,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    prepared_now(e)
}

fn done(actions: &[Action], command: u32) -> Option<String> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: id,
            result,
        }) if *id == command => Some(result.clone()),
        _ => None,
    })
}

#[test]
fn a_keyed_account_starts_with_no_active_asset_data() {
    let (_, _, initial) = ready();
    assert!(
        asset_topics(&initial, "subscribe").is_empty(),
        "no per-market settings stream: {initial:?}"
    );
}

#[test]
fn selecting_a_symbol_streams_its_settings_and_answers_from_a_fresh_read() {
    let (mut e, _, _) = ready();
    let actions = select(&mut e, 2000., 41, "ETH-PERP");
    assert_eq!(asset_topics(&sent(&actions), "subscribe"), ["ETH"]);
    let read = leverage_read(&actions);
    assert!(read.body.as_deref().unwrap().contains("\"coin\":\"ETH\""));
    let answered = answer(
        &mut e,
        2001.,
        read,
        r#"{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","coin":"ETH","leverage":{"type":"isolated","value":7},"maxTradeSzs":["1","1"],"availableToTrade":["1","1"]}"#,
    );
    assert_eq!(
        done(&answered, 41).as_deref(),
        Some(
            r#"{"buyLeverage":7,"sellLeverage":7,"marginType":"isolated","positionMode":"oneway"}"#
        )
    );
    let setting = e.store().setting("ETH-PERP").unwrap();
    assert_eq!(
        (setting.leverage, setting.margin_type),
        (7., Some(MarginType::Isolated))
    );

    // A new selection moves the stream; nothing else in scope keeps ETH.
    let moved = select(&mut e, 2002., 42, "BTC-PERP");
    assert_eq!(asset_topics(&sent(&moved), "unsubscribe"), ["ETH"]);
    assert_eq!(asset_topics(&sent(&moved), "subscribe"), ["BTC"]);
}

#[test]
fn a_held_symbols_read_answers_without_overriding_its_position() {
    let (mut e, _, _) = ready();
    let before = e.store().setting("BTC-PERP").map(|s| s.leverage);
    let actions = select(&mut e, 2000., 43, "BTC-PERP");
    let answered = answer(
        &mut e,
        2001.,
        leverage_read(&actions),
        r#"{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","coin":"BTC","leverage":{"type":"cross","value":3},"maxTradeSzs":["1","1"],"availableToTrade":["1","1"]}"#,
    );
    assert!(done(&answered, 43).unwrap().contains("\"buyLeverage\":3"));
    assert_eq!(
        e.store().setting("BTC-PERP").map(|s| s.leverage),
        before,
        "clearinghouseState owns a held symbol's leverage"
    );
}

#[test]
fn an_open_order_streams_its_symbol_until_it_closes() {
    let (mut e, socket, _) = ready();
    let order = |status: &str| {
        format!(
            r#"{{"channel":"orderUpdates","data":[{{"order":{{"coin":"ETH","oid":9,"side":"B","limitPx":"90","origSz":"1","sz":"1","reduceOnly":false,"orderType":"Limit","timestamp":2000}},"status":"{status}","statusTimestamp":2000}}]}}"#
        )
    };
    e.handle(
        2000.,
        Event::WsMessage {
            socket,
            text: &order("open"),
        },
    );
    assert_eq!(
        asset_topics(&sent(&prepared_now(&mut e)), "subscribe"),
        ["ETH"]
    );
    e.handle(
        2001.,
        Event::WsMessage {
            socket,
            text: &order("canceled"),
        },
    );
    assert_eq!(
        asset_topics(&sent(&prepared_now(&mut e)), "unsubscribe"),
        ["ETH"]
    );
}

#[test]
fn the_account_reports_its_subscription_count() {
    let (mut e, _, initial) = ready();
    // Past the minute between reports, so the selection's change is reported.
    let actions = select(&mut e, 70_000., 44, "ETH-PERP");
    let fields = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::SUBSCRIPTION_COUNT =>
            {
                Some(event.fields.clone())
            }
            _ => None,
        })
        .expect("a subscription.count event");
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.clone())
    };
    let total = initial
        .iter()
        .filter(|text| text.contains("\"method\":\"subscribe\""))
        .count()
        + 1;
    assert_eq!(field("subscriptions"), Some((total as f64).into()));
    assert_eq!(field("activeAssetDataSubscriptions"), Some(1.0.into()));
}
