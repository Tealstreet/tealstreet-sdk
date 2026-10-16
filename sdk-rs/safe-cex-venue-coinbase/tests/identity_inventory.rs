use std::collections::{BTreeMap, BTreeSet};
use syn::{
    Ident, Item, LitStr, Token, Type, braced, bracketed,
    ext::IdentExt,
    parse::{Parse, ParseStream},
};

struct RecordCoverage {
    name: String,
    fields: BTreeSet<String>,
}
impl Parse for RecordCoverage {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let name = input.parse::<Ident>()?.to_string();
        let body;
        braced!(body in input);
        let mut fields = BTreeSet::new();
        while !body.is_empty() {
            let _section: Ident = body.parse()?;
            body.parse::<Token![:]>()?;
            let entries;
            bracketed!(entries in body);
            while !entries.is_empty() {
                let field = entries.call(Ident::parse_any)?.to_string();
                fields.insert(field);
                if entries.peek(Token![:]) {
                    entries.parse::<Token![:]>()?;
                    entries.parse::<Type>()?;
                }
                entries.parse::<Token![=>]>()?;
                entries.parse::<LitStr>()?;
                if !entries.is_empty() {
                    entries.parse::<Token![,]>()?;
                }
            }
            if !body.is_empty() {
                body.parse::<Token![;]>()?;
            }
        }
        Ok(Self { name, fields })
    }
}

#[test]
fn native_identity_covers_declared_record_fields() {
    let expected: BTreeMap<_, _> = [
        include_str!("../src/types.rs"),
        include_str!("../src/starbase_cutover.rs"),
    ]
    .into_iter()
    .flat_map(|source| syn::parse_file(source).unwrap().items)
    .filter_map(|item| match item {
        Item::Struct(record) if matches!(record.vis, syn::Visibility::Public(_)) => Some((
            record.ident.to_string(),
            record
                .fields
                .iter()
                .map(|field| field.ident.as_ref().unwrap().to_string())
                .collect::<BTreeSet<_>>(),
        )),
        _ => None,
    })
    .collect();
    assert_eq!(
        expected.len(),
        116,
        "independent native record inventory floor"
    );
    let actual: BTreeMap<_, _> = syn::parse_file(include_str!("support/wire_identity.rs"))
        .unwrap()
        .items
        .into_iter()
        .filter_map(|item| match item {
            Item::Macro(item) if item.mac.path.is_ident("impl_identity") => {
                let coverage: RecordCoverage = syn::parse2(item.mac.tokens).unwrap();
                Some((coverage.name, coverage.fields))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        actual, expected,
        "native field coverage matches production declarations"
    );
}
