#!/usr/bin/env python3
"""Compiled controls for the single JS BaseExchange construction factory."""
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "sdk-rs/safe-cex-exchange/src/js_runtime/base_exchange.rs"
SPOT = [
    ("safe-cex-venue-kraken", "exchange_binding", "Kraken source isSpot flag reaches typed base"),
    ("safe-cex-venue-kucoin", "exchange_trait", "KuCoin source isSpot flag reaches typed base"),
]
DEMO = [
    ("safe-cex-venue-bybit", "exchange_binding", "Bybit source isDemo flag reaches typed base"),
    ("safe-cex-venue-okx", "exchange_binding", "OKX source isDemo flag reaches typed base"),
]

def run(crate, target, expected, control):
    result = subprocess.run([
        "cargo", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p", crate,
        "--test", target, "source_identity_flags_survive",
    ], cwd=ROOT, capture_output=True, text=True, timeout=600)
    output = result.stdout + result.stderr
    log = Path(tempfile.gettempdir()) / f"source-identity-{control}-{crate}.txt"
    log.write_text(output)
    if control == "restored":
        assert result.returncode == 0 and "1 passed" in output, str(log)
    else:
        assert result.returncode == 101 and expected in output and "panicked at" in output, str(log)
        assert "could not compile" not in output, str(log)
        print(f"{control}: {crate} compiled assertion caught", flush=True)

original = SOURCE.read_bytes()
try:
    for flag, witnesses in [("spot", SPOT), ("demo", DEMO)]:
        line = f'    core.is_{flag} = this.get("is{flag.title()}")?.truthy();\n'.encode()
        assert original.count(line) == 1
        SOURCE.write_bytes(original.replace(line, b""))
        for crate, target, expected in witnesses:
            run(crate, target, expected, flag)
finally:
    SOURCE.write_bytes(original)
assert SOURCE.read_bytes() == original
for crate, target, expected in SPOT + DEMO:
    run(crate, target, expected, "restored")
print("Four compiled facade controls caught; exact bytes restored; four witnesses pass.")
