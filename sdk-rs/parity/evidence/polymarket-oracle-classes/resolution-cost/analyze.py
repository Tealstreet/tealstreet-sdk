"""Validate exact-SHA receipts, then price only the observed marginal cycles."""
from pathlib import Path
from datetime import datetime,timezone
import argparse,hashlib,importlib.util,json,math,statistics,subprocess
HERE=Path(__file__).resolve().parent;ROOT=HERE.parents[4]
spec=importlib.util.spec_from_file_location('resolution_driver',ROOT/'sdk-rs/safe-cex-venue-polymarket/tests/resolution_cost.py');d=importlib.util.module_from_spec(spec);spec.loader.exec_module(d)
parser=argparse.ArgumentParser();parser.add_argument('--sha',required=True);args=parser.parse_args();sha=args.sha
assert len(sha)==40,'use full measured SHA'
expected_inputs=d.inputs()
for name,digest in expected_inputs.items():
 blob=subprocess.check_output(['git','show',sha+':'+name],cwd=ROOT)
 assert hashlib.sha256(blob).hexdigest()==digest,('current/measured input mismatch',name)
def accepted(label,exit_code,test=None,mutation=None):
 r=json.loads((HERE/(label+'.json')).read_text());log=(HERE/(label+'.log')).read_bytes();text=log.decode()
 assert r['sha']==sha and r['label']==label,('receipt SHA/label',label)
 assert hashlib.sha256(log).hexdigest()==r['log_sha256'],('log hash',label)
 assert r['exit']==exit_code and math.isfinite(r['wall_seconds']) and r['wall_seconds']>=0,label
 inputs=expected_inputs.copy()
 if mutation:
  name,file,before,after,selected,assertion=mutation
  source=str((d.d.SRC/file).resolve().relative_to(ROOT));original=subprocess.check_output(['git','show',sha+':'+source],cwd=ROOT)
  assert original.decode().count(before)==1
  inputs[source]=hashlib.sha256(original.decode().replace(before,after).encode()).hexdigest()
  assert r['mutation']==name and r['source']==source and r['original_sha256']==expected_inputs[source] and r['mutated_sha256']==inputs[source]
  assert r['non_evidence_dirty_inputs']==[source]
  assert r['expected_assertion']==assertion and 'panicked at' in text and assertion in text and 'could not compile' not in text
 else:assert r['non_evidence_dirty_inputs']==[]
 assert r['input_sha256']==inputs,('input digest set',label)
 if test:
  assert r['test']==test and r['command']==d.cargo(test)
  assert f'test {test} ... '+('FAILED' if exit_code else 'ok') in text,label
  assert ('test result: FAILED. 0 passed; 1 failed;' if exit_code else 'test result: ok. 1 passed; 0 failed;') in text,label
 return r
census=accepted('resolution-census',0)
assert census['command']==['node','sdk-rs/safe-cex-venue-polymarket/tests/class_cost_census.cjs','--resolve-private']
raw=json.loads((HERE/'resolution-census.log').read_text());old=json.loads((HERE.parent/'census.json').read_text());frozen=json.loads((HERE.parent/'inventory.json').read_text())
assert len(raw['inventory'])==len(old['inventory'])==len(frozen)==7341
selection=[]
for ref,historical,current in zip(frozen,old['inventory'],raw['inventory'],strict=True):
 assert all(ref[k]==historical[k]==current[k] for k in ['source','ts','kind','manifest'])
 if historical['class']=='unpriced_resolution':selection.append(current)
assert len(selection)==1400
from collections import Counter
counts=dict(Counter(r['class'] for r in selection));assert sum(counts.values())==1400
assert counts=={'receiver_sync':718,'unpriced_external_calls':120,'orchestration':247,'closed':3,'unpriced_binding':26,'scalar':127,'unpriced_resolution':134,'collection':9,'http':13,'unpriced_callable':3}
controls=[]
for mutation in d.MUTATIONS:
 name,file,before,after,test,assertion=mutation
 mutant=accepted('mutant-'+name,101,test,mutation);restored=accepted('restore-'+name,0,test)
 assert restored['restored_sha256']==mutant['original_sha256']
 controls.append({'name':name,'mutant_exit':101,'restore_exit':0,'assertion':assertion,'mutant_seconds':mutant['wall_seconds'],'restore_seconds':restored['wall_seconds']})
assert len(controls)==6
rows=[]
for index,(family,test,symbol) in enumerate(d.SAMPLES):
 matching=[r for r in selection if r['ts']==symbol];assert len(matching)==1 and matching[0]['class']=='receiver_sync'
 baseline=accepted('baseline-'+family,0,test)
 warm=[accepted(f'warm-{n}-{family}',0,test) for n in range(3)]
 for n,r in enumerate(warm):assert r['repetition']==n and r['row']==symbol and r['cases']==512
 positive=statistics.median(r['wall_seconds'] for r in warm)
 control=controls[index]
 rows.append({'row':symbol,'class':'receiver_sync','baseline_seconds':baseline['wall_seconds'],'positive_seconds':[r['wall_seconds'] for r in warm],'median_positive_seconds':positive,'row_mutation_seconds':control['mutant_seconds'],'row_restoration_seconds':control['restore_seconds'],'cycle_minutes':(positive+control['mutant_seconds']+control['restore_seconds'])/60})
protocol=json.loads((HERE/'protocol.json').read_text());at=datetime.now(timezone.utc);started=datetime.fromisoformat(protocol['first_instrumented_utc'])
checkpoint_path=HERE/'native-checkpoint.json'
checkpoint=json.loads(checkpoint_path.read_text()) if checkpoint_path.exists() else None
native_endpoint=at
native_window=(at-started).total_seconds()/60
if checkpoint:
 assert checkpoint['sha']==sha
 expected_window=(datetime.fromisoformat(checkpoint['verified_utc'])-started).total_seconds()/60
 assert checkpoint['gross_instrumented_window_minutes']==expected_window
 native_window=expected_window
 native_endpoint=datetime.fromisoformat(checkpoint['verified_utc'])
# Sum disjoint subprocess intervals for all fresh campaign receipts, not ancestor runs.
receipts=[json.loads(p.read_text()) for p in HERE.glob('*.json') if p.name.startswith(('baseline-','warm-','mutant-','restore-')) or p.name=='resolution-census.json']
assert len(receipts)==25 and all(r['sha']==sha for r in receipts)
assert 'resolution_native_id:exchange:192' in (HERE/'mutant-id-length65.log').read_text()
expansion=json.loads((HERE/'profile-expansion.json').read_text())
earlier_gates=json.loads((HERE/expansion['earlier_gate_attempt']).read_text())
assert earlier_gates['sha']==expansion['coverage_probe_sha'] and [r['exit'] for r in earlier_gates['runs']]==[0,0,-15]
prior_gate_minutes=sum(r['wall_seconds'] for r in earlier_gates['runs'])/60
later_gate_path=HERE/'gate-summary-ef6cffc8bb.json'
later_gate_minutes=0
if later_gate_path.exists():
 later_gates=json.loads(later_gate_path.read_text()); assert [r['exit'] for r in later_gates['runs']]==[0,0,101]
 later_gate_minutes=sum(r['wall_seconds'] for r in later_gates['runs'])/60
result={'sha':sha,'verified_utc':at.isoformat(),'measured_rows':3,'case_instances_per_row':512,'case_stop_index':511,'selected_controls':6,'rust_controls_caught':5,'source_controls_caught':1,'restorations':6,'survivors':0,'control_stop_index':5,'text_lengths':32,'max_utf16_length':1025,'length65_first_iteration':192,'rows':rows,'mean_cycle_minutes':statistics.mean(r['cycle_minutes'] for r in rows),'frozen_denominator':7341,'historical_resolution_population':1400,'names_resolved':1266,'remaining_resolution':134,'recovered_route_counts':counts,'candidate_sync_rows':718,'candidate_fraction_of_7341':718/7341,'actual_sample_fraction_of_7341':3/7341,'whole_population_cost':None,'reason_no_population_price':'Private naming failures are not a homogeneous runtime class; three purposive recovered methods do not validate transfer to718/1400 rows, much less7341.','gross_instrumented_window_minutes':native_window,'prior_gate_attempt_minutes':prior_gate_minutes,'clippy_failed_gate_attempt_minutes':later_gate_minutes,'profile_expansion_window_minutes':(native_endpoint-datetime.fromisoformat(expansion['started_utc'])).total_seconds()/60,'gross_elapsed_at_validation_minutes':(at-started).total_seconds()/60,'fresh_command_minutes':sum(r['wall_seconds'] for r in receipts)/60,'window_caveat':'Starts after initial selection/merge; current native endpoint includes earlier interrupted and failed gate attempts plus the BUY-only coverage probe. Expanded-profile phase is separate; later shipping gates excluded. Earlier discovery and reviewer effort uninstrumented.','nan_comparison':'NaN class only; other numbers compare native binary64 bits.','not_proven':['complete-function parity or any population confidence percentage','all input lengths/numbers/objects/coercion/alias/scheduling histories','remote services, browser/wasm, constructor/module composition or child correctness','cost for unresolved declarations, other receiver_sync members, or full unpriced remainder']}
(HERE/'analysis.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:result[k] for k in ['sha','measured_rows','rows','mean_cycle_minutes','names_resolved','candidate_sync_rows','gross_instrumented_window_minutes','whole_population_cost']},indent=2))
