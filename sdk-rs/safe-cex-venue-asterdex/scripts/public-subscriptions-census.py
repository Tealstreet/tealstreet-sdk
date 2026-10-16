"""Independent eight-row expectation, measured with tomllib; iteration 1."""
import collections
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASE = "1674d852f7"
PREFIX = "sdk-rs/parity/manifest/exchanges/asterdex"
EXPECTED = {
    "AsterDEXPublicWebsocket.onOpen": "divergent",
    "AsterDEXPublicWebsocket.listenOrderBook": "divergent",
    "AsterDEXPublicWebsocket#subscribeOrderBook": "divergent",
    "AsterDEXPublicWebsocket#unsubscribeOrderBookListener": "divergent",
    "AsterDEXPublicWebsocket#clearOrderBookSubscriptionRetries": "ported",
    "AsterDEXPublicWebsocket.subscribeToKline": "divergent",
    "AsterDEXPublicWebsocket.unsubscribeFromKline": "ported",
    "AsterDEXPublicWebsocket.dispose": "divergent",
}
expected = EXPECTED
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
assert before == {"ported": 121, "divergent": 60, "pending": 50}
assert moved == expected, moved
assert after == {"ported": 123, "divergent": 66, "pending": 42}
assert len(set(divergent_notes)) == 6 and len(set(divergent_targets)) == 6
print(f"Base {BASE}: {dict(before)}")
print(f"Current: {dict(after)}; 2 pending->ported, 6 pending->divergent")
print("Iteration 1:8 files/231 rows,8 independently named changes;6 distinct divergent notes/targets")
