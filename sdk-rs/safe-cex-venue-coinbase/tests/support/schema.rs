use serde_json::{Value, json};
use std::collections::HashMap;
use syn::{GenericArgument, GenericParam, Item, LitStr, PathArguments, Type, TypeParamBound};

// Compare the Rust declarations as well as decoding TS examples: narrowing TS
// must detect a stale Rust optional field or enum variant that no sample visits.
pub fn wire_schema(source: &str, name: &str) -> Value {
    wire_schema_from_sources(&[source], name)
}

pub fn wire_schema_from_sources(sources: &[&str], name: &str) -> Value {
    let sources: Vec<_> = sources
        .iter()
        .map(|source| syn::parse_file(source).unwrap())
        .collect();
    let declarations: Vec<_> = sources
        .iter()
        .flat_map(|source| source.items.iter())
        .filter_map(|item| match item {
            Item::Struct(s) => Some((s.ident.to_string(), item)),
            Item::Enum(e) => Some((e.ident.to_string(), item)),
            Item::Type(t) => Some((t.ident.to_string(), item)),
            _ => None,
        })
        .collect();
    let count = declarations.len();
    let items: HashMap<_, _> = declarations.into_iter().collect();
    assert_eq!(
        items.len(),
        count,
        "ambiguous cross-module schema declarations"
    );
    named(name, &items)
}
fn named(name: &str, items: &HashMap<String, &Item>) -> Value {
    match name {
        "Method" | "Params" | "T" | "Gateway" | "Intent" => {
            return json!({"kind":"parameter", "name":name});
        }
        "Value" => return json!({"kind":"unknown"}),
        "str" | "String" => return json!({"kind":"string"}),
        "f64" => return json!({"kind":"number"}),
        "bool" => return json!({"kind":"boolean"}),
        _ => {}
    }
    match items[name] {
        Item::Struct(s) if s.ident == "True" => {
            assert!(matches!(s.fields, syn::Fields::Unit));
            json!({"kind":"enum","values":[true]})
        }
        Item::Struct(s) => {
            let mut schema = json!({"kind":"object", "fields": s.fields.iter().map(|field| {
            let field_name = field.ident.as_ref().unwrap().to_string();
            let mut field_name = field_name.strip_prefix("r#").unwrap_or(&field_name).to_owned();
            for attr in &field.attrs {
                if attr.path().is_ident("serde") {
                    attr.parse_nested_meta(|meta| {
                        if meta.path.is_ident("rename") { field_name = meta.value()?.parse::<LitStr>()?.value(); }
                        else if meta.input.peek(syn::Token![=]) { let _: syn::Expr = meta.value()?.parse()?; }
                        Ok(())
                    }).unwrap();
                }
            }
            let optional = argument(&field.ty,"Option");
            json!({"name": field_name, "optional": optional.is_some(), "schema": shape(optional.unwrap_or(&field.ty),items)})
        }).collect::<Vec<_>>()});
            if s.generics
                .params
                .iter()
                .any(|p| !matches!(p, GenericParam::Lifetime(_)))
            {
                assert!(
                    s.generics.where_clause.is_none(),
                    "unhandled where constraint"
                );
                schema["parameters"] = json!(s.generics.params.iter().filter(|p| !matches!(p, GenericParam::Lifetime(_))).map(|parameter| {
                    let GenericParam::Type(parameter) = parameter else { panic!("non-type generic") };
                    assert_eq!(parameter.bounds.len(), 1, "generic must retain one wire constraint");
                    let TypeParamBound::Trait(bound) = &parameter.bounds[0] else {panic!("non-trait constraint")};
                    assert!(bound.lifetimes.is_none() && bound.paren_token.is_none());
                    bound.modifiers.require_empty().unwrap();
                    assert!(bound.maybe.is_none());
                    assert_eq!(bound.path.segments.len(), 1);
                    let trait_name = &bound.path.segments[0].ident;
                    assert!(trait_name == "AsRef" || trait_name == "Into", "unsupported wire constraint");
                    let PathArguments::AngleBracketed(arguments) = &bound.path.segments[0].arguments else {panic!("missing constraint")};
                    assert_eq!(arguments.args.len(), 1);
                    let GenericArgument::Type(constraint) = &arguments.args[0] else {panic!("non-type constraint")};
                    json!({"name":parameter.ident.to_string(),"constraint":shape(constraint,items),"default":parameter.default.as_ref().map(|(_, ty)|shape(ty,items))})
                }).collect::<Vec<_>>());
            }
            schema
        }
        Item::Enum(e) if e.ident == "CoinbaseStarbaseFalse" => {
            assert_eq!(e.variants.len(), 1);
            assert_eq!(e.variants[0].ident, "False");
            json!({"kind":"enum","values":[false]})
        }
        Item::Enum(e) if e.ident == "CoinbaseStarbaseOrderBookDepth" => {
            let mut representation = None;
            for attr in &e.attrs {
                if attr.path().is_ident("serde") {
                    attr.parse_nested_meta(|meta| {
                        assert!(
                            meta.path.is_ident("into"),
                            "unclassified numeric serialization"
                        );
                        representation = Some(meta.value()?.parse::<LitStr>()?.value());
                        Ok(())
                    })
                    .unwrap();
                }
            }
            assert_eq!(representation.as_deref(), Some("u32"));
            json!({"kind":"enum","values":e.variants.iter().map(|v| {
                assert!(v.attrs.is_empty(), "unclassified numeric variant attribute");
                let (_, syn::Expr::Lit(value)) = v.discriminant.as_ref().unwrap() else { panic!("numeric literal discriminant") };
                let syn::Lit::Int(value) = &value.lit else { panic!("integer discriminant") };
                value.base10_parse::<u32>().unwrap()
            }).collect::<Vec<_>>()})
        }
        Item::Enum(e)
            if e.variants
                .iter()
                .any(|v| !matches!(v.fields, syn::Fields::Unit)) =>
        {
            let mut tag = None;
            let mut untagged = false;
            for attribute in &e.attrs {
                if attribute.path().is_ident("serde") {
                    attribute
                        .parse_nested_meta(|meta| {
                            if meta.path.is_ident("tag") {
                                tag = Some(meta.value()?.parse::<LitStr>()?.value());
                            } else {
                                assert!(
                                    meta.path.is_ident("untagged"),
                                    "unclassified union representation"
                                );
                                untagged = true;
                            }
                            Ok(())
                        })
                        .unwrap();
                }
            }
            assert_ne!(
                tag.is_some(),
                untagged,
                "union must select exactly one representation"
            );
            let variants: Vec<Value> = e.variants.iter().map(|variant| {
                if untagged {
                    assert!(variant.attrs.is_empty(), "unclassified untagged variant attribute");
                    let syn::Fields::Unnamed(fields) = &variant.fields else { panic!("newtype union branch required") };
                    assert_eq!(fields.unnamed.len(), 1);
                    shape(&fields.unnamed[0].ty, items)
                } else {
                    let mut value = None;
                    for attribute in &variant.attrs {
                        assert!(attribute.path().is_ident("serde"));
                        attribute.parse_nested_meta(|meta| {
                            assert!(meta.path.is_ident("rename"));
                            value = Some(meta.value()?.parse::<LitStr>()?.value());
                            Ok(())
                        }).unwrap();
                    }
                    let mut fields = vec![json!({"name":tag,"optional":false,"schema":{"kind":"enum","values":[value.unwrap()]}})];
                    let syn::Fields::Named(named) = &variant.fields else {panic!("named tagged fields required")};
                    for field in &named.named {
                        assert!(field.attrs.is_empty(), "unclassified tagged field attribute");
                        fields.push(json!({"name":field.ident.as_ref().unwrap().to_string(),"optional":false,"schema":shape(&field.ty,items)}));
                    }
                    json!({"kind":"object","fields":fields})
                }
            }).collect();
            let mut result = json!({"kind":"union","variants":variants});
            if !e.generics.params.is_empty() {
                assert!(e.generics.where_clause.is_none());
                result["parameters"] = json!(e.generics.params.iter().map(|parameter| {
                    let GenericParam::Type(parameter) = parameter else {panic!("unclassified generic union parameter")};
                    assert!(parameter.bounds.is_empty(), "unclassified union constraint");
                    json!({"name":parameter.ident.to_string(),"constraint":null,"default":parameter.default.as_ref().map(|(_,ty)|shape(ty,items))})
                }).collect::<Vec<_>>());
            }
            result
        }
        Item::Enum(e) => json!({"kind":"enum", "values": e.variants.iter().map(|variant| {
            let mut rename = None;
            for attr in &variant.attrs {
                if attr.path().is_ident("serde") {
                    attr.parse_nested_meta(|meta| {
                        assert!(meta.path.is_ident("rename"), "unhandled serde enum attribute");
                        rename = Some(meta.value()?.parse::<LitStr>()?.value());
                        Ok(())
                    }).unwrap();
                }
            }
            rename.expect("wire variants require explicit spelling")
        }).collect::<Vec<_>>()}),
        Item::Type(t) => shape(&t.ty, items),
        _ => panic!("unsupported Rust declaration {name}"),
    }
}
fn argument<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    if let Type::Path(p) = ty {
        let last = p.path.segments.last().unwrap();
        if last.ident == name
            && let PathArguments::AngleBracketed(args) = &last.arguments
            && let GenericArgument::Type(inner) = args.args.first().unwrap()
        {
            return Some(inner);
        }
    }
    None
}
fn shape(ty: &Type, items: &HashMap<String, &Item>) -> Value {
    if let Type::Array(array) = ty {
        let syn::Expr::Lit(length) = &array.len else {
            panic!("literal tuple length required")
        };
        let syn::Lit::Int(length) = &length.lit else {
            panic!("integer tuple length required")
        };
        return json!({"kind":"tuple","items":vec![shape(&array.elem,items);length.base10_parse::<usize>().unwrap()]});
    }
    if let Type::Reference(reference) = ty {
        assert!(
            reference.mutability.is_none(),
            "wire references must be immutable"
        );
        return shape(&reference.elem, items);
    }
    if let Type::Path(p) = ty {
        let last = p.path.segments.last().unwrap();
        if last.ident == "Map" {
            let PathArguments::AngleBracketed(args) = &last.arguments else {
                panic!("record arguments")
            };
            assert_eq!(args.args.len(), 2);
            let shapes: Vec<_> = args
                .args
                .iter()
                .map(|arg| {
                    let GenericArgument::Type(ty) = arg else {
                        panic!("record type")
                    };
                    shape(ty, items)
                })
                .collect();
            return json!({"kind":"record","key":shapes[0],"value":shapes[1]});
        }
    }
    if let Some(inner) = argument(ty, "Vec") {
        return json!({"kind":"array","item":shape(inner,items)});
    }
    if let Type::Path(p) = ty {
        return named(&p.path.segments.last().unwrap().ident.to_string(), items);
    }
    panic!("unsupported Rust type in wire schema")
}
