//! Spot shares the documented BingX mainnet origin, never a testnet fallback.
use safe_cex_native::resolve::{
    Provider, RoutingError, RoutingInputs, resolve_routing_plan, rest_base_url,
};
const ORIGIN: &str = "https://open-api.bingx.com";
#[test]
fn spot_mainnet_origin_matches_perpetual_rest_without_changing_its_value() {
    assert_eq!(rest_base_url("bingx", false).unwrap(), ORIGIN);
    assert_eq!(rest_base_url("bingxspot", false).unwrap(), ORIGIN);
}
#[test]
fn both_bingx_products_refuse_testnet_without_a_live_fallback() {
    for name in ["bingx", "bingxspot"] {
        assert_eq!(
            rest_base_url(name, true),
            Err(RoutingError::NoBaseUrl(name.into()))
        );
    }
}
#[test]
fn origin_lookup_preserves_the_existing_spot_route() {
    for public in [false, true] {
        for tunnel in [None, Some(false), Some(true)] {
            let inputs = RoutingInputs {
                has_key: !public,
                public_data_only: Some(public),
                tealstreet_tunnel: tunnel,
                ..Default::default()
            };
            let working =
                resolve_routing_plan("bingxspot", &inputs, ORIGIN, Provider::Tealstreet).unwrap();
            let found = rest_base_url("bingxspot", false).unwrap();
            let actual =
                resolve_routing_plan("bingxspot", &inputs, found, Provider::Tealstreet).unwrap();
            assert_eq!(
                serde_json::to_string(&actual).unwrap(),
                serde_json::to_string(&working).unwrap()
            );
        }
    }
}
