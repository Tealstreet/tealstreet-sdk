//! Native single-page trades through real managed preparation and Read authority.
mod support;

use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Outcome, Query,
    Response, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex};
use support::{Harness, MARKETS, TICKERS};

const NOW: f64 = 1_700_000_000_000.;
const SYMBOL: &str = "BTC /&-USD-PERP";
const ROW: &str = r#"{"id":"trade-a","market":"BTC /&-USD-PERP","side":"BUY","price":"30000.12","size":"0.5","created_at":1681375176910}"#;

fn engine(testnet: bool, private: bool) -> Engine<Paradex> {
    let mut h = Harness::empty(Options {
        testnet,
        public_data_only: !private,
        // Existing delegated-signing fixture identifiers; no signing is needed here.
        key: private.then(|| "opaque-public-subkey".into()),
        paradex_address: private.then(|| "0x123".into()),
        subkey_address: private.then(|| "0x456".into()),
        payload_signer: private,
    });
    h.reply("/markets", &MARKETS.replace("BTC-USD-PERP", SYMBOL));
    h.reply("summary", &TICKERS.replace("BTC-USD-PERP", SYMBOL));
    h.e.take_frame(NOW);
    h.e
}

fn ask(
    e: &mut Engine<Paradex>,
    now: f64,
    command: u32,
    symbol: &str,
    limit: Option<u32>,
) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: symbol.into(),
                limit,
            },
        }),
    );
    e.drain_actions().collect()
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::SignPayload { .. } | Action::Sign { .. }
        )),
        "public history must await managed preparation without legacy HTTP or signing"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    assert!(request.signer_headers.is_empty());
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("public trades must use managed preparation")
}

fn release(e: &mut Engine<Paradex>, token: Preparation, now: f64) -> HttpRequest {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared public trade read must bind its exact request");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}

fn reply(
    e: &mut Engine<Paradex>,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}

fn results(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn only_retirement(actions: &[Action], token: Preparation) {
    for action in actions {
        match action {
            Action::Http(_) | Action::Sign { .. } | Action::SignPayload { .. } => {
                panic!("single public page cannot send or sign another request")
            }
            Action::ManagedHttp(action) => assert!(
                matches!(
                    action.as_ref(),
                    HttpAction::Cancel { preparation, signing: None } if *preparation == token
                ),
                "only this page's retirement is allowed"
            ),
            _ => {}
        }
    }
}

fn no_marker(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::Event(event)) if event.code == "history.incomplete"
    )));
}

#[test]
fn native_unsigned_query_clamp_and_schema_survive_managed_single_page_release() {
    for private in [false, true] {
        for testnet in [false, true] {
            for (limit, clamped) in [
                (None, 100),
                (Some(0), 1),
                (Some(1), 1),
                (Some(500), 500),
                (Some(900), 500),
            ] {
                let mut e = engine(testnet, private);
                let actions = ask(&mut e, NOW, 901, SYMBOL, limit);
                let (token, unsigned) = preparation(&actions);
                let host = if testnet { "testnet" } else { "prod" };
                assert_eq!(unsigned.method, Method::Get);
                assert_eq!(
                    unsigned.url,
                    format!(
                        "https://api.{host}.paradex.trade/v1/trades?market=BTC%20%2F%26-USD-PERP&page_size={clamped}"
                    )
                );
                assert!(unsigned.headers.is_empty());
                assert!(unsigned.body.is_none());
                assert!(results(&actions, 901).is_empty());
                let body = format!(r#"{{"next":"must-not-page","results":[{ROW}]}}"#);
                assert!(results(&reply(&mut e, NOW + 1., &unsigned, 200, &body), 901).is_empty());
                let request = release(&mut e, token, NOW + 2.);
                assert_eq!(request.url, unsigned.url);
                assert_eq!(request.headers, unsigned.headers);
                let actions = reply(&mut e, NOW + 3., &request, 200, &body);
                let done = results(&actions, 901);
                assert_eq!(done.len(), 1);
                let rows: Vec<serde_json::Value> =
                    serde_json::from_str(done[0].as_ref().unwrap()).unwrap();
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    rows[0],
                    serde_json::json!({"id":"trade-a","symbol":SYMBOL,"price":30000.12,"qty":0.5,"quoteQty":15000.06,"side":"buy","timestamp":1681375176910.0})
                );
                only_retirement(&actions, token);
                no_marker(&actions);
                assert!(e.store().orders().is_empty());
                assert!(e.store().positions().is_empty());
                assert!(e.take_frame(NOW + 3.).is_none());
                let duplicate = reply(&mut e, NOW + 4., &request, 200, &body);
                assert!(results(&duplicate, 901).is_empty());
                no_marker(&duplicate);
            }
        }
    }
}

#[test]
fn native_encounter_order_first_identity_and_truncate_are_preserved() {
    let mut e = engine(false, false);
    let (token, _) = preparation(&ask(&mut e, NOW, 902, SYMBOL, Some(2)));
    let request = release(&mut e, token, NOW + 1.);
    let older = ROW
        .replace("trade-a", "older")
        .replace("1681375176910", "1681375176000");
    let newer = ROW.replace("trade-a", "newer").replace("BUY", "SELL");
    let duplicate = older.replace("30000.12", "999.0");
    let foreign = ROW
        .replace(SYMBOL, "ETH-USD-PERP")
        .replace("trade-a", "newer");
    let tail = ROW.replace("trade-a", "tail");
    let body =
        format!(r#"{{"next":"ignored","results":[{older},{duplicate},{foreign},{newer},{tail}]}}"#);
    let actions = reply(&mut e, NOW + 2., &request, 200, &body);
    let done = results(&actions, 902);
    assert_eq!(done.len(), 1);
    let rows: Vec<serde_json::Value> = serde_json::from_str(done[0].as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "older");
    assert_eq!(rows[0]["price"], 30000.12);
    assert_eq!(rows[1]["id"], "newer");
    assert_eq!(rows[1]["side"], "sell");
    only_retirement(&actions, token);
    no_marker(&actions);
}

#[test]
fn invalid_native_page_never_returns_a_valid_prefix_or_marker() {
    let foreign_invalid = ROW
        .replace(SYMBOL, "ETH-USD-PERP")
        .replace("30000.12", "bad");
    for (status, body) in [
        (503, format!(r#"{{"results":[{ROW}]}}"#)),
        (200, format!(r#"{{"results":[{ROW},{{"id":"broken"}}]}}"#)),
        (200, format!(r#"{{"results":[{ROW},{foreign_invalid}]}}"#)),
        (200, format!(r#"{{"results":[{ROW}],"next":7}}"#)),
        (200, r#"{"results":null}"#.into()),
        (200, format!(r#"{{"results":[{ROW},"#)),
    ] {
        let mut e = engine(false, false);
        let (token, _) = preparation(&ask(&mut e, NOW, 903, SYMBOL, None));
        let request = release(&mut e, token, NOW + 1.);
        let actions = reply(&mut e, NOW + 2., &request, status, &body);
        let done = results(&actions, 903);
        assert_eq!(done.len(), 1);
        assert_eq!(
            done[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Internal)
        );
        assert_eq!(done[0].as_ref().unwrap_err().outcome, None);
        no_marker(&actions);
        assert!(e.store().orders().is_empty());
        assert!(e.take_frame(NOW + 2.).is_none());
    }
}

#[test]
fn typed_preparation_and_released_transport_failures_are_strict() {
    for prepared in [false, true] {
        let mut e = engine(false, false);
        let (token, _) = preparation(&ask(&mut e, NOW, 904, SYMBOL, None));
        if prepared {
            let request = release(&mut e, token, NOW + 1.);
            e.handle(
                NOW + 2.,
                Event::HttpFailed {
                    request: request.id,
                    error: "released loss",
                },
            );
        } else {
            e.http_preparation_failed(
                NOW + 1.,
                token,
                Error::State,
                VenueError::new("route denied").kind(ErrorKind::AuthPermission),
            );
        }
        let actions: Vec<_> = e.drain_actions().collect();
        let done = results(&actions, 904);
        assert_eq!(done.len(), 1);
        let error = done[0].as_ref().unwrap_err();
        assert_eq!(
            error.kind,
            Some(if prepared {
                ErrorKind::Network
            } else {
                ErrorKind::AuthPermission
            })
        );
        if !prepared {
            assert_eq!(error.outcome, Some(Outcome::NotSent));
            assert_eq!(error.message, "route denied");
        } else {
            assert_eq!(error.outcome, None);
        }
        no_marker(&actions);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::Notice(Notice::AccountAuth { .. })
        )));
    }
}

#[test]
fn deadline_and_id_reuse_deny_old_pages_and_preparation_tokens() {
    for claimed in [false, true] {
        let mut e = engine(false, false);
        let actions = ask(&mut e, NOW, 905, SYMBOL, None);
        let (token, unsigned) = preparation(&actions);
        let timer = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        if claimed {
            release(&mut e, token, NOW + 19_000.);
        }
        e.handle(NOW + 20_000., Event::TimerFired { timer });
        let expired: Vec<_> = e.drain_actions().collect();
        let failed = results(&expired, 905);
        assert_eq!(failed.len(), 1);
        assert_eq!(
            failed[0].as_ref().unwrap_err().kind,
            Some(ErrorKind::Timeout)
        );
        assert_eq!(
            failed[0].as_ref().unwrap_err().outcome,
            if claimed {
                None
            } else {
                Some(Outcome::NotSent)
            }
        );
        no_marker(&expired);
        let (next, _) = preparation(&ask(&mut e, NOW + 20_001., 905, SYMBOL, None));
        let body = format!(r#"{{"results":[{ROW}]}}"#);
        let stale = reply(&mut e, NOW + 20_002., &unsigned, 200, &body);
        assert!(results(&stale, 905).is_empty());
        no_marker(&stale);
        e.http_prepared(
            NOW + 20_002.,
            token,
            Prepared {
                id: unsigned.id.0,
                expires_at: None,
            },
        );
        let stale: Vec<_> = e.drain_actions().collect();
        assert!(!stale.iter().any(|a| matches!(a, Action::ManagedHttp(action) if matches!(action.as_ref(), HttpAction::Bind { .. }))));
        assert!(results(&stale, 905).is_empty());
        no_marker(&stale);
        let request = release(&mut e, next, NOW + 20_003.);
        let actions = reply(&mut e, NOW + 20_004., &request, 200, &body);
        assert!(matches!(results(&actions, 905).as_slice(), [Ok(_)]));
        no_marker(&actions);
    }
}

#[test]
fn initial_capacity_denial_also_precedes_missing_market_empty_completion() {
    let mut e = engine(false, false);
    let mut first = None;
    for command in 1..=256 {
        let (token, _) = preparation(&ask(&mut e, NOW, command, SYMBOL, None));
        first.get_or_insert(token);
    }
    for symbol in [SYMBOL, "UNKNOWN"] {
        let actions = ask(&mut e, NOW, 906, symbol, None);
        let failed = results(&actions, 906);
        assert_eq!(failed.len(), 1);
        let error = failed[0].as_ref().unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(error.message, "Engine request capacity reached");
        assert_eq!(error.outcome, None);
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::ManagedHttp(_)
                | Action::SetTimer { .. }
                | Action::Sign { .. }
                | Action::SignPayload { .. }
        )));
        no_marker(&actions);
    }
    e.http_preparation_failed(
        NOW + 1.,
        first.unwrap(),
        Error::State,
        VenueError::new("free one slot"),
    );
    e.drain_actions().for_each(drop);
    let actions = ask(&mut e, NOW + 2., 906, "UNKNOWN", None);
    assert_eq!(results(&actions, 906).as_slice(), [Ok("[]".into())]);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
    )));
    no_marker(&actions);
    preparation(&ask(&mut e, NOW + 3., 907, SYMBOL, None));
}

#[test]
fn incremental_valid_prefix_is_discarded_on_transport_loss_or_broken_end() {
    for transport in [false, true] {
        let mut e = engine(false, false);
        let (token, _) = preparation(&ask(&mut e, NOW, 908, SYMBOL, None));
        let request = release(&mut e, token, NOW + 1.);
        e.handle(
            NOW + 2.,
            Event::HttpStart {
                request: request.id,
                status: 200,
                headers: "",
            },
        );
        let prefix = format!(r#"{{"results":[{ROW},"#);
        e.handle(
            NOW + 3.,
            Event::HttpChunk {
                request: request.id,
                data: prefix.as_bytes(),
            },
        );
        let pending: Vec<_> = e.drain_actions().collect();
        assert!(results(&pending, 908).is_empty());
        no_marker(&pending);
        if transport {
            e.handle(
                NOW + 4.,
                Event::HttpFailed {
                    request: request.id,
                    error: "loss after valid prefix",
                },
            );
        } else {
            e.handle(
                NOW + 4.,
                Event::HttpEnd {
                    request: request.id,
                },
            );
        }
        let actions: Vec<_> = e.drain_actions().collect();
        let done = results(&actions, 908);
        assert_eq!(done.len(), 1);
        let error = done[0].as_ref().unwrap_err();
        assert_eq!(
            error.kind,
            Some(if transport {
                ErrorKind::Network
            } else {
                ErrorKind::Internal
            })
        );
        assert_eq!(error.outcome, None);
        no_marker(&actions);
        only_retirement(&actions, token);
        assert!(e.store().orders().is_empty());
        assert!(e.store().positions().is_empty());
        assert!(e.take_frame(NOW + 4.).is_none());
    }
}
