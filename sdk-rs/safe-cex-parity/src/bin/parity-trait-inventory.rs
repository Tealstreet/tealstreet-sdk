use std::{collections::BTreeMap, fs, path::Path};

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

fn collect(items: &[syn::Item], prefix: &str, output: &mut BTreeMap<String, &'static str>) {
    for item in items {
        match item {
            syn::Item::Trait(item)
                if !test_only(&item.attrs) && matches!(item.vis, syn::Visibility::Public(_)) =>
            {
                let owner = format!("{prefix}{}", item.ident);
                output.insert(owner.clone(), "trait");
                for member in &item.items {
                    let name = match member {
                        syn::TraitItem::Fn(item) if !test_only(&item.attrs) => {
                            item.sig.ident.to_string()
                        }
                        syn::TraitItem::Const(item) if !test_only(&item.attrs) => {
                            item.ident.to_string()
                        }
                        syn::TraitItem::Type(item) if !test_only(&item.attrs) => {
                            item.ident.to_string()
                        }
                        _ => continue,
                    };
                    output.insert(format!("{owner}::{name}"), "trait-member");
                }
            }
            syn::Item::Mod(item) if !test_only(&item.attrs) => {
                if let Some((_, items)) = &item.content {
                    collect(items, &format!("{prefix}{}::", item.ident), output);
                }
            }
            _ => {}
        }
    }
}

fn run(workspace: &Path) -> Result<(), String> {
    let index = safe_cex_parity::rust_index(workspace)?;
    let files: std::collections::BTreeSet<_> = index
        .all
        .iter()
        .filter_map(|target| target.split_once("::").map(|(file, _)| file))
        .collect();
    for file in files {
        let path = workspace.join(file);
        let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let ast = syn::parse_file(&source).map_err(|error| error.to_string())?;
        let mut declarations = BTreeMap::new();
        collect(&ast.items, &format!("{file}::"), &mut declarations);
        for (target, kind) in declarations {
            // Explicit test-only filtering above prevents nominal/trait ID collisions.
            // The maintained public index decides remaining reachability and visibility.
            if index.public.contains(&target) {
                println!("{kind}\t{target}");
            }
        }
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = if args.len() == 1 {
        run(Path::new(&args[0]))
    } else {
        Err("Usage: parity-trait-inventory <sdk-workspace>".into())
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traits_and_members_have_independent_native_identities() {
        let ast = syn::parse_file(
            "pub trait Host { type Value; const LIMIT: usize; fn send(&self); }
             pub struct Receiver; impl Receiver { pub fn send(&self) {} }
             #[cfg(test)] pub trait Receiver { fn send(&self); }
             trait Restricted { fn send(&self); }
             pub struct Restricted;
             #[cfg(test)] mod ignored { pub trait Host { fn send(&self); } }
             mod nested { pub trait Host { fn close(&self); } }",
        )
        .unwrap();
        let mut actual = BTreeMap::new();
        collect(&ast.items, "fixture.rs::", &mut actual);
        let expected = BTreeMap::from([
            ("fixture.rs::Host".into(), "trait"),
            ("fixture.rs::Host::Value".into(), "trait-member"),
            ("fixture.rs::Host::LIMIT".into(), "trait-member"),
            ("fixture.rs::Host::send".into(), "trait-member"),
            ("fixture.rs::nested::Host".into(), "trait"),
            ("fixture.rs::nested::Host::close".into(), "trait-member"),
        ]);
        assert_eq!(
            actual, expected,
            "TRAITS exact native declaration identities"
        );
    }
}
