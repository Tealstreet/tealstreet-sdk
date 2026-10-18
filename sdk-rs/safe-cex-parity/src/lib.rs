//! Parity tooling; behavioral documentation remains in packages/safe-cex/CLAUDE.md.

pub mod accepted_source;
pub mod progress;
mod reexports;

// compile_error! is a diagnostic and cannot declare items; shadowed origins remain rejected.
// Additions require a demonstration that the macro cannot declare an item,
// never an argument that it probably does not.
const NON_DECLARING_ITEM_MACROS: &[&str] = &["compile_error"];

use reexports::Reexports;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub file: Source,
    #[serde(default)]
    pub symbol: Vec<Symbol>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub ts: String,
    #[serde(default)]
    pub rust_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    pub ts: String,
    pub kind: String,
    pub layer: String,
    pub rs: RustMapping,
    pub status: String,
    pub note: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RustMapping {
    Single(String),
    Split(Vec<String>),
}

impl From<&str> for RustMapping {
    fn from(value: &str) -> Self {
        Self::Single(value.into())
    }
}

impl RustMapping {
    fn items(&self) -> Vec<&str> {
        match self {
            Self::Single(value) if value.is_empty() => Vec::new(),
            Self::Single(value) => vec![value],
            Self::Split(values) => values.iter().map(String::as_str).collect(),
        }
    }

    fn is_empty(&self) -> bool {
        self.items().is_empty()
    }
}

fn expected_leaf(row: &Symbol) -> String {
    let leaf = row.ts.rsplit(['.', '#']).next().unwrap_or(&row.ts);
    let leaf = leaf.strip_prefix("static ").unwrap_or(leaf);
    let leaf = leaf
        .strip_prefix("get ")
        .or_else(|| leaf.strip_prefix("set "))
        .unwrap_or(leaf);
    if row.kind == "constructor" {
        return "new".into();
    }
    if matches!(
        row.kind.as_str(),
        "class" | "type" | "enum" | "const" | "variable"
    ) {
        return leaf.into();
    }
    let chars: Vec<_> = leaf.chars().collect();
    let mut result = String::new();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let previous = chars[i - 1];
            let next_lower = chars.get(i + 1).is_some_and(|c| c.is_lowercase());
            if previous.is_lowercase()
                || previous.is_numeric()
                || (previous.is_uppercase() && next_lower)
            {
                result.push('_');
            }
        }
        result.extend(ch.to_lowercase());
    }
    result
}

fn expected_item_path(row: &Symbol) -> String {
    // Hash is a member marker after the dot, or the separator in legacy Class#member.
    let owner = row.ts.rsplit_once('.').or_else(|| row.ts.rsplit_once('#'));
    let leaf = expected_leaf(row);
    match owner {
        Some((qualifier, _)) => format!("{}::{leaf}", qualifier.replace('.', "::")),
        None => leaf,
    }
}

fn note_names_target(note: &str, target: &str) -> bool {
    note.split_whitespace().any(|word| {
        word.trim_matches(['`', '\'', '"', ',', ';', '.', '(', ')', '[', ']']) == target
    })
}

fn split_explanation(row: &Symbol) -> bool {
    // Remove declarations even inside comma-separated/backticked lists. Residual prose
    // must exist independently; judging its substance remains a review obligation.
    let mut explanation = row.note.clone();
    let mut targets = row.rs.items();
    targets.sort_by_key(|target| std::cmp::Reverse(target.len()));
    for target in targets {
        if !target.is_empty() {
            explanation = explanation.replace(target, "");
        }
    }
    explanation.chars().any(char::is_alphanumeric)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exemptions {
    #[serde(default)]
    pub exempt: Vec<Exemption>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exemption {
    pub rs: String,
    pub reason: String,
}

fn read_toml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn files(dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            found.extend(files(&path, extension)?);
        } else if path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

pub fn manifests(dir: &Path) -> Result<Vec<Manifest>> {
    let paths = files(dir, "toml")?;
    if paths.is_empty() {
        return Err(format!("{}: no manifest files found", dir.display()));
    }
    paths.iter().map(|p| read_toml(p)).collect()
}

pub fn exemptions(path: &Path) -> Result<Exemptions> {
    read_toml(path)
}

#[derive(Debug, Default)]
pub struct RustIndex {
    pub all: BTreeSet<String>,
    pub public: BTreeSet<String>,
}

fn valid_rust_file(file: &str) -> bool {
    !file.starts_with('/')
        && !file.contains(['*', '?', '[', ']', ':', '\\'])
        && file.ends_with(".rs")
        && file.contains('/')
        && file
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
}

fn public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.parse_args::<syn::Path>()
                .is_ok_and(|p| p.is_ident("test"))
    })
}

impl RustIndex {
    fn insert(&mut self, base: &str, scope: &str, name: &str, is_public: bool) {
        let id = format!("{base}::{scope}{name}");
        self.all.insert(id.clone());
        if is_public {
            self.public.insert(id);
        }
    }

    fn source(
        &mut self,
        root: &Path,
        name: &str,
        path: &Path,
        active: &mut BTreeSet<PathBuf>,
        is_target_root: bool,
        reexports: &mut Reexports,
    ) -> Result<()> {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !active.insert(canonical.clone()) {
            return Err(format!("{}: cyclic module declaration", path.display()));
        }
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        let base = format!("{name}/{}", relative.display());
        let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let ast = syn::parse_file(&source).map_err(|e| format!("{}: {e}", path.display()))?;
        if !test_only(&ast.attrs) {
            reexports.file_configuration(&ast.attrs);
            self.items(&base, "", &ast.items, reexports)?;
            reexports.collect(&base, "", &ast.items)?;
            let parent = path.parent().ok_or("source parent missing")?;
            let module_dir = if is_target_root
                || matches!(path.file_name().and_then(|s| s.to_str()), Some("mod.rs"))
            {
                parent.to_path_buf()
            } else {
                parent.join(path.file_stem().ok_or("source stem missing")?)
            };
            self.modules(
                root,
                name,
                &ast.items,
                (&module_dir, parent),
                active,
                reexports,
            )?;
        }
        active.remove(&canonical);
        Ok(())
    }

    fn modules(
        &mut self,
        root: &Path,
        name: &str,
        items: &[syn::Item],
        directories: (&Path, &Path),
        active: &mut BTreeSet<PathBuf>,
        reexports: &mut Reexports,
    ) -> Result<()> {
        let (dir, path_dir) = directories;
        for item in items {
            if let syn::Item::Mod(module) = item {
                if test_only(&module.attrs) {
                    continue;
                }
                let explicit = module
                    .attrs
                    .iter()
                    .find(|a| a.path().is_ident("path"))
                    .map(|a| {
                        if let syn::Meta::NameValue(value) = &a.meta
                            && let syn::Expr::Lit(lit) = &value.value
                            && let syn::Lit::Str(path) = &lit.lit
                        {
                            return Ok(path_dir.join(path.value()));
                        }
                        Err(format!("module {}: invalid path attribute", module.ident))
                    })
                    .transpose()?;
                if module.attrs.iter().any(|a| a.path().is_ident("cfg_attr")) {
                    return Err(format!(
                        "module {}: cfg_attr module paths need checked-in explicit modules",
                        module.ident
                    ));
                }
                reexports.namespace.push(module.ident.to_string());
                if let Some((_, inner)) = &module.content {
                    let nested = explicit.unwrap_or_else(|| dir.join(module.ident.to_string()));
                    self.modules(root, name, inner, (&nested, &nested), active, reexports)?;
                } else {
                    let path = if let Some(path) = explicit {
                        path
                    } else {
                        let flat = dir.join(format!("{}.rs", module.ident));
                        let nested = dir.join(module.ident.to_string()).join("mod.rs");
                        match (flat.exists(), nested.exists()) {
                            (true, false) => flat,
                            (false, true) => nested,
                            _ => {
                                return Err(format!(
                                    "module {}: expected exactly one of {} and {}",
                                    module.ident,
                                    flat.display(),
                                    nested.display()
                                ));
                            }
                        }
                    };
                    self.source(root, name, &path, active, false, reexports)?;
                }
                reexports.namespace.pop();
            }
        }
        Ok(())
    }

    fn record(
        &mut self,
        base: &str,
        scope: &str,
        attrs: &[syn::Attribute],
        ident: &syn::Ident,
        visibility: &syn::Visibility,
    ) {
        if !test_only(attrs) {
            self.insert(base, scope, &ident.to_string(), public(visibility));
        }
    }

    fn items(
        &mut self,
        base: &str,
        scope: &str,
        items: &[syn::Item],
        reexports: &mut Reexports,
    ) -> Result<()> {
        // Modules and proven workspace reexports are structural; fields/variants belong to their type row.
        // Decoder and private-construction duplication impls are gated by the compiler-backed accepted-set inventory.
        // Generated/FFI declarations need exact exemptions, never a directory-wide bypass.
        for item in items {
            match item {
                syn::Item::Fn(i) => {
                    if !test_only(&i.attrs) {
                        self.insert(base, scope, &i.sig.ident.to_string(), public(&i.vis));
                    }
                }
                syn::Item::Struct(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Enum(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Union(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Type(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Const(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Static(i) => self.record(base, scope, &i.attrs, &i.ident, &i.vis),
                syn::Item::Trait(i) => {
                    self.record(base, scope, &i.attrs, &i.ident, &i.vis);
                    if !test_only(&i.attrs) {
                        let nested = format!("{scope}{}::", i.ident);
                        for member in &i.items {
                            match member {
                                syn::TraitItem::Fn(m) if !test_only(&m.attrs) => self.insert(
                                    base,
                                    &nested,
                                    &m.sig.ident.to_string(),
                                    public(&i.vis),
                                ),
                                syn::TraitItem::Const(m) if !test_only(&m.attrs) => {
                                    self.insert(base, &nested, &m.ident.to_string(), public(&i.vis))
                                }
                                syn::TraitItem::Type(m) if !test_only(&m.attrs) => {
                                    self.insert(base, &nested, &m.ident.to_string(), public(&i.vis))
                                }
                                _ => {}
                            }
                        }
                    }
                }
                syn::Item::Mod(i) if !test_only(&i.attrs) => {
                    if let Some((_, inner)) = &i.content {
                        reexports.namespace.push(i.ident.to_string());
                        self.items(base, &format!("{scope}{}::", i.ident), inner, reexports)?;
                        reexports.namespace.pop();
                    }
                }
                syn::Item::Impl(i) if !test_only(&i.attrs) && i.trait_.is_none() => {
                    let syn::Type::Path(ty) = &*i.self_ty else {
                        return Err(format!("{base}: unsupported inherent impl type"));
                    };
                    if ty.qself.is_some()
                        || ty.path.leading_colon.is_some()
                        || ty.path.segments.len() != 1
                    {
                        return Err(format!(
                            "{base}: qualified inherent impl owners are unsupported; use a local type declaration"
                        ));
                    }
                    let owner = ty.path.segments[0].ident.to_string();
                    if items.iter().any(|item| matches!(item, syn::Item::Use(import) if !test_only(&import.attrs) && imports_name(&import.tree, &owner))) {
                        reexports.inherent_owner(base, scope, &owner);
                    }
                    let nested = format!("{scope}{owner}::");
                    for member in &i.items {
                        match member {
                            syn::ImplItem::Fn(m) if !test_only(&m.attrs) => {
                                self.insert(base, &nested, &m.sig.ident.to_string(), public(&m.vis))
                            }
                            syn::ImplItem::Const(m) if !test_only(&m.attrs) => {
                                self.insert(base, &nested, &m.ident.to_string(), public(&m.vis))
                            }
                            syn::ImplItem::Type(m) if !test_only(&m.attrs) => {
                                self.insert(base, &nested, &m.ident.to_string(), public(&m.vis))
                            }
                            _ => {}
                        }
                    }
                }
                syn::Item::ForeignMod(i) if !test_only(&i.attrs) => {
                    for member in &i.items {
                        match member {
                            syn::ForeignItem::Fn(m) if !test_only(&m.attrs) => {
                                self.insert(base, scope, &m.sig.ident.to_string(), public(&m.vis))
                            }
                            syn::ForeignItem::Static(m) if !test_only(&m.attrs) => {
                                self.insert(base, scope, &m.ident.to_string(), public(&m.vis))
                            }
                            syn::ForeignItem::Type(m) if !test_only(&m.attrs) => {
                                self.insert(base, scope, &m.ident.to_string(), public(&m.vis))
                            }
                            _ => {}
                        }
                    }
                }
                syn::Item::Macro(i) if !test_only(&i.attrs) => {
                    if let Some(name) = &i.ident {
                        self.insert(
                            base,
                            scope,
                            &name.to_string(),
                            i.attrs.iter().any(|a| a.path().is_ident("macro_export")),
                        );
                    } else if let Some(name) = NON_DECLARING_ITEM_MACROS
                        .iter()
                        .find(|name| i.mac.path.is_ident(name))
                    {
                        reexports.diagnostic_macros.insert((*name).into());
                    } else {
                        return Err(format!(
                            "{base}: item macro {} cannot be indexed without expansion; provide checked-in generated declarations",
                            i.mac
                                .path
                                .segments
                                .iter()
                                .map(|s| s.ident.to_string())
                                .collect::<Vec<_>>()
                                .join("::")
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn imports_name(tree: &syn::UseTree, name: &str) -> bool {
    match tree {
        syn::UseTree::Path(p) => imports_name(&p.tree, name),
        syn::UseTree::Name(n) => n.ident == name,
        syn::UseTree::Rename(n) => n.rename == name,
        syn::UseTree::Group(g) => g.items.iter().any(|t| imports_name(t, name)),
        syn::UseTree::Glob(_) => true,
    }
}

fn target_roots(root: &Path, config: &toml::Value) -> Result<BTreeSet<PathBuf>> {
    let mut roots = BTreeSet::new();
    let package = config.get("package").ok_or("package missing")?;
    if let Some(lib) = config.get("lib") {
        roots.insert(
            root.join(
                lib.get("path")
                    .and_then(|p| p.as_str())
                    .unwrap_or("src/lib.rs"),
            ),
        );
    } else if root.join("src/lib.rs").exists() {
        roots.insert(root.join("src/lib.rs"));
    }
    if package.get("autobins").and_then(|v| v.as_bool()) != Some(false) {
        if root.join("src/main.rs").exists() {
            roots.insert(root.join("src/main.rs"));
        }
        let bins = root.join("src/bin");
        if bins.exists() {
            for entry in fs::read_dir(&bins).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().is_some_and(|e| e == "rs") {
                    roots.insert(path);
                } else if path.is_dir() && path.join("main.rs").exists() {
                    roots.insert(path.join("main.rs"));
                }
            }
        }
    }
    if let Some(bins) = config.get("bin").and_then(|v| v.as_array()) {
        for bin in bins {
            let path = bin
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("explicit bin requires path for parity indexing")?;
            roots.insert(root.join(path));
        }
    }
    if roots.is_empty() {
        return Err(format!("{}: no library or binary roots", root.display()));
    }
    Ok(roots)
}

pub fn rust_index(workspace: &Path) -> Result<RustIndex> {
    let config: toml::Value = read_toml(&workspace.join("Cargo.toml"))?;
    let members = config
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or("workspace.members must be an array")?;
    let mut index = RustIndex::default();
    let mut reexports = Reexports::default();
    let mut crates = Vec::new();
    // Scope is workspace members excluding this tool; external reexports are indexed at their alias.
    for member in members {
        let member = member.as_str().ok_or("workspace member must be a string")?;
        if member.contains('*') {
            return Err(format!(
                "unsupported workspace member glob {member}; list members explicitly"
            ));
        }
        let root = workspace.join(member);
        let crate_config: toml::Value = read_toml(&root.join("Cargo.toml"))?;
        let name = crate_config
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .ok_or("package.name missing")?;
        if name == "safe-cex-parity" {
            continue;
        }
        crates.push((root, name.to_string(), crate_config));
    }
    for (root, name, crate_config) in &crates {
        reexports.workspace_dependencies =
            workspace_dependencies(workspace, root, name, crate_config, &config, &crates)?;
        for path in target_roots(root, crate_config)? {
            reexports.namespace = vec![format!(
                "{name}/{}",
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .display()
            )];
            index.source(
                root,
                name,
                &path,
                &mut BTreeSet::new(),
                true,
                &mut reexports,
            )?;
        }
    }
    reexports.finish(&mut index)?;
    Ok(index)
}

fn workspace_dependencies(
    workspace: &Path,
    root: &Path,
    name: &str,
    config: &toml::Value,
    workspace_config: &toml::Value,
    crates: &[(PathBuf, String, toml::Value)],
) -> Result<BTreeMap<String, String>> {
    let mut roots = BTreeMap::new();
    let lib_name = config
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(|n| n.as_str())
        .map_or_else(|| name.replace('-', "_"), str::to_string);
    roots.insert(lib_name, library_namespace(name, config));
    let mut ownership: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut sections = vec![config];
    if let Some(targets) = config.get("target").and_then(|t| t.as_table()) {
        sections.extend(targets.values());
    }
    for section in sections {
        if let Some(dependencies) = section.get("dependencies").and_then(|d| d.as_table()) {
            for (alias, value) in dependencies {
                let inherited = value.get("workspace").and_then(|v| v.as_bool()) == Some(true);
                let (base, value) = if inherited {
                    (
                        workspace,
                        workspace_config
                            .get("workspace")
                            .and_then(|w| w.get("dependencies"))
                            .and_then(|d| d.get(alias))
                            .ok_or_else(|| format!("missing workspace dependency {alias}"))?,
                    )
                } else {
                    (root, value)
                };
                let mut owner = None;
                let mut custom_lib = None;
                if let Some(path) = value.get("path").and_then(|p| p.as_str()) {
                    let path = base
                        .join(path)
                        .canonicalize()
                        .map_err(|e| format!("dependency {alias}: {e}"))?;
                    for (member_root, member_name, member_config) in crates {
                        if member_root.canonicalize().map_err(|e| e.to_string())? == path {
                            owner = Some(library_namespace(member_name, member_config));
                            if value.get("package").is_none() {
                                custom_lib = member_config
                                    .get("lib")
                                    .and_then(|lib| lib.get("name"))
                                    .and_then(|n| n.as_str());
                            }
                        }
                    }
                }
                if let Some(previous) = ownership.get(alias)
                    && previous != &owner
                {
                    return Err(format!(
                        "dependency {alias}: differing workspace ownership across target configurations cannot be resolved"
                    ));
                }
                ownership.insert(alias.clone(), owner.clone());
                if let Some(owner) = owner {
                    roots.insert(alias.replace('-', "_"), owner.clone());
                    if let Some(lib) = custom_lib {
                        roots.insert(lib.to_string(), owner);
                    }
                }
            }
        }
    }
    Ok(roots)
}

fn library_namespace(name: &str, config: &toml::Value) -> String {
    let path = config
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .and_then(|p| p.as_str())
        .unwrap_or("src/lib.rs");
    format!("{name}/{path}")
}

pub fn check(manifests: &[Manifest], index: &RustIndex, exemptions: &Exemptions) -> Vec<String> {
    let mut errors = Vec::new();
    let mut mapped = BTreeSet::new();
    let mut rows = BTreeSet::new();
    let mut ported_claims: BTreeMap<&str, Vec<(String, &str)>> = BTreeMap::new();
    for manifest in manifests {
        let mut declared = BTreeSet::new();
        for file in &manifest.file.rust_files {
            if !valid_rust_file(file) || !declared.insert(file) {
                errors.push(format!("status: {}: invalid or duplicate Rust source file in [file].rust_files: {file}", manifest.file.ts));
            }
        }
        let mut used_files = BTreeSet::new();
        for row in &manifest.symbol {
            let label = format!("{}::{} ({})", manifest.file.ts, row.ts, row.kind);
            if !rows.insert((&manifest.file.ts, &row.ts, &row.kind)) {
                errors.push(format!("status: {label}: duplicate row"));
            }
            if !matches!(
                row.kind.as_str(),
                "function"
                    | "method"
                    | "field-fn"
                    | "constructor"
                    | "accessor"
                    | "class"
                    | "const-fn"
                    | "const"
                    | "variable"
                    | "variable-fn"
                    | "type"
                    | "enum"
            ) {
                errors.push(format!("status: {label}: unknown kind {:?}", row.kind));
            }
            if !matches!(row.layer.as_str(), "" | "mirror" | "adapted") {
                errors.push(format!("status: {label}: unknown layer {:?}", row.layer));
            }
            match row.status.as_str() {
                "pending" => {
                    if !row.rs.is_empty() {
                        errors.push(format!(
                            "status: {label}: pending requires empty rs: {:?}",
                            row.rs
                        ));
                    }
                }
                "ported" => {
                    if row.rs.is_empty() {
                        errors.push(format!("status: {label}: ported requires rs"));
                    }
                }
                "divergent" | "not-applicable" => {
                    // Divergence claims an existing Rust implementation, not an unimplemented row.
                    if row.status == "divergent" && row.rs.is_empty() {
                        errors.push(format!("status: {label}: divergent requires rs"));
                    }
                    if row.note.trim().is_empty() {
                        errors.push(format!("status: {label}: {} requires note", row.status));
                    }
                }
                _ => errors.push(format!("status: {label}: unknown status {:?}", row.status)),
            }
            // Layer describes supplied Rust, independently of the status label.
            if !row.rs.is_empty() && row.layer.trim().is_empty() {
                errors.push(format!("status: {label}: nonempty rs requires layer"));
            }
            if row.rs.is_empty() && !row.layer.is_empty() {
                errors.push(format!("status: {label}: empty rs requires empty layer"));
            }
            if row.status == "not-applicable" && !row.rs.is_empty() {
                errors.push(format!("status: {label}: not-applicable requires empty rs"));
            }
            // Rename-target tokens are declarations, not a split explanation.
            if matches!(row.rs, RustMapping::Split(_)) && !split_explanation(row) {
                errors.push(format!("status: {label}: array rs requires note explaining the split beyond Rust target tokens"));
            }
            let mut row_targets = BTreeSet::new();
            for rs in row.rs.items() {
                // A split lists distinct implementations; repetition is not another mapping.
                if !row_targets.insert(rs) {
                    errors.push(format!("status: {label}: duplicate rs array member: {rs}"));
                    continue;
                }
                if row.status != "pending" {
                    mapped.insert(rs.to_owned());
                }
                if row.status == "ported" {
                    ported_claims
                        .entry(rs)
                        .or_default()
                        .push((label.clone(), &row.note));
                }
                if matches!(row.status.as_str(), "ported" | "divergent") {
                    let rust_file = rs.split_once("::").map(|(file, _)| file);
                    if let Some(file) = rust_file {
                        used_files.insert(file);
                    }
                    if !rust_file.is_some_and(|file| {
                        manifest
                            .file
                            .rust_files
                            .iter()
                            .any(|declared| declared == file)
                    }) {
                        errors.push(format!("status: {label}: Rust source file is not declared for {}: {rs}; declare the exact crate/file in [file].rust_files", manifest.file.ts));
                    }
                    let expected = expected_item_path(row);
                    // A split/shared-claim explanation authorizes no rename unless it names this target.
                    if rs.split_once("::").map(|(_, path)| path) != Some(expected.as_str())
                        && !note_names_target(&row.note, rs)
                    {
                        errors.push(format!("status: {label}: Rust item path must be {expected}: {rs}; a rename note must name this exact Rust target"));
                    }
                }
                if !index.all.contains(rs) {
                    errors.push(format!("status: {label}: rs does not resolve: {rs}"));
                }
            }
        }
        // Broad unused declarations hide cross-venue swaps; expose each unused allowance.
        for file in declared {
            if !used_files.contains(file.as_str()) {
                errors.push(format!("status: {}: declared Rust source file has no ported or divergent row mapping: {file}; remove unused [file].rust_files declaration", manifest.file.ts));
            }
        }
    }
    for (rs, claims) in ported_claims {
        if claims.len() > 1 {
            for (label, note) in &claims {
                if note.trim().is_empty() {
                    errors.push(format!("status: {label}: {rs} is claimed by {} ported rows; every sharing row requires note", claims.len()));
                }
            }
        }
    }
    let mut exempt = BTreeSet::new();
    for entry in &exemptions.exempt {
        // Exemptions cover plumbing only; extra behavior needs divergent + note in the manifest.
        if entry.reason.trim().is_empty() {
            errors.push(format!(
                "invention: {}: exemption requires reason",
                entry.rs
            ));
        } else {
            exempt.insert(entry.rs.clone());
        }
        if !index.public.contains(&entry.rs) {
            errors.push(format!(
                "invention: {}: exemption does not name a public Rust item",
                entry.rs
            ));
        }
    }
    for item in &index.public {
        if !mapped.contains(item) && !exempt.contains(item) {
            errors.push(format!(
                "invention: {item}: public item has no manifest rs or exemption"
            ));
        }
    }
    errors
}

#[derive(Default)]
struct Counts {
    total: usize,
    ported: usize,
    divergent: usize,
    na: usize,
    claimed: BTreeSet<String>,
}
impl Counts {
    fn add(&mut self, row: &Symbol) {
        self.total += 1;
        self.ported += usize::from(row.status == "ported");
        self.divergent += usize::from(row.status == "divergent");
        self.na += usize::from(row.status == "not-applicable");
        if row.status != "pending" && !row.rs.is_empty() {
            self.claimed
                .extend(row.rs.items().into_iter().map(str::to_owned));
        }
    }
}

pub fn report(
    manifests: &[Manifest],
    index: &RustIndex,
    exemptions: &Exemptions,
) -> Result<String> {
    // Never publish progress from unvalidated claims; the source index anchors every mapping.
    let errors = check(manifests, index, exemptions);
    if !errors.is_empty() {
        return Err(format!(
            "parity report refused: Rust-index validation failed\n{}",
            errors.join("\n")
        ));
    }
    let mut groups: BTreeMap<String, Counts> = BTreeMap::new();
    for phase in progress::PHASES {
        groups.entry(format!("phase {phase}")).or_default();
    }
    for manifest in manifests {
        for row in &manifest.symbol {
            groups
                .entry(format!("file {}", manifest.file.ts))
                .or_default()
                .add(row);
            let mut parent = Path::new(&manifest.file.ts).parent();
            while let Some(dir) = parent.filter(|d| !d.as_os_str().is_empty()) {
                groups
                    .entry(format!("dir {}", dir.display()))
                    .or_default()
                    .add(row);
                parent = dir.parent();
            }
            let items = row.rs.items();
            let crates: BTreeSet<_> = items
                .iter()
                .map(|rs| rs.split_once('/').map_or("(unassigned)", |(name, _)| name))
                .collect();
            if crates.is_empty() {
                groups
                    .entry("crate (unassigned)".into())
                    .or_default()
                    .add(row);
            }
            for name in crates {
                let counts = groups.entry(format!("crate {name}")).or_default();
                counts.add(row);
                counts
                    .claimed
                    .retain(|rs| rs.split_once('/').is_some_and(|(owner, _)| owner == name));
            }
            groups
                .entry(format!("phase {}", progress::phase(&manifest.file.ts)))
                .or_default()
                .add(row);
            groups.entry("total".into()).or_default().add(row);
        }
        groups
            .entry(format!("file {}", manifest.file.ts))
            .or_default();
    }
    let mut out = String::from(
        "Basis: manifest claims validated against the Rust source index (status, resolution, source provenance, name correspondence, invention); behavioral parity is not verified.\n\
         scope\tverified ported rows / total rows\tdistinct validated Rust items claimed\tdivergent\tnot-applicable\n",
    );
    for (name, n) in groups {
        out.push_str(&format!(
            "{name}\t{} / {}\t{}\t{}\t{}\n",
            n.ported,
            n.total,
            n.claimed.len(),
            n.divergent,
            n.na
        ));
    }
    Ok(out)
}

pub fn run(report_only: bool) -> Result<()> {
    run_output(report_only, false)
}

pub fn run_progress() -> Result<()> {
    run_output(true, true)
}

fn run_output(report_only: bool, progress_only: bool) -> Result<()> {
    let mut workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("workspace parent missing")?
        .to_path_buf();
    let mut manifest = None;
    let mut exempt = None;
    let mut invention_crate = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" {
            println!(
                "Options: --workspace <sdk-rs> --manifest <directory> --exempt <file> [--invention-crate <package>]"
            );
            return Ok(());
        }
        let value = args.next().ok_or_else(|| format!("{arg}: missing path"))?;
        match arg.as_str() {
            "--workspace" => workspace = PathBuf::from(value),
            "--manifest" => manifest = Some(PathBuf::from(value)),
            "--exempt" => exempt = Some(PathBuf::from(value)),
            "--invention-crate" => invention_crate = Some(value),
            _ => return Err(format!("unknown option {arg}")),
        }
    }
    if report_only && invention_crate.is_some() {
        return Err("--invention-crate is supported only by parity-check".into());
    }
    if invention_crate.is_some() && (manifest.is_none() || exempt.is_none()) {
        return Err("--invention-crate requires explicit --manifest and --exempt paths".into());
    }
    let manifests = manifests(&manifest.unwrap_or_else(|| workspace.join("parity/manifest")))?;
    let mut index = rust_index(&workspace)?;
    if let Some(name) = invention_crate {
        // Resolve reexports against the complete workspace before narrowing obligations.
        // Keep index.all intact: every supplied mapping must still resolve normally.
        let prefix = format!("{name}/");
        if name.is_empty()
            || name.contains(['/', '\\'])
            || !index.all.iter().any(|id| id.starts_with(&prefix))
        {
            return Err(format!("--invention-crate: no indexed package {name}"));
        }
        index.public.retain(|id| id.starts_with(&prefix));
        println!("parity: invention scope {name}; complete workspace resolution retained");
    }
    let exemptions = exemptions(&exempt.unwrap_or_else(|| workspace.join("parity/exempt.toml")))?;
    if report_only {
        let validated = report(&manifests, &index, &exemptions)?;
        print!(
            "{}",
            if progress_only {
                progress::from_report(&validated)?
            } else {
                validated
            }
        );
    } else {
        let errors = check(&manifests, &index, &exemptions);
        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }
        println!(
            "parity: invention and status gates passed ({} rows, {} public Rust items)",
            manifests.iter().map(|m| m.symbol.len()).sum::<usize>(),
            index.public.len()
        );
    }
    Ok(())
}
