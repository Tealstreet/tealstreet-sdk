//! Contract vectors from each native adapter audit; no production venue I/O.
use super::*;
use serde_json::json;

const INTENT: &str = "018bcfe5-6800-7101-8101-010101010101";
const HEX: &str = "018bcfe5680071018101010101010101";

#[test]
fn client_id_apexomni() {
    let actual = serde_json::to_value(client_order_id("apexomni", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "apexomni: native uuid contract");
}

#[test]
fn client_id_asterdex() {
    let actual = serde_json::to_value(client_order_id("asterdex", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "asterdex: native none contract");
}

// AGT-50: the hosted intent UUID is not a native caller ID for backpack;
// omit it and retain the router ledger/UUID as the dedupe authority.
#[test]
fn client_id_backpack() {
    let actual = serde_json::to_value(client_order_id("backpack", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "backpack: native none contract");
}

#[test]
fn client_id_binance() {
    let actual = serde_json::to_value(client_order_id("binance", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "binance: native uuid contract");
}

#[test]
fn client_id_binanceinverse() {
    let actual = serde_json::to_value(client_order_id("binanceinverse", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "binanceinverse: native none contract");
}

#[test]
fn client_id_binanceoptions() {
    let actual = serde_json::to_value(client_order_id("binanceoptions", INTENT)).unwrap();
    assert_eq!(
        actual,
        json!(INTENT),
        "binanceoptions: native uuid contract"
    );
}

#[test]
fn client_id_binancepm() {
    let actual = serde_json::to_value(client_order_id("binancepm", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "binancepm: native hex contract");
}

#[test]
fn client_id_binancespot() {
    let actual = serde_json::to_value(client_order_id("binancespot", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "binancespot: native uuid contract");
}

#[test]
fn client_id_bingx() {
    let actual = serde_json::to_value(client_order_id("bingx", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bingx: native uuid contract");
}

#[test]
fn client_id_bitget() {
    let actual = serde_json::to_value(client_order_id("bitget", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "bitget: native hex contract");
}

#[test]
fn client_id_bitgetinverse() {
    let actual = serde_json::to_value(client_order_id("bitgetinverse", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "bitgetinverse: native hex contract");
}

#[test]
fn client_id_bitgetusdc() {
    let actual = serde_json::to_value(client_order_id("bitgetusdc", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "bitgetusdc: native hex contract");
}

#[test]
fn client_id_bitunix() {
    let actual = serde_json::to_value(client_order_id("bitunix", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bitunix: native uuid contract");
}

#[test]
fn client_id_blofin() {
    let actual = serde_json::to_value(client_order_id("blofin", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "blofin: native hex contract");
}

#[test]
fn client_id_bybit() {
    let actual = serde_json::to_value(client_order_id("bybit", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bybit: native uuid contract");
}

#[test]
fn client_id_bybitdemo() {
    let actual = serde_json::to_value(client_order_id("bybitdemo", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bybitdemo: native uuid contract");
}

#[test]
fn client_id_bybitinverse() {
    let actual = serde_json::to_value(client_order_id("bybitinverse", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bybitinverse: native uuid contract");
}

#[test]
fn client_id_bybitspot() {
    let actual = serde_json::to_value(client_order_id("bybitspot", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "bybitspot: native uuid contract");
}

#[test]
fn client_id_coinbase() {
    let actual = serde_json::to_value(client_order_id("coinbase", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "coinbase: native uuid contract");
}

#[test]
fn client_id_coinbasespot() {
    let actual = serde_json::to_value(client_order_id("coinbasespot", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "coinbasespot: native uuid contract");
}

#[test]
fn client_id_cryptocom() {
    let actual = serde_json::to_value(client_order_id("cryptocom", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "cryptocom: native uuid contract");
}

// AGT-50: the hosted intent UUID is not a native caller ID for grvt;
// omit it and retain the router ledger/UUID as the dedupe authority.
#[test]
fn client_id_grvt() {
    let actual = serde_json::to_value(client_order_id("grvt", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "grvt: native none contract");
}

#[test]
fn client_id_hyperliquid() {
    let actual = serde_json::to_value(client_order_id("hyperliquid", INTENT)).unwrap();
    assert_eq!(
        actual,
        json!(format!("0x{HEX}")),
        "hyperliquid: native hl contract"
    );
}

#[test]
fn client_id_hyperliquidspot() {
    let actual = serde_json::to_value(client_order_id("hyperliquidspot", INTENT)).unwrap();
    assert_eq!(
        actual,
        json!(format!("0x{HEX}")),
        "hyperliquidspot: native hl contract"
    );
}

// AGT-50: the hosted intent UUID is not a native caller ID for hyrotrader;
// omit it and retain the router ledger/UUID as the dedupe authority.
#[test]
fn client_id_hyrotrader() {
    let actual = serde_json::to_value(client_order_id("hyrotrader", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "hyrotrader: native none contract");
}

#[test]
fn client_id_kraken() {
    let actual = serde_json::to_value(client_order_id("kraken", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "kraken: native none contract");
}

#[test]
fn client_id_kucoin() {
    let actual = serde_json::to_value(client_order_id("kucoin", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "kucoin: native uuid contract");
}

#[test]
fn client_id_kucoinspot() {
    let actual = serde_json::to_value(client_order_id("kucoinspot", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "kucoinspot: native uuid contract");
}

#[test]
fn client_id_lighter() {
    let actual = serde_json::to_value(client_order_id("lighter", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "lighter: native none contract");
}

#[test]
fn client_id_lighter_robinhood() {
    let actual = serde_json::to_value(client_order_id("lighter-robinhood", INTENT)).unwrap();
    assert_eq!(
        actual,
        Value::Null,
        "lighter-robinhood: native none contract"
    );
}

#[test]
fn client_id_mexc() {
    let actual = serde_json::to_value(client_order_id("mexc", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "mexc: native none contract");
}

#[test]
fn client_id_okx() {
    let actual = serde_json::to_value(client_order_id("okx", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "okx: native hex contract");
}

#[test]
fn client_id_okxdemo() {
    let actual = serde_json::to_value(client_order_id("okxdemo", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "okxdemo: native hex contract");
}

#[test]
fn client_id_okxeu() {
    let actual = serde_json::to_value(client_order_id("okxeu", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "okxeu: native hex contract");
}

#[test]
fn client_id_okxeudemo() {
    let actual = serde_json::to_value(client_order_id("okxeudemo", INTENT)).unwrap();
    assert_eq!(actual, json!(HEX), "okxeudemo: native hex contract");
}

#[test]
fn client_id_paradex() {
    let actual = serde_json::to_value(client_order_id("paradex", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "paradex: native uuid contract");
}

#[test]
fn client_id_phemex() {
    let actual = serde_json::to_value(client_order_id("phemex", INTENT)).unwrap();
    assert_eq!(actual, json!(INTENT), "phemex: native uuid contract");
}

// Was `json!(INTENT)` until the 2026-11-19 woo-t report: WOO refused the UUID
// before the venue, so every DELEGATED plain market `order.submit` failed.
#[test]
fn client_id_woo() {
    let actual = serde_json::to_value(client_order_id("woo", INTENT)).unwrap();
    assert_eq!(actual, Value::Null, "woo: native none contract");
}

#[test]
fn client_id_audit_covers_every_hosted_delegated_venue() {
    let audited: std::collections::BTreeSet<_> = [
        "apexomni",
        "asterdex",
        "backpack",
        "binance",
        "binanceinverse",
        "binanceoptions",
        "binancepm",
        "binancespot",
        "bingx",
        "bitget",
        "bitgetinverse",
        "bitgetusdc",
        "bitunix",
        "blofin",
        "bybit",
        "bybitdemo",
        "bybitinverse",
        "bybitspot",
        "coinbase",
        "coinbasespot",
        "cryptocom",
        "grvt",
        "hyperliquid",
        "hyperliquidspot",
        "hyrotrader",
        "kraken",
        "kucoin",
        "kucoinspot",
        "lighter",
        "lighter-robinhood",
        "mexc",
        "okx",
        "okxdemo",
        "okxeu",
        "okxeudemo",
        "paradex",
        "phemex",
        "woo",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(
        crate::delegated::enabled_venues()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        audited
    );
    // 058ce5b07 initially counted Bitrue; its missing native origin keeps it
    // with the unhosted declarations, outside the active derivation census.
    for venue in ["bingxspot", "txflow", "bitrue"] {
        assert!(!crate::delegated::enabled(venue));
    }
}
