//! A malformed base override fails the table load, so startup refuses it.
//! Its own binary: the overrides are read once, before the table first loads.
use safe_cex_native::resolve::route_bases;

#[test]
fn a_malformed_tunnel_override_refuses_with_its_name() {
    // SAFETY: the only test in this binary, before any thread reads the process variables.
    unsafe {
        std::env::set_var(
            "PROXY_TUNNEL_URL",
            "https://tunnel.vpc.tealstreet.io/tunnel",
        )
    };
    let error = route_bases().err().expect("refused").to_string();
    assert_eq!(
        error,
        "invalid routing table: PROXY_TUNNEL_URL must be a wss URL on a tealstreet.io host, without query or fragment"
    );
}
