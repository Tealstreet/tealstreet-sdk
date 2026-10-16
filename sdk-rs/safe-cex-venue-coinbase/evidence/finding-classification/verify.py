"""Verify the frozen Coinbase classification inventory; no behaviour proof."""

from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib


root = Path(__file__).resolve().parents[4]
ledger = json.loads(Path(__file__).with_name("ledger.json").read_text())
observed = []
counts = Counter()
paths = sorted((root / "sdk-rs/parity/manifest/exchanges/coinbase").glob("*.toml"))
for path in paths:
    relative = path.relative_to(root).as_posix()
    raw = path.read_bytes()
    assert hashlib.sha256(raw).hexdigest() == ledger["manifest_sha256"][relative], (
        "Coinbase manifest bytes preserved"
    )
    for row in tomllib.loads(raw.decode())["symbol"]:
        counts[row["status"]] += 1
        if row["status"] == "divergent" and row["note"].startswith("FINDING:"):
            observed.append((relative, row))

assert len(paths) == 16 and sum(counts.values()) == 426, "Coinbase manifest/row floor"
assert {key: counts[key] for key in ("ported", "divergent", "pending")} == {
    "ported": 393,
    "divergent": 33,
    "pending": 0,
}, "Coinbase accepted census"
assert [(name, row["ts"]) for name, row in observed] == [
    (row["manifest"], row["ts"]) for row in ledger["rows"]
], "Coinbase FINDING identity coverage"
assert len(observed) == 16, "Coinbase selected FINDING floor"
assert [row["index"] for row in ledger["rows"]] == list(range(16)), (
    "Coinbase frozen classification indexes"
)
for (_, native), reviewed in zip(observed, ledger["rows"]):
    assert native["rs"] == reviewed["rs"], "Coinbase FINDING targets preserved"
    assert hashlib.sha256(native["note"].encode()).hexdigest() == reviewed["note_sha256"], (
        "Coinbase FINDING note preserved"
    )
assert Counter(row["classification"] for row in ledger["rows"]) == {
    "behaviour": 5,
    "rationale/proof": 11,
}, "Coinbase reviewed classification totals"
assert {
    (Path(row["manifest"]).name, row["ts"])
    for row in ledger["rows"] if row["classification"] == "behaviour"
} == {
    ("coinbase.exchange.ts.toml", "CoinbaseExchange"),
    ("coinbase.exchange.ts.toml", "CoinbaseExchange.tick"),
    ("coinbase.exchange.ts.toml", "CoinbaseExchange.updateOrder"),
    ("coinbase.ws-private.ts.toml", "CoinbasePrivateWebsocket.handleUserEventOrders"),
    ("coinbasespot.exchange.ts.toml", "CoinbaseSpotExchange"),
}, "Coinbase reviewed behaviour identities"
for relative, expected in ledger["reviewed_input_sha256"].items():
    assert hashlib.sha256((root / relative).read_bytes()).hexdigest() == expected, (
        "Coinbase reviewed source bytes preserved"
    )
print(json.dumps({
    "sha": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
    "census": {key: counts[key] for key in ("ported", "divergent", "pending")},
    "classification": {"behaviour": 5, "rationale/proof": 11},
    "status_rows_moved": 0,
    "stopped_after": "index 15 CoinbaseSpotExchange",
    "scope": "Inventory and frozen source only; not behavioural conformance",
}, indent=2))
