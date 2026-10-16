"""Substitute plausible fallback/validation defects and require named failures."""
from pathlib import Path
import subprocess
crate=Path(__file__).resolve().parents[1]
root=crate.parents[1]
source=crate/'src/utils.rs'
original=source.read_text()
command=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-mexc','--test','utils','--jobs','1','--','--exact','mexc_utils_live_typescript_every_value','--nocapture']
fallback='.filter(|n| n.is_finite() && *n > 0.0)\n        .unwrap_or(1.0)'
validation='    let mut primary = None;'
controls=[('normalizeTimestampToMs','numeric > 0.0','numeric >= 0.0'),
          ('normalizeTimestampToMs','numeric.floor()','safe_cex_num::js_time_clip(numeric)'),
          ('getContractSize',fallback,fallback.replace('unwrap_or(1.0)','unwrap_or(2.0)')),
          ('normalizeBalance',validation,'    for row in rows { record(row)?; }\n'+validation),
          ('normalizeMarket amount identity','amount: amount_step,','amount: price_step,'),
          ('normalizeMarket price identity',
           'let price_step = to_number(\n        raw.get("priceUnit"),\n        Some(step_from_scale(raw.get("priceScale"), 0.00000001)?),\n    )?;',
           'let price_step = amount_step;')]
try:
    for name,before,after in controls:
        assert original.count(before)==1
        source.write_text(original.replace(before,after))
        result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        assert result.returncode==101,result.stdout
        assert 'mexc_utils_live_typescript_every_value ... FAILED' in result.stdout,result.stdout
        if name.endswith('amount identity'):
            assertion='CHASER-PRECISION native amount step'
        elif name.endswith('price identity'):
            assertion='CHASER-PRECISION native price step'
        else:
            assertion=name
        assert 'assertion `left == right` failed' in result.stdout and assertion in result.stdout,result.stdout
        print(f'Rust {assertion} mutation: named assertion fails, exit 101',flush=True)
finally:source.write_text(original)
subprocess.run(command,cwd=root,check=True)
