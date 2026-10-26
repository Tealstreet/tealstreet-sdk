#![allow(dead_code)]
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_orderly::{Options, Orderly};
pub type Driver = Engine<Orderly>;
pub const MARKETS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","base_tick":0.001,"quote_tick":0.1,"base_min":0.001,"base_max":20,"base_imr":0.01},{"symbol":"PERP_ETH_USDC","base_tick":0.001,"quote_tick":0.01,"base_min":0.001,"base_max":100,"base_imr":0.02}]}}"#;
pub const TICKERS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","24h_open":50000,"24h_close":51000,"index_price":50999,"mark_price":51000,"24h_volume":10,"24h_amount":500000},{"symbol":"PERP_ETH_USDC","24h_open":2000,"24h_close":2100,"mark_price":2100}]}}"#;
pub const ORDER: &str = r#"{"order_id":9007199254740993,"symbol":"PERP_BTC_USDC","type":"LIMIT","side":"BUY","status":"NEW","price":50000,"quantity":1,"total_executed_quantity":0,"executed":0,"visible_quantity":1,"reduce_only":false,"margin_mode":"CROSS","client_order_id":"caller-42"}"#;
pub const POSITIONS: &str = r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","position_qty":1,"average_open_price":50000,"mark_price":51000,"margin_mode":"CROSS","leverage":10},{"symbol":"PERP_BTC_USDC","position_qty":-0.5,"average_open_price":52000,"mark_price":51000,"margin_mode":"ISOLATED","leverage":5,"margin":6000}],"total_collateral_value":10000,"free_collateral":7000}}"#;
/// Prepared actions, firing engine wakes (a released Plan pause or a woken
/// Preflight) already due by the fixtures' 2000 ms event clock.
pub fn actions(e: &mut Driver) -> Vec<Action> {
    settled(e, 2000.0)
}
/// Tracks armed timers so tests can advance time instead of firing timer IDs.
pub struct Clock {
    pub now: f64,
    timers: Vec<(TimerId, f64)>,
}
impl Clock {
    pub fn new(now: f64) -> Self {
        Self {
            now,
            timers: vec![],
        }
    }
    pub fn track(&mut self, out: &[Action]) {
        for action in out {
            match action {
                Action::SetTimer { timer, at_ms } => self.timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| t != timer),
                _ => {}
            }
        }
    }
    pub fn drain(&mut self, e: &mut Driver) -> Vec<Action> {
        let out = safe_cex_engine::test_support::prepared_actions(e, self.now);
        self.track(&out);
        out
    }
    pub fn handle(&mut self, e: &mut Driver, event: Event<'_>) -> Vec<Action> {
        e.handle(self.now, event);
        self.drain(e)
    }
    pub fn advance(&mut self, e: &mut Driver, until: f64) -> Vec<(f64, Action)> {
        let mut fired = vec![];
        while self.timers.iter().any(|(_, at)| *at <= until) {
            fired.extend(self.next(e));
        }
        self.now = until;
        fired
    }
    /// Fires only the earliest armed timer.
    pub fn next(&mut self, e: &mut Driver) -> Vec<(f64, Action)> {
        let Some((timer, at)) = self
            .timers
            .iter()
            .copied()
            .min_by(|a, b| a.1.total_cmp(&b.1))
        else {
            return vec![];
        };
        self.timers.retain(|(t, _)| *t != timer);
        self.now = self.now.max(at);
        let out = self.handle(e, Event::TimerFired { timer });
        out.into_iter().map(|a| (self.now, a)).collect()
    }
}
pub fn requests(e: &mut Driver) -> Vec<HttpRequest> {
    actions(e)
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
/// Prepared actions at `now`, firing every timer already due by `now` (an
/// engine Plan wake after a released pause), until none is left.
pub fn settled(e: &mut Driver, now: f64) -> Vec<Action> {
    let mut all = vec![];
    loop {
        let out = safe_cex_engine::test_support::prepared_actions(e, now);
        let due: Vec<_> = out
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms <= now => Some(*timer),
                _ => None,
            })
            .collect();
        all.extend(out);
        if due.is_empty() {
            return all;
        }
        for timer in due {
            e.handle(now, Event::TimerFired { timer });
        }
    }
}
/// The HTTP requests among `settled(e, now)`.
pub fn settled_requests(e: &mut Driver, now: f64) -> Vec<HttpRequest> {
    settled(e, now)
        .into_iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect()
}
pub fn reply(e: &mut Driver, r: &HttpRequest, status: u16, body: &str) {
    e.handle(
        1000.0,
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
pub fn ws(e: &mut Driver, socket: SocketId, text: &str) {
    e.handle(2000.0, Event::WsMessage { socket, text });
}
/// The fixture account's presented key and Ed25519 seed.
pub const KEY: &str = "ed25519:11111111111111111111111111111111";
pub const SEED: [u8; 32] = [7; 32];
pub fn options() -> Options {
    Options {
        application_id: Some("test-account".into()),
        ..Default::default()
    }
}
/// Trusted setup as the registry does it: the provider holds the declared
/// base58 seed and the venue receives only the refs.
pub fn trusted(
    mut options: Options,
) -> (
    Option<std::sync::Arc<safe_cex_crypto::signer::LocalSigner>>,
    Options,
) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
    };
    // The fixture account: a keyed one names its applicationId.
    if options.public_data_only || options.application_id.is_none() {
        return (None, options);
    }
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new(KEY.into()),
                secret: Secret::new(bs58::encode(SEED).into_string()),
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base58),
                signing_algorithm: Some(Algorithm::Ed25519),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    options.identities = identities;
    (Some(std::sync::Arc::new(signer)), options)
}
/// A seeded engine over `options`, with the provider installed when keyed.
pub fn engine(options: Options) -> Driver {
    let (signer, options) = trusted(options);
    let identities = options.identities.clone();
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(options));
    if let Some(signer) = signer {
        e.set_signer(Some(signer)).unwrap();
        e.set_identities(identities).unwrap();
    }
    e
}
pub fn ready() -> (Driver, SocketId) {
    ready_with(ORDER, "[]", POSITIONS)
}
pub fn ready_with(order: &str, algos: &str, positions: &str) -> (Driver, SocketId) {
    let (e, socket, _, _) = ready_full(order, algos, positions);
    (e, socket)
}
/// Bootstraps like `ready_with` and returns the messages logged on the way.
pub fn ready_with_logs(
    order: &str,
    algos: &str,
    positions: &str,
) -> (Driver, SocketId, Vec<String>) {
    let (e, socket, _, logs) = ready_full(order, algos, positions);
    (e, socket, logs)
}
/// Like `ready_with`, also returning the account poll tick timer.
pub fn ready_ticking(order: &str, algos: &str, positions: &str) -> (Driver, SocketId, TimerId) {
    let (e, socket, tick, _) = ready_full(order, algos, positions);
    (e, socket, tick.unwrap())
}
/// The local provider behind a switch: once set, every signature is refused.
pub struct Refusable {
    inner: std::sync::Arc<safe_cex_crypto::signer::LocalSigner>,
    pub refuse: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl safe_cex_engine::signing::Signer for Refusable {
    fn sign(
        &self,
        request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.inner.sign(request)
    }
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(
        &self,
        request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>>
    {
        if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
            return Some(Err(safe_cex_engine::signing::SignError::Refused));
        }
        self.inner.sign_ready(request)
    }
    fn can_sign(
        &self,
        key: &safe_cex_engine::signing::KeyRef,
        algorithm: safe_cex_engine::signing::Algorithm,
    ) -> safe_cex_engine::signing::Capability {
        self.inner.can_sign(key, algorithm)
    }
}
/// A keyed engine whose provider a test can switch to refusing.
pub fn refusable_engine() -> (Driver, std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let (signer, options) = trusted(options());
    let refuse = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let identities = options.identities.clone();
    let mut e = safe_cex_engine::test_support::seeded_engine(Orderly::new(options));
    e.set_signer(Some(std::sync::Arc::new(Refusable {
        inner: signer.unwrap(),
        refuse: refuse.clone(),
    })))
    .unwrap();
    e.set_identities(identities).unwrap();
    (e, refuse)
}
/// `ready()` over a provider that a test can switch to refusing.
pub fn ready_refusable() -> (
    Driver,
    SocketId,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let (e, refuse) = refusable_engine();
    let (e, socket, _, _) = bootstrap(e, ORDER, "[]", POSITIONS);
    (e, socket, refuse)
}
fn ready_full(
    order: &str,
    algos: &str,
    positions: &str,
) -> (Driver, SocketId, Option<TimerId>, Vec<String>) {
    bootstrap(engine(options()), order, algos, positions)
}
fn bootstrap(
    mut e: Driver,
    order: &str,
    algos: &str,
    positions: &str,
) -> (Driver, SocketId, Option<TimerId>, Vec<String>) {
    e.start(1000.0);
    let mut private = None;
    let mut logs = vec![];
    let mut tick = None;
    for _ in 0..10 {
        let out = actions(&mut e);
        if out.is_empty() {
            break;
        }
        for a in out {
            match a {
                Action::Http(r) => {
                    let body = account_body(&r.url, order, algos, positions);
                    reply(&mut e, &r, 200, &body);
                }
                Action::WsConnect { socket, url, .. } if url.contains("private") => {
                    private = Some(socket)
                }
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => logs.push(message),
                Action::SetTimer {
                    timer,
                    at_ms: 3000.0,
                } => tick = Some(timer),
                _ => {}
            }
        }
    }
    let socket = private.unwrap();
    e.handle(1000.0, Event::WsOpened { socket });
    actions(&mut e);
    ws(&mut e, socket, r#"{"event":"auth","success":true}"#);
    (e, socket, tick, logs)
}
pub fn account_body(url: &str, order: &str, algos: &str, positions: &str) -> String {
    if url.ends_with("/v1/client/info") {
        r#"{"success":true,"data":{"max_leverage":20}}"#.into()
    } else if url.ends_with("/v1/public/info") {
        MARKETS.into()
    } else if url.ends_with("/v1/public/futures") {
        TICKERS.into()
    } else if url.ends_with("/v1/client/margin_modes") {
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDC","default_margin_mode":"CROSS"},{"symbol":"PERP_ETH_USDC","default_margin_mode":"ISOLATED"}]}}"#.into()
    } else if url.ends_with("/v1/positions") {
        positions.into()
    } else if url.contains("/v1/algo/orders") {
        format!(
            r#"{{"success":true,"data":{{"rows":{algos},"meta":{{"total":{}}}}}}}"#,
            if algos == "[]" { 0 } else { 1 }
        )
    } else {
        format!(r#"{{"success":true,"data":{{"rows":[{order}],"meta":{{"total":1}}}}}}"#)
    }
}
pub fn fill_amounts(e: &mut Driver) -> Vec<f64> {
    let Some(bytes) = e.take_frame(2000.0) else {
        return vec![];
    };
    let mut at = 16;
    let count = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
    for _ in 0..count {
        let kind = u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        if kind == 11 {
            let n = u32::from_le_bytes(bytes[start..start + 4].try_into().unwrap()) as usize;
            let data = (start + 4 + n * 12 + 7) & !7;
            return (0..n)
                .map(|i| {
                    f64::from_le_bytes(
                        bytes[data + i * 16 + 8..data + i * 16 + 16]
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect();
        }
        at = start + len;
    }
    vec![]
}
