mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, Notice, Query, Response, SocketId, TimerId, wire,
};
use safe_cex_venue_bitrue::Bitrue;
use serde::Deserialize;
use support::Options;

const SYMBOL: &str = "E-BTC-USDT";
const CANDLE: &str = r#"{"ds":"2024-01-01 08:00:00","open":60000,"high":60001,"low":59999,"close":60000,"vol":1000}"#;
const TRADE: &str = r#"{"side":"buy","price":60000,"vol":1,"amount":60000}"#;
#[derive(Deserialize)]
struct Sent {
    event: String,
    params: Params,
}
#[derive(Deserialize)]
struct Params {
    cb_id: String,
    #[serde(default)]
    top: Option<u32>,
    #[serde(default, rename = "endIdx")]
    end_idx: Option<String>,
}
fn requests(actions: &[Action]) -> Vec<Sent> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => serde_json::from_str::<Sent>(text)
                .ok()
                .filter(|s| s.event == "req"),
            _ => None,
        })
        .collect()
}
fn completions(actions: &[Action]) -> Vec<(u32, Result<String, String>)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) => {
                Some((*command, Ok(result.clone())))
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                Some((*command, Err(error.legacy_text())))
            }
            _ => None,
        })
        .collect()
}
fn drain(e: &mut Engine<Bitrue>, now: f64) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now).into_iter()
}
fn boot() -> (Engine<Bitrue>, SocketId) {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.0);
    let r = drain(&mut e, 0.0)
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    e.handle(1.0, Event::HttpResponse { request: r.id, response: Response {
        status: 200, headers: "", body: r#"[{"symbol":"E-BTC-USDT","pricePrecision":2,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":1000,"status":1}]"#,
    }});
    let socket = drain(&mut e, 1.0)
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(2.0, Event::WsOpened { socket });
    drain(&mut e, 2.0).for_each(drop);
    e.take_frame(2.0);
    (e, socket)
}
fn message(e: &mut Engine<Bitrue>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
fn candles(e: &mut Engine<Bitrue>, now: f64, command: u32, to: Option<f64>) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: to,
        }),
    );
}
fn trades(e: &mut Engine<Bitrue>, command: u32, limit: u32) {
    e.handle(
        2.0,
        Event::Command(Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: SYMBOL.into(),
                limit: Some(limit),
            },
        }),
    );
}
fn reconnect(e: &mut Engine<Bitrue>, actions: Vec<Action>, now: f64) -> SocketId {
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > now && *at_ms <= now + 1000.0 => {
                Some(*timer)
            }
            _ => None,
        })
        .unwrap();
    e.handle(now + 1000.0, Event::TimerFired { timer });
    let socket = drain(e, now + 1000.0)
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(now + 1001.0, Event::WsOpened { socket });
    drain(e, now + 1001.0).for_each(drop);
    socket
}
#[test]
fn subscribed_candles_keep_routing_after_reconnect() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    drain(&mut e, 2.0).for_each(drop);
    e.handle(3.0, Event::WsClosed { socket, code: 1006 });
    let actions = drain(&mut e, 3.0).collect();
    let next = reconnect(&mut e, actions, 3.0);
    e.take_frame(1004.0);
    message(
        &mut e,
        next,
        1005.0,
        &format!(r#"{{"channel":"market_e_btcusdt_kline_1min","tick":{CANDLE}}}"#),
    );
    let frame = e.take_market_frame(1005.0).unwrap();
    assert!(wire::sections(frame).any(|(kind, _)| kind == wire::section::CANDLES));
}
#[test]
fn different_trade_limits_are_serialized_and_mismatched_reply_ids_are_ignored() {
    let (mut e, socket) = boot();
    trades(&mut e, 1, 1);
    let first_actions: Vec<_> = drain(&mut e, 2.0).collect();
    let first = requests(&first_actions).remove(0);
    assert_eq!(first.params.top, Some(1));
    trades(&mut e, 2, 2);
    assert!(requests(&drain(&mut e, 2.0).collect::<Vec<_>>()).is_empty());
    message(
        &mut e,
        socket,
        3.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_trade_ticker","cb_id":"wrong","status":"ok","ts":1700000000000,"data":[{TRADE}]}}"#
        ),
    );
    assert!(completions(&drain(&mut e, 3.0).collect::<Vec<_>>()).is_empty());
    message(
        &mut e,
        socket,
        4.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_trade_ticker","cb_id":"{}","status":"ok","ts":1700000000000,"data":[{TRADE}]}}"#,
            first.params.cb_id
        ),
    );
    let actions: Vec<_> = drain(&mut e, 4.0).collect();
    let done = completions(&actions);
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].0, 1);
    assert!(done[0].1.is_ok());
    let second = requests(&actions).remove(0);
    assert_ne!(first.params.cb_id, second.params.cb_id);
    assert_eq!(second.params.top, Some(2));
    message(
        &mut e,
        socket,
        5.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_trade_ticker","cb_id":"{}","status":"ok","ts":1700000000000,"data":[{TRADE},{TRADE}]}}"#,
            second.params.cb_id
        ),
    );
    let done = completions(&drain(&mut e, 5.0).collect::<Vec<_>>());
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].0, 2);
    assert_eq!(done[0].1.as_ref().unwrap().matches("quoteQty").count(), 2);
}
#[test]
fn different_candle_windows_do_not_share_the_first_response() {
    let (mut e, socket) = boot();
    candles(&mut e, 2.0, 1, Some(1704067200000.0));
    let actions: Vec<_> = drain(&mut e, 2.0).collect();
    let first = requests(&actions).remove(0);
    assert_eq!(first.params.end_idx.as_deref(), Some("1704067200"));
    candles(&mut e, 2.0, 2, Some(1704153600000.0));
    assert!(requests(&drain(&mut e, 2.0).collect::<Vec<_>>()).is_empty());
    message(
        &mut e,
        socket,
        3.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{}","data":[{CANDLE}]}}"#,
            first.params.cb_id
        ),
    );
    let actions: Vec<_> = drain(&mut e, 3.0).collect();
    assert_eq!(completions(&actions)[0].0, 1);
    let second = requests(&actions).remove(0);
    assert_eq!(second.params.end_idx.as_deref(), Some("1704153600"));
    let tomorrow = CANDLE.replace("2024-01-01", "2024-01-02");
    message(
        &mut e,
        socket,
        4.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{}","data":[{tomorrow}]}}"#,
            second.params.cb_id
        ),
    );
    let done = completions(&drain(&mut e, 4.0).collect::<Vec<_>>());
    assert_eq!(done[0].0, 2);
    let result = done[0].1.as_ref().unwrap();
    assert!(result.contains("1704153600"));
    assert!(!result.contains("1704067200"));
}
#[test]
fn timed_out_reads_close_the_socket_and_late_responses_cannot_resolve_a_new_read() {
    let (mut e, socket) = boot();
    candles(&mut e, 2.0, 1, None);
    let actions: Vec<_> = drain(&mut e, 2.0).collect();
    let timer: TimerId = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 5002.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(5002.0, Event::TimerFired { timer });
    let actions: Vec<_> = drain(&mut e, 5002.0).collect();
    let done = completions(&actions);
    assert_eq!(done[0].0, 1);
    assert!(done[0].1.is_err());
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
    let next = reconnect(&mut e, actions, 5002.0);
    candles(&mut e, 6003.0, 2, None);
    let sent: Vec<_> = drain(&mut e, 6003.0).collect();
    let cb = requests(&sent).remove(0).params.cb_id;
    message(
        &mut e,
        socket,
        6004.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","data":[{CANDLE}]}}"#
        ),
    );
    assert!(completions(&drain(&mut e, 6004.0).collect::<Vec<_>>()).is_empty());
    message(
        &mut e,
        next,
        6005.0,
        &format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{cb}","data":[{CANDLE}]}}"#
        ),
    );
    let done = completions(&drain(&mut e, 6005.0).collect::<Vec<_>>());
    assert_eq!(done[0].0, 2);
    assert!(done[0].1.is_ok());
}
#[test]
fn rejected_candle_history_never_returns_an_authoritative_empty_list() {
    let (mut e, socket) = boot();
    candles(&mut e, 2.0, 1, None);
    drain(&mut e, 2.0).for_each(drop);
    message(
        &mut e,
        socket,
        3.0,
        r#"{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","status":"error","data":[]}"#,
    );
    let done = completions(&drain(&mut e, 3.0).collect::<Vec<_>>());
    assert_eq!(done[0].0, 1);
    assert!(done[0].1.is_err());
}

#[test]
fn malformed_candle_dates_reject_empty_and_mixed_history_before_window_filtering() {
    let invalid = CANDLE.replace("2024-01-01", "2024-02-30");
    for rows in [invalid.clone(), format!("{CANDLE},{invalid}")] {
        let (mut e, socket) = boot();
        candles(&mut e, 2.0, 1, Some(1704067260000.0));
        drain(&mut e, 2.0).for_each(drop);
        message(
            &mut e,
            socket,
            3.0,
            &format!(
                r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","status":"ok","data":[{rows}]}}"#
            ),
        );
        let done = completions(&drain(&mut e, 3.0).collect::<Vec<_>>());
        assert_eq!(done[0].0, 1);
        assert!(
            done[0]
                .1
                .as_ref()
                .is_err_and(|error| error.contains("Malformed Bitrue candle timestamp"))
        );
    }
}

#[test]
fn candle_stream_requires_close_and_ticker_null_never_overwrites_last_good_price() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    drain(&mut e, 2.0).for_each(drop);
    for candle in [
        CANDLE.replace(",\"close\":60000", ""),
        CANDLE.replace("\"close\":60000", "\"close\":null"),
    ] {
        message(
            &mut e,
            socket,
            3.0,
            &format!(r#"{{"channel":"market_e_btcusdt_kline_1min","tick":{candle}}}"#),
        );
        assert!(e.take_market_frame(3.0).is_none());
    }
    message(
        &mut e,
        socket,
        4.0,
        r#"{"channel":"market_e_btcusdt_ticker","tick":{"close":60000}}"#,
    );
    message(
        &mut e,
        socket,
        5.0,
        r#"{"channel":"market_e_btcusdt_ticker","tick":{"close":null}}"#,
    );
    assert_eq!(e.store().ticker(SYMBOL).unwrap().last, 60000.0);
}
