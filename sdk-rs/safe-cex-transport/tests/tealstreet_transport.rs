// Arc is required by the production signature under test, not chosen here: the
// transport provider surface is `Arc<dyn ...>`, so a host fixture that satisfies
// it holds an Arc even though its interior is RefCell/Cell and so not Send+Sync.
#![allow(clippy::arc_with_non_send_sync)]
use safe_cex_transport::{tealstreet::*, tealstreet_time_source::*, transport_resolution::*, *};
use safe_cex_types::{ExchangeName, ExchangeOptions, QueryValue, object_map::JsObjectMap};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("TRANSPORT unexpected pending"),
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    #[serde(rename = "baseURL")]
    base_url: Option<String>,
    headers: indexmap::IndexMap<String, String>,
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    #[serde(rename = "baseURL")]
    base_url: Option<String>,
    proxied: bool,
    prefix: String,
    require_proxy: bool,
    private_url: String,
    server_url: String,
    routing: ResolvedRouting,
}
#[derive(serde::Deserialize)]
struct Case {
    exchange: ExchangeName,
    options: ExchangeOptions,
    config: Config,
    expected: Expected,
}
#[derive(serde::Deserialize)]
struct Auth {
    exchange: ExchangeName,
    token: String,
    node: bool,
    headers: indexmap::IndexMap<String, String>,
    #[serde(rename = "privateHeaders")]
    private_headers: BTreeMap<String, String>,
    events: Vec<String>,
}
#[derive(serde::Deserialize)]
struct Resolution {
    account: ResolvedTransport,
    meta: TransportMetadata,
    result: ResolvedTransport,
    direct: bool,
}
#[derive(serde::Deserialize)]
struct Connection {
    kind: String,
    headers: BTreeMap<String, String>,
}
#[derive(serde::Deserialize)]
struct UrlCase {
    url: String,
    tealstreet: bool,
    connection: Option<Connection>,
}
#[derive(serde::Deserialize)]
struct Endpoint {
    configured: Option<String>,
    endpoint: String,
}
#[derive(serde::Deserialize)]
struct RouteCase {
    exchange: ExchangeName,
    options: RoutingOptions,
    expected: ResolvedRouting,
}
#[derive(serde::Deserialize)]
struct DirectCase {
    name: String,
    #[serde(rename = "baseURL")]
    base_url: Option<String>,
    proxied: bool,
}
#[derive(serde::Deserialize)]
struct Source {
    pools: BTreeMap<String, String>,
    cases: Vec<Case>,
    auth: Vec<Auth>,
    resolutions: Vec<Resolution>,
    connections: Vec<UrlCase>,
    times: Vec<Endpoint>,
    routes: Vec<RouteCase>,
    direct: DirectCase,
}
fn source() -> Source {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/tealstreet-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TRANSPORT actual source assertions: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
struct Adapter(u16);
impl RestAdapter for Adapter {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            Ok(RestResponse {
                status: self.0,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        serde_json::Value::Null,
                    )
                    .unwrap(),
                ),
            })
        })
    }
}
struct Host {
    token: RefCell<String>,
    node: Cell<bool>,
    events: RefCell<Vec<String>>,
    regions: RefCell<Vec<Option<RestAdapterRegion>>>,
    default: Arc<dyn RestAdapter>,
    big: Arc<dyn RestAdapter>,
    fail: Cell<bool>,
}
impl Host {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            token: RefCell::new("token".into()),
            node: Cell::new(false),
            events: RefCell::new(vec![]),
            regions: RefCell::new(vec![]),
            default: Arc::new(Adapter(201)),
            big: Arc::new(Adapter(202)),
            fail: Cell::new(false),
        })
    }
}
impl TealstreetTransportHost for Host {
    fn get_proxy_token(&self) -> TransportFuture<'_, String> {
        self.events.borrow_mut().push("token".into());
        Box::pin(async move {
            if self.fail.get() {
                Err(TransportError::Host("canonical token refusal".into()))
            } else {
                Ok(self.token.borrow().clone())
            }
        })
    }
    fn is_node_or_bun_runtime(&self) -> bool {
        self.node.get()
    }
    fn mark_rest_proxy_header(&self, _: &mut RestRequest, shape: &'static str) {
        self.events.borrow_mut().push(format!("mark:{shape}"));
    }
    fn default_adapter(&self) -> Arc<dyn RestAdapter> {
        self.default.clone()
    }
    fn big_int_adapter(&self) -> Arc<dyn RestAdapter> {
        self.big.clone()
    }
    fn regional_adapter(&self, region: Option<RestAdapterRegion>) -> Arc<dyn RestAdapter> {
        self.regions.borrow_mut().push(region);
        Arc::new(Adapter(203))
    }
}
#[test]
fn native_provider_selection_matches_actual_source() {
    let source = source();
    assert_eq!(
        source.cases.len(),
        22,
        "TRANSPORT independent provider-case floor"
    );
    let provider = TealstreetTransport::new(Host::new());
    let first = &source.cases[0];
    let direct_request = RestRequest {
        base_url: first.config.base_url.clone(),
        ..Default::default()
    };
    let direct = safe_cex_transport::direct::DIRECT_TRANSPORT
        .rest(RestTransportContext {
            exchange: &first.exchange,
            options: &first.options,
            config: &direct_request,
        })
        .unwrap();
    assert_eq!(
        safe_cex_transport::direct::DIRECT_TRANSPORT.name(),
        source.direct.name,
        "TRANSPORT native executable direct-provider name"
    );
    assert_eq!(
        direct.base_url, source.direct.base_url,
        "TRANSPORT native executable direct-provider REST URL"
    );
    assert_eq!(direct.proxied, source.direct.proxied);
    assert!(direct.authorize.is_none());
    for case in source.cases {
        let request = RestRequest {
            base_url: case.config.base_url,
            headers: case.config.headers,
            ..Default::default()
        };
        let context = || RestTransportContext {
            exchange: &case.exchange,
            options: &case.options,
            config: &request,
        };
        let actual = provider.rest(context()).unwrap();
        assert_eq!(
            actual.base_url, case.expected.base_url,
            "TRANSPORT native REST URL"
        );
        assert_eq!(
            actual.proxied, case.expected.proxied,
            "TRANSPORT native proxied remains independent of override"
        );
        assert!(actual.authorize.is_some());
        assert_eq!(
            provider.proxy_prefix(context()),
            case.expected.prefix,
            "TRANSPORT native proxy prefix precedence"
        );
        let mut expected = case.options.clone();
        expected.tealstreet_proxy = Some(case.expected.require_proxy);
        assert_eq!(
            provider.require_proxy(case.options.clone()),
            expected,
            "TRANSPORT native requireProxy full options"
        );
        assert_eq!(
            provider.private_proxy(&case.exchange).unwrap().url(),
            case.expected.private_url
        );
        assert_eq!(
            provider.server_proxy_url(&case.exchange),
            Some(case.expected.server_url)
        );
        assert_eq!(
            provider.ws_routing(&case.exchange, &RoutingOptions::from(&case.options)),
            case.expected.routing,
            "TRANSPORT native routing fields"
        );
    }
    assert_eq!(provider.name(), "tealstreet");
    assert_eq!(
        provider.public_cors_proxy(),
        Some(AWS_PUBLIC_CORS_PROXY.into())
    );
    assert_eq!(
        safe_cex_transport::direct::DIRECT_TRANSPORT.name(),
        "direct"
    );
    for (key, constant) in [
        ("PRIVATE_PROXY_GRVT_TINY", PRIVATE_PROXY_GRVT_TINY),
        ("PRIVATE_PROXY_JP", PRIVATE_PROXY_JP),
        ("PRIVATE_PROXY_ASIA", PRIVATE_PROXY_ASIA),
        ("AWS_PUBLIC_CORS_PROXY", AWS_PUBLIC_CORS_PROXY),
    ] {
        assert_eq!(
            source.pools[key], constant,
            "TRANSPORT native endpoint literal"
        );
    }
}
#[test]
fn native_authorization_empty_tokens_headers_identity_and_adapter_dispatch() {
    let source = source();
    assert_eq!(
        source.auth.len(),
        12,
        "TRANSPORT independent auth-case floor"
    );
    let host = Host::new();
    let provider = TealstreetTransport::new(host.clone());
    let options: ExchangeOptions = serde_json::from_str(r#"{"key":"k","secret":"s"}"#).unwrap();
    for case in source.auth {
        *host.token.borrow_mut() = case.token;
        host.node.set(case.node);
        host.events.borrow_mut().clear();
        let mut request = RestRequest {
            headers: indexmap::IndexMap::from([("Authorization".into(), "venue-auth".into())]),
            query_params: Some(QueryValue::Number(-0.0)),
            ..Default::default()
        };
        let auth = provider
            .rest(RestTransportContext {
                exchange: &case.exchange,
                options: &options,
                config: &request,
            })
            .unwrap()
            .authorize
            .unwrap();
        let pending = auth.authorize(&mut request);
        assert_eq!(
            *host.events.borrow(),
            ["token"],
            "TRANSPORT native token lookup before polling"
        );
        ready(pending).unwrap();
        assert_eq!(
            request.headers, case.headers,
            "TRANSPORT native authorization headers including empty credential"
        );
        assert_eq!(
            *host.events.borrow(),
            case.events,
            "TRANSPORT native diagnostic stamping order"
        );
        let Some(QueryValue::Number(value)) = request.query_params else {
            panic!("TRANSPORT query carrier was changed")
        };
        assert_eq!(
            value.to_bits(),
            (-0.0f64).to_bits(),
            "TRANSPORT native query signed zero untouched"
        );
        let private = provider.private_proxy(&case.exchange).unwrap();
        assert_eq!(
            ready(private.resolve_headers()).unwrap(),
            case.private_headers,
            "TRANSPORT native private headers"
        );
    }
    let context = |variant| RestAdapterContext {
        exchange: &ExchangeName::Bybit,
        options: &options,
        variant,
    };
    let default = provider
        .rest_adapter(context(RestAdapterVariant::Default))
        .unwrap();
    assert!(
        Arc::ptr_eq(&default, &host.default),
        "TRANSPORT native default adapter identity"
    );
    assert_eq!(
        ready(default.execute(RestRequest::default()))
            .unwrap()
            .status,
        201
    );
    let big = provider
        .rest_adapter(context(RestAdapterVariant::BigInt))
        .unwrap();
    assert!(
        Arc::ptr_eq(&big, &host.big),
        "TRANSPORT native bigint adapter identity"
    );
    assert_eq!(
        ready(big.execute(RestRequest::default())).unwrap().status,
        202
    );
    for region in [
        None,
        Some(RestAdapterRegion::Ja),
        Some(RestAdapterRegion::Sg),
        Some(RestAdapterRegion::Other(String::new())),
        Some(RestAdapterRegion::Other("US".into())),
    ] {
        let adapter = provider
            .rest_adapter(context(RestAdapterVariant::Region { region }))
            .unwrap();
        assert_eq!(
            ready(adapter.execute(RestRequest::default()))
                .unwrap()
                .status,
            203
        );
    }
    assert_eq!(
        *host.regions.borrow(),
        [
            None,
            Some(RestAdapterRegion::Ja),
            Some(RestAdapterRegion::Sg),
            Some(RestAdapterRegion::Other(String::new())),
            Some(RestAdapterRegion::Other("US".into()))
        ]
    );
    host.fail.set(true);
    host.events.borrow_mut().clear();
    let mut request = RestRequest::default();
    let auth = provider
        .rest(RestTransportContext {
            exchange: &ExchangeName::Grvt,
            options: &options,
            config: &request,
        })
        .unwrap()
        .authorize
        .unwrap();
    assert!(
        matches!(ready(auth.authorize(&mut request)),Err(TransportError::Host(message)) if message=="canonical token refusal")
    );
    assert!(request.headers.is_empty());
    assert_eq!(
        *host.events.borrow(),
        ["token"],
        "TRANSPORT native token refusal precedes diagnostic/header changes"
    );
    assert!(
        ready(
            provider
                .private_proxy(&ExchangeName::Grvt)
                .unwrap()
                .resolve_headers()
        )
        .is_err()
    );
}
#[test]
fn native_preferences_preserve_undefined_final_operands_and_region_order() {
    let source = source();
    assert_eq!(
        source.resolutions.len(),
        162,
        "TRANSPORT independent preference-case floor"
    );
    for case in source.resolutions {
        let actual = resolve_account_transport(&case.account, &case.meta);
        assert_eq!(
            actual.tealstreet_proxy, case.result.tealstreet_proxy,
            "TRANSPORT native optional proxy final operand"
        );
        assert_eq!(
            actual.tealstreet_tunnel, case.result.tealstreet_tunnel,
            "TRANSPORT native optional tunnel final operand"
        );
        assert_eq!(actual.proxy_region, case.result.proxy_region);
        assert_eq!(is_direct_transport(&actual), case.direct);
    }
    assert_eq!(default_proxy_region(None), "SG");
    let regions: JsObjectMap<Vec<String>> = [("".into(), vec![])].into_iter().collect();
    assert_eq!(default_proxy_region(Some(&regions)), "SG");
    let regions: JsObjectMap<Vec<String>> = [
        ("JA".into(), vec![]),
        ("2".into(), vec![]),
        ("1".into(), vec![]),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        default_proxy_region(Some(&regions)),
        "1",
        "TRANSPORT native JS integer key order"
    );
}
#[test]
fn native_websocket_relay_precedence_headers_and_runtime_gate() {
    let source = source();
    assert_eq!(
        source.connections.len(),
        7,
        "TRANSPORT independent URL-case floor"
    );
    let provider = TealstreetTransport::new(Host::new());
    for case in source.connections {
        assert_eq!(is_tealstreet_web_socket_url(&case.url), case.tealstreet);
        let actual = provider.ws_connection(&case.url);
        match (actual, case.connection) {
            (None, None) => {}
            (Some(actual), Some(expected)) => {
                let (kind, headers) = match actual {
                    WsConnection::Relay { headers } => ("relay", headers),
                    WsConnection::Headers { headers } => ("headers", headers),
                };
                assert_eq!(
                    kind, expected.kind,
                    "TRANSPORT native relay before generic headers"
                );
                assert_eq!(
                    headers, expected.headers,
                    "TRANSPORT native upgrade header values"
                );
            }
            _ => panic!("TRANSPORT native connection absence mismatch"),
        }
    }
    assert!(!safe_cex_transport::ws_relay::is_ws_relay_enabled());
    safe_cex_transport::ws_relay::set_ws_relay_enabled(true);
    assert!(safe_cex_transport::ws_relay::is_ws_relay_enabled());
    safe_cex_transport::ws_relay::set_ws_relay_enabled(false);
    assert!(!safe_cex_transport::ws_relay::is_ws_relay_enabled());
}
struct TimeHost {
    now: Cell<f64>,
    origin: RefCell<Option<String>>,
    configured: RefCell<Option<String>>,
    unix: Cell<Option<f64>>,
    parse_fail: Cell<bool>,
    events: RefCell<Vec<String>>,
    fail: Cell<bool>,
}
impl TimeHost {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            now: Cell::new(1000.0),
            origin: RefCell::new(None),
            configured: RefCell::new(None),
            unix: Cell::new(Some(2000.0)),
            parse_fail: Cell::new(false),
            events: RefCell::new(vec![]),
            fail: Cell::new(false),
        })
    }
}
impl TealstreetTimeHost for TimeHost {
    fn origin(&self) -> Option<String> {
        self.origin.borrow().clone()
    }
    fn configured_time_origin(&self) -> Option<String> {
        self.configured.borrow().clone()
    }
    fn now_ms(&self) -> f64 {
        self.events.borrow_mut().push("clock".into());
        self.now.get()
    }
    fn get_time(&self, endpoint: String, timeout: u64) -> TransportFuture<'_, TimeResponse> {
        self.events
            .borrow_mut()
            .push(format!("{endpoint}:{timeout}"));
        Box::pin(async move {
            if self.fail.get() {
                Err(TransportError::Timeout)
            } else {
                Ok(TimeResponse {
                    unix_ms: self.unix.get(),
                    time: Some(serde_json::json!("1970-01-01T00:00:02.000Z")),
                })
            }
        })
    }
    fn parse_utc_time(&self, time: Option<&serde_json::Value>) -> Result<f64, TransportError> {
        assert_eq!(
            time.and_then(serde_json::Value::as_str),
            Some("1970-01-01T00:00:02.000Z")
        );
        if self.parse_fail.get() {
            return Err(TransportError::Host("invalid date".into()));
        }
        self.events.borrow_mut().push("parse".into());
        self.now.set(self.now.get() + 100.0);
        Ok(2000.0)
    }
}
#[test]
fn native_time_source_starts_eagerly_and_floors_rtt_before_parsing() {
    let source = source();
    assert_eq!(
        source.times.len(),
        6,
        "TRANSPORT independent endpoint-case floor"
    );
    let host = TimeHost::new();
    *host.origin.borrow_mut() = Some("https://app".into());
    assert_eq!(get_base_url(host.as_ref()), "https://app");
    for case in source.times {
        *host.configured.borrow_mut() = case.configured;
        assert_eq!(get_tealstreet_time_endpoint(host.as_ref()), case.endpoint);
    }
    *host.configured.borrow_mut() = None;
    *host.origin.borrow_mut() = None;
    let future = tealstreet_time_source(host.as_ref());
    assert_eq!(
        *host.events.borrow(),
        ["clock", "/api/time:5000"],
        "TRANSPORT native time request before poll"
    );
    host.now.set(1007.0);
    assert_eq!(
        ready(future),
        Some(2003.0),
        "TRANSPORT native half RTT floor"
    );
    host.unix.set(None);
    host.now.set(1000.0);
    host.events.borrow_mut().clear();
    let future = tealstreet_time_source(host.as_ref());
    host.now.set(1007.0);
    assert_eq!(ready(future), Some(2003.0));
    assert_eq!(
        *host.events.borrow(),
        ["clock", "/api/time:5000", "clock", "parse"],
        "TRANSPORT native response timestamp before date parse"
    );
    host.unix.set(Some(2000.0));
    host.now.set(1000.0);
    let future = tealstreet_time_source(host.as_ref());
    host.now.set(997.0);
    assert_eq!(
        ready(future),
        Some(1998.0),
        "TRANSPORT native negative RTT floors"
    );
    for unix in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        host.unix.set(Some(unix));
        assert_eq!(
            ready(tealstreet_time_source(host.as_ref())),
            None,
            "TRANSPORT native nonfinite server time"
        );
    }
    host.unix.set(None);
    host.parse_fail.set(true);
    assert_eq!(
        ready(tealstreet_time_source(host.as_ref())),
        None,
        "TRANSPORT native date refusal caught"
    );
    host.fail.set(true);
    assert_eq!(
        ready(tealstreet_time_source(host.as_ref())),
        None,
        "TRANSPORT native request refusal caught"
    );
}

#[test]
fn native_compiled_routing_table_matches_source_channels() {
    let source = source();
    assert_eq!(
        source.routes.len(),
        1152,
        "TRANSPORT independent route floor (48*2*2*3*2)"
    );
    for case in source.routes {
        let actual = resolve_exchange_routing(&case.exchange, &case.options);
        assert_eq!(
            actual.rest_proxy_host, case.expected.rest_proxy_host,
            "TRANSPORT native routing-table REST host"
        );
        assert_eq!(
            actual.how_egress, case.expected.how_egress,
            "TRANSPORT native routing-table HOW egress"
        );
        assert_eq!(
            actual.ws_public, case.expected.ws_public,
            "TRANSPORT native routing-table public channel"
        );
        assert_eq!(
            actual.ws_private, case.expected.ws_private,
            "TRANSPORT native routing-table private channel"
        );
        assert_eq!(
            actual.ws_business, case.expected.ws_business,
            "TRANSPORT native routing-table business channel"
        );
    }
}
