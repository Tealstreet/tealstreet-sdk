"""Compile shortened test walks: independent executed-comparison counters must reject."""
import pathlib,subprocess,json
ROOT=pathlib.Path(__file__).resolve().parents[3]
TESTS=ROOT/'sdk-rs/safe-cex-venue-blofin/tests'
logs=ROOT/'sdk-rs/target/blofin-cardinality-counters';logs.mkdir(exist_ok=True)
sites=[(name,'ingress','for path in 0..3 {','for path in 0..2 {') for name in ['income','history','utility','symbols','tickers','orders','positions','inline_tpsl']]
sites += [('types','cases','for i in 0..pin.count("/cases") {','for i in 1..pin.count("/cases") {'),('types','fields','for row in 0..pin.case_count(i, "/value/data") {','for row in 0..0 {'),('orders','aliases','for index in 0..pin.case_count(i, "/aliases") {','for index in 0..0 {'),('inline_tpsl','generated','for n in input.len()..pin.case_count(i, "/result/ok") {','for n in input.len()..input.len() {'),('inline_tpsl','prefix','for n in 0..pin.case_count(i, "/prefixAliases") {','for n in 0..0 {'),('inline_tpsl','info','for n in 0..pin.case_count(i, "/infoAliases") {','for n in 0..0 {')]
sites.append(('utility','index','pin.compared(i, path);','pin.compared(0, path);'))
summary=[]
for target,label,old,new in sites:
 p=TESTS/(target+'.rs');original=p.read_bytes();assert old in original.decode()
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(ROOT/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-blofin','--locked','--jobs','1','--test',target,'--','--nocapture']
 try:
  p.write_text(original.decode().replace(old,new));r=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True);output=r.stdout+r.stderr
  (logs/f'{target}.{label}.mutated.log').write_text(output)
  assert r.returncode==101 and 'panicked at' in output and 'BLOFIN-CARDINALITY executed comparisons' in output,output
 finally:p.write_bytes(original)
 r=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True);(logs/f'{target}.{label}.restored.log').write_text(r.stdout+r.stderr);assert r.returncode==0,r.stdout+r.stderr
 summary.append({'target':target,'loop':label,'mutated':101,'restored':0});(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');print(target,label,101,0,flush=True)
