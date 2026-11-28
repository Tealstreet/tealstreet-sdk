//! Bitrue perpetual REST origin and existing Japan HOW policy, without venue I/O.
use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url, route_bases,
};

const LIVE: &str = "https://fapi.bitrue.com";

#[test]
fn bitrue_mainnet_origin_is_the_native_perpetual_host_root() {
    assert_eq!(rest_base_url("bitrue", false), Ok(LIVE));
}

#[test]
fn bitrue_hosted_testnet_refuses_without_a_live_alias() {
    assert_eq!(
        rest_base_url("bitrue", true),
        Err(RoutingError::NoBaseUrl("bitrue".into()))
    );
}

fn assert_existing_how(base: &str) {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../src/routing/table.json")).unwrap();
    let policy = &table["exchanges"]["bitrue"];
    assert_eq!(policy["routing"]["howEgress"], "jp");
    assert_eq!(policy["routing"]["restPool"], "asia");
    assert_eq!(policy["enforceWsTunnel"], true);
    assert_eq!(policy["supportsTunnel"], false);
    for proxy in [None, Some(false), Some(true)] {
        for tunnel in [None, Some(false), Some(true)] {
            for region in [None, Some("JA"), Some("SG")] {
                let inputs = account_transport(
                    "bitrue",
                    &AccountTransport {
                        tealstreet_proxy: proxy,
                        tealstreet_tunnel: tunnel,
                        proxy_region: region.map(str::to_owned),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert_eq!(inputs.tealstreet_proxy, Some(true));
                assert_eq!(inputs.tealstreet_tunnel, Some(true));
                for public in [false, true] {
                    let inputs = RoutingInputs {
                        has_key: !public,
                        public_data_only: Some(public),
                        ..inputs.clone()
                    };
                    let plan = resolve_routing_plan("bitrue", &inputs, base, Provider::Tealstreet)
                        .unwrap();
                    assert_eq!(plan.http.mode, HttpRouteMode::How);
                    assert_eq!(
                        plan.http.tunnel_url.as_ref(),
                        Some(&route_bases().unwrap().tunnel["jp"])
                    );
                    assert!(plan.http.prefix.is_empty());
                    assert!(
                        !plan.http.authorize,
                        "the account token authenticates HOW only"
                    );
                    assert_eq!(plan.http.authorization_header, "Authorization");
                    assert!(plan.http.header_aliases.is_empty());
                    assert!(plan.http.headers.is_empty());
                    // Native Bitrue owns its public socket and listen-key private socket.
                    for ws in [&plan.ws.public, &plan.ws.private, &plan.ws.business] {
                        assert!(ws.host.is_none());
                        assert!(ws.fallback.is_none());
                        assert!(ws.relay.is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn bitrue_origin_feeds_japan_how_for_every_saved_flag_and_region() {
    assert_existing_how(rest_base_url("bitrue", false).unwrap());
}

#[test]
fn existing_bitrue_policy_already_routes_the_literal_origin_through_japan_how() {
    assert_existing_how(LIVE);
}

#[test]
fn direct_provider_refuses_normalized_bitrue_hosted_routing() {
    let inputs = account_transport("bitrue", &AccountTransport::default()).unwrap();
    assert!(matches!(
        resolve_routing_plan("bitrue", &inputs, LIVE, Provider::Direct),
        Err(RoutingError::DirectRefusesTealstreet(name)) if name == "bitrue"
    ));
}

#[test]
fn unrelated_missing_origins_and_unknown_bitrue_aliases_stay_closed() {
    for venue in ["txflow", "Bitrue", "bitrue-spot"] {
        for testnet in [false, true] {
            assert_eq!(
                rest_base_url(venue, testnet),
                Err(RoutingError::NoBaseUrl(venue.into()))
            );
        }
    }
}
