//! B1 originals (`docs/plans/RUST_BITRUE_REQUEST_FUNNEL.md`): Bitrue's reads and
//! listen-key sessions leave the legacy funnel for managed Contexts while keeping
//! native retries, the -1022 re-sign and per-owner command completion.
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, HttpRequest, Method, Notice, Query, ReadCredentials,
    RequestId, Response, VenueError,
};
use safe_cex_venue_bitrue::Bitrue;
use std::collections::HashMap;
use support::Options;

const MARKET: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":10,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
const LISTEN: &str = r#"{"code":200,"data":{"listenKey":"futures-key"}}"#;
const EMPTY: &str = r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"totalPositionNormal":7,"sumOpenRealizedAmount":2,"positionVos":[]}]}}"#;
const POSITION: &str = r#"{"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":100,"accountNormal":80,"accountLock":5,"totalPositionNormal":7,"sumOpenRealizedAmount":2,"positionVos":[{"contractName":"E-BTC-USDT","positions":[{"side":"BUY","volume":2,"openPrice":100,"leverageLevel":25,"positionType":2,"positionBalance":200,"forceLiquidationPrice":50}]}]}]}}"#;
const NO_ORDERS: &str = r#"{"code":"0","data":[]}"#;
const SIGNATURE_REFUSED: &str =
    r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#;

struct Harness {
    e: Engine<Bitrue>,
    pending: Vec<HttpRequest>,
    actions: Vec<Action>,
    intents: HashMap<RequestId, RoutingIntent>,
    now: f64,
}
impl Harness {
    fn new(options: Options) -> Self {
        let mut e = support::engine(options);
        e.start(0.0);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            intents: HashMap::new(),
            now: 0.0,
        };
        h.release();
        h
    }
    fn keyed() -> Self {
        Self::new(Options {
            key: Some("primary-key".into()),
            secret: Some("primary-secret".into()),
            read_credentials: Some(ReadCredentials {
                key: "read-key".into(),
                secret: "read-secret".into(),
                passphrase: None,
            }),
            ..Options::default()
        })
    }
    /// Markets loaded, the listen key adopted and the first account and order
    /// reads answered: only the recurring private work remains.
    fn started() -> Self {
        let mut h = Self::keyed();
        h.reply("/contracts", MARKET);
        h.reply("listenKey", LISTEN);
        h.reply("/account", EMPTY);
        h.reply("/openOrders", NO_ORDERS);
        h
    }
    /// Fires the one armed timer due exactly at `at`.
    fn fire_at(&mut self, at: f64) {
        let i = self
            .actions
            .iter()
            .rposition(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == at))
            .unwrap_or_else(|| panic!("no timer at {at}"));
        let Action::SetTimer { timer, .. } = self.actions.remove(i) else {
            unreachable!()
        };
        self.handle(at, Event::TimerFired { timer });
    }
    /// Releases managed work at `now`, recording each Prepare's routing intent;
    /// legacy requests pass straight through with no intent.
    fn release(&mut self) {
        let now = self.now;
        self.e.prepare_dispatch(now);
        let mut queue: std::collections::VecDeque<_> = self.e.drain_actions().collect();
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(managed) => match *managed {
                    HttpAction::Prepare {
                        preparation,
                        intent,
                        request,
                    } => {
                        if self.e.http_preparation_pending(preparation) {
                            self.intents.insert(request.id, intent);
                            self.e.http_prepared(
                                now,
                                preparation,
                                Prepared {
                                    id: request.id.0,
                                    expires_at: None,
                                },
                            );
                        }
                    }
                    HttpAction::Bind {
                        permit, request, ..
                    } => {
                        if let Ok(Claim::Released(_)) = self.e.claim_http(now, permit) {
                            self.pending.push(request);
                        }
                    }
                    HttpAction::Cancel { .. } => {}
                    HttpAction::BindGroup { .. } => panic!("B1 declares no Mutation barriers"),
                },
                Action::Http(request) => self.pending.push(request),
                action => self.actions.push(action),
            }
            for action in self.e.drain_actions().collect::<Vec<_>>().into_iter().rev() {
                queue.push_front(action);
            }
        }
    }
    fn handle(&mut self, at: f64, event: Event<'_>) {
        self.now = self.now.max(at);
        self.e.handle(self.now, event);
        self.release();
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .pending
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.pending));
        self.pending.remove(i)
    }
    fn count(&self, path: &str) -> usize {
        self.pending.iter().filter(|r| r.url.contains(path)).count()
    }
    /// The managed routing intent of `request`; legacy HTTP has none.
    fn intent(&self, request: &HttpRequest) -> RoutingIntent {
        *self
            .intents
            .get(&request.id)
            .unwrap_or_else(|| panic!("{} must be managed, not legacy HTTP", request.url))
    }
    fn respond(&mut self, request: &HttpRequest, status: u16, body: &str) {
        let now = self.now;
        self.handle(
            now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
    }
    fn reply(&mut self, path: &str, body: &str) {
        let request = self.take(path);
        self.respond(&request, 200, body);
    }
    fn lose(&mut self, request: &HttpRequest) {
        let now = self.now;
        self.handle(
            now,
            Event::HttpFailed {
                request: request.id,
                error: "connection reset",
            },
        );
    }
    fn command(&mut self, at: f64, command: Command) {
        self.handle(at, Event::Command(command));
    }
    fn terminals(&self, id: u32) -> Vec<Result<String, VenueError>> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                    Some(Ok(result.clone()))
                }
                Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                    Some(Err(error.clone()))
                }
                _ => None,
            })
            .collect()
    }
    fn fire_timers_until(&mut self, at: f64) {
        loop {
            let due = self
                .actions
                .iter()
                .enumerate()
                .filter_map(|(i, a)| match a {
                    Action::SetTimer { at_ms, .. } if *at_ms <= at => Some((i, *at_ms)),
                    _ => None,
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((i, at_ms)) = due else {
                break;
            };
            let Action::SetTimer { timer, .. } = self.actions.remove(i) else {
                unreachable!()
            };
            self.handle(at_ms, Event::TimerFired { timer });
        }
        self.now = self.now.max(at);
    }
}
fn api_key(request: &HttpRequest) -> Option<&str> {
    request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("X-CH-APIKEY"))
        .map(|(_, value)| value.as_str())
}

#[test]
fn keyed_markets_are_a_managed_primary_catalog_read_with_native_headers() {
    let mut h = Harness::keyed();
    let request = h.take("/contracts");
    assert_eq!(h.intent(&request), RoutingIntent::Primary);
    assert_eq!(api_key(&request), Some("primary-key"));
    for header in ["Content-Type", "X-Referer"] {
        assert!(
            request.headers.iter().any(|(name, _)| *name == header),
            "{header}: {:?}",
            request.headers
        );
    }
}

#[test]
fn keyless_markets_are_a_managed_unsigned_read_on_the_primary_route() {
    let mut h = Harness::new(Options::default());
    let request = h.take("/contracts");
    assert_eq!(h.intent(&request), RoutingIntent::Primary);
    assert_eq!(api_key(&request), None);
}

#[test]
fn account_reads_use_the_read_key_and_keep_three_native_retries() {
    let mut h = Harness::keyed();
    h.reply("/contracts", MARKET);
    let mut read = h.take("/account");
    assert_eq!(h.intent(&read), RoutingIntent::Secondary);
    assert_eq!(api_key(&read), Some("read-key"));
    for retry in 1..=3 {
        h.lose(&read);
        read = h.take("/account");
        assert_eq!(h.intent(&read), RoutingIntent::Secondary, "retry {retry}");
    }
    h.lose(&read);
    assert_eq!(h.count("/account"), 0, "a 4th attempt is never sent");
}

#[test]
fn a_proven_signature_refusal_re_signs_one_managed_read_once() {
    let mut h = Harness::keyed();
    h.reply("/contracts", MARKET);
    let first = h.take("/account");
    h.respond(&first, 200, SIGNATURE_REFUSED);
    let second = h.take("/account");
    assert_eq!(h.intent(&second), RoutingIntent::Secondary);
    h.respond(&second, 200, SIGNATURE_REFUSED);
    assert_eq!(h.count("/account"), 0, "only one re-sign");
}

#[test]
fn listen_key_posts_are_serial_session_operations_with_three_native_retries() {
    let mut h = Harness::keyed();
    h.reply("/contracts", MARKET);
    let mut post = h.take("listenKey");
    assert_eq!(post.method, Method::Post);
    assert_eq!(h.intent(&post), RoutingIntent::Primary);
    let first = post.clone();
    for retry in 1..=3 {
        h.lose(&post);
        assert_eq!(
            h.count("listenKey"),
            1,
            "retry {retry}: never two in flight"
        );
        post = h.take("listenKey");
        assert_eq!(h.intent(&post), RoutingIntent::Primary, "retry {retry}");
    }
    // A late answer to the finished first POST adopts no key.
    h.respond(&first, 200, LISTEN);
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { url, .. } if url.contains("listenKey"))),
        "a finished POST's late key is refused"
    );
    h.lose(&post);
    assert_eq!(h.count("listenKey"), 0, "a 4th POST is never sent");
}

#[test]
fn the_keepalive_put_is_a_managed_session_write() {
    let mut h = Harness::started();
    // Native refreshes the key 30 minutes after it was adopted, at t=0.
    h.fire_at(30.0 * 60_000.0);
    let put = h.take("listenKey/futures-key");
    assert_eq!(put.method, Method::Put);
    assert_eq!(h.intent(&put), RoutingIntent::Primary);
}

#[test]
fn a_client_order_lookup_completes_once_through_its_own_read() {
    let mut h = Harness::started();
    let start = h.now;
    h.command(
        start,
        Command::Query {
            command: 41,
            query: Query::OrderByClientId {
                symbol: "BTCUSDT".into(),
                client_order_id: "missing".into(),
            },
        },
    );
    let read = h.take("/openOrders");
    assert_eq!(h.intent(&read), RoutingIntent::Secondary);
    // The command's own 20 s bound expires before the late answer.
    h.fire_timers_until(start + 20_001.0);
    h.respond(&read, 200, NO_ORDERS);
    let terminals = h.terminals(41);
    assert_eq!(terminals.len(), 1, "{terminals:?}");
    assert_eq!(
        terminals[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
}

#[test]
fn joined_leverage_owners_complete_from_one_background_snapshot_each_once() {
    let mut h = Harness::started();
    let first = h.now;
    h.command(
        first,
        Command::FetchLeverage {
            command: 51,
            symbol: "BTCUSDT".into(),
        },
    );
    let snapshot = h.take("/account");
    assert_eq!(h.intent(&snapshot), RoutingIntent::Secondary);
    // The second owner joins later, so the first owner's bound expires first.
    h.command(
        first + 15_000.0,
        Command::FetchLeverage {
            command: 52,
            symbol: "BTCUSDT".into(),
        },
    );
    h.fire_timers_until(first + 20_001.0);
    h.respond(&snapshot, 200, POSITION);
    // A snapshot requested for the later joiner may follow; answer it.
    while h.count("/account") > 0 {
        h.reply("/account", POSITION);
    }
    let expired = h.terminals(51);
    assert_eq!(expired.len(), 1, "{expired:?}");
    assert_eq!(
        expired[0].as_ref().unwrap_err().kind,
        Some(ErrorKind::Timeout)
    );
    let live = h.terminals(52);
    assert_eq!(live.len(), 1, "{live:?}");
    assert!(live[0].as_ref().unwrap().contains("\"buyLeverage\":25"));
}
