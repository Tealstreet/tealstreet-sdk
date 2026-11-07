//! Native protocol fixtures; lifecycle controls exercise actual engine ingress.
mod support;
use safe_cex_engine::{Action, Engine, Event, Response, SocketId};
use safe_cex_venue_phemex::PhemexPerpetuals;
use support::Options;

const PRODUCTS: &str = r#"{"code":0,"data":{"perpProductsV2":[{"symbol":"BTCUSDT","type":"PerpetualV2","settleCurrency":"USDT","baseCurrency":"BTC","quoteCurrency":"USDT","qtyStepSize":"0.001","tickSize":"0.1","maxOrderQtyRq":"500","maxLeverage":"100"}]}}"#;
const DAILY: &str = r#"{"error":null,"result":[{"symbol":"BTCUSDT","bidRp":"20690.1","askRp":"20690.2","lastRp":"20690.2","markRp":"20690.3","indexRp":"20690","openRp":"20000","openInterestRv":"250","fundingRateRr":"0.0001","volumeRq":"10"}]}"#;
const TICKER: &str = r#"{"method":"perp_market24h_pack_p.update","data":[["BTCUSDT","20000",0,0,"21000","12",0,"300","20999","21001","0.0002",0]]}"#;

fn actions(engine: &mut Engine<PhemexPerpetuals>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}

fn connecting() -> (Engine<PhemexPerpetuals>, SocketId) {
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

/// Phemex's documented `kline_p` shapes: a subscribe answers with history as a
/// `snapshot` (newest first), then live rows come as `incremental`.
const KLINE_SNAPSHOT: &str = r#"{"kline_p":[[1666854120,60,"20735.3","20735.3","20741.0","20735.3","20740.1","0.120","2488.8"],[1666854060,60,"20730.0","20730.0","20736.0","20729.5","20735.3","0.300","6220.6"],[1666854000,60,"20725.0","20725.0","20731.0","20724.0","20730.0","0.200","4146.0"]],"sequence":62147218,"symbol":"BTCUSDT","type":"snapshot"}"#;
const KLINE_INCREMENTAL: &str = r#"{"kline_p":[[1666854180,60,"20740.1","20740.1","20745.0","20740.1","20744.2","0.050","1037.2"]],"sequence":62147219,"symbol":"BTCUSDT","type":"incremental"}"#;

fn subscribed_to_candles() -> (Engine<PhemexPerpetuals>, SocketId) {
    use safe_cex_engine::{Command, model::Interval};
    let (mut engine, socket, _) = opened();
    engine.handle(
        4.,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: Interval::M1,
        }),
    );
    actions(&mut engine, 4.);
    engine.take_frame(4.);
    (engine, socket)
}

/// Each candle in a frame's candles section, by its timestamp: rows end the section as six f64s.
fn candle_timestamps(frame: &[u8]) -> Vec<f64> {
    safe_cex_engine::wire::sections(frame)
        .filter(|(kind, _)| *kind == 6)
        .flat_map(|(_, body)| {
            let count = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
            let rows = &body[body.len() - count * 48..];
            (0..count)
                .map(|row| f64::from_le_bytes(rows[row * 48..row * 48 + 8].try_into().unwrap()))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn kline_snapshots_never_publish_live_candles() {
    let (mut engine, socket) = subscribed_to_candles();
    // Multi-row, single-row and empty history snapshots must all stay out of
    // the live path, including a delayed snapshot after a live update.
    for text in [
        KLINE_SNAPSHOT.to_owned(),
        KLINE_INCREMENTAL.replace("incremental", "snapshot"),
        r#"{"kline_p":[],"symbol":"BTCUSDT","type":"snapshot"}"#.to_owned(),
    ] {
        engine.handle(
            5.,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        let frame = engine.take_frame(5.).unwrap_or_default();
        assert!(
            candle_timestamps(frame).is_empty(),
            "snapshot must never publish a live candle: {text}"
        );
    }
    engine.handle(
        6.,
        Event::WsMessage {
            socket,
            text: KLINE_INCREMENTAL,
        },
    );
    let live = engine.take_frame(6.).expect("the live candle").to_vec();
    assert_eq!(candle_timestamps(&live), [1666854180.0]);
    engine.handle(
        7.,
        Event::WsMessage {
            socket,
            text: KLINE_SNAPSHOT,
        },
    );
    assert!(candle_timestamps(engine.take_frame(7.).unwrap_or_default()).is_empty());
}

#[test]
fn live_klines_keep_seconds_and_native_ohlcv() {
    let (mut engine, socket) = subscribed_to_candles();
    engine.handle(
        5.,
        Event::WsMessage {
            socket,
            text: KLINE_INCREMENTAL,
        },
    );
    let frame = engine.take_frame(5.).expect("live candle");
    assert_eq!(candle_timestamps(frame), [1666854180.0]);
    let (_, body) = safe_cex_engine::wire::sections(frame)
        .find(|(kind, _)| *kind == 6)
        .unwrap();
    let row: Vec<_> = body[body.len() - 48..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|bytes| f64::from_le_bytes(*bytes))
        .collect();
    assert_eq!(row, [1666854180., 20740.1, 20745., 20740.1, 20744.2, 0.05]);
}

#[test]
fn only_incremental_klines_can_publish_live_candles() {
    let (mut engine, socket) = subscribed_to_candles();
    for text in [
        KLINE_INCREMENTAL.replace("incremental", "backfill"),
        KLINE_INCREMENTAL.replace(r#","type":"incremental""#, ""),
    ] {
        engine.handle(
            5.,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        assert!(
            candle_timestamps(engine.take_frame(5.).unwrap_or_default()).is_empty(),
            "unclassified/backfill rows must never enter the live path: {text}"
        );
    }
}

fn malformed_notices(output: &[Action]) -> Vec<&safe_cex_engine::log::LogEvent> {
    output
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::STREAM_MALFORMED =>
            {
                Some(event)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn every_kline_parse_failure_logs_raw_byte_size_and_type() {
    let (mut engine, socket) = subscribed_to_candles();
    let short =
        r#"{"kline_p":[[1666854180,60]],"symbol":"BTCUSDT","type":"incremental","extra":"€"}"#;
    for at in [5., 6., 6.] {
        engine.handle(
            at,
            Event::WsMessage {
                socket,
                text: short,
            },
        );
    }
    let output = actions(&mut engine, 6.);
    let notices = malformed_notices(&output);
    assert_eq!(notices.len(), 3, "every failed frame needs a diagnostic");
    for event in notices {
        assert_eq!(event.level, safe_cex_engine::Level::Warn);
        assert!(
            event.message.contains(&format!("bytes={}", short.len())),
            "{}",
            event.message
        );
        assert!(
            event.message.contains(r#"type="incremental""#),
            "{}",
            event.message
        );
    }
    assert!(candle_timestamps(engine.take_frame(6.).unwrap_or_default()).is_empty());
}

#[test]
fn malformed_kline_shapes_are_logged_without_publishing_partial_rows() {
    let (mut engine, socket) = subscribed_to_candles();
    let mixed = KLINE_INCREMENTAL.replace(r#""1037.2"]]"#, r#""1037.2"],[1666854240,60]]"#);
    let bad_number = KLINE_INCREMENTAL.replace("20744.2", "bad");
    let bad_type = KLINE_INCREMENTAL.replace(r#""type":"incremental""#, r#""type":123"#);
    for (text, kind) in [
        (mixed.as_str(), r#""incremental""#),
        (bad_number.as_str(), r#""incremental""#),
        (bad_type.as_str(), "123"),
        (
            r#"{"kline_p":{},"symbol":"BTCUSDT","type":"snapshot"}"#,
            r#""snapshot""#,
        ),
        (
            r#"{"kline_p":[[1666854180,60]],"symbol":123,"type":"incremental"}"#,
            r#""incremental""#,
        ),
        (r#"{"kline_p":[],"type":"incremental""#, "unknown"),
    ] {
        engine.handle(5., Event::WsMessage { socket, text });
        let output = actions(&mut engine, 5.);
        let notices = malformed_notices(&output);
        assert_eq!(notices.len(), 1, "failed frame: {text}");
        assert!(
            notices[0]
                .message
                .contains(&format!("bytes={}", text.len())),
            "{}",
            notices[0].message
        );
        assert!(
            notices[0].message.contains(&format!("type={kind}")),
            "{}",
            notices[0].message
        );
        assert!(candle_timestamps(engine.take_frame(5.).unwrap_or_default()).is_empty());
    }
    // A rejected frame must not prevent subsequent live delivery.
    engine.handle(
        6.,
        Event::WsMessage {
            socket,
            text: KLINE_INCREMENTAL,
        },
    );
    assert_eq!(
        candle_timestamps(engine.take_frame(6.).unwrap()),
        [1666854180.]
    );
}

#[test]
fn other_public_parse_failures_also_log_frame_size_and_type() {
    let (mut engine, socket) = subscribed_to_candles();
    for (text, kind) in [
        (
            r#"{"method":"perp_market24h_pack_p.update","data":false}"#,
            "unknown",
        ),
        (
            r#"{"orderbook_p":false,"symbol":"BTCUSDT","sequence":1,"type":"snapshot"}"#,
            r#""snapshot""#,
        ),
    ] {
        engine.handle(5., Event::WsMessage { socket, text });
        let output = actions(&mut engine, 5.);
        let notices = malformed_notices(&output);
        assert_eq!(notices.len(), 1, "failed public frame: {text}");
        assert!(
            notices[0]
                .message
                .contains(&format!("bytes={}", text.len()))
        );
        assert!(notices[0].message.contains(&format!("type={kind}")));
    }
}
