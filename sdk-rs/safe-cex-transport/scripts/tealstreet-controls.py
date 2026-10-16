#!/usr/bin/env python3.11
"""Serial compiled negative controls; restore exact source bytes after each attempt."""
from pathlib import Path
import os,json,subprocess,hashlib
root=Path(__file__).resolve().parents[3]
sha=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
logs=root/'sdk-rs/target/tealstreet-controls';logs.mkdir(parents=True,exist_ok=True)
env=dict(os.environ);env.setdefault('SAFE_CEX_TYPESCRIPT_PATH','/Users/sam/cs/tealstreet-next/node_modules/typescript')
controls=[
 ('blofin-exception','tealstreet.rs','matches!(context.exchange, ExchangeName::Grvt | ExchangeName::Blofin)','matches!(context.exchange, ExchangeName::Grvt)','native_provider_selection_matches_actual_source','TRANSPORT native REST URL'),
 ('empty-token','tealstreet.rs','if !token.is_empty() {','if true {','native_authorization_empty_tokens_headers_identity_and_adapter_dispatch','TRANSPORT native authorization headers including empty credential'),
 ('undefined-proxy','transport_resolution.rs','} else {\n        metadata.enforce_ws_tunnel\n    };','} else {\n        Some(metadata.enforce_ws_tunnel.unwrap_or(false))\n    };','native_preferences_preserve_undefined_final_operands_and_region_order','TRANSPORT native optional proxy final operand'),
 ('signed-rtt','tealstreet_time_source.rs','((received_at - requested_at) / 2.0).floor()','((received_at - requested_at) / 2.0).trunc()','native_time_source_starts_eagerly_and_floors_rtt_before_parsing','TRANSPORT native negative RTT floors'),
 ('compiled-route','routing_generated.rs','rest_proxy_host: "",','rest_proxy_host: "https://deliberate-control.invalid",','native_compiled_routing_table_matches_source_channels','TRANSPORT native routing-table REST host'),
]
results=[]
for label,file,needle,mutation,test,assertion in controls:
    path=root/'sdk-rs/safe-cex-transport/src'/file;original=path.read_bytes();source=original.decode()
    assert original==subprocess.check_output(['git','show',sha+':'+path.relative_to(root).as_posix()],cwd=root),file+' must equal committed gate input'
    if label!='compiled-route': assert source.count(needle)==1,(label,source.count(needle))
    else: assert source.count(needle)>0
    command=['cargo','test','--manifest-path',str(root/'sdk-rs/Cargo.toml'),'-p','safe-cex-transport','--test','tealstreet_transport','--jobs','2',test,'--','--exact']
    try:
        path.write_text(source.replace(needle,mutation,1))
        broken=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True)
        output=broken.stdout+broken.stderr;(logs/(label+'-broken.log')).write_text(output)
    finally: path.write_bytes(original)
    restored=subprocess.run(command,cwd=root,env=env,capture_output=True,text=True)
    restored_output=restored.stdout+restored.stderr;(logs/(label+'-restored.log')).write_text(restored_output)
    discriminates=broken.returncode==101 and assertion in output and 'test '+test+' ... FAILED' in output and 'error[E' not in output
    restored_ok=restored.returncode==0 and '1 passed; 0 failed' in restored_output
    row={'gate_sha':sha,'control':label,'call_site':str(path.relative_to(root)),'test':test,'assertion':assertion,'broken_exit':broken.returncode,'restored_exit':restored.returncode,'discriminates':discriminates,'restored_pass':restored_ok,'restored_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'exact_restoration':path.read_bytes()==original}
    results.append(row);(logs/'results.json').write_text(json.dumps(results,indent=2))
    print(json.dumps(row),flush=True)
    if not discriminates or not restored_ok: raise SystemExit('Finding: control survived or gate failed; inspect '+str(logs))
assert len(results)==5,'independent control floor; iteration 1 stops at five mutations'
