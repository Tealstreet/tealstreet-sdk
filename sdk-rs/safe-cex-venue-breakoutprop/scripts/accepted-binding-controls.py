"""Compile native witnesses for newly indexed binding APIs, then prove failures.

Each witness calls the exact private production API in its owning module. Tests
are injected under cfg(test), with independently authored expectations loaded
from this crate's reviewed contracts; no production body is replaced for a
baseline/restored run. Controls change one production behavior at a time.
All source bytes are restored in finally; run serially with --jobs 1.
"""
from pathlib import Path
import json, os, re, subprocess, sys

crate=Path(__file__).resolve().parents[1]
contracts=json.loads((crate.parent/'safe-cex-parity/accepted-set/contracts/safe-cex-venue-breakoutprop.json').read_text())
prefix='safe_cex_venue_breakoutprop::'
base={f:(crate/'src'/f).read_bytes() for f in ['binding.rs','host.rs','lifecycle.rs']}
extra={f:[] for f in base}

def test(file,name,call,cases,typename='unused'):
    lines=[f'#[test] fn {name}() {{']
    for i,c in enumerate(cases):
        text=json.dumps(c['input'],ensure_ascii=False)
        expected=json.dumps(c['output'],ensure_ascii=False)
        expr=call.replace('$value','input').replace('$type','super::'+typename)
        lines.append(f'let input:serde_json::Value=serde_json::from_str(r####"{text}"####).unwrap();')
        lines.append(f'let expected:serde_json::Value=serde_json::from_str(r####"{expected}"####).unwrap();')
        lines.append(f'let actual={expr};assert_eq!(actual,expected,"{name} case{i} actual production output");')
    lines.append('}')
    extra[file].append('\n'.join(lines))

for name in ['decoded','native_call']:
    row=contracts[prefix+'binding::'+name]
    test('binding.rs','native_'+name,row['call'],row['cases'])
for name in ['PollingBase','ProductionHost']:
    row=contracts[prefix+'host::'+name]
    if len(row['entrypoints'])==1:
        call,cases=row['call'],row['cases']
    else:
        e=next(e for e in row['entrypoints'] if e['input']=='observed');call,cases=e['call'],e['cases']
    test('host.rs','native_'+name.lower(),call,cases,name)
for name in ['fetch_positions','fetch_orders']:
    row=contracts[prefix+'lifecycle::BreakoutpropExchange']
    e=next(e for e in row['entrypoints'] if e['api']['name']==name)
    test('lifecycle.rs','native_'+name,e['call'],e['cases'],'BreakoutpropExchange')
report_call='''{ #[allow(dead_code)] mod fixture { use crate as safe_cex_venue_breakoutprop; include!(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/support/accepted_binding.rs")); } fixture::accepted_report(&$value, |exchange,message| super::report(exchange,message)) }'''
test('binding.rs','native_report_effects',report_call,[{'input':{'message':'native-report 雪','disposed':False},'output':{'ok':True,'events':[['Exchange Error: native-report 雪',{'suppressConsoleLog':False,'sendToSentry':True,'sentryData':{'tags':{'exchange':'breakoutprop','module':'Exchange','severity':'error'},'extra':{'originalMessage':'Exchange Error: native-report 雪','exchangeOptions':{}},'error':{'name':'ExchangeError','message':'Exchange Error: native-report 雪'}}}]],'reports':0,'http':0}},{'input':{'message':'suppressed','disposed':True},'output':{'ok':True,'events':[],'reports':0,'http':0}}])
add={f:'\n#[cfg(test)] mod __accepted_binding_owner_controls {\n'+'\n'.join(rows)+'\n}\n' for f,rows in extra.items()}
env={**os.environ,'RUSTUP_TOOLCHAIN':'1.99.0','RUSTC_BOOTSTRAP':'1'}
cmd=['cargo','+1.99.0','test','--locked','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-breakoutprop','--lib','--jobs','1']

def run(label,filter,fail=False):
    r=subprocess.run(cmd+[filter],env=env,capture_output=True,text=True)
    out=r.stdout+r.stderr
    print(label,flush=True)
    print(out,flush=True)
    if fail:
        assert r.returncode==101 and 'test result: FAILED' in out and re.search(r'^test \S*::'+re.escape(filter)+r' \.\.\. FAILED$',out,re.MULTILINE),(label,'no named runtime failure')
    else:
        assert r.returncode==0,(label,r.returncode)
        expected=(['native_decoded','native_native_call','native_pollingbase','native_productionhost','native_fetch_positions','native_fetch_orders','native_report_effects']
                  if filter=='__accepted_binding_owner_controls::native_' else [filter])
        for name in expected:
            assert re.search(r'^test \S*::'+re.escape(name)+r' \.\.\. ok$',out,re.MULTILINE),(label,'missing named restored/baseline pass',name)

def put(file,text): (crate/'src'/file).write_text(text+add[file])
def replace_once(s,a,b):
    assert s.count(a)==1,(a,s.count(a));return s.replace(a,b,1)

try:
    for f,b in base.items():put(f,b.decode())
    run('all seven actual owner native witness baselines','__accepted_binding_owner_controls::native_')
    if '--baseline-only' not in sys.argv:
        controls=[
            ('binding.rs','native_decoded',lambda s:replace_once(s,'to_typed(value).map_err(Into::into)','{let _decode=to_typed::<T>;let _=value;Err(ExchangeError::new("CONTROL decoded"))}')),
            ('binding.rs','native_native_call',lambda s:replace_once(s,'"fetchPositions" => {','"CONTROL_fetchPositions" => {')),
            ('host.rs','native_pollingbase',lambda s:replace_once(s,'self.core.cleanup_message_handler_stats(topic)','{let _=topic;Ok(())}')),
            ('host.rs','native_productionhost',lambda s:replace_once(s,'            handlers,\n','            handlers: {let _=handlers; Rc::default()},\n')),
        ]
        def no_report(s):
            start=s.index('    exchange.base.emit_and_log_error(',s.index('fn report('));end=s.index('\n}',start)
            return s[:start]+'    let _=(exchange,message); Ok(())'+s[end:]
        controls.append(('binding.rs','native_report_effects',no_report))
        for file,name,mutate in controls:
            original=base[file].decode()
            put(file,mutate(original))
            run('CONTROL '+name,name,True)
            put(file,original)
            run('RESTORED '+name,name)
finally:
    for f,b in base.items():(crate/'src'/f).write_bytes(b)
    assert all((crate/'src'/f).read_bytes()==b for f,b in base.items())
    print('Exact source bytes restored for all injected witnesses/controls',flush=True)
