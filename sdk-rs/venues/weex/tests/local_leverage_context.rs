//! Native local leverage refusals require the genuine command's Read authority.
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Event, Method, Notice, Response, SocketId, TimerId,
    Venue, VenueError,
};
use safe_cex_venue_weex::{Pending, WeexFutures};
use support::setup::{self, Options};

const NOW: f64 = 1_000.;
const AT: f64 = 1_002.;
const COMMAND: u32 = 42;
const PROBE: &str = "__retained_read_probe__";
const RETIRE: &str = "__retire_read_probe__";

// Sentinel commands retain genuine engine roots solely for capacity controls.
// All tested FetchLeverage commands and native callbacks delegate unchanged.
struct Probe {
    venue: WeexFutures,
    roots: Vec<Context>,
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == PROBE => {
                if let Ok(root) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.roots.push(root);
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == [RETIRE] => {
                let root = self.roots.remove(0);
                cx.fail_context(&root, "retained fixture peer retired")
                    .unwrap();
            }
            _ => self.venue.on_command(cx, command),
        }
    }
}
fn market() -> Market {
    Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn engine(private: bool) -> Engine<Probe> {
    let mut e = setup::with_venue(
        Options {
            key: private.then(|| "key".into()),
            secret: private.then(|| "secret".into()),
            passphrase: private.then(|| "passphrase".into()),
            ..Default::default()
        },
        |venue| Probe {
            venue,
            roots: vec![],
        },
    );
    e.store_mut().set_markets(vec![market()]);
    e.store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 13.);
    e
}
fn actions(e: &mut Engine<Probe>, at: f64, command: Command) -> Vec<Action> {
    e.handle(at, Event::Command(command));
    safe_cex_engine::test_support::prepared_actions(e, at)
}
fn leverage(e: &mut Engine<Probe>, at: f64, command: u32, symbol: &str) -> Vec<Action> {
    actions(
        e,
        at,
        Command::FetchLeverage {
            command,
            symbol: symbol.into(),
        },
    )
}
fn failure(actions: &[Action], owner: u32) -> &VenueError {
    assert!(
        matches!(actions, [Action::Notice(Notice::CommandFailed { command, .. })] if *command == owner),
        "exactly the correlated failure, no transport/timer/signer/auth effects: {actions:?}"
    );
    let Action::Notice(Notice::CommandFailed { error, .. }) = &actions[0] else {
        unreachable!()
    };
    error
}
fn pause(e: &mut Engine<Probe>) {
    let a = actions(
        e,
        NOW,
        Command::FetchOrderBook {
            command: 900,
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
    );
    let requests: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "native unsigned pause source");
    assert_eq!(requests[0].method, Method::Get);
    assert!(requests[0].url.contains("/depth?symbol=BTCUSDT&limit=1"));
    assert!(requests[0].headers.is_empty());
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: requests[0].id,
            response: Response {
                status: 429,
                headers: "Retry-After: 3\r\n",
                body: "busy",
            },
        },
    );
    let a = safe_cex_engine::test_support::prepared_actions(e, NOW + 1.);
    assert_eq!(
        a.iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 900, .. })
            ))
            .count(),
        1
    );
}
#[derive(Clone, Copy)]
enum Refusal {
    UnknownMarket,
    Paused,
    Credentials,
}
impl Refusal {
    fn symbol(self) -> &'static str {
        if matches!(self, Self::UnknownMarket) {
            "MISSING"
        } else {
            "BTCUSDT"
        }
    }
    fn error(self) -> VenueError {
        VenueError::new(match self {
            Self::UnknownMarket => "unknown market MISSING",
            Self::Paused => "WEEX account reads are rate limited",
            Self::Credentials => "WEEX leverage requires credentials",
        })
    }
}
fn native(refusal: Refusal, private: bool, paused: bool) {
    let mut e = engine(private);
    let before = e.store().setting("BTCUSDT");
    if paused {
        pause(&mut e);
    }
    for _ in 0..3 {
        let a = leverage(&mut e, AT, COMMAND, refusal.symbol());
        assert_eq!(failure(&a, COMMAND), &refusal.error());
        assert_eq!(e.store().setting("BTCUSDT"), before);
    }
}
#[test]
fn unknown_market_wins_over_pause_and_missing_credentials_with_native_fields() {
    for private in [false, true] {
        native(Refusal::UnknownMarket, private, true);
        native(Refusal::UnknownMarket, private, false);
    }
}
#[test]
fn native_pause_wins_over_missing_credentials_and_allows_id_reuse() {
    for private in [false, true] {
        native(Refusal::Paused, private, true);
    }
}
#[test]
fn missing_credentials_keeps_native_fields_and_settings_without_io() {
    native(Refusal::Credentials, false, false);
}
#[test]
fn pause_equality_reaches_the_native_missing_credentials_refusal() {
    let mut e = engine(false);
    pause(&mut e);
    assert_eq!(
        failure(&leverage(&mut e, 4_000., COMMAND, "BTCUSDT"), COMMAND),
        &Refusal::Paused.error()
    );
    assert_eq!(
        failure(&leverage(&mut e, 4_001., COMMAND, "BTCUSDT"), COMMAND),
        &Refusal::Credentials.error()
    );
}
fn invalid_clock(refusal: Refusal) {
    let mut e = engine(false);
    let before = e.store().setting("BTCUSDT");
    let a = leverage(&mut e, f64::NAN, COMMAND, refusal.symbol());
    assert_eq!(
        failure(&a, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
    assert_eq!(e.store().setting("BTCUSDT"), before);
}
#[test]
fn unknown_market_cannot_bypass_an_unstamped_invalid_clock() {
    invalid_clock(Refusal::UnknownMarket);
}
#[test]
fn missing_credentials_cannot_bypass_an_unstamped_invalid_clock() {
    invalid_clock(Refusal::Credentials);
}
fn retain(e: &mut Engine<Probe>, owner: u32) -> Vec<Action> {
    actions(
        e,
        AT,
        Command::FetchOrderBook {
            command: owner,
            symbol: PROBE.into(),
            limit: None,
        },
    )
}
fn fill(e: &mut Engine<Probe>) {
    for owner in 1_000..1_256 {
        assert!(
            retain(e, owner).is_empty(),
            "a genuine root with no physical request"
        );
    }
}
fn overflow(e: &mut Engine<Probe>) -> VenueError {
    let a = retain(e, 2_000);
    let error = failure(&a, 2_000).clone();
    assert_eq!(
        error,
        safe_cex_engine::request_context::Error::Capacity.read_venue_error()
    );
    error
}
fn retire(e: &mut Engine<Probe>, owner: u32) {
    let a = actions(
        e,
        AT,
        Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        },
    );
    assert_eq!(
        failure(&a, owner),
        &VenueError::new("retained fixture peer retired")
    );
}
fn saturated(refusal: Refusal) {
    let mut e = engine(false);
    let before = e.store().setting("BTCUSDT");
    if matches!(refusal, Refusal::Paused) {
        pause(&mut e);
    }
    fill(&mut e);
    let expected = overflow(&mut e);
    let a = leverage(&mut e, AT, COMMAND, refusal.symbol());
    assert_eq!(failure(&a, COMMAND), &expected);
    assert_eq!(e.store().setting("BTCUSDT"), before);
    retire(&mut e, 1_000);
    let a = leverage(&mut e, AT, COMMAND, refusal.symbol());
    assert_eq!(failure(&a, COMMAND), &refusal.error());
    retire(&mut e, 1_001);
}
#[test]
fn unknown_market_cannot_bypass_genuine_owner_capacity() {
    saturated(Refusal::UnknownMarket);
}
#[test]
fn paused_read_cannot_bypass_genuine_owner_capacity() {
    saturated(Refusal::Paused);
}
#[test]
fn missing_credentials_cannot_bypass_genuine_owner_capacity() {
    saturated(Refusal::Credentials);
}
#[test]
fn genuine_overflow_preserves_a_duplicate_peer_and_releases_capacity() {
    let mut e = engine(false);
    fill(&mut e);
    overflow(&mut e);
    let a = leverage(&mut e, AT, 1_000, "MISSING");
    assert!(
        matches!(a.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
        "original peer remains live: {a:?}"
    );
    retire(&mut e, 1_000);
    assert!(retain(&mut e, 2_000).is_empty());
    retire(&mut e, 1_001);
}
#[test]
fn valid_signed_leverage_read_remains_managed_and_preserves_native_output() {
    let mut e = engine(true);
    let a = leverage(&mut e, AT, COMMAND, "BTCUSDT");
    let requests: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .url
            .ends_with("/capi/v3/account/symbolConfig?symbol=BTCUSDT")
    );
    assert_eq!(requests[0].method, Method::Get);
    assert!(
        requests[0]
            .headers
            .iter()
            .any(|(name, _)| *name == "ACCESS-SIGN")
    );
    e.handle(AT + 1., Event::HttpResponse { request: requests[0].id, response: Response {
        status: 200, headers: "", body: r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"7","isolatedShortLeverage":"9"}]"#,
    }});
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, AT + 1.);
    assert_eq!(a.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: COMMAND, result }) if result == r#"{"buyLeverage":7.0,"sellLeverage":9.0,"marginType":"isolated"}"#)).count(), 1);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

fn invalid_clock_then_fresh_local(private: bool, refusal: Refusal) {
    let mut e = engine(private);
    let a = leverage(&mut e, f64::NAN, COMMAND, refusal.symbol());
    assert_eq!(
        failure(&a, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
    let a = leverage(&mut e, AT, COMMAND, refusal.symbol());
    assert_eq!(failure(&a, COMMAND), &refusal.error());
}
#[test]
fn private_invalid_clock_refuses_unknown_market_then_allows_fresh_id_reuse() {
    invalid_clock_then_fresh_local(true, Refusal::UnknownMarket);
}
#[test]
fn keyless_invalid_clock_refuses_unknown_market_then_allows_fresh_id_reuse() {
    invalid_clock_then_fresh_local(false, Refusal::UnknownMarket);
}
#[test]
fn keyless_invalid_clock_refuses_missing_credentials_then_allows_fresh_id_reuse() {
    invalid_clock_then_fresh_local(false, Refusal::Credentials);
}
fn signed_wire_and_settings(secondary_only: bool) {
    let mut e = if secondary_only {
        let mut e = setup::with_venue(
            Options {
                read_credentials: Some(safe_cex_engine::ReadCredentials {
                    key: "read-key".into(),
                    secret: "read-secret".into(),
                    passphrase: Some("read-pass".into()),
                }),
                ..Default::default()
            },
            |venue| Probe {
                venue,
                roots: vec![],
            },
        );
        e.store_mut().set_markets(vec![market()]);
        e
    } else {
        engine(true)
    };
    let a = leverage(&mut e, AT, COMMAND, "BTCUSDT");
    let requests: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    let request = requests[0];
    let (key, secret, passphrase) = if secondary_only {
        ("read-key", "read-secret", "read-pass")
    } else {
        ("key", "secret", "passphrase")
    };
    assert_eq!(request.method, Method::Get);
    assert_eq!(request.body, None);
    assert_eq!(
        request.url,
        "https://api-contract.weex.com/capi/v3/account/symbolConfig?symbol=BTCUSDT"
    );
    assert_eq!(
        request.headers,
        setup::oracle(
            key,
            secret,
            passphrase,
            Method::Get,
            "/capi/v3/account/symbolConfig?symbol=BTCUSDT",
            "",
            "1002"
        )
    );
    e.handle(AT + 1., Event::HttpResponse { request: request.id, response: Response {
        status: 200, headers: "", body: r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"7","isolatedShortLeverage":"9"}]"#,
    }});
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, AT + 1.);
    assert_eq!(a.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: COMMAND, result }) if result == r#"{"buyLeverage":7.0,"sellLeverage":9.0,"marginType":"isolated"}"#)).count(), 1);
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    let setting = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(7.));
    assert_eq!(setting.sell_leverage, Some(9.));
    assert_eq!(
        setting.margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
}
#[test]
fn primary_signed_read_preserves_exact_oracle_signature_and_published_settings() {
    signed_wire_and_settings(false);
}
#[test]
fn secondary_only_signed_read_preserves_exact_oracle_signature_and_published_settings() {
    signed_wire_and_settings(true);
}
