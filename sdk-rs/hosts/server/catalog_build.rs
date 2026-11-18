use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path};

pub const DIRECTORY: &str = "src/delegated/cosigned-proof-catalog";
const SHARED_PROBE: &str = "src/delegated/probe_tests.rs";
const SHARED_SESSION: &str = "src/session/tests/cosigned_venues.rs";

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "lowercase", deny_unknown_fields)]
enum Record {
    Ready {
        #[serde(rename = "probeTests")]
        probe_tests: String,
        #[serde(rename = "sessionTests")]
        session_tests: String,
        #[serde(
            default,
            rename = "excludedShapes",
            deserialize_with = "deserialize_shape_exclusions"
        )]
        excluded_shapes: Option<Vec<String>>,
    },
    Excluded {
        reason: String,
    },
}

// An explicitly present exclusion must be a list; null cannot erase accounting.
fn deserialize_shape_exclusions<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer).map(Some)
}

pub fn generate(root: &Path) -> Result<String, String> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root.join(DIRECTORY)).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err("catalog entries must be regular JSON files".into());
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF8 entry")?;
        let text = fs::read_to_string(entry.path()).map_err(|e| e.to_string())?;
        files.push((name, text));
    }
    render(root, files)
}

fn proof_path(root: &Path, path: &str) -> Result<(), String> {
    let relative = Path::new(path);
    if !path.starts_with("src/")
        || relative.extension().is_none_or(|ext| ext != "rs")
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!("invalid proof source: {path}"));
    }
    let resolved = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("{path}: {e}"))?;
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    if !resolved.starts_with(canonical_root) || !resolved.is_file() {
        return Err(format!("proof source leaves the server crate: {path}"));
    }
    Ok(())
}

pub fn render(root: &Path, mut files: Vec<(String, String)>) -> Result<String, String> {
    let rules: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            root.join("../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let declared: BTreeSet<_> = rules["venues"]
        .as_object()
        .ok_or("missing venue declarations")?
        .iter()
        .filter(|(_, rule)| rule["remoteSigning"]["enabled"] == true)
        .map(|(name, _)| name.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    let mut ready = Vec::new();
    let mut probe = Vec::new();
    let mut session = Vec::new();
    let mut standalone_probe = Vec::new();
    let mut standalone_session = Vec::new();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (file, text) in &files {
        let name = file
            .strip_suffix(".json")
            .ok_or_else(|| format!("not a JSON record: {file}"))?;
        if !declared.contains(name) || !seen.insert(name) {
            return Err(format!("unknown or duplicate exact venue: {name}"));
        }
        let record: Record = serde_json::from_str(text).map_err(|e| format!("{file}: {e}"))?;
        match record {
            Record::Ready {
                probe_tests,
                session_tests,
                excluded_shapes,
            } => {
                match (name, excluded_shapes.as_deref()) {
                    ("asterdex", Some([shape])) if shape == "v3" => {}
                    (_, None) if name != "asterdex" => {}
                    _ => {
                        return Err(format!(
                            "{file}: invalid excludedShapes; only AsterDEX v3 is known"
                        ));
                    }
                }
                proof_path(root, &probe_tests)?;
                proof_path(root, &session_tests)?;
                ready.push(name);
                if probe_tests == SHARED_PROBE {
                    probe.push(name);
                } else {
                    standalone_probe.push(name);
                }
                if session_tests == SHARED_SESSION {
                    session.push(name);
                } else {
                    standalone_session.push(name);
                }
            }
            Record::Excluded { reason } => {
                if reason.trim().is_empty() {
                    return Err(format!("{file}: empty exclusion reason"));
                }
            }
        }
    }
    if seen != declared {
        return Err(format!(
            "unaccounted Hosted venues: {:?}",
            declared.difference(&seen).collect::<Vec<_>>()
        ));
    }
    let mut source = String::from("// Generated from server-only per-venue proof records.\n");
    for (name, values, test_only) in [
        ("PRIVATE_STARTUP_VENUES", ready, false),
        ("SHARED_PROBE_VENUES", probe, true),
        ("SHARED_SESSION_VENUES", session, true),
        ("STANDALONE_PROBE_VENUES", standalone_probe, true),
        ("STANDALONE_SESSION_VENUES", standalone_session, true),
    ] {
        if test_only {
            source.push_str("#[cfg(test)]\n");
        }
        source.push_str(&format!(
            "pub(crate) const {name}: &[&str] = &{values:?};\n"
        ));
    }
    Ok(source)
}
