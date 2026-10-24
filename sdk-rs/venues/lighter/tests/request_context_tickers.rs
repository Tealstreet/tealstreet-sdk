//! Background ticker snapshots own their preparation, response and retirement.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, SocketId, TimerId, VenueError,
};
use safe_cex_venue_lighter::{Lighter, Options};
use support::{META, NOW, reply_at};
const ROW: &str = r#"{"market_id":1,"mark_price":"100","index_price":"99","last_trade_price":"101","best_bid_price":"98","best_ask_price":"102","open_interest":"12","current_funding_rate":"0.0012","daily_base_token_volume":"20","daily_quote_token_volume":"2000","daily_price_change":"3"}"#;
const BODY: &str = r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"100"}]}"#;
const ISSUE: f64 = NOW + 2_000.;
fn boot(robinhood: bool, testnet: bool) -> (Engine<Lighter>, TimerId, SocketId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(Options {
        public_data_only: true,
        robinhood,
        testnet,
        ..Options::default()
    }));
    engine.start(NOW);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply_at(&mut engine, NOW, &request, 200, META);
    let actions: Vec<_> = engine.drain_actions().collect();
    let poll = timer(&actions, NOW + 2_000.);
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    engine.take_frame(NOW);
    (engine, poll, socket)
}
fn timer(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("expected exact native timer")
}
fn fire(engine: &mut Engine<Lighter>, now: f64, timer: TimerId) -> Vec<Action> {
    engine.handle(now, Event::TimerFired { timer });
    engine.drain_actions().collect()
}
fn prep(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.ends_with("exchangeStats"))),
        "ticker must await managed claim"
    );
    let mut preps = actions.iter().filter_map(|a| match a {
        Action::ManagedHttp(a) => match a.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                intent,
            } if request.url.ends_with("exchangeStats") => {
                assert_eq!(*intent, RoutingIntent::Public);
                Some((*preparation, request.clone()))
            }
            _ => None,
        },
        _ => None,
    });
    let result = preps.next().expect("ticker emits one managed preparation");
    assert!(preps.next().is_none());
    result
}
fn release(engine: &mut Engine<Lighter>, now: f64, token: Preparation) -> HttpRequest {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    request
}
fn no_command(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )),
        "background work cannot complete commands"
    );
}
fn wire(request: &HttpRequest, robinhood: bool, testnet: bool) {
    let host = match (robinhood, testnet) {
        (false, false) => "https://mainnet.zklighter.elliot.ai",
        (false, true) => "https://testnet-api.lighter.xyz",
        (true, false) => "https://api.rh.lighter.xyz",
        (true, true) => "https://api.rh-testnet.lighter.xyz",
    };
    assert_eq!(request.url, format!("{host}/api/v1/exchangeStats"));
    assert_eq!(request.method, Method::Get);
    assert!(request.headers.is_empty());
    assert!(request.signer_headers.is_empty());
    assert!(request.body.is_none());
}
#[test]
fn all_deployments_preserve_aliases_and_native_fields_only_after_claim() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            for path in [
                ["order_book_stats", ""],
                ["data", "order_book_stats"],
                ["result", "order_book_stats"],
                ["tickers", ""],
                ["data", "tickers"],
                ["result", "tickers"],
            ] {
                let (mut engine, poll, _) = boot(robinhood, testnet);
                let actions = fire(&mut engine, ISSUE, poll);
                let (token, unsigned) = prep(&actions);
                wire(&unsigned, robinhood, testnet);
                no_command(&actions);
                reply_at(&mut engine, ISSUE + 1., &unsigned, 200, BODY);
                engine.handle(
                    ISSUE + 2.,
                    Event::HttpStart {
                        request: unsigned.id,
                        status: 200,
                        headers: "",
                    },
                );
                engine.handle(
                    ISSUE + 2.,
                    Event::HttpChunk {
                        request: unsigned.id,
                        data: BODY.as_bytes(),
                    },
                );
                engine.handle(
                    ISSUE + 2.,
                    Event::HttpEnd {
                        request: unsigned.id,
                    },
                );
                assert_eq!(engine.store().ticker("ETH").unwrap().mark, 0.);
                assert!(engine.take_frame(ISSUE + 2.).is_none());
                no_command(&engine.drain_actions().collect::<Vec<_>>());
                let request = release(&mut engine, ISSUE + 3., token);
                wire(&request, robinhood, testnet);
                let rows = format!("[{ROW}]");
                let body = if path[1].is_empty() {
                    format!(r#"{{"code":200,"{}":[{ROW}]}}"#, path[0])
                } else {
                    format!(r#"{{"code":200,"{}":{{"{}":{rows}}}}}"#, path[0], path[1])
                };
                reply_at(&mut engine, ISSUE + 4., &request, 200, &body);
                let t = engine.store().ticker("ETH").unwrap();
                assert_eq!(
                    (t.mark, t.index, t.last, t.bid, t.ask),
                    (100., 99., 101., 98., 102.)
                );
                assert_eq!(
                    (
                        t.open_interest,
                        t.funding_rate,
                        t.volume,
                        t.quote_volume,
                        t.percentage
                    ),
                    (12., 0.0001, 20., 2000., 3.)
                );
                assert!(engine.take_frame(ISSUE + 4.).is_some());
                no_command(&engine.drain_actions().collect::<Vec<_>>());
            }
        }
    }
}
#[test]
fn native_partial_rows_keep_newer_websocket_symbol_only() {
    let (mut engine, poll, socket) = boot(false, false);
    let (token, _) = prep(&fire(&mut engine, ISSUE, poll));
    let request = release(&mut engine, ISSUE + 1., token);
    engine.handle(ISSUE + 2., Event::WsOpened { socket });
    engine.drain_actions().for_each(drop);
    engine.handle(
        ISSUE + 2.,
        Event::WsMessage {
            socket,
            text: r#"{"channel":"ticker:1","ticker":{"s":"ETH","mark_price":"123"}}"#,
        },
    );
    engine.take_frame(ISSUE + 2.);
    engine.drain_actions().for_each(drop);
    reply_at(
        &mut engine,
        ISSUE + 3.,
        &request,
        200,
        &format!(r#"{{"code":200,"tickers":[{ROW},{{"market_id":2,"mark_price":"200"}},{{}}]}}"#),
    );
    assert_eq!(engine.store().ticker("ETH").unwrap().mark, 123.);
    assert_eq!(engine.store().ticker("BTC").unwrap().mark, 200.);
    no_command(&engine.drain_actions().collect::<Vec<_>>());
}
fn retry(engine: &mut Engine<Lighter>, actions: &[Action]) -> Preparation {
    no_command(actions);
    let (t, at) = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("native startup backoff remains scheduled");
    prep(&fire(engine, at, t)).0
}
#[test]
fn failed_prefixes_and_typed_failures_discard_rows_and_recover() {
    for failure in 0..5 {
        let (mut engine, poll, _) = boot(false, false);
        let (token, unsigned) = prep(&fire(&mut engine, ISSUE, poll));
        if failure == 0 {
            engine.http_preparation_failed(
                ISSUE + 1.,
                token,
                Error::State,
                VenueError::new("route denied"),
            );
        } else {
            let request = release(&mut engine, ISSUE + 1., token);
            match failure {
                1 => reply_at(&mut engine, ISSUE + 2., &request, 500, "{}"),
                2 => reply_at(
                    &mut engine,
                    ISSUE + 2.,
                    &request,
                    200,
                    r#"{"code":200,"wrong":[]}"#,
                ),
                3 => {
                    engine.handle(
                        ISSUE + 2.,
                        Event::HttpStart {
                            request: request.id,
                            status: 200,
                            headers: "",
                        },
                    );
                    engine.handle(
                        ISSUE + 2.,
                        Event::HttpChunk {
                            request: request.id,
                            data: format!(r#"{{"code":200,"tickers":[{ROW},"#).as_bytes(),
                        },
                    );
                    assert_eq!(engine.store().ticker("ETH").unwrap().mark, 0.);
                    engine.handle(
                        ISSUE + 2.,
                        Event::HttpEnd {
                            request: request.id,
                        },
                    );
                }
                _ => engine.handle(
                    ISSUE + 2.,
                    Event::HttpFailed {
                        request: request.id,
                        error: "network lost",
                    },
                ),
            }
        }
        assert_eq!(engine.store().ticker("ETH").unwrap().mark, 0.);
        assert!(engine.take_frame(ISSUE + 2.).is_none());
        let actions: Vec<_> = engine.drain_actions().collect();
        let next = retry(&mut engine, &actions);
        assert_ne!(token, next);
        let request = release(&mut engine, ISSUE + 5_000., next);
        reply_at(&mut engine, ISSUE + 5_001., &request, 200, BODY);
        assert_eq!(engine.store().ticker("ETH").unwrap().mark, 100.);
        wire(&unsigned, false, false);
        no_command(&engine.drain_actions().collect::<Vec<_>>());
    }
}
#[test]
fn parked_and_released_requests_share_original_deadline() {
    for released in [false, true] {
        let (mut engine, poll, _) = boot(false, false);
        let actions = fire(&mut engine, ISSUE, poll);
        let (token, unsigned) = prep(&actions);
        let deadline = timer(&actions, ISSUE + 20_000.);
        if released {
            release(&mut engine, ISSUE + 19_000., token);
        }
        let actions = fire(&mut engine, ISSUE + 20_000., deadline);
        assert_eq!(engine.store().ticker("ETH").unwrap().mark, 0.);
        assert!(engine.take_frame(ISSUE + 20_000.).is_none());
        let next = retry(&mut engine, &actions);
        assert_ne!(token, next);
        reply_at(&mut engine, ISSUE + 21_001., &unsigned, 200, BODY);
        no_command(&engine.drain_actions().collect::<Vec<_>>());
        assert_eq!(engine.store().ticker("ETH").unwrap().mark, 0.);
        let request = release(&mut engine, ISSUE + 22_000., next);
        reply_at(&mut engine, ISSUE + 22_001., &request, 200, BODY);
        assert_eq!(engine.store().ticker("ETH").unwrap().mark, 100.);
    }
}
fn candle(engine: &mut Engine<Lighter>, now: f64, owner: u32) -> Preparation {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "ETH".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
    engine
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.contains("/candles?") => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
#[test]
fn capacity_refusal_never_strands_ticker_or_retires_candle_peer() {
    let (mut engine, poll, _) = boot(false, false);
    let peers: Vec<_> = (1..=256)
        .map(|owner| candle(&mut engine, NOW, owner))
        .collect();
    let actions = fire(&mut engine, ISSUE, poll);
    no_command(&actions);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "full owner capacity refuses ticker before transport allocation"
    );
    engine.http_preparation_failed(
        ISSUE + 1.,
        peers[0],
        Error::State,
        VenueError::new("free peer"),
    );
    let actions_peer: Vec<_> = engine.drain_actions().collect();
    assert!(
        actions_peer
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
    );
    let retry_action: Vec<_> = actions
        .into_iter()
        .filter(|a| !matches!(a,Action::SetTimer{at_ms,..} if *at_ms==ISSUE+2_000.))
        .collect();
    let token = retry(&mut engine, &retry_action);
    let request = release(&mut engine, ISSUE + 5_000., token);
    reply_at(&mut engine, ISSUE + 5_001., &request, 200, BODY);
    no_command(&engine.drain_actions().collect::<Vec<_>>());
    engine.http_preparation_failed(
        ISSUE + 5_002.,
        peers[1],
        Error::State,
        VenueError::new("other peer"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:2,error})if error.message=="other peer")));
}
#[test]
fn successful_background_cycles_retire_every_owner() {
    let (mut engine, mut poll, _) = boot(false, false);
    let mut now = ISSUE;
    for _ in 0..300 {
        let actions = fire(&mut engine, now, poll);
        let (token, _) = prep(&actions);
        poll = timer(&actions, now + 2_000.);
        let request = release(&mut engine, now + 1., token);
        reply_at(&mut engine, now + 2., &request, 200, BODY);
        no_command(&engine.drain_actions().collect::<Vec<_>>());
        now += 2_000.;
    }
    for owner in 1..=256 {
        candle(&mut engine, now, owner);
    }
}
