//! Timestamp recovery uses public venue time and never extends the read receipt.
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, Venue,
};
use safe_cex_venue_kucoin::Kucoin;
use support::credentials::Options;

const SKEW: &str = r#"{"code":"400002","msg":"Invalid KC-API-TIMESTAMP"}"#;
const TIME: &str = r#"{"code":"200000","data":50000}"#;

struct H<V: Venue> {
    e: Engine<V>,
    now: f64,
    actions: Vec<Action>,
}
impl<V: Venue> H<V> {
    fn new(make: impl FnOnce(safe_cex_venue_kucoin::Options) -> V) -> Self {
        Self {
            e: support::credentials::engine(
                Options {
                    key: Some("primary-key".into()),
                    secret: Some("primary-secret".into()),
                    passphrase: Some("primary-pass".into()),
                    read_credentials: Some(safe_cex_engine::ReadCredentials {
                        key: "read-key".into(),
                        secret: "read-secret".into(),
                        passphrase: Some("read-pass".into()),
                    }),
                    ..Default::default()
                },
                make,
            ),
            now: 1000.,
            actions: vec![],
        }
    }
    fn collect(&mut self) {
        self.actions
            .extend(safe_cex_engine::test_support::prepared_actions(
                &mut self.e,
                self.now,
            ));
    }
    fn ask(&mut self, wait: u32) {
        self.e
            .handle_command(self.now, Command::FetchAccount { command: 71 }, Some(wait))
            .unwrap();
        self.collect();
    }
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let i = self
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.url.contains(suffix)))
            .unwrap_or_else(|| panic!("missing {suffix}: {:?}", self.actions));
        let Action::Http(r) = self.actions.remove(i) else {
            unreachable!()
        };
        r
    }
    fn reply(&mut self, r: &HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn terminals(&self) -> usize {
        self.actions
            .iter()
            .filter(|a| {
                matches!(
                    a,
                    Action::Notice(
                        Notice::CommandDone { command: 71, .. }
                            | Notice::CommandFailed { command: 71, .. }
                    )
                )
            })
            .count()
    }
}
fn header<'a>(r: &'a HttpRequest, key: &str) -> &'a str {
    &r.headers.iter().find(|(k, _)| *k == key).unwrap().1
}
fn recovery<V: Venue>(make: impl FnOnce(safe_cex_venue_kucoin::Options) -> V, origin: &str) {
    let mut h = H::new(make);
    h.ask(20_000);
    let first = h.take("/user");
    h.reply(&first, 400, SKEW);
    assert_eq!(h.terminals(), 0);
    let clock = h.take("/api/v1/timestamp");
    assert_eq!(clock.url, format!("{origin}/api/v1/timestamp"));
    assert_eq!(clock.method, Method::Get);
    assert!(clock.headers.iter().all(|(k, _)| !k.starts_with("KC-")));
    assert_eq!(clock.route, None);
    h.now += 100.;
    h.reply(&clock, 200, TIME);
    let second = h.take("/user");
    assert_eq!(second.url, first.url);
    assert_eq!(second.body, first.body);
    assert_eq!(header(&second, "KC-API-KEY"), "read-key");
    assert_eq!(header(&second, "KC-API-TIMESTAMP"), "50000");
    let raw_path = second.url.strip_prefix(origin).unwrap();
    assert_eq!(
        header(&second, "KC-API-SIGN"),
        safe_cex_crypto::hmac_sha256(
            b"read-secret",
            format!("50000GET{raw_path}").as_bytes(),
            safe_cex_crypto::Encoding::Base64
        )
    );
    assert_eq!(second.route, first.route);
    h.reply(&second, 400, SKEW);
    assert_eq!(h.terminals(), 1);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}
#[test]
fn futures_recovers_once_with_the_original_selected_identity() {
    recovery(Kucoin::new, "https://api-futures.kucoin.com");
}
#[cfg(feature = "spot")]
#[test]
fn spot_recovers_once_with_the_original_selected_identity() {
    recovery(
        safe_cex_venue_kucoin::spot::KucoinSpot::new,
        "https://api.kucoin.com",
    );
}

fn slow_time_probe<V: Venue>(make: impl FnOnce(safe_cex_venue_kucoin::Options) -> V) {
    let mut h = H::new(make);
    h.ask(20_000);
    let first = h.take("/user");
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    assert_eq!(clock.timeout_ms, Some(20_000));
    h.now += 6_000.;
    h.reply(&clock, 200, TIME);
    let replay = h.take("/user");
    assert_eq!(replay.url, first.url);
    assert_eq!(header(&replay, "KC-API-KEY"), "read-key");
    assert_eq!(replay.timeout_ms, Some(14_000));
    h.reply(
        &replay,
        200,
        r#"{"code":"200000","data":{"userId":"user"}}"#,
    );
    assert_eq!(h.terminals(), 1);
    assert!(h.actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 71, .. })
    )));
    assert!(
        !h.actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
}

#[test]
fn futures_clock_recovery_waits_for_the_legacy_read_budget_without_renewing_the_root() {
    slow_time_probe(Kucoin::new);
}

#[cfg(feature = "spot")]
#[test]
fn spot_clock_recovery_waits_for_the_legacy_read_budget_without_renewing_the_root() {
    slow_time_probe(safe_cex_venue_kucoin::spot::KucoinSpot::new);
}

#[test]
fn background_order_books_can_recover_from_a_time_probe_slower_than_five_seconds() {
    for path in ["/api/v1/orders?", "/api/v1/stopOrders?"] {
        let mut h = futures_snapshot();
        let first = h.take(path);
        assert_eq!(first.timeout_ms, Some(20_000));
        h.actions.clear();
        h.reply(&first, 400, SKEW);
        let clock = h.take("/timestamp");
        assert_eq!(clock.timeout_ms, Some(20_000));
        assert!(clock.headers.iter().all(|(key, _)| !key.starts_with("KC-")));
        h.now += 6_000.;
        h.reply(&clock, 200, TIME);
        let replay = h.take(path);
        assert_eq!(replay.url, first.url);
        assert_eq!(header(&replay, "KC-API-KEY"), "read-key");
        h.reply(&replay, 400, SKEW);
        assert!(!h.e.store().loaded().orders);
        assert!(
            !h.actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}

#[test]
fn failed_order_book_time_probe_reports_the_original_read_without_restarting_recovery() {
    for path in ["/api/v1/orders?", "/api/v1/stopOrders?"] {
        let mut h = futures_snapshot();
        let first = h.take(path);
        h.actions.clear();
        h.reply(&first, 400, SKEW);
        let clock = h.take("/timestamp");
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: clock.id,
                error: "untrusted transport text",
            },
        );
        h.collect();
        let failures = h
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::REQUEST_FAILED =>
                {
                    Some(event)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 1);
        assert!(failures[0].message.contains(path));
        assert!(
            failures[0]
                .message
                .contains("source=transport signError=false HTTP=none")
        );
        assert!(!failures[0].message.contains("untrusted transport text"));
        assert!(
            !h.actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
    }
}

#[test]
fn expired_probe_cannot_calibrate_or_resend() {
    let mut h = H::new(Kucoin::new);
    h.ask(200);
    let first = h.take("/user");
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    assert_eq!(clock.timeout_ms, Some(200));
    h.now = 1200.;
    h.reply(&clock, 200, TIME);
    assert_eq!(h.terminals(), 1);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    h.now = 1300.;
    h.ask(200);
    let next = h.take("/user");
    assert_eq!(header(&next, "KC-API-TIMESTAMP"), "1300");
}

#[test]
fn invalid_time_never_changes_the_clock_or_resends() {
    for body in [
        "{}",
        r#"{"code":"200000","data":0}"#,
        r#"{"code":"200000","data":-1}"#,
        r#"{"code":"200000","data":1.5}"#,
        r#"{"code":"200000","data":9007199254740992}"#,
        r#"{"code":"400002","data":50000}"#,
    ] {
        let mut h = H::new(Kucoin::new);
        h.ask(20_000);
        let first = h.take("/user");
        h.reply(&first, 400, SKEW);
        let clock = h.take("/timestamp");
        h.reply(&clock, 200, body);
        assert_eq!(h.terminals(), 1, "{body}");
        assert!(
            !h.actions.iter().any(|a| matches!(a, Action::Http(_))),
            "{body}"
        );
    }
}

#[test]
fn successful_recovery_calibrates_only_this_engine_and_later_reads() {
    let mut h = H::new(Kucoin::new);
    h.ask(20_000);
    let first = h.take("/user");
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    h.reply(&clock, 200, TIME);
    let second = h.take("/user");
    h.reply(
        &second,
        200,
        r#"{"code":"200000","data":{"userId":"user"}}"#,
    );
    assert_eq!(h.terminals(), 1);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 71, .. })))
    );
    h.now += 10.;
    h.ask(20_000);
    assert_eq!(header(&h.take("/user"), "KC-API-TIMESTAMP"), "50010");
    let mut peer = H::new(Kucoin::new);
    peer.ask(20_000);
    assert_eq!(header(&peer.take("/user"), "KC-API-TIMESTAMP"), "1000");
}

#[test]
fn a_failed_or_hung_probe_terminates_the_read_once() {
    for status in [429, 500] {
        let mut h = H::new(Kucoin::new);
        h.ask(20_000);
        let first = h.take("/user");
        h.reply(&first, 400, SKEW);
        let clock = h.take("/timestamp");
        h.reply(&clock, status, TIME);
        assert_eq!(h.terminals(), 1);
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
    let mut h = H::new(Kucoin::new);
    h.ask(20_000);
    let first = h.take("/user");
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    assert_eq!(clock.timeout_ms, Some(20_000));
    let (timer, at) = h
        .actions
        .iter()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                Some((*timer, *at_ms))
            } else {
                None
            }
        })
        .find(|(_, at)| *at == 21_000.)
        .expect("original twenty second read deadline");
    h.now = at;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    assert_eq!(h.terminals(), 1);
    h.reply(&clock, 200, TIME);
    assert_eq!(h.terminals(), 1);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn a_private_futures_history_page_retains_its_query_and_completes_after_resync() {
    let mut h = H::new(Kucoin::new);
    h.e.handle_command(
        h.now,
        Command::Query {
            command: 71,
            query: safe_cex_engine::Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(1.),
                to_ms: Some(2.),
                limit: Some(10),
            },
        },
        Some(20_000),
    )
    .unwrap();
    h.collect();
    let first = h.take("/fills?");
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    h.reply(&clock, 200, TIME);
    let replay = h.take("/fills?");
    assert_eq!(first.url, replay.url);
    h.reply(
        &replay,
        200,
        r#"{"code":"200000","data":{"currentPage":1,"totalPage":1,"totalNum":0,"items":[]}}"#,
    );
    assert_eq!(h.terminals(), 1);
    assert!(h.actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command:71,result }) if result=="[]")
    ));
}

fn futures_snapshot() -> H<Kucoin> {
    let mut h = H::new(Kucoin::new);
    h.e.start(h.now);
    h.collect();
    let markets = h.take("/contracts/active");
    h.reply(&markets,200,r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"XBT","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":100000,"maxLeverage":100}]}"#);
    h
}

#[test]
fn background_snapshot_has_one_timestamp_recovery_and_preserves_the_store() {
    let mut h = futures_snapshot();
    let first = h.take("/positions");
    h.actions.clear();
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    h.reply(&clock, 200, TIME);
    let replay = h.take("/positions");
    assert_eq!(replay.url, first.url);
    h.reply(&replay, 400, SKEW);
    assert!(!h.e.store().loaded().positions);
    assert!(
        !h.actions.iter().any(|a| matches!(a, Action::Http(_))),
        "outer account retries must not restart recovery"
    );
}

#[test]
fn a_background_time_probe_failure_cannot_restart_account_retries_or_log_transport_text() {
    let mut h = futures_snapshot();
    let first = h.take("/positions");
    h.actions.clear();
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: clock.id,
            error: "secret-transport-text",
        },
    );
    h.collect();
    assert!(!h.e.store().loaded().positions);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!format!("{:?}", h.actions).contains("secret-transport-text"));
}

#[test]
fn managed_context_routes_private_secondary_then_public_probe_then_secondary_replay() {
    use safe_cex_engine::request_context::{HttpAction, Prepared, RoutingIntent};
    fn prepare(h: &mut H<Kucoin>, expected: RoutingIntent) {
        let mut seen = 0;
        h.e.prepare_dispatch(h.now);
        let mut actions = std::collections::VecDeque::from(h.e.drain_actions().collect::<Vec<_>>());
        while let Some(action) = actions.pop_front() {
            if let Action::ManagedHttp(action) = action {
                match *action {
                    HttpAction::Prepare {
                        preparation,
                        intent,
                        request,
                        ..
                    } => {
                        assert_eq!(intent, expected);
                        seen += 1;
                        h.e.http_prepared(
                            h.now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        assert!(matches!(
                            h.e.claim_http(h.now, permit),
                            Ok(safe_cex_engine::request_context::Claim::Released(_))
                        ));
                        h.actions.push(Action::Http(request));
                    }
                    HttpAction::Cancel { .. } => {}
                    _ => panic!("unexpected HTTP handoff"),
                }
            } else {
                h.actions.push(action);
            }
            actions.extend(h.e.drain_actions());
        }
        assert_eq!(seen, 1);
    }
    let mut h = H::new(Kucoin::new);
    h.e.handle_command(h.now, Command::FetchAccount { command: 71 }, Some(20_000))
        .unwrap();
    prepare(&mut h, RoutingIntent::Secondary);
    let first = h.take("/user");
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 400,
                headers: "",
                body: SKEW,
            },
        },
    );
    prepare(&mut h, RoutingIntent::Public);
    let clock = h.take("/timestamp");
    h.e.handle(
        h.now,
        Event::HttpResponse {
            request: clock.id,
            response: Response {
                status: 200,
                headers: "",
                body: TIME,
            },
        },
    );
    prepare(&mut h, RoutingIntent::Secondary);
    let _ = h.take("/user");
}

// Drive only short read retries, never unrelated socket or root deadlines.
fn finish_recovery_failure(h: &mut H<Kucoin>, path: &str, status: u16) {
    let first = h.take(path);
    h.actions.clear();
    h.reply(&first, 400, SKEW);
    let clock = h.take("/timestamp");
    h.reply(&clock, 200, TIME);
    let replay = h.take(path);
    h.reply(&replay, status, r#"{"code":"500000","msg":"Unavailable"}"#);
    for _ in 0..4 {
        if let Some(i) = h
            .actions
            .iter()
            .position(|a| matches!(a, Action::Http(r) if r.url.contains(path)))
        {
            let Action::Http(next) = h.actions.remove(i) else {
                unreachable!()
            };
            h.reply(&next, 400, SKEW);
        } else if let Some((timer, at)) = h
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms: at } if *at < 5000. => Some((*timer, *at)),
                _ => None,
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            h.actions
                .retain(|a| !matches!(a, Action::SetTimer { timer: t, .. } if *t == timer));
            h.now = at;
            h.e.handle(h.now, Event::TimerFired { timer });
            h.collect();
        } else {
            break;
        }
    }
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("/timestamp"))),
        "a failed resend must not regain a clock probe: {:?}",
        h.actions
    );
}

#[test]
fn account_http_retry_after_resync_cannot_probe_the_clock_again() {
    for status in [429, 503] {
        let mut h = futures_snapshot();
        finish_recovery_failure(&mut h, "/positions", status);
        assert!(!h.e.store().loaded().positions);
        assert!(h.actions.iter().any(|a| {
            matches!(a, Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::REQUEST_FAILED)
        }));
    }
}

#[test]
fn history_http_retry_after_resync_cannot_probe_the_clock_again() {
    for status in [429, 503] {
        let mut h = H::new(Kucoin::new);
        h.e.handle_command(
            h.now,
            Command::Query {
                command: 71,
                query: safe_cex_engine::Query::Fills {
                    symbol: None,
                    order_id: None,
                    from_ms: Some(1.),
                    to_ms: Some(2.),
                    limit: Some(10),
                },
            },
            Some(20_000),
        )
        .unwrap();
        h.collect();
        finish_recovery_failure(&mut h, "/fills?", status);
        assert_eq!(h.terminals(), 1);
        assert!(
            h.actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 71, .. })))
        );
    }
}
