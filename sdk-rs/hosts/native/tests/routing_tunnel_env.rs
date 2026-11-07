//! `PROXY_TUNNEL_URL` moves every tunnel route the native resolver builds.
//! Its own binary: the overrides are read once, before the table first loads.
use safe_cex_native::resolve::{
    AccountTransport, Provider, account_transport, resolve_routing_plan, rest_base_url, route_bases,
};

const TUNNEL: &str = "wss://tunnel.vpc.tealstreet.io/tunnel";
const TABLE_TUNNEL: &str = "wss://fsjnpy143pbz-tyo.tealstreet.io/tunnel";

#[test]
fn every_tunnelled_route_uses_the_deployed_tunnel() {
    // SAFETY: the only test in this binary, before any thread reads the process variables.
    unsafe { std::env::set_var("PROXY_TUNNEL_URL", TUNNEL) };
    let bases = route_bases().unwrap();
    assert!(bases.tunnel.values().all(|host| host == TUNNEL));
    assert!(bases.to_string().contains(TUNNEL));

    let account = AccountTransport {
        tealstreet_proxy: Some(true),
        tealstreet_tunnel: Some(true),
        ..Default::default()
    };
    let mut tunnelled = 0;
    for name in ["bitget", "bitgetusdc", "blofin", "okx", "okxeu"] {
        let inputs = account_transport(name, &account).unwrap();
        let base = rest_base_url(name, false).unwrap();
        let plan = resolve_routing_plan(name, &inputs, base, Provider::Tealstreet).unwrap();
        let plan = serde_json::to_string(&plan).unwrap();
        assert!(!plan.contains(TABLE_TUNNEL), "{name}: {plan}");
        tunnelled += usize::from(plan.contains(TUNNEL));
    }
    assert_eq!(tunnelled, 5);
}
