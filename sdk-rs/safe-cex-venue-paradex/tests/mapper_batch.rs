use safe_cex_num::{
    TableHit,
    numeric_ingress::{js_deserialize_passthrough, js_from_str, js_value_as_f64},
};
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderType};
use safe_cex_venue_paradex::{
    order_mapper::map_order,
    order_maps::{map_order_type, map_time_in_force},
    public_ws_helpers::get_order_book_channel,
};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command};

const PROTOTYPES: [&str; 12] = [
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "hasOwnProperty",
    "__lookupGetter__",
    "__lookupSetter__",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "valueOf",
    "__proto__",
    "toLocaleString",
];
fn base() -> Value {
    json!({"id":"order-A","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","status":"NEW",
        "price":"123.75","trigger_price":"246.5","size":"17.25","remaining_size":"3.5",
        "created_at":"2026-10-12T08:09:10.123Z","updated_at":"2026-10-13T11:12:13Z",
        "flags":["REDUCE_ONLY"],"reduce_only":false,"client_id":"client-original","time_in_force":"IOC",
        "extra":{"marker":"raw-extra"}})
}
fn definitions() -> Vec<(String, bool)> {
    let mut defs = Vec::new();
    let mut add = |patch: Value, remove: &[&str], refused| {
        let mut row = base();
        for (key, value) in patch.as_object().unwrap() {
            row[key] = value.clone();
        }
        for key in remove {
            row.as_object_mut().unwrap().shift_remove(*key);
        }
        defs.push((row.to_string(), refused));
    };
    for kind in [
        "LIMIT",
        "MARKET",
        "STOP_LIMIT",
        "STOP_MARKET",
        "STOP_LOSS_LIMIT",
        "STOP_LOSS_MARKET",
        "TAKE_PROFIT_LIMIT",
        "TAKE_PROFIT_MARKET",
    ] {
        for reduce_only in [false, true] {
            for side in ["BUY", "SELL"] {
                add(
                    json!({"type":kind,"reduce_only":reduce_only,"side":side}),
                    &[],
                    false,
                );
            }
        }
    }
    add(json!({"type":"unknown","status":"unknown"}), &[], false);
    for status in [
        "NEW",
        "OPEN",
        "PENDING",
        "UNTRIGGERED",
        "FILLED",
        "CANCELED",
        "REJECTED",
    ] {
        add(json!({"status":status}), &[], false);
    }
    for patch in [
        json!({"price":""}),
        json!({"price":"-0"}),
        json!({"price":"bad"}),
        json!({"price":"Infinity"}),
        json!({"price":" 19.5tail"}),
        json!({"type":"STOP_MARKET","trigger_price":""}),
        json!({"type":"STOP_LIMIT","trigger_price":"-0"}),
        json!({"type":"TAKE_PROFIT_MARKET","trigger_price":"73.125"}),
    ] {
        add(patch, &[], false);
    }
    for size in [
        "0",
        "-0",
        "-1e-9999",
        "1e400",
        "-1e400",
        "9007199254740993",
        "1.0000000000000002",
        "17.25x",
    ] {
        add(json!({"size":size}), &[], false);
    }
    for date in [
        "not-a-date",
        "",
        "2026-10-12T08:09:10.123",
        "2026-10-12T08:09:10.123+03:30",
        "+275760-09-13T00:00:00.001Z",
    ] {
        add(json!({"created_at":date}), &[], false);
    }
    add(json!({"reduce_only":null}), &[], false);
    add(json!({"reduce_only":"yes"}), &[], false);
    for flags in [
        json!(["REDUCE_ONLY"]),
        Value::Null,
        json!(["reduce_only"]),
        json!(["x", "REDUCE_ONLY"]),
        json!("REDUCE_ONLY"),
    ] {
        add(json!({"flags":flags}), &["reduce_only"], false);
    }
    let mut numeric_defs = Vec::new();
    for raw in [
        "0",
        "-0",
        "-0.0",
        "-1e-9999",
        "1e400",
        "-1e400",
        "9007199254740993",
        "1.0000000000000002",
    ] {
        let mut row = base();
        row["extra"] = json!({"token":"TOKEN"});
        numeric_defs.push((row.to_string().replace("\"TOKEN\"", raw), false));
    }
    for field in ["side", "type", "status"] {
        for key in PROTOTYPES {
            add(json!({field:key}), &[], true);
        }
    }
    for patch in [
        json!({"side":"unknown"}),
        json!({"side":""}),
        json!({"id":null}),
    ] {
        add(patch, &[], true);
    }
    add(json!({"created_at":7}), &[], false);
    defs.splice(68..68, numeric_defs);
    for raw in [
        "1784505600000",
        "-1784505600000",
        "7.9",
        "-7.9",
        "0",
        "-0",
        "8640000000000000",
        "8640000000000001",
        "-8640000000000000",
        "-8640000000000001",
        "1e400",
        "-1e400",
    ] {
        let mut row = base();
        row["created_at"] = json!("TOKEN");
        defs.push((row.to_string().replace("\"TOKEN\"", raw), false));
    }
    for date in [Value::Null, json!(false), json!(true)] {
        let mut row = base();
        row["created_at"] = date;
        defs.push((row.to_string(), false));
    }
    let mut row = base();
    row.as_object_mut().unwrap().shift_remove("created_at");
    defs.push((row.to_string(), false));
    for field in ["size", "remaining_size"] {
        for raw in [
            "0.006",
            "-0",
            "null",
            "true",
            "false",
            "[\"17.25tail\"]",
            "{}",
        ] {
            let mut row = base();
            row[field] = json!("TOKEN");
            defs.push((row.to_string().replace("\"TOKEN\"", raw), false));
        }
        let mut row = base();
        row.as_object_mut().unwrap().shift_remove(field);
        defs.push((row.to_string(), false));
    }
    for date in [
        json!({}),
        json!([]),
        json!([null]),
        json!(["2026-07-20T00:00:00Z"]),
    ] {
        let mut row = base();
        row["created_at"] = date;
        defs.push((row.to_string(), false));
    }
    defs
}
fn decode(raw: &str) -> Value {
    js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)).unwrap()
}
fn source_field<'a>(source: &'a Value, key: &str) -> &'a Value {
    &source["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry[0] == key)
        .unwrap()[1]
}
fn assert_string(native: &str, source: &Value, label: &str) {
    assert_eq!(source["kind"], "string", "{label}_kind");
    assert_eq!(native, source["value"].as_str().unwrap(), "{label}");
}
fn assert_number(native: f64, source: &Value, label: &str) {
    assert_eq!(source["kind"], "number", "{label}_kind");
    let bits = if native.is_nan() {
        0x7ff8000000000000
    } else {
        native.to_bits()
    };
    assert_eq!(
        bits,
        u64::from_str_radix(source["bits"].as_str().unwrap(), 16).unwrap(),
        "{label}"
    );
}
fn assert_json(native: &Value, source: &Value, label: &str) {
    match native {
        Value::Null => assert_eq!(source["kind"], "null", "{label}"),
        Value::Bool(value) => {
            assert_eq!(source["kind"], "boolean", "{label}");
            assert_eq!(Some(*value), source["value"].as_bool(), "{label}");
        }
        Value::String(value) => assert_string(value, source, label),
        Value::Number(_) => assert_number(js_value_as_f64(native).unwrap(), source, label),
        Value::Array(values) => {
            assert_eq!(source["kind"], "array", "{label}");
            let entries = source["entries"].as_array().unwrap();
            assert_eq!(values.len(), entries.len(), "{label}_array_count");
            for (value, expected) in values.iter().zip(entries) {
                assert_json(value, expected, label);
            }
        }
        Value::Object(values) => {
            assert_eq!(source["kind"], "object", "{label}");
            let entries = source["entries"].as_array().unwrap();
            assert_eq!(values.len(), entries.len(), "{label}_object_count");
            for ((key, value), entry) in values.iter().zip(entries) {
                assert_eq!(key, entry[0].as_str().unwrap(), "{label}_key_order");
                assert_json(value, &entry[1], label);
            }
        }
    }
}
fn assert_order(order: &Order, source: &Value) {
    assert_eq!(
        source["entries"].as_array().unwrap().len(),
        13,
        "order_source_field_count"
    );
    assert_string(&order.id, source_field(source, "id"), "order_native_id");
    assert_string(
        &order.symbol,
        source_field(source, "symbol"),
        "order_native_symbol",
    );
    if order.side.is_undefined() {
        assert_eq!(
            source_field(source, "side")["kind"],
            "undefined",
            "order_native_undefined_side"
        );
    } else {
        let side = match order.side.value().expect("present source enum in fixture") {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        };
        assert_string(side, source_field(source, "side"), "order_native_side");
    }
    let kind = match order
        .r#type
        .value()
        .expect("present source enum in fixture")
    {
        OrderType::Market => "market",
        OrderType::Limit => "limit",
        OrderType::Conditional => "conditional",
        OrderType::StopLoss => "stop_market",
        OrderType::TakeProfit => "take_profit_market",
        OrderType::TrailingStopLoss => "trailing_stop_market",
    };
    let status = match order
        .status
        .value()
        .expect("present source enum in fixture")
    {
        OrderStatus::Open => "open",
        OrderStatus::Closed => "closed",
        OrderStatus::Canceled => "canceled",
        OrderStatus::Creating => "creating",
        OrderStatus::Updating => "updating",
        OrderStatus::Canceling => "canceling",
    };
    assert_string(kind, source_field(source, "type"), "order_native_type");
    assert_string(
        status,
        source_field(source, "status"),
        "order_native_status",
    );
    for (key, value) in [
        ("price", order.price.to_number()),
        ("amount", order.amount),
        ("filled", order.filled),
        ("remaining", order.remaining),
        ("lastUpdate", order.last_update),
    ] {
        assert_number(
            value,
            source_field(source, key),
            &format!("order_native_{key}"),
        );
    }
    let reduce = source_field(source, "reduceOnly");
    assert_eq!(reduce["kind"], "boolean");
    assert_eq!(
        order.reduce_only.into_value(),
        reduce["value"].as_bool(),
        "order_native_reduce_only"
    );
    assert!(order.time_in_force.is_none(), "order_native_time_in_force");
    assert_eq!(source_field(source, "timeInForce")["kind"], "undefined");
    assert_json(
        order.info.as_ref().unwrap(),
        source_field(source, "info"),
        "order_native_info",
    );
    assert!(
        order.parent_id.is_none()
            && order.close.is_none()
            && order.trigger_type.is_none()
            && order.tp_trigger_type.is_none()
            && order.sl_trigger_type.is_none()
            && order.take_profit.is_none()
            && order.stop_loss.is_none()
            && order.extra.is_none(),
        "order_native_absent_extras"
    );
}
#[test]
fn actual_typescript_mapper_batch() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/mapper_batch_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "batch_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert_eq!(corpus["header"]["timezone"], "UTC");
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let expected = definitions();
    assert_eq!(expected.len(), 152, "batch_independent_fixture_count");
    let orders = corpus["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 152, "batch_source_order_count");
    let (mut mapped, mut refused, mut local_dates, mut recovered_sides) = (0, 0, 0, 0);
    for (i, (raw, reject)) in expected.iter().enumerate() {
        let case = &orders[i];
        assert_eq!(
            case["refused"].as_bool(),
            Some(*reject),
            "batch_live_refusal_content {i}"
        );
        let input = decode(raw);
        assert_json(&input, &case["input"], "batch_live_input_content");
        let row = input.as_object().unwrap();
        let local_calls = Cell::new(0);
        let result = map_order(row, |ms| {
            local_calls.set(local_calls.get() + 1);
            ms
        });
        // The frozen source corpus retains its original refusal labels. These
        // two source-undefined cases now inhabit the existing ModelField carrier.
        let recovered_side = (112..114).contains(&i);
        if recovered_side {
            assert_eq!(row["side"], if i == 112 { "unknown" } else { "" });
            assert_eq!(
                source_field(&case["output"], "side")["kind"],
                "undefined",
                "order_source_unknown_side"
            );
            assert!(
                result.as_ref().unwrap().side.is_undefined(),
                "order_native_unknown_side"
            );
            recovered_sides += 1;
        }
        let native_reject = *reject && !recovered_side;
        assert_eq!(
            result.is_err(),
            native_reject,
            "order_native_boundary_refusal {i}"
        );
        if native_reject {
            assert!(!result.unwrap_err().is_empty(), "order_visible_refusal");
            let source = &case["output"];
            if i < 112 {
                let field = ["side", "type", "status"][(i - 76) / 12];
                assert!(
                    matches!(
                        source_field(source, field)["kind"].as_str(),
                        Some("function" | "object")
                    ),
                    "order_source_prototype_boundary"
                );
            } else {
                assert_eq!(
                    source_field(source, "id")["kind"],
                    "null",
                    "order_source_null_id"
                );
            }
            refused += 1;
        } else {
            let order = result.unwrap();
            assert_order(&order, &case["output"]);
            assert_eq!(
                order.info.as_ref().unwrap().as_object().unwrap().len(),
                row.len(),
                "order_native_info_independent_count"
            );
            assert_eq!(case["ownsUndefined"].as_bool(), Some(true));
            assert_eq!(case["aliasesInput"].as_bool(), Some(true));
            mapped += 1;
        }
        assert_eq!(
            local_calls.get(),
            usize::from(i == 58),
            "order_native_local_timezone_calls {i}"
        );
        local_dates += local_calls.get();
    }
    assert_eq!(mapped, 115, "batch_completed_native_orders");
    assert_eq!(refused, 37, "batch_completed_refusals");
    assert_eq!(recovered_sides, 2, "batch_completed_undefined_sides");
    assert_eq!(local_dates, 1, "batch_completed_local_dates");
    let mut tif_keys = vec![None, Some("")];
    tif_keys.extend(
        [
            "GoodTillCancel",
            "GTC",
            "ImmediateOrCancel",
            "IOC",
            "PostOnly",
            "POST_ONLY",
            "GTT",
            "FOK",
        ]
        .map(Some),
    );
    tif_keys.extend(PROTOTYPES.map(Some));
    tif_keys.extend(["unknown", "é😀", "fok"].map(Some));
    let tif = corpus["timeInForce"].as_array().unwrap();
    assert_eq!(tif.len(), 25, "batch_tif_count");
    let mut completed_tif = 0;
    for (i, key) in tif_keys.iter().enumerate() {
        assert_eq!(tif[i]["key"].as_str(), *key, "tif_live_input_content");
        let source = &tif[i]["output"];
        match map_time_in_force(*key) {
            TableHit::Mapped(value) => assert_string(value, source, "tif_native_value"),
            TableHit::Inherited(member) => {
                assert_eq!(
                    source["kind"],
                    if member == safe_cex_num::ProtoMember::Prototype {
                        "object"
                    } else {
                        "function"
                    },
                    "tif_native_inherited_kind"
                );
                if member != safe_cex_num::ProtoMember::Prototype {
                    assert_eq!(
                        member.js_string(),
                        source["text"],
                        "tif_native_inherited_identity"
                    );
                }
            }
            TableHit::Missing => panic!("tif_native_missing_fallback"),
        }
        completed_tif += 1;
    }
    assert_eq!(completed_tif, 25, "batch_completed_tif");
    let kinds = [
        (OrderType::Market, "market"),
        (OrderType::Limit, "limit"),
        (OrderType::Conditional, "conditional"),
        (OrderType::StopLoss, "stop_market"),
        (OrderType::TakeProfit, "take_profit_market"),
        (OrderType::TrailingStopLoss, "trailing_stop_market"),
    ];
    let types = corpus["types"].as_array().unwrap();
    assert_eq!(types.len(), 6, "batch_types_count");
    let mut completed_types = 0;
    for (i, (kind, key)) in kinds.into_iter().enumerate() {
        assert_eq!(types[i]["key"], key, "type_live_input_content");
        assert_eq!(
            map_order_type(kind),
            types[i]["output"].as_str().unwrap(),
            "type_native_value"
        );
        completed_types += 1;
    }
    assert_eq!(completed_types, 6, "batch_completed_types");
    let channels = corpus["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 7, "batch_channel_count");
    let mut completed_channels = 0;
    for (i, key) in ["BTC-USD-PERP", "", "é😀", "constructor", "a.b", "\0", "x\n"]
        .iter()
        .enumerate()
    {
        assert_eq!(channels[i]["key"], *key, "channel_live_input_content");
        assert_eq!(
            get_order_book_channel(key),
            channels[i]["output"].as_str().unwrap(),
            "channel_native_value"
        );
        completed_channels += 1;
    }
    assert_eq!(completed_channels, 7, "batch_completed_channels");
    let mut raw = base();
    let order = map_order(raw.as_object().unwrap(), |ms| ms).unwrap();
    raw["extra"]["marker"] = json!("changed-after-map");
    assert_eq!(
        order.info.as_ref().unwrap()["extra"]["marker"],
        "raw-extra",
        "order_native_owned_info"
    );
    assert_eq!(
        corpus["boundary"]["aliasMarker"], "changed-after-map",
        "order_source_aliased_info"
    );
    assert_eq!(
        corpus["boundary"]["surrogateTif"], "GTC",
        "tif_source_surrogate"
    );
    assert_eq!(
        corpus["boundary"]["surrogateOrder"],
        json!([0xd800]),
        "order_source_surrogate"
    );
    let mut channel_units: Vec<u16> = "order_book.".encode_utf16().collect();
    channel_units.push(0xd800);
    channel_units.extend(".snapshot@15@100ms".encode_utf16());
    assert_eq!(
        corpus["boundary"]["surrogateChannel"],
        json!(channel_units),
        "channel_source_surrogate"
    );
    assert!(
        js_from_str::<String>(corpus["boundary"]["surrogateRaw"].as_str().unwrap()).is_err(),
        "batch_utf8_boundary_refusal"
    );
    // Wire reflection differs for the source's own undefined property.
    assert!(
        serde_json::to_value(&order)
            .unwrap()
            .get("timeInForce")
            .is_none(),
        "order_owned_undefined_wire_boundary"
    );
}
