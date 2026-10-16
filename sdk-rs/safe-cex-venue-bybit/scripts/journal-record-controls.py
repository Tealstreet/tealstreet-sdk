#!/usr/bin/env python3
"""Serial production controls; restore exact bytes even when a gate fails."""
import os
from pathlib import Path
import subprocess
root = Path(__file__).resolve().parents[3]
source = root / 'sdk-rs/safe-cex-venue-bybit/src/journal_fold.rs'
original = source.read_text()
command = ['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--lib','journal_records_match_source_native_values','--jobs','2','--','--include-ignored']
env = dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls = [
    ('eager-optional-leg', 'None => "both".into(),', 'None => { let _ = order_id(row)?; "both".into() },', 'JOURNAL native success case'),
    ('finite-num-zero', 'Ok(if number.is_finite() { number } else { 0. })', 'Ok(if number.is_finite() { 0. } else { number })', 'JOURNAL native record count'),
    ('closing-side-inversion', 'PositionSide::Long\n    } else {\n        PositionSide::Short', 'PositionSide::Short\n    } else {\n        PositionSide::Long', 'JOURNAL native side'),
    ('funding-sign', 'total.net_pnl + total.fees - funding', 'total.net_pnl + total.fees + funding', 'JOURNAL native realizedPnl'),
    ('partial-output', 'Ok(records)\n}', 'records.truncate(1);\n    Ok(records)\n}', 'JOURNAL native record count'),
    ('inherited-fallback', 'return Err(\n                "RepresentationError: inherited closeReason is outside PositionCloseReason".into(),\n            );', 'PositionCloseReason::User', 'JOURNAL native inherited refusal'),
]
try:
    for name, before, after, assertion in controls:
        assert original.count(before) == 1, (name, 'mutation selector drift')
        source.write_text(original.replace(before,after))
        result = subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        print(name, 'exit', result.returncode, flush=True)
        print(result.stdout, flush=True)
        assert result.returncode == 101 and assertion in result.stdout and 'panicked at' in result.stdout, (name,'SURVIVOR or non-runtime failure')
finally:
    source.write_text(original)
result = subprocess.run(command,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
print('restored exit',result.returncode,flush=True)
print(result.stdout,flush=True)
assert result.returncode == 0 and '1 passed; 0 failed; 0 ignored' in result.stdout
