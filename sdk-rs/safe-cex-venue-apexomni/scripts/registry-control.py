#!/usr/bin/env python3
"""Prove native construction depends on the actual ApexOmni registry entry."""
from pathlib import Path
import subprocess

workspace = Path(__file__).resolve().parents[2]
source = workspace / 'safe-cex-registry/src/rust_venues.rs'
original = source.read_text()
needle = '    (ExchangeName::Apexomni, apexomni_constructor),\n'
command = ['cargo', 'test', '-p', 'safe-cex-registry', '--jobs', '1', '--test', 'native_host_apexomni', 'apexomni_native_registry_bootstraps_signs_places_and_cancels', '--', '--exact']
assert original.count(needle) == 1
try:
    source.write_text(original.replace(needle, ''))
    result = subprocess.run(command, cwd=workspace, text=True, capture_output=True)
    output = result.stdout + result.stderr
    assert result.returncode == 101, output
    assert 'test apexomni_native_registry_bootstraps_signs_places_and_cancels ... FAILED' in output, output
    assert 'a Rust-constructed venue in the registry table' in output, output
    print('CONTROL PASS: missing ApexOmni constructor fails native-host E2E', flush=True)
finally:
    source.write_text(original)
result = subprocess.run(command, cwd=workspace, text=True, capture_output=True)
assert result.returncode == 0 and '1 passed' in result.stdout, result.stdout + result.stderr
print('RESTORED PASS: native-host ApexOmni registry', flush=True)
