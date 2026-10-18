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
// Unknown feature/target predicates stay possible; only provably test-only code
// is excluded. In particular any(test, feature=...) still has a production arm.
fn production_cfg(meta: &syn::Meta) -> Option<bool> {
    use syn::parse::Parser;
    match meta {
        syn::Meta::Path(path) if path.is_ident("test") => Some(false),
        syn::Meta::List(list) => {
            let args = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
                .parse2(list.tokens.clone())
                .ok()?;
            let values: Vec<_> = args.iter().map(production_cfg).collect();
            if list.path.is_ident("all") {
                if values.contains(&Some(false)) {
                    Some(false)
                } else if values.iter().all(|v| *v == Some(true)) {
                    Some(true)
                } else {
                    None
                }
            } else if list.path.is_ident("any") {
                if values.contains(&Some(true)) {
                    Some(true)
                } else if values.iter().all(|v| *v == Some(false)) {
                    Some(false)
                } else {
                    None
                }
            } else if list.path.is_ident("not") && values.len() == 1 {
                values[0].map(|v| !v)
            } else {
                None
            }
        }
        _ => None,
    }
}
fn skip(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg")
                && a.parse_args::<syn::Meta>()
                    .ok()
                    .as_ref()
                    .is_some_and(|m| production_cfg(m) == Some(false)))
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
    pair_helpers: BTreeMap<String, usize>,
    members: BTreeMap<String, BTreeSet<String>>,
    locals: BTreeMap<String, BTreeSet<String>>,
    wrappers: BTreeMap<String, (String, usize)>,
    tables: BTreeMap<String, BTreeSet<String>>,
    local_classes: BTreeMap<String, String>,
    intrinsic_globals: bool,
    captures: BTreeSet<String>,
    capture_mode: String,
    global_aliases: BTreeMap<String, String>,
}
impl Scan {
    fn reference(&mut self, kind: &str, value: &Expr) {
        if let Some(name) = literal(value) {
            let (kind, name) = if kind == "capture" {
                if self.capture_mode == "receiver-pair" {
                    return;
                }
                if self.capture_mode == "receiver" {
                    return;
                }
                if self.capture_mode == "explicit-imports"
                    && !["IMPORTED_CALLEES", "MODULE_BINDINGS"].iter().any(|table| {
                        self.tables
                            .get(*table)
                            .is_some_and(|names| names.contains(&name))
                    })
                {
                    return;
                }

                if name == "this" {
                    return;
                }
                if let Some((_, tail)) = name.split_once(':') {
                    if tail == "unbound" {
                        return;
                    }
                    if name.starts_with("super:") {
                        ("capture", name)
                    } else {
                        (
                            "global",
                            if tail.contains('.') {
                                tail.to_owned()
                            } else {
                                format!("ENDPOINTS.{tail}")
                            },
                        )
                    }
                } else if name.starts_with("this.") {
                    return;
                } else if name == "super" {
                    ("capture", name)
                } else {
                    ("global", name)
                }
            } else {
                (kind, name)
            };
            self.refs
                .entry(format!("{kind} {name}"))
                .or_default()
                .insert(format!("{}::{}", self.file, self.function));
        }
    }
    fn call(&mut self, name: &str, args: &[&Expr], receiver: &str) {
        let wrapper = if receiver.is_empty() {
            self.wrappers.get(name).cloned()
        } else {
            None
        };
        let reference = if let Some((ref kind, i)) = wrapper {
            Some((kind.as_str(), i))
        } else {
            match name {
                "global" | "global_binding" | "global_value" | "global_call" | "call_global"
                | "import" | "imported" | "binding_value" | "binding" | "function_value" => Some((
                    "global",
                    if receiver.is_empty() && args.first().is_some_and(|e| literal(e).is_none()) {
                        1
                    } else {
                        0
                    },
                )),
                "constant" if !receiver.is_empty() => Some(("global", 0)),
                "capture" => Some((
                    "capture",
                    if receiver.is_empty() && args.first().is_some_and(|e| literal(e).is_none()) {
                        1
                    } else {
                        0
                    },
                )),
                "operation" if self.capture_mode == "receiver" => Some((
                    "intrinsic",
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
            if kind == "capture" && self.capture_mode == "receiver-pair" {
                if args.get(i).and_then(|e| literal(e)).as_deref() == Some("imports") {
                    if let Some(key) = args.get(i + 1) {
                        self.reference("global", key);
                    }
                }
            } else if name == "capture" && args.len() > i + 1 {
                // Two-argument socket captures select a member on a receiver
                // field, unlike single-argument module captures.
            } else if let Some(arg) = args.get(i) {
                self.reference(kind, arg);
            }
        }
        if name == "define_on" && args.len() > 2 {
            if let Some(key) = literal(args[1]) {
                let owner = args[0].to_token_stream().to_string().replace(' ', "");
                let owner = owner.trim_start_matches('&');
                self.members
                    .entry(owner.into())
                    .or_default()
                    .insert(key.clone());
                if let Some(global) = self.global_aliases.get(owner) {
                    self.globals.insert(format!("{global}.{key}"));
                }
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
                        for member in object_keys(value, &self.pair_helpers) {
                            self.globals.insert(format!("{name}.{member}"));
                        }
                    }
                }
            }
        }
    }
}
fn object_keys(e: &Expr, pairs: &BTreeMap<String, usize>) -> BTreeSet<String> {
    struct Keys<'p>(BTreeSet<String>, &'p BTreeMap<String, usize>);
    impl<'a> Visit<'a> for Keys<'_> {
        fn visit_arm(&mut self, arm: &'a syn::Arm) {
            if !skip(&arm.attrs) {
                visit::visit_arm(self, arm);
            }
        }
        fn visit_expr_array(&mut self, n: &'a syn::ExprArray) {
            for item in &n.elems {
                if let Some(k) = literal(item) {
                    self.0.insert(k);
                } else {
                    self.visit_expr(item);
                }
            }
        }
        fn visit_expr_closure(&mut self, _: &'a syn::ExprClosure) {}
        fn visit_expr_call(&mut self, call: &'a syn::ExprCall) {
            if let Expr::Path(path) = call.func.as_ref() {
                if let Some(index) = path
                    .path
                    .get_ident()
                    .and_then(|n| self.1.get(&n.to_string()))
                {
                    if let Some(key) = call.args.get(*index).and_then(literal) {
                        self.0.insert(key);
                    }
                }
            }
            visit::visit_expr_call(self, call);
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
    let mut k = Keys(BTreeSet::new(), pairs);
    k.visit_expr(e);
    k.0
}
impl<'a> Visit<'a> for Scan {
    fn visit_arm(&mut self, arm: &'a syn::Arm) {
        if !skip(&arm.attrs) {
            visit::visit_arm(self, arm);
        }
    }
    fn visit_block(&mut self, block: &'a syn::Block) {
        let pairs = self.pair_helpers.clone();
        visit::visit_block(self, block);
        self.pair_helpers = pairs;
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
    fn visit_expr_let(&mut self, n: &'a syn::ExprLet) {
        if let (syn::Pat::TupleStruct(p), Expr::MethodCall(c)) = (&*n.pat, &*n.expr) {
            if c.method == "get"
                && matches!(
                    c.receiver.to_token_stream().to_string().as_str(),
                    "g" | "globals"
                )
            {
                if let (Some(syn::Pat::Ident(var)), Some(root)) =
                    (p.elems.first(), c.args.first().and_then(literal))
                {
                    self.global_aliases.insert(var.ident.to_string(), root);
                }
            }
        }
        visit::visit_expr_let(self, n);
    }
    fn visit_item_impl(&mut self, n: &'a syn::ItemImpl) {
        if !skip(&n.attrs) {
            visit::visit_item_impl(self, n);
        }
    }
    fn visit_local(&mut self, n: &'a syn::Local) {
        let helper_pattern = match &n.pat {
            syn::Pat::Type(typed) => typed.pat.as_ref(),
            pattern => pattern,
        };
        if let (syn::Pat::Ident(name), Some(init)) = (helper_pattern, &n.init) {
            self.pair_helpers.remove(&name.ident.to_string());
            if let Expr::Closure(closure) = init.expr.as_ref() {
                let body = match closure.body.as_ref() {
                    Expr::Block(block) => block.block.stmts.last().and_then(|stmt| match stmt {
                        syn::Stmt::Expr(expr, _) => Some(expr),
                        _ => None,
                    }),
                    expr => Some(expr),
                };
                if let Some(Expr::Tuple(tuple)) = body {
                    if let Some(Expr::Path(key)) = tuple.elems.first() {
                        for (index, input) in closure.inputs.iter().enumerate() {
                            let input = match input {
                                syn::Pat::Type(typed) => typed.pat.as_ref(),
                                input => input,
                            };
                            if let syn::Pat::Ident(param) = input {
                                if key.path.is_ident(&param.ident) {
                                    self.pair_helpers.insert(name.ident.to_string(), index);
                                }
                            }
                        }
                    }
                }
            }
        }
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
                    .extend(object_keys(&init.expr, &self.pair_helpers));
                self.locals.insert(
                    p.ident.to_string(),
                    object_keys(&init.expr, &self.pair_helpers),
                );
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
            let pairs = std::mem::take(&mut self.pair_helpers);
            visit::visit_item_fn(self, n);
            self.pair_helpers = pairs;
            self.function = old;
        }
    }
    fn visit_impl_item_fn(&mut self, n: &'a syn::ImplItemFn) {
        if !skip(&n.attrs) {
            let old = std::mem::replace(&mut self.function, n.sig.ident.to_string());
            let pairs = std::mem::take(&mut self.pair_helpers);
            visit::visit_impl_item_fn(self, n);
            self.pair_helpers = pairs;
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
            } else if f.contains('.') {
                "callback"
            } else {
                ""
            },
        );
        visit::visit_expr_call(self, n);
    }
    fn visit_expr_match(&mut self, n: &'a syn::ExprMatch) {
        let intrinsic_dispatch = self.function == "intrinsic"
            || (self.function == "operation" && self.file.contains("/binding/"))
            || (self.function == "call" && self.file.ends_with("binding/primitives.rs"));
        let capture_dispatch = self.function == "capture";
        let global_dispatch = matches!(
            self.function.as_str(),
            "import" | "global" | "imported" | "global_binding" | "constant"
        );
        if self.binding
            && (intrinsic_dispatch || capture_dispatch || global_dispatch)
            && matches!(
                n.expr.to_token_stream().to_string().as_str(),
                "name" | "code" | "n"
            )
        {
            for arm in &n.arms {
                if skip(&arm.attrs) {
                    continue;
                }
                let mut s = Strings::default();
                s.visit_pat(&arm.pat);
                if capture_dispatch {
                    self.captures.extend(s.0);
                } else if global_dispatch {
                    self.globals.extend(s.0);
                } else {
                    self.intrinsics.extend(s.0);
                    if matches!(arm.pat, syn::Pat::Wild(_)) {
                        let body = arm.body.to_token_stream().to_string();
                        let key = n.expr.to_token_stream().to_string();
                        if body.contains(&format!(". global ({key})"))
                            || body.contains(&format!("global_path (rt , {key})"))
                        {
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
                self.globals
                    .extend(object_keys(&n.expr, &self.pair_helpers));
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
        struct Targets(BTreeSet<String>);
        impl<'a> Visit<'a> for Targets {
            fn visit_arm(&mut self, arm: &'a syn::Arm) {
                if !skip(&arm.attrs) {
                    visit::visit_arm(self, arm);
                }
            }
            fn visit_expr_method_call(&mut self, n: &'a syn::ExprMethodCall) {
                if n.method == "define" {
                    self.0
                        .insert(n.receiver.to_token_stream().to_string().replace(' ', ""));
                }
                visit::visit_expr_method_call(self, n);
            }
            fn visit_expr_call(&mut self, n: &'a syn::ExprCall) {
                if n.func.to_token_stream().to_string() == "define_on" {
                    if let Some(owner) = n.args.first() {
                        self.0.insert(
                            owner
                                .to_token_stream()
                                .to_string()
                                .replace(' ', "")
                                .trim_start_matches('&')
                                .into(),
                        );
                    }
                }
                visit::visit_expr_call(self, n);
            }
        }
        let mut targets = Targets(BTreeSet::new());
        targets.visit_block(&n.body);
        let keys = object_keys(&n.expr, &self.pair_helpers);
        for receiver in targets.0 {
            self.members
                .entry(receiver.clone())
                .or_default()
                .extend(keys.iter().cloned());
            if let Some(global) = self.global_aliases.get(&receiver) {
                for key in &keys {
                    self.globals.insert(format!("{global}.{key}"));
                }
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
            fn call(&mut self, name: &str, args: Vec<&Expr>, free: bool) {
                let spec = if free {
                    self.known.get(name).cloned()
                } else {
                    None
                }
                .or_else(|| match name {
                    "global" | "global_binding" | "import" | "imported" | "binding"
                    | "binding_value" | "function_value" => {
                        Some(("global".into(), usize::from(free)))
                    }
                    "capture" => Some(("capture".into(), usize::from(free))),
                    "intrinsic" | "operation" => {
                        Some(("intrinsic".into(), usize::from(free && args.len() > 2)))
                    }
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
            fn visit_arm(&mut self, arm: &'a syn::Arm) {
                if !skip(&arm.attrs) {
                    visit::visit_arm(self, arm);
                }
            }
            fn visit_expr_method_call(&mut self, n: &'a syn::ExprMethodCall) {
                self.call(&n.method.to_string(), n.args.iter().collect(), false);
                visit::visit_expr_method_call(self, n);
            }
            fn visit_expr_call(&mut self, n: &'a syn::ExprCall) {
                let f = n.func.to_token_stream().to_string().replace(' ', "");
                let name = f
                    .trim_matches(['(', ')'])
                    .rsplit(['.', ':'])
                    .next()
                    .unwrap_or(&f);
                self.call(name, n.args.iter().collect(), !f.contains('.'));
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
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        if skip(&ast.attrs) {
            return;
        }
        out.push(file.to_owned());
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
    let capture_mode = match root
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
    {
        "safe-cex-venue-bitget" => "receiver",
        "safe-cex-venue-lighter" => "explicit-imports",
        "safe-cex-venue-polymarket" => "receiver-pair",
        _ => "module",
    }
    .into();
    let mut scan = Scan {
        capture_mode,
        ..Scan::default()
    };
    let source_files = files(root);
    for file in &source_files {
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        for item in &ast.items {
            if let syn::Item::Const(c) = item {
                if skip(&c.attrs) {
                    continue;
                }
                let keys = object_keys(&c.expr, &BTreeMap::new());
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
        scan.pair_helpers.clear();
        scan.members.clear();
        scan.locals.clear();
        scan.wrappers.clear();
        scan.local_classes.clear();
        scan.global_aliases.clear();
        scan.binding = true;
        let ast = syn::parse_file(&fs::read_to_string(file).unwrap()).unwrap();
        for item in &ast.items {
            if let syn::Item::Fn(f) = item {
                if skip(&f.attrs) {
                    continue;
                }
                for stmt in &f.block.stmts {
                    if let syn::Stmt::Expr(e, _) = stmt {
                        let keys = object_keys(e, &BTreeMap::new());
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
            let bound = if kind == "capture" {
                venue.captures.contains(name)
            } else if kind == "intrinsic" {
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
    let previous = previous_baseline(ws, BASELINE_PATH);
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
        "Unbound inventory changed: fix new names; remove fixed names from the corresponding known-unbound baseline in the same PR"
    );
}
fn previous_baseline(workspace: &Path, baseline_path: &str) -> Option<Names> {
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
        .args(["show", &format!("{base}:{baseline_path}")])
        .current_dir(workspace)
        .output()
        .unwrap();
    if out.status.success() {
        Some(serde_json::from_slice(&out.stdout).expect("base binding baseline must be valid JSON"))
    } else {
        let exists = Command::new("git")
            .args(["ls-tree", "--name-only", &base, "--", baseline_path])
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

#[test]
fn review_control_existing_lookup_apis_must_not_escape_inventory() {
    let ast=syn::parse_file(r#"fn p(h:&Host){h.binding("missingBinding");(h.capture)("missingCapture");(h.capture)("this.method");}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global missingBinding"));
    assert!(scan.refs.contains_key("global missingCapture"));
    assert!(!scan.refs.contains_key("global this.method"));
}
#[test]
fn review_control_capture_dispatch_is_not_an_intrinsic_registration() {
    let ast=syn::parse_file(r#"fn capture(name:&str){match name{"super"=>value,_=>error}} fn p(h:&Host){(h.intrinsic)("super",args);}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("intrinsic super"));
    assert!(!scan.intrinsics.contains("super"));
}
#[test]
fn review_control_test_impl_cannot_supply_a_production_registration() {
    let ast=syn::parse_file(r#"fn p(h:&Host){h.global("missing");} #[cfg(test)] impl Host{fn install(g:&Js){g.define("missing",value);}}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global missing"));
    assert!(!scan.globals.contains("missing"));
}

#[test]
fn source_constant_wrapper_reads_its_class_instead_of_a_free_global() {
    let ast=syn::parse_file(r#"fn constant(h:&Host,k:&str){let c=h.imported("Class")?;h.get(&c,k)} fn p(h:&Host){constant(h,"TIMEOUT")?;}"#).unwrap();
    let mut d = Discover {
        known: BTreeMap::new(),
        found: BTreeMap::new(),
    };
    d.visit_file(&ast);
    let mut scan = Scan {
        binding: true,
        wrappers: d.found,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global Class"));
    assert!(!scan.refs.contains_key("global TIMEOUT"));
}

#[test]
fn shared_library_members_added_to_global_aliases_are_bindings() {
    let ast=syn::parse_file(r#"fn install(g:&Js){if let Ok(clock)=g.get("virtualClock"){define_on(&clock,"getCurrentTime",value);}if let Ok(ctor)=g.get("BigNumber"){for (key,order) in [("minimum",less)]{define_on(&ctor,key,value);}}let lodash=object;for (key,f) in [("flatten",flatten)]{define_on(&lodash,key,f);}g.define("lodash",lodash);}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    for key in [
        "virtualClock.getCurrentTime",
        "BigNumber.minimum",
        "lodash.flatten",
    ] {
        assert!(scan.globals.contains(key), "{key}");
    }
}

#[test]
fn review_control_two_argument_socket_capture_reads_receiver_fields() {
    let ast = syn::parse_file(r#"fn body(h:&Host){ (h.capture)("publicWebsocket", "listenOHLCV"); h.capture("actualGlobal"); }"#).unwrap();
    let mut scan = Scan::default();
    scan.visit_file(&ast);
    assert!(!scan.refs.contains_key("global publicWebsocket"));
    assert!(scan.refs.contains_key("global actualGlobal"));
    scan.wrappers.insert("call".into(), ("capture".into(), 1));
    scan.visit_file(&syn::parse_file(r#"fn p(h:&Host){call(h,"missingGlobal",vec![]);}"#).unwrap());
    assert!(scan.refs.contains_key("global missingGlobal"));
}

#[test]
fn review_control_public_lighter_module_bindings_are_checked() {
    let ast = syn::parse_file(r#"fn body(h:&Host){ h.capture("missingPublicImport"); }"#).unwrap();
    let mut scan = Scan {
        capture_mode: "explicit-imports".into(),
        ..Scan::default()
    };
    scan.tables.insert(
        "MODULE_BINDINGS".into(),
        BTreeSet::from(["missingPublicImport".into()]),
    );
    scan.visit_file(&ast);
    assert!(scan.refs.contains_key("global missingPublicImport"));
}

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
#[path = "venue_method_gate/mod.rs"]
mod method_surface;

#[test]
fn compound_test_only_cfg_never_supplies_production_bindings() {
    let ast = syn::parse_file(r#"
        #[cfg(all(test, not(target_arch="wasm32")))] mod fixtures { fn install(g:&Js){ g.define("fakeModule",value); } }
        #[cfg(all(feature="native",test))] impl Host { fn install(g:&Js){g.define("fakeImpl",value);} }
        #[cfg(any(test,feature="native"))] fn install(g:&Js){g.define("real",value);}
        #[cfg(not(not(test)))] fn hidden(g:&Js){g.define("fakeNot",value);}
    "#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.globals.contains("real"));
    for fake in ["fakeModule", "fakeImpl", "fakeNot"] {
        assert!(!scan.globals.contains(fake), "{fake}");
    }
}

#[test]
fn review_control_function_debug_names_do_not_register_object_members() {
    let ast = syn::parse_file(
        r#"
        fn install(g:&Js) {
            let pair = |name:&str, value:Js| (name,value);
            g.define("Actual", Js::object(vec![pair("realMember",value)]));
            g.define("Factory", Js::function("missingMember", |_,_| {
                Js::object(vec![("returnedMember", value)])
            }));
        }
        fn production(h:&Host) { h.global("Factory.missingMember"); }
    "#,
    )
    .unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.globals.contains("Factory"));
    assert!(
        scan.globals.contains("Actual.realMember"),
        "tuple-producing helper supplies the actual member"
    );
    assert!(
        !scan.globals.contains("Factory.missingMember"),
        "debug name is not a member"
    );
    assert!(
        !scan.globals.contains("Factory.returnedMember"),
        "callback result is not a member"
    );
}

#[test]
fn review_control_test_only_tables_and_files_cannot_register_bindings() {
    let temp = std::env::temp_dir().join(format!(
        "venue-binding-cfg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let root = temp.join("safe-cex-venue-control/src");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("lib.rs"),
        r#"
        mod hidden;
        #[cfg(not(test))] const MEMBERS: &[(&str,i32)] = &[("realMember",1)];
        #[cfg(all(test,feature="native"))] const MEMBERS: &[(&str,i32)] = &[("fakeMember",1)];
        fn install(g:&Js) { g.define("Root", Js::object(MEMBERS)); }
    "#,
    )
    .unwrap();
    fs::write(
        root.join("hidden.rs"),
        r#"
        #![cfg(all(test,feature="native"))]
        fn install(g:&Js) { g.define("fakeFileBinding",value); }
    "#,
    )
    .unwrap();
    let observed = scan(&root, false);
    let admitted = files(&root);
    fs::remove_dir_all(temp).unwrap();
    assert!(
        observed.globals.contains("Root.realMember"),
        "production table is retained"
    );
    assert!(
        !observed.globals.contains("Root.fakeMember"),
        "test-only table cannot register members"
    );
    assert!(
        !observed.globals.contains("fakeFileBinding"),
        "inner cfg excludes registrations"
    );
    assert_eq!(
        admitted.len(),
        1,
        "inner cfg file is outside the production graph"
    );
}

#[test]
fn review_control_file_level_test_cfg_excludes_direct_scans() {
    let ast =
        syn::parse_file(r#"#![cfg(test)] fn install(g:&Js) {g.define("fake",value);}"#).unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(
        scan.globals.is_empty(),
        "file-level cfg(test) excludes direct AST scans"
    );
}

#[test]
fn review_control_tuple_helper_shadowing_respects_lexical_blocks_and_types() {
    let ast = syn::parse_file(
        r#"
        fn install(g:&Js) {
            let pair=|name,value|Js::function(name,value);
            { let pair=|name,value|(name,value); consume(pair("inner",value)); }
            g.define("Factory",pair("missingMember",callback));
            let pair=|name,value|(name,value);
            { let pair:NativeFn=make_function(); consume(pair("innerFunction",callback)); }
            g.define("Actual",Js::object(vec![pair("realMember",value)]));
            let pair:NativeFn=make_function();
            g.define("TypedFactory",pair("typedMissingMember",callback));
        }
    "#,
    )
    .unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(
        !scan.globals.contains("Factory.missingMember"),
        "nested tuple helper cannot classify outer function"
    );
    assert!(
        scan.globals.contains("Actual.realMember"),
        "outer tuple helper survives inner function shadow"
    );
    assert!(
        !scan.globals.contains("TypedFactory.typedMissingMember"),
        "typed shadow invalidates tuple helper"
    );
}

#[test]
fn review_control_test_only_match_arms_cannot_supply_production_intrinsics() {
    let ast = syn::parse_file(
        r#"
        fn intrinsic(name:&str) { match name {
            "real"=>value,
            #[cfg(all(test,feature="native"))] "fake"=>host.global("testOnlyReference"),
            _=>value,
        }}
    "#,
    )
    .unwrap();
    let mut scan = Scan {
        binding: true,
        ..Scan::default()
    };
    scan.visit_file(&ast);
    assert!(scan.intrinsics.contains("real"));
    assert!(
        !scan.intrinsics.contains("fake"),
        "test-only match arm is not a production intrinsic"
    );
    assert!(!scan.refs.contains_key("global testOnlyReference"));
}
