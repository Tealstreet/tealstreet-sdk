#![allow(dead_code)]
//! Private startup until the account is ready: transient failures back off and
//! retry forever; a documented key refusal fails startup exactly once.
mod support;
use crate::support::FixtureOptions as Options;
use crate::support::*;
use safe_cex_engine::{
    Action, Engine, ErrorKind, Event, Notice, ReadCredentials, Response, SocketId, TimerId,
    VenueError,
};
use safe_cex_venue_okx::Okx;

fn keyed() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }
}

#[derive(Default)]
struct Rig {
    timers: Vec<(TimerId, f64)>,
    failures: Vec<VenueError>,
    deactivated: usize,
}
impl Rig {
    /// Records timers and startup notices from actions released at `now`;
    /// returns the remaining actions.
    fn take(&mut self, engine: &mut Engine<Okx>, now: f64) -> Vec<Action> {
        let mut rest = Vec::new();
        for action in prepared(engine, now) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::StartupFailed { error, .. }) => self.failures.push(error),
                Action::Notice(Notice::Deactivate) => self.deactivated += 1,
                other => rest.push(other),
            }
        }
        rest
    }
    /// Fires the earliest timer; returns its time and the actions it produced.
    fn fire_next(&mut self, engine: &mut Engine<Okx>) -> Option<(f64, Vec<Action>)> {
        self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        if self.timers.is_empty() {
            return None;
        }
        let (timer, at) = self.timers.remove(0);
        engine.handle(at, Event::TimerFired { timer });
        Some((at, self.take(engine, at)))
    }
    /// Fires timers until `pick` finds a request; returns when it was sent.
    fn until(
        &mut self,
        engine: &mut Engine<Okx>,
        pick: impl Fn(&Action) -> bool,
    ) -> Option<(f64, Action)> {
        for _ in 0..64 {
            let (at, out) = self.fire_next(engine)?;
            if let Some(action) = out.into_iter().find(|a| pick(a)) {
                return Some((at, action));
            }
        }
        None
    }
    /// Fires every timer up to `horizon`, returning all actions produced.
    fn run(&mut self, engine: &mut Engine<Okx>, horizon: f64) -> Vec<Action> {
        let mut out = Vec::new();
        while let Some(at) = self.timers.iter().map(|(_, at)| *at).reduce(f64::min) {
            if at > horizon {
                break;
            }
            out.extend(self.fire_next(engine).unwrap().1);
        }
        out
    }
}

fn respond(engine: &mut Engine<Okx>, action: &Action, status: u16, body: &str, now: f64) {
    respond_with(engine, action, status, "", body, now);
}

fn respond_with(
    engine: &mut Engine<Okx>,
    action: &Action,
    status: u16,
    headers: &str,
    body: &str,
    now: f64,
) {
    let Action::Http(r) = action else {
        panic!("expected request")
    };
    engine.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
}

fn is_config(action: &Action) -> bool {
    is_get(action, "/api/v5/account/config")
}

/// Loads the catalogue and returns the first account config read.
fn to_config(engine: &mut Engine<Okx>, rig: &mut Rig) -> Action {
    engine.start(0.);
    let catalogue = rig.take(engine, 0.).remove(0);
    reply(engine, &catalogue, &envelope(MARKET), 1.);
    rig.take(engine, 1.).into_iter().find(is_config).unwrap()
}

fn connected(out: &[Action], suffix: &str) -> Option<SocketId> {
    out.iter().find_map(|a| match a {
        Action::WsConnect { socket, url, .. } if url.ends_with(suffix) => Some(*socket),
        _ => None,
    })
}

#[test]
fn transient_config_failures_back_off_and_the_account_still_loads() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let mut config = to_config(&mut engine, &mut rig);
    let failures: [(u16, &str); 7] = [
        (500, "Internal Server Error"),
        (
            503,
            r#"{"code":"50001","msg":"Service temporarily unavailable","data":[]}"#,
        ),
        (502, "<html>bad gateway</html>"),
        (200, "{"),
        (200, r#"{"code":"59999","msg":"unknown","data":[]}"#),
        (401, r#"{"code":"50113","msg":"Invalid Sign","data":[]}"#),
        (
            200,
            r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#,
        ),
    ];
    let mut sent = 1.;
    let mut delays = Vec::new();
    for (status, body) in failures {
        respond(&mut engine, &config, status, body, sent);
        let rest = rig.take(&mut engine, sent);
        assert!(!rest.iter().any(is_config), "no immediate resend");
        let (at, next) = rig.until(&mut engine, is_config).expect("config retried");
        delays.push(at - sent);
        sent = at;
        config = next;
    }
    let nominal = [1_000., 2_000., 4_000., 8_000., 16_000., 30_000., 30_000.];
    assert_eq!(delays.len(), nominal.len());
    assert!(
        delays.iter().zip(nominal).all(|(d, n)| jittered(*d, n)),
        "{delays:?} not jittered from {nominal:?}"
    );
    respond(
        &mut engine,
        &config,
        200,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        sent,
    );
    let mut pending: Vec<_> = rig
        .take(&mut engine, sent)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade")))
        .collect();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut engine, &a, &body, sent + 1.);
        pending.extend(rig.take(&mut engine, sent + 1.).into_iter().filter(|a| {
            matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade"))
        }));
    }
    let loaded = engine.store().loaded();
    assert!(loaded.balance && loaded.orders && loaded.positions);
    assert!(rig.failures.is_empty() && rig.deactivated == 0);
}

#[test]
fn transport_failure_of_config_is_retried() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let config = to_config(&mut engine, &mut rig);
    let Action::Http(request) = &config else {
        unreachable!()
    };
    engine.handle(
        2.,
        Event::HttpFailed {
            request: request.id,
            error: "timed out",
        },
    );
    rig.take(&mut engine, 2.);
    assert!(rig.until(&mut engine, is_config).is_some());
    assert!(rig.failures.is_empty());
}

#[test]
fn unanswered_config_read_times_out_into_startup_backoff() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    // The config read is sent at 1 ms; its 20 s shared deadline is 20_001 ms.
    to_config(&mut engine, &mut rig);
    let early = rig.run(&mut engine, 20_000.);
    assert!(
        !early.iter().any(is_config),
        "no resend before the shared deadline"
    );
    let resent = rig.until(&mut engine, is_config);
    assert!(
        resent.is_some(),
        "an unanswered config read must time out into the startup backoff and be re-sent"
    );
    let (at, _) = resent.unwrap();
    assert!(
        jittered(at - 20_001., 1_000.),
        "re-sent at {at}, not one first backoff step after the deadline"
    );
    assert!(rig.failures.is_empty() && rig.deactivated == 0);
}

#[test]
fn rejected_key_fails_startup_once_and_stops_retrying() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let config = to_config(&mut engine, &mut rig);
    respond(
        &mut engine,
        &config,
        401,
        r#"{"code":"50111","msg":"Invalid OK-ACCESS-KEY","data":[]}"#,
        2.,
    );
    let out = rig.take(&mut engine, 2.);
    let later = rig.run(&mut engine, 600_000.);
    assert!(!out.iter().chain(&later).any(is_config));
    assert!(connected(&later, "/private").is_none());
    assert_eq!(rig.failures.len(), 1);
    let error = &rig.failures[0];
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.venue_code.as_deref(), Some("50111"));
    assert!(error.message.contains("OKX rejected the API key"));
    assert_eq!(rig.deactivated, 1);
}

#[test]
fn invalid_signature_is_final_only_after_one_re_signed_resend() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let config = to_config(&mut engine, &mut rig);
    let sign = r#"{"code":"50113","msg":"Invalid Sign","data":[]}"#;
    respond(&mut engine, &config, 401, sign, 2.);
    rig.take(&mut engine, 2.);
    assert!(rig.failures.is_empty());
    let (at, config) = rig.until(&mut engine, is_config).unwrap();
    respond(&mut engine, &config, 401, sign, at);
    rig.take(&mut engine, at);
    assert!(rig.run(&mut engine, 600_000.).iter().all(|a| !is_config(a)));
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("50113"));
}

#[test]
fn invalid_signature_counts_only_consecutive_answers() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let mut config = to_config(&mut engine, &mut rig);
    let sign = r#"{"code":"50113","msg":"Invalid Sign","data":[]}"#;
    let busy = r#"{"code":"50011","msg":"Too Many Requests","data":[]}"#;
    let mut sent = 2.;
    for interruption in [Some((200, busy)), Some((503, "<html>down</html>")), None] {
        respond(&mut engine, &config, 401, sign, sent);
        rig.take(&mut engine, sent);
        let (at, next) = rig.until(&mut engine, is_config).expect("re-signed");
        match interruption {
            Some((status, body)) => respond(&mut engine, &next, status, body, at),
            None => {
                let Action::Http(request) = &next else {
                    unreachable!()
                };
                engine.handle(
                    at,
                    Event::HttpFailed {
                        request: request.id,
                        error: "timed out",
                    },
                );
            }
        }
        rig.take(&mut engine, at);
        (sent, config) = rig.until(&mut engine, is_config).expect("retried");
    }
    respond(&mut engine, &config, 401, sign, sent);
    rig.take(&mut engine, sent);
    assert!(rig.until(&mut engine, is_config).is_some(), "re-signed");
    assert!(rig.failures.is_empty() && rig.deactivated == 0);
}

#[test]
fn rejected_read_only_key_fails_startup_without_deactivating() {
    let mut engine = crate::support::engine(Options {
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..keyed()
    });
    let mut rig = Rig::default();
    let config = to_config(&mut engine, &mut rig);
    respond(
        &mut engine,
        &config,
        200,
        r#"{"code":"50120","msg":"API key doesn't have permission","data":[]}"#,
        2.,
    );
    rig.take(&mut engine, 2.);
    rig.run(&mut engine, 600_000.);
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(rig.failures[0].message.contains("read-only API key"));
    assert_eq!(rig.deactivated, 0);
}

#[test]
fn an_ip_outside_the_key_whitelist_fails_startup_saying_to_add_the_new_ips() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let config = to_config(&mut engine, &mut rig);
    respond(
        &mut engine,
        &config,
        200,
        r#"{"code":"50110","msg":"Your IP 203.0.113.7 is not included in your API key's IP whitelist.","data":[]}"#,
        2.,
    );
    rig.take(&mut engine, 2.);
    rig.run(&mut engine, 600_000.);
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(
        rig.failures[0]
            .message
            .ends_with(safe_cex_engine::error::IP_WHITELIST_HINT),
        "{}",
        rig.failures[0].message
    );
    assert_eq!(rig.deactivated, 0);
}

#[test]
fn invalid_read_only_credentials_fail_startup() {
    let mut engine = crate::support::engine(Options {
        read_credentials: Some(ReadCredentials {
            key: String::new(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..keyed()
    });
    let mut rig = Rig::default();
    engine.start(0.);
    let out = rig.take(&mut engine, 0.);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::InvalidInput));
}

/// Loads the account to the point the private socket is open and logging in.
fn to_private_login(engine: &mut Engine<Okx>, rig: &mut Rig) -> SocketId {
    let config = to_config(engine, rig);
    respond(
        engine,
        &config,
        200,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        2.,
    );
    let private = connected(&rig.take(engine, 2.), "/private").expect("private socket");
    engine.handle(3., Event::WsOpened { socket: private });
    rig.take(engine, 3.);
    private
}

#[test]
fn rejected_socket_login_fails_startup_and_stops_reconnecting() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let private = to_private_login(&mut engine, &mut rig);
    ws(
        &mut engine,
        private,
        r#"{"event":"error","code":"60024","msg":"Wrong passphrase"}"#,
    );
    rig.take(&mut engine, 4.);
    let later = rig.run(&mut engine, 600_000.);
    assert!(connected(&later, "/private").is_none(), "no reconnect");
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("60024"));
    assert_eq!(rig.deactivated, 1);
}

#[test]
fn transient_socket_login_failure_reconnects() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let private = to_private_login(&mut engine, &mut rig);
    ws(
        &mut engine,
        private,
        r#"{"event":"error","code":"60014","msg":"Requests too frequent"}"#,
    );
    rig.take(&mut engine, 4.);
    assert!(
        rig.until(
            &mut engine,
            |a| matches!(a, Action::WsConnect { url, .. } if url.ends_with("/private"))
        )
        .is_some()
    );
    assert!(rig.failures.is_empty());
}

#[test]
fn rate_limited_config_read_waits_out_the_rate_limit_window() {
    // OKX's default matches its discovery pause; Retry-After overrides it.
    for (status, headers, wait) in [
        (429, "Retry-After: 120", 120_000.),
        (418, "", 10_000.),
        (429, "", 10_000.),
    ] {
        let mut engine = crate::support::engine(keyed());
        let mut rig = Rig::default();
        let config = to_config(&mut engine, &mut rig);
        respond_with(
            &mut engine,
            &config,
            status,
            headers,
            "Too Many Requests",
            1.,
        );
        let mut early = rig.take(&mut engine, 1.);
        early.extend(rig.run(&mut engine, wait));
        assert!(
            !early.iter().any(is_config),
            "{status} {headers:?}: retried early"
        );
        let (at, _) = rig.until(&mut engine, is_config).expect("config retried");
        assert_eq!(at, 1. + wait, "{status} {headers:?}");
        assert!(rig.failures.is_empty());
    }
}

#[test]
fn only_current_documented_key_or_passphrase_socket_codes_end_startup() {
    for code in ["60005", "60024", "60032"] {
        let mut engine = crate::support::engine(keyed());
        let mut rig = Rig::default();
        let private = to_private_login(&mut engine, &mut rig);
        ws(
            &mut engine,
            private,
            &format!(r#"{{"event":"error","code":"{code}","msg":"refused"}}"#),
        );
        rig.take(&mut engine, 4.);
        assert_eq!(rig.failures.len(), 1, "{code}");
        assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(rig.failures[0].venue_code.as_deref(), Some(code));
        assert_eq!(rig.deactivated, 1);
    }
    // Generic login/signature, clock, rate, protocol and internal errors do not
    // establish a permanently invalid credential. Retired undocumented codes
    // must not inherit terminal meanings from obsolete API tables.
    for code in [
        "60001", "60003", "60004", "60006", "60007", "60009", "60014", "60023", "60031", "63999",
    ] {
        let mut engine = crate::support::engine(keyed());
        let mut rig = Rig::default();
        let private = to_private_login(&mut engine, &mut rig);
        ws(
            &mut engine,
            private,
            &format!(r#"{{"event":"error","code":"{code}","msg":"retryable or unknown"}}"#),
        );
        rig.take(&mut engine, 4.);
        assert!(
            rig.until(
                &mut engine,
                |a| matches!(a, Action::WsConnect { url, .. } if url.ends_with("/private"))
            )
            .is_some(),
            "{code}"
        );
        assert!(rig.failures.is_empty(), "{code}");
        assert_eq!(rig.deactivated, 0);
    }
}

fn to_account_reads(engine: &mut Engine<Okx>, rig: &mut Rig) -> Vec<Action> {
    let config = to_config(engine, rig);
    respond(
        engine,
        &config,
        200,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        2.,
    );
    rig.take(engine, 2.)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade")))
        .collect()
}

#[test]
fn a_key_refused_the_risk_read_fails_startup_instead_of_polling() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let risk = to_account_reads(&mut engine, &mut rig)
        .into_iter()
        .find(|a| matches!(a, Action::Http(r) if r.url.contains("account-position-risk")))
        .expect("the risk read");
    respond(
        &mut engine,
        &risk,
        200,
        r#"{"code":"50110","msg":"Your IP is not included in your API key's whitelist","data":[]}"#,
        3.,
    );
    rig.take(&mut engine, 3.);
    assert_eq!(rig.failures.len(), 1, "{:?}", rig.failures);
}

#[test]
fn an_empty_risk_read_does_not_hang_startup() {
    let mut engine = crate::support::engine(keyed());
    let mut rig = Rig::default();
    let mut pending = to_account_reads(&mut engine, &mut rig);
    while let Some(a) = pending.pop() {
        reply(&mut engine, &a, &envelope(""), 3.);
        pending.extend(rig.take(&mut engine, 3.).into_iter().filter(|a| {
            matches!(a, Action::Http(r) if r.url.contains("/api/v5/account") || r.url.contains("/api/v5/trade"))
        }));
    }
    assert!(engine.store().loaded().balance);
}

#[test]
fn a_refusal_ends_the_start_however_it_arrives() {
    for (status, body) in [
        (
            401,
            r#"{"code":"50110","msg":"Your IP is not on the API key's IP allowlist","data":[]}"#,
        ),
        (
            200,
            r#"{"code":"50110","msg":"Your IP is not on the API key's IP allowlist","data":[]}"#,
        ),
        (
            401,
            r#"{"code":"50111","msg":"Invalid OK-ACCESS-KEY","data":[]}"#,
        ),
    ] {
        let mut engine = crate::support::engine(keyed());
        let mut rig = Rig::default();
        let config = to_config(&mut engine, &mut rig);
        respond(&mut engine, &config, status, body, 2.);
        rig.take(&mut engine, 2.);
        rig.run(&mut engine, 120_000.);
        assert_eq!(rig.failures.len(), 1, "{status} {body}");
    }
}

fn eea_config(demo: bool, warm: bool) -> (Engine<Okx>, Rig, Action) {
    let mut engine = crate::support::engine(Options {
        eea: true,
        testnet: demo,
        ..keyed()
    });
    if warm {
        let (seed, _, _) = public(Options {
            public_data_only: true,
            ..Options::default()
        });
        engine
            .store_mut()
            .set_markets(seed.store().markets().to_vec());
    }
    let mut rig = Rig::default();
    engine.start(0.);
    let out = rig.take(&mut engine, 0.);
    let reads: Vec<_> = out
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    assert_eq!(reads.len(), 1);
    assert!(
        is_config(&reads[0]),
        "EEA config must precede keyed discovery"
    );
    let Action::Http(request) = &reads[0] else {
        unreachable!()
    };
    assert_eq!(request.url, "https://eea.okx.com/api/v5/account/config");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "OK-ACCESS-KEY" && value == "key")
    );
    assert_eq!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "x-simulated-trading" && value == "1"),
        demo
    );
    (engine, rig, reads.into_iter().next().unwrap())
}

fn config_ok(engine: &mut Engine<Okx>, rig: &mut Rig, config: &Action) -> Action {
    respond(
        engine,
        config,
        200,
        &envelope(r#"{"posMode":"long_short_mode","acctLv":"3"}"#),
        1.,
    );
    let out = rig.take(engine, 1.);
    assert!(
        !engine.store().hedged(),
        "early config cannot publish settings"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    let reads: Vec<_> = out
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    assert_eq!(reads.len(), 1);
    assert!(is_get(
        &reads[0],
        "/api/v5/account/instruments?instType=SWAP"
    ));
    reads.into_iter().next().unwrap()
}

#[test]
fn eea_config_authentication_precedes_cold_and_warm_keyed_discovery() {
    for demo in [false, true] {
        for warm in [false, true] {
            let (mut engine, mut rig, config) = eea_config(demo, warm);
            let swap = config_ok(&mut engine, &mut rig, &config);
            respond(&mut engine, &swap, 200, &envelope(MARKET), 2.);
            let out = rig.take(&mut engine, 2.);
            let futures = out
                .iter()
                .find(|a| is_get(a, "/api/v5/account/instruments?instType=FUTURES"))
                .unwrap();
            assert!(!engine.store().hedged());
            respond(&mut engine, futures, 200, &envelope(""), 3.);
            let out = rig.take(&mut engine, 3.);
            assert!(engine.store().hedged());
            assert!(
                !out.iter().any(is_config),
                "do not duplicate the authenticated config"
            );
            assert!(
                out.iter()
                    .any(|a| matches!(a,Action::Http(r)if r.url.contains("account-position-risk")))
            );
            let loaded = engine.store().loaded();
            assert!(
                !(loaded.balance || loaded.orders || loaded.positions),
                "config plus catalog is not complete startup proof"
            );
        }
    }
}

#[test]
fn eea_config_re_signs_once_then_refuses_without_later_polling() {
    for demo in [false, true] {
        let (mut engine, mut rig, first) = eea_config(demo, false);
        let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
        respond(&mut engine, &first, 401, refused, 1.);
        rig.take(&mut engine, 1.);
        let (at, second) = rig
            .until(&mut engine, is_config)
            .expect("one re-signed retry");
        assert!(jittered(at - 1., 1000.));
        let signature = |action: &Action| {
            let Action::Http(r) = action else {
                unreachable!()
            };
            r.headers
                .iter()
                .find(|(name, _)| *name == "OK-ACCESS-SIGN")
                .unwrap()
                .1
                .clone()
        };
        assert_ne!(signature(&first), signature(&second));
        respond(&mut engine, &second, 401, refused, at);
        rig.take(&mut engine, at);
        assert_eq!(rig.failures.len(), 1);
        assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(rig.failures[0].venue_code.as_deref(), Some("50113"));
        assert!(
            rig.run(&mut engine, 600_000.)
                .iter()
                .all(|a| !matches!(a, Action::Http(_)))
        );
    }
}

#[test]
fn eea_revocation_during_keyed_catalogue_refuses_after_one_resend() {
    for demo in [false, true] {
        let (mut engine, mut rig, config) = eea_config(demo, false);
        let first = config_ok(&mut engine, &mut rig, &config);
        let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
        respond(&mut engine, &first, 401, refused, 2.);
        rig.take(&mut engine, 2.);
        let (at, second) = rig
            .until(&mut engine, |a| {
                is_get(a, "/api/v5/account/instruments?instType=SWAP")
            })
            .expect("one re-signed catalog retry");
        assert!(jittered(at - 2., 1000.));
        respond(&mut engine, &second, 401, refused, at);
        rig.take(&mut engine, at);
        assert_eq!(rig.failures.len(), 1);
        assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
        assert_eq!(rig.failures[0].venue_code.as_deref(), Some("50113"));
        assert!(
            !engine.store().hedged(),
            "refused catalog cannot publish staged settings"
        );
        assert!(
            rig.run(&mut engine, 600_000.)
                .iter()
                .all(|a| !matches!(a, Action::Http(_)))
        );
    }
}

fn streamed(engine: &mut Engine<Okx>, action: &Action, status: u16, body: &str, now: f64) {
    let Action::Http(request) = action else {
        unreachable!()
    };
    engine.handle(
        now,
        Event::HttpStart {
            request: request.id,
            status,
            headers: "",
        },
    );
    engine.handle(
        now,
        Event::HttpChunk {
            request: request.id,
            data: body.as_bytes(),
        },
    );
    engine.handle(
        now,
        Event::HttpEnd {
            request: request.id,
        },
    );
}

#[test]
fn eea_streamed_catalogue_refusal_has_the_same_bounded_retry() {
    let (mut engine, mut rig, config) = eea_config(false, false);
    let first = config_ok(&mut engine, &mut rig, &config);
    let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
    streamed(&mut engine, &first, 200, refused, 2.);
    rig.take(&mut engine, 2.);
    let (at, second) = rig
        .until(&mut engine, |a| {
            is_get(a, "/api/v5/account/instruments?instType=SWAP")
        })
        .unwrap();
    streamed(&mut engine, &second, 200, refused, at);
    rig.take(&mut engine, at);
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("50113"));
    assert!(
        rig.run(&mut engine, 600_000.)
            .iter()
            .all(|a| !matches!(a, Action::Http(_)))
    );
}

#[test]
fn eea_config_rate_limit_with_signature_body_is_a_typed_transient_failure() {
    for demo in [false, true] {
        let (mut engine, mut rig, config) = eea_config(demo, false);
        respond_with(
            &mut engine,
            &config,
            429,
            "Retry-After: 12",
            r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#,
            1.,
        );
        let out = rig.take(&mut engine, 1.);
        assert!(rig.failures.is_empty());
        let diagnostic = out
            .iter()
            .find_map(|action| match action {
                Action::Notice(Notice::Event(event))
                    if event.fields.contains(&("kind", "rateLimited".into())) =>
                {
                    Some(event)
                }
                _ => None,
            })
            .expect("HTTP 429 must retain its typed rate-limit diagnosis");
        assert_eq!(diagnostic.code, "request.failed");
        assert!(
            diagnostic
                .fields
                .contains(&("retryAfterMs", 12_000.0.into()))
        );
        assert!(!out.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "auth.rejected"
        )));
        let (at, _) = rig
            .until(&mut engine, is_config)
            .expect("rate limit keeps the retry");
        assert!(at >= 12_001.);
    }
}

#[test]
fn eea_config_interrupted_signature_streak_remains_transient() {
    let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
    for interruption in [Some(503), Some(429), None] {
        let (mut engine, mut rig, first) = eea_config(false, false);
        respond(&mut engine, &first, 401, refused, 1.);
        rig.take(&mut engine, 1.);
        let (at, retry) = rig.until(&mut engine, is_config).unwrap();
        if let Some(status) = interruption {
            respond(&mut engine, &retry, status, refused, at);
        } else {
            let Action::Http(request) = retry else {
                unreachable!()
            };
            engine.handle(
                at,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost reply",
                },
            );
        }
        let out = rig.take(&mut engine, at);
        assert!(!out.iter().any(|action| matches!(action,
            Action::Notice(Notice::Event(event)) if event.code == "auth.rejected"
        )));
        if interruption == Some(429) {
            assert!(out.iter().any(|action| matches!(action,
                Action::Notice(Notice::Event(event))
                    if event.fields.contains(&("kind", "rateLimited".into()))
            )));
        }
        let (next_at, retry) = rig.until(&mut engine, is_config).unwrap();
        if interruption == Some(429) {
            assert!(next_at >= at + 10_000.);
        }
        respond(&mut engine, &retry, 401, refused, next_at);
        rig.take(&mut engine, next_at);
        assert!(rig.failures.is_empty());
        assert!(rig.until(&mut engine, is_config).is_some());
    }
}

#[test]
fn eea_expired_early_config_cannot_publish_settings_after_slow_catalogue() {
    let (mut engine, mut rig, config) = eea_config(false, false);
    let swap = config_ok(&mut engine, &mut rig, &config);
    respond(&mut engine, &swap, 200, &envelope(MARKET), 2.);
    let out = rig.take(&mut engine, 2.);
    let futures = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/instruments?instType=FUTURES"))
        .unwrap();
    respond(&mut engine, futures, 200, &envelope(""), 20_001.);
    let out = rig.take(&mut engine, 20_001.);
    assert!(
        !engine.store().hedged(),
        "expired config has no publication authority"
    );
    assert!(!out.iter().any(|a| matches!(a, Action::WsConnect { .. })));
    assert!(
        rig.until(&mut engine, is_config).is_some(),
        "fresh config required after its original deadline"
    );
}

#[test]
fn eea_config_signature_retry_can_succeed() {
    let (mut engine, mut rig, first) = eea_config(false, false);
    respond(
        &mut engine,
        &first,
        401,
        r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#,
        1.,
    );
    rig.take(&mut engine, 1.);
    let (at, retry) = rig.until(&mut engine, is_config).unwrap();
    respond(
        &mut engine,
        &retry,
        200,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        at,
    );
    let out = rig.take(&mut engine, at);
    assert!(
        out.iter()
            .any(|a| is_get(a, "/api/v5/account/instruments?instType=SWAP"))
    );
    assert!(rig.failures.is_empty());
}

#[test]
fn eea_bad_config_and_documented_permission_refuse_without_keyed_discovery() {
    for (body, kind) in [
        (r#"{"code":"0","data":[]}"#, ErrorKind::Unsupported),
        (
            r#"{"code":"0","data":[{"posMode":"not-a-mode","acctLv":"2"}]}"#,
            ErrorKind::Unsupported,
        ),
        (
            r#"{"code":"50120","msg":"synthetic permission refusal","data":[]}"#,
            ErrorKind::AuthPermission,
        ),
    ] {
        let (mut engine, mut rig, config) = eea_config(false, false);
        respond(&mut engine, &config, 200, body, 1.);
        let mut out = rig.take(&mut engine, 1.);
        out.extend(rig.run(&mut engine, 600_000.));
        assert_eq!(rig.failures.len(), 1);
        assert_eq!(rig.failures[0].kind, Some(kind));
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn eea_read_only_key_refusal_never_deactivates_the_primary() {
    let mut engine = crate::support::engine(Options {
        eea: true,
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..keyed()
    });
    let mut rig = Rig::default();
    engine.start(0.);
    let config = rig
        .take(&mut engine, 0.)
        .into_iter()
        .find(is_config)
        .unwrap();
    respond(
        &mut engine,
        &config,
        200,
        r#"{"code":"50111","msg":"synthetic refused read key","data":[]}"#,
        1.,
    );
    rig.take(&mut engine, 1.);
    assert_eq!(rig.failures.len(), 1);
    assert!(rig.failures[0].message.contains("read-only API key"));
    assert_eq!(rig.deactivated, 0);
}

#[test]
fn eea_catalogue_transport_and_rate_interruptions_reset_signature_streak() {
    let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
    for interruption in [Some(503), Some(429), None] {
        let (mut engine, mut rig, config) = eea_config(false, false);
        let first = config_ok(&mut engine, &mut rig, &config);
        respond(&mut engine, &first, 401, refused, 2.);
        rig.take(&mut engine, 2.);
        let catalog = |a: &Action| is_get(a, "/api/v5/account/instruments?instType=SWAP");
        let (at, retry) = rig.until(&mut engine, catalog).unwrap();
        if let Some(status) = interruption {
            respond(
                &mut engine,
                &retry,
                status,
                r#"{"code":"50111","msg":"untrusted error body","data":[]}"#,
                at,
            );
        } else {
            let Action::Http(request) = retry else {
                unreachable!()
            };
            engine.handle(
                at,
                Event::HttpFailed {
                    request: request.id,
                    error: "lost reply",
                },
            );
        }
        rig.take(&mut engine, at);
        let (next_at, retry) = rig.until(&mut engine, catalog).unwrap();
        if interruption == Some(429) {
            assert!(next_at >= at + 10_000.);
        }
        respond(&mut engine, &retry, 401, refused, next_at);
        rig.take(&mut engine, next_at);
        assert!(rig.failures.is_empty());
        assert!(rig.until(&mut engine, catalog).is_some());
    }
}

fn streamed_catalogue_interruption_resets_streak(status: u16, body: &str) {
    let refused = r#"{"code":"50113","msg":"synthetic refusal","data":[]}"#;
    let (mut engine, mut rig, config) = eea_config(false, false);
    let first = config_ok(&mut engine, &mut rig, &config);
    streamed(&mut engine, &first, 200, refused, 2.);
    rig.take(&mut engine, 2.);
    let catalog = |action: &Action| is_get(action, "/api/v5/account/instruments?instType=SWAP");
    let (at, retry) = rig.until(&mut engine, catalog).unwrap();
    streamed(&mut engine, &retry, status, body, at);
    rig.take(&mut engine, at);
    assert!(rig.failures.is_empty());
    let (next_at, retry) = rig.until(&mut engine, catalog).expect("rate retry");
    if status == 429 {
        assert!(next_at >= at + 10_000.);
    }
    streamed(&mut engine, &retry, 200, refused, next_at);
    rig.take(&mut engine, next_at);
    assert!(
        rig.failures.is_empty(),
        "rate interruption must reset the signature streak"
    );
    assert!(
        rig.until(&mut engine, catalog).is_some(),
        "a fresh signature retry remains"
    );
}

#[test]
fn eea_streamed_catalogue_no_list_rate_limit_resets_signature_streak() {
    streamed_catalogue_interruption_resets_streak(
        429,
        r#"{"code":"50113","msg":"synthetic rate interruption"}"#,
    );
}

#[test]
fn eea_streamed_catalogue_list_rate_limit_resets_signature_streak() {
    streamed_catalogue_interruption_resets_streak(
        429,
        r#"{"code":"50113","msg":"synthetic rate interruption","data":[]}"#,
    );
}

#[test]
fn eea_streamed_catalogue_no_list_signature_refusal_stays_permanent_on_second_answer() {
    let (mut engine, mut rig, config) = eea_config(false, false);
    let first = config_ok(&mut engine, &mut rig, &config);
    let refused = r#"{"code":"50113","msg":"synthetic refusal"}"#;
    streamed(&mut engine, &first, 200, refused, 2.);
    rig.take(&mut engine, 2.);
    assert!(rig.failures.is_empty());
    let catalog = |action: &Action| is_get(action, "/api/v5/account/instruments?instType=SWAP");
    let (at, retry) = rig
        .until(&mut engine, catalog)
        .expect("one signature retry");
    streamed(&mut engine, &retry, 200, refused, at);
    rig.take(&mut engine, at);
    assert_eq!(
        rig.failures.len(),
        1,
        "genuine authentication answers are not interruptions"
    );
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("50113"));
    assert!(!rig.run(&mut engine, 600_000.).iter().any(catalog));
}

#[test]
fn eea_streamed_catalogue_no_list_200_error_resets_signature_streak() {
    streamed_catalogue_interruption_resets_streak(
        200,
        r#"{"code":"50011","msg":"synthetic rate interruption"}"#,
    );
}

#[test]
fn eea_streamed_catalogue_list_200_error_resets_signature_streak() {
    streamed_catalogue_interruption_resets_streak(
        200,
        r#"{"code":"50011","msg":"synthetic rate interruption","data":[]}"#,
    );
}
