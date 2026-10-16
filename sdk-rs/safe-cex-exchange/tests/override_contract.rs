//! Source override coverage and the known base-dispatch escape hatch are separate gates.
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};
use syn::visit::Visit;

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}
fn source_methods(rows: &serde_json::Value) -> BTreeSet<String> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|row| row["method"].as_str().unwrap().to_owned())
        .collect()
}
fn trait_source() -> syn::ItemTrait {
    syn::parse_file(include_str!("../src/exchange.rs"))
        .unwrap()
        .items
        .into_iter()
        .find_map(|item| match item {
            syn::Item::Trait(item) if item.ident == "Exchange" => Some(item),
            _ => None,
        })
        .expect("Exchange trait must exist")
}
fn assert_hooks(rows: &serde_json::Value, exchange: &syn::ItemTrait) {
    for source in source_methods(rows) {
        let method = exchange
            .items
            .iter()
            .find_map(|item| match item {
                syn::TraitItem::Fn(method)
                    if normalize(&method.sig.ident.to_string()) == normalize(&source) =>
                {
                    Some(method)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("TypeScript override {source} has no Exchange hook"));
        let sized = method.sig.generics.where_clause.as_ref().is_some_and(|clause| clause.predicates.iter().any(|predicate| {
            matches!(predicate, syn::WherePredicate::Type(bound) if matches!(&bound.bounded_ty, syn::Type::Path(path) if path.path.is_ident("Self")) && bound.bounds.iter().any(|bound| matches!(bound, syn::TypeParamBound::Trait(bound) if bound.path.is_ident("Sized"))))
        }));
        if matches!(source.as_str(), "placeOrder" | "placeOrders") {
            assert!(
                sized,
                "typed placement must retain its validated-token boundary"
            );
            let binding = if source == "placeOrder" {
                "PlaceOrderBinding"
            } else {
                "PlaceOrdersBinding"
            };
            assert!(
                include_str!("../src/capabilities.rs").contains(binding),
                "{source} raw dispatch binding missing"
            );
        } else {
            assert!(
                !sized,
                "TypeScript override {source} hook must remain callable through dyn Exchange"
            );
            assert!(
                method
                    .sig
                    .generics
                    .params
                    .iter()
                    .all(|param| matches!(param, syn::GenericParam::Lifetime(_))),
                "TypeScript override {source} hook cannot have type/const generic dispatch"
            );
        }
    }
}
#[test]
fn every_committed_typescript_override_has_a_dynamic_hook() {
    let rows = serde_json::from_str(include_str!("venue-overrides.json")).unwrap();
    assert_hooks(&rows, &trait_source());
}
#[test]
#[ignore = "requires isolated live TypeScript; mandatory core CI runs --include-ignored"]
fn live_typescript_override_inventory_matches_committed_contract() {
    let output = Command::new("node")
        .arg("scripts/override-inventory.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node required for live override inventory");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let live: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_hooks(&live, &trait_source());
    let committed: serde_json::Value =
        serde_json::from_str(include_str!("venue-overrides.json")).unwrap();
    assert_eq!(
        live, committed,
        "TypeScript override inventory changed: review hooks and regenerate venue-overrides.json from override-inventory.cjs"
    );
}

struct Calls {
    hooks: BTreeSet<String>,
    context: String,
    allowed: bool,
    base_impl: bool,
    handler_fields: BTreeMap<String, BTreeSet<String>>,
    current_handler_fields: BTreeSet<String>,
    aliases: BTreeSet<String>,
    findings: Vec<String>,
    body_hash: String,
    inventory: Vec<serde_json::Value>,
}
fn fingerprint(tokens: &impl ToTokens) -> String {
    let hash = tokens
        .to_token_stream()
        .to_string()
        .bytes()
        .fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    format!("{hash:016x}")
}
fn base_type(ty: &syn::Type) -> bool {
    match ty {
        syn::Type::Reference(reference) => base_type(&reference.elem),
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "BaseExchange"),
        _ => false,
    }
}
fn base_receiver(expr: &syn::Expr, aliases: &BTreeSet<String>, base_impl: bool) -> bool {
    match expr {
        syn::Expr::MethodCall(call) => call.method == "base" || call.method == "as_base",
        syn::Expr::Field(field) => {
            matches!(&field.member, syn::Member::Named(name) if name == "base" || name == "core")
        }
        syn::Expr::Path(path) => {
            path.path.is_ident("base")
                || path.path.is_ident("core")
                || (base_impl && path.path.is_ident("self"))
                || path
                    .path
                    .get_ident()
                    .is_some_and(|name| aliases.contains(&name.to_string()))
        }
        syn::Expr::Reference(value) => base_receiver(&value.expr, aliases, base_impl),
        syn::Expr::Paren(value) => base_receiver(&value.expr, aliases, base_impl),
        _ => false,
    }
}
impl Calls {
    fn base_receiver(&self, expr: &syn::Expr) -> bool {
        if let syn::Expr::Field(field) = expr
            && matches!(field.base.as_ref(), syn::Expr::Path(path) if path.path.is_ident("self"))
            && let syn::Member::Named(name) = &field.member
            && self.current_handler_fields.contains(&name.to_string())
        {
            return false;
        }
        base_receiver(expr, &self.aliases, self.base_impl)
    }
}
// This is a syntactic distinction, not compiler type resolution. Only the
// declared worker BaseHandler type is excluded; unknown field types stay audited.
fn handler_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident == "BaseHandler" {
        return true;
    }
    if segment.ident == "Rc"
        && let syn::PathArguments::AngleBracketed(args) = &segment.arguments
        && args.args.len() == 1
        && let Some(syn::GenericArgument::Type(inner)) = args.args.first()
    {
        return handler_type(inner);
    }
    false
}
impl<'ast> Visit<'ast> for Calls {
    fn visit_file(&mut self, node: &'ast syn::File) {
        for item in &node.items {
            if let syn::Item::Struct(item) = item {
                let fields = item
                    .fields
                    .iter()
                    .filter_map(|field| {
                        handler_type(&field.ty)
                            .then(|| field.ident.as_ref().map(ToString::to_string))
                            .flatten()
                    })
                    .collect();
                self.handler_fields.insert(item.ident.to_string(), fields);
            }
        }
        syn::visit::visit_file(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if node.attrs.iter().any(|attr| {
            attr.path().is_ident("cfg")
                && matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string() == "test")
        }) {
            return;
        }
        syn::visit::visit_item_mod(self, node);
    }
    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        // Defaults are the explicit equivalent of super. No arbitrary caller exemption.
        let old = self.allowed;
        self.allowed = node.ident == "Exchange";
        syn::visit::visit_item_trait(self, node);
        self.allowed = old;
    }
    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let old = self.base_impl;
        let fields = std::mem::take(&mut self.current_handler_fields);
        if let syn::Type::Path(path) = node.self_ty.as_ref()
            && let Some(segment) = path.path.segments.last()
        {
            self.current_handler_fields = self
                .handler_fields
                .get(&segment.ident.to_string())
                .cloned()
                .unwrap_or_default();
        }
        self.base_impl = matches!(node.self_ty.as_ref(), syn::Type::Path(path) if path.path.segments.last().is_some_and(|segment| segment.ident == "BaseExchange"));
        syn::visit::visit_item_impl(self, node);
        self.base_impl = old;
        self.current_handler_fields = fields;
    }
    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        let old = std::mem::replace(&mut self.context, node.sig.ident.to_string());
        let body = std::mem::replace(&mut self.body_hash, fingerprint(node));
        let aliases = std::mem::take(&mut self.aliases);
        syn::visit::visit_trait_item_fn(self, node);
        self.aliases = aliases;
        self.body_hash = body;
        self.context = old;
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        // Rust #[test] functions are test-only even in separately stored module
        // files. Do not classify fixture teardown as a production dispatch edge.
        if node.attrs.iter().any(|attr| attr.path().is_ident("test")) {
            return;
        }
        let old = std::mem::replace(&mut self.context, node.sig.ident.to_string());
        let body = std::mem::replace(&mut self.body_hash, fingerprint(node));
        let aliases = std::mem::take(&mut self.aliases);
        syn::visit::visit_item_fn(self, node);
        self.aliases = aliases;
        self.body_hash = body;
        self.context = old;
    }
    fn visit_fn_arg(&mut self, node: &'ast syn::FnArg) {
        if let syn::FnArg::Typed(argument) = node
            && base_type(&argument.ty)
            && let syn::Pat::Ident(binding) = argument.pat.as_ref()
        {
            self.aliases.insert(binding.ident.to_string());
        }
        syn::visit::visit_fn_arg(self, node);
    }
    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let Some(init) = &node.init
            && self.base_receiver(&init.expr)
            && let syn::Pat::Ident(binding) = &node.pat
        {
            self.aliases.insert(binding.ident.to_string());
        }
        syn::visit::visit_local(self, node);
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let old = std::mem::replace(&mut self.context, node.sig.ident.to_string());
        let body = std::mem::replace(&mut self.body_hash, fingerprint(node));
        let aliases = std::mem::take(&mut self.aliases);
        syn::visit::visit_impl_item_fn(self, node);
        self.aliases = aliases;
        self.body_hash = body;
        self.context = old;
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        if self.hooks.contains(&node.method.to_string()) {
            self.inventory.push(serde_json::json!({"function":self.context,"body":self.body_hash,"call":node.to_token_stream().to_string()}));
        }

        if !(self.allowed && node.method == self.context)
            && self.hooks.contains(&node.method.to_string())
            && self.base_receiver(&node.receiver)
        {
            self.findings.push(format!(
                "{} bypasses dynamic hook {} through base",
                self.context, node.method
            ));
        }
        syn::visit::visit_expr_method_call(self, node);
    }
    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        let segments: Vec<_> = node.path.segments.iter().collect();
        if segments.len() >= 2
            && segments
                .last()
                .is_some_and(|segment| self.hooks.contains(&segment.ident.to_string()))
        {
            self.inventory.push(serde_json::json!({"function":self.context,"body":self.body_hash,"reference":node.to_token_stream().to_string()}));
        }

        if segments.len() >= 2
            && segments[segments.len() - 2].ident == "BaseExchange"
            && self
                .hooks
                .contains(&segments.last().unwrap().ident.to_string())
        {
            self.findings.push(format!(
                "{} captures or calls BaseExchange::{} instead of dynamic hook",
                self.context,
                segments.last().unwrap().ident
            ));
        }
        syn::visit::visit_expr_path(self, node);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = node.func.as_ref() {
            let segments: Vec<_> = path.path.segments.iter().collect();
            if !self.allowed
                && segments.len() >= 2
                && segments[segments.len() - 2].ident == "BaseExchange"
                && self
                    .hooks
                    .contains(&segments.last().unwrap().ident.to_string())
            {
                self.findings.push(format!(
                    "{} bypasses dynamic hook via BaseExchange::{}",
                    self.context,
                    segments.last().unwrap().ident
                ));
            }
        }
        syn::visit::visit_expr_call(self, node);
    }
}
fn walk(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
#[test]
fn production_callers_do_not_bypass_exchange_override_hooks() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("venue-overrides.json")).unwrap();
    let sources = source_methods(&rows);
    let mut hooks: BTreeSet<String> = trait_source()
        .items
        .iter()
        .filter_map(|item| match item {
            syn::TraitItem::Fn(method)
                if sources
                    .iter()
                    .any(|name| normalize(name) == normalize(&method.sig.ident.to_string())) =>
            {
                Some(method.sig.ident.to_string())
            }
            _ => None,
        })
        .collect();
    assert_dynamic_base_bridges();
    for bridge in [
        "place_order",
        "place_orders",
        "after_method_called_in_worker",
    ] {
        assert!(
            hooks.remove(bridge),
            "stale dynamic bridge classification {bridge}"
        );
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = vec![];
    for entry in std::fs::read_dir(workspace).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().starts_with("safe-cex")
            && entry.path().join("src").is_dir()
        {
            walk(&entry.path().join("src"), &mut files);
        }
    }
    files.sort();
    let mut inventory = vec![];
    for file in files {
        let source = syn::parse_file(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let mut calls = Calls {
            hooks: hooks.clone(),
            context: file.display().to_string(),
            allowed: false,
            base_impl: false,
            handler_fields: BTreeMap::new(),
            current_handler_fields: BTreeSet::new(),
            aliases: BTreeSet::new(),
            findings: vec![],
            body_hash: String::new(),
            inventory: vec![],
        };
        calls.visit_file(&source);
        for mut row in calls.inventory {
            row["file"] =
                serde_json::json!(file.strip_prefix(workspace).unwrap().to_string_lossy());
            inventory.push(row);
        }

        assert!(
            calls.findings.is_empty(),
            "{}: {}",
            file.display(),
            calls.findings.join("; ")
        );
    }
    let inventory = serde_json::json!(inventory);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/override-call-sites.json");
    if std::env::var_os("UPDATE_EXCHANGE_CALL_INVENTORY").is_some() {
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&inventory).unwrap() + "\n",
        )
        .unwrap();
    }
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        inventory, expected,
        "override call sites or their enclosing bodies changed; review dynamic dispatch before updating override-call-sites.json"
    );
}

fn assert_dynamic_base_bridges() {
    struct Dispatch<'a> {
        name: &'a str,
        found: bool,
    }
    impl<'ast> Visit<'ast> for Dispatch<'_> {
        fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
            if self.name == "after_method_called_in_worker" {
                self.found |= node.method == self.name
                    && matches!(node.receiver.as_ref(), syn::Expr::Path(path) if path.path.is_ident("venue"));
            } else if node.method == "call_raw"
                && matches!(node.args.first(), Some(syn::Expr::Path(path)) if path.path.is_ident("venue"))
                && let syn::Expr::Field(field) = node.receiver.as_ref()
                && matches!(&field.member, syn::Member::Named(name) if name == self.name)
                && let syn::Expr::MethodCall(capabilities) = field.base.as_ref()
            {
                self.found |= capabilities.method == "capabilities"
                    && matches!(capabilities.receiver.as_ref(), syn::Expr::Path(path) if path.path.is_ident("venue"));
            }
            syn::visit::visit_expr_method_call(self, node);
        }
    }
    let source = syn::parse_file(include_str!("../src/exchange.rs")).unwrap();
    for name in [
        "place_order",
        "place_orders",
        "after_method_called_in_worker",
    ] {
        let mut dispatch = Dispatch { name, found: false };
        for item in &source.items {
            if let syn::Item::Impl(item) = item
                && matches!(item.self_ty.as_ref(), syn::Type::Path(path) if path.path.is_ident("BaseExchange"))
            {
                for member in &item.items {
                    if let syn::ImplItem::Fn(method) = member
                        && method.sig.ident == name
                    {
                        dispatch.visit_block(&method.block);
                    }
                }
            }
        }
        assert!(
            dispatch.found,
            "classified base bridge {name} must actually dispatch to the venue"
        );
    }
}

#[test]
fn test_attributes_do_not_hide_production_dispatch_bypasses() {
    let source = syn::parse_file(
        r#"
        #[test] fn fixture() { base.dispose(); }
        fn production() { base.dispose(); }
    "#,
    )
    .unwrap();
    let mut calls = Calls {
        hooks: BTreeSet::from(["dispose".into()]),
        context: "separate-module".into(),
        allowed: false,
        base_impl: false,
        handler_fields: BTreeMap::new(),
        current_handler_fields: BTreeSet::new(),
        aliases: BTreeSet::new(),
        findings: vec![],
        body_hash: String::new(),
        inventory: vec![],
    };
    calls.visit_file(&source);
    assert_eq!(
        calls.findings.len(),
        1,
        "test-only teardown must be excluded without hiding the production sibling"
    );
    assert!(
        calls.findings[0].starts_with("production "),
        "production sibling bypass must remain visible"
    );
}

#[test]
fn worker_handler_fields_do_not_hide_exchange_base_bypasses() {
    let source = syn::parse_file(
        r#"
        struct Worker { base: Rc<BaseHandler>, core: BaseExchange }
        impl Worker { fn dispose(&self) { self.base.dispose(); self.core.dispose(); } }
        struct Venue { base: BaseExchange }
        impl Venue { fn dispose(&self) { self.base.dispose(); } }
    "#,
    )
    .unwrap();
    let mut calls = Calls {
        hooks: BTreeSet::from(["dispose".into()]),
        context: String::new(),
        allowed: false,
        base_impl: false,
        handler_fields: BTreeMap::new(),
        current_handler_fields: BTreeSet::new(),
        aliases: BTreeSet::new(),
        findings: vec![],
        body_hash: String::new(),
        inventory: vec![],
    };
    calls.visit_file(&source);
    assert_eq!(
        calls.findings.len(),
        2,
        "worker BaseHandler exclusion must retain both exchange base bypasses"
    );
}
