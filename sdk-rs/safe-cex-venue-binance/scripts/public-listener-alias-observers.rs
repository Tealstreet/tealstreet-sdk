// Decode the actual Book alias, then inspect native field bits and cell identity.
pub fn book(decoded: &crate::public_listeners::Book) -> serde_json::Value {
    let retained = decoded.clone();
    let row = decoded.borrow();
    let levels = |rows: &[safe_cex_types::OrderBookOrders]| {
        rows.iter().map(|row| {
        serde_json::json!({"price":format!("{:016x}",row.price.to_bits()),
            "amount":format!("{:016x}",row.amount.to_bits()),"total":format!("{:016x}",row.total.to_bits())})
    }).collect::<Vec<_>>()
    };
    serde_json::json!({"sameCell":std::rc::Rc::ptr_eq(decoded,&retained),
        "bids":levels(&row.bids),"asks":levels(&row.asks)})
}
