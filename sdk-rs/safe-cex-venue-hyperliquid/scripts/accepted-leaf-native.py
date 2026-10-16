#!/usr/bin/env python3
"""Execute the first 37 contracts as native evidence, without accepted-set permission.

The ordinary gate still owns compiler, namespace, local, alias and dependency
coverage. This finite witness cannot clear those obligations. Temporary tests
call production functions and project native fields, then restore source bytes.
"""
import json, os, subprocess
from pathlib import Path
root=Path(__file__).resolve().parents[2]
rows=json.loads((root/'safe-cex-parity/accepted-set/contracts/safe-cex-venue-hyperliquid.json').read_text())
assert len(rows)==37, 'Review the finite cohort before extending this witness.'
saved={}; additions={}; count=0
for i,(name,row) in enumerate(rows.items()):
    parts=name.split('::')
    assert len(parts)==3 and parts[0]=='safe_cex_venue_hyperliquid',name
    module,ident=parts[1:]
    source=root/f'safe-cex-venue-hyperliquid/src/{module}.rs'
    saved.setdefault(source,source.read_bytes())
    fixture=row.get('nativeObservation',row)
    cases=fixture.get('cases',[])
    typ='super::'+row.get('instantiation',ident)
    call=fixture.get('call','').replace('$value','__hl_native_input').replace('$type',typ)
    if row.get('category')=='non-decoder':
        assert ident=='format_hyperliquid_cloid'
        body='assert_eq!(super::format_hyperliquid_cloid([0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]), "0x000102030405060708090a0b0c0d0e0f");'
    else:
        assert call and cases,name
        count+=len(cases)
        body=f'fn observe(__hl_native_input:&serde_json::Value)->serde_json::Value {call}\nlet mut failures=Vec::new();'
        for j,case in enumerate(cases):
            a=json.dumps(case['input'],ensure_ascii=False);b=json.dumps(case['output'],ensure_ascii=False)
            body+=f'\n{{let value:serde_json::Value=serde_json::from_str(r####"{a}"####).unwrap();let expected:serde_json::Value=serde_json::from_str(r####"{b}"####).unwrap();let actual=observe(&value);if actual!=expected {{failures.push(format!("case{j}: expected {{expected:?}}, actual {{actual:?}}"));}}}}'
        body+='\nassert!(failures.is_empty(),"direct native expectations: {failures:?}");'
    additions.setdefault(source,[]).append(f'\n#[cfg(test)] mod __hl_review_native_{i} {{#[test] fn {ident.lower()}() {{{body}}}}}\n')
print(f'Direct tests: {len(rows)} owners, {count} independently authored cases plus formatter; formal local/dependency/matrix obligations remain unresolved.',flush=True)
try:
    for p,extra in additions.items():p.write_bytes(saved[p]+''.join(extra).encode())
    result=subprocess.run(['cargo','+1.99.0','test','--locked','-p','safe-cex-venue-hyperliquid','--jobs','1','--lib','__hl_review_native_','--','--nocapture'],cwd=root,env={**os.environ,'RUSTUP_TOOLCHAIN':'1.99.0','RUSTC_BOOTSTRAP':'1'})
    print(f'Direct native exit {result.returncode}; no accepted-set clearance claimed.',flush=True)
finally:
    for p,b in saved.items():p.write_bytes(b)
    assert all(p.read_bytes()==b for p,b in saved.items())
    print('Every temporary cfg(test) module restored byte-for-byte.',flush=True)

raise SystemExit(result.returncode)
