#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, PublicFeedPolicy, Response, SocketChannel,
    SocketId, TimerId,
};
use safe_cex_venue_paradex::{Options, Paradex};
pub const MARKETS: &str = r#"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"}},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"}]}"#;
pub const TICKERS: &str = r#"{"results":[{"symbol":"BTC-USD-PERP","bid":"99","ask":"101","last_traded_price":"100","mark_price":"100.5","underlying_price":"100.2","price_change_rate_24h":"0.02","funding_rate":"0.0001","volume_24h":"200","open_interest":"50"},{"symbol":"ETH-USD-PERP","bid":"9","ask":"11","last_traded_price":"10","mark_price":"10.5","volume_24h":"20"},{"symbol":"BTC-USD-250101-100000-C","last_traded_price":"4"}]}"#;
/// The synthetic Stark key the private fixtures sign with; never a real key.
pub const STARK_KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

/// Trusted local Stark setup as the registry performs it: a 66-character key
/// is the Stark scalar, any other an Ethereum key it derives from.
pub fn keyed(
    mut options: Options,
    key: &str,
) -> (
    Options,
    safe_cex_engine::signing::Provider,
    safe_cex_engine::signing::Identities,
) {
    use safe_cex_crypto::starknet;
    let ethereum = key.len() != 66;
    let scalar = if ethereum {
        starknet::derive_from_ethereum(
            key,
            safe_cex_venue_paradex::sign::ethereum_chain(options.testnet),
        )
    } else {
        starknet::parse_stark_key(key)
    }
    .unwrap();
    let policy = serde_json::from_str(
        r#"{"primary":{"secretEncoding":"hex","signingAlgorithm":"starkDigest","source":"scalarOnly"}}"#,
    )
    .unwrap();
    let (signer, identities) = safe_cex_crypto::signer::LocalSigner::from_json_with_policy(
        serde_json::json!({"primary": {"secret": safe_cex_crypto::hex(&scalar[..])}}).to_string(),
        policy,
    )
    .unwrap();
    options.credentials = identities.primary.clone();
    options.ethereum_key = ethereum;
    (options, std::sync::Arc::new(signer), identities)
}

/// The trusted local provider answering through the host, so a test can
/// inspect or replace its results. It presents the same incarnation.
pub struct Deferred(pub safe_cex_engine::signing::Provider);
impl safe_cex_engine::signing::Signer for Deferred {
    fn sign(
        &self,
        work: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.0.sign(work)
    }
    fn incarnation(&self) -> Option<&safe_cex_engine::signing::Incarnation> {
        self.0.incarnation()
    }
}

/// An unstarted engine, with trusted setup's provider when `key` is given.
pub fn engine(options: Options, key: Option<&str>) -> Engine<Paradex> {
    engine_with(options, key, false)
}
pub fn engine_with(options: Options, key: Option<&str>, deferred: bool) -> Engine<Paradex> {
    let (options, installed) = match key {
        Some(key) => {
            let (options, signer, identities) = keyed(options, key);
            let signer: safe_cex_engine::signing::Provider = if deferred {
                std::sync::Arc::new(Deferred(signer))
            } else {
                signer
            };
            (options, Some((signer, identities)))
        }
        None => (options, None),
    };
    let mut e = safe_cex_engine::test_support::seeded_engine(Paradex::new(options));
    if let Some((signer, identities)) = installed {
        e.set_signer(Some(signer)).unwrap();
        e.set_identities(identities).unwrap();
    }
    e
}

pub enum ReceiptTrace {
    Progress(u32, Vec<safe_cex_engine::OrderIdentityReceipt>),
    Http(safe_cex_engine::RequestId),
}
pub struct Harness {
    pub e: Engine<Paradex>,
    pub pending: Vec<HttpRequest>,
    pub actions: Vec<Action>,
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub receipt_trace: Vec<ReceiptTrace>,
    pub public: SocketId,
    pub now: f64,
}
impl Harness {
    pub fn empty(options: Options) -> Self {
        Self::empty_with_policy(options, PublicFeedPolicy::Direct)
    }
    /// A private account whose `key` went through trusted local setup.
    pub fn keyed(options: Options, key: &str) -> Self {
        Self::keyed_with_policy(options, key, PublicFeedPolicy::Direct)
    }
    pub fn keyed_with_policy(options: Options, key: &str, policy: PublicFeedPolicy) -> Self {
        Self::start(engine(options, Some(key)), policy)
    }
    /// As `keyed`, with every signature answered by the test through the host.
    pub fn deferred(options: Options, key: &str) -> Self {
        Self::start(
            engine_with(options, key.into(), true),
            PublicFeedPolicy::Direct,
        )
    }
    pub fn empty_with_policy(options: Options, policy: PublicFeedPolicy) -> Self {
        Self::start(engine(options, None), policy)
    }
    fn start(mut e: Engine<Paradex>, policy: PublicFeedPolicy) -> Self {
        e.set_public_feed_policy(policy).unwrap();
        e.start(1_700_000_000_000.);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            failures: vec![],
            receipt_trace: vec![],
            public: SocketId(0),
            now: 1_700_000_000_000.,
        };
        h.collect();
        h
    }
    pub fn new() -> Self {
        let mut h = Self::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", MARKETS);
        h.reply("summary", TICKERS);
        h.public = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.e.handle(h.now, Event::WsOpened { socket: h.public });
        h.collect();
        h
    }
    pub fn collect(&mut self) {
        for a in safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now) {
            match &a {
                Action::Http(r) => self.receipt_trace.push(ReceiptTrace::Http(r.id)),
                Action::Notice(Notice::CommandProgress { command, receipts }) => self
                    .receipt_trace
                    .push(ReceiptTrace::Progress(*command, receipts.clone())),
                _ => {}
            }
            match a {
                Action::Http(r) => self.pending.push(r),
                // Failures as the plain text the host renders.
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.actions.push(Action::Notice(Notice::CommandFailed {
                        command,
                        error: error.clone(),
                    }));
                    self.failures.push(error);
                }
                a => self.actions.push(a),
            }
        }
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .pending
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.pending));
        self.pending.remove(i)
    }
    pub fn respond(&mut self, r: HttpRequest, status: u16, body: &str) {
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
    pub fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, 200, body);
    }
    pub fn ws(&mut self, text: &str) {
        self.e.handle(
            self.now,
            Event::WsMessage {
                socket: self.public,
                text,
            },
        );
        self.collect();
    }
    pub fn command(&mut self, c: Command) {
        self.e.handle(self.now, Event::Command(c));
        self.collect();
    }
    pub fn timer_at(&self, at: f64) -> TimerId {
        self.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .unwrap()
    }
    pub fn reconnect(&mut self) {
        let (timer, at) = self
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms }
                    if (750. ..=1000.).contains(&(*at_ms - self.now)) =>
                {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("bounded first reconnect");
        self.now = at;
        self.e.handle(at, Event::TimerFired { timer });
        self.collect();
    }
    /// Fires the failed-read retry jittered into `[0.75, 1.0] * nominal` at
    /// the nominal delay, which is never before the jittered deadline.
    pub fn retry(&mut self, nominal: f64) {
        let timer = self
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms }
                    if (0.75 * nominal..=nominal).contains(&(*at_ms - self.now)) =>
                {
                    Some(*timer)
                }
                _ => None,
            })
            .expect("jittered retry timer");
        self.now += nominal;
        self.e.handle(self.now, Event::TimerFired { timer });
        self.collect();
    }
    pub fn timer(&mut self, at: f64) {
        let timer = self.timer_at(at);
        self.now = at;
        self.e.handle(self.now, Event::TimerFired { timer });
        self.collect();
    }
    pub fn sent(&self) -> Vec<&str> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    pub fn result(&self, id: u32) -> Result<&str, &str> {
        self.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                    Some(Ok(result.as_str()))
                }
                Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                    Some(Err(error.message.as_str()))
                }
                _ => None,
            })
            .unwrap()
    }
}
