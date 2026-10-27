mod support;

use safe_cex_engine::{Action, Command, Event, SocketChannel, SocketId, Venue};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::{Material, Prepared, TestEngine};

const TICK: &str = r#"{"topic":"instrumentInfo.all","data":[{"s":"BTC-USDT","p":"60000","xp":"60001","pr":"0.0123","o":"20","fr":"-0.0002","v":"4","to":"240000"}]}"#;

fn connecting(private: bool) -> (TestEngine, SocketId, Option<SocketId>) {
    let mut engine = if private {
        support::private_engine(Options::default(), &Material::default())
    } else {
        safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
            public_data_only: true,
            ..Default::default()
        }))
    };
    engine.start(0.0);
    let catalogue = support::requests(&mut engine, 0.0).remove(0);
    support::response(&mut engine, &catalogue, 10.0, support::MARKETS);
    let actions: Vec<_> = engine.prepared(10.0).collect();
    let socket = |role| {
        actions.iter().find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(channel),
                ..
            } if *channel == role => Some(*socket),
            _ => None,
        })
    };
    (
        engine,
        socket(SocketChannel::Public).expect("genuine public connect"),
        socket(SocketChannel::Private),
    )
}

#[test]
fn actual_apex_binding_declares_managed_socket_custody() {
    assert!(<ApexOmni as Venue>::managed_sockets());
}

#[test]
fn a_duplicate_public_open_cannot_replay_topics_or_reset_supervision() {
    let (mut engine, public, _) = connecting(false);
    engine.handle(20.0, Event::WsOpened { socket: public });
    let first: Vec<_> = engine.prepared(20.0).collect();
    assert!(first.iter().any(|a| matches!(a,
        Action::WsSend { socket, text } if *socket == public && text.contains("instrumentInfo.all"))));
    engine.handle(21.0, Event::WsOpened { socket: public });
    assert!(
        engine.prepared(21.0).next().is_none(),
        "duplicate public Opened must not replay frames or replace timers"
    );
}

#[test]
fn a_duplicate_private_open_cannot_repeat_login_subscriptions_or_reads() {
    let (mut engine, _, private) = connecting(true);
    let private = private.expect("installed Primary opens private feed");
    engine.handle(20.0, Event::WsOpened { socket: private });
    let first: Vec<_> = engine.prepared(20.0).collect();
    let sends: Vec<_> = first
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { socket, text } if *socket == private => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(sends[0].contains(r#""op":"login""#));
    assert_eq!(
        sends[1],
        r#"{"op":"subscribe","args":["ws_zk_accounts_v3","ws_user_v1","ws_notify_v1"]}"#
    );
    assert_eq!(sends[2], r#"{"op":"ping"}"#);
    engine.handle(21.0, Event::WsOpened { socket: private });
    assert!(
        engine.prepared(21.0).next().is_none(),
        "duplicate private Opened must not grant another login or refresh"
    );
}

#[test]
fn a_connecting_public_epoch_cannot_publish_before_its_first_open() {
    let (mut engine, public, _) = connecting(false);
    let before = engine.store().tickers()[0].clone();
    engine.handle(
        11.0,
        Event::WsMessage {
            socket: public,
            text: TICK,
        },
    );
    assert_eq!(engine.store().tickers()[0].last, before.last);
    assert!(engine.prepared(11.0).next().is_none());
    engine.handle(20.0, Event::WsOpened { socket: public });
    engine.prepared(20.0).for_each(drop);
    engine.handle(
        21.0,
        Event::WsMessage {
            socket: public,
            text: TICK,
        },
    );
    assert_eq!(engine.store().tickers()[0].last, 60000.0);
}

#[test]
fn connecting_book_and_candle_demand_is_sent_on_the_first_open() {
    let (mut engine, public, _) = connecting(false);
    for command in [
        Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        },
        Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        },
    ] {
        engine.handle(11.0, Event::Command(command));
        assert!(
            engine
                .prepared(11.0)
                .all(|a| !matches!(a, Action::WsSend { .. }))
        );
    }
    engine.handle(20.0, Event::WsOpened { socket: public });
    let sends: Vec<_> = engine
        .prepared(20.0)
        .filter_map(|a| match a {
            Action::WsSend { socket, text } if socket == public => Some(text),
            _ => None,
        })
        .collect();
    assert!(sends.iter().any(|t| t.contains("orderBook200.H.BTCUSDT")));
    assert!(sends.iter().any(|t| t.contains("candle.1.BTCUSDT")));
    assert!(!sends.iter().any(|t| t.contains("recentlyTrade")));
}
