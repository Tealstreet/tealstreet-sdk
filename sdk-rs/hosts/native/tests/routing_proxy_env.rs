//! The proxy hosts de-aws deploys, joined as the TS resolver joins them:
//! `${proxy}/${venue URL}`, the venue URL's bytes unencoded (transport/engine.ts
//! `restRoute`; the native host's `format!("{prefix}/{url}")`, transport/http.rs).
//! Its own binary: the overrides are read once, before the table first loads.
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingInputs, account_transport, resolve_routing_plan,
};

const PRIVATE: &str = "https://proxies.tyo.tealstreet.io/api/exchange_proxy/";
const PUBLIC: &str = "https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy";

fn join(prefix: &str, url: &str) -> String {
    if prefix.is_empty() {
        url.into()
    } else {
        format!("{prefix}/{url}")
    }
}

#[test]
fn deployed_proxy_hosts_join_venue_urls_exactly_as_typescript() {
    // SAFETY: the only test in this binary, before any thread reads the process variables.
    unsafe {
        std::env::set_var("PROXY_PRIVATE_URL", PRIVATE);
        std::env::set_var("PROXY_PUBLIC_URL", PUBLIC);
    }
    let base = "https://api.binance.com";
    let account = AccountTransport {
        tealstreet_proxy: Some(true),
        ..Default::default()
    };
    let inputs = account_transport("binancespot", &account).unwrap();
    let plan = resolve_routing_plan("binancespot", &inputs, base, Provider::Tealstreet).unwrap();

    // Private (signed) route: the trailing slash is normalised, the query kept verbatim.
    let signed = "https://api.binance.com/api/v3/order?symbol=BTCUSDT&timestamp=1&signature=ab%2Bc";
    assert!(plan.http.authorize);
    assert_eq!(
        join(&plan.http.prefix, signed),
        "https://proxies.tyo.tealstreet.io/api/exchange_proxy/https://api.binance.com/api/v3/order?symbol=BTCUSDT&timestamp=1&signature=ab%2Bc"
    );

    // Public CORS read (binancespot publicHttp), token attached as in TS.
    let public = plan.public_http.unwrap();
    assert!(public.authorize);
    assert_eq!(
        join(
            &public.prefix,
            "https://api.binance.com/api/v3/depth?symbol=BTCUSDT&limit=100"
        ),
        "https://fsjnpy143pbz-tyo.tealstreet.io/api/public_proxy/https://api.binance.com/api/v3/depth?symbol=BTCUSDT&limit=100"
    );

    // A public-only account's primary route is the same public CORS join.
    let public_only = RoutingInputs {
        public_data_only: Some(true),
        ..inputs
    };
    let plan =
        resolve_routing_plan("binancespot", &public_only, base, Provider::Tealstreet).unwrap();
    assert_eq!(plan.http.prefix, PUBLIC);
}
