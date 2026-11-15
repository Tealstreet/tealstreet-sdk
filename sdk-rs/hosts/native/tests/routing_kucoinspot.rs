//! Hosted Spot origin and existing KuCoin routing, without venue network I/O.
use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport, resolve_routing_plan,
    rest_base_url, route_bases,
};

const SPOT: &str = "https://api.kucoin.com";
const FUTURES: &str = "https://api-futures.kucoin.com";

#[test]
fn kucoinspot_hosted_origin_is_spot_and_keeps_futures_separate() {
    assert_eq!(rest_base_url("kucoin", false).unwrap(), FUTURES);
    assert_eq!(rest_base_url("kucoinspot", false).unwrap(), SPOT);
}

#[test]
fn kucoinspot_hosted_testnet_refuses_without_a_production_fallback() {
    for venue in ["kucoinspot", "kucoin"] {
        assert!(
            matches!(rest_base_url(venue, true), Err(RoutingError::NoBaseUrl(name)) if name == venue),
            "{venue}: hosted testnet must refuse before a routing plan is built"
        );
    }
}

fn assert_private_japan_how(base: &str, account: AccountTransport) {
    let inputs = account_transport("kucoinspot", &account).unwrap();
    // enforceWsTunnel survives unset/false saved flags, independently of region.
    assert_eq!(inputs.tealstreet_proxy, Some(true));
    assert_eq!(inputs.tealstreet_tunnel, Some(true));
    let plan = resolve_routing_plan("kucoinspot", &inputs, base, Provider::Tealstreet).unwrap();
    assert_eq!(plan.http.mode, HttpRouteMode::How);
    assert_eq!(
        plan.http.tunnel_url.as_ref(),
        Some(&route_bases().unwrap().tunnel["jp"])
    );
    assert!(plan.http.prefix.is_empty());
    assert!(!plan.http.authorize, "account token authenticates HOW only");
    assert!(plan.http.header_aliases.is_empty());
    assert_eq!(plan.http.authorization_header, "Authorization");
    for path in ["/api/v1/accounts", "/api/v1/hf/orders/active/page"] {
        let url = reqwest::Url::parse(&format!("{base}{path}")).unwrap();
        assert_eq!(url.origin().ascii_serialization(), SPOT);
        assert_eq!(url.path(), path);
    }
    // KuCoin's bullet response still owns the actual socket endpoint.
    for socket in [&plan.ws.public, &plan.ws.private, &plan.ws.business] {
        assert!(socket.host.is_none());
        assert!(socket.fallback.is_none());
        assert!(socket.relay.is_none());
    }
}

#[test]
fn kucoinspot_existing_private_routing_uses_japan_even_with_saved_flags_off() {
    for flags in [None, Some(false), Some(true)] {
        for region in [None, Some("SG"), Some("US"), Some("JP")] {
            assert_private_japan_how(
                SPOT,
                AccountTransport {
                    tealstreet_proxy: flags,
                    tealstreet_tunnel: flags,
                    proxy_region: region.map(str::to_owned),
                    ..Default::default()
                },
            );
        }
    }
}

#[test]
fn kucoinspot_hosted_origin_feeds_the_existing_private_route() {
    let base = rest_base_url("kucoinspot", false).unwrap();
    assert_private_japan_how(base, AccountTransport::default());
}

#[test]
fn kucoinspot_public_and_direct_plans_keep_the_venue_origin() {
    for provider in [Provider::Tealstreet, Provider::Direct] {
        let inputs = RoutingInputs {
            public_data_only: Some(true),
            ..Default::default()
        };
        let plan = resolve_routing_plan("kucoinspot", &inputs, SPOT, provider).unwrap();
        assert_eq!(plan.http.mode, HttpRouteMode::Direct);
        assert!(plan.http.prefix.is_empty());
        assert!(plan.http.tunnel_url.is_none());
        assert!(!plan.http.authorize);
        assert!(plan.http.header_aliases.is_empty());
    }
    let private_direct = resolve_routing_plan(
        "kucoinspot",
        &RoutingInputs {
            has_key: true,
            ..Default::default()
        },
        SPOT,
        Provider::Direct,
    )
    .unwrap();
    assert_eq!(private_direct.http.mode, HttpRouteMode::Direct);
    assert!(private_direct.http.prefix.is_empty());
    assert!(private_direct.http.tunnel_url.is_none());
    assert!(!private_direct.http.authorize);
}
