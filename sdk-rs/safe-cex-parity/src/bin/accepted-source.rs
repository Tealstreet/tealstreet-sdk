fn main() {
    let mut arguments = std::env::args().skip(1).peekable();
    if arguments
        .peek()
        .is_some_and(|arg| arg == "--binary-entry-json")
    {
        use syn::spanned::Spanned;
        arguments.next();
        let root = std::path::PathBuf::from(arguments.next().expect("binary root required"));
        let begin: [usize; 2] =
            serde_json::from_str(&arguments.next().expect("compiler begin required"))
                .expect("compiler begin JSON");
        let end: [usize; 2] =
            serde_json::from_str(&arguments.next().expect("compiler end required"))
                .expect("compiler end JSON");
        let source = std::fs::read_to_string(&root).expect("binary source required");
        let syntax = syn::parse_file(&source).expect("binary source parses");
        assert!(
            syntax
                .attrs
                .iter()
                .all(|attr| attr.path().is_ident("doc") || attr.path().is_ident("cfg")),
            "accepted binary: custom crate entry attributes unsupported"
        );
        let mains: Vec<_> = syntax
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Fn(item) if item.sig.ident == "main" => Some(item),
                _ => None,
            })
            .collect();
        let matches: Vec<_> = mains
            .iter()
            .filter(|item| {
                let start = item.span().start();
                let function = item.sig.fn_token.span.start();
                let finish = item.span().end();
                (begin == [start.line, start.column + 1]
                    || begin == [function.line, function.column + 1])
                    && end == [finish.line, finish.column + 1]
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "accepted binary: compiler must identify one lexical root main"
        );
        let item = matches[0];
        let sig = &item.sig;
        assert!(
            sig.asyncness.is_none()
                && sig.constness.is_none()
                && matches!(&sig.safety, syn::Safety::Default)
                && sig.abi.is_none()
                && sig.generics.params.is_empty()
                && sig.generics.where_clause.is_none()
                && sig.inputs.is_empty()
                && sig.variadic.is_none()
                && sig.ident == "main",
            "accepted binary: ordinary zero-argument main required"
        );
        assert!(
            item.attrs
                .iter()
                .all(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("doc")),
            "accepted binary: custom entry attributes unsupported"
        );
        let declarations: std::collections::BTreeSet<_> = mains
            .iter()
            .map(|item| {
                let at = item.sig.ident.span().start();
                (at.line, at.column)
            })
            .collect();
        for file in std::iter::once(root.clone()).chain(arguments.map(std::path::PathBuf::from)) {
            let bytes = std::fs::read_to_string(&file).expect("binary production source required");
            let tokens: proc_macro2::TokenStream = bytes.parse().expect("binary source tokens");
            let mut pending: Vec<_> = tokens.into_iter().collect();
            while let Some(token) = pending.pop() {
                match token {
                    proc_macro2::TokenTree::Group(group) => pending.extend(group.stream()),
                    proc_macro2::TokenTree::Ident(ident) => {
                        let spelling = ident.to_string();
                        let name = spelling.strip_prefix("r#").unwrap_or(&spelling);
                        assert!(
                            name != "__accepted_set_original_main" && name != "__accepted_set_run",
                            "accepted binary: generated entry name collision"
                        );
                        let at = ident.span().start();
                        assert!(
                            name != "main"
                                || (file == root && declarations.contains(&(at.line, at.column))),
                            "accepted binary: production main references cannot be redirected to harness"
                        );
                    }
                    _ => {}
                }
            }
        }
        let at = sig.ident.span().start();
        serde_json::to_writer(
            std::io::stdout(),
            &serde_json::json!({"ident":[at.line,at.column+1]}),
        )
        .expect("binary entry JSON");
        return;
    }
    if arguments
        .peek()
        .is_some_and(|arg| arg == "--number-output-provider-json")
    {
        arguments.next();
        let root = std::path::PathBuf::from(arguments.next().expect("SDK root required"));
        assert!(arguments.next().is_none(), "unexpected provider argument");
        match number_output_provider(&root) {
            Ok(provider) => {
                serde_json::to_writer(std::io::stdout(), &provider).expect("provider JSON")
            }
            Err(error) => {
                eprintln!("accepted-set: NUMBER_OUTPUT_PROVIDER source assertion: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if arguments
        .peek()
        .is_some_and(|arg| arg == "--alias-claims-json")
    {
        arguments.next();
        let workspace = std::path::PathBuf::from(arguments.next().expect("workspace required"));
        let mut declarations = std::collections::BTreeMap::new();
        for source in arguments {
            let path = workspace.join(&source);
            let syntax = std::fs::read_to_string(path).expect("compiler alias source required");
            declarations.insert(
                source,
                safe_cex_parity::accepted_source::public_aliases(&syntax)
                    .expect("public alias source parses"),
            );
        }
        let manifests = safe_cex_parity::manifests(&workspace.join("parity/manifest"))
            .expect("existing parity manifests required");
        let mut mapped = std::collections::BTreeSet::new();
        for row in manifests.into_iter().flat_map(|manifest| manifest.symbol) {
            if !matches!(row.status.as_str(), "ported" | "divergent") {
                continue;
            }
            match row.rs {
                safe_cex_parity::RustMapping::Single(target) if !target.is_empty() => {
                    mapped.insert(target);
                }
                safe_cex_parity::RustMapping::Split(targets) => mapped.extend(targets),
                _ => {}
            }
        }
        let exemptions = safe_cex_parity::exemptions(&workspace.join("parity/exempt.toml"))
            .expect("existing parity exemptions required");
        let exempt: std::collections::BTreeMap<_, _> = exemptions
            .exempt
            .into_iter()
            .filter(|entry| !entry.reason.trim().is_empty())
            .map(|entry| (entry.rs, entry.reason))
            .collect();
        serde_json::to_writer(
            std::io::stdout(),
            &serde_json::json!({"declarations":declarations,"mapped":mapped,"exempt":exempt}),
        )
        .expect("alias claims JSON");
        return;
    }
    if arguments.peek().is_some_and(|arg| arg == "--nominal-json") {
        let sources: Vec<String> = serde_json::from_reader(std::io::stdin()).expect("source JSON");
        let names: Vec<_> = sources
            .iter()
            .map(|source| {
                safe_cex_parity::accepted_source::nominal_names(source)
                    .expect("historical source parses")
            })
            .collect();
        serde_json::to_writer(std::io::stdout(), &names).expect("name JSON");
        return;
    }
    if arguments.peek().is_some_and(|arg| arg == "--doc-guards") {
        arguments.next();
        for argument in arguments {
            if let Err(error) =
                safe_cex_parity::accepted_source::doc_guards(std::path::Path::new(&argument))
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let macro_origins: std::collections::HashSet<String> = if arguments
        .peek()
        .is_some_and(|arg| arg == "--macro-origins-json")
    {
        arguments.next();
        serde_json::from_str(&arguments.next().expect("verified origin JSON required"))
            .expect("verified origin list required")
    } else {
        std::collections::HashSet::new()
    };
    let arguments: Vec<String> = arguments.collect();
    let paths: Vec<&std::path::Path> = arguments.iter().map(std::path::Path::new).collect();
    let recipes = safe_cex_parity::accepted_source::module_recipes(&paths).expect("sources parse");
    let unused_core = if macro_origins.contains("core") {
        unreferenced_core_modules(&paths, macro_origins.contains("std"))
            .expect("source namespace proof parses")
    } else {
        std::collections::HashSet::new()
    };
    for argument in &arguments {
        let path = std::path::Path::new(argument);
        for (name, line, column) in safe_cex_parity::accepted_source::local_types_with_recipes(
            path,
            &macro_origins,
            &recipes,
        )
        .expect("source parses")
        {
            if name == "UNSUPPORTED_MACRO_NAMESPACE_SHADOW"
                && unused_core.contains(&(path.to_path_buf(), line, column))
            {
                continue;
            }
            println!("{argument}\t{name}\t{line}\t{column}");
        }
    }
}

struct CoreValueReferences {
    identifiers: std::collections::HashSet<(usize, usize)>,
    verified_std: bool,
}
impl CoreValueReferences {
    fn record(&mut self, ident: &syn::Ident) {
        if ident.to_string().trim_start_matches("r#") == "core" {
            let at = ident.span().start();
            self.identifiers.insert((at.line, at.column));
        }
    }
}
impl<'ast> syn::visit::Visit<'ast> for CoreValueReferences {
    fn visit_pat_ident(&mut self, binding: &'ast syn::PatIdent) {
        self.record(&binding.ident);
        syn::visit::visit_pat_ident(self, binding);
    }
    fn visit_expr_path(&mut self, value: &'ast syn::ExprPath) {
        if value.qself.is_none()
            && value.path.leading_colon.is_none()
            && value.path.segments.len() == 1
        {
            self.record(&value.path.segments[0].ident);
        }
        syn::visit::visit_expr_path(self, value);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // Only the authenticated std vec grammar is transparent here.
        // Generic macro arguments, transcribers and cfg tokens stay opaque.
        let vector = mac.path.is_ident("vec")
            || mac.path.segments.len() == 2
                && mac.path.segments[0].ident == "std"
                && mac.path.segments[1].ident == "vec";
        if self.verified_std && vector {
            let bracket =
                proc_macro2::Group::new(proc_macro2::Delimiter::Bracket, mac.tokens.clone());
            if let Ok(expr) =
                syn::parse2::<syn::Expr>(proc_macro2::TokenTree::Group(bracket).into())
            {
                self.visit_expr(&expr);
            }
        }
        syn::visit::visit_macro(self, mac);
    }
}

fn core_module_declarations(
    items: &[syn::Item],
    identifiers: &mut std::collections::HashSet<(usize, usize)>,
    ends: &mut Vec<(usize, usize)>,
) {
    // Recurse only through modules. A function/impl/const-local module is
    // not eligible; its identifier remains an ordinary token reference.
    for item in items {
        if let syn::Item::Mod(module) = item {
            if module.ident.to_string().trim_start_matches("r#") == "core" {
                let ident = module.ident.span().start();
                identifiers.insert((ident.line, ident.column));
                let end = syn::spanned::Spanned::span(module).end();
                ends.push((end.line, end.column));
            }
            if let Some((_, items)) = &module.content {
                core_module_declarations(items, identifiers, ends);
            }
        }
    }
}
fn references_core_namespace(
    tokens: proc_macro2::TokenStream,
    declarations: &std::collections::HashSet<(usize, usize)>,
) -> bool {
    tokens.into_iter().any(|token| match token {
        proc_macro2::TokenTree::Ident(ident)
            if ident.to_string().trim_start_matches("r#") == "core" =>
        {
            let start = ident.span().start();
            !declarations.contains(&(start.line, start.column))
        }
        proc_macro2::TokenTree::Group(group) => {
            references_core_namespace(group.stream(), declarations)
        }
        _ => false,
    })
}

// A module called core cannot redirect a trusted core macro when that spelling
// is never used as a namespace anywhere in the selected source graph. Ordinary
// value bindings/references do not resolve in the macro namespace. Inspect raw tokens
// too: cfg-inactive branches and macro transcribers must revoke the proof. This
// deliberately refuses even absolute ::core paths and harmless type references.
// The normal source scanner remains authoritative for every other finding.
fn unreferenced_core_modules(
    paths: &[&std::path::Path],
    verified_std: bool,
) -> Result<std::collections::HashSet<(std::path::PathBuf, usize, usize)>, String> {
    use std::collections::HashSet;
    use syn::visit::Visit;

    let mut output = HashSet::new();
    for path in paths {
        let source = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
        let syntax = syn::parse_file(&source).map_err(|error| error.to_string())?;
        let mut identifiers = HashSet::new();
        let mut ends = Vec::new();
        core_module_declarations(&syntax.items, &mut identifiers, &mut ends);
        let mut values = CoreValueReferences {
            identifiers,
            verified_std,
        };
        values.visit_file(&syntax);
        let tokens = source
            .parse::<proc_macro2::TokenStream>()
            .map_err(|error| error.to_string())?;
        if references_core_namespace(tokens, &values.identifiers) {
            return Ok(HashSet::new());
        }
        output.extend(
            ends.into_iter()
                .map(|(line, column)| (path.to_path_buf(), line, column)),
        );
    }
    Ok(output)
}

// The permission producer inspects the real, unconditional source-module chain,
// independently of rustdoc's selected sources. No invocation spelling grants it.
fn number_output_provider(root: &std::path::Path) -> Result<serde_json::Value, String> {
    use std::{fs, path::Path};
    use syn::spanned::Spanned;
    fn source(root: &Path, relative: &str) -> Result<syn::File, String> {
        let path = root
            .join(relative)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if path != root.join(relative) {
            return Err(format!("provider source must be canonical: {relative}"));
        }
        let ast = syn::parse_file(&fs::read_to_string(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        if ast.attrs.iter().any(|attr| !attr.path().is_ident("doc")) {
            return Err(format!("conditional or opaque provider file: {relative}"));
        }
        Ok(ast)
    }
    fn module(ast: &syn::File, name: &str) -> Result<(), String> {
        let modules: Vec<_> = ast
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Mod(module) if module.ident == name => Some(module),
                _ => None,
            })
            .collect();
        if modules.len() != 1
            || modules[0].content.is_some()
            || modules[0]
                .attrs
                .iter()
                .any(|attr| !attr.path().is_ident("doc"))
        {
            return Err(format!(
                "expected one unconditional external provider module: {name}"
            ));
        }
        Ok(())
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    module(
        &source(&root, "safe-cex-num/src/lib.rs")?,
        "numeric_ingress",
    )?;
    module(
        &source(&root, "safe-cex-num/src/numeric_ingress.rs")?,
        "eligibility",
    )?;
    let ast = source(&root, "safe-cex-num/src/numeric_ingress/eligibility.rs")?;
    let macros: Vec<_> = ast
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Macro(item)
                if item
                    .ident
                    .as_ref()
                    .is_some_and(|name| name == "js_number_output") =>
            {
                Some(item)
            }
            _ => None,
        })
        .collect();
    if macros.len() != 1
        || !macros[0].mac.path.is_ident("macro_rules")
        || macros[0]
            .attrs
            .iter()
            .filter(
                |attr| matches!(&attr.meta, syn::Meta::Path(path) if path.is_ident("macro_export")),
            )
            .count()
            != 1
        || macros[0].attrs.iter().any(|attr| {
            !attr.path().is_ident("doc")
                && !matches!(&attr.meta, syn::Meta::Path(path) if path.is_ident("macro_export"))
        })
    {
        return Err("expected one unconditional macro_export definition".into());
    }
    // Attribute coverage belongs only to this independently validated top-level
    // definition. The ordinary scope scanner still rejects every macro_export.
    let export = macros[0]
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("macro_export"))
        .expect("the exact export attribute was validated above")
        .span()
        .end();
    Ok(serde_json::json!({
        "recipeFormat": "proc-macro2-token-string-v1",
        "tokens": macros[0].mac.tokens.to_string(),
        "definition": "safe-cex-num/src/numeric_ingress/eligibility.rs",
        "exportAttribute": { "ident": "UNSUPPORTED_ATTRIBUTE_EXPANSION:macro_export", "line": export.line, "column": export.column },
        "moduleChain": ["safe-cex-num/src/lib.rs", "safe-cex-num/src/numeric_ingress.rs", "safe-cex-num/src/numeric_ingress/eligibility.rs"]
    }))
}
