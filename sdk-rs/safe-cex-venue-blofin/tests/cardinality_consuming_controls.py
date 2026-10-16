"""Temporary shared production SeqAccess drain; never commit a shared-code change.

Consume every retained input element before returning None, defeating Serde's
trailing-input protection. Each native venue target must reject at runtime.
"""
import atexit, pathlib, subprocess, json, sys
MODE=sys.argv[1] if len(sys.argv)>1 else "guarded"
ROOT=pathlib.Path(__file__).resolve().parents[3]
TESTS=ROOT/'sdk-rs/safe-cex-venue-blofin/tests'
path=ROOT/'sdk-rs/safe-cex-num/src/numeric_ingress.rs'
original=path.read_bytes()
anchor='''        self.0
            .next()
            .map(|value| seed.deserialize(JsNumberDeserializer(value.0)))
            .transpose()'''
assert original.decode().count(anchor)==1
mutant=original.decode().replace(anchor,'''        let _ = seed;
        for _ in self.0.by_ref() {} // Consume ALL elements, then falsely report EOF.
        Ok(None)''')
logs=ROOT/f'sdk-rs/target/blofin-cardinality-consuming-{MODE}';logs.mkdir(exist_ok=True)
saved={p:p.read_bytes() for p in [TESTS/'support/cardinality.rs',TESTS/'types.rs',TESTS/'positions.rs',TESTS/'inline_tpsl.rs']}
for p,b in saved.items():atexit.register(p.write_bytes,b)
if MODE=='no-length':
 p=TESTS/'support/cardinality.rs';source=saved[p].decode();a=source.index('        assert_eq!(',source.index('    pub fn native('));b=source.index('\n    }',a);p.write_text(source[:a]+'        let _ = (index, suffix, length);'+source[b:])
 p=TESTS/'types.rs';source=saved[p].decode();block='''                    assert_eq!(
                        data.len(),
                        value["data"].as_array().unwrap().len(),
                        "data_length"
                    );''';assert source.count(block)==4;p.write_text(source.replace(block,''))
allowed = {
 'types': ['BLOFIN-CARDINALITY native row coverage' if MODE=='no-length' else 'BLOFIN-CARDINALITY native length'],
 'income': ['income_to_number_native','income_fill_native','BLOFIN-CARDINALITY native length'],
 'history': ['history_to_number_native','history_optional_native','history_order_native'],
 'utility': ['utility_parsePositivePrice_native','utility_parseOptionalNumber_native','utility_getNormalizedInstType_native','utility_isKnownNonSwapInstrument_native'],
 'symbols': ['symbols_symbolToMarketId_native','symbols_marketIdToSymbol_native'],
 'tickers': ['BLOFIN-CARDINALITY native length','ticker_native_fields'],
 'orders': ['BLOFIN-CARDINALITY native length','orders_native_fields'],
 'positions': ['BLOFIN-CARDINALITY native length','positions_native_fields','positions_warning_trace'],
 'inline_tpsl': ['BLOFIN-CARDINALITY native length','source_inline_original_order_aliases','inline_native_fields'],
}
if MODE=='no-length':
 for target,label in [('positions','positions_alias_census'),('inline_tpsl','inline_order_count')]:
  p=TESTS/(target+'.rs');source=saved[p].decode();name=source.index('"'+label+'"');a=source.rfind('                assert_eq!(',0,name);b=source.index(');',name)+2;assert a>=0 and 'rows.len()' in source[a:b];p.write_text(source[:a]+source[b:])
targets=['types','income','history','utility','symbols','tickers','orders','positions','inline_tpsl']
def run(target):
 return subprocess.run(['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-blofin','--locked','--jobs','1','--test',target,'--','--nocapture'],cwd=ROOT,capture_output=True,text=True)
summary=[]
try:
 path.write_text(mutant)
 for target in targets:
  r=run(target);out=r.stdout+r.stderr;(logs/f'{target}.mutated.log').write_text(out)
  assert r.returncode==101 and 'test result: FAILED' in out and 'panicked at' in out,out
  assert 'trailing array elements' not in out,out
  matched=[name for name in allowed[target] if name in out];assert matched,out
  summary.append({'target':target,'mutated':101,'assertion':matched[0]});print(target,'consuming-empty',101,flush=True)
finally:path.write_bytes(original)
for row in summary:
 r=run(row['target']);(logs/(row['target']+'.restored.log')).write_text(r.stdout+r.stderr);assert r.returncode==0,r.stdout+r.stderr;row['restored']=0;print(row['target'],'RESTORED',0,flush=True)
for p,b in saved.items():p.write_bytes(b)
assert path.read_bytes()==original
(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
