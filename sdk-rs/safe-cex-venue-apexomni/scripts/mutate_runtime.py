"""One native result or operation mutation per newly implemented venue row."""
from pathlib import Path
import subprocess, re
root=Path(__file__).resolve().parents[3]
crate=root/'sdk-rs/safe-cex-venue-apexomni'
logs=root/'sdk-rs/target/apexomni-controls';logs.mkdir(parents=True,exist_ok=True)
controls=[]
def control(file,before,after,test,label): controls.append((crate/'src'/file,before,after,test,label))
control('utils.rs','safe_cex_crypto::Encoding::Base64','safe_cex_crypto::Encoding::Hex','hmac_sha256_native','hmacSha256Base64 native signature')
control('exchange_utils.rs','safe_cex_crypto::sha256(String::from_utf16_lossy(client_order_id).as_bytes())','{ let mut digest = safe_cex_crypto::sha256(String::from_utf16_lossy(client_order_id).as_bytes()); digest[0] ^= 1; digest }','hash_client_order_id_native','hashClientOrderId native digest bytes')
control('api.rs','serialize(payload, false)','serialize(payload, true)','serialize_primary_native','serializeApexPayload native result')
control('api.rs','serialize(payload, true)','serialize(payload, false)','serialize_legacy_native','serializeApexPayloadLegacy native result')
control('exchange_utils.rs','math(decimal_multiply(number, scale))? / scale','math(decimal_multiply(number, scale))? * scale','fee_helper_native','feeHelper native result')
control('zklink_host.rs','if !self.initialized.get()','if self.initialized.get()','initialization_native','initSdk operations')
originals={file:file.read_text() for file,*_ in controls}
# Simultaneous changes must match the original source, never each other's replacement.
mutants=dict(originals)
for file in originals:
 changes=[(before,after)for target,before,after,_,_ in controls if target==file]
 for index,(before,after)in enumerate(changes):
  assert originals[file].count(before)==1,(file,before)
  mutants[file]=mutants[file].replace(before,f'RUNTIME_MUTATION_{index}')
 for index,(_,after)in enumerate(changes):mutants[file]=mutants[file].replace(f'RUNTIME_MUTATION_{index}',after)
cmd=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-venue-apexomni','--test','pure','--test','signing','--jobs','2','--','--test-threads=1']
try:
 for file,text in mutants.items():file.write_text(text)
 result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr
 (logs/'runtime-mutation.log').write_text(output)
 assert result.returncode==101 and 'test result: FAILED.' in output,output
 for _,_,_,test,label in controls:
  # Cargo stops after the first failed binary; initialization has a separate target below.
  if test=='initialization_native':continue
  assert f'test {test} ... FAILED' in output and label in output,(test,label,output)
 print('5 pure controls: exit 101; each named native assertion failed',flush=True)
 result=subprocess.run(cmd[:cmd.index('--test')]+['--test','signing','--jobs','2','initialization_native'],cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr
 (logs/'runtime-init-mutation.log').write_text(output)
 assert result.returncode==101 and 'initSdk operations' in output and 'test result: FAILED.' in output,output
 print('initSdk control: exit 101; named native operations assertion failed',flush=True)
finally:
 for file,text in originals.items():file.write_text(text)
result=subprocess.run(cmd,cwd=root,capture_output=True,text=True);output=result.stdout+result.stderr
(logs/'runtime-restored.log').write_text(output);assert result.returncode==0,output
print('pure/signing restored: exit 0',flush=True)
