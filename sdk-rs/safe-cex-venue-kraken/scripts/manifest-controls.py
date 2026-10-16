#!/usr/bin/env python3
"""Prove the scoped wrapper checks committed provenance rather than inventing it."""
from pathlib import Path
import subprocess

crate=Path(__file__).resolve().parents[1]
manifest=crate.parent/'parity/manifest/exchanges/kraken/kraken.api.ts.toml'
command=['python3.12',str(crate/'scripts/types-parity-check.py')]
def run():
    return subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
baseline=run()
assert baseline.returncode==0,baseline.stdout
original=manifest.read_bytes()
source=original.decode()
anchors=[line for line in source.splitlines() if line.startswith('rust_files = ')]
assert len(anchors)==1,'one committed provenance declaration required'
anchor=anchors[0]
try:
    manifest.write_text(source.replace(anchor,'rust_files = []'))
    broken=run()
    assertion='Rust source file is not declared for packages/safe-cex/src/exchanges/kraken/kraken.api.ts'
    assert broken.returncode not in (0,126) and assertion in broken.stdout,broken.stdout
    print('missing committed provenance failed named gate: '+assertion,flush=True)
finally:
    manifest.write_bytes(original)
restored=run()
assert restored.returncode==0,restored.stdout
print('committed provenance restored: pass',flush=True)
