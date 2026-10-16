"""Two production controls and one test-boundary overlay, restored after each."""
from pathlib import Path
import os,subprocess,json,sys

source=Path('sdk-rs/safe-cex-venue-woo/src/runtime_selector.rs')
original=source.read_bytes()
text=original.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
artifact=Path(sys.argv[1]) if len(sys.argv)>1 else Path('/tmp/woo-runtime-plan-controls.json')
env=dict(os.environ, SAFE_CEX_TYPESCRIPT_PATH='/Users/sam/cs/tealstreet-next/node_modules/typescript')
env['PATH']='/opt/homebrew/opt/rustup/bin:'+env['PATH']
cmd=['cargo','test','--manifest-path','sdk-rs/Cargo.toml','-p','safe-cex-venue-woo','--test','runtime_selector','--jobs','2']
# Paired decoder/text corruption preserves the strings but swaps native enum meaning.
swapped=text.replace('#[serde(rename = "markets")]','#[serde(rename = "__probe__")]').replace('#[serde(rename = "tickers")]','#[serde(rename = "markets")]').replace('#[serde(rename = "__probe__")]','#[serde(rename = "tickers")]')
swapped=swapped.replace('Self::Markets => "markets"','Self::Markets => "tickers"').replace('Self::Tickers => "tickers"','Self::Tickers => "markets"')
# Swap both distinct shadow paths at their real selector construction call sites.
start = text.index('authoritative: native_target', text.index('WooV3RuntimeRouteSelection::ShadowRead('))
end = text.index('store_authority:', start)
before = text[start:end]
assert text.count(before)==1, 'shadow path mutation call site'
after=before.replace('&route.legacy_path','TEMP_PROBE_PATH').replace('v3_path','&route.legacy_path').replace('TEMP_PROBE_PATH','v3_path')
controls=[('symmetric-route-id-swap',swapped,'typed native decoded route ID discriminant markets'),('shadow-target-path-swap',text.replace(before,after),'typed native shadow authoritative path')]
report={'gate_sha':sha,'controls':[]}
for name,broken,assertion in controls:
    assert broken!=text
    entry={'name':name,'native_assertion':assertion,'command':cmd}
    try:
        source.write_text(broken)
        run=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        entry.update(mutated_exit=run.returncode,mutated_output=run.stdout)
        print(name,'mutated',run.returncode,flush=True)
        assert run.returncode==101 and 'test result: FAILED' in run.stdout and assertion in run.stdout,run.stdout
    finally:
        source.write_bytes(original)
        assert source.read_bytes()==original
    run=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    entry.update(restored_exit=run.returncode,restored_output=run.stdout)
    report['controls'].append(entry)
    artifact.write_text(json.dumps(report,indent=2))
    print(name,'restored',run.returncode,flush=True)
    assert run.returncode==0,run.stdout
print('production controls complete at iteration 2',flush=True)

# Test-ingress overlay: simulate a lossy substitution before the shared native parser.
# This is a boundary fixture control, not a production-type implementation mutation.
tests=Path('sdk-rs/safe-cex-venue-woo/tests/runtime_selector.rs')
test_original=tests.read_bytes()
test_text=test_original.decode()
before='serde_json::from_str::<Value>(row["wire"].as_str().unwrap()).is_err()'
after=r'serde_json::from_str::<Value>(&row["wire"].as_str().unwrap().replace("\\ud800", "\\ufffd")).is_err()'
assert test_text.count(before)==1
name='lossy-surrogate-boundary-overlay'
assertion='native whole-record surrogate representation refusal target'
entry={'name':name,'mutation_kind':'test-ingress overlay; not production code','call_site':str(tests)+'::declared_record_domains_observe_surrogates_and_native_whole_record_refusal','native_assertion':assertion,'command':cmd}
try:
    tests.write_text(test_text.replace(before,after))
    run=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    entry.update(mutated_exit=run.returncode,mutated_output=run.stdout)
    print(name,'mutated',run.returncode,flush=True)
    assert run.returncode==101 and 'test result: FAILED' in run.stdout and assertion in run.stdout,run.stdout
finally:
    tests.write_bytes(test_original)
    assert tests.read_bytes()==test_original
run=subprocess.run(cmd,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
entry.update(restored_exit=run.returncode,restored_output=run.stdout)
report['controls'].append(entry)
artifact.write_text(json.dumps(report,indent=2))
print(name,'restored',run.returncode,flush=True)
assert run.returncode==0,run.stdout
print('stopped after iteration 3: two production controls, one test-boundary overlay; no survivors',flush=True)
