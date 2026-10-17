//! Existing finite carrier decoders, with actual native variant identity.
use safe_cex_venue_cryptocom::ws_payloads::*;
use serde_json::Value;
#[test]
fn carrier_enum_wsvaluationchannel() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","units":[109,97,114,107]},"output":{"ok":true,"variant":"Mark","ordinal":0}},{"input":{"kind":"string","units":[105,110,100,101,120]},"output":{"ok":true,"variant":"Index","ordinal":1}},{"input":{"kind":"string","units":[102,117,110,100,105,110,103]},"output":{"ok":true,"variant":"Funding","ordinal":2}},{"input":{"kind":"string","units":[101,115,116,105,109,97,116,101,100,102,117,110,100,105,110,103]},"output":{"ok":true,"variant":"Estimatedfunding","ordinal":3}},{"input":{"kind":"string","units":[95,95,105,110,118,97,108,105,100,95,95]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[32,109,97,114,107]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[109,97,114,107,32]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[65279,109,97,114,107]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[77,65,82,75]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[56320]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296,120]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"undefined"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"null"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"bool"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"number"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"array"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"object","tag":"mark"},"output":{"ok":false,"error":"string"}}]"###).unwrap();
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::ws_payloads::PayloadValue;
            let input = match value["kind"].as_str().unwrap() {
                "undefined" => PayloadValue::Undefined,
                "null" => PayloadValue::Null,
                "bool" => PayloadValue::Bool(true),
                "number" => PayloadValue::Number(-0.0),
                "string" => PayloadValue::String(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                        .collect(),
                ),
                "array" => PayloadValue::Array(vec![PayloadValue::Null]),
                "object" => PayloadValue::Object(std::collections::BTreeMap::from([(
                    value["tag"].as_str().unwrap().encode_utf16().collect(),
                    PayloadValue::Null,
                )])),
                _ => panic!("unclassified native fixture"),
            };
            let result = CryptocomWsValuationChannel::from_value(&input);
            match result {
                Ok(variant) => {
                    let (variant, ordinal) = match variant {
                        CryptocomWsValuationChannel::Mark => ("Mark", 0u32),
                        CryptocomWsValuationChannel::Index => ("Index", 1u32),
                        CryptocomWsValuationChannel::Funding => ("Funding", 2u32),
                        CryptocomWsValuationChannel::Estimatedfunding => ("Estimatedfunding", 3u32),
                    };
                    serde_json::json!({"ok":true,"variant":variant,"ordinal":ordinal})
                }
                Err(error) => serde_json::json!({"ok":false,"error":error.0}),
            }
        };
        assert_eq!(
            observed, case["output"],
            "native CryptocomWsValuationChannel identity/refusal input {value}"
        );
    }
}
#[test]
fn carrier_enum_wsside() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","units":[66,85,89]},"output":{"ok":true,"variant":"Buy","ordinal":0}},{"input":{"kind":"string","units":[83,69,76,76]},"output":{"ok":true,"variant":"Sell","ordinal":1}},{"input":{"kind":"string","units":[95,95,105,110,118,97,108,105,100,95,95]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[32,66,85,89]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[66,85,89,32]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[65279,66,85,89]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[98,117,121]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[56320]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296,120]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"undefined"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"null"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"bool"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"number"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"array"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"object","tag":"BUY"},"output":{"ok":false,"error":"string"}}]"###).unwrap();
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::ws_payloads::PayloadValue;
            let input = match value["kind"].as_str().unwrap() {
                "undefined" => PayloadValue::Undefined,
                "null" => PayloadValue::Null,
                "bool" => PayloadValue::Bool(true),
                "number" => PayloadValue::Number(-0.0),
                "string" => PayloadValue::String(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                        .collect(),
                ),
                "array" => PayloadValue::Array(vec![PayloadValue::Null]),
                "object" => PayloadValue::Object(std::collections::BTreeMap::from([(
                    value["tag"].as_str().unwrap().encode_utf16().collect(),
                    PayloadValue::Null,
                )])),
                _ => panic!("unclassified native fixture"),
            };
            let result = CryptocomWsSide::from_value(&input);
            match result {
                Ok(variant) => {
                    let (variant, ordinal) = match variant {
                        CryptocomWsSide::Buy => ("Buy", 0u32),
                        CryptocomWsSide::Sell => ("Sell", 1u32),
                    };
                    serde_json::json!({"ok":true,"variant":variant,"ordinal":ordinal})
                }
                Err(error) => serde_json::json!({"ok":false,"error":error.0}),
            }
        };
        assert_eq!(
            observed, case["output"],
            "native CryptocomWsSide identity/refusal input {value}"
        );
    }
}
#[test]
fn carrier_enum_wsrefpricetype() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","units":[77,65,82,75,95,80,82,73,67,69]},"output":{"ok":true,"variant":"MarkPrice","ordinal":0}},{"input":{"kind":"string","units":[73,78,68,69,88,95,80,82,73,67,69]},"output":{"ok":true,"variant":"IndexPrice","ordinal":1}},{"input":{"kind":"string","units":[76,65,83,84,95,80,82,73,67,69]},"output":{"ok":true,"variant":"LastPrice","ordinal":2}},{"input":{"kind":"string","units":[95,95,105,110,118,97,108,105,100,95,95]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[32,77,65,82,75,95,80,82,73,67,69]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[77,65,82,75,95,80,82,73,67,69,32]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[65279,77,65,82,75,95,80,82,73,67,69]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[109,97,114,107,95,112,114,105,99,101]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[56320]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296,120]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"undefined"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"null"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"bool"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"number"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"array"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"object","tag":"MARK_PRICE"},"output":{"ok":false,"error":"string"}}]"###).unwrap();
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::ws_payloads::PayloadValue;
            let input = match value["kind"].as_str().unwrap() {
                "undefined" => PayloadValue::Undefined,
                "null" => PayloadValue::Null,
                "bool" => PayloadValue::Bool(true),
                "number" => PayloadValue::Number(-0.0),
                "string" => PayloadValue::String(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                        .collect(),
                ),
                "array" => PayloadValue::Array(vec![PayloadValue::Null]),
                "object" => PayloadValue::Object(std::collections::BTreeMap::from([(
                    value["tag"].as_str().unwrap().encode_utf16().collect(),
                    PayloadValue::Null,
                )])),
                _ => panic!("unclassified native fixture"),
            };
            let result = CryptocomWsRefPriceType::from_value(&input);
            match result {
                Ok(variant) => {
                    let (variant, ordinal) = match variant {
                        CryptocomWsRefPriceType::MarkPrice => ("MarkPrice", 0u32),
                        CryptocomWsRefPriceType::IndexPrice => ("IndexPrice", 1u32),
                        CryptocomWsRefPriceType::LastPrice => ("LastPrice", 2u32),
                    };
                    serde_json::json!({"ok":true,"variant":variant,"ordinal":ordinal})
                }
                Err(error) => serde_json::json!({"ok":false,"error":error.0}),
            }
        };
        assert_eq!(
            observed, case["output"],
            "native CryptocomWsRefPriceType identity/refusal input {value}"
        );
    }
}
#[test]
fn carrier_enum_wspositiontype() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","units":[80,69,82,80,69,84,85,65,76,95,83,87,65,80]},"output":{"ok":true,"variant":"PerpetualSwap","ordinal":0}},{"input":{"kind":"string","units":[76,79,78,71,95,80,79,83,73,84,73,79,78]},"output":{"ok":true,"variant":"LongPosition","ordinal":1}},{"input":{"kind":"string","units":[83,72,79,82,84,95,80,79,83,73,84,73,79,78]},"output":{"ok":true,"variant":"ShortPosition","ordinal":2}},{"input":{"kind":"string","units":[95,95,105,110,118,97,108,105,100,95,95]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[32,80,69,82,80,69,84,85,65,76,95,83,87,65,80]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[80,69,82,80,69,84,85,65,76,95,83,87,65,80,32]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[65279,80,69,82,80,69,84,85,65,76,95,83,87,65,80]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[112,101,114,112,101,116,117,97,108,95,115,119,97,112]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[56320]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296,120]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"undefined"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"null"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"bool"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"number"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"array"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"object","tag":"PERPETUAL_SWAP"},"output":{"ok":false,"error":"string"}}]"###).unwrap();
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::ws_payloads::PayloadValue;
            let input = match value["kind"].as_str().unwrap() {
                "undefined" => PayloadValue::Undefined,
                "null" => PayloadValue::Null,
                "bool" => PayloadValue::Bool(true),
                "number" => PayloadValue::Number(-0.0),
                "string" => PayloadValue::String(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                        .collect(),
                ),
                "array" => PayloadValue::Array(vec![PayloadValue::Null]),
                "object" => PayloadValue::Object(std::collections::BTreeMap::from([(
                    value["tag"].as_str().unwrap().encode_utf16().collect(),
                    PayloadValue::Null,
                )])),
                _ => panic!("unclassified native fixture"),
            };
            let result = CryptocomWsPositionType::from_value(&input);
            match result {
                Ok(variant) => {
                    let (variant, ordinal) = match variant {
                        CryptocomWsPositionType::PerpetualSwap => ("PerpetualSwap", 0u32),
                        CryptocomWsPositionType::LongPosition => ("LongPosition", 1u32),
                        CryptocomWsPositionType::ShortPosition => ("ShortPosition", 2u32),
                    };
                    serde_json::json!({"ok":true,"variant":variant,"ordinal":ordinal})
                }
                Err(error) => serde_json::json!({"ok":false,"error":error.0}),
            }
        };
        assert_eq!(
            observed, case["output"],
            "native CryptocomWsPositionType identity/refusal input {value}"
        );
    }
}
#[test]
fn carrier_enum_wsisolationtype() {
    let cases:Vec<Value>=serde_json::from_str(r###"[{"input":{"kind":"string","units":[73,83,79,76,65,84,69,68,95,77,65,82,71,73,78]},"output":{"ok":true,"variant":"IsolatedMargin","ordinal":0}},{"input":{"kind":"string","units":[95,95,105,110,118,97,108,105,100,95,95]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[32,73,83,79,76,65,84,69,68,95,77,65,82,71,73,78]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[73,83,79,76,65,84,69,68,95,77,65,82,71,73,78,32]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[65279,73,83,79,76,65,84,69,68,95,77,65,82,71,73,78]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[105,115,111,108,97,116,101,100,95,109,97,114,103,105,110]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[56320]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"string","units":[55296,120]},"output":{"ok":false,"error":"string literal"}},{"input":{"kind":"undefined"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"null"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"bool"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"number"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"array"},"output":{"ok":false,"error":"string"}},{"input":{"kind":"object","tag":"ISOLATED_MARGIN"},"output":{"ok":false,"error":"string"}}]"###).unwrap();
    for case in cases {
        let value = &case["input"];
        let observed = {
            use safe_cex_venue_cryptocom::ws_payloads::PayloadValue;
            let input = match value["kind"].as_str().unwrap() {
                "undefined" => PayloadValue::Undefined,
                "null" => PayloadValue::Null,
                "bool" => PayloadValue::Bool(true),
                "number" => PayloadValue::Number(-0.0),
                "string" => PayloadValue::String(
                    value["units"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
                        .collect(),
                ),
                "array" => PayloadValue::Array(vec![PayloadValue::Null]),
                "object" => PayloadValue::Object(std::collections::BTreeMap::from([(
                    value["tag"].as_str().unwrap().encode_utf16().collect(),
                    PayloadValue::Null,
                )])),
                _ => panic!("unclassified native fixture"),
            };
            let result = CryptocomWsIsolationType::from_value(&input);
            match result {
                Ok(variant) => {
                    let (variant, ordinal) = match variant {
                        CryptocomWsIsolationType::IsolatedMargin => ("IsolatedMargin", 0u32),
                    };
                    serde_json::json!({"ok":true,"variant":variant,"ordinal":ordinal})
                }
                Err(error) => serde_json::json!({"ok":false,"error":error.0}),
            }
        };
        assert_eq!(
            observed, case["output"],
            "native CryptocomWsIsolationType identity/refusal input {value}"
        );
    }
}
