use safe_cex_venue_coinbase::starbase_orders::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/special_numbers.rs"]
mod special;
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/write-edit.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
        capture_metadata::assert_capture_metadata(&oracle, 2_888);
        oracle
    })
}
#[test]
fn edit_builder_live_blocker_identity_validation_order_and_payload() {
    let cases = oracle()["cases"].as_array().unwrap();
    for (reduce_only, reject_post_only) in [(false, true), (true, false)] {
        assert!(
            cases.iter().any(|case| {
                case["intent"]["postOnly"] == true
                    && case["intent"]["reduceOnly"] == reduce_only
                    && case["intent"]["rejectPostOnly"] == reject_post_only
            }),
            "edit oracle must distinguish reduceOnly from rejectPostOnly"
        );
    }
    for case in cases {
        let input = &case["intent"];
        let number = |key: &str| {
            case["bits"]
                .get(key)
                .map(|v| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
        };
        let intent = CoinbaseStarbaseEditIntent {
            order_id: input["orderId"].as_str().unwrap().into(),
            price: number("price"),
            post_only: input.get("postOnly").map(|v| v.as_bool().unwrap()),
            reduce_only: input.get("reduceOnly").map(|v| v.as_bool().unwrap()),
            reject_post_only: input.get("rejectPostOnly").map(|v| v.as_bool().unwrap()),
            size: input
                .get("size")
                .map(|v| match v["unit"].as_str().unwrap() {
                    "base" => CoinbaseStarbaseWriteAmountIntent::Base(
                        coinbase_starbase_total_base_amount(v["totalBaseAmount"].as_f64().unwrap())
                            .unwrap(),
                    ),
                    "contracts" => CoinbaseStarbaseWriteAmountIntent::Contracts(
                        coinbase_starbase_total_contracts(v["totalContracts"].as_f64().unwrap())
                            .unwrap(),
                    ),
                    other => panic!("unclassified amount {other}"),
                }),
            trigger_offset: number("triggerOffset"),
            trigger_price: number("triggerPrice"),
            valid_until: input.get("validUntil").map(|v| {
                coinbase_starbase_unresolved_valid_until(
                    v["gatewayIntegerTimestamp"].as_f64().unwrap(),
                )
                .unwrap()
            }),
        };
        let actual = match build_coinbase_starbase_edit(&intent) {
            Err(error) => json!({"error":error}),
            Ok(gate) => {
                if let CoinbaseStarbaseWriteGate::Ready { value } = &gate {
                    assert_eq!(
                        value.params.post_only, intent.post_only,
                        "postOnly field identity"
                    );
                    assert_eq!(
                        value.params.reduce_only, intent.reduce_only,
                        "reduceOnly field identity"
                    );
                    assert_eq!(
                        value.params.reject_post_only, intent.reject_post_only,
                        "rejectPostOnly field identity"
                    );
                }
                let mut output = json!(gate);
                if let CoinbaseStarbaseWriteGate::Blocked { intent: borrow, .. } = &gate {
                    assert!(std::ptr::eq(*borrow, &intent));
                    comparison::compare(
                        &output["intent"],
                        &json!(intent),
                        "actual blocked payload before projection",
                    );
                    for (key, number) in [
                        ("price", intent.price),
                        ("triggerOffset", intent.trigger_offset),
                        ("triggerPrice", intent.trigger_price),
                    ] {
                        if let Some(number) = number {
                            special::project_number(&mut output["intent"], key, number);
                        }
                    }
                }
                json!({"ok":output})
            }
        };
        comparison::compare(&actual, &case["result"], &case.to_string());
    }
}
#[test]
fn edit_builder_live_return_contract() {
    let file = syn::parse_file(include_str!("../src/starbase_orders.rs")).unwrap();
    let function = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(f) if f.sig.ident == "build_coinbase_starbase_edit" => Some(f),
            _ => None,
        })
        .unwrap();
    let syn::ReturnType::Type(_, output) = &function.sig.output else {
        panic!("missing return")
    };
    let syn::Type::Path(path) = output.as_ref() else {
        panic!("expected result")
    };
    let segment = path.path.segments.last().unwrap();
    assert_eq!(segment.ident, "Result");
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        panic!("missing result arguments")
    };
    let syn::GenericArgument::Type(syn::Type::Path(gate)) = &args.args[0] else {
        panic!("missing gate")
    };
    let gate = gate.path.segments.last().unwrap();
    let syn::PathArguments::AngleBracketed(args) = &gate.arguments else {
        panic!("missing gate arguments")
    };
    let args: Vec<String> = args
        .args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            let syn::GenericArgument::Type(mut ty) = arg.clone() else {
                panic!("non-type argument")
            };
            if i == 1 {
                let syn::Type::Reference(reference) = ty else {
                    panic!("must borrow original intent")
                };
                assert!(reference.mutability.is_none());
                ty = *reference.elem;
            }
            let syn::Type::Path(path) = ty else {
                panic!("expected payload type")
            };
            assert_eq!(path.path.segments.len(), 1);
            path.path.segments.last().unwrap().ident.to_string()
        })
        .collect();
    assert_eq!(
        json!({"name":gate.ident.to_string(),"args":args}),
        oracle()["returnContract"]
    );
}
