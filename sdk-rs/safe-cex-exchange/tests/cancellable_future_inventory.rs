//! Syntax discovery is deliberately separate from reviewed cancellation classifications.
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};
use syn::visit::{self, Visit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Discovered {
    owner: String,
    boundaries: Vec<String>,
    producer_tokens: String,
}

#[derive(Default)]
struct Boundaries(Vec<String>, std::collections::BTreeSet<String>);
impl<'ast> Visit<'ast> for Boundaries {
    fn visit_expr_async(&mut self, node: &'ast syn::ExprAsync) {
        self.0.push("async-block".into());
        visit::visit_expr_async(self, node);
    }
    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        if node.asyncness.is_some() {
            self.0.push("async-closure".into());
        }
        visit::visit_expr_closure(self, node);
    }
    fn visit_expr_await(&mut self, node: &'ast syn::ExprAwait) {
        self.0.push("await".into());
        visit::visit_expr_await(self, node);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*node.func
            && path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "poll_fn")
        {
            self.0.push("poll-fn".into());
        }
        if let syn::Expr::Field(field) = &*node.func
            && matches!(&field.member, syn::Member::Named(name) if name == "spawn")
        {
            self.0.push("host-spawn".into());
        }
        visit::visit_expr_call(self, node);
    }
    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let (syn::Pat::Ident(name), Some(initializer)) = (&node.pat, &node.init)
            && let syn::Expr::MethodCall(borrow) = &*initializer.expr
            && borrow.method == "borrow_mut"
            && matches!(&*borrow.receiver, syn::Expr::Field(field) if matches!(&field.member, syn::Member::Named(name) if name == "wakers" || name == "waiters"))
        {
            self.1.insert(name.ident.to_string());
        }
        visit::visit_local(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if node.method == "push"
            && matches!(&*node.receiver, syn::Expr::Path(path) if path.path.get_ident().is_some_and(|name| self.1.contains(&name.to_string())))
        {
            self.0.push("shared-waiter-registration".into());
        }
        if node.method == "push"
            && matches!(&*node.receiver, syn::Expr::Field(field) if matches!(&field.member, syn::Member::Named(name) if name == "wakers" || name == "waiters"))
        {
            self.0.push("shared-waiter-registration".into());
        }
        if matches!(
            node.method.to_string().as_str(),
            "spawn_local" | "spawn_owned" | "spawn_detached"
        ) {
            self.0.push("owned-or-host-spawn".into());
        }
        visit::visit_expr_method_call(self, node);
    }
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        let known = node.path.segments.last().is_some_and(|segment| {
            matches!(
                segment.ident.to_string().as_str(),
                "pin"
                    | "json"
                    | "format"
                    | "vec"
                    | "matches"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "panic"
                    | "unreachable"
                    | "include_str"
                    | "env"
            )
        });
        if !known {
            self.0.push("opaque-macro-review".into());
        }
        if let Ok(expression) = syn::parse2::<syn::Expr>(node.tokens.clone()) {
            self.visit_expr(&expression);
        } else {
            let tokens = &node.tokens;
            if let Ok(expression) = syn::parse2::<syn::Expr>(quote::quote!([#tokens])) {
                self.visit_expr(&expression);
            } else if tokens.to_string().split_whitespace().any(|token| {
                matches!(
                    token,
                    "async"
                        | "await"
                        | "spawn_local"
                        | "spawn_owned"
                        | "poll_fn"
                        | "Future"
                        | "ExchangeFuture"
                )
            }) {
                self.0.push("unparsed-macro-future-review".into());
            }
        }
    }
    // A nested function is discovered independently rather than charged to its creator.
    fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
}

fn cfg_can_be_production(meta: &syn::Meta) -> (bool, bool) {
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => (true, false),
        syn::Meta::List(list) => {
            use syn::parse::Parser;
            let arguments =
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                    .parse2(list.tokens.clone())
                    .expect("cfg expression must parse");
            let values: Vec<_> = arguments.iter().map(cfg_can_be_production).collect();
            if list.path.is_ident("all") {
                (
                    values.iter().any(|value| value.0),
                    values.iter().all(|value| value.1),
                )
            } else if list.path.is_ident("any") {
                (
                    values.iter().all(|value| value.0),
                    values.iter().any(|value| value.1),
                )
            } else if list.path.is_ident("not") && values.len() == 1 {
                (values[0].1, values[0].0)
            } else {
                (true, true)
            }
        }
        _ => (true, true), // Keep every potentially enabled platform/feature branch.
    }
}
fn test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && !cfg_can_be_production(&attr.parse_args::<syn::Meta>().expect("cfg must parse"))
                    .1)
    })
}

struct Discovery {
    file: String,
    scope: Vec<String>,
    rows: Vec<Discovered>,
    future_names: std::collections::BTreeSet<String>,
}
impl Discovery {
    fn function(&mut self, signature: &syn::Signature, body: &syn::Block) {
        let mut boundaries = Boundaries::default();
        if signature.asyncness.is_some() {
            boundaries.0.push("async-function".into());
        }
        let signature_tokens = signature.to_token_stream().to_string();
        if signature_tokens
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|name| self.future_names.contains(name))
        {
            boundaries.0.push("future-signature".into());
        }
        let output = signature.output.to_token_stream().to_string();
        if output.split_whitespace().any(|token| token == "Self")
            && self.scope.iter().any(|scope| {
                scope
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|token| self.future_names.contains(token))
            })
        {
            boundaries.0.push("future-self-return".into());
        }
        if output
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|name| self.future_names.contains(name))
        {
            boundaries.0.push("future-return".into());
        }
        if signature.ident == "poll" {
            boundaries.0.push("future-poll".into());
        }
        boundaries.visit_block(body);
        if !boundaries.0.is_empty() {
            let mut scope = self.scope.clone();
            scope.push(signature.ident.to_string());
            self.rows.push(Discovered {
                owner: format!("{}::{}", self.file, scope.join("::")),
                boundaries: boundaries.0,
                // Includes guard creation BEFORE async/spawn, not merely the future body.
                producer_tokens: format!(
                    "{} {}",
                    signature.to_token_stream(),
                    body.to_token_stream()
                ),
            });
        }
    }
}
impl<'ast> Visit<'ast> for Discovery {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if test_only(&node.attrs) {
            return;
        }
        let mut scope = self.scope.clone();
        scope.push(format!(
            "opaque-macro:{}:{}",
            node.mac.path.to_token_stream(),
            self.rows.len()
        ));
        self.rows.push(Discovered {
            owner: format!("{}::{}", self.file, scope.join("::")),
            boundaries: vec!["opaque-macro-review".into()],
            producer_tokens: node.to_token_stream().to_string(),
        });
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if test_only(&node.attrs) {
            return;
        }
        self.scope.push(node.ident.to_string());
        visit::visit_item_mod(self, node);
        self.scope.pop();
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if test_only(&node.attrs) {
            return;
        }
        let name = match &node.trait_ {
            Some((trait_, _)) => format!(
                "{} as {}",
                node.self_ty.to_token_stream(),
                trait_.to_token_stream()
            ),
            None => node.self_ty.to_token_stream().to_string(),
        };
        self.scope.push(name);
        visit::visit_item_impl(self, node);
        self.scope.pop();
    }
    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if test_only(&node.attrs) {
            return;
        }
        self.scope.push(node.ident.to_string());
        visit::visit_item_trait(self, node);
        self.scope.pop();
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if !test_only(&node.attrs) {
            self.function(&node.sig, &node.block);
            visit::visit_item_fn(self, node);
        }
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if !test_only(&node.attrs) {
            self.function(&node.sig, &node.block);
            visit::visit_impl_item_fn(self, node);
        }
    }
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        if !test_only(&node.attrs)
            && let Some(body) = &node.default
        {
            self.function(&node.sig, body);
            visit::visit_trait_item_fn(self, node);
        }
    }
}

fn future_names_with_known(
    parsed: &syn::File,
    known: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<String> {
    struct Aliases {
        definitions: Vec<(String, String)>,
        implementations: Vec<String>,
    }
    impl<'ast> Visit<'ast> for Aliases {
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            if !test_only(&node.attrs) {
                visit::visit_item_mod(self, node);
            }
        }
        fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
            if !test_only(&node.attrs) {
                self.definitions.push((
                    node.ident.to_string(),
                    node.ty.to_token_stream().to_string(),
                ));
            }
        }
        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            if test_only(&node.attrs) {
                return;
            }
            if node.trait_.as_ref().is_some_and(|(path, _)| {
                path.segments
                    .last()
                    .is_some_and(|part| part.ident == "Future")
            }) && let syn::Type::Path(path) = &*node.self_ty
            {
                self.implementations
                    .push(path.path.segments.last().unwrap().ident.to_string());
            }
            visit::visit_item_impl(self, node);
        }
    }
    let mut aliases = Aliases {
        definitions: vec![],
        implementations: vec![],
    };
    aliases.visit_file(parsed);
    let mut names = known.clone();
    names.extend(aliases.implementations);
    loop {
        let before = names.len();
        for (name, tokens) in &aliases.definitions {
            if tokens
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|token| names.contains(token))
            {
                names.insert(name.clone());
            }
        }
        if names.len() == before {
            break;
        }
    }
    names
}
fn known_future_names() -> std::collections::BTreeSet<String> {
    ["Future", "ExchangeFuture", "LocalFuture", "RestFuture"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}
fn discover(file: &str, source: &str) -> Vec<Discovered> {
    let parsed = syn::parse_file(source)
        .expect("production source must parse; discovery never skips failures");
    let mut visitor = Discovery {
        file: file.into(),
        scope: vec![],
        rows: vec![],
        future_names: future_names_with_known(&parsed, &known_future_names()),
    };
    visitor.visit_file(&parsed);
    visitor.rows
}

#[test]
fn discovery_sees_creators_poll_futures_and_excludes_test_only_code() {
    let source = r#"
        fn creator() { let guard = pre_pin(); spawn_local(async move { guard.finish(); }); }
        fn polling() -> impl Future<Output=()> { poll_fn(|_| Poll::Pending) }
        async fn awaited() { fetch().await; }
        #[cfg(test)] mod tests { async fn fixture() { pending().await; } }
    "#;
    let rows = discover("fixture.rs", source);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].producer_tokens.contains("pre_pin"));
    assert!(rows[0].boundaries.contains(&"async-block".into()));
    assert!(rows[1].boundaries.contains(&"poll-fn".into()));
    assert!(rows[2].boundaries.contains(&"async-function".into()));
}

fn production_discovery() -> Vec<Discovered> {
    production_discovery_at(Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn production_discovery_at(root: &Path) -> Vec<Discovered> {
    fn modules(
        items: &[syn::Item],
        directory: &Path,
        custom_directory: &Path,
        pending: &mut Vec<std::path::PathBuf>,
    ) {
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            if test_only(&module.attrs) {
                continue;
            }
            if let Some((_, items)) = &module.content {
                let inline = directory.join(module.ident.to_string());
                modules(items, &inline, &inline, pending);
            } else {
                let custom = module.attrs.iter().find_map(|attribute| {
                    if !attribute.path().is_ident("path") {
                        return None;
                    }
                    match &attribute.meta {
                        syn::Meta::NameValue(value) => match &value.value {
                            syn::Expr::Lit(literal) => match &literal.lit {
                                syn::Lit::Str(value) => Some(value.value()),
                                _ => None,
                            },
                            _ => None,
                        },
                        _ => None,
                    }
                });
                let file = if let Some(custom) = custom {
                    custom_directory.join(custom)
                } else {
                    let file = directory.join(format!("{}.rs", module.ident));
                    let nested = directory.join(module.ident.to_string()).join("mod.rs");
                    assert!(
                        !(file.exists() && nested.exists()),
                        "ambiguous production module"
                    );
                    if file.exists() { file } else { nested }
                };
                assert!(
                    file.is_file(),
                    "production module missing: {}",
                    file.display()
                );
                pending.push(file);
            }
        }
    }
    let mut pending = vec![root.join("src/lib.rs")];
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = BTreeMap::new();
    let mut files = BTreeMap::new();
    while let Some(path) = pending.pop() {
        assert!(
            seen.insert(path.clone()),
            "production module cycle or duplicate source root"
        );
        let source = fs::read_to_string(&path).unwrap();
        let file = path.strip_prefix(root).unwrap().to_str().unwrap();
        let parsed = syn::parse_file(&source).unwrap();
        let directory = if matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("lib.rs" | "mod.rs")
        ) {
            path.parent().unwrap().to_path_buf()
        } else {
            path.parent().unwrap().join(path.file_stem().unwrap())
        };
        modules(
            &parsed.items,
            &directory,
            path.parent().unwrap(),
            &mut pending,
        );
        files.insert(file.to_owned(), parsed);
    }
    let mut names = known_future_names();
    loop {
        let before = names.len();
        for parsed in files.values() {
            names.extend(future_names_with_known(parsed, &names.clone()));
        }
        if names.len() == before {
            break;
        }
    }
    for (file, parsed) in files {
        let mut visitor = Discovery {
            file,
            scope: vec![],
            rows: vec![],
            future_names: names.clone(),
        };
        visitor.visit_file(&parsed);
        for row in visitor.rows {
            assert!(
                rows.insert(row.owner.clone(), row).is_none(),
                "discovery owner must be unique"
            );
        }
    }
    rows.into_values().collect()
}

#[test]
#[ignore = "explicit discovery export; never updates reviewed classifications"]
fn export_cancellable_future_discovery() {
    let rows = production_discovery();
    let explicit_destination = std::env::var("CANCELLABLE_DISCOVERY_OUTPUT").ok();
    let destination = explicit_destination.clone().unwrap_or_else(|| {
        std::env::temp_dir()
            .join(format!("core-future-discovery-{}.json", std::process::id()))
            .to_string_lossy()
            .into_owned()
    });
    fs::write(
        &destination,
        serde_json::to_string_pretty(&rows).unwrap() + "\n",
    )
    .unwrap();
    if explicit_destination.is_none() {
        fs::remove_file(destination).unwrap();
    }
}

fn witness_attributes_active(attrs: &[syn::Attribute]) -> bool {
    fn enabled(meta: &syn::Meta) -> Option<bool> {
        match meta {
            syn::Meta::Path(path) if path.is_ident("test") => Some(true),
            syn::Meta::NameValue(value) => {
                let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(name),
                    ..
                }) = &value.value
                else {
                    return None;
                };
                if value.path.is_ident("feature") {
                    return match name.value().as_str() {
                        "health" => Some(cfg!(feature = "health")),
                        "native" => Some(cfg!(feature = "native")),
                        _ => None,
                    };
                }
                (value.path.is_ident("target_arch") && name.value() == "wasm32")
                    .then_some(cfg!(target_arch = "wasm32"))
            }
            syn::Meta::List(list) => {
                use syn::parse::Parser;
                let args =
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                        .parse2(list.tokens.clone())
                        .ok()?;
                let values: Vec<bool> = args.iter().map(enabled).collect::<Option<_>>()?;
                if list.path.is_ident("all") {
                    Some(values.iter().all(|value| *value))
                } else if list.path.is_ident("any") {
                    Some(values.iter().any(|value| *value))
                } else if list.path.is_ident("not") && values.len() == 1 {
                    Some(!values[0])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    attrs.iter().all(|attr| {
        if attr.path().is_ident("ignore") || attr.path().is_ident("cfg_attr") {
            return false;
        }
        if attr.path().is_ident("cfg") {
            return attr
                .parse_args::<syn::Meta>()
                .ok()
                .and_then(|meta| enabled(&meta))
                == Some(true);
        }
        true
    })
}
struct TestFunctions {
    name: String,
    matches: usize,
}
impl<'ast> Visit<'ast> for TestFunctions {
    fn visit_file(&mut self, node: &'ast syn::File) {
        if witness_attributes_active(&node.attrs) {
            visit::visit_file(self, node);
        }
    }
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if witness_attributes_active(&node.attrs) {
            visit::visit_item_mod(self, node);
        }
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if !witness_attributes_active(&node.attrs) {
            return;
        }
        if node.sig.ident == self.name && node.attrs.iter().any(|attr| attr.path().is_ident("test"))
        {
            assert!(
                !node.block.stmts.is_empty(),
                "empty cancellation witness is not coverage"
            );
            self.matches += 1;
        }
        visit::visit_item_fn(self, node);
    }
}

#[test]
fn cancellation_witness_must_be_enabled_and_not_ignored() {
    for (source, expected) in [
        ("#[test] fn evidence() { drop(task); }", 1),
        (
            "#[cfg(feature = \"health\")] #[test] fn evidence() { drop(task); }",
            usize::from(cfg!(feature = "health")),
        ),
        (
            "#[cfg(test)] mod tests { #[test] fn evidence() { drop(task); } }",
            1,
        ),
        (
            "#![cfg(all(feature = \"native\", not(target_arch = \"wasm32\")))] #[test] fn evidence() { drop(task); }",
            usize::from(cfg!(all(feature = "native", not(target_arch = "wasm32")))),
        ),
        (
            "#![cfg(all(feature = \"native\", not(target_arch = \"wasm32\")))] #[cfg(not(feature = \"native\"))] #[test] fn evidence() { drop(task); }",
            0,
        ),
        (
            "#[cfg(not(unknown_gate))] #[test] fn evidence() { drop(task); }",
            0,
        ),
        ("#[test] #[ignore] fn evidence() { drop(task); }", 0),
        (
            "#[test] #[ignore = \"not running\"] fn evidence() { drop(task); }",
            0,
        ),
        ("#[test] #[cfg(any())] fn evidence() { drop(task); }", 0),
        ("#![cfg(any())] #[test] fn evidence() { drop(task); }", 0),
        (
            "#[cfg(any())] mod disabled { #[test] fn evidence() { drop(task); } }",
            0,
        ),
        (
            "#[cfg_attr(test, ignore)] mod hidden { #[test] fn evidence() { drop(task); } }",
            0,
        ),
    ] {
        let mut visitor = TestFunctions {
            name: "evidence".into(),
            matches: 0,
        };
        visitor.visit_file(&syn::parse_file(source).unwrap());
        assert_eq!(
            visitor.matches, expected,
            "inactive cancellation witness must not count: {source}"
        );
    }
}

#[test]
fn production_cancellable_future_classifications_are_current() {
    struct DropImplementations {
        name: String,
        matches: usize,
    }
    impl<'ast> Visit<'ast> for DropImplementations {
        fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
            if !test_only(&node.attrs) {
                visit::visit_item_mod(self, node);
            }
        }
        fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
            if test_only(&node.attrs) {
                return;
            }
            if node.trait_.as_ref().is_some_and(|(path, _)| {
                path.segments
                    .last()
                    .is_some_and(|part| part.ident == "Drop")
            }) && matches!(&*node.self_ty, syn::Type::Path(path) if path.path.segments.last().is_some_and(|part| part.ident == self.name))
            {
                self.matches += 1;
            }
            visit::visit_item_impl(self, node);
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reviewed: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("tests/cancellable-futures.json")).unwrap(),
    )
    .unwrap();
    for row in reviewed["owners"].as_array().unwrap() {
        if row["classification"] == "guarded" {
            for guard in row["guards"]
                .as_array()
                .filter(|guards| !guards.is_empty())
                .expect("guarded owner needs named guard definitions")
            {
                let file = guard["file"].as_str().unwrap();
                assert!(
                    row["source_dependencies"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|dependency| dependency["file"] == file),
                    "guard definition file must be fingerprinted: {file}"
                );
                let parsed =
                    syn::parse_file(&fs::read_to_string(root.join(file)).unwrap()).unwrap();
                let mut definitions = DropImplementations {
                    name: guard["type"].as_str().unwrap().into(),
                    matches: 0,
                };
                definitions.visit_file(&parsed);
                assert_eq!(
                    definitions.matches, 1,
                    "guard must name exactly one real production Drop implementation: {file}::{}",
                    definitions.name
                );
            }
        }
        if matches!(row["classification"].as_str(), Some("guarded" | "finding")) {
            for witness in row["witnesses"]
                .as_array()
                .expect("guarded/finding owner needs witnesses")
            {
                let witness = witness.as_str().unwrap();
                let (file, name) = witness
                    .split_once("::")
                    .expect("witness must name file and test function");
                assert!(file.starts_with("src/") || file.starts_with("tests/"));
                assert!(
                    !Path::new(file)
                        .components()
                        .any(|part| part == std::path::Component::ParentDir)
                );
                let parsed =
                    syn::parse_file(&fs::read_to_string(root.join(file)).unwrap()).unwrap();
                let mut tests = TestFunctions {
                    name: name.rsplit("::").next().unwrap().into(),
                    matches: 0,
                };
                tests.visit_file(&parsed);
                assert_eq!(
                    tests.matches, 1,
                    "cancellation witness must name exactly one enabled non-ignored #[test]: {witness}"
                );
            }
        }
    }
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "core-cancellable-validation-{}.json",
        std::process::id()
    )));
    fs::write(
        &scratch.0,
        serde_json::to_string(&production_discovery()).unwrap(),
    )
    .unwrap();
    let output = std::process::Command::new("node")
        .current_dir(root)
        .args(["scripts/cancellable-futures.cjs", "--check-discovery"])
        .arg(&scratch.0)
        .output()
        .expect("Node required; cancellation gate never skips");
    assert!(
        output.status.success(),
        "production cancellation classification gate failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn discovery_covers_waiter_aliases_macros_and_compound_test_cfg() {
    let source = r#"
        fn alias() -> impl Future<Output=()> {
            poll_fn(|cx| { let mut wakers = self.wakers.borrow_mut(); wakers.push(cx.waker().clone()); Poll::Pending })
        }
        fn literal_macro() { unknown_future_factory!(1); }
        fn in_vec() { let work = vec![async move { fetch().await; }]; }
        #[cfg(all(test, feature="health"))] mod fixtures { async fn ignored() {} }
        #[cfg(any(test, feature="health"))] mod production { async fn enabled() {} }
    "#;
    let rows = discover("fixture.rs", source);
    assert_eq!(
        rows.len(),
        4,
        "test-only conjunction is excluded; potentially enabled branches stay visible"
    );
    assert!(
        rows[0]
            .boundaries
            .contains(&"shared-waiter-registration".into())
    );
    assert!(rows[1].boundaries.contains(&"opaque-macro-review".into()));
    assert!(rows[2].boundaries.contains(&"async-block".into()));
    assert!(rows[3].owner.ends_with("production::enabled"));
}

#[test]
fn discovery_sees_local_future_aliases_nominal_implementations_and_input_only_factories() {
    let rows = discover(
        "fixture.rs",
        r#"
        type Work = Pin<Box<dyn Future<Output=()>>>;
        type Renamed = Work;
        fn alias_factory(work: Renamed) -> Renamed { work }
        fn input_only(work: impl Future<Output=()>) { host.take(work); }
        struct Pending;
        impl Pending { fn new() -> Self { Self } }
        impl Future for Pending { type Output=(); fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> { Poll::Pending } }
        unknown_items!();
    "#,
    );
    assert_eq!(rows.len(), 5);
    assert!(rows[0].boundaries.contains(&"future-return".into()));
    assert!(rows[1].boundaries.contains(&"future-signature".into()));
    assert!(rows[2].boundaries.contains(&"future-self-return".into()));
    assert!(rows[3].boundaries.contains(&"future-poll".into()));
    assert!(rows[4].boundaries.contains(&"opaque-macro-review".into()));
}

#[test]
fn discovery_follows_path_modules_relative_to_declaring_file() {
    let root =
        std::env::temp_dir().join(format!("cancellable-path-fixture-{}", std::process::id()));
    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let fixture = Fixture(root);
    fs::create_dir_all(fixture.0.join("src/outer/inline")).unwrap();
    fs::write(fixture.0.join("src/lib.rs"), "mod outer;").unwrap();
    fs::write(
        fixture.0.join("src/outer.rs"),
        r#"#[path="renamed.rs"] mod child; mod inline { #[path="nested.rs"] mod nested; }"#,
    )
    .unwrap();
    fs::write(fixture.0.join("src/renamed.rs"), "async fn work() {}").unwrap();
    fs::write(
        fixture.0.join("src/outer/inline/nested.rs"),
        "async fn work() {}",
    )
    .unwrap();
    let rows = production_discovery_at(&fixture.0);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.owner == "src/renamed.rs::work"));
    assert!(
        rows.iter()
            .any(|row| row.owner == "src/outer/inline/nested.rs::work")
    );
}
