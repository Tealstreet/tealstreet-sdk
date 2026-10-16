use safe_cex_parity::{check, manifests, rust_index};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn main() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let mut rows = manifests(&workspace.join("parity/manifest/exchanges/okx")).unwrap();
    rows.push(
        toml::from_str(
            &fs::read_to_string(
                workspace.join("parity/manifest/journal/okx.position-adapter.ts.toml"),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let scope = scoped_workspace(workspace);
    let index = rust_index(&scope).unwrap();
    assert!(
        index
            .all
            .iter()
            .all(|id| id.starts_with("safe-cex-venue-okx/")),
        "OKX index contains only venue targets"
    );
    assert!(!index.public.is_empty(), "OKX public inventory is nonempty");
    let mut exemptions =
        safe_cex_parity::exemptions(&workspace.join("parity/exempt.toml")).unwrap();
    exemptions
        .exempt
        .retain(|e| e.rs.starts_with("safe-cex-venue-okx/"));
    let errors = check(&rows, &index, &exemptions);
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    println!(
        "OKX manifest gate: {} rows; {} public items; 0 errors",
        rows.iter().map(|m| m.symbol.len()).sum::<usize>(),
        index.public.len()
    );
}

fn scoped_workspace(workspace: &Path) -> PathBuf {
    let source = fs::read_to_string(workspace.join("Cargo.toml")).unwrap();
    let mut config: toml::Value = toml::from_str(&source).unwrap();
    let member = workspace.join("safe-cex-venue-okx");
    config["workspace"]["members"] = toml::Value::Array(vec![toml::Value::String(
        member.to_str().unwrap().to_owned(),
    )]);
    if let Some(dependencies) = config["workspace"]["dependencies"].as_table_mut() {
        for (_, dependency) in dependencies {
            if let Some(path) = dependency.as_table_mut().and_then(|d| d.get_mut("path")) {
                *path = toml::Value::String(
                    workspace
                        .join(path.as_str().unwrap())
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
            }
        }
    }
    let scope = workspace.join("target/okx-manifest-scope");
    fs::create_dir_all(&scope).unwrap();
    fs::write(scope.join("Cargo.toml"), toml::to_string(&config).unwrap()).unwrap();
    scope
}
