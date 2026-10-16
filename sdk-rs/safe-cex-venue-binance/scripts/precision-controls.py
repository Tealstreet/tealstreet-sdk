#!/usr/bin/env python3
"""Serial, restoring controls for the eight-case main-precision supplement.

Run from this worktree. Logs and SHA-bound results go under sdk-rs/target;
survivors are recorded as findings and make this audit exit nonzero.
"""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
OUTPUT = ROOT / "sdk-rs/target/binance-precision-controls"
FAMILIES = ("pm", "spot", "futures")  # Resume the previously active PM control first.
SWAPS = (
    ("lot-size", "adjust((amount - rest) / lots, market.precision.amount)",
     "PRECISION-IDENTITY native quantity case 1 row 0"),
    ("main-price", None, "PRECISION-IDENTITY native main price case 0 row 0"),
    ("initial-amount", "adjust(opts.amount, market.precision.amount)",
     "PRECISION-IDENTITY native payload count case 0"),
    ("split-remainder", "adjust(amount % max, market.precision.amount)",
     "PRECISION-IDENTITY native payload count case 0"),
)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def run(label, args, expected=None):
    process = subprocess.run(args, cwd=ROOT, env=ENV, stdout=subprocess.PIPE,
                             stderr=subprocess.STDOUT, text=True, check=False)
    (OUTPUT / f"{label}.log").write_text(process.stdout)
    compiled = "Finished `test` profile" in process.stdout
    if expected is None:
        accepted = (process.returncode == 0 and compiled
                    and process.stdout.count("test result: ok. 2 passed; 0 failed; 0 ignored") == 3
                    and all(f"test {family}_main_precision_identity ... ok" in process.stdout
                            for family in FAMILIES))
    else:
        accepted = (process.returncode == 101 and compiled and expected in process.stdout
                    and "test result: FAILED. 0 passed; 1 failed; 0 ignored" in process.stdout)
    entry = {"label": label, "exit": process.returncode, "compiled": compiled,
             "expected_assertion": expected, "accepted": accepted,
             "log_sha256": digest(process.stdout.encode())}
    RESULTS["runs"].append(entry)
    save()
    print(json.dumps(entry), flush=True)
    if not accepted:
        print(process.stdout, flush=True)
        raise RuntimeError(f"FINDING: {label} did not reach its required result; see its call site and log")


def save():
    (OUTPUT / "results.json").write_text(json.dumps(RESULTS, indent=2) + "\n")


def command(family=None):
    args = ["cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml",
            "-p", "safe-cex-venue-binance", "--jobs", "2"]
    if family:
        args += ["--test", f"{family}_orders", f"{family}_main_precision_identity",
                 "--", "--ignored", "--exact"]
    else:
        for target in ("spot_orders", "futures_orders", "pm_orders"):
            args += ["--test", target]
        args += ["--", "--ignored"]
    return args


def mutate(label, path, original, changed, family, assertion):
    if path.read_bytes() != original:
        raise RuntimeError(f"Refusing to overwrite concurrent edits to {path}")
    try:
        path.write_bytes(changed)
        run(label, command(family), assertion)
    finally:
        path.write_bytes(original)
        if path.read_bytes() != original:
            raise RuntimeError(f"Restore failed: {path}")


if __name__ == "__main__":
    ENV = os.environ.copy()
    if not ENV.get("SAFE_CEX_TYPESCRIPT_PATH"):
        sys.exit("Set SAFE_CEX_TYPESCRIPT_PATH explicitly before running source controls")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    RESULTS = {"sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
               "typescript_path": ENV["SAFE_CEX_TYPESCRIPT_PATH"], "runs": [], "restored": {}}
    paths = [CRATE / folder / f"{family}{suffix}" for family in FAMILIES
             for folder, suffix in (("src", "_orders.rs"), ("scripts", "-orders.mjs"))]
    # Controls must cover the named commit, not inherited uncommitted code mutations.
    for path in paths + [CRATE / "tests" / f"{f}_orders.rs" for f in FAMILIES] + [CRATE / "tests/support/precision_identity.rs"]:
        relative = str(path.relative_to(ROOT))
        committed = subprocess.check_output(["git", "show", f"HEAD:{relative}"], cwd=ROOT)
        if committed != path.read_bytes():
            sys.exit(f"Uncommitted audit input: {relative}")
    originals = {path: path.read_bytes() for path in paths}
    try:
        run("baseline", command())
        for family in FAMILIES:
            path = CRATE / "src" / f"{family}_orders.rs"
            original = originals[path]
            for name, fragment, assertion in SWAPS:
                if fragment is None:
                    argument = {"spot": "p", "futures": "n", "pm": "price"}[family]
                    # Include the main-price closure: PM also rounds bracket prices.
                    fragment = f".map(|{argument}| adjust({argument}, market.precision.price))"
                old = fragment.encode()
                if original.count(old) != 1:
                    raise RuntimeError(f"Expected one mutation site: {family} {fragment}")
                new = old.replace(b"precision.amount", b"precision.price") if name != "main-price" else old.replace(b"precision.price", b"precision.amount")
                mutate(f"{family}-{name}", path, original, original.replace(old, new), family, assertion)
        for family in FAMILIES:
            path = CRATE / "scripts" / f"{family}-orders.mjs"
            original = originals[path]
            marker = b"process.stdout.write(JSON.stringify("
            if original.count(marker) != 1:
                raise RuntimeError(f"Expected one source output site: {family}")
            for name, body in (("same-length", "results[originalTrialCount].amount='3.7';"),
                               ("reversal", "results.splice(originalTrialCount,8,...results.slice(originalTrialCount).reverse());")):
                injection = f"if(process.argv.includes('--precision-identity')){{{body}}}\n".encode()
                mutate(f"{family}-source-{name}", path, original, original.replace(marker, injection + marker),
                       family, "PRECISION-IDENTITY source content case 0")
    finally:
        # Each mutation restores independently; no successful final gate is implied here.
        RESULTS["restored"] = {str(path.relative_to(ROOT)): path.read_bytes() == original
                               for path, original in originals.items()}
        save()
    run("final-restore", command())
