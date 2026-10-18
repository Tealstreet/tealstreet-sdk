use safe_cex_venue_woo::{types::*, v3_contracts::*};
use serde::Deserialize;
use serde_json::Value;
fn objects(value: &Value, out: &mut Vec<Value>) {
    match value {
        Value::Object(m) => {
            out.push(value.clone());
            for v in m.values() {
                objects(v, out);
            }
        }
        Value::Array(a) => {
            for v in a {
                objects(v, out);
            }
        }
        _ => {}
    }
}
#[test]
fn records_reject_positional_arrays_through_public_calls() {
    let mut candidates = Vec::new();
    objects(
        &serde_json::from_str::<Value>(include_str!("fixtures/types.json")).unwrap(),
        &mut candidates,
    );
    objects(
        &serde_json::from_str::<Value>(include_str!("fixtures/contracts.json")).unwrap(),
        &mut candidates,
    );
    let mut widened = Vec::new();
    {
        let fields = [
            "applicationId",
            "account",
            "alias",
            "accountMode",
            "positionMode",
            "leverage",
            "takerFeeRate",
            "makerFeeRate",
            "interestRate",
            "futuresTakerFeeRate",
            "futuresMakerFeeRate",
            "otpauth",
            "marginRatio",
            "openMarginRatio",
            "initialMarginRatio",
            "maintenanceMarginRatio",
            "totalCollateral",
            "freeCollateral",
            "totalAccountValue",
            "totalVaultValue",
            "totalStakingValue",
            "referrerID",
            "accountType",
            "totalLaunchpadVaultValue",
            "totalEarnValue",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <AccountResponse>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: AccountResponse"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <AccountResponse>::deserialize(array.clone()).is_ok() {
            widened.push("AccountResponse");
        }
        assert!(<AccountResponse as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "success",
            "timestamp",
            "order_id",
            "order_type",
            "order_price",
            "order_quantity",
            "order_amount",
            "client_order_id",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <PlaceOrderResponse>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: PlaceOrderResponse"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <PlaceOrderResponse>::deserialize(array.clone()).is_ok() {
            widened.push("PlaceOrderResponse");
        }
        assert!(<PlaceOrderResponse as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["orderId"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <PlaceAlgoOrderResponseDataRowsItem>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: PlaceAlgoOrderResponseDataRowsItem"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <PlaceAlgoOrderResponseDataRowsItem>::deserialize(array.clone()).is_ok() {
            widened.push("PlaceAlgoOrderResponseDataRowsItem");
        }
        assert!(
            <PlaceAlgoOrderResponseDataRowsItem as Deserialize>::deserialize(valid.clone()).is_ok()
        );
    }
    {
        let fields = ["rows"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <PlaceAlgoOrderResponseData>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: PlaceAlgoOrderResponseData"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <PlaceAlgoOrderResponseData>::deserialize(array.clone()).is_ok() {
            widened.push("PlaceAlgoOrderResponseData");
        }
        assert!(<PlaceAlgoOrderResponseData as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["success", "timestamp", "data"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <PlaceAlgoOrderResponse>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: PlaceAlgoOrderResponse"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <PlaceAlgoOrderResponse>::deserialize(array.clone()).is_ok() {
            widened.push("PlaceAlgoOrderResponse");
        }
        assert!(<PlaceAlgoOrderResponse as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["position_side", "leverage"];
        let mut valid = serde_json::Map::new();
        for v in candidates.iter().filter(|v| {
            <GetFuturesLeverageResponseDataDetailsItem>::deserialize((*v).clone()).is_ok()
        }) {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetFuturesLeverageResponseDataDetailsItem"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetFuturesLeverageResponseDataDetailsItem>::deserialize(array.clone()).is_ok() {
            widened.push("GetFuturesLeverageResponseDataDetailsItem");
        }
        assert!(
            <GetFuturesLeverageResponseDataDetailsItem as Deserialize>::deserialize(valid.clone())
                .is_ok()
        );
    }
    {
        let fields = [
            "symbol",
            "margin_mode",
            "default_margin_mode",
            "position_mode",
            "details",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetFuturesLeverageResponseData>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetFuturesLeverageResponseData"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetFuturesLeverageResponseData>::deserialize(array.clone()).is_ok() {
            widened.push("GetFuturesLeverageResponseData");
        }
        assert!(
            <GetFuturesLeverageResponseData as Deserialize>::deserialize(valid.clone()).is_ok()
        );
    }
    {
        let fields = ["success", "data", "timestamp"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetFuturesLeverageResponse>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetFuturesLeverageResponse"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetFuturesLeverageResponse>::deserialize(array.clone()).is_ok() {
            widened.push("GetFuturesLeverageResponse");
        }
        assert!(<GetFuturesLeverageResponse as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "open",
            "close",
            "low",
            "high",
            "volume",
            "amount",
            "symbol",
            "type",
            "start_timestamp",
            "end_timestamp",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetKlineResponseDataRowsItem>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetKlineResponseDataRowsItem"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetKlineResponseDataRowsItem>::deserialize(array.clone()).is_ok() {
            widened.push("GetKlineResponseDataRowsItem");
        }
        assert!(<GetKlineResponseDataRowsItem as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["total", "records_per_page", "current_page"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetKlineResponseDataMeta>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetKlineResponseDataMeta"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetKlineResponseDataMeta>::deserialize(array.clone()).is_ok() {
            widened.push("GetKlineResponseDataMeta");
        }
        assert!(<GetKlineResponseDataMeta as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["rows", "meta"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetKlineResponseData>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetKlineResponseData"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetKlineResponseData>::deserialize(array.clone()).is_ok() {
            widened.push("GetKlineResponseData");
        }
        assert!(<GetKlineResponseData as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["success", "data", "timestamp"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <GetKlineResponse>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: GetKlineResponse"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <GetKlineResponse>::deserialize(array.clone()).is_ok() {
            widened.push("GetKlineResponse");
        }
        assert!(<GetKlineResponse as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "id",
            "legacyMethod",
            "legacyPath",
            "v3Method",
            "v3Path",
            "gate",
            "consumer",
            "runtimeLifecycle",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3MigrationRoute>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3MigrationRoute"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3MigrationRoute>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3MigrationRoute");
        }
        assert!(<WooV3MigrationRoute as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["success", "data", "timestamp"];
        let mut valid = serde_json::Map::new();
        for v in candidates.iter().filter(|v| {
            <WooV3Response<safe_cex_num::numeric_ingress::JsNumberInput>>::deserialize((*v).clone())
                .is_ok()
        }) {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3Response"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3Response<safe_cex_num::numeric_ingress::JsNumberInput>>::deserialize(array.clone())
            .is_ok()
        {
            widened.push("WooV3Response");
        }
        assert!(<WooV3Response<safe_cex_num::numeric_ingress::JsNumberInput> as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["total", "recordsPerPage", "currentPage"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3PageMeta>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3PageMeta"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3PageMeta>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3PageMeta");
        }
        assert!(<WooV3PageMeta as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["rows", "meta"];
        let mut valid = serde_json::Map::new();
        for v in candidates.iter().filter(|v| {
            <WooV3Rows<safe_cex_num::numeric_ingress::JsNumberInput>>::deserialize((*v).clone())
                .is_ok()
        }) {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3Rows"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3Rows<safe_cex_num::numeric_ingress::JsNumberInput>>::deserialize(array.clone())
            .is_ok()
        {
            widened.push("WooV3Rows");
        }
        assert!(
            <WooV3Rows<safe_cex_num::numeric_ingress::JsNumberInput> as Deserialize>::deserialize(
                valid.clone()
            )
            .is_ok()
        );
    }
    {
        let fields = [
            "symbol",
            "status",
            "baseAsset",
            "quoteAsset",
            "baseAssetMultiplier",
            "baseMin",
            "baseMax",
            "baseTick",
            "quoteTick",
            "minNotional",
            "baseIMR",
            "fundingIntervalHours",
            "isAllowedRpi",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3Instrument>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3Instrument"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3Instrument>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3Instrument");
        }
        assert!(<WooV3Instrument as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "token",
            "holding",
            "frozen",
            "availableBalance",
            "pendingShortQty",
            "pendingLongQty",
            "averageOpenPrice",
            "markPrice",
            "timestamp",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3Balance>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3Balance"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3Balance>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3Balance");
        }
        assert!(<WooV3Balance as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "symbol",
            "holding",
            "positionSide",
            "pendingLongQty",
            "pendingShortQty",
            "settlePrice",
            "averageOpenPrice",
            "markPrice",
            "estLiqPrice",
            "timestamp",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3Position>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3Position"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3Position>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3Position");
        }
        assert!(<WooV3Position as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "applicationId",
            "account",
            "alias",
            "accountMode",
            "positionMode",
            "leverage",
            "marginRatio",
            "totalCollateral",
            "totalAccountValue",
            "accountType",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3AccountInfo>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3AccountInfo"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3AccountInfo>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3AccountInfo");
        }
        assert!(<WooV3AccountInfo as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "symbol",
            "side",
            "type",
            "clientOrderId",
            "orderTag",
            "positionSide",
            "price",
            "quantity",
            "amount",
            "reduceOnly",
            "visibleQuantity",
            "marginMode",
            "bidAskLevel",
            "postOnlyAdjusted",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3PlaceOrderRequest>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3PlaceOrderRequest"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3PlaceOrderRequest>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3PlaceOrderRequest");
        }
        assert!(<WooV3PlaceOrderRequest as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = [
            "orderId",
            "type",
            "price",
            "quantity",
            "amount",
            "clientOrderId",
            "bidAskLevel",
        ];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3PlaceOrderResult>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3PlaceOrderResult"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3PlaceOrderResult>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3PlaceOrderResult");
        }
        assert!(<WooV3PlaceOrderResult as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["type"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3ListenKeyRequest>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3ListenKeyRequest"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3ListenKeyRequest>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3ListenKeyRequest");
        }
        assert!(<WooV3ListenKeyRequest as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    {
        let fields = ["authKey", "expiredTime"];
        let mut valid = serde_json::Map::new();
        for v in candidates
            .iter()
            .filter(|v| <WooV3ListenKeyResult>::deserialize((*v).clone()).is_ok())
        {
            for (k, x) in v.as_object().unwrap() {
                valid.insert(k.clone(), x.clone());
            }
        }
        let valid = Value::Object(valid);
        assert!(
            fields.iter().all(|k| valid.get(k).is_some()),
            "full object witness: WooV3ListenKeyResult"
        );
        let array = Value::Array(fields.iter().map(|k| valid[*k].clone()).collect());
        if <WooV3ListenKeyResult>::deserialize(array.clone()).is_ok() {
            widened.push("WooV3ListenKeyResult");
        }
        assert!(<WooV3ListenKeyResult as Deserialize>::deserialize(valid.clone()).is_ok());
    }
    assert!(
        widened.is_empty(),
        "positional arrays accepted: {widened:?}"
    );
}
