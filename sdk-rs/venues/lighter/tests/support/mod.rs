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
pub fn reply<V: safe_cex_engine::Venue>(
    e: &mut Engine<V>,
    r: &HttpRequest,
    status: u16,
    body: &str,
) {
    reply_at(e, NOW, r, status, body)
}
pub fn reply_at<V: safe_cex_engine::Venue>(
    e: &mut Engine<V>,
    now: f64,
    r: &HttpRequest,
    status: u16,
    body: &str,
) {
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
    requests_at(e, NOW)
}
pub fn requests_at(e: &mut Engine<Lighter>, now: f64) -> Vec<HttpRequest> {
    safe_cex_engine::test_support::prepared_actions(e, now)
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
    drain_at(e, timers, NOW)
}
pub fn drain_at(e: &mut Engine<Lighter>, timers: &mut Timers, now: f64) -> Vec<Action> {
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
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
        out.extend(drain_at(e, timers, at));
    }
    out
}
pub fn ready_tracked(options: Options) -> (Engine<Lighter>, SocketId, Timers) {
    ready_tracked_meta(options, META)
}
pub fn ready_tracked_meta(options: Options, meta: &str) -> (Engine<Lighter>, SocketId, Timers) {
    ready_engine_meta(engine(options), meta)
}
pub fn ready_engine_meta<V: safe_cex_engine::Venue>(
    mut e: Engine<V>,
    meta: &str,
) -> (Engine<V>, SocketId, Timers) {
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

pub fn engine(options: Options) -> Engine<Lighter> {
    try_engine(options).expect("valid trusted fixture setup")
}
pub fn try_engine(
    options: Options,
) -> Result<Engine<Lighter>, safe_cex_engine::signing::SignError> {
    try_engine_with(options, |venue| venue)
}
pub fn try_engine_with<V: safe_cex_engine::Venue>(
    mut options: Options,
    wrap: impl FnOnce(Lighter) -> V,
) -> Result<Engine<V>, safe_cex_engine::signing::SignError> {
    use safe_cex_crypto::signer::{
        LighterPrincipal, LocalIdentity, LocalSetup, LocalSigner, Secret,
    };
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    if !options.public_data_only
        && options
            .signer_provider
            .as_deref()
            .is_some_and(|provider| !provider.is_empty() && provider != "tealstreet-sdk")
    {
        return Err(safe_cex_engine::signing::SignError::Unsupported);
    }
    let key = options.key.take();
    if options.l1_address.is_none() {
        options.l1_address = key.as_deref().and_then(|key| {
            let start = key.find("0x")?;
            let address = key.get(start..start + 42)?;
            (address[2..].bytes().all(|b| b.is_ascii_hexdigit())
                && key
                    .as_bytes()
                    .get(start + 42)
                    .is_none_or(|b| !b.is_ascii_hexdigit()))
            .then(|| address.to_owned())
        });
    }
    let secret = options.secret.take();
    let primary = if key.is_some() || secret.is_some() {
        let mut primary = LocalIdentity::default();
        if let Some(key) = key {
            primary.key = Secret::new(key);
        }
        if let Some(secret) = secret {
            primary.secret = Secret::new(secret);
        }
        Some(primary)
    } else {
        None
    };
    let setup = LocalSetup {
        primary,
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
    let index = if options.public_data_only || setup.primary.is_none() {
        0
    } else {
        options
            .api_key_index
            .as_deref()
            .and_then(|s| s.parse().ok())
            .ok_or(safe_cex_engine::signing::SignError::InvalidRequest)?
    };
    let (signer, identities, _) = LocalSigner::from_setup_with_lighter_principal(
        setup,
        policy,
        options.public_data_only,
        LighterPrincipal::new(index)?,
    )?;
    options.identity = identities.primary.clone();
    let mut engine =
        safe_cex_engine::test_support::seeded_engine(wrap(Lighter::new(options.into())));
    engine
        .set_signer(Some(std::sync::Arc::new(signer)))
        .unwrap();
    engine.set_identities(identities).unwrap();
    Ok(engine)
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
    fn default() -> Self {
        Self {
            testnet: false,
            robinhood: false,
            public_data_only: false,
            key: None,
            secret: None,
            identity: None,
            l1_address: None,
            remote_signer: false,
            signer_provider: None,
            application_id: None,
            api_key_index: None,
            integrator_account_index: None,
            stop_slippage_pct: 1.0,
        }
    }
}
