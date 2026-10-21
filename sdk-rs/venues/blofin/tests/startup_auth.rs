//! Private startup until the account is ready: transient failures back off and
//! retry forever; a documented key refusal fails startup exactly once.
use safe_cex_engine::{
    Action, Engine, ErrorKind, Event, Notice, ReadCredentials, Response, SocketId, TimerId,
    VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options};

const INSTRUMENTS: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
const MARGIN: &str = r#"{"code":"0","data":{"marginMode":"cross"}}"#;
const MODE: &str = r#"{"code":"0","data":{"positionMode":"net_mode"}}"#;

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
    fn take(&mut self, engine: &mut Engine<Blofin>) -> Vec<Action> {
        let mut rest = Vec::new();
        for action in engine.drain_actions() {
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
    fn fire_next(&mut self, engine: &mut Engine<Blofin>) -> Option<(f64, Vec<Action>)> {
        self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
        if self.timers.is_empty() {
            return None;
        }
        let (timer, at) = self.timers.remove(0);
        engine.handle(at, Event::TimerFired { timer });
        Some((at, self.take(engine)))
    }
    fn until(
        &mut self,
        engine: &mut Engine<Blofin>,
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
    fn run(&mut self, engine: &mut Engine<Blofin>, horizon: f64) -> Vec<Action> {
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

fn respond(engine: &mut Engine<Blofin>, action: &Action, status: u16, body: &str, now: f64) {
    let Action::Http(r) = action else {
        panic!("expected request")
    };
    engine.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn get(suffix: &'static str) -> impl Fn(&Action) -> bool {
    move |a| matches!(a, Action::Http(r) if r.url.ends_with(suffix))
}

fn private_connect(out: &[Action]) -> Option<SocketId> {
    out.iter().find_map(|a| match a {
        Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
        _ => None,
    })
}

/// Loads the catalogue; returns the margin and position-mode reads and the private socket.
fn to_settings(engine: &mut Engine<Blofin>, rig: &mut Rig) -> (Action, Action, SocketId) {
    engine.start(0.);
    let instruments = rig.take(engine).remove(0);
    respond(engine, &instruments, 200, INSTRUMENTS, 1.);
    let out = rig.take(engine);
    let margin = out.iter().find(|a| get("margin-mode")(a)).unwrap().clone();
    let mode = out
        .iter()
        .find(|a| get("position-mode")(a))
        .unwrap()
        .clone();
    (margin, mode, private_connect(&out).unwrap())
}

#[test]
fn transient_settings_failures_back_off_and_the_account_still_loads() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (mut margin, mode, _) = to_settings(&mut engine, &mut rig);
    respond(&mut engine, &mode, 200, MODE, 2.);
    let failures: [(u16, &str); 6] = [
        (429, "Too Many Requests"),
        (503, r#"{"code":"503","msg":"Service unavailable"}"#),
        (200, "{"),
        (200, r#"{"code":"0","data":{}}"#),
        (
            200,
            r#"{"code":"152409","msg":"Signature verification failed"}"#,
        ),
        (200, r#"{"code":"999999","msg":"unknown"}"#),
    ];
    let mut sent = 2.;
    let mut delays = Vec::new();
    for (status, body) in failures {
        respond(&mut engine, &margin, status, body, sent);
        assert!(!rig.take(&mut engine).iter().any(get("margin-mode")));
        let (at, next) = rig.until(&mut engine, get("margin-mode")).expect("retried");
        delays.push(at - sent);
        sent = at;
        margin = next;
    }
    assert_eq!(delays, [1_000., 2_000., 4_000., 8_000., 16_000., 30_000.]);
    respond(&mut engine, &margin, 200, MARGIN, sent);
    let out = rig.take(&mut engine);
    assert!(out.iter().any(get("account/positions")), "account loads");
    assert!(rig.failures.is_empty() && rig.deactivated == 0);
}

#[test]
fn transport_failure_of_a_setting_read_is_retried() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (_, mode, _) = to_settings(&mut engine, &mut rig);
    let Action::Http(request) = &mode else {
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
    assert!(rig.until(&mut engine, get("position-mode")).is_some());
    assert!(rig.failures.is_empty());
}

#[test]
fn rejected_key_fails_startup_once_and_stops_retrying() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (margin, mode, _) = to_settings(&mut engine, &mut rig);
    let refused = r#"{"code":"152401","msg":"Access key does not exist"}"#;
    respond(&mut engine, &margin, 200, refused, 2.);
    respond(&mut engine, &mode, 200, refused, 2.);
    let mut out = rig.take(&mut engine);
    out.extend(rig.run(&mut engine, 600_000.));
    assert!(!out.iter().any(get("margin-mode")));
    assert!(!out.iter().any(get("position-mode")));
    assert!(private_connect(&out).is_none(), "no reconnect");
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("152401"));
    assert!(
        rig.failures[0]
            .message
            .contains("BloFin rejected the API key")
    );
    assert_eq!(rig.deactivated, 1);
}

#[test]
fn bad_signature_is_final_only_after_one_re_signed_resend() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (_, mode, _) = to_settings(&mut engine, &mut rig);
    let sign = r#"{"code":"152409","msg":"Signature verification failed"}"#;
    respond(&mut engine, &mode, 200, sign, 2.);
    rig.take(&mut engine);
    assert!(rig.failures.is_empty());
    let (at, mode) = rig.until(&mut engine, get("position-mode")).unwrap();
    respond(&mut engine, &mode, 200, sign, at);
    rig.take(&mut engine);
    assert!(
        !rig.run(&mut engine, 600_000.)
            .iter()
            .any(get("position-mode"))
    );
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("152409"));
}

#[test]
fn rejected_read_only_ip_fails_startup_without_deactivating() {
    let mut engine = Engine::new(Blofin::new(Options {
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..keyed()
    }));
    let mut rig = Rig::default();
    let (margin, _, _) = to_settings(&mut engine, &mut rig);
    respond(
        &mut engine,
        &margin,
        200,
        r#"{"code":"152406","msg":"Your IP is not included in your API key's IP whitelist"}"#,
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
fn missing_passphrase_fails_startup() {
    let mut engine = Engine::new(Blofin::new(Options {
        passphrase: None,
        ..keyed()
    }));
    let mut rig = Rig::default();
    engine.start(0.);
    let instruments = rig.take(&mut engine).remove(0);
    respond(&mut engine, &instruments, 200, INSTRUMENTS, 1.);
    rig.take(&mut engine);
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].message, "BloFin requires a passphrase");
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn invalid_read_only_credentials_fail_startup() {
    let mut engine = Engine::new(Blofin::new(Options {
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: String::new(),
            passphrase: None,
        }),
        ..keyed()
    }));
    let mut rig = Rig::default();
    engine.start(0.);
    assert!(
        !rig.take(&mut engine)
            .iter()
            .any(|a| matches!(a, Action::Http(_)))
    );
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn rejected_socket_login_fails_startup_and_stops_reconnecting() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (_, _, private) = to_settings(&mut engine, &mut rig);
    engine.handle(2., Event::WsOpened { socket: private });
    rig.take(&mut engine);
    engine.handle(
        3.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"error","code":"152408","msg":"Passphrase error"}"#,
        },
    );
    rig.take(&mut engine);
    let later = rig.run(&mut engine, 600_000.);
    assert!(private_connect(&later).is_none(), "no reconnect");
    assert_eq!(rig.failures.len(), 1);
    assert_eq!(rig.failures[0].venue_code.as_deref(), Some("152408"));
    assert_eq!(rig.deactivated, 1);
}

#[test]
fn generic_socket_login_failure_reconnects() {
    let mut engine = Engine::new(Blofin::new(keyed()));
    let mut rig = Rig::default();
    let (_, _, private) = to_settings(&mut engine, &mut rig);
    engine.handle(2., Event::WsOpened { socket: private });
    rig.take(&mut engine);
    engine.handle(
        3.,
        Event::WsMessage {
            socket: private,
            text: r#"{"event":"error","code":"60009","msg":"Login failed."}"#,
        },
    );
    rig.take(&mut engine);
    let (at, _) = rig
        .until(&mut engine, |a| {
            private_connect(std::slice::from_ref(a)).is_some()
        })
        .expect("reconnects");
    assert!(
        at < 10_000.,
        "closed on the rejection, not the auth deadline"
    );
    assert!(rig.failures.is_empty());
}
