use crate::history;
use serde::Serialize;

#[derive(Serialize)]
pub struct HistoryOutput {
    pub orders: Vec<history::HistoricalOrder>,
    pub trades: Vec<history::HistoricalTrade>,
    pub closed: Vec<history::ClosedPnlEntry>,
    pub income: Vec<history::IncomeRecord>,
}

pub fn fixture() -> HistoryOutput {
    HistoryOutput {
        orders: serde_json::from_str(r#"[
            {"id":"order-1","symbol":"BTC","side":"buy","type":"market","status":"closed","amount":2,"filled":0,"createdTime":1},
            {"id":"order-2","symbol":"BTC","side":"sell","type":"limit","status":"canceled","price":0,"averagePrice":0,"amount":3,"filled":1,"reduceOnly":false,"positionSide":"both","createdTime":2,"updatedTime":3,"realizedPnl":12,"fees":2,"feeAsset":"USD","rebate":0.5,"rebateAsset":"USD"}
        ]"#).unwrap(),
        trades: serde_json::from_str(r#"[
            {"symbol":"BTC","orderId":"order-1","price":1,"qty":0,"positionSide":"long","side":"buy","time":1},
            {"id":"execution-2","symbol":"BTC","orderId":"order-1","price":2,"qty":3,"quoteQty":6,"realizedPnl":12,"positionSide":"both","side":"sell","buyer":false,"maker":false,"commission":0,"commissionAsset":"USD","time":2}
        ]"#).unwrap(),
        closed: serde_json::from_str(r#"[
            {"provenance":"venue","symbol":"BTC","notional":20,"entryPrice":10,"exitPrice":12,"unrealizedPnl":2,"side":"long","fees":0,"time":1,"closedTime":2,"account":"fixture","contracts":2,"leverage":1},
            {"provenance":"derived","symbol":"BTC","notional":"20.00","entryPrice":"10.00","exitPrice":"12.00","unrealizedPnl":"2.00","side":"short","fees":"0.00","time":3,"closedTime":4,"account":"fixture","contracts":"2.00","leverage":"1.00"},
            {"provenance":"venue","symbol":"BTC","notional":null,"entryPrice":null,"exitPrice":null,"unrealizedPnl":null,"side":"long","fees":null,"time":null,"closedTime":null,"account":"fixture","contracts":null,"leverage":null}
        ]"#).unwrap(),
        income: serde_json::from_str(r#"[
            {"symbol":"BTC","incomeType":"RAW_PNL","income":12,"asset":"USD","timestamp":1,"tranId":"bill-1","category":"realized_pnl"},
            {"symbol":"BTC","incomeType":"RAW_FEE","income":-2,"asset":"USD","timestamp":2,"tranId":"bill-1","category":"commission"},
            {"symbol":"BTC","incomeType":"RAW_FUNDING","income":-1,"asset":"USD","timestamp":3,"tranId":"bill-1","category":"funding"},
            {"symbol":"","incomeType":"RAW_TRANSFER","income":0,"asset":"USD","timestamp":4,"tranId":"bill-1","category":"transfer"}
        ]"#).unwrap(),
    }
}
