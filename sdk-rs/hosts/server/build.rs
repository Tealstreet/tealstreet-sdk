#[path = "catalog_build.rs"]
mod catalog;

fn main() {
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    // Revalidate referenced proof files when their source changes or disappears.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=src/delegated/cosigned-proof-catalog");
    println!(
        "cargo:rerun-if-changed=../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    );
    let source = catalog::generate(&root).expect("invalid COSIGNED proof catalog");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("cosigned_catalog.rs"), source).unwrap();
}
