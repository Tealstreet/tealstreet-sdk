"""Replay the actual pre-fix reader bodies, then restore exact bytes and GREEN.

Run serially, with no other Cargo work in this worktree. Compiler/fixture failures
are never accepted as the scheduling RED. The reference commit preserves the
unchanged master implementation on which the initial three failures were found.
"""
import hashlib
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[3]
SDK = ROOT / "sdk-rs"
SOURCE = SDK / "safe-cex-venue-blofin/src/exchange.rs"
RELATIVE = str(SOURCE.relative_to(ROOT))
BEFORE = "ef5fd04477"
NAMES = [
    "cached_margin_read_in_normal_formatter_defers_the_position_mode_observation",
    "cached_position_side_in_algo_formatter_defers_the_margin_observation",
    "cached_margin_read_in_trigger_formatter_defers_the_position_mode_observation",
    "cached_pair_in_batch_placement_defers_all_formatter_observations",
]


def run(label, selected):
    env = dict(os.environ)
    env.setdefault("SAFE_CEX_TYPESCRIPT_PATH", str(ROOT / "node_modules/typescript"))
    result = subprocess.run(
        ["cargo", "test", "--locked", "-p", "safe-cex-venue-blofin", "--test",
         "divergence_timing_binding", "--jobs", "1", *selected],
        cwd=SDK, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    )
    log = Path(tempfile.gettempdir()) / f"port-txflow-blofin-{label}.log"
    log.write_text(result.stdout)
    print(f"{label}: exit {result.returncode}, {log}", flush=True)
    return result


def main():
    original = SOURCE.read_bytes()
    sha = hashlib.sha256(original).hexdigest()
    reference = subprocess.check_output(["git", "show", f"{BEFORE}:{RELATIVE}"], cwd=ROOT).decode()
    start = reference.index("    pub async fn get_or_fetch_position_mode(")
    end = reference.index("    pub async fn fetch_leverage_for_symbol(", start)
    current = original.decode()
    left = current.index("    pub fn get_or_fetch_position_mode(")
    right = current.index("    pub async fn fetch_leverage_for_symbol(", left)
    try:
        SOURCE.write_text(current[:left] + reference[start:end] + current[right:])
        red = run("cached-original-body-red4", ["cached_"])
        assert red.returncode == 101, red.stdout
        assert "could not compile" not in red.stdout and "error[E" not in red.stdout, red.stdout
        assert red.stdout.count("CACHED-AWAIT") == 4, red.stdout
        for name in NAMES:
            assert f"test {name} ... FAILED" in red.stdout, red.stdout
    finally:
        SOURCE.write_bytes(original)
    assert hashlib.sha256(SOURCE.read_bytes()).hexdigest() == sha
    green = run("cached-restored-green", [])
    assert green.returncode == 0, green.stdout
    assert "7 passed; 0 failed" in green.stdout, green.stdout
    print(f"Exact production restoration SHA256 {sha}; compiled RED4 -> GREEN7", flush=True)


if __name__ == "__main__":
    main()
