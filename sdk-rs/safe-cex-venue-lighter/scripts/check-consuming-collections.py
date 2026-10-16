#!/usr/bin/env python3
"""Prove consuming collection mutants, restore bytes, then rerun scoped baselines."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys


CRATE = Path(__file__).resolve().parents[1]
ROOT = CRATE.parents[1]
NUMERIC = ROOT / "sdk-rs/safe-cex-num/src/numeric_ingress.rs"
COMMON = CRATE / "src/sdk_common.rs"
ORDERS = CRATE / "src/sdk_orders.rs"
RESPONSES = CRATE / "tests/live_sdk_responses.rs"
COLLECTIONS = CRATE / "tests/live_sdk_collections.rs"
FIELDS = {
    "live_sdk_responses": [
        "MaintenanceWindow.affectedServices",
        "AccountApiKeys.apiKeys",
        "ExchangeInfo.markets",
        "ExchangeInfo.tokens",
        "ExchangeInfo.rateLimits",
        "FeeSchedule.makerTiers",
        "FeeSchedule.takerTiers",
        "SystemStatus.components",
        "SystemStatus.scheduledMaintenance",
    ],
    "live_sdk_collections": [
        "Account.balances",
        "OrderBook.bids",
        "OrderBook.asks",
        "PaginatedResponse.items",
        "BatchCancelOrdersParams.orderIds",
        "BatchCancelOrdersParams.clientOrderIds",
    ],
}
VECTOR_HELPER = """
fn consuming_collection_mutant<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where D: serde::Deserializer<'de>, T: serde::Deserialize<'de> {
    let consumed = Vec::<T>::deserialize(deserializer)?;
    drop(consumed);
    Ok(Vec::new())
}
"""
OPTION_HELPER = """
fn consuming_optional_collection_mutant<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where D: serde::Deserializer<'de>, T: serde::Deserialize<'de> {
    let consumed = Vec::<T>::deserialize(deserializer)?;
    drop(consumed);
    Ok(Some(Vec::new()))
}
"""


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def replace_once(text, old, new, label):
    require(text.count(old) == 1, f"Source drift: expected one {label}")
    return text.replace(old, new, 1)


def consuming_array(text):
    start = text.index("impl<'de> SeqAccess<'de> for Array")
    end = text.index("struct Object", start)
    original = text[start:end]
    changed = replace_once(
        original,
        """        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()""",
        """        let _ = seed;
        self.0.by_ref().for_each(drop);
        Ok(None)""",
        "Array::next_element_seed implementation",
    )
    return text[:start] + changed + text[end:]


def wire_field(text, struct, field, optional=False):
    start = text.index(f"struct {struct}")
    end = text.index("\n}", start)
    original = text[start:end]
    if optional:
        old = f'    #[serde(default, deserialize_with = "present")]\n    {field}:'
        new = (
            '    #[serde(default, deserialize_with = "consuming_optional_collection_mutant")]'
            f"\n    {field}:"
        )
    else:
        old = f"    {field}:"
        new = f'    #[serde(deserialize_with = "consuming_collection_mutant")]\n    {field}:'
    changed = replace_once(original, old, new, f"{struct}.{field}")
    return text[:start] + changed + text[end:]


def consuming_common(text):
    for struct, field in [
        ("AccountWire", "balances"),
        ("OrderBookWire", "bids"),
        ("OrderBookWire", "asks"),
        ("PaginatedResponseWire", "items"),
    ]:
        text = wire_field(text, struct, field)
    return text + VECTOR_HELPER


def consuming_orders(text):
    for field in ["order_ids", "client_order_ids"]:
        text = wire_field(text, "BatchCancelOrdersParamsWire", field, optional=True)
    return text + OPTION_HELPER


def remove_native_guards(text):
    if "if actual_len != 4" in text:
        length_guard = '''        if actual_len != 4 {
            failures.push(format!("{path} native length {actual_len} != 4"));
        }'''
        count_guard = '''    if compared != 24 {
        failures.push(format!("native comparisons {compared} != 24"));
    }'''
        replacement = "        let _ = actual_len;"
    else:
        length_guard = '''        if actual.len() != 2 {
            failures.push(format!("{path} native length {} != 2", actual.len()));
        }'''
        count_guard = '''    if compared != path_count * 2 {
        failures.push(format!(
            "native comparisons {compared} != {}",
            path_count * 2
        ));
    }'''
        replacement = ""
    text = replace_once(text, length_guard, replacement, "native length guard")
    text = replace_once(text, count_guard, "    let _ = compared;", "native comparison-count guard")
    require('"SDK-CARDINALITY {name}.{field} source length"' in text,
            "Source length guard must remain enabled")
    require('failures.push(format!("{path} native collection identity"))' in text,
            "Native identity assertion must remain enabled")
    return text


class Review:
    def __init__(self, logs):
        self.logs = logs
        self.head = git("rev-parse", "HEAD")
        self.saved = {p: p.read_bytes() for p in [NUMERIC, COMMON, ORDERS, RESPONSES, COLLECTIONS]}
        self.expected = dict(self.saved)
        for path in [NUMERIC, COMMON, ORDERS]:
            require(self.saved[path] == git("show", f"HEAD:{path.relative_to(ROOT)}"),
                    f"Refusing to mutate modified production file: {path}")
        self.env = os.environ.copy()
        require(not any(k.startswith("LIGHTER_") and "MUTATION" in k for k in self.env),
                "Remove inherited LIGHTER_*MUTATION variables before running controls")
        self.results = []

    def check(self):
        require(git("rev-parse", "HEAD") == self.head, "HEAD changed during mutation review")
        for path, data in self.expected.items():
            require(path.read_bytes() == data, f"Concurrent edit detected: {path}")

    def write(self, path, data):
        self.check()
        encoded = data.encode() if isinstance(data, str) else data
        self.expected[path] = encoded
        path.write_bytes(encoded)

    def restore(self):
        conflicts = []
        for path, original in self.saved.items():
            if path.read_bytes() != self.expected[path]:
                backup = self.logs / (path.name + ".original")
                backup.write_bytes(original)
                conflicts.append(f"{path}; original preserved at {backup}")
            else:
                path.write_bytes(original)
                self.expected[path] = original
        require(not conflicts, "Concurrent edits preserved; manual restoration required: " + "; ".join(conflicts))
        require(all(p.read_bytes() == b for p, b in self.saved.items()), "Byte restoration failed")

    def run(self, target, phase, mutant=False, identity_only=False):
        self.check()
        command = [
            "cargo", "test", "--manifest-path", str(ROOT / "sdk-rs/Cargo.toml"),
            "-p", "safe-cex-venue-lighter", "--locked", "--jobs", "1",
            "--test", target, "sdk_collection", "--", "--include-ignored",
            "--nocapture", "--test-threads=1",
        ]
        log = self.logs / f"{target}-{phase}.log"
        with log.open("wb") as output:
            child = subprocess.Popen(command, cwd=ROOT, env=self.env, stdout=output,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            try:
                child.wait()
            except BaseException:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass  # The owned process group already exited.
                child.wait()
                raise
        self.check()
        text = log.read_text(errors="replace")
        count = len(FIELDS[target])
        require("error[E" not in text and "could not compile" not in text,
                f"Control did not compile; inspect {log}")
        if mutant:
            require(child.returncode == 101, f"Expected runtime exit 101, got {child.returncode}: {log}")
            require(f"0 passed; {count} failed; 0 ignored" in text,
                    f"Expected all {count} selected fields to fail: {log}")
            for field in FIELDS[target]:
                rows = [line for line in text.splitlines() if line.startswith(f"SDK-CARDINALITY {field}:")]
                require(len(rows) == 1, f"Missing unique named assertion for {field}: {log}")
                paths = ["bare raw", "bare owned", "bare borrowed"]
                if target == "live_sdk_responses" or field == "PaginatedResponse.items":
                    paths += ["shared raw", "shared owned", "shared borrowed"]
                size = 4 if target == "live_sdk_responses" else 2
                for path in paths:
                    require(f"{path} native collection identity" in rows[0],
                            f"Missing identity failure for {field} {path}: {log}")
                    if not identity_only:
                        require(f"{path} native length 0 != {size}" in rows[0],
                                f"Missing zero-length failure for {field} {path}: {log}")
                if not identity_only:
                    require(f"native comparisons 0 != {len(paths) * size}" in rows[0],
                            f"Missing zero-comparison failure for {field}: {log}")
                require("must decode successfully" not in text and "trailing array elements" not in text,
                        f"Decode rejection is not consuming proof: {field}")
                if identity_only:
                    require("native length" not in rows[0] and "native comparisons" not in rows[0],
                            f"Native count guards remain active: {field}")
        else:
            require(child.returncode == 0 and f"{count} passed; 0 failed; 0 ignored" in text,
                    f"Restored/baseline gate failed: {log}")
        self.results.append({"target": target, "phase": phase, "exit": child.returncode, "log": str(log)})
        print(f"{target} {phase}: exit {child.returncode}; {log}", flush=True)

    def execute(self):
        for target in FIELDS:
            self.run(target, "baseline")
        try:
            self.write(NUMERIC, consuming_array(self.saved[NUMERIC].decode()))
            self.run("live_sdk_responses", "consuming", mutant=True)
            self.write(RESPONSES, remove_native_guards(self.saved[RESPONSES].decode()))
            self.run("live_sdk_responses", "identity-only", mutant=True, identity_only=True)
            self.restore()
            self.write(COMMON, consuming_common(self.saved[COMMON].decode()))
            self.write(ORDERS, consuming_orders(self.saved[ORDERS].decode()))
            self.run("live_sdk_collections", "consuming", mutant=True)
            self.write(COLLECTIONS, remove_native_guards(self.saved[COLLECTIONS].decode()))
            self.run("live_sdk_collections", "identity-only", mutant=True, identity_only=True)
        finally:
            self.restore()
        for target in FIELDS:
            self.run(target, "restored")
        (self.logs / "results.json").write_text(json.dumps({
            "head": self.head.decode().strip(),
            "restored_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(b).hexdigest() for p, b in self.saved.items()},
            "runs": self.results,
            "scope": "nine shared Array fields and six private Wire consuming-field decoders",
        }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exclusive-worktree", action="store_true",
                        help="confirm no other process will build or edit this workspace during controls")
    args = parser.parse_args()
    require(args.exclusive_worktree, "Requires --exclusive-worktree; this runner temporarily mutates build inputs")
    logs = ROOT / ".aimux/consuming-collections"
    logs.mkdir(parents=True, exist_ok=True)
    lock = ROOT / ".aimux/consuming-collections.lock"
    fd = os.open(lock, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    os.write(fd, f"pid={os.getpid()}\n".encode())
    os.close(fd)
    signal.signal(signal.SIGTERM, lambda _signum, _frame: sys.exit("Terminated; restoring owned mutations"))
    try:
        Review(logs).execute()
    finally:
        lock.unlink()


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, FileExistsError) as error:
        print(f"consuming collection audit refused/failed: {error}", file=sys.stderr)
        sys.exit(1)
