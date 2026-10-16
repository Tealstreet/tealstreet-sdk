"""One compiled discriminating control per newly ported auth/init row."""
from pathlib import Path
import json, os, re, subprocess
crate=Path(__file__).resolve().parents[1];source=crate/'src/auth_init.rs'
logs=Path(os.environ['POLYMARKET_AUTH_INIT_CONTROL_LOG_DIR']);logs.mkdir(parents=True,exist_ok=True)
assert os.environ.get('SAFE_CEX_TYPESCRIPT_PATH')
assert not subprocess.check_output(['git','status','--porcelain','--',str(crate),str(crate.parent/'parity/manifest/exchanges/polymarket'),str(crate.parent.parent/'packages/safe-cex/src/exchanges/polymarket')],text=True).strip(),'commit all inputs before controls'
original=source.read_text()
def span(name):
 match=re.search(r'\bfn '+re.escape(name)+r'[<(]',original);assert match,name
 start=match.start();begin=original.index('{',start);end=begin+1;depth=1
 while depth:
  if original[end]=='{':depth+=1
  if original[end]=='}':depth-=1
  end+=1
 return start,end
controls=[('createWallet', 'wallet_native', 'create_wallet', 'vec![private_key]', 'vec![{ let _=private_key;host.text("wrong-key") }]', 'native text wallet private key'), ('createSignedOrder', 'signed_order_native', 'create_signed_order', None, None, 'native constructor captured before parameter getter'), ('generateHmacSignature', 'hmac_native', 'generate_hmac_signature', 'host.uppercase(method)?', 'method', 'native independent UTF8 message bytes'), ('buildL2Headers', 'headers_native', 'build_l2_headers', '("POLY_ADDRESS", address)', '("POLY_ADDRESS", { let _=address;host.field(&credentials,"apiKey")? })', 'native text L2 wallet address'), ('signApiKeyDerivationMessage', 'auth_signature_native', 'sign_api_key_derivation_message', 'host.text("ClobAuthDomain")', 'host.text("Wrong")', 'native text auth domain name'), ('apiConstructor', 'api_constructor_native', 'initialize_api', '("timeout", host.number(30000.0))', '("timeout", host.number(30001.0))', 'native bits client timeout'), ('privateConstructor', 'private_constructor_native', 'initialize_private', 'host.assign(&receiver, "getCredentials", get_credentials)?;', 'let _=get_credentials;host.assign(&receiver,"getCredentials",host.undefined())?;', 'native private credential callback alias'), ('publicConstructor', 'public_constructor_native', 'initialize_public', 'vec![parent]', 'vec![{ let _=parent;host.undefined() }]', 'native original public/private parent alias')]
assert len(controls)==8 and controls[-1][0]=="publicConstructor"
summary={'sha':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'declared_controls':8,'floor':'baseline only','controls':[]}
def run(label,test=None):
 cmd=['/opt/homebrew/opt/rustup/bin/cargo','test','--manifest-path',str(crate.parent/'Cargo.toml'),'-p','safe-cex-venue-polymarket','--test','auth_init','--locked','--jobs','2','--']
 if test:cmd.extend(['--exact',test])
 cmd.append('--nocapture');r=subprocess.run(cmd,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(logs/(label+'.log')).write_text('exit_code='+str(r.returncode)+'\n'+r.stdout);return r
baseline=run('baseline');summary['baseline_exit']=baseline.returncode;(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n');assert baseline.returncode==0 and '8 passed; 0 failed' in baseline.stdout,baseline.stdout
for index,(name,test,owner,old,new,assertion) in enumerate(controls,1):
 begin,end=span(owner);block=original[begin:end]
 if name=='createSignedOrder':
  capture='            let ctor = host.capture_constructor("SDK.OrderBuilder")?;\n'
  anchor='            let funder = host.field(&params, "funderAddress")?;\n'
  assert block.count(capture)==block.count(anchor)==1
  mutated=block.replace(capture,'').replace(anchor,anchor+capture)
 elif name=='apiConstructor':
  assert block.count(old)==2;mutated=block.replace(old,new,1)
 else:
  assert block.count(old)==1,(name,old);mutated=block.replace(old,new)
 try:
  source.write_text(original[:begin]+mutated+original[end:]);broken=run(name+'-mutation',test)
  caught=broken.returncode==101 and assertion in broken.stdout and f'test {test} ... FAILED' in broken.stdout and 'Finished `test` profile' in broken.stdout
 finally:source.write_text(original)
 restored=run(name+'-restored',test);summary['controls'].append({'family':name,'target':'src/auth_init.rs::'+owner,'test':test,'assertion':assertion,'baseline_exit':baseline.returncode,'mutation_exit':broken.returncode,'restored_exit':restored.returncode,'caught':caught});summary['floor']=f"{len(summary['controls'])} executed; stopped after {name}";(logs/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
 assert restored.returncode==0,restored.stdout
 assert caught,f'FINDING survivor or wrong failure at {owner}: {broken.stdout}'
 print(f'{name}: baseline0 / mutation101 ({assertion}) / restored0',flush=True)
assert source.read_text()==original
