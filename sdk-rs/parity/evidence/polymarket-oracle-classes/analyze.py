"""Compute observations/scenarios; never convert these into parity percentages."""
from pathlib import Path
from datetime import datetime, timezone
import importlib.util,json,statistics,hashlib,subprocess,re,tomllib,math
from collections import Counter

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
driver=ROOT/'sdk-rs/safe-cex-venue-polymarket/tests/class_cost.py'
spec=importlib.util.spec_from_file_location('cost_driver',driver)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
labels=['name','tick-membership','precision-default','token-identity','record-identity','http-endpoint','market-quote','ticker-batch','placement-side']
protocol=json.loads((HERE/'protocol.json').read_text())
expected_sha=protocol['integration_update']['merge_sha']
reference=json.loads((HERE/'warm-0-class_cost_closed_native.json').read_text())['input_sha256']
assert set(reference)==set(module.input_hashes()),'independently enumerated input path set differs'
targets={test:target for cls,target,test in module.SAMPLES}
for path,digest in reference.items():
    blob=subprocess.check_output(['git','show',expected_sha+':'+path],cwd=ROOT)
    assert hashlib.sha256(blob).hexdigest()==digest,('reference differs from measured commit',path)

def accepted(label,exit_code,assertion=None):
    record=json.loads((HERE/(label+'.json')).read_text())
    log=(HERE/(label+'.log')).read_bytes()
    assert record['sha']==expected_sha,('mixed measurement SHA',label)
    assert hashlib.sha256(log).hexdigest()==record['log_sha256'],('log digest',label)
    expected=reference.copy()
    if record.get('mutation'):
        source=record['source']
        assert record['original_sha256']==reference[source],label
        expected[source]=record['mutated_sha256']
        assert record['non_evidence_dirty_inputs']==[source],label
    else:assert record['non_evidence_dirty_inputs']==[],label
    assert record['input_sha256']==expected,('corpus/source mismatch',label)
    assert record['exit']==exit_code,label
    assert math.isfinite(record['wall_seconds']) and record['wall_seconds']>=0,label
    text=log.decode()
    test=record['test']
    assert record['command']==module.cargo(targets[test],test),('wrong command/selector',label)
    assert 'could not compile' not in text,label
    assert f'test {test} ... '+('FAILED' if exit_code else 'ok') in text,('wrong runtime test',label)
    if exit_code==101:
        assert assertion and record['expected_assertion']==assertion,label
        assert re.search(r'panicked at[\s\S]*?'+re.escape(assertion),text),label
        assert 'test result: FAILED. 0 passed; 1 failed;' in text,label
    else:
        assert 'test result: ok. 1 passed; 0 failed;' in text,label
    return record

assertions=['class_cost_closed_native','generated_tick_native','generated_precision_native','generated_tokens_native_token','class_cost_record_price','class_cost_http_url','class_cost_native','class_cost_native','class_cost_native']
controls=[]
def control(label,assertion,test):
    mutant=accepted('mutant-'+label,101 if assertion else 0,assertion)
    restored=accepted('restore-'+label,0)
    assert mutant['mutation']==label and mutant['label']=='mutant-'+label and restored['label']=='restore-'+label,label
    assert mutant['test']==restored['test']==test,label
    assert restored['restored_sha256']==mutant['original_sha256'],label
    controls.append({'label':label,'mutant_exit':mutant['exit'],'restore_exit':restored['exit'],'assertion':assertion,'sha':expected_sha,'outcome':'caught' if assertion else 'SURVIVED: corpus ends at32, mutation applies only at65'})
    return mutant,restored
rows=[]
for (cls,target,test),label,assertion in zip(module.SAMPLES,labels,assertions,strict=True):
    warm=[accepted(f'warm-{i}-{test}',0) for i in range(3)]
    assert all(r['class']==cls and r['cases']==512 for r in warm),test
    mutant,restored=control(label,assertion,test)
    assert all(x['test']==test for x in warm+[mutant,restored]),label
    warm_seconds=statistics.median(x['wall_seconds'] for x in warm)
    rows.append({'class':cls,'test':test,'generated_calls':512,'warm_command_median_seconds':warm_seconds,'fault_command_seconds':mutant['wall_seconds'],'restore_command_seconds':restored['wall_seconds'],'one_positive_plus_one_compiled_fault_and_restoration_seconds':warm_seconds+mutant['wall_seconds']+restored['wall_seconds'],'input_measurement_shas':[expected_sha]})
for label,assertion,test in [('source-map-erasure','class_cost_native_fields','class_cost_markets_native'),('token-length17','generated_tokens_native_length','class_cost_tokens_native'),('token-length65-survivor',None,'class_cost_tokens_native')]:control(label,assertion,test)
census=json.loads((HERE/'census.json').read_text())
inventory=json.loads((HERE/'inventory.json').read_text())
key=lambda r:(r['manifest'],r['ts'],r['kind'])
assert census['sha']==expected_sha
assert len(inventory)==len(census['inventory'])==census['denominator']==7341
assert Counter(map(key,inventory))==Counter(map(key,census['inventory']))
assert Counter(r['class'] for r in census['inventory'])==census['counts']
# Validate the frozen inventory against the measured Git tree, not its own length.
actual=[]
manifest_tree=subprocess.check_output(['git','ls-tree','-r',expected_sha,'--','sdk-rs/parity/manifest'],cwd=ROOT,text=True)
for line in manifest_tree.splitlines():
    meta,path=line.split('\t',1)
    if not path.endswith('.toml'):continue
    raw=(ROOT/path).read_bytes()
    git_digest=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
    assert git_digest==meta.split()[2],('manifest changed since measured SHA',path)
    data=tomllib.loads(raw.decode())
    actual.extend({'source':data['file']['ts'],'ts':r['ts'],'kind':r['kind'],'status':r['status'],'manifest':path} for r in data.get('symbol',[]))
assert Counter(json.dumps(r,sort_keys=True) for r in actual)==Counter(json.dumps(r,sort_keys=True) for r in inventory),'inventory differs from Git-bound manifests'

classes=[]
for cls in ['closed','scalar','collection','record','http','orchestration']:
    selected=[r for r in rows if r['class']==cls]
    rate=statistics.mean(r['one_positive_plus_one_compiled_fault_and_restoration_seconds'] for r in selected)
    classes.append({'class':cls,'purposive_sample_rows':len(selected),'syntactic_rows':census['counts'][cls],'fraction_of_7341':census['counts'][cls]/7341,'mean_warm_command_seconds':statistics.mean(r['warm_command_median_seconds'] for r in selected),'mean_evidence_cycle_minutes':rate/60,'conditional_scenario_hours':census['counts'][cls]*rate/3600})
priced=sum(c['syntactic_rows'] for c in classes);unpriced=7341-priced
partial=sum(c['conditional_scenario_hours'] for c in classes)
rates=[c['mean_evidence_cycle_minutes'] for c in classes]
checkpoint=json.loads((HERE/'native-checkpoint.json').read_text())
assert checkpoint['measured_sha']==expected_sha
now=datetime.fromisoformat(checkpoint['endpoint_utc'])
start=datetime.fromisoformat(protocol['assignment_started_utc'])
all_runs=[]
for path in HERE.rglob('*.json'):
    if path.name in ['analysis.json','census.json','inventory.json','protocol.json']:continue
    data=json.loads(path.read_text())
    if isinstance(data,dict) and all(k in data for k in ['label','started_utc','finished_utc','wall_seconds','exit']):all_runs.append(data)
# Retries have separate immutable archived timestamps. Avoid counting latest copies twice.
unique={(r['label'],r['started_utc']):r for r in all_runs}
intervals=sorted((max(start,datetime.fromisoformat(r['started_utc'])),min(now,datetime.fromisoformat(r['finished_utc']))) for r in unique.values() if r.get('phase')!='workspace-gate' and not r['label'].startswith('gate-') and datetime.fromisoformat(r['started_utc'])<now and datetime.fromisoformat(r['finished_utc'])>start)
merged=[]
for lo,hi in intervals:
    if merged and lo<=merged[-1][1]:merged[-1]=(merged[-1][0],max(merged[-1][1],hi))
    else:merged.append((lo,hi))
measured_intervals=sum((hi-lo).total_seconds() for lo,hi in merged)
gross=(now-start).total_seconds()
result={'created_utc':datetime.now(timezone.utc).isoformat(),'measured_sha':expected_sha,'validated_controls':controls,'scope':'Marginal bounded oracle evidence on existing ports and reused hosts. Syntactic routes are scenario proxies, not measured homogeneous populations. No complete-function or statistical-confidence claim.','rows':rows,'classes':classes,'priced_rows':priced,'unpriced_rows':unpriced,'partial_conditional_scenario_hours':partial,'whole_port_total':'UNIDENTIFIED: unpriced rows and new implementation/harness adaptation costs have not been measured.','hypothetical_unpriced_imputations_not_bounds':[{'assumption':'unpriced rows use fastest observed class evidence-cycle rate','hours':partial+unpriced*min(rates)/60},{'assumption':'unpriced rows use slowest observed class evidence-cycle rate','hours':partial+unpriced*max(rates)/60}],'observed_workflow':{'started_utc':start.isoformat(),'endpoint_utc':now.isoformat(),'gross_before_workspace_gates_minutes':gross/60,'gross_divided_by_nine_minutes_explicit_equal_allocation':gross/540,'unique_recorded_command_intervals_minutes':measured_intervals/60,'outside_recorded_intervals_minutes':(gross-measured_intervals)/60,'note':'Includes reading, census, review, repairs, first exploratory run, compilation, controls and reporting so far. Raw phase stamps are not class-authoring stopwatches. Intervals may contain provenance overhead. Full workspace gates and later publication work are separate; this is a stated checkpoint, not total active labor.'},'controls':'All12 control receipts, hashes, source maps and named runtime outcomes validated before analysis:10 of11 selected Rust faults caught; length65 survives; source Map-erasure meta-control caught.','not_a_forecast':'Nine purposive rows from one venue cannot determine class means, error probability, confidence percentage, or a defensible universal 7341-row total.'}
(HERE/'analysis.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({k:v for k,v in result.items() if k in ['classes','partial_conditional_scenario_hours','hypothetical_unpriced_imputations_not_bounds','observed_workflow']},indent=2))
