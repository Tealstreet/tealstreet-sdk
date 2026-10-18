//! Literal standard-method lookup coverage against the actual native JS host.
use super::{Names, assert_ratchet, files, fs, literal, previous_baseline, skip};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        promise::Promise,
        scope::{Scope, ScopeShared},
        value::Js,
    },
    native::native_runtime,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    rc::Rc,
    sync::Arc,
};
use syn::{
    Expr,
    parse::Parser,
    punctuated::Punctuated,
    visit::{self, Visit},
};

type Catalog = BTreeMap<String, BTreeSet<String>>;
type Wrappers = BTreeMap<String, BTreeSet<usize>>;
const BASELINE: &str = "sdk-rs/safe-cex-exchange/tests/venue-known-unbound-methods.json";

fn scope() -> Scope<()> {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    library::install_library(&shared.globals, Rc::downgrade(&shared));
    Scope {
        shared,
        this: Js::Undefined,
    }
}

// Recognition names come from Node/dayjs, never from the implementation under test.
// Availability is read from installed production values, not a duplicate allowlist.
fn available_methods(scope: &Scope<()>, catalog: &Catalog) -> BTreeSet<String> {
    let runtime = scope.exchange_runtime();
    let (base, core) = safe_cex_exchange::js_runtime::base_exchange::create(
        &scope.shared.sockets,
        safe_cex_exchange::js_runtime::base_exchange::BaseExchangeParts {
            name: safe_cex_types::ExchangeName::Okx,
            options: serde_json::from_value(serde_json::json!({"key":"","secret":""})).unwrap(),
            store: scope.shared.store.clone(),
            settings: Default::default(),
            external_public_data: None,
        },
        Js::str("okx"),
        Js::object(vec![]),
        Js::object(vec![]),
        |_| Ok(()),
    )
    .unwrap();
    let values = BTreeMap::from([
        (
            "RegExp",
            safe_cex_exchange::js_runtime::regexp::regexp(&[], &[]).unwrap(),
        ),
        ("Boolean", Js::Bool(true)),
        (
            "Error",
            scope
                .global("Error")
                .unwrap()
                .call(Js::Undefined, vec![Js::str("sample")])
                .unwrap(),
        ),
        ("MathStatic", scope.global("Math").unwrap()),
        ("PromiseStatic", scope.global("Promise").unwrap()),
        ("BaseExchange", base),
        ("Array", Js::array(vec![])),
        ("String", Js::str("sample")),
        ("Number", Js::Number(1.)),
        ("Map", Js::new_map()),
        ("Set", Js::new_set()),
        (
            "Promise",
            Js::Promise(Promise::settled(&runtime, Ok(Js::Undefined))),
        ),
        ("Function", Js::function("sample", |_, _| Ok(Js::Undefined))),
        ("Object", Js::object(vec![])),
        ("ObjectStatic", scope.global("Object").unwrap()),
        ("JSONStatic", scope.global("JSON").unwrap()),
        ("DateStatic", scope.global("Date").unwrap()),
        (
            "BigNumber",
            scope
                .global("BigNumber")
                .unwrap()
                .call(Js::Undefined, vec![Js::Number(1.)])
                .unwrap(),
        ),
        ("NumberStatic", scope.global("Number").unwrap()),
        ("ArrayStatic", scope.global("Array").unwrap()),
        (
            "Dayjs",
            scope
                .global("dayjs")
                .unwrap()
                .call(Js::Undefined, vec![Js::Number(0.)])
                .unwrap(),
        ),
    ]);
    let mut available = BTreeSet::new();
    for name in catalog.values().flatten() {
        if values
            .values()
            .any(|value| value.get(name).is_ok_and(|value| value.is_callable()))
        {
            available.insert(name.clone());
        }
    }
    core.dispose().unwrap();
    available
}

struct Calls<'a> {
    catalog: &'a BTreeSet<String>,
    wrappers: &'a Wrappers,
    lookups: &'a Wrappers,
    lookup_discovery: bool,
    locals: BTreeMap<String, Expr>,
    // During discovery, a forwarded key parameter identifies a lookup wrapper.
    parameters: BTreeMap<String, usize>,
    forwarded: BTreeSet<usize>,
    found: BTreeMap<String, BTreeSet<String>>,
    file: String,
    function: String,
}
impl<'a> Calls<'a> {
    fn new(catalog: &'a BTreeSet<String>, wrappers: &'a Wrappers, lookups: &'a Wrappers) -> Self {
        Self {
            catalog,
            wrappers,
            lookups,
            lookup_discovery: false,
            locals: BTreeMap::new(),
            parameters: BTreeMap::new(),
            forwarded: BTreeSet::new(),
            found: BTreeMap::new(),
            file: String::new(),
            function: String::new(),
        }
    }
}
fn callee_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Paren(e) => callee_name(&e.expr),
        Expr::Path(e) => e.path.segments.last().map(|s| s.ident.to_string()),
        Expr::Field(e) => match &e.member {
            syn::Member::Named(n) => Some(n.to_string()),
            _ => None,
        },
        _ => None,
    }
}
impl Calls<'_> {
    fn invoked(&mut self, expr: &Expr) {
        match expr {
            Expr::Try(e) => self.invoked(&e.expr),
            Expr::Paren(e) => self.invoked(&e.expr),
            Expr::Path(p) => {
                if let Some(init) = p
                    .path
                    .get_ident()
                    .and_then(|n| self.locals.get(&n.to_string()))
                    .cloned()
                {
                    let name = p.path.get_ident().unwrap().to_string();
                    let saved = self.locals.remove(&name).unwrap();
                    self.invoked(&init);
                    self.locals.insert(name, saved);
                }
            }
            Expr::MethodCall(c) if c.method == "get" || c.method == "member" => {
                self.inspect("member", &c.args.iter().collect::<Vec<_>>());
            }
            Expr::Call(c) => {
                if let Some(name) = callee_name(&c.func) {
                    if matches!(name.as_str(), "get" | "member") {
                        self.inspect("member", &c.args.iter().collect::<Vec<_>>());
                    } else if let Some(indices) = self.lookups.get(&name) {
                        let args: Vec<_> = indices.iter().filter_map(|i| c.args.get(*i)).collect();
                        self.inspect("member", &args);
                    }
                }
            }
            _ => {}
        }
    }
    fn inspect(&mut self, name: &str, args: &[&Expr]) {
        let known = if self.lookup_discovery {
            self.lookups
        } else {
            self.wrappers
        };
        let indices: Vec<usize> = if let Some(indices) = known.get(name) {
            indices.iter().copied().collect()
        } else {
            match name {
                "get" if self.lookup_discovery => (0..args.len().min(3)).collect(),
                "call_method" | "method_call" | "member" | "method" | "select" | "invoke" => {
                    (0..args.len().min(3)).collect()
                }
                _ => return,
            }
        };
        for i in indices {
            let Some(arg) = args.get(i) else { continue };
            if let Some(name) = literal(arg) {
                if self.catalog.contains(&name) {
                    self.found
                        .entry(name)
                        .or_default()
                        .insert(format!("{}::{}", self.file, self.function));
                }
            } else if let Expr::Path(path) = arg {
                if let Some(i) = path
                    .path
                    .get_ident()
                    .and_then(|name| self.parameters.get(&name.to_string()))
                {
                    self.forwarded.insert(*i);
                }
            }
        }
    }
}
impl<'a> Visit<'a> for Calls<'_> {
    fn visit_arm(&mut self, arm: &'a syn::Arm) {
        if !skip(&arm.attrs) {
            visit::visit_arm(self, arm);
        }
    }
    fn visit_file(&mut self, file: &'a syn::File) {
        if !skip(&file.attrs) {
            visit::visit_file(self, file);
        }
    }
    fn visit_item_const(&mut self, item: &'a syn::ItemConst) {
        if !skip(&item.attrs) {
            visit::visit_item_const(self, item);
        }
    }
    fn visit_item_mod(&mut self, item: &'a syn::ItemMod) {
        if !skip(&item.attrs) {
            visit::visit_item_mod(self, item);
        }
    }
    fn visit_item_impl(&mut self, item: &'a syn::ItemImpl) {
        if !skip(&item.attrs) {
            visit::visit_item_impl(self, item);
        }
    }
    fn visit_item_fn(&mut self, item: &'a syn::ItemFn) {
        if !skip(&item.attrs) {
            self.locals.clear();
            let previous = std::mem::replace(&mut self.function, item.sig.ident.to_string());
            visit::visit_item_fn(self, item);
            self.function = previous;
        }
    }
    fn visit_impl_item_fn(&mut self, item: &'a syn::ImplItemFn) {
        if !skip(&item.attrs) {
            self.locals.clear();
            let previous = std::mem::replace(&mut self.function, item.sig.ident.to_string());
            visit::visit_impl_item_fn(self, item);
            self.function = previous;
        }
    }
    fn visit_local(&mut self, local: &'a syn::Local) {
        if let syn::Pat::Ident(name) = &local.pat {
            if let Some(init) = &local.init {
                self.locals
                    .insert(name.ident.to_string(), (*init.expr).clone());
            }
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_method_call(&mut self, call: &'a syn::ExprMethodCall) {
        if call.method == "call" || call.method == "apply" {
            self.invoked(&call.receiver);
        }
        if matches!(
            call.method.to_string().as_str(),
            "call" | "invoke" | "apply"
        ) {
            if let Some(callee) = call.args.first() {
                self.invoked(callee);
            }
        }
        self.inspect(
            &call.method.to_string(),
            &call.args.iter().collect::<Vec<_>>(),
        );
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_call(&mut self, call: &'a syn::ExprCall) {
        if let Some(name) = callee_name(&call.func) {
            if matches!(name.as_str(), "call" | "invoke" | "apply") {
                if let Some(callee) = call.args.first() {
                    self.invoked(callee);
                }
            }
            self.inspect(&name, &call.args.iter().collect::<Vec<_>>());
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'a syn::Macro) {
        if let Ok(args) =
            Punctuated::<Expr, syn::Token![,]>::parse_terminated.parse2(mac.tokens.clone())
        {
            if let Some(name) = mac.path.segments.last() {
                self.inspect(&name.ident.to_string(), &args.iter().collect::<Vec<_>>());
            }
            for expr in &args {
                self.visit_expr(expr);
            }
        }
    }
}

fn discover(
    ast: &syn::File,
    catalog: &BTreeSet<String>,
    wrappers: &mut Wrappers,
    lookups: &Wrappers,
    lookup_discovery: bool,
) {
    // Wrapper APIs are free functions with a forwarded string-key argument.
    for item in &ast.items {
        if let syn::Item::Fn(f) = item {
            if skip(&f.attrs) {
                continue;
            }
            let mut calls = Calls::new(catalog, wrappers, lookups);
            calls.lookup_discovery = lookup_discovery;
            for (i, arg) in f.sig.inputs.iter().enumerate() {
                if let syn::FnArg::Typed(arg) = arg {
                    if let syn::Pat::Ident(param) = arg.pat.as_ref() {
                        if quote::ToTokens::to_token_stream(&arg.ty)
                            .to_string()
                            .replace(' ', "")
                            == "&str"
                        {
                            calls.parameters.insert(param.ident.to_string(), i);
                        }
                    }
                }
            }
            calls.visit_block(&f.block);
            let forwarded = calls.forwarded;
            if !forwarded.is_empty() {
                wrappers
                    .entry(f.sig.ident.to_string())
                    .or_default()
                    .extend(forwarded);
            }
        }
    }
}

fn references(root: &Path, catalog: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let asts: Vec<_> = files(root)
        .into_iter()
        .map(|file| {
            let ast = syn::parse_file(&fs::read_to_string(&file).unwrap()).unwrap();
            (file, ast)
        })
        .collect();
    let mut lookups = Wrappers::new();
    loop {
        let before = lookups.clone();
        for (_, ast) in &asts {
            discover(ast, catalog, &mut lookups, &before, true);
        }
        if lookups == before {
            break;
        }
    }
    let mut wrappers = Wrappers::new();
    loop {
        let before = wrappers.clone();
        for (_, ast) in &asts {
            discover(ast, catalog, &mut wrappers, &lookups, false);
        }
        if wrappers == before {
            break;
        }
    }
    let mut calls = Calls::new(catalog, &wrappers, &lookups);
    for (file, ast) in &asts {
        calls.file = file
            .strip_prefix(root.parent().unwrap())
            .unwrap()
            .display()
            .to_string();
        calls.visit_file(ast);
    }
    calls.found
}

#[test]
fn venue_standard_method_names_exist_in_the_real_shared_runtime() {
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let catalog: Catalog =
        serde_json::from_str(include_str!("../venue-standard-methods.json")).unwrap();
    let catalog_path = "sdk-rs/safe-cex-exchange/tests/venue-standard-methods.json";
    if let Some(before) = previous_baseline(ws, catalog_path) {
        assert_catalog_expands(&catalog, &before);
    }
    let recognized = catalog.values().flatten().cloned().collect();
    let available = available_methods(&scope(), &catalog);
    let mut inventory = BTreeMap::new();
    for entry in fs::read_dir(ws).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_str().unwrap();
        // Hyperliquid owns a distinct production realm and builtin surface.
        if !dir.is_dir()
            || !name.starts_with("safe-cex-venue-")
            || name == "safe-cex-venue-hyperliquid"
        {
            continue;
        }
        let missing: BTreeMap<_, _> = references(&dir.join("src"), &recognized)
            .into_iter()
            .filter(|(name, _)| !available.contains(name))
            .collect();
        inventory.insert(name.to_owned(), missing);
    }
    if let Ok(path) = std::env::var("VENUE_METHOD_INVENTORY") {
        fs::write(path, serde_json::to_string_pretty(&inventory).unwrap()).unwrap();
    }
    let observed: Names = inventory
        .into_iter()
        .map(|(venue, names)| (venue, names.into_keys().collect()))
        .collect();
    let baseline =
        serde_json::from_str(include_str!("../venue-known-unbound-methods.json")).unwrap();
    assert_ratchet(
        &observed,
        &baseline,
        previous_baseline(ws, BASELINE).as_ref(),
    );
}

#[test]
fn method_controls_cover_forwarded_keys_macros_and_exclude_test_hosts() {
    let catalog = BTreeSet::from(["pop".into(), "reverse".into(), "startOf".into()]);
    let ast = syn::parse_file(
        r#"
        fn forward(h:&Host, value:&Js, key:&str) { h.member(value,key).call(args); }
        fn outer(h:&Host,value:&Js,key:&str) { forward(h,value,key); }
        fn field_get(h:&Host,value:&Js,key:&str) { (h.get)(value,key) }
        fn field_forward(h:&Host,value:&Js,key:&str) { let f = field_get(h,value,key)?; (h.invoke)(f,value,args) }
        fn production_outer(h:&Host) { outer(h,value,"pop"); row.get("startOf"); }
        fn production_macro(h:&Host) { method!(h,value,"reverse",vec![]); }
        fn production_field(h:&Host) { field_forward(h,value,"pop"); (h.member)(value,"reverse"); }
        fn production_host_call(h:&Host) { let f=h.get(value,"pop")?; h.call(f,value,vec![]); }
        fn production_host_apply(h:&Host) { let f=h.get(value,"reverse")?; h.apply(f,value,vec![]); }
        #[cfg(test)] mod fake { fn body(h:&Host) { h.call_method("startOf",args); } }
        #[cfg(test)] impl Host { fn fake(&self) { self.call_method("startOf",args); } }
    "#,
    )
    .unwrap();
    let mut lookups = Wrappers::new();
    for _ in 0..3 {
        let before = lookups.clone();
        discover(&ast, &catalog, &mut lookups, &before, true);
    }
    let mut wrappers = Wrappers::new();
    for _ in 0..3 {
        discover(&ast, &catalog, &mut wrappers, &lookups, false);
    }
    assert!(
        wrappers
            .get("field_forward")
            .is_some_and(|keys| keys.contains(&2))
    );
    let mut calls = Calls::new(&catalog, &wrappers, &lookups);
    calls.visit_file(&ast);
    for (name, function) in [
        ("pop", "production_outer"),
        ("reverse", "production_macro"),
        ("pop", "production_field"),
        ("reverse", "production_field"),
        ("pop", "production_host_call"),
        ("reverse", "production_host_apply"),
    ] {
        assert!(
            calls
                .found
                .get(name)
                .is_some_and(|sites| sites.contains(&format!("::{function}"))),
            "{function}: {name}"
        );
    }
    assert_eq!(
        calls.found.keys().cloned().collect::<BTreeSet<_>>(),
        BTreeSet::from(["pop".into(), "reverse".into()])
    );
}

fn assert_catalog_expands(catalog: &Catalog, before: &Catalog) {
    for (family, names) in before {
        assert!(
            catalog
                .get(family)
                .is_some_and(|after| names.is_subset(after)),
            "standard method recognition catalog may only expand: {family}"
        );
    }
}
#[test]
fn method_catalog_cannot_hide_a_gap_by_removing_a_recognized_name() {
    let before = BTreeMap::from([("Array".into(), BTreeSet::from(["pop".into()]))]);
    assert_catalog_expands(&before, &before);
    assert!(
        std::panic::catch_unwind(|| assert_catalog_expands(&BTreeMap::new(), &before)).is_err()
    );
    let empty_family = BTreeMap::from([("Array".into(), BTreeSet::new())]);
    assert!(std::panic::catch_unwind(|| assert_catalog_expands(&empty_family, &before)).is_err());
}

#[test]
fn review_control_method_alias_chains_remain_visible_and_cycles_terminate() {
    let ast = syn::parse_file(
        r#"
        fn aliases(row:&Js) { let first=row.get("pop")?; let second=first; second.call(this,args); }
        fn cycle(row:&Js) { let first=second; let second=first; second.call(this,args); }
    "#,
    )
    .unwrap();
    let catalog = BTreeSet::from(["pop".to_owned()]);
    let wrappers = Wrappers::new();
    let mut calls = Calls::new(&catalog, &wrappers, &wrappers);
    calls.visit_file(&ast);
    assert!(
        calls
            .found
            .get("pop")
            .is_some_and(|sites| sites.contains("::aliases")),
        "two-hop method alias remains visible"
    );
}

#[test]
fn review_control_test_only_match_arms_do_not_add_method_references() {
    let ast = syn::parse_file(
        r#"fn body(h:&Host) {match state {
        #[cfg(test)] true=>h.call_method("reverse",args),
        false=>h.call_method("pop",args),
    }}"#,
    )
    .unwrap();
    let catalog = BTreeSet::from(["pop".to_owned(), "reverse".to_owned()]);
    let wrappers = Wrappers::new();
    let mut calls = Calls::new(&catalog, &wrappers, &wrappers);
    calls.visit_file(&ast);
    assert!(calls.found.contains_key("pop"));
    assert!(
        !calls.found.contains_key("reverse"),
        "test-only arm cannot add a production method"
    );
}
