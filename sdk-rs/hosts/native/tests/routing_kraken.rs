//! Hosted Futures uses the native root and its existing table-selected HOW route.
use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url,
};

const LIVE: &str = "https://futures.kraken.com";

#[test]
fn kraken_futures_mainnet_resolves_the_native_host_root() {
    assert_eq!(rest_base_url("kraken", false), Ok(LIVE));
}

#[test]
fn hosted_testnet_refuses_the_retired_demo_instead_of_aliasing_mainnet() {
    assert_eq!(
        rest_base_url("kraken", true),
        Err(RoutingError::NoBaseUrl("kraken".into()))
    );
}

fn assert_japan_how(base: &str) {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../src/routing/table.json")).unwrap();
    let policy = &table["exchanges"]["kraken"];
    assert_eq!(policy["routing"]["howEgress"], "jp");
    assert_eq!(policy["enforceWsTunnel"], true);
    assert_eq!(policy["supportsTunnel"], false);
    for proxy in [None, Some(false), Some(true)] {
        for tunnel in [None, Some(false), Some(true)] {
            for region in [None, Some("JA"), Some("SG")] {
                let input = account_transport(
                    "kraken",
                    &AccountTransport {
                        tealstreet_proxy: proxy,
                        tealstreet_tunnel: tunnel,
                        proxy_region: region.map(str::to_owned),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(input.tealstreet_proxy, Some(true));
                assert_eq!(input.tealstreet_tunnel, Some(true));
                for public in [false, true] {
                    let input = RoutingInputs {
                        public_data_only: Some(public),
                        has_key: !public,
                        ..input.clone()
                    };
                    let plan =
                        resolve_routing_plan("kraken", &input, base, Provider::Tealstreet).unwrap();
                    assert_eq!(plan.http.mode, HttpRouteMode::How);
                    assert_eq!(
                        plan.http.tunnel_url.as_deref(),
                        table["hosts"]["tunnel"]["jp"].as_str()
                    );
                    assert!(plan.http.prefix.is_empty());
                    assert!(!plan.http.authorize);
                    assert!(plan.http.headers.is_empty());
                    assert!(plan.http.header_aliases.is_empty());
                    assert_eq!(plan.http.authorization_header, "Authorization");
                }
            }
        }
    }
}

#[test]
fn lookup_preserves_japan_how_for_all_saved_flags_and_account_modes() {
    assert_japan_how(rest_base_url("kraken", false).unwrap());
}

#[test]
fn existing_policy_already_routes_the_literal_native_origin_through_japan_how() {
    assert_japan_how(LIVE);
}

#[test]
fn native_derivatives_history_and_charts_paths_keep_their_full_prefixes() {
    let base = rest_base_url("kraken", false).unwrap();
    for (path, expected, query) in [
        (
            "/derivatives/api/v3/accounts",
            "/derivatives/api/v3/accounts",
            None,
        ),
        (
            "/api/history/v3/account-log?from=1&cursor=a%2Bb",
            "/api/history/v3/account-log",
            Some("from=1&cursor=a%2Bb"),
        ),
        (
            "/api/charts/v1/trade/PI_XBTUSD/1m?from=1&to=2",
            "/api/charts/v1/trade/PI_XBTUSD/1m",
            Some("from=1&to=2"),
        ),
    ] {
        let url = reqwest::Url::parse(&format!("{base}{path}")).unwrap();
        assert_eq!(url.host_str(), Some("futures.kraken.com"));
        assert_eq!(url.path(), expected);
        assert_eq!(url.query(), query);
    }
}

#[test]
fn krakenspot_and_unknown_futures_aliases_remain_unavailable() {
    for venue in ["krakenspot", "Kraken", "kraken-futures"] {
        for testnet in [false, true] {
            assert_eq!(
                rest_base_url(venue, testnet),
                Err(RoutingError::NoBaseUrl(venue.into()))
            );
        }
    }
}
