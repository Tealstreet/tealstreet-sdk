use safe_cex_store::orderbook::{calc_order_book_total, sort_order_book};
use safe_cex_types::{OrderBook, OrderBookOrders};
use serde_json::Value;
fn equal(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                equal(v, &b[k]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires root TypeScript dependencies"]
fn current_source_orderbook_sort_and_totals() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_orderbook_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 200);
    for row in rows {
        let mut book: OrderBook = serde_json::from_value(row["input"].clone()).unwrap();
        sort_order_book(&mut book);
        equal(&serde_json::to_value(&book).unwrap(), &row["sorted"]);
        calc_order_book_total(&mut book);
        equal(&serde_json::to_value(&book).unwrap(), &row["total"]);
    }
}
#[test]
fn native_nan_book_order_matches_ci_v8_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/book-sort-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 112, "complete native orderbook profiles");
    let mut failures = Vec::new();
    for (case, row) in rows.iter().enumerate() {
        let prices: Vec<f64> = row["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bits| f64::from_bits(u64::from_str_radix(bits.as_str().unwrap(), 16).unwrap()))
            .collect();
        let levels = || {
            prices
                .iter()
                .enumerate()
                .map(|(id, price)| OrderBookOrders {
                    price: *price,
                    amount: (id + 1) as f64,
                    total: -(id as f64),
                })
                .collect()
        };
        let mut book = OrderBook {
            bids: levels(),
            asks: levels(),
        };
        sort_order_book(&mut book);
        for (side, actual) in [("bids", &book.bids), ("asks", &book.asks)] {
            let expected = row[side].as_array().unwrap();
            assert_eq!(actual.len(), prices.len());
            assert_eq!(expected.len(), prices.len());
            for (level, id) in actual.iter().zip(expected) {
                let id = id.as_u64().unwrap() as usize;
                if level.price.to_bits() != prices[id].to_bits()
                    || level.amount.to_bits() != ((id + 1) as f64).to_bits()
                    || level.total.to_bits() != (-(id as f64)).to_bits()
                {
                    failures.push((case, side));
                    break;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "STORE-BOOK-SORT exact Node permutation/retained fields: {failures:?}"
    );
}
