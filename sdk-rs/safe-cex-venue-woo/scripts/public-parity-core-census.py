"""Independent nine-symbol movement expectation, using tomllib at a named HEAD."""
import collections
import json
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = "c4caf76e6cdb22b858492ca47796fcb46a212d9e"
DIRECTORY = "sdk-rs/parity/manifest/exchanges/woo"
PUBLIC = DIRECTORY + "/woo.v3.public-parity.ts.toml"
RUNTIME = DIRECTORY + "/woo.v3.runtime-selector.ts.toml"
EXPECTED = {
    "WooV3PublicParityRouteId": "ported",
    "WooV3PublicParityMismatch": "divergent",
    "WooV3PublicParityBlocker": "divergent",
    "WooV3PublicParityResult": "divergent",
    "blocker": "divergent",
    "mismatch": "divergent",
    "compareFields": "divergent",
    "compareKeyed": "divergent",
    "result": "divergent",
}

def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

def rows(revision, file):
    return tomllib.loads(git("show", f"{revision}:{file}"))["symbol"]

def triple(revision, files):
    counts = collections.Counter(r["status"] for f in files for r in rows(revision, f))
    return [counts[x] for x in ("ported", "divergent", "pending")]

head = git("rev-parse", "HEAD")
before = {r["ts"]: r for r in rows(BASE, PUBLIC)}
after = {r["ts"]: r for r in rows(head, PUBLIC)}
changed = {s for s in before if before[s] != after[s]}
assert changed == set(EXPECTED), ("exact assigned public changes", changed)
for name, status in EXPECTED.items():
    assert before[name]["status"] == "pending", name
    assert after[name]["status"] == status, name
    assert after[name]["rs"], ("real native target", name)
    if status == "divergent":
        assert after[name]["note"].startswith("MIXED:"), name
assert rows(BASE, RUNTIME) == rows(head, RUNTIME), "other assigned file unchanged"
files = git("ls-tree", "-r", "--name-only", head, DIRECTORY).splitlines()
files = [f for f in files if f.endswith(".toml")]
print(json.dumps({
    "sha": head, "parent": BASE, "moved": {"ported": 1, "divergent": 8, "total": 9},
    "triple_order": ["ported", "divergent", "pending"],
    "assigned_before": triple(BASE, [PUBLIC, RUNTIME]),
    "assigned_after": triple(head, [PUBLIC, RUNTIME]),
    "woo_before": triple(BASE, files), "woo_after": triple(head, files),
    "enumeration_floor": "all tracked WOO manifest files at this SHA; selected nine public symbols, not behavioral completeness",
}, indent=2))
