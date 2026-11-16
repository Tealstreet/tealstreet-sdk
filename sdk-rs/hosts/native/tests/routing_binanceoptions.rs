use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url, route_bases,
};

const ORIGIN: &str = "https://eapi.binance.com";

#[test]
fn binanceoptions_mainnet_resolves_the_options_origin() {
    assert_eq!(rest_base_url("binanceoptions", false), Ok(ORIGIN));
}

#[test]
fn binanceoptions_hosted_testnet_refuses_instead_of_using_live_or_futures_origins() {
    assert_eq!(
        rest_base_url("binanceoptions", true),
        Err(RoutingError::NoBaseUrl("binanceoptions".into()))
    );
}

fn assert_existing_routing(base: &str) {
    let bases = route_bases().unwrap();
    for proxy in [None, Some(false), Some(true)] {
        for tunnel in [None, Some(false), Some(true)] {
            for region in [None, Some("JA"), Some("SG")] {
                let inputs = account_transport(
                    "binanceoptions",
                    &AccountTransport {
                        tealstreet_proxy: proxy,
                        tealstreet_tunnel: tunnel,
                        proxy_region: region.map(str::to_owned),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(inputs.tealstreet_proxy, Some(proxy == Some(true)));
                for public in [false, true] {
                    let inputs = RoutingInputs {
                        has_key: !public,
                        public_data_only: Some(public),
                        ..inputs.clone()
                    };
                    let plan =
                        resolve_routing_plan("binanceoptions", &inputs, base, Provider::Tealstreet)
                            .unwrap();
                    let how = tunnel == Some(true);
                    let expected = if how {
                        ""
                    } else if public {
                        &bases.public_proxy
                    } else if proxy == Some(true) {
                        &bases.private_proxy["jp"]
                    } else {
                        ""
                    };
                    assert_eq!(plan.http.prefix, expected);
                    assert_eq!(
                        plan.http.mode,
                        if how {
                            HttpRouteMode::How
                        } else {
                            HttpRouteMode::Direct
                        }
                    );
                    assert_eq!(plan.http.authorize, !expected.is_empty());
                    assert_eq!(plan.http.authorization_header, "Authorization");
                    assert!(plan.http.header_aliases.is_empty());
                    assert_eq!(
                        plan.http.tunnel_url.as_deref(),
                        how.then_some(bases.tunnel["jp"].as_str())
                    );
                    let public_route = plan.public_http.unwrap();
                    assert_eq!(
                        public_route.prefix,
                        if how { "" } else { &bases.public_proxy }
                    );
                    assert_eq!(public_route.authorize, !how);
                    assert!(public_route.header_aliases.is_empty());
                    assert!(plan.ws.public.host.is_none());
                    assert!(plan.ws.private.host.is_none());
                }
            }
        }
    }
}

#[test]
fn binanceoptions_origin_lookup_retains_the_existing_account_routing() {
    assert_existing_routing(rest_base_url("binanceoptions", false).unwrap());
}

#[test]
fn existing_options_routing_with_the_literal_origin_is_a_working_control() {
    assert_existing_routing(ORIGIN);
}

#[test]
fn other_binance_product_origins_remain_separate() {
    for (name, mainnet, testnet) in [
        (
            "binance",
            "https://fapi.binance.com",
            "https://demo-fapi.binance.com",
        ),
        (
            "binancepm",
            "https://papi.binance.com",
            "https://demo-fapi.binance.com",
        ),
        (
            "binanceinverse",
            "https://dapi.binance.com",
            "https://demo-dapi.binance.com",
        ),
        (
            "binancespot",
            "https://api.binance.com",
            "https://testnet.binance.vision",
        ),
    ] {
        assert_eq!(rest_base_url(name, false), Ok(mainnet));
        assert_eq!(rest_base_url(name, true), Ok(testnet));
    }
}
