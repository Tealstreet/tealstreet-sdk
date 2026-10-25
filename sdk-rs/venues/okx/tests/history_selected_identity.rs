//! Native selection controls; the original RED is the removal-only inventory.
#![allow(dead_code)]
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Query, ReadCredentials, Response,
    VenueAction,
};
use safe_cex_venue_okx::Okx;
use support::FixtureOptions;

const NOW: f64 = 1_710_000_000_000.;
const OWNER: u32 = 883;
fn options(primary: bool, secondary: bool, public_data_only: bool) -> FixtureOptions {
    FixtureOptions {
        key: primary.then(|| "fixture-primary-key".into()),
        secret: primary.then(|| "fixture-primary-material".into()),
        passphrase: primary.then(|| "fixture-primary-pass".into()),
        read_credentials: secondary.then(|| ReadCredentials {
            key: "fixture-read-key".into(),
            secret: "fixture-read-material".into(),
            passphrase: Some("fixture-read-pass".into()),
        }),
        public_data_only,
        ..Default::default()
    }
}
fn physical(a: &[Action], path: &str) -> HttpRequest {
    let mut requests = a.iter().filter_map(|a| match a {
        Action::Http(r) if r.url.contains(path) => Some(r.clone()),
        _ => None,
    });
    let r = requests.next().expect("native request");
    assert!(requests.next().is_none());
    r
}
fn ready(opts: FixtureOptions) -> Engine<Okx> {
    let mut e = support::engine(opts);
    e.start(NOW - 1000.);
    let out = support::prepared(&mut e, NOW - 1000.);
    let r = physical(&out, "/public/instruments");
    reply(&mut e, &r, NOW - 900., &support::envelope(support::MARKET));
    let out = support::prepared(&mut e, NOW - 900.);
    let r = physical(&out, "/market/tickers");
    reply(&mut e, &r, NOW - 800., &support::envelope(support::TICKER));
    support::prepared(&mut e, NOW - 800.);
    e
}
fn reply(e: &mut Engine<Okx>, r: &HttpRequest, now: f64, body: &str) {
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
fn query(query: Query) -> Command {
    Command::Query {
        command: OWNER,
        query,
    }
}
fn fills() -> Command {
    query(Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from_ms: None,
        to_ms: None,
        limit: Some(1),
    })
}
fn orders() -> Command {
    query(Query::OrderHistory {
        symbol: Some("BTCUSDT".into()),
        from_ms: None,
        to_ms: None,
        limit: None,
    })
}
fn closed() -> Command {
    Command::Venue {
        command: OWNER,
        action: VenueAction::FetchClosedPositions {
            symbol: Some("BTCUSDT".into()),
            from_ms: None,
            to_ms: None,
        },
    }
}
fn send(e: &mut Engine<Okx>, command: Command, now: f64) -> Vec<Action> {
    e.handle(now, Event::Command(command));
    support::prepared(e, now)
}
fn key(r: &HttpRequest) -> &str {
    r.headers
        .iter()
        .find_map(|(name, value)| (*name == "OK-ACCESS-KEY").then_some(value.as_str()))
        .expect("signed key")
}
fn signed(r: &HttpRequest, secondary: bool) {
    assert_eq!(
        key(r),
        if secondary {
            "fixture-read-key"
        } else {
            "fixture-primary-key"
        }
    );
    assert_eq!(
        r.route, None,
        "prepared fixture transport carries no route override"
    );
    assert_eq!(r.timeout_ms, Some(20000));
    assert!(r.body.is_none());
}
#[test]
fn declared_route_intent_matches_the_selected_signed_identity() {
    use safe_cex_engine::request_context::{HttpAction, RoutingIntent};
    for secondary in [false, true] {
        for command in [fills(), closed()] {
            let mut e = ready(options(true, secondary, false));
            e.handle(NOW, Event::Command(command));
            let intents = e
                .drain_actions()
                .filter_map(|a| match a {
                    Action::ManagedHttp(action) => match *action {
                        HttpAction::Prepare { intent, .. } => Some(intent),
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                intents,
                [if secondary {
                    RoutingIntent::Secondary
                } else {
                    RoutingIntent::Primary
                }]
            );
        }
    }
}
#[test]
fn ordinary_history_and_closed_keep_secondary_else_primary() {
    for secondary in [false, true] {
        for (command, path) in [
            (fills(), "/trade/fills?"),
            (closed(), "/account/positions-history?"),
        ] {
            let mut e = ready(options(true, secondary, false));
            signed(&physical(&send(&mut e, command, NOW), path), secondary);
        }
    }
}
#[test]
fn secondary_only_and_filtered_primary_private_histories_remain_empty() {
    for opts in [options(false, true, false), options(true, true, true)] {
        for command in [fills(), closed()] {
            let mut e = ready(opts.clone());
            let a = send(&mut e, command, NOW);
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
            );
            assert_eq!(a.iter().filter(|a| matches!(a,Action::Notice(Notice::CommandDone { command: OWNER, result }) if result=="[]")).count(),1);
        }
    }
}
#[test]
fn keyless_invalid_private_range_keeps_empty_before_validation() {
    let mut e = ready(options(false, true, false));
    let a = send(
        &mut e,
        query(Query::Fills {
            symbol: Some("UNKNOWN".into()),
            order_id: None,
            from_ms: Some(2.),
            to_ms: Some(1.),
            limit: None,
        }),
        NOW,
    );
    assert_eq!(a.iter().filter(|a| matches!(a,Action::Notice(Notice::CommandDone { command: OWNER, result }) if result=="[]")).count(),1);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
}
#[test]
fn primary_client_lookup_never_uses_secondary_signing() {
    let mut e = ready(options(true, true, false));
    let a = send(
        &mut e,
        query(Query::OrderByClientId {
            symbol: "BTCUSDT".into(),
            client_order_id: "caller1".into(),
        }),
        NOW,
    );
    signed(&physical(&a, "/trade/order?"), false);
}
#[test]
fn secondary_only_disables_cached_primary_lookup_before_credential_refusal() {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType};
    for secondary in [false, true] {
        let mut e = ready(options(false, secondary, false));
        e.store_mut().set_orders(vec![Order {
            id: "cached".into(),
            parent_id: None,
            status: OrderStatus::Closed,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 100.,
            amount: 2.,
            filled: 2.,
            remaining: 0.,
            reduce_only: false,
            close: None,
            client_order_id: Some("cached1".into()),
            time_in_force: None,
            last_update: 123.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        }]);
        let before = e.account_snapshot_frame(NOW).to_vec();
        let a = send(
            &mut e,
            query(Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "cached1".into(),
            }),
            NOW,
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        if secondary {
            assert_eq!(a.iter().filter(|a| matches!(a,Action::Notice(Notice::CommandFailed { command: OWNER, error }) if error.message=="OKX client order lookup requires credentials")).count(),1);
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
            );
        } else {
            let result = a
                .iter()
                .find_map(|a| match a {
                    Action::Notice(Notice::CommandDone {
                        command: OWNER,
                        result,
                    }) => Some(result),
                    _ => None,
                })
                .unwrap();
            let result: serde_json::Value = serde_json::from_str(result).unwrap();
            assert_eq!(result["found"], true);
            assert_eq!(result["order"]["id"], "cached");
        }
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn public_trades_stay_unsigned_with_demo_headers_even_with_secondary() {
    let mut opts = options(false, true, false);
    opts.testnet = true;
    let mut e = ready(opts);
    let r = physical(
        &send(
            &mut e,
            query(Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(2),
            }),
            NOW,
        ),
        "/market/trades?",
    );
    assert!(
        r.headers
            .iter()
            .all(|(name, _)| !name.starts_with("OK-ACCESS-"))
    );
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "x-simulated-trading" && value == "1")
    );
    assert!(r.body.is_none());
}
fn advance(e: &mut Engine<Okx>, at: f64) -> Vec<Action> {
    let a = support::prepared(e, at - 100.);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("native100ms page wait");
    e.handle(at, Event::TimerFired { timer });
    support::prepared(e, at)
}
#[test]
fn later_closed_page_keeps_original_secondary_selection_and_wire() {
    let mut e = ready(options(true, true, false));
    let r = physical(&send(&mut e, closed(), NOW), "/account/positions-history?");
    let rows = (0..100)
        .map(|i| {
            format!(
                r#"{{"uTime":"{}","posId":"{i}"}}"#,
                1_709_500_000_000_u64 - i
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    reply(&mut e, &r, NOW + 1., &support::envelope(&rows));
    let next = physical(&advance(&mut e, NOW + 101.), "/account/positions-history?");
    signed(&next, true);
    assert!(next.url.contains("after=1709499999902"));
    assert_ne!(r.id, next.id);
}
#[test]
fn later_order_page_keeps_original_secondary_selection_and_wire() {
    let mut e = ready(options(true, true, false));
    let r = physical(&send(&mut e, orders(), NOW), "/trade/orders-history?");
    let rows=(0..100).map(|i|format!(r#"{{"instId":"BTC-USDT-SWAP","ordId":"{}","side":"sell","ordType":"post_only","state":"mmp_canceled","sz":"3","accFillSz":"1.5","px":"41000","avgPx":"","reduceOnly":"true","posSide":"net","cTime":"1710000000123","uTime":"1710000000200"}}"#,100000-i)).collect::<Vec<_>>().join(",");
    reply(&mut e, &r, NOW + 1., &support::envelope(&rows));
    let next = physical(&advance(&mut e, NOW + 101.), "/trade/orders-history?");
    signed(&next, true);
    assert!(next.url.contains("after=99901"));
    assert_ne!(r.id, next.id);
}
