mod build_support;

fn main() {
    let source = "../../packages/safe-cex/src/exchanges/rust/construction-rules.json";
    println!("cargo:rerun-if-changed={source}");
    let json = std::fs::read_to_string(source).expect("read shared construction rules");
    let table = build_support::generate(&json).expect("valid shared construction rules");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    std::fs::write(output.join("construction_rules.rs"), table).expect("write construction rules");
}
