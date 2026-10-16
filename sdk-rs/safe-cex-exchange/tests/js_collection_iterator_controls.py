#!/usr/bin/env python3
"""Compiled collection faults, actual Hyro reset witness, exact restorations."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
BUILTINS = ROOT / "sdk-rs/safe-cex-exchange/src/js_runtime/builtins.rs"
COLLECTIONS = ROOT / "sdk-rs/safe-cex-exchange/src/js_runtime/collections.rs"
GLOBALS = ROOT / "sdk-rs/safe-cex-exchange/src/js_runtime/globals.rs"
ORIGINAL = {path: path.read_bytes() for path in (BUILTINS, COLLECTIONS, GLOBALS)}
LOGS = Path(tempfile.mkdtemp(prefix="js-collection-iterator-controls-"))
COMMAND = [os.environ.get("CARGO", "cargo"), "test", "--manifest-path", str(ROOT / "sdk-rs/Cargo.toml"), "--locked", "--jobs", "1"]


def run(label, crate, target, failures=()):
    result = subprocess.run(COMMAND + ["-p", crate, "--test", target], cwd=ROOT,
                            capture_output=True, timeout=600)
    output = (result.stdout + result.stderr).decode(errors="replace")
    log = LOGS / f"{label}-{target}.txt"
    log.write_text(output)
    assert "Finished `test`" in output and "could not compile" not in output, str(log)
    assert result.returncode == (101 if failures else 0), str(log)
    if failures:
        assert "panicked at" in output, str(log)
        for name in failures:
            assert f"test {name} ... FAILED" in output, str(log)
    else:
        assert f"{6 if target == 'js_collection_iterators' else 1} passed; 0 failed" in output, str(log)
    print(f"{label}/{target}: {'compiled runtime failure caught' if failures else 'pass'}", flush=True)


CORE = ("safe-cex-exchange", "js_collection_iterators")
HYRO = ("safe-cex-venue-bybit", "hyro_native_iterator")
TRACE = "collection_iterator_live_traces_match_node"
RESET = "hyro_reset_session_uses_shared_live_pending_ack_iterator"
FACTORY = b"Ok(super::collections::collection_iterator(receiver, kind))"
SNAPSHOT = b"Ok({ let _ = kind; Js::array(vec![]) })"
START = b"pub(crate) fn collection_iterator(collection: Js, kind: Kind) -> Js {\n"
COPY = b"""    let snapshot = match &collection {
        Js::Map(entries) => {
            let copy = Js::new_map();
            for (key, value) in entries.borrow().clone() {
                copy.call_method("set", vec![key, value]).unwrap();
            }
            copy
        }
        Js::Set(entries) => {
            let copy = Js::new_set();
            for value in entries.borrow().clone() {
                copy.call_method("add", vec![value]).unwrap();
            }
            copy
        }
        _ => unreachable!(),
    };
    let collection = snapshot;
"""
TERMINAL = b"""        if item.is_none() {
            done.set(true);
            collection.borrow_mut().take();
        }
"""


def fault(label):
    if label in ("map-snapshot-array", "set-snapshot-array"):
        original = ORIGINAL[BUILTINS]
        assert original.count(FACTORY) == 2
        index = original.index(FACTORY) if label == "map-snapshot-array" else original.rindex(FACTORY)
        BUILTINS.write_bytes(original[:index] + SNAPSHOT + original[index + len(FACTORY):])
    elif label == "frozen-collection":
        assert ORIGINAL[COLLECTIONS].count(START) == 1
        COLLECTIONS.write_bytes(ORIGINAL[COLLECTIONS].replace(START, START + COPY))
    elif label == "iterator-close":
        marker = b'iterator.get("return")'
        assert ORIGINAL[GLOBALS].count(marker) == 1
        GLOBALS.write_bytes(ORIGINAL[GLOBALS].replace(marker, b"Ok::<Js, Thrown>(Js::Undefined)"))
    else:
        assert label == "revive-after-done" and ORIGINAL[COLLECTIONS].count(TERMINAL) == 1
        COLLECTIONS.write_bytes(ORIGINAL[COLLECTIONS].replace(TERMINAL, b""))


try:
    run("baseline", *CORE)
    run("baseline", *HYRO)
    labels = ("map-snapshot-array", "set-snapshot-array", "frozen-collection", "revive-after-done", "iterator-close")
    if len(sys.argv) > 1:
        assert len(sys.argv) == 2 and sys.argv[1].startswith("--only=")
        labels = (sys.argv[1][len("--only="):],)
        assert labels[0] in ("map-snapshot-array", "set-snapshot-array", "frozen-collection", "revive-after-done", "iterator-close")
    for label in labels:
        fault(label)
        try:
            failures = ("array_from_closes_iterator_on_mapper_error_preserving_original_throw",) if label == "iterator-close" else (TRACE,)
            if label == "frozen-collection":
                failures += ("collection_iterators_feed_live_array_from_and_collection_constructors",)
            run(label, *CORE, failures=failures)
            run(label, *HYRO, failures=(RESET,) if label in ("map-snapshot-array", "frozen-collection") else ())
        finally:
            for path, content in ORIGINAL.items():
                path.write_bytes(content)
        run(f"{label}-restored", *CORE)
        run(f"{label}-restored", *HYRO)
finally:
    for path, content in ORIGINAL.items():
        path.write_bytes(content)
assert all(path.read_bytes() == content for path, content in ORIGINAL.items())
print(f"{len(labels)} compiled faults caught; exact restorations pass; logs: {LOGS}")
