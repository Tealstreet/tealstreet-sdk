//! Explicit instrument discovery never replaces a good catalogue with partial data.
mod support;
use safe_cex_engine::model::{
    Balance, Interval, Order, OrderSide, OrderStatus, OrderType, Position, PositionSide,
};
use safe_cex_engine::{Command, VenueAction};
use serde_json::{Value, json};
use support::{Harness, INSTRUMENTS, PRELAUNCH};

fn refresh(h: &mut Harness, command: u32) {
    h.command(Command::Venue {
        command,
        action: VenueAction::RefreshMarkets,
    });
}
fn page(symbols: &[&str], cursor: &str, label: &str) -> String {
    let source: Value = serde_json::from_str(INSTRUMENTS).unwrap();
    let rows = symbols
        .iter()
        .map(|symbol| {
            let mut row = source["result"]["list"][0].clone();
            row["symbol"] = json!(symbol);
            row["baseCoin"] = json!(symbol.trim_end_matches("USDT"));
            row["symbolType"] = json!(label);
            row
        })
        .collect::<Vec<_>>();
    json!({"retCode":0,"retMsg":"OK","result":{"list":rows,"nextPageCursor":cursor}}).to_string()
}

#[test]
fn pages_commit_atomically_preserving_fees_financial_state_books_and_subscriptions() {
    let mut h = Harness::new();
    h.engine.store_mut().update_markets(|market| {
        if market.symbol == "BTCUSDT" {
            market.taker_fee = Some(0.001);
            market.maker_fee = Some(0.0002);
        }
    });
    h.engine
        .store_mut()
        .update_setting("BTCUSDT", |setting| setting.leverage = 7.0);
    h.engine.store_mut().set_balance(Balance {
        used: 1.,
        free: 2.,
        total: 3.,
        upnl: 5.,
    });
    h.engine.store_mut().set_positions(vec![Position {
        id: "p".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 100.,
        notional: 200.,
        leverage: 7.,
        margin_type: None,
        position_mode: None,
        unrealized_pnl: 5.,
        realized_pnl: None,
        contracts: 2.,
        liquidation_price: 0.,
        margin_size: None,
        last_update: Some(1.),
    }]);
    h.engine.store_mut().set_orders(vec![Order {
        id: "o".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 99.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 1.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }]);
    h.command(Command::SubscribeOrderBook {
        symbol: "BTCUSDT".into(),
    });
    h.message("/public/linear",r#"{"topic":"orderbook.1000.BTCUSDT","type":"snapshot","data":{"s":"BTCUSDT","b":[["100","1"]],"a":[["101","2"]],"u":1,"seq":1}}"#);
    h.command(Command::SubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::H1,
    });
    h.flush_subscriptions();
    h.sent.clear();
    let balance = h.engine.store().balance().clone();
    let positions = h.engine.store().positions().to_vec();
    let orders = h.engine.store().orders().to_vec();
    let previous = h
        .engine
        .store()
        .instrument_classifications()
        .unwrap()
        .to_vec();
    let last = h.engine.store().ticker("BTCUSDT").unwrap().last;
    let sockets = h.sockets.len();
    refresh(&mut h, 10);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond(
        "instruments-info",
        200,
        &page(&["BTCUSDT"], "next", "new-label"),
    );
    assert_eq!(
        h.engine.store().instrument_classifications().unwrap(),
        previous
    );
    assert!(h.engine.store().market("SOLUSDT").is_none());
    assert!(h.result(10).is_none());
    h.respond("cursor=next", 200, &page(&["SOLUSDT"], "", "new-label"));
    assert!(h.result(10).unwrap().is_ok());
    assert!(h.engine.store().market("ETHUSDT").is_none());
    assert!(h.engine.store().market("SOLUSDT").is_some());
    assert_eq!(
        h.engine.store().market("BTCUSDT").unwrap().taker_fee,
        Some(0.001)
    );
    assert_eq!(h.engine.store().market("SOLUSDT").unwrap().taker_fee, None);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 7.);
    assert_eq!(h.engine.store().balance(), &balance);
    assert_eq!(h.engine.store().positions(), positions);
    assert_eq!(h.engine.store().orders(), orders);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, last);
    assert!(h.engine.book("BTCUSDT").is_some());
    assert_eq!(h.sockets.len(), sockets);
    h.flush_subscriptions();
    assert!(
        h.sent
            .iter()
            .any(|(_, text)| text.contains("tickers.SOLUSDT"))
    );
    assert!(
        h.engine
            .store()
            .instrument_classifications()
            .unwrap()
            .iter()
            .all(|row| row.symbol_type.as_deref() == Some("new-label"))
    );
    h.command(Command::UnsubscribeCandles {
        symbol: "BTCUSDT".into(),
        interval: Interval::H1,
    });
    h.flush_subscriptions();
    assert!(
        h.sent
            .iter()
            .any(|(_, text)| text.contains("unsubscribe") && text.contains("kline.60.BTCUSDT"))
    );
    assert!(
        h.requests
            .iter()
            .all(|request| request.url.contains("instruments-info")),
        "refresh may not restart account/startup"
    );
}

#[test]
fn concurrent_waiters_share_one_walk_and_a_later_call_starts_another() {
    let mut h = Harness::new();
    refresh(&mut h, 11);
    let count = h.requests.len();
    refresh(&mut h, 12);
    assert_eq!(h.requests.len(), count);
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("instruments-info", 200, &page(&["BTCUSDT"], "", ""));
    assert!(h.result(11).unwrap().is_ok());
    assert!(h.result(12).unwrap().is_ok());
    refresh(&mut h, 13);
    assert_eq!(h.requests.len(), 2);
    assert!(h.result(13).is_none());
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond(
        "instruments-info",
        200,
        &page(&["BTCUSDT", "ETHUSDT"], "", ""),
    );
    assert!(h.result(13).unwrap().is_ok());
    assert!(h.engine.store().market("ETHUSDT").is_some());
}

#[test]
fn malformed_optional_pages_and_cursor_loops_preserve_catalogue_and_staged_metadata() {
    for failure in ["bad-envelope", "optional", "cursor"] {
        let mut h = Harness::new();
        let before = h.engine.store().markets().to_vec();
        let labels = h
            .engine
            .store()
            .instrument_classifications()
            .unwrap()
            .to_vec();
        let auth = h.engine.health().auth_failures;
        refresh(&mut h, 14);
        if failure == "optional" {
            h.respond(
                "instruments-info",
                200,
                &page(&["BTCUSDT", "SOLUSDT"], "", "changed"),
            );
            h.respond(
                "status=PreLaunch",
                403,
                r#"{"retCode":10003,"retMsg":"invalid key"}"#,
            );
        } else {
            h.respond("status=PreLaunch", 200, PRELAUNCH);
            h.respond(
                "instruments-info",
                200,
                &page(&["BTCUSDT"], "repeat", "changed"),
            );
            if failure == "cursor" {
                h.respond(
                    "cursor=repeat",
                    200,
                    &page(&["SOLUSDT"], "repeat", "changed"),
                );
            } else {
                h.respond("cursor=repeat", 200, r#"{"retCode":0,"result":{}}"#);
            }
        }
        assert!(h.result(14).unwrap().is_err());
        assert_eq!(h.engine.store().markets(), before);
        assert_eq!(
            h.engine.store().instrument_classifications().unwrap(),
            labels
        );
        assert_eq!(h.engine.health().auth_failures, auth);
        assert!(!h.deactivated);
    }
}

#[test]
fn timeout_and_late_old_generation_cannot_finish_or_pollute_a_new_walk() {
    let mut h = Harness::new();
    refresh(&mut h, 15);
    let old = std::mem::take(&mut h.requests);
    h.advance(30_001.);
    assert!(h.result(15).unwrap().is_err());
    refresh(&mut h, 16);
    for request in old {
        h.reply(request.id, 200, &page(&["STALEUSDT"], "", "stale"));
    }
    assert!(h.result(16).is_none());
    assert!(h.engine.store().market("STALEUSDT").is_none());
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond(
        "instruments-info",
        200,
        &page(&["BTCUSDT", "SOLUSDT"], "", "fresh"),
    );
    assert!(h.result(16).unwrap().is_ok());
    assert!(h.engine.store().market("STALEUSDT").is_none());
}

#[test]
fn joined_owners_time_out_once_each_and_pages_keep_the_broker_header() {
    let mut h = Harness::new();
    let before = h.requests.len();
    refresh(&mut h, 21);
    let pages = h.requests.len() - before;
    assert!(pages > 0);
    for request in &h.requests[before..] {
        assert_eq!(
            Harness::header(request, "X-Referer").as_deref(),
            Some("Tealstreet")
        );
    }
    h.advance(10_000.);
    refresh(&mut h, 22);
    assert_eq!(
        h.requests.len(),
        before + pages,
        "late owner joins the walk"
    );
    h.advance(20_001.);
    for command in [21, 22] {
        let terminals = h.done.iter().filter(|(c, _)| *c == command).count();
        assert_eq!(terminals, 1, "command {command} terminal count");
        assert!(h.result(command).unwrap().is_err());
        let kind = h.notices.iter().find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: c, error } if *c == command => {
                error.kind
            }
            _ => None,
        });
        assert_eq!(kind, Some(safe_cex_engine::ErrorKind::Timeout));
    }
    // The ended walk does not absorb a new caller.
    refresh(&mut h, 23);
    assert!(h.requests.len() > before + pages);
    assert!(h.result(23).is_none());
}

#[test]
fn a_join_refused_while_the_walk_is_live_fails_only_that_command() {
    let mut h = Harness::new();
    let before = h.requests.len();
    for command in 100..164 {
        refresh(&mut h, command);
    }
    let walk = h.requests.len();
    refresh(&mut h, 164);
    assert_eq!(h.requests.len(), walk, "no second physical walk");
    assert!(h.result(164).unwrap().is_err());
    h.respond("status=PreLaunch", 200, PRELAUNCH);
    h.respond("instruments-info", 200, &page(&["BTCUSDT"], "", ""));
    for command in 100..164 {
        assert!(h.result(command).unwrap().is_ok(), "command {command}");
    }
    assert!(walk > before);
}

#[test]
fn malformed_rows_and_transport_failures_fail_atomic_and_cooldown_blocks_rediscovery() {
    for failure in ["row", "transport", "rate"] {
        let mut h = Harness::new();
        let before = h.engine.store().markets().to_vec();
        refresh(&mut h, 30);
        let request = h.take("status=PreLaunch");
        match failure {
            "row" => h.reply(
                request.id,
                200,
                r#"{"retCode":0,"result":{"list":[{"symbol":"BAD"}]}}"#,
            ),
            "transport" => h.event(safe_cex_engine::Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            }),
            _ => h.reply(
                request.id,
                429,
                r#"{"retCode":10006,"retMsg":"Too many visits"}"#,
            ),
        }
        assert!(h.result(30).unwrap().is_err());
        assert_eq!(h.engine.store().markets(), before);
        if failure == "rate" {
            let requests = h.requests.len();
            refresh(&mut h, 31);
            assert!(h.result(31).unwrap().is_err());
            assert_eq!(h.requests.len(), requests);
        }
    }
}

#[test]
fn spot_refresh_has_no_pagination_parameters_and_hyro_keeps_filtering_and_deduplication() {
    for variant in [
        safe_cex_venue_bybit::Variant::Spot,
        safe_cex_venue_bybit::Variant::Hyrotrader,
    ] {
        let mut h = Harness::started(support::Options {
            variant,
            public_data_only: true,
            endpoints: variant.is_hyro().then(|| safe_cex_venue_bybit::Endpoints {
                rest: "https://hyro.example".into(),
                public_ws: "wss://hyro.example/public/linear".into(),
                private_ws: "wss://hyro.example/private".into(),
            }),
            ..Default::default()
        });
        if variant.is_hyro() {
            h.respond("status=PreLaunch", 200, PRELAUNCH);
        }
        h.respond("instruments-info", 200, INSTRUMENTS);
        h.respond("tickers", 200, support::TICKERS);
        let socket = h.sockets[0].0;
        h.event(safe_cex_engine::Event::WsOpened { socket });
        h.flush_subscriptions();
        refresh(&mut h, 32);
        if variant.is_spot() {
            assert_eq!(h.requests.len(), 1);
            let request = &h.requests[0];
            assert!(request.url.ends_with("instruments-info?category=spot"));
            h.respond(
                "instruments-info",
                200,
                &page(&["BTCUSDT", "SOLUSDT"], "ignored-spot-cursor", "spot"),
            );
            assert!(h.result(32).unwrap().is_ok());
            assert!(!h.has_request("cursor="));
        } else {
            let mut body: Value = serde_json::from_str(&page(
                &["BTCUSDT", "SOLUSDT", "OTHERUSDT", "BTCUSDC"],
                "",
                "hyro",
            ))
            .unwrap();
            body["result"]["list"][2]["contractType"] = json!("InversePerpetual");
            body["result"]["list"][3]["quoteCoin"] = json!("USDC");
            h.respond("instruments-info", 200, &body.to_string());
            let mut duplicate: Value =
                serde_json::from_str(&page(&["BTCUSDT"], "", "prelaunch")).unwrap();
            duplicate["result"]["list"][0]["status"] = json!("PreLaunch");
            h.respond("status=PreLaunch", 200, &duplicate.to_string());
            assert!(h.result(32).unwrap().is_ok());
            assert_eq!(
                h.engine
                    .store()
                    .markets()
                    .iter()
                    .map(|row| row.symbol.as_str())
                    .collect::<Vec<_>>(),
                ["BTCUSDT", "SOLUSDT"]
            );
            assert_eq!(
                h.engine.store().instrument_classifications().unwrap()[0]
                    .symbol_type
                    .as_deref(),
                Some("prelaunch")
            );
        }
    }
}

#[test]
fn a_shared_consumer_refuses_independent_discovery_before_any_refresh_io() {
    use safe_cex_engine::{Action, ErrorKind, Notice, PublicFeedPolicy};
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_bybit::BybitLinear::new(safe_cex_venue_bybit::Options {
            public_data_only: true,
            ..Default::default()
        }),
    );
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(1000.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        1001.,
        safe_cex_engine::Event::Command(Command::Venue {
            command: 50,
            action: VenueAction::RefreshMarkets,
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action|matches!(action,Action::Notice(Notice::CommandFailed{command:50,error}) if error.kind==Some(ErrorKind::Unsupported))));
}

mod envelope_rate_limits {
    use super::*;
    use safe_cex_engine::{ErrorKind, Event, Notice, Response, VenueError};

    fn reply(h: &mut Harness, path: &str, headers: &str, body: &str) {
        let request = h.take(path);
        h.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers,
                body,
            },
        });
    }

    fn limited_once(h: &Harness, command: u32) -> &VenueError {
        let mut failures = h.notices.iter().filter_map(|notice| match notice {
            Notice::CommandFailed { command: id, error } if *id == command => Some(error),
            _ => None,
        });
        let error = failures.next().expect("one rate-limited completion");
        assert!(failures.next().is_none(), "a waiter must fail only once");
        assert_eq!(error.kind, Some(ErrorKind::RateLimited));
        assert_eq!(h.done.iter().filter(|(id, _)| *id == command).count(), 1);
        error
    }

    /// An HTTP status is never a venue code: a non-JSON 403 (Bybit's IP ban)
    /// fails the refresh rate-limited with no `venue_code`.
    #[test]
    fn a_non_json_refresh_failure_carries_no_venue_code() {
        let mut h = Harness::new();
        refresh(&mut h, 70);
        // The prelaunch page stays pending; the trading page fails.
        let _prelaunch = h.take("status=PreLaunch");
        let request = h.take("instruments-info");
        h.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 403,
                headers: "",
                body: "<html>403 Forbidden</html>",
            },
        });
        let error = limited_once(&h, 70);
        assert_eq!(error.venue_code, None, "HTTP 403 is not a Bybit retCode");
    }

    #[test]
    fn refresh_envelope_limits_preserve_catalogue_and_wait_for_the_later_hint() {
        for with_list in [false, true] {
            for (reset_ms, retry_seconds, wait_ms) in [
                (Some(25_000.0), None, 25_000.0),
                (None, Some(45), 45_000.0),
                (Some(25_000.0), Some(45), 45_000.0),
                (Some(45_000.0), Some(25), 45_000.0),
            ] {
                let mut h = Harness::new();
                let before = h.engine.store().markets().to_vec();
                let labels = h
                    .engine
                    .store()
                    .instrument_classifications()
                    .unwrap()
                    .to_vec();
                let auth = h.engine.health().auth_failures;
                refresh(&mut h, 60);
                refresh(&mut h, 61);
                assert_eq!(h.requests.len(), 2, "coalesced waiters share one walk");
                // Retain a peer response to prove that a late completion cannot commit.
                let peer = h.take("status=PreLaunch");
                let mut body: Value = if with_list {
                    serde_json::from_str(&page(&["BTCUSDT", "SOLUSDT"], "", "staged")).unwrap()
                } else {
                    json!({"result":{}})
                };
                body["retCode"] = json!(10006);
                body["retMsg"] = json!("Too many visits!");
                let mut headers = String::new();
                if let Some(delay) = reset_ms {
                    headers.push_str(&format!(
                        "X-Bapi-Limit-Reset-Timestamp: {}\r\n",
                        h.now + delay
                    ));
                }
                if let Some(seconds) = retry_seconds {
                    headers.push_str(&format!("Retry-After: {seconds}\r\n"));
                }
                reply(&mut h, "instruments-info", &headers, &body.to_string());
                for command in [60, 61] {
                    assert_eq!(
                        limited_once(&h, command).venue_code.as_deref(),
                        Some("10006")
                    );
                }
                assert_eq!(h.engine.store().markets(), before);
                assert_eq!(
                    h.engine.store().instrument_classifications().unwrap(),
                    labels
                );
                assert!(h.requests.is_empty(), "failed refresh must not retry");
                refresh(&mut h, 62);
                assert_eq!(limited_once(&h, 62).retry_after_ms, Some(wait_ms));
                assert!(
                    h.requests.is_empty(),
                    "cooldown must refuse a new walk before I/O"
                );
                h.reply(peer.id, 200, PRELAUNCH);
                assert_eq!(h.engine.store().markets(), before);
                assert_eq!(
                    h.engine.store().instrument_classifications().unwrap(),
                    labels
                );
                limited_once(&h, 60);
                limited_once(&h, 61);
                h.advance(wait_ms - 1.0);
                assert!(h.requests.is_empty(), "cooldown may not schedule discovery");
                refresh(&mut h, 63);
                assert_eq!(limited_once(&h, 63).retry_after_ms, Some(1.0));
                assert!(h.requests.is_empty());
                assert_eq!(h.engine.store().markets(), before);
                h.advance(1.0);
                refresh(&mut h, 64);
                assert_eq!(
                    h.requests.len(),
                    2,
                    "refresh resumes at the cooldown boundary"
                );
                assert!(h.result(64).is_none());
                h.respond("status=PreLaunch", 200, PRELAUNCH);
                h.respond(
                    "instruments-info",
                    200,
                    &page(&["BTCUSDT", "SOLUSDT"], "", "fresh"),
                );
                assert!(h.result(64).unwrap().is_ok());
                assert!(h.engine.store().market("SOLUSDT").is_some());
                assert!(h.engine.store().market("ETHUSDT").is_none());
                limited_once(&h, 60);
                limited_once(&h, 61);
                assert_eq!(h.engine.health().auth_failures, auth);
                assert!(!h.deactivated);
            }
        }
    }

    #[test]
    fn successful_refresh_headers_do_not_poison_the_next_refresh() {
        let mut h = Harness::new();
        refresh(&mut h, 70);
        let headers = format!(
            "X-Bapi-Limit-Reset-Timestamp: {}\r\nRetry-After: 120\r\n",
            h.now + 120_000.0
        );
        reply(&mut h, "status=PreLaunch", &headers, PRELAUNCH);
        reply(
            &mut h,
            "instruments-info",
            &headers,
            &page(&["BTCUSDT"], "", "first"),
        );
        assert!(h.result(70).unwrap().is_ok());
        assert!(h.requests.is_empty());
        refresh(&mut h, 71);
        assert!(h.result(71).is_none());
        assert_eq!(
            h.requests.len(),
            2,
            "success headers are not rate-limit evidence"
        );
        h.respond("status=PreLaunch", 200, PRELAUNCH);
        h.respond(
            "instruments-info",
            200,
            &page(&["BTCUSDT", "SOLUSDT"], "", "next"),
        );
        assert!(h.result(71).unwrap().is_ok());
        assert!(h.engine.store().market("SOLUSDT").is_some());
    }
}

/// Slice 2 RED: the shared-consumer refusal is a typed NotSent terminal under
/// the command's own admitted authority.
#[test]
fn a_shared_consumer_refusal_is_typed_not_sent_under_command_authority() {
    use safe_cex_engine::{Action, ErrorKind, Notice, Outcome, PublicFeedPolicy};
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_bybit::BybitLinear::new(safe_cex_venue_bybit::Options {
            public_data_only: true,
            ..Default::default()
        }),
    );
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(1000.);
    engine.drain_actions().for_each(drop);
    engine.handle(
        1001.,
        safe_cex_engine::Event::Command(Command::Venue {
            command: 51,
            action: VenueAction::RefreshMarkets,
        }),
    );
    let failures: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: 51, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1, "one terminal");
    assert_eq!(failures[0].kind, Some(ErrorKind::Unsupported));
    assert_eq!(failures[0].outcome, Some(Outcome::NotSent));
}
