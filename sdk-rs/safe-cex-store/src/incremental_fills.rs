use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalFill {
    pub qty: f64,
    pub value: Option<f64>,
}

pub fn create_incremental_fills(
    max_orders: f64,
) -> impl FnMut(&str, f64, Option<f64>) -> IncrementalFill {
    let mut seen: VecDeque<(String, f64, f64)> = VecDeque::new();
    move |order_id, cumulative_qty, cumulative_value| {
        let previous = seen
            .iter()
            .find(|(id, _, _)| id == order_id)
            .map(|(_, qty, value)| (*qty, *value))
            .unwrap_or((0.0, 0.0));
        let qty = cumulative_qty - previous.0;
        if qty.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return IncrementalFill {
                qty: 0.0,
                value: None,
            };
        }
        seen.retain(|(id, _, _)| id != order_id);
        seen.push_back((
            order_id.into(),
            cumulative_qty,
            cumulative_value.unwrap_or(previous.1),
        ));
        while !seen.is_empty() {
            if seen.len() as f64 <= max_orders {
                break;
            }
            seen.pop_front();
        }
        let value = cumulative_value
            .map(|value| value - previous.1)
            .filter(|value| *value > 0.0);
        IncrementalFill { qty, value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replays_older_frames_missing_values_and_restarts() {
        let mut fills = create_incremental_fills(500.0);
        assert_eq!(
            fills("A", 2.0, Some(20.0)),
            IncrementalFill {
                qty: 2.0,
                value: Some(20.0)
            }
        );
        for qty in [2.0, 1.0, f64::NAN] {
            assert_eq!(
                fills("A", qty, Some(99.0)),
                IncrementalFill {
                    qty: 0.0,
                    value: None
                }
            );
        }
        assert_eq!(
            fills("A", 3.0, None),
            IncrementalFill {
                qty: 1.0,
                value: None
            }
        );
        assert_eq!(
            fills("A", 4.0, Some(40.0)),
            IncrementalFill {
                qty: 1.0,
                value: Some(20.0)
            }
        );
        assert_eq!(
            fills("A", 5.0, Some(35.0)),
            IncrementalFill {
                qty: 1.0,
                value: None
            }
        );
        assert_eq!(
            fills("A", 6.0, Some(40.0)),
            IncrementalFill {
                qty: 1.0,
                value: Some(5.0)
            }
        );
        let mut restarted = create_incremental_fills(500.0);
        assert_eq!(
            restarted("A", 6.0, Some(40.0)),
            IncrementalFill {
                qty: 6.0,
                value: Some(40.0)
            }
        );
    }
    #[test]
    fn only_advancing_fills_refresh_the_bounded_order_cache() {
        let mut fills = create_incremental_fills(2.0);
        fills("A", 1.0, None);
        fills("B", 1.0, None);
        fills("A", 1.0, None);
        fills("C", 1.0, None);
        assert_eq!(fills("A", 2.0, None).qty, 2.0);
        fills("C", 2.0, None);
        fills("D", 1.0, None);
        assert_eq!(fills("A", 3.0, None).qty, 3.0);
        for capacity in [0.0, -1.0, f64::NAN] {
            let mut fills = create_incremental_fills(capacity);
            assert_eq!(fills("A", 1.0, None).qty, 1.0);
            assert_eq!(fills("A", 2.0, None).qty, 2.0);
        }
    }
    #[test]
    fn checked_in_typescript_fill_oracle() {
        fn number(value: &serde_json::Value) -> f64 {
            value
                .as_f64()
                .unwrap_or_else(|| match value.as_str().unwrap() {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    other => panic!("unexpected number {other}"),
                })
        }
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/fills_oracle.json")).unwrap();
        for case in cases {
            let mut fills = create_incremental_fills(number(&case["capacity"]));
            for step in case["steps"].as_array().unwrap() {
                let value = if step["value"].is_null() {
                    None
                } else {
                    Some(number(&step["value"]))
                };
                let actual = fills(step["id"].as_str().unwrap(), number(&step["qty"]), value);
                assert_eq!(
                    actual.qty,
                    step["expected"]["qty"].as_f64().unwrap(),
                    "{step}"
                );
                assert_eq!(actual.value, step["expected"]["value"].as_f64(), "{step}");
            }
        }
    }
}
