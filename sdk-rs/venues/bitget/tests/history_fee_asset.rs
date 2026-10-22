//! Classic fills schema with native fee currencies, including a delisted symbol.
//! https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade
use crate::support::FixtureOptions as Options;
mod support;

use safe_cex_engine::{Command, Query};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::Harness;

fn inverse() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        product: Product::Inverse,
        ..Options::default()
    });
    h.respond(
        "/contracts",
        200,
        &support::CONTRACTS.replace("USDT", "USD"),
    );
    h.respond("/tickers", 200, &support::TICKERS.replace("USDT", "USD"));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.command(Command::Query {
        command: 42,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: None,
        },
    });
    h
}

fn fill(id: &str, symbol: &str, fee_coin: &str) -> Value {
    json!({
        "tradeId": id,
        "symbol": symbol,
        "orderId": "historical-order",
        "price": "100",
        "baseVolume": "1",
        "feeDetail": [{"feeCoin": fee_coin, "totalFee": "-0.1"}],
        "side": "buy",
        "profit": "1",
        "tradeSide": "buy_single",
        "tradeScope": "taker",
        "cTime": "1760879999000"
    })
}

fn page(h: &mut Harness, rows: Value) {
    h.respond(
        "/order/fills",
        200,
        &json!({"code":"00000","data":{"fillList":rows,"endId":""}}).to_string(),
    );
}

#[test]
fn explicit_fee_currency_keeps_delisted_fills_beside_current_market_fallback() {
    let mut h = inverse();
    assert!(h.engine.store().market("LTCUSD").is_none());
    page(
        &mut h,
        json!([
            fill("current", "BTCUSD", ""),
            fill("delisted", "LTCUSD", "LTC")
        ]),
    );
    let result = &h.done.iter().find(|(id, _)| *id == 42).unwrap().1;
    let rows: Value = serde_json::from_str(result.as_ref().unwrap()).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 2);
    for (index, id, symbol, coin) in [
        (0, "current", "BTCUSD", "BTC"),
        (1, "delisted", "LTCUSD", "LTC"),
    ] {
        assert_eq!(rows[index]["id"], id);
        assert_eq!(rows[index]["symbol"], symbol);
        assert_eq!(rows[index]["commissionAsset"], coin);
        assert_eq!(rows[index]["commission"], 0.1);
        assert_eq!(rows[index]["qty"], 1.0);
    }
}

#[test]
fn missing_fee_currency_and_market_still_fail_without_partial_results() {
    let mut h = inverse();
    let rows = json!([
        fill("current", "BTCUSD", "BTC"),
        fill("delisted", "LTCUSD", "")
    ]);
    for attempt in 0..4 {
        page(&mut h, rows.clone());
        if attempt < 3 {
            assert!(h.done.iter().all(|(id, _)| *id != 42));
            h.timer_after(250.0 * f64::from(attempt + 1));
        }
    }
    let results: Vec<_> = h.done.iter().filter(|(id, _)| *id == 42).collect();
    assert_eq!(results.len(), 1);
    assert!(results[0].1.as_ref().unwrap_err().contains("incomplete"));
}
