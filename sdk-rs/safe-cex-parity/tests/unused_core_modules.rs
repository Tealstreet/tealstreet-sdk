use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_SOURCE: AtomicUsize = AtomicUsize::new(0);

struct Sources(PathBuf);
impl Sources {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "accepted-core-modules-{}-{}-{}",
            std::process::id(),
            NEXT_SOURCE.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn scan(&self, sources: &[&str], origins: &str) -> String {
        let paths: Vec<_> = sources
            .iter()
            .enumerate()
            .map(|(i, source)| {
                let path = self.0.join(format!("source-{i}.rs"));
                fs::write(&path, source).unwrap();
                path
            })
            .collect();
        let output = Command::new(env!("CARGO_BIN_EXE_accepted-source"))
            .args(["--macro-origins-json", origins])
            .args(paths)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
        String::from_utf8(output.stdout).unwrap()
    }
}
impl Drop for Sources {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn module_only_core_spelling_keeps_local_decoder_findings() {
    let sources = Sources::new();
    let input = [
        "pub mod core { pub struct Error<V> { pub native: V } }",
        "pub(crate) mod r#core; fn decode() { #[derive(serde::Deserialize)] struct Wire { n: f64 } let core = 1; let _ = [core]; let _ = vec![core]; }",
    ];
    let found = sources.scan(&input, r#"["core","std","serde"]"#);
    assert!(found.contains("\tWire\t"), "{found}");
    assert!(!found.contains("UNSUPPORTED_"), "{found}");

    let unauthenticated = sources.scan(&input, r#"["std","serde"]"#);
    assert_eq!(
        unauthenticated
            .matches("UNSUPPORTED_MACRO_NAMESPACE_SHADOW")
            .count(),
        2,
        "actual core compiler origin is required: {unauthenticated}"
    );
}

#[test]
fn every_target_source_token_can_revoke_core_declaration_proof() {
    let sources = Sources::new();
    let declaration = "pub mod core {}";
    for reference in [
        "fn x() { core::assert!(true); }",
        "fn x() { ::core::assert!(true); }",
        "fn x() { r#core::assert!(true); }",
        "use core::assert;",
        "#[cfg(any())] fn hidden() { core::assert!(true); }",
        "macro_rules! later { () => { core::assert!(true) } }",
        "#[derive(core::Clone)] struct Hidden;",
        "use forged as core;",
        "extern crate forged as core;",
        "fn nested() { mod core {} }",
        "const X: () = { mod core {} };",
        "macro_rules! create { () => { mod core {} } }",
        "#[cfg_attr(any(), core::attribute)] struct Hidden;",
        "use core::{self as renamed, *};",
        "fn x() { unknown!(core); }",
        "fn x() { vec![core::identity]; }",
    ] {
        let found = sources.scan(&[declaration, reference], r#"["core"]"#);
        assert!(
            found
                .lines()
                .any(|line| line.contains("source-0.rs\tUNSUPPORTED_MACRO_NAMESPACE_SHADOW")),
            "cross-file reference must revoke the declaration proof: {reference}: {found}"
        );
    }
    let restored = sources.scan(&[declaration, "fn ordinary() {}"], r#"["core"]"#);
    assert!(restored.is_empty(), "exact source restoration: {restored}");
    let unauthenticated_vec = sources.scan(
        &[declaration, "fn x() { let core = 1; let _ = vec![core]; }"],
        r#"["core"]"#,
    );
    assert!(unauthenticated_vec.contains("UNSUPPORTED_MACRO_NAMESPACE_SHADOW"));
}

#[test]
fn aliases_externs_other_namespaces_and_opaque_expansion_stay_refused() {
    let sources = Sources::new();
    for declaration in [
        "use forged as core;",
        "extern crate forged as core;",
        "fn nested() { mod core {} }",
        "mod std {}",
        "mod serde {}",
        "mod safe_cex_num {}",
    ] {
        let found = sources.scan(&[declaration], r#"["core","std","serde"]"#);
        assert!(
            found.contains("UNSUPPORTED_MACRO_NAMESPACE_SHADOW"),
            "{declaration}: {found}"
        );
    }
    let opaque = sources.scan(&["mod core {} mystery!();"], r#"["core"]"#);
    assert!(
        opaque.contains("UNSUPPORTED_MODULE_MACRO_INVOCATION:mystery"),
        "{opaque}"
    );
}
