use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "parity-invention-scope-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for directory in ["sample/src", "shared/src", "manifest"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers=['sample','shared']\n",
        )
        .unwrap();
        fs::write(root.join("sample/Cargo.toml"), "[package]\nname='sample'\nversion='0.0.0'\n[dependencies]\nshared={path='../shared'}\n").unwrap();
        fs::write(
            root.join("shared/Cargo.toml"),
            "[package]\nname='shared'\nversion='0.0.0'\n",
        )
        .unwrap();
        fs::write(
            root.join("shared/src/lib.rs"),
            "pub struct External;\npub fn imported() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("sample/src/lib.rs"),
            "mod functions;\npub use functions::first;\npub use shared::External;\n",
        )
        .unwrap();
        fs::write(
            root.join("sample/src/functions.rs"),
            "pub fn first() {}\npub fn second() {}\n",
        )
        .unwrap();
        fs::write(root.join("manifest/sample.toml"), r#"
[file]
ts = "packages/safe-cex/src/sample.ts"
rust_files = ["sample/src/functions.rs", "shared/src/lib.rs"]

[[symbol]]
ts = "first"
kind = "function"
layer = "mirror"
rs = ["sample/src/functions.rs::first", "sample/src/functions.rs::second", "shared/src/lib.rs::imported"]
status = "ported"
note = "Split fixture calls sample/src/functions.rs::first, sample/src/functions.rs::second and shared/src/lib.rs::imported separately."
"#).unwrap();
        fs::write(root.join("exempt.toml"), "exempt=[]\n").unwrap();
        Self(root)
    }

    fn run(&self, scope: Option<&str>) -> (bool, String) {
        let mut command = Command::new(env!("CARGO_BIN_EXE_parity-check"));
        command
            .arg("--workspace")
            .arg(&self.0)
            .arg("--manifest")
            .arg(self.0.join("manifest"))
            .arg("--exempt")
            .arg(self.0.join("exempt.toml"));
        if let Some(scope) = scope {
            command.arg("--invention-crate").arg(scope);
        }
        let output = command.output().unwrap();
        (
            output.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn split_mapping_scope_retains_reexports_and_unmapped_and_provenance_guards() {
    let fixture = Fixture::new();
    let (passed, output) = fixture.run(Some("sample"));
    assert!(passed, "{output}");
    assert!(output.contains("1 rows, 2 public Rust items"), "{output}");

    let (passed, output) = fixture.run(None);
    assert!(!passed);
    assert!(
        output.contains("invention: shared/src/lib.rs::External"),
        "{output}"
    );

    let functions = fixture.0.join("sample/src/functions.rs");
    let original = fs::read_to_string(&functions).unwrap();
    fs::write(&functions, format!("{original}pub fn unmapped() {{}}\n")).unwrap();
    let (passed, output) = fixture.run(Some("sample"));
    assert!(!passed);
    assert!(
        output.contains("invention: sample/src/functions.rs::unmapped"),
        "{output}"
    );
    fs::write(&functions, original).unwrap();

    let manifest = fixture.0.join("manifest/sample.toml");
    let original = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        original.replace(
            "rust_files = [\"sample/src/functions.rs\", \"shared/src/lib.rs\"]",
            "rust_files = []",
        ),
    )
    .unwrap();
    let (passed, output) = fixture.run(Some("sample"));
    assert!(!passed);
    assert!(
        output.contains("Rust source file is not declared"),
        "{output}"
    );
    fs::write(&manifest, original).unwrap();
    let (passed, output) = fixture.run(Some("sample"));
    assert!(passed, "{output}");

    let (passed, output) = fixture.run(Some("unknown"));
    assert!(!passed);
    assert!(output.contains("no indexed package unknown"), "{output}");
}

#[test]
fn scoped_invention_requires_explicit_manifest_and_exemption_inputs() {
    let output = Command::new(env!("CARGO_BIN_EXE_parity-check"))
        .args(["--invention-crate", "sample"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("requires explicit --manifest and --exempt")
    );
}

#[test]
fn report_does_not_accept_a_partial_invention_scope() {
    let output = Command::new(env!("CARGO_BIN_EXE_parity-report"))
        .args(["--invention-crate", "sample"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("supported only by parity-check"));
}
