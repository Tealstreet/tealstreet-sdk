#!/usr/bin/env python3
"""Prove the two reviewed Clone witnesses detect detached or reconstructed state."""
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CRATE = ROOT / "safe-cex-venue-hyperliquid"
ENV = {**os.environ, "RUSTUP_TOOLCHAIN": "1.99.0", "RUSTC_BOOTSTRAP": "1"}
COMMAND = ["cargo", "+1.99.0", "test", "--locked", "-p", "safe-cex-venue-hyperliquid",
           "--jobs", "1", "--lib"]
CONTROLS = [
    (
        CRATE / "src/helper_tasks.rs",
        "helper_tasks::accepted_clone::helper_observer_clone_shares_pending_completion_and_original_values",
        "#[derive(Clone)]\npub struct HelperObserver<V, E>",
        "pub struct HelperObserver<V, E>",
        "impl<V: Clone, E: Clone> Future for HelperObserver<V, E>",
        """impl<V: Clone, E: Clone> Clone for HelperObserver<V, E> {
    fn clone(&self) -> Self {
        Self { state: Rc::new(RefCell::new(State {
            result: self.state.borrow().result.clone(), wakers: Vec::new()
        })) }
    }
}
""",
    ),
    (
        CRATE / "src/signing.rs",
        "signing::accepted_clone::signing_payload_clone_retains_distinct_wire_and_encoding_snapshots",
        "#[derive(Debug, Clone, PartialEq)]\npub struct SigningPayload",
        "#[derive(Debug, PartialEq)]\npub struct SigningPayload",
        "pub trait SigningInput",
        """impl Clone for SigningPayload {
    fn clone(&self) -> Self { Self::from(self.wire.clone()) }
}
""",
    ),
]
ORIGINALS = {file: file.read_bytes() for file, *_ in CONTROLS}


def run(name="accepted_clone", *, failure=False):
    args = COMMAND + [name] + (["--", "--exact"] if name != "accepted_clone" else [])
    result = subprocess.run(args, cwd=ROOT, env=ENV, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    print(result.stdout, flush=True)
    if failure:
        assert result.returncode == 101, "control must compile and fail the named native test"
        assert "error[E" not in result.stdout, "compiler errors are not control evidence"
        assert re.search(rf"^test {re.escape(name)} \.\.\. FAILED$", result.stdout, re.M)
    else:
        assert result.returncode == 0, "restored native witnesses must pass"
        names = [row[1] for row in CONTROLS] if name == "accepted_clone" else [name]
        for expected in names:
            assert re.search(rf"^test {re.escape(expected)} \.\.\. ok$", result.stdout, re.M)


try:
    run()
    for file, name, old, new, anchor, implementation in CONTROLS:
        text = ORIGINALS[file].decode()
        assert text.count(old) == 1 and text.count(anchor) == 1, name
        print(f"CONTROL {name}", flush=True)
        file.write_text(text.replace(old, new).replace(anchor, implementation + anchor))
        try:
            run(name, failure=True)
        finally:
            file.write_bytes(ORIGINALS[file])
        run(name)
    run()
finally:
    for file, original in ORIGINALS.items():
        file.write_bytes(original)
        assert file.read_bytes() == original

print("2 compiled named Clone failures / 2 restored passes; final 2/2 PASS", flush=True)
