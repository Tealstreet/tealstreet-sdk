//! The live exchange boxes point both proxy overrides at the fleet's load balancer,
//! which admits by network and forwards every header: no route to it, private or
//! public, may carry our proxy JWT.
//! Its own binary: the overrides are read once, before the table first loads.
use safe_cex_native::resolve::{
    AccountTransport, Provider, account_transport, resolve_routing_plan,
};

const FLEET: &str = "https://proxies.tyo.tealstreet.io";

#[test]
fn no_route_to_the_fleet_carries_our_token() {
    // SAFETY: the only test in this binary, before any thread reads the process variables.
    unsafe {
        std::env::set_var("PROXY_PRIVATE_URL", FLEET);
        std::env::set_var("PROXY_PUBLIC_URL", FLEET);
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
    assert_eq!(plan.http.prefix, FLEET);
    assert!(!plan.http.authorize);
    // Binance's public CORS reads go to the same host, so they carry no token either.
    let public = plan.public_http.unwrap();
    assert_eq!(public.prefix, FLEET);
    assert!(!public.authorize);

    // OKX read our Bearer as an OAuth token on third-party-app keys (53000).
    let inputs = account_transport("okx", &account).unwrap();
    let plan =
        resolve_routing_plan("okx", &inputs, "https://www.okx.com", Provider::Tealstreet).unwrap();
    assert_eq!(plan.http.prefix, FLEET);
    assert!(!plan.http.authorize);
}
