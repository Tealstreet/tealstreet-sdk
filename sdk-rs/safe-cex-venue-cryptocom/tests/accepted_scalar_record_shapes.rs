//! Reviewed scalar fields, retained opaque values, native bits and complete arrays.
use safe_cex_num::numeric_ingress::{self, JsNumberOutput};
use safe_cex_venue_cryptocom::wire::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn paths<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
) -> [Result<T, serde_json::Error>; 6] {
    let input: Value = serde_json::from_str(raw).unwrap();
    [
        serde_json::from_str(raw),
        T::deserialize(input.clone()),
        T::deserialize(&input),
        numeric_ingress::js_from_str(raw),
        numeric_ingress::js_from_value(input.clone()),
        numeric_ingress::js_from_value_ref(&input),
    ]
}
fn native_json(v: &Value) -> Value {
    match v {
        Value::Null => json!({"kind":"null"}),
        Value::Bool(b) => json!({"kind":"boolean","value":b}),
        Value::String(s) => json!({"kind":"string","value":s}),
        Value::Number(_) => {
            json!({"kind":"number","bits":format!("{:016x}",numeric_ingress::js_value_as_f64(v).unwrap().to_bits())})
        }
        Value::Array(v) => {
            json!({"kind":"array","items":v.iter().map(native_json).collect::<Vec<_>>()})
        }
        Value::Object(m) => {
            json!({"kind":"object","fields":m.iter().map(|(k,v)|(k.clone(),native_json(v))).collect::<serde_json::Map<_,_>>()})
        }
    }
}
fn observe_instrument(v: &CryptocomInstrument) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("symbol".into(), json!(v.symbol));
    out.insert("inst_type".into(), json!(v.inst_type));
    out.insert("display_name".into(), json!(v.display_name));
    out.insert("base_ccy".into(), json!(v.base_ccy));
    out.insert("quote_ccy".into(), json!(v.quote_ccy));
    out.insert(
        "quote_decimals_bits".into(),
        json!(format!("{:016x}", v.quote_decimals.to_bits())),
    );
    out.insert(
        "quantity_decimals_bits".into(),
        json!(format!("{:016x}", v.quantity_decimals.to_bits())),
    );
    out.insert("price_tick_size".into(), json!(v.price_tick_size));
    out.insert("qty_tick_size".into(), json!(v.qty_tick_size));
    out.insert("max_leverage".into(), json!(v.max_leverage));
    out.insert("tradable".into(), json!(v.tradable));
    out.insert(
        "expiry_timestamp_ms_bits".into(),
        json!(format!("{:016x}", v.expiry_timestamp_ms.to_bits())),
    );
    out.insert("beta_product".into(), json!(v.beta_product));
    out.insert("underlying_symbol".into(), json!(v.underlying_symbol));
    out.insert("contract_size".into(), json!(v.contract_size));
    out.insert("margin_buy_enabled".into(), json!(v.margin_buy_enabled));
    out.insert("margin_sell_enabled".into(), json!(v.margin_sell_enabled));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_instrument() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"symbol\":\"symbol-A\",\"inst_type\":\"inst_type-A\",\"display_name\":\"display_name-A\",\"base_ccy\":\"base_ccy-A\",\"quote_ccy\":\"quote_ccy-A\",\"quote_decimals\":-0.0,\"quantity_decimals\":17.5,\"price_tick_size\":\"price_tick_size-A\",\"qty_tick_size\":\"qty_tick_size-A\",\"max_leverage\":\"max_leverage-A\",\"tradable\":true,\"expiry_timestamp_ms\":31.5,\"beta_product\":false,\"underlying_symbol\":\"underlying_symbol-A\",\"contract_size\":\"contract_size-A\",\"margin_buy_enabled\":false,\"margin_sell_enabled\":false,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"symbol":"symbol-A","inst_type":"inst_type-A","display_name":"display_name-A","base_ccy":"base_ccy-A","quote_ccy":"quote_ccy-A","quote_decimals_bits":"8000000000000000","quantity_decimals_bits":"4031800000000000","price_tick_size":"price_tick_size-A","qty_tick_size":"qty_tick_size-A","max_leverage":"max_leverage-A","tradable":true,"expiry_timestamp_ms_bits":"403f800000000000","beta_product":false,"underlying_symbol":"underlying_symbol-A","contract_size":"contract_size-A","margin_buy_enabled":false,"margin_sell_enabled":false,"additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["symbol","inst_type","display_name","base_ccy","quote_ccy","quote_decimals","quantity_decimals","price_tick_size","qty_tick_size","max_leverage","tradable","expiry_timestamp_ms","beta_product","underlying_symbol","contract_size","margin_buy_enabled","margin_sell_enabled","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"margin_sell_enabled\":false,\"margin_buy_enabled\":false,\"contract_size\":\"contract_size-A\",\"underlying_symbol\":\"underlying_symbol-A\",\"beta_product\":false,\"expiry_timestamp_ms\":31.5,\"tradable\":true,\"max_leverage\":\"max_leverage-A\",\"qty_tick_size\":\"qty_tick_size-A\",\"price_tick_size\":\"price_tick_size-A\",\"quantity_decimals\":17.5,\"quote_decimals\":-0.0,\"quote_ccy\":\"quote_ccy-A\",\"base_ccy\":\"base_ccy-A\",\"display_name\":\"display_name-A\",\"inst_type\":\"inst_type-A\",\"symbol\":\"symbol-A\"}","output":{"ok":true,"symbol":"symbol-A","inst_type":"inst_type-A","display_name":"display_name-A","base_ccy":"base_ccy-A","quote_ccy":"quote_ccy-A","quote_decimals_bits":"8000000000000000","quantity_decimals_bits":"4031800000000000","price_tick_size":"price_tick_size-A","qty_tick_size":"qty_tick_size-A","max_leverage":"max_leverage-A","tradable":true,"expiry_timestamp_ms_bits":"403f800000000000","beta_product":false,"underlying_symbol":"underlying_symbol-A","contract_size":"contract_size-A","margin_buy_enabled":false,"margin_sell_enabled":false,"additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","margin_sell_enabled","margin_buy_enabled","contract_size","underlying_symbol","beta_product","expiry_timestamp_ms","tradable","max_leverage","qty_tick_size","price_tick_size","quantity_decimals","quote_decimals","quote_ccy","base_ccy","display_name","inst_type","symbol"]}},{"input":"{\"symbol\":\"symbol-A\",\"inst_type\":\"inst_type-A\",\"display_name\":\"display_name-A\",\"base_ccy\":\"base_ccy-A\",\"quote_ccy\":\"quote_ccy-A\",\"quote_decimals\":-0.0,\"quantity_decimals\":17.5,\"price_tick_size\":\"price_tick_size-A\",\"qty_tick_size\":\"qty_tick_size-A\",\"max_leverage\":\"max_leverage-A\",\"tradable\":false,\"expiry_timestamp_ms\":31.5,\"beta_product\":true,\"underlying_symbol\":\"underlying_symbol-A\",\"contract_size\":\"contract_size-A\",\"margin_buy_enabled\":false,\"margin_sell_enabled\":false,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"symbol":"symbol-A","inst_type":"inst_type-A","display_name":"display_name-A","base_ccy":"base_ccy-A","quote_ccy":"quote_ccy-A","quote_decimals_bits":"8000000000000000","quantity_decimals_bits":"4031800000000000","price_tick_size":"price_tick_size-A","qty_tick_size":"qty_tick_size-A","max_leverage":"max_leverage-A","tradable":false,"expiry_timestamp_ms_bits":"403f800000000000","beta_product":true,"underlying_symbol":"underlying_symbol-A","contract_size":"contract_size-A","margin_buy_enabled":false,"margin_sell_enabled":false,"additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["symbol","inst_type","display_name","base_ccy","quote_ccy","quote_decimals","quantity_decimals","price_tick_size","qty_tick_size","max_leverage","tradable","expiry_timestamp_ms","beta_product","underlying_symbol","contract_size","margin_buy_enabled","margin_sell_enabled","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"symbol\":\"symbol-A\",\"inst_type\":\"inst_type-A\",\"display_name\":\"display_name-A\",\"base_ccy\":\"base_ccy-A\",\"quote_ccy\":\"quote_ccy-A\",\"quote_decimals\":-0.0,\"quantity_decimals\":17.5,\"price_tick_size\":\"price_tick_size-A\",\"qty_tick_size\":\"qty_tick_size-A\",\"max_leverage\":\"max_leverage-A\",\"tradable\":false,\"expiry_timestamp_ms\":31.5,\"beta_product\":false,\"underlying_symbol\":\"underlying_symbol-A\",\"contract_size\":\"contract_size-A\",\"margin_buy_enabled\":true,\"margin_sell_enabled\":false,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"symbol":"symbol-A","inst_type":"inst_type-A","display_name":"display_name-A","base_ccy":"base_ccy-A","quote_ccy":"quote_ccy-A","quote_decimals_bits":"8000000000000000","quantity_decimals_bits":"4031800000000000","price_tick_size":"price_tick_size-A","qty_tick_size":"qty_tick_size-A","max_leverage":"max_leverage-A","tradable":false,"expiry_timestamp_ms_bits":"403f800000000000","beta_product":false,"underlying_symbol":"underlying_symbol-A","contract_size":"contract_size-A","margin_buy_enabled":true,"margin_sell_enabled":false,"additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["symbol","inst_type","display_name","base_ccy","quote_ccy","quote_decimals","quantity_decimals","price_tick_size","qty_tick_size","max_leverage","tradable","expiry_timestamp_ms","beta_product","underlying_symbol","contract_size","margin_buy_enabled","margin_sell_enabled","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"symbol\":\"symbol-A\",\"inst_type\":\"inst_type-A\",\"display_name\":\"display_name-A\",\"base_ccy\":\"base_ccy-A\",\"quote_ccy\":\"quote_ccy-A\",\"quote_decimals\":-0.0,\"quantity_decimals\":17.5,\"price_tick_size\":\"price_tick_size-A\",\"qty_tick_size\":\"qty_tick_size-A\",\"max_leverage\":\"max_leverage-A\",\"tradable\":false,\"expiry_timestamp_ms\":31.5,\"beta_product\":false,\"underlying_symbol\":\"underlying_symbol-A\",\"contract_size\":\"contract_size-A\",\"margin_buy_enabled\":false,\"margin_sell_enabled\":true,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"symbol":"symbol-A","inst_type":"inst_type-A","display_name":"display_name-A","base_ccy":"base_ccy-A","quote_ccy":"quote_ccy-A","quote_decimals_bits":"8000000000000000","quantity_decimals_bits":"4031800000000000","price_tick_size":"price_tick_size-A","qty_tick_size":"qty_tick_size-A","max_leverage":"max_leverage-A","tradable":false,"expiry_timestamp_ms_bits":"403f800000000000","beta_product":false,"underlying_symbol":"underlying_symbol-A","contract_size":"contract_size-A","margin_buy_enabled":false,"margin_sell_enabled":true,"additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["symbol","inst_type","display_name","base_ccy","quote_ccy","quote_decimals","quantity_decimals","price_tick_size","qty_tick_size","max_leverage","tradable","expiry_timestamp_ms","beta_product","underlying_symbol","contract_size","margin_buy_enabled","margin_sell_enabled","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[\"symbol-A\",\"inst_type-A\",\"display_name-A\",\"base_ccy-A\",\"quote_ccy-A\",-0.0,17.5,\"price_tick_size-A\",\"qty_tick_size-A\",\"max_leverage-A\",true,31.5,false,\"underlying_symbol-A\",\"contract_size-A\",false,false]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{\"inst_type\": \"inst_type-A\", \"display_name\": \"display_name-A\", \"base_ccy\": \"base_ccy-A\", \"quote_ccy\": \"quote_ccy-A\", \"quote_decimals\": -0.0, \"quantity_decimals\": 17.5, \"price_tick_size\": \"price_tick_size-A\", \"qty_tick_size\": \"qty_tick_size-A\", \"max_leverage\": \"max_leverage-A\", \"tradable\": true, \"expiry_timestamp_ms\": 31.5, \"beta_product\": false, \"underlying_symbol\": \"underlying_symbol-A\", \"contract_size\": \"contract_size-A\", \"margin_buy_enabled\": false, \"margin_sell_enabled\": false}","output":{"ok":false}},{"input":"{\"symbol\": null, \"inst_type\": \"inst_type-A\", \"display_name\": \"display_name-A\", \"base_ccy\": \"base_ccy-A\", \"quote_ccy\": \"quote_ccy-A\", \"quote_decimals\": -0.0, \"quantity_decimals\": 17.5, \"price_tick_size\": \"price_tick_size-A\", \"qty_tick_size\": \"qty_tick_size-A\", \"max_leverage\": \"max_leverage-A\", \"tradable\": true, \"expiry_timestamp_ms\": 31.5, \"beta_product\": false, \"underlying_symbol\": \"underlying_symbol-A\", \"contract_size\": \"contract_size-A\", \"margin_buy_enabled\": false, \"margin_sell_enabled\": false}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomInstrument>(raw).into_iter().enumerate() {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_instrument(&value),
                        case["output"],
                        "CryptocomInstrument fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_instrument(&copy),
                        observe_instrument(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomInstrument rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_instrument() {
    let raw = r###"["symbol-A","inst_type-A","display_name-A","base_ccy-A","quote_ccy-A",-0.0,17.5,"price_tick_size-A","qty_tick_size-A","max_leverage-A",true,31.5,false,"underlying_symbol-A","contract_size-A",false,false]"###;
    for (path, result) in paths::<CryptocomInstrument>(raw).into_iter().enumerate() {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomInstrument path {path}"
        );
    }
}
fn observe_ticker(v: &CryptocomTicker) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("i".into(), json!(v.i));
    out.insert("b".into(), json!(v.b));
    out.insert("k".into(), json!(v.k));
    out.insert("a".into(), json!(v.a));
    out.insert("c".into(), json!(v.c));
    out.insert("h".into(), json!(v.h));
    out.insert("l".into(), json!(v.l));
    out.insert("v".into(), json!(v.v));
    out.insert("vv".into(), json!(v.vv));
    out.insert("oi".into(), json!(v.oi));
    out.insert("t_bits".into(), json!(format!("{:016x}", v.t.to_bits())));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_ticker() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"i\":\"i-A\",\"b\":\"b-A\",\"k\":\"k-A\",\"a\":\"a-A\",\"c\":\"c-A\",\"h\":\"h-A\",\"l\":\"l-A\",\"v\":\"v-A\",\"vv\":\"vv-A\",\"oi\":\"oi-A\",\"t\":-0.0,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"i":"i-A","b":"b-A","k":"k-A","a":"a-A","c":"c-A","h":"h-A","l":"l-A","v":"v-A","vv":"vv-A","oi":"oi-A","t_bits":"8000000000000000","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["i","b","k","a","c","h","l","v","vv","oi","t","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"t\":-0.0,\"oi\":\"oi-A\",\"vv\":\"vv-A\",\"v\":\"v-A\",\"l\":\"l-A\",\"h\":\"h-A\",\"c\":\"c-A\",\"a\":\"a-A\",\"k\":\"k-A\",\"b\":\"b-A\",\"i\":\"i-A\"}","output":{"ok":true,"i":"i-A","b":"b-A","k":"k-A","a":"a-A","c":"c-A","h":"h-A","l":"l-A","v":"v-A","vv":"vv-A","oi":"oi-A","t_bits":"8000000000000000","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","t","oi","vv","v","l","h","c","a","k","b","i"]}},{"input":"{\"i\":\"i-B\",\"b\":\"b-B\",\"k\":\"k-B\",\"a\":\"a-B\",\"c\":\"c-B\",\"h\":\"h-B\",\"l\":\"l-B\",\"v\":\"v-B\",\"vv\":\"vv-B\",\"oi\":\"oi-B\",\"t\":17.5,\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"i":"i-B","b":"b-B","k":"k-B","a":"a-B","c":"c-B","h":"h-B","l":"l-B","v":"v-B","vv":"vv-B","oi":"oi-B","t_bits":"4031800000000000","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["i","b","k","a","c","h","l","v","vv","oi","t","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[\"i-A\",\"b-A\",\"k-A\",\"a-A\",\"c-A\",\"h-A\",\"l-A\",\"v-A\",\"vv-A\",\"oi-A\",-0.0]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{\"b\": \"b-A\", \"k\": \"k-A\", \"a\": \"a-A\", \"c\": \"c-A\", \"h\": \"h-A\", \"l\": \"l-A\", \"v\": \"v-A\", \"vv\": \"vv-A\", \"oi\": \"oi-A\", \"t\": -0.0}","output":{"ok":false}},{"input":"{\"i\": null, \"b\": \"b-A\", \"k\": \"k-A\", \"a\": \"a-A\", \"c\": \"c-A\", \"h\": \"h-A\", \"l\": \"l-A\", \"v\": \"v-A\", \"vv\": \"vv-A\", \"oi\": \"oi-A\", \"t\": -0.0}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomTicker>(raw).into_iter().enumerate() {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_ticker(&value),
                        case["output"],
                        "CryptocomTicker fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_ticker(&copy),
                        observe_ticker(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomTicker rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_ticker() {
    let raw = r###"["i-A","b-A","k-A","a-A","c-A","h-A","l-A","v-A","vv-A","oi-A",-0.0]"###;
    for (path, result) in paths::<CryptocomTicker>(raw).into_iter().enumerate() {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomTicker path {path}"
        );
    }
}
fn observe_candlestick(v: &CryptocomCandlestick) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("t_bits".into(), json!(format!("{:016x}", v.t.to_bits())));
    out.insert("o".into(), json!(v.o));
    out.insert("h".into(), json!(v.h));
    out.insert("l".into(), json!(v.l));
    out.insert("c".into(), json!(v.c));
    out.insert("v".into(), json!(v.v));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_candlestick() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"t\":-0.0,\"o\":\"o-A\",\"h\":\"h-A\",\"l\":\"l-A\",\"c\":\"c-A\",\"v\":\"v-A\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"t_bits":"8000000000000000","o":"o-A","h":"h-A","l":"l-A","c":"c-A","v":"v-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["t","o","h","l","c","v","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"v\":\"v-A\",\"c\":\"c-A\",\"l\":\"l-A\",\"h\":\"h-A\",\"o\":\"o-A\",\"t\":-0.0}","output":{"ok":true,"t_bits":"8000000000000000","o":"o-A","h":"h-A","l":"l-A","c":"c-A","v":"v-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","v","c","l","h","o","t"]}},{"input":"{\"t\":17.5,\"o\":\"o-B\",\"h\":\"h-B\",\"l\":\"l-B\",\"c\":\"c-B\",\"v\":\"v-B\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"t_bits":"4031800000000000","o":"o-B","h":"h-B","l":"l-B","c":"c-B","v":"v-B","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["t","o","h","l","c","v","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[-0.0,\"o-A\",\"h-A\",\"l-A\",\"c-A\",\"v-A\"]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{\"o\": \"o-A\", \"h\": \"h-A\", \"l\": \"l-A\", \"c\": \"c-A\", \"v\": \"v-A\"}","output":{"ok":false}},{"input":"{\"t\": null, \"o\": \"o-A\", \"h\": \"h-A\", \"l\": \"l-A\", \"c\": \"c-A\", \"v\": \"v-A\"}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomCandlestick>(raw).into_iter().enumerate() {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_candlestick(&value),
                        case["output"],
                        "CryptocomCandlestick fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_candlestick(&copy),
                        observe_candlestick(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomCandlestick rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_candlestick() {
    let raw = r###"[-0.0,"o-A","h-A","l-A","c-A","v-A"]"###;
    for (path, result) in paths::<CryptocomCandlestick>(raw).into_iter().enumerate() {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomCandlestick path {path}"
        );
    }
}
fn observe_positionbalance(v: &CryptocomPositionBalance) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("instrument_name".into(), json!(v.instrument_name));
    out.insert("quantity".into(), json!(v.quantity));
    out.insert("reserved_qty".into(), json!(v.reserved_qty));
    out.insert("collateral_amount".into(), json!(v.collateral_amount));
    out.insert("collateral_eligible".into(), json!(v.collateral_eligible));
    out.insert("haircut".into(), json!(v.haircut));
    out.insert("market_value".into(), json!(v.market_value));
    out.insert(
        "max_withdrawal_balance".into(),
        json!(v.max_withdrawal_balance),
    );
    out.insert("hourly_interest_rate".into(), json!(v.hourly_interest_rate));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_positionbalance() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"instrument_name\":\"instrument_name-A\",\"quantity\":\"quantity-A\",\"reserved_qty\":\"reserved_qty-A\",\"collateral_amount\":\"collateral_amount-A\",\"collateral_eligible\":false,\"haircut\":\"haircut-A\",\"market_value\":\"market_value-A\",\"max_withdrawal_balance\":\"max_withdrawal_balance-A\",\"hourly_interest_rate\":\"hourly_interest_rate-A\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"instrument_name":"instrument_name-A","quantity":"quantity-A","reserved_qty":"reserved_qty-A","collateral_amount":"collateral_amount-A","collateral_eligible":false,"haircut":"haircut-A","market_value":"market_value-A","max_withdrawal_balance":"max_withdrawal_balance-A","hourly_interest_rate":"hourly_interest_rate-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["instrument_name","quantity","reserved_qty","collateral_amount","collateral_eligible","haircut","market_value","max_withdrawal_balance","hourly_interest_rate","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"hourly_interest_rate\":\"hourly_interest_rate-A\",\"max_withdrawal_balance\":\"max_withdrawal_balance-A\",\"market_value\":\"market_value-A\",\"haircut\":\"haircut-A\",\"collateral_eligible\":false,\"collateral_amount\":\"collateral_amount-A\",\"reserved_qty\":\"reserved_qty-A\",\"quantity\":\"quantity-A\",\"instrument_name\":\"instrument_name-A\"}","output":{"ok":true,"instrument_name":"instrument_name-A","quantity":"quantity-A","reserved_qty":"reserved_qty-A","collateral_amount":"collateral_amount-A","collateral_eligible":false,"haircut":"haircut-A","market_value":"market_value-A","max_withdrawal_balance":"max_withdrawal_balance-A","hourly_interest_rate":"hourly_interest_rate-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","hourly_interest_rate","max_withdrawal_balance","market_value","haircut","collateral_eligible","collateral_amount","reserved_qty","quantity","instrument_name"]}},{"input":"{\"instrument_name\":\"instrument_name-B\",\"quantity\":\"quantity-B\",\"reserved_qty\":\"reserved_qty-B\",\"collateral_amount\":\"collateral_amount-B\",\"collateral_eligible\":true,\"haircut\":\"haircut-B\",\"market_value\":\"market_value-B\",\"max_withdrawal_balance\":\"max_withdrawal_balance-B\",\"hourly_interest_rate\":\"hourly_interest_rate-B\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"instrument_name":"instrument_name-B","quantity":"quantity-B","reserved_qty":"reserved_qty-B","collateral_amount":"collateral_amount-B","collateral_eligible":true,"haircut":"haircut-B","market_value":"market_value-B","max_withdrawal_balance":"max_withdrawal_balance-B","hourly_interest_rate":"hourly_interest_rate-B","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["instrument_name","quantity","reserved_qty","collateral_amount","collateral_eligible","haircut","market_value","max_withdrawal_balance","hourly_interest_rate","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[\"instrument_name-A\",\"quantity-A\",\"reserved_qty-A\",\"collateral_amount-A\",false,\"haircut-A\",\"market_value-A\",\"max_withdrawal_balance-A\",\"hourly_interest_rate-A\"]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{\"quantity\": \"quantity-A\", \"reserved_qty\": \"reserved_qty-A\", \"collateral_amount\": \"collateral_amount-A\", \"collateral_eligible\": false, \"haircut\": \"haircut-A\", \"market_value\": \"market_value-A\", \"max_withdrawal_balance\": \"max_withdrawal_balance-A\", \"hourly_interest_rate\": \"hourly_interest_rate-A\"}","output":{"ok":false}},{"input":"{\"instrument_name\": null, \"quantity\": \"quantity-A\", \"reserved_qty\": \"reserved_qty-A\", \"collateral_amount\": \"collateral_amount-A\", \"collateral_eligible\": false, \"haircut\": \"haircut-A\", \"market_value\": \"market_value-A\", \"max_withdrawal_balance\": \"max_withdrawal_balance-A\", \"hourly_interest_rate\": \"hourly_interest_rate-A\"}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomPositionBalance>(raw)
            .into_iter()
            .enumerate()
        {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_positionbalance(&value),
                        case["output"],
                        "CryptocomPositionBalance fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_positionbalance(&copy),
                        observe_positionbalance(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomPositionBalance rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_positionbalance() {
    let raw = r###"["instrument_name-A","quantity-A","reserved_qty-A","collateral_amount-A",false,"haircut-A","market_value-A","max_withdrawal_balance-A","hourly_interest_rate-A"]"###;
    for (path, result) in paths::<CryptocomPositionBalance>(raw)
        .into_iter()
        .enumerate()
    {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomPositionBalance path {path}"
        );
    }
}
fn observe_error(v: &CryptocomError) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert(
        "code_bits".into(),
        json!(format!("{:016x}", v.code.to_bits())),
    );
    out.insert("message".into(), json!(v.message));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_error() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"code\":-0.0,\"message\":\"message-A\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"code_bits":"8000000000000000","message":"message-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["code","message","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"message\":\"message-A\",\"code\":-0.0}","output":{"ok":true,"code_bits":"8000000000000000","message":"message-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","message","code"]}},{"input":"{\"code\":17.5,\"message\":\"message-B\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"code_bits":"4031800000000000","message":"message-B","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["code","message","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[-0.0,\"message-A\"]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{\"message\": \"message-A\"}","output":{"ok":false}},{"input":"{\"code\": null, \"message\": \"message-A\"}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomError>(raw).into_iter().enumerate() {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_error(&value),
                        case["output"],
                        "CryptocomError fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_error(&copy),
                        observe_error(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomError rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_error() {
    let raw = r###"[-0.0,"message-A"]"###;
    for (path, result) in paths::<CryptocomError>(raw).into_iter().enumerate() {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomError path {path}"
        );
    }
}
fn observe_accountidentity(v: &CryptocomAccountIdentity) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("ok".into(), json!(true));
    out.insert("uuid".into(), json!(v.uuid));
    out.insert(
        "additional_fields".into(),
        native_json(&v.additional_fields),
    );
    out.insert("own_keys".into(), json!(v.own_keys()));
    Value::Object(out)
}
#[test]
fn scalar_record_accountidentity() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":"{\"uuid\":\"uuid-A\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"uuid":"uuid-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["uuid","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"{\"opaqueArray\":[-0.0,\"opaque\"],\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueZero\":-0.0,\"opaqueNumber\":17.5,\"uuid\":\"uuid-A\"}","output":{"ok":true,"uuid":"uuid-A","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["opaqueArray","opaqueObject","opaqueZero","opaqueNumber","uuid"]}},{"input":"{\"uuid\":\"uuid-B\",\"opaqueNumber\":17.5,\"opaqueZero\":-0.0,\"opaqueObject\":{\"kept\":\"yes\"},\"opaqueArray\":[-0.0,\"opaque\"]}","output":{"ok":true,"uuid":"uuid-B","additional_fields":{"kind":"object","fields":{"opaqueNumber":{"kind":"number","bits":"4031800000000000"},"opaqueZero":{"kind":"number","bits":"8000000000000000"},"opaqueObject":{"kind":"object","fields":{"kept":{"kind":"string","value":"yes"}}},"opaqueArray":{"kind":"array","items":[{"kind":"number","bits":"8000000000000000"},{"kind":"string","value":"opaque"}]}}},"own_keys":["uuid","opaqueNumber","opaqueZero","opaqueObject","opaqueArray"]}},{"input":"[\"uuid-A\"]","output":{"ok":false}},{"input":"[]","output":{"ok":false}},{"input":"null","output":{"ok":false}},{"input":"true","output":{"ok":false}},{"input":"0","output":{"ok":false}},{"input":"\"record\"","output":{"ok":false}},{"input":"{}","output":{"ok":false}},{"input":"{\"uuid\": null}","output":{"ok":false}}]"###).unwrap();
    for case in cases {
        let raw = case["input"].as_str().unwrap();
        for (path, result) in paths::<CryptocomAccountIdentity>(raw)
            .into_iter()
            .enumerate()
        {
            match result {
                Ok(value) => {
                    assert_eq!(
                        observe_accountidentity(&value),
                        case["output"],
                        "CryptocomAccountIdentity fields path {path} input {raw}"
                    );
                    let mut copy = value.clone();
                    assert_eq!(
                        observe_accountidentity(&copy),
                        observe_accountidentity(&value),
                        "native Clone field preservation"
                    );
                    copy.additional_fields["opaqueObject"] = json!({"changed":true});
                    assert_eq!(
                        value.additional_fields["opaqueObject"],
                        json!({"kept":"yes"}),
                        "owned clone extras are separate"
                    );
                }
                Err(_) => assert_eq!(
                    case["output"],
                    json!({"ok":false}),
                    "CryptocomAccountIdentity rejected positive path {path} input {raw}"
                ),
            }
        }
    }
}
#[test]
fn complete_array_accountidentity() {
    let raw = r###"["uuid-A"]"###;
    for (path, result) in paths::<CryptocomAccountIdentity>(raw)
        .into_iter()
        .enumerate()
    {
        assert!(
            result.is_err(),
            "complete positional array accepted for CryptocomAccountIdentity path {path}"
        );
    }
}
