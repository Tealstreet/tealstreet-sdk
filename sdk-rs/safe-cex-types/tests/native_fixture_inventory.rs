use quote::ToTokens;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use syn::visit::Visit;

#[derive(Default)]
struct Inventory {
    files: usize,
    functions: usize,
    native_records: usize,
    json_records: usize,
    equal_groups: usize,
    dynamic_json_macros: usize,
    dynamic_buckets: BTreeMap<&'static str, usize>,
    bit_functions: Vec<String>,
    current: String,
}
impl Inventory {
    fn groups(&mut self, fields: impl Iterator<Item = (String, String)>) {
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, value) in fields {
            groups.entry(value).or_default().push(name);
        }
        for names in groups.values().filter(|names| names.len() > 1) {
            self.equal_groups += 1;
            eprintln!("NATIVE-FIXTURE {} equal candidates {names:?}", self.current);
        }
    }
    fn json(&mut self, value: &Value) {
        match value {
            Value::Object(fields) => {
                self.json_records += 1;
                self.groups(
                    fields
                        .iter()
                        .filter(|(_, value)| {
                            value.is_boolean() || value.is_number() || value.is_string()
                        })
                        .map(|(key, value)| (key.clone(), value.to_string())),
                );
                for value in fields.values() {
                    self.json(value);
                }
            }
            Value::Array(values) => {
                for value in values {
                    self.json(value);
                }
            }
            _ => {}
        }
    }
}
impl<'ast> Visit<'ast> for Inventory {
    fn visit_item_fn(&mut self, function: &'ast syn::ItemFn) {
        self.functions += 1;
        self.current = function.sig.ident.to_string();
        let body = function.block.to_token_stream().to_string();
        if body.contains("to_bits") || body.contains("from_bits") {
            self.bit_functions.push(self.current.clone());
        }
        syn::visit::visit_item_fn(self, function);
    }
    fn visit_impl_item_fn(&mut self, function: &'ast syn::ImplItemFn) {
        self.functions += 1;
        self.current = function.sig.ident.to_string();
        let body = function.block.to_token_stream().to_string();
        if body.contains("to_bits") || body.contains("from_bits") {
            self.bit_functions.push(self.current.clone());
        }
        syn::visit::visit_impl_item_fn(self, function);
    }
    fn visit_expr_struct(&mut self, record: &'ast syn::ExprStruct) {
        self.native_records += 1;
        self.groups(record.fields.iter().map(|field| {
            (
                field.member.to_token_stream().to_string(),
                field.expr.to_token_stream().to_string(),
            )
        }));
        syn::visit::visit_expr_struct(self, record);
    }
    fn visit_macro(&mut self, value: &'ast syn::Macro) {
        if value
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "json")
        {
            match serde_json::from_str::<Value>(&value.tokens.to_string()) {
                Ok(value) => self.json(&value),
                Err(_) => {
                    self.dynamic_json_macros += 1;
                    let bucket = dynamic_bucket(&self.current);
                    *self.dynamic_buckets.entry(bucket).or_default() += 1;
                    eprintln!("NATIVE-FIXTURE dynamic {} {}", self.current, value.tokens);
                }
            }
        }
        syn::visit::visit_macro(self, value);
    }
}
fn dynamic_bucket(function: &str) -> &'static str {
    match function {
        "decoded_task_snapshots_use_distinct_dimensions_and_one_hot_flags"
        | "record"
        | "nullable_index_ticker_fixture"
        | "roundtrip"
        | "ticker_derivative_sentinels_are_required_and_preserve_fractional_numbers"
        | "recursive_balances_and_execution_legs_keep_absence_distinct_from_zero"
        | "update_order_union_retains_both_declared_fields"
        | "snapshot_health_flags_and_leverage_keep_native_identity"
        | "market"
        | "order"
        | "boxed_ticker_keeps_native_fields_and_untagged_wire_shapes"
        | "market_perpetual_native_boolean_and_absence"
        | "market_option_terms_and_price_limit_decode_natively_and_round_trip"
        | "position_greeks_order_iv_and_market_toggles_decode_natively"
        | "inherited_instrument_members_keep_native_identity_tags" => "decoded-record",
        "union" | "reject_shapes" | "string_union" => "rejected-shape",
        "current_source_accounts"
        | "model_utilities_typescript_differential"
        | "symbol_conversion_typescript_differential"
        | "actual_validation_method_retains_non_string_error_values"
        | "native_unknown_send_veto_response_precedence_and_codes" => "operation-result",
        "tagged"
        | "encoded"
        | "encoded_number"
        | "json_comparator_ignores_only_object_key_order"
        | "live_typescript_risk_and_returns"
        | "fill_increments_match_actual_typescript_at_venue_decimals"
        | "rest_native_signed_zero_extrema" => "numeric-observation",
        "classification_retains_unknown_and_bybit_wire_values"
        | "javascript_trim_and_prototype_lookup_bugs_are_preserved"
        | "first_subscriber_failure_surfaces_after_committed_setting_and_skips_fanout"
        | "full_store_typescript_differential"
        | "encoded_book"
        | "sequenced_book_live_typescript_every_value_and_transition"
        | "frozen_typescript_terminal_oracle"
        | "prototype_risk_error_is_catchable_and_store_clearing_is_observable"
        | "production_dictionary_write_mechanisms_and_safe_local_outcomes"
        | "redaction_ordinary_assignment_omits_prototype_key" => "effect-observation",
        _ => panic!("DYNAMIC-DISPOSITION unclassified macro in {function}"),
    }
}
fn visit_inline_tests(inventory: &mut Inventory, items: &[syn::Item]) {
    for item in items {
        if let syn::Item::Mod(module) = item {
            if let Some((_, children)) = &module.content {
                if module.ident == "tests"
                    || module.attrs.iter().any(|attribute| {
                        attribute
                            .to_token_stream()
                            .to_string()
                            .contains("cfg (test)")
                    })
                {
                    for child in children {
                        inventory.visit_item(child);
                    }
                } else {
                    visit_inline_tests(inventory, children);
                }
            }
        } else if let syn::Item::Fn(function) = item
            && function
                .attrs
                .iter()
                .any(|attribute| attribute.path().is_ident("test"))
        {
            inventory.visit_item_fn(function);
        }
    }
}
fn collect_rs(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rs(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
#[test]
fn parse_native_test_fixtures_and_bit_comparison_functions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut inventory = Inventory::default();
    for directory in [
        root.join("tests"),
        root.join("../safe-cex-store/tests"),
        root.join("src"),
        root.join("../safe-cex-store/src"),
    ] {
        let mut files = Vec::new();
        collect_rs(&directory, &mut files);
        files.sort();
        for path in files {
            let syntax = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
            inventory.files += 1;
            eprintln!(
                "NATIVE-FIXTURE file {}",
                path.file_name().unwrap().to_string_lossy()
            );
            if path.components().any(|part| part.as_os_str() == "tests") {
                inventory.visit_file(&syntax);
            } else {
                visit_inline_tests(&mut inventory, &syntax.items);
            }
        }
    }
    // The canonical Store owner moved to exchange-core. Follow its existing
    // fixtures rather than lowering the reviewed observation bucket counts.
    for relative in [
        "../safe-cex-exchange/tests/store_bucket_witnesses.rs",
        "../safe-cex-exchange/tests/store_clone_construction.rs",
        "../safe-cex-exchange/tests/store_contract_runtime.rs",
        "../safe-cex-exchange/tests/store_notification.rs",
        "../safe-cex-exchange/tests/store_p4_store.rs",
        "../safe-cex-exchange/tests/store_patch_info.rs",
        "../safe-cex-exchange/tests/store_ticker_timestamp.rs",
        "../safe-cex-exchange/src/js_runtime/native_store/legacy_owner_tests.rs",
        "../safe-cex-exchange/src/js_runtime/native_store/legacy_request_tests.rs",
        "../safe-cex/tests/native_store_patches.rs",
        "../safe-cex/tests/risk_recovery.rs",
        "../safe-cex/tests/dictionary_write.rs",
        "../safe-cex/tests/position_record_numbers.rs",
    ] {
        let syntax =
            syn::parse_file(&std::fs::read_to_string(root.join(relative)).unwrap()).unwrap();
        inventory.files += 1;
        inventory.visit_file(&syntax);
    }
    eprintln!(
        "NATIVE-FIXTURE counts {}",
        json!({"files":inventory.files,"functions":inventory.functions,"native_records":inventory.native_records,"literal_json_records":inventory.json_records,"equal_groups":inventory.equal_groups,"dynamic_json_macros":inventory.dynamic_json_macros,"bit_functions":inventory.bit_functions})
    );
    assert!(
        inventory.files > 20,
        "NATIVE-FIXTURE both crate test directories parsed"
    );
    assert_eq!(
        inventory.dynamic_buckets,
        BTreeMap::from([
            ("decoded-record", 19),
            ("effect-observation", 19),
            ("numeric-observation", 12),
            ("operation-result", 16),
            ("rejected-shape", 17),
        ]),
        "DYNAMIC-DISPOSITION complete bucket coverage"
    );
}
