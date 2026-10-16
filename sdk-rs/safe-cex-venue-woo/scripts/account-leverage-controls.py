"""One compiling native control per account leverage slice family; serial cargo, exact restoration."""
from pathlib import Path
import subprocess,os,json,argparse
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
crate=Path(__file__).resolve().parents[1];file=crate/'src/v3_account_writes.rs';saved=file.read_bytes();text=saved.decode()
sha=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
command=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-woo','--test','account_leverage','--jobs','2','--offline','--locked']
start=text.index('pub enum WooV3AccountPositionMode {')
end=text.index('impl safe_cex_num',start)
mode=text[start:end]
swapped=mode.replace('rename = "ONE_WAY"','rename = "TEMP"').replace('rename = "HEDGE_MODE"','rename = "ONE_WAY"').replace('rename = "TEMP"','rename = "HEDGE_MODE"')
start=text.index('impl WooV3AccountConfigResponseError {')
end=text.index('// Native exception transport',start)
constructor=text[start:end]
assert constructor.count('            code,')==1
controls=[
 ('declaration-contracts',mode,swapped,'ACCOUNT-LEVERAGE native position mode'),
 ('validators','let n = value.and_then(|v| v.clone().decode::<f64>().ok());','let n = value.and_then(|v| v.clone().decode::<f64>().ok()).map(f64::abs);','ACCOUNT-LEVERAGE native integer'),
 ('get-descriptor','captured_string(mode_text(target.position_mode))','captured_string(mode_text(WooV3AccountPositionMode::OneWay))','ACCOUNT-LEVERAGE native captured string'),
 ('set-descriptor','account.account_mode != "MARGIN"','account.account_mode == "MARGIN"','ACCOUNT-LEVERAGE native set build readiness'),
 ('readback','leverage: long.leverage,','leverage: { let _ = long.leverage; short.leverage },','ACCOUNT-LEVERAGE native detail leverage'),
 ('mutation','timestamp: decode_success(input, "set futures leverage")?,','timestamp: decode_success(input, "set futures leverage")? + 1.0,','ACCOUNT-LEVERAGE native mutation timestamp'),
 ('gateway-error',constructor,constructor.replace('            code,','            code: code.map(|v|v+1.0),'),'ACCOUNT-LEVERAGE native gateway code'),
 ('metadata','runtime_imported: false,','runtime_imported: true,','ACCOUNT-LEVERAGE native runtime imported'),
]
parser=argparse.ArgumentParser()
parser.add_argument('--start-at',choices=[c[0] for c in controls],default=controls[0][0])
args=parser.parse_args()
selected=controls[next(i for i,c in enumerate(controls) if c[0]==args.start_at):]
results=[]
for name,old,new,assertion in selected:
 try:
  assert text.count(old)==1 and old!=new,(name,old)
  file.write_text(text.replace(old,new))
  run=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  Path('/tmp/woo-account-leverage-'+name+'.log').write_text(run.stdout)
  assert run.returncode==101 and 'panicked at' in run.stdout and assertion in run.stdout,run.stdout
  print(name+' named failure exit 101: '+assertion,flush=True)
 finally:file.write_bytes(saved)
 restored=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
 Path('/tmp/woo-account-leverage-'+name+'-restored.log').write_text(restored.stdout)
 assert restored.returncode==0,restored.stdout
 results.append({'family':name,'old':old,'new':new,'assertion':assertion,'exit':run.returncode,'restored_exit':restored.returncode,'call_site':'safe-cex-venue-woo/src/v3_account_writes.rs:'+str(text[:text.index(old)].count('\n')+1)})
 print(name+' restored exit 0',flush=True)
Path('/tmp/woo-account-leverage-controls.json').write_text(json.dumps({'sha':sha,'command':command,'controls':results,'start_at':args.start_at,'iterations':len(selected),'survivors':0},indent=2)+'\n')
