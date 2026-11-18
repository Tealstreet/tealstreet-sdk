//! The fleet is recognized by its host, not by one override's exact text: a public
//! override on the load balancer's host under another path carries no token either.
//! Its own binary: the overrides are read once, before the table first loads.
use safe_cex_native::resolve::{
    AccountTransport, Provider, account_transport, resolve_routing_plan,
};

const PRIVATE: &str = "https://proxies.tyo.tealstreet.io/api/exchange_proxy";
const PUBLIC: &str = "https://proxies.tyo.tealstreet.io/api/public_proxy";

#[test]
fn a_public_route_on_the_fleets_host_carries_no_token() {
    // SAFETY: the only test in this binary, before any thread reads the process variables.
    unsafe {
        std::env::set_var("PROXY_PRIVATE_URL", PRIVATE);
        std::env::set_var("PROXY_PUBLIC_URL", PUBLIC);
    }
    let account = AccountTransport {
        tealstreet_proxy: Some(true),
        ..Default::default()
    };
    let inputs = account_transport("binancespot", &account).unwrap();
    let plan = resolve_routing_plan(
        "binancespot",
        &inputs,
        "https://api.binance.com",
        Provider::Tealstreet,
    )
    .unwrap();
    assert_eq!(plan.http.prefix, PRIVATE);
    assert!(!plan.http.authorize);
    let public = plan.public_http.unwrap();
    assert_eq!(public.prefix, PUBLIC);
    assert!(!public.authorize);
}
