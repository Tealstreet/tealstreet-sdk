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
fn malformed_nan_price_permutation_is_explicitly_divergent() {
    let mut book = OrderBook {
        bids: vec![],
        asks: [3.0, f64::NAN, 2.0, 1.0]
            .into_iter()
            .enumerate()
            .map(|(i, price)| OrderBookOrders {
                price,
                amount: 1.0,
                total: i as f64,
            })
            .collect(),
    };
    sort_order_book(&mut book);
    assert_eq!(
        book.asks.iter().map(|x| x.total).collect::<Vec<_>>(),
        [3.0, 2.0, 0.0, 1.0]
    );
}
