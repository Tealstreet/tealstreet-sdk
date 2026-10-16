use safe_cex_transport::how_config::{HOWRegion, LatencyEntry};
use safe_cex_transport::{HowEgress, ResolvedRouting, ResolvedWs, RoutingOptions};
use safe_cex_types::ExchangeOptionsBybitSiteId;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

fn record<T: DeserializeOwned>(object: Value, array: Value, observe: impl Fn(T)) {
    for decoded in [
        serde_json::from_str::<T>(&object.to_string()).unwrap(),
        serde_json::from_value::<T>(object.clone()).unwrap(),
        T::deserialize(&object).unwrap(),
        <T as Deserialize>::deserialize(object).unwrap(),
    ] {
        observe(decoded);
    }
    assert!(
        serde_json::from_str::<T>(&array.to_string()).is_err(),
        "complete ordered array must be refused by raw record decoder"
    );
    assert!(
        serde_json::from_value::<T>(array.clone()).is_err(),
        "complete ordered array must be refused by owned record decoder"
    );
    assert!(
        T::deserialize(&array).is_err(),
        "complete ordered array must be refused by borrowed record decoder"
    );
    assert!(
        <T as Deserialize>::deserialize(array).is_err(),
        "complete ordered array must be refused by qualified record decoder"
    );
    for invalid in [
        json!(null),
        json!(true),
        json!(1),
        json!("record"),
        json!([]),
    ] {
        assert!(serde_json::from_str::<T>(&invalid.to_string()).is_err());
        assert!(serde_json::from_value::<T>(invalid.clone()).is_err());
        assert!(T::deserialize(&invalid).is_err());
        assert!(<T as Deserialize>::deserialize(invalid).is_err());
    }
}

fn string_union<T: DeserializeOwned + Serialize + PartialEq + Debug>(cases: &[(&str, T)]) {
    for (literal, expected) in cases {
        let input = json!(literal);
        for decoded in [
            serde_json::from_str::<T>(&input.to_string()).unwrap(),
            serde_json::from_value::<T>(input.clone()).unwrap(),
            T::deserialize(&input).unwrap(),
            <T as Deserialize>::deserialize(input).unwrap(),
        ] {
            assert_eq!(&decoded, expected, "native variant identity for {literal}");
            assert_eq!(serde_json::to_value(decoded).unwrap(), json!(literal));
        }
        let tagged = json!({(*literal): null});
        assert!(
            serde_json::from_str::<T>(&tagged.to_string()).is_err(),
            "valid tagged map must be refused by raw string decoder for {literal}"
        );
        assert!(serde_json::from_value::<T>(tagged.clone()).is_err());
        assert!(T::deserialize(&tagged).is_err());
        assert!(<T as Deserialize>::deserialize(tagged).is_err());
    }
}

#[test]
fn how_region_requires_strings_and_preserves_native_variants() {
    string_union(&[("JA", HOWRegion::JA), ("SG", HOWRegion::SG)]);
}

#[test]
fn how_egress_requires_strings_and_preserves_native_variants() {
    string_union(&[("jp", HowEgress::Jp), ("sg", HowEgress::Sg)]);
}

#[test]
fn latency_entry_requires_an_object_and_preserves_native_fields() {
    record::<LatencyEntry>(
        json!({"latencyMs":1.25,"source":"server","cached":true,"sampleCount":3,"measuredAt":17,"updatedAt":29}),
        json!([1.25, "server", true, 3, 17, 29]),
        |row| {
            assert_eq!(
                row.latency_ms.as_f64().unwrap().to_bits(),
                1.25_f64.to_bits()
            );
            assert_eq!(row.source, json!("server"));
            assert_eq!(row.cached, json!(true));
            assert_eq!(row.sample_count, json!(3));
            assert_eq!(row.measured_at, json!(17));
            assert_eq!(row.updated_at, 29);
        },
    );
}

fn ws(prefix: &str, tunnelled: bool) -> Value {
    json!({"tunnelled":tunnelled,"host":format!("{prefix}-host"),"fallback":format!("{prefix}-fallback"),"relay":format!("{prefix}-relay"),"readonly":format!("{prefix}-readonly")})
}

fn observe_ws(row: ResolvedWs, prefix: &str, tunnelled: bool) {
    assert_eq!(row.tunnelled, tunnelled);
    assert_eq!(row.host.as_deref(), Some(format!("{prefix}-host").as_str()));
    assert_eq!(
        row.fallback.as_deref(),
        Some(format!("{prefix}-fallback").as_str())
    );
    assert_eq!(
        row.relay.as_deref(),
        Some(format!("{prefix}-relay").as_str())
    );
    assert_eq!(
        row.readonly.as_deref(),
        Some(format!("{prefix}-readonly").as_str())
    );
}

#[test]
fn resolved_ws_requires_an_object_and_preserves_native_fields() {
    record::<ResolvedWs>(
        ws("public", true),
        json!([
            true,
            "public-host",
            "public-fallback",
            "public-relay",
            "public-readonly"
        ]),
        |row| observe_ws(row, "public", true),
    );
    for raw in [
        r#"{"tunnelled":false}"#,
        r#"{"tunnelled":false,"host":null,"fallback":null,"relay":null,"readonly":null}"#,
    ] {
        let row: ResolvedWs = serde_json::from_str(raw).unwrap();
        assert!(!row.tunnelled);
        assert!(
            row.host.is_none()
                && row.fallback.is_none()
                && row.relay.is_none()
                && row.readonly.is_none()
        );
    }
}

#[test]
fn resolved_routing_requires_an_object_and_preserves_native_fields() {
    record::<ResolvedRouting>(
        json!({"restProxyHost":"native-rest","howEgress":"sg","wsPublic":ws("public",false),"wsPrivate":ws("private",true),"wsBusiness":ws("business",false)}),
        json!([
            "native-rest",
            "sg",
            ws("public", false),
            ws("private", true),
            ws("business", false)
        ]),
        |row| {
            assert_eq!(row.rest_proxy_host, "native-rest");
            assert_eq!(row.how_egress, HowEgress::Sg);
            observe_ws(row.ws_public, "public", false);
            observe_ws(row.ws_private, "private", true);
            observe_ws(row.ws_business, "business", false);
        },
    );
}

#[test]
fn routing_options_require_an_object_and_preserve_native_fields() {
    record::<RoutingOptions>(
        json!({"tealstreetProxy":true,"tealstreetWsRoute":false,"proxyRegion":"JA","bybitSiteId":"BRA_BTL"}),
        json!([true, false, "JA", "BRA_BTL"]),
        |row| {
            assert!(row.tealstreet_proxy);
            assert!(!row.tealstreet_ws_route);
            assert_eq!(row.proxy_region.as_deref(), Some("JA"));
            assert_eq!(row.bybit_site_id, Some(ExchangeOptionsBybitSiteId::BraBtl));
        },
    );
    let row: RoutingOptions = serde_json::from_str("{}").unwrap();
    assert!(!row.tealstreet_proxy && !row.tealstreet_ws_route);
    assert!(row.proxy_region.is_none() && row.bybit_site_id.is_none());
    let row: RoutingOptions = serde_json::from_str(r#"{"tealstreetWsRoute":true}"#).unwrap();
    assert!(!row.tealstreet_proxy && row.tealstreet_ws_route);
}

#[test]
fn resolution_transport_requires_object_and_preserves_optional_flags() {
    use safe_cex_transport::transport_resolution::ResolvedTransport;
    record::<ResolvedTransport>(
        json!({"tealstreetProxy":true,"tealstreetTunnel":false,"proxyRegion":"JA"}),
        json!([true, false, "JA"]),
        |row| {
            assert_eq!(row.tealstreet_proxy, Some(true));
            assert_eq!(row.tealstreet_tunnel, Some(false));
            assert_eq!(row.proxy_region.as_deref(), Some("JA"));
        },
    );
    for raw in [
        "{}",
        r#"{"tealstreetProxy":null,"tealstreetTunnel":null,"proxyRegion":null}"#,
    ] {
        let row: ResolvedTransport = serde_json::from_str(raw).unwrap();
        assert!(row.tealstreet_proxy.is_none());
        assert!(row.tealstreet_tunnel.is_none());
        assert!(row.proxy_region.is_none());
    }
}

#[test]
fn resolution_metadata_requires_object_and_preserves_native_region_order() {
    use safe_cex_transport::transport_resolution::TransportMetadata;
    record::<TransportMetadata>(
        json!({"requiresCors":true,"enforceWsTunnel":false,"supportsTunnel":true,"privateProxyRegions":{"SG":["binance"],"JA":["okx"]}}),
        json!([true, false, true, {"SG":["binance"],"JA":["okx"]}]),
        |row| {
            assert!(row.requires_cors);
            assert_eq!(row.enforce_ws_tunnel, Some(false));
            assert_eq!(row.supports_tunnel, Some(true));
            assert_eq!(
                row.private_proxy_regions.keys().collect::<Vec<_>>(),
                vec!["SG", "JA"]
            );
            assert_eq!(
                row.private_proxy_regions.get("SG").unwrap(),
                &vec!["binance".to_owned()]
            );
            assert_eq!(
                row.private_proxy_regions.get("JA").unwrap(),
                &vec!["okx".to_owned()]
            );
        },
    );
    let row: TransportMetadata =
        serde_json::from_str(r#"{"requiresCors":false,"privateProxyRegions":{}}"#).unwrap();
    assert!(!row.requires_cors);
    assert!(row.enforce_ws_tunnel.is_none() && row.supports_tunnel.is_none());
    assert!(row.private_proxy_regions.is_empty());
    assert!(serde_json::from_str::<TransportMetadata>("{}").is_err());
}
