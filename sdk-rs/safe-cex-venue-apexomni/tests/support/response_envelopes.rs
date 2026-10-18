use super::*;

fn captured() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/response-envelopes.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "APEX-ENVELOPE source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn run_envelopes(method: &str, expected_count: usize) {
    let rows = captured()["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 33, "APEX-ENVELOPE independent source floor");
    let mut count = 0;
    for row in rows.iter().filter(|row| row["name"] == method) {
        let host = Rc::new(Host::default());
        let endpoint = if method == "fetchOrders" {
            "/v3/open-orders"
        } else {
            "/v3/account"
        };
        host.responses
            .borrow_mut()
            .insert(endpoint, native::input(&row["payload"]));
        let store = fixture(&host);
        let base = BaseExchange::new(
            ExchangeName::Apexomni,
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            store.clone(),
            host.clone(),
            ExchangeSettings::default(),
        );
        let exchange = ApexOmniExchange::new(base, host.clone()).unwrap();
        exchange
            .recently_cancelled_orders
            .borrow_mut()
            .insert("17".into(), 1700001233568.0);
        if row["label"].as_str().unwrap().starts_with("cache-") {
            let J::Number(timestamp) = native::input(&row["cancelTime"]) else {
                panic!("authored cancellation timestamp")
            };
            exchange
                .recently_cancelled_orders
                .borrow_mut()
                .insert("order-1".into(), timestamp);
        }
        host.operations.borrow_mut().clear();
        let actual = futures::executor::block_on(async {
            match method {
                "getAccount" => exchange.get_account().await.map(|v| native::retained(&v)),
                "fetchPositions" => exchange
                    .fetch_positions()
                    .await
                    .map(|rows| J::Array(rows.iter().map(position).collect())),
                "fetchOrders" => exchange
                    .fetch_orders()
                    .await
                    .map(|rows| J::Array(rows.iter().map(order).collect())),
                _ => panic!("APEX-ENVELOPE authored method"),
            }
        });
        let label = format!("APEX-ENVELOPE {method}/{}", row["label"].as_str().unwrap());
        if method == "fetchOrders" && row["label"] == "numeric-map-key" {
            assert_eq!(
                row["result"]["ok"], true,
                "{label} source numeric id accepted"
            );
            let source = native::input(&row["result"]["value"]);
            let J::Array(source_orders) = source else {
                panic!("authored source orders")
            };
            let J::Object(source_order) = &source_orders[0] else {
                panic!("authored source order")
            };
            assert!(
                source_order
                    .iter()
                    .any(|(key, value)| key == "id" && *value == n(17.0)),
                "{label} source native numeric id"
            );
            let error = actual.expect_err("native closed Order.id must visibly refuse numeric ids");
            assert_eq!(error.name, "Error", "{label} native typed refusal kind");
            assert_eq!(
                error.message, "shared typed record refuses a non-string field",
                "{label} native typed refusal cause"
            );
        } else {
            match actual {
                Ok(value) => {
                    assert_eq!(
                        row["result"]["ok"], true,
                        "{label} native expected rejection"
                    );
                    native::check_fields(
                        &value,
                        &row["result"]["value"],
                        &format!("{label} native value"),
                    );
                }
                Err(error) => {
                    assert_eq!(
                        row["result"]["ok"], false,
                        "{label} native unexpected rejection: {error:?}"
                    );
                    assert_eq!(
                        error.name,
                        row["result"]["name"].as_str().unwrap(),
                        "{label} native error kind"
                    );
                    assert_eq!(
                        error.message,
                        row["result"]["message"].as_str().unwrap(),
                        "{label} native error message"
                    );
                }
            }
        }
        native::check(
            &J::Array(host.operations.borrow().clone()),
            &row["operations"],
            &format!("{label} native operation order"),
        );
        native::check_fields(
            &native::retained(store.zk_link_config().unwrap().as_ref().unwrap()),
            &row["zk"],
            &format!("{label} native config unchanged"),
        );
        let mut cancelled: Vec<_> = exchange
            .recently_cancelled_orders
            .borrow()
            .iter()
            .map(|(id, timestamp)| (id.clone(), *timestamp))
            .collect();
        cancelled.sort_by(|left, right| left.0.cmp(&right.0));
        native::check(
            &J::Array(
                cancelled
                    .into_iter()
                    .map(|(id, timestamp)| J::Array(vec![J::String(id), n(timestamp)]))
                    .collect(),
            ),
            &row["cancelled"],
            &format!("{label} native cancellation state"),
        );
        count += 1;
    }
    assert_eq!(
        count, expected_count,
        "APEX-ENVELOPE {method} independent executed floor"
    );
}
#[test]
fn account_envelopes_match_actual_source() {
    run_envelopes("getAccount", 7);
}
#[test]
fn position_envelopes_match_actual_source() {
    run_envelopes("fetchPositions", 10);
}
#[test]
fn order_envelopes_match_actual_source() {
    run_envelopes("fetchOrders", 16);
}
