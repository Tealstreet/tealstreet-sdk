"""Empty/subset source controls; baseline oracle bytes never change."""
from pathlib import Path
import os,subprocess,re,sys
crate=Path(__file__).resolve().parents[1]
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-phemex','--locked','--jobs','1']
checks=[
 ('utilities','utility_cardinality_oracle.cjs','cases','price diff corpus census'),
 ('constants','constants_oracle.cjs','lookups.BASE_URL','prototype observation retained'),
 ('constants','constants_oracle.cjs','ordered','PHEMEX-CARDINALITY source constant total'),
 ('types','cardinality_oracle.cjs','cases','PHEMEX-CARDINALITY source case total'),
 ('types','cardinality_oracle.cjs','envelopes','PHEMEX-CARDINALITY source envelope total'),
 ('types','cardinality_oracle.cjs','supplement','PHEMEX-CARDINALITY supplement case total'),
 ('types','cardinality_oracle.cjs','supplement.2.expected','PHEMEX-CARDINALITY expected position count'),
 ('utilities','utility_cardinality_oracle.cjs','symbolInputs','PHEMEX-CARDINALITY symbol input total'),
 ('utilities','utility_cardinality_oracle.cjs','symbolOutputs','PHEMEX-CARDINALITY symbol output total'),
 ('utilities','utility_cardinality_oracle.cjs','numericOutputs','PHEMEX-CARDINALITY numeric output total'),
 ('utilities','utility_cardinality_oracle.cjs','upper','PHEMEX-CARDINALITY uppercase mapping total'),
 ('utilities','utility_cardinality_oracle.cjs','supplementOutputs','PHEMEX-CARDINALITY utility supplement total'),
 ('utilities','utility_cardinality_oracle.cjs','supplementOutputs.1.result','PHEMEX-CARDINALITY utility source array length'),
 ('account','account_oracle.cjs','cases','account case census'),
 ('ticker_reads','ticker_reads_oracle.cjs','cases','ticker case census'),
]
bypass='--bypass' in sys.argv
independent={
 ('constants','ordered'):'constant export census',
 ('constants','lookups.BASE_URL'):'PHEMEX-CARDINALITY lookup comparison total',
 ('types','cases'):'PHEMEX-CARDINALITY fixed cases case',
 ('types','envelopes'):'PHEMEX-CARDINALITY fixed envelopes case',
 ('types','supplement'):'PHEMEX-CARDINALITY fixed supplement case',
 ('types','supplement.2.expected'):'PHEMEX-CARDINALITY expected/native positions',
 ('utilities','cases'):'PHEMEX-CARDINALITY fixed cases case',
 ('utilities','symbolInputs'):'PHEMEX-CARDINALITY fixed symbolInputs case',
 ('utilities','symbolOutputs'):'PHEMEX-CARDINALITY fixed symbolOutputs case',
 ('utilities','numericOutputs'):'PHEMEX-CARDINALITY fixed numericOutputs case',
 ('utilities','upper'):'native symbol uppercase',
 ('utilities','supplementOutputs'):'PHEMEX-CARDINALITY fixed supplementOutputs case',
 ('utilities','supplementOutputs.1.result'):'native symbol output first match',
 ('account','cases'):'PHEMEX-CARDINALITY fixed cases case',
 ('ticker_reads','cases'):'PHEMEX-CARDINALITY fixed cases case',
}
guards=['source constant total','prototype observation retained','lookup census:',
 'source case total','source envelope total','supplement case total','source per-case count',
 'expected position count','raw source position count','symbol input total','symbol output total',
 'numeric output total','price diff corpus census','uppercase mapping total','utility supplement total',
 'utility source array length','account case census','ticker case census']
assert len(checks)==15, 'PHEMEX-SOURCE-CONTROLS full inventory'
expected_count=2 if '--new-only' in sys.argv else 15
selected=[row for row in checks if '--new-only' not in sys.argv or (row[0],row[2]) in [('utilities','cases'),('constants','lookups.BASE_URL')]]
assert len(selected)==expected_count, 'PHEMEX-SOURCE-CONTROLS selected inventory'
completed=0
for target,oracle,path,assertion in selected:
 if bypass:assertion=independent[target,path]
 source=crate/f'tests/{target}.rs';original=source.read_text()
 try:
  assert original.count('/tests/'+oracle)==1
  modified=original.replace('/tests/'+oracle,'/tests/cardinality_source_proxy.cjs')
  if bypass:
   modified=re.sub(r'assert(?:_eq)?!\([\s\S]*?\);',lambda m:'if false { '+m[0]+' }' if any(g in m[0] for g in guards) else m[0],modified)
  source.write_text(modified)
  for mode in ['empty','subset']:
   env=dict(os.environ,PHEMEX_ORACLE_FILE=oracle,PHEMEX_DROP_PATH=path,PHEMEX_DROP_MODE=mode)
   result=subprocess.run(command+['--test',target],capture_output=True,text=True,env=env)
   evidence=result.stdout+result.stderr
   assert result.returncode==101 and 'running 1 test' in evidence and 'test result: FAILED' in evidence and assertion in evidence,(target,path,mode,evidence)
   print(f"COMPILED FAIL101 source {'BYPASS ' if bypass else ''}{target}/{path}/{mode}: {assertion}",flush=True)
 finally:source.write_text(original)
 result=subprocess.run(command+['--test',target],capture_output=True,text=True)
 assert result.returncode==0,result.stdout+result.stderr
 print(f'PASS restored: {target}/{path}',flush=True)

 completed+=1
assert completed==expected_count, 'PHEMEX-SOURCE-CONTROLS executed inventory'
