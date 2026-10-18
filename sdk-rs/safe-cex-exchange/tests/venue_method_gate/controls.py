#!/usr/bin/env python3
"""Compile real production registration faults; require RED and restored GREEN."""
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
CARGO = ["cargo", "test", "--manifest-path", str(ROOT / "sdk-rs/Cargo.toml"),
         "-p", "safe-cex-exchange", "--features", "native", "--test", "venue_binding_gate"]


def run(test, log):
    with log.open("w") as output:
        result = subprocess.run(CARGO + [test, "--", "--exact"], cwd=ROOT,
                                stdout=output, stderr=subprocess.STDOUT)
    return result.returncode, log.read_text()


def control(label, relative, old, new, test, marker, logs):
    source = ROOT / relative
    saved = source.read_bytes()
    text = saved.decode()
    if text.count(old) != 1:
        raise AssertionError(f"{label}: production fault anchor must be unique")
    try:
        source.write_text(text.replace(old, new, 1))
        code, output = run(test, logs / f"{label}-red.log")
        if code != 101 or "Unbound inventory changed" not in output or marker not in output:
            raise AssertionError(f"{label}: expected named compiled RED, got {code}; see {logs}")
    finally:
        source.write_bytes(saved)
    if source.read_bytes() != saved:
        raise AssertionError(f"{label}: production source not restored")
    code, _ = run(test, logs / f"{label}-restored.log")
    if code:
        raise AssertionError(f"{label}: restored gate failed ({code}); see {logs}")
    print(f"{label}: compiled RED / exact restore / GREEN", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--log-dir", type=Path)
    args = parser.parse_args()
    logs = args.log_dir or Path(tempfile.mkdtemp(prefix="venue-binding-controls-"))
    logs.mkdir(parents=True, exist_ok=True)
    prefix = "sdk-rs/safe-cex-exchange/src/js_runtime/"
    control("missing-array-push", prefix + "builtins.rs",
            '        "push" => method(name,', '        "__gate_missing_push" => method(name,',
            "method_surface::venue_standard_method_names_exist_in_the_real_shared_runtime",
            '"push"', logs)
    control("missing-math-registration", prefix + "globals.rs",
            '        "Math",\n        Js::object(vec![',
            '        "__gate_missing_Math",\n        Js::object(vec![',
            "every_venue_reference_has_a_production_binding", "global Math.", logs)
    print(f"logs: {logs}", flush=True)


if __name__ == "__main__":
    main()
