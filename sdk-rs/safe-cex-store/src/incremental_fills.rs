use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq)]
pub struct IncrementalFill {
    pub qty: f64,
    pub value: Option<f64>,
}

// Number(n.toPrecision(digits)). toPrecision rounds the exact decimal value with ties going
// up in magnitude (Rust's `{:e}` breaks ties to even), and -0 prints as 0.
fn to_precision(n: f64, digits: usize) -> f64 {
    if !n.is_finite() {
        return n;
    }
    if n == 0.0 {
        return 0.0;
    }
    let magnitude = n.abs();
    // 800 fractional digits hold every f64's exact decimal expansion.
    let exact = format!("{magnitude:.800e}");
    let (mantissa, exponent) = exact.split_once('e').expect("exponent form");
    let exponent: i32 = exponent.parse().expect("exponent");
    let all: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let (head, tail) = all.split_at(digits);
    let tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let last = exponent - (digits as i32 - 1);
    let rounded: f64 = if tie {
        let up = head.parse::<u64>().expect("significant digits") + 1;
        format!("{up}e{last}").parse().expect("decimal")
    } else {
        format!("{magnitude:.prec$e}", prec = digits - 1)
            .parse()
            .expect("decimal")
    };
    rounded.copysign(n)
}

// ts: cleanFloat — Number(n.toPrecision(15)) for finite n: drops binary noise past DBL_DIG.
pub fn clean_float(n: f64) -> f64 {
    to_precision(n, 15)
}

// ts: decimalsOf — digits after the point in String(n); undefined in exponent form.
fn decimals_of(n: f64) -> Option<usize> {
    let text = safe_cex_num::js_number_string(n);
    if text.contains('e') {
        return None;
    }
    Some(text.split_once('.').map_or(0, |(_, fraction)| fraction.len()))
}

// ts: subtractDecimals — a - b rounded to the inputs' own decimal places (exact for numbers
// parsed from venue strings), else cleanFloat(a - b).
fn subtract_decimals(a: f64, b: f64) -> f64 {
    let (Some(da), Some(db)) = (decimals_of(a), decimals_of(b)) else {
        return clean_float(a - b);
    };
    let digits = da.max(db).min(20) as f64;
    let fixed = safe_cex_num::js_to_fixed(a - b, digits).expect("toFixed digits within 0..=20");
    safe_cex_num::number_from_string(&fixed)
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

#[cfg(test)]
mod clean_float_tests {
    use super::clean_float;
    #[test]
    fn clean_float_matches_to_precision_fifteen() {
        let cases: [(f64, f64); 12] = [
            (4847.5704 / 0.058, 83578.8),
            (0.1 + 0.2, 0.3),
            (-0.1 - 0.2, -0.3),
            (123456789012.5, 123456789012.5),
            (123456789012345.5, 123456789012346.0),
            (-123456789012345.5, -123456789012346.0),
            (123456789012344.5, 123456789012345.0),
            (1e21 / 3.0, 333333333333333000000.0),
            (5e-324, 5e-324),
            (100000.1 - 100000.0, 0.100000000005821),
            (-0.0, 0.0),
            (1.0, 1.0),
        ];
        for (input, expected) in cases {
            let got = clean_float(input);
            assert_eq!(got.to_bits(), expected.to_bits(), "clean_float({input:e})");
        }
        let mut fills = super::create_incremental_fills(500.0);
        fills("big", 100000.0, None);
        assert_eq!(
            fills("big", 100000.1, None).qty.to_bits(),
            0.1_f64.to_bits(),
            "a small fill off a large total rounds to the inputs' decimals (cleanFloat alone gives 0.100000000005821)"
        );
        fills("a", 0.1, Some(0.1));
        let fill = fills("a", 0.3, Some(0.3));
        assert_eq!(
            (fill.qty.to_bits(), fill.value.map(f64::to_bits)),
            (0.2_f64.to_bits(), Some(0.2_f64.to_bits())),
            "cumulative differences are cleaned (raw 0.19999999999999998)"
        );
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                clean_float(value).to_bits(),
                value.to_bits(),
                "nonfinite passthrough"
            );
        }
    }
}
