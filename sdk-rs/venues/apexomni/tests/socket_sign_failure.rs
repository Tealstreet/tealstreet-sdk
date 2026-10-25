//! A permanent provider refusal means the installed credentials can never
//! sign. It must end once, visibly, instead of retrying for ever; transient
//! remote-provider failures must keep retrying exactly as before.
mod support;
use safe_cex_engine::request_context::{HttpAction, Prepared as HostPrepared};
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::{Action, Event, Notice, Response, SocketChannel, TimerId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use std::sync::Arc;
use support::*;

#[derive(Default)]
struct Seen {
    config_invalid: usize,
    signed_sent: usize,
    startup_failed: usize,
    io: usize,
    private_connects: usize,
    signs: Vec<safe_cex_engine::RequestId>,
}

/// Runs ten simulated minutes, firing every timer in order and answering each
/// host signature with the next of `replies`. With `serve`, sent requests get
/// the market catalogue and every socket opens. A `remote` provider is
/// answered through the raw host handshake; a local one through the fixture.
fn run(e: &mut TestEngine, replies: &[SignError], serve: bool, remote: bool) -> Seen {
    let mut seen = Seen::default();
    let mut timers: Vec<(f64, TimerId)> = Vec::new();
    let mut now = 0.0;
    let mut prepared = 0;
    loop {
        let mut signed = Vec::new();
        let mut preparations = Vec::new();
        let mut sent = Vec::new();
        let mut opened = Vec::new();
        let actions: Vec<_> = if remote {
            e.drain_actions().collect()
        } else {
            e.prepared(now).collect()
        };
        for action in actions {
            match action {
                Action::SetTimer { timer, at_ms } => timers.push((at_ms, timer)),
                Action::ManagedHttp(http) => match *http {
                    HttpAction::Prepare { preparation, .. } => preparations.push(preparation),
                    HttpAction::Bind { request, .. } => {
                        seen.io += 1;
                        sent.push(request.id);
                    }
                    HttpAction::BindGroup { .. } => seen.io += 1,
                    HttpAction::Cancel { .. } => {}
                },
                Action::Http(request) => {
                    seen.io += 1;
                    if request
                        .headers
                        .iter()
                        .any(|(name, _)| *name == "APEX-SIGNATURE")
                    {
                        seen.signed_sent += 1;
                    }
                    sent.push(request.id);
                }
                Action::WsConnect {
                    socket, channel, ..
                } => {
                    seen.io += 1;
                    opened.push(socket);
                    if channel == Some(SocketChannel::Private) {
                        seen.private_connects += 1;
                    }
                }
                Action::Notice(Notice::StartupFailed { .. }) => seen.startup_failed += 1,
                Action::Notice(Notice::Event(event)) if event.code == "config.invalid" => {
                    seen.config_invalid += 1
                }
                Action::Sign { attempt, .. } => signed.push(attempt),
                _ => {}
            }
        }
        for preparation in preparations {
            prepared += 1;
            e.http_prepared(
                now,
                preparation,
                HostPrepared {
                    id: prepared,
                    expires_at: None,
                },
            );
        }
        for attempt in signed {
            let error = replies[seen.signs.len() % replies.len()];
            seen.signs.push(attempt);
            e.handle(
                now,
                Event::Signed {
                    attempt,
                    result: Err(error),
                },
            );
        }
        if serve {
            for request in sent {
                let response = Response {
                    status: 200,
                    headers: "",
                    body: MARKETS,
                };
                e.handle(now, Event::HttpResponse { request, response });
            }
            for socket in opened {
                e.handle(now, Event::WsOpened { socket });
            }
        }
        // Answers can produce further actions at the same instant.
        if e.has_actions() {
            continue;
        }
        timers.sort_by(|a, b| b.0.total_cmp(&a.0));
        let Some((at, timer)) = timers.pop() else {
            break;
        };
        if at > 600_000.0 {
            break;
        }
        now = at.max(now);
        e.handle(now, Event::TimerFired { timer });
    }
    seen
}

#[test]
fn a_key_only_account_fails_startup_once_with_no_io_for_ten_minutes() {
    let mut e = private_engine(
        Options::default(),
        &Material {
            key: "key",
            signing: "",
            passphrase: Some("passphrase"),
            seed: None,
        },
    );
    e.start(0.0);
    let seen = run(&mut e, &[SignError::Unavailable], false, false);
    assert_eq!(seen.startup_failed, 1, "one visible terminal failure");
    assert_eq!(seen.io, 0, "no HTTP and no socket");
    assert_eq!(seen.private_connects, 0);
}

/// A host-fulfilled provider: every signature crosses the host boundary.
struct Remote(Arc<dyn Signer>);
impl Signer for Remote {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(std::future::pending())
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        None
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}

#[test]
fn transient_remote_failures_keep_retrying_and_never_fail_startup() {
    let (provider, identities) = fixture_provider(&Material {
        key: "key",
        signing: "c2VjcmV0",
        passphrase: Some("pass"),
        seed: None,
    });
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Options::default()
    }));
    e.set_identities(identities).unwrap();
    e.set_signer(Some(Arc::new(Remote(provider)))).unwrap();
    e.start(0.0);
    let seen = run(
        &mut e,
        &[
            SignError::Unavailable,
            SignError::Refused,
            SignError::TimedOut,
            SignError::InvalidResult,
        ],
        false,
        true,
    );
    assert!(
        seen.signs.len() >= 8,
        "every kind retried twice: {}",
        seen.signs.len()
    );
    assert_eq!(seen.startup_failed, 0);
}

/// Signs REST with the fixture but refuses every socket login with `.1`.
struct NoSocketLogin(Arc<dyn Signer>, SignError);
impl Signer for NoSocketLogin {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).expect("immediate");
        Box::pin(std::future::ready(result))
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        if request.context.method == "WS" {
            Some(Err(self.1))
        } else {
            self.0.sign_ready(request)
        }
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.0.can_sign(key, algorithm)
    }
}

fn refusing_socket_logins(error: SignError) -> TestEngine {
    let (provider, identities) = fixture_provider(&Material::default());
    let mut e = safe_cex_engine::test_support::seeded_engine(ApexOmni::new(Options {
        identity: identities.primary.clone(),
        ..Options::default()
    }));
    e.set_identities(identities).unwrap();
    e.set_signer(Some(Arc::new(NoSocketLogin(provider, error))))
        .unwrap();
    e.start(0.0);
    e
}

#[test]
fn a_socket_login_that_cannot_sign_fails_once_and_never_reconnects() {
    let mut e = refusing_socket_logins(SignError::InvalidRequest);
    let seen = run(&mut e, &[SignError::Unavailable], true, false);
    assert_eq!(
        seen.private_connects, 1,
        "the login is never retried by reconnecting"
    );
    assert_eq!(seen.config_invalid, 1, "one visible error");
}

/// An algorithm the provider lacks fails that request kind only: it never
/// latches, and every other signed request keeps being sent.
#[test]
fn an_unsupported_login_fails_alone_and_signed_reads_keep_working() {
    let mut e = refusing_socket_logins(SignError::Unsupported);
    let seen = run(&mut e, &[SignError::Unavailable], true, false);
    assert_eq!(seen.config_invalid, 0);
    assert_eq!(seen.startup_failed, 0);
    assert!(seen.private_connects > 1, "the login is retried as before");
    assert!(
        seen.signed_sent > seen.private_connects,
        "signed reads continue: {} signed, {} connects",
        seen.signed_sent,
        seen.private_connects
    );
}
