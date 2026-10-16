"""tomllib census; independent expectation is eight named source rows, iteration 1."""
import collections
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = "85f3b9e596582fce3738807c4d9b7032c5b5a802"
PREFIX = "sdk-rs/parity/manifest/exchanges/asterdex/"
EXPECTED = {
    "AsterDEXVirtualClock": "divergent",
    "AsterDEXVirtualClock.start": "divergent",
    "AsterDEXVirtualClock.stop": "ported",
    "AsterDEXVirtualClock.syncClientServerTime": "divergent",
    "AsterDEXVirtualClock#syncClientServerTimeInner": "divergent",
    "AsterDEXVirtualClock#fetchServerTime": "ported",
    "AsterDEXVirtualClock.forceSyncAndGetOffset": "divergent",
    "asterDEXVirtualClock": "divergent",
}
paths = sorted((ROOT / PREFIX).glob("*.toml"))
assert len(paths) == 8, "iteration 1 has eight source files"
before = collections.Counter()
after = collections.Counter()
moved = {}
for path in paths:
    relative = str(path.relative_to(ROOT))
    previous = tomllib.loads(subprocess.check_output(["git", "show", f"{BASE}:{relative}"], cwd=ROOT, text=True))
    current = tomllib.loads(path.read_text())
    old = {(r["kind"], r["ts"]): r for r in previous["symbol"]}
    new = {(r["kind"], r["ts"]): r for r in current["symbol"]}
    assert old.keys() == new.keys(), "no source identities created or removed"
    before.update(r["status"] for r in old.values())
    after.update(r["status"] for r in new.values())
    for identity, row in new.items():
        if row["status"] != old[identity]["status"]:
            assert old[identity]["status"] == "pending"
            assert row["rs"] and row["note"]
            moved[row["ts"]] = row["status"]
assert before == {"ported": 113, "divergent": 43, "pending": 75}
assert moved == EXPECTED, (moved, EXPECTED)
assert after == {"ported": 115, "divergent": 49, "pending": 67}
print(f"Base {BASE}: {dict(before)}")
print(f"Current: {dict(after)}; 2 pending->ported, 6 pending->divergent")
print("Enumeration floor: iteration 1, eight files / 231 rows, eight named changes")
