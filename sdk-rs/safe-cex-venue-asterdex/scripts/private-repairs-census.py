"""Independent nine-row expectation, measured with tomllib; iteration 1."""
import collections
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = "0f7fd2ce92e79d0c38e87ea5dd15b21927ea8c1a"
PREFIX = "sdk-rs/parity/manifest/exchanges/asterdex"
EXPECTED = {
    "schedulePositionRepair": "divergent", "runPositionRepair": "divergent",
    "clearPositionRepairTimer": "ported",
    "scheduleBalanceReconcile": "divergent", "runBalanceReconcile": "divergent",
    "clearBalanceReconcileTimer": "ported",
    "scheduleOrderReconcile": "divergent", "runOrderReconcile": "divergent",
    "clearOrderReconcileTimer": "ported",
}
expected = {"AsterDEXPrivateWebsocket#" + name: status for name, status in EXPECTED.items()}
paths = sorted((ROOT / PREFIX).glob("*.toml"))
assert len(paths) == 8
before, after = collections.Counter(), collections.Counter()
moved, divergent_notes, divergent_targets = {}, [], []
for path in paths:
    relative = path.relative_to(ROOT)
    old = tomllib.loads(subprocess.check_output(["git", "show", f"{BASE}:{relative}"], cwd=ROOT, text=True))
    current = tomllib.loads(path.read_text())
    old_rows = {(r["kind"], r["ts"]): r for r in old["symbol"]}
    new_rows = {(r["kind"], r["ts"]): r for r in current["symbol"]}
    assert old_rows.keys() == new_rows.keys()
    before.update(r["status"] for r in old_rows.values())
    after.update(r["status"] for r in new_rows.values())
    for identity, row in new_rows.items():
        if old_rows[identity]["status"] != row["status"]:
            assert old_rows[identity]["status"] == "pending"
            moved[row["ts"]] = row["status"]
            assert row["rs"] and row["note"]
            if row["status"] == "divergent":
                assert row["note"].startswith("CHOSEN:")
                divergent_notes.append(row["note"])
                divergent_targets.append(row["rs"])
assert before == {"ported": 115, "divergent": 49, "pending": 67}
assert moved == expected, moved
assert after == {"ported": 118, "divergent": 55, "pending": 58}
assert len(set(divergent_notes)) == 6 and len(set(divergent_targets)) == 6
print(f"Base {BASE}: {dict(before)}")
print(f"Current: {dict(after)}; 3 pending->ported, 6 pending->divergent")
print("Iteration 1:8 files/231 rows,9 independently named changes;6 distinct divergent notes/targets")
