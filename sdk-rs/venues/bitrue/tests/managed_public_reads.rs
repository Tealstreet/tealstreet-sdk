mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Query, Response, SocketId, TimerId};
use safe_cex_venue_bitrue::Bitrue;
use support::Options;

const SYMBOL: &str = "E-BTC-USDT";
const TRADE: &str = r#"{"side":"buy","price":60000,"vol":1,"amount":60000}"#;
fn drain(e: &mut Engine<Bitrue>, now: f64) -> std::vec::IntoIter<Action> {
    safe_cex_engine::test_support::prepared_actions(e, now).into_iter()
}
fn boot() -> (Engine<Bitrue>, SocketId) {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.);
    let request = drain(&mut e, 0.)
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    e.handle(1., Event::HttpResponse { request, response: Response { status: 200, headers: "", body: r#"[{"symbol":"E-BTC-USDT","pricePrecision":2,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":1000,"status":1}]"# } });
    let socket = drain(&mut e, 1.)
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(2., Event::WsOpened { socket });
    drain(&mut e, 2.).for_each(drop);
    (e, socket)
}
fn trades(e: &mut Engine<Bitrue>, now: f64, command: u32) {
    e.handle(
        now,
        Event::Command(Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: SYMBOL.into(),
                limit: Some(2),
            },
        }),
    );
}
fn request(actions: &[Action]) -> String {
    actions
        .iter()
        .find_map(|a| match a {
            Action::WsSend { text, .. } => {
                let value: serde_json::Value = serde_json::from_str(text).ok()?;
                (value["event"] == "req")
                    .then(|| value["params"]["cb_id"].as_str().unwrap().to_owned())
            }
            _ => None,
        })
        .expect("one native public read")
}
fn reply(e: &mut Engine<Bitrue>, socket: SocketId, now: f64, cb: &str) {
    let text = format!(
        r#"{{"event_rep":"rep","channel":"market_e_btcusdt_trade_ticker","cb_id":"{cb}","status":"ok","ts":1700000000000,"data":[{TRADE}]}}"#
    );
    e.handle(
        now,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
}
fn terminal(actions: &[Action]) -> Vec<(u32, bool)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, .. }) => Some((*command, true)),
            Action::Notice(Notice::CommandFailed { command, .. }) => Some((*command, false)),
            _ => None,
        })
        .collect()
}
fn deadline(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("native queue-inclusive read deadline")
}

#[test]
fn late_ws_reply_cannot_publish_success_before_the_timer_is_delivered() {
    let (mut e, socket) = boot();
    trades(&mut e, 2., 71);
    let actions = drain(&mut e, 2.).collect::<Vec<_>>();
    let cb = request(&actions);
    let timer = deadline(&actions, 5002.);
    reply(&mut e, socket, 5003., &cb);
    assert!(
        terminal(&drain(&mut e, 5003.).collect::<Vec<_>>()).is_empty(),
        "expired response cannot consume logical timeout authority"
    );
    e.handle(5003., Event::TimerFired { timer });
    let actions = drain(&mut e, 5003.).collect::<Vec<_>>();
    assert_eq!(terminal(&actions), vec![(71, false)]);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: closed } if *closed == socket))
    );
    reply(&mut e, socket, 5004., &cb);
    assert!(terminal(&drain(&mut e, 5004.).collect::<Vec<_>>()).is_empty());
}
#[test]
fn expired_fifo_reply_cannot_dispatch_the_next_queued_owner() {
    let (mut e, socket) = boot();
    trades(&mut e, 2., 72);
    let actions = drain(&mut e, 2.).collect::<Vec<_>>();
    let cb = request(&actions);
    let timer = deadline(&actions, 5002.);
    trades(&mut e, 3., 73);
    let queued = drain(&mut e, 3.).collect::<Vec<_>>();
    deadline(&queued, 5003.);
    assert!(!queued.iter().any(|a| matches!(a, Action::WsSend { .. })));
    reply(&mut e, socket, 5004., &cb);
    let late = drain(&mut e, 5004.).collect::<Vec<_>>();
    assert!(terminal(&late).is_empty());
    assert!(
        !late.iter().any(|a| matches!(a, Action::WsSend { .. })),
        "expired first owner cannot advance the FIFO onto another expired owner"
    );
    e.handle(5004., Event::TimerFired { timer });
    let mut done = terminal(&drain(&mut e, 5004.).collect::<Vec<_>>());
    done.sort_unstable();
    assert_eq!(done, vec![(72, false), (73, false)]);
}
#[test]
fn fifo_correlation_duplicate_and_id_reuse_keep_independent_completions() {
    let (mut e, socket) = boot();
    trades(&mut e, 2., 74);
    let first = request(&drain(&mut e, 2.).collect::<Vec<_>>());
    trades(&mut e, 3., 75);
    drain(&mut e, 3.).for_each(drop);
    reply(&mut e, socket, 4., "wrong");
    assert!(terminal(&drain(&mut e, 4.).collect::<Vec<_>>()).is_empty());
    reply(&mut e, socket, 5., &first);
    let actions = drain(&mut e, 5.).collect::<Vec<_>>();
    assert_eq!(terminal(&actions), vec![(74, true)]);
    let second = request(&actions);
    reply(&mut e, socket, 6., &first);
    assert!(terminal(&drain(&mut e, 6.).collect::<Vec<_>>()).is_empty());
    reply(&mut e, socket, 7., &second);
    assert_eq!(
        terminal(&drain(&mut e, 7.).collect::<Vec<_>>()),
        vec![(75, true)]
    );
    trades(&mut e, 8., 74);
    let reused = request(&drain(&mut e, 8.).collect::<Vec<_>>());
    assert_ne!(reused, first);
    reply(&mut e, socket, 9., &first);
    assert!(terminal(&drain(&mut e, 9.).collect::<Vec<_>>()).is_empty());
    reply(&mut e, socket, 10., &reused);
    assert_eq!(
        terminal(&drain(&mut e, 10.).collect::<Vec<_>>()),
        vec![(74, true)]
    );
}

fn candles(e: &mut Engine<Bitrue>, now: f64, command: u32) {
    e.handle(
        now,
        Event::Command(Command::FetchCandles {
            command,
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
}
#[test]
fn unrelated_admission_after_expiry_cannot_erase_the_original_timer_owner() {
    let (mut e, _) = boot();
    trades(&mut e, 2., 76);
    let timer = deadline(&drain(&mut e, 2.).collect::<Vec<_>>(), 5002.);
    trades(&mut e, 5003., 77);
    drain(&mut e, 5003.).for_each(drop);
    e.handle(5003., Event::TimerFired { timer });
    let mut done = terminal(&drain(&mut e, 5003.).collect::<Vec<_>>());
    done.sort_unstable();
    assert_eq!(
        done,
        vec![(76, false), (77, false)],
        "each original owner must retain its terminal authority"
    );
}
#[test]
fn repeated_id_after_expiry_cannot_steal_the_delayed_timer_completion() {
    let (mut e, _) = boot();
    trades(&mut e, 2., 78);
    let timer = deadline(&drain(&mut e, 2.).collect::<Vec<_>>(), 5002.);
    trades(&mut e, 5003., 78);
    let repeated = drain(&mut e, 5003.).collect::<Vec<_>>();
    assert!(terminal(&repeated).is_empty());
    assert!(!repeated.iter().any(|action| matches!(
        action,
        Action::SetTimer { .. } | Action::WsSend { .. } | Action::Http(_) | Action::ManagedHttp(_)
    )));
    assert!(repeated.iter().any(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)));
    trades(&mut e, 5003., 79);
    drain(&mut e, 5003.).for_each(drop);
    e.handle(5003., Event::TimerFired { timer });
    let mut done = terminal(&drain(&mut e, 5003.).collect::<Vec<_>>());
    done.sort_unstable();
    assert_eq!(done, vec![(78, false), (79, false)]);
}
#[test]
fn exact_deadline_candle_reply_waits_for_native_timeout_cleanup() {
    let (mut e, socket) = boot();
    candles(&mut e, 2., 80);
    let actions = drain(&mut e, 2.).collect::<Vec<_>>();
    let timer = deadline(&actions, 5002.);
    let cb = request(&actions);
    let text = format!(
        r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{cb}","data":[{{"ds":"2024-01-01 08:00:00","open":60000,"high":60001,"low":59999,"close":60000,"vol":1000}}]}}"#
    );
    e.handle(
        5002.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    assert!(terminal(&drain(&mut e, 5002.).collect::<Vec<_>>()).is_empty());
    e.handle(5002., Event::TimerFired { timer });
    assert_eq!(
        terminal(&drain(&mut e, 5002.).collect::<Vec<_>>()),
        vec![(80, false)]
    );
}
#[test]
fn first_timeout_still_fails_live_siblings_on_both_channels() {
    let (mut e, socket) = boot();
    trades(&mut e, 2., 81);
    let timer = deadline(&drain(&mut e, 2.).collect::<Vec<_>>(), 5002.);
    trades(&mut e, 3., 82);
    drain(&mut e, 3.).for_each(drop);
    candles(&mut e, 4., 83);
    drain(&mut e, 4.).for_each(drop);
    e.handle(5002., Event::TimerFired { timer });
    let actions = drain(&mut e, 5002.).collect::<Vec<_>>();
    let mut done = terminal(&actions);
    done.sort_unstable();
    assert_eq!(done, vec![(81, false), (82, false), (83, false)]);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: closed } if *closed == socket))
    );
}
#[test]
fn empty_correlation_and_capacity_refusal_keep_native_compatibility() {
    let (mut e, socket) = boot();
    trades(&mut e, 2., 84);
    drain(&mut e, 2.).for_each(drop);
    reply(&mut e, socket, 3., "");
    assert_eq!(
        terminal(&drain(&mut e, 3.).collect::<Vec<_>>()),
        vec![(84, true)]
    );
    for command in 100..132 {
        trades(&mut e, 4., command);
        drain(&mut e, 4.).for_each(drop);
    }
    trades(&mut e, 4., 132);
    let actions = drain(&mut e, 4.).collect::<Vec<_>>();
    assert_eq!(terminal(&actions), vec![(132, false)]);
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}

#[test]
fn rejected_or_malformed_candle_head_fails_once_then_dispatches_next_owner() {
    for first_data in [
        r#""status":"error","data":[]"#,
        r#""status":"ok","data":[{"ds":"bad","open":1,"high":2,"low":1,"close":2,"vol":1}]"#,
    ] {
        let (mut e, socket) = boot();
        candles(&mut e, 2., 85);
        let first = request(&drain(&mut e, 2.).collect::<Vec<_>>());
        candles(&mut e, 3., 86);
        drain(&mut e, 3.).for_each(drop);
        let text = format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{first}",{first_data}}}"#
        );
        e.handle(
            4.,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        let actions = drain(&mut e, 4.).collect::<Vec<_>>();
        assert_eq!(terminal(&actions), vec![(85, false)]);
        let next = request(&actions);
        assert_ne!(next, first);
        let text = format!(
            r#"{{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","cb_id":"{next}","status":"ok","data":[{{"ds":"2024-01-01 08:00:00","open":60000,"high":60001,"low":59999,"close":60000,"vol":1000}}]}}"#
        );
        e.handle(
            5.,
            Event::WsMessage {
                socket,
                text: &text,
            },
        );
        assert_eq!(
            terminal(&drain(&mut e, 5.).collect::<Vec<_>>()),
            vec![(86, true)]
        );
    }
}

#[test]
fn repeated_preflight_refusals_release_the_original_entry_without_io() {
    let (mut e, _) = boot();
    for _ in 0..270 {
        e.handle(
            3.,
            Event::Command(Command::Query {
                command: 200,
                query: Query::PublicTrades {
                    symbol: "unknown".into(),
                    limit: Some(1),
                },
            }),
        );
        let actions = drain(&mut e, 3.).collect::<Vec<_>>();
        assert_eq!(terminal(&actions), vec![(200, false)]);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.legacy_text() == "Unknown Bitrue market")));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::SetTimer { .. }
                | Action::WsSend { .. }
                | Action::Http(_)
                | Action::ManagedHttp(_)
        )));
    }
}
