//! Binance history on the production native host: income paging through the shared
//! pager and the journal fold, against a local server that honours the time window.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_exchange::journal::HistoricalPositionOptions;
use serde_json::{Value, json};
use std::rc::Rc;
use support::*;

const T0: f64 = 1_700_000_000_000.0;
const HOUR: f64 = 3_600_000.0;
const DAY: f64 = 24.0 * HOUR;

fn query_number(query: &str, key: &str) -> Option<f64> {
    query_value(query, key).and_then(|v| v.parse().ok())
}
/// Binance windows: rows inside [startTime, endTime], oldest first, at most `limit`.
fn window(rows: &[Value], request: &Seen) -> Value {
    let start = query_number(&request.query, "startTime").unwrap_or(f64::MIN);
    let end = query_number(&request.query, "endTime").unwrap_or(f64::MAX);
    let limit = query_number(&request.query, "limit").unwrap_or(1000.0) as usize;
    let symbol = query_value(&request.query, "symbol");
    Value::Array(
        rows.iter()
            .filter(|row| {
                let time = row["time"].as_f64().unwrap();
                time >= start && time <= end && symbol.is_none_or(|s| row["symbol"] == s)
            })
            .take(limit)
            .cloned()
            .collect(),
    )
}

#[test]
fn income_range_pages_through_the_shared_pager_over_http() {
    run_local(async {
        let ledger: Rc<Vec<Value>> = Rc::new(
            (0..1500)
                .map(|i| json!({"symbol":"BTCUSDT","incomeType":"FUNDING_FEE","income":"-0.01","asset":"USDT",
                    "time": T0 + f64::from(i) * 60_000.0, "tranId": 10_000 + i, "tradeId": ""}))
                .collect(),
        );
        let rows = ledger.clone();
        let h = binance_harness(Rc::new(move |r: &Seen| {
            (r.path == "/fapi/v1/income").then(|| window(&rows, r))
        }))
        .await;
        let records = h
            .exchange
            .fetch_income_history_range(T0, T0 + 2.0 * DAY, None)
            .await
            .expect("fetchIncomeHistoryRange over HTTP");
        assert_eq!(records.len(), 1500, "every ledger row in the window");
        let mut ids: Vec<_> = records.iter().map(|r| r.tran_id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 1500, "no row lost or duplicated across pages");
        let pages = h
            .seen
            .borrow()
            .iter()
            .filter(|s| s.path == "/fapi/v1/income")
            .count();
        assert!(
            pages >= 2,
            "a 1500-row window needs at least two 1000-row pages, saw {pages}"
        );
    });
}

#[test]
fn historical_positions_fold_fills_and_income_over_http() {
    run_local(async {
        let trades: Rc<Vec<Value>> = Rc::new(vec![
            json!({"symbol":"BTCUSDT","id":1,"orderId":11,"side":"BUY","price":"100","qty":"1","quoteQty":"100",
                "realizedPnl":"0","commission":"0.1","commissionAsset":"USDT","time":T0 + HOUR,
                "positionSide":"BOTH","buyer":true,"maker":false}),
            json!({"symbol":"BTCUSDT","id":2,"orderId":12,"side":"SELL","price":"110","qty":"1","quoteQty":"110",
                "realizedPnl":"10","commission":"0.1","commissionAsset":"USDT","time":T0 + 2.0 * HOUR,
                "positionSide":"BOTH","buyer":false,"maker":false}),
        ]);
        let income: Rc<Vec<Value>> =
            Rc::new(vec![json!({"symbol":"BTCUSDT","incomeType":"REALIZED_PNL",
            "income":"10","asset":"USDT","time":T0 + 2.0 * HOUR,"tranId":7,"tradeId":"2"})]);
        let h = binance_harness(Rc::new(move |r: &Seen| match r.path.as_str() {
            "/fapi/v1/userTrades" => Some(window(&trades, r)),
            "/fapi/v1/income" => Some(window(&income, r)),
            _ => None,
        }))
        .await;
        let options = HistoricalPositionOptions {
            account: "native-account".into(),
            from: Some(T0),
            to: Some(T0 + DAY),
        };
        let records = h
            .exchange
            .fetch_historical_positions(options)
            .await
            .expect("fetchHistoricalPositions folds");
        assert_eq!(records.len(), 1, "one round trip: {records:?}");
        let record = &records[0];
        assert_eq!(record["symbol"], "BTCUSDT");
        assert_eq!(record["account"], "native-account");
        assert_eq!(record["realizedPnl"], json!(10.0));
        assert_eq!(record["settleCurrency"], "USDT");
        assert!(
            h.seen
                .borrow()
                .iter()
                .any(|s| s.path == "/fapi/v1/userTrades" && s.query.contains("symbol=BTCUSDT"))
        );
    });
}
