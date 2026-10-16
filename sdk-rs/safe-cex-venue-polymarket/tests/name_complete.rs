use safe_cex_types::ExchangeName;
use safe_cex_venue_polymarket::spot_contracts::name;
use std::process::Command;
use syn::{Expr, Fields, Item, PathArguments, ReturnType, Stmt, Type};

fn native_body_has_one_closed_value() {
    let file = syn::parse_file(include_str!("../src/spot_contracts.rs")).expect("parse Rust body");
    let functions: Vec<_> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(item) if item.sig.ident == "name" => Some(item),
            _ => None,
        })
        .collect();
    assert_eq!(functions.len(), 1, "name_complete_rust_body_schema");
    let function = functions[0];
    assert!(
        function.attrs.iter().all(|a| a.path().is_ident("doc")),
        "name_complete_rust_body_schema"
    );
    let signature = &function.sig;
    assert!(
        signature.inputs.is_empty()
            && signature.generics.params.is_empty()
            && signature.generics.where_clause.is_none()
            && signature.constness.is_none()
            && signature.asyncness.is_none()
            && matches!(signature.safety, syn::Safety::Default)
            && signature.abi.is_none()
            && signature.variadic.is_none(),
        "name_complete_rust_body_schema"
    );
    let ReturnType::Type(_, return_type) = &signature.output else {
        panic!("name_complete_rust_body_schema")
    };
    let Type::Path(return_type) = return_type.as_ref() else {
        panic!("name_complete_rust_body_schema")
    };
    assert!(
        return_type.qself.is_none() && return_type.path.is_ident("ExchangeName"),
        "name_complete_rust_body_schema"
    );
    let [Stmt::Expr(Expr::Path(value), None)] = function.block.stmts.as_slice() else {
        panic!("name_complete_rust_body_schema")
    };
    assert!(
        value.attrs.is_empty()
            && value.qself.is_none()
            && value.path.leading_colon.is_none()
            && value.path.segments.len() == 2,
        "name_complete_rust_body_schema"
    );
    assert!(
        value.path.segments[0].ident == "ExchangeName"
            && value
                .path
                .segments
                .iter()
                .all(|s| matches!(s.arguments, PathArguments::None)),
        "name_complete_rust_body_schema"
    );
    let types = syn::parse_file(include_str!("../../safe-cex-types/src/lib.rs"))
        .expect("parse actual shared enum");
    let enums: Vec<_> = types
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Enum(item) if item.ident == "ExchangeName" => Some(item),
            _ => None,
        })
        .collect();
    assert_eq!(enums.len(), 1, "name_complete_actual_native_enum");
    let variant = enums[0]
        .variants
        .iter()
        .find(|v| v.ident == value.path.segments[1].ident)
        .expect("name_complete_actual_native_variant");
    assert!(
        matches!(variant.fields, Fields::Unit) && variant.discriminant.is_none(),
        "name_complete_actual_native_unit_value"
    );
}

#[test]
fn name_complete_current_runtime() {
    native_body_has_one_closed_value();
    let native = name();
    // Independent native variant assertion comes before Node capture or any wire assertion.
    assert!(
        matches!(&native, ExchangeName::Polymarket),
        "name_complete_native_value"
    );

    let output = Command::new("node")
        .args(["--import", "tsx"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/name_complete.mjs"
        ))
        .output()
        .expect("Node runtime required");
    assert!(
        output.status.success(),
        "name_complete_source_runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).expect("source capture");
    eprintln!(
        "name complete runtime: {} sourceSha256={}",
        source["capture"], source["sourceSha256"]
    );
    assert_eq!(
        source["proofClasses"], 1,
        "name_complete_closed_body_classes"
    );
    assert_eq!(
        source["receiverWitnesses"], 21,
        "name_complete_independent_receiver_floor"
    );
    assert_eq!(source["calls"], 42, "name_complete_independent_call_floor");
    assert_eq!(source["traps"], 0, "name_complete_no_effects");
    assert_eq!(
        source["knownDifferenceFilters"], 0,
        "name_complete_no_difference_filters"
    );
    let ids = [
        "undefined",
        "null",
        "false",
        "true",
        "positive_zero",
        "negative_zero",
        "nan",
        "positive_infinity",
        "negative_infinity",
        "number",
        "bigint",
        "symbol",
        "empty_string",
        "ascii_string",
        "lone_surrogate_string",
        "object",
        "null_proto_object",
        "array",
        "function",
        "poison_proxy",
        "revoked_proxy",
    ];
    let cases = source["cases"].as_array().expect("source cases");
    assert_eq!(cases.len(), 42, "name_complete_independent_capture_floor");
    for (i, id) in ids.iter().enumerate() {
        for (j, arguments) in ["no_args", "extra_args"].iter().enumerate() {
            let case = &cases[i * 2 + j];
            assert_eq!(
                case["id"],
                format!("{id}/{arguments}"),
                "name_complete_case_identity"
            );
            assert_eq!(case["type"], "string", "name_complete_source_native_type");
            assert_eq!(
                case["result"], "polymarket",
                "name_complete_source_native_value"
            );
        }
    }
    // Supplementary representation check ONLY after all native/source assertions.
    assert_eq!(
        serde_json::to_value(native).expect("wire tag"),
        "polymarket",
        "name_complete_wire_tag"
    );
}
