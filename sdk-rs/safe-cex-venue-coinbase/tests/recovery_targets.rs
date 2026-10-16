use serde_json::Value;
use std::collections::BTreeSet;
use syn::{ImplItem, Item, Type};

fn collect(prefix: &str, items: &[Item], found: &mut BTreeSet<String>) {
    for item in items {
        let name = match item {
            Item::Fn(item) => Some(item.sig.ident.to_string()),
            Item::Struct(item) => Some(item.ident.to_string()),
            Item::Enum(item) => Some(item.ident.to_string()),
            Item::Type(item) => Some(item.ident.to_string()),
            Item::Const(item) => Some(item.ident.to_string()),
            Item::Static(item) => Some(item.ident.to_string()),
            _ => None,
        };
        if let Some(name) = name {
            found.insert(format!("{prefix}::{name}"));
        }
        if let Item::Impl(item) = item {
            if let Type::Path(owner) = item.self_ty.as_ref() {
                let owner = owner.path.segments.last().unwrap().ident.to_string();
                for member in &item.items {
                    if let ImplItem::Fn(method) = member {
                        found.insert(format!("{prefix}::{owner}::{}", method.sig.ident));
                    }
                }
            }
        }
    }
}

#[test]
fn recovered_off_pending_rows_resolve_to_native_rust_items() {
    let audit: Value = serde_json::from_str(include_str!("../RECOVERY_ROWS.json")).unwrap();
    let rows = audit["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 25, "independent recovered row floor");
    let mut found = BTreeSet::new();
    let mut verified = 0;
    for row in rows {
        let targets = match &row["rs"] {
            Value::String(target) => vec![target.as_str()],
            Value::Array(targets) => targets
                .iter()
                .map(|target| target.as_str().unwrap())
                .collect(),
            other => panic!("invalid target {other}"),
        };
        assert!(
            !targets.is_empty(),
            "recovered row must have a target: {}",
            row["ts"]
        );
        for target in targets {
            let (file, _) = target.split_once("::").unwrap();
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .join(file),
            )
            .unwrap();
            collect(file, &syn::parse_file(&source).unwrap().items, &mut found);
            assert!(
                found.contains(target),
                "recovered target has no native Rust item: {} -> {target}",
                row["ts"]
            );
        }
        assert_ne!(row["merged_status"], "pending");
        verified += 1;
    }
    assert_eq!(verified, 25, "independent verified target row floor");
}
