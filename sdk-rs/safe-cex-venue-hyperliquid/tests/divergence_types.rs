use safe_cex_venue_hyperliquid::types;

#[test]
fn audit_duplicate_root_02() {
    let value: types::ClearinghouseState = serde_json::from_str(r####"{"assetPositions":[{"position":{"coin":"\ud800","coin":"雪😀﻿","cumFunding":{"allTime":"wire-雪\u0000","sinceChange":"wire-雪\u0000","sinceOpen":"wire-雪\u0000"},"entryPx":"wire-雪\u0000","leverage":{"rawUsd":"wire-雪\u0000","type":"wire-雪\u0000","value":17},"liquidationPx":"wire-雪\u0000","marginUsed":"wire-雪\u0000","maxLeverage":17,"positionValue":"wire-雪\u0000","returnOnEquity":"wire-雪\u0000","szi":"wire-雪\u0000","unrealizedPnl":"wire-雪\u0000"},"type":"wire-雪\u0000"}],"crossMaintenanceMarginUsed":"wire-雪\u0000","crossMarginSummary":{"accountValue":"wire-雪\u0000","totalMarginUsed":"wire-雪\u0000","totalNtlPos":"wire-雪\u0000","totalRawUsd":"wire-雪\u0000"},"marginSummary":{"accountValue":"wire-雪\u0000","totalMarginUsed":"wire-雪\u0000","totalNtlPos":"wire-雪\u0000","totalRawUsd":"wire-雪\u0000"},"time":17,"withdrawable":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode ClearinghouseState");
    let selected = value.asset_positions[0].position.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten ClearinghouseState /assetPositions/0/position/coin"
    );
}

#[test]
fn audit_duplicate_root_03() {
    let value: types::UserFills = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode UserFills");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserFills /coin"
    );
}

#[test]
fn audit_duplicate_root_06() {
    let value: types::WsTrade = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","side":"wire-雪\u0000","px":"wire-雪\u0000","sz":"wire-雪\u0000","hash":"wire-雪\u0000","time":17,"tid":17}"####)
            .expect("HL TYPES public duplicate decode WsTrade");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsTrade /coin"
    );
}

#[test]
fn audit_duplicate_root_07() {
    let value: types::WsBook = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","levels":[[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}],[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}]],"time":17}"####)
            .expect("HL TYPES public duplicate decode WsBook");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsBook /coin"
    );
}

#[test]
fn audit_duplicate_root_08() {
    let value: types::WsBbo = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","bbo":[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17},{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}],"time":17}"####)
            .expect("HL TYPES public duplicate decode WsBbo");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsBbo /coin"
    );
}

#[test]
fn audit_duplicate_root_09() {
    let value: types::WsLevel =
        serde_json::from_str(r####"{"px":"\ud800","px":"雪😀﻿","sz":"wire-雪\u0000","n":17}"####)
            .expect("HL TYPES public duplicate decode WsLevel");
    let selected = value.px.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsLevel /px"
    );
}

#[test]
fn audit_duplicate_root_10() {
    let value: types::WsOrder = serde_json::from_str(r####"{"order":{"coin":"\ud800","coin":"雪😀﻿","side":"wire-雪\u0000","limitPx":"wire-雪\u0000","sz":"wire-雪\u0000","oid":17,"timestamp":17,"origSz":"wire-雪\u0000"},"status":"wire-雪\u0000","statusTimestamp":17,"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsOrder");
    let selected = value.order.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsOrder /order/coin"
    );
}

#[test]
fn audit_duplicate_root_11() {
    let value: types::WsFill = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode WsFill");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsFill /coin"
    );
}

#[test]
fn audit_duplicate_root_12() {
    let value: types::WsUserFunding = serde_json::from_str(r####"{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","szi":"wire-雪\u0000","fundingRate":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFunding");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFunding /coin"
    );
}

#[test]
fn audit_duplicate_root_13() {
    let value: types::WsLiquidation = serde_json::from_str(r####"{"lid":17,"liquidator":"\ud800","liquidator":"雪😀﻿","liquidated_user":"wire-雪\u0000","liquidated_ntl_pos":"wire-雪\u0000","liquidated_account_value":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsLiquidation");
    let selected = value.liquidator.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsLiquidation /liquidator"
    );
}

#[test]
fn audit_duplicate_root_14() {
    let value: types::WsNonUserCancel =
        serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","oid":17}"####)
            .expect("HL TYPES public duplicate decode WsNonUserCancel");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsNonUserCancel /coin"
    );
}

#[test]
fn audit_duplicate_root_18() {
    let value: types::L2Book = serde_json::from_str(r####"{"levels":[[{"px":"\ud800","px":"雪😀﻿","sz":"wire-雪\u0000","n":17}],[{"px":"wire-雪\u0000","sz":"wire-雪\u0000","n":17}]]}"####)
            .expect("HL TYPES public duplicate decode L2Book");
    let selected = value.levels.0[0].px.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten L2Book /levels/0/0/px"
    );
}

#[test]
fn audit_duplicate_root_26() {
    let value: types::FundingHistoryEntry = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","premium":"wire-雪\u0000","time":17}"####)
            .expect("HL TYPES public duplicate decode FundingHistoryEntry");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten FundingHistoryEntry /coin"
    );
}

#[test]
fn audit_duplicate_root_27() {
    let value: types::FundingHistory = serde_json::from_str(r####"[{"coin":"\ud800","coin":"雪😀﻿","fundingRate":"wire-雪\u0000","premium":"wire-雪\u0000","time":17}]"####)
            .expect("HL TYPES public duplicate decode FundingHistory");
    let selected = value[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten FundingHistory /0/coin"
    );
}

#[test]
fn audit_duplicate_root_33() {
    let value: types::UserOpenOrder = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","limitPx":"wire-雪\u0000","oid":17,"side":"wire-雪\u0000","sz":"wire-雪\u0000","timestamp":17}"####)
            .expect("HL TYPES public duplicate decode UserOpenOrder");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserOpenOrder /coin"
    );
}

#[test]
fn audit_duplicate_root_34() {
    let value: types::UserOpenOrders = serde_json::from_str(r####"[{"coin":"\ud800","coin":"雪😀﻿","limitPx":"wire-雪\u0000","oid":17,"side":"wire-雪\u0000","sz":"wire-雪\u0000","timestamp":17}]"####)
            .expect("HL TYPES public duplicate decode UserOpenOrders");
    let selected = value[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten UserOpenOrders /0/coin"
    );
}

#[test]
fn audit_duplicate_root_37() {
    let value: types::CancelOrderRequest =
        serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","o":17}"####)
            .expect("HL TYPES public duplicate decode CancelOrderRequest");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten CancelOrderRequest /coin"
    );
}

#[test]
fn audit_duplicate_root_40() {
    let value: types::Signature =
        serde_json::from_str(r####"{"r":"\ud800","r":"雪😀﻿","s":"wire-雪\u0000","v":17}"####)
            .expect("HL TYPES public duplicate decode Signature");
    let selected = value.r.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Signature /r"
    );
}

#[test]
fn audit_duplicate_root_41() {
    let value: types::Candle = serde_json::from_str(r####"{"t":17,"T":17,"s":"\ud800","s":"雪😀﻿","i":"wire-雪\u0000","o":"wire-雪\u0000","c":"wire-雪\u0000","h":"wire-雪\u0000","l":"wire-雪\u0000","v":"wire-雪\u0000","n":17,"coin":"wire-雪\u0000","interval":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode Candle");
    let selected = value.s.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten Candle /s"
    );
}

#[test]
fn audit_duplicate_root_42() {
    let value: types::HyperliquidCandle = serde_json::from_str(r####"{"t":17,"T":17,"s":"\ud800","s":"雪😀﻿","i":"wire-雪\u0000","o":"wire-雪\u0000","c":"wire-雪\u0000","h":"wire-雪\u0000","l":"wire-雪\u0000","v":"wire-雪\u0000","n":17,"coin":"wire-雪\u0000","interval":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode HyperliquidCandle");
    let selected = value.s.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten HyperliquidCandle /s"
    );
}

#[test]
fn audit_duplicate_root_43() {
    let value: types::WsUserFill = serde_json::from_str(r####"{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}"####)
            .expect("HL TYPES public duplicate decode WsUserFill");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFill /coin"
    );
}

#[test]
fn audit_duplicate_root_44() {
    let value: types::WsUserFills = serde_json::from_str(r####"{"isSnapshot":true,"fills":[{"coin":"\ud800","coin":"雪😀﻿","px":"wire-雪\u0000","sz":"wire-雪\u0000","side":"wire-雪\u0000","time":17,"startPosition":"wire-雪\u0000","dir":"wire-雪\u0000","closedPnl":"wire-雪\u0000","hash":"wire-雪\u0000","oid":17,"crossed":true,"fee":"wire-雪\u0000","tid":17}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFills");
    let selected = value.fills[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFills /fills/0/coin"
    );
}

#[test]
fn audit_duplicate_root_45() {
    let value: types::WsUserFundings = serde_json::from_str(r####"{"isSnapshot":true,"fundings":[{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","szi":"wire-雪\u0000","fundingRate":"wire-雪\u0000"}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserFundings");
    let selected = value.fundings[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserFundings /fundings/0/coin"
    );
}

#[test]
fn audit_duplicate_root_46() {
    let value: types::WsUserNonFundingLedgerUpdate = serde_json::from_str(r####"{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","type":"deposit"}"####)
            .expect("HL TYPES public duplicate decode WsUserNonFundingLedgerUpdate");
    let selected = value.coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserNonFundingLedgerUpdate /coin"
    );
}

#[test]
fn audit_duplicate_root_47() {
    let value: types::WsUserNonFundingLedgerUpdates = serde_json::from_str(r####"{"isSnapshot":true,"updates":[{"time":17,"coin":"\ud800","coin":"雪😀﻿","usdc":"wire-雪\u0000","type":"deposit"}],"user":"wire-雪\u0000"}"####)
            .expect("HL TYPES public duplicate decode WsUserNonFundingLedgerUpdates");
    let selected = value.updates[0].coin.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten WsUserNonFundingLedgerUpdates /updates/0/coin"
    );
}

#[test]
fn audit_duplicate_root_49() {
    let value: types::BuilderInfo =
        serde_json::from_str(r####"{"b":"\ud800","b":"雪😀﻿","f":17}"####)
            .expect("HL TYPES public duplicate decode BuilderInfo");
    let selected = value.b.as_str();
    assert_eq!(
        selected, "雪😀\u{feff}",
        "HL TYPES native overwritten BuilderInfo /b"
    );
}

#[test]
fn audit_duplicate_selection_retains_last_value_validation_and_key_position() {
    use safe_cex_types::retained_json::RetainedJsonInput;
    let raw = r#"{"z":"\ud800","a":1,"z":"kept"}"#;
    let value = serde_json::from_str::<RetainedJsonInput>(raw)
        .unwrap()
        .into_value()
        .unwrap();
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        ["z", "a"],
        "AUDIT native first insertion order survives last replacement"
    );
    assert_eq!(value["z"].as_str(), Some("kept"));
    for raw in [
        r#"{"z":"kept","z":"\ud800"}"#,
        r#"{"z":"kept","z":"\udc00"}"#,
    ] {
        // JSON.parse retains lone code units. Only the later UTF-8 Value
        // projection refuses them; the canonical retained input stays lossless.
        let source = std::process::Command::new("node")
            .args([
                "-e",
                "const s=JSON.parse(process.argv[1]).z; process.stdout.write(JSON.stringify(Array.from({length:s.length},(_,i)=>s.charCodeAt(i))));",
                raw,
            ])
            .output()
            .expect("Node JSON.parse duplicate-selection oracle");
        assert!(
            source.status.success(),
            "{}",
            String::from_utf8_lossy(&source.stderr)
        );
        let expected: Vec<u16> = serde_json::from_slice(&source.stdout).unwrap();
        let retained = serde_json::from_str::<RetainedJsonInput>(raw)
            .expect("JSON.parse admits surviving lone surrogate code units");
        let RetainedJsonInput::Object(entries) = &retained else {
            panic!("retained duplicate object");
        };
        assert_eq!(
            entries.len(),
            1,
            "last duplicate replaces its earlier value"
        );
        assert_eq!(entries[0].0, [u16::from(b'z')]);
        let RetainedJsonInput::Text(units) = &entries[0].1 else {
            panic!("surviving duplicate string remains native text");
        };
        assert_eq!(
            units, &expected,
            "retained native code units match JSON.parse"
        );
        assert!(
            retained.into_value().is_err(),
            "UTF-8 Value projection must refuse a surviving lone surrogate"
        );
    }
    assert!(
        serde_json::from_str::<RetainedJsonInput>(r#"{"z":invalid,"z":"kept"}"#).is_err(),
        "AUDIT invalid syntax is refused even before an overwritten value"
    );
    let overwritten = format!(
        "{{\"z\":{}0{},\"z\":\"kept\"}}",
        "[".repeat(140),
        "]".repeat(140)
    );
    let value = serde_json::from_str::<RetainedJsonInput>(&overwritten)
        .unwrap()
        .into_value()
        .unwrap();
    assert_eq!(value["z"].as_str(), Some("kept"));
    let surviving = format!(
        "{{\"z\":\"kept\",\"z\":{}0{}}}",
        "[".repeat(140),
        "]".repeat(140)
    );
    assert!(
        serde_json::from_str::<RetainedJsonInput>(&surviving).is_err(),
        "AUDIT surviving depth budget remains enforced"
    );
}

#[test]
fn audit_any_indexed_roots_preserve_utf16_and_array_native_alternatives() {
    use safe_cex_venue_hyperliquid::json_values::{JsonContainer, JsonValue};
    let raw = r#"{"bad":"\ud800","literal":{"$serde_json::private::Number":"120"}}"#;
    for value in [
        serde_json::from_str::<types::UserRateLimit>(raw).unwrap(),
        serde_json::from_str::<types::OrderStatus>(raw).unwrap(),
    ] {
        let JsonContainer::Object(object) = value else {
            panic!("AUDIT native any-indexed object");
        };
        let Some(JsonValue::String(text)) = object.get(&"bad".encode_utf16().collect::<Vec<_>>())
        else {
            panic!("AUDIT native any-indexed retained string");
        };
        assert_eq!(text.0, [0xd800], "AUDIT native any-indexed lone unit");
        let Some(JsonValue::Object(marker)) =
            object.get(&"literal".encode_utf16().collect::<Vec<_>>())
        else {
            panic!("AUDIT native any-indexed marker stays object");
        };
        let Some(JsonValue::String(text)) = marker.get(
            &"$serde_json::private::Number"
                .encode_utf16()
                .collect::<Vec<_>>(),
        ) else {
            panic!("AUDIT native any-indexed own marker field");
        };
        assert_eq!(text.0, "120".encode_utf16().collect::<Vec<_>>());
    }
    for value in [
        serde_json::from_str::<types::UserRateLimit>(r#"["\udfff"]"#).unwrap(),
        serde_json::from_str::<types::OrderStatus>(r#"["\udfff"]"#).unwrap(),
    ] {
        let JsonContainer::Array(values) = value else {
            panic!("AUDIT native any-indexed array alternative");
        };
        assert_eq!(values.len(), 1);
        let JsonValue::String(text) = &values[0] else {
            panic!("AUDIT native any-indexed array string");
        };
        assert_eq!(text.0, [0xdfff]);
    }
}
