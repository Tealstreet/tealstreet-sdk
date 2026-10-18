use super::{WeexUtilityInput, normalize_balance};
use crate::market_data::{normalize_market, normalize_ticker};
use safe_cex_num::numeric_ingress::js_from_value;
use serde_json::json;

#[path = "source_profile.rs"]
mod source_profile;

const SOURCE: &str = "../parity/evidence/okx-kraken-weex-audit/source-observations.mjs";

#[test]
fn market_info_mutations_keep_both_native_snapshots() {
    let mut input: WeexUtilityInput = js_from_value(json!({
        "symbol":"B/Q", "baseAsset":"B", "quoteAsset":"Q", "extra":{"probe":1}
    }))
    .unwrap();
    let mut native = normalize_market(&input).unwrap();
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("extra")
            .unwrap()
            .get("probe")
            .unwrap()
            .as_f64()
            .unwrap(),
        1.0,
        "WEEX market native initial info"
    );
    input.value["extra"]["probe"] = json!(7);
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("extra")
            .unwrap()
            .get("probe")
            .unwrap()
            .as_f64()
            .unwrap(),
        1.0,
        "WEEX market input mutation keeps snapshot"
    );
    *native
        .info
        .as_mut()
        .unwrap()
        .get_mut("extra")
        .unwrap()
        .get_mut("probe")
        .unwrap() = safe_cex_types::retained_json::RetainedJsonInput::from(json!(11));
    assert_eq!(
        input.value["extra"]["probe"], 7,
        "WEEX market output mutation keeps input"
    );
    for observation in source_profile::observe(
        SOURCE,
        "weex-market-alias",
        "SOURCE-WEEX-MARKET-ALIAS input-to-output",
    ) {
        assert_eq!(observation["inputToOutput"], 7);
        assert_eq!(observation["outputToInput"], 11);
    }
}

#[test]
fn ticker_and_book_mutations_keep_both_native_snapshots() {
    let market = normalize_market(
        &js_from_value(json!({"symbol":"B/Q","baseAsset":"B","quoteAsset":"Q"})).unwrap(),
    )
    .unwrap();
    let mut ticker: WeexUtilityInput =
        js_from_value(json!({"lastPrice":3,"extra":"ordinary"})).unwrap();
    let mut book: WeexUtilityInput = js_from_value(json!({"bidPrice":2})).unwrap();
    let mut native = normalize_ticker(Some(&ticker), &market, Some(&book)).unwrap();
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("ticker")
            .unwrap()
            .get("lastPrice")
            .unwrap()
            .as_f64()
            .unwrap(),
        3.0,
        "WEEX ticker native initial info"
    );
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("bookTicker")
            .unwrap()
            .get("bidPrice")
            .unwrap()
            .as_f64()
            .unwrap(),
        2.0,
        "WEEX book native initial info"
    );
    ticker.value["lastPrice"] = json!(7);
    book.value["bidPrice"] = json!(13);
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("ticker")
            .unwrap()
            .get("lastPrice")
            .unwrap()
            .as_f64()
            .unwrap(),
        3.0,
        "WEEX ticker input mutation keeps snapshot"
    );
    assert_eq!(
        native
            .info
            .as_ref()
            .unwrap()
            .get("bookTicker")
            .unwrap()
            .get("bidPrice")
            .unwrap()
            .as_f64()
            .unwrap(),
        2.0,
        "WEEX book input mutation keeps snapshot"
    );
    *native
        .info
        .as_mut()
        .unwrap()
        .get_mut("ticker")
        .unwrap()
        .get_mut("lastPrice")
        .unwrap() = safe_cex_types::retained_json::RetainedJsonInput::from(json!(11));
    *native
        .info
        .as_mut()
        .unwrap()
        .get_mut("bookTicker")
        .unwrap()
        .get_mut("bidPrice")
        .unwrap() = safe_cex_types::retained_json::RetainedJsonInput::from(json!(17));
    assert_eq!(
        ticker.value["lastPrice"], 7,
        "WEEX ticker output mutation keeps input"
    );
    assert_eq!(
        book.value["bidPrice"], 13,
        "WEEX book output mutation keeps input"
    );
    let unit = r#"{"lastPrice":3,"extra":"\ud800"}"#;
    assert!(
        serde_json::from_str::<WeexUtilityInput>(unit).is_err(),
        "WEEX ticker info unit serde whole input"
    );
    assert!(
        safe_cex_num::numeric_ingress::js_from_str::<WeexUtilityInput>(unit).is_err(),
        "WEEX ticker info unit numeric whole input"
    );
    for observation in source_profile::observe(
        SOURCE,
        "weex-ticker-alias-unit",
        "SOURCE-WEEX-TICKER-ALIAS input-to-output",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
        assert_eq!(observation["directions"], json!([7, 11, 13, 17]));
    }
    for observation in source_profile::observe(
        "tests/record-boundary-oracle.cjs",
        "ticker-input",
        "WEEX SOURCE unused Market ticker id",
    ) {
        let cases = observation["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2, "WEEX unused ticker Market input floor");
        assert_eq!(cases[0]["id"], "unused-base-unit");
        assert_eq!(cases[1]["id"], "unused-info-depth");
        for case in cases {
            assert_eq!(case["last"], 3);
        }
    }
}

#[test]
fn balance_info_mutations_keep_both_native_snapshots() {
    let mut input: WeexUtilityInput =
        js_from_value(json!([{"asset":"USDT","balance":"3","tag":"ordinary"}])).unwrap();
    let mut native = normalize_balance(&input).unwrap();
    assert_eq!(
        native.info.as_ref().unwrap().as_array().unwrap().len(),
        1,
        "WEEX balance native info row floor"
    );
    assert_eq!(
        native.info.as_ref().unwrap().as_array().unwrap()[0]
            .get("balance")
            .unwrap()
            .as_text()
            .unwrap(),
        &"3".encode_utf16().collect::<Vec<_>>(),
        "WEEX balance native initial info"
    );
    input.value[0]["balance"] = json!("7");
    assert_eq!(
        native.info.as_ref().unwrap().as_array().unwrap()[0]
            .get("balance")
            .unwrap()
            .as_text()
            .unwrap(),
        &"3".encode_utf16().collect::<Vec<_>>(),
        "WEEX balance input mutation keeps snapshot"
    );
    *native.info.as_mut().unwrap().as_array_mut().unwrap()[0]
        .get_mut("balance")
        .unwrap() = json!("11").into();
    assert_eq!(
        input.value[0]["balance"], "7",
        "WEEX balance output mutation keeps input"
    );
    let unit = r#"[{"asset":"USDT","balance":"3","tag":"\ud800"}]"#;
    assert!(
        serde_json::from_str::<WeexUtilityInput>(unit).is_err(),
        "WEEX balance info unit serde whole input"
    );
    assert!(
        safe_cex_num::numeric_ingress::js_from_str::<WeexUtilityInput>(unit).is_err(),
        "WEEX balance info unit numeric whole input"
    );
    for observation in source_profile::observe(
        "../parity/evidence/okx-kraken-weex-audit/full-source-observations.mjs",
        "weex-balance-alias-unit",
        "FULL-WEEX-BALANCE-ALIAS input to output",
    ) {
        assert_eq!(observation["units"], json!([0xd800]));
        assert_eq!(observation["directions"], json!([7, 11]));
    }
}
