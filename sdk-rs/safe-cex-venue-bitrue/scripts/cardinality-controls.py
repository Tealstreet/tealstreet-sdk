"""Sequential empty/subset controls. One Node process per scoped test corpus."""
from pathlib import Path
import os, subprocess, sys
root=Path(__file__).resolve().parents[3]
venue=root/'sdk-rs/safe-cex-venue-bitrue'
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-bitrue','--locked','--jobs','1']
def prove(label,target,edits,assertion):
 if len(sys.argv)>1 and label not in sys.argv[1:]:return
 originals={p:p.read_text() for p,_,_ in edits}
 try:
  for p,old,new in edits:
   text=p.read_text();assert text.count(old)==1,(label,old);p.write_text(text.replace(old,new))
  r=subprocess.run(command+['--test',target],capture_output=True,text=True);output=r.stdout+r.stderr
  assert r.returncode==101 and assertion in output and 'could not compile' not in output,output
  print('FAIL101 PROVED: '+label+' -> '+assertion,flush=True)
 finally:
  for p,text in originals.items():p.write_text(text)
 r=subprocess.run(command+['--test',target],capture_output=True,text=True)
 assert r.returncode==0,r.stdout+r.stderr
 print('RESTORED PASS: '+label,flush=True)
# Captures are altered only inside the test adapter. Oracle files/bytes stay fixed.
for target in ['accounts','balance','leverage','read_helpers','utils','wire','tables','configuration']:
 p=venue/'tests'/(target+'.rs');text=p.read_text()
 marker=next(line for line in text.splitlines() if line.strip().startswith('source_cardinality('))
 for label,operation in [('empty','clear()'),('subset','pop()')]:
  if target in ['wire','tables']:
   name='cases' if target=='wire' else 'rows'
   injected=f'    let {name} = {{ let mut changed = {name}; changed.{operation}; changed }};\n'
  else:
   name='actual' if target=='configuration' else 'capture'
   key='BASE_WS_URL' if target=='configuration' else 'updates' if target=='leverage' else 'rows'
   injected=f'    let {name} = {{ let mut changed = {name}; changed["{key}"].as_array_mut().unwrap().{operation}; changed }};\n'
  prove(target+' source '+label,target,[(p,marker,injected+marker)],'BITRUE-SOURCE-CARDINALITY total/per-case: '+target)
# Real production Store accessor. Restore the shared source after EVERY control.
store=root/'sdk-rs/safe-cex-store/src/store.rs'
old="pub fn markets(&self) -> Ref<'_, [Market]> {\n        Ref::map(self.state.borrow(), |state| state.markets())\n    }"
for label,expression in [('empty','&state.markets()[..0]'),('subset','&state.markets()[..state.markets().len().min(1)]')]:
 new=old.replace('state.markets())',expression+')')
 prove('production Store markets '+label,'leverage',[(store,old,new)],'BITRUE-LEVERAGE-CARDINALITY retained markets')
# Remove BOTH source-count layers, then drop expected rows: native length is
# independently checked and cannot be rescued by accepting the changed census.
p=venue/'tests/leverage.rs';text=p.read_text()
marker='    source_cardinality(&capture, "leverage");'
old_count='''        assert_eq!(
            expected_markets.len(),
            2,
            "BITRUE-SOURCE-CARDINALITY markets per case"
        );'''
# Keep the source census call load-bearing/used but move it before test-only
# expected-output corruption. The per-case count is bypassed explicitly.
for label,operation in [('empty','clear()'),('subset','pop()')]:
 injected=marker+f'\n    let capture = {{ let mut changed = capture; changed["updates"][0]["markets"].as_array_mut().unwrap().{operation}; changed }};'
 prove('expected markets '+label+' after source census bypass','leverage',[(p,marker,injected),(p,old_count,'')],'BITRUE-LEVERAGE-CARDINALITY retained markets')

# Removing entire cases after accepting the source census must still fail the
# independently asserted number of comparisons, even when no pair is visited.
for label,operation in [('empty','clear()'),('subset','pop()')]:
 injected=marker+f'\n    let capture = {{ let mut changed = capture; changed["updates"].as_array_mut().unwrap().{operation}; changed }};'
 prove('update cases '+label+' after source census bypass','leverage',[(p,marker,injected)],'BITRUE-NATIVE-CARDINALITY executed comparisons: leverage')
