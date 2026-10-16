use safe_cex_parity::{Exemptions, check, manifests, rust_index};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let rows = manifests(&workspace.join("parity/manifest/exchanges/phemex")).unwrap();
    let scope = scoped_workspace(workspace);
    let index = rust_index(&scope).unwrap();
    assert!(
        index
            .all
            .iter()
            .all(|id| id.starts_with("safe-cex-venue-phemex/")),
        "Phemex index contains only venue targets"
    );
    assert!(
        !index.public.is_empty(),
        "Phemex public inventory is nonempty"
    );
    let errors = check(&rows, &index, &Exemptions::default());
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    println!(
        "Phemex manifest gate: {} rows; {} public items; 0 errors",
        rows.iter().map(|m| m.symbol.len()).sum::<usize>(),
        index.public.len()
    );
}

fn scoped_workspace(workspace: &Path) -> PathBuf {
    let source = fs::read_to_string(workspace.join("Cargo.toml")).unwrap();
    let mut config: toml::Value = toml::from_str(&source).unwrap();
    let member = workspace.join("safe-cex-venue-phemex");
    config["workspace"]["members"] = toml::Value::Array(vec![toml::Value::String(
        member.to_str().unwrap().to_owned(),
    )]);
    let scope = workspace.join("target/phemex-manifest-scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Cargo.toml"), toml::to_string(&config).unwrap()).unwrap();
    scope
}
