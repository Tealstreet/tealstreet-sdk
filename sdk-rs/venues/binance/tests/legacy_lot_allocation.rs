//! Pin the legacy allocator's exact quantities, including its rounding loss.
mod support;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

fn param(url: &str, name: &str) -> String {
    let value = url
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == name).then_some(value)
        })
        .unwrap();
    let mut decoded = Vec::new();
    let bytes = value.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            decoded.push(u8::from_str_radix(&value[at + 1..at + 3], 16).unwrap());
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).unwrap()
}

fn quantities(amount: f64, max: f64, step: f64, min: f64) -> Vec<Vec<String>> {
    [Variant::UsdM, Variant::CoinM, Variant::Spot, Variant::PortfolioMargin].into_iter().map(|variant| {
        let mut h = Harness::started(Options {
            variant, key: Some("fixture-key".into()), secret: Some("fixture-seed".into()),
            ..Options::default()
        });
        let (prefix, symbol) = match variant {
            Variant::Spot => ("/api/v3", "BTCUSDT"),
            Variant::CoinM => ("/dapi/v1", "BTCUSD_PERP"),
            _ => ("/fapi/v1", "BTCUSDT"),
        };
        let info = serde_json::json!({"symbols":[{"symbol":symbol,"status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":step.to_string(),"minQty":min.to_string(),"maxQty":max.to_string()},{"filterType":"MARKET_LOT_SIZE","stepSize":step.to_string(),"minQty":min.to_string(),"maxQty":max.to_string()}]}]});
        h.respond(&format!("{prefix}/exchangeInfo"), 200, &info.to_string());
        assert!(h.engine.store().market(symbol).is_some(), "{:?} {:?}", h.logs, h.paths());
        h.respond(&format!("{prefix}/ticker/24hr"), 200, &support::DAILY.replace("BTCUSDT", symbol));
        h.requests.clear();
        h.event(Event::Command(Command::PlaceOrders {
            command: 77,
            orders: serde_json::from_value(serde_json::json!([{"symbol":symbol,"type":"limit","side":"buy","amount":amount,"price":100}])).unwrap(),
        }));
        let mut recorded = Vec::new();
        while !h.requests.is_empty() {
            let request = h.requests.remove(0);
            assert_eq!(request.method, Method::Post, "{:?}", h.paths());
            let batch = request.url.contains("batchOrders=");
            let rows = if batch {
                serde_json::from_str::<Vec<serde_json::Value>>(&param(&request.url, "batchOrders")).unwrap()
            } else {
                vec![serde_json::json!({"quantity":param(&request.url, "quantity")})]
            };
            let replies: Vec<_> = rows.iter().enumerate().map(|(index, _)| serde_json::json!({"orderId":recorded.len()+index+1})).collect();
            recorded.extend(rows.iter().map(|row| row["quantity"].as_str().unwrap().to_string()));
            h.reply(request.id, 200, "", &if batch { serde_json::to_string(&replies).unwrap() } else { replies[0].to_string() });
        }
        assert!(h.result(77).unwrap().is_ok(), "{:?}", h.done);
        recorded
    }).collect()
}

fn pin(amount: f64, max: f64, step: f64, min: f64, expected: &[&str], expected_sum: f64) {
    for actual in quantities(amount, max, step, min) {
        assert_eq!(actual, expected);
        assert_eq!(
            actual
                .iter()
                .map(|value| value.parse::<f64>().unwrap())
                .sum::<f64>(),
            expected_sum,
        );
    }
}

// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1830-1844;
// binancespot.exchange.ts:535-547; binanceinverse.exchange.ts:919-924;
// binancepm.exchange.ts:901-913. Verified by executing the TS allocator with safe-math.adjust.
#[test]
// KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
fn uneven_26_max_5_step_1_matches_ts_exactly() {
    pin(
        26.0,
        5.0,
        1.0,
        1.0,
        &["4", "4", "4", "4", "4", "4", "1"],
        25.0,
    );
}

#[test]
// KNOWN-ISSUE: legacy TS under-allocates (sum 20 of requested 22); kept for exact TS parity per Sam's decision
fn uneven_step_greater_than_one_matches_ts_exactly() {
    pin(22.0, 10.0, 2.0, 2.0, &["6", "6", "6", "2"], 20.0);
}

#[test]
// KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
fn uneven_remainder_below_minimum_still_matches_ts_exactly() {
    pin(12.0, 5.0, 1.0, 3.0, &["3", "3", "3", "2"], 11.0);
}
