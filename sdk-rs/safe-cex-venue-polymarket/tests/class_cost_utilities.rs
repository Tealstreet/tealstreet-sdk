//! Bounded generated differential observations, without a completeness claim.
use safe_cex_num::PrecisionInput;
use safe_cex_venue_polymarket::utilities::{
    normalize_tick_size, parse_gamma_market_tokens, precision_from_tick_size,
};
use serde_json::Value;
use std::process::Command;

fn input(value: &Value) -> Option<PrecisionInput> {
    match value["kind"].as_str().unwrap() {
        "null" => None,
        "number" => Some(PrecisionInput::Number(f64::from_bits(
            u64::from_str_radix(value["bits"].as_str().unwrap(), 16).unwrap(),
        ))),
        "string" => Some(PrecisionInput::String(
            value["value"].as_str().unwrap().to_owned(),
        )),
        _ => panic!("unrecognized generated input"),
    }
}
fn units(value: &Value) -> Vec<u16> {
    serde_json::from_value(value.clone()).unwrap()
}

fn encoded(value: Option<PrecisionInput>) -> Value {
    match value {
        None => serde_json::json!({"kind":"null"}),
        Some(PrecisionInput::Number(n)) => {
            serde_json::json!({"kind":"number","bits":format!("{:016x}", n.to_bits())})
        }
        Some(PrecisionInput::String(s)) => serde_json::json!({"kind":"string","value":s}),
    }
}

// Construct inputs independently in Rust so replacing inputs and outputs together
// cannot silently change the declared profile. Expectations never call the port.
fn token_profile_length(n: usize) -> usize {
    if n < 33 {
        n
    } else {
        [
            33, 34, 63, 64, 65, 66, 127, 128, 129, 130, 255, 256, 257, 258, 511, 512, 513, 514,
            1023, 1024, 1025, 47, 48, 49, 95, 96, 97, 191, 192, 193, 194,
        ][n - 33]
    }
}

fn expected_inputs(index: usize) -> (Value, Value, Value) {
    let group = index % 8;
    let n = index / 8;
    let tick = ["0.1", "0.01", "0.005", "0.0025", "0.001", "0.0001"][n % 6];
    let mut state = 0x0123_4567_89ab_cdef_u64;
    for _ in 0..=index {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
    }
    let random = f64::from_bits(state & 0xffef_ffff_ffff_ffff);
    let string = |s: String| Some(PrecisionInput::String(s));
    let number = |n| Some(PrecisionInput::Number(n));
    let tick_input = match group {
        0 => string(tick.to_owned()),
        1 => number(tick.parse().unwrap()),
        2 => string(format!(" {tick} ")),
        3 => None,
        4 => number(random),
        5 => string(format!("😀é-{n}")),
        6 => string(String::new()),
        7 => number(-(n as f64)),
        _ => unreachable!(),
    };
    let halfway = ((2 * n + 1) as f64 / 512.0).to_bits();
    let adjacent = halfway.wrapping_add_signed(n as i64 % 3 - 1);
    let precision_input = match group {
        0 => number(random),
        1 => number(f64::from_bits(adjacent)),
        2 => string(tick.to_owned()),
        3 => string(
            [
                "0x10",
                "0b11",
                "1_000",
                "NaN",
                "Infinity",
                " 0.0025 ",
                "\u{feff}0.001",
                "1.23x",
            ][n % 8]
                .to_owned(),
        ),
        4 => number(-(n as f64)),
        5 => number([f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0][n % 4]),
        6 => number([1e21, 1e22, 1e30, 1e100, 1e300][n % 5]),
        7 => string(format!("{n}.{:08}", n * 73)),
        _ => unreachable!(),
    };
    let ids: Vec<_> = (0..token_profile_length(n))
        .map(|j| format!("token-{n}-{j}"))
        .collect();
    let outcomes: Vec<_> = (0..token_profile_length(n))
        .map(|j| format!("outcome-{n}-{j}"))
        .collect();
    let json = |v: Value| v.to_string();
    let pair = match group {
        0 => (
            Some(json(serde_json::json!(ids))),
            json(serde_json::json!(outcomes)),
        ),
        1 => {
            let mut extra = outcomes;
            extra.push("extra".to_owned());
            (
                Some(json(serde_json::json!(ids))),
                json(serde_json::json!(extra)),
            )
        }
        2 => (
            Some("[broken".to_owned()),
            json(serde_json::json!(outcomes)),
        ),
        3 => (
            Some(json(serde_json::json!({"n":n}))),
            json(serde_json::json!(outcomes)),
        ),
        4 => (
            Some(json(serde_json::json!([
                null,
                n,
                format!("id-{n}"),
                {},
                []
            ]))),
            json(serde_json::json!([
                "skip",
                "skip",
                format!("yes-{n}"),
                "skip",
                "skip"
            ])),
        ),
        5 => (
            if n % 2 == 1 {
                None
            } else {
                Some(String::new())
            },
            json(serde_json::json!(outcomes)),
        ),
        6 => (
            Some(json(serde_json::json!([
                format!("😀-{n}"),
                format!("é-{n}")
            ]))),
            json(serde_json::json!([format!("Yes-{n}"), format!("No-{n}")])),
        ),
        7 => (
            Some(json(serde_json::json!([
                format!("same-{n}"),
                format!("same-{n}")
            ]))),
            json(serde_json::json!(["Yes", "No"])),
        ),
        _ => unreachable!(),
    };
    (
        encoded(tick_input),
        serde_json::json!({"tokenIds":pair.0,"outcomes":pair.1}),
        encoded(precision_input),
    )
}

fn class_cost_utility(family: &str) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/class_cost_utilities.cjs"
        ))
        .arg(family)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generated_oracle_execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("generated capture: {}", corpus["capture"]);
    let mut performed = [0; 3];
    let mut token_pairs = 0;
    let mut length65_witnesses = 0;
    for (column, row) in ["ticks", "tokens", "precisions"].iter().enumerate() {
        if *row != family {
            continue;
        }
        let cases = corpus[row].as_array().unwrap();
        assert_eq!(cases.len(), 512, "generated_independent_case_count:{row}");
        for (index, case) in cases.iter().enumerate() {
            assert_eq!(case["index"], index, "generated_case_identity:{row}");
            let (tick_input, token_input, precision_input) = expected_inputs(index);
            match column {
                0 => assert_eq!(case["input"], tick_input, "generated_tick_input:{index}"),
                1 => {
                    assert_eq!(
                        case["tokenIds"], token_input["tokenIds"],
                        "generated_token_input:{index}"
                    );
                    assert_eq!(
                        case["outcomes"], token_input["outcomes"],
                        "generated_outcome_input:{index}"
                    );
                }
                2 => assert_eq!(
                    case["input"], precision_input,
                    "generated_precision_input:{index}"
                ),
                _ => unreachable!(),
            }
            match column {
                0 => {
                    let native = normalize_tick_size(input(&case["input"]).as_ref());
                    if let Some(expected) = case.get("ok") {
                        assert_eq!(
                            native.as_ref().map(String::as_str),
                            Ok(expected.as_str().unwrap()),
                            "generated_tick_native:{index}"
                        );
                    } else {
                        assert_eq!(case["errorName"], "Error");
                        let error = native.expect_err("generated_tick_native_error");
                        assert_eq!(
                            error.encode_utf16().collect::<Vec<_>>(),
                            units(&case["error"]),
                            "generated_tick_native:{index}"
                        );
                    }
                }
                1 => {
                    let native = parse_gamma_market_tokens(
                        case["tokenIds"].as_str(),
                        case["outcomes"].as_str(),
                    );
                    let n = index / 8;
                    let expected_len = match index % 8 {
                        0 => token_profile_length(n),
                        4 => 1,
                        6 | 7 => 2,
                        _ => 0,
                    };
                    assert_eq!(
                        native.len(),
                        expected_len,
                        "generated_tokens_native_length:{index}"
                    );
                    if index == 296 {
                        assert_eq!(native.len(), 65, "generated_tokens_native_length65_witness");
                        length65_witnesses += 1;
                    }
                    token_pairs += native.len();
                    let expected = case["output"].as_array().unwrap();
                    assert_eq!(
                        expected.len(),
                        expected_len,
                        "generated_tokens_source_length:{index}"
                    );
                    for (j, (token, outcome)) in native.iter().enumerate() {
                        assert_eq!(
                            token.encode_utf16().collect::<Vec<_>>(),
                            units(&expected[j]["tokenId"]),
                            "generated_tokens_native_token:{index}:{j}"
                        );
                        assert_eq!(
                            outcome.encode_utf16().collect::<Vec<_>>(),
                            units(&expected[j]["outcome"]),
                            "generated_tokens_native_outcome:{index}:{j}"
                        );
                    }
                }
                2 => {
                    let native = precision_from_tick_size(input(&case["input"]).as_ref());
                    assert_eq!(
                        format!("{:016x}", native.to_bits()),
                        case["outputBits"],
                        "generated_precision_native:{index}"
                    );
                }
                _ => unreachable!(),
            }
            performed[column] += 1;
        }
    }
    let mut expected = [0; 3];
    expected[match family {
        "ticks" => 0,
        "tokens" => 1,
        "precisions" => 2,
        _ => unreachable!(),
    }] = 512;
    assert_eq!(performed, expected, "generated_performed_native_calls");
    if family == "tokens" {
        // Independent fixed totals: sum(0..32) + 31 declared boundary lengths,
        // plus 64 mixed/Unicode/duplicate profiles returning 1+2+2 pairs each.
        assert_eq!(token_pairs, 9037, "generated_tokens_native_pair_floor");
        assert_eq!(
            length65_witnesses, 1,
            "generated_tokens_length65_witness_floor"
        );
        eprintln!(
            "generated token boundary witness: index296 length65; 9037 native pairs; max profile length1025"
        );
    }
    eprintln!("generated native agreement: {family}: 512 calls; stop index511");
}

#[test]
fn class_cost_normalize_native() {
    class_cost_utility("ticks")
}
#[test]
fn class_cost_tokens_native() {
    class_cost_utility("tokens")
}
#[test]
fn class_cost_precision_native() {
    class_cost_utility("precisions")
}
