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
    /// Records timers and startup notices; returns the remaining actions.
    fn take(&mut self, engine: &mut Engine<Okx>) -> Vec<Action> {
        let mut rest = Vec::new();
        for action in actions(engine) {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::StartupFailed { error }) => self.failures.push(error),
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
        Some((at, self.take(engine)))
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
    let catalogue = rig.take(engine).remove(0);
    reply(engine, &catalogue, &envelope(MARKET), 1.);
    rig.take(engine).into_iter().find(is_config).unwrap()
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
        let rest = rig.take(&mut engine);
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
        .take(&mut engine)
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
        pending.extend(rig.take(&mut engine).into_iter().filter(|a| {
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
    rig.take(&mut engine);
    assert!(rig.until(&mut engine, is_config).is_some());
    assert!(rig.failures.is_empty());
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
    let out = rig.take(&mut engine);
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
    rig.take(&mut engine);
    assert!(rig.failures.is_empty());
    let (at, config) = rig.until(&mut engine, is_config).unwrap();
    respond(&mut engine, &config, 401, sign, at);
    rig.take(&mut engine);
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
        rig.take(&mut engine);
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
        rig.take(&mut engine);
        (sent, config) = rig.until(&mut engine, is_config).expect("retried");
    }
    respond(&mut engine, &config, 401, sign, sent);
    rig.take(&mut engine);
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
    rig.take(&mut engine);
    rig.run(&mut engine, 600_000.);
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(rig.failures[0].message.contains("read-only API key"));
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
    let out = rig.take(&mut engine);
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
    let private = connected(&rig.take(engine), "/private").expect("private socket");
    engine.handle(3., Event::WsOpened { socket: private });
    rig.take(engine);
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
    rig.take(&mut engine);
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
    rig.take(&mut engine);
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
        let mut early = rig.take(&mut engine);
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
        rig.take(&mut engine);
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
        rig.take(&mut engine);
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
