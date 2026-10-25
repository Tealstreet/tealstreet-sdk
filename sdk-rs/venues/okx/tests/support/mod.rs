use safe_cex_engine::{Action, Engine, Event, Method, Notice, Response, SocketId, TimerId};
use safe_cex_venue_okx::{Okx, Options as NativeOptions};
type Options = FixtureOptions;

// OKX v5 docs: public instruments, tickers and books payload schemas.
// https://www.okx.com/docs-v5/en/#public-data-rest-api-get-instruments
pub const MARKET: &str = r#"{"instId":"BTC-USDT-SWAP","instType":"SWAP","ctType":"linear","ctVal":"0.01","ctValCcy":"BTC","settleCcy":"USDT","state":"live","lotSz":"0.01","tickSz":"0.1","minSz":"0.01","maxIcebergSz":"1000","maxLmtSz":"1000","maxMktSz":"1000","maxStopSz":"1000","maxTriggerSz":"1000","maxTwapSz":"1000","lever":"100"}"#;
pub const TICKER: &str = r#"{"instId":"BTC-USDT-SWAP","last":"40000","open24h":"40000","bidPx":"39999.9","askPx":"40000.1","volCcy24h":"123.4","vol24h":"12340"}"#;

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
/// The retry timer armed at `sent` plus a jittered `nominal` delay; firing it
/// at `sent + nominal` is always at or after its due time.
pub fn retry_timer(out: &[Action], sent: f64, nominal: f64) -> Option<TimerId> {
    out.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if jittered(at_ms - sent, nominal) => Some(*timer),
        _ => None,
    })
}

/// Actions with structured failures read as their legacy `CommandDone` text.
pub fn actions(engine: &mut Engine<Okx>) -> Vec<Action> {
    engine
        .drain_actions()
        .map(|action| match action {
            Action::Notice(Notice::CommandFailed { command, error }) => {
                Action::Notice(Notice::CommandFailed {
                    command,
                    error: error.clone(),
                })
            }
            action => action,
        })
        .collect()
}
/// Actions released through the engine's managed-HTTP fixture handshake at
/// `now`, the time of the event that produced them. Legacy actions pass through.
pub fn prepared(engine: &mut Engine<Okx>, now: f64) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
/// Removes the first HTTP request; managed reads also arm deadline timers.
pub fn take_http(actions: &mut Vec<Action>) -> Action {
    let i = actions
        .iter()
        .position(|a| matches!(a, Action::Http(_)))
        .expect("an HTTP request");
    actions.remove(i)
}
pub fn reply(engine: &mut Engine<Okx>, action: &Action, body: &str, now: f64) {
    let Action::Http(r) = action else {
        panic!("expected request")
    };
    engine.handle(
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
pub fn envelope(row: &str) -> String {
    format!(r#"{{"code":"0","msg":"","data":[{row}]}}"#)
}
pub fn public(options: Options) -> (Engine<Okx>, SocketId, SocketId) {
    let mut engine = engine(options);
    engine.start(1_709_209_849_123.0);
    let request = prepared(&mut engine, 1_709_209_849_123.0)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    reply(&mut engine, &request, &envelope(MARKET), 1.0);
    let request = prepared(&mut engine, 1.0)
        .into_iter()
        .find(|a| matches!(a, Action::Http(_)))
        .unwrap();
    reply(&mut engine, &request, &envelope(TICKER), 2.0);
    let sockets: Vec<_> = prepared(&mut engine, 2.0)
        .into_iter()
        .filter_map(|a| {
            if let Action::WsConnect { socket, url, .. } = a {
                Some((socket, url))
            } else {
                None
            }
        })
        .collect();
    let public = sockets
        .iter()
        .find(|(_, u)| u.ends_with("/public"))
        .unwrap()
        .0;
    let business = sockets
        .iter()
        .find(|(_, u)| u.ends_with("/business"))
        .unwrap()
        .0;
    engine.handle(3.0, Event::WsOpened { socket: public });
    engine.handle(3.0, Event::WsOpened { socket: business });
    actions(&mut engine);
    (engine, public, business)
}
pub fn ws(engine: &mut Engine<Okx>, socket: SocketId, text: &str) {
    engine.handle(4.0, Event::WsMessage { socket, text });
}
pub fn is_get(action: &Action, suffix: &str) -> bool {
    matches!(action, Action::Http(r) if r.method == Method::Get && r.url.ends_with(suffix))
}

pub fn trading_engine() -> Engine<Okx> {
    trading_engine_configure(|_| {})
}

pub fn trading_engine_configure(configure: impl FnOnce(&mut Engine<Okx>)) -> Engine<Okx> {
    trading_engine_with(
        Options {
            key: Some("test-key".into()),
            secret: Some("test-secret".into()),
            passphrase: Some("test-passphrase".into()),
            ..Options::default()
        },
        configure,
    )
}

/// A ready private account from explicit credentials, e.g. with read keys.
pub fn trading_engine_with(
    options: Options,
    configure: impl FnOnce(&mut Engine<Okx>),
) -> Engine<Okx> {
    let mut e = engine(options);
    configure(&mut e);
    e.start(0.0);
    let a = take_http(&mut prepared(&mut e, 0.0));
    reply(&mut e, &a, &envelope(MARKET), 1.0);
    let mut out = prepared(&mut e, 1.0);
    let a = take_http(&mut out);
    reply(&mut e, &a, &envelope(TICKER), 2.0);
    out.extend(prepared(&mut e, 2.0));
    let a = out
        .iter()
        .find(|a| is_get(a, "/api/v5/account/config"))
        .unwrap();
    reply(
        &mut e,
        a,
        &envelope(r#"{"posMode":"net_mode","acctLv":"2"}"#),
        3.0,
    );
    let mut pending: Vec<_> = prepared(&mut e, 3.0)
        .into_iter()
        .filter(|a| matches!(a, Action::Http(_)))
        .collect();
    while let Some(a) = pending.pop() {
        let body = if matches!(&a, Action::Http(r) if r.url.contains("account-position-risk")) {
            envelope(r#"{"balData":[{"ccy":"USDT","eq":"5000"}]}"#)
        } else {
            envelope("")
        };
        reply(&mut e, &a, &body, 4.0);
        pending.extend(
            prepared(&mut e, 4.0)
                .into_iter()
                .filter(|a| matches!(a, Action::Http(_))),
        );
    }
    e
}

/// Raw test inputs enter the same trusted local-provider setup as the registry.
#[derive(Clone, Default)]
pub struct FixtureOptions {
    pub testnet: bool,
    pub eea: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
}

pub fn try_engine(
    mut options: FixtureOptions,
) -> Result<Engine<Okx>, safe_cex_engine::signing::SignError> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SetupFieldPolicy};
    let primary = options.key.as_ref().is_some_and(|s| !s.is_empty())
        && options.secret.as_ref().is_some_and(|s| !s.is_empty());
    let setup = LocalSetup {
        primary: primary.then(|| LocalIdentity {
            key: Secret::new(options.key.take().unwrap()),
            secret: Secret::new(options.secret.take().unwrap()),
            passphrase: options.passphrase.take().map(Secret::new),
            ..Default::default()
        }),
        secondary: options
            .read_credentials
            .take()
            .map(|mut read| LocalIdentity {
                key: Secret::new(std::mem::take(&mut read.key)),
                secret: Secret::new(std::mem::take(&mut read.secret)),
                passphrase: read.passphrase.take().map(Secret::new),
                ..Default::default()
            }),
    };
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            passphrase: Some(SetupFieldPolicy::RequiredForPrivate),
            ..Default::default()
        }),
        secondary: Some(IdentitySetupPolicy {
            passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
            ..Default::default()
        }),
    };
    let (provider, identities) =
        LocalSigner::from_setup_with_policy_and_mode(setup, policy, options.public_data_only)?;
    let mut engine = safe_cex_engine::test_support::seeded_engine(Okx::new(NativeOptions {
        testnet: options.testnet,
        eea: options.eea,
        public_data_only: options.public_data_only,
        credentials: identities.primary,
        read_credentials: identities.secondary,
    }));
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    Ok(engine)
}

pub fn engine(options: FixtureOptions) -> Engine<Okx> {
    try_engine(options).expect("valid fixture provider setup")
}
