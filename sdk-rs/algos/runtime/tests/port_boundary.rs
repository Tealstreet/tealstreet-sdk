//! The algos reach the engine only through `AccountPort`: from safe_cex_engine
//! they may name the port's vocabulary (model rows, commands, fills, errors,
//! capabilities), never engine internals (Store, Engine, Cx, Venue, request
//! contexts, ingest...). That keeps a separately loaded algos module a build
//! and loader change. The host-side projection is the one adapter allowed
//! the Store.
use std::path::{Path, PathBuf};

/// Engine paths the algos may name, as their first segments.
const ALLOWED: &[&str] = &[
    "model",
    "engine::Command",
    "engine::CommandId",
    "engine::Fill",
    "engine::Query",
    "error",
    "command::decode_command",
    "Capabilities",
    "capabilities",
];

/// Host-side adapters that read the engine Store for the port.
const ADAPTERS: &[&str] = &["runtime/src/projection.rs"];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every `safe_cex_engine::...` path named in `text`, with grouped imports expanded.
fn engine_paths(text: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("safe_cex_engine::") {
        rest = &rest[at + "safe_cex_engine::".len()..];
        let head: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        let after = &rest[head.len()..];
        if head.is_empty() && after.starts_with('{')
            || head.ends_with("::") && after.starts_with('{')
        {
            let close = after.find('}').expect("closed group");
            for item in after[1..close].split(',') {
                let item = item.trim();
                if !item.is_empty() {
                    paths.push(format!("{head}{item}"));
                }
            }
        } else {
            paths.push(head.trim_end_matches("::").to_owned());
        }
    }
    paths
}

#[test]
fn the_algos_name_only_the_ports_engine_vocabulary() {
    let algos = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for crate_dir in ["runtime", "chaser", "twap", "exec"] {
        sources(&algos.join(crate_dir).join("src"), &mut files);
    }
    assert!(files.len() > 10, "found the algo sources");
    let mut violations = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(algos)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if ADAPTERS.contains(&rel.as_str()) {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for path in engine_paths(&text) {
            let allowed = ALLOWED
                .iter()
                .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}::")));
            if !allowed {
                violations.push(format!("{rel}: safe_cex_engine::{path}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "algos must reach the engine only through AccountPort:\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_scanner_sees_grouped_imports_and_internals() {
    let found = engine_paths(
        "use safe_cex_engine::{Store, model::Order};\nuse safe_cex_engine::engine::{Command, Fill};",
    );
    assert_eq!(
        found,
        ["Store", "model::Order", "engine::Command", "engine::Fill"]
    );
}
