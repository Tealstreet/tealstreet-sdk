#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketContext,
    SocketId, TimerId,
};
use safe_cex_venue_bitget::{BitgetFutures, Options as NativeOptions, Product};
type Options = FixtureOptions;

// Bitget Classic contracts/tickers examples, reduced to the fields we consume.
pub const CONTRACTS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}]}"#;
pub const TICKERS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","bidPr":"99.9","askPr":"100.1","lastPr":"100","indexPrice":"99","changeUtc24h":"0.01","baseVolume":"1000","quoteVolume":"100000","fundingRate":"0.0001"}]}"#;

pub struct Harness {
    pub engine: Engine<BitgetFutures>,
    pub now: f64,
    pub requests: Vec<HttpRequest>,
    pub sockets: Vec<(SocketId, String)>,
    pub socket_contexts: Vec<(SocketId, String, Option<SocketContext>)>,
    pub sent: Vec<(SocketId, String)>,
    pub closed: Vec<SocketId>,
    pub timers: Vec<(TimerId, f64)>,
    pub done: Vec<(u32, Result<String, String>)>,
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub logs: Vec<String>,
    pub command_errors: Vec<(u32, String)>,
    pub command_notices: Vec<(u32, &'static str)>,
    pub progress: Vec<(u32, Vec<safe_cex_engine::OrderIdentityReceipt>)>,
    pub progress_at_request: Vec<(RequestId, usize)>,
    /// Set by tests that expect startup to fail; any other failure panics.
    pub expect_startup_failure: bool,
    pub startup_failures: Vec<safe_cex_engine::VenueError>,
    pub deactivations: usize,
}
impl Harness {
    pub fn started(options: Options) -> Self {
        Self::started_expecting(options, false)
    }
    pub fn started_expecting(options: Options, expect_startup_failure: bool) -> Self {
        Self::started_configure(options, expect_startup_failure, |_| {})
    }
    pub fn started_configure(
        options: Options,
        expect_startup_failure: bool,
        configure: impl FnOnce(&mut Engine<BitgetFutures>),
    ) -> Self {
        let mut engine = engine(options);
        configure(&mut engine);
        let mut h = Self {
            engine,
            now: 1_000_000.0,
            requests: vec![],
            sockets: vec![],
            socket_contexts: vec![],
            sent: vec![],
            closed: vec![],
            timers: vec![],
            done: vec![],
            failures: vec![],
            logs: vec![],
            command_errors: vec![],
            command_notices: vec![],
            progress: vec![],
            progress_at_request: vec![],
            expect_startup_failure,
            startup_failures: vec![],
            deactivations: 0,
        };
        h.engine.start(h.now);
        h.collect();
        h
    }
    pub fn public() -> Self {
        let mut h = Self::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.load_public();
        h
    }
    pub fn load_public(&mut self) {
        self.respond("/contracts", 200, CONTRACTS);
        self.respond("/tickers", 200, TICKERS);
        for (socket, _) in self.sockets.clone() {
            self.event(Event::WsOpened { socket });
        }
    }
    pub fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::ManagedHttp(_) => {
                    panic!("managed HTTP requires the prepared test-driver handshake")
                }
                Action::Sign { .. }
                | Action::SignedOutbound { .. }
                | Action::SignPayload { .. } => {
                    panic!("delegated signing is unsupported in the bitget test harness")
                }
                Action::Http(r) => {
                    self.progress_at_request.push((r.id, self.progress.len()));
                    self.requests.push(r);
                }
                Action::WsConnect {
                    socket,
                    url,
                    context,
                    ..
                } => {
                    self.socket_contexts.push((socket, url.clone(), context));
                    self.sockets.push((socket, url))
                }
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::AccountAuth { .. }) => {}
                Action::Notice(Notice::CommandProgress { command, receipts }) => {
                    self.progress.push((command, receipts))
                }
                Action::Notice(Notice::CommandError { command, message }) => {
                    self.logs.push(message.clone());
                    self.command_errors.push((command, message));
                    self.command_notices.push((command, "error"));
                }
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.command_notices.push((command, "done"));
                    self.done.push((command, Ok(result)))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())));
                    self.failures.push(error);
                }
                Action::Notice(Notice::StartupFailed { error }) => {
                    assert!(
                        self.expect_startup_failure,
                        "unexpected Bitget startup failure: {error}"
                    );
                    self.startup_failures.push(error);
                }
                Action::Notice(Notice::Event(event)) => self.logs.push(event.message),
                Action::Notice(Notice::Deactivate) => {
                    assert!(
                        self.expect_startup_failure,
                        "unexpected Bitget account deactivation"
                    );
                    self.deactivations += 1;
                }
            }
        }
    }
    pub fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    pub fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("no {path}: {:?}", self.requests));
        self.requests.remove(i)
    }
    pub fn reply(&mut self, request: RequestId, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    pub fn respond(&mut self, path: &str, status: u16, body: &str) {
        let r = self.take(path);
        self.reply(r.id, status, body);
    }
    pub fn message(&mut self, text: &str) {
        self.event(Event::WsMessage {
            socket: self.sockets[0].0,
            text,
        });
    }
    pub fn book(&mut self) {
        self.command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        });
    }
}

pub const POSITION: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","openPriceAvg":"90","markPrice":"100","leverage":"20","unrealizedPL":"10","liquidationPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","uTime":"100"}]}"#;
pub const ACCOUNT: &str = r#"{"code":"00000","data":[{"marginCoin":"USDT","unionTotalMargin":"1010","unionAvailable":"900","unrealizedPL":"10"}]}"#;
pub const OPEN_ORDER: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","clientOid":"caller"}],"endId":""}}"#;

impl Harness {
    pub fn private_started() -> Self {
        let mut h = Self::started(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Options::default()
        });
        h.load_public();
        h
    }
    pub fn classic() -> Self {
        let mut h = Self::private_started();
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
        );
        h.respond("/all-position", 200, POSITION);
        h.respond("/account/accounts", 200, ACCOUNT);
        h.orders(OPEN_ORDER);
        h.login();
        h
    }
    pub fn orders(&mut self, normal: &str) {
        self.respond("/orders-pending", 200, normal);
        self.respond(
            "planType=profit_loss",
            200,
            r#"{"code":"00000","data":{"entrustedList":[]}}"#,
        );
        self.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    }
    pub fn login(&mut self) {
        let socket = self
            .sockets
            .iter()
            .find(|(_, u)| u.ends_with("/private"))
            .unwrap()
            .0;
        self.event(Event::WsOpened { socket });
        self.private_message(r#"{"event":"login","code":0,"msg":"","connId":"extra-field"}"#);
    }
    pub fn private_message(&mut self, text: &str) {
        let socket = self
            .sockets
            .iter()
            .rev()
            .find(|(_, u)| u.ends_with("/private"))
            .unwrap()
            .0;
        self.event(Event::WsMessage { socket, text });
    }
    pub fn timer_after(&mut self, delay: f64) {
        let (timer, at) = self
            .timers
            .iter()
            .copied()
            .find(|(_, at)| (*at - self.now - delay).abs() < 0.01)
            .unwrap_or_else(|| panic!("no timer after {delay} at {}: {:?}", self.now, self.timers));
        self.timers.retain(|(t, _)| *t != timer);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

#[derive(Clone, Default)]
pub struct FixtureOptions {
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub product: Product,
}

pub fn try_engine(
    mut options: FixtureOptions,
) -> Result<Engine<BitgetFutures>, safe_cex_engine::signing::SignError> {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SetupFieldPolicy};
    let primary =
        if options.key.is_some() || options.secret.is_some() || options.passphrase.is_some() {
            Some(LocalIdentity {
                key: Secret::new(options.key.take().unwrap_or_default()),
                secret: Secret::new(options.secret.take().unwrap_or_default()),
                passphrase: options.passphrase.take().map(Secret::new),
                ..Default::default()
            })
        } else {
            None
        };
    let secondary = options
        .read_credentials
        .take()
        .map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        });
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
    let (provider, identities) = LocalSigner::from_setup_with_policy_and_mode(
        LocalSetup { primary, secondary },
        policy,
        options.public_data_only,
    )?;
    let mut engine =
        safe_cex_engine::test_support::seeded_engine(BitgetFutures::new(NativeOptions {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            testnet: options.testnet,
            public_data_only: options.public_data_only,
            product: options.product,
        }));
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    Ok(engine)
}

pub fn engine(options: FixtureOptions) -> Engine<BitgetFutures> {
    try_engine(options).unwrap()
}
