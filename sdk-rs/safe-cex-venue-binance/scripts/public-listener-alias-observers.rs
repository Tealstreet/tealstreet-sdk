// Decode the actual Book alias, then inspect native field bits and cell identity.
pub fn book(value: &serde_json::Value) -> serde_json::Value {
    type Book = safe_cex_venue_binance::public_listeners::Book;
    let Ok(decoded) = serde_json::from_str::<Book>(value["wire"].as_str().unwrap()) else {
        return serde_json::Value::Null;
    };
    let retained = decoded.clone();
    let row = decoded.borrow();
    let levels = |rows: &[safe_cex_types::OrderBookOrders]| rows.iter().map(|row| {
        serde_json::json!({"price":format!("{:016x}",row.price.to_bits()),
            "amount":format!("{:016x}",row.amount.to_bits()),"total":format!("{:016x}",row.total.to_bits())})
    }).collect::<Vec<_>>();
    serde_json::json!({"sameCell":std::rc::Rc::ptr_eq(&decoded,&retained),
        "bids":levels(&row.bids),"asks":levels(&row.asks)})
}
