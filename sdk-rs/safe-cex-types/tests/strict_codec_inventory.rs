use quote::ToTokens;
use syn::{Attribute, Fields, Item};

fn serde_attributes(attributes: &[Attribute]) -> Vec<String> {
    attributes
        .iter()
        .filter(|a| {
            a.path().is_ident("serde") && !a.to_token_stream().to_string().contains("remote")
        })
        .map(|a| a.to_token_stream().to_string())
        .collect()
}
#[test]
fn private_companions_cannot_drift_from_canonical_definitions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for relative in [
        "src/lib.rs",
        "src/store_model.rs",
        "src/extra_model.rs",
        "src/accounts.rs",
        "src/gateway.rs",
        "../safe-cex-store/src/contract.rs",
    ] {
        let file = syn::parse_file(&std::fs::read_to_string(root.join(relative)).unwrap()).unwrap();
        let module = file
            .items
            .iter()
            .find_map(|item| match item {
                Item::Mod(m) if m.ident == "strict_codecs" => Some(m),
                _ => None,
            })
            .unwrap();
        assert!(
            matches!(module.vis, syn::Visibility::Inherited),
            "{relative}: companions must be private"
        );
        let wire = &module.content.as_ref().unwrap().1;
        let mut count = 0;
        for original in &file.items {
            let (name, attributes, fields, variants) = match original {
                Item::Struct(s) if matches!(s.fields, Fields::Named(_)) => {
                    (&s.ident, &s.attrs, Some(&s.fields), None)
                }
                Item::Enum(e)
                    if !e.variants.is_empty()
                        && e.variants.iter().all(|v| matches!(v.fields, Fields::Unit)) =>
                {
                    (&e.ident, &e.attrs, None, Some(&e.variants))
                }
                _ => continue,
            };
            if !attributes.iter().any(|a| {
                a.path().is_ident("derive") && a.to_token_stream().to_string().contains("Serialize")
            }) {
                continue;
            }
            assert!(
                !attributes.iter().any(|a| a.path().is_ident("derive")
                    && a.to_token_stream().to_string().contains("Deserialize")),
                "{relative}:{name}: public derive reopens broad inputs"
            );
            // These outputs intentionally have no Deserialize implementation.
            if matches!(
                name.to_string().as_str(),
                "ExchangeAccountConfigError" | "ParseImportResult"
            ) {
                continue;
            }
            let expected = format!("Wire{name}");
            let companion = wire
                .iter()
                .find(|item| match item {
                    Item::Struct(s) => s.ident == expected,
                    Item::Enum(e) => e.ident == expected,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("{relative}:{name}: missing private companion"));
            let (visibility, attrs) = match companion {
                Item::Struct(s) => (&s.vis, &s.attrs),
                Item::Enum(e) => (&e.vis, &e.attrs),
                _ => unreachable!(),
            };
            assert!(matches!(visibility, syn::Visibility::Inherited));
            assert_eq!(
                serde_attributes(attributes),
                serde_attributes(attrs),
                "{relative}:{name}: container metadata drift"
            );
            if let (Some(Fields::Named(a)), Item::Struct(b)) = (fields, companion) {
                let Fields::Named(b) = &b.fields else {
                    panic!("record became a tuple")
                };
                assert_eq!(a.named.len(), b.named.len());
                for (a, b) in a.named.iter().zip(&b.named) {
                    assert_eq!(a.ident, b.ident);
                    assert_eq!(
                        a.ty.to_token_stream().to_string(),
                        b.ty.to_token_stream().to_string()
                    );
                    assert_eq!(
                        serde_attributes(&a.attrs),
                        serde_attributes(&b.attrs),
                        "{relative}:{name}: field metadata drift"
                    );
                }
            }
            if let (Some(a), Item::Enum(b)) = (variants, companion) {
                assert_eq!(a.len(), b.variants.len());
                for (a, b) in a.iter().zip(&b.variants) {
                    assert_eq!(a.ident, b.ident);
                    assert_eq!(
                        serde_attributes(&a.attrs),
                        serde_attributes(&b.attrs),
                        "{relative}:{name}: variant metadata drift"
                    );
                }
            }
            count += 1;
        }
        assert!(count > 0);
    }
}
