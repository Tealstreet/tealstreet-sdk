#!/usr/bin/env python3
"""Five independent compiled faults through actual native factory bindings."""
from pathlib import Path
import subprocess
import sys
if not __debug__:
    raise SystemExit('Controls require Python assertions; do not use -O')
workspace=Path(__file__).resolve().parents[2]
source=workspace/'safe-cex-venue-binance/src/api_factory.rs'
original=source.read_bytes()
logs=workspace/'target/api-factory-controls'
logs.mkdir(parents=True,exist_ok=True)
factory='api_factory_binding::native_factory_retries_unsigned_listen_post_and_encodes_signed_query'
auth='private_owner_binding::native_spot_auth_uses_actual_parent_clock_secret_and_socket'
controls=[
 ('query','serialize','private_classes',factory,b'\n let _=(self,params); String::new()\n'),
 ('retry','matches_read','private_classes',factory,b'\n let _=(self,method,url); false\n'),
 ('endpoint','endpoint','private_classes',factory,b'\n let _=(table,key); "/not-listen-key"\n'),
 ('optional','optional','json_error_binding','futures_leverage_error_messages_keep_raw_json_shape',b'\n let _=(value,key); Err(type_error("broken optional"))\n'),
 ('hmac','utf8_hmac_sha256_hex','private_classes',auth,b''),
]
def run(target,test,label):
    r=subprocess.run(['cargo','test','--locked','--jobs','2','-p','safe-cex-venue-binance','--features','native','--test',target,test,'--','--exact'],cwd=workspace,capture_output=True,text=True,timeout=1200)
    output=r.stdout+r.stderr
    (logs/(label+'.log')).write_text(output)
    return r.returncode,output
assert len(sys.argv)<=2
chosen=None if len(sys.argv)==1 else sys.argv[1].removeprefix('--only=')
assert chosen is None or (sys.argv[1].startswith('--only=') and chosen in [r[0] for r in controls])
try:
 for name,function,target,test,replacement in controls:
    if chosen is not None and name != chosen: continue
    assert source.read_bytes()==original
    marker=('fn '+function+'(').encode();assert original.count(marker)==1,function
    start=original.index(b'{',original.index(marker));end=start+1;depth=1
    while depth:
        depth+=(original[end]==123)-(original[end]==125);end+=1
    if name == "hmac":
        body=original[start+1:end-1]
        before=b"safe_cex_crypto::Encoding::Hex"
        assert body.count(before)==1
        replacement=body.replace(before,b"safe_cex_crypto::Encoding::Base64")
    source.write_bytes(original[:start+1]+replacement+original[end-1:])
    code,output=run(target,test,name+'-red')
    assert code==101 and f'test {test} ... FAILED' in output and 'could not compile' not in output,output[-8000:]
    print(name+': compiled named real-native RED',flush=True)
    source.write_bytes(original);assert source.read_bytes()==original
    code,output=run(target,test,name+'-restored')
    assert code==0 and f'test {test} ... ok' in output and '1 passed; 0 failed' in output,output[-8000:]
    print(name+': byte-exact restored real-native GREEN',flush=True)
finally:
 source.write_bytes(original)
