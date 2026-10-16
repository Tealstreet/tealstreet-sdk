//! Supplemental fail-closed checks for source scopes rustdoc does not expose.
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use syn::{
    spanned::Spanned,
    visit::{self, Visit},
};

/// File-relative public aliases, keyed by the compiler-compatible declaration end.
/// Only module items participate; function-local aliases belong to local probes.
pub fn public_aliases(source: &str) -> Result<Vec<(String, [usize; 2])>, syn::Error> {
    fn collect(items: &[syn::Item], scope: &str, output: &mut Vec<(String, [usize; 2])>) {
        for item in items {
            match item {
                syn::Item::Type(alias)
                    if super::public(&alias.vis) && !super::test_only(&alias.attrs) =>
                {
                    let end = alias.span().end();
                    output.push((
                        format!("{scope}{}", alias.ident),
                        [end.line, end.column + 1],
                    ));
                }
                syn::Item::Mod(module) if !super::test_only(&module.attrs) => {
                    if let Some((_, items)) = &module.content {
                        collect(items, &format!("{scope}{}::", module.ident), output);
                    }
                }
                _ => {}
            }
        }
    }
    let syntax = syn::parse_file(source)?;
    let mut output = Vec::new();
    if !super::test_only(&syntax.attrs) {
        collect(&syntax.items, "", &mut output);
    }
    Ok(output)
}

/// Declaration history is independent of active cfgs and decoding implementations.
pub fn nominal_names(source: &str) -> Result<Vec<String>, syn::Error> {
    struct Names(std::collections::BTreeSet<String>, bool);
    impl<'ast> Visit<'ast> for Names {
        fn visit_item_macro(&mut self, _: &'ast syn::ItemMacro) {
            self.1 = true;
        }
        fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
            fn has_path(tokens: proc_macro2::TokenStream) -> bool {
                tokens.into_iter().any(|token| match token {
                    proc_macro2::TokenTree::Ident(name) => {
                        name.to_string().trim_start_matches("r#") == "path"
                    }
                    proc_macro2::TokenTree::Group(group) => has_path(group.stream()),
                    _ => false,
                })
            }
            if attr.path().is_ident("path")
                || !known_attribute(&attr.meta)
                || (attr.path().is_ident("cfg_attr") && has_path(quote_meta(&attr.meta)))
            {
                self.1 = true;
            }
        }
        fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
            self.0
                .insert(item.ident.to_string().trim_start_matches("r#").to_owned());
            visit::visit_item_struct(self, item);
        }
        fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
            self.0
                .insert(item.ident.to_string().trim_start_matches("r#").to_owned());
            visit::visit_item_enum(self, item);
        }
        fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
            self.0
                .insert(item.ident.to_string().trim_start_matches("r#").to_owned());
            visit::visit_item_union(self, item);
        }
    }
    let ast = syn::parse_file(source)?;
    let mut expansion = LocalTypes::default();
    expansion.visit_file(&ast);
    let mut names = Names(Default::default(), false);
    names.visit_file(&ast);
    if names.1
        || expansion
            .names
            .iter()
            .any(|(name, _, _)| name.starts_with("UNSUPPORTED_"))
    {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "historical owner expansion or path modules require explicit birth provenance",
        ));
    }
    Ok(names.0.into_iter().collect())
}

/// Check source modules independently of rustdoc's `doc` configuration.
pub fn doc_guards(root: &Path) -> Result<(), String> {
    fn file(
        path: &Path,
        root: bool,
        seen: &mut std::collections::BTreeSet<std::path::PathBuf>,
    ) -> Result<(), String> {
        let canonical = path
            .canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))?;
        if !seen.insert(canonical) {
            return Ok(());
        }
        let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let ast = syn::parse_file(&source).map_err(|error| error.to_string())?;
        let mut guards = DocGuards(false);
        guards.visit_file(&ast);
        if guards.0 {
            return Err(format!(
                "{}: accepted-set assertion: DOC_CONFIGURATION changes production discovery",
                path.display()
            ));
        }
        let parent = path.parent().ok_or("source parent missing")?;
        let directory = if root || path.file_name().is_some_and(|name| name == "mod.rs") {
            parent.to_path_buf()
        } else {
            parent.join(path.file_stem().ok_or("source stem missing")?)
        };
        modules(&ast.items, &directory, parent, seen)
    }
    fn modules(
        items: &[syn::Item],
        directory: &Path,
        path_directory: &Path,
        seen: &mut std::collections::BTreeSet<std::path::PathBuf>,
    ) -> Result<(), String> {
        for item in items {
            let syn::Item::Mod(module) = item else {
                continue;
            };
            if super::test_only(&module.attrs) {
                continue;
            }
            if module
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("cfg_attr"))
            {
                return Err(format!(
                    "{}: accepted-set assertion: CONDITIONAL_MODULE requires explicit source paths",
                    module.ident
                ));
            }
            let explicit = module
                .attrs
                .iter()
                .find(|attr| attr.path().is_ident("path"))
                .map(|attr| {
                    if let syn::Meta::NameValue(value) = &attr.meta
                        && let syn::Expr::Lit(value) = &value.value
                        && let syn::Lit::Str(value) = &value.lit
                    {
                        Ok(path_directory.join(value.value()))
                    } else {
                        Err("invalid module path".to_string())
                    }
                })
                .transpose()?;
            if let Some((_, items)) = &module.content {
                let nested = explicit.unwrap_or_else(|| directory.join(module.ident.to_string()));
                modules(items, &nested, &nested, seen)?;
            } else {
                let source = explicit.unwrap_or_else(|| {
                    let flat = directory.join(format!("{}.rs", module.ident));
                    if flat.exists() {
                        flat
                    } else {
                        directory.join(module.ident.to_string()).join("mod.rs")
                    }
                });
                file(&source, false, seen)?;
            }
        }
        Ok(())
    }
    file(root, true, &mut std::collections::BTreeSet::new())
}

struct DocGuards(bool);
impl<'ast> Visit<'ast> for DocGuards {
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if !super::test_only(&item.attrs) {
            visit::visit_item_macro(self, item);
        }
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if !super::test_only(&item.attrs) {
            visit::visit_item_static(self, item);
        }
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        // Rejection-only inspection before rustdoc can erase cfg(doc) items.
        // Permission still requires the independent compiler-origin check.
        // The bounded numeric registration admits only literal `self`, never
        // an opaque expression that could hide a doc-erased declaration.
        if number_output_name(item) && !number_output_grammar(item.tokens.clone()) {
            self.0 = true;
        }
        if item
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "thread_local")
        {
            if let Ok(file) = syn::parse2::<syn::File>(item.tokens.clone()) {
                visit::visit_file(self, &file);
            }
        }
        visit::visit_macro(self, item);
    }
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().get_ident().is_some_and(|name| {
            matches!(
                name.to_string().trim_start_matches("r#"),
                "cfg" | "cfg_attr"
            )
        }) && !known_attribute(&attr.meta)
        {
            self.0 = true;
        }
    }
}

#[derive(Default)]
struct LocalMacroEffects {
    unsupported: bool,
    attributes: Vec<syn::Attribute>,
    macros: Vec<syn::Macro>,
}
impl<'ast> Visit<'ast> for LocalMacroEffects {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if matches!(
            item,
            syn::Item::Impl(_)
                | syn::Item::Trait(_)
                | syn::Item::TraitAlias(_)
                | syn::Item::Mod(_)
                | syn::Item::Use(_)
                | syn::Item::ExternCrate(_)
                | syn::Item::ForeignMod(_)
                | syn::Item::Verbatim(_)
        ) || matches!(item, syn::Item::Macro(item) if item.mac.path.is_ident("macro_rules"))
        {
            self.unsupported = true;
            return;
        }
        visit::visit_item(self, item);
    }
    fn visit_expr(&mut self, expression: &'ast syn::Expr) {
        if matches!(expression, syn::Expr::Verbatim(_)) {
            self.unsupported = true;
            return;
        }
        visit::visit_expr(self, expression);
    }
    fn visit_type(&mut self, ty: &'ast syn::Type) {
        if matches!(ty, syn::Type::Verbatim(_)) {
            self.unsupported = true;
            return;
        }
        visit::visit_type(self, ty);
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        // Ordinary declaration attributes have compiler coverage; an opaque
        // recipe does not. Keep only bounded derives and inert attributes here.
        let inert = attribute.path().get_ident().is_some_and(|name| {
            matches!(
                name.to_string().as_str(),
                "allow"
                    | "expect"
                    | "warn"
                    | "deny"
                    | "forbid"
                    | "doc"
                    | "cfg"
                    | "serde"
                    | "derive"
                    | "repr"
                    | "inline"
                    | "cold"
                    | "must_use"
                    | "non_exhaustive"
                    | "deprecated"
                    | "default"
            )
        });
        self.unsupported |= !inert || !known_attribute(&attribute.meta);
        self.attributes.push(attribute.clone());
    }
    fn visit_macro(&mut self, invocation: &'ast syn::Macro) {
        self.macros.push(invocation.clone());
    }
}

// A deliberately bounded invocation grammar, not a macro_rules expander.
// Other matchers/arms/forwarding require explicit effect support before use.
fn local_macro_body(tokens: proc_macro2::TokenStream) -> Option<syn::Block> {
    use proc_macro2::{Delimiter, TokenStream, TokenTree};
    let mut tokens = tokens.into_iter();
    let TokenTree::Group(matcher) = tokens.next()? else {
        return None;
    };
    if matcher.delimiter() != Delimiter::Parenthesis || !matcher.stream().is_empty() {
        return None;
    }
    let (TokenTree::Punct(equal), TokenTree::Punct(arrow), TokenTree::Group(body)) =
        (tokens.next()?, tokens.next()?, tokens.next()?)
    else {
        return None;
    };
    if equal.as_char() != '=' || arrow.as_char() != '>' || body.delimiter() != Delimiter::Brace {
        return None;
    }
    if let Some(separator) = tokens.next() {
        if !matches!(separator, TokenTree::Punct(separator) if separator.as_char() == ';')
            || tokens.next().is_some()
        {
            return None;
        }
    }
    syn::parse2::<syn::Block>(TokenStream::from(TokenTree::Group(body))).ok()
}

/// Module-scope `macro_rules!` recipes proven to expand to expressions only,
/// keyed by name, with every arm's transcriber for nested-invocation checks.
pub type ModuleRecipes = HashMap<String, proc_macro2::TokenStream>;

fn fragment_named(tokens: proc_macro2::TokenStream, fragment: &str) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();
    tokens.iter().enumerate().any(|(index, token)| match token {
        proc_macro2::TokenTree::Ident(name) => {
            name == fragment
                && index > 0
                && matches!(&tokens[index - 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
        }
        proc_macro2::TokenTree::Group(group) => fragment_named(group.stream(), fragment),
        _ => false,
    })
}

// Every arm must transcribe without a declaration keyword or `item` fragment;
// arguments are keyword-checked per invocation, so no expansion declares items.
fn expression_recipe(tokens: proc_macro2::TokenStream) -> Option<proc_macro2::TokenStream> {
    use proc_macro2::TokenTree;
    let tokens: Vec<_> = tokens.into_iter().collect();
    let mut transcribers = proc_macro2::TokenStream::new();
    let mut index = 0;
    while index < tokens.len() {
        let (
            TokenTree::Group(matcher),
            Some(TokenTree::Punct(equal)),
            Some(TokenTree::Punct(arrow)),
            Some(TokenTree::Group(body)),
        ) = (
            &tokens[index],
            tokens.get(index + 1),
            tokens.get(index + 2),
            tokens.get(index + 3),
        )
        else {
            return None;
        };
        if equal.as_char() != '='
            || arrow.as_char() != '>'
            || fragment_named(matcher.stream(), "item")
            || declarations_in_tokens(body.stream())
        {
            return None;
        }
        transcribers.extend(body.stream());
        index += 4;
        match tokens.get(index) {
            Some(TokenTree::Punct(separator)) if separator.as_char() == ';' => index += 1,
            None => {}
            Some(_) => return None,
        }
    }
    (index > 0).then_some(transcribers)
}

/// Collects module-scope recipes across one compiler target's source files.
/// A name is usable only when EVERY definition of it is an unconditional
/// expression recipe that does not shadow a trusted builtin spelling.
pub fn module_recipes(paths: &[&Path]) -> super::Result<ModuleRecipes> {
    #[derive(Default)]
    struct Collector(Vec<(String, Option<proc_macro2::TokenStream>)>);
    impl<'ast> Visit<'ast> for Collector {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if !super::test_only(&item.attrs) {
                visit::visit_item_mod(self, item);
            }
        }
        fn visit_item_fn(&mut self, _: &'ast syn::ItemFn) {}
        fn visit_item_impl(&mut self, _: &'ast syn::ItemImpl) {}
        fn visit_item_trait(&mut self, _: &'ast syn::ItemTrait) {}
        fn visit_item_const(&mut self, _: &'ast syn::ItemConst) {}
        fn visit_item_static(&mut self, _: &'ast syn::ItemStatic) {}
        fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
            let Some(name) = item.ident.as_ref() else {
                return;
            };
            if !item.mac.path.is_ident("macro_rules") || super::test_only(&item.attrs) {
                return;
            }
            let name = name.to_string().trim_start_matches("r#").to_owned();
            let unconditional = item.attrs.iter().all(|attr| {
                attr.path().get_ident().is_some_and(|name| {
                    matches!(name.to_string().as_str(), "allow" | "doc" | "macro_export")
                })
            });
            let trusted = expression_macro(&name) || name == "js_number_output";
            let recipe = (unconditional && !trusted)
                .then(|| expression_recipe(item.mac.tokens.clone()))
                .flatten();
            self.0.push((name, recipe));
        }
    }
    let mut collector = Collector::default();
    for path in paths {
        let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let syntax = syn::parse_file(&source).map_err(|error| error.to_string())?;
        collector.visit_file(&syntax);
    }
    let mut recipes: HashMap<String, Option<proc_macro2::TokenStream>> = HashMap::new();
    for (name, recipe) in collector.0 {
        match (recipes.get_mut(&name), recipe) {
            (None, recipe) => {
                recipes.insert(name, recipe);
            }
            (Some(Some(existing)), Some(recipe)) => existing.extend(recipe),
            (Some(slot), _) => *slot = None,
        }
    }
    Ok(recipes
        .into_iter()
        .filter_map(|(name, recipe)| recipe.map(|recipe| (name, recipe)))
        .collect())
}

#[derive(Default)]
struct LocalTypes {
    depth: usize,
    verified_macro_origins: HashSet<String>,
    // A conditional nearest binding is present but unsupported (None). It must
    // never fall through to an outer recipe or a builtin of the same name.
    local_macro_scopes: Vec<HashMap<String, Option<proc_macro2::TokenStream>>>,
    inside_local_recipe: bool,
    names: Vec<(String, usize, usize)>,
    untrusted_macro_import: bool,
    unqualified_macro: bool,
    untrusted_attribute_import: bool,
    macro_attribute: bool,
    generated_provider_file: bool,
    generated_provider_module: bool,
    number_output_used: bool,
    glob_import: bool,
    serde_forward_used: bool,
    number_output_imported: bool,
    number_output_shadowed: bool,
    module_recipes: ModuleRecipes,
    foreign_imports: HashSet<String>,
    deserialize_owners: Vec<String>,
    inspected_recipes: HashSet<String>,
}
impl LocalTypes {
    fn verified_thread_local(&self, item: &syn::Macro) -> bool {
        self.verified_macro_origins.contains("std")
            && (item.path.is_ident("thread_local")
                || item.path.segments.len() == 2
                    && item.path.segments[0].ident == "std"
                    && item.path.segments[1].ident == "thread_local")
    }
    fn inspect_thread_local(&mut self, item: &syn::Macro) {
        let end = item.span().end();
        let Ok(mut file) = syn::parse2::<syn::File>(item.tokens.clone()) else {
            self.names.push((
                "UNSUPPORTED_THREAD_LOCAL_GRAMMAR".into(),
                end.line,
                end.column,
            ));
            return;
        };
        if !file.attrs.is_empty()
            || file
                .items
                .iter()
                .any(|item| !matches!(item, syn::Item::Static(_)))
        {
            self.names.push((
                "UNSUPPORTED_THREAD_LOCAL_GRAMMAR".into(),
                end.line,
                end.column,
            ));
            return;
        }
        file.items.retain(
            |item| !matches!(item, syn::Item::Static(item) if super::test_only(&item.attrs)),
        );
        if file.items.iter().any(|item| {
            matches!(item, syn::Item::Static(item) if !matches!(item.vis, syn::Visibility::Inherited))
        }) {
            self.names.push((
                "UNSUPPORTED_THREAD_LOCAL_VISIBILITY".into(),
                end.line,
                end.column,
            ));
            return;
        }
        let mut guards = DocGuards(false);
        guards.visit_file(&file);
        if guards.0 {
            self.names.push((
                "UNSUPPORTED_THREAD_LOCAL_DOC_CONFIGURATION".into(),
                end.line,
                end.column,
            ));
            return;
        }
        // Original token spans and lexical depth allow the ordinary visitors
        // to inspect initializer functions, types, attributes and macro calls.
        visit::visit_file(self, &file);
    }
    fn inspect_macro(&mut self, item: &syn::Macro) {
        if number_output_name(item) {
            self.number_output_used = true;
            let verified = item.path.segments.len() == 2
                && item
                    .path
                    .segments
                    .iter()
                    .all(|part| part.arguments.is_empty())
                && ((item.path.segments[0].ident == "safe_cex_num"
                    && self
                        .verified_macro_origins
                        .contains("safe_cex_num::js_number_output"))
                    || (item.path.leading_colon.is_none()
                        && item.path.segments[0].ident == "crate"
                        && self
                            .verified_macro_origins
                            .contains("safe_cex_num::js_number_output::self")))
                || (item.path.segments.len() == 1
                    && item.path.leading_colon.is_none()
                    && item.path.segments[0].arguments.is_empty()
                    && self.number_output_imported
                    && !self.number_output_shadowed
                    && self
                        .verified_macro_origins
                        .contains("safe_cex_num::js_number_output"));
            if !verified || !number_output_grammar(item.tokens.clone()) {
                let end = item.span().end();
                self.names.push((
                    "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR".into(),
                    end.line,
                    end.column,
                ));
            }
            return;
        }
        let qualified = item.path.segments.len() > 1;
        let local_macro = (!qualified)
            .then(|| item.path.segments.first())
            .flatten()
            .and_then(|name| {
                let name = name.ident.to_string();
                self.local_macro_scopes
                    .iter()
                    .rev()
                    .find_map(|scope| scope.get(name.trim_start_matches("r#")))
                    .cloned()
            });
        if let Some(recipe) = local_macro {
            let body = recipe.and_then(local_macro_body);
            let mut effects = LocalMacroEffects::default();
            if let Some(body) = &body {
                // No cfg filtering: hidden impls must be rejected too.
                effects.visit_block(body);
            }
            if self.inside_local_recipe
                || !item.tokens.is_empty()
                || body.is_none()
                || effects.unsupported
            {
                let end = item.span().end();
                self.names.push((
                    format!(
                        "UNSUPPORTED_LOCAL_MACRO_EFFECTS:{}",
                        item.path.segments[0].ident
                    ),
                    end.line,
                    end.column,
                ));
                return;
            }
            for attribute in effects.attributes {
                self.visit_attribute(&attribute);
            }
            self.inside_local_recipe = true;
            for invocation in effects.macros {
                self.inspect_macro(&invocation);
            }
            self.inside_local_recipe = false;
            return;
        }
        let local_path = item.path.leading_colon.is_none()
            && item
                .path
                .segments
                .iter()
                .rev()
                .skip(1)
                .enumerate()
                .all(|(index, part)| {
                    index + 2 < item.path.segments.len()
                        || matches!(part.ident.to_string().as_str(), "crate" | "self" | "super")
                });
        let recipe_name = item
            .path
            .segments
            .last()
            .map(|part| part.ident.to_string().trim_start_matches("r#").to_owned())
            .unwrap_or_default();
        if local_path
            && !(!qualified && self.foreign_imports.contains(&recipe_name))
            && let Some(transcriber) = self.module_recipes.get(&recipe_name).cloned()
        {
            if !qualified {
                self.unqualified_macro = true;
            }
            if declarations_in_tokens(item.tokens.clone()) {
                let end = item.span().end();
                self.names.push((
                    format!("UNSUPPORTED_LOCAL_MACRO:{recipe_name}"),
                    end.line,
                    end.column,
                ));
            }
            if self.inspected_recipes.insert(recipe_name) {
                for nested in nested_macro_invocations(transcriber) {
                    self.inspect_macro(&nested);
                }
            }
            for nested in nested_macro_invocations(item.tokens.clone()) {
                self.inspect_macro(&nested);
            }
            return;
        }
        let name = item
            .path
            .segments
            .last()
            .map(|part| part.ident.to_string())
            .unwrap_or_default();
        // Reviewed serde 1.0.229 expands this identifier-only form solely to
        // forwarding Deserializer methods. Rustdoc inventories those methods;
        // this permission neither classifies them nor admits arbitrary items.
        if name == "forward_to_deserialize_any"
            && item.path.segments.len() == 2
            && item.path.segments[0].ident == "serde"
            && item
                .path
                .segments
                .iter()
                .all(|part| part.arguments.is_empty())
            && self.verified_macro_origins.contains("serde")
            && serde_forward_grammar(item.tokens.clone())
        {
            self.serde_forward_used = true;
            return;
        }
        let origin = if qualified {
            item.path
                .segments
                .first()
                .map(|part| part.ident.to_string())
                .unwrap_or_default()
        } else {
            match name.as_str() {
                "json" => "serde_json".into(),
                "Token" => "syn".into(),
                _ if self.verified_macro_origins.contains("std") => "std".into(),
                _ => "core".into(),
            }
        };
        let standard = self.verified_macro_origins.contains(&origin)
            && match origin.as_str() {
                "std" | "core" => !matches!(name.as_str(), "json" | "Token"),
                "serde_json" => name == "json",
                // syn 2.0.119/3.0.7 grouping arms are a match on private parse helpers.
                "syn" => matches!(
                    name.as_str(),
                    "Token" | "braced" | "bracketed" | "parenthesized"
                ),
                _ => false,
            };
        if !qualified {
            self.unqualified_macro = true;
        }
        if self.verified_thread_local(item) {
            self.inspect_thread_local(item);
            return;
        }
        let token_keyword = item
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "Token")
            && token_keyword_grammar(item.tokens.clone());
        if !standard
            || !item
                .path
                .segments
                .last()
                .is_some_and(|segment| expression_macro(&segment.ident.to_string()))
            || (!token_keyword && declarations_in_tokens(item.tokens.clone()))
        {
            let end = item.span().end();
            self.names.push((
                format!(
                    "UNSUPPORTED_LOCAL_MACRO:{}",
                    item.path
                        .segments
                        .last()
                        .map(|part| part.ident.to_string())
                        .unwrap_or_default()
                ),
                end.line,
                end.column,
            ));
        }
        for nested in nested_macro_invocations(item.tokens.clone()) {
            self.inspect_macro(&nested);
        }
    }
}

impl<'ast> Visit<'ast> for LocalTypes {
    fn visit_file(&mut self, file: &'ast syn::File) {
        visit::visit_file(self, file);
        if self.untrusted_attribute_import && self.macro_attribute {
            self.names
                .push(("UNSUPPORTED_ATTRIBUTE_IMPORT_SCOPE".into(), 1, 0));
        }
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().is_ident("derive")
            || attr.path().is_ident("cfg_attr")
            || attr.path().is_ident("wasm_bindgen")
            || attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "clippy"
                && attr.path().segments[1].ident == "msrv"
            || attr.path().segments.len() == 2
                && attr.path().segments[0].ident == "rustfmt"
                && attr.path().segments[1].ident == "skip"
        {
            self.macro_attribute = true;
        }
        let attribute_name = attr
            .path()
            .segments
            .iter()
            .map(|part| part.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        let pinned_provider_attribute = (attribute_name == "clippy::msrv"
            && self.generated_provider_file)
            || (attribute_name == "rustfmt::skip" && self.generated_provider_module);
        if !known_attribute(&attr.meta)
            || is_generated_tool_attribute(&attr.meta) && !pinned_provider_attribute
        {
            let end = attr.span().end();
            let attribute = attr
                .path()
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            self.names.push((
                format!("UNSUPPORTED_ATTRIBUTE_EXPANSION:{attribute}"),
                end.line,
                end.column,
            ));
        }
        visit::visit_attribute(self, attr);
    }

    fn visit_item(&mut self, item: &'ast syn::Item) {
        if let syn::Item::Mod(module) = item
            && super::test_only(&module.attrs)
        {
            return;
        }
        visit::visit_item(self, item);
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if super::test_only(&item.attrs)
            || item.attrs.iter().any(|attr| attr.path().is_ident("test"))
        {
            return;
        }
        if self.depth > 0 {
            let end = item.span().end();
            self.names.push((
                format!("LOCAL_FUNCTION:{}", item.sig.ident),
                end.line,
                end.column,
            ));
        }
        self.depth += 1;
        visit::visit_item_fn(self, item);
        self.depth -= 1;
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.depth += 1;
        visit::visit_impl_item_fn(self, item);
        self.depth -= 1;
    }
    fn visit_block(&mut self, block: &'ast syn::Block) {
        self.depth += 1;
        self.local_macro_scopes.push(HashMap::new());
        visit::visit_block(self, block);
        self.local_macro_scopes.pop();
        self.depth -= 1;
    }
    fn visit_item_macro(&mut self, item: &'ast syn::ItemMacro) {
        if super::test_only(&item.attrs) {
            return;
        }
        if self.verified_thread_local(&item.mac) {
            for attr in &item.attrs {
                self.visit_attribute(attr);
            }
            self.inspect_macro(&item.mac);
            return;
        }
        if item.mac.path.is_ident("macro_rules") {
            if self.depth > 0 {
                if let (Some(scope), Some(name)) =
                    (self.local_macro_scopes.last_mut(), item.ident.as_ref())
                {
                    let conditional = item.attrs.iter().any(|attribute| {
                        attribute.path().get_ident().is_some_and(|name| {
                            matches!(
                                name.to_string().trim_start_matches("r#"),
                                "cfg" | "cfg_attr"
                            )
                        })
                    });
                    scope.insert(
                        name.to_string().trim_start_matches("r#").to_owned(),
                        (!conditional).then(|| item.mac.tokens.clone()),
                    );
                }
            } else if !item.ident.as_ref().is_some_and(|name| {
                self.module_recipes
                    .contains_key(name.to_string().trim_start_matches("r#"))
            }) {
                let end = item.span().end();
                self.names.push((
                    format!(
                        "UNSUPPORTED_MODULE_MACRO:{}",
                        item.ident
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "macro_rules".into())
                    ),
                    end.line,
                    end.column,
                ));
            }
            for attr in &item.attrs {
                self.visit_attribute(attr);
            }
            return;
        }
        if self.depth == 0 && item.ident.is_none() {
            let parts = item
                .mac
                .path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>();
            let diagnostic = parts.last().is_some_and(|part| part == "compile_error")
                && (parts.len() == 1
                    || parts.len() == 2 && matches!(parts[0].as_str(), "std" | "core"));
            if diagnostic {
                self.unqualified_macro |= parts.len() == 1;
            } else {
                let end = item.span().end();
                self.names.push((
                    format!("UNSUPPORTED_MODULE_MACRO_INVOCATION:{}", parts.join("::")),
                    end.line,
                    end.column,
                ));
            }
        }
        if (self.depth > 0 && item.ident.is_some())
            || item
                .ident
                .as_ref()
                .is_some_and(|name| expression_macro(&name.to_string()))
        {
            let end = item.span().end();
            self.names.push((
                format!(
                    "UNSUPPORTED_LOCAL_MACRO:{}",
                    item.ident
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "invocation".into())
                ),
                end.line,
                end.column,
            ));
        }
        visit::visit_item_macro(self, item);
    }
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let previous = self.generated_provider_module;
        self.generated_provider_module = item.ident == "data"
            && item.attrs.iter().any(|attr| {
                attr.path().is_ident("path")
                    && matches!(&attr.meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(expr) if matches!(&expr.lit, syn::Lit::Str(path) if path.value() == "collation_data/provider.rs")))
            });
        if self.depth > 0 && item.content.is_none() {
            let end = item.span().end();
            self.names.push((
                "UNSUPPORTED_LOCAL_EXTERNAL_MODULE".into(),
                end.line,
                end.column,
            ));
        }
        if trusted_namespace(&item.ident.to_string()) {
            let end = item.span().end();
            self.names.push((
                "UNSUPPORTED_MACRO_NAMESPACE_SHADOW".into(),
                end.line,
                end.column,
            ));
        }
        self.local_macro_scopes.push(HashMap::new());
        visit::visit_item_mod(self, item);
        self.local_macro_scopes.pop();
        self.generated_provider_module = previous;
    }
    fn visit_item_extern_crate(&mut self, item: &'ast syn::ItemExternCrate) {
        if trusted_namespace(&item.ident.to_string())
            || item
                .rename
                .as_ref()
                .is_some_and(|(_, name)| trusted_namespace(&name.to_string()))
        {
            let end = item.span().end();
            self.names.push((
                "UNSUPPORTED_MACRO_NAMESPACE_SHADOW".into(),
                end.line,
                end.column,
            ));
        }
        visit::visit_item_extern_crate(self, item);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        fn glob(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Glob(_) => true,
                syn::UseTree::Path(path) => glob(&path.tree),
                syn::UseTree::Group(group) => group.items.iter().any(glob),
                _ => false,
            }
        }
        self.glob_import |= glob(&item.tree);
        // Only a direct `safe_cex_num::js_number_output` binding authorizes the
        // unqualified spelling; any other binding of that name revokes it.
        fn number_output_bindings(
            tree: &syn::UseTree,
            parent: Option<&syn::Ident>,
            verified: &mut bool,
            other: &mut bool,
        ) {
            match tree {
                syn::UseTree::Name(name) if name.ident == "js_number_output" => {
                    if parent.is_some_and(|parent| parent == "safe_cex_num") {
                        *verified = true;
                    } else {
                        *other = true;
                    }
                }
                syn::UseTree::Rename(name)
                    if name.rename == "js_number_output" || name.ident == "js_number_output" =>
                {
                    *other = true
                }
                syn::UseTree::Group(group) => {
                    for tree in &group.items {
                        number_output_bindings(tree, parent, verified, other);
                    }
                }
                syn::UseTree::Path(path) => {
                    number_output_bindings(&path.tree, Some(&path.ident), verified, other)
                }
                _ => {}
            }
        }
        let (mut verified, mut other) = (false, false);
        number_output_bindings(&item.tree, None, &mut verified, &mut other);
        let rooted = matches!(&item.tree, syn::UseTree::Path(path) if path.ident == "safe_cex_num");
        self.number_output_imported |= verified && rooted;
        self.number_output_shadowed |= other || (verified && !rooted);
        fn foreign_leaves(tree: &syn::UseTree, local: bool, output: &mut HashSet<String>) {
            match tree {
                syn::UseTree::Name(name) if !local => {
                    output.insert(name.ident.to_string().trim_start_matches("r#").to_owned());
                }
                syn::UseTree::Rename(name) if !local => {
                    output.insert(name.rename.to_string().trim_start_matches("r#").to_owned());
                }
                syn::UseTree::Group(group) => {
                    for tree in &group.items {
                        foreign_leaves(tree, local, output);
                    }
                }
                syn::UseTree::Path(path) => foreign_leaves(&path.tree, local, output),
                _ => {}
            }
        }
        let local_root = item.leading_colon.is_none()
            && matches!(&item.tree, syn::UseTree::Path(path) if matches!(path.ident.to_string().as_str(), "crate" | "self" | "super"));
        foreign_leaves(&item.tree, local_root, &mut self.foreign_imports);
        fn namespace_alias(tree: &syn::UseTree, parent: Option<&syn::Ident>) -> bool {
            match tree {
                syn::UseTree::Name(name) => {
                    trusted_namespace(&name.ident.to_string())
                        || name.ident == "self"
                            && parent.is_some_and(|name| trusted_namespace(&name.to_string()))
                }
                syn::UseTree::Rename(name) => trusted_namespace(&name.rename.to_string()),
                syn::UseTree::Group(group) => {
                    group.items.iter().any(|tree| namespace_alias(tree, parent))
                }
                syn::UseTree::Path(path) => namespace_alias(&path.tree, Some(&path.ident)),
                syn::UseTree::Glob(_) => false,
            }
        }
        if namespace_alias(&item.tree, None) {
            let end = item.span().end();
            self.names.push((
                "UNSUPPORTED_MACRO_NAMESPACE_SHADOW".into(),
                end.line,
                end.column,
            ));
        }
        fn attribute_binding(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Name(name) => attribute_macro_name(&name.ident.to_string()),
                syn::UseTree::Rename(name) => attribute_macro_name(&name.rename.to_string()),
                syn::UseTree::Group(group) => group.items.iter().any(attribute_binding),
                syn::UseTree::Path(path) => attribute_binding(&path.tree),
                syn::UseTree::Glob(_) => true,
            }
        }
        let attribute_origin = matches!(&item.tree, syn::UseTree::Path(path) if matches!(path.ident.to_string().as_str(), "std" | "core" | "serde" | "ts_rs" | "wasm_bindgen" | "napi_derive"));
        if !attribute_origin && attribute_binding(&item.tree) {
            self.untrusted_attribute_import = true;
        }
        fn ambiguous(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Name(name) => expression_macro(&name.ident.to_string()),
                syn::UseTree::Rename(name) => expression_macro(&name.rename.to_string()),
                syn::UseTree::Glob(_) => true,
                syn::UseTree::Group(group) => group.items.iter().any(ambiguous),
                syn::UseTree::Path(path) => ambiguous(&path.tree),
            }
        }
        fn renamed_macro(tree: &syn::UseTree) -> bool {
            match tree {
                syn::UseTree::Rename(name) => {
                    name.ident != name.rename && expression_macro(&name.rename.to_string())
                }
                syn::UseTree::Path(path) => renamed_macro(&path.tree),
                syn::UseTree::Group(group) => group.items.iter().any(renamed_macro),
                _ => false,
            }
        }
        // Verified crate identity cannot certify a different imported macro's
        // grammar merely because its destination binding resembles a builtin.
        if renamed_macro(&item.tree) {
            self.untrusted_macro_import = true;
        }
        let standard = matches!(&item.tree, syn::UseTree::Path(path) if self.verified_macro_origins.contains(&path.ident.to_string()));
        if !standard && ambiguous(&item.tree) {
            self.untrusted_macro_import = true;
        }
        visit::visit_item_use(self, item);
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        self.inspect_macro(item);
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if self.depth > 0 {
            let end = item.span().end();
            self.names.push((
                format!("UNSUPPORTED_LOCAL_TRAIT:{}", item.ident),
                end.line,
                end.column,
            ));
        }
        visit::visit_item_trait(self, item);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if self.depth > 0 && item.trait_.is_none() {
            let end = item.span().end();
            self.names
                .push(("UNSUPPORTED_LOCAL_INHERENT".into(), end.line, end.column));
        }
        if self.depth > 0
            && let Some((path, _)) = &item.trait_
        {
            let name = path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if !matches!(
                name.as_str(),
                "serde::Deserialize" | "serde::de::Deserialize" | "std::str::FromStr"
            ) {
                let end = item.span().end();
                // A serde helper inside a module-level Deserialize impl names
                // that impl's owner, whose own probes drive the helper.
                let helper = matches!(
                    path.segments
                        .last()
                        .map(|part| part.ident.to_string())
                        .as_deref(),
                    Some("Visitor" | "DeserializeSeed")
                );
                let enclosing = self
                    .deserialize_owners
                    .last()
                    .filter(|_| helper)
                    .map(|owner| format!("@{owner}"))
                    .unwrap_or_default();
                self.names.push((
                    format!(
                        "LOCAL_TRAIT_IMPL:{name}:{}{enclosing}",
                        match &*item.self_ty {
                            syn::Type::Path(owner) => owner
                                .path
                                .segments
                                .last()
                                .map(|part| part.ident.to_string())
                                .unwrap_or_default(),
                            _ => "unsupported".to_owned(),
                        }
                    ),
                    end.line,
                    end.column,
                ));
            }
        }
        let owner = (self.depth == 0)
            .then_some(())
            .and(item.trait_.as_ref())
            .filter(|(path, _)| {
                path.segments
                    .last()
                    .is_some_and(|part| part.ident == "Deserialize")
            })
            .and(match &*item.self_ty {
                syn::Type::Path(owner) if owner.qself.is_none() => owner
                    .path
                    .segments
                    .last()
                    .map(|part| part.ident.to_string()),
                _ => None,
            });
        if let Some(owner) = &owner {
            self.deserialize_owners.push(owner.clone());
        }
        visit::visit_item_impl(self, item);
        if owner.is_some() {
            self.deserialize_owners.pop();
        }
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if self.depth > 0 {
            let end = item.span().end();
            self.names.push((
                if item.generics.params.is_empty() && !item.attrs.iter().any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr") || matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string().contains("remote"))) {
                    item.ident.to_string()
                } else {
                    format!("UNSUPPORTED_GENERIC:{}", item.ident)
                },
                end.line,
                end.column,
            ));
        }
        visit::visit_item_struct(self, item);
    }
    fn visit_item_union(&mut self, item: &'ast syn::ItemUnion) {
        if self.depth > 0 {
            let end = item.span().end();
            self.names.push((
                format!("UNSUPPORTED_LOCAL_UNION:{}", item.ident),
                end.line,
                end.column,
            ));
        }
        visit::visit_item_union(self, item);
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if self.depth > 0 {
            let end = item.span().end();
            self.names.push((
                if item.generics.params.is_empty() && !item.attrs.iter().any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr") || matches!(&attr.meta, syn::Meta::List(list) if list.tokens.to_string().contains("remote"))) {
                    item.ident.to_string()
                } else {
                    format!("UNSUPPORTED_GENERIC:{}", item.ident)
                },
                end.line,
                end.column,
            ));
        }
        visit::visit_item_enum(self, item);
    }
}

fn known_attribute(meta: &syn::Meta) -> bool {
    use syn::parse::Parser;
    let name = meta
        .path()
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    if name == "cfg" {
        return !matches!(meta, syn::Meta::List(list) if mentions_doc(list.tokens.clone()));
    }
    if name == "cfg_attr" {
        return matches!(meta, syn::Meta::List(list) if syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated.parse2(list.tokens.clone()).is_ok_and(|items| items.first().is_some_and(|predicate| !mentions_doc(quote_meta(predicate))) && items.iter().skip(1).all(known_attribute)));
    }
    if name == "derive" {
        return matches!(meta, syn::Meta::List(list) if syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated.parse2(list.tokens.clone()).is_ok_and(|items| items.iter().all(|path| {
            let name = path.segments.iter().map(|part| part.ident.to_string()).collect::<Vec<_>>().join("::");
            matches!(name.as_str(), "Debug" | "Default" | "Clone" | "Copy" | "PartialEq" | "Eq" | "PartialOrd" | "Ord" | "Hash" | "Serialize" | "Deserialize" | "serde::Serialize" | "serde::Deserialize" | "TS" | "ts_rs::TS")
        })));
    }
    // This exact tool attribute is emitted by the pinned ICU4X provider generator.
    // Keep its accepted syntax narrow so aliases or malformed values still fail closed.
    if name == "clippy::msrv" {
        return matches!(meta, syn::Meta::NameValue(value) if matches!(&value.value, syn::Expr::Lit(expr) if matches!(&expr.lit, syn::Lit::Str(version) if version.value() == "1.88")));
    }
    if name == "rustfmt::skip" {
        return matches!(meta, syn::Meta::Path(_));
    }
    // Known SDK binding macros are compiler-inventoried; arbitrary expansion fails closed.
    matches!(
        name.as_str(),
        "allow"
            | "expect"
            | "warn"
            | "deny"
            | "forbid"
            | "doc"
            | "cfg"
            | "path"
            | "serde"
            | "ts"
            | "default"
            | "repr"
            | "inline"
            | "cold"
            | "must_use"
            | "non_exhaustive"
            | "deprecated"
            | "no_mangle"
            | "export_name"
            | "link"
            | "link_name"
            | "link_section"
            | "unsafe"
            | "test"
            | "ignore"
            | "should_panic"
            | "wasm_bindgen"
            | "wasm_bindgen::prelude::wasm_bindgen"
            | "napi_derive::napi"
    )
}

fn is_generated_tool_attribute(meta: &syn::Meta) -> bool {
    let name = meta
        .path()
        .segments
        .iter()
        .map(|part| part.ident.to_string())
        .collect::<Vec<_>>()
        .join("::");
    matches!(name.as_str(), "clippy::msrv" | "rustfmt::skip")
}

fn quote_meta(meta: &syn::Meta) -> proc_macro2::TokenStream {
    match meta {
        syn::Meta::Path(path) => path
            .segments
            .iter()
            .map(|segment| proc_macro2::TokenTree::Ident(segment.ident.clone()))
            .collect(),
        syn::Meta::List(list) => list.tokens.clone(),
        syn::Meta::NameValue(_) => proc_macro2::TokenStream::new(),
    }
}
fn mentions_doc(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(name) => name.to_string().trim_start_matches("r#") == "doc",
        proc_macro2::TokenTree::Group(group) => mentions_doc(group.stream()),
        _ => false,
    })
}

fn attribute_macro_name(name: &str) -> bool {
    matches!(
        name,
        "Debug"
            | "Default"
            | "Clone"
            | "Copy"
            | "PartialEq"
            | "Eq"
            | "PartialOrd"
            | "Ord"
            | "Hash"
            | "Serialize"
            | "Deserialize"
            | "TS"
            | "wasm_bindgen"
            | "napi"
    )
}

fn trusted_namespace(name: &str) -> bool {
    matches!(
        name.trim_start_matches("r#"),
        "std"
            | "safe_cex_num"
            | "core"
            | "serde_json"
            | "syn"
            | "wasm_bindgen"
            | "serde"
            | "ts_rs"
            | "napi_derive"
            | "clippy"
            | "rustfmt"
    )
}

fn number_output_name(item: &syn::Macro) -> bool {
    item.path
        .segments
        .last()
        .is_some_and(|part| part.ident.to_string().trim_start_matches("r#") == "js_number_output")
}

// This is a deliberately bounded subset of the reviewed four-arm provider.
// Literal self and identifier-only registrations contain no opaque expressions,
// attributes, calls or declarations. All other operands remain unsupported.
fn number_output_grammar(tokens: proc_macro2::TokenStream) -> bool {
    use syn::{Token, parse::ParseStream, parse::Parser};
    fn fields(input: ParseStream<'_>, trailing: bool) -> syn::Result<Vec<String>> {
        let fields = syn::punctuated::Punctuated::<syn::Ident, Token![,]>::parse_terminated(input)?;
        if !trailing && fields.trailing_punct() {
            return Err(input.error("variant fields cannot have a trailing comma"));
        }
        let names: Vec<_> = fields
            .iter()
            .map(|name| name.to_string().trim_start_matches("r#").to_owned())
            .collect();
        if names.iter().collect::<HashSet<_>>().len() != names.len() {
            return Err(input.error("duplicate numeric registration"));
        }
        Ok(names)
    }
    (|input: ParseStream<'_>| {
        input.parse::<Token![self]>()?;
        input.parse::<Token![,]>()?;
        let content;
        if input.peek(Token![struct]) {
            input.parse::<Token![struct]>()?;
            syn::braced!(content in input);
            let mut names = fields(&content, true)?;
            if !input.is_empty() {
                let tag: syn::Ident = input.parse()?;
                if tag != "passthrough" {
                    return Err(input.error("expected passthrough registration"));
                }
                let opaque;
                syn::braced!(opaque in input);
                let extra = fields(&opaque, true)?;
                if extra.is_empty() {
                    return Err(opaque.error("passthrough registration is nonempty"));
                }
                names.extend(extra);
                if names.iter().collect::<HashSet<_>>().len() != names.len() {
                    return Err(input.error("duplicate numeric registration"));
                }
            }
        } else if input.peek(Token![enum]) {
            input.parse::<Token![enum]>()?;
            syn::braced!(content in input);
            let mut variants = HashSet::new();
            while !content.is_empty() {
                let name: syn::Ident = content.parse()?;
                if !variants.insert(name.to_string().trim_start_matches("r#").to_owned()) {
                    return Err(content.error("duplicate numeric variant"));
                }
                let payload;
                if content.peek(syn::token::Paren) {
                    syn::parenthesized!(payload in content);
                    fields(&payload, false)?;
                } else if content.peek(syn::token::Brace) {
                    syn::braced!(payload in content);
                    fields(&payload, false)?;
                }
                if !content.is_empty() {
                    content.parse::<Token![,]>()?;
                }
            }
        } else {
            let tag: syn::Ident = input.parse()?;
            if tag != "tuple" {
                return Err(input.error("expected tuple registration"));
            }
            syn::parenthesized!(content in input);
            fields(&content, true)?;
        }
        Ok(())
    })
    .parse2(tokens)
    .is_ok()
}

fn declarations_in_tokens(tokens: proc_macro2::TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(name) => matches!(
            name.to_string().as_str(),
            "struct"
                | "enum"
                | "union"
                | "impl"
                | "macro_rules"
                | "include"
                | "fn"
                | "trait"
                | "mod"
                | "type"
                | "const"
                | "static"
                | "use"
                | "extern"
        ),
        proc_macro2::TokenTree::Group(group) => declarations_in_tokens(group.stream()),
        _ => false,
    })
}

// syn does not visit macro arguments. Inspect nested invocations explicitly so
// an accepted outer expression macro cannot conceal an opaque inner expansion.
fn nested_macro_invocations(tokens: proc_macro2::TokenStream) -> Vec<syn::Macro> {
    use proc_macro2::TokenTree;
    let tokens: Vec<_> = tokens.into_iter().collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        if matches!(&tokens[index], TokenTree::Ident(_)) {
            let mut end = index + 1;
            while end + 2 < tokens.len()
                && matches!(&tokens[end], TokenTree::Punct(p) if p.as_char() == ':')
                && matches!(&tokens[end + 1], TokenTree::Punct(p) if p.as_char() == ':')
                && matches!(&tokens[end + 2], TokenTree::Ident(_))
            {
                end += 3;
            }
            if end + 1 < tokens.len()
                && matches!(&tokens[end], TokenTree::Punct(p) if p.as_char() == '!')
                && matches!(&tokens[end + 1], TokenTree::Group(_))
            {
                let invocation = tokens[index..=end + 1].iter().cloned().collect();
                if let Ok(invocation) = syn::parse2(invocation) {
                    found.push(invocation);
                    index = end + 2;
                    continue;
                }
            }
        }
        if let TokenTree::Group(group) = &tokens[index] {
            found.extend(nested_macro_invocations(group.stream()));
        }
        index += 1;
    }
    found
}

// syn::Token accepts an individual keyword or punctuation token sequence, not
// an expression block. A lone `fn`/`type` token cannot carry a declaration.
fn token_keyword_grammar(tokens: proc_macro2::TokenStream) -> bool {
    let tokens: Vec<_> = tokens.into_iter().collect();
    matches!(tokens.as_slice(), [proc_macro2::TokenTree::Ident(_)])
        || (!tokens.is_empty()
            && tokens
                .iter()
                .all(|token| matches!(token, proc_macro2::TokenTree::Punct(_))))
}

// These expression macros do not declare decoder APIs; shadowing definitions are rejected.
fn serde_forward_grammar(tokens: proc_macro2::TokenStream) -> bool {
    let mut count = 0;
    for token in tokens {
        let proc_macro2::TokenTree::Ident(name) = token else {
            return false;
        };
        if !matches!(
            name.to_string().as_str(),
            "bool"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "i128"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "u128"
                | "f32"
                | "f64"
                | "char"
                | "str"
                | "string"
                | "bytes"
                | "byte_buf"
                | "option"
                | "unit"
                | "unit_struct"
                | "newtype_struct"
                | "seq"
                | "tuple"
                | "tuple_struct"
                | "map"
                | "struct"
                | "enum"
                | "identifier"
                | "ignored_any"
        ) {
            return false;
        }
        count += 1;
    }
    count > 0
}

fn expression_macro(name: &str) -> bool {
    matches!(
        name,
        "compile_error"
            | "format"
            | "format_args"
            | "write"
            | "writeln"
            | "print"
            | "println"
            | "eprint"
            | "eprintln"
            | "vec"
            | "json"
            | "assert"
            | "assert_eq"
            | "assert_ne"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "panic"
            | "unreachable"
            | "todo"
            | "unimplemented"
            | "matches"
            | "cfg"
            | "env"
            | "option_env"
            | "concat"
            | "stringify"
            | "include_str"
            | "include_bytes"
            | "line"
            | "column"
            | "file"
            | "module_path"
            | "thread_local"
            | "pin"
            | "Token"
            | "braced"
            | "bracketed"
            | "parenthesized"
    )
}

pub fn local_types(
    path: &Path,
    macro_origins: &HashSet<String>,
) -> super::Result<Vec<(String, usize, usize)>> {
    local_types_with_recipes(path, macro_origins, &ModuleRecipes::new())
}

pub fn local_types_with_recipes(
    path: &Path,
    macro_origins: &HashSet<String>,
    module_recipes: &ModuleRecipes,
) -> super::Result<Vec<(String, usize, usize)>> {
    let source = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let syntax = syn::parse_file(&source).map_err(|error| error.to_string())?;
    let mut visitor = LocalTypes {
        verified_macro_origins: macro_origins.clone(),
        module_recipes: module_recipes.clone(),
        generated_provider_file: path
            .ends_with(Path::new("safe-cex-num/src/collation_data/provider.rs"))
            && source.starts_with(
                "// @generated by icu4x-datagen 2.3.0; expanded private provider arms.",
            ),
        ..LocalTypes::default()
    };
    visitor.visit_file(&syntax);
    if visitor.untrusted_macro_import && visitor.unqualified_macro {
        visitor
            .names
            .push(("UNSUPPORTED_MACRO_IMPORT_SCOPE".into(), 1, 0));
    }
    if visitor.number_output_used && visitor.glob_import {
        visitor
            .names
            .push(("UNSUPPORTED_NUMBER_OUTPUT_IMPORT_SCOPE".into(), 1, 0));
    }
    if visitor.serde_forward_used && visitor.glob_import {
        visitor
            .names
            .push(("UNSUPPORTED_SERDE_FORWARD_IMPORT_SCOPE".into(), 1, 0));
    }
    Ok(visitor.names)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn serde_forwarding_requires_verified_origin_and_bounded_method_tokens() {
        let call = |tokens: &str| {
            format!(
                "impl<'de> serde::Deserializer<'de> for Input {{ serde::forward_to_deserialize_any! {{ {tokens} }} }}"
            )
        };
        for (source, verified, accepted) in [
            (call("bool i8 struct enum ignored_any"), true, true),
            (call("bool"), false, false),
            (call(""), true, false),
            (call("bool mystery"), true, false),
            (call("bool; struct Hidden;"), true, false),
            (
                call("bool { fn decode(_: &str) -> bool { true } }"),
                true,
                false,
            ),
            (call("<V: Visitor<'de>> bool"), true, false),
            (
                call("bool").replace("serde::forward", "vendor::forward"),
                true,
                false,
            ),
            (
                call("bool").replace("serde::forward", "forward"),
                true,
                false,
            ),
            (format!("mod serde {{}} {}", call("bool")), true, false),
            (format!("use vendor::*; {}", call("bool")), true, false),
            (
                format!("use vendor as serde; {}", call("bool")),
                true,
                false,
            ),
        ] {
            let mut visitor = LocalTypes {
                verified_macro_origins: if verified {
                    ["serde".into()].into()
                } else {
                    HashSet::new()
                },
                ..LocalTypes::default()
            };
            visitor.visit_file(&syn::parse_file(&source).unwrap());
            assert_eq!(
                visitor.names.is_empty() && !(visitor.serde_forward_used && visitor.glob_import),
                accepted,
                "SERDE-FORWARD origin/grammar refusal: {source}: {:?}",
                visitor.names
            );
        }
    }
    #[test]
    fn number_output_unqualified_spelling_requires_a_direct_verified_import() {
        let scan = |source: &str| {
            let mut visitor = LocalTypes {
                verified_macro_origins: ["safe_cex_num::js_number_output".into()].into(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&syn::parse_file(source).unwrap());
            visitor.names
        };
        let call = "fn check(&self) { js_number_output!(self, struct {} passthrough { value }); }";
        for import in [
            "use safe_cex_num::js_number_output;",
            "use safe_cex_num::{js_number_output, numeric_ingress::JsNumberOutput};",
        ] {
            let names = scan(&format!("{import} {call}"));
            assert!(names.is_empty(), "{import}: {names:?}");
        }
        for import in [
            "",
            "use vendor::js_number_output;",
            "use safe_cex_num::js_number_output; use vendor::js_number_output;",
            "use safe_cex_num::other::js_number_output;",
            "use safe_cex_num::{js_number_output, js_number_output as other};",
            "use vendor::hidden as js_number_output; use safe_cex_num::js_number_output;",
        ] {
            let names = scan(&format!("{import} {call}"));
            assert!(
                names
                    .iter()
                    .any(|(name, _, _)| name == "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR"),
                "{import}: {names:?}"
            );
        }
    }
    #[test]
    fn number_output_accepts_only_the_verified_self_registration_subset() {
        for registration in [
            "self, struct { candles, last_accessed }",
            "self, struct { r#type, } passthrough { info, }",
            "self, tuple(a, b,)",
            "self, enum { Unit, Tuple(a, b), Named { r#type, count }, Empty(), }",
        ] {
            let ast = syn::parse_file(&format!(
                "fn check(&self) {{ safe_cex_num::js_number_output!({registration}); }}"
            ))
            .unwrap();
            let mut visitor = LocalTypes {
                verified_macro_origins: ["safe_cex_num::js_number_output".into()].into(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&ast);
            assert!(
                visitor.names.is_empty(),
                "verified numeric registration: {registration}: {:?}",
                visitor.names
            );
            let mut unverified = LocalTypes::default();
            unverified.visit_file(&ast);
            assert!(
                unverified
                    .names
                    .iter()
                    .any(|(name, _, _)| name == "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR"),
                "numeric origin must be verified"
            );
        }
    }

    #[test]
    fn number_output_refuses_opaque_operands_and_malformed_registrations_before_doc() {
        for registration in [
            "(self), struct { value }",
            "{ #[cfg(not(doc))] impl Hidden {} self }, struct { value }",
            "vec!({ #[cfg(not(doc))] impl Hidden {} self }), struct { value }",
            "self, struct { #[cfg(not(doc))] value }",
            "self, struct { module::value }",
            "self, struct { value, value }",
            "self, struct { value } passthrough {}",
            "self, enum { V(a,) }",
            "self, enum { V { a, } }",
            "self, enum { V(a) { b } }",
            "self, tuple(value) extra",
        ] {
            let ast = syn::parse_file(&format!(
                "fn check(&self) {{ safe_cex_num::r#js_number_output!({registration}); }}"
            ))
            .unwrap();
            let mut guards = DocGuards(false);
            guards.visit_file(&ast);
            assert!(guards.0, "numeric pre-doc argument refusal: {registration}");
            let mut visitor = LocalTypes {
                verified_macro_origins: ["safe_cex_num::js_number_output".into()].into(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&ast);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|(name, _, _)| name == "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR"),
                "numeric local argument refusal: {registration}"
            );
        }
    }

    #[test]
    fn number_output_cannot_inherit_permission_through_namespace_shadows() {
        for suffix in [
            "mod safe_cex_num {}",
            "mod r#safe_cex_num {}",
            "use fake as safe_cex_num;",
            "use fake::safe_cex_num::{self};",
            "use fake::{r#safe_cex_num::{self}};",
            "extern crate fake as safe_cex_num;",
            "fn nested() { mod safe_cex_num {} }",
            "mod nested { use fake as safe_cex_num; }",
        ] {
            let ast = syn::parse_file(&format!(
                "fn check(&self) {{ safe_cex_num::js_number_output!(self, struct {{ value }}); }} {suffix}"
            )).unwrap();
            let mut visitor = LocalTypes {
                verified_macro_origins: ["safe_cex_num::js_number_output".into()].into(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&ast);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|(name, _, _)| name == "UNSUPPORTED_MACRO_NAMESPACE_SHADOW"),
                "later numeric namespace shadow: {suffix}"
            );
        }
        for source in [
            "fn check(&self) { crate::js_number_output!(self, struct { value }); }",
            "fn check(&self) { js_number_output!(self, struct { value }); }",
            "fn check(&self) { other::js_number_output!(self, struct { value }); }",
        ] {
            let ast = syn::parse_file(source).unwrap();
            let mut visitor = LocalTypes {
                verified_macro_origins: ["safe_cex_num::js_number_output".into()].into(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&ast);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|(name, _, _)| name == "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR"),
                "numeric path requires exact external binding"
            );
        }
    }
    #[test]
    fn number_output_self_permission_requires_the_distinct_provider_origin() {
        let positive = "fn check(&self) { crate::js_number_output!(self, struct { value }); }";
        for (source, origins, accepted) in [
            (positive, vec!["safe_cex_num::js_number_output::self"], true),
            (positive, vec!["safe_cex_num::js_number_output"], false),
            (positive, vec![], false),
            (
                "fn check(&self) { js_number_output!(self, struct { value }); }",
                vec!["safe_cex_num::js_number_output::self"],
                false,
            ),
            (
                "fn check(&self) { other::js_number_output!(self, struct { value }); }",
                vec!["safe_cex_num::js_number_output::self"],
                false,
            ),
            (
                "fn check(&self) { safe_cex_num::js_number_output!(self, struct { value }); }",
                vec!["safe_cex_num::js_number_output::self"],
                false,
            ),
            (
                "fn check(&self) { crate::js_number_output!({ struct Hidden; self }, struct { value }); }",
                vec!["safe_cex_num::js_number_output::self"],
                false,
            ),
        ] {
            let mut visitor = LocalTypes {
                verified_macro_origins: origins.into_iter().map(str::to_owned).collect(),
                ..LocalTypes::default()
            };
            visitor.visit_file(&syn::parse_file(source).unwrap());
            let refused = visitor
                .names
                .iter()
                .any(|(name, _, _)| name == "UNSUPPORTED_NUMBER_OUTPUT_ORIGIN_OR_GRAMMAR");
            assert_eq!(
                !refused, accepted,
                "numeric provider self origin/grammar: {source}"
            );
        }
    }
    fn verified_test_visitor() -> LocalTypes {
        LocalTypes {
            verified_macro_origins: ["std", "core", "serde_json", "syn"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            ..LocalTypes::default()
        }
    }
    #[test]
    fn enclosing_procedural_attributes_cannot_hide_local_expansions() {
        for source in [
            "#[custom_decoder] fn f() {}",
            "#[cfg_attr(feature = \"extra\", custom_decoder)] mod hidden {}",
            "#[derive(CustomDecoder)] struct Hidden;",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_ATTRIBUTE_EXPANSION:")),
                "{source}: {:?}",
                visitor.names
            );
        }
    }

    #[test]
    fn serde_helpers_name_their_enclosing_deserialize_owner() {
        let source = "impl<'de> serde::Deserialize<'de> for Owner { fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> { struct Only; impl<'de> serde::de::Visitor<'de> for Only { type Value = Owner; } impl Clone for Only { fn clone(&self) -> Self { Only } } d.deserialize_map(Only) } } fn free() { struct Loose; impl<'de> serde::de::Visitor<'de> for Loose { type Value = (); } }";
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syn::parse_file(source).unwrap());
        let labels: Vec<_> = visitor.names.iter().map(|row| row.0.as_str()).collect();
        assert!(
            labels.contains(&"LOCAL_TRAIT_IMPL:serde::de::Visitor:Only@Owner"),
            "{labels:?}"
        );
        assert!(
            labels.contains(&"LOCAL_TRAIT_IMPL:Clone:Only"),
            "only serde helpers are bound: {labels:?}"
        );
        assert!(
            labels.contains(&"LOCAL_TRAIT_IMPL:serde::de::Visitor:Loose"),
            "free functions bind no owner: {labels:?}"
        );
    }

    #[test]
    fn syn_grouping_macros_need_the_verified_syn_origin() {
        let source = "fn parse(input: ParseStream) -> Result<()> { let content; syn::braced!(content in input); let inner; syn::parenthesized!(inner in content); let list; syn::bracketed!(list in inner); Ok(()) }";
        let syntax = syn::parse_file(source).unwrap();
        let mut verified = verified_test_visitor();
        verified.visit_file(&syntax);
        assert!(verified.names.is_empty(), "{:?}", verified.names);
        for origins in [vec!["std", "core"], vec!["std", "core", "serde_json"]] {
            let mut unverified = LocalTypes {
                verified_macro_origins: origins.into_iter().map(str::to_owned).collect(),
                ..LocalTypes::default()
            };
            unverified.visit_file(&syntax);
            assert!(
                unverified
                    .names
                    .iter()
                    .any(|row| row.0 == "UNSUPPORTED_LOCAL_MACRO:braced"),
                "{:?}",
                unverified.names
            );
        }
        let mut vendor = verified_test_visitor();
        vendor
            .visit_file(&syn::parse_file("fn f() { vendor::braced!(content in input); }").unwrap());
        assert!(
            vendor
                .names
                .iter()
                .any(|row| row.0 == "UNSUPPORTED_LOCAL_MACRO:braced"),
            "{:?}",
            vendor.names
        );
    }

    #[test]
    fn lint_expectations_are_inert_attributes() {
        let source = "#![cfg_attr(not(test), expect(dead_code, reason = \"pending\"))] #[expect(clippy::arc_with_non_send_sync)] fn f() {}";
        let syntax = syn::parse_file(source).unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);
        let mut guards = DocGuards(false);
        guards.visit_file(&syntax);
        assert!(
            !guards.0,
            "cfg_attr selecting a lint expectation is not doc-dependent"
        );
    }

    #[test]
    fn generated_provider_msrv_attribute_is_exact_and_namespace_protected() {
        let accepted = syn::parse_file("#[clippy::msrv = \"1.88\"] struct Generated;").unwrap();
        let mut visitor = LocalTypes {
            generated_provider_file: true,
            ..LocalTypes::default()
        };
        visitor.visit_file(&accepted);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);

        for source in [
            "#[clippy::msrv(\"1.88\")] struct Generated;",
            "#[clippy::msrv = 1.88] struct Generated;",
            "#[clippy::msrv = \"1.89\"] struct Generated;",
            "#[unknown_tool::setting] struct Generated;",
            "use forged::clippy; #[clippy::msrv = \"1.88\"] struct Generated;",
            "use forged::*; #[clippy::msrv = \"1.88\"] struct Generated;",
            "mod clippy {} #[clippy::msrv = \"1.88\"] struct Generated;",
            "extern crate clippy; #[clippy::msrv = \"1.88\"] struct Generated;",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0 == "UNSUPPORTED_ATTRIBUTE_IMPORT_SCOPE"
                        || row.0 == "UNSUPPORTED_MACRO_NAMESPACE_SHADOW"
                        || row.0.starts_with("UNSUPPORTED_ATTRIBUTE_EXPANSION:")),
                "{source}: {:?}",
                visitor.names
            );
        }

        let ordinary = syn::parse_file("#[clippy::msrv = \"1.88\"] struct Ordinary;").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&ordinary);
        assert!(
            visitor
                .names
                .iter()
                .any(|row| row.0 == "UNSUPPORTED_ATTRIBUTE_EXPANSION:clippy::msrv")
        );
    }

    #[test]
    fn generated_provider_rustfmt_skip_is_exact_and_namespace_protected() {
        let accepted =
            syn::parse_file("#[rustfmt::skip] #[path = \"collation_data/provider.rs\"] mod data;")
                .unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&accepted);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);

        for source in [
            "#[rustfmt::skip()] mod generated {}",
            "use forged::rustfmt; #[rustfmt::skip] mod generated {}",
            "use forged::*; #[rustfmt::skip] mod generated {}",
            "mod rustfmt {} #[rustfmt::skip] mod generated {}",
            "extern crate rustfmt; #[rustfmt::skip] mod generated {}",
            "#[rustfmt::skip] mod ordinary {}",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0 == "UNSUPPORTED_ATTRIBUTE_IMPORT_SCOPE"
                        || row.0 == "UNSUPPORTED_MACRO_NAMESPACE_SHADOW"
                        || row.0.starts_with("UNSUPPORTED_ATTRIBUTE_EXPANSION:")),
                "{source}: {:?}",
                visitor.names
            );
        }
    }

    #[test]
    fn macro_rules_scope_is_based_on_lexical_placement_not_name() {
        let local = syn::parse_file(
            "fn owner() { macro_rules! patch { () => { let _ = 1; }; } patch!(); }",
        )
        .unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&local);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);

        let hoisted = syn::parse_file(
            "macro_rules! patch { () => { let _ = 1; }; } fn owner() { patch!(); }",
        )
        .unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&hoisted);
        assert!(
            visitor
                .names
                .iter()
                .any(|row| row.0 == "UNSUPPORTED_MODULE_MACRO:patch"),
            "{:?}",
            visitor.names
        );
    }

    #[test]
    fn invoked_local_macro_effects_preserve_helpers_and_refuse_global_or_opaque_items() {
        for source in [
            "fn owner() { macro_rules! helper { () => { #[derive(serde::Deserialize)] struct Hidden { value: bool } }; } helper!(); }",
            "fn owner() { macro_rules! helper { () => { fn nested(value: &str) -> bool { value == \"buy\" } }; } helper!(); let _ = nested(\"buy\"); }",
            "fn owner() { #[allow(unused_macros)] macro_rules! never_called { () => { #[cfg(not(doc))] impl FromStr for Side {} }; } }",
        ] {
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syn::parse_file(source).unwrap());
            assert!(
                visitor.names.is_empty(),
                "local helper scope: {source}: {:?}",
                visitor.names
            );
        }
        for source in [
            "fn owner() { macro_rules! install { () => { #[cfg(not(doc))] impl FromStr for Side {} }; } install!(); }",
            "fn owner() { macro_rules! install { ($item:item) => { $item }; } install! { #[cfg(not(doc))] impl FromStr for Side {} } }",
            "fn owner() { macro_rules! install { () => { fn nested() { impl FromStr for Side {} } }; } install!(); }",
            "fn owner() { macro_rules! install { () => { const _: () = { #[cfg(test)] impl FromStr for Side {} }; }; } install!(); }",
            "fn owner() { macro_rules! install { () => { let _ = 1; }; } { macro_rules! install { ($item:item) => { $item }; } install! { impl FromStr for Side {} } } }",
            "fn owner() { macro_rules! install { () => { macro_rules! next { () => { 1 }; } }; } install!(); }",
            "fn owner() { macro_rules! install { () => { let _ = include!(\"hidden.rs\"); }; } install!(); }",
            "fn owner() { macro_rules! install { () => { #[vendor::decorate] struct Hidden; }; } install!(); }",
            "fn owner() { macro_rules! install { () => { #[cfg_attr(any(), allow(dead_code))] struct Hidden; }; } install!(); }",
            "use custom_proc::Hidden as Deserialize; fn owner() { macro_rules! install { () => { #[derive(Deserialize)] struct Hidden; }; } install!(); }",
        ] {
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syn::parse_file(source).unwrap());
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_")),
                "MACRO-EFFECTS invoked recipe must be refused: {source}: {:?}",
                visitor.names
            );
        }
    }

    #[test]
    fn local_macro_bindings_preserve_modules_and_refuse_conditional_shadowing() {
        for source in [
            "fn owner() { macro_rules! install { () => { impl FromStr for Side {} }; } #[cfg(any())] macro_rules! install { () => { let _ = 1; }; } install!(); }",
            "fn owner() { macro_rules! install { () => { impl FromStr for Side {} }; } #[cfg_attr(all(), cfg(any()))] macro_rules! install { () => { let _ = 1; }; } install!(); }",
            "fn owner() { macro_rules! install { () => { impl FromStr for Side {} }; } #[r#cfg(any())] macro_rules! install { () => { let _ = 1; }; } install!(); }",
            "fn owner() { #[cfg(any())] macro_rules! vec { () => { let _ = 1; }; } vec!(); }",
            "fn owner() { macro_rules! install { () => { impl FromStr for Side {} }; } mod inner { macro_rules! install { () => { let _ = 1; }; } } install!(); }",
            "fn owner() { macro_rules! install { () => { impl FromStr for Side {} }; } mod left { macro_rules! install { () => { let _ = 1; }; } } mod right { install!(); } }",
        ] {
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syn::parse_file(source).unwrap());
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_LOCAL_MACRO_EFFECTS:")),
                "MACRO-BINDING must reject wrong recipe selection: {source}: {:?}",
                visitor.names
            );
        }
        for source in [
            "fn owner() { macro_rules! helper { () => { struct Marker; }; } mod inner { helper!(); } }",
            "fn owner() { macro_rules! helper { () => { let _ = 1; }; } mod inner { #[allow(unused_macros)] macro_rules! helper { () => { impl FromStr for Side {} }; } } helper!(); }",
        ] {
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syn::parse_file(source).unwrap());
            assert!(
                visitor.names.is_empty(),
                "MACRO-BINDING private module scope: {source}: {:?}",
                visitor.names
            );
        }
    }

    #[test]
    fn renamed_custom_derives_and_attributes_fail_closed() {
        for source in [
            "use custom_proc::HiddenDecoder as Deserialize; #[derive(Deserialize)] struct Marker;",
            "use custom_proc::attribute as wasm_bindgen; #[wasm_bindgen] fn f() {}",
            "mod serde {}",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_")),
                "{source}"
            );
        }
    }
    #[test]
    fn public_alias_claims_keep_exact_inline_scope_and_source_end() {
        let source = "pub type Side = bool;\nmod nested { pub type Side = u8; }\nfn local() { type Side = u16; }\n#[cfg(test)] mod tests { pub type Side = u32; }";
        assert_eq!(
            public_aliases(source).unwrap(),
            [("Side".into(), [1, 22]), ("nested::Side".into(), [2, 33])]
        );
    }

    #[test]
    fn module_item_invocations_cannot_hide_unreviewed_declaration_sources() {
        for source in [
            "include!(\"hidden.rs\");",
            "vendor::hidden!();",
            "use vendor::hidden; hidden!();",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_MODULE_MACRO_INVOCATION:")),
                "{source}: {:?}",
                visitor.names
            );
        }
        let syntax = syn::parse_file("const DATA: &str = include_str!(\"data.txt\"); type Token = syn::Token![+]; fn f() { let _ = vec![1, 2]; } #[cfg(any())] compile_error!(\"disabled\");").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);
    }

    #[test]
    fn expression_macros_cannot_hide_declarations_in_arguments() {
        for source in [
            "fn owner() { let _ = vec![{ fn decode(value: &str) -> bool { value == \"BUY\" } decode(\"BUY\") }]; }",
            "const FLAG: bool = matches!({ fn decode(_: &str) -> bool { true } decode(\"BUY\") }, true);",
            "fn owner() { let _ = vec![{ trait Decode {} 1 }]; }",
            "fn owner() { let _ = vec![{ mod hidden {} 1 }]; }",
            "fn owner() { let _ = vec![{ type Wire = String; 1 }]; }",
            "fn owner() { let _ = vec![{ const VALUE: bool = true; VALUE }]; }",
            "fn owner() { let _ = vec![{ static VALUE: bool = true; VALUE }]; }",
            "fn owner() { let _ = syn::Token![{ fn decode() {} }]; }",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_LOCAL_MACRO:")),
                "{source}: {:?}",
                visitor.names
            );
        }
        let syntax = syn::parse_file("type FnToken = syn::Token![fn]; type TypeToken = syn::Token![type]; fn owner() { let _: syn::Token![const]; let _ = vec![1, 2]; macro_rules! local { () => { fn nested() {} } } local!(); }").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);
    }

    #[test]
    fn expression_arguments_do_not_hide_nested_macro_calls_or_imports() {
        for source in [
            "fn owner() { let _ = vec![vendor::hidden!()]; }",
            "fn owner() { let _ = format!(\"{}\", vendor::hidden!()); }",
            "fn owner() { let _ = vec![{ use vendor::json; json!(); 0 }]; }",
            "fn owner() { let _ = vec![{ extern crate vendor; 0 }]; }",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_LOCAL_MACRO:")),
                "{source}: {:?}",
                visitor.names
            );
        }
        let syntax = syn::parse_file("fn owner() { let _ = format!(concat!(\"a\", \"b\")); let _ = vec![serde_json::json!({\"ok\": true})]; }").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(visitor.names.is_empty(), "{:?}", visitor.names);
    }

    #[test]
    fn macro_capabilities_require_verified_origins_in_calls_and_imports() {
        for source in [
            "fn owner() { let _ = serde_json::json!({}); }",
            "fn owner() { let _: syn::Token![fn]; }",
            "use serde_json::json; fn owner() { let _ = json!({}); }",
            "use syn::Token; fn owner() { let _: Token![fn]; }",
            "fn owner() { let _ = std::vec![1]; }",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = LocalTypes::default();
            visitor.visit_file(&syntax);
            assert!(
                visitor
                    .names
                    .iter()
                    .any(|row| row.0.starts_with("UNSUPPORTED_")),
                "{source}: {:?}",
                visitor.names
            );
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor.names.is_empty(),
                "verified {source}: {:?}",
                visitor.names
            );
        }
        let syntax = syn::parse_file("fn owner() { let _ = syn::json!({}); }").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(
            visitor
                .names
                .iter()
                .any(|row| row.0 == "UNSUPPORTED_LOCAL_MACRO:json")
        );
    }

    #[test]
    fn verified_import_roots_cannot_rename_opaque_macros_as_expressions() {
        for source in [
            "use std::include as vec; fn owner() { let _ = vec!(\"hidden.rs\"); }",
            "use std::{include as compile_error}; compile_error!(\"hidden.rs\");",
            "use serde_json::json_internal as json; fn owner() { let _ = json!({}); }",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor.untrusted_macro_import && visitor.unqualified_macro,
                "{source}"
            );
        }
        let syntax =
            syn::parse_file("use std::vec as vec; fn owner() { let _ = vec![1]; }").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(!visitor.untrusted_macro_import);
        assert!(visitor.names.is_empty());
    }

    #[test]
    fn verified_thread_locals_expose_original_initializer_scopes() {
        for source in [
            "std::thread_local! { static CACHE: bool = true; }",
            "fn owner() { thread_local! { static CACHE: bool = const { true }; } }",
            "#[cfg(test)] thread_local! { static CACHE: bool = { fn hidden(_: &str) -> bool { true } hidden(\"BUY\") }; }",
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(visitor.names.is_empty(), "{source}: {:?}", visitor.names);
        }
        for (source, expected) in [
            (
                "std::thread_local! { pub static DECODE: fn(&str) -> bool = |value| value == \"BUY\"; }",
                "UNSUPPORTED_THREAD_LOCAL_VISIBILITY",
            ),
            (
                "std::thread_local! { pub(crate) static CACHE: bool = true; }",
                "UNSUPPORTED_THREAD_LOCAL_VISIBILITY",
            ),
            (
                "std::thread_local! { static CACHE: bool = { fn decode(_: &str) -> bool { true } decode(\"BUY\") }; }",
                "LOCAL_FUNCTION:decode",
            ),
            (
                "std::thread_local! { static CACHE: bool = vendor::hidden!(); }",
                "UNSUPPORTED_LOCAL_MACRO:hidden",
            ),
            (
                "std::thread_local! { #[cfg(not(doc))] static CACHE: bool = true; }",
                "UNSUPPORTED_THREAD_LOCAL_DOC_CONFIGURATION",
            ),
            (
                "std::thread_local! { struct Hidden; }",
                "UNSUPPORTED_THREAD_LOCAL_GRAMMAR",
            ),
        ] {
            let syntax = syn::parse_file(source).unwrap();
            let mut visitor = verified_test_visitor();
            visitor.visit_file(&syntax);
            assert!(
                visitor.names.iter().any(|row| row.0 == expected),
                "{source}: {:?}",
                visitor.names
            );
        }
        let syntax = syn::parse_file(
            "use vendor::thread_local; thread_local! { static CACHE: bool = true; }",
        )
        .unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert!(visitor.untrusted_macro_import && visitor.unqualified_macro);
        let mut visitor = LocalTypes::default();
        visitor.visit_file(&syntax);
        assert!(
            visitor
                .names
                .iter()
                .any(|row| row.0.starts_with("UNSUPPORTED_"))
        );
        let syntax =
            syn::parse_file("std::thread_local! { #[cfg(not(doc))] static CACHE: bool = true; }")
                .unwrap();
        let mut guards = DocGuards(false);
        guards.visit_file(&syntax);
        assert!(guards.0);
    }

    #[test]
    fn local_manual_types_cannot_hide_behind_missing_derive_syntax() {
        let syntax = syn::parse_file("fn decode() { struct Manual; } #[cfg(test)] mod tests { fn helper() { struct TestOnly; } }").unwrap();
        let mut visitor = verified_test_visitor();
        visitor.visit_file(&syntax);
        assert_eq!(
            visitor
                .names
                .iter()
                .map(|row| row.0.as_str())
                .collect::<Vec<_>>(),
            ["Manual"]
        );
    }

    fn scan_with_module_recipes(source: &str) -> Vec<(String, usize, usize)> {
        let directory = std::env::temp_dir().join(format!(
            "accepted-module-recipes-{}-{}",
            std::process::id(),
            source.len()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("lib.rs");
        fs::write(&path, source).unwrap();
        let recipes = module_recipes(&[path.as_path()]).unwrap();
        let origins = ["std", "core", "serde_json", "syn"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let names = local_types_with_recipes(&path, &origins, &recipes).unwrap();
        fs::remove_dir_all(&directory).unwrap();
        names
    }

    #[test]
    fn module_expression_recipes_are_accepted_only_when_no_expansion_can_declare() {
        for source in [
            "macro_rules! call { ($h:expr, $name:expr) => {{ let selected = $h.select($name)?; $h.call(selected) }}; } fn owner(h: &H) -> R { call!(h, \"a\") }",
            "#[allow(unused_macros)] macro_rules! join { ($host:expr; $($part:expr),* $(,)?) => {{ let mut parts = Vec::new(); $(parts.push($part);)* $host.join(parts) }}; } fn owner() { let _ = crate::join!(h; 1, 2); }",
            "macro_rules! outer { ($v:expr) => { inner!($v) }; } macro_rules! inner { ($v:expr) => { vec![$v] }; } fn owner() { let _ = outer!(1); }",
        ] {
            let names = scan_with_module_recipes(source);
            assert!(names.is_empty(), "{source}: {names:?}");
        }
        for (source, expected) in [
            (
                "macro_rules! make { () => { fn decode(_: &str) -> bool { true } }; } fn owner() { make!(); }",
                "UNSUPPORTED_MODULE_MACRO:make",
            ),
            (
                "macro_rules! wrap { ($i:item) => { $i }; } fn owner() { wrap!(); }",
                "UNSUPPORTED_MODULE_MACRO:wrap",
            ),
            (
                "#[cfg(not(doc))] macro_rules! call { ($v:expr) => { $v }; } fn owner() { call!(1); }",
                "UNSUPPORTED_MODULE_MACRO:call",
            ),
            (
                "macro_rules! call { ($v:expr) => { $v }; } mod inner { macro_rules! call { () => { struct Hidden; }; } } fn owner() { call!(1); }",
                "UNSUPPORTED_LOCAL_MACRO:call",
            ),
            (
                "macro_rules! call { ($($t:tt)*) => { $($t)* }; } fn owner() { call!({ struct Hidden; 1 }); }",
                "UNSUPPORTED_LOCAL_MACRO:call",
            ),
            (
                "macro_rules! call { ($v:expr) => { vendor::hidden!($v) }; } fn owner() { call!(1); }",
                "UNSUPPORTED_LOCAL_MACRO:hidden",
            ),
            (
                "use vendor::call; macro_rules! call { ($v:expr) => { $v }; } fn owner() { call!(1); }",
                "UNSUPPORTED_LOCAL_MACRO:call",
            ),
            (
                "macro_rules! format { ($v:expr) => { $v }; } fn owner() { format!(1); }",
                "UNSUPPORTED_MODULE_MACRO:format",
            ),
        ] {
            let names = scan_with_module_recipes(source);
            assert!(
                names.iter().any(|row| row.0 == expected),
                "{source}: {names:?}"
            );
        }
    }
}
