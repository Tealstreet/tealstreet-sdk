#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_lighter::Lighter;
pub const NOW: f64 = 1_700_000_000_000.0;
pub const META: &str = r#"{"code":200,"order_book_details":[{"market_id":1,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":3,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250},{"market_id":2,"symbol":"BTC","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":1,"min_base_amount":"0.0001","min_quote_amount":"1","min_initial_margin_fraction":500,"maintenance_margin_fraction":250}]}"#;
pub const ACCOUNT: &str = r#"{"code":200,"accounts":[{"index":718895,"collateral":"1000","total_asset_value":"1000","available_balance":"900","positions":[{"market_id":1,"position":"2","sign":1,"avg_entry_price":"100","unrealized_pnl":"0","realized_pnl":"3","initial_margin_fraction":1000,"margin_mode":0,"liquidation_price":"50"}]}]}"#;
pub const OPEN: &str = r#"{"code":200,"orders":[{"order_index":"42","market_id":1,"status":"open","type":"limit","time_in_force":1,"is_ask":false,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2","price":"90"}]}"#;
pub fn options() -> Options {
    Options {
        application_id: Some("718895".into()),
        api_key_index: Some("1".into()),
        secret: Some(format!("01{}", "00".repeat(39))),
        ..Options::default()
    }
}
pub fn reply(e: &mut Engine<Lighter>, r: &HttpRequest, status: u16, body: &str) {
    reply_at(e, NOW, r, status, body)
}
pub fn reply_at(e: &mut Engine<Lighter>, now: f64, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
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
pub fn message(e: &mut Engine<Lighter>, socket: SocketId, now: f64, text: &str) {
    e.handle(now, Event::WsMessage { socket, text });
}
pub fn requests(e: &mut Engine<Lighter>) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
pub fn ready() -> (Engine<Lighter>, SocketId) {
    ready_with(options())
}
pub fn ready_with(options: Options) -> (Engine<Lighter>, SocketId) {
    ready_with_meta(options, META)
}
pub fn ready_with_meta(options: Options, meta: &str) -> (Engine<Lighter>, SocketId) {
    let (e, socket, _) = ready_tracked_meta(options, meta);
    (e, socket)
}
pub type Timers = Vec<(TimerId, f64)>;
pub fn track(timers: &mut Timers, action: &Action) {
    match action {
        Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
        Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
        _ => {}
    }
}
pub fn drain(e: &mut Engine<Lighter>, timers: &mut Timers) -> Vec<Action> {
    let actions = safe_cex_engine::test_support::prepared_actions(e, NOW);
    actions.iter().for_each(|a| track(timers, a));
    actions
}
pub fn advance(e: &mut Engine<Lighter>, timers: &mut Timers, until: f64) -> Vec<Action> {
    let mut out = drain(e, timers);
    while let Some((timer, at)) = timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        out.extend(drain(e, timers));
    }
    out
}
pub fn ready_tracked(options: Options) -> (Engine<Lighter>, SocketId, Timers) {
    ready_tracked_meta(options, META)
}
pub fn ready_tracked_meta(options: Options, meta: &str) -> (Engine<Lighter>, SocketId, Timers) {
    ready_engine_meta(engine(options), meta)
}
pub fn ready_engine_meta(
    mut e: Engine<Lighter>,
    meta: &str,
) -> (Engine<Lighter>, SocketId, Timers) {
    let mut timers = Timers::new();
    e.start(NOW);
    let mut private = None;
    for _ in 0..30 {
        let mut actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        actions.sort_by_key(|a| !matches!(a, Action::Http(r) if r.url.contains("exchangeStats")));
        if actions.is_empty() {
            break;
        }
        for action in actions {
            track(&mut timers, &action);
            match action {
                Action::Http(r) => {
                    let body = if r.url.contains("orderBookDetails") || r.url.contains("orderBooks")
                    {
                        meta
                    } else if r.url.contains("exchangeStats") {
                        r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","mark_price":"100","index_price":"100"},{"market_id":2,"last_trade_price":"200","mark_price":"200","index_price":"200"}]}"#
                    } else if r.url.contains("accountActiveOrders") {
                        OPEN
                    } else if r.url.contains("/account?") {
                        ACCOUNT
                    } else {
                        panic!("unhandled bootstrap {}", r.url)
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::WsConnect { socket, .. } => {
                    private = Some(socket);
                    e.handle(NOW, Event::WsOpened { socket });
                }
                _ => {}
            }
        }
    }
    (e, private.expect("private socket"), timers)
}
pub fn fill_count(frame: &[u8]) -> u32 {
    let mut at = 16;
    while at + 8 <= frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let size = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            return u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + size;
    }
    0
}

/// A failed-read retry delay lies in the engine's jittered `[0.75, 1.0] * nominal`.
pub fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}

pub fn engine(mut options: Options) -> Engine<Lighter> {
    use safe_cex_crypto::signer::{
        LighterPrincipal, LocalIdentity, LocalSetup, LocalSigner, Secret,
    };
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    let key = options.key.take();
    options.l1_address = key.clone();
    let secret = options.secret.take();
    let setup = LocalSetup {
        primary: secret.map(|secret| {
            let mut primary = LocalIdentity {
                secret: Secret::new(secret),
                ..Default::default()
            };
            if let Some(key) = key {
                primary.key = Secret::new(key);
            }
            primary
        }),
        secondary: None,
    };
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Hex),
            signing_algorithm: Some(Algorithm::SchnorrDigest),
            ..Default::default()
        }),
        ..Default::default()
    };
    let index = options
        .api_key_index
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let (signer, identities, _) = LocalSigner::from_setup_with_lighter_principal(
        setup,
        policy,
        options.public_data_only,
        LighterPrincipal::new(index).unwrap(),
    )
    .unwrap();
    options.identity = identities.primary.clone();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(options.into()));
    engine
        .set_signer(Some(std::sync::Arc::new(signer)))
        .unwrap();
    engine.set_identities(identities).unwrap();
    engine
}

#[derive(Clone, Debug)]
pub struct Options {
    pub testnet: bool,
    pub robinhood: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub identity: Option<safe_cex_engine::signing::Identity>,
    pub l1_address: Option<String>,
    pub remote_signer: bool,
    pub signer_provider: Option<String>,
    pub application_id: Option<String>,
    pub api_key_index: Option<String>,
    pub integrator_account_index: Option<u64>,
    pub stop_slippage_pct: f64,
}
impl From<Options> for safe_cex_venue_lighter::Options {
    fn from(options: Options) -> Self {
        Self {
            testnet: options.testnet,
            robinhood: options.robinhood,
            public_data_only: options.public_data_only,
            identity: options.identity,
            l1_address: options.l1_address.or(options.key),
            remote_signer: options.remote_signer,
            signer_provider: options.signer_provider,
            application_id: options.application_id,
            api_key_index: options.api_key_index,
            integrator_account_index: options.integrator_account_index,
            stop_slippage_pct: options.stop_slippage_pct,
        }
    }
}

impl Default for Options {
    fn default() -> Self { Self { testnet: false, robinhood: false, public_data_only: false, key: None, secret: None, identity: None, l1_address: None, remote_signer: false, signer_provider: None, application_id: None, api_key_index: None, integrator_account_index: None, stop_slippage_pct: 1.0 } }
}
