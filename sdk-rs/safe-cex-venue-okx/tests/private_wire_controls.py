"""Compile private-wire guard and literal-schema mutations, then restore bytes."""
from pathlib import Path
import os
import re
import subprocess

crate = Path(__file__).resolve().parents[1]
workspace = crate.parent
source = crate / "src/types.rs"
logs = workspace / "target/okx-private-wire-controls"


def without_string_admission(text, owner):
    pattern = re.compile(
        r"(impl<'de> Deserialize<'de> for " + re.escape(owner)
        + r" \{\n    fn deserialize[^\n]+\{\n)(.*?)(\n    \}\n\})",
        re.S,
    )
    matches = list(pattern.finditer(text))
    assert len(matches) == 1, f"{owner}: exact private decoder required"
    match = matches[0]
    body = match.group(2)
    assert body.count("String::deserialize(deserializer)?") == 1
    assert body.count(f"{owner}Schema::deserialize(") == 1
    # Keep source line spans stable for production-probe replay as well.
    replacement = f"        {owner}Schema::deserialize(deserializer)" + "\n" * body.count("\n")
    return text[:match.start(2)] + replacement + text[match.end(2):]


def run(name, selection):
    result = subprocess.run(
        ["cargo", "test", "--locked", "-p", "safe-cex-venue-okx", "--lib",
         "--jobs=1", selection, "--", "--include-ignored"],
        cwd=workspace, env={**os.environ, "CARGO_BUILD_JOBS": "1"},
        capture_output=True, text=True,
    )
    output = result.stdout + result.stderr
    logs.mkdir(parents=True, exist_ok=True)
    (logs / f"{name}.log").write_text(output)
    assert "Finished `test`" in output, output
    assert not re.search(r"error\[E\d+\]|could not compile", output), output
    return result.returncode, output


def main():
    original = source.read_bytes()
    for owner, selection in [
        ("RegionWire", "private_wire_region_rejects_tagged_maps"),
        ("InstrumentWire", "private_wire_instrument_rejects_tagged_maps"),
    ]:
        assert run(f"{owner}-baseline", selection)[0] == 0
        try:
            source.write_text(without_string_admission(original.decode(), owner))
            code, output = run(f"{owner}-guard-removed", selection)
            assert code == 101 and f"{owner} tagged owned shape" in output, output
            print(f"{owner}: compiled guard removal failed the named tagged-map assertion")
        finally:
            source.write_bytes(original)
        assert run(f"{owner}-restored", selection)[0] == 0

    selection = "derived_accepted_sets_match_typescript"
    assert run("schema-baseline", selection)[0] == 0
    before = 'enum RegionWireSchema {\n    #[serde(rename = "okx")]'
    after = 'enum RegionWireSchema {\n    #[serde(rename = "okx", alias = "OKX_ALIAS")]'
    assert original.decode().count(before) == 1
    try:
        source.write_text(original.decode().replace(before, after))
        code, output = run("schema-extra-alias", selection)
        assert code == 101 and selection in output and "OKX_ALIAS" in output, output
        print("RegionWireSchema: compiled extra alias failed the complete source literal inventory")
    finally:
        source.write_bytes(original)
    assert run("schema-restored", selection)[0] == 0
    assert source.read_bytes() == original
    print(f"All exact source restorations passed; logs: {logs}")


if __name__ == "__main__":
    main()
