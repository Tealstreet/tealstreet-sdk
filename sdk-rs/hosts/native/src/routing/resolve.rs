//! Native port of safe-cex's `resolveEngineRoutingPlan` and the account
//! transport defaults (`normalizeExchangeAccountConfig` + `resolveAccountTransport`).
//! `table.json` is generated from the TypeScript tables, and
//! `tests/routing_port.rs` holds the result to the TypeScript plans.
use super::{
    HttpRoute, HttpRouteMode, PublicWsRoute, RoutingPlan, SocketChannel, SocketChannelRule,
    WsRoute, WsRoutes,
};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    hosts: Hosts,
    rest_headers: BTreeMap<String, String>,
    ws_headers: BTreeMap<String, String>,
    exchanges: HashMap<String, Exchange>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hosts {
    private_proxy: HashMap<String, String>,
    public_cors_proxy: String,
    tunnel: HashMap<String, String>,
    relay_query_param: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Exchange {
    routing: Spec,
    requires_cors: bool,
    enforce_ws_tunnel: bool,
    supports_tunnel: Option<bool>,
    private_proxy_regions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    rest_pool: String,
    how_egress: String,
    #[serde(default)]
    ws: WsSpecs,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WsSpecs {
    public: Option<Strategy>,
    private: Option<Strategy>,
    business: Option<Strategy>,
    private_uta: Option<Strategy>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Strategy {
    #[serde(flatten)]
    kind: Kind,
    fallback: Option<String>,
    relay_upstream: Option<String>,
    #[serde(default)]
    relay_when_tunnelled: bool,
    initial_silent_connects: Option<usize>,
    ignored_messages: Option<Vec<String>>,
    direct_urls: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum Kind {
    Direct,
    AlwaysProxy { host: String },
    ProxyConditional { host: String },
    RegionConditional { ja: String, other: String },
    BybitBrazilConditional { host: String },
    FallbackOnly,
    WsRouteCandidates { proxy: String },
}

/// The table, validated once: every pool and egress an entry can select exists,
/// so the host lookups below cannot fail after `loaded()` succeeds.
fn loaded() -> Result<&'static Table, RoutingError> {
    static TABLE: OnceLock<Result<Table, String>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut table: Table =
                serde_json::from_str(include_str!("table.json")).map_err(|e| e.to_string())?;
            apply_proxy_overrides(
                &mut table.hosts,
                std::env::var("PROXY_PRIVATE_URL").ok(),
                std::env::var("PROXY_PUBLIC_URL").ok(),
            )?;
            for pool in ["jp", "asia", "tiny"] {
                if !table.hosts.private_proxy.contains_key(pool) {
                    return Err(format!("no private proxy pool {pool}"));
                }
            }
            for egress in ["jp", "sg"] {
                if !table.hosts.tunnel.contains_key(egress) {
                    return Err(format!("no tunnel egress {egress}"));
                }
            }
            if let Some((name, _)) = table
                .exchanges
                .iter()
                .find(|(_, e)| !table.hosts.private_proxy.contains_key(&e.routing.rest_pool))
            {
                return Err(format!("{name} names an unknown proxy pool"));
            }
            Ok(table)
        })
        .as_ref()
        .map_err(|e| RoutingError::InvalidTable(e.clone()))
}

/// The deployment's proxy hosts: `PROXY_PRIVATE_URL` replaces every whitelisted
/// private pool and `PROXY_PUBLIC_URL` the public CORS proxy. Unset or empty keeps
/// the table's value; a trailing `/` is dropped because routes append `/<origin>`.
/// Anything but an https `tealstreet.io` host refuses at table load.
fn apply_proxy_overrides(
    hosts: &mut Hosts,
    private: Option<String>,
    public: Option<String>,
) -> Result<(), String> {
    let parse = |name: &str, value: Option<String>| -> Result<Option<String>, String> {
        let Some(value) = value.map(|v| v.trim().trim_end_matches('/').to_owned()) else {
            return Ok(None);
        };
        if value.is_empty() {
            return Ok(None);
        }
        // These hosts receive users' proxy JWTs: only a Tealstreet host is accepted.
        let host = value
            .strip_prefix("https://")
            // WHATWG URLs also split https paths on `\`; refuse it anywhere.
            .filter(|_| !value.contains(['?', '#', '\\']))
            .and_then(|rest| rest.split('/').next())
            // Host and port only: no userinfo or other authority characters.
            .filter(|authority| {
                authority
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
            })
            .and_then(|authority| authority.split(':').next());
        if !host.is_some_and(|host| host == "tealstreet.io" || host.ends_with(".tealstreet.io")) {
            return Err(format!(
                "{name} must be an https URL on a tealstreet.io host, without query or fragment"
            ));
        }
        Ok(Some(value))
    };
    if let Some(private) = parse("PROXY_PRIVATE_URL", private)? {
        for host in hosts.private_proxy.values_mut() {
            host.clone_from(&private);
        }
    }
    if let Some(public) = parse("PROXY_PUBLIC_URL", public)? {
        hosts.public_cors_proxy = public;
    }
    Ok(())
}

fn table() -> &'static Table {
    loaded().expect("validated by every public entry point")
}

/// Which TS transport provider the account would have used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Tealstreet's proxy pools, tradetunnel and passthroughs (every hosted account).
    Tealstreet,
    /// Straight to the venue; refuses an account that asks for Tealstreet routing.
    Direct,
}

/// The routing-relevant subset of normalized `ExchangeOptions`. No credential
/// values and no proxy JWT: `has_key` and `payload_signer` are presence only.
/// There is deliberately no `overrideProxyUrl`: an authorized route would carry
/// the account's proxy token to whatever host it named. Unknown fields refuse.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingInputs {
    pub tealstreet_proxy: Option<bool>,
    pub tealstreet_tunnel: Option<bool>,
    pub tealstreet_ws_route: Option<bool>,
    pub proxy_region: Option<String>,
    pub bybit_site_id: Option<String>,
    pub public_data_only: Option<bool>,
    #[serde(default)]
    pub has_key: bool,
    #[serde(default)]
    pub payload_signer: bool,
}

/// A saved account's transport preferences, as the hosted init payload carries them.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountTransport {
    pub tealstreet_proxy: Option<bool>,
    pub tealstreet_tunnel: Option<bool>,
    pub tealstreet_ws_route: Option<bool>,
    pub proxy_region: Option<String>,
    pub testnet: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutingError {
    UnknownExchange(String),
    /// The direct provider refuses an account that asked for Tealstreet routing.
    DirectRefusesTealstreet(String),
    /// Secondary read keys are not yet routed natively.
    SecondaryUnsupported,
    UnsupportedRewrite,
    InvalidTable(String),
    /// No fixed REST origin: the venue is not served natively, or (HyroTrader)
    /// its endpoints come from the account's configuration.
    NoBaseUrl(String),
}

impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownExchange(exchange) => write!(f, "unknown exchange {exchange}"),
            Self::DirectRefusesTealstreet(exchange) => write!(
                f,
                "[{exchange}] tealstreetProxy/tealstreetTunnel requested but the direct transport is active. \
                 Register a transport with setDefaultTransport() or pass ExchangeOptions.transport."
            ),
            Self::SecondaryUnsupported => {
                write!(f, "native routing does not yet route secondary read keys")
            }
            Self::UnsupportedRewrite => write!(f, "Unsupported native URL rewrite"),
            Self::InvalidTable(error) => write!(f, "invalid routing table: {error}"),
            Self::NoBaseUrl(exchange) => write!(f, "no fixed REST base URL for {exchange}"),
        }
    }
}

impl std::error::Error for RoutingError {}

fn exchange(name: &str) -> Result<&'static Exchange, RoutingError> {
    loaded()?
        .exchanges
        .get(name)
        .ok_or_else(|| RoutingError::UnknownExchange(name.into()))
}

/// The venue's own REST base URL, which `resolve_routing_plan` takes as
/// `base_url`. Copied from each venue crate's REST constant, with the registry's
/// demo-as-testnet choices (OKX demo, Bybit demo) applied. A testnet account on a
/// venue with no testnet refuses rather than resolving to the live host; the
/// mainnet host is returned only where the venue's demo rides it (Bitget's
/// simulated `s*-futures` products, Bitunix's `x-simulated-trading`, OKX EU's EEA host).
pub fn rest_base_url(name: &str, testnet: bool) -> Result<&'static str, RoutingError> {
    let pick = |mainnet, test| if testnet { test } else { mainnet };
    Ok(match name {
        "binance" => pick("https://fapi.binance.com", "https://demo-fapi.binance.com"),
        "binancepm" => pick("https://papi.binance.com", "https://demo-fapi.binance.com"),
        "binanceinverse" => pick("https://dapi.binance.com", "https://demo-dapi.binance.com"),
        "binancespot" => pick("https://api.binance.com", "https://testnet.binance.vision"),
        "bybit" | "bybitinverse" | "bybitspot" => {
            pick("https://api.bybit.com", "https://api-testnet.bybit.com")
        }
        "bybitdemo" => pick(
            "https://api-demo.bybit.com",
            "https://api-testnet.bybit.com",
        ),
        "okx" => pick("https://www.okx.com", "https://openapi.okx.com"),
        "okxdemo" => "https://openapi.okx.com",
        "okxeu" | "okxeudemo" => "https://eea.okx.com",
        "bingx" if testnet => {
            return Err(RoutingError::NoBaseUrl(name.into()));
        }
        "bitget" | "bitgetusdc" | "bitgetinverse" => "https://api.bitget.com",
        "hyperliquid" | "hyperliquidspot" => pick(
            "https://api.hyperliquid.xyz",
            "https://api.hyperliquid-testnet.xyz",
        ),
        "blofin" => pick(
            "https://openapi.blofin.com",
            "https://demo-trading-openapi.blofin.com",
        ),
        "bingx" => "https://open-api.bingx.com",
        "lighter" => pick(
            "https://mainnet.zklighter.elliot.ai",
            "https://testnet-api.lighter.xyz",
        ),
        "lighter-robinhood" => pick(
            "https://api.rh.lighter.xyz",
            "https://api.rh-testnet.lighter.xyz",
        ),
        "paradex" => pick(
            "https://api.prod.paradex.trade/v1",
            "https://api.testnet.paradex.trade/v1",
        ),
        "phemex" => pick("https://api.phemex.com", "https://testnet-api.phemex.com"),
        "bitunix" => "https://fapi.bitunix.com",
        _ => return Err(RoutingError::NoBaseUrl(name.into())),
    })
}

/// `accountToExchangeOptions`' transport fields for a hosted account: the
/// persist-time `normalizeExchangeAccountConfig`, then `resolveAccountTransport`.
pub fn account_transport(
    name: &str,
    account: &AccountTransport,
) -> Result<RoutingInputs, RoutingError> {
    let metadata = exchange(name)?;
    let default_region = || {
        metadata
            .private_proxy_regions
            .first()
            .cloned()
            .unwrap_or_else(|| "SG".into())
    };
    // normalizeExchangeAccountConfig: a region survives only with the proxy on.
    let saved_proxy = metadata.requires_cors
        || (!account.testnet.unwrap_or(false) && account.tealstreet_proxy.unwrap_or(false));
    let saved_region = saved_proxy.then(|| {
        account
            .proxy_region
            .clone()
            .filter(|r| !r.is_empty())
            .unwrap_or_else(default_region)
    });
    // resolveAccountTransport.
    let proxy_region = saved_region.or_else(|| metadata.requires_cors.then(default_region));
    let proxy = saved_proxy || metadata.requires_cors || metadata.enforce_ws_tunnel;
    let tunnel = if proxy && (metadata.supports_tunnel != Some(false) || metadata.enforce_ws_tunnel)
    {
        Some(true)
    } else {
        account.tealstreet_tunnel
    };
    Ok(RoutingInputs {
        tealstreet_proxy: Some(proxy),
        tealstreet_tunnel: tunnel,
        tealstreet_ws_route: account.tealstreet_ws_route,
        proxy_region,
        has_key: true,
        ..RoutingInputs::default()
    })
}

#[derive(Clone, Default)]
struct ResolvedWs {
    tunnelled: bool,
    host: Option<String>,
    fallback: Option<String>,
    relay: Option<String>,
    initial_silent_connects: Option<usize>,
    ignored_messages: Option<Vec<String>>,
    direct_urls: Option<Vec<String>>,
}

struct Resolved {
    rest_proxy_host: String,
    how_egress: &'static str,
    public: ResolvedWs,
    private: ResolvedWs,
    business: ResolvedWs,
    private_uta: Option<ResolvedWs>,
}

/// `URLSearchParams` form encoding: unreserved `*-._` and alphanumerics stay.
fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn tunnel_url(egress: &str) -> String {
    table().hosts.tunnel[egress].clone()
}

fn relay_url(egress: &str, upstream: &str) -> String {
    format!(
        "{}?{}={}",
        tunnel_url(egress),
        table().hosts.relay_query_param,
        form_encode(upstream)
    )
}

fn resolve_ws(strategy: Option<&Strategy>, o: &RoutingInputs, egress: &str) -> ResolvedWs {
    let Some(strategy) = strategy else {
        return ResolvedWs::default();
    };
    let proxy = o.tealstreet_proxy.unwrap_or(false);
    let tunnelled = |host: &str| ResolvedWs {
        tunnelled: true,
        host: Some(host.into()),
        ..ResolvedWs::default()
    };
    let mut resolved = match &strategy.kind {
        Kind::Direct => ResolvedWs::default(),
        Kind::AlwaysProxy { host } => tunnelled(host),
        Kind::ProxyConditional { host } if proxy => tunnelled(host),
        Kind::RegionConditional { ja, other } if proxy => {
            tunnelled(if o.proxy_region.as_deref() == Some("JA") {
                ja
            } else {
                other
            })
        }
        Kind::BybitBrazilConditional { host } if o.bybit_site_id.as_deref() == Some("BRA_BTL") => {
            tunnelled(host)
        }
        Kind::FallbackOnly => ResolvedWs {
            fallback: strategy.fallback.clone(),
            ..ResolvedWs::default()
        },
        Kind::WsRouteCandidates { proxy } if o.tealstreet_ws_route.unwrap_or(false) => {
            tunnelled(proxy)
        }
        _ => ResolvedWs::default(),
    };
    if resolved.tunnelled {
        resolved.direct_urls = strategy.direct_urls.clone();
        if strategy.relay_when_tunnelled {
            resolved.relay = strategy
                .relay_upstream
                .as_deref()
                .map(|u| relay_url(egress, u));
        }
        return resolved;
    }
    resolved.initial_silent_connects = strategy.initial_silent_connects;
    resolved.ignored_messages = strategy.ignored_messages.clone();
    resolved.direct_urls = strategy.direct_urls.clone();
    if resolved.fallback.is_none() {
        resolved.fallback = strategy.fallback.clone();
    }
    resolved.relay = strategy
        .relay_upstream
        .as_deref()
        .map(|u| relay_url(egress, u));
    resolved
}

fn resolve_exchange_routing(
    name: &str,
    o: &RoutingInputs,
    provider: Provider,
) -> Result<Resolved, RoutingError> {
    let spec = &exchange(name)?.routing;
    if provider == Provider::Direct {
        return Ok(Resolved {
            rest_proxy_host: String::new(),
            how_egress: "sg",
            public: ResolvedWs::default(),
            private: ResolvedWs::default(),
            business: ResolvedWs::default(),
            private_uta: None,
        });
    }
    let ja = o.proxy_region.as_deref() == Some("JA");
    let how_egress = match spec.how_egress.as_str() {
        "regionConditional" if ja => "jp",
        "jp" => "jp",
        _ => "sg",
    };
    let pool = if spec.rest_pool == "tiny" {
        "tiny"
    } else if ja {
        "jp"
    } else {
        spec.rest_pool.as_str()
    };
    Ok(Resolved {
        rest_proxy_host: if o.tealstreet_proxy.unwrap_or(false) {
            table().hosts.private_proxy[pool].clone()
        } else {
            String::new()
        },
        how_egress,
        public: resolve_ws(spec.ws.public.as_ref(), o, how_egress),
        private: resolve_ws(spec.ws.private.as_ref(), o, how_egress),
        business: resolve_ws(spec.ws.business.as_ref(), o, how_egress),
        private_uta: spec
            .ws
            .private_uta
            .as_ref()
            .map(|s| resolve_ws(Some(s), o, how_egress)),
    })
}

/// The hosted server never enables the browser's `WS_RELAY_PRIVATE` relay gate.
fn ws_route(route: &ResolvedWs) -> WsRoute {
    WsRoute {
        host: route.host.clone().filter(|_| route.tunnelled),
        fallback: route.fallback.clone(),
        relay: None,
        initial_silent_connects: route.initial_silent_connects,
        ignored_messages: route.ignored_messages.clone().unwrap_or_default(),
        direct_urls: route.direct_urls.clone().unwrap_or_default(),
    }
}

fn venue_headers(name: &str, o: &RoutingInputs) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    if matches!(name, "bybit" | "bybitinverse" | "bybitspot")
        && o.bybit_site_id.as_deref() == Some("BRA_BTL")
    {
        headers.insert("x-site-id".into(), "BRA_BTL".into());
    }
    headers
}

fn proxy_authorization_header(name: &str) -> &'static str {
    if matches!(name, "grvt" | "mexc" | "paradex" | "breakoutprop") {
        "Tealstreet-Authorization"
    } else {
        "Authorization"
    }
}

// Factory adapter gates, distinct from saved-account metadata defaults.
fn uses_tunnel(name: &str, o: &RoutingInputs) -> bool {
    let public = o.public_data_only.unwrap_or(false);
    match name {
        "paradex" => !public && !o.payload_signer,
        "hyperliquid" | "hyperliquidspot" | "kucoin" | "kucoinspot" => !public,
        "bingx" => !public && o.tealstreet_tunnel != Some(false),
        "weex" | "grvt" | "blofin" | "mexc" => false,
        _ => o.tealstreet_tunnel.unwrap_or(false),
    }
}

const BINANCE_FUTURES: [&str; 3] = ["binance", "binancepm", "binanceinverse"];

/// `restRoute` + the provider's `rest()`: (routed base URL, proxied).
fn rest_route(
    name: &str,
    o: &RoutingInputs,
    routing: &Resolved,
    url: &str,
    public: bool,
    provider: Provider,
) -> Result<(String, bool), RoutingError> {
    if public && BINANCE_FUTURES.contains(&name) {
        return Ok((url.into(), false));
    }
    let origin_end = url
        .find("://")
        .map(|scheme| {
            let rest = &url[scheme + 3..];
            scheme + 3 + rest.find(['/', '?', '#']).unwrap_or(rest.len())
        })
        .ok_or(RoutingError::UnsupportedRewrite)?;
    let (origin, path) = url.split_at(origin_end);
    let (base, proxied) = match provider {
        Provider::Direct => {
            if o.tealstreet_proxy.unwrap_or(false) || o.tealstreet_tunnel.unwrap_or(false) {
                return Err(RoutingError::DirectRefusesTealstreet(name.into()));
            }
            (origin.into(), false)
        }
        Provider::Tealstreet => {
            let tunnel = o.tealstreet_tunnel.unwrap_or(false);
            let grvt_or_blofin = matches!(name, "grvt" | "blofin");
            let proxy = o.tealstreet_proxy.unwrap_or(false) || (grvt_or_blofin && tunnel);
            let prefix = if tunnel && !grvt_or_blofin {
                String::new()
            } else if origin.contains("binance") && public {
                table().hosts.public_cors_proxy.clone()
            } else if proxy {
                routing.rest_proxy_host.clone()
            } else {
                String::new()
            };
            let proxied = !prefix.is_empty();
            let base = if proxied {
                format!("{prefix}/{origin}")
            } else {
                origin.into()
            };
            (base, proxied)
        }
    };
    Ok((format!("{base}{path}"), proxied))
}

fn socket_channels() -> Vec<SocketChannelRule> {
    [
        ("/business", SocketChannel::Business),
        ("/ws-futures", SocketChannel::Business),
        ("/private", SocketChannel::Private),
        ("/user", SocketChannel::Private),
        ("/account", SocketChannel::Private),
        ("listenKey=", SocketChannel::Private),
    ]
    .into_iter()
    .map(|(contains, channel)| SocketChannelRule {
        contains: contains.into(),
        channel,
    })
    .collect()
}

/// `resolveEngineRoutingPlan(exchange, options, realRestBaseURL)` with the relay gate off.
pub fn resolve_routing_plan(
    name: &str,
    o: &RoutingInputs,
    base_url: &str,
    provider: Provider,
) -> Result<RoutingPlan, RoutingError> {
    let mut routing = resolve_exchange_routing(name, o, provider)?;
    // Bitunix's TS sockets choose their declared fallback on this explicit option.
    let force_bitunix = |route: &mut ResolvedWs| {
        if name == "bitunix"
            && o.tealstreet_ws_route.unwrap_or(false)
            && let Some(fallback) = route.fallback.clone()
        {
            *route = ResolvedWs {
                tunnelled: true,
                host: Some(fallback),
                ..ResolvedWs::default()
            };
        }
    };
    force_bitunix(&mut routing.public);
    force_bitunix(&mut routing.private);
    force_bitunix(&mut routing.business);
    let rest_routing = if matches!(name, "grvt" | "blofin")
        && o.tealstreet_tunnel.unwrap_or(false)
        && !o.tealstreet_proxy.unwrap_or(false)
    {
        let proxied = RoutingInputs {
            tealstreet_proxy: Some(true),
            ..o.clone()
        };
        Some(resolve_exchange_routing(name, &proxied, provider)?)
    } else {
        None
    };
    let rest_routing = rest_routing.as_ref().unwrap_or(&routing);
    let tealstreet = provider == Provider::Tealstreet;
    let http = |public: bool| -> Result<HttpRoute, RoutingError> {
        let (target, proxied) = rest_route(name, o, rest_routing, base_url, public, provider)?;
        if target != base_url && !target.ends_with(&format!("/{base_url}")) {
            return Err(RoutingError::UnsupportedRewrite);
        }
        let how = uses_tunnel(name, o) && tealstreet;
        let mut header_aliases = BTreeMap::new();
        if matches!(name, "coinbase" | "coinbasespot") && proxied && !how {
            header_aliases.insert("Authorization".into(), "Coinbase-Authorization".into());
        }
        let mut headers = venue_headers(name, o);
        if proxied && tealstreet {
            headers.extend(table().rest_headers.clone());
        }
        Ok(HttpRoute {
            mode: if how {
                HttpRouteMode::How
            } else {
                HttpRouteMode::Direct
            },
            prefix: if target == base_url {
                String::new()
            } else {
                target[..target.len() - base_url.len() - 1].into()
            },
            authorize: proxied,
            authorization_header: proxy_authorization_header(name).into(),
            headers,
            header_aliases,
            tunnel_url: how.then(|| tunnel_url(routing.how_egress)),
        })
    };
    let private = !o.public_data_only.unwrap_or(false) && o.has_key;
    let public_cors_http =
        (tealstreet && matches!(name, "asterdex" | "txflow")).then(|| HttpRoute {
            prefix: table().hosts.public_cors_proxy.clone(),
            authorization_header: proxy_authorization_header(name).into(),
            ..HttpRoute::default()
        });
    let binance = name.starts_with("binance");
    let public_ws_routes = if name == "bybit" {
        let product = |variant: &str| -> Result<WsRoute, RoutingError> {
            Ok(ws_route(
                &resolve_exchange_routing(variant, o, provider)?.public,
            ))
        };
        vec![
            PublicWsRoute {
                contains: "/v5/public/inverse".into(),
                route: product("bybitinverse")?,
            },
            PublicWsRoute {
                contains: "/v5/public/spot".into(),
                route: product("bybitspot")?,
            },
            PublicWsRoute {
                contains: "/v5/public/option".into(),
                route: WsRoute::default(),
            },
        ]
    } else {
        Vec::new()
    };
    let private_ws_routes = routing
        .private_uta
        .as_ref()
        .map(|uta| {
            // The same venue pong is not route traffic on either private socket.
            let uta = ResolvedWs {
                ignored_messages: routing.private.ignored_messages.clone(),
                ..uta.clone()
            };
            vec![PublicWsRoute {
                contains: "/v3/ws/private".into(),
                route: ws_route(&uta),
            }]
        })
        .unwrap_or_default();
    Ok(RoutingPlan {
        http: http(!private)?,
        public_http: if binance { Some(http(true)?) } else { None },
        read_http: None,
        read_public_http: None,
        read_header: None,
        read_public: false,
        public_cors_http,
        public_header: binance.then(|| "X-MBX-APIKEY".into()),
        ws: WsRoutes {
            public: ws_route(&routing.public),
            private: ws_route(&routing.private),
            business: ws_route(&routing.business),
        },
        ws_headers: if tealstreet {
            table().ws_headers.clone()
        } else {
            BTreeMap::new()
        },
        venue_ws_headers: venue_headers(name, o),
        socket_channels: socket_channels(),
        public_ws_routes,
        private_ws_routes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> Hosts {
        serde_json::from_str::<Table>(include_str!("table.json"))
            .unwrap()
            .hosts
    }

    #[test]
    fn proxy_env_overrides_replace_every_pool_and_the_public_proxy() {
        let mut hosts = hosts();
        let url = "https://proxies.tyo.tealstreet.io/api/exchange_proxy/";
        apply_proxy_overrides(&mut hosts, Some(url.into()), Some(url.into())).unwrap();
        let expected = "https://proxies.tyo.tealstreet.io/api/exchange_proxy";
        assert!(hosts.private_proxy.values().all(|host| host == expected));
        assert_eq!(hosts.private_proxy.len(), 3);
        assert_eq!(hosts.public_cors_proxy, expected);
    }

    #[test]
    fn unset_or_empty_overrides_keep_the_table_and_bad_ones_refuse() {
        let table = hosts();
        let mut kept = hosts();
        apply_proxy_overrides(&mut kept, None, Some(" ".into())).unwrap();
        assert_eq!(kept.private_proxy, table.private_proxy);
        assert_eq!(kept.public_cors_proxy, table.public_cors_proxy);
        apply_proxy_overrides(
            &mut hosts(),
            Some("https://tealstreet.io:8443/p".into()),
            None,
        )
        .unwrap();
        for bad in [
            "http://plain.invalid",
            "https://x.invalid/?a=1",
            "proxies.tyo",
            "https://proxies.tyo.tealstreet.io.evil.invalid/api",
            "https://eviltealstreet.io/api",
            "https://user@evil.invalid/x.tealstreet.io",
            "http://proxies.tyo.tealstreet.io/api",
            "https://evil.invalid\\.tealstreet.io/api",
            "https://evil.invalid%2F.tealstreet.io/api",
            "https://evil.invalid\t.tealstreet.io/api",
        ] {
            assert!(
                apply_proxy_overrides(&mut hosts(), Some(bad.into()), None).is_err(),
                "{bad}"
            );
        }
    }
}
