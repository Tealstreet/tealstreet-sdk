use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_venue_bitunix::wire::*;
use serde_json::{Value, json};
use std::process::Command;
fn number(value: f64) -> Value {
    json!({"type":"number", "bits":format!("{:016x}", if value.is_nan() { 0x7ff8000000000000 } else { value.to_bits() })})
}
fn string(value: &str) -> Value {
    json!({"type":"string", "utf16":value.encode_utf16().collect::<Vec<_>>()})
}
fn pendingpositionside(value: PendingPositionSide) -> &'static str {
    match value {
        PendingPositionSide::Buy => "BUY",
        PendingPositionSide::Sell => "SELL",
        PendingPositionSide::Long => "LONG",
        PendingPositionSide::Short => "SHORT",
    }
}
fn pendingmarginmode(value: PendingMarginMode) -> &'static str {
    match value {
        PendingMarginMode::Isolation => "ISOLATION",
        PendingMarginMode::Cross => "CROSS",
    }
}
fn pendingpositionmode(value: PendingPositionMode) -> &'static str {
    match value {
        PendingPositionMode::OneWay => "ONE_WAY",
        PendingPositionMode::Hedged => "HEDGED",
        PendingPositionMode::Hedge => "HEDGE",
    }
}
fn klineinterval(value: KlineInterval) -> &'static str {
    match value {
        KlineInterval::Value0 => "1min",
        KlineInterval::MarkKline1min => "mark_kline_1min",
        KlineInterval::Value2 => "3min",
        KlineInterval::MarkKline3min => "mark_kline_3min",
        KlineInterval::Value4 => "5min",
        KlineInterval::MarkKline5min => "mark_kline_5min",
        KlineInterval::Value6 => "15min",
        KlineInterval::MarkKline15min => "mark_kline_15min",
        KlineInterval::Value8 => "30min",
        KlineInterval::MarkKline30min => "mark_kline_30min",
        KlineInterval::Value10 => "60min",
        KlineInterval::MarkKline60min => "mark_kline_60min",
        KlineInterval::H2 => "2h",
        KlineInterval::MarkKline2h => "mark_kline_2h",
        KlineInterval::H4 => "4h",
        KlineInterval::MarkKline4h => "mark_kline_4h",
        KlineInterval::H6 => "6h",
        KlineInterval::MarkKline6h => "mark_kline_6h",
        KlineInterval::H8 => "8h",
        KlineInterval::MarkKline8h => "mark_kline_8h",
        KlineInterval::H12 => "12h",
        KlineInterval::MarkKline12h => "mark_kline_12h",
        KlineInterval::Value22 => "1day",
        KlineInterval::MarkKline1day => "mark_kline_1day",
        KlineInterval::Value24 => "3day",
        KlineInterval::MarkKline3day => "mark_kline_3day",
        KlineInterval::Value26 => "1week",
        KlineInterval::MarkKline1week => "mark_kline_1week",
        KlineInterval::Value28 => "1month",
        KlineInterval::MarkKline1month => "mark_kline_1month",
    }
}
fn native(row: &PendingPosition) -> Value {
    let mut out = serde_json::Map::new();
    out.insert("symbol".into(), string(&row.symbol));
    out.insert("positionId".into(), string(&row.position_id));
    out.insert("qty".into(), string(&row.qty));
    if let Some(value) = &row.max_qty {
        out.insert("maxQty".into(), string(value));
    }
    out.insert("entryValue".into(), string(&row.entry_value));
    if let Some(value) = &row.entry_price {
        out.insert("entryPrice".into(), string(value));
    }
    if let Some(value) = &row.avg_open_price {
        out.insert("avgOpenPrice".into(), string(value));
    }
    out.insert("side".into(), string(pendingpositionside(row.side)));
    out.insert(
        "marginMode".into(),
        string(pendingmarginmode(row.margin_mode)),
    );
    out.insert(
        "positionMode".into(),
        string(pendingpositionmode(row.position_mode)),
    );
    out.insert("leverage".into(), number(row.leverage));
    out.insert("fee".into(), string(&row.fee));
    out.insert("funding".into(), string(&row.funding));
    out.insert("realizedPNL".into(), string(&row.realized_pnl));
    out.insert("margin".into(), string(&row.margin));
    out.insert("unrealizedPNL".into(), string(&row.unrealized_pnl));
    out.insert("liqPrice".into(), string(&row.liq_price));
    if let Some(value) = &row.margin_rate {
        out.insert("marginRate".into(), string(value));
    }
    out.insert("ctime".into(), number(row.ctime));
    out.insert("mtime".into(), number(row.mtime));
    Value::Object(out)
}

#[test]
fn actual_typescript_wire_declarations_and_numeric_bits() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/wire-cardinality-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "wire source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    // SOURCE_CARDINALITY_START
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        103,
        "wire source case count"
    );
    assert_eq!(
        corpus["intervals"].as_array().unwrap().len(),
        30,
        "wire source interval count"
    );
    assert_eq!(
        corpus["fields"].as_array().unwrap().len(),
        20,
        "wire source field count"
    );
    assert_eq!(
        corpus["supplement"].as_array().unwrap().len(),
        2,
        "wire supplement count"
    );
    // SOURCE_CARDINALITY_END
    let mut source_rows = 0;
    let mut compared_rows = 0;
    let mut route_checks = 0;
    let mut surrogate_cases = 0;
    for case in corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .chain(corpus["supplement"].as_array().unwrap())
    {
        let raw = case["raw"].as_str().unwrap();
        if case["sourceAccepted"] == true {
            surrogate_cases += 1;
            assert_eq!(
                case["native"][0]["symbol"]["utf16"],
                json!([case["surrogate"]]),
                "actual TS lone surrogate capture"
            );
            typed_utf16::assert_projection(
                raw,
                &(js_from_str::<GetPendingPositionsResponse>(raw))
                    .expect("typed surrogate record is retained"),
            );
            typed_utf16::assert_projection(
                raw,
                &(serde_json::from_str::<GetPendingPositionsResponse>(raw))
                    .expect("typed surrogate record is retained"),
            );
            continue;
        }
        let value: Value = serde_json::from_str(raw).unwrap();
        if case["accepted"] == true {
            let raw_rows = value.as_array().unwrap().len();
            source_rows += raw_rows;
            // SOURCE_OUTPUT_CARDINALITY_START
            assert_eq!(
                case["native"].as_array().unwrap().len(),
                raw_rows,
                "wire source per-case output count {}",
                case["label"]
            );
            // SOURCE_OUTPUT_CARDINALITY_END
        }
        let routes = [
            ("raw", js_from_str::<GetPendingPositionsResponse>(raw)),
            (
                "owned",
                js_from_value::<GetPendingPositionsResponse>(value.clone()),
            ),
            (
                "borrowed",
                js_from_value_ref::<GetPendingPositionsResponse>(&value),
            ),
            (
                "serde raw",
                serde_json::from_str::<GetPendingPositionsResponse>(raw),
            ),
        ];
        for (route, result) in routes {
            route_checks += 1;
            assert_eq!(
                result.is_ok(),
                case["accepted"].as_bool().unwrap(),
                "position shape {} {route}: {result:?}",
                case["label"]
            );
            if let Ok(rows) = result {
                assert_eq!(
                    rows.len(),
                    case["native"].as_array().unwrap().len(),
                    "wire native response count {} {route}",
                    case["label"]
                );
                compared_rows += rows.len();
                assert_eq!(
                    json!(rows.iter().map(native).collect::<Vec<_>>()),
                    case["native"],
                    "position decoded fields {} {route}",
                    case["label"]
                );
            }
        }
    }
    assert_eq!(source_rows, 40, "wire source total rows");
    assert_eq!(compared_rows, 160, "wire actual native row comparisons");
    assert_eq!(route_checks, 412, "wire route comparison count");
    assert_eq!(surrogate_cases, 2, "wire declared surrogate refusals");
    let mut interval_decodes = 0;
    let mut interval_serializations = 0;
    for literal in corpus["intervals"].as_array().unwrap() {
        let result: KlineInterval = js_from_str(&literal.to_string()).unwrap();
        let direct: KlineInterval = serde_json::from_str(&literal.to_string()).unwrap();
        interval_decodes += 2;
        assert_eq!(result, direct, "interval direct decoded variant");
        assert_eq!(
            klineinterval(result),
            literal.as_str().unwrap(),
            "interval decoded variant {literal}"
        );
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            *literal,
            "interval serialization {literal}"
        );
        interval_serializations += 1;
    }
    assert_eq!(interval_decodes, 60, "wire native interval decode count");
    assert_eq!(
        interval_serializations, 30,
        "wire native interval serialization count"
    );
    assert!(
        serde_json::from_str::<PendingPositionSide>(r#"{"BUY":null}"#).is_err(),
        "position-side direct string shape"
    );
    assert!(
        serde_json::from_str::<PendingMarginMode>(r#"{"CROSS":null}"#).is_err(),
        "margin-mode direct string shape"
    );
    assert!(
        serde_json::from_str::<PendingPositionMode>(r#"{"HEDGE":null}"#).is_err(),
        "position-mode direct string shape"
    );
    let mut invalid_interval_checks = 0;
    for invalid in [
        "null",
        "1",
        "[]",
        "{}",
        "\"1m\"",
        "\"unknown\"",
        "{\"1min\":null}",
    ] {
        assert!(
            serde_json::from_str::<KlineInterval>(invalid).is_err(),
            "interval direct string shape {invalid}"
        );
        assert!(
            js_from_str::<KlineInterval>(invalid).is_err(),
            "interval refusal {invalid}"
        );
        invalid_interval_checks += 2;
    }
    assert_eq!(
        invalid_interval_checks, 14,
        "wire fixed invalid interval comparison count"
    );
    println!(
        "wire corpus: {} position cases, {} interval literals",
        corpus["cases"].as_array().unwrap().len(),
        corpus["intervals"].as_array().unwrap().len()
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
