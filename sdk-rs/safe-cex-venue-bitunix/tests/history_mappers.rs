use safe_cex_num::{JsCoercionError, numeric_ingress::js_deserialize_passthrough};
use safe_cex_types::*;
use safe_cex_venue_bitunix::history_mappers::*;
use serde_json::{Value, json};
use std::process::Command;
fn number(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn optional(n: Option<f64>) -> Value {
    n.map_or(json!({"undefined":true}), number)
}
fn result(r: Result<Value, JsCoercionError>) -> Value {
    match r {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e.name,"message":e.message}),
    }
}
fn income(r: IncomeRecord) -> Value {
    json!({"symbol":r.symbol,"incomeType":r.income_type,"income":number(r.income),"asset":r.asset,"timestamp":number(r.timestamp),"tranId":r.tran_id,"tradeId":r.trade_id.unwrap(),"category":match r.category.unwrap(){IncomeCategory::RealizedPnl=>"realized_pnl",IncomeCategory::Commission=>"commission",IncomeCategory::Rebate=>"rebate",_=>panic!("HISTORY native unexpected category")}})
}
fn closed(r: ClosedPnlEntry) -> Value {
    let ClosedPnlEntryNotional::Variant1(notional) = r.notional else {
        panic!("HISTORY native numeric notional")
    };
    let ClosedPnlEntryContracts::Variant1(contracts) = r.contracts else {
        panic!("HISTORY native numeric contracts")
    };
    let ClosedPnlEntryEntryPrice::Variant1(entry) = r.entry_price else {
        panic!("HISTORY native numeric entry")
    };
    let ClosedPnlEntryExitPrice::Variant1(exit) = r.exit_price else {
        panic!("HISTORY native numeric exit")
    };
    let ClosedPnlEntryUnrealizedPnl::Variant1(pnl) = r.unrealized_pnl else {
        panic!("HISTORY native numeric pnl")
    };
    let ClosedPnlEntryFees::Variant1(fees) = r.fees else {
        panic!("HISTORY native numeric fees")
    };
    let ClosedPnlEntryLeverage::Variant0(leverage) = r.leverage else {
        panic!("HISTORY native numeric leverage")
    };
    assert_eq!(
        r.provenance,
        DataProvenance::Venue,
        "HISTORY native provenance"
    );
    json!({"provenance":"venue","symbol":r.symbol,"notional":number(notional),"contracts":number(contracts),"entryPrice":number(entry),"exitPrice":number(exit),"unrealizedPnl":number(pnl),"side":r.side,"fees":number(fees),"time":number(r.time),"closedTime":number(r.closed_time),"account":r.account,"leverage":number(leverage)})
}
fn order(r: HistoricalOrder) -> Value {
    assert!(
        r.position_side.is_none()
            && r.realized_pnl.is_none()
            && r.fees.is_none()
            && r.fee_asset.is_none()
            && r.rebate.is_none()
            && r.rebate_asset.is_none(),
        "HISTORY native absent extension fields"
    );
    json!({"id":r.id,"symbol":r.symbol,"side":match r.side{OrderSide::Buy=>"buy",OrderSide::Sell=>"sell"},"type":match r.r#type{OrderType::Market=>"market",OrderType::Limit=>"limit",_=>panic!("HISTORY native unexpected type")},"status":match r.status{OrderStatus::Open=>"open",OrderStatus::Closed=>"closed",OrderStatus::Canceled=>"canceled",_=>panic!("HISTORY native unexpected status")},"price":optional(r.price),"averagePrice":optional(r.average_price),"amount":number(r.amount),"filled":number(r.filled),"reduceOnly":r.reduce_only.unwrap(),"createdTime":number(r.created_time),"updatedTime":optional(r.updated_time)})
}
#[test]
fn actual_typescript_history_mappers() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/history-mappers-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "HISTORY actual source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("oracle runtime {}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 528, "HISTORY independent case count");
    let mut counts = [0; 3];
    for (i, row) in rows.iter().enumerate() {
        let native = js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
            row["raw"].as_str().unwrap(),
        ))
        .unwrap();
        let actual = match row["key"].as_str().unwrap() {
            "trade" => {
                counts[0] += 1;
                result(
                    map_bitunix_trade_to_income_records(&native)
                        .map(|r| json!(r.into_iter().map(income).collect::<Vec<_>>())),
                )
            }
            "closed" => {
                counts[1] += 1;
                result(
                    map_bitunix_closed_position(&native, "acct")
                        .map(|r| r.map_or(Value::Null, closed)),
                )
            }
            "order" => {
                counts[2] += 1;
                result(map_bitunix_historical_order(&native).map(|r| r.map_or(Value::Null, order)))
            }
            _ => panic!(),
        };
        assert_eq!(
            actual, row["result"],
            "HISTORY decoded native {} case {i}",
            row["key"]
        );
    }
    assert_eq!(counts, [176; 3], "HISTORY independent method count");
}
