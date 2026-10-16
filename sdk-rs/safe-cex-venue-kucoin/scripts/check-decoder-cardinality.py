"""Compile consuming venue decoder mutations; restore all source bytes on exit."""
from pathlib import Path
import argparse
import subprocess

CRATE = Path(__file__).resolve().parents[1]
REPOSITORY = CRATE.parent.parent
CASES = {
    "server": ("records", "server_cardinality_supplement_retains_both_native_records", "spot_types.rs", "instance_servers"),
    "classic": ("account_mode", "subaccount_cardinality_supplement_retains_both_native_vectors", "account_mode.rs", "classic_sub_account"),
    "unified": ("account_mode", "subaccount_cardinality_supplement_retains_both_native_vectors", "account_mode.rs", "unified_sub_account"),
}


def remove_assertion(text, label):
    position = text.index(label)
    start = text.rfind("assert_eq!(", 0, position)
    assert start >= 0, label
    end = text.index(");", position) + 2
    return text[:start] + text[end:]


def without_length_guards(text, role):
    if role == "server":
        text = remove_assertion(text, "KUCOIN-CARDINALITY server fixture total")
        return remove_assertion(text, "KUCOIN-CARDINALITY server native retained rows")
    position = text.index("KUCOIN-CARDINALITY subaccount fixture total")
    start = text.rfind('    for field in [', 0, position)
    end = text.index("\n    }", position) + len("\n    }")
    assert start >= 0
    text = text[:start] + text[end:]
    for field in ["classic", "unified"]:
        text = remove_assertion(text, f"KUCOIN-CARDINALITY {field} native retained rows")
    return text


def mutate(text, role, field, variant):
    expression = "wire.instance_servers" if role == "server" else field
    if variant == "extra":
        value = f"{expression}.iter().chain({expression}.iter()).cloned().collect()"
    else:
        count = 0 if variant.endswith("empty") else 1
        value = f"{expression}.into_iter().take({count}).collect()"
    if role == "server":
        old = "instance_servers: wire.instance_servers,"
        new = f"instance_servers: {value},"
    else:
        old = """                    } => Ok(Self::Identified {
                        account_mode,
                        classic_sub_account,
                        unified_sub_account,
                    }),"""
        new = old.replace(f"                        {field},", f"                        {field}: {value},")
    assert text.count(old) == 1, "decoder mutation anchor changed"
    return text.replace(old, new)


def check(role):
    target, test_name, filename, field = CASES[role]
    production = CRATE / "src" / filename
    test = CRATE / "tests" / f"{target}.rs"
    originals = {path: path.read_bytes() for path in [production, test]}
    command = ["cargo", "test", "--locked", "--manifest-path", "sdk-rs/Cargo.toml",
               "-p", "safe-cex-venue-kucoin", "--jobs", "1", "--test", target,
               test_name, "--", "--exact", "--nocapture"]

    def run(expected_code, assertion=None):
        result = subprocess.run(command, cwd=REPOSITORY, capture_output=True, text=True)
        output = result.stdout + result.stderr
        if result.returncode != expected_code or (assertion and assertion not in output):
            raise RuntimeError(f"{role}: unexpected code/assertion\n{output}")

    run(0)
    try:
        for variant in ["empty", "partial", "extra", "guards-removed-empty", "guards-removed-partial"]:
            production.write_text(mutate(originals[production].decode(), role, field, variant))
            text = originals[test].decode()
            removed = variant.startswith("guards-removed")
            if removed:
                text = without_length_guards(text, role)
            test.write_text(text)
            suffix = "index coverage" if removed else "retained rows"
            assertion = f"KUCOIN-CARDINALITY {role} native {suffix}"
            run(101, assertion)
            print(f"{role} {variant}: compiled, 101 at {assertion}", flush=True)
    finally:
        for path, original in originals.items():
            path.write_bytes(original)
    run(0)
    print(f"{role}: restored baseline passes", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roles", nargs="*", choices=list(CASES))
    arguments = parser.parse_args()
    for selected in arguments.roles or CASES:
        check(selected)
