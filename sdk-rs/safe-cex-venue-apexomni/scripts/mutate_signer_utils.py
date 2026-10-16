"""Seven row controls in one production build/corpus, restore and pass."""
from pathlib import Path
import os
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / "sdk-rs/safe-cex-venue-apexomni/src/signer_utils.rs"
original = source.read_text()
mutated = original
replacements=[("(now() / 1000.0).floor()","(now() / 1000.0).ceil()"),
              ("Err(message.to_owned())",'Err(message.to_owned() + "_MUTATED")')]
for kind in ["Array", "Bool", "Number", "String"]:
    before=f"matches!(value, JsJsonValue::{kind}(_))"
    replacements.append((before,"!"+before))
for before,after in replacements:
    assert mutated.count(before)==1, before
    mutated=mutated.replace(before,after)
start=mutated.index("// ts: assertSubAccountId")
assert mutated[start:].count("Ok(())")==1
mutated=mutated[:start]+mutated[start:].replace("Ok(())",'Err("MUTATED noop".into())')
labels=["timestamp native bits", "assert exact message"]+[f"{name} independent positive axis" for name in ["assertTypeArray", "assertTypeBoolean", "assertTypeNumber", "assertTypeString", "assertSubAccountId"]]
assert os.environ.get("SAFE_CEX_TYPESCRIPT_PATH")
cmd=["/opt/homebrew/opt/rustup/bin/cargo","test","--locked","--manifest-path",str(root/"sdk-rs/Cargo.toml"),"-p","safe-cex-venue-apexomni","--test","signer_utils","--jobs","1","signer_rows_match_actual_typescript","--","--test-threads=1","--nocapture"]
logs=root/"sdk-rs/target/apexomni-controls"
logs.mkdir(parents=True,exist_ok=True)
try:
    source.write_text(mutated)
    result=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
    output=result.stdout+result.stderr
    (logs/"signer-utils-row-mutations.log").write_text(output)
    assert result.returncode==101 and "test result: FAILED" in output,output
    for label in labels: assert label in output,output
    print("7 compiled mutations failed their independent named assertions",flush=True)
finally:
    source.write_text(original)
result=subprocess.run(cmd,cwd=root,capture_output=True,text=True)
(logs/"signer-utils-row-restored.log").write_text(result.stdout+result.stderr)
assert result.returncode==0,result.stdout+result.stderr
print("Restored 7 rows pass; 2 sequential Cargo invocations, 1 Node corpus each",flush=True)
