"""Measure only the two currently required gates at a clean local commit."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


root = Path(__file__).resolve().parents[4]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=root, text=True).strip()


assert not git("diff", "--name-only"), "Commit tracked edits before measuring gates"
assert not git("diff", "--cached", "--name-only"), "Commit staged edits before gates"
sha = git("rev-parse", "HEAD")
folder = root / ".aimux/coinbase-finding-handoff" / sha
folder.mkdir(parents=True, exist_ok=True)
receipt = {
    "sha": sha,
    "branch": git("branch", "--show-current"),
    "cwd": "sdk-rs",
    "gates": [],
    "not_run": [
        "Host clippy: cancelled by user; centrally owned",
        "Canonical wasm clippy: centrally owned",
        "Wiring, layers and routing: centrally owned",
        "CI Rust / TypeScript parity job: green completion unconfirmed",
        "Workspace check/tests, fresh venue tests, extractor and source oracles",
    ],
}
for name, command in [
    ("fmt", ["cargo", "fmt", "--all", "--check"]),
    ("workspace-build", ["cargo", "build", "--workspace", "--locked", "--jobs", "2"]),
]:
    log = folder / f"{name}.log"
    started = time.monotonic()
    with log.open("w") as output:
        result = subprocess.run(
            command,
            cwd=root / "sdk-rs",
            env=dict(os.environ, CARGO_INCREMENTAL="0"),
            stdout=output,
            stderr=subprocess.STDOUT,
        )
    log.write_text(log.read_text().replace(str(root), "$WORKTREE").replace(str(Path.home()), "$HOME"))
    assert git("rev-parse", "HEAD") == sha, "Gate commit changed during measurement"
    receipt["gates"].append({
        "name": name,
        "command": command,
        "exit": result.returncode,
        "seconds": round(time.monotonic() - started, 3),
        "log": log.name,
        "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest(),
    })
    receipt["required_gate_bar_complete"] = len(receipt["gates"]) == 2 and all(
        gate["exit"] == 0 for gate in receipt["gates"]
    )
    (folder / "gates.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(f"{sha} {name}: exit {result.returncode}", flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)
