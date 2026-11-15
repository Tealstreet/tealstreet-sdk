use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url,
};

const ORIGINS: [(&str, &str); 2] = [
    ("coinbase", "https://drb.coinbase.com/api/v2"),
    ("coinbasespot", "https://api.coinbase.com"),
];

#[test]
fn coinbase_perpetuals_resolve_the_native_drb_gateway() {
    assert_eq!(rest_base_url("coinbase", false), Ok(ORIGINS[0].1));
}

#[test]
fn coinbase_spot_resolves_advanced_trade() {
    assert_eq!(rest_base_url("coinbasespot", false), Ok(ORIGINS[1].1));
}

#[test]
fn hosted_testnet_refuses_instead_of_aliasing_either_live_origin() {
    for (venue, _) in ORIGINS {
        assert_eq!(
            rest_base_url(venue, true),
            Err(RoutingError::NoBaseUrl(venue.into()))
        );
    }
}

fn assert_existing_routing(venue: &str, base: &str) {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../src/routing/table.json")).unwrap();
    assert_eq!(table["exchanges"][venue]["routing"]["restPool"], "asia");
    assert_eq!(table["exchanges"][venue]["routing"]["howEgress"], "sg");
    for saved_proxy in [None, Some(false), Some(true)] {
        for saved_tunnel in [None, Some(false), Some(true)] {
            for region in [None, Some("JA"), Some("SG")] {
                let account = AccountTransport {
                    tealstreet_proxy: saved_proxy,
                    tealstreet_tunnel: saved_tunnel,
                    proxy_region: region.map(str::to_owned),
                    ..Default::default()
                };
                let inputs = account_transport(venue, &account).unwrap();
                assert_eq!(inputs.tealstreet_proxy, Some(true));
                let how = saved_tunnel == Some(true);
                let pool = if region == Some("JA") { "jp" } else { "asia" };
                for public_only in [false, true] {
                    let inputs = RoutingInputs {
                        public_data_only: Some(public_only),
                        has_key: !public_only,
                        ..inputs.clone()
                    };
                    let plan =
                        resolve_routing_plan(venue, &inputs, base, Provider::Tealstreet).unwrap();
                    if how {
                        assert_eq!(plan.http.mode, HttpRouteMode::How);
                        assert_eq!(
                            plan.http.tunnel_url.as_deref(),
                            table["hosts"]["tunnel"]["sg"].as_str()
                        );
                        assert!(plan.http.prefix.is_empty());
                        assert!(!plan.http.authorize);
                        assert!(plan.http.header_aliases.is_empty());
                    } else {
                        assert_eq!(plan.http.mode, HttpRouteMode::Direct);
                        assert_eq!(
                            Some(plan.http.prefix.as_str()),
                            table["hosts"]["privateProxy"][pool].as_str()
                        );
                        assert!(plan.http.authorize);
                        assert!(plan.http.tunnel_url.is_none());
                        assert_eq!(
                            plan.http
                                .header_aliases
                                .get("Authorization")
                                .map(String::as_str),
                            Some("Coinbase-Authorization")
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn origin_lookup_preserves_existing_coinbase_egress_and_header_policy() {
    for (venue, _) in ORIGINS {
        let base = rest_base_url(venue, false).unwrap();
        assert_existing_routing(venue, base);
    }
}

#[test]
fn existing_routing_already_supports_both_real_origins() {
    for (venue, base) in ORIGINS {
        assert_existing_routing(venue, base);
    }
}

#[test]
fn phemex_and_bingx_origin_controls_are_unchanged() {
    for (venue, testnet, base) in [
        ("phemex", false, "https://api.phemex.com"),
        ("phemex", true, "https://testnet-api.phemex.com"),
        ("bingx", false, "https://open-api.bingx.com"),
    ] {
        assert_eq!(rest_base_url(venue, testnet), Ok(base));
    }
    assert_eq!(
        rest_base_url("bingx", true),
        Err(RoutingError::NoBaseUrl("bingx".into()))
    );
}
