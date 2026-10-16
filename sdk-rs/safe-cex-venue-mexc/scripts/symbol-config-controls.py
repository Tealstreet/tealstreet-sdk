"""Compile plausible symbol-setting selection defects and require named failures."""
from pathlib import Path
import subprocess
crate = Path(__file__).resolve().parents[1]
source = crate / 'src/symbol_config.rs'
original = source.read_text()
command = ['cargo', 'test', '--manifest-path', 'sdk-rs/Cargo.toml', '--locked', '-p', 'safe-cex-venue-mexc', '--test', 'symbol_config', '--jobs', '2', '--', '--exact', 'mexc_symbol_config_live_source', '--nocapture']
controls = [
 ('wrong position mode', [('position_mode == PositionMode::OneWay', 'position_mode == PositionMode::Hedged')]),
 ('positive-only contracts', [('p.contracts != 0.0 && !p.contracts.is_nan()', 'p.contracts > 0.0')]),
 ('native number display preserves negative zero', [('js_number_string(value)', 'value.to_string()'), ('use safe_cex_num::{JsCoercionError, js_number_string};', 'use safe_cex_num::JsCoercionError;')]),
 ('closing side inverted', [('if opts.side == OrderSide::Sell {', 'if opts.side == OrderSide::Buy {')]),
 ('cached config ignored', [('Some(config) => config,', 'Some(_) => { derived = derive_symbol_config(&opts.symbol, positions, existing, account_mode)?; &derived },')]),
]
try:
 for name,replacements in controls:
  changed=original
  for before,after in replacements:
   assert changed.count(before)==1,(name,before,changed.count(before))
   changed=changed.replace(before,after)
  source.write_text(changed)
  result=subprocess.run(command,cwd=crate.parents[1],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert result.returncode==101,result.stdout
  assert 'mexc_symbol_config_live_source ... FAILED' in result.stdout,result.stdout
  assert 'assertion' in result.stdout,result.stdout
  print(name+': compiled, named assertion failed at101',flush=True)
finally:
 source.write_text(original)
subprocess.run(command,cwd=crate.parents[1],check=True)
