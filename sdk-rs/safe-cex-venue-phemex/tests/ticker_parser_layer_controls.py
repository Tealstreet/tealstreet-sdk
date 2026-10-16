"""Keep cardinality mutants while bypassing guards; fixed walks must still fail."""
from pathlib import Path
import os,re,subprocess
crate=Path(__file__).resolve().parents[1]
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
source=crate/'src/ticker_parser.rs';test=crate/'tests/ticker_parser.rs';oracle=crate/'tests/ticker_parser_oracle.cjs'
# Invoke only after ticker_parser_controls.py restores; never concurrently.
original={p:p.read_text() for p in [source,test,oracle]}
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--test','ticker_parser','--locked','--jobs','1']
controls=[]
for statement in ['tickers.clear();','tickers.truncate(1);']:
 test_text=re.sub(r'assert_eq!\([\s\S]*?\);',lambda m:'if false { '+m[0]+' }' if any(label in m[0] for label in ['independent native output count','expected/native output count']) else m[0],original[test])
 assert test_text.count('if false {')==2,'PHEMEX-PARSER-LAYERS native guard inventory'
 controls.append((statement,{source:original[source].replace('Ok(tickers)',statement+' Ok(tickers)'),test:test_text},'fixed native output coverage'))
for statement in ['splice(0)','splice(1)']:
 test_text=re.sub(r'assert_eq!\([\s\S]*?\);',lambda m:'if false { '+m[0]+' }' if 'source per-case output count' in m[0] else m[0],original[test])
 assert test_text.count('if false {')==1,'PHEMEX-PARSER-LAYERS source guard inventory'
 before='process.stdout.write(JSON.stringify({profile:'
 assert original[oracle].count(before)==1
 oracle_text=original[oracle].replace(before,f'cases[12].expected.value.items.{statement};\n '+before)
 controls.append((statement,{oracle:oracle_text,test:test_text},'expected/native output count'))
assert len(controls)==4,'PHEMEX-PARSER-LAYERS full inventory'
completed=0
for name,changes,assertion in controls:
 try:
  for file,text in changes.items():file.write_text(text)
  result=subprocess.run(command,capture_output=True,text=True);evidence=result.stdout+result.stderr
  assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and 'PHEMEX-PARSER '+assertion in evidence,(name,evidence)
  print(f'COMPILED FAIL101: guard bypass {name}: PHEMEX-PARSER {assertion}',flush=True)
 finally:
  for file,text in original.items():file.write_text(text)
 result=subprocess.run(command,capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print('PASS restored: guard bypass '+name,flush=True);completed+=1
assert completed==4,'PHEMEX-PARSER-LAYERS executed inventory'
