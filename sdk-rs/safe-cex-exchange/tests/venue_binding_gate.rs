//! Production host binding inventory; test modules never supply registrations.
use quote::ToTokens;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use syn::{
    Expr,
    visit::{self, Visit},
};
#[derive(Default)]
struct Strings(BTreeSet<String>);
impl<'a> Visit<'a> for Strings {
    fn visit_lit_str(&mut self, s: &'a syn::LitStr) {
        self.0.insert(s.value());
    }
    fn visit_macro(&mut self, m: &'a syn::Macro) {
        if m.path.is_ident("vec") {
            if let Ok(e) = syn::parse_str::<Expr>(&format!("[{}]", m.tokens)) {
                self.visit_expr(&e);
            }
        }
    }
}
fn literal(e: &Expr) -> Option<String> {
    match e {
        Expr::Lit(l) => {
            if let syn::Lit::Str(s) = &l.lit {
                Some(s.value())
            } else {
                None
            }
        }
        Expr::Reference(r) => literal(&r.expr),
        Expr::Paren(p) => literal(&p.expr),
        _ => None,
    }
}
fn global_object(e: &Expr) -> bool {
    match e {
        Expr::Reference(r) => global_object(&r.expr),
        Expr::Try(t) => global_object(&t.expr),
        Expr::Paren(p) => global_object(&p.expr),
        Expr::MethodCall(c) => c.method == "global" && c.args.is_empty(),
        _ => false,
    }
}
fn skip(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.meta.to_token_stream().to_string().replace(' ', "") == "cfg(test)")
    })
}
#[derive(Default)]
struct Scan {
    refs: BTreeMap<String, BTreeSet<String>>,
    globals: BTreeSet<String>,
    intrinsics: BTreeSet<String>,
    file: String,
    binding: bool,
    function: String,
    aliases: BTreeSet<String>,
    members: BTreeMap<String, BTreeSet<String>>,
    locals: BTreeMap<String, BTreeSet<String>>,
    wrappers: BTreeMap<String, (String, usize)>,
    tables: BTreeMap<String, BTreeSet<String>>,
    local_classes: BTreeMap<String, String>,
    intrinsic_globals: bool,
}
impl Scan {
    fn reference(&mut self, kind: &str, value: &Expr) {
        if let Some(name) = literal(value) {
            self.refs
                .entry(format!("{kind} {name}"))
                .or_default()
                .insert(format!("{}::{}", self.file, self.function));
        }
    }
    fn call(&mut self, name: &str, args: &[&Expr], receiver: &str) {
        let wrapper = self.wrappers.get(name).cloned();
        let reference = if let Some((ref kind, i)) = wrapper {
            Some((kind.as_str(), i))
        } else {
            match name {
                "global" | "global_binding" | "global_value" | "global_call" | "call_global"
                | "import" | "imported" | "binding_value" | "constant" => Some((
                    "global",
                    if receiver.is_empty() && args.first().is_some_and(|e| literal(e).is_none()) {
                        1
                    } else {
                        0
                    },
                )),
                "intrinsic" => Some((
                    "intrinsic",
                    if receiver.is_empty() && args.first().is_some_and(|e| literal(e).is_none()) {
                        1
                    } else {
                        0
                    },
                )),
                "import_call" => Some(("global", 1)),
                _ => None,
            }
        };
        if matches!(name, "get" | "select") && args.len() > 1 && global_object(args[0]) {
            self.reference("global", args[1]);
        }
        if let Some((kind, i)) = reference {
            if let Some(arg) = args.get(i) {
                self.reference(kind, arg);
            }
        }
        if !self.binding {
            return;
        }
        if name == "define_data" && args.len() > 2 {
            if let Some(k) = literal(args[1]) {
                if let Some(class) = self
                    .local_classes
                    .get(
                        args[0]
                            .to_token_stream()
                            .to_string()
                            .replace(' ', "")
                            .trim_start_matches('&'),
                    )
                    .cloned()
                {
                    self.globals.insert(format!("{class}.{k}"));
                }
            }
        }
        if name == "define" {
            if let Some(k) = args.first().and_then(|e| literal(e)) {
                self.members
                    .entry(receiver.to_owned())
                    .or_default()
                    .insert(k);
            }
        }
        if name == "finish_class_binding" {
            if let Some(arg) = args.first() {
                let local = arg.to_token_stream().to_string().replace(' ', "");
                if let Some(class) = self.local_classes.get(local.trim_start_matches('&')) {
                    self.globals.insert(class.clone());
                }
            }
        }
        let index = if name == "class_pair" && !receiver.is_empty() {
            Some(if receiver == "ufcs-class-pair" { 1 } else { 0 })
        } else if name == "install_module_helper" {
            Some(0)
        } else if name == "define_absent" || name == "define_missing" {
            Some(1)
        } else if name == "def" && args.len() > 3 {
            Some(2)
        } else if self.aliases.contains(name) {
            Some(0)
        } else {
            match name {
                "bind_import" | "set_global" | "define_global" | "def" | "bind" | "builtin"
                | "global_fn" | "helper" => Some(0),
                "define"
                    if matches!(receiver, "g" | "globals" | "global")
                        || receiver.ends_with(".globals") =>
                {
                    Some(0)
                }
                _ => None,
            }
        };
        if let Some(i) = index {
            if let Some(arg) = args.get(i) {
                if let Some(name) = literal(arg) {
                    self.globals.insert(name.clone());
                    if let Some(value) = args.get(i + 1) {
                        let text = value.to_token_stream().to_string();
                        for (table, keys) in &self.tables {
                            if text
                                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                                .any(|t| t == table)
                            {
                                for k in keys {
                                    self.globals.insert(format!("{name}.{k}"));
                                }
                            }
                        }
                    }
                    if let Some(value) = args.get(i + 1) {
                        if value
                            .to_token_stream()
                            .to_string()
                            .contains("base_websocket_class")
                        {
                            self.globals.insert(format!("{name}.prototype"));
                        }
                    }
                    if let Some(value) = args.get(i + 1) {
                        let v = value.to_token_stream().to_string().replace(' ', "");
                        if let Some(keys) = self.members.get(&v) {
                            for k in keys {
                                self.globals.insert(format!("{name}.{k}"));
                            }
                        }
                    }
                    if let Some(value) = args.get(i + 1) {
                        for member in object_keys(value) {
                            self.globals.insert(format!("{name}.{member}"));
                        }
                    }
                }
            }
        }
    }
}
fn object_keys(e: &Expr) -> BTreeSet<String> {
    struct Keys(BTreeSet<String>);
    impl<'a> Visit<'a> for Keys {
        fn visit_expr_array(&mut self, n: &'a syn::ExprArray) {
            for item in &n.elems {
                if let Some(k) = literal(item) {
                    self.0.insert(k);
                } else {
                    self.visit_expr(item);
                }
            }
        }
        fn visit_expr_call(&mut self, n: &'a syn::ExprCall) {
            if let Some(k) = n.args.first().and_then(literal) {
                if n.args.len() == 2 {
                    self.0.insert(k);
                }
            }
            visit::visit_expr_call(self, n);
        }
        fn visit_expr_tuple(&mut self, t: &'a syn::ExprTuple) {
            if t.elems.len() >= 2 {
                if let Some(k) = literal(&t.elems[0]) {
                    self.0.insert(k);
                }
            }
        }
        fn visit_macro(&mut self, m: &'a syn::Macro) {
            if m.path.is_ident("vec") {
                if let Ok(e) = syn::parse_str::<Expr>(&format!("[{}]", m.tokens)) {
                    self.visit_expr(&e);
                }
            }
        }
    }
    let mut k = Keys(BTreeSet::new());
    k.visit_expr(e);
    k.0
}
impl<'a> Visit<'a> for Scan {
    fn visit_local(&mut self, n: &'a syn::Local) {
        if self.binding {
            if let syn::Pat::Ident(p) = &n.pat {
                if let Some(init) = &n.init {
                    let e = if let Expr::Try(t) = &*init.expr {
                        &*t.expr
                    } else {
                        &*init.expr
                    };
                    if let Expr::MethodCall(c) = e {
                        if c.method == "create_derived_class" {
                            if let Some(name) = c.args.iter().nth(1).and_then(literal) {
                                self.local_classes.insert(p.ident.to_string(), name);
                            }
                        }
                    }
                }
            }
        }
        if self.binding {
            let pat = if let syn::Pat::Type(t) = &n.pat {
                &*t.pat
            } else {
                &n.pat
            };
            if let (syn::Pat::Ident(p), Some(init)) = (pat, &n.init) {
                self.members
                    .entry(p.ident.to_string())
                    .or_default()
                    .extend(object_keys(&init.expr));
                self.locals
                    .insert(p.ident.to_string(), object_keys(&init.expr));
            }
        }
        if self.binding {
            if let Some(init) = &n.init {
                if matches!(*init.expr, Expr::Closure(_)) {
                    let text = init.expr.to_token_stream().to_string();
                    if [
                        "g . define (",
                        "globals . define (",
                        ". bind_import (",
                        ". set_global (",
                    ]
                    .iter()
                    .any(|p| text.contains(p))
                    {
                        if let syn::Pat::Ident(p) = &n.pat {
                            self.aliases.insert(p.ident.to_string());
                        }
                    }
                }
            }
        }
        visit::visit_local(self, n);
    }

    fn visit_item_mod(&mut self, n: &'a syn::ItemMod) {
        if !skip(&n.attrs) {
            visit::visit_item_mod(self, n);
        }
    }
    fn visit_item_fn(&mut self, n: &'a syn::ItemFn) {
        if !skip(&n.attrs) {
            let old = std::mem::replace(&mut self.function, n.sig.ident.to_string());
            visit::visit_item_fn(self, n);
            self.function = old;
        }
    }
    fn visit_impl_item_fn(&mut self, n: &'a syn::ImplItemFn) {
        if !skip(&n.attrs) {
            let old = std::mem::replace(&mut self.function, n.sig.ident.to_string());
            visit::visit_impl_item_fn(self, n);
            self.function = old;
        }
    }
    fn visit_expr_method_call(&mut self, n: &'a syn::ExprMethodCall) {
        let recv = n.receiver.to_token_stream().to_string().replace(' ', "");
        self.call(
            &n.method.to_string(),
            &n.args.iter().collect::<Vec<_>>(),
            &recv,
        );
        visit::visit_expr_method_call(self, n);
    }
    fn visit_expr_call(&mut self, n: &'a syn::ExprCall) {
        let f = n.func.to_token_stream().to_string().replace(' ', "");
        let f = f.trim_matches(['(', ')']);
        let name = f.rsplit(['.', ':']).next().unwrap_or(f);
        self.call(
            name,
            &n.args.iter().collect::<Vec<_>>(),
            if name == "class_pair" && f.contains("::") {
                "ufcs-class-pair"
            } else {
                ""
            },
        );
        visit::visit_expr_call(self, n);
    }
    fn visit_expr_match(&mut self, n: &'a syn::ExprMatch) {
        if self.binding
            && matches!(
                n.expr.to_token_stream().to_string().as_str(),
                "name" | "code" | "n"
            )
        {
            for arm in &n.arms {
                let mut s = Strings::default();
                s.visit_pat(&arm.pat);
                if matches!(
                    self.function.as_str(),
                    "import" | "global" | "imported" | "global_binding" | "constant"
                ) {
                    self.globals.extend(s.0);
                } else {
                    self.intrinsics.extend(s.0);
                    if matches!(arm.pat, syn::Pat::Wild(_)) {
                        let body = arm.body.to_token_stream().to_string();
                        let key = n.expr.to_token_stream().to_string();
                        if body.contains(&format!(". global ({key})")) {
                            self.intrinsic_globals = true;
                        }
                    }
                }
            }
        }
        visit::visit_expr_match(self, n);
    }
    fn visit_expr_for_loop(&mut self, n: &'a syn::ExprForLoop) {
        if self.binding {
            let body = n.body.to_token_stream().to_string();
            if [
                "g . define (",
                "globals . define (",
                ". bind_import (",
                ". set_global (",
                "define_missing (",
                ". install_module_helper (",
                "def (",
                "bind (",
            ]
            .iter()
            .any(|p| body.contains(p))
            {
                self.globals.extend(object_keys(&n.expr));
                if let Expr::Call(c) = &*n.expr {
                    let k = c.func.to_token_stream().to_string();
                    if let Some(v) = self.tables.get(&k) {
                        self.globals.extend(v.iter().cloned());
                    }
                }
                if let Some(v) = self.locals.get(&n.expr.to_token_stream().to_string()) {
                    self.globals.extend(v.iter().cloned());
                }
            }
        }
        let body = n.body.to_token_stream().to_string();
        for receiver in ["number", "date", "math"] {
            if body.contains(&format!("{receiver} . define (")) {
                self.members
                    .entry(receiver.into())
                    .or_default()
                    .extend(object_keys(&n.expr));
            }
        }
        visit::visit_expr_for_loop(self, n);
    }
    fn visit_macro(&mut self, n: &'a syn::Macro) {
        if n.path.is_ident("global") || n.path.is_ident("global_call") {
            if let Ok(e) = syn::parse_str::<Expr>(&format!("({})", n.tokens)) {
                let first = match &e {
                    Expr::Tuple(t) => t.elems.first(),
                    Expr::Paren(p) => Some(&*p.expr),
                    _ => None,
                };
                if let Some(first) = first {
                    self.reference("global", first);
                }
            }
        } else if n.path.is_ident("imported") {
            if let Ok(Expr::Tuple(t)) = syn::parse_str::<Expr>(&format!("({})", n.tokens)) {
                if let Some(key) = t.elems.iter().nth(1) {
                    self.reference("global", key);
                }
            }
        } else if n.path.is_ident("enums") {
            if let Ok(e) = syn::parse_str::<Expr>(&format!("({})", n.tokens)) {
                if let Expr::Tuple(t) = e {
                    if let Some(k) = t.elems.first().and_then(literal) {
                        self.globals.insert(k);
                    }
                }
            }
        } else if n.path.is_ident("vec") {
            if let Ok(e) = syn::parse_str::<Expr>(&format!("[{}]", n.tokens)) {
                self.visit_expr(&e);
            }
        }
    }
}
struct Discover {
    known: BTreeMap<String, (String, usize)>,
    found: BTreeMap<String, (String, usize)>,
}
impl Discover {
    fn function(&mut self, sig: &syn::Signature, body: &syn::Block) {
        let params: Vec<_> = sig
            .inputs
            .iter()
            .filter_map(|a| {
                if let syn::FnArg::Typed(t) = a {
                    if let syn::Pat::Ident(p) = &*t.pat {
                        Some(p.ident.to_string())
                    } else {
                        Some(String::new())
                    }
                } else {
                    None
                }
            })
            .collect();
        struct Calls<'b> {
            params: &'b [String],
            known: &'b BTreeMap<String, (String, usize)>,
            found: Option<(String, usize)>,
        }
        impl Calls<'_> {
            fn call(&mut self, name: &str, args: Vec<&Expr>) {
                let spec = self.known.get(name).cloned().or_else(|| match name {
                    "global" | "import" | "imported" => Some(("global".into(), 0)),
                    "intrinsic" => Some(("intrinsic".into(), 0)),
                    _ => None,
                });
                if let Some((kind, i)) = spec {
                    if let Some(e) = args.get(i) {
                        let text = e.to_token_stream().to_string();
                        if let Some(j) = self.params.iter().position(|p| p == &text) {
                            self.found = Some((kind, j));
                        }
                    }
                }
            }
        }
        impl<'a> Visit<'a> for Calls<'_> {
            fn visit_expr_method_call(&mut self, n: &'a syn::ExprMethodCall) {
                self.call(&n.method.to_string(), n.args.iter().collect());
                visit::visit_expr_method_call(self, n);
            }
            fn visit_expr_call(&mut self, n: &'a syn::ExprCall) {
                let f = n.func.to_token_stream().to_string().replace(' ', "");
                let name = f
                    .trim_matches(['(', ')'])
                    .rsplit(['.', ':'])
                    .next()
                    .unwrap_or(&f);
                self.call(name, n.args.iter().collect());
                visit::visit_expr_call(self, n);
            }
        }
        let mut c = Calls {
            params: &params,
            known: &self.known,
            found: None,
        };
        c.visit_block(body);
        if let Some(v) = c.found {
            self.found.insert(sig.ident.to_string(), v);
        }
    }
}
impl<'a> Visit<'a> for Discover {
    fn visit_item_mod(&mut self, n: &'a syn::ItemMod) {
        if !skip(&n.attrs) {
            visit::visit_item_mod(self, n);
        }
    }
    fn visit_item_fn(&mut self, n: &'a syn::ItemFn) {
        if !skip(&n.attrs) {
            self.function(&n.sig, &n.block);
        }
    }
    // Method names often implement the same operation across many traits. Their callers use the known operation names directly.
}
fn files(root: &Path) -> Vec<PathBuf> {
    fn modules(items: &[syn::Item], dir: &Path, out: &mut Vec<PathBuf>) {
        for item in items {
            if let syn::Item::Mod(m) = item {
                if skip(&m.attrs) {
                    continue;
                }
                let name = m.ident.to_string();
                if let Some((_, items)) = &m.content {
                    modules(items, &dir.join(name), out);
                } else {
                    let explicit = m.attrs.iter().find_map(|a| {
                        if a.path().is_ident("path") {
                            if let syn::Meta::NameValue(n) = &a.meta {
                                literal(&n.value).map(PathBuf::from)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });
                    let file = explicit
                        .map(|p| {
                            let direct = dir.join(&p);
                            if direct.exists() {
                                direct
                            } else {
                                dir.parent().unwrap().join(p)
                            }
                        })
                        .unwrap_or_else(|| {
                            let p = dir.join(format!("{name}.rs"));
                            if p.exists() {
                                p
                            } else {
                                dir.join(&name).join("mod.rs")
                            }
                        });
                    assert!(
                        file.exists(),
                        "missing production module {}",
                        file.display()
                    );
                    walk(&file, out);
                }
            }
        }
    }
    fn walk(file: &Path, out: &mut Vec<PathBuf>) {
        if out.contains(&file.to_path_buf()) {
            return;
        }
        out.push(file.to_owned());
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        let dir = if ["lib.rs", "mod.rs"].contains(&file.file_name().unwrap().to_str().unwrap()) {
            file.parent().unwrap().to_path_buf()
        } else {
            file.with_extension("")
        };
        modules(&ast.items, &dir, out);
    }
    let mut out = vec![];
    let entry = if root.join("lib.rs").exists() {
        root.join("lib.rs")
    } else {
        root.join("mod.rs")
    };
    walk(&entry, &mut out);
    out
}
fn scan(root: &Path, _shared: bool) -> Scan {
    let mut scan = Scan::default();
    let source_files = files(root);
    for file in &source_files {
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        for item in &ast.items {
            if let syn::Item::Const(c) = item {
                let keys = object_keys(&c.expr);
                scan.tables.insert(c.ident.to_string(), keys);
            }
        }
    }
    for file in source_files {
        scan.file = file
            .strip_prefix(root.parent().unwrap())
            .unwrap()
            .display()
            .to_string();
        scan.aliases.clear();
        scan.members.clear();
        scan.locals.clear();
        scan.wrappers.clear();
        scan.local_classes.clear();
        scan.binding = true;
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        for item in &ast.items {
            if let syn::Item::Fn(f) = item {
                if skip(&f.attrs) {
                    continue;
                }
                for stmt in &f.block.stmts {
                    if let syn::Stmt::Expr(e, _) = stmt {
                        let keys = object_keys(e);
                        if !keys.is_empty() {
                            scan.tables.insert(f.sig.ident.to_string(), keys);
                        }
                    }
                }
            }
        }
        for _ in 0..3 {
            let mut d = Discover {
                known: scan.wrappers.clone(),
                found: BTreeMap::new(),
            };
            d.visit_file(&ast);
            scan.wrappers.extend(d.found);
        }
        scan.visit_file(&ast);
    }
    scan
}
#[test]
fn every_venue_reference_has_a_production_binding() {
    let ws = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let shared = scan(&ws.join("safe-cex-exchange/src/js_runtime"), true);
    let mut inventory = BTreeMap::new();
    for e in fs::read_dir(ws).unwrap() {
        let dir = e.unwrap().path();
        let name = dir.file_name().unwrap().to_str().unwrap();
        if !name.starts_with("safe-cex-venue-") || !dir.is_dir() {
            continue;
        }

        let venue = scan(&dir.join("src"), false);
        let mut missing = BTreeMap::new();
        for (key, sites) in &venue.refs {
            let (kind, name) = key.split_once(' ').unwrap();
            let bound = if kind == "intrinsic" {
                venue.intrinsics.contains(name)
                    || (venue.intrinsic_globals
                        && (venue.globals.contains(name) || shared.globals.contains(name)))
            } else {
                venue.globals.contains(name)
                    || (dir.file_name().unwrap() != "safe-cex-venue-hyperliquid"
                        && shared.globals.contains(name))
            };
            if !bound
                && !(name == "super.dispose"
                    && dir.file_name().unwrap() == "safe-cex-venue-lighter")
                && !(dir.file_name().unwrap() == "safe-cex-venue-lighter"
                    && venue
                        .tables
                        .get("CLASS_STATICS")
                        .is_some_and(|keys| keys.contains(name)))
            {
                missing.insert(key.clone(), sites.clone());
            }
        }
        inventory.insert(name.to_owned(), missing);
    }
    let json = serde_json::to_string_pretty(&inventory).unwrap();
    if let Ok(path) = std::env::var("VENUE_BINDING_INVENTORY") {
        fs::write(path, &json).unwrap();
    }
    let observed: BTreeMap<String, BTreeSet<String>> = inventory
        .into_iter()
        .map(|(venue, rows)| (venue, rows.into_keys().collect()))
        .collect();
    let baseline = serde_json::from_str(include_str!("venue-known-unbound.json")).unwrap();
    let previous = previous_baseline(ws);
    assert_ratchet(&observed, &baseline, previous.as_ref());
}

type Names = BTreeMap<String, BTreeSet<String>>;
const BASELINE_PATH: &str = "sdk-rs/safe-cex-exchange/tests/venue-known-unbound.json";
fn assert_ratchet(observed: &Names, baseline: &Names, previous: Option<&Names>) {
    if let Some(previous) = previous {
        for (venue, names) in baseline {
            for name in names {
                assert!(
                    previous
                        .get(venue)
                        .is_some_and(|before| before.contains(name)),
                    "known-unbound baseline may only shrink: {venue}: {name}"
                );
            }
        }
    }
    assert_eq!(
        observed, baseline,
        "Unbound inventory changed: fix new names; remove fixed names from venue-known-unbound.json in the same PR"
    );
}
fn previous_baseline(workspace: &Path) -> Option<Names> {
    use std::process::Command;
    let base = match std::env::var("VENUE_BINDING_BASE_REF") {
        Ok(base) if !base.is_empty() => base,
        _ => {
            let out = Command::new("git")
                .args(["merge-base", "HEAD", "origin/master"])
                .current_dir(workspace)
                .output()
                .expect("git is required for the binding ratchet");
            assert!(out.status.success(), "cannot identify binding-ratchet base");
            String::from_utf8(out.stdout).unwrap().trim().into()
        }
    };
    let commit = Command::new("git")
        .args(["cat-file", "-e", &format!("{base}^{{commit}}")])
        .current_dir(workspace)
        .status()
        .unwrap();
    assert!(
        commit.success(),
        "binding-ratchet base commit must be fetched"
    );
    let out = Command::new("git")
        .args(["show", &format!("{base}:{BASELINE_PATH}")])
        .current_dir(workspace)
        .output()
        .unwrap();
    if out.status.success() {
        Some(serde_json::from_slice(&out.stdout).expect("base binding baseline must be valid JSON"))
    } else {
        let exists = Command::new("git")
            .args(["ls-tree", "--name-only", &base, "--", BASELINE_PATH])
            .current_dir(workspace)
            .output()
            .unwrap();
        assert!(
            exists.status.success() && exists.stdout.is_empty(),
            "cannot read existing base binding baseline"
        );
        None
    }
}
#[test]
fn ratchet_rejects_new_names_and_retained_fixed_names_and_baseline_growth() {
    let names = |rows: &[&str]| {
        BTreeMap::from([("venue".into(), rows.iter().map(|s| s.to_string()).collect())])
    };
    let baseline = names(&["global old"]);
    assert_ratchet(&baseline, &baseline, Some(&baseline));
    assert!(
        std::panic::catch_unwind(|| assert_ratchet(
            &names(&["global old", "global new"]),
            &baseline,
            None
        ))
        .is_err()
    );
    assert!(std::panic::catch_unwind(|| assert_ratchet(&names(&[]), &baseline, None)).is_err());
    let expanded = names(&["global old", "global new"]);
    assert!(
        std::panic::catch_unwind(|| assert_ratchet(&expanded, &expanded, Some(&baseline))).is_err()
    );
    assert_ratchet(&names(&[]), &names(&[]), Some(&baseline));
}

#[test]
fn test_only_registrations_do_not_satisfy_production_references() {
    let ast = syn::parse_file(
        r#"
        fn production(h: &Host) { h.global("missing"); h.global("present"); }
        fn install(g: &Js) { g.define("present", value); }
        #[cfg(test)] mod fixtures { fn install(g: &Js) { g.define("missing", value); } }
        #[test] fn fake_host() { g.define("missing", value); }
    "#,
    )
    .unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global missing"));
    assert!(scan.globals.contains("present"));
    assert!(!scan.globals.contains("missing"));
}
#[test]
fn wrappers_and_intrinsic_names_keep_separate_registration_requirements() {
    let ast = syn::parse_file(
        r#"
        fn imported(h: &Host, key: &str) { h.global(key); }
        fn production(h: &Host) { imported(h,"missing"); (h.intrinsic)("missing", args); }
        fn install(g: &Js) { g.define("missing", value); }
        fn intrinsic(name: &str) {match name {"present" => value, _ => panic!()}}
    "#,
    )
    .unwrap();
    let mut discover = Discover {
        known: BTreeMap::new(),
        found: BTreeMap::new(),
    };
    discover.visit_file(&ast);
    let mut scan = Scan {
        binding: true,
        wrappers: discover.found,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global missing"));
    assert!(scan.refs.contains_key("intrinsic missing"));
    assert!(scan.globals.contains("missing"));
    assert!(!scan.intrinsics.contains("missing"));
    assert!(scan.intrinsics.contains("present"));
}

#[test]
fn table_values_and_unrelated_object_members_are_not_global_registrations() {
    let ast = syn::parse_file(
        r#"
        fn install(g:&Js){
            for (name,value) in [("bound","not-a-global")] {g.define(name,value);}
            for (name,value) in [("also-not-global",1)] {number.define(name,value);}
        }
    "#,
    )
    .unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.globals.contains("bound"));
    assert!(!scan.globals.contains("not-a-global"));
    assert!(!scan.globals.contains("also-not-global"));
}

#[test]
fn member_of_a_global_object_is_not_an_independent_global_name() {
    let ast=syn::parse_file(r#"fn body(h:&Host){h.get(&h.global(),"Promise");h.get(&h.get(&h.global(),"Promise")?,"all");}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global Promise"));
    assert!(!scan.refs.contains_key("global all"));
}

#[test]
fn class_creation_needs_the_production_class_registration() {
    let ast=syn::parse_file(r#"
        fn install(realm:&Host){let class=realm.create_derived_class(base,"Absent",1,body)?;}
        fn registered(realm:&Host){let class=realm.create_derived_class(base,"Present",1,body)?;realm.finish_class_binding(&class)?;}
    "#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(!scan.globals.contains("Absent"));
    assert!(scan.globals.contains("Present"));
}
