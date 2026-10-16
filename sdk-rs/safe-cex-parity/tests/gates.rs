use safe_cex_parity::{
    Exemption, Exemptions, Manifest, RustIndex, Source, Symbol, check, exemptions, manifests,
    rust_index,
};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/clean")
}
fn pending_manifest() -> PathBuf {
    fixture().parent().unwrap().join("pending/manifest")
}
fn baseline() -> Vec<Manifest> {
    manifests(&pending_manifest()).unwrap()
}
fn row(status: &str, rs: &str, layer: &str, note: &str) -> Symbol {
    Symbol {
        ts: "OrderSide".into(),
        kind: "enum".into(),
        status: status.into(),
        rs: rs.into(),
        layer: layer.into(),
        note: note.into(),
    }
}
fn one(row: Symbol) -> Vec<Manifest> {
    vec![Manifest {
        file: Source {
            ts: "packages/safe-cex/src/types.ts".into(),
            rust_files: if matches!(row.status.as_str(), "ported" | "divergent")
                && !matches!(&row.rs, safe_cex_parity::RustMapping::Single(s) if s.is_empty())
            {
                vec!["sample/src/lib.rs".into()]
            } else {
                vec![]
            },
        },
        symbol: vec![row],
    }]
}

fn index() -> RustIndex {
    RustIndex {
        all: ["sample/src/lib.rs::OrderSide".into()].into(),
        public: ["sample/src/lib.rs::OrderSide".into()].into(),
    }
}

#[test]
fn clean_all_pending_corpus_passes_both_gates() {
    let fixture = fixture();
    let workspace = Workspace::new("fn helper() {}");
    let index = rust_index(&workspace.0).unwrap();
    let mut manifests = baseline();
    manifests[0].symbol.extend((3..5395).map(|n| Symbol {
        ts: format!("Pending{n}"),
        ..row("pending", "", "", "")
    }));
    assert_eq!(manifests[0].symbol.len(), 5395);
    assert!(
        check(
            &manifests,
            &index,
            &exemptions(&fixture.join("exempt.toml")).unwrap()
        )
        .is_empty()
    );
}

#[test]
fn public_enum_without_mapping_fails_invention() {
    let errors = check(&baseline(), &index(), &Exemptions::default());
    assert_eq!(
        errors,
        ["invention: sample/src/lib.rs::OrderSide: public item has no manifest rs or exemption"]
    );
}

#[test]
fn mapped_enum_and_justified_exemption_pass() {
    assert!(
        check(
            &one(row("ported", "sample/src/lib.rs::OrderSide", "mirror", "")),
            &index(),
            &Exemptions::default()
        )
        .is_empty()
    );
    let exemptions = Exemptions {
        exempt: vec![Exemption {
            rs: "sample/src/lib.rs::OrderSide".into(),
            reason: "Fixture for exact exemption handling".into(),
        }],
    };
    assert!(check(&baseline(), &index(), &exemptions).is_empty());
}

#[test]
fn notes_are_required_only_for_divergent_and_not_applicable() {
    for status in ["pending", "ported", "divergent", "not-applicable"] {
        let rs = if status == "ported" {
            "sample/src/lib.rs::OrderSide"
        } else {
            ""
        };
        let errors = check(
            &one(row(status, rs, "mirror", " \n")),
            &index(),
            &Exemptions::default(),
        );
        let note_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.contains("requires note"))
            .collect();
        assert_eq!(
            note_errors.len(),
            usize::from(matches!(status, "divergent" | "not-applicable")),
            "{errors:?}"
        );
        let (rs, rust) = if matches!(status, "pending" | "not-applicable") {
            ("", RustIndex::default())
        } else {
            ("sample/src/lib.rs::OrderSide", index())
        };
        assert!(
            check(
                &one(row(
                    status,
                    rs,
                    if rs.is_empty() { "" } else { "mirror" },
                    "intentional"
                )),
                &rust,
                &Exemptions::default()
            )
            .is_empty()
        );
    }
}

#[test]
fn ported_requires_both_mapping_and_layer() {
    let errors = check(
        &one(row("ported", "", "", "")),
        &RustIndex::default(),
        &Exemptions::default(),
    );
    assert_eq!(errors.len(), 1);
    assert!(errors[0].ends_with("ported requires rs"));
}

#[test]
fn every_status_resolves_nonempty_mapping() {
    for status in ["pending", "ported", "divergent", "not-applicable"] {
        let errors = check(
            &one(row(
                status,
                "sample/src/lib.rs::missing",
                "adapted",
                "Intentional target `sample/src/lib.rs::missing`",
            )),
            &RustIndex {
                ..RustIndex::default()
            },
            &Exemptions::default(),
        );
        assert_eq!(
            errors.len(),
            if matches!(status, "pending" | "not-applicable") {
                2
            } else {
                1
            }
        );
        assert!(errors.iter().any(|error| error.contains("OrderSide")
            && error.ends_with("rs does not resolve: sample/src/lib.rs::missing")));
    }
}

#[test]
fn invalid_and_stale_exemptions_fail() {
    for (rs, reason, expected) in [
        (
            "sample/src/lib.rs::OrderSide",
            " ",
            "exemption requires reason",
        ),
        (
            "missing",
            "glue",
            "exemption does not name a public Rust item",
        ),
    ] {
        let errors = check(
            &baseline(),
            &index(),
            &Exemptions {
                exempt: vec![Exemption {
                    rs: rs.into(),
                    reason: reason.into(),
                }],
            },
        );
        assert!(errors.iter().any(|e| e.contains(expected)), "{errors:?}");
        assert!(
            errors
                .iter()
                .any(|e| e.contains("public item has no manifest"))
        );
    }
}

struct Workspace(PathBuf);
impl Workspace {
    fn new(source: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "safe-cex-parity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(path.join("sample/src")).unwrap();
        fs::write(
            path.join("Cargo.toml"),
            "[workspace]\nmembers = [\"sample\"]\n",
        )
        .unwrap();
        fs::write(
            path.join("sample/Cargo.toml"),
            "[package]\nname = \"sample\"\n",
        )
        .unwrap();
        fs::write(path.join("sample/src/lib.rs"), source).unwrap();
        Self(path)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn syn_discovers_enums_methods_and_inline_namespaces() {
    let workspace = Workspace::new(
        r#"
        #[derive(Clone)] pub enum OrderSide { Buy, Sell }
        pub struct Exchange { pub field: usize }
        impl Exchange { pub fn place_order(&self) {} fn private_helper(&self) {} }
        pub mod nested { pub fn place_order() {} }
        mod private { pub fn place_order() {} }
        pub use nested::place_order;
        impl Clone for Exchange { fn clone(&self) -> Self { todo!() } }
        #[cfg(test)] pub fn test_helper() {}
        pub(crate) fn restricted() {}
    "#,
    );
    let index = rust_index(&workspace.0).unwrap();
    assert_eq!(index.public.len(), 5, "{:?}", index.public);
    for name in [
        "OrderSide",
        "Exchange",
        "Exchange::place_order",
        "nested::place_order",
        "private::place_order",
    ] {
        assert!(index.public.contains(&format!("sample/src/lib.rs::{name}")));
    }
    assert!(
        index
            .all
            .contains("sample/src/lib.rs::Exchange::private_helper")
    );
    assert!(index.all.contains("sample/src/lib.rs::restricted"));
    assert!(
        !index
            .all
            .iter()
            .any(|i| i.contains("test_helper") || i.ends_with("::Buy") || i.ends_with("::field"))
    );
    assert!(
        check(
            &one(row(
                "ported",
                "sample/src/lib.rs::Exchange::private_helper",
                "adapted",
                "Fixture maps `sample/src/lib.rs::Exchange::private_helper`"
            )),
            &index,
            &Exemptions::default()
        )
        .iter()
        .all(|e| e.starts_with("invention:"))
    );
}

#[test]
fn missing_inputs_and_unexpanded_macros_fail_closed() {
    assert!(manifests(&fixture().join("missing")).is_err());
    let workspace = Workspace::new("generate_api!();");
    assert!(
        rust_index(&workspace.0)
            .unwrap_err()
            .contains("cannot be indexed without expansion")
    );
}

#[test]
fn report_has_file_directory_crate_and_progress_counts() {
    let mut manifests = baseline();
    manifests[0].file.rust_files = vec!["sample/src/lib.rs".into()];
    manifests[0].symbol[0] = row("ported", "sample/src/lib.rs::OrderSide", "mirror", "");
    manifests[0].symbol[1].status = "divergent".into();
    manifests[0].symbol[2].status = "not-applicable".into();
    manifests[0].symbol[1].note =
        "Fixture deliberately diverges via `sample/src/lib.rs::OrderSide`.".into();
    manifests[0].symbol[1].rs = "sample/src/lib.rs::OrderSide".into();
    manifests[0].symbol[1].layer = "adapted".into();
    manifests[0].symbol[2].note = "Fixture has no Rust counterpart".into();
    let report = safe_cex_parity::report(&manifests, &index(), &Exemptions::default()).unwrap();
    for expected in [
        "file packages/safe-cex/src/types.ts\t1 / 3\t1\t1\t1",
        "dir packages/safe-cex/src\t1 / 3\t1\t1\t1",
        "crate sample\t1 / 2\t1\t1\t0",
        "crate (unassigned)\t0 / 1\t0\t0\t1",
        "total\t1 / 3\t1\t1\t1",
    ] {
        assert!(report.contains(expected), "{report}");
    }
}

#[test]
fn resolution_follows_declared_modules_and_excludes_external_tests() {
    let workspace =
        Workspace::new("pub mod live; #[cfg(test)] mod tests; #[path = \"other.rs\"] mod renamed;");
    fs::write(
        workspace.0.join("sample/src/live.rs"),
        "pub enum OrderSide { Buy, Sell }",
    )
    .unwrap();
    fs::write(
        workspace.0.join("sample/src/tests.rs"),
        "pub fn test_helper() {}",
    )
    .unwrap();
    fs::write(workspace.0.join("sample/src/dead.rs"), "pub fn ghost() {}").unwrap();
    fs::write(
        workspace.0.join("sample/src/other.rs"),
        "fn private_helper() {}",
    )
    .unwrap();
    let index = rust_index(&workspace.0).unwrap();
    assert_eq!(index.public.len(), 1);
    assert!(index.all.contains("sample/src/live.rs::OrderSide"));
    assert!(index.all.contains("sample/src/other.rs::private_helper"));
    for missing in [
        "sample/src/tests.rs::test_helper",
        "sample/src/dead.rs::ghost",
    ] {
        let errors = check(
            &one(row("pending", missing, "", "")),
            &index,
            &Exemptions::default(),
        );
        assert!(
            errors.iter().any(|e| e.contains("rs does not resolve")),
            "{errors:?}"
        );
    }
}

#[test]
fn unsupported_impl_owners_fail_explicitly() {
    for source in [
        "struct Exchange; mod nested { impl super::Exchange { pub fn f() {} } }",
        "use somewhere::Exchange as Alias; impl Alias { pub fn f() {} }",
    ] {
        let workspace = Workspace::new(source);
        assert!(rust_index(&workspace.0).unwrap_err().contains("impl owner"));
    }
}

#[test]
fn direct_same_crate_impl_owners_remain_invention_gated() {
    for source in [
        "mod model { pub struct Owner; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "mod model { pub struct Original; } use crate::model::Original as Owner; impl Owner { pub fn added() {} }",
        "use external::*; pub struct Owner; impl Owner { pub fn added() {} }",
        "use external::Trait as _; pub struct Owner; impl Owner { pub fn added() {} }",
    ] {
        let workspace = Workspace::new(source);
        let rust = rust_index(&workspace.0).unwrap();
        let target = "sample/src/lib.rs::Owner::added";
        assert!(rust.public.contains(target), "OWNER public method indexed");
        assert!(
            check(&baseline(), &rust, &Exemptions::default()).contains(&format!(
                "invention: {target}: public item has no manifest rs or exemption"
            )),
            "OWNER public method stays invention-gated"
        );
    }
}

#[test]
fn production_cross_trade_nominal_beside_glob_remains_indexed() {
    let workspace = Workspace::new("pub mod cross_trade;");
    let folder = workspace.0.join("sample/src/cross_trade");
    fs::create_dir(&folder).unwrap();
    for (name, source) in [
        (
            "mod.rs",
            include_str!("../../safe-cex-exchange/src/cross_trade/mod.rs"),
        ),
        (
            "types.rs",
            include_str!("../../safe-cex-exchange/src/cross_trade/types.rs"),
        ),
        (
            "helpers.rs",
            include_str!("../../safe-cex-exchange/src/cross_trade/helpers.rs"),
        ),
        (
            "manager.rs",
            include_str!("../../safe-cex-exchange/src/cross_trade/manager.rs"),
        ),
        (
            "place_order.rs",
            include_str!("../../safe-cex-exchange/src/cross_trade/place_order.rs"),
        ),
    ] {
        fs::write(folder.join(name), source).unwrap();
    }
    let rust = rust_index(&workspace.0)
        .unwrap_or_else(|error| panic!("GLOB production CrossTradeLoop must index: {error}"));
    let expected = std::collections::BTreeSet::from([
        "sample/src/cross_trade/mod.rs::CrossTradeLoop".to_string(),
        "sample/src/cross_trade/mod.rs::CrossTradeLoop::is_disposed".to_string(),
        "sample/src/cross_trade/mod.rs::CrossTradeLoop::run".to_string(),
    ]);
    let actual: std::collections::BTreeSet<_> = rust
        .public
        .iter()
        .filter(|target| {
            target.as_str() == "sample/src/cross_trade/mod.rs::CrossTradeLoop"
                || target.starts_with("sample/src/cross_trade/mod.rs::CrossTradeLoop::")
        })
        .cloned()
        .collect();
    assert_eq!(
        actual, expected,
        "GLOB exact native CrossTradeLoop declarations"
    );
    let errors = check(&baseline(), &rust, &Exemptions::default());
    for target in expected {
        assert!(
            errors.contains(&format!(
                "invention: {target}: public item has no manifest rs or exemption"
            )),
            "GLOB resolved public declarations remain invention-gated: {target}"
        );
    }
}

#[test]
fn uncertain_imported_impl_owners_still_fail_closed() {
    for source in [
        "use external::Owner; impl Owner { pub fn added() {} }",
        "mod model { pub struct Original; pub type Owner = Original; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "mod model { pub struct Original; pub use Original as Owner; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "#[cfg(unix)] mod model { pub struct Owner; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "#[cfg_attr(feature=\"x\", cfg(unix))] mod model { pub struct Owner; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "mod model { #[cfg(unix)] pub struct Owner; } use crate::model::Owner; impl Owner { pub fn added() {} }",
        "mod model { pub struct Owner; } #[cfg(unix)] use crate::model::Owner; impl Owner { pub fn added() {} }",
        "mod model { pub struct Owner; } use crate::model::Owner; use crate::model::Owner; impl Owner { pub fn added() {} }",
        "pub struct Owner; use external::Owner; impl Owner { pub fn added() {} }",
        "#[cfg(unix)] pub struct Owner; use external::*; impl Owner { pub fn added() {} }",
        "mod model { pub struct Owner; } use crate::model::*; impl Owner { pub fn added() {} }",
        "pub struct Owner; mod nested { use external::Owner; impl Owner { pub fn added() {} } }",
    ] {
        let workspace = Workspace::new(source);
        assert!(
            rust_index(&workspace.0).is_err(),
            "OWNER uncertain origin must refuse: {source}"
        );
    }
    let workspace =
        Workspace::new("mod model; use crate::model::Owner; impl Owner { pub fn added() {} }");
    fs::write(
        workspace.0.join("sample/src/model.rs"),
        "#![cfg(unix)] pub struct Owner;",
    )
    .unwrap();
    assert!(
        rust_index(&workspace.0).is_err(),
        "OWNER conditional file must refuse"
    );
}

#[test]
fn nested_imported_impl_owners_use_their_own_namespace() {
    let workspace = Workspace::new(
        "mod model { pub struct Owner; } mod nested { use crate::model::Owner; impl Owner { pub fn added() {} } }",
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert!(
        rust.public
            .contains("sample/src/lib.rs::nested::Owner::added"),
        "OWNER nested public method indexed"
    );
}

#[test]
fn production_worker_startup_keeps_indexable_declarations() {
    let workspace = Workspace::new("mod exchange_worker; mod start;");
    fs::write(
        workspace.0.join("sample/src/exchange_worker.rs"),
        include_str!("../../safe-cex-webworker/src/exchange_worker.rs"),
    )
    .unwrap();
    fs::write(
        workspace.0.join("sample/src/start.rs"),
        include_str!("../../safe-cex-webworker/src/exchange_worker_start.rs"),
    )
    .unwrap();
    let rust = rust_index(&workspace.0)
        .unwrap_or_else(|error| panic!("WORKER startup source index: {error}"));
    let target = "sample/src/start.rs::ExchangeWorker::start";
    assert!(
        rust.public.contains(target),
        "WORKER startup public method indexed"
    );
    assert!(
        check(&baseline(), &rust, &Exemptions::default()).contains(&format!(
            "invention: {target}: public item has no manifest rs or exemption"
        )),
        "WORKER startup stays invention-gated"
    );
}

#[test]
fn custom_and_binary_roots_use_their_parent_module_directory() {
    let workspace = Workspace::new("mod helper;");
    fs::rename(
        workspace.0.join("sample/src/lib.rs"),
        workspace.0.join("sample/src/root.rs"),
    )
    .unwrap();
    fs::write(
        workspace.0.join("sample/Cargo.toml"),
        "[package]\nname = \"sample\"\n[lib]\npath = \"src/root.rs\"\n",
    )
    .unwrap();
    fs::write(
        workspace.0.join("sample/src/helper.rs"),
        "pub fn from_custom_root() {}",
    )
    .unwrap();
    fs::create_dir_all(workspace.0.join("sample/src/bin")).unwrap();
    fs::write(workspace.0.join("sample/src/bin/worker.rs"), "mod helpers;").unwrap();
    fs::write(
        workspace.0.join("sample/src/bin/helpers.rs"),
        "pub fn from_binary_root() {}",
    )
    .unwrap();
    let index = rust_index(&workspace.0).unwrap();
    assert!(
        index
            .public
            .contains("sample/src/helper.rs::from_custom_root")
    );
    assert!(
        index
            .public
            .contains("sample/src/bin/helpers.rs::from_binary_root")
    );
}

#[test]
fn external_module_named_main_is_not_a_crate_root() {
    let workspace = Workspace::new("#[path = \"main.rs\"] mod inner;");
    fs::write(
        workspace.0.join("sample/Cargo.toml"),
        "[package]\nname = \"sample\"\nautobins = false\n",
    )
    .unwrap();
    fs::write(workspace.0.join("sample/src/main.rs"), "mod child;").unwrap();
    fs::create_dir_all(workspace.0.join("sample/src/main")).unwrap();
    fs::write(
        workspace.0.join("sample/src/main/child.rs"),
        "pub fn child() {}",
    )
    .unwrap();
    assert!(
        rust_index(&workspace.0)
            .unwrap()
            .public
            .contains("sample/src/main/child.rs::child")
    );
}

fn cli(workspace: &std::path::Path, manifest: &std::path::Path) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_parity-check"))
        .args([
            "--workspace",
            workspace.to_str().unwrap(),
            "--manifest",
            manifest.to_str().unwrap(),
            "--exempt",
            fixture().join("exempt.toml").to_str().unwrap(),
        ])
        .output()
        .unwrap()
}

#[test]
fn cli_passes_mapped_clean_fixture_and_fails_invention_with_enum_name() {
    let clean = cli(&fixture().join("sdk-rs"), &fixture().join("manifest"));
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    assert!(String::from_utf8_lossy(&clean.stdout).contains("3 rows, 1 public Rust items"));
    let workspace = Workspace::new("pub enum OrderSide { Buy, Sell }");
    let failure = cli(&workspace.0, &pending_manifest());
    assert!(!failure.status.success());
    assert!(
        String::from_utf8_lossy(&failure.stderr)
            .contains("invention: sample/src/lib.rs::OrderSide: public item has no manifest rs")
    );
}

#[test]
fn cli_reports_all_contradictory_rows_and_missing_targets() {
    let workspace = Workspace::new("fn helper() {}");
    let dir = workspace.0.join("manifest");
    fs::create_dir(&dir).unwrap();
    fs::write(
        dir.join("broken.ts.toml"),
        r#"
        [file]
        ts = "packages/safe-cex/src/broken.ts"
        [[symbol]]
        ts = "missingPort"
        kind = "function"
        rs = ""
        layer = ""
        status = "ported"
        note = ""
        [[symbol]]
        ts = "unexplained"
        kind = "function"
        rs = "sample/src/lib.rs::missing"
        layer = ""
        status = "divergent"
        note = ""
        [[symbol]]
        ts = "inapplicable"
        kind = "function"
        rs = ""
        layer = ""
        status = "not-applicable"
        note = ""
    "#,
    )
    .unwrap();
    let output = cli(&workspace.0, &dir);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "missingPort (function): ported requires rs",
        "unexplained (function): nonempty rs requires layer",
        "unexplained (function): divergent requires note",
        "rs does not resolve: sample/src/lib.rs::missing",
        "inapplicable (function): not-applicable requires note",
    ] {
        assert!(stderr.contains(expected), "{stderr}");
    }
}

#[test]
fn report_cli_prints_mapped_progress_and_bad_options_fail() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parity-report"))
        .arg("--workspace")
        .arg(fixture().join("sdk-rs"))
        .arg("--manifest")
        .arg(fixture().join("manifest"))
        .arg("--exempt")
        .arg(fixture().join("exempt.toml"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Basis: manifest claims validated against the Rust source index"));
    assert!(stdout.contains("behavioral parity is not verified"));
    assert!(stdout.contains("verified ported rows / total rows"));
    assert!(stdout.contains("total\t1 / 3\t1\t0\t0"));
    let bad = std::process::Command::new(env!("CARGO_BIN_EXE_parity-check"))
        .arg("--manifest")
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("--manifest: missing path"));
}

#[test]
fn cli_accepts_a_parsed_5395_row_pending_baseline() {
    let workspace = Workspace::new("fn helper() {}");
    let dir = workspace.0.join("manifest");
    fs::create_dir(&dir).unwrap();
    let mut corpus = String::from("[file]\nts = \"packages/safe-cex/src/types.ts\"\n");
    for n in 0..5395 {
        corpus.push_str(&format!("\n[[symbol]]\nts = \"Symbol{n}\"\nkind = \"type\"\nlayer = \"\"\nrs = \"\"\nstatus = \"pending\"\nnote = \"\"\n"));
    }
    fs::write(dir.join("types.ts.toml"), corpus).unwrap();
    let output = cli(&workspace.0, &dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("5395 rows, 0 public Rust items"));
}

#[test]
fn cli_accepts_real_mapped_enum_with_other_rows_pending() {
    let workspace = Workspace::new("pub enum OrderSide { Buy, Sell }");
    let dir = workspace.0.join("manifest");
    fs::create_dir(&dir).unwrap();
    let corpus = fs::read_to_string(pending_manifest().join("types.ts.toml"))
        .unwrap()
        .replacen("[file]", "[file]\nrust_files = [\"sample/src/lib.rs\"]", 1)
        .replacen("rs = \"\"", "rs = \"sample/src/lib.rs::OrderSide\"", 1)
        .replacen("layer = \"\"", "layer = \"mirror\"", 1)
        .replacen("status = \"pending\"", "status = \"ported\"", 1);
    fs::write(dir.join("types.ts.toml"), corpus).unwrap();
    let output = cli(&workspace.0, &dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn pending_mapping_is_a_status_error_and_does_not_discharge_invention() {
    let errors = check(
        &one(row("pending", "sample/src/lib.rs::OrderSide", "", "")),
        &index(),
        &Exemptions::default(),
    );
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|e| e.contains("OrderSide (enum): pending requires empty rs"))
    );
    assert!(
        errors
            .iter()
            .any(|e| e.starts_with("invention: sample/src/lib.rs::OrderSide"))
    );
}

fn collapsed_rows(first_note: &str, second_note: &str) -> Vec<Manifest> {
    let mut first = one(row(
        "ported",
        "sample/src/lib.rs::OrderSide",
        "mirror",
        first_note,
    ));
    let mut second = one(row(
        "ported",
        "sample/src/lib.rs::OrderSide",
        "mirror",
        second_note,
    ));
    second[0].file.ts = "packages/safe-cex/src/other.ts".into();
    second[0].symbol[0].ts = "OrderSide".into();
    first.extend(second);
    first
}

#[test]
fn duplicate_ported_claims_need_notes_on_every_row_across_files() {
    for (first_note, second_note, expected) in [
        ("", "", 2),
        ("Both enums share representations", " \n", 1),
        ("Both enums share representations", "Same translation", 0),
    ] {
        let errors = check(
            &collapsed_rows(first_note, second_note),
            &index(),
            &Exemptions::default(),
        );
        assert_eq!(errors.len(), expected, "{errors:?}");
        assert!(
            errors
                .iter()
                .all(|e| e.contains("sample/src/lib.rs::OrderSide is claimed by 2 ported rows"))
        );
        if first_note.is_empty() {
            assert!(errors.iter().any(|e| e.contains("types.ts::OrderSide")));
        }
        if second_note.trim().is_empty() {
            assert!(errors.iter().any(|e| e.contains("other.ts::OrderSide")));
        }
    }
    let mut mixed = collapsed_rows("", "Intentional divergence");
    mixed[1].symbol[0].status = "divergent".into();
    assert!(check(&mixed, &index(), &Exemptions::default()).is_empty());
}

#[test]
fn report_exposes_collapsed_rows_and_counts_nonpending_claims_once_per_scope() {
    let mut corpus = collapsed_rows("shared implementation", "shared implementation");
    corpus[0].symbol.push(Symbol {
        ts: "AdaptedSide".into(),
        ..row(
            "divergent",
            "sample/src/lib.rs::adapted",
            "adapted",
            "Projection is `sample/src/lib.rs::adapted`",
        )
    });
    corpus[0].symbol.push(Symbol {
        ts: "UnusedSide".into(),
        ..row("not-applicable", "", "", "no analogue")
    });
    corpus[0].symbol.push(Symbol {
        ts: "PendingSide".into(),
        ..row("pending", "", "", "")
    });
    let mut rust = index();
    rust.all.insert("sample/src/lib.rs::adapted".into());
    let report = safe_cex_parity::report(&corpus, &rust, &Exemptions::default()).unwrap();
    for expected in [
        "file packages/safe-cex/src/types.ts\t1 / 4\t2\t1\t1",
        "file packages/safe-cex/src/other.ts\t1 / 1\t1\t0\t0",
        "dir packages/safe-cex/src\t2 / 5\t2\t1\t1",
        "crate sample\t2 / 3\t2\t1\t0",
        "crate (unassigned)\t0 / 2\t0\t0\t1",
        "total\t2 / 5\t2\t1\t1",
    ] {
        assert!(report.contains(expected), "{report}");
    }
}

#[test]
fn external_public_reexports_require_rows_or_exact_exemptions() {
    for (source, exported) in [
        ("pub use std::time::Duration as Interval;", "Interval"),
        ("pub use reqwest::Client;", "Client"),
        (
            "use reqwest::Client as Hidden; pub use Hidden as Client;",
            "Client",
        ),
        (
            "use reqwest::Client as Hidden; pub use crate::Hidden as Client;",
            "Client",
        ),
        (
            "mod inner { use reqwest::Client as Hidden; pub use self::Hidden as Client; }",
            "inner::Client",
        ),
        (
            "mod inner { use reqwest::Client as Hidden; mod nested { pub use super::Hidden as Client; } }",
            "inner::nested::Client",
        ),
    ] {
        let workspace = Workspace::new(source);
        let rust = rust_index(&workspace.0).unwrap();
        let id = format!("sample/src/lib.rs::{exported}");
        assert_eq!(rust.public, [id.clone()].into(), "{source}");
        let failure = check(&baseline(), &rust, &Exemptions::default());
        assert!(
            failure
                .iter()
                .any(|e| e.starts_with(&format!("invention: {id}:"))),
            "{failure:?}"
        );
        assert!(
            check(
                &one(row(
                    "divergent",
                    &id,
                    "adapted",
                    &format!("Deliberate dependency API exposure `{id}`")
                )),
                &rust,
                &Exemptions::default()
            )
            .is_empty()
        );
        let exemptions = Exemptions {
            exempt: vec![Exemption {
                rs: id,
                reason: "FFI fixture uses the host duration type".into(),
            }],
        };
        assert!(check(&baseline(), &rust, &exemptions).is_empty());
    }
}

#[test]
fn reexports_handle_groups_self_absolute_paths_and_nested_scopes() {
    let workspace = Workspace::new(
        r#"
        mod std { pub mod time { pub struct Duration; } }
        pub use ::std::time::{self as clock, Duration as Interval};
        mod exports { pub use std::{time::Duration, collections::{HashMap as Map, HashSet}}; }
        #[cfg(test)] pub use reqwest::Client as TestClient;
        #[cfg(feature = "transport")] pub use reqwest::Client as FeatureClient;
        pub(crate) use reqwest::Client as RestrictedClient;
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    for name in [
        "clock",
        "Interval",
        "exports::Duration",
        "exports::Map",
        "exports::HashSet",
        "FeatureClient",
    ] {
        assert!(
            rust.public.contains(&format!("sample/src/lib.rs::{name}")),
            "{:?}",
            rust.public
        );
    }
    assert!(!rust.public.contains("sample/src/lib.rs::TestClient"));
    assert!(!rust.public.contains("sample/src/lib.rs::RestrictedClient"));
    assert!(
        rust.public
            .contains("sample/src/lib.rs::std::time::Duration")
    );
}

#[test]
fn proven_local_reexports_remain_structural() {
    let workspace = Workspace::new(
        r#"
        pub enum OrderSide { Buy, Sell }
        pub use self::{OrderSide as Side, OrderSide::Buy};
        pub use crate::OrderSide as RootSide;
        pub mod inner {
            pub fn place_order() {}
            pub use self::place_order as send;
            pub use super::OrderSide as ParentSide;
        }
        pub use inner::{place_order as send, self as nested};
        pub use crate::inner::*;
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(
        rust.public,
        [
            "sample/src/lib.rs::OrderSide".into(),
            "sample/src/lib.rs::inner::place_order".into()
        ]
        .into()
    );
}

#[test]
fn external_and_unresolved_glob_reexports_fail_loudly() {
    for source in [
        "pub use reqwest::*;",
        "use reqwest as transport; pub use crate::transport::*;",
        "pub use {std::time::*, std::collections::*};",
    ] {
        let workspace = Workspace::new(source);
        let error = rust_index(&workspace.0).unwrap_err();
        assert!(
            error.contains("sample/src/lib.rs::*")
                && error.contains("public glob re-export cannot be indexed"),
            "{error}"
        );
    }
}

#[test]
fn workspace_dependency_reexports_resolve_declared_owners_even_with_aliases() {
    let workspace = Workspace::new("pub use types::{OrderSide as Side, self as domain};");
    fs::create_dir_all(workspace.0.join("types/src")).unwrap();
    fs::write(
        workspace.0.join("types/Cargo.toml"),
        "[package]\nname = \"safe-cex-types\"\n[lib]\nname = \"wire_types\"\n",
    )
    .unwrap();
    fs::write(
        workspace.0.join("types/src/lib.rs"),
        "pub enum OrderSide { Buy, Sell }",
    )
    .unwrap();
    fs::write(workspace.0.join("Cargo.toml"), "[workspace]\nmembers = [\"sample\", \"types\"]\n[workspace.dependencies]\ntypes = { package = \"safe-cex-types\", path = \"types\" }\n").unwrap();
    fs::write(
        workspace.0.join("sample/Cargo.toml"),
        "[package]\nname = \"sample\"\n[dependencies]\ntypes.workspace = true\n",
    )
    .unwrap();
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(
        rust.public,
        ["safe-cex-types/src/lib.rs::OrderSide".into()].into()
    );
    fs::write(
        workspace.0.join("sample/src/lib.rs"),
        "pub use wire_types::OrderSide as Side;",
    )
    .unwrap();
    fs::write(
        workspace.0.join("sample/Cargo.toml"),
        "[package]\nname = \"sample\"\n[dependencies]\nsafe-cex-types = { path = \"../types\" }\n",
    )
    .unwrap();
    assert_eq!(rust_index(&workspace.0).unwrap().public, rust.public);
}

#[test]
fn dependency_names_cannot_mask_local_external_import_aliases() {
    let workspace = Workspace::new("mod sample {} pub use ::std::time::Duration as Interval;");
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(rust.public, ["sample/src/lib.rs::Interval".into()].into());
    fs::write(
        workspace.0.join("sample/src/lib.rs"),
        "use std::time as sample; pub use sample::Duration as Interval;",
    )
    .unwrap();
    assert_eq!(rust_index(&workspace.0).unwrap().public, rust.public);
}

#[test]
fn reexports_of_external_items_fail_through_cli_with_exported_alias_name() {
    let workspace = Workspace::new("pub use std::time::Duration as Interval;");
    let output = cli(&workspace.0, &pending_manifest());
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "invention: sample/src/lib.rs::Interval: public item has no manifest rs or exemption"
    ));
}

#[test]
fn conditional_imports_cannot_borrow_a_local_declarations_proof() {
    for path in ["transport::Client", "crate::transport::Client"] {
        let workspace = Workspace::new(&format!(
            r#"
            #[cfg(unix)] mod transport {{ pub struct Client; }}
            #[cfg(windows)] use reqwest as transport;
            pub use {path} as ExternalClient;
        "#
        ));
        let rust = rust_index(&workspace.0).unwrap();
        assert!(rust.public.contains("sample/src/lib.rs::ExternalClient"));
        let errors = check(
            &one(row(
                "ported",
                "sample/src/lib.rs::transport::Client",
                "adapted",
                "",
            )),
            &rust,
            &Exemptions::default(),
        );
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("invention: sample/src/lib.rs::ExternalClient"))
        );
    }
}

#[test]
fn library_and_binary_roots_cannot_share_reexport_proofs() {
    let workspace = Workspace::new("pub use reqwest::Client;");
    fs::write(
        workspace.0.join("sample/src/main.rs"),
        "mod reqwest { pub struct Client; } fn main() {}",
    )
    .unwrap();
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(
        rust.public,
        [
            "sample/src/lib.rs::Client".into(),
            "sample/src/main.rs::reqwest::Client".into()
        ]
        .into()
    );
    let mut corpus = one(row(
        "ported",
        "sample/src/main.rs::reqwest::Client",
        "adapted",
        "Fixture maps `sample/src/main.rs::reqwest::Client`",
    ));
    corpus[0].file.rust_files = vec!["sample/src/main.rs".into()];
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("invention: sample/src/lib.rs::Client"));
}

#[test]
fn local_foreign_declaration_reexports_remain_structural() {
    let workspace = Workspace::new(
        r#"
        unsafe extern "C" { pub fn foreign_call(); pub static FOREIGN_FLAG: u8; }
        pub use crate::{foreign_call as call, FOREIGN_FLAG as FLAG};
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(
        rust.public,
        [
            "sample/src/lib.rs::foreign_call".into(),
            "sample/src/lib.rs::FOREIGN_FLAG".into()
        ]
        .into()
    );
}

#[test]
fn conflicting_dependency_ownership_across_targets_fails_loudly() {
    for (first, second) in [
        ("{ path = '../types' }", "{ version = '1' }"),
        ("{ version = '1' }", "{ path = '../types' }"),
    ] {
        let workspace = Workspace::new("pub use other::Client;");
        fs::create_dir_all(workspace.0.join("types/src")).unwrap();
        fs::write(
            workspace.0.join("types/Cargo.toml"),
            "[package]\nname = \"other\"\n",
        )
        .unwrap();
        fs::write(workspace.0.join("types/src/lib.rs"), "pub struct Client;").unwrap();
        fs::write(
            workspace.0.join("Cargo.toml"),
            "[workspace]\nmembers = [\"sample\", \"types\"]\n",
        )
        .unwrap();
        fs::write(workspace.0.join("sample/Cargo.toml"), format!("[package]\nname = \"sample\"\n[target.'cfg(unix)'.dependencies]\nother = {first}\n[target.'cfg(windows)'.dependencies]\nother = {second}\n")).unwrap();
        let error = rust_index(&workspace.0).unwrap_err();
        assert!(
            error.contains(
                "dependency other: differing workspace ownership across target configurations"
            ),
            "{error}"
        );
    }
}

#[test]
fn conditional_private_globs_cannot_borrow_a_local_declarations_proof() {
    let workspace = Workspace::new(
        r#"
        #[cfg(unix)] mod transport { pub struct Client; }
        #[cfg(windows)] use reqwest::*;
        pub use transport::Client as ExternalClient;
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert!(rust.public.contains("sample/src/lib.rs::ExternalClient"));
    let failure = check(
        &one(row(
            "ported",
            "sample/src/lib.rs::transport::Client",
            "adapted",
            "",
        )),
        &rust,
        &Exemptions::default(),
    );
    assert!(
        failure
            .iter()
            .any(|e| e.starts_with("invention: sample/src/lib.rs::ExternalClient"))
    );
}

#[test]
fn extern_crate_aliases_do_not_hide_dependency_exports() {
    let workspace = Workspace::new(
        r#"
        #[cfg(unix)] mod transport { pub mod time { pub struct Duration; } }
        #[cfg(windows)] extern crate std as transport;
        pub use transport::time::Duration as Interval;
        pub extern crate reqwest as public_transport;
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert!(rust.public.contains("sample/src/lib.rs::Interval"));
    assert!(rust.public.contains("sample/src/lib.rs::public_transport"));
}

#[test]
fn local_bridge_globs_cannot_hide_external_aliases() {
    let workspace = Workspace::new(
        r#"
        mod bridge { pub use reqwest::Client; }
        #[cfg(unix)] pub struct Client;
        #[cfg(windows)] use bridge::*;
        pub use Client as Alias;
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert!(rust.public.contains("sample/src/lib.rs::Alias"));
    let mut corpus = one(row(
        "ported",
        "sample/src/lib.rs::Client",
        "adapted",
        "Fixture maps `sample/src/lib.rs::Client`",
    ));
    corpus[0].symbol.push(row(
        "ported",
        "sample/src/lib.rs::bridge::Client",
        "adapted",
        "Fixture maps `sample/src/lib.rs::bridge::Client`",
    ));
    corpus[0].symbol[1].ts = "TransportClient".into();
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].starts_with("invention: sample/src/lib.rs::Alias"));
}

#[test]
fn committed_clean_fixture_maps_a_real_public_enum() {
    let rust = rust_index(&fixture().join("sdk-rs")).unwrap();
    assert_eq!(rust.public, ["sample/src/lib.rs::OrderSide".into()].into());
    let corpus = manifests(&fixture().join("manifest")).unwrap();
    assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
}

#[test]
fn diagnostic_item_macro_does_not_hide_following_public_items() {
    let workspace = Workspace::new(
        r#"
        #[cfg(all(feature = "node", feature = "wasm"))]
        compile_error!("mutually exclusive features");
        pub enum OrderSide { Buy, Sell }
    "#,
    );
    let rust = rust_index(&workspace.0).unwrap();
    assert_eq!(rust.public, ["sample/src/lib.rs::OrderSide".into()].into());
    assert!(check(&baseline(), &rust, &Exemptions::default())[0].starts_with("invention:"));
}

#[test]
fn all_other_item_macro_invocations_still_fail_loudly() {
    for source in [
        "include!(\"generated.rs\");",
        "generate_api!();",
        "thread_local! { pub static UNMAPPED: u32 = 0; }",
        "std::thread_local! { pub static UNMAPPED: u32 = 0; }",
        "macro_rules! generate_api { () => { pub fn invented() {} } } generate_api!();",
        "other::compile_error!(\"not proven diagnostic\");",
    ] {
        let workspace = Workspace::new(source);
        assert!(
            rust_index(&workspace.0)
                .unwrap_err()
                .contains("cannot be indexed without expansion")
        );
    }
}

#[test]
fn production_chaser_singleton_keeps_indexable_declarations() {
    let workspace = Workspace::new(include_str!(
        "../../safe-cex-webworker/src/chaser_manager.rs"
    ));
    let rust = rust_index(&workspace.0)
        .unwrap_or_else(|error| panic!("CHASER production source index: {error}"));
    let target = "sample/src/lib.rs::chaser_order_manager_instance";
    assert!(
        rust.public.contains(target),
        "CHASER public accessor indexed"
    );
    let errors = check(&baseline(), &rust, &Exemptions::default());
    assert!(
        errors.contains(&format!(
            "invention: {target}: public item has no manifest rs or exemption"
        )),
        "CHASER accessor stays invention-gated: {errors:?}"
    );
}

#[test]
fn production_twap_singleton_keeps_indexable_declarations() {
    let workspace = Workspace::new(include_str!("../../safe-cex-webworker/src/twap_manager.rs"));
    let rust = rust_index(&workspace.0)
        .unwrap_or_else(|error| panic!("TWAP production source index: {error}"));
    let target = "sample/src/lib.rs::twap_manager";
    assert!(rust.public.contains(target), "TWAP public accessor indexed");
    assert!(
        check(&baseline(), &rust, &Exemptions::default()).contains(&format!(
            "invention: {target}: public item has no manifest rs or exemption"
        )),
        "TWAP accessor stays invention-gated"
    );
}

#[test]
fn production_poseidon_example_mapping_accounts_for_its_console_split() {
    let workspace = Workspace::new("mod poseidon_example;");
    fs::rename(
        workspace.0.join("sample"),
        workspace.0.join("safe-cex-crypto"),
    )
    .unwrap();
    fs::write(
        workspace.0.join("Cargo.toml"),
        "[workspace]\nmembers = [\"safe-cex-crypto\"]\n",
    )
    .unwrap();
    fs::write(
        workspace.0.join("safe-cex-crypto/Cargo.toml"),
        "[package]\nname = \"safe-cex-crypto\"\n",
    )
    .unwrap();
    fs::write(
        workspace.0.join("safe-cex-crypto/src/poseidon_example.rs"),
        include_str!("../../safe-cex-crypto/src/poseidon_example.rs"),
    )
    .unwrap();
    let rust = rust_index(&workspace.0).unwrap();
    let manifest: Manifest = toml::from_str(include_str!(
        "../../parity/manifest/poseidon-crypto/example.ts.toml"
    ))
    .unwrap();
    let [main] = manifest.symbol.as_slice() else {
        panic!("POSEIDON example has exactly the source main row");
    };
    assert_eq!(main.ts, "main");
    assert_eq!(main.kind, "function");
    assert_eq!(main.status, "divergent");
    assert_eq!(main.layer, "adapted");
    let safe_cex_parity::RustMapping::Split(targets) = &main.rs else {
        panic!("POSEIDON production split mapping");
    };
    let expected = [
        "safe-cex-crypto/src/poseidon_example.rs::main",
        "safe-cex-crypto/src/poseidon_example.rs::ConsoleValue",
        "safe-cex-crypto/src/poseidon_example.rs::ExampleConsole",
        "safe-cex-crypto/src/poseidon_example.rs::ExampleConsole::log",
    ]
    .map(str::to_owned);
    assert_eq!(
        targets,
        expected.as_slice(),
        "POSEIDON exact native targets"
    );
    assert_eq!(
        rust.public,
        expected.into_iter().collect(),
        "POSEIDON independently expected public declarations"
    );
    let errors = check(&[manifest], &rust, &Exemptions::default());
    assert!(
        errors.is_empty(),
        "POSEIDON production manifest validates without exemptions: {errors:?}"
    );
}

#[test]
fn production_lighter_transaction_declarations_are_indexable() {
    let workspace = Workspace::new("mod transactions;");
    fs::write(
        workspace.0.join("sample/src/transactions.rs"),
        include_str!("../../safe-cex-venue-lighter/src/http_transactions.rs"),
    )
    .unwrap();
    let rust = rust_index(&workspace.0)
        .unwrap_or_else(|error| panic!("LIGHTER transaction source index: {error}"));
    for name in [
        "L2CreateOrderTxInfo",
        "L2CreateGroupedOrdersTxInfo",
        "L2CancelOrderTxInfo",
        "L2TransferTxInfo",
        "L2UpdateLeverageTxInfo",
        "L2ApproveIntegratorTxInfo",
        "L2WithdrawalTxInfo",
        "AnyTxInfo",
    ] {
        let target = format!("sample/src/transactions.rs::{name}");
        assert!(
            rust.public.contains(&target),
            "LIGHTER public transaction indexed: {name}"
        );
        assert!(
            check(&baseline(), &rust, &Exemptions::default()).contains(&format!(
                "invention: {target}: public item has no manifest rs or exemption"
            )),
            "LIGHTER transaction stays invention-gated: {name}"
        );
    }
}

#[test]
fn shadowed_or_uncertain_diagnostic_macros_fail_loudly() {
    for source in [
        "macro_rules! compile_error { () => { pub fn invented() {} } } compile_error!();",
        "use external::declare as compile_error; compile_error!();",
        "use external::*; compile_error!();",
        "#[macro_use] extern crate external; compile_error!();",
        "#[cfg_attr(feature = \"x\", macro_use)] extern crate external; compile_error!();",
        "#[cfg_attr(feature = \"x\", cfg_attr(feature = \"y\", macro_use))] extern crate external; compile_error!();",
        "#[macro_use] mod macros {} compile_error!();",
        "compile_error!(); mod nested { macro_rules! compile_error { () => {} } }",
    ] {
        let workspace = Workspace::new(source);
        assert!(
            rust_index(&workspace.0)
                .unwrap_err()
                .contains("shadowed or uncertain diagnostic origin")
        );
    }
}

#[test]
fn conditional_module_macro_imports_remain_unsupported() {
    let workspace = Workspace::new(
        r#"
        #[cfg_attr(feature = "x", macro_use)] mod macros {}
        compile_error!();
    "#,
    );
    assert!(rust_index(&workspace.0).is_err());
}

#[test]
fn hardening_fixtures_prove_each_gate_through_cli() {
    let root = fixture().parent().unwrap().join("hardening");
    for (name, expected) in [
        ("layer-valid", None),
        ("layer-invalid", Some("nonempty rs requires layer")),
        ("array-valid", None),
        ("array-invalid", Some("array rs requires note")),
        (
            "array-unresolved",
            Some("rs does not resolve: sample/src/lib.rs::missing"),
        ),
        ("names-valid", None),
        ("names-noted", None),
        (
            "names-invalid",
            Some("Rust item path must be can_sign_payload"),
        ),
    ] {
        let output = cli(&root.join("sdk-rs"), &root.join(name));
        let stderr = String::from_utf8_lossy(&output.stderr);
        if let Some(expected) = expected {
            assert!(!output.status.success(), "{name} unexpectedly passed");
            assert!(stderr.contains(expected), "{name}: {stderr}");
        } else {
            assert!(output.status.success(), "{name}: {stderr}");
        }
    }
    let corpus = manifests(&root.join("array-valid")).unwrap();
    let rust = rust_index(&root.join("sdk-rs")).unwrap();
    let report = safe_cex_parity::report(&corpus, &rust, &Exemptions::default()).unwrap();
    assert!(report.contains("crate sample\t1 / 1\t2\t0\t0"), "{report}");
    assert!(report.contains("total\t1 / 1\t2\t0\t0"), "{report}");
}

#[test]
fn empty_mappings_and_not_applicable_cannot_carry_layer_or_rust() {
    for status in ["pending", "divergent", "not-applicable"] {
        let errors = check(
            &one(row(status, "", "mirror", "reason")),
            &RustIndex::default(),
            &Exemptions::default(),
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("empty rs requires empty layer")),
            "{errors:?}"
        );
    }
    let errors = check(
        &one(row(
            "not-applicable",
            "sample/src/lib.rs::OrderSide",
            "adapted",
            "reason",
        )),
        &index(),
        &Exemptions::default(),
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("not-applicable requires empty rs")),
        "{errors:?}"
    );
}

#[test]
fn naming_conventions_cover_manifest_member_forms() {
    for (ts, kind, leaf) in [
        ("Exchange.placeOrder", "field-fn", "Exchange::place_order"),
        (
            "Exchange.static fetchHTTPResponse",
            "method",
            "Exchange::fetch_http_response",
        ),
        (
            "Exchange.#fetchBybitTime",
            "method",
            "Exchange::fetch_bybit_time",
        ),
        (
            "Exchange.static #fetchBybitTime",
            "method",
            "Exchange::fetch_bybit_time",
        ),
        (
            "Exchange.get #currentTime",
            "accessor",
            "Exchange::current_time",
        ),
        (
            "Exchange.set #currentTime",
            "accessor",
            "Exchange::current_time",
        ),
        (
            "VirtualClock#notifyTimeSync",
            "method",
            "VirtualClock::notify_time_sync",
        ),
        (
            "Exchange.get currentTime",
            "accessor",
            "Exchange::current_time",
        ),
        (
            "Exchange.set currentTime",
            "accessor",
            "Exchange::current_time",
        ),
        ("Exchange.constructor", "constructor", "Exchange::new"),
        ("HTTPResponse", "type", "HTTPResponse"),
        ("Exchange", "class", "Exchange"),
    ] {
        let rs = format!("sample/src/lib.rs::{leaf}");
        let rust = RustIndex {
            all: [rs.clone()].into(),
            public: [rs.clone()].into(),
        };
        let symbol = Symbol {
            ts: ts.into(),
            kind: kind.into(),
            ..row("ported", &rs, "adapted", "")
        };
        assert!(
            check(&one(symbol), &rust, &Exemptions::default()).is_empty(),
            "{ts}"
        );
    }
}

#[test]
fn split_reports_filter_items_per_crate_and_pending_cannot_discharge_invention() {
    let corpus: Vec<Manifest> = vec![
        toml::from_str(
            r#"
        [file]
        ts = "packages/safe-cex/src/provider.ts"
rust_files = ["first/src/lib.rs", "second/src/lib.rs"]
        [[symbol]]
        ts = "Provider"
        kind = "type"
        rs = ["first/src/lib.rs::Provider", "second/src/lib.rs::Options"]
        layer = "adapted"
        status = "ported"
        note = "Split: first holds the provider trait; `second/src/lib.rs::Options` holds its options"
    "#,
        )
        .unwrap(),
    ];
    let rust = RustIndex {
        all: [
            "first/src/lib.rs::Provider".into(),
            "second/src/lib.rs::Options".into(),
        ]
        .into(),
        public: [
            "first/src/lib.rs::Provider".into(),
            "second/src/lib.rs::Options".into(),
        ]
        .into(),
    };
    let report = safe_cex_parity::report(&corpus, &rust, &Exemptions::default()).unwrap();
    for expected in [
        "crate first\t1 / 1\t1\t0\t0",
        "crate second\t1 / 1\t1\t0\t0",
        "total\t1 / 1\t2\t0\t0",
    ] {
        assert!(report.contains(expected), "{report}");
    }
    assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
    let mut pending = corpus;
    pending[0].symbol[0].status = "pending".into();
    let errors = check(&pending, &rust, &Exemptions::default());
    assert_eq!(
        errors
            .iter()
            .filter(|e| e.starts_with("invention:"))
            .count(),
        2,
        "{errors:?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("pending requires empty rs"))
    );
}

#[test]
fn qualified_names_reject_same_leaf_on_wrong_class_and_allow_noted_rename() {
    let root = fixture().parent().unwrap().join("qualified-names");
    for name in ["valid", "noted-rename"] {
        let output = cli(&root.join("sdk-rs"), &root.join(name));
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = cli(&root.join("sdk-rs"), &root.join("wrong-class"));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "same leaf on wrong class unexpectedly passed"
    );
    for (expected, actual) in [
        ("BybitExchange::place_order", "OkxExchange::place_order"),
        ("OkxExchange::place_order", "BybitExchange::place_order"),
    ] {
        assert!(
            stderr.lines().any(
                |line| line.contains(&format!("Rust item path must be {expected}"))
                    && line.contains(&format!("sample/src/lib.rs::{actual}"))
            ),
            "{stderr}"
        );
    }
}

#[test]
fn report_cli_refuses_ported_counts_for_unresolved_mappings() {
    let root = fixture()
        .parent()
        .unwrap()
        .join("report-unresolved/manifest");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_parity-report"))
        .arg("--workspace")
        .arg(fixture().join("sdk-rs"))
        .arg("--manifest")
        .arg(root)
        .arg("--exempt")
        .arg(fixture().join("exempt.toml"))
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "unresolved mapping still printed progress: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stdout.is_empty(),
        "invalid mappings must not produce progress counts"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(
            "OrderSide (enum): rs does not resolve: sample/src/lib.rs::THIS_DOES_NOT_EXIST"
        ),
        "{stderr}"
    );
}

#[test]
fn shared_claim_explanations_do_not_authorize_unrelated_names() {
    let corpus = collapsed_rows("Shared representation", "Shared representation");
    let mut corpus = corpus;
    corpus[1].symbol[0].ts = "DifferentSide".into();
    let errors = check(&corpus, &index(), &Exemptions::default());
    assert!(
        errors.iter().any(|e| e.contains("other.ts::DifferentSide")
            && e.contains("Rust item path must be DifferentSide")),
        "{errors:?}"
    );
}

#[test]
fn divergent_renames_require_the_exact_target_in_the_note() {
    for note in [
        "x",
        "This explains a divergence",
        "OrderSide",
        "sample/src/lib.rs::OrderSideExtra",
    ] {
        let mut symbol = row("divergent", "sample/src/lib.rs::OrderSide", "adapted", note);
        symbol.ts = "OtherSide".into();
        let errors = check(&one(symbol), &index(), &Exemptions::default());
        assert!(
            errors.iter().any(|e| e.contains("OtherSide (enum)")
                && e.contains("Rust item path must be OtherSide")),
            "{note}: {errors:?}"
        );
    }
    let mut symbol = row(
        "divergent",
        "sample/src/lib.rs::OrderSide",
        "adapted",
        "Renamed to `sample/src/lib.rs::OrderSide`; represents both sides.",
    );
    symbol.ts = "OtherSide".into();
    assert!(check(&one(symbol), &index(), &Exemptions::default()).is_empty());
}

#[test]
fn every_array_target_requires_its_own_correspondence_or_named_escape() {
    let source = r#"
        [file]
        ts = "packages/safe-cex/src/types.ts"
rust_files = ["sample/src/lib.rs"]
        [[symbol]]
        ts = "OrderSide"
        kind = "enum"
        rs = ["sample/src/lib.rs::OrderSide", "sample/src/lib.rs::OtherSide"]
        layer = "adapted"
        status = "ported"
        note = "Split: shared options and representation"
    "#;
    let rust = RustIndex {
        all: [
            "sample/src/lib.rs::OrderSide".into(),
            "sample/src/lib.rs::OtherSide".into(),
        ]
        .into(),
        public: [
            "sample/src/lib.rs::OrderSide".into(),
            "sample/src/lib.rs::OtherSide".into(),
        ]
        .into(),
    };
    let mut corpus: Vec<Manifest> = vec![toml::from_str(source).unwrap()];
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("types.ts::OrderSide (enum)")
                && e.contains("sample/src/lib.rs::OtherSide")
                && e.contains("Rust item path must be OrderSide")),
        "{errors:?}"
    );
    corpus[0].symbol[0].note =
        "Split: additional representation is `sample/src/lib.rs::OtherSide`.".into();
    assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
    corpus[0].symbol[0].rs =
        toml::from_str::<Manifest>(&source.replace("\"sample/src/lib.rs::OrderSide\", ", ""))
            .unwrap()
            .symbol
            .remove(0)
            .rs;
    corpus[0].symbol[0].note = "x".into();
    assert!(
        check(&corpus, &rust, &Exemptions::default())
            .iter()
            .any(|e| e.contains("Rust item path must be OrderSide"))
    );
}

#[test]
fn source_provenance_rejects_same_names_in_other_crates_and_files_despite_named_notes() {
    let root = fixture().parent().unwrap().join("source-provenance");
    let invoke = |manifest: &str| {
        std::process::Command::new(env!("CARGO_BIN_EXE_parity-check"))
            .arg("--workspace")
            .arg(root.join("sdk-rs"))
            .arg("--manifest")
            .arg(root.join(manifest))
            .arg("--exempt")
            .arg(root.join("exempt.toml"))
            .output()
            .unwrap()
    };
    let valid = invoke("valid");
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    for (manifest, source, actual) in [
        (
            "wrong-crate",
            "packages/safe-cex/src/exchanges/bybit/bybit.api.ts::createAPI",
            "safe-cex-venue-okx/src/api.rs::create_api",
        ),
        (
            "wrong-file",
            "packages/safe-cex/src/exchanges/binance-usdm/ws-private.ts::BinancePrivateWebsocket.onMessage",
            "safe-cex-venue-binance/src/spot.rs::BinancePrivateWebsocket::on_message",
        ),
    ] {
        let output = invoke(manifest);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{manifest} unexpectedly passed");
        assert!(
            stderr.lines().any(|line| line.contains(source)
                && line.contains(actual)
                && line.contains("Rust source file is not declared")),
            "{stderr}"
        );
    }
}

#[test]
fn missing_source_declarations_and_malformed_paths_fail_closed() {
    let rust = index();
    let mut corpus = one(row(
        "ported",
        "sample/src/lib.rs::OrderSide",
        "mirror",
        "Declared rename target `sample/src/lib.rs::OrderSide`.",
    ));
    corpus[0].file.rust_files.clear();
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert!(
        errors
            .iter()
            .any(|e| e.contains("types.ts::OrderSide")
                && e.contains("Rust source file is not declared")),
        "{errors:?}"
    );
    for file in [
        "sample/src/*.rs",
        "sample/src/../lib.rs",
        "sample/src/lib.rs::OrderSide",
    ] {
        corpus[0].file.rust_files = vec![file.into()];
        let errors = check(&corpus, &rust, &Exemptions::default());
        assert!(
            errors.iter().any(|error| error.contains("types.ts")
                && error.contains("invalid or duplicate Rust source file")),
            "{errors:?}"
        );
    }
}

#[test]
fn one_character_split_note_cannot_discharge_ten_unrelated_items() {
    let targets: Vec<_> = (0..10)
        .map(|n| {
            format!(
                "{}/src/lib.rs::Item{n}",
                if n < 5 { "first" } else { "second" }
            )
        })
        .collect();
    let corpus = vec![
        toml::from_str::<Manifest>(&format!(
            r#"
        [file]
        ts = "packages/safe-cex/src/provider.ts"
rust_files = ["first/src/lib.rs", "second/src/lib.rs"]
        [[symbol]]
        ts = "Provider"
        kind = "type"
        layer = "adapted"
        rs = {targets:?}
        status = "ported"
        note = "x"
    "#
        ))
        .unwrap(),
    ];
    let rust = RustIndex {
        all: targets.iter().cloned().collect(),
        public: targets.iter().cloned().collect(),
    };
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert_eq!(errors.len(), 10, "{errors:?}");
    for target in targets {
        assert!(
            errors
                .iter()
                .any(|error| error.contains("provider.ts::Provider (type)")
                    && error.contains("Rust item path must be Provider")
                    && error.contains(&target)),
            "{errors:?}"
        );
    }
}

#[test]
fn source_provenance_checks_every_array_member_in_both_mapped_statuses() {
    let source = r#"
        [file]
        ts = "packages/safe-cex/src/types.ts"
rust_files = ["sample/src/lib.rs"]
        [[symbol]]
        ts = "OrderSide"
        kind = "enum"
        rs = ["sample/src/lib.rs::OrderSide", "other/src/lib.rs::OrderSide"]
        layer = "adapted"
        status = "ported"
        note = "Split: additional representation is `other/src/lib.rs::OrderSide`."
    "#;
    let mut corpus = vec![toml::from_str::<Manifest>(source).unwrap()];
    let mut rust = index();
    rust.all.insert("other/src/lib.rs::OrderSide".into());
    rust.public.insert("other/src/lib.rs::OrderSide".into());
    for status in ["ported", "divergent"] {
        corpus[0].symbol[0].status = status.into();
        let errors = check(&corpus, &rust, &Exemptions::default());
        assert_eq!(errors.len(), 1, "{status}: {errors:?}");
        assert!(
            errors[0].contains("types.ts::OrderSide")
                && errors[0].contains("other/src/lib.rs::OrderSide")
                && errors[0].contains("Rust source file is not declared"),
            "{errors:?}"
        );
    }
    corpus[0].file.rust_files.push("other/src/lib.rs".into());
    assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
}

#[test]
fn array_rename_tokens_do_not_supply_a_split_explanation() {
    let mut corpus = one(row("ported", "", "adapted", ""));
    corpus[0].file.rust_files = vec!["sample/src/lib.rs".into()];
    corpus[0].symbol[0].ts = "CombinedSide".into();
    let targets = vec![
        "sample/src/lib.rs::OrderSide".to_string(),
        "sample/src/lib.rs::OtherSide".to_string(),
    ];
    corpus[0].symbol[0].rs = safe_cex_parity::RustMapping::Split(targets.clone());
    let rust = RustIndex {
        all: targets.iter().cloned().collect(),
        public: targets.iter().cloned().collect(),
    };
    for status in ["ported", "divergent"] {
        corpus[0].symbol[0].status = status.into();
        for note in [
            targets.join(" "),
            format!("`{}`; `{}`.", targets[0], targets[1]),
            format!(
                "{} {} [{},{}]",
                targets[0], targets[1], targets[0], targets[1]
            ),
        ] {
            corpus[0].symbol[0].note = note;
            let errors = check(&corpus, &rust, &Exemptions::default());
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert!(
                errors[0].contains("types.ts::CombinedSide (enum)")
                    && errors[0].contains("explaining the split beyond Rust target tokens"),
                "{errors:?}"
            );
        }
        corpus[0].symbol[0].note = format!(
            "Split: separate order and other-side representations. {}",
            targets.join(" ")
        );
        assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
    }
}

#[test]
fn corresponding_array_targets_alone_do_not_explain_a_split() {
    let targets = [
        "sample/src/lib.rs::OrderSide",
        "sample/src/main.rs::OrderSide",
    ];
    let mut corpus = one(row(
        "ported",
        "",
        "adapted",
        &format!("[{},{}]", targets[0], targets[1]),
    ));
    corpus[0].file.rust_files = vec!["sample/src/lib.rs".into(), "sample/src/main.rs".into()];
    corpus[0].symbol[0].rs =
        safe_cex_parity::RustMapping::Split(targets.iter().map(|x| x.to_string()).collect());
    let rust = RustIndex {
        all: targets.iter().map(|x| x.to_string()).collect(),
        public: targets.iter().map(|x| x.to_string()).collect(),
    };
    let errors = check(&corpus, &rust, &Exemptions::default());
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("array rs requires note explaining the split"),
        "{errors:?}"
    );
}

#[test]
fn superset_provenance_exposes_unused_files_through_check_and_report() {
    let root = fixture().parent().unwrap().join("source-provenance");
    let scratch = Workspace::new("");
    let files = [
        "safe-cex-venue-bybit/src/api.rs",
        "safe-cex-venue-okx/src/api.rs",
        "safe-cex-venue-binance/src/usdm.rs",
        "safe-cex-venue-binance/src/spot.rs",
    ];
    for variant in ["wrong-crate", "wrong-file"] {
        let dir = scratch.0.join(variant);
        fs::create_dir(&dir).unwrap();
        for entry in fs::read_dir(root.join(variant)).unwrap() {
            let path = entry.unwrap().path();
            let mut doc: toml::Value = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            doc["file"]["rust_files"] = toml::Value::Array(
                files
                    .iter()
                    .map(|x| toml::Value::String(x.to_string()))
                    .collect(),
            );
            for row in doc["symbol"].as_array_mut().unwrap() {
                row["note"] = toml::Value::String(String::new());
            }
            fs::write(
                dir.join(path.file_name().unwrap()),
                toml::to_string(&doc).unwrap(),
            )
            .unwrap();
        }
        let corpus = manifests(&dir).unwrap();
        let rust = rust_index(&root.join("sdk-rs")).unwrap();
        let errors = check(&corpus, &rust, &Exemptions::default());
        assert_eq!(errors.len(), 12, "{variant}: {errors:?}");
        for manifest in &corpus {
            let used = match &manifest.symbol[0].rs {
                safe_cex_parity::RustMapping::Single(rs) => rs.split_once("::").unwrap().0,
                _ => panic!("single fixture"),
            };
            for file in files.iter().filter(|file| **file != used) {
                assert!(
                    errors.iter().any(|e| e.contains(&manifest.file.ts)
                        && e.contains(file)
                        && e.contains(
                            "declared Rust source file has no ported or divergent row mapping"
                        )),
                    "{errors:?}"
                );
            }
        }
        for binary in [
            env!("CARGO_BIN_EXE_parity-check"),
            env!("CARGO_BIN_EXE_parity-report"),
        ] {
            let output = std::process::Command::new(binary)
                .arg("--workspace")
                .arg(root.join("sdk-rs"))
                .arg("--manifest")
                .arg(&dir)
                .arg("--exempt")
                .arg(root.join("exempt.toml"))
                .output()
                .unwrap();
            assert!(!output.status.success(), "{variant} passed {binary}");
            assert!(output.stdout.is_empty(), "failed report emitted counts");
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("declared Rust source file has no ported or divergent row mapping")
            );
        }
    }
}

#[test]
fn divergent_requires_a_nonempty_string_or_array_mapping() {
    for mapping in [
        safe_cex_parity::RustMapping::Single(String::new()),
        safe_cex_parity::RustMapping::Split(vec![]),
    ] {
        let mut corpus = one(row("divergent", "", "", "Deliberate difference"));
        corpus[0].symbol[0].rs = mapping;
        let errors = check(&corpus, &RustIndex::default(), &Exemptions::default());
        assert_eq!(
            errors,
            ["status: packages/safe-cex/src/types.ts::OrderSide (enum): divergent requires rs"]
        );
    }
}

#[test]
fn duplicate_array_members_fail_naming_the_row_and_target() {
    for status in ["ported", "divergent"] {
        let mut corpus = one(row(
            status,
            "sample/src/lib.rs::OrderSide",
            "adapted",
            "Split representation",
        ));
        corpus[0].symbol[0].rs = safe_cex_parity::RustMapping::Split(vec![
            "sample/src/lib.rs::OrderSide".into(),
            "sample/src/lib.rs::OrderSide".into(),
        ]);
        let errors = check(&corpus, &index(), &Exemptions::default());
        assert_eq!(
            errors,
            [
                "status: packages/safe-cex/src/types.ts::OrderSide (enum): duplicate rs array member: sample/src/lib.rs::OrderSide"
            ]
        );
        corpus[0].symbol[0].rs = "sample/src/lib.rs::OrderSide".into();
        assert!(check(&corpus, &index(), &Exemptions::default()).is_empty());
    }
}

#[test]
fn numeric_boundaries_preserve_snake_case_and_type_spelling() {
    for (ts, kind, expected, wrong) in [
        (
            "Exchange.getPendingHip3Order",
            "method",
            "Exchange::get_pending_hip3_order",
            "Exchange::get_pending_hip3order",
        ),
        ("L2Book", "function", "l2_book", "l2book"),
        ("L2Book", "type", "L2Book", "l2_book"),
    ] {
        for (path, accepted) in [(expected, true), (wrong, false)] {
            let target = format!("sample/src/lib.rs::{path}");
            let rust = RustIndex {
                all: [target.clone()].into(),
                public: [target.clone()].into(),
            };
            let mut corpus = one(row("ported", &target, "mirror", ""));
            corpus[0].symbol[0].ts = ts.into();
            corpus[0].symbol[0].kind = kind.into();
            let errors = check(&corpus, &rust, &Exemptions::default());
            if accepted {
                assert!(errors.is_empty(), "{ts}: {errors:?}");
            } else {
                assert_eq!(errors.len(), 1, "{errors:?}");
                assert!(
                    errors[0].contains(ts)
                        && errors[0].contains(expected)
                        && errors[0].contains(&target),
                    "{errors:?}"
                );
            }
        }
    }
}

#[test]
fn exported_value_constants_resolve_and_keep_declaration_spelling() {
    let workspace = Workspace::new("pub const ALL_EXCHANGES: &str = \"*\";");
    let dir = workspace.0.join("manifest");
    fs::create_dir(&dir).unwrap();
    let manifest = "[file]\nts = \"packages/safe-cex/src/types.ts\"\nrust_files = [\"sample/src/lib.rs\"]\n[[symbol]]\nts = \"ALL_EXCHANGES\"\nkind = \"const\"\nlayer = \"mirror\"\nrs = \"sample/src/lib.rs::ALL_EXCHANGES\"\nstatus = \"ported\"\nnote = \"\"\n";
    fs::write(dir.join("types.ts.toml"), manifest).unwrap();
    let output = cli(&workspace.0, &dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(
        dir.join("types.ts.toml"),
        manifest.replacen("ts = \"ALL_EXCHANGES\"", "ts = \"OTHER_EXCHANGES\"", 1),
    )
    .unwrap();
    let output = cli(&workspace.0, &dir);
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        error.contains("types.ts::OTHER_EXCHANGES (const)")
            && error.contains("Rust item path must be OTHER_EXCHANGES")
            && error.contains("sample/src/lib.rs::ALL_EXCHANGES"),
        "{error}"
    );
}

#[test]
fn progress_agrees_with_validated_report_and_allocates_each_row_once() {
    let workspace = Workspace::new(
        "pub enum OrderSide { Buy } pub enum SideWire { Bid } pub enum OrderType { Limit } pub struct SecondOrderType;",
    );
    let corpus = manifests(&fixture().parent().unwrap().join("progress/manifest")).unwrap();
    let rust = rust_index(&workspace.0).unwrap();
    let report = safe_cex_parity::report(&corpus, &rust, &Exemptions::default()).unwrap();
    assert!(report.contains("total\t1 / 10\t4\t2\t3"), "{report}");
    let progress = safe_cex_parity::progress::from_report(&report).unwrap();
    assert!(
        progress.contains("4\tpackages/safe-cex/src/types.ts"),
        "{progress}"
    );
    let fraction = report
        .lines()
        .find_map(|line| line.strip_prefix("total\t"))
        .unwrap()
        .split('\t')
        .next()
        .unwrap();
    assert!(
        progress
            .lines()
            .any(|line| line == format!("Overall\t{fraction}")),
        "{progress}"
    );
    let phase_total: usize = report
        .lines()
        .filter(|line| line.starts_with("phase "))
        .map(|line| {
            line.split('\t')
                .nth(1)
                .unwrap()
                .split(" / ")
                .nth(1)
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .sum();
    assert_eq!(phase_total, 10);
    assert!(progress.contains("P1 pure core\t1 / 10"));
    assert!(progress.contains("P0 foundations\t0 / 0"));
    assert!(progress.contains("not phase completion"));
    let pending = baseline();
    let report =
        safe_cex_parity::report(&pending, &RustIndex::default(), &Exemptions::default()).unwrap();
    assert!(
        safe_cex_parity::progress::from_report(&report)
            .unwrap()
            .contains("3\tpackages/safe-cex/src/types.ts")
    );
}

#[test]
fn progress_cli_refuses_unvalidated_claims_and_matches_report_total() {
    let workspace = Workspace::new(
        "pub enum OrderSide { Buy } pub enum SideWire { Bid } pub enum OrderType { Limit } pub struct SecondOrderType;",
    );
    let invoke = |binary| {
        std::process::Command::new(binary)
            .arg("--workspace")
            .arg(&workspace.0)
            .arg("--manifest")
            .arg(fixture().parent().unwrap().join("progress/manifest"))
            .arg("--exempt")
            .arg(fixture().join("exempt.toml"))
            .output()
            .unwrap()
    };
    let report = invoke(env!("CARGO_BIN_EXE_parity-report"));
    let progress = invoke(env!("CARGO_BIN_EXE_parity-progress"));
    assert!(report.status.success() && progress.status.success());
    let report = String::from_utf8(report.stdout).unwrap();
    let progress = String::from_utf8(progress.stdout).unwrap();
    assert!(report.contains("total\t1 / 10\t4\t2\t3"), "{report}");
    assert!(
        progress.contains("4\tpackages/safe-cex/src/types.ts"),
        "{progress}"
    );
    let total = report
        .lines()
        .find(|line| line.starts_with("total\t"))
        .unwrap()
        .split('\t')
        .nth(1)
        .unwrap();
    assert!(
        progress
            .lines()
            .any(|line| line == format!("Overall\t{total}")),
        "{progress}"
    );
    fs::write(
        workspace.0.join("sample/src/lib.rs"),
        "pub enum OtherSide { Buy }",
    )
    .unwrap();
    let rejected = invoke(env!("CARGO_BIN_EXE_parity-progress"));
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("rs does not resolve"));
}

#[test]
fn mutable_binding_kinds_keep_value_and_callable_correspondence_gates() {
    for (kind, ts_name, rust_name) in [
        ("variable", "currentProxyToken", "currentProxyToken"),
        ("variable-fn", "currentHandler", "current_handler"),
    ] {
        let item = format!("sample/src/lib.rs::{rust_name}");
        let rust = RustIndex {
            all: [item.clone()].into(),
            public: [item.clone()].into(),
        };
        let mut symbol = row("ported", &item, "adapted", "");
        symbol.kind = kind.into();
        symbol.ts = ts_name.into();
        let mut corpus = one(symbol);
        assert!(check(&corpus, &rust, &Exemptions::default()).is_empty());
        let symbol = &mut corpus[0].symbol[0];
        symbol.ts = "wrongBinding".into();
        let errors = check(&corpus, &rust, &Exemptions::default());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("wrongBinding") && error.contains(&item)),
            "{errors:?}"
        );
        let mut pending = row("pending", "", "", "");
        pending.kind = kind.into();
        assert!(check(&one(pending), &RustIndex::default(), &Exemptions::default()).is_empty());
    }
}
