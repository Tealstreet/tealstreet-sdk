use safe_cex_native::HttpRouteMode;
use safe_cex_native::resolve::{
    AccountTransport, Provider, RoutingError, RoutingInputs, account_transport,
    resolve_routing_plan, rest_base_url,
};

#[test]
fn asterdex_mainnet_origin_matches_the_adapter() {
    assert_eq!(
        rest_base_url("asterdex", false),
        Ok("https://fapi.asterdex.com")
    );
}

#[test]
fn asterdex_testnet_has_its_own_origin() {
    assert_eq!(
        rest_base_url("asterdex", true),
        Ok("https://fapi.asterdex-testnet.com")
    );
    assert_ne!(
        rest_base_url("asterdex", true).unwrap(),
        rest_base_url("asterdex", false).unwrap()
    );
}

fn assert_existing_egress(testnet: bool, base: &str) {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../src/routing/table.json")).unwrap();
    assert_eq!(table["exchanges"]["asterdex"]["routing"]["howEgress"], "jp");
    let tunnel = table["hosts"]["tunnel"]["jp"].as_str().unwrap();
    for saved_proxy in [None, Some(false), Some(true)] {
        for saved_tunnel in [None, Some(false), Some(true)] {
            for region in [None, Some("JA"), Some("SG")] {
                let account = AccountTransport {
                    testnet: Some(testnet),
                    tealstreet_proxy: saved_proxy,
                    tealstreet_tunnel: saved_tunnel,
                    proxy_region: region.map(str::to_owned),
                    ..Default::default()
                };
                let inputs = account_transport("asterdex", &account).unwrap();
                assert_eq!(inputs.tealstreet_proxy, Some(true));
                assert_eq!(inputs.tealstreet_tunnel, Some(true));
                for public_only in [false, true] {
                    let inputs = RoutingInputs {
                        public_data_only: Some(public_only),
                        has_key: !public_only,
                        ..inputs.clone()
                    };
                    let plan =
                        resolve_routing_plan("asterdex", &inputs, base, Provider::Tealstreet)
                            .unwrap();
                    assert_eq!(plan.http.mode, HttpRouteMode::How);
                    assert_eq!(plan.http.tunnel_url.as_deref(), Some(tunnel));
                    assert!(plan.http.prefix.is_empty());
                    assert!(!plan.http.authorize);
                    assert!(plan.http.header_aliases.is_empty());
                    assert!(plan.ws.public.host.is_none());
                    assert!(plan.ws.private.host.is_none());
                }
            }
        }
    }
}

#[test]
fn asterdex_hosted_origin_lookup_preserves_the_existing_japan_tunnel() {
    for testnet in [false, true] {
        let base = rest_base_url("asterdex", testnet).unwrap();
        assert_existing_egress(testnet, base);
    }
}

#[test]
fn asterdex_routing_table_already_supports_both_adapter_origins() {
    assert_existing_egress(false, "https://fapi.asterdex.com");
    assert_existing_egress(true, "https://fapi.asterdex-testnet.com");
}

#[test]
fn phemex_and_bingx_origin_controls_are_unchanged() {
    for (venue, testnet, base) in [
        ("phemex", false, "https://api.phemex.com"),
        ("phemex", true, "https://testnet-api.phemex.com"),
        ("bingx", false, "https://open-api.bingx.com"),
    ] {
        assert_eq!(rest_base_url(venue, testnet), Ok(base));
        let inputs = account_transport(
            venue,
            &AccountTransport {
                testnet: Some(testnet),
                ..Default::default()
            },
        )
        .unwrap();
        resolve_routing_plan(venue, &inputs, base, Provider::Tealstreet).unwrap();
    }
    assert_eq!(
        rest_base_url("bingx", true),
        Err(RoutingError::NoBaseUrl("bingx".into()))
    );
}
