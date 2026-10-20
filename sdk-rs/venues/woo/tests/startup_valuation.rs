//! Scripted staging account; markPrice schema:
//! https://docs.woox.io/#get-all-position-info-new. No network transport is used.
use safe_cex_engine::model::Ticker;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, Response, TimerId};
use safe_cex_venue_woo::{Options, Woo};
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
    let mut e = Engine::new(Woo::new(Options {
        testnet: true,
        key: Some("scripted-key".into()),
        secret: Some("scripted-secret".into()),
        application_id: Some("scripted-app".into()),
        ..Default::default()
    }));
    e.start(0.0);
    let catalogue = request(&e.drain_actions().collect::<Vec<_>>(), "/public/info");
    reply(&mut e, &catalogue, 0.0, CATALOGUE);
    let early = e.drain_actions().collect::<Vec<_>>();
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
        Action::Notice(Notice::Log {
            level: safe_cex_engine::LogLevel::Warning,
            ..
        })
    )));
    assert!(!failed.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::Log {
            level: safe_cex_engine::LogLevel::Error,
            ..
        })
    )));
    let retry = failed
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1000.0,
            } => Some(timer),
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
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(Notice::Log { message, .. }) if message.contains("position mark unavailable"))));
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
    let ticker = request(&e.drain_actions().collect::<Vec<_>>(), "/public/futures");
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
    for previous in [false, true] {
        let (mut e, positions, _, poll) = pending();
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
            Action::Notice(Notice::Log { message, .. }) if message.contains("owned position market unavailable"))));
    }
}
