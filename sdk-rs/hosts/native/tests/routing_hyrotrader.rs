//! HyroTrader's dedicated challenge origin and existing egress, without network IO.
use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url, route_bases,
};

fn origin() -> &'static str {
    rest_base_url("hyrotrader", false).unwrap()
}

fn assert_native_sockets(plan: &safe_cex_native::RoutingPlan) {
    for route in [&plan.ws.public, &plan.ws.private, &plan.ws.business] {
        assert!(route.host.is_none());
        assert!(route.fallback.is_none());
        assert!(route.relay.is_none());
    }
    assert!(plan.http.header_aliases.is_empty());
    assert_eq!(plan.http.authorization_header, "Authorization");
}

#[test]
fn hyrotrader_origin_matches_the_ts_challenge_default_without_bybit_fallback() {
    let source = include_str!("../../../../packages/safe-cex/src/exchanges/bybit/bybit.types.ts");
    let expected = source
        .split("export const HYROTRADER_DEFAULT_PROD_REST_ORIGIN = '")
        .nth(1)
        .unwrap()
        .split('\'')
        .next()
        .unwrap();
    assert_eq!(origin(), expected);
    assert_ne!(origin(), rest_base_url("bybit", false).unwrap());
    for path in ["/v5/account/wallet-balance", "/v5/order/realtime"] {
        let url = reqwest::Url::parse(&format!("{}{path}", origin())).unwrap();
        assert_eq!(url.origin().ascii_serialization(), expected);
        assert_eq!(url.path(), path);
    }
}

#[test]
fn hyrotrader_hosted_testnet_refuses_instead_of_reusing_challenge_or_bybit() {
    assert!(
        matches!(rest_base_url("hyrotrader", true), Err(RoutingError::NoBaseUrl(name)) if name == "hyrotrader")
    );
}

#[test]
fn hyrotrader_default_and_direct_routes_keep_native_http_and_sockets() {
    for provider in [Provider::Tealstreet, Provider::Direct] {
        for public in [false, true] {
            for saved in [None, Some(false)] {
                let inputs = account_transport(
                    "hyrotrader",
                    &AccountTransport {
                        tealstreet_proxy: saved,
                        tealstreet_tunnel: saved,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(inputs.tealstreet_proxy, Some(false));
                assert_ne!(inputs.tealstreet_tunnel, Some(true));
                let inputs = RoutingInputs {
                    has_key: !public,
                    public_data_only: Some(public),
                    ..inputs
                };
                let plan = resolve_routing_plan("hyrotrader", &inputs, origin(), provider).unwrap();
                assert_eq!(plan.http.mode, HttpRouteMode::Direct);
                assert!(plan.http.prefix.is_empty());
                assert!(!plan.http.authorize);
                assert!(plan.http.tunnel_url.is_none());
                assert_native_sockets(&plan);
            }
        }
    }
}

#[test]
fn hyrotrader_opt_in_proxy_preserves_the_existing_regional_pool() {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../src/routing/table.json")).unwrap();
    assert_eq!(
        table["exchanges"]["hyrotrader"]["routing"]["restPool"],
        "asia"
    );
    for (region, pool) in [(None, "asia"), (Some("SG"), "asia"), (Some("JA"), "jp")] {
        let inputs = account_transport(
            "hyrotrader",
            &AccountTransport {
                tealstreet_proxy: Some(true),
                tealstreet_tunnel: Some(false),
                proxy_region: region.map(str::to_owned),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(inputs.tealstreet_proxy, Some(true));
        assert_eq!(inputs.tealstreet_tunnel, Some(false));
        let plan =
            resolve_routing_plan("hyrotrader", &inputs, origin(), Provider::Tealstreet).unwrap();
        assert_eq!(plan.http.mode, HttpRouteMode::Direct);
        assert_eq!(
            plan.http.prefix,
            table["hosts"]["privateProxy"][pool].as_str().unwrap()
        );
        assert!(plan.http.authorize);
        assert!(plan.http.tunnel_url.is_none());
        assert_native_sockets(&plan);
    }
}

#[test]
fn hyrotrader_saved_tunnel_retains_singapore_how_without_socket_rewrites() {
    for region in [None, Some("JA"), Some("SG")] {
        for proxy in [None, Some(false), Some(true)] {
            let inputs = account_transport(
                "hyrotrader",
                &AccountTransport {
                    tealstreet_proxy: proxy,
                    tealstreet_tunnel: Some(true),
                    proxy_region: region.map(str::to_owned),
                    ..Default::default()
                },
            )
            .unwrap();
            let plan = resolve_routing_plan("hyrotrader", &inputs, origin(), Provider::Tealstreet)
                .unwrap();
            assert_eq!(plan.http.mode, HttpRouteMode::How);
            assert_eq!(
                plan.http.tunnel_url.as_ref(),
                Some(&route_bases().unwrap().tunnel["sg"])
            );
            assert!(plan.http.prefix.is_empty());
            assert!(
                !plan.http.authorize,
                "the account token authenticates HOW only"
            );
            assert_native_sockets(&plan);
        }
    }
}
