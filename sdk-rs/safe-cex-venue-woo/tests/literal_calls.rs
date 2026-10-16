use safe_cex_venue_woo::v3_contracts::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[test]
fn public_qualified_deserialize_cannot_bypass_string_only_unions() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/contracts.json")).unwrap();
    macro_rules! check {
        ($ty:ident, $key:literal) => {
            for value in fixture["literalSets"][$key].as_array().unwrap() {
                let token=value.as_str().unwrap();
                assert!($ty::deserialize(json!(token)).is_ok());
                assert!($ty::deserialize(json!({token:null})).is_err(), "{} public qualified decoder accepts an object", $key);
                assert!(<$ty as Deserialize>::deserialize(json!({token:null})).is_err());
            }
        };
    }
    check!(WooV3Endpoint, "WooV3Endpoint");
    check!(WooV3HttpMethod, "WooV3HttpMethod");
    check!(WooV3MigrationGate, "WooV3MigrationGate");
    check!(
        WooV3MigrationRouteRuntimeLifecycle,
        "WooV3MigrationRoute.runtimeLifecycle"
    );
    check!(WooV3PositionPositionSide, "WooV3Position.positionSide");
    check!(
        WooV3AccountInfoPositionMode,
        "WooV3AccountInfo.positionMode"
    );
    check!(WooV3PlaceOrderRequestSide, "WooV3PlaceOrderRequest.side");
    check!(WooV3PlaceOrderRequestType, "WooV3PlaceOrderRequest.type");
    check!(
        WooV3PlaceOrderRequestPositionSide,
        "WooV3PlaceOrderRequest.positionSide"
    );
    check!(
        WooV3PlaceOrderRequestMarginMode,
        "WooV3PlaceOrderRequest.marginMode"
    );
    check!(
        WooV3PlaceOrderRequestBidAskLevel,
        "WooV3PlaceOrderRequest.bidAskLevel"
    );
    check!(WooV3ListenKeyRequestType, "WooV3ListenKeyRequest.type");
}
