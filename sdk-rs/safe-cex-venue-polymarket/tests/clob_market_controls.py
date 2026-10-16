"""CLOB wire identity and consuming-cardinality controls, one Node corpus per run."""
from pathlib import Path
import json
import os
import subprocess

crate = Path(__file__).resolve().parents[1]
rust = crate/'src/types.rs'
shared = crate.parent/'safe-cex-num/src/numeric_ingress.rs'
harness = crate/'tests/auth_types.rs'
oracle = crate/'tests/auth_types_oracle.cjs'
ts = crate.parents[1]/'packages/safe-cex/src/exchanges/polymarket/polymarket.types.ts'
logs = Path(os.environ['POLYMARKET_CONTROL_LOG_DIR']); logs.mkdir(parents=True, exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')

def replace(old, new):
    def change(text):
        assert text.count(old) == 1, old
        return text.replace(old,new)
    return change

def swap(first, second):
    def change(text):
        start = text.index('pub struct PolymarketClobMarket {')
        prefix, region = text[:start], text[start:]
        one, two = f'{first}: fields.{first},', f'{second}: fields.{second},'
        assert region.count(one)==1 and region.count(two)==1
        return prefix+region.replace(one,f'{first}: fields.{second},').replace(two,f'{second}: fields.{first},')
    return change

def consume(mode):
    def change(text):
        start = text.index("impl<'de> SeqAccess<'de> for Array {")
        a = text.index('        self.0\n            .next()', start)
        b = text.index('\n    }', a)
        if mode == 'empty':
            body = '        let _seed = seed;\n        self.0.by_ref().for_each(drop);\n        Ok(None)'
        else:
            body = '        let first = self.0.next();\n        self.0.by_ref().for_each(drop);\n        first.map(|value| seed.deserialize(JsNumberDeserializer(value.0))).transpose()'
        return text[:a]+body+text[b:]
    return change

controls = []
for name, a, b, assertion in [
    ('ids','condition_id','token_id','native_clob.condition_id'),
    ('outcome','outcome','token_id','native_clob.token_id'),
    ('steps','minimum_order_size','minimum_tick_size','native_clob.minimum_order_size'),
    ('flags','active','closed','native_clob.active'),
]:
    controls.append(('clob-swap-'+name, assertion, [(rust,swap(a,b))]))
for mode in ['empty','subset']:
    controls.append(('clob-consume-'+mode,'native_clob.batch_length',[(shared,consume(mode))]))
controls.append(('clob-consume-empty-bypass-lengths','native_clob.sample_comparison_count',[
    (shared,consume('empty')),
    (harness,lambda s: replace('    assert_eq!(sample.len(), 2, "clob_source_sample_count");\n','')(replace('    assert_eq!(native.len(), sample.len(), "native_clob.batch_length");\n','')(s)))
]))
controls.append(('clob-source-cases-empty','clob_string_case_count',[(oracle,replace('  process.stdout.write(', '  clobCases.length = 0;\n  process.stdout.write('))]))
controls.append(('clob-source-sample-subset','clob_source_sample_count',[(oracle,replace('  process.stdout.write(', '  clobSample.pop();\n  process.stdout.write('))]))

def drift(text):
    a=text.index('export interface PolymarketClobMarket {');b=text.index('\n}',a)
    part=text[a:b];assert 'closed: boolean;' in part
    return text[:a]+part.replace('closed: boolean;','closed: number;')+text[b:]
controls.append(('clob-source-schema-drift','declared field closed',[(ts,drift)]))
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','auth_types','--locked','--jobs','1','--','clob_market_record_actual_typescript','--nocapture']

def run(name):
    result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (logs/(name+'.log')).write_text(f'exit_code={result.returncode}\n'+result.stdout)
    assert result.stdout.count('polymarket auth oracle: one Node invocation for the entire corpus')==1,result.stdout
    return result

assert run('clob-baseline').returncode==0
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],cwd=crate,text=True).strip(),'controls':[]}
for name, assertion, changes in controls:
    originals={p:p.read_text() for p,_ in changes}
    try:
        for p,change in changes:
            changed=change(originals[p]); assert changed!=originals[p],name;p.write_text(changed)
        result=run(name+'-mutation')
        assert result.returncode==101 and assertion in result.stdout,result.stdout
    finally:
        for p,original in originals.items():p.write_text(original)
    restored=run(name+'-restored');assert restored.returncode==0,restored.stdout
    summary['controls'].append({'name':name,'mutation_exit':result.returncode,'assertion':assertion,'restored_exit':restored.returncode})
    (logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
    print(f'{name}: 101 {assertion}; restored 0',flush=True)
