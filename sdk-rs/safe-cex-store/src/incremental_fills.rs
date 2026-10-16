use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalFill {
    pub qty: f64,
    pub value: Option<f64>,
}

/// ts: cleanFloat. Drops binary noise past the fifteen significant digits a double
/// holds exactly: 83578.79999999999 -> 83578.8.
pub fn clean_float(n: f64) -> f64 {
    if !n.is_finite() {
        return n;
    }
    let text = safe_cex_num::js_to_precision(n, 15.0).expect("15 is a valid precision");
    text.parse().expect("toPrecision text parses")
}
// ts: decimalsOf
fn decimals_of(n: f64) -> Option<usize> {
    let text = safe_cex_num::js_number_string(n);
    if text.contains('e') {
        return None;
    }
    Some(
        text.split_once('.')
            .map_or(0, |(_, fraction)| fraction.len()),
    )
}
/// ts: subtractDecimals. A small fill off a large running total keeps noise past
/// fifteen digits, so round to the inputs' own decimal places.
fn subtract_decimals(a: f64, b: f64) -> f64 {
    let (Some(da), Some(db)) = (decimals_of(a), decimals_of(b)) else {
        return clean_float(a - b);
    };
    let digits = da.max(db).min(20) as f64;
    let text = safe_cex_num::js_to_fixed(a - b, digits).expect("0..=20 digits");
    // Number(text): Rust's parser reads NaN, Infinity and exponent forms alike.
    text.parse().expect("toFixed text parses")
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
        let qty = subtract_decimals(cumulative_qty, previous.0);
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
            .map(|value| subtract_decimals(value, previous.1))
            .filter(|value| *value > 0.0);
        IncrementalFill { qty, value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fifteen_digit_ties_round_away_from_zero_as_to_precision_does() {
        // Node: (123456789012344.5).toPrecision(15); ties-to-even would give ...344.
        for (input, expected) in [
            (123456789012345.5, 123456789012346.0_f64),
            (-123456789012345.5, -123456789012346.0),
            (123456789012344.5, 123456789012345.0),
        ] {
            assert_eq!(
                clean_float(input).to_bits(),
                expected.to_bits(),
                "cleanFloat({input})"
            );
        }
        assert_eq!(clean_float(100000.1 - 100000.0), 0.100000000005821);
        assert_eq!(subtract_decimals(100000.1, 100000.0), 0.1);
    }
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
