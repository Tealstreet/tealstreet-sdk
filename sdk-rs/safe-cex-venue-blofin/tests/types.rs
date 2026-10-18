#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_blofin::types::*;
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn batch_position_side_accepts_only_declared_strings() {
    use serde::Deserialize as _;

    for (literal, expected) in [
        ("net", BatchPositionSide::Net),
        ("long", BatchPositionSide::Long),
        ("short", BatchPositionSide::Short),
    ] {
        let value = json!(literal);
        assert_eq!(
            js_from_value::<BatchPositionSide>(value.clone()).unwrap(),
            expected
        );
        assert_eq!(
            js_from_value_ref::<BatchPositionSide>(&value).unwrap(),
            expected
        );
        assert_eq!(BatchPositionSide::deserialize(value).unwrap(), expected);
        assert_eq!(serde_json::to_value(&expected).unwrap(), json!(literal));

        let tagged = json!({(literal): null});
        let raw = tagged.to_string();
        assert!(
            serde_json::from_str::<BatchPositionSide>(&raw).is_err(),
            "batch_position_side_string_only serde text: {tagged}"
        );
        assert!(
            serde_json::from_value::<BatchPositionSide>(tagged.clone()).is_err(),
            "batch_position_side_string_only serde owned: {tagged}"
        );
        assert!(
            BatchPositionSide::deserialize(&tagged).is_err(),
            "batch_position_side_string_only serde borrowed: {tagged}"
        );
        assert!(
            js_from_str::<BatchPositionSide>(&raw).is_err(),
            "batch_position_side_string_only raw: {tagged}"
        );
        assert!(
            js_from_value::<BatchPositionSide>(tagged.clone()).is_err(),
            "batch_position_side_string_only owned: {tagged}"
        );
        assert!(
            js_from_value_ref::<BatchPositionSide>(&tagged).is_err(),
            "batch_position_side_string_only borrowed: {tagged}"
        );
        assert!(
            BatchPositionSide::deserialize(tagged.clone()).is_err(),
            "batch_position_side_string_only qualified: {tagged}"
        );
    }
}

#[test]
fn live_typescript_declarations() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/types_oracle.cjs"
        ))
        .output()
        .expect("node required");
    assert!(
        output.status.success(),
        "live_typescript_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("types", &oracle);
    let header = &oracle["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("types capture header: {header}");
    let constants = &oracle["constants"];
    assert_eq!(
        oracle["schemas"],
        serde_json::from_str::<Value>(include_str!("types-schema.json")).unwrap(),
        "source_schema_census"
    );
    assert_eq!(
        RECV_WINDOW.to_bits(),
        constants["RECV_WINDOW"].as_f64().unwrap().to_bits(),
        "constant_RECV_WINDOW"
    );
    assert_eq!(
        json!(BROKER_ID),
        constants["BROKER_ID"],
        "constant_BROKER_ID"
    );
    assert_eq!(
        json!(USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY),
        constants["USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY"],
        "constant_USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY"
    );
    assert_eq!(*BASE_URL, constants["BASE_URL"], "constant_BASE_URL");
    assert_eq!(
        *BASE_WS_URL, constants["BASE_WS_URL"],
        "constant_BASE_WS_URL"
    );
    assert_eq!(*ENDPOINTS, constants["ENDPOINTS"], "constant_ENDPOINTS");
    assert_eq!(
        *GRAFANA_ENDPOINTS_EVERY, constants["GRAFANA_ENDPOINTS_EVERY"],
        "constant_GRAFANA_ENDPOINTS_EVERY"
    );
    assert_eq!(
        *GRAFANA_ENDPOINTS_PERIODIC, constants["GRAFANA_ENDPOINTS_PERIODIC"],
        "constant_GRAFANA_ENDPOINTS_PERIODIC"
    );
    assert_eq!(*INTERVAL, constants["INTERVAL"], "constant_INTERVAL");
    assert_eq!(
        *ORDER_STATUS, constants["ORDER_STATUS"],
        "constant_ORDER_STATUS"
    );
    assert_eq!(*ORDER_TYPE, constants["ORDER_TYPE"], "constant_ORDER_TYPE");
    assert_eq!(
        *REVERSE_ORDER_TYPE, constants["REVERSE_ORDER_TYPE"],
        "constant_REVERSE_ORDER_TYPE"
    );
    assert_eq!(*ORDER_SIDE, constants["ORDER_SIDE"], "constant_ORDER_SIDE");
    assert_eq!(
        *POSITION_SIDE, constants["POSITION_SIDE"],
        "constant_POSITION_SIDE"
    );
    assert_eq!(
        serde_json::to_string(&*BASE_URL).unwrap(),
        constants["BASE_URL"].to_string(),
        "constant_order_BASE_URL"
    );
    assert_eq!(
        serde_json::to_string(&*BASE_WS_URL).unwrap(),
        constants["BASE_WS_URL"].to_string(),
        "constant_order_BASE_WS_URL"
    );
    assert_eq!(
        serde_json::to_string(&*ENDPOINTS).unwrap(),
        constants["ENDPOINTS"].to_string(),
        "constant_order_ENDPOINTS"
    );
    assert_eq!(
        serde_json::to_string(&*GRAFANA_ENDPOINTS_EVERY).unwrap(),
        constants["GRAFANA_ENDPOINTS_EVERY"].to_string(),
        "constant_order_GRAFANA_ENDPOINTS_EVERY"
    );
    assert_eq!(
        serde_json::to_string(&*GRAFANA_ENDPOINTS_PERIODIC).unwrap(),
        constants["GRAFANA_ENDPOINTS_PERIODIC"].to_string(),
        "constant_order_GRAFANA_ENDPOINTS_PERIODIC"
    );
    assert_eq!(
        serde_json::to_string(&*INTERVAL).unwrap(),
        constants["INTERVAL"].to_string(),
        "constant_order_INTERVAL"
    );
    assert_eq!(
        serde_json::to_string(&*ORDER_STATUS).unwrap(),
        constants["ORDER_STATUS"].to_string(),
        "constant_order_ORDER_STATUS"
    );
    assert_eq!(
        serde_json::to_string(&*ORDER_TYPE).unwrap(),
        constants["ORDER_TYPE"].to_string(),
        "constant_order_ORDER_TYPE"
    );
    assert_eq!(
        serde_json::to_string(&*REVERSE_ORDER_TYPE).unwrap(),
        constants["REVERSE_ORDER_TYPE"].to_string(),
        "constant_order_REVERSE_ORDER_TYPE"
    );
    assert_eq!(
        serde_json::to_string(&*ORDER_SIDE).unwrap(),
        constants["ORDER_SIDE"].to_string(),
        "constant_order_ORDER_SIDE"
    );
    assert_eq!(
        serde_json::to_string(&*POSITION_SIDE).unwrap(),
        constants["POSITION_SIDE"].to_string(),
        "constant_order_POSITION_SIDE"
    );
    for i in 0..pin.count("/cases") {
        let case = &oracle["cases"][i];
        let value = &case["value"];
        let raw = serde_json::to_string(value).unwrap();
        match case["name"].as_str().unwrap() {
            "FETCH_POSITION_RESPONSE" => {
                let decoded: FetchPositionResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<FetchPositionResponse>(value.clone()).unwrap(),
                    "owned_ingress_FetchPositionResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<FetchPositionResponse>(value).unwrap(),
                    "borrowed_ingress_FetchPositionResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_FetchPositionResponse_code"
                );
                assert_eq!(
                    Some(decoded.msg.as_str()),
                    value["msg"].as_str(),
                    "decoded_FetchPositionResponse_msg"
                );
                {
                    let data = &decoded.data;
                    pin.native(i, "/value/data", data.len());
                    assert_eq!(
                        data.len(),
                        value["data"].as_array().unwrap().len(),
                        "data_length"
                    );
                    for row in 0..pin.case_count(i, "/value/data") {
                        let native = data
                            .get(row)
                            .expect("BLOFIN-CARDINALITY native row coverage");
                        let expected = &value["data"][row];
                        assert_eq!(
                            native.position_id.as_str(),
                            expected["positionId"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_position_id"
                        );
                        assert_eq!(
                            native.inst_id.as_str(),
                            expected["instId"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_inst_id"
                        );
                        assert_eq!(
                            native.inst_type.as_str(),
                            expected["instType"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_inst_type"
                        );
                        assert_eq!(
                            native.margin_mode.as_str(),
                            expected["marginMode"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_margin_mode"
                        );
                        assert_eq!(
                            native.position_side.as_str(),
                            expected["positionSide"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_position_side"
                        );
                        assert_eq!(
                            native.adl.as_str(),
                            expected["adl"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_adl"
                        );
                        assert_eq!(
                            native.positions.as_str(),
                            expected["positions"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_positions"
                        );
                        assert_eq!(
                            native.available_positions.as_str(),
                            expected["availablePositions"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_available_positions"
                        );
                        assert_eq!(
                            native.average_price.as_str(),
                            expected["averagePrice"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_average_price"
                        );
                        assert_eq!(
                            native.margin.as_str(),
                            expected["margin"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_margin"
                        );
                        assert_eq!(
                            native.mark_price.as_str(),
                            expected["markPrice"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_mark_price"
                        );
                        assert_eq!(
                            native.margin_ratio.as_str(),
                            expected["marginRatio"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_margin_ratio"
                        );
                        assert_eq!(
                            native.liquidation_price.as_str(),
                            expected["liquidationPrice"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_liquidation_price"
                        );
                        assert_eq!(
                            native.unrealized_pnl.as_str(),
                            expected["unrealizedPnl"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_unrealized_pnl"
                        );
                        assert_eq!(
                            native.unrealized_pnl_ratio.as_str(),
                            expected["unrealizedPnlRatio"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_unrealized_pnl_ratio"
                        );
                        assert_eq!(
                            native.maintenance_margin.as_str(),
                            expected["maintenanceMargin"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_maintenance_margin"
                        );
                        assert_eq!(
                            native.create_time.as_str(),
                            expected["createTime"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_create_time"
                        );
                        assert_eq!(
                            native.update_time.as_str(),
                            expected["updateTime"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_update_time"
                        );
                        assert_eq!(
                            native.leverage.as_str(),
                            expected["leverage"].as_str().unwrap(),
                            "decoded_FetchPositionResponseData_leverage"
                        );
                        pin.hit("data-fields");
                    }
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_FetchPositionResponse"
                );
            }
            "POSITION_MODE_RESPONSE" => {
                let decoded: PositionModeResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<PositionModeResponse>(value.clone()).unwrap(),
                    "owned_ingress_PositionModeResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<PositionModeResponse>(value).unwrap(),
                    "borrowed_ingress_PositionModeResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_PositionModeResponse_code"
                );
                assert_eq!(
                    Some(decoded.msg.as_str()),
                    value["msg"].as_str(),
                    "decoded_PositionModeResponse_msg"
                );
                {
                    let data = &decoded.data;
                    let native = data;
                    let expected = &value["data"];
                    assert_eq!(
                        native.position_mode.as_str(),
                        expected["positionMode"].as_str().unwrap(),
                        "decoded_PositionModeResponseData_position_mode"
                    );
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_PositionModeResponse"
                );
            }
            "FETCH_MARGIN_MODE_RESPONSE" => {
                let decoded: FetchMarginModeResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<FetchMarginModeResponse>(value.clone()).unwrap(),
                    "owned_ingress_FetchMarginModeResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<FetchMarginModeResponse>(value).unwrap(),
                    "borrowed_ingress_FetchMarginModeResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_FetchMarginModeResponse_code"
                );
                assert_eq!(
                    Some(decoded.msg.as_str()),
                    value["msg"].as_str(),
                    "decoded_FetchMarginModeResponse_msg"
                );
                {
                    let data = &decoded.data;
                    let native = data;
                    let expected = &value["data"];
                    assert_eq!(
                        native.margin_mode.as_str(),
                        expected["marginMode"].as_str().unwrap(),
                        "decoded_FetchMarginModeResponseData_margin_mode"
                    );
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_FetchMarginModeResponse"
                );
            }
            "FETCH_BATCH_LEV_RESPONSE" => {
                let decoded: FetchBatchLevResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<FetchBatchLevResponse>(value.clone()).unwrap(),
                    "owned_ingress_FetchBatchLevResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<FetchBatchLevResponse>(value).unwrap(),
                    "borrowed_ingress_FetchBatchLevResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_FetchBatchLevResponse_code"
                );
                assert_eq!(
                    Some(decoded.msg.as_str()),
                    value["msg"].as_str(),
                    "decoded_FetchBatchLevResponse_msg"
                );
                {
                    let data = &decoded.data;
                    pin.native(i, "/value/data", data.len());
                    assert_eq!(
                        data.len(),
                        value["data"].as_array().unwrap().len(),
                        "data_length"
                    );
                    for row in 0..pin.case_count(i, "/value/data") {
                        let native = data
                            .get(row)
                            .expect("BLOFIN-CARDINALITY native row coverage");
                        let expected = &value["data"][row];
                        assert_eq!(
                            native.leverage.as_str(),
                            expected["leverage"].as_str().unwrap(),
                            "decoded_FetchBatchLevResponseData_leverage"
                        );
                        assert_eq!(
                            native.margin_mode.as_str(),
                            expected["marginMode"].as_str().unwrap(),
                            "decoded_FetchBatchLevResponseData_margin_mode"
                        );
                        assert_eq!(
                            native.inst_id.as_str(),
                            expected["instId"].as_str().unwrap(),
                            "decoded_FetchBatchLevResponseData_inst_id"
                        );
                        assert_eq!(
                            match native.position_side {
                                BatchPositionSide::Net => "net",
                                BatchPositionSide::Long => "long",
                                BatchPositionSide::Short => "short",
                            },
                            expected["positionSide"].as_str().unwrap(),
                            "decoded_FetchBatchLevResponseData_position_side"
                        );
                        pin.hit("data-fields");
                    }
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_FetchBatchLevResponse"
                );
            }
            "ORDERS_RESPONSE" => {
                let decoded: OrdersResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<OrdersResponse>(value.clone()).unwrap(),
                    "owned_ingress_OrdersResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<OrdersResponse>(value).unwrap(),
                    "borrowed_ingress_OrdersResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_OrdersResponse_code"
                );
                assert_eq!(
                    decoded.msg.as_deref(),
                    value["msg"].as_str(),
                    "decoded_OrdersResponse_msg"
                );
                assert_eq!(
                    decoded.data.is_some(),
                    value.get("data").is_some(),
                    "optional_data_presence"
                );
                if let Some(data) = &decoded.data {
                    pin.native(i, "/value/data", data.len());
                    assert_eq!(
                        data.len(),
                        value["data"].as_array().unwrap().len(),
                        "data_length"
                    );
                    for row in 0..pin.case_count(i, "/value/data") {
                        let native = data
                            .get(row)
                            .expect("BLOFIN-CARDINALITY native row coverage");
                        let expected = &value["data"][row];
                        assert_eq!(
                            native.order_id.as_str(),
                            expected["orderId"].as_str().unwrap(),
                            "decoded_OrdersResponseData_order_id"
                        );
                        assert_eq!(
                            native.inst_id.as_str(),
                            expected["instId"].as_str().unwrap(),
                            "decoded_OrdersResponseData_inst_id"
                        );
                        assert_eq!(
                            native.td_mode.as_str(),
                            expected["tdMode"].as_str().unwrap(),
                            "decoded_OrdersResponseData_td_mode"
                        );
                        assert_eq!(
                            native.c_time.as_str(),
                            expected["cTime"].as_str().unwrap(),
                            "decoded_OrdersResponseData_c_time"
                        );
                        assert_eq!(
                            native.u_time.as_str(),
                            expected["uTime"].as_str().unwrap(),
                            "decoded_OrdersResponseData_u_time"
                        );
                        assert_eq!(
                            native.price.as_str(),
                            expected["price"].as_str().unwrap(),
                            "decoded_OrdersResponseData_price"
                        );
                        assert_eq!(
                            native.size.as_str(),
                            expected["size"].as_str().unwrap(),
                            "decoded_OrdersResponseData_size"
                        );
                        assert_eq!(
                            native.side.as_str(),
                            expected["side"].as_str().unwrap(),
                            "decoded_OrdersResponseData_side"
                        );
                        assert_eq!(
                            native.order_type.as_str(),
                            expected["orderType"].as_str().unwrap(),
                            "decoded_OrdersResponseData_order_type"
                        );
                        assert_eq!(
                            native.state.as_str(),
                            expected["state"].as_str().unwrap(),
                            "decoded_OrdersResponseData_state"
                        );
                        assert_eq!(
                            native.filled_size.as_str(),
                            expected["filledSize"].as_str().unwrap(),
                            "decoded_OrdersResponseData_filled_size"
                        );
                        assert_eq!(
                            native.avg_fill_price.as_str(),
                            expected["avgFillPrice"].as_str().unwrap(),
                            "decoded_OrdersResponseData_avg_fill_price"
                        );
                        assert_eq!(
                            native.fee.as_str(),
                            expected["fee"].as_str().unwrap(),
                            "decoded_OrdersResponseData_fee"
                        );
                        assert_eq!(
                            native.fee_currency.as_str(),
                            expected["feeCurrency"].as_str().unwrap(),
                            "decoded_OrdersResponseData_fee_currency"
                        );
                        pin.hit("data-fields");
                    }
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_OrdersResponse"
                );
            }
            "POSITIONS_RESPONSE" => {
                let decoded: PositionsResponse = js_from_str(&raw).unwrap();
                assert_eq!(
                    decoded,
                    js_from_value::<PositionsResponse>(value.clone()).unwrap(),
                    "owned_ingress_PositionsResponse"
                );
                assert_eq!(
                    decoded,
                    js_from_value_ref::<PositionsResponse>(value).unwrap(),
                    "borrowed_ingress_PositionsResponse"
                );
                assert_eq!(
                    Some(decoded.code.as_str()),
                    value["code"].as_str(),
                    "decoded_PositionsResponse_code"
                );
                assert_eq!(
                    Some(decoded.msg.as_str()),
                    value["msg"].as_str(),
                    "decoded_PositionsResponse_msg"
                );
                assert_eq!(
                    decoded.data.is_some(),
                    value.get("data").is_some(),
                    "optional_data_presence"
                );
                if let Some(data) = &decoded.data {
                    pin.native(i, "/value/data", data.len());
                    assert_eq!(
                        data.len(),
                        value["data"].as_array().unwrap().len(),
                        "data_length"
                    );
                    for row in 0..pin.case_count(i, "/value/data") {
                        let native = data
                            .get(row)
                            .expect("BLOFIN-CARDINALITY native row coverage");
                        let expected = &value["data"][row];
                        assert_eq!(
                            native.position_id.as_str(),
                            expected["positionId"].as_str().unwrap(),
                            "decoded_PositionsResponseData_position_id"
                        );
                        assert_eq!(
                            native.inst_id.as_str(),
                            expected["instId"].as_str().unwrap(),
                            "decoded_PositionsResponseData_inst_id"
                        );
                        assert_eq!(
                            native.inst_type.as_str(),
                            expected["instType"].as_str().unwrap(),
                            "decoded_PositionsResponseData_inst_type"
                        );
                        assert_eq!(
                            native.margin_mode.as_str(),
                            expected["marginMode"].as_str().unwrap(),
                            "decoded_PositionsResponseData_margin_mode"
                        );
                        assert_eq!(
                            native.position_side.as_str(),
                            expected["positionSide"].as_str().unwrap(),
                            "decoded_PositionsResponseData_position_side"
                        );
                        assert_eq!(
                            native.adl.as_str(),
                            expected["adl"].as_str().unwrap(),
                            "decoded_PositionsResponseData_adl"
                        );
                        assert_eq!(
                            native.positions.as_str(),
                            expected["positions"].as_str().unwrap(),
                            "decoded_PositionsResponseData_positions"
                        );
                        assert_eq!(
                            native.available_positions.as_str(),
                            expected["availablePositions"].as_str().unwrap(),
                            "decoded_PositionsResponseData_available_positions"
                        );
                        assert_eq!(
                            native.average_price.as_str(),
                            expected["averagePrice"].as_str().unwrap(),
                            "decoded_PositionsResponseData_average_price"
                        );
                        assert_eq!(
                            native.mark_price.as_str(),
                            expected["markPrice"].as_str().unwrap(),
                            "decoded_PositionsResponseData_mark_price"
                        );
                        assert_eq!(
                            native.margin_ratio.as_str(),
                            expected["marginRatio"].as_str().unwrap(),
                            "decoded_PositionsResponseData_margin_ratio"
                        );
                        assert_eq!(
                            native.liquidation_price.as_str(),
                            expected["liquidationPrice"].as_str().unwrap(),
                            "decoded_PositionsResponseData_liquidation_price"
                        );
                        assert_eq!(
                            native.unrealized_pnl.as_str(),
                            expected["unrealizedPnl"].as_str().unwrap(),
                            "decoded_PositionsResponseData_unrealized_pnl"
                        );
                        assert_eq!(
                            native.unrealized_pnl_ratio.as_str(),
                            expected["unrealizedPnlRatio"].as_str().unwrap(),
                            "decoded_PositionsResponseData_unrealized_pnl_ratio"
                        );
                        assert_eq!(
                            native.initial_margin.as_str(),
                            expected["initialMargin"].as_str().unwrap(),
                            "decoded_PositionsResponseData_initial_margin"
                        );
                        assert_eq!(
                            native.maintenance_margin.as_str(),
                            expected["maintenanceMargin"].as_str().unwrap(),
                            "decoded_PositionsResponseData_maintenance_margin"
                        );
                        assert_eq!(
                            native.create_time.as_str(),
                            expected["createTime"].as_str().unwrap(),
                            "decoded_PositionsResponseData_create_time"
                        );
                        assert_eq!(
                            native.update_time.as_str(),
                            expected["updateTime"].as_str().unwrap(),
                            "decoded_PositionsResponseData_update_time"
                        );
                        assert_eq!(
                            native.leverage.as_str(),
                            expected["leverage"].as_str().unwrap(),
                            "decoded_PositionsResponseData_leverage"
                        );
                        pin.hit("data-fields");
                    }
                }
                assert_eq!(
                    serde_json::to_value(&decoded).unwrap(),
                    *value,
                    "wire_PositionsResponse"
                );
            }
            name => panic!("uncovered_source_type: {name}"),
        }
        pin.compared(i, 0);
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(1);
    pin.finish("data-fields", 26);
    for index in 0..pin.count("/probes") {
        let probe = &oracle["probes"][index];
        let raw = probe["raw"].as_str().unwrap();
        let accepted = match probe["name"].as_str().unwrap() {
            "FETCH_POSITION_RESPONSE" => js_from_str::<FetchPositionResponse>(raw).is_ok(),
            "POSITION_MODE_RESPONSE" => js_from_str::<PositionModeResponse>(raw).is_ok(),
            "FETCH_MARGIN_MODE_RESPONSE" => js_from_str::<FetchMarginModeResponse>(raw).is_ok(),
            "FETCH_BATCH_LEV_RESPONSE" => js_from_str::<FetchBatchLevResponse>(raw).is_ok(),
            "ORDERS_RESPONSE" => js_from_str::<OrdersResponse>(raw).is_ok(),
            "POSITIONS_RESPONSE" => js_from_str::<PositionsResponse>(raw).is_ok(),
            name => panic!("uncovered_probe_type: {name}"),
        };
        let string_surrogate = if probe["reason"] == "surrogate" {
            let mut field_shape = &oracle["schemas"][probe["name"].as_str().unwrap()];
            for key in probe["path"]
                .as_str()
                .unwrap()
                .split('.')
                .filter(|key| !key.is_empty())
            {
                field_shape = if key.parse::<usize>().is_ok() {
                    &field_shape["array"]
                } else {
                    &field_shape["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|field| field["name"] == key)
                        .unwrap()["shape"]
                };
            }
            field_shape["string"] == true
        } else {
            false
        };
        if string_surrogate {
            macro_rules! observe {
                ($t:ty) => {
                    typed_utf16::assert_projection(raw, &js_from_str::<$t>(raw).unwrap())
                };
            }
            match probe["name"].as_str().unwrap() {
                "FETCH_POSITION_RESPONSE" => observe!(FetchPositionResponse),
                "POSITION_MODE_RESPONSE" => observe!(PositionModeResponse),
                "FETCH_MARGIN_MODE_RESPONSE" => observe!(FetchMarginModeResponse),
                "FETCH_BATCH_LEV_RESPONSE" => observe!(FetchBatchLevResponse),
                "ORDERS_RESPONSE" => observe!(OrdersResponse),
                "POSITIONS_RESPONSE" => observe!(PositionsResponse),
                name => panic!("uncovered_probe_type: {name}"),
            }
            assert!(accepted, "typed surrogate record retains the frame");
        } else {
            assert_eq!(
                accepted,
                !probe["reject"].as_bool().unwrap(),
                "record_boundary: {probe}"
            );
        }
        if probe["reason"] == "duplicate_key" {
            let native_code = match probe["name"].as_str().unwrap() {
                "FETCH_POSITION_RESPONSE" => {
                    js_from_str::<FetchPositionResponse>(raw).unwrap().code
                }
                "POSITION_MODE_RESPONSE" => js_from_str::<PositionModeResponse>(raw).unwrap().code,
                "FETCH_MARGIN_MODE_RESPONSE" => {
                    js_from_str::<FetchMarginModeResponse>(raw).unwrap().code
                }
                "FETCH_BATCH_LEV_RESPONSE" => {
                    js_from_str::<FetchBatchLevResponse>(raw).unwrap().code
                }
                "ORDERS_RESPONSE" => js_from_str::<OrdersResponse>(raw).unwrap().code,
                "POSITIONS_RESPONSE" => js_from_str::<PositionsResponse>(raw).unwrap().code,
                name => panic!("uncovered_duplicate_type: {name}"),
            };
            assert_eq!(native_code, "last", "decoded_duplicate_last_wins");
        }
        if probe["reason"] == "extra_field" {
            let projected = match probe["name"].as_str().unwrap() {
                "FETCH_POSITION_RESPONSE" => {
                    serde_json::to_value(js_from_str::<FetchPositionResponse>(raw).unwrap())
                        .unwrap()
                }
                "POSITION_MODE_RESPONSE" => {
                    serde_json::to_value(js_from_str::<PositionModeResponse>(raw).unwrap()).unwrap()
                }
                "FETCH_MARGIN_MODE_RESPONSE" => {
                    serde_json::to_value(js_from_str::<FetchMarginModeResponse>(raw).unwrap())
                        .unwrap()
                }
                "FETCH_BATCH_LEV_RESPONSE" => {
                    serde_json::to_value(js_from_str::<FetchBatchLevResponse>(raw).unwrap())
                        .unwrap()
                }
                "ORDERS_RESPONSE" => {
                    serde_json::to_value(js_from_str::<OrdersResponse>(raw).unwrap()).unwrap()
                }
                "POSITIONS_RESPONSE" => {
                    serde_json::to_value(js_from_str::<PositionsResponse>(raw).unwrap()).unwrap()
                }
                name => panic!("uncovered_extra_type: {name}"),
            };
            assert_eq!(
                serde_json::from_str::<Value>(raw).unwrap()["extra"],
                "retained-in-source",
                "source_extra_retained"
            );
            assert!(projected.get("extra").is_none(), "typed_extra_projection");
        }
        if probe["reason"] == "surrogate" {
            assert_eq!(probe["sourceType"], "string", "source_surrogate_type");
            assert_eq!(probe["sourceCodeUnit"], 55296, "source_surrogate_unit");
        }
        pin.hit("probes-probe-1");
    }
    pin.finish("probes-probe-1", pin.count("/probes"));
    assert_eq!(
        oracle["schemas"].as_object().unwrap().len(),
        6,
        "source_type_census"
    );
    assert_eq!(
        constants.as_object().unwrap().len(),
        14,
        "source_constant_census"
    );
    assert_eq!(
        oracle["boundaries"][0]["codeUnit"], 55296,
        "source_lone_surrogate"
    );
    assert_eq!(
        oracle["boundaries"][2]["bits"], "8000000000000000",
        "source_negative_zero_bits"
    );
    assert_eq!(
        oracle["boundaries"][3]["bits"], "7ff0000000000000",
        "source_overflow_bits"
    );
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        assert!(
            js_from_str::<FetchPositionResponse>(b["raw"].as_str().unwrap()).is_err(),
            "typed_boundary_FetchPositionResponse"
        );
        pin.hit("boundaries-b-1");
    }
    pin.finish("boundaries-b-1", pin.count("/boundaries"));
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        assert!(
            js_from_str::<PositionModeResponse>(b["raw"].as_str().unwrap()).is_err(),
            "typed_boundary_PositionModeResponse"
        );
        pin.hit("boundaries-b-2");
    }
    pin.finish("boundaries-b-2", pin.count("/boundaries"));
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        assert!(
            js_from_str::<FetchMarginModeResponse>(b["raw"].as_str().unwrap()).is_err(),
            "typed_boundary_FetchMarginModeResponse"
        );
        pin.hit("boundaries-b-3");
    }
    pin.finish("boundaries-b-3", pin.count("/boundaries"));
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        assert!(
            js_from_str::<FetchBatchLevResponse>(b["raw"].as_str().unwrap()).is_err(),
            "typed_boundary_FetchBatchLevResponse"
        );
        pin.hit("boundaries-b-4");
    }
    pin.finish("boundaries-b-4", pin.count("/boundaries"));
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        if index == 0 {
            let raw = b["raw"].as_str().unwrap();
            let native: OrdersResponse = js_from_str(raw).unwrap();
            assert_eq!(native.code, "\u{fffd}", "typed code replacement");
            assert!(
                native.msg.is_none() && native.data.is_none(),
                "optional fields remain absent"
            );
            typed_utf16::assert_projection(raw, &native);
        } else {
            assert!(
                js_from_str::<OrdersResponse>(b["raw"].as_str().unwrap()).is_err(),
                "typed_boundary_OrdersResponse"
            );
        }
        pin.hit("boundaries-b-5");
    }
    pin.finish("boundaries-b-5", pin.count("/boundaries"));
    for index in 0..pin.count("/boundaries") {
        let b = &oracle["boundaries"][index];
        assert!(
            js_from_str::<PositionsResponse>(b["raw"].as_str().unwrap()).is_err(),
            "typed_boundary_PositionsResponse"
        );
        pin.hit("boundaries-b-6");
    }
    pin.finish("boundaries-b-6", pin.count("/boundaries"));
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
