#!/usr/bin/env python3
"""One compiling fault per new row family, with exact restoration and native failures."""
from pathlib import Path
import os, subprocess
root=Path(__file__).resolve().parents[3]
source=root/'sdk-rs/safe-cex-venue-bybit/src'
env=dict(os.environ,SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-bybit','--test','ledger_mapping','--jobs','2','--','--include-ignored']
controls=[
 ('number','ledger_mapping.rs','Ok(if parsed.is_finite() { parsed } else { 0. })','Ok(if parsed.is_finite() { -parsed } else { 0. })','LEDGER native toNumber'),
 ('optional-number','ledger_mapping.rs','parsed.is_finite() && parsed != 0.','parsed.is_finite()','LEDGER native absent toOptionalNumber'),
 ('timestamp','ledger_mapping.rs','parsed.is_finite() && parsed > 0.','parsed.is_finite()','LEDGER native toTimestamp'),
 ('historical-order','ledger_mapping.rs','        price,\n        average_price,','        price: average_price,\n        average_price: price,','LEDGER native order price'),
 ('closed-pnl','ledger_mapping.rs','entry_price: ClosedPnlEntryEntryPrice::Variant1(entry_price),\n        exit_price: ClosedPnlEntryExitPrice::Variant1(exit_price),','entry_price: ClosedPnlEntryEntryPrice::Variant1(exit_price),\n        exit_price: ClosedPnlEntryExitPrice::Variant1(entry_price),','LEDGER native closed entryPrice'),
 ('category','ledger_mapping.rs','"SETTLEMENT" => IncomeCategory::Funding,','"SETTLEMENT" => IncomeCategory::Other,','LEDGER native named category'),
 ('income','ledger_mapping.rs','            -fee,','            fee,','LEDGER native income amount'),
 ('fee-type','fee_overlay.rs','    pub taker_fee_rate: String,\n    pub maker_fee_rate: String,','    #[serde(rename = "makerFeeRate")]\n    pub taker_fee_rate: String,\n    #[serde(rename = "takerFeeRate")]\n    pub maker_fee_rate: String,','LEDGER native fee taker input'),
 ('fee-overlay','fee_overlay.rs','        .iter()\n        .cloned()','        .iter()\n        .rev()\n        .cloned()','LEDGER native overlay takerFee'),
]
print('candidate SHA',subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip(),flush=True)
for family,file,before,after,assertion in controls:
 p=source/file;original=p.read_text();assert original.count(before)==1,(family,'selector drift')
 try:
  p.write_text(original.replace(before,after));r=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  print(family,'mutated exit',r.returncode,flush=True);print(r.stdout,flush=True)
  assert r.returncode==101 and 'panicked at' in r.stdout and assertion in r.stdout,(family,'SURVIVOR or non-runtime failure')
 finally:p.write_text(original)
 r=subprocess.run(cmd,cwd=root,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 print(family,'restored exit',r.returncode,flush=True);print(r.stdout,flush=True)
 assert r.returncode==0 and '1 passed; 0 failed; 0 ignored' in r.stdout,(family,'restore failed')
