//! Scripted staging account; markPrice schema:
//! https://docs.woox.io/#get-all-position-info-new. No network transport is used.
mod support;
use safe_cex_engine::model::Ticker;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, TimerId};
use safe_cex_venue_woo::{Options, Woo};
use support::jittered;
const CATALOGUE: &str = r#"{"success":true,"rows":[{"symbol":"PERP_JTO_USDT","base_tick":0.1,"quote_tick":0.0001,"base_min":0.1,"base_max":1000,"base_imr":0.1},{"symbol":"PERP_W_USDT","base_tick":0.1,"quote_tick":0.0001,"base_min":0.1,"base_max":1000,"base_imr":0.1}]}"#;
const INFO: &str = r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"ONE_WAY","freeCollateral":900,"totalCollateral":1000,"totalAccountValue":1000}}"#;
const NATIVE: &str = r#"{"success":true,"data":{"positions":[{"symbol":"PERP_JTO_USDT","holding":20,"averageOpenPrice":2.0771,"markPrice":2.07460399,"pnl24H":0,"estLiqPrice":1.92748111,"leverage":10,"marginMode":"ISOLATED"},{"symbol":"PERP_W_USDT","holding":-50,"averageOpenPrice":0.1906,"markPrice":0.2103828568089886,"pnl24H":0,"estLiqPrice":4.542491646808989,"leverage":20,"marginMode":"CROSS"}]}}"#;
const UNPRICED: &str = r#"{"success":true,"data":{"positions":[{"symbol":"PERP_JTO_USDT","holding":20,"averageOpenPrice":2.0771,"leverage":10,"marginMode":"ISOLATED"}]}}"#;
const TICKERS: &str = r#"{"success":true,"rows":[{"symbol":"PERP_JTO_USDT","24h_open":2,"24h_close":2.1,"mark_price":2.1,"index_price":2.1,"24h_volume":1,"24h_amount":1,"last_funding_rate":0,"next_funding_time":0,"open_interest":1},{"symbol":"PERP_W_USDT","24h_open":0.2,"24h_close":0.21,"mark_price":0.21,"index_price":0.21,"24h_volume":1,"24h_amount":1,"last_funding_rate":0,"next_funding_time":0,"open_interest":1}]}"#;
fn reply(e: &mut Engine<Woo>, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn request(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing scripted request {path}"))
}
fn pending() -> (Engine<Woo>, HttpRequest, TimerId, TimerId) {
    pending_with(CATALOGUE)
}
fn pending_with(catalogue: &str) -> (Engine<Woo>, HttpRequest, TimerId, TimerId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        testnet: true,
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        application_id: Some("scripted-app".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let catalogue_request = request(&support::prepared_seed_actions(&mut e, 0.), "/public/info");
    reply(&mut e, &catalogue_request, 0.0, catalogue);
    let early = support::prepared_seed_actions(&mut e, 0.);
    let info = request(&early, "/v3/accountinfo");
    let ticker = request(&early, "/public/futures");
    let poll = early
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 2000.0,
            } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        0.0,
        Event::HttpFailed {
            request: ticker.id,
            error: "scripted public outage",
        },
    );
    let failed = e.drain_actions().collect::<Vec<_>>();
    assert!(failed.iter().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Warn,
            ..
        }))
    )));
    assert!(!failed.iter().any(|action| matches!(
        action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
            level: safe_cex_engine::Level::Error,
            ..
        }))
    )));
    let retry = failed
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if jittered(at_ms, 1000.0) => Some(timer),
            _ => None,
        })
        .unwrap();
    reply(&mut e, &info, 0.0, INFO);
    let positions = request(&e.drain_actions().collect::<Vec<_>>(), "/v3/positions");
    assert!(!e.store().loaded().tickers);
    (e, positions, retry, poll)
}
#[test]
fn funded_native_positions_remain_visible_when_public_tickers_fail() {
    let (mut e, positions, _, _) = pending();
    reply(&mut e, &positions, 0.0, NATIVE);
    assert!(!e.store().loaded().tickers);
    assert!(e.store().loaded().positions);
    let long = e.store().position("JTOUSDT").unwrap();
    assert_eq!(long.contracts, 20.0);
    assert!((long.notional - 41.4920798).abs() < 1e-9);
    assert_eq!(long.unrealized_pnl, -0.05);
    let short = e.store().position("WUSDT").unwrap();
    assert_eq!(short.contracts, 50.0);
    assert!((short.notional - 10.51914284044943).abs() < 1e-9);
    assert_eq!(short.unrealized_pnl, -0.99);
    assert_eq!(e.store().balance().upnl, -1.04);
    assert_eq!(e.store().balance().total, 1001.04);
}
#[test]
fn native_position_mark_takes_precedence_over_a_stale_public_mark() {
    let (mut e, positions, _, _) = pending();
    e.store_mut().set_tickers(vec![Ticker {
        id: "JTOUSDT".into(),
        symbol: "JTOUSDT".into(),
        mark: 1.0,
        last: 1.0,
        ..Default::default()
    }]);
    reply(&mut e, &positions, 0.0, NATIVE);
    assert!((e.store().position("JTOUSDT").unwrap().notional - 41.4920798).abs() < 1e-9);
}
#[test]
fn unpriced_native_holding_is_an_error_and_never_an_empty_success() {
    let (mut e, positions, _, _) = pending();
    reply(&mut e, &positions, 0.0, UNPRICED);
    assert!(!e.store().loaded().positions);
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("position mark unavailable"))));
}
#[test]
fn an_unpriced_refresh_preserves_previous_positions_and_account_valuation() {
    let (mut e, positions, _, poll) = pending();
    reply(&mut e, &positions, 0.0, NATIVE);
    let before = e.store().positions().to_vec();
    let balance = e.store().balance().clone();
    e.drain_actions().for_each(drop);
    e.handle(2000.0, Event::TimerFired { timer: poll });
    let positions = request(&e.drain_actions().collect::<Vec<_>>(), "/v3/positions");
    reply(&mut e, &positions, 2000.0, UNPRICED);
    assert_eq!(e.store().positions(), before);
    assert_eq!(*e.store().balance(), balance);
}
#[test]
fn ticker_recovery_immediately_reconciles_a_previously_unpriced_position() {
    let (mut e, positions, retry, _) = pending();
    reply(&mut e, &positions, 0.0, UNPRICED);
    e.drain_actions().for_each(drop);
    e.handle(1000.0, Event::TimerFired { timer: retry });
    let ticker = request(
        &support::prepared_seed_actions(&mut e, 1000.),
        "/public/futures",
    );
    reply(&mut e, &ticker, 1000.0, TICKERS);
    let positions = request(&e.drain_actions().collect::<Vec<_>>(), "/v3/positions");
    reply(&mut e, &positions, 1000.0, UNPRICED);
    assert!(e.store().loaded().positions);
    let p = e.store().position("JTOUSDT").unwrap();
    assert_eq!(
        (p.contracts, p.notional, p.unrealized_pnl),
        (20.0, 42.0, 0.46)
    );
}

#[test]
fn owned_position_outside_partial_catalogue_cannot_clear_a_snapshot() {
    // The catalogue skipped PERP_MISSING_USDT, so the holding may belong to it.
    let catalogue = CATALOGUE.replace("]}", r#",{"symbol":"PERP_MISSING_USDT"}]}"#);
    for previous in [false, true] {
        let (mut e, positions, _, poll) = pending_with(&catalogue);
        let request = if previous {
            reply(&mut e, &positions, 0.0, NATIVE);
            e.drain_actions().for_each(drop);
            e.handle(2000.0, Event::TimerFired { timer: poll });
            request(&e.drain_actions().collect::<Vec<_>>(), "/v3/positions")
        } else {
            positions
        };
        let before = e.store().positions().to_vec();
        let balance = e.store().balance().clone();
        reply(
            &mut e,
            &request,
            2000.0,
            &NATIVE.replace("PERP_JTO_USDT", "PERP_MISSING_USDT"),
        );
        assert_eq!(e.store().positions(), before);
        assert_eq!(*e.store().balance(), balance);
        assert_eq!(e.store().loaded().positions, previous);
        assert!(e.drain_actions().any(|action| matches!(action,
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("owned position market unavailable"))));
    }
}

#[test]
fn a_refused_key_ends_the_start_with_the_reason() {
    let (mut e, info, _, _) = pending_with(CATALOGUE);
    e.handle(
        1.0,
        Event::HttpResponse {
            request: info.id,
            response: Response {
                status: 401,
                headers: "",
                body: r#"{"success":false,"code":-1001,"message":"bad key"}"#,
            },
        },
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::StartupFailed { .. })
    )));
}

#[test]
fn account_http_failures_keep_kind_status_and_code_without_response_bodies() {
    use safe_cex_engine::{ErrorKind, Notice};
    for (status, code, expected, auth) in [
        (404, Some(-1006), ErrorKind::Unsupported, false),
        (429, Some(-1003), ErrorKind::RateLimited, false),
        (503, Some(-1000), ErrorKind::VenueUnavailable, false),
        (500, Some(-1002), ErrorKind::VenueUnavailable, false),
        (408, None, ErrorKind::Timeout, false),
        (400, Some(-1005), ErrorKind::Rejected, false),
        (200, Some(-1005), ErrorKind::Rejected, false),
        (401, Some(-1001), ErrorKind::AuthInvalid, true),
        (401, Some(-1002), ErrorKind::AuthInvalid, true),
        (401, None, ErrorKind::AuthInvalid, true),
        (403, None, ErrorKind::AuthPermission, true),
        (400, Some(-1001), ErrorKind::AuthInvalid, true),
        (200, Some(-1002), ErrorKind::AuthInvalid, true),
        (404, Some(-1002), ErrorKind::Unsupported, false),
        (429, Some(-1002), ErrorKind::RateLimited, false),
        (200, None, ErrorKind::Internal, false),
    ] {
        let (mut engine, info, _, _) = pending_with(CATALOGUE);
        let loaded_before = engine.store().loaded();
        let balance_before = engine.store().balance().clone();
        let body = serde_json::json!({
            "success": false,
            "code": code,
            "message": "private-response-marker",
        })
        .to_string();
        engine.handle(
            1.0,
            Event::HttpResponse {
                request: info.id,
                response: Response {
                    status,
                    headers: "",
                    body: &body,
                },
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        let event = actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::Event(event))
                    if event.message.starts_with("WOO ")
                        && event.fields.iter().any(|(key, _)| *key == "kind") =>
                {
                    Some(event)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("typed account failure for HTTP {status}, code {code:?}"));
        assert!(
            event.fields.iter().any(|(key, value)| *key == "kind"
                && value == &safe_cex_engine::log::Value::from(expected.as_str())),
            "HTTP {status}, code {code:?}: {event:?}"
        );
        assert!(event.message.contains(&format!("HTTP {status}")));
        assert_eq!(
            event.code == safe_cex_engine::log::code::AUTH_REJECTED,
            auth
        );
        assert_eq!(
            event.fields.iter().any(|(key, _)| *key == "venueCode"),
            code.is_some()
        );
        if let Some(code) = code {
            assert!(event.message.contains(&format!("WOO code {code}")));
            assert!(event.fields.iter().any(|(key, value)| *key == "venueCode"
                && value == &safe_cex_engine::log::Value::from(code.to_string())));
        }
        assert!(!event.message.contains("private-response-marker"));
        assert!(!format!("{:?}", event.fields).contains("private-response-marker"));
        assert_eq!(
            event.message.contains("refused the API key"),
            expected == ErrorKind::AuthInvalid
        );
        assert_eq!(
            actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::StartupFailed { error, .. }) if error.kind == Some(expected))),
            auth
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Notice(Notice::Deactivate)))
        );
        assert_eq!(engine.store().loaded(), loaded_before);
        assert_eq!(*engine.store().balance(), balance_before);
        assert!(engine.store().orders().is_empty());
    }
}

#[test]
fn account_network_failure_is_typed_without_echoing_transport_text() {
    use safe_cex_engine::Notice;
    let (mut engine, info, _, _) = pending_with(CATALOGUE);
    engine.handle(
        1.0,
        Event::HttpFailed {
            request: info.id,
            error: "private-transport-marker",
        },
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    let event = actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(Notice::Event(event)) if event.message.starts_with("WOO ") => {
                Some(event)
            }
            _ => None,
        })
        .expect("account-owned network failure");
    assert!(event.fields.iter().any(
        |(key, value)| *key == "kind" && value == &safe_cex_engine::log::Value::from("network")
    ));
    assert_eq!(event.code, safe_cex_engine::log::code::REQUEST_FAILED);
    assert!(!event.message.contains("private-transport-marker"));
    assert!(!event.message.contains("API key"));
    assert!(!event.fields.iter().any(|(key, _)| *key == "venueCode"));
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::StartupFailed { .. }) | Action::Notice(Notice::Deactivate)
    )));
}

#[test]
fn malformed_account_response_keeps_status_without_echoing_the_body() {
    use safe_cex_engine::{ErrorKind, Notice};
    for (status, kind) in [
        (200, ErrorKind::Internal),
        (502, ErrorKind::VenueUnavailable),
    ] {
        let (mut engine, request, _, _) = pending_with(CATALOGUE);
        let before = engine.store().positions().to_vec();
        engine.handle(
            1.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body: "<html>private-response-marker</html>",
                },
            },
        );
        let actions: Vec<_> = engine.drain_actions().collect();
        let event = actions
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::Event(event)) if event.message.starts_with("WOO ") => {
                    Some(event)
                }
                _ => None,
            })
            .expect("typed malformed response");
        assert!(event.message.contains(&format!("HTTP {status}")));
        assert!(!event.message.contains("private-response-marker"));
        assert!(event.fields.iter().any(|(key, value)| *key == "kind"
            && value == &safe_cex_engine::log::Value::from(kind.as_str())));
        assert!(!event.fields.iter().any(|(key, _)| *key == "venueCode"));
        assert_eq!(engine.store().positions(), before);
        assert!(!actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::StartupFailed { .. }) | Action::Notice(Notice::Deactivate)
        )));
    }
}
