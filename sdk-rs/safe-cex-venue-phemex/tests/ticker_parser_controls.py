"""Compiling production/cardinality controls, one Node corpus per scoped target."""
from pathlib import Path
import os, subprocess
crate=Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
source=crate/'src/ticker_parser.rs';test=crate/'tests/ticker_parser.rs';oracle=crate/'tests/ticker_parser_oracle.cjs'
original=source.read_text();test_original=test.read_text();oracle_original=oracle.read_text()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','ticker_parser','--locked','--jobs','1']
controls=[
 ('bid/ask swap',source,'bid: numeric(row.get("bidRp"))?,','bid: numeric(row.get("askRp"))?,','native bid'),
 ('percentage wrong divisor',source,'/ numeric(row.get("openRp"))?)','/ numeric(row.get("lastRp"))?)','native percentage'),
 ('quote volume field swap',source,'quote_volume: numeric(row.get("volumeRq"))? * numeric(row.get("lastRp"))?,','quote_volume: numeric(row.get("volumeRq"))? * numeric(row.get("markRp"))?,','native quoteVolume'),
 ('uppercase suffix mistake',source,'if !raw_symbol.ends_with("USDT") && !raw_symbol.ends_with("USDC")','if !symbol.ends_with("USDT") && !raw_symbol.ends_with("USDC")','independent native output count'),
 ('consumed native empty',source,'Ok(tickers)','tickers.clear(); Ok(tickers)','independent native output count'),
 ('consumed native subset',source,'Ok(tickers)','tickers.truncate(1); Ok(tickers)','independent native output count'),
 ('extra native output',source,'Ok(tickers)','if let Some(first) = tickers.first().cloned() { tickers.push(first); } Ok(tickers)','independent native output count'),
 ('wrapper drops parser input',source,'Ok(parse_tickers(Some(&data), store)?)','let _ = data; Ok(parse_tickers(None, store)?)','native id'),
 ('source empty',oracle,'for(const fixture of cases){','cases.splice(0); for(const fixture of cases){','source case total'),
 ('source subset',oracle,'for(const fixture of cases){','cases.splice(1); for(const fixture of cases){','source case total'),
 ('gated identity disabled',test,'if index >= 4 {','if index >= 6 {','complete comparison census'),
]
assert len(controls)==11,'ticker parser control inventory'
completed=0
for name,file,before,after,assertion in controls:
 try:
  baseline={source:original,test:test_original,oracle:oracle_original}[file]
  assert baseline.count(before)==1,(name,before,baseline.count(before))
  file.write_text(baseline.replace(before,after))
  result=subprocess.run(command,capture_output=True,text=True);evidence=result.stdout+result.stderr
  assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and 'PHEMEX-PARSER '+assertion in evidence,(name,evidence)
  print(f'COMPILED FAIL101: {name}: PHEMEX-PARSER {assertion}',flush=True)
 finally:
  source.write_text(original);test.write_text(test_original);oracle.write_text(oracle_original)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('PASS restored: '+name,flush=True);completed+=1
assert completed==11,'ticker parser executed control inventory'
