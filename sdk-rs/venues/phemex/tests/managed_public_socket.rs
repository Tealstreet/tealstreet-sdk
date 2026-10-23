//! Native protocol fixtures; lifecycle controls exercise actual engine ingress.
use safe_cex_engine::{Action, Engine, Event, Response, SocketId};
use safe_cex_venue_phemex::{Options, PhemexPerpetuals};

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"500","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"20690.1","askRp":"20690.2","lastRp":"20690.2","markRp":"20690.3","indexRp":"20690","openRp":"20000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
const TICKER: &str = r#"{"method":"perp_market24h_pack_p.update","data":[["BTCUSDT","20000",0,0,"21000","12",0,"300","20999","21001","0.0002",0]]}"#;

fn actions(engine: &mut Engine<PhemexPerpetuals>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

fn connecting() -> (Engine<PhemexPerpetuals>, SocketId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(PhemexPerpetuals::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    engine.start(0.);
    for (at, path, body) in [(1., "products", PRODUCTS), (2., "24hr/all", DAILY)] {
        let request = actions(&mut engine, at - 1.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(request) if request.url.contains(path) => Some(request.id),
                _ => None,
            })
            .expect("real public discovery request");
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
    let socket = actions(&mut engine, 2.)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("native public connect");
    (engine, socket)
}

fn opened() -> (Engine<PhemexPerpetuals>, SocketId, Vec<Action>) {
    let (mut engine, socket) = connecting();
    engine.handle(3., Event::WsOpened { socket });
    let opening = actions(&mut engine, 3.);
    (engine, socket, opening)
}

fn messages(actions: &[Action], socket: SocketId) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend {
                socket: current,
                text,
            } if *current == socket => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn accepted_open_preserves_exact_fixed_subscription_then_ping() {
    let (_, socket, output) = opened();
    assert_eq!(
        messages(&output, socket),
        [
            r#"{"id":1,"method":"perp_market24h_pack_p.subscribe","params":[]}"#,
            r#"{"id":2,"method":"server.ping","params":[]}"#,
        ]
    );
}

#[test]
fn current_live_packed_ticker_keeps_native_values() {
    let (mut engine, socket, _) = opened();
    engine.handle(
        4.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    let ticker = engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!(ticker.last, 21000.);
    assert_eq!(ticker.bid, 20690.1);
    assert_eq!(ticker.quote_volume, 252000.);
}

#[test]
fn duplicate_open_never_replays_subscription_or_ping() {
    let (mut engine, socket, _) = opened();
    engine.handle(4., Event::WsOpened { socket });
    let output = actions(&mut engine, 4.);
    assert!(
        messages(&output, socket).is_empty(),
        "duplicate open must not replay the live feed's native frames"
    );
}

#[test]
fn late_open_cannot_revive_an_expired_connect() {
    let (mut engine, socket) = connecting();
    engine.handle(10003., Event::WsOpened { socket });
    let output = actions(&mut engine, 10003.);
    assert!(
        messages(&output, socket).is_empty(),
        "late open must not release subscriptions after connect expiry"
    );
    assert!(
        output
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: current } if *current == socket))
    );
}

#[test]
fn late_ticker_cannot_publish_after_idle_expiry_without_a_watchdog_turn() {
    let (mut engine, socket, _) = opened();
    let previous = engine.store().ticker("BTCUSDT").unwrap().last;
    engine.handle(
        20004.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    assert_eq!(
        engine.store().ticker("BTCUSDT").unwrap().last,
        previous,
        "ordinary ingress must reject an already expired public epoch"
    );
    let output = actions(&mut engine, 20004.);
    assert!(
        output
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: current } if *current == socket))
    );
}

#[test]
fn retired_open_and_ticker_are_inert() {
    let (mut engine, socket, _) = opened();
    engine.handle(4., Event::WsClosed { socket, code: 1006 });
    actions(&mut engine, 4.);
    let previous = engine.store().ticker("BTCUSDT").unwrap().last;
    engine.handle(5., Event::WsOpened { socket });
    engine.handle(
        5.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    let output = actions(&mut engine, 5.);
    assert!(messages(&output, socket).is_empty());
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, previous);
}

#[test]
fn book_refcounts_release_only_the_first_subscribe_and_last_unsubscribe() {
    use safe_cex_engine::Command;
    let (mut engine, socket, _) = opened();
    let add = || {
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
    };
    let remove = || {
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
    };
    engine.handle(4., add());
    let first = actions(&mut engine, 4.);
    assert_eq!(messages(&first, socket).len(), 1);
    assert!(
        messages(&first, socket)[0]
            .contains(r#""method":"orderbook_p.subscribe","params":["BTCUSDT",false,0]"#)
    );
    assert!(
        !first
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
    engine.handle(5., add());
    assert!(messages(&actions(&mut engine, 5.), socket).is_empty());
    engine.handle(6., remove());
    assert!(messages(&actions(&mut engine, 6.), socket).is_empty());
    engine.handle(7., remove());
    let last = actions(&mut engine, 7.);
    assert_eq!(messages(&last, socket).len(), 1);
    assert!(
        messages(&last, socket)[0]
            .contains(r#""method":"orderbook_p.unsubscribe","params":["BTCUSDT"]"#)
    );
}

#[test]
fn reconnect_replays_each_retained_native_topic_before_opening_ping() {
    use safe_cex_engine::{Command, model::Interval};
    let (mut engine, socket, _) = opened();
    engine.handle(
        4.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    actions(&mut engine, 4.);
    engine.handle(
        5.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    actions(&mut engine, 5.);
    engine.handle(6., Event::WsClosed { socket, code: 1006 });
    let retired = actions(&mut engine, 6.);
    let (timer, at) = retired
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (756. ..=1006.).contains(at_ms) => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .expect("native bounded retry");
    engine.handle(at, Event::TimerFired { timer });
    let replacement = actions(&mut engine, at)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .expect("replacement socket");
    assert_ne!(replacement, socket);
    engine.handle(
        at + 1.,
        Event::WsOpened {
            socket: replacement,
        },
    );
    let output = actions(&mut engine, at + 1.);
    #[derive(serde::Deserialize)]
    struct Rpc {
        id: u32,
        method: String,
    }
    let frames: Vec<Rpc> = messages(&output, replacement)
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    assert_eq!(
        frames.iter().map(|f| f.method.as_str()).collect::<Vec<_>>(),
        [
            "perp_market24h_pack_p.subscribe",
            "orderbook_p.subscribe",
            "kline_p.subscribe",
            "server.ping"
        ]
    );
    assert!(frames.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(messages(&output, replacement)[1].contains(r#""params":["BTCUSDT",false,0]"#));
    assert!(messages(&output, replacement)[2].contains(r#""params":["BTCUSDT",60]"#));
    assert!(
        !output
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
}

#[test]
fn malformed_current_text_retains_the_native_any_text_liveness_policy() {
    let (mut engine, socket, _) = opened();
    engine.handle(18000., Event::WsMessage { socket, text: "{" });
    actions(&mut engine, 18000.);
    engine.handle(
        20004.,
        Event::WsMessage {
            socket,
            text: TICKER,
        },
    );
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 21000.);
}
