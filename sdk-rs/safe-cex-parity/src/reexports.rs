use crate::{Result, RustIndex, public, test_only};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct Reexports {
    pub namespace: Vec<String>,
    pub diagnostic_macros: BTreeSet<String>,
    macro_use: bool,
    pub workspace_dependencies: BTreeMap<String, String>,
    declarations: BTreeSet<Vec<String>>,
    modules: BTreeSet<Vec<String>>,
    bindings: BTreeSet<Vec<String>>,
    imports: BTreeSet<Vec<String>>,
    uses: Vec<PublicUse>,
    nominal_declarations: BTreeMap<Vec<String>, bool>,
    conditional_scopes: BTreeSet<Vec<String>>,
    inherent_owners: Vec<InherentOwner>,
}

struct InherentOwner {
    id: String,
    name: String,
    namespace: Vec<String>,
}

struct PublicUse {
    id: String,
    local_target: Option<Vec<String>>,
    local_root: Option<Vec<String>>,
    dependency_target: Option<Vec<String>>,
    glob: bool,
    is_public: bool,
    namespace: Vec<String>,
    unconditional: bool,
}

impl Reexports {
    pub fn file_configuration(&mut self, attrs: &[syn::Attribute]) {
        if !unconditional(attrs) {
            self.conditional_scopes.insert(self.namespace.clone());
        }
    }

    pub fn inherent_owner(&mut self, base: &str, scope: &str, name: &str) {
        self.inherent_owners.push(InherentOwner {
            id: format!("{base}::{scope}{name}"),
            name: name.into(),
            namespace: self.namespace.clone(),
        });
    }

    pub fn collect(&mut self, base: &str, scope: &str, items: &[syn::Item]) -> Result<()> {
        self.macro_use |= items.iter().any(|item| {
            let attrs = match item {
                syn::Item::Mod(i) => &i.attrs,
                syn::Item::ExternCrate(i) => &i.attrs,
                _ => return false,
            };
            !test_only(attrs) && attrs.iter().any(|a| imports_macros(&a.meta))
        });
        self.modules.insert(self.namespace.clone());
        self.declarations.insert(self.namespace.clone());
        for item in items {
            if let syn::Item::ExternCrate(import) = item {
                if test_only(&import.attrs) {
                    continue;
                }
                let name = import.rename.as_ref().map_or_else(
                    || import.ident.to_string(),
                    |(_, rename)| rename.to_string(),
                );
                let mut binding = self.namespace.clone();
                binding.push(name.clone());
                self.bindings.insert(binding.clone());
                self.imports.insert(binding);
                if public(&import.vis) {
                    let target = if import.ident == "self" {
                        Some(self.namespace[..1].to_vec())
                    } else {
                        self.workspace_dependencies
                            .get(&import.ident.to_string())
                            .map(|owner| vec![owner.clone()])
                    };
                    self.uses.push(PublicUse {
                        id: format!("{base}::{scope}{name}"),
                        local_target: None,
                        local_root: None,
                        dependency_target: target,
                        glob: false,
                        is_public: true,
                        namespace: self.namespace.clone(),
                        unconditional: unconditional(&import.attrs),
                    });
                }
                continue;
            }
            if let syn::Item::ForeignMod(foreign) = item {
                if test_only(&foreign.attrs) {
                    continue;
                }
                for item in &foreign.items {
                    let (name, attrs) = match item {
                        syn::ForeignItem::Fn(i) => (&i.sig.ident, &i.attrs),
                        syn::ForeignItem::Static(i) => (&i.ident, &i.attrs),
                        syn::ForeignItem::Type(i) => (&i.ident, &i.attrs),
                        _ => continue,
                    };
                    if !test_only(attrs) {
                        let mut path = self.namespace.clone();
                        path.push(name.to_string());
                        self.bindings.insert(path.clone());
                        self.declarations.insert(path);
                    }
                }
                continue;
            }
            let (name, attrs) = match item {
                syn::Item::Fn(i) => (Some(&i.sig.ident), &i.attrs),
                syn::Item::Struct(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Enum(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Union(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Type(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Const(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Static(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Trait(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Macro(i) => (i.ident.as_ref(), &i.attrs),
                syn::Item::Mod(i) => (Some(&i.ident), &i.attrs),
                syn::Item::Use(i) => (None, &i.attrs),
                _ => continue,
            };
            if test_only(attrs) {
                continue;
            }
            if let Some(name) = name {
                let mut path = self.namespace.clone();
                path.push(name.to_string());
                self.bindings.insert(path.clone());
                self.declarations.insert(path.clone());
                if matches!(
                    item,
                    syn::Item::Struct(_) | syn::Item::Enum(_) | syn::Item::Union(_)
                ) {
                    self.nominal_declarations
                        .entry(path.clone())
                        .and_modify(|proof| *proof = false)
                        .or_insert_with(|| unconditional(attrs));
                }
                if let syn::Item::Enum(i) = item {
                    for variant in &i.variants {
                        let mut member = path.clone();
                        member.push(variant.ident.to_string());
                        self.declarations.insert(member);
                    }
                }
                if let syn::Item::Mod(module) = item {
                    if !unconditional(&module.attrs) {
                        self.conditional_scopes.insert(path.clone());
                    }
                    self.modules.insert(path);
                    if let Some((_, inner)) = &module.content {
                        self.namespace.push(name.to_string());
                        self.collect(base, &format!("{scope}{name}::"), inner)?;
                        self.namespace.pop();
                    }
                }
            }
            if let syn::Item::Use(import) = item {
                self.use_tree(base, scope, import, &import.tree, &[])?;
            }
        }
        Ok(())
    }

    fn use_tree(
        &mut self,
        base: &str,
        scope: &str,
        import: &syn::ItemUse,
        tree: &syn::UseTree,
        prefix: &[String],
    ) -> Result<()> {
        let (name, path, glob) = match tree {
            syn::UseTree::Path(part) => {
                let mut prefix = prefix.to_vec();
                prefix.push(part.ident.to_string());
                return self.use_tree(base, scope, import, &part.tree, &prefix);
            }
            syn::UseTree::Group(group) => {
                for child in &group.items {
                    self.use_tree(base, scope, import, child, prefix)?;
                }
                return Ok(());
            }
            syn::UseTree::Glob(_) => ("*".into(), prefix.to_vec(), true),
            syn::UseTree::Name(part) => {
                let mut path = prefix.to_vec();
                let name = if part.ident == "self" {
                    prefix
                        .last()
                        .cloned()
                        .ok_or_else(|| format!("{base}: unsupported bare self import"))?
                } else {
                    path.push(part.ident.to_string());
                    part.ident.to_string()
                };
                (name, path, false)
            }
            syn::UseTree::Rename(part) => {
                let mut path = prefix.to_vec();
                if part.ident != "self" {
                    path.push(part.ident.to_string());
                }
                (part.rename.to_string(), path, false)
            }
        };
        if !glob {
            let mut binding = self.namespace.clone();
            binding.push(name.clone());
            self.bindings.insert(binding.clone());
            self.imports.insert(binding);
        }
        let is_public = public(&import.vis);
        if !is_public && !glob && name == "_" {
            return Ok(());
        }
        if name == "_" || path.is_empty() {
            return Err(format!(
                "{base}: anonymous public re-export cannot be indexed"
            ));
        }
        let absolute = import.leading_colon.is_some();
        let first = &path[0];
        let (local_target, local_root, dependency_target) = if !absolute && first == "crate" {
            let mut target = vec![self.namespace[0].clone()];
            target.extend_from_slice(&path[1..]);
            (Some(target), None, None)
        } else if !absolute && (first == "self" || first == "super") {
            let mut target = self.namespace.clone();
            let mut offset = 1;
            if first == "super" {
                offset = 0;
                while path.get(offset).is_some_and(|p| p == "super") {
                    if target.len() == 1 {
                        return Err(format!("{base}: re-export traverses above crate root"));
                    }
                    target.pop();
                    offset += 1;
                }
            }
            target.extend_from_slice(&path[offset..]);
            (Some(target), None, None)
        } else {
            let dependency = self.workspace_dependencies.get(first).map(|name| {
                let mut target = vec![name.clone()];
                target.extend_from_slice(&path[1..]);
                target
            });
            if absolute {
                (None, None, dependency)
            } else {
                let mut target = self.namespace.clone();
                target.extend_from_slice(&path);
                let mut root = self.namespace.clone();
                root.push(first.clone());
                (Some(target), Some(root), dependency)
            }
        };
        self.uses.push(PublicUse {
            id: format!("{base}::{scope}{name}"),
            local_target,
            local_root,
            dependency_target,
            glob,
            is_public,
            namespace: self.namespace.clone(),
            unconditional: unconditional(&import.attrs),
        });
        Ok(())
    }

    fn proven(&self, import: &PublicUse, ambiguous_globs: &BTreeSet<Vec<String>>) -> bool {
        let local_shadows_dependency = import.local_root.as_ref().is_some_and(|root| {
            self.bindings.contains(root) || ambiguous_globs.contains(&root[..root.len() - 1])
        });
        let target_set = if import.glob {
            &self.modules
        } else {
            &self.declarations
        };
        let direct = |target: &Vec<String>| {
            target_set.contains(target)
                && !(1..=target.len()).any(|len| self.imports.contains(&target[..len]))
                && !(1..target.len()).any(|len| ambiguous_globs.contains(&target[..len]))
        };
        import.local_target.as_ref().is_some_and(direct)
            || (!local_shadows_dependency && import.dependency_target.as_ref().is_some_and(direct))
    }

    fn glob_imports_uncertain_exports(
        &self,
        import: &PublicUse,
        ambiguous_globs: &BTreeSet<Vec<String>>,
    ) -> bool {
        [
            import.local_target.as_ref(),
            import.dependency_target.as_ref(),
        ]
        .into_iter()
        .flatten()
        .filter(|target| self.modules.contains(*target))
        .any(|target| {
            self.uses.iter().any(|export| {
                export.is_public
                    && export.namespace.starts_with(target)
                    && !self.proven(export, ambiguous_globs)
            })
        })
    }

    pub fn finish(self, index: &mut RustIndex) -> Result<()> {
        let mut ambiguous_globs = BTreeSet::new();
        loop {
            let scopes: Vec<_> = self
                .uses
                .iter()
                .filter(|import| {
                    import.glob
                        && (!self.proven(import, &ambiguous_globs)
                            || self.glob_imports_uncertain_exports(import, &ambiguous_globs))
                })
                .map(|import| import.namespace.clone())
                .collect();
            let before = ambiguous_globs.len();
            ambiguous_globs.extend(scopes);
            if before == ambiguous_globs.len() {
                break;
            }
        }
        for owner in &self.inherent_owners {
            let mut local = owner.namespace.clone();
            local.push(owner.name.clone());
            let unconditionally_nominal = |path: &Vec<String>| {
                self.nominal_declarations.get(path) == Some(&true)
                    && !self
                        .conditional_scopes
                        .iter()
                        .any(|scope| path.starts_with(scope))
            };
            if unconditionally_nominal(&local) && !self.imports.contains(&local) {
                continue;
            }
            let candidates: Vec<_> = self
                .uses
                .iter()
                .filter(|import| !import.glob && import.id == owner.id)
                .collect();
            if let [import] = candidates.as_slice()
                && import.unconditional
                && !self
                    .conditional_scopes
                    .iter()
                    .any(|scope| owner.namespace.starts_with(scope))
                && self.proven(import, &ambiguous_globs)
                && import.local_target.as_ref().is_some_and(|target| {
                    target.first() == owner.namespace.first() && unconditionally_nominal(target)
                })
            {
                continue;
            }
            return Err(format!(
                "{}: imported inherent impl owner cannot be proven as one unconditional same-crate nominal declaration",
                owner.id
            ));
        }
        for name in &self.diagnostic_macros {
            if self.macro_use
                || !ambiguous_globs.is_empty()
                || self.bindings.iter().any(|path| path.last() == Some(name))
            {
                return Err(format!(
                    "item macro {name}: shadowed or uncertain diagnostic origin cannot be indexed without expansion"
                ));
            }
        }
        for import in self.uses.iter().filter(|import| import.is_public) {
            // Only proven workspace declarations are structural; aliases with uncertain origins stay gated.
            if self.proven(import, &ambiguous_globs) {
                continue;
            }
            if import.glob {
                return Err(format!(
                    "{}: external or unresolved public glob re-export cannot be indexed; name its exported items explicitly",
                    import.id
                ));
            }
            index.all.insert(import.id.clone());
            index.public.insert(import.id.clone());
        }
        Ok(())
    }
}

fn unconditional(attrs: &[syn::Attribute]) -> bool {
    !attrs
        .iter()
        .any(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
}

fn imports_macros(meta: &syn::Meta) -> bool {
    if meta.path().is_ident("macro_use") {
        return true;
    }
    if let syn::Meta::List(list) = meta
        && list.path.is_ident("cfg_attr")
    {
        use syn::parse::Parser;
        return syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
            .parse2(list.tokens.clone())
            .map_or(true, |args| args.iter().skip(1).any(imports_macros));
    }
    false
}
