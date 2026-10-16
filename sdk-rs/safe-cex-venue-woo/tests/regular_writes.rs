use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_woo::{
    v3_contracts::{WooV3Endpoint, WooV3HttpMethod},
    v3_private::{WooV3AccountPositionMode, WooV3PositionSide},
    v3_private_orders::{
        WooV3OrderAmount, WooV3OrderSide, WooV3OrderStatus, WooV3RegularOrder,
        WooV3RegularOrderType,
    },
    v3_regular_writes::*,
    v3_request::WooV3RequestInput,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/regular-writes-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "REGULAR-WRITES source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
fn arg(row: &Value, i: usize) -> Option<JsNumberInput> {
    row["args"][i].as_str().map(|s| js_from_str(s).unwrap())
}
fn record(v: &JsNumberInput) -> JsObjectMap<JsNumberInput> {
    v.clone().decode().unwrap()
}
fn number(a: f64, e: &Value, label: &str) {
    assert_eq!(
        a.to_bits(),
        e["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "REGULAR-WRITES native {label}"
    );
}
fn input(a: &JsNumberInput, e: &Value) {
    if e.get("$number").is_some() {
        number(a.clone().decode::<f64>().unwrap(), e, "captured number");
    } else {
        match e {
            Value::Null => assert!(
                a.clone().decode::<Option<bool>>().unwrap().is_none(),
                "REGULAR-WRITES native null"
            ),
            Value::Bool(b) => assert_eq!(
                a.clone().decode::<bool>().unwrap(),
                *b,
                "REGULAR-WRITES native captured boolean"
            ),
            Value::String(s) => assert_eq!(
                a.clone().decode::<String>().unwrap(),
                *s,
                "REGULAR-WRITES native captured string"
            ),
            Value::Array(xs) => {
                let vs = a.clone().decode::<Vec<JsNumberInput>>().unwrap();
                assert_eq!(vs.len(), xs.len(), "REGULAR-WRITES native array length");
                for (v, x) in vs.iter().zip(xs) {
                    input(v, x);
                }
            }
            Value::Object(_) => map(&record(a), e),
            _ => panic!("number must use oracle marker"),
        }
    }
}
fn map(a: &JsObjectMap<JsNumberInput>, e: &Value) {
    let object = e.as_object().unwrap();
    assert_eq!(a.len(), object.len(), "REGULAR-WRITES native key count");
    for (k, v) in object {
        input(
            a.get(k)
                .unwrap_or_else(|| panic!("REGULAR-WRITES native missing key {k}")),
            v,
        );
    }
}
fn target(a: &WooV3RegularWriteTarget, e: &Value) {
    match a {
        WooV3RegularWriteTarget::OrderId(n) => {
            assert!(
                e.get("orderId").is_some(),
                "REGULAR-WRITES native order target variant"
            );
            number(*n, &e["orderId"], "orderId");
        }
        WooV3RegularWriteTarget::ClientOrderId(n) => {
            assert!(
                e.get("clientOrderId").is_some(),
                "REGULAR-WRITES native client target variant"
            );
            number(*n, &e["clientOrderId"], "clientOrderId");
        }
    }
}
fn kind(e: &Value) -> WooV3RegularPlaceType {
    match e.as_str().unwrap() {
        "LIMIT" => WooV3RegularPlaceType::Limit,
        "MARKET" => WooV3RegularPlaceType::Market,
        "IOC" => WooV3RegularPlaceType::Ioc,
        "FOK" => WooV3RegularPlaceType::Fok,
        "POST_ONLY" => WooV3RegularPlaceType::PostOnly,
        _ => panic!("source type"),
    }
}
fn status(e: &Value) -> WooV3RegularMutationStatus {
    match e.as_str().unwrap() {
        "NEW" => WooV3RegularMutationStatus::New,
        "CANCELLED" => WooV3RegularMutationStatus::Cancelled,
        "PARTIAL_FILLED" => WooV3RegularMutationStatus::PartialFilled,
        "FILLED" => WooV3RegularMutationStatus::Filled,
        "REJECTED" => WooV3RegularMutationStatus::Rejected,
        "INCOMPLETE" => WooV3RegularMutationStatus::Incomplete,
        "COMPLETED" => WooV3RegularMutationStatus::Completed,
        _ => panic!("source status"),
    }
}
fn cancel_status(e: &Value) -> WooV3RegularCancelStatus {
    match e.as_str().unwrap() {
        "CANCELLED" => WooV3RegularCancelStatus::Cancelled,
        "REJECTED" => WooV3RegularCancelStatus::Rejected,
        _ => panic!("source status"),
    }
}
fn request(a: &WooV3RequestInput, e: &Value) {
    assert_eq!(
        a.method,
        match e["method"].as_str().unwrap() {
            "POST" => WooV3HttpMethod::Post,
            "PUT" => WooV3HttpMethod::Put,
            "DELETE" => WooV3HttpMethod::Delete,
            _ => panic!("source method"),
        },
        "REGULAR-WRITES native request method"
    );
    assert_eq!(
        a.path,
        match e["path"].as_str().unwrap() {
            "/v3/trade/order" => WooV3Endpoint::TradeOrder,
            "/v3/trade/orders" => WooV3Endpoint::TradeOrders,
            _ => panic!("source endpoint"),
        },
        "REGULAR-WRITES native request endpoint"
    );
    for (a, e) in [(&a.body, e.get("body")), (&a.query, e.get("query"))] {
        assert_eq!(
            a.is_some(),
            e.is_some(),
            "REGULAR-WRITES native request presence"
        );
        if let (Some(a), Some(e)) = (a, e) {
            map(a, e);
        }
    }
}
fn checked<T>(r: Result<T, String>, e: &Value, f: impl FnOnce(T, &Value)) {
    match e.get("error") {
        Some(error) => {
            assert!(r.is_err(), "REGULAR-WRITES native expected refusal");
            assert_eq!(
                r.err().unwrap(),
                error["message"].as_str().unwrap(),
                "REGULAR-WRITES native first error"
            );
            assert_eq!(error["name"], "Error");
        }
        None => {
            assert!(
                r.is_ok(),
                "REGULAR-WRITES native expected success {r:?}",
                r = r.as_ref().map(|_| ())
            );
            f(r.ok().unwrap(), &e["ok"]);
        }
    }
}
fn write_checked<T>(r: Result<T, WooV3RegularWriteError>, e: &Value, f: impl FnOnce(T, &Value)) {
    if let Some(error) = e.get("error") {
        match r
            .err()
            .expect("REGULAR-WRITES native expected write refusal")
        {
            WooV3RegularWriteError::Validation(s) => {
                assert_eq!(
                    error["name"], "Error",
                    "REGULAR-WRITES native validation class"
                );
                assert_eq!(
                    s, error["message"],
                    "REGULAR-WRITES native first write error"
                );
            }
            WooV3RegularWriteError::Gateway(g) => {
                assert_eq!(g.name, error["name"], "REGULAR-WRITES native gateway name");
                assert_eq!(
                    g.message, error["message"],
                    "REGULAR-WRITES native gateway message"
                );
                assert_eq!(
                    g.contract_status, error["contractStatus"],
                    "REGULAR-WRITES native gateway contract status"
                );
                assert_eq!(
                    g.code.is_some(),
                    error.get("code").is_some(),
                    "REGULAR-WRITES native gateway code presence"
                );
                if let Some(n) = g.code {
                    number(n, &error["code"], "gateway code");
                }
                map(&g.gateway_response, &error["gatewayResponse"]);
            }
        }
    } else {
        assert!(r.is_ok(), "REGULAR-WRITES native expected write success");
        f(r.ok().unwrap(), &e["ok"]);
    }
}
fn place(a: &WooV3RegularPlaceResult, e: &Value) {
    number(a.order_id, &e["orderId"], "place orderId");
    assert_eq!(
        a.r#type,
        kind(&e["type"]),
        "REGULAR-WRITES native place type"
    );
    assert_eq!(a.price, e["price"], "REGULAR-WRITES native place price");
    assert_eq!(
        a.quantity, e["quantity"],
        "REGULAR-WRITES native place quantity"
    );
    assert_eq!(a.amount, e["amount"], "REGULAR-WRITES native place amount");
    number(
        a.client_order_id,
        &e["clientOrderId"],
        "place clientOrderId",
    );
    number(a.bid_ask_level, &e["bidAskLevel"], "place bidAskLevel");
    number(a.timestamp, &e["timestamp"], "place timestamp");
}
fn mutation(a: &WooV3RegularMutationResult, e: &Value) {
    assert_eq!(
        a.status,
        status(&e["status"]),
        "REGULAR-WRITES native mutation status"
    );
    target(&a.target, &e["target"]);
    number(a.timestamp, &e["timestamp"], "mutation timestamp");
    assert_eq!(
        a.identity_evidence, e["identityEvidence"],
        "REGULAR-WRITES native identity evidence"
    );
}
fn pending(a: &WooV3RegularCancelPendingResult, e: &Value) {
    assert_eq!(
        a.status,
        cancel_status(&e["status"]),
        "REGULAR-WRITES native pending status"
    );
    assert_eq!(
        a.scope.symbol.as_deref(),
        e["scope"]["symbol"].as_str(),
        "REGULAR-WRITES native pending scope"
    );
    number(a.timestamp, &e["timestamp"], "pending timestamp");
}
#[test]
fn native_validators() {
    let s = source();
    let rows = s["validators"].as_array().unwrap();
    assert_eq!(rows.len(), 100, "REGULAR-WRITES validator floor");
    for e in rows {
        let a = arg(e, 0);
        let b = arg(e, 1);
        match e["name"].as_str().unwrap() {
            "requireRecord" => checked(require_record(a.as_ref(), "probe"), e, |v, x| map(&v, x)),
            "requireBoolean" => checked(require_boolean(a.as_ref(), "probe"), e, |v, x| {
                assert_eq!(
                    v,
                    x.as_bool().unwrap(),
                    "REGULAR-WRITES native validator boolean"
                )
            }),
            "requireInteger" => checked(
                require_integer(a.as_ref(), "probe", b.unwrap().decode::<f64>().unwrap()),
                e,
                |v, x| number(v, x, "validator integer"),
            ),
            "requireString" => checked(require_string(a.as_ref(), "probe"), e, |v, x| {
                assert_eq!(
                    v,
                    x.as_str().unwrap(),
                    "REGULAR-WRITES native validator string"
                )
            }),
            "requireEnum" => checked(require_enum(a.as_ref(), &["A", "B"], "probe"), e, |v, x| {
                assert_eq!(
                    v,
                    x.as_str().unwrap(),
                    "REGULAR-WRITES native validator enum"
                )
            }),
            "requireDecimal" => checked(
                require_decimal(a.as_ref(), "probe", b.unwrap().decode::<bool>().unwrap()),
                e,
                |v, x| {
                    assert_eq!(
                        v,
                        x.as_str().unwrap(),
                        "REGULAR-WRITES native validator decimal"
                    )
                },
            ),
            "decimalParts" => {
                let (l, r) = decimal_parts(&a.unwrap().decode::<String>().unwrap());
                assert_eq!(l, e["ok"][0], "REGULAR-WRITES native decimal whole");
                assert_eq!(r, e["ok"][1], "REGULAR-WRITES native decimal fraction");
            }
            "compareDecimals" => number(
                f64::from(compare_decimals(
                    &a.unwrap().decode::<String>().unwrap(),
                    &b.unwrap().decode::<String>().unwrap(),
                )),
                &e["ok"],
                "decimal comparison",
            ),
            "requirePerpetualSymbol" => checked(
                require_perpetual_symbol(&a.unwrap().decode::<String>().unwrap()),
                e,
                |v, x| assert_eq!(v, x.as_str().unwrap(), "REGULAR-WRITES native symbol"),
            ),
            "validateTarget" => checked(validate_target(&record(&a.unwrap())), e, |v, x| {
                target(&v, x)
            }),
            "mapPlaceType" => checked(map_place_type(&record(&a.unwrap())), e, |v, x| {
                assert_eq!(v, kind(x), "REGULAR-WRITES native mapped place type")
            }),
            _ => panic!("source helper"),
        }
    }
}
#[test]
fn native_place_builders() {
    let s = source();
    let rows = s["places"].as_array().unwrap();
    assert_eq!(rows.len(), 31, "REGULAR-WRITES place floor");
    for e in rows {
        checked(
            build_woo_v3_regular_place_request(&record(&arg(e, 0).unwrap())),
            e,
            |v, x| request(&v, x),
        );
    }
}
#[test]
fn native_amend_builders() {
    let s = source();
    let rows = s["amends"].as_array().unwrap();
    assert_eq!(rows.len(), 22, "REGULAR-WRITES amend floor");
    for e in rows {
        checked(
            build_woo_v3_regular_amend_request(&record(&arg(e, 0).unwrap())),
            e,
            |v, x| match v {
                WooV3RegularAmendBuildResult::Ready {
                    request: r,
                    target: t,
                    quantity_semantics,
                } => {
                    assert_eq!(x["ready"], true, "REGULAR-WRITES native ready variant");
                    request(&r, &x["request"]);
                    target(&t, &x["target"]);
                    assert_eq!(
                        quantity_semantics, x["quantitySemantics"],
                        "REGULAR-WRITES native quantity semantics"
                    );
                }
                WooV3RegularAmendBuildResult::Blocked { blocker, target: t } => {
                    assert_eq!(x["ready"], false, "REGULAR-WRITES native blocked variant");
                    assert_eq!(blocker, x["blocker"], "REGULAR-WRITES native first blocker");
                    target(&t, &x["target"]);
                }
            },
        );
    }
}
#[test]
fn native_cancel_builders() {
    let s = source();
    let rows = s["cancels"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "REGULAR-WRITES cancel floor");
    for e in rows {
        let a = arg(e, 0).map(|v| v.decode::<String>().unwrap());
        let r = if e["name"] == "single" {
            build_woo_v3_regular_cancel_request(a.as_deref().unwrap(), &record(&arg(e, 1).unwrap()))
        } else {
            build_woo_v3_regular_cancel_pending_request(a.as_deref())
        };
        checked(r, e, |v, x| request(&v, x));
    }
}
#[test]
fn native_response_decoders() {
    let s = source();
    let rows = s["successes"].as_array().unwrap();
    assert_eq!(rows.len(), 10, "REGULAR-WRITES success floor");
    for e in rows {
        write_checked(
            decode_success_data(arg(e, 0).as_ref(), "probe"),
            e,
            |v, x| {
                map(&v.data, &x["data"]);
                number(v.timestamp, &x["timestamp"], "success timestamp");
            },
        );
    }
    let rows = s["placeResponses"].as_array().unwrap();
    assert_eq!(rows.len(), 12, "REGULAR-WRITES place response floor");
    for e in rows {
        let request = arg(e, 1).unwrap().decode::<WooV3RequestInput>().unwrap();
        let known = arg(e, 2).unwrap().decode::<Vec<f64>>().unwrap();
        write_checked(
            decode_woo_v3_regular_place_response(arg(e, 0).as_ref(), &request, &known),
            e,
            |v, x| place(&v, x),
        );
    }
    let rows = s["mutations"].as_array().unwrap();
    assert_eq!(rows.len(), 23, "REGULAR-WRITES mutation response floor");
    for e in rows {
        let a = arg(e, 0);
        if e["name"] == "pending" {
            let symbol = arg(e, 1).map(|v| v.decode::<String>().unwrap());
            write_checked(
                decode_woo_v3_regular_cancel_pending_response(a.as_ref(), symbol.as_deref()),
                e,
                |v, x| pending(&v, x),
            );
        } else {
            let target_index = if e["name"] == "direct" { 2 } else { 1 };
            let t = record(&arg(e, target_index).unwrap());
            let r = match e["name"].as_str().unwrap() {
                "amend" => decode_woo_v3_regular_amend_response(a.as_ref(), &t),
                "cancel" => decode_woo_v3_regular_cancel_response(a.as_ref(), &t),
                "direct" => decode_mutation_response(a.as_ref(), "probe", &t, &["NEW"]),
                _ => panic!("mutation name"),
            };
            write_checked(r, e, |v, x| mutation(&v, x));
        }
    }
    // Concrete constructor fields and an owned deep snapshot, not source shallow reference identity.
    let mut gateway = js_from_str::<JsObjectMap<JsNumberInput>>(
        "{\"success\":false,\"code\":7,\"message\":\"original\",\"nested\":{\"x\":1}}",
    )
    .unwrap();
    let error = WooV3WriteResponseError::new("direct-59".into(), &gateway, Some(-0.0));
    let expected = &s["directError"];
    assert_eq!(
        error.name, expected["name"],
        "REGULAR-WRITES native direct error name"
    );
    assert_eq!(
        error.message, expected["message"],
        "REGULAR-WRITES native direct error message"
    );
    assert_eq!(
        error.contract_status, expected["contractStatus"],
        "REGULAR-WRITES native direct error contract"
    );
    number(error.code.unwrap(), &expected["code"], "direct error code");
    map(&error.gateway_response, &expected["gatewayResponse"]);
    gateway.insert("message".into(), js_from_str("\"changed\"").unwrap());
    assert_eq!(
        error
            .gateway_response
            .get("message")
            .unwrap()
            .clone()
            .decode::<String>()
            .unwrap(),
        "original",
        "REGULAR-WRITES native error snapshot"
    );
}
fn regular(a: &WooV3RegularOrder, e: &Value) {
    assert_eq!(a.symbol, e["symbol"]);
    assert_eq!(a.status, WooV3OrderStatus::New);
    assert_eq!(a.side, WooV3OrderSide::Buy);
    assert_eq!(a.position_side, WooV3PositionSide::Both);
    number(
        a.created_time,
        &e["createdTime"],
        "amend current createdTime",
    );
    number(a.order_id, &e["orderId"], "amend current orderId");
    assert_eq!(a.order_tag, e["orderTag"]);
    assert_eq!(a.price, e["price"]);
    assert_eq!(a.r#type, WooV3RegularOrderType::Limit);
    assert_eq!(a.quantity, e["quantity"]);
    assert!(matches!(a.amount, WooV3OrderAmount::Null));
    assert_eq!(a.visible, e["visible"]);
    assert_eq!(a.executed, e["executed"]);
    assert_eq!(a.total_fee, e["totalFee"]);
    assert_eq!(a.fee_asset, e["feeAsset"]);
    number(
        a.total_rebate,
        &e["totalRebate"],
        "amend current totalRebate",
    );
    assert_eq!(a.rebate_currency, e["rebateCurrency"]);
    number(
        a.client_order_id,
        &e["clientOrderId"],
        "amend current clientOrderId",
    );
    assert_eq!(a.reduce_only, e["reduceOnly"].as_bool().unwrap());
    assert!(a.realized_pnl.is_none());
    assert_eq!(a.average_executed_price, e["averageExecutedPrice"]);
    assert!(a.leverage.is_none() && a.margin_mode.is_none() && a.bid_ask_level.is_none());
}
fn place_input(a: &WooV3RegularPlaceInput, e: &Value) {
    assert_eq!(
        a.symbol, e["symbol"],
        "REGULAR-WRITES native declaration symbol"
    );
    assert_eq!(a.side, WooV3OrderSide::Buy);
    assert_eq!(a.position_mode, WooV3AccountPositionMode::OneWay);
    assert_eq!(
        a.r#type,
        if e["type"] == "LIMIT" {
            WooV3RegularPlaceKind::Limit
        } else {
            WooV3RegularPlaceKind::Market
        }
    );
    assert!(a.time_in_force.is_none());
    assert_eq!(
        a.quantity, e["quantity"],
        "REGULAR-WRITES native declaration quantity"
    );
    assert_eq!(
        a.price.as_deref(),
        e["price"].as_str(),
        "REGULAR-WRITES native declaration price"
    );
    assert_eq!(
        a.reduce_only,
        e["reduceOnly"].as_bool().unwrap(),
        "REGULAR-WRITES native declaration reduceOnly"
    );
    assert_eq!(
        a.client_order_id.is_some(),
        e.get("clientOrderId").is_some()
    );
    if let Some(v) = a.client_order_id {
        number(v, &e["clientOrderId"], "declaration clientOrderId");
    }
    assert_eq!(a.order_tag.as_deref(), e["orderTag"].as_str());
    assert_eq!(a.visible_quantity.as_deref(), e["visibleQuantity"].as_str());
    assert_eq!(
        a.post_only_adjusted,
        e["postOnlyAdjusted"].as_bool(),
        "REGULAR-WRITES native declaration postOnlyAdjusted"
    );
}
#[test]
fn native_declarations() {
    let s = source();
    let rows = s["declarations"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "REGULAR-WRITES declaration floor");
    for row in rows {
        let a: JsNumberInput = js_from_str(row["input"].as_str().unwrap()).unwrap();
        let e = &row["expected"];
        match row["type"].as_str().unwrap() {
            "placeInput" => place_input(&a.decode::<WooV3RegularPlaceInput>().unwrap(), e),
            "placeResult" => place(&a.decode::<WooV3RegularPlaceResult>().unwrap(), e),
            "amendInput" => {
                let v = a.decode::<WooV3RegularAmendInput>().unwrap();
                regular(&v.current_order, &e["currentOrder"]);
                target(&v.target, &e["target"]);
                assert_eq!(
                    v.update.price.as_deref(),
                    e["update"]["price"].as_str(),
                    "REGULAR-WRITES native amend update price"
                );
                assert_eq!(
                    v.update.total_quantity.as_deref(),
                    e["update"]["totalQuantity"].as_str(),
                    "REGULAR-WRITES native amend update totalQuantity"
                );
            }
            "mutationResult" => mutation(&a.decode::<WooV3RegularMutationResult>().unwrap(), e),
            "cancelPendingResult" => {
                pending(&a.decode::<WooV3RegularCancelPendingResult>().unwrap(), e)
            }
            "target" => target(&a.decode::<WooV3RegularWriteTarget>().unwrap(), e),
            _ => panic!("declaration name"),
        }
    }
    let expected = [
        WooV3RegularTimeInForce::Gtc,
        WooV3RegularTimeInForce::Ioc,
        WooV3RegularTimeInForce::Fok,
        WooV3RegularTimeInForce::PostOnly,
    ];
    let values = s["timeInForce"].as_array().unwrap();
    assert_eq!(values.len(), 4, "REGULAR-WRITES TIF floor");
    for (e, v) in expected.iter().zip(values) {
        let a: JsNumberInput = js_from_str(&serde_json::to_string(v).unwrap()).unwrap();
        assert_eq!(
            a.decode::<WooV3RegularTimeInForce>().unwrap(),
            *e,
            "REGULAR-WRITES native declared TIF"
        );
    }
    // Strict record declaration ingress, required fields, present null, exact unions and literal evidence.
    for text in ["[]", "{}", "{\"symbol\":\"x\"}"] {
        assert!(
            js_from_str::<WooV3RegularPlaceInput>(text).is_err(),
            "REGULAR-WRITES native declaration missing/record refusal"
        );
    }
    let refusals = s["declarationRefusals"].as_array().unwrap();
    assert_eq!(
        refusals.len(),
        8,
        "REGULAR-WRITES declaration refusal floor"
    );
    for e in refusals {
        let input: JsNumberInput = js_from_str(e["input"].as_str().unwrap()).unwrap();
        let failed = if e["type"] == "placeInput" {
            input.decode::<WooV3RegularPlaceInput>().is_err()
        } else {
            input.decode::<WooV3RegularMutationResult>().is_err()
        };
        assert!(
            failed,
            "REGULAR-WRITES native declaration present/literal refusal"
        );
    }
    for text in [
        "{}",
        "{\"orderId\":1,\"clientOrderId\":2}",
        "{\"orderId\":null}",
        "[]",
    ] {
        assert!(
            js_from_str::<WooV3RegularWriteTarget>(text).is_err(),
            "REGULAR-WRITES native exclusive target refusal"
        );
    }
    let p = WOO_V3_REGULAR_WRITE_PARITY;
    let e = &s["parity"];
    assert_eq!(
        p.request_and_response_fixtures,
        e["requestAndResponseFixtures"].as_bool().unwrap()
    );
    assert_eq!(p.runtime_imported, e["runtimeImported"].as_bool().unwrap());
    assert_eq!(
        p.credentialed_demo_lifecycle,
        e["credentialedDemoLifecycle"].as_bool().unwrap()
    );
    assert_eq!(
        p.partial_fill_amend_quantity_semantics,
        e["partialFillAmendQuantitySemantics"]
    );
    assert_eq!(p.amend_response_identity, e["amendResponseIdentity"]);
    assert_eq!(p.client_order_id_range, e["clientOrderIdRange"]);
    assert_eq!(p.cross_family_all_orders, e["crossFamilyAllOrders"]);
}

#[test]
fn live_set_membership_precedes_later_place_response_validation() {
    use std::{cell::Cell, rc::Rc};
    let request = WooV3RequestInput {
        method: WooV3HttpMethod::Post,
        path: WooV3Endpoint::TradeOrder,
        query: None,
        body: Some(
            js_from_str::<JsObjectMap<JsNumberInput>>(
                r#"{"type":"LIMIT","quantity":"3.25","price":"101.25","clientOrderId":83}"#,
            )
            .unwrap(),
        ),
    };
    let response: JsNumberInput = js_from_str(r#"{"success":true,"timestamp":97,"data":{"orderId":81,"type":"LIMIT","quantity":"3.25","price":"101.25","amount":"4.75","clientOrderId":83,"bidAskLevel":89}}"#).unwrap();
    let calls = Cell::new(0);
    let actual = decode_woo_v3_regular_place_response_with(Some(&response), &request, |id| {
        assert_eq!(
            id.to_bits(),
            81_f64.to_bits(),
            "WOO-MEMBERSHIP native queried ID"
        );
        calls.set(calls.get() + 1);
        Ok::<_, Rc<String>>(false)
    })
    .expect("WOO-MEMBERSHIP native false override must allow known ID");
    assert_eq!(calls.get(), 1, "WOO-MEMBERSHIP native callback count");
    assert!(matches!(actual.r#type, WooV3RegularPlaceType::Limit));
    assert_eq!(
        (
            actual.price.as_str(),
            actual.quantity.as_str(),
            actual.amount.as_str()
        ),
        ("101.25", "3.25", "4.75")
    );
    for (value, expected) in [
        (actual.order_id, 81_f64),
        (actual.client_order_id, 83_f64),
        (actual.bid_ask_level, 89_f64),
        (actual.timestamp, 97_f64),
    ] {
        assert_eq!(
            value.to_bits(),
            expected.to_bits(),
            "WOO-MEMBERSHIP native decoded numeric field"
        );
    }
    let invalid: JsNumberInput =
        js_from_str(r#"{"success":true,"timestamp":97,"data":{"orderId":81,"type":null}}"#)
            .unwrap();
    let reused = decode_woo_v3_regular_place_response_with(Some(&invalid), &request, |_| {
        Ok::<_, Rc<String>>(true)
    })
    .unwrap_err();
    let WooV3PlaceResponseError::Response(WooV3RegularWriteError::Validation(message)) = reused
    else {
        panic!("WOO-MEMBERSHIP native reused classification")
    };
    assert_eq!(message, "WOO V3 place response reused a known orderId");
    let sentinel = Rc::new("original has error".to_owned());
    let failed = decode_woo_v3_regular_place_response_with(Some(&invalid), &request, |_| {
        Err::<bool, _>(sentinel.clone())
    })
    .unwrap_err();
    let WooV3PlaceResponseError::Membership(error) = failed else {
        panic!("WOO-MEMBERSHIP native original error priority")
    };
    assert!(
        Rc::ptr_eq(&error, &sentinel),
        "WOO-MEMBERSHIP native original error identity"
    );
    let invalid_id: JsNumberInput =
        js_from_str(r#"{"success":true,"timestamp":97,"data":{"orderId":0}}"#).unwrap();
    let before = Cell::new(0);
    assert!(
        decode_woo_v3_regular_place_response_with(Some(&invalid_id), &request, |_| {
            before.set(before.get() + 1);
            Ok::<_, ()>(false)
        })
        .is_err()
    );
    let mut wrong_request = request.clone();
    wrong_request.method = WooV3HttpMethod::Get;
    assert!(
        decode_woo_v3_regular_place_response_with(Some(&response), &wrong_request, |_| {
            before.set(before.get() + 1);
            Ok::<_, ()>(false)
        })
        .is_err()
    );
    assert_eq!(
        before.get(),
        0,
        "WOO-MEMBERSHIP native earlier refusal suppresses callback"
    );
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/regular-writes-oracle.cjs"
        ))
        .arg("--membership-audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["cases"], 5,
        "WOO-MEMBERSHIP source floor stops at wrong request"
    );
}

#[test]
fn unsupported_native_mutation_status_returns_error() {
    let response: JsNumberInput =
        js_from_str(r#"{"success":true,"timestamp":97,"data":{"status":"BAD"}}"#).unwrap();
    let target = js_from_str::<JsObjectMap<JsNumberInput>>(r#"{"orderId":81}"#).unwrap();
    let result = decode_mutation_response(Some(&response), "audit", &target, &["BAD"]);
    let Err(WooV3RegularWriteError::Validation(message)) = result else {
        panic!("WOO-MUTATION-DOMAIN native unsupported status must return validation error");
    };
    assert_eq!(
        message, "WOO V3 mutation status is outside the declared status domain",
        "WOO-MUTATION-DOMAIN native refusal text"
    );
}
