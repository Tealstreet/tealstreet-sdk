#!/usr/bin/env python3
"""Serial compiling faults through the actual Hyro construction/HTTP/socket/Store target."""
import os
import argparse
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
ENV = dict(os.environ, NEXT_PUBLIC_HYROTRADER_PROD_REST_URL="https://prod-mode.example",
           NEXT_PUBLIC_HYROTRADER_SIMULATOR_REST_URL="https://sim-mode.example",
           NEXT_PUBLIC_HYROTRADER_PROD_PUBLIC_WS_URL="", NEXT_PUBLIC_HYROTRADER_PROD_PRIVATE_WS_URL="",
           NEXT_PUBLIC_HYROTRADER_SIMULATOR_PUBLIC_WS_URL="", NEXT_PUBLIC_HYROTRADER_SIMULATOR_PRIVATE_WS_URL="")
ENV.setdefault("SAFE_CEX_TYPESCRIPT_PATH", str(ROOT / "node_modules/typescript"))
COMMAND = ["cargo", "+1.99.0", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex-venue-bybit",
           "--jobs", "1", "--test", "hyro_mode_binding", "hyro_mode_profile_matches_source", "--", "--exact", "--nocapture"]
DIRECTORY = Path(tempfile.mkdtemp(prefix="hyro-mode-controls-"))


def run(name, failure=None, command=COMMAND):
    result = subprocess.run(command, cwd=ROOT, env=ENV, text=True, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, timeout=600)
    (DIRECTORY / (name + ".log")).write_text(result.stdout)
    if failure:
        assert result.returncode != 0 and "Finished `test`" in result.stdout and "test result: FAILED" in result.stdout and failure in result.stdout, result.stdout
    else:
        assert result.returncode == 0 and "1 passed; 0 failed" in result.stdout, result.stdout
    print(name, "compiled RED" if failure else "GREEN", DIRECTORY / (name + ".log"), flush=True)


controls = [
    ("production-route", "sdk-rs/safe-cex-venue-bybit/src/hyro_configuration.rs",
     "if configured(production.1) {\n        return production;", "if configured(production.1) {\n        return simulator;", "assertion `left == right` failed", COMMAND),
    ("protected-override", "sdk-rs/safe-cex-venue-bybit/src/binding/members/hyro.rs",
     '        "shouldUseLocalRestOrderLifecycleFallback",\n', "", "Hyro production retains the local REST cancellation fallback", COMMAND),
    ("account-mode", "sdk-rs/safe-cex/src/account.rs",
     "if account.exchange == ExchangeName::Hyrotrader {", "if account.exchange != ExchangeName::Hyrotrader {",
     "source Hyro account flag/override",
     ["cargo", "+1.99.0", "test", "--manifest-path", "sdk-rs/Cargo.toml", "-p", "safe-cex",
      "--jobs", "1", "--test", "root_services", "hyro_account_mode_maps_before_presence_aware_overrides",
      "--", "--exact", "--nocapture"]),
]
parser = argparse.ArgumentParser()
parser.add_argument("--only", choices=[row[0] for row in controls])
selected = parser.parse_args().only
for name, relative, old, new, failure, command in controls:
    if selected and name != selected:
        continue
    path = ROOT / relative
    original = path.read_bytes()
    source = original.decode()
    assert source.count(old) == 1, (name, source.count(old))
    try:
        path.write_text(source.replace(old, new, 1))
        run(name, failure, command)
    finally:
        path.write_bytes(original)
    assert path.read_bytes() == original
    run(name + "-restored", command=command)
